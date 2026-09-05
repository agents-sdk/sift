//! HTML 正文提取器：保留文章主体的可读 Markdown，移除页面结构噪声。
//!
//! 参考 Headroom 的 HTMLExtractor 行为：优先选择 article/main/body，过滤
//! script/style/nav/header/footer/aside 等非正文节点。变换有损，完整 HTML 由
//! 上层写入 stash；HTML 重排没有可靠的原始行映射，因此不伪造行范围。

use crate::content::ContentType;
use crate::stash;
use crate::transforms::{CompressionContext, OffloadOutput, OffloadTransform, TransformError};

#[derive(Debug, Clone)]
pub struct HtmlExtractorConfig {
    pub include_links: bool,
    pub include_images: bool,
    pub include_tables: bool,
}

impl Default for HtmlExtractorConfig {
    fn default() -> Self {
        Self {
            include_links: true,
            include_images: false,
            include_tables: true,
        }
    }
}

pub struct HtmlExtractor {
    config: HtmlExtractorConfig,
}

impl HtmlExtractor {
    pub fn new(config: HtmlExtractorConfig) -> Self {
        Self { config }
    }

    pub fn extract(&self, html: &str) -> String {
        let fragment = main_fragment(html).unwrap_or(html);
        render_markdown(fragment, &self.config)
    }
}

impl OffloadTransform for HtmlExtractor {
    fn name(&self) -> &'static str {
        "html_extractor"
    }

    fn applies_to(&self) -> ContentType {
        ContentType::Html
    }

    fn estimate_bloat(&self, input: &str) -> f64 {
        let tag_bytes = input
            .split('<')
            .skip(1)
            .filter_map(|part| part.find('>').map(|end| end + 2))
            .sum::<usize>();
        tag_bytes as f64 / input.len().max(1) as f64
    }

    fn cache_key(&self, input: &str) -> String {
        stash::compute_key(input)
    }

    fn apply(
        &self,
        input: &str,
        _ctx: &CompressionContext,
    ) -> Result<OffloadOutput, TransformError> {
        let extracted = self.extract(input);
        if extracted.trim().is_empty() || extracted.len() >= input.len() {
            return Err(TransformError::Skipped);
        }
        Ok(OffloadOutput::new(extracted, input.to_string()))
    }
}

fn main_fragment(html: &str) -> Option<&str> {
    find_element_contents(html, "article")
        .or_else(|| find_element_contents(html, "main"))
        .or_else(|| find_element_contents(html, "body"))
}

fn find_element_contents<'a>(html: &'a str, tag: &str) -> Option<&'a str> {
    let lower = html.to_ascii_lowercase();
    let open_prefix = format!("<{tag}");
    let close = format!("</{tag}");
    let mut cursor = 0usize;
    while let Some(relative) = lower[cursor..].find(&open_prefix) {
        let start = cursor + relative;
        let boundary = lower.as_bytes().get(start + open_prefix.len()).copied();
        if !matches!(
            boundary,
            Some(b'>') | Some(b'/') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        ) {
            cursor = start + open_prefix.len();
            continue;
        }
        let open_end = find_tag_end(html, start)?;
        let close_start = lower[open_end + 1..].find(&close)? + open_end + 1;
        return Some(&html[open_end + 1..close_start]);
    }
    None
}

fn find_tag_end(html: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, ch) in html[start..].char_indices() {
        match ch {
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            '>' if quote.is_none() => return Some(start + offset),
            _ => {}
        }
    }
    None
}

fn render_markdown(fragment: &str, config: &HtmlExtractorConfig) -> String {
    let mut output = String::new();
    let mut cursor = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    let mut links: Vec<Option<String>> = Vec::new();
    let mut in_pre = false;

    while cursor < fragment.len() {
        let Some(relative) = fragment[cursor..].find('<') else {
            if skipped.is_empty() {
                append_text(&mut output, &fragment[cursor..], in_pre);
            }
            break;
        };
        let tag_start = cursor + relative;
        if skipped.is_empty() {
            append_text(&mut output, &fragment[cursor..tag_start], in_pre);
        }
        if fragment[tag_start..].starts_with("<!--") {
            cursor = fragment[tag_start + 4..]
                .find("-->")
                .map_or(fragment.len(), |end| tag_start + 4 + end + 3);
            continue;
        }
        let Some(tag_end) = find_tag_end(fragment, tag_start) else {
            break;
        };
        let raw = fragment[tag_start + 1..tag_end].trim();
        let (closing, name, self_closing) = parse_tag(raw);
        cursor = tag_end + 1;
        if name.is_empty() {
            continue;
        }

        if is_noise_tag(&name) {
            if closing {
                if skipped.last().is_some_and(|current| current == &name) {
                    skipped.pop();
                }
            } else if !self_closing {
                skipped.push(name);
            }
            continue;
        }
        if !skipped.is_empty() {
            continue;
        }

        if closing {
            match name.as_str() {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "blockquote" => {
                    ensure_newlines(&mut output, 2)
                }
                "li" | "tr" => ensure_newlines(&mut output, 1),
                "ul" | "ol" | "table" | "section" | "div" => ensure_newlines(&mut output, 2),
                "pre" => {
                    output.push_str("\n```\n");
                    ensure_newlines(&mut output, 2);
                    in_pre = false;
                }
                "strong" | "b" => output.push_str("**"),
                "em" | "i" => output.push('*'),
                "code" if !in_pre => output.push('`'),
                "a" => {
                    if let Some(Some(href)) = links.pop() {
                        output.push_str("](");
                        output.push_str(&href);
                        output.push(')');
                    }
                }
                "td" | "th" if config.include_tables => output.push_str(" |"),
                _ => {}
            }
        } else {
            match name.as_str() {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    ensure_newlines(&mut output, 2);
                    let level = name[1..].parse::<usize>().unwrap_or(1);
                    output.push_str(&"#".repeat(level));
                    output.push(' ');
                }
                "p" | "blockquote" | "section" | "div" => ensure_newlines(&mut output, 2),
                "li" => {
                    ensure_newlines(&mut output, 1);
                    output.push_str("- ");
                }
                "br" => ensure_newlines(&mut output, 1),
                "pre" => {
                    ensure_newlines(&mut output, 2);
                    output.push_str("```\n");
                    in_pre = true;
                }
                "strong" | "b" => output.push_str("**"),
                "em" | "i" => output.push('*'),
                "code" if !in_pre => output.push('`'),
                "a" => {
                    let href = config
                        .include_links
                        .then(|| attribute(raw, "href"))
                        .flatten();
                    if href.is_some() {
                        output.push('[');
                    }
                    links.push(href);
                }
                "tr" if config.include_tables => {
                    ensure_newlines(&mut output, 1);
                    output.push('|');
                }
                "td" | "th" if config.include_tables => output.push(' '),
                "img" if config.include_images => {
                    if let Some(alt) = attribute(raw, "alt") {
                        output.push_str("![");
                        output.push_str(&alt);
                        output.push_str("](");
                        output.push_str(&attribute(raw, "src").unwrap_or_default());
                        output.push(')');
                    }
                }
                _ => {}
            }
        }
    }

    output.trim().to_string()
}

