//! 裸文本压缩入口：直接压缩单个字符串（如把工具输出原文送进任意 API 前）。
//!
//! 复用 live_zone 的单候选压缩管线（保护标签 → 无损 reformat 短路 →
//! 有损压缩 → token 校验 → stash 卸载），有损时输出尾部带 `<<stash:KEY>>` 标记。

use crate::live_zone::{process_block_text, BlockOutcome};
use crate::stash::StashStore;
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
    compress_text_with_source_path(text, store, query, None)
}

/// 压缩单个字符串，并在输入与源文件逐行对应时把可选文件路径传给代码压缩器。
/// 使用落盘 stash 时，按行抽取的省略点引用其绝对文件路径、行数和准确起始行。
pub fn compress_text_with_source_path(
    text: &str,
    store: Option<&dyn StashStore>,
    query: Option<&str>,
    source_path: Option<&str>,
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
        source_path: source_path.map(str::to_string),
        stash_file_path: None,
        stash_line_offset: 0,
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
            if store.put(&stash_key, text).is_err() {
                return passthrough();
            }
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
    use crate::stash::{FileStashStore, InMemoryStashStore};
    use crate::tokenizer::Tokenizer;
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
        let tokenizer = EstimatingCounter::new();
        assert_eq!(
            r.tokens_saved,
            tokenizer.count_text(&raw) as i64 - tokenizer.count_text(&r.text) as i64,
            "tokens_saved 必须包含最终 stash marker 的成本"
        );
        // 原文可回取。
        let key = r.stash_key.as_ref().unwrap();
        assert_eq!(store.get(key).unwrap(), raw);
    }

    #[test]
    fn search_results_lossy_has_single_marker() {
        // 回归：search_compressor 的折叠标记曾内嵌 `<<stash:KEY>>`，框架又在末尾
        // 追加一次，导致输出出现两个标记。这里断言有损输出只有一个取回标记。
        let files = [
            "crates/sift/src/lib.rs",
            "crates/sift/src/stash.rs",
            "npm/core/src/index.ts",
        ];
        let mut raw = String::new();
        for i in 1..=60 {
            let f = files[i % 3];
            raw.push_str(&format!(
                "{f}:{}:siftText StashStore related handler {i}\n",
                i * 13
            ));
        }
        let store = InMemoryStashStore::new();
        let r = compress_text(&raw, Some(&store), Some("siftText StashStore"));
        assert!(r.changed);
        assert!(r.lossy);
        assert_eq!(
            r.text.matches("<<stash:").count(),
            1,
            "输出应只有一个 stash 标记: {}",
            r.text
        );
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

    #[test]
    fn already_compressed_text_is_idempotent() {
        let rows: Vec<_> = (0..200).map(|i| json!({"id": i, "status": "ok"})).collect();
        let raw = serde_json::to_string(&rows).unwrap();
        let store = InMemoryStashStore::new();
        let first = compress_text(&raw, Some(&store), None);
        assert!(first.lossy);
        assert_eq!(first.text.matches("<<stash:").count(), 1);

        let second = compress_text(&first.text, Some(&store), None);
        assert!(!second.changed);
        assert_eq!(second.text, first.text);
        assert_eq!(second.text.matches("<<stash:").count(), 1);
    }

    #[test]
    fn lossy_source_fold_reverts_if_it_would_hide_a_secret() {
        let secret = "ghp_48xKq2mN7vJz3pLw9RtY5bEcVdXfGaHiQw";
        let mut raw = "fn deploy() {\n".to_string();
        for i in 0..50 {
            if i == 25 {
                raw.push_str(&format!("    let token = \"{secret}\";\n"));
            } else {
                raw.push_str(&format!("    let value_{i} = {i};\n"));
            }
        }
        raw.push_str("}\n");
        let store = InMemoryStashStore::new();
        let result = compress_text_with_source_path(
            &raw,
            Some(&store),
            Some("deploy token"),
            Some("src/deploy.rs"),
        );
        assert!(!result.changed, "隐藏凭据的源码折叠必须回退");
        assert_eq!(result.text, raw);
        assert!(store.is_empty());
    }

    #[test]
    fn source_path_routes_all_supported_languages_to_inline_file_slices() {
        fn source(prefix: &str, body: &str, suffix: &str) -> String {
            let mut text = prefix.to_string();
            for i in 0..35 {
                text.push_str(&body.replace("$i", &i.to_string()));
                text.push('\n');
            }
            text.push_str(suffix);
            text
        }

        let cases = [
            (
                "/workspace/src/demo.py",
                source(
                    "def build(x):\n",
                    "    value_$i = x + $i",
                    "    return value_0\n",
                ),
                ("#", 7, 31),
            ),
            (
                "/workspace/src/demo.js",
                source(
                    "function build(x) {\n",
                    "  const value_$i = x + $i;",
                    "  return value_0;\n}\n",
                ),
                ("//", 7, 31),
            ),
            (
                "/workspace/src/demo.ts",
                source(
                    "function build(x: number): number {\n",
                    "  const value_$i: number = x + $i;",
                    "  return value_0;\n}\n",
                ),
                ("//", 7, 31),
            ),
            (
                "/workspace/src/demo.go",
                source(
                    "package main\n\nfunc build(x int) int {\n",
                    "    value$i := x + $i",
                    "    return value0\n}\n",
                ),
                ("//", 9, 31),
            ),
            (
                "/workspace/src/demo.rs",
                source(
                    "fn build(x: usize) -> usize {\n",
                    "    let value_$i = x + $i;",
                    "    value_0\n}\n",
                ),
                ("//", 7, 31),
            ),
            (
                "/workspace/src/Demo.java",
                source(
                    "public class Demo {\n    public int build(int x) {\n",
                    "        int value$i = x + $i;",
                    "        return value0;\n    }\n}\n",
                ),
                ("//", 8, 31),
            ),
            (
                "/workspace/src/demo.c",
                source(
                    "int build(int x) {\n",
                    "    int value$i = x + $i;",
                    "    return value0;\n}\n",
                ),
                ("//", 7, 31),
            ),
            (
                "/workspace/src/demo.cpp",
                source(
                    "int build(int x) {\n",
                    "    int value$i = x + $i;",
                    "    return value0;\n}\n",
                ),
                ("//", 7, 31),
            ),
        ];

        let dir = std::env::temp_dir().join(format!("sift-text-languages-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = FileStashStore::new(&dir).unwrap();
        for (path, text, (comment, start_line, omitted)) in cases {
            let result = compress_text_with_source_path(&text, Some(&store), None, Some(path));
            assert!(result.changed, "{path} 应进入源码压缩路径");
            let stash_path = store
                .file_path(result.stash_key.as_deref().unwrap())
                .unwrap();
            let stash_path = serde_json::to_string(&stash_path.to_string_lossy()).unwrap();
            assert!(
                result.text.contains(&format!(
                    "{comment} ... {omitted} lines omitted from file {stash_path}, starting at line {start_line}"
                )),
                "path={path}\n{}",
                result.text
            );
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn source_without_path_uses_stash_file_at_each_fold() {
        let mut text = String::from("function resolveWorkerPath(): string {\n");
        for i in 0..18 {
            text.push_str(&format!(
                "  const candidate_{i} = resolve(process.cwd(), 'worker-{i}.js');\n"
            ));
        }
        text.push_str("  return candidate_0;\n}\n");

        let dir = std::env::temp_dir().join(format!("sift-text-stash-path-{}", std::process::id()));
        let store = FileStashStore::new(&dir).unwrap();
        let result = compress_text(&text, Some(&store), None);
        let key = crate::stash::compute_key(&text);
        let stash_path = store.file_path(&key).unwrap();
        let stash_path = serde_json::to_string(&stash_path.to_string_lossy()).unwrap();

        assert!(
            result.changed,
            "无 sourcePath 的 TypeScript 也应进入源码压缩"
        );
        assert!(result.lossy);
        assert_eq!(result.stash_key.as_deref(), Some(key.as_str()));
        assert!(
            result.text.contains(&format!(
                "// ... 14 lines omitted from file {stash_path}, starting at line 7"
            )),
            "{}",
            result.text
        );
        assert!(result.text.ends_with(&format!("<<stash:{key}>>")));

        let slice = store.get_lines(&key, 7, 14).unwrap();
        assert_eq!(slice.line_count, 14);
        assert!(slice.text.starts_with("  const candidate_5"));
        assert!(slice.text.ends_with("  return candidate_0;\n"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn html_extracts_article_and_stashes_exact_original() {
        let html = include_str!("../tests/fixtures/article_page.html");
        let store = InMemoryStashStore::new();

        let result = compress_text(html, Some(&store), None);

        assert!(result.changed);
        assert!(result.lossy);
        assert!(result
            .text
            .contains("# Compress HTML without losing the article"));
        assert!(result.text.contains("Article paragraphs remain visible."));
        assert!(!result.text.contains("analyticsToken"));
        assert!(!result.text.contains("Buy unrelated products"));
        assert!(!result.text.contains("Copyright 2026"));
        let key = result.stash_key.as_deref().unwrap();
        assert_eq!(store.get(key).as_deref(), Some(html));
    }

    #[test]
    fn structured_config_elides_comments_and_stashes_exact_original() {
        let config = include_str!("../tests/fixtures/deployment_config.yaml");
        let store = InMemoryStashStore::new();

        let result = compress_text(config, Some(&store), None);

        assert!(result.changed);
        assert!(result.lossy);
        assert!(result.text.contains("name: context-api"));
        assert!(result.text.contains("maxDelayMs: 2000"));
        assert!(!result.text.contains("Production deployment configuration"));
        assert!(result.text.contains("comment/blank lines elided"));
        let key = result.stash_key.as_deref().unwrap();
        assert_eq!(store.get(key).as_deref(), Some(config));
    }

    #[test]
    fn yaml_block_scalar_is_not_elided() {
        let config = format!(
            "script: |\n  # This is data, not a comment.\n  echo hello\nmetadata:\n  owner: team\n  enabled: true\n{}",
            "padding: keep-this-value\n".repeat(30)
        );
        let store = InMemoryStashStore::new();
        let result = compress_text(&config, Some(&store), None);
        assert!(!result.changed);
        assert_eq!(result.text, config);
        assert!(store.is_empty());
    }

    #[test]
    fn tabular_text_uses_smart_crusher_and_stashes_exact_original() {
        let mut csv = String::from("id,service,region,status,owner,latency_ms\n");
        for index in 0..120 {
            let status = if index == 97 { "degraded" } else { "healthy" };
            let latency = if index == 97 { 240 } else { 40 + index };
            csv.push_str(&format!(
                "{index},service-{index},us-east-1,{status},platform,{latency}\n"
            ));
        }
        let store = InMemoryStashStore::new();

        let result = compress_text(&csv, Some(&store), Some("degraded latency"));

        assert!(result.changed, "{result:?}");
        assert!(result.lossy);
        assert!(result.text.contains("service"));
        assert!(result.text.contains("degraded"));
        assert!(result.text.len() < csv.len());
        let key = result.stash_key.as_deref().unwrap();
        assert_eq!(store.get(key).as_deref(), Some(csv.as_str()));
    }

    #[test]
    fn ragged_table_is_not_rewritten() {
        let mut csv = String::from("id,name,status\n");
        for index in 0..40 {
            if index == 20 {
                csv.push_str("20,missing-status\n");
            } else {
                csv.push_str(&format!("{index},service-{index},healthy\n"));
            }
        }
        let store = InMemoryStashStore::new();
        let result = compress_text(&csv, Some(&store), None);
        assert!(!result.changed);
        assert_eq!(result.text, csv);
        assert!(store.is_empty());
    }
}
