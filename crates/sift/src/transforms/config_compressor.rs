//! YAML/TOML/INI 配置压缩器。
//!
//! 对齐 Headroom ConfigCompressor 的安全注释层：仅删除整行注释与空行，
//! 配置键值和顺序字节不动；YAML block scalar 与 TOML 多行字符串直接回退。
//! 完整原文由统一 stash 管线保存。

use crate::content::{detect_config_flavor, ConfigFlavor, ContentType};
use crate::stash;
use crate::transforms::{
    CompressionContext, OffloadOutput, OffloadTransform, OmissionRange, TransformError,
};

#[derive(Debug, Clone)]
pub struct ConfigCompressorConfig {
    pub min_savings_bytes: usize,
}

impl Default for ConfigCompressorConfig {
    fn default() -> Self {
        Self {
            min_savings_bytes: 1,
        }
    }
}

pub struct ConfigCompressor {
    config: ConfigCompressorConfig,
}

impl ConfigCompressor {
    pub fn new(config: ConfigCompressorConfig) -> Self {
        Self { config }
    }

    fn compress(&self, input: &str) -> Result<OffloadOutput, TransformError> {
        let flavor = detect_config_flavor(input).ok_or(TransformError::InvalidInput)?;
        if !elision_safe(input, flavor) {
            return Err(TransformError::Skipped);
        }

        let trailing_newline = input.ends_with('\n');
        let lines: Vec<&str> = input.lines().collect();
        let mut kept = Vec::with_capacity(lines.len());
        let mut omissions = Vec::new();
        let mut omitted_start = None;
        let mut omitted_count = 0usize;

        for (index, line) in lines.iter().enumerate() {
            let omit = should_elide(line, flavor);
            if omit {
                omitted_start.get_or_insert(index + 1);
                omitted_count += 1;
            } else {
                if let Some(start_line) = omitted_start.take() {
                    omissions.push(OmissionRange {
                        start_line,
                        line_count: omitted_count,
                    });
                    omitted_count = 0;
                }
                kept.push(*line);
            }
        }
        if let Some(start_line) = omitted_start {
            omissions.push(OmissionRange {
                start_line,
                line_count: omitted_count,
            });
        }
        let elided = omissions
            .iter()
            .map(|range| range.line_count)
            .sum::<usize>();
        if elided == 0 {
            return Err(TransformError::Skipped);
        }

        let mut compressed = kept.join("\n");
        if trailing_newline && !compressed.is_empty() {
            compressed.push('\n');
        }
        compressed.push_str(&format!("[{elided} comment/blank lines elided]"));
        if input.len().saturating_sub(compressed.len()) < self.config.min_savings_bytes {
            return Err(TransformError::Skipped);
        }
        Ok(OffloadOutput {
            compressed,
            original: input.to_string(),
            omissions,
            deferred_stashes: Vec::new(),
        })
    }
}

impl OffloadTransform for ConfigCompressor {
    fn name(&self) -> &'static str {
        "config_compressor"
    }

    fn applies_to(&self) -> ContentType {
        ContentType::StructuredConfig
    }

    fn estimate_bloat(&self, input: &str) -> f64 {
        let Some(flavor) = detect_config_flavor(input) else {
            return 0.0;
        };
        let total = input.lines().count().max(1);
        let removable = input
            .lines()
            .filter(|line| should_elide(line, flavor))
            .count();
        removable as f64 / total as f64
    }

    fn cache_key(&self, input: &str) -> String {
        stash::compute_key(input)
    }

    fn apply(
        &self,
        input: &str,
        _ctx: &CompressionContext,
    ) -> Result<OffloadOutput, TransformError> {
        self.compress(input)
    }
}

fn should_elide(line: &str, flavor: ConfigFlavor) -> bool {
    match flavor {
        ConfigFlavor::Yaml | ConfigFlavor::Toml => {
            line.trim().is_empty() || line.trim_start().starts_with('#')
        }
        // INI 的缩进续行可能是值，空行也可能属于多行值；只删列 0 注释。
        ConfigFlavor::Ini => line.starts_with(['#', ';']),
    }
}

fn elision_safe(input: &str, flavor: ConfigFlavor) -> bool {
    match flavor {
        ConfigFlavor::Yaml => !input.lines().any(|line| {
            line.split_once(':').is_some_and(|(_, tail)| {
                let indicator = tail.trim();
                indicator.starts_with('|') || indicator.starts_with('>')
            })
        }),
        ConfigFlavor::Toml => !input.contains("\"\"\"") && !input.contains("'''"),
        ConfigFlavor::Ini => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_elides_only_comments_and_blanks() {
        let input = include_str!("../../tests/fixtures/deployment_config.yaml");
        let result = ConfigCompressor::new(ConfigCompressorConfig::default())
            .compress(input)
            .unwrap();

        assert!(result.compressed.contains("name: context-api"));
        assert!(result.compressed.contains("maxDelayMs: 2000"));
        assert!(!result.compressed.contains("Container settings"));
        assert!(result.compressed.contains("comment/blank lines elided"));
        assert!(!result.omissions.is_empty());
    }

    #[test]
    fn block_scalar_and_toml_multiline_strings_are_untouched() {
        let compressor = ConfigCompressor::new(ConfigCompressorConfig::default());
        let yaml = "script: |\n  # data\n  echo ok\nmetadata:\n  owner: team\n  enabled: true\n";
        assert!(matches!(
            compressor.compress(yaml),
            Err(TransformError::Skipped)
        ));

        let toml = "[package]\nname = \"sift\"\ndescription = \"\"\"long\n# data\ntext\"\"\"\nversion = \"1\"\n";
        assert!(matches!(
            compressor.compress(toml),
            Err(TransformError::Skipped)
        ));
    }

    #[test]
    fn ini_keeps_blanks_and_indented_continuations() {
        let input = "; this long explanatory comment is safe to remove and recover from stash later\n[server]\nhost = localhost\nport = 8080\n\n[message]\ntext = first line\n  # continuation data\n";
        let result = ConfigCompressor::new(ConfigCompressorConfig::default())
            .compress(input)
            .unwrap();
        assert!(result.compressed.contains("\n\n[message]"));
        assert!(result.compressed.contains("  # continuation data"));
        assert!(!result.compressed.contains("remove me"));
    }
}