fn parse_tag(raw: &str) -> (bool, String, bool) {
    let closing = raw.starts_with('/');
    let body = raw.trim_start_matches('/').trim_start();
    let name = body
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect::<String>()
        .to_ascii_lowercase();
    (closing, name, raw.ends_with('/'))
}

fn is_noise_tag(name: &str) -> bool {
    matches!(
        name,
        "script"
            | "style"
            | "nav"
            | "header"
            | "footer"
            | "aside"
            | "form"
            | "noscript"
            | "svg"
            | "canvas"
            | "template"
    )
}

fn attribute(raw: &str, wanted: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    let mut cursor = 0usize;
    while let Some(relative) = lower[cursor..].find(wanted) {
        let start = cursor + relative;
        let before_ok = start == 0 || lower.as_bytes()[start - 1].is_ascii_whitespace();
        let mut value_start = start + wanted.len();
        while lower
            .as_bytes()
            .get(value_start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            value_start += 1;
        }
        if !before_ok || lower.as_bytes().get(value_start) != Some(&b'=') {
            cursor = start + wanted.len();
            continue;
        }
        value_start += 1;
        while lower
            .as_bytes()
            .get(value_start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            value_start += 1;
        }
        let quote = raw.as_bytes().get(value_start).copied();
        if matches!(quote, Some(b'\'') | Some(b'"')) {
            let value_start = value_start + 1;
            let end = raw[value_start..].find(quote.unwrap() as char)? + value_start;
            return Some(decode_entities(&raw[value_start..end]));
        }
        let end = raw[value_start..]
            .find(|ch: char| ch.is_ascii_whitespace() || ch == '>')
            .map_or(raw.len(), |end| value_start + end);
        return Some(decode_entities(&raw[value_start..end]));
    }
    None
}

fn append_text(output: &mut String, text: &str, in_pre: bool) {
    let decoded = decode_entities(text);
    if in_pre {
        output.push_str(decoded.trim());
        return;
    }
    for word in decoded.split_whitespace() {
        if !output.is_empty() && !output.ends_with([' ', '\n', '[', '*', '`']) {
            output.push(' ');
        }
        output.push_str(word);
    }
    if decoded.chars().last().is_some_and(char::is_whitespace)
        && !output.is_empty()
        && !output.ends_with([' ', '\n'])
    {
        output.push(' ');
    }
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn ensure_newlines(output: &mut String, count: usize) {
    while output.ends_with(' ') {
        output.pop();
    }
    let existing = output.chars().rev().take_while(|ch| *ch == '\n').count();
    output.extend(std::iter::repeat('\n').take(count.saturating_sub(existing)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_article_and_removes_page_noise() {
        let html = include_str!("../../tests/fixtures/article_page.html");
        let extractor = HtmlExtractor::new(HtmlExtractorConfig::default());

        let output = extractor.extract(html);

        assert!(output.contains("# Compress HTML without losing the article"));
        assert!(output.contains("Sift keeps the main explanation & removes page chrome."));
        assert!(output.contains("- Article paragraphs remain visible."));
        assert!(output.contains("```\nconst result = siftText(html);\n```"));
        assert!(!output.contains("analyticsToken"));
        assert!(!output.contains("Home"));
        assert!(!output.contains("Buy unrelated products"));
        assert!(!output.contains("Copyright 2026"));
    }

    #[test]
    fn fallback_body_filters_structural_noise() {
        let html = "<html><body><nav>menu</nav><p>Useful body text.</p><script>bad()</script><footer>legal</footer></body></html>";
        let output = HtmlExtractor::new(HtmlExtractorConfig::default()).extract(html);
        assert_eq!(output, "Useful body text.");
    }

    #[test]
    fn preserves_links_and_inline_formatting() {
        let html = r#"<article><p>Read <a href="/guide">the <strong>guide</strong></a> today.</p></article>"#;
        let output = HtmlExtractor::new(HtmlExtractorConfig::default()).extract(html);
        assert_eq!(output, "Read [the **guide**](/guide) today.");
    }
}
