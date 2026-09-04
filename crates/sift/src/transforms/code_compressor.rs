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

use crate::content::ContentType;
use crate::stash;
use crate::transforms::{
    CompressionContext, OffloadOutput, OffloadTransform, OmissionRange, TransformError,
};
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
    /// 每个函数体最多保留的完整语句行数（参考默认 max_body_lines=5）。
    pub max_body_lines: usize,
    /// 压缩后 token 低于原文的 5% 视为过度压缩，回退。
    pub min_output_ratio: f64,
}

impl Default for CodeCompressorConfig {
    fn default() -> Self {
        Self {
            min_tokens_for_compression: 100,
            max_body_lines: 5,
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
    /// 被折叠的原文连续行范围（1-based）。
    pub omissions: Vec<OmissionRange>,
    pub passthrough: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FunctionLayout {
    body_start: usize,
    body_end: usize,
    statements: Vec<(usize, usize)>,
    docstring: Option<(usize, usize)>,
}

fn function_layout(node: tree_sitter::Node, language: CodeLanguage) -> Option<FunctionLayout> {
    let body = node.child_by_field_name("body")?;
    let docstring = if language == CodeLanguage::Python {
        body.named_children(&mut body.walk())
            .find(|child| !is_comment_node(child.kind()))
            .and_then(|child| {
                let is_string = child.kind() == "string"
                    || (child.kind() == "expression_statement"
                        && child
                            .named_child(0)
                            .is_some_and(|node| node.kind() == "string"));
                is_string.then(|| (child.start_position().row, child.end_position().row))
            })
    } else {
        None
    };
    let mut statements = Vec::new();
    for child in body.children(&mut body.walk()) {
        if !child.is_named() || is_comment_node(child.kind()) {
            continue;
        }
        let range = (child.start_position().row, child.end_position().row);
        if Some(range) == docstring {
            continue;
        }
        if child.kind() == "statement_list" {
            for inner in child.children(&mut child.walk()) {
                if inner.is_named() && !is_comment_node(inner.kind()) {
                    statements.push((inner.start_position().row, inner.end_position().row));
                }
            }
        } else {
            statements.push((child.start_position().row, child.end_position().row));
        }
    }
    Some(FunctionLayout {
        body_start: body.start_position().row,
        body_end: body.end_position().row,
        statements,
        docstring,
    })
}

fn is_comment_node(kind: &str) -> bool {
    matches!(kind, "comment" | "line_comment" | "block_comment")
}

fn line_indent(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

fn first_line_docstring(lines: &[&str], start: usize, end: usize) -> String {
    let first = lines[start];
    if start == end {
        return first.to_string();
    }
    let indent = line_indent(first);
    let stripped = first.trim();
    let opener = ["r\"\"\"", "r'''", "\"\"\"", "'''"]
        .into_iter()
        .find(|candidate| stripped.starts_with(candidate));
    let Some(opener) = opener else {
        return first.to_string();
    };
    let quote = &opener[opener.len() - 3..];
    let first_content = stripped[opener.len()..]
        .trim()
        .strip_suffix(quote)
        .unwrap_or(stripped[opener.len()..].trim())
        .trim();
    if !first_content.is_empty() {
        return format!("{indent}{opener}{first_content}{quote}");
    }
    let second_content = lines
        .get(start + 1)
        .map(|line| line.trim())
        .unwrap_or_default()
        .strip_suffix(quote)
        .unwrap_or_else(|| {
            lines
                .get(start + 1)
                .map(|line| line.trim())
                .unwrap_or_default()
        })
        .trim();
    if second_content.is_empty() {
        first.to_string()
    } else {
        format!("{indent}{opener}{second_content}{quote}")
    }
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
    if joined.contains("function ") || joined.contains("const ") || joined.contains("=>") {
        return CodeLanguage::Javascript;
    }
    CodeLanguage::Unknown
}

/// 根据调用方提供的源文件路径识别语言。
///
/// 路径比短代码片段的内容启发式更可靠，也让只含一个长函数的文件能够稳定进入
/// AST 压缩。只接受明确映射到现有 tree-sitter grammar 的扩展名。
pub fn detect_language_from_path(path: &str) -> CodeLanguage {
    let extension = path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "py" | "pyi" => CodeLanguage::Python,
        "js" | "jsx" | "mjs" | "cjs" => CodeLanguage::Javascript,
        "ts" | "tsx" | "mts" | "cts" => CodeLanguage::Typescript,
        "go" => CodeLanguage::Go,
        "rs" => CodeLanguage::Rust,
        "java" => CodeLanguage::Java,
        "c" => CodeLanguage::C,
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => CodeLanguage::Cpp,
        _ => CodeLanguage::Unknown,
    }
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
    parser.set_language(&language.into()).map_err(|_| ()).ok()?;
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
        self.compress_with_hints(code, None, None, 0)
    }

    fn compress_with_hints(
        &self,
        code: &str,
        source_path: Option<&str>,
        stash_file_path: Option<&str>,
        stash_line_offset: usize,
    ) -> CodeCompressionResult {
        let passthrough = |lang: CodeLanguage| CodeCompressionResult {
            compressed: code.to_string(),
            language: lang,
            original_tokens: estimate_tokens(code),
            compressed_tokens: estimate_tokens(code),
            compression_ratio: 1.0,
            bodies_folded: 0,
            omissions: Vec::new(),
            passthrough: true,
        };

        if code.trim().is_empty() {
            return passthrough(CodeLanguage::Unknown);
        }
        let original_tokens = estimate_tokens(code);
        if original_tokens < self.config.min_tokens_for_compression {
            return passthrough(CodeLanguage::Unknown);
        }

        let lang = source_path
            .map(detect_language_from_path)
            .filter(|language| *language != CodeLanguage::Unknown)
            .unwrap_or_else(|| detect_language(code));
        let Some(cfg) = lang_config(lang) else {
            return passthrough(lang);
        };
        let Some(tree) = parse_code(code, lang) else {
            return passthrough(lang);
        };

        let mut lines: Vec<&str> = code.split('\n').collect();
        // 以 `\n` 结尾时 `split` 会产生一个空的尾元素；重建循环会给每行补 `\n`，
        // 不弹掉会把幻影行也输出，导致末尾多一个空行。只弹这一个，保留真实的结尾空行。
        if code.ends_with('\n') {
            lines.pop();
        }
        let root = tree.root_node();

        // 收集结构骨架：imports / package / 类型整段保留；类只保留头
        //（到 `{` 行），成员再深入——方法签名保留、长体折叠，字段等
        // 短成员整段保留；顶层函数签名保留、长体折叠。
        let mut pieces: Vec<(usize, usize, Option<FunctionLayout>)> = Vec::new();
        let mut stack = vec![(root, false)];
        while let Some((node, in_class)) = stack.pop() {
            let kind = node.kind();
            let start = node.start_position().row;
            let end = node.end_position().row;

            if cfg.package_node == Some(kind) || cfg.import_nodes.contains(&kind) {
                pieces.push((start, end, None));
                continue; // 不深入
            }
            if cfg.class_nodes.contains(&kind) {
                // 类头：从声明首行到含 `{` 的行（含注解/类名/泛型/extends）
                let header_end = node
                    .child_by_field_name("body")
                    .map(|body| body.start_position().row.saturating_sub(1))
                    .unwrap_or(end)
                    .max(start);
                pieces.push((start, header_end, None));
                // 类的收尾 `}` 行单独保留，维持语法合法
                if end > header_end {
                    pieces.push((end, end, None));
                }
                // 深入类体成员（方法可折叠，字段等保留）；
                // 跳过 class_body 容器本身，只推其子节点
                if let Some(body) = node.child_by_field_name("body") {
                    let n = body.child_count();
                    for i in (0..n).rev() {
                        if let Some(child) = body.child(i) {
                            stack.push((child, true));
                        }
                    }
                }
                continue;
            }
            if cfg.type_nodes.contains(&kind) {
                pieces.push((start, end, None));
                continue;
            }
            if cfg.function_nodes.contains(&kind) {
                pieces.push((start, end, function_layout(node, lang)));
                continue;
            }
            if in_class {
                // 类内非函数成员（字段 / 构造代码块等）：整段保留
                pieces.push((start, end, None));
                continue;
            }
            // 深入子节点（逆序 push 保持顺序）
            let n = node.child_count();
            for i in (0..n).rev() {
                if let Some(child) = node.child(i) {
                    stack.push((child, false));
                }
            }
        }
        pieces.sort();
        pieces.dedup();

        let mut out = String::with_capacity(code.len() / 2);
        let mut bodies_folded = 0usize;
        let mut omissions = Vec::new();
        let mut cursor = 0usize;
        for (start, end, function) in &pieces {
            let end = (*end).min(lines.len() - 1);
            if *start > cursor {
                for line in &lines[cursor..*start] {
                    out.push_str(line);
                    out.push('\n');
                }
            }
            if *start < cursor {
                continue;
            }
            let seg_lines = &lines[*start..=end];
            let Some(function) = function else {
                // 整段保留
                for l in seg_lines {
                    out.push_str(l);
                    out.push('\n');
                }
                cursor = end + 1;
                continue;
            };
            // 与 Headroom 一致：函数总行数不超过 body_limit + 签名/闭合两行时原样保留。
            if seg_lines.len() <= self.config.max_body_lines + 2 {
                for l in seg_lines {
                    out.push_str(l);
                    out.push('\n');
                }
                cursor = end + 1;
                continue;
            }
            // 只保留完整 AST 语句。即使首条语句超过预算也完整保留，绝不截断循环、
            // match 或多行表达式；后续语句超过剩余预算时停止。
            let mut kept_end = function.body_start.saturating_sub(1);
            let mut kept_lines = 0usize;
            for &(stmt_start, stmt_end) in &function.statements {
                let statement_lines = stmt_end.saturating_sub(stmt_start) + 1;
                if kept_lines > 0 && kept_lines + statement_lines > self.config.max_body_lines {
                    break;
                }
                kept_end = kept_end.max(stmt_end);
                kept_lines += statement_lines;
            }
            let brace_language = lang != CodeLanguage::Python;
            let closing_start = if brace_language {
                function.body_end
            } else {
                end + 1
            };
            let content_end = function
                .docstring
                .map_or(kept_end, |(_, doc_end)| kept_end.max(doc_end));
            let omission_start = content_end.saturating_add(1);
            let omission_end = closing_start.saturating_sub(1).min(end);
            let mut folded = false;
            if let Some((doc_start, doc_end)) = function.docstring {
                for l in &lines[*start..doc_start] {
                    out.push_str(l);
                    out.push('\n');
                }
                out.push_str(&first_line_docstring(&lines, doc_start, doc_end));
                out.push('\n');
                if doc_end > doc_start {
                    let range = OmissionRange {
                        start_line: doc_start + 1,
                        line_count: doc_end - doc_start + 1,
                    };
                    let indent = line_indent(lines[doc_start]);
                    if let Some(path) = super::line_omissions::actionable_file_path(stash_file_path)
                    {
                        let hint = super::line_omissions::hint(path, &range, stash_line_offset);
                        out.push_str(&format!("{indent}{} ... {hint}\n", cfg.comment_prefix));
                    } else {
                        out.push_str(&format!(
                            "{indent}{} ... {} lines omitted\n",
                            cfg.comment_prefix, range.line_count
                        ));
                    }
                    omissions.push(range);
                    folded = true;
                }
                if doc_end < kept_end.min(end) {
                    for l in &lines[doc_end + 1..=kept_end.min(end)] {
                        out.push_str(l);
                        out.push('\n');
                    }
                }
            } else {
                for l in &lines[*start..=kept_end.min(end)] {
                    out.push_str(l);
                    out.push('\n');
                }
            }
            let omitted = omission_end
                .checked_sub(omission_start)
                .map_or(0, |span| span + 1);
            if omitted > 0 {
                let indent = lines
                    .get(omission_start)
                    .or_else(|| lines.get(function.body_start))
                    .map_or("    ", |line| line_indent(line));
                if let Some(path) = super::line_omissions::actionable_file_path(stash_file_path) {
                    let range = OmissionRange {
                        start_line: omission_start + 1,
                        line_count: omitted,
                    };
                    let hint = super::line_omissions::hint(path, &range, stash_line_offset);
                    out.push_str(&format!("{indent}{} ... {hint}\n", cfg.comment_prefix));
                } else {
                    out.push_str(&format!(
                        "{indent}{} ... {} lines omitted\n",
                        cfg.comment_prefix, omitted
                    ));
                }
                // brace 语言保留闭合结构；Python 已保留至少一条完整语句。
                if brace_language {
                    for line in &lines[closing_start..=end] {
                        out.push_str(line);
                        out.push('\n');
                    }
                }
                omissions.push(OmissionRange {
                    start_line: omission_start + 1,
                    line_count: omitted,
                });
                folded = true;
            } else if brace_language {
                for line in &lines[closing_start..=end] {
                    out.push_str(line);
                    out.push('\n');
                }
            }
            if folded {
                bodies_folded += 1;
            }
            cursor = end + 1;
        }
        for line in &lines[cursor..] {
            out.push_str(line);
            out.push('\n');
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
            omissions,
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
        let indented = input
            .lines()
            .filter(|l| l.starts_with(' ') || l.starts_with('\t'))
            .count();
        indented as f64 / total as f64
    }

    fn cache_key(&self, input: &str) -> String {
        stash::compute_key(input)
    }

    fn apply(
        &self,
        input: &str,
        ctx: &CompressionContext,
    ) -> Result<OffloadOutput, TransformError> {
        let result = self.compress_with_hints(
            input,
            ctx.source_path.as_deref(),
            ctx.stash_file_path.as_deref(),
            ctx.stash_line_offset,
        );
        if result.passthrough || result.compressed == input {
            return Err(TransformError::Skipped);
        }
        Ok(OffloadOutput {
            compressed: result.compressed,
            original: input.to_string(),
            omissions: result.omissions,
        })
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
        assert_eq!(
            r.omissions,
            vec![OmissionRange {
                start_line: 11,
                line_count: 26,
            }]
        );
        for i in 0..5 {
            assert!(r.compressed.contains(&format!("value_{i} =")));
        }
        assert!(!r.compressed.contains("value_5 ="));
        assert!(r.compressed.contains("def helper"));
        assert!(r.compressed.contains("import os"));
    }

    #[test]
    fn nested_python_omission_uses_original_body_indent() {
        let mut code = String::from("class Service:\n    def calculate(self, x):\n");
        for i in 0..20 {
            code.push_str(&format!("        value_{i} = x + {i}\n"));
        }
        code.push_str("        return value_0\n");

        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let result = c.compress(&code);

        assert!(!result.passthrough);
        assert!(result.compressed.contains("\n        # ... "));
    }

    #[test]
    fn python_docstring_keeps_first_line_without_spending_body_budget() {
        let mut code = String::from(
            "def calculate(x):\n    \"\"\"Explain the calculation.\n\n    This detail is intentionally long.\n    It should be recoverable from stash.\n    \"\"\"\n",
        );
        for i in 0..20 {
            code.push_str(&format!("    value_{i} = x + {i}\n"));
        }
        code.push_str("    return value_0\n");

        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let result = c.compress(&code);

        assert!(!result.passthrough);
        assert!(result
            .compressed
            .contains("    \"\"\"Explain the calculation.\"\"\""));
        assert!(!result
            .compressed
            .contains("This detail is intentionally long"));
        for i in 0..5 {
            assert!(result.compressed.contains(&format!("value_{i} =")));
        }
        assert!(!result.compressed.contains("value_5 ="));
        assert_eq!(result.omissions.len(), 2);
    }

    #[test]
    fn trailing_newline_input_does_not_gain_blank_line() {
        // 回归：`long_python()` 以 '\n' 结尾，输出不得再补一个幻影空行。
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let r = c.compress(&long_python());
        assert!(!r.passthrough);
        assert!(r.compressed.ends_with("    return helper(2)\n"));
        assert!(!r.compressed.ends_with("\n\n"));
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
        let ctx = CompressionContext {
            source_path: Some("/workspace/app.py".to_string()),
            stash_file_path: Some("/tmp/sift-stash/0123456789abcdef01234567".to_string()),
            ..CompressionContext::default()
        };
        let result = c.apply(&long_python(), &ctx).unwrap();
        assert!(result
            .compressed
            .contains(r#"# ... 26 lines omitted from file "/tmp/sift-stash/0123456789abcdef01234567", starting at line 11"#));
        assert_eq!(result.original, long_python());
        assert_eq!(result.omissions.len(), 1);
    }

    #[test]
    fn cache_key_and_bloat_deterministic() {
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let code = long_python();
        assert_eq!(c.cache_key(&code), c.cache_key(&code));
        assert!(c.estimate_bloat(&code) > 0.0);
    }

    #[test]
    fn folds_long_java_method_bodies() {
        let mut s = String::from(
            "import java.util.List;\n\npublic class Service {\n    private final Repo repo;\n\n    public void big(int x) {\n",
        );
        for i in 0..25 {
            s.push_str(&format!("        int v{i} = x * {i};\n"));
        }
        s.push_str("        return;\n    }\n}\n");
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let r = c.compress(&s);
        assert!(!r.passthrough, "ratio={}", r.compression_ratio);
        assert_eq!(r.language, CodeLanguage::Java);
        assert!(r.compressed.contains("lines omitted"));
        assert!(r.compressed.contains("public class Service"));
        assert!(r.compressed.contains("public void big"));
        assert!(r.compressed.contains("import java.util.List"));
    }

    #[test]
    fn folds_annotated_java_class_without_dropping_declaration() {
        let mut s = String::from(
            "package com.example.orders;\n\nimport org.springframework.stereotype.Service;\n\n@Service\npublic class OrderService {\n    private final Repo repo;\n\n    @Transactional\n    public void createOrder(int x) {\n",
        );
        for i in 0..30 {
            s.push_str(&format!("        int value{i} = x + {i};\n"));
        }
        s.push_str("        repo.save(x);\n    }\n}\n");

        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let r = c.compress(&s);

        assert!(!r.passthrough, "annotated Java class should compress");
        assert!(r
            .compressed
            .contains("@Service\npublic class OrderService {"));
        assert!(r.compressed.contains("public void createOrder"));
        assert!(r.compressed.contains("lines omitted"));
    }

    #[test]
    fn compresses_official_java_demo_fixture() {
        let code = include_str!("../../tests/fixtures/order_service.java");
        let c = CodeAwareCompressor::new(CodeCompressorConfig::default());
        let r = c.compress(code);

        assert!(
            !r.passthrough,
            "official Java demo should compress, output={}",
            r.compressed
        );
        assert!(r.compressed.contains("public class OrderService"));
        assert!(r.bodies_folded >= 1);
    }

    #[test]
    fn go_and_java_detected() {
        assert_eq!(
            detect_language("package main\n\nfunc main() {}"),
            CodeLanguage::Go
        );
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
