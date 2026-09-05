////! @agent-context/sift 的 Node 原生模块。
//!
//! napi-rs 桥：把 sift 的压缩能力暴露给 JS/TS。
//! 构建产物为 `.node` cdylib，由 npm 包的 dist/index.js 加载。
//!
//! stash store 用**落盘文件**（[`FileStashStore`]）：原文写到磁盘，进程重启不丢。
//! 目录由环境变量 `SIFT_STASH_DIR` 指定，默认 `~/.sift/stash`。
//! 多实例/集群需把该目录挂到共享文件系统（或替换为外部 store 后端）。

use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;

use sift::stash::{FileStashStore, StashStore};

const MAX_RETRIEVE_LINES: u32 = 1_000;

/// 全局 stash 落盘 store（跨 JS 调用、跨进程重启持久）。
fn store() -> &'static FileStashStore {
    static STORE: OnceLock<FileStashStore> = OnceLock::new();
    STORE.get_or_init(|| {
        let dir = stash_dir();
        FileStashStore::new(&dir)
            .unwrap_or_else(|e| panic!("无法初始化 stash 落盘存储 {}: {e}", dir.display()))
    })
}

/// 解析 stash 存储目录：`SIFT_STASH_DIR` > `~/.sift/stash` > 系统临时目录。
fn stash_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("SIFT_STASH_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".sift").join("stash");
    }
    std::env::temp_dir().join("sift-stash")
}

/// 使用独立 stash store 的 Node 实例，由 npm 层的 `createSift` 创建。
#[napi]
pub struct SiftInstance {
    store: FileStashStore,
}

#[napi]
impl SiftInstance {
    #[napi(constructor)]
    pub fn new(stash_dir: String) -> Result<Self> {
        if stash_dir.trim().is_empty() {
            return Err(Error::from_reason("stashDir 不能为空"));
        }
        let store = FileStashStore::new(&stash_dir).map_err(|e| {
            Error::from_reason(format!("无法初始化 stash 落盘存储 {stash_dir}: {e}"))
        })?;
        Ok(Self { store })
    }

    /// 使用该实例的 stash store 压缩请求 body。
    #[napi]
    pub fn sift_request(&self, body: Value, query: Option<String>) -> Result<CompressResult> {
        Ok(compress_request_with_store(
            body,
            &self.store,
            query.as_deref(),
        ))
    }

    /// 使用该实例的 stash store 压缩裸文本。
    #[napi]
    pub fn sift_text(
        &self,
        text: String,
        query: Option<String>,
        source_path: Option<String>,
    ) -> Result<TextCompressResult> {
        Ok(compress_text_with_store(
            &text,
            &self.store,
            query.as_deref(),
            source_path.as_deref(),
        ))
    }

    /// 只从该实例的 stash store 取回原文。
    #[napi]
    pub fn retrieve(&self, key: String) -> Option<String> {
        self.store.get(&key)
    }

    /// 从该实例的 stash store 按原文行号读取一个连续分片。
    #[napi]
    pub fn retrieve_lines(
        &self,
        key: String,
        start_line: u32,
        line_count: u32,
    ) -> Result<Option<StashSliceResult>> {
        retrieve_lines_with_store(&self.store, &key, start_line, line_count)
    }
}

/// 压缩请求 body 的结果。
///
/// 输入/输出都是 JS 对象（会被 parse 成 serde_json::Value）。
#[napi(object)]
pub struct CompressResult {
    /// 压缩后的请求 body（JS 对象）
    pub body: Value,
    /// 是否发生了实际压缩
    pub changed: bool,
    /// 检查过的 text block 数
    pub blocks_examined: u32,
    /// 实际压缩的 block 数
    pub blocks_compressed: u32,
    /// 因 token 校验未通过而回退的 block 数
    pub blocks_reverted: u32,
    /// 冻结前缀消息条数（cache 锚点，未被触碰；OpenAI 格式恒为 0）
    pub frozen_messages: u32,
    /// 写入 stash store 的原文条数
    pub stash_stored: u32,
    /// 估算节省的 token 数
    pub tokens_saved: i64,
}

