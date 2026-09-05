//! Live-zone 压缩入口。
//!
//! 按格式适配层（[`crate::formats`]）定位 live zone 并枚举文本候选，
//! 只对区间内的候选做消息内压缩，冻结前缀（cache_control 标记以下）字节不动。
//!
//! 边界说明：本 crate 的输入是已解析的 [`serde_json::Value`]（来自 napi 层的
//! JS 对象），没有原始字节可保，因此采用「就地修改 text 字段 + 序列化重建」。
//! 冻结区的 Value 不被触碰，内容等价性由 Value 不变保证。真字节区间手术
//! （保持 cache SHA 不变）需在更高层（HTTP 代理）实现。

use crate::formats::TextCandidates;
use crate::stash::{marker_for, StashStore};
use crate::tokenizer::{EstimatingCounter, Tokenizer};
use crate::transforms::tag_protector::{TagProtector, TagProtectorConfig};
use crate::transforms::{compressor_for, reformat_for, CompressionContext};
use serde_json::Value;

/// live zone 定位结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveZone {
    /// 可压缩区间的消息下标（含）。
    pub floor: usize,
    pub ceiling: usize,
}

/// 定位 live zone（Anthropic 语义）：floor = 冻结条数，ceiling = 最后一条
/// user 消息下标。没有可压缩区间时返回 None。
pub fn locate_live_zone(body: &Value) -> Option<LiveZone> {
    crate::formats::anthropic::AnthropicFormat.live_zone(body)
}

/// 压缩结果统计。
#[derive(Debug, Default, PartialEq)]
pub struct LiveZoneOutcome {
    /// 是否发生了实际压缩。
    pub changed: bool,
    /// 检查过的 text block 数。
    pub blocks_examined: usize,
    /// 实际压缩的 block 数。
    pub blocks_compressed: usize,
    /// 因 token 校验未通过而回退的 block 数。
    pub blocks_reverted: usize,
    /// 写入 stash store 的原文条数。
    pub stash_stored: usize,
    /// 估算节省的 token 数。
    pub tokens_saved: i64,
}

/// 入口：对 body 的 live zone 就地应用压缩（自动检测请求格式）。
///
/// - `body`：请求体（Anthropic /v1/messages、OpenAI Chat Completions 或
///   Responses API 风格），就地修改。
/// - `store`：有损压缩的恢复通道；传入 `None` 则跳过压缩（无法保证不变量 3）。
/// - `query`：当前用户 query，供相关性锚点压缩器使用。
///
/// 每个格式适配层枚举出的文本候选：检测类型 → 分发压缩器 → 压缩 →
/// token 校验（压缩后 token 不减则回退）→ 原文卸载进 store →
/// 追加 `<<stash:HASH>>` 标记。
pub fn compress_live_zone(
    body: &mut Value,
    store: Option<&dyn StashStore>,
    query: Option<&str>,
) -> LiveZoneOutcome {
    let fmt = crate::formats::detect_request_format(body);
    let Some(strategy) = crate::formats::strategy_for(fmt) else {
        return LiveZoneOutcome::default();
    };
    let Some(zone) = strategy.live_zone(body) else {
        return LiveZoneOutcome::default();
    };
    let store = match store {
        Some(s) => s,
        None => return LiveZoneOutcome::default(),
    };

    let tokenizer = EstimatingCounter::new();
    let ctx = CompressionContext {
        query: query.map(|s| s.to_string()),
        token_budget: None,
        source_path: None,
        stash_file_path: None,
        stash_line_offset: 0,
    };
    // 压缩前保护自定义 XML 标签，防止压缩器误伤；压缩后恢复。
    let protector = TagProtector::new(TagProtectorConfig::default());

    let mut outcome = LiveZoneOutcome::default();
    let messages = strategy.messages_mut(body).expect("live_zone 已验证存在");

    for msg in &mut messages[zone.floor..=zone.ceiling] {
        // 候选文本来源由格式适配层枚举（如 text block 的 `text` 字段、
        // tool_result 的 `content`、tool 消息的 `content`、function_call_output
        // 的 `output`）。只压消息内的大文本块，不改消息结构。
        strategy.for_each_candidate(msg, &mut |holder, text_field| {
            let Some(text) = holder.get(text_field).and_then(|t| t.as_str()) else {
                return;
            };
            if text.len() < crate::content::MIN_BLOCK_BYTES {
                return;
            }
            outcome.blocks_examined += 1;

            let result = process_block_text(text, store, &ctx, &protector, &tokenizer);
            match result {
                BlockOutcome::Unchanged => {}
                BlockOutcome::Lossless(new_text, tokens_saved) => {
                    holder[text_field] = Value::String(new_text);
                    outcome.blocks_compressed += 1;
                    outcome.changed = true;
                    outcome.tokens_saved += tokens_saved as i64;
                }
                BlockOutcome::Lossy {
                    new_text,
                    stash_key,
                    tokens_saved,
                    deferred_stashes_stored,
                } => {
                    // 先确认原文已持久化，再发布 marker。写入失败必须原样回退，
                    // 否则会产生无法恢复的有损结果。
                    if store.put(&stash_key, text).is_ok() {
                        outcome.stash_stored += 1 + deferred_stashes_stored;
                        outcome.tokens_saved += tokens_saved as i64;
                        holder[text_field] = Value::String(new_text);
                        outcome.blocks_compressed += 1;
                        outcome.changed = true;
                    } else {
                        outcome.blocks_reverted += 1;
                    }
                }
                BlockOutcome::Reverted => outcome.blocks_reverted += 1,
            }
        });
    }
    outcome
}

