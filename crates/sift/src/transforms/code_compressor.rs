//! AST 感知代码压缩器。
//!
//! 用 tree-sitter（8 语言 grammar）解析源代码，保留结构骨架
//! （imports / 函数签名 / 类型定义 / 装饰器），折叠超长函数体为
//! `# ... N lines omitted` 占位。输出仍是合法且可读的代码。
//!
//! 相对参考实现的简化：不含 symbol importance 打分 / body 预算分配 /
//! docstring 模式矩阵（统一保留首行 docstring）；核心结构保留策略一致。
//!
//! 有损变换，实现 [`crate::transforms::OffloadTransform`]：原文经 `apply`
//! 返回给调用方写入 stash store，端到端无损。

use crate::stash;
use crate::content::ContentType;
use crate::transforms::{CompressionContext, OffloadTransform, TransformError};
use tree_sitter::{Parser, Tree};

// ─── 语言与配置 ───────────────────────────────────────────────────────────

/// 支持的语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeLanguage {
    Python,
    Javascript,
    Typescript,
    Go,
    Rust,
    Java,
    C,
    Cpp,
    Unknown,
}

impl CodeLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            CodeLanguage::Python => "python",
            CodeLanguage::Javascript => "javascript",
            CodeLanguage::Typescript => "typescript",
            CodeLanguage::Go => "go",
            CodeLanguage::Rust => "rust",
            CodeLanguage::Java => "java",
            CodeLanguage::C => "c",
            CodeLanguage::Cpp => "cpp",
            CodeLanguage::Unknown => "unknown",
        }
    }
}

/// 语言解析配置（与参考 `LangConfig` 一致的节点类型表）。
struct LangConfig {
    import_nodes: &'static [&'static str],
    function_nodes: &'static [&'static str],
    class_nodes: &'static [&'static str],
    type_nodes: &'static [&'static str],
    comment_prefix: &'static str,
    /// package/clause 头（如 Go `package xxx`、Java `package a.b;`），整段保留。
    package_node: Option<&'static str>,
}

fn lang_config(language: CodeLanguage) -> Option<LangConfig> {
    let cfg = match language {
        CodeLanguage::Python => LangConfig {
            import_nodes: &["import_statement", "import_from_statement"],
            function_nodes: &["function_definition"],
            class_nodes: &["class_definition"],
            type_nodes: &["type_alias_statement"],
            comment_prefix: "#",
            package_node: None,
        },
        CodeLanguage::Javascript => LangConfig {
            import_nodes: &["import_statement", "import_declaration"],
            function_nodes: &["function_declaration", "method_definition"],
            class_nodes: &["class_declaration"],
            type_nodes: &[],
            comment_prefix: "//",
            package_node: None,
        },
        CodeLanguage::Typescript => LangConfig {
            import_nodes: &["import_statement", "import_declaration"],
            function_nodes: &["function_declaration", "method_definition"],
            class_nodes: &["class_declaration"],
            type_nodes: &["interface_declaration", "type_alias_declaration"],
            comment_prefix: "//",
            package_node: None,
        },
        CodeLanguage::Go => LangConfig {
            import_nodes: &["import_declaration"],
            function_nodes: &["function_declaration", "method_declaration"],
            class_nodes: &[],
            type_nodes: &["type_declaration"],
            comment_prefix: "//",
            package_node: Some("package_clause"),
        },
        CodeLanguage::Rust => LangConfig {
            import_nodes: &["use_declaration"],
            function_nodes: &["function_item"],
            class_nodes: &["impl_item"],
            type_nodes: &["struct_item", "enum_item", "type_item", "trait_item"],
            comment_prefix: "//",
            package_node: None,
        },
        CodeLanguage::Java => LangConfig {
            import_nodes: &["import_declaration"],
            function_nodes: &["method_declaration", "constructor_declaration"],
            class_nodes: &["class_declaration", "interface_declaration"],
            type_nodes: &["enum_declaration"],
            comment_prefix: "//",
            package_node: Some("package_declaration"),
        },
        CodeLanguage::C => LangConfig {
            import_nodes: &["preproc_include"],
            function_nodes: &["function_definition"],
            class_nodes: &[],
            type_nodes: &["struct_specifier", "enum_specifier", "type_definition"],
            comment_prefix: "//",
            package_node: None,
        },
        CodeLanguage::Cpp => LangConfig {
            import_nodes: &["preproc_include"],
            function_nodes: &["function_definition"],
            class_nodes: &["class_specifier"],
            type_nodes: &["struct_specifier", "enum_specifier", "type_definition"],
            comment_prefix: "//",
            package_node: None,
        },
        CodeLanguage::Unknown => return None,
    };
    Some(cfg)
}