/// 压缩请求 body（自动检测 Anthropic /v1/messages、OpenAI Chat Completions、
/// OpenAI Responses API 三种格式）。默认只压缩工具输出，保护
/// system/user/assistant prompt。`query` 为当前用户 query（供相关性锚点压缩器
/// 使用，可空）。
#[napi]
pub fn sift_request(body: Value, query: Option<String>) -> Result<CompressResult> {
    Ok(compress_request_with_store(body, store(), query.as_deref()))
}

fn compress_request_with_store(
    mut body: Value,
    store: &dyn StashStore,
    query: Option<&str>,
) -> CompressResult {
    use sift::formats::{detect_request_format, frozen_message_count};
    let frozen = frozen_message_count(&body, detect_request_format(&body)) as u32;
    let outcome = sift::live_zone::compress_live_zone(&mut body, Some(store), query);
    CompressResult {
        body,
        changed: outcome.changed,
        blocks_examined: outcome.blocks_examined as u32,
        blocks_compressed: outcome.blocks_compressed as u32,
        blocks_reverted: outcome.blocks_reverted as u32,
        frozen_messages: frozen,
        stash_stored: outcome.stash_stored as u32,
        tokens_saved: outcome.tokens_saved,
    }
}

/// 按取回标记 key 取回原文（压缩时卸载进 store 的原始内容）。
#[napi]
pub fn retrieve(key: String) -> Option<String> {
    store().get(&key)
}

/// stash 原文的按行读取结果。行号从 1 开始，文本保留原始换行。
#[napi(object)]
pub struct StashSliceResult {
    pub text: String,
    pub start_line: u32,
    pub line_count: u32,
    pub total_lines: u32,
    pub has_more: bool,
}

/// 按 stash 原文的 1-based 行号读取连续分片，单次最多 1000 行。
#[napi]
pub fn retrieve_lines(
    key: String,
    start_line: u32,
    line_count: u32,
) -> Result<Option<StashSliceResult>> {
    retrieve_lines_with_store(store(), &key, start_line, line_count)
}

fn retrieve_lines_with_store(
    store: &dyn StashStore,
    key: &str,
    start_line: u32,
    line_count: u32,
) -> Result<Option<StashSliceResult>> {
    if start_line == 0 {
        return Err(Error::from_reason("retrieveLines: startLine 必须从 1 开始"));
    }
    if line_count == 0 || line_count > MAX_RETRIEVE_LINES {
        return Err(Error::from_reason(format!(
            "retrieveLines: lineCount 必须在 1..={MAX_RETRIEVE_LINES} 之间"
        )));
    }
    Ok(store
        .get_lines(key, start_line as usize, line_count as usize)
        .map(|slice| StashSliceResult {
            text: slice.text,
            start_line: slice.start_line as u32,
            line_count: slice.line_count as u32,
            total_lines: slice.total_lines as u32,
            has_more: slice.has_more,
        }))
}

/// 裸文本压缩结果（[`sift_text`] 的返回值）。
#[napi(object)]
pub struct TextCompressResult {
    /// 压缩后的文本（有损时尾部带 `<<stash:KEY>>` 标记）
    pub text: String,
    /// 是否发生了实际压缩
    pub changed: bool,
    /// 是否有损（原文已写入 stash store，可用 retrieve 取回）
    pub lossy: bool,
    /// 有损时的取回 key
    pub stash_key: Option<String>,
    /// 估算节省的 token 数
    pub tokens_saved: i64,
}

/// 压缩单个字符串（如把工具输出原文送进任意 API 之前）。
/// `query` 为当前用户 query（供相关性锚点压缩器使用，可空）。
#[napi]
pub fn sift_text(
    text: String,
    query: Option<String>,
    source_path: Option<String>,
) -> Result<TextCompressResult> {
    Ok(compress_text_with_store(
        &text,
        store(),
        query.as_deref(),
        source_path.as_deref(),
    ))
}

