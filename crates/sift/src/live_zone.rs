//! Live-zone 压缩入口。
//!
//! 按格式适配层（[`crate::formats`]）定位 live zone 并枚举文本候选，
//! 只对区间内的候选做消息内压缩，冻结前缀（cache_control 标记以下）字节不动。
//!
//! 边界说明：本 crate 的输入是已解析的 [`serde_json::Value`]（来自 napi 层的
//! JS 对象），没有原始字节可保，因此采用「就地修改 text 字段 + 序列化重建」。
//! 冻结区的 Value 不被触碰，内容等价性由 Value 不变保证。真字节区间手术
//! （保持 cache SHA 不变）需在更高层（HTTP 代理）实现。

use crate::stash::{marker_for, StashStore};
use crate::formats::TextCandidates;
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
                } => {
                    store.put(&stash_key, text);
                    outcome.stash_stored += 1;
                    outcome.tokens_saved += tokens_saved as i64;
                    holder[text_field] = Value::String(new_text);
                    outcome.blocks_compressed += 1;
                    outcome.changed = true;
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
    },
    /// 压缩后 token 不减，回退原样。
    Reverted,
}

/// 混合内容回退路由：分段（mixed_content）+ 段内嵌入 JSON span（recursive_json）。
/// 每段独立分发压缩器，不可压的段原样保留；返回 Some 仅当整体确实变小。
/// 有损：原文由调用方（Lossy 分支）写入 stash store。
fn route_mixed_fallback(text: &str, ctx: &CompressionContext) -> Option<String> {
    let sections = crate::mixed_content::split_into_sections(text);
    if sections.len() <= 1 {
        // 单一段落：嵌入 JSON span 仍可能存在（如 bash 回显 + jq 尾巴同行），
        // 用 recursive_json 做单段内替换。
        let any = std::cell::Cell::new(false);
        let replaced = crate::recursive_json::replace_json_spans(text, |span| {
            let c = compressor_for(crate::content::detect_content_type(span))?;
            match c.apply(span, ctx) {
                Ok((out, _)) if out.len() < span.len() => {
                    any.set(true);
                    Some(out)
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
        if i > 0 && !out.ends_with('\n') {
            out.push('\n');
        }
        match compressor_for(sec.content_type) {
            Some(c) => match c.apply(&sec.content, ctx) {
                Ok((compressed, _)) if compressed.len() < sec.content.len() => {
                    any = true;
                    out.push_str(&compressed);
                }
                _ => out.push_str(&sec.content),
            },
            None => {
                // 无压缩器的段（如纯文本段）：尝试段内嵌入 JSON span 替换。
                let seg = &sec.content;
                let seg_any = std::cell::Cell::new(false);
                let replaced = crate::recursive_json::replace_json_spans(seg, |span| {
                    let c = compressor_for(crate::content::detect_content_type(span))?;
                    match c.apply(span, ctx) {
                        Ok((out, _)) if out.len() < span.len() => {
                            seg_any.set(true);
                            Some(out)
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
/// 保护标签 → 无损 reformat（缩够 ≤80% 即短路）→ 有损压缩 → token 校验。
#[allow(clippy::too_many_arguments)]
pub(crate) fn process_block_text(
    text: &str,
    store: &dyn StashStore,
    ctx: &CompressionContext,
    protector: &TagProtector,
    tokenizer: &EstimatingCounter,
) -> BlockOutcome {
    let original_tokens = tokenizer.count_text(text);

    // 阶段 0：保护自定义标签（无标签文本是透传）。
    let (protected_text, tag_map) = protector.protect(text);

    // 阶段 1：无损重排。
    // JSON 空白剥离 / 日志模板挖掘——输出可完全重建，无需 stash。
    let mut current = protected_text.clone();
    let mut reformatted = false;
    if let Some(reformatter) = reformat_for(crate::content::detect_content_type(&current)) {
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

    // 无损已缩够（≤ 80% 原体积）则跳过有损压缩，避免不必要的 stash 卸载。
    let reformat_ratio =
        tokenizer.count_text(&current) as f64 / original_tokens.max(1) as f64;
    if reformatted && reformat_ratio <= 0.8 {
        if let Some(outcome) = commit_lossless(&current) {
            return outcome;
        }
    }

    // 阶段 2：有损压缩。reformat 缩不够（或没做）时，在重排后的文本上执行。
    let content_type = crate::content::detect_content_type(&current);

    // 混合内容优先：整块落到 PlainText 兜底时，先尝试分段路由 +
    // 嵌入 JSON span 路由（split_into_sections + recursive_json）
    // ——分段比整块纯文本压缩精准。
    if content_type == crate::content::ContentType::PlainText {
        if let Some(compressed) = route_mixed_fallback(&current, ctx) {
            let key = crate::stash::compute_key(text);
            let saved = original_tokens.saturating_sub(tokenizer.count_text(&compressed));
            let mut final_text = compressed;
            final_text.push_str(&marker_for(&key));
            return BlockOutcome::Lossy {
                new_text: final_text,
                stash_key: key,
                tokens_saved: saved,
            };
        }
    }

    let Some(compressor) = compressor_for(content_type) else {
        // 无压缩器（Html 兜底）但 reformat 有收益：保留无损结果。
        return match (reformatted, commit_lossless(&current)) {
            (true, Some(outcome)) => outcome,
            _ => BlockOutcome::Unchanged,
        };
    };

    let (compressed_protected, _) = match compressor.apply(&current, ctx) {
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
    let compressed = protector.restore(&compressed_protected, &tag_map);
    if compressed.is_empty() || compressed == text {
        return match (reformatted, commit_lossless(&current)) {
            (true, Some(outcome)) => outcome,
            _ => BlockOutcome::Unchanged,
        };
    }

    // token 校验：压缩后 token 不减则回退，避免「越压越大」。
    let after = tokenizer.count_text(&compressed);
    if after >= original_tokens {
        return match (reformatted, commit_lossless(&current)) {
            (true, Some(outcome)) => outcome,
            (..) => BlockOutcome::Reverted,
        }
    }

    // stash 存保护前的真正原文，保证 retrieve 拿回完整内容。
    let key = compressor.cache_key(text);
    let _ = store; // store 写入由调用方完成（见 Lossy 分支）
    let mut final_text = compressed;
    final_text.push_str(&marker_for(&key));
    BlockOutcome::Lossy {
        new_text: final_text,
        stash_key: key,
        tokens_saved: original_tokens.saturating_sub(after),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stash::InMemoryStashStore;
    use serde_json::json;

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
        // bash 输出：命令回显（文本）+ 大 JSON 数组（jq 结果）+ 尾部文本。
        // 整块 detect 落到 PlainText → 混合分段路由：JSON 段被压缩，
        // 其余文本段原样保留，整体带 stash 标记。
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
        assert!(outcome.stash_stored >= 1, "混合路由应有损+stash: {outcome:?}");
        let txt = b["messages"][0]["content"][0]["content"].as_str().unwrap();
        // 文本段保留、JSON 段被压、含取回标记。
        assert!(txt.contains("$ gh api repos/x/y/issues"));
        assert!(txt.contains("<<stash:"));
        assert!(txt.len() < mixed.len());
        // 原文可回取（含完整 JSON）。
        let key_start = txt.rfind("<<stash:").unwrap();
        let key = &txt[key_start + "<<stash:".len()..txt.len() - 2];
        assert_eq!(store.get(key).unwrap(), mixed);
    }

    #[test]
    fn lossless_reformat_short_circuits_stash() {
        // pretty-print 的 JSON（大量缩进空白）：JsonMinifier 无损剥离后
        // ≤80% 体积 → 走 Lossless 短路，不写 stash、无取回标记。
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
        // 无损结果不含取回标记，且仍可解析回等价 JSON。
        assert!(!txt.contains("<<stash:"), "无损路径不应有标记: {txt}");
        let parsed: serde_json::Value = serde_json::from_str(txt).unwrap();
        assert_eq!(parsed, serde_json::Value::Array(rows.clone()));
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
        assert!(outcome.stash_stored >= 1);
        // 压缩后 text 含取回标记。
        let blocks = b["messages"][2]["content"].as_array().unwrap();
        let txt = blocks[0]["content"].as_str().unwrap();
        assert!(txt.contains("<<stash:"));
        // 标记可回取原文：从尾部 <<stash:KEY>> 提取 key 查 store。
        let marker_start = txt.rfind("<<stash:").unwrap();
        let key = &txt[marker_start + "<<stash:".len()..txt.len() - 2];
        let restored = store.get(key).expect("原文应可从 stash store 回取");
        assert_eq!(restored, serde_json::to_string(&rows).unwrap());
    }

    #[test]
    fn stash_stores_original_with_custom_tags() {
        // 大 JSON 数组，多数行 note 重复（可去重压缩），少数行含自定义标签。
        // stash 应存保护前的完整原文（含标签）。
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
        assert!(outcome.stash_stored >= 1);

        let txt = b["messages"][0]["content"][0]["content"].as_str().unwrap();
        let marker_start = txt.rfind("<<stash:").unwrap();
        let key = &txt[marker_start + "<<stash:".len()..txt.len() - 2];
        // retrieve 取回的是保护前的完整原文，含自定义标签。
        let restored = store.get(key).expect("原文应可回取");
        assert_eq!(restored, original);
        assert!(restored.contains("<meta>"), "原文应含自定义标签");
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
        assert!(outcome.stash_stored >= 1);
        let txt = b["messages"][2]["content"].as_str().unwrap();
        assert!(txt.contains("<<stash:"));
        assert!(txt.len() < raw.len());
        // 消息结构与 tool_call_id 不变。
        assert_eq!(b["messages"][2]["tool_call_id"], json!("c1"));
        assert_eq!(b["messages"].as_array().unwrap().len(), 4);
        // 原文可回取。
        let marker_start = txt.rfind("<<stash:").unwrap();
        let key = &txt[marker_start + "<<stash:".len()..txt.len() - 2];
        assert_eq!(store.get(key).unwrap(), raw);
    }

    #[test]
    fn compresses_chat_completions_text_parts() {
        // parts 数组 content：各 type:"text" part 独立压缩，image_url part 不动。
        let big = "line of log output\n".repeat(60);
        let mut b = json!({
            "messages": [
                {"role": "user", "content": [
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
        assert!(out.contains("<<stash:"));
        assert!(out.len() < raw.len());
        // function_call（模型发出的调用）未被触碰。
        assert_eq!(b["input"][1]["arguments"], json!("{}"));
        // 原文可回取。
        let marker_start = out.rfind("<<stash:").unwrap();
        let key = &out[marker_start + "<<stash:".len()..out.len() - 2];
        assert_eq!(store.get(key).unwrap(), raw);
    }

    #[test]
    fn responses_string_input_passthrough() {
        // input 为纯字符串 = 当前 query，不压缩。
        let mut b = json!({"input": "just the current prompt".repeat(40)});
        let store = InMemoryStashStore::new();
        let outcome = compress_live_zone(&mut b, Some(&store), None);
        assert!(!outcome.changed);
    }
}