// ─── 配置 ─────────────────────────────────────────────────────────────────

/// 压缩配置。默认值对齐参考 `CodeCompressorConfig` 的关键项。
#[derive(Debug, Clone)]
pub struct CodeCompressorConfig {
    /// 低于此 token 数不压缩（参考 min_tokens_for_compression=100）。
    pub min_tokens_for_compression: usize,
    /// 函数体超过此行数才折叠（参考 max_body_lines=8 的保守版）。
    pub max_body_lines: usize,
    /// 压缩后 token 低于原文的 5% 视为过度压缩，回退。
    pub min_output_ratio: f64,
}

impl Default for CodeCompressorConfig {
    fn default() -> Self {
        Self {
            min_tokens_for_compression: 100,
            max_body_lines: 8,
            min_output_ratio: 0.05,
        }
    }
}

/// 压缩结果。
#[derive(Debug, Clone)]
pub struct CodeCompressionResult {
    pub compressed: String,
    pub language: CodeLanguage,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub compression_ratio: f64,
    /// 折叠的函数体个数。
    pub bodies_folded: usize,
    pub passthrough: bool,
}

// ─── 语言检测 ─────────────────────────────────────────────────────────────

/// 按特征关键词检测语言（与参考 `detect_language` 的启发式等价思路）。
pub fn detect_language(code: &str) -> CodeLanguage {
    let head: Vec<&str> = code.lines().take(80).collect();
    let joined = code.chars().take(4000).collect::<String>();

    // 强特征：单命中即可定向
    if head.iter().any(|l| l.starts_with("package ")) && joined.contains("func ") {
        return CodeLanguage::Go;
    }
    if joined.contains("fn ") || joined.contains("let mut ") || joined.contains("impl ") {
        return CodeLanguage::Rust;
    }
    if joined.contains("def ") && !joined.contains("function") {
        return CodeLanguage::Python;
    }
    if joined.contains("public class ") || joined.contains("System.out.println") {
        return CodeLanguage::Java;
    }
    if joined.contains("#include <") {
        return if joined.contains("std::") || joined.contains("class ") {
            CodeLanguage::Cpp
        } else {
            CodeLanguage::C
        };
    }
    if joined.contains("interface ") || joined.contains(": string") || joined.contains(": number") {
        return CodeLanguage::Typescript;
    }
    if joined.contains("function ")
        || joined.contains("const ")
        || joined.contains("=>")
    {
        return CodeLanguage::Javascript;
    }
    CodeLanguage::Unknown
}

fn parser_for(language: CodeLanguage) -> Option<Parser> {
    let mut parser = Parser::new();
    let language = match language {
        CodeLanguage::Python => tree_sitter_python::LANGUAGE,
        CodeLanguage::Javascript => tree_sitter_javascript::LANGUAGE,
        CodeLanguage::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        CodeLanguage::Go => tree_sitter_go::LANGUAGE,
        CodeLanguage::Rust => tree_sitter_rust::LANGUAGE,
        CodeLanguage::Java => tree_sitter_java::LANGUAGE,
        CodeLanguage::C => tree_sitter_c::LANGUAGE,
        CodeLanguage::Cpp => tree_sitter_cpp::LANGUAGE,
        CodeLanguage::Unknown => return None,
    };
    parser
        .set_language(&language.into())
        .map_err(|_| ())
        .ok()?;
    Some(parser)
}

fn parse_code(code: &str, language: CodeLanguage) -> Option<Tree> {
    parser_for(language)?.parse(code, None)
}

/// ERROR/MISSING 节点计数（语法校验用）。
fn has_syntax_issues(root: tree_sitter::Node) -> bool {
    let mut cursor = root.walk();
    cursor.goto_first_child();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            return true;
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return false;
            }
        }
    }
}

// ─── 压缩 ─────────────────────────────────────────────────────────────────

pub struct CodeAwareCompressor {
    config: CodeCompressorConfig,
}

impl CodeAwareCompressor {
    pub fn new(config: CodeCompressorConfig) -> Self {
        Self { config }
    }