fn compress_text_with_store(
    text: &str,
    store: &dyn StashStore,
    query: Option<&str>,
    source_path: Option<&str>,
) -> TextCompressResult {
    let r = sift::text_api::compress_text_with_source_path(text, Some(store), query, source_path);
    TextCompressResult {
        text: r.text,
        changed: r.changed,
        lossy: r.lossy,
        stash_key: r.stash_key,
        tokens_saved: r.tokens_saved,
    }
}

/// 请求格式检测：'anthropic' | 'chat_completions' | 'responses' | 'unknown'。
#[napi]
pub fn detect_request_format(body: Value) -> String {
    use sift::formats::RequestFormat;
    match sift::formats::detect_request_format(&body) {
        RequestFormat::Anthropic => "anthropic",
        RequestFormat::ChatCompletions => "chat_completions",
        RequestFormat::Responses => "responses",
        RequestFormat::Unknown => "unknown",
    }
    .to_string()
}

/// 单独暴露的内容类型检测（便于 JS 侧诊断）。
#[napi]
pub fn detect_content_type(text: String) -> String {
    use sift::content::ContentType;
    match sift::content::detect_content_type(&text) {
        ContentType::JsonArray => "json_array",
        ContentType::BuildOutput => "build_output",
        ContentType::SearchResults => "search_results",
        ContentType::GitDiff => "git_diff",
        ContentType::SourceCode => "source_code",
        ContentType::PlainText => "plain_text",
        ContentType::Html => "html",
        ContentType::StructuredConfig => "structured_config",
    }
    .to_string()
}

// ────────────────────────────── 单元测试 ──────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_dispatches_per_format() {
        // 三种格式都能走通（napi 函数在测试里就是普通 Rust 函数）。
        let anthropic = serde_json::json!({"messages": [
            {"role": "user", "content": "hi"},
        ]});
        assert_eq!(detect_request_format(anthropic.clone()), "anthropic");

        let chat = serde_json::json!({"messages": [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": null, "tool_calls": []},
        ]});
        assert_eq!(detect_request_format(chat.clone()), "chat_completions");

        let responses = serde_json::json!({"input": "hi"});
        assert_eq!(detect_request_format(responses.clone()), "responses");
    }

    #[test]
    fn compress_text_roundtrip_via_store() {
        let big = format!("log line {} with payload\n", 1).repeat(80);
        let r = sift_text(big.clone(), None, None).unwrap();
        if r.lossy {
            let key = r.stash_key.clone().unwrap();
            assert_eq!(retrieve(key).unwrap(), big);
        } else {
            // 无损或透传时无需回取。
            assert!(!r.text.contains("<<stash:") || !r.lossy);
        }
    }

    #[test]
    fn instance_uses_its_own_stash_directory() {
        let dir = std::env::temp_dir().join(format!(
            "sift-node-instance-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let instance = SiftInstance::new(dir.to_string_lossy().into_owned()).unwrap();
        assert_eq!(instance.store.dir(), std::fs::canonicalize(&dir).unwrap());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn instance_retrieves_bounded_line_slice() {
        let dir = std::env::temp_dir().join(format!(
            "sift-node-lines-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let instance = SiftInstance::new(dir.to_string_lossy().into_owned()).unwrap();
        let content = "one\ntwo\nthree\nfour";
        let key = sift::stash::compute_key(content);
        instance.store.put(&key, content).unwrap();

        let slice = instance.retrieve_lines(key, 2, 2).unwrap().unwrap();
        assert_eq!(slice.text, "two\nthree\n");
        assert_eq!(slice.total_lines, 4);
        assert!(slice.has_more);
        assert!(instance
            .retrieve_lines("0".repeat(24), 1, MAX_RETRIEVE_LINES + 1)
            .is_err());

        std::fs::remove_dir_all(dir).ok();
    }
}
