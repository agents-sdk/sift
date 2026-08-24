//! 裸文本压缩入口：直接压缩单个字符串（如把工具输出原文送进任意 API 前）。
//!
//! 复用 live_zone 的单候选压缩管线（保护标签 → 无损 reformat 短路 →
//! 有损压缩 → token 校验 → stash 卸载），有损时输出尾部带 `<<stash:KEY>>` 标记。

use crate::stash::StashStore;
use crate::live_zone::{process_block_text, BlockOutcome};
use crate::tokenizer::EstimatingCounter;
use crate::transforms::tag_protector::{TagProtector, TagProtectorConfig};
use crate::transforms::CompressionContext;

/// 裸文本压缩结果。
#[derive(Debug, Default, PartialEq)]
pub struct TextCompressResult {
    /// 压缩后的文本（有损时尾部带 `<<stash:KEY>>` 标记）。
    pub text: String,
    /// 是否发生了实际压缩。
    pub changed: bool,
    /// 是否有损（原文已写入 stash store，可用 retrieve(key) 回取）。
    pub lossy: bool,
    /// 有损时的取回 key。
    pub stash_key: Option<String>,
    /// 估算节省的 token 数。
    pub tokens_saved: i64,
}

/// 压缩单个字符串。
///
/// - `store`：有损压缩的恢复通道；传入 `None` 则直接透传（无法保证不变量 3）。
/// - `query`：当前用户 query，供相关性压缩器使用。
///
/// 小于 [`crate::content::MIN_BLOCK_BYTES`] 的输入透传（与消息内路径一致）。
pub fn compress_text(
    text: &str,
    store: Option<&dyn StashStore>,
    query: Option<&str>,
) -> TextCompressResult {
    let passthrough = || TextCompressResult {
        changed: false,
        lossy: false,
        stash_key: None,
        tokens_saved: 0,
        text: text.to_string(),
    };
    let Some(store) = store else {
        return passthrough();
    };
    if text.len() < crate::content::MIN_BLOCK_BYTES {
        return passthrough();
    }

    let tokenizer = EstimatingCounter::new();
    let ctx = CompressionContext {
        query: query.map(|s| s.to_string()),
        token_budget: None,
    };
    let protector = TagProtector::new(TagProtectorConfig::default());

    match process_block_text(text, store, &ctx, &protector, &tokenizer) {
        BlockOutcome::Unchanged | BlockOutcome::Reverted => passthrough(),
        BlockOutcome::Lossless(new_text, saved) => TextCompressResult {
            text: new_text,
            changed: true,
            lossy: false,
            stash_key: None,
            tokens_saved: saved as i64,
        },
        BlockOutcome::Lossy {
            new_text,
            stash_key,
            tokens_saved,
        } => {
            // 单字符串入口没有外层循环代写 store，由本函数自己卸载原文。
            store.put(&stash_key, text);
            TextCompressResult {
                text: new_text,
                changed: true,
                lossy: true,
                stash_key: Some(stash_key),
                tokens_saved: tokens_saved as i64,
            }
        }
    }
}

// ────────────────────────────── 单元测试 ──────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stash::InMemoryStashStore;
    use serde_json::json;

    #[test]
    fn small_text_passthrough() {
        let store = InMemoryStashStore::new();
        let r = compress_text("tiny string", Some(&store), None);
        assert!(!r.changed);
        assert_eq!(r.text, "tiny string");
    }

    #[test]
    fn none_store_passthrough() {
        let big = "filler line for size\n".repeat(60);
        let r = compress_text(&big, None, None);
        assert!(!r.changed);
        assert_eq!(r.text, big);
    }

    #[test]
    fn large_json_array_lossy_with_marker() {
        let mut rows = Vec::new();
        for i in 0..200 {
            rows.push(json!({"id": i, "name": format!("item-{}", i), "status": "ok"}));
        }
        let raw = serde_json::to_string(&rows).unwrap();
        let store = InMemoryStashStore::new();
        let r = compress_text(&raw, Some(&store), None);
        assert!(r.changed);
        assert!(r.lossy);
        assert!(r.text.contains("<<stash:"));
        assert!(r.text.len() < raw.len());
        assert!(r.tokens_saved > 0);
        // 原文可回取。
        let key = r.stash_key.as_ref().unwrap();
        assert_eq!(store.get(key).unwrap(), raw);
    }

    #[test]
    fn pretty_json_lossless_without_marker() {
        let mut rows = Vec::new();
        for i in 0..50 {
            rows.push(json!({"id": i, "name": format!("item-{}", i), "status": "ok"}));
        }
        let pretty = serde_json::to_string_pretty(&rows).unwrap();
        let store = InMemoryStashStore::new();
        let r = compress_text(&pretty, Some(&store), None);
        assert!(r.changed);
        assert!(!r.lossy, "无损短路不应写 stash: {r:?}");
        assert!(r.stash_key.is_none());
        assert!(!r.text.contains("<<stash:"));
        // 无损结果仍可解析回等价 JSON。
        let parsed: serde_json::Value = serde_json::from_str(&r.text).unwrap();
        assert_eq!(parsed, serde_json::Value::Array(rows));
    }
}
