//! 压缩变换：trait 边界 + 具体压缩器。
//!

pub mod code_compressor;
pub mod diff_compressor;
pub mod log_compressor;
pub mod reformats;
pub mod search_compressor;
pub mod smart_crusher;
pub mod tag_protector;
pub mod text_crusher;

use crate::content::ContentType;
use thiserror::Error;

/// 变换错误。
#[derive(Debug, Error, PartialEq)]
pub enum TransformError {
    #[error("invalid input")]
    InvalidInput,
    #[error("skipped")]
    Skipped,
    #[error("internal error: {0}")]
    Internal(String),
}

/// 压缩上下文：当前 query（relevance 用）与 token 预算。
#[derive(Debug, Clone, Default)]
pub struct CompressionContext {
    pub query: Option<String>,
    pub token_budget: Option<usize>,
}

/// 无损重排变换（输出可完全重建原文，如 JSON 空白剥离、日志模板挖掘）。
pub trait ReformatTransform: Send + Sync {
    fn name(&self) -> &'static str;
    fn applies_to(&self) -> ContentType;
    fn apply(&self, input: &str, ctx: &CompressionContext) -> Result<String, TransformError>;
}

/// 有损卸载变换：压缩后原文进 stash store，输出带取回标记。
/// 类型系统强制实现者提供 cache_key。
pub trait OffloadTransform: Send + Sync {
    fn name(&self) -> &'static str;
    fn applies_to(&self) -> ContentType;
    /// 估算膨胀度（0.0-1.0+），≥ 阈值才值得压缩。
    fn estimate_bloat(&self, input: &str) -> f64;
    fn cache_key(&self, input: &str) -> String;
    fn apply(
        &self,
        input: &str,
        ctx: &CompressionContext,
    ) -> Result<(String, String /* original */), TransformError>;
}

/// 按内容类型分发到压缩器。live_zone 的核心调度点。
/// 返回 None 表示该类型当前是 no-op（SourceCode/Html）。
pub fn dispatch_compressor(text: &str) -> Option<&'static str> {
    match crate::content::detect_content_type(text) {
        ContentType::JsonArray => Some("smart_crusher"),
        ContentType::BuildOutput => Some("log_compressor"),
        ContentType::SearchResults => Some("search_compressor"),
        ContentType::GitDiff => Some("diff_compressor"),
        ContentType::PlainText => Some("text_crusher"),
        ContentType::SourceCode => Some("code_compressor"),
        ContentType::Html => None,
    }
}

/// 按内容类型构造对应的有损压缩器实例（用默认配置）。
/// 返回 None 表示该类型无可压的压缩器（SourceCode/Html 当前 no-op）。
pub fn compressor_for(content_type: ContentType) -> Option<Box<dyn OffloadTransform>> {
    match content_type {
        ContentType::JsonArray => Some(Box::new(
            smart_crusher::SmartCrusher::new(smart_crusher::SmartCrusherConfig::default()),
        )),
        ContentType::BuildOutput => Some(Box::new(log_compressor::LogCompressor::new(
            log_compressor::LogCompressorConfig::default(),
        ))),
        ContentType::SearchResults => Some(Box::new(
            search_compressor::SearchCompressorTransform::with_defaults(),
        )),
        ContentType::GitDiff => Some(Box::new(
            diff_compressor::DiffCompressorTransform::with_defaults(),
        )),
        ContentType::PlainText => Some(Box::new(text_crusher::TextCrusher::new(
            text_crusher::TextCrusherConfig::default(),
        ))),
        ContentType::SourceCode => Some(Box::new(code_compressor::CodeAwareCompressor::new(
            code_compressor::CodeCompressorConfig::default(),
        ))),
        ContentType::Html => None,
    }
}

/// 按内容类型构造对应的无损重排器实例（用默认配置）。
/// 无损重排先于有损压缩执行：剥离空白/挖模板，不丢信息、无需 CCR。
pub fn reformat_for(content_type: ContentType) -> Option<Box<dyn ReformatTransform>> {
    match content_type {
        ContentType::JsonArray => Some(Box::new(reformats::JsonMinifier)),
        ContentType::BuildOutput => Some(Box::new(reformats::LogTemplate::new(
            reformats::LogTemplateConfig::default(),
        ))),
        ContentType::SearchResults
        | ContentType::GitDiff
        | ContentType::SourceCode
        | ContentType::PlainText
        | ContentType::Html => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_routes_by_type() {
        assert_eq!(dispatch_compressor("[1,2,3]"), Some("smart_crusher"));
        assert_eq!(dispatch_compressor("just text"), Some("text_crusher"));
        // 完整 HTML（带 doctype）才达到 Html 检测阈值 → no-op
        assert_eq!(
            dispatch_compressor("<!doctype html><html><head></head><body><p>hi</p></body></html>"),
            None
        );
    }

    #[test]
    fn compressor_for_matches_dispatch() {
        assert!(compressor_for(ContentType::JsonArray).is_some());
        assert!(compressor_for(ContentType::BuildOutput).is_some());
        assert!(compressor_for(ContentType::SearchResults).is_some());
        assert!(compressor_for(ContentType::GitDiff).is_some());
        assert!(compressor_for(ContentType::PlainText).is_some());
        assert!(compressor_for(ContentType::SourceCode).is_some());
        assert!(compressor_for(ContentType::Html).is_none());
    }

    #[test]
    fn reformat_for_covers_lossless_types() {
        assert!(reformat_for(ContentType::JsonArray).is_some());
        assert!(reformat_for(ContentType::BuildOutput).is_some());
        assert!(reformat_for(ContentType::PlainText).is_none());
    }
}
