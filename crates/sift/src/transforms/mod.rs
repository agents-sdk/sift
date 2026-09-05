//! 压缩变换：trait 边界 + 具体压缩器。
//!

pub mod code_compressor;
pub mod config_compressor;
pub mod diff_compressor;
mod diff_noise;
pub mod html_extractor;
mod json_compactor;
mod line_omissions;
pub mod log_compressor;
mod log_context;
pub mod reformats;
pub mod search_compressor;
pub mod smart_crusher;
pub mod tabular_compressor;
pub mod tag_protector;
mod text_blocks;
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

/// 压缩上下文：query、token 预算、源码 grammar 提示路径，以及框架为完整原文
/// 分配的本地 stash 文件路径和已验证的分段行偏移。
#[derive(Debug, Clone, Default)]
pub struct CompressionContext {
    pub query: Option<String>,
    pub token_budget: Option<usize>,
    pub source_path: Option<String>,
    /// 仅当 stash 后端提供本地文件、且压缩器视图与 stash 原文逐行一致时设置。
    pub stash_file_path: Option<String>,
    /// 混合内容整行分段相对完整 stash 的行偏移；非精确分段不得携带文件路径。
    pub stash_line_offset: usize,
}

/// 有损压缩明确丢弃的连续原文行范围。行号从 1 开始，并且只允许指向
/// `OffloadOutput::original` 中的精确位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmissionRange {
    pub start_line: usize,
    pub line_count: usize,
}

/// 压缩正文内嵌 marker 对应的附加 stash 写入；调用方必须先持久化这些内容，
/// 才能发布压缩结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredStash {
    pub key: String,
    pub content: String,
}

/// 有损压缩结果。可证明逐行映射的压缩器填充 `omissions`；JSON 结构重写等
/// 尚无精确坐标的路径不填写，不能将对象数、句子数冒充省略行数。
#[derive(Debug, Clone)]
pub struct OffloadOutput {
    pub compressed: String,
    pub original: String,
    pub omissions: Vec<OmissionRange>,
    pub deferred_stashes: Vec<DeferredStash>,
}

impl OffloadOutput {
    pub fn new(compressed: String, original: String) -> Self {
        Self {
            compressed,
            original,
            omissions: Vec::new(),
            deferred_stashes: Vec::new(),
        }
    }
}

/// 无损重排变换（输出可完全重建原文，如 JSON 空白剥离、日志模板挖掘）。
pub trait ReformatTransform: Send + Sync {
    fn name(&self) -> &'static str;
    fn applies_to(&self) -> ContentType;
    /// 无损结果可直接短路的最大输出/输入比例。
    fn max_output_ratio(&self) -> f64 {
        0.8
    }
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
    fn apply(&self, input: &str, ctx: &CompressionContext)
        -> Result<OffloadOutput, TransformError>;
}

/// 按内容类型分发到压缩器。live_zone 的核心调度点。
/// 返回 None 表示该类型当前没有压缩器。
pub fn dispatch_compressor(text: &str) -> Option<&'static str> {
    match crate::content::detect_content_type(text) {
        ContentType::JsonArray => Some("smart_crusher"),
        ContentType::BuildOutput => Some("log_compressor"),
        ContentType::SearchResults => Some("search_compressor"),
        ContentType::GitDiff => Some("diff_compressor"),
        ContentType::PlainText => Some("text_crusher"),
        ContentType::SourceCode => Some("code_compressor"),
        ContentType::Html => Some("html_extractor"),
        ContentType::StructuredConfig => Some("config_compressor"),
        ContentType::Tabular => Some("tabular_compressor"),
    }
}

/// 按内容类型构造对应的有损压缩器实例（用默认配置）。
/// 返回 None 表示该类型无可压的压缩器。
pub fn compressor_for(content_type: ContentType) -> Option<Box<dyn OffloadTransform>> {
    match content_type {
        ContentType::JsonArray => Some(Box::new(smart_crusher::SmartCrusher::new(
            smart_crusher::SmartCrusherConfig::default(),
        ))),
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
        ContentType::Html => Some(Box::new(html_extractor::HtmlExtractor::new(
            html_extractor::HtmlExtractorConfig::default(),
        ))),
        ContentType::StructuredConfig => Some(Box::new(config_compressor::ConfigCompressor::new(
            config_compressor::ConfigCompressorConfig::default(),
        ))),
        ContentType::Tabular => Some(Box::new(tabular_compressor::TabularCompressor::new(
            tabular_compressor::TabularCompressorConfig::default(),
        ))),
    }
}

/// 按内容类型构造对应的无损重排器实例（用默认配置）。
/// 无损重排先于有损压缩执行：剥离空白/挖模板，不丢信息、无需 stash。
pub fn reformat_for(content_type: ContentType) -> Option<Box<dyn ReformatTransform>> {
    match content_type {
        ContentType::JsonArray => Some(Box::new(reformats::JsonReformatter::default())),
        ContentType::BuildOutput => Some(Box::new(reformats::LogTemplate::new(
            reformats::LogTemplateConfig::default(),
        ))),
        ContentType::SearchResults
        | ContentType::GitDiff
        | ContentType::SourceCode
        | ContentType::PlainText
        | ContentType::Html
        | ContentType::StructuredConfig
        | ContentType::Tabular => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_routes_by_type() {
        assert_eq!(dispatch_compressor("[1,2,3]"), Some("smart_crusher"));
        assert_eq!(dispatch_compressor("just text"), Some("text_crusher"));
        // 完整 HTML（带 doctype）进入正文提取。
        assert_eq!(
            dispatch_compressor("<!doctype html><html><head></head><body><p>hi</p></body></html>"),
            Some("html_extractor")
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
        assert!(compressor_for(ContentType::Html).is_some());
        assert!(compressor_for(ContentType::StructuredConfig).is_some());
        assert!(compressor_for(ContentType::Tabular).is_some());
    }

    #[test]
    fn reformat_for_covers_lossless_types() {
        assert!(reformat_for(ContentType::JsonArray).is_some());
        assert!(reformat_for(ContentType::BuildOutput).is_some());
        assert!(reformat_for(ContentType::PlainText).is_none());
    }
}