/// 单 block 的压缩结果。
pub(crate) enum BlockOutcome {
    Unchanged,
    /// 无损 reformat 结果（无需 stash，可完全重建）。
    Lossless(String, usize),
    /// 有损压缩结果：原文已在 store，输出带取回标记。
    Lossy {
        new_text: String,
        stash_key: String,
        tokens_saved: usize,
        /// 正文内嵌 marker 已成功写入的去重 stash 数；整块原文由调用方另写。
        deferred_stashes_stored: usize,
    },
    /// 压缩后 token 不减，回退原样。
    Reverted,
}

/// 混合内容回退路由：分段（mixed_content）+ 段内嵌入 JSON span（recursive_json）。
/// 每段独立分发压缩器，不可压的段原样保留；返回 Some 仅当整体确实变小。
/// 有损：原文由调用方（Lossy 分支）写入 stash store。
fn route_mixed_fallback(text: &str, ctx: &CompressionContext) -> Option<String> {
    // 行内 JSON span 没有整行坐标保证；不得继承整块的可读路径。
    let mut span_ctx = ctx.clone();
    span_ctx.stash_file_path = None;
    let sections = crate::mixed_content::split_into_sections(text);
    if sections.len() <= 1 {
        // 单一段落：嵌入 JSON span 仍可能存在（如 bash 回显 + jq 尾巴同行），
        // 用 recursive_json 做单段内替换。
        let any = std::cell::Cell::new(false);
        let replaced = crate::recursive_json::replace_json_spans(text, |span| {
            let c = compressor_for(crate::content::detect_content_type(span))?;
            match c.apply(span, &span_ctx) {
                Ok(result) if result.compressed.len() < span.len() => {
                    any.set(true);
                    Some(result.compressed)
                }
                _ => None,
            }
        });
        if !any.get() {
            return None;
        }
        return Some(replaced);
    }

    // 多段：逐段压缩（JSON 段走 smart_crusher，日志段走 log_compressor 等），
    // 文本段内再做嵌入 span 替换。
    let mut out = String::with_capacity(text.len() / 2);
    let mut any = false;
    for (i, sec) in sections.iter().enumerate() {
        let mut section_ctx = ctx.clone();
        section_ctx.stash_line_offset += sec.start_line;
        // 只在原始整行切片字节相等时传递坐标，JSON 字节切片等保守回退。
        let original_section = text
            .split('\n')
            .skip(sec.start_line)
            .take(sec.end_line - sec.start_line + 1)
            .collect::<Vec<_>>()
            .join("\n");
        if sec.content != original_section {
            section_ctx.stash_file_path = None;
        }
        if i > 0 && !out.ends_with('\n') {
            out.push('\n');
        }
        match compressor_for(sec.content_type) {
            Some(c) => match c.apply(&sec.content, &section_ctx) {
                Ok(result) if result.compressed.len() < sec.content.len() => {
                    any = true;
                    out.push_str(&result.compressed);
                }
                _ => out.push_str(&sec.content),
            },
            None => {
                // 无压缩器的段（如纯文本段）：尝试段内嵌入 JSON span 替换。
                let seg = &sec.content;
                let seg_any = std::cell::Cell::new(false);
                let replaced = crate::recursive_json::replace_json_spans(seg, |span| {
                    let c = compressor_for(crate::content::detect_content_type(span))?;
                    match c.apply(span, &span_ctx) {
                        Ok(result) if result.compressed.len() < span.len() => {
                            seg_any.set(true);
                            Some(result.compressed)
                        }
                        _ => None,
                    }
                });
                any |= seg_any.get();
                out.push_str(&replaced);
            }
        }
    }
    if !any || out.len() >= text.len() {
        return None;
    }
    Some(out)
}