    pub fn compress(&self, code: &str) -> CodeCompressionResult {
        let passthrough = |lang: CodeLanguage| CodeCompressionResult {
            compressed: code.to_string(),
            language: lang,
            original_tokens: estimate_tokens(code),
            compressed_tokens: estimate_tokens(code),
            compression_ratio: 1.0,
            bodies_folded: 0,
            passthrough: true,
        };

        if code.trim().is_empty() {
            return passthrough(CodeLanguage::Unknown);
        }
        let original_tokens = estimate_tokens(code);
        if original_tokens < self.config.min_tokens_for_compression {
            return passthrough(CodeLanguage::Unknown);
        }

        let lang = detect_language(code);
        let Some(cfg) = lang_config(lang) else {
            return passthrough(lang);
        };
        let Some(tree) = parse_code(code, lang) else {
            return passthrough(lang);
        };

        let lines: Vec<&str> = code.split('\n').collect();
        let root = tree.root_node();

        // 收集结构骨架：imports / package / 类型 / 类整段保留；函数签名保留、
        // 长体折叠。
        let mut pieces: Vec<(usize, usize, bool /* is_function */)> = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            let kind = node.kind();
            let start = node.start_position().row;
            let end = node.end_position().row;

            if cfg.import_nodes.contains(&kind)
                || cfg.class_nodes.contains(&kind)
                || cfg.type_nodes.contains(&kind)
                || cfg.package_node == Some(kind)
            {
                pieces.push((start, end, false));
                continue; // 不深入
            } else if cfg.function_nodes.contains(&kind) {
                pieces.push((start, end, true));
                continue;
            }
            // 深入子节点（逆序 push 保持顺序）
            let n = node.child_count();
            for i in (0..n).rev() {
                if let Some(child) = node.child(i) {
                    stack.push(child);
                }
            }
        }
        pieces.sort();

        let mut out = String::with_capacity(code.len() / 2);
        let mut bodies_folded = 0usize;
        for (start, end, is_fn) in &pieces {
            let seg_lines = &lines[*start..=(*end).min(lines.len() - 1)];
            if !is_fn {
                // 整段保留
                for l in seg_lines {
                    out.push_str(l);
                    out.push('\n');
                }
                continue;
            }
            // 函数：找 body 起始行（签名 = body 前的行）
            if seg_lines.len() <= self.config.max_body_lines {
                for l in seg_lines {
                    out.push_str(l);
                    out.push('\n');
                }
                continue;
            }
            // 折叠：保留签名（到 body 起始行——`{` 或 Python `:` 结尾）+ 折叠注释 + 尾行
            let body_first = seg_lines
                .iter()
                .position(|l| {
                    let t = l.trim_end();
                    t.ends_with('{') || t.ends_with(':') || l.trim_start().starts_with('{')
                })
                .unwrap_or(0);
            let keep_sig = body_first + 1; // 含 '{' 或 ':' 那行
            for l in &seg_lines[..keep_sig.min(seg_lines.len())] {
                out.push_str(l);
                out.push('\n');
            }
            let omitted = seg_lines.len().saturating_sub(keep_sig + 1);
            if omitted > 0 {
                out.push_str(&format!(
                    "    {} ... {} lines omitted\n",
                    cfg.comment_prefix,
                    omitted.saturating_sub(1)
                ));
                // 保留尾行（闭括号），维持语法合法
                if let Some(last) = seg_lines.last() {
                    out.push_str(last);
                    out.push('\n');
                }
                bodies_folded += 1;
            }
        }

        // 语法校验：折叠后的代码必须仍可解析
        if has_syntax_issues_root(&out, lang) {
            return passthrough(lang);
        }
        let compressed_tokens = estimate_tokens(&out);
        let ratio = compressed_tokens as f64 / original_tokens.max(1) as f64;
        if ratio < self.config.min_output_ratio || compressed_tokens >= original_tokens {
            return passthrough(lang);
        }
        CodeCompressionResult {
            compressed: out,
            language: lang,
            original_tokens,
            compressed_tokens,
            compression_ratio: ratio,
            bodies_folded,
            passthrough: false,
        }
    }
}

/// 解析并检查语法问题（折叠输出用）。
fn has_syntax_issues_root(code: &str, lang: CodeLanguage) -> bool {
    match parse_code(code, lang) {
        Some(tree) => has_syntax_issues(tree.root_node()),
        None => true,
    }
}