/// 单个 block 文本的完整压缩流程（纯函数，不触碰 JSON 结构）：
/// 保护标签 → 无损 reformat（按变换收益门槛短路）→ 有损压缩 → token 校验。
#[allow(clippy::too_many_arguments)]
pub(crate) fn process_block_text(
    text: &str,
    store: &dyn StashStore,
    ctx: &CompressionContext,
    protector: &TagProtector,
    tokenizer: &EstimatingCounter,
) -> BlockOutcome {
    // marker 已代表一段先前压缩结果。整块跳过可保证重复调用幂等，避免
    // `marker -> marker -> original` 的递归恢复链。
    if crate::stash::contains_marker(text) {
        return BlockOutcome::Unchanged;
    }

    // source_path 可覆盖内容探测；同时决定源码凭据候选的误报抑制规则。
    let path_language = ctx
        .source_path
        .as_deref()
        .map(crate::transforms::code_compressor::detect_language_from_path)
        .unwrap_or(crate::transforms::code_compressor::CodeLanguage::Unknown);
    let original_type =
        if path_language != crate::transforms::code_compressor::CodeLanguage::Unknown {
            crate::content::ContentType::SourceCode
        } else {
            crate::content::detect_content_type(text)
        };
    let source_secret_mode = original_type == crate::content::ContentType::SourceCode;

    let original_tokens = tokenizer.count_text(text);

    // 阶段 0：保护自定义标签（无标签文本是透传）。
    let (protected_text, tag_map) = protector.protect(text);

    // 阶段 1：无损重排。
    // JSON 空白剥离 / 日志模板挖掘——输出可完全重建，无需 stash。
    let mut current = protected_text.clone();
    let mut reformatted = false;
    let mut reformat_max_ratio = 0.8;
    if let Some(reformatter) = reformat_for(crate::content::detect_content_type(&current)) {
        reformat_max_ratio = reformatter.max_output_ratio();
        if let Ok(refmt) = reformatter.apply(&current, ctx) {
            if refmt.len() < current.len() {
                reformatted = true;
                current = refmt;
            }
        }
    }

    let commit_lossless = |current: &str| -> Option<BlockOutcome> {
        let final_text = protector.restore(current, &tag_map);
        if final_text.len() >= text.len() {
            return None;
        }
        let saved = original_tokens.saturating_sub(tokenizer.count_text(&final_text));
        Some(BlockOutcome::Lossless(final_text, saved))
    };

    // 无损已达到该重排器的收益门槛则跳过有损压缩，避免不必要的 stash 卸载。
    let reformat_ratio = tokenizer.count_text(&current) as f64 / original_tokens.max(1) as f64;
    if reformatted && reformat_ratio <= reformat_max_ratio {
        if let Some(outcome) = commit_lossless(&current) {
            return outcome;
        }
    }

    // 阶段 2：落盘模式的有损压缩直接使用原文视图，不猜测模板化/重排行号。
    // 已成功的无损结果仍作为无收益时的回退候选。
    let key = crate::stash::compute_key(text);
    let stash_path = store
        .file_path(&key)
        .and_then(|path| path.to_str().map(str::to_owned));
    let lossy_input = if stash_path.is_some() && protected_text == text {
        protected_text.as_str()
    } else {
        current.as_str()
    };
    let mut compressor_ctx = ctx.clone();
    compressor_ctx.stash_line_offset = 0;
    if lossy_input == text {
        compressor_ctx.stash_file_path = stash_path;
    } else {
        compressor_ctx.source_path = None;
        compressor_ctx.stash_file_path = None;
    }
    let content_type = if lossy_input == text
        && path_language != crate::transforms::code_compressor::CodeLanguage::Unknown
    {
        crate::content::ContentType::SourceCode
    } else {
        crate::content::detect_content_type(lossy_input)
    };

    // 混合内容优先：整块落到 PlainText 兜底时，先尝试分段路由 +
    // 嵌入 JSON span 路由（split_into_sections + recursive_json）
    // ——分段比整块纯文本压缩精准。
    if content_type == crate::content::ContentType::PlainText {
        let mut mixed_ctx = compressor_ctx.clone();
        mixed_ctx.source_path = None;
        if let Some(compressed) = route_mixed_fallback(lossy_input, &mixed_ctx) {
            if !crate::secrets::preserves_secret_tokens(text, &compressed, source_secret_mode) {
                return match (reformatted, commit_lossless(&current)) {
                    (true, Some(outcome)) => outcome,
                    (..) => BlockOutcome::Reverted,
                };
            }
            let key = crate::stash::compute_key(text);
            let mut final_text = compressed;
            final_text.push_str(&marker_for(&key));
            let final_tokens = tokenizer.count_text(&final_text);
            if final_tokens >= original_tokens {
                return match (reformatted, commit_lossless(&current)) {
                    (true, Some(outcome)) => outcome,
                    (..) => BlockOutcome::Reverted,
                };
            }
            return BlockOutcome::Lossy {
                new_text: final_text,
                stash_key: key,
                tokens_saved: original_tokens.saturating_sub(final_tokens),
                deferred_stashes_stored: 0,
            };
        }
    }

    let Some(compressor) = compressor_for(content_type) else {
        // 无压缩器但 reformat 有收益：保留无损结果。
        return match (reformatted, commit_lossless(&current)) {
            (true, Some(outcome)) => outcome,
            _ => BlockOutcome::Unchanged,
        };
    };

    let offload = match compressor.apply(lossy_input, &compressor_ctx) {
        Ok(r) => r,
        // 无压缩空间（Skipped）或输入不符（InvalidInput）：
        // 若 reformat 有收益则保留无损结果。
        Err(_) => {
            return match (reformatted, commit_lossless(&current)) {
                (true, Some(outcome)) => outcome,
                _ => BlockOutcome::Unchanged,
            }
        }
    };

    // 恢复标签占位符，得到最终输出。
    let compressed = protector.restore(&offload.compressed, &tag_map);
    if compressed.is_empty() || compressed == text {
        return match (reformatted, commit_lossless(&current)) {
            (true, Some(outcome)) => outcome,
            _ => BlockOutcome::Unchanged,
        };
    }

    // 对齐 Headroom 的 entropy mask：每个高熵候选必须逐次留在可见输出，或由
    // 已验证的内嵌 stash marker 恢复；否则拒绝整次有损结果。
    // 标签保护会改变单元格原文字节；在没有精确反向映射到独立 stash 内容前，
    // 禁止发布由保护视图生成的内嵌 marker。
    if protected_text != text && !offload.deferred_stashes.is_empty() {
        return match (reformatted, commit_lossless(&current)) {
            (true, Some(outcome)) => outcome,
            (..) => BlockOutcome::Reverted,
        };
    }

    // 高熵候选既可以直接留在输出，也可以由输出中的有效 marker 指向已排队的
    // stash 内容。把每个 marker 对应的原文追加到校验视图，仍按出现次数核对。
    let mut secret_validation_view = compressed.clone();
    for deferred in &offload.deferred_stashes {
        if deferred.key != crate::stash::compute_key(&deferred.content)
            || !compressed.contains(&crate::stash::marker_for(&deferred.key))
        {
            return BlockOutcome::Reverted;
        }
        secret_validation_view.push('\n');
        secret_validation_view.push_str(&deferred.content);
    }
    if !crate::secrets::preserves_secret_tokens(text, &secret_validation_view, source_secret_mode) {
        return match (reformatted, commit_lossless(&current)) {
            (true, Some(outcome)) => outcome,
            (..) => BlockOutcome::Reverted,
        };
    }

    // stash 存保护前的真正原文，保证 retrieve 拿回完整内容。
    let mut final_text = compressed;
    final_text.push_str(&marker_for(&key));
    // 收益校验必须覆盖真正发送给模型的最终文本，包括 marker 开销。
    let final_tokens = tokenizer.count_text(&final_text);
    if final_tokens >= original_tokens {
        return match (reformatted, commit_lossless(&current)) {
            (true, Some(outcome)) => outcome,
            (..) => BlockOutcome::Reverted,
        };
    }
    let mut stored = std::collections::BTreeSet::new();
    for deferred in &offload.deferred_stashes {
        if stored.insert(deferred.key.as_str())
            && store.put(&deferred.key, &deferred.content).is_err()
        {
            return match (reformatted, commit_lossless(&current)) {
                (true, Some(outcome)) => outcome,
                (..) => BlockOutcome::Reverted,
            };
        }
    }
    BlockOutcome::Lossy {
        new_text: final_text,
        stash_key: key,
        tokens_saved: original_tokens.saturating_sub(final_tokens),
        deferred_stashes_stored: stored.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stash::{FileStashStore, InMemoryStashStore};
    use serde_json::json;

    struct FailingStore;

    impl StashStore for FailingStore {
        fn put(&self, _key: &str, _content: &str) -> std::io::Result<()> {
            Err(std::io::Error::other("injected write failure"))
        }

        fn get(&self, _key: &str) -> Option<String> {
            None
        }

        fn len(&self) -> usize {
            0
        }
    }

    fn body() -> Value {
        json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "cached", "cache_control": {"type": "ephemeral"}}
                ]},
                {"role": "assistant", "content": "old answer"},
                {"role": "user", "content": "latest question"},
            ]
        })
    }

    #[test]
    fn locates_zone_above_frozen_floor() {
        let zone = locate_live_zone(&body()).unwrap();
        assert_eq!(zone.floor, 1);
        assert_eq!(zone.ceiling, 2);
    }

    #[test]
    fn no_store_means_passthrough() {
        let mut b = body();
        let outcome = compress_live_zone(&mut b, None, None);
        assert!(!outcome.changed);
    }

    #[test]
    fn mixed_bash_output_routes_per_section() {
        // JSON 主体带轻量 wrapper：无损 schema 紧凑化后保留首尾文本。
        let mut rows = Vec::new();
        for i in 0..200 {
            rows.push(json!({"id": i, "name": format!("item-{}", i), "status": "ok"}));
        }
        let mixed = format!(
            "$ gh api repos/x/y/issues\n{}\ndone, {} rows above\n",
            serde_json::to_string(&rows).unwrap(),
            rows.len()
        );
        let mut b = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": mixed}
                ]},
            ]
        });
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(outcome.changed, "outcome={:?}", outcome);
        assert_eq!(outcome.stash_stored, 0, "无损 schema 不应写 stash");
        let txt = b["messages"][0]["content"][0]["content"].as_str().unwrap();
        // 文本 wrapper 保留，JSON 主体变成 schema，且无取回标记。
        assert!(txt.contains("$ gh api repos/x/y/issues"));
        assert!(txt.contains("[200]{id:int,name:string,status:string}"));
        assert!(txt.contains("done, 200 rows above"));
        assert!(!txt.contains("<<stash:"));
        assert!(txt.len() < mixed.len());
    }

    #[test]
    fn lossless_reformat_short_circuits_stash() {
        // pretty-print 的规则 JSON 数组优先转为 CSV-schema，完整保留所有行。
        let mut rows = Vec::new();
        for i in 0..50 {
            rows.push(json!({"id": i, "name": format!("item-{}", i), "status": "ok"}));
        }
        let pretty = serde_json::to_string_pretty(&rows).unwrap();
        let mut b = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": pretty}
                ]},
            ]
        });
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(outcome.changed, "outcome={:?}", outcome);
        // 无损路径：不写 stash store。
        assert_eq!(outcome.stash_stored, 0, "无损短路不应写 stash: {outcome:?}");
        let txt = b["messages"][0]["content"][0]["content"].as_str().unwrap();
        // 无损结果不含取回标记，schema 中保留首尾记录。
        assert!(!txt.contains("<<stash:"), "无损路径不应有标记: {txt}");
        assert!(txt.starts_with("[50]{id:int,name:string,status:string}\n"));
        assert!(txt.contains("0,item-0,ok"));
        assert!(txt.contains("49,item-49,ok"));
    }

    #[test]
    fn json_schema_compaction_keeps_every_row_without_stash() {
        let rows = (0..50)
            .map(|index| json!({"id": index, "name": format!("item-{index}"), "status": "ok"}))
            .collect::<Vec<_>>();
        let raw = serde_json::to_string(&rows).unwrap();
        let store = InMemoryStashStore::new();
        let ctx = CompressionContext::default();
        let protector = TagProtector::new(TagProtectorConfig::default());
        let tokenizer = EstimatingCounter::new();

        let BlockOutcome::Lossless(output, _) =
            process_block_text(&raw, &store, &ctx, &protector, &tokenizer)
        else {
            panic!("规则对象数组应由无损 CSV-schema 路径短路");
        };
        assert!(output.starts_with("[50]{id:int,name:string,status:string}\n"));
        assert!(output.contains("49,item-49,ok"));
        assert!(!output.contains("<<stash:"));
        assert!(store.is_empty());
    }

    #[test]
    fn sparse_json_schema_remains_stash_recoverable() {
        let mut rows = (0..40)
            .map(|index| json!({"id": index, "tag": format!("node-{index}")}))
            .collect::<Vec<_>>();
        rows[17].as_object_mut().unwrap().remove("tag");
        let raw = serde_json::to_string(&rows).unwrap();
        let mut body = json!({"messages": [{"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": raw}
        ]}]});
        let store = InMemoryStashStore::new();

        let outcome = compress_live_zone(&mut body, Some(&store), None);

        assert!(outcome.changed, "{outcome:?}");
        assert_eq!(outcome.stash_stored, 1);
        let output = body["messages"][0]["content"][0]["content"]
            .as_str()
            .unwrap();
        assert!(output.starts_with("[40]{id:int,tag:string?}\n"));
        let marker_start = output.rfind("<<stash:").unwrap();
        let key = &output[marker_start + "<<stash:".len()..output.len() - 2];
        assert_eq!(store.get(key).as_deref(), Some(raw.as_str()));
    }

    #[test]
    fn opaque_json_cells_count_deferred_and_outer_stashes() {
        let detail = "diagnostic paragraph with repeated low entropy words ".repeat(20);
        let rows = (0..40)
            .map(|index| json!({"id": index, "status": "ready", "detail": detail}))
            .collect::<Vec<_>>();
        let raw = serde_json::to_string(&rows).unwrap();
        let mut body = json!({"messages": [{"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": raw}
        ]}]});
        let store = InMemoryStashStore::new();

        let outcome = compress_live_zone(&mut body, Some(&store), None);

        assert!(outcome.changed);
        assert_eq!(outcome.stash_stored, 2);
        assert_eq!(store.len(), 2);
        let output = body["messages"][0]["content"][0]["content"]
            .as_str()
            .unwrap();
        assert!(output.contains(&marker_for(&crate::stash::compute_key(&detail))));
    }

    #[test]
    fn opaque_json_cell_write_failure_reverts_before_marker_is_published() {
        let detail = "diagnostic paragraph with repeated low entropy words ".repeat(20);
        let rows = (0..40)
            .map(|index| json!({"id": index, "status": "ready", "detail": detail}))
            .collect::<Vec<_>>();
        let raw = serde_json::to_string(&rows).unwrap();
        let ctx = CompressionContext::default();
        let protector = TagProtector::new(TagProtectorConfig::default());
        let tokenizer = EstimatingCounter::new();

        let outcome = process_block_text(&raw, &FailingStore, &ctx, &protector, &tokenizer);

        assert!(matches!(outcome, BlockOutcome::Reverted));
    }

    #[test]
    fn compresses_large_json_tool_result() {
        let mut rows = Vec::new();
        for i in 0..200 {
            rows.push(json!({"id": i, "name": format!("item-{}", i), "status": "ok"}));
        }
        let mut b = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "cached", "cache_control": {"type": "ephemeral"}}
                ]},
                {"role": "assistant", "content": "done"},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": serde_json::to_string(&rows).unwrap()}
                ]},
            ]
        });
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        // tool_result 的 content 是文本块，应被识别为 JSON 数组并压缩。
        assert!(outcome.blocks_compressed >= 1, "outcome={:?}", outcome);
        assert_eq!(outcome.stash_stored, 0);
        // 规则数组无损转为 CSV-schema，不需要取回标记。
        let blocks = b["messages"][2]["content"].as_array().unwrap();
        let txt = blocks[0]["content"].as_str().unwrap();
        assert!(txt.starts_with("[200]{id:int,name:string,status:string}\n"));
        assert!(txt.contains("199,item-199,ok"));
        assert!(!txt.contains("<<stash:"));
    }

    #[test]
    fn exact_code_omission_is_rendered_inline_with_file_slice_coordinates() {
        let mut code = String::from("use std::collections::HashMap;\n\nfn build() -> usize {\n");
        for i in 0..40 {
            code.push_str(&format!("    let value_{i} = {i};\n"));
        }
        code.push_str("    value_39\n}\n");
        let dir = std::env::temp_dir().join(format!("sift-live-code-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = FileStashStore::new(&dir).unwrap();
        let ctx = CompressionContext {
            source_path: Some("/workspace/src/lib.rs".to_string()),
            ..CompressionContext::default()
        };
        let protector = TagProtector::new(TagProtectorConfig::default());
        let tokenizer = EstimatingCounter::new();

        let BlockOutcome::Lossy {
            new_text,
            stash_key,
            ..
        } = process_block_text(&code, &store, &ctx, &protector, &tokenizer)
        else {
            panic!("长代码应进入有损路径");
        };
        let stash_path = store.file_path(&stash_key).unwrap();
        let stash_path = serde_json::to_string(&stash_path.to_string_lossy()).unwrap();
        assert!(new_text.contains(&format!(
            "// ... 36 lines omitted from file {stash_path}, starting at line 9"
        )));
        assert!(!new_text.contains("[sift: omitted"));
        assert!(!new_text.contains("retrieveLines"));
        assert!(new_text.ends_with(&marker_for(&stash_key)));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn official_java_demo_remains_compressible() {
        let code = include_str!("../tests/fixtures/order_service.java");
        let dir = std::env::temp_dir().join(format!("sift-live-java-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = FileStashStore::new(&dir).unwrap();
        let ctx = CompressionContext {
            source_path: Some(
                "src/main/java/com/example/orders/service/OrderService.java".to_string(),
            ),
            ..CompressionContext::default()
        };
        let protector = TagProtector::new(TagProtectorConfig::default());
        let tokenizer = EstimatingCounter::new();

        let outcome = process_block_text(code, &store, &ctx, &protector, &tokenizer);
        let (new_text, stash_key) = match outcome {
            BlockOutcome::Lossy {
                new_text,
                stash_key,
                ..
            } => (new_text, stash_key),
            BlockOutcome::Unchanged => panic!("官网 Java 样例不应保持不变"),
            BlockOutcome::Lossless(..) => panic!("官网 Java 样例不应只做无损压缩"),
            BlockOutcome::Reverted => panic!("官网 Java 样例不应因收益不足回退"),
        };
        assert!(new_text.contains("public class OrderService"));
        let stash_path = store.file_path(&stash_key).unwrap();
        let stash_path = serde_json::to_string(&stash_path.to_string_lossy()).unwrap();
        assert!(new_text.contains(&format!(
            "// ... 26 lines omitted from file {stash_path}, starting at line 36"
        )));
        assert!(!new_text.contains("[sift: omitted"));
        assert!(new_text.len() < code.len());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn changed_tag_mapping_suppresses_line_range_hint() {
        let mut code = String::from(
            "<custom-meta>\nthese lines\nare protected\n</custom-meta>\nuse std::collections::HashMap;\n\nfn build() -> usize {\n",
        );
        for i in 0..40 {
            code.push_str(&format!("    let value_{i} = {i};\n"));
        }
        code.push_str("    value_39\n}\n");
        let store = InMemoryStashStore::new();
        let ctx = CompressionContext::default();
        let protector = TagProtector::new(TagProtectorConfig::default());
        let tokenizer = EstimatingCounter::new();

        let BlockOutcome::Lossy { new_text, .. } =
            process_block_text(&code, &store, &ctx, &protector, &tokenizer)
        else {
            panic!("带受保护标签的长代码仍应可有损压缩");
        };
        assert!(!new_text.contains("omitted from file"));
        assert!(!new_text.contains("starting at line"));
    }

    #[test]
    fn lossless_schema_preserves_custom_tags() {
        // 标签先占位保护，schema 紧凑化后必须恢复到可见输出。
        let mut rows = Vec::new();
        for i in 0..200 {
            let note = if i % 50 == 0 {
                format!("special <meta>important {i}</meta>")
            } else {
                "regular note".to_string()
            };
            rows.push(json!({ "id": i, "note": note }));
        }
        let original = serde_json::to_string(&rows).unwrap();
        let mut b = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": original}
                ]},
            ]
        });
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(outcome.changed, "outcome={:?}", outcome);
        assert_eq!(outcome.stash_stored, 0);

        let txt = b["messages"][0]["content"][0]["content"].as_str().unwrap();
        assert!(txt.contains("<meta>important 0</meta>"));
        assert!(txt.contains("<meta>important 150</meta>"));
        assert!(!txt.contains("<<stash:"));
    }

    #[test]
    fn compresses_chat_completions_tool_message() {
        // OpenAI Chat Completions：role:"tool" 消息的字符串 content 是工具输出，
        // 应被识别为 JSON 数组并压缩；消息结构与 tool_call_id 不变。
        let mut rows = Vec::new();
        for i in 0..200 {
            rows.push(json!({"id": i, "name": format!("item-{}", i), "status": "ok"}));
        }
        let raw = serde_json::to_string(&rows).unwrap();
        let mut b = json!({
            "model": "gpt-5",
            "messages": [
                {"role": "user", "content": "list the items"},
                {"role": "assistant", "content": null, "tool_calls": [
                    {"id": "c1", "type": "function", "function": {"name": "list", "arguments": "{}"}}
                ]},
                {"role": "tool", "tool_call_id": "c1", "content": raw},
                {"role": "user", "content": "summarize"},
            ]
        });
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(outcome.blocks_compressed >= 1, "outcome={:?}", outcome);
        assert_eq!(outcome.stash_stored, 0);
        let txt = b["messages"][2]["content"].as_str().unwrap();
        assert!(txt.starts_with("[200]{id:int,name:string,status:string}\n"));
        assert!(txt.contains("199,item-199,ok"));
        assert!(!txt.contains("<<stash:"));
        assert!(txt.len() < raw.len());
        // 消息结构与 tool_call_id 不变。
        assert_eq!(b["messages"][2]["tool_call_id"], json!("c1"));
        assert_eq!(b["messages"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn compresses_chat_completions_tool_text_parts() {
        // tool 的 parts 数组 content：各 type:"text" part 独立压缩，image_url part 不动。
        let big = "line of log output\n\n".repeat(60);
        let mut b = json!({
            "messages": [
                {"role": "tool", "tool_call_id": "c1", "content": [
                    {"type": "text", "text": "check this log"},
                    {"type": "text", "text": big},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAA"}},
                ]},
            ]
        });
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        // 只有超 MIN_BLOCK_BYTES 的大文本 part 会进入检查，小 part 透传。
        assert!(outcome.blocks_examined >= 1, "outcome={:?}", outcome);
        assert!(outcome.changed);
        let parts = b["messages"][0]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2]["type"], json!("image_url"));
    }

    #[test]
    fn compresses_responses_function_call_output() {
        // Responses API：function_call_output 的 output 字符串被压缩，
        // input_text part 同样是候选。
        let mut rows = Vec::new();
        for i in 0..150 {
            rows.push(json!({"idx": i, "state": "active", "tag": "t"}));
        }
        let raw = serde_json::to_string(&rows).unwrap();
        let mut b = json!({
            "model": "gpt-5",
            "input": [
                {"role": "user", "content": [
                    {"type": "input_text", "text": "fetch items"}
                ]},
                {"type": "function_call", "call_id": "c1", "name": "fetch", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "c1", "output": raw},
                {"role": "user", "content": [
                    {"type": "input_text", "text": "summarize"}
                ]},
            ]
        });
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(outcome.blocks_compressed >= 1, "outcome={:?}", outcome);
        let out = b["input"][2]["output"].as_str().unwrap();
        assert!(out.starts_with("[150]{idx:int,state:string,tag:string}\n"));
        assert!(out.contains("149,active,t"));
        assert!(!out.contains("<<stash:"));
        assert!(out.len() < raw.len());
        // function_call（模型发出的调用）未被触碰。
        assert_eq!(b["input"][1]["arguments"], json!("{}"));
    }

    #[test]
    fn responses_string_input_passthrough() {
        // input 为纯字符串 = 当前 query，不压缩。
        let mut b = json!({"input": "just the current prompt".repeat(40)});
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(!outcome.changed);
    }

    #[test]
    fn chat_trailing_tool_output_is_compressed() {
        let rows: Vec<Value> = (0..200).map(|i| json!({"id": i, "status": "ok"})).collect();
        let raw = serde_json::to_string(&rows).unwrap();
        let mut b = json!({"model": "gpt-5", "messages": [
            {"role": "user", "content": "fetch"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "fetch", "arguments": "{}"}}
            ]},
            {"role": "tool", "tool_call_id": "c1", "content": raw},
        ]});
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(outcome.changed, "{outcome:?}");
        assert!(b["messages"][2]["content"]
            .as_str()
            .unwrap()
            .starts_with("[200]{id:int,status:string}\n"));
    }

    #[test]
    fn responses_trailing_function_output_is_compressed() {
        let rows: Vec<Value> = (0..200).map(|i| json!({"id": i, "status": "ok"})).collect();
        let raw = serde_json::to_string(&rows).unwrap();
        let mut b = json!({"model": "gpt-5", "input": [
            {"role": "user", "content": [{"type": "input_text", "text": "fetch"}]},
            {"type": "function_call", "call_id": "c1", "name": "fetch", "arguments": "{}"},
            {"type": "function_call_output", "call_id": "c1", "output": raw},
        ]});
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(outcome.changed, "{outcome:?}");
        assert!(b["input"][2]["output"]
            .as_str()
            .unwrap()
            .starts_with("[200]{id:int,status:string}\n"));
    }

    #[test]
    fn prompt_roles_remain_unchanged() {
        let raw = serde_json::to_string(
            &(0..200)
                .map(|i| json!({"id": i, "status": "ok"}))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let mut b = json!({"messages": [
            {"role": "system", "content": raw},
            {"role": "assistant", "content": null, "tool_calls": []},
            {"role": "user", "content": raw},
        ]});
        let original = b.clone();
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(!outcome.changed);
        assert_eq!(b, original);
    }

    #[test]
    fn stash_write_failure_reverts_lossy_block() {
        let rows: Vec<Value> = (0..200)
            .map(|i| json!(format!("repeated diagnostic value {}", i % 8)))
            .collect();
        let raw = serde_json::to_string(&rows).unwrap();
        let mut b = json!({"messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": raw}
            ]}
        ]});
        let original = b.clone();
        let outcome = compress_live_zone(&mut b, Some(&FailingStore), None);
        assert!(!outcome.changed);
        assert_eq!(outcome.stash_stored, 0);
        assert_eq!(outcome.blocks_reverted, 1);
        assert_eq!(b, original);
    }

    #[test]
    fn secret_line_stays_visible_while_other_log_lines_compress() {
        let secret = "ghp_48xKq2mN7vJz3pLw9RtY5bEcVdXfGaHiQw";
        let raw = format!(
            "2026-08-24 ERROR credential={secret}\n{}",
            "2026-08-24 INFO repeated build line\n".repeat(80)
        );
        let mut b = json!({"messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": raw}
            ]}
        ]});
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(outcome.changed);
        let compressed = b["messages"][0]["content"][0]["content"].as_str().unwrap();
        assert!(compressed.contains(secret));
        let key = crate::stash::compute_key(&raw);
        assert_eq!(store.get(&key).unwrap(), raw);
    }

    #[test]
    fn anthropic_error_tool_result_is_protected() {
        let raw = "neutral failure details\n".repeat(80);
        let mut b = json!({"messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "is_error": true, "content": raw}
            ]}
        ]});
        let original = b.clone();
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(!outcome.changed);
        assert_eq!(b, original);
    }
}