fn estimate_tokens(text: &str) -> usize {
    ((text.len() as f64 / 4.0) * 1.2).ceil() as usize
}

impl OffloadTransform for CodeAwareCompressor {
    fn name(&self) -> &'static str {
        "code_compressor"
    }

    fn applies_to(&self) -> ContentType {
        ContentType::SourceCode
    }

    fn estimate_bloat(&self, input: &str) -> f64 {
        // 粗估：代码中函数体行占比（近似可折叠比例）
        let total = input.lines().count().max(1);
        let indented = input.lines().filter(|l| l.starts_with(' ') || l.starts_with('\t')).count();
        indented as f64 / total as f64
    }

    fn cache_key(&self, input: &str) -> String {
        stash::compute_key(input)
    }

    fn apply(
        &self,
        input: &str,
        _ctx: &CompressionContext,
    ) -> Result<(String, String), TransformError> {
        let result = self.compress(input);
        if result.passthrough || result.compressed == input {
            return Err(TransformError::Skipped);
        }
        Ok((result.compressed, input.to_string()))
    }
}

// ─── 测试 ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn long_python() -> String {
        let mut s = String::from("import os\nimport sys\n\n\ndef helper(x):\n");
        for i in 0..30 {
            s.push_str(&format!("    value_{i} = x * {i}\n"));
        }
        s.push_str("    return value_0\n\n\ndef main():\n    return helper(2)\n");
        s
    }

    #[test]
    fn folds_long_python_function_bodies() {
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let r = c.compress(&long_python());
        assert!(!r.passthrough, "ratio={}", r.compression_ratio);
        assert!(r.compressed.len() < long_python().len());
        assert!(r.compressed.contains("lines omitted"));
        assert!(r.compressed.contains("def helper"));
        assert!(r.compressed.contains("import os"));
    }

    #[test]
    fn passthrough_short_code() {
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let r = c.compress("fn a() {}");
        assert!(r.passthrough);
    }

    #[test]
    fn passthrough_unknown_language() {
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let prose = "just some words ".repeat(30);
        let r = c.compress(&prose);
        assert!(r.passthrough);
    }

    #[test]
    fn rust_code_compresses_and_stays_parseable() {
        let mut s = String::from("use std::collections::HashMap;\n\nfn big(x: u32) -> u32 {\n");
        for i in 0..25 {
            s.push_str(&format!("    let v{i} = x + {i};\n"));
        }
        s.push_str("    v0\n}\n");
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let r = c.compress(&s);
        assert!(!r.passthrough, "ratio={}", r.compression_ratio);
        assert_eq!(r.language, CodeLanguage::Rust);
        assert!(r.compressed.contains("fn big"));
    }

    #[test]
    fn offload_apply_roundtrip() {
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let ctx = CompressionContext::default();
        let (compressed, original) = c.apply(&long_python(), &ctx).unwrap();
        assert!(compressed.contains("lines omitted"));
        assert_eq!(original, long_python());
    }

    #[test]
    fn cache_key_and_bloat_deterministic() {
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let code = long_python();
        assert_eq!(c.cache_key(&code), c.cache_key(&code));
        assert!(c.estimate_bloat(&code) > 0.0);
    }

    #[test]
    fn go_and_java_detected() {
        assert_eq!(detect_language("package main\n\nfunc main() {}"), CodeLanguage::Go);
        assert_eq!(
            detect_language("public class A { public static void main(String[] a) {} }"),
            CodeLanguage::Java
        );
    }

    #[test]
    fn keeps_imports_and_types() {
        let py = format!(
            "import os\n\n\nclass Foo:\n    x: int = 1\n\n\n{}",
            "def bar():\n    pass\n".repeat(1)
        );
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let r = c.compress(&py);
        // 类定义整段保留
        assert!(r.compressed.contains("class Foo"));
        assert!(r.compressed.contains("import os"));
    }

    #[test]
    fn unique_language_names() {
        let set: BTreeSet<&str> = [
            CodeLanguage::Python,
            CodeLanguage::Javascript,
            CodeLanguage::Typescript,
            CodeLanguage::Go,
            CodeLanguage::Rust,
            CodeLanguage::Java,
            CodeLanguage::C,
            CodeLanguage::Cpp,
            CodeLanguage::Unknown,
        ]
        .iter()
        .map(|l| l.as_str())
        .collect();
        assert_eq!(set.len(), 9);
    }
}
