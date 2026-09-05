//! 内容类型检测：压缩分发的唯一键。
//!
//! 参考实现为 regex 驱动；本 crate 不依赖 regex，因此所有判据
//! （diff 头、grep 行、日志特征、代码骨架、HTML 标签等）均用
//! 手写结构化解析等价复刻，置信度公式与分发阈值保持一致。
//!
//! 分发顺序（与参考实现一致）：
//! 1. 空 / 纯空白 → PlainText（confidence 0.0）
//! 2. JSON 对象/数组（含连续对象与 JSON 为主体的轻量 wrapper）
//! 3. git diff（confidence ≥ 0.7）
//! 4. HTML（confidence ≥ 0.7）
//! 5. grep/ripgrep 搜索结果（confidence ≥ 0.6）
//! 6. 构建/测试日志（confidence ≥ 0.5）
//! 7. 源代码（confidence ≥ 0.5）
//! 8. 兜底 → PlainText（confidence 0.5）

use serde_json::Value;

/// 内容类型，决定分发到哪个压缩器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    /// JSON 对象/数组（工具输出常见）→ SmartCrusher
    JsonArray,
    /// 构建/测试日志 → LogCompressor
    BuildOutput,
    /// grep/ripgrep 搜索结果 → SearchCompressor
    SearchResults,
    /// unified git diff → DiffCompressor
    GitDiff,
    /// 源代码 → CodeCompressor
    SourceCode,
    /// 普通文本 → TextCrusher
    PlainText,
    /// HTML → HtmlExtractor
    Html,
}

/// 单 block 参与压缩的最小字节数。
pub const MIN_BLOCK_BYTES: usize = 512;

/// 检测入口：按参考实现的分发顺序逐个尝试各判据。
pub fn detect_content_type(text: &str) -> ContentType {
    if text.is_empty() || text.trim().is_empty() {
        return ContentType::PlainText;
    }
    if try_detect_json(text) {
        return ContentType::JsonArray;
    }
    if let Some(c) = try_detect_diff(text) {
        if c >= 0.7 {
            return ContentType::GitDiff;
        }
    }
    if let Some(c) = try_detect_html(text) {
        if c >= 0.7 {
            return ContentType::Html;
        }
    }
    if let Some(c) = try_detect_search(text) {
        if c >= 0.6 {
            return ContentType::SearchResults;
        }
    }
    if let Some(c) = try_detect_log(text) {
        if c >= 0.5 {
            return ContentType::BuildOutput;
        }
    }
    if let Some((_, c)) = try_detect_code(text) {
        if c >= 0.5 {
            return ContentType::SourceCode;
        }
    }
    ContentType::PlainText
}

// ─── JSON ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct JsonPayload {
    pub value: Value,
    pub start: usize,
    pub end: usize,
    pub normalized: bool,
}

/// 解析结构化 JSON payload。除完整对象/数组外，还接受空白分隔的连续对象，
/// 以及 JSON 占去空白后文本至少 60% 的轻量 wrapper。
pub(crate) fn parse_json_payload(text: &str) -> Option<JsonPayload> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let trim_start = text.len() - text.trim_start().len();

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if value.is_array() || value.is_object() {
            return Some(JsonPayload {
                value,
                start: trim_start,
                end: trim_start + trimmed.len(),
                normalized: false,
            });
        }
        return None;
    }

    if trimmed.starts_with('{') {
        let mut values = Vec::new();
        for value in serde_json::Deserializer::from_str(trimmed).into_iter::<Value>() {
            let Ok(value) = value else {
                values.clear();
                break;
            };
            if !value.is_object() {
                values.clear();
                break;
            }
            values.push(value);
        }
        if values.len() >= 2 {
            return Some(JsonPayload {
                value: Value::Array(values),
                start: trim_start,
                end: trim_start + trimmed.len(),
                normalized: true,
            });
        }
    }

    let relative_start = [trimmed.find('{'), trimmed.find('[')]
        .into_iter()
        .flatten()
        .min()?;
    let mut stream =
        serde_json::Deserializer::from_str(&trimmed[relative_start..]).into_iter::<Value>();
    let value = stream.next()?.ok()?;
    if !value.is_array() && !value.is_object() {
        return None;
    }
    let relative_end = relative_start + stream.byte_offset();
    if relative_end - relative_start < trimmed.len() * 3 / 5 {
        return None;
    }
    Some(JsonPayload {
        value,
        start: trim_start + relative_start,
        end: trim_start + relative_end,
        normalized: true,
    })
}

fn try_detect_json(text: &str) -> bool {
    parse_json_payload(text).is_some()
}

// ─── git diff ─────────────────────────────────────────────────────────

/// diff 头检测窗口（参考实现 2026-04-25 修复后为 500 行）。
const DIFF_WINDOW_LINES: usize = 500;

/// 单行是否为 diff 头：`diff --git` / `diff --combined ` / `diff --cc ` /
/// `--- a/` / 普通 hunk 头 `@@ -A,B +C,D @@` / 合并 diff hunk 头 `@@@ ... @@@`。
fn is_diff_header(line: &str) -> bool {
    line.starts_with("diff --git")
        || line.starts_with("diff --combined ")
        || line.starts_with("diff --cc ")
        || line.starts_with("--- a/")
        || is_hunk_header(line)
        || is_combined_hunk_header(line)
}

/// `@@\s+-\d+,\d+\s+\+\d+,\d+\s+@@`
fn is_hunk_header(line: &str) -> bool {
    let mut s = match line.strip_prefix("@@") {
        Some(s) => s,
        None => return false,
    };
    // `@@\s+` 至少一个空白；顺带拒绝 `@@@`（合并头单走 is_combined_hunk_header）
    s = match skip_ws_at_least_one(s) {
        Some(s) => s,
        None => return false,
    };
    s = match parse_range_pair(s) {
        Some(s) => s,
        None => return false,
    };
    s = match skip_ws_at_least_one(s) {
        Some(s) => s,
        None => return false,
    };
    s = match parse_range_pair(s) {
        Some(s) => s,
        None => return false,
    };
    match skip_ws_at_least_one(s) {
        Some(s) => s.starts_with("@@"),
        None => false,
    }
}

/// `@@@+\s+-\d+(,\d+)?\s+(?:-\d+(,\d+)?\s+)+\+\d+(,\d+)?\s+@@@+`
fn is_combined_hunk_header(line: &str) -> bool {
    // 前缀至少 3 个 `@`，后跟至少一个空白
    let at_count = line.chars().take_while(|&c| c == '@').count();
    if at_count < 3 {
        return false;
    }
    let mut s = match skip_ws_at_least_one(&line[at_count..]) {
        Some(s) => s,
        None => return false,
    };
    // 第一个 `-A,B`
    s = match parse_range_opt_len(s) {
        Some(s) => s,
        None => return false,
    };
    s = match skip_ws_at_least_one(s) {
        Some(s) => s,
        None => return false,
    };
    // 一个或多个 `-A,B`（父提交区间；只接受 `-` 开头，`+` 区间留给后面）
    loop {
        if !s.starts_with('-') {
            break;
        }
        let next = match parse_range_opt_len(s) {
            Some(n) => match skip_ws_at_least_one(n) {
                Some(n) => n,
                None => break,
            },
            None => break,
        };
        s = next;
    }
    // `+A,B`
    s = match parse_range_opt_len(s) {
        Some(s) => s,
        None => return false,
    };
    match skip_ws_at_least_one(s) {
        Some(s) => s.chars().take_while(|&c| c == '@').count() >= 3,
        None => false,
    }
}

/// 解析 `±数字,数字`（逗号与第二个数字必须存在）。
fn parse_range_pair(s: &str) -> Option<&str> {
    let s = strip_sign(s)?;
    let s = skip_digits(s)?;
    let s = s.strip_prefix(',')?;
    skip_digits(s)
}

/// 去掉一个 `-` 或 `+` 前缀。
fn strip_sign(s: &str) -> Option<&str> {
    s.strip_prefix('-').or_else(|| s.strip_prefix('+'))
}

/// 解析 `±数字(,数字)?`。
fn parse_range_opt_len(s: &str) -> Option<&str> {
    let s = strip_sign(s)?;
    let s = skip_digits(s)?;
    if let Some(rest) = s.strip_prefix(',') {
        if let Some(after) = skip_digits(rest) {
            return Some(after);
        }
    }
    Some(s)
}

fn skip_digits(s: &str) -> Option<&str> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        None
    } else {
        Some(&s[end..])
    }
}

fn skip_ws_at_least_one(s: &str) -> Option<&str> {
    let t = s.trim_start();
    if t.len() == s.len() {
        None
    } else {
        Some(t)
    }
}

/// 单行是否为真实变更行：`^[+-][^+-]`（排除 `+++ b/` 这类头）。
fn is_diff_change(line: &str) -> bool {
    let b = line.as_bytes();
    b.len() >= 2 && (b[0] == b'+' || b[0] == b'-') && b[1] != b'+' && b[1] != b'-'
}

/// diff 检测：窗口内统计头行与变更行，
/// confidence = min(1.0, 0.5 + 0.2*头 + 0.05*变更)。
fn try_detect_diff(text: &str) -> Option<f64> {
    let mut headers = 0u32;
    let mut changes = 0u32;
    for line in text.split('\n').take(DIFF_WINDOW_LINES) {
        if is_diff_header(line) {
            headers += 1;
        }
        if is_diff_change(line) {
            changes += 1;
        }
    }
    if headers == 0 {
        return None;
    }
    Some((0.5 + headers as f64 * 0.2 + changes as f64 * 0.05).min(1.0))
}

// ─── HTML ─────────────────────────────────────────────────────────────

/// HTML 检测采样窗口（前 3000 字节，与参考实现一致）。
const HTML_SAMPLE_BYTES: usize = 3000;

/// confidence = 0.5*doctype + 0.3*<html> + 0.1*<head> + 0.1*<body>
///            + min(0.3, 0.03*结构标签数)；<0.5 时返回 None。
fn try_detect_html(text: &str) -> Option<f64> {
    let mut cutoff = HTML_SAMPLE_BYTES.min(text.len());
    while !text.is_char_boundary(cutoff) {
        cutoff -= 1;
    }
    let sample = &text[..cutoff];
    let lower = sample.to_ascii_lowercase();

    let has_doctype = lower.trim_start().starts_with("<!doctype html") && {
        // 对应 `<!doctype\s+html`：`<!doctype` 后至少一个空白
        let rest = lower.trim_start().strip_prefix("<!doctype").unwrap_or("");
        rest.starts_with(char::is_whitespace)
    };
    let has_html_tag = contains_tag(&lower, "html");
    let has_head = contains_tag(&lower, "head");
    let has_body = contains_tag(&lower, "body");
    let structural = count_structural_tags(&lower);

    if !has_doctype && !has_html_tag && structural < 3 {
        return None;
    }

    let mut confidence = 0.0_f64;
    if has_doctype {
        confidence += 0.5;
    }
    if has_html_tag {
        confidence += 0.3;
    }
    if has_head {
        confidence += 0.1;
    }
    if has_body {
        confidence += 0.1;
    }
    confidence += (structural as f64 * 0.03).min(0.3);
    confidence = confidence.min(1.0);
    if confidence < 0.5 {
        return None;
    }
    Some(confidence)
}

/// `<tag[\s>]`（大小写不敏感；`lower` 已小写）。
fn contains_tag(lower: &str, tag: &str) -> bool {
    let pat = format!("<{tag}");
    let mut from = 0;
    while let Some(pos) = lower[from..].find(&pat) {
        let after = &lower[from + pos + pat.len()..];
        if after.starts_with(' ') || after.starts_with('\t') || after.starts_with('>') {
            return true;
        }
        from += pos + pat.len();
    }
    false
}

/// 结构标签（div/span/script/style/link/meta/nav/header/footer/aside/article/
/// section/main）出现次数。
fn count_structural_tags(lower: &str) -> u32 {
    const TAGS: [&str; 12] = [
        "div", "span", "script", "style", "link", "meta", "nav", "header", "footer", "aside",
        "article", "section", // main 在下方单独统计
    ];
    let mut count = TAGS.iter().map(|&t| tag_occurrences(lower, t)).sum::<u32>();
    count += tag_occurrences(lower, "main");
    count
}

fn tag_occurrences(lower: &str, tag: &str) -> u32 {
    let pat = format!("<{tag}");
    let mut count = 0;
    let mut from = 0;
    while let Some(pos) = lower[from..].find(&pat) {
        let after = &lower[from + pos + pat.len()..];
        let c = after.chars().next();
        if c == Some(' ') || c == Some('\t') || c == Some('>') {
            count += 1;
        }
        from += pos + pat.len();
    }
    count
}

// ─── 搜索结果（grep -n 风格） ────────────────────────────────────────

/// 搜索检测窗口。
const SEARCH_WINDOW_LINES: usize = 100;

/// 单行是否匹配 `^[^\s:]+:\d+:`（file:line:content）。
fn is_search_line(line: &str) -> bool {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i] != b':' && !(b[i] as char).is_whitespace() {
        i += 1;
    }
    if i == 0 || i >= b.len() || b[i] != b':' {
        return false;
    }
    let mut j = i + 1;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    j > i + 1 && j < b.len() && b[j] == b':'
}

/// 搜索检测：窗口内匹配比例 ≥ 0.3，
/// confidence = min(1.0, 0.4 + ratio*0.6)。
fn try_detect_search(text: &str) -> Option<f64> {
    let lines: Vec<&str> = text.split('\n').take(SEARCH_WINDOW_LINES).collect();
    let non_empty = lines.iter().filter(|l| !l.trim().is_empty()).count() as u32;
    if non_empty == 0 {
        return None;
    }
    let matching = lines
        .iter()
        .filter(|l| !l.trim().is_empty() && is_search_line(l))
        .count() as u32;
    if matching == 0 {
        return None;
    }
    let ratio = matching as f64 / non_empty as f64;
    if ratio < 0.3 {
        return None;
    }
    Some((0.4 + ratio * 0.6).min(1.0))
}

// ─── 构建/测试日志 ────────────────────────────────────────────────────

/// 日志检测窗口。
const LOG_WINDOW_LINES: usize = 200;

/// 单行日志判据。返回命中模式的序号；前两类（ERROR/WARN 族）视为
/// error 命中，对 confidence 有额外贡献。
fn log_pattern_index(line: &str) -> Option<usize> {
    // 0: \b(ERROR|FAIL|FAILED|FATAL|CRITICAL)\b（大小写不敏感）
    if ["error", "fail", "failed", "fatal", "critical"]
        .iter()
        .any(|kw| contains_word_ci(line, kw))
    {
        return Some(0);
    }
    // 1: \b(WARN|WARNING)\b（大小写不敏感）
    if ["warn", "warning"].iter().any(|kw| contains_word_ci(line, kw)) {
        return Some(1);
    }
    // 2: \b(INFO|DEBUG|TRACE)\b（大小写不敏感）
    if ["info", "debug", "trace"].iter().any(|kw| contains_word_ci(line, kw)) {
        return Some(2);
    }
    let s = line.trim_start();
    // 3: \d{4}-\d{2}-\d{2}
    if s.len() >= 10
        && s.as_bytes()[..4].iter().all(u8::is_ascii_digit)
        && s.as_bytes()[4] == b'-'
        && s.as_bytes()[5..7].iter().all(u8::is_ascii_digit)
        && s.as_bytes()[7] == b'-'
        && s.as_bytes()[8..10].iter().all(u8::is_ascii_digit)
    {
        return Some(3);
    }
    // 4: \[\d{2}:\d{2}:\d{2}\]
    if s.len() >= 10
        && s.starts_with('[')
        && s.as_bytes()[1..3].iter().all(u8::is_ascii_digit)
        && s.as_bytes()[3] == b':'
        && s.as_bytes()[4..6].iter().all(u8::is_ascii_digit)
        && s.as_bytes()[6] == b':'
        && s.as_bytes()[7..9].iter().all(u8::is_ascii_digit)
        && s.as_bytes()[9] == b']'
    {
        return Some(4);
    }
    // 5: ^={3,}|^-{3,}
    if s.len() >= 3
        && ((s.starts_with("===") && s.chars().take_while(|&c| c == '=').count() >= 3)
            || (s.starts_with("---") && s.chars().take_while(|&c| c == '-').count() >= 3))
    {
        return Some(5);
    }
    // 6: ^\s*(PASSED|FAILED|SKIPPED)（大小写敏感）
    if s.starts_with("PASSED") || s.starts_with("FAILED") || s.starts_with("SKIPPED") {
        return Some(6);
    }
    // 7: ^npm ERR!|^yarn error|^cargo error
    if s.starts_with("npm ERR!") || s.starts_with("yarn error") || s.starts_with("cargo error") {
        return Some(7);
    }
    // 8: Traceback（子串，大小写敏感）
    if line.contains("Traceback (most recent call last)") {
        return Some(8);
    }
    // 9: ^\s*at\s+[\w.$]+\(（栈帧）
    if let Some(rest) = s.strip_prefix("at") {
        if let Some(t) = skip_ws_at_least_one(rest) {
            let end = t
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '$'))
                .unwrap_or(t.len());
            if end > 0 && t[end..].starts_with('(') {
                return Some(9);
            }
        }
    }
    None
}

/// 大小写不敏感的整词包含：`\b<kw>\b`。
fn contains_word_ci(line: &str, kw: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(kw) {
        let abs = from + pos;
        let before_ok = abs == 0 || !is_word_char(lower[..abs].chars().next_back().unwrap());
        let after_idx = abs + kw.len();
        let after_ok = after_idx >= lower.len()
            || !is_word_char(lower[after_idx..].chars().next().unwrap());
        if before_ok && after_ok {
            return true;
        }
        from = abs + 1;
        if from >= lower.len() {
            break;
        }
    }
    false
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 日志检测：命中比例 ≥ 0.1，
/// confidence = min(1.0, 0.3 + ratio*0.5 + error_matches*0.05)。
fn try_detect_log(text: &str) -> Option<f64> {
    let lines: Vec<&str> = text.split('\n').take(LOG_WINDOW_LINES).collect();
    let non_empty = lines.iter().filter(|l| !l.trim().is_empty()).count() as u32;
    if non_empty == 0 {
        return None;
    }
    let mut pattern_matches = 0u32;
    let mut error_matches = 0u32;
    for line in &lines {
        if let Some(idx) = log_pattern_index(line) {
            pattern_matches += 1;
            if idx < 2 {
                error_matches += 1;
            }
        }
    }
    if pattern_matches == 0 {
        return None;
    }
    let ratio = pattern_matches as f64 / non_empty as f64;
    if ratio < 0.1 {
        return None;
    }
    Some((0.3 + ratio * 0.5 + error_matches as f64 * 0.05).min(1.0))
}

// ─── 源代码 ───────────────────────────────────────────────────────────

/// 代码检测窗口。
const CODE_WINDOW_LINES: usize = 100;

/// 单语言判据：`s` 为去掉行首空白后的内容。
fn match_python(s: &str) -> bool {
    (kw_ws_word(s, &["def", "class", "import", "from", "async def"]))
        || (s.starts_with('@') && s[1..].chars().next().is_some_and(is_word_char))
        || s.starts_with("\"\"\"")
        || {
            // `if __name__\s*==`
            match s.strip_prefix("if __name__") {
                Some(rest) => rest.trim_start().starts_with("=="),
                None => false,
            }
        }
}

fn match_javascript(s: &str) -> bool {
    if kw_ws(s, &["function", "const", "let", "var", "class", "import", "export"]) {
        return true;
    }
    // `async\s+function`（无尾部要求）
    if let Some(rest) = s.strip_prefix("async") {
        if let Some(t) = skip_ws_at_least_one(rest) {
            if t.starts_with("function") {
                return true;
            }
        }
    }
    // `=>\s*\{`
    if let Some(rest) = s.strip_prefix("=>") {
        if rest.trim_start().starts_with('{') {
            return true;
        }
    }
    s.starts_with("module.exports")
}

fn match_typescript(s: &str) -> bool {
    if kw_ws_word(s, &["interface", "type", "enum", "namespace"]) {
        return true;
    }
    // `:\s*(string|number|boolean|any|void)\b`
    if let Some(rest) = s.strip_prefix(':') {
        let t = rest.trim_start();
        for kw in ["string", "number", "boolean", "any", "void"] {
            if let Some(after) = t.strip_prefix(kw) {
                if !after.chars().next().is_some_and(is_word_char) {
                    return true;
                }
            }
        }
    }
    false
}

fn match_go(s: &str) -> bool {
    if kw_ws(s, &["func", "type", "package", "import"]) {
        return true;
    }
    // `func\s+\([^)]+\)\s+\w+`（方法接收者）
    if let Some(rest) = s.strip_prefix("func") {
        if let Some(t) = skip_ws_at_least_one(rest) {
            if let Some(open) = t.find('(') {
                if open == 0 {
                    if let Some(close) = t.find(')') {
                        if close > 1 {
                            if let Some(u) = skip_ws_at_least_one(&t[close + 1..]) {
                                if u.chars().next().is_some_and(is_word_char) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn match_rust(s: &str) -> bool {
    kw_ws(s, &["fn", "struct", "enum", "impl", "mod", "use", "pub"]) || s.starts_with("#[")
}

fn match_java(s: &str) -> bool {
    // `(public|private|protected)\s+(class|interface|enum)`
    for vis in ["public", "private", "protected"] {
        if let Some(rest) = s.strip_prefix(vis) {
            if let Some(t) = skip_ws_at_least_one(rest) {
                if ["class", "interface", "enum"].iter().any(|kw| t.starts_with(kw)) {
                    return true;
                }
            }
        }
    }
    if s.starts_with('@') && s[1..].chars().next().is_some_and(is_word_char) {
        return true;
    }
    // `package\s+[\w.]+;`
    if let Some(rest) = s.strip_prefix("package") {
        if let Some(t) = skip_ws_at_least_one(rest) {
            let end = t
                .find(|c: char| !(is_word_char(c) || c == '.'))
                .unwrap_or(t.len());
            if end > 0 && t[end..].starts_with(';') {
                return true;
            }
        }
    }
    false
}

/// 语言表；顺序与参考实现一致（影响平分时的语言选择）。
const CODE_LANGUAGES: [&str; 6] = ["python", "javascript", "typescript", "go", "rust", "java"];

fn line_matches_language(lang: &str, s: &str) -> bool {
    match lang {
        "python" => match_python(s),
        "javascript" => match_javascript(s),
        "typescript" => match_typescript(s),
        "go" => match_go(s),
        "rust" => match_rust(s),
        "java" => match_java(s),
        _ => false,
    }
}

/// `kw\s+`：关键字后跟至少一个空白。
fn kw_ws(s: &str, keywords: &[&str]) -> bool {
    keywords
        .iter()
        .any(|kw| s.strip_prefix(kw).is_some_and(|r| skip_ws_at_least_one(r).is_some()))
}

/// `kw\s+\w`：关键字 + 空白 + 一个词字符。
fn kw_ws_word(s: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| {
        s.strip_prefix(kw)
            .and_then(skip_ws_at_least_one)
            .and_then(|t| t.chars().next())
            .is_some_and(is_word_char)
    })
}

/// 代码检测：逐行给各语言计分（每语言每行至多 1 分），
/// 最佳语言得分 ≥ 3 才算命中；
/// confidence = min(1.0, 0.4 + ratio*0.4 + best_score*0.02)。
/// 平分时取「首次得分出现」的语言（与参考实现的 dict 顺序语义一致）。
fn try_detect_code(text: &str) -> Option<(&'static str, f64)> {
    let lines: Vec<&str> = text.split('\n').take(CODE_WINDOW_LINES).collect();
    // (语言, 得分)，按首次命中插入
    let mut scores: Vec<(&'static str, u32)> = Vec::new();

    for line in &lines {
        let s = line.trim_start();
        for &lang in CODE_LANGUAGES.iter() {
            if line_matches_language(lang, s) {
                if let Some(entry) = scores.iter_mut().find(|(n, _)| *n == lang) {
                    entry.1 += 1;
                } else {
                    scores.push((lang, 1));
                }
                // 每行每语言只计一次；参考实现对每语言独立计分，
                // 这里外层循环已保证。
            }
        }
    }

    let max_score = scores.iter().map(|x| x.1).max()?;
    let (best_lang, best_score) = *scores.iter().find(|x| x.1 == max_score)?;
    if best_score < 3 {
        return None;
    }
    let non_empty = lines.iter().filter(|l| !l.trim().is_empty()).count() as u32;
    let ratio = best_score as f64 / non_empty.max(1) as f64;
    let confidence = (0.4 + ratio * 0.4 + best_score as f64 * 0.02).min(1.0);
    Some((best_lang, confidence))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── JSON ─────────────────────────────────────────────────────────

    #[test]
    fn json_array_of_dicts() {
        assert_eq!(detect_content_type(r#"[{"id": 1}, {"id": 2}]"#), ContentType::JsonArray);
    }

    #[test]
    fn json_array_of_scalars() {
        assert_eq!(detect_content_type("[1, 2, 3]"), ContentType::JsonArray);
    }

    #[test]
    fn json_empty_array() {
        assert_eq!(detect_content_type("[]"), ContentType::JsonArray);
    }

    #[test]
    fn json_with_leading_whitespace() {
        assert_eq!(detect_content_type(r#"  [{"a": 1}]"#), ContentType::JsonArray);
    }

    #[test]
    fn json_object_is_structured_json() {
        assert_eq!(detect_content_type(r#"{"id": 1}"#), ContentType::JsonArray);
    }

    #[test]
    fn concatenated_json_objects_are_structured_json() {
        assert_eq!(
            detect_content_type(r#"{"id":1} {"id":2} {"id":3}"#),
            ContentType::JsonArray
        );
        let payload = parse_json_payload(r#"{"id":1} {"id":2}"#).unwrap();
        assert!(payload.normalized);
        assert_eq!(payload.value.as_array().unwrap().len(), 2);
    }

    #[test]
    fn json_dominant_wrapper_is_structured_json() {
        let text = format!(
            "Exit code: 0\n{}\ndone",
            r#"{"rows":[1,2,3],"pad":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"}"#
        );
        assert_eq!(detect_content_type(&text), ContentType::JsonArray);
        let payload = parse_json_payload(&text).unwrap();
        assert_eq!(
            &text[payload.start..payload.end],
            serde_json::to_string(&payload.value).unwrap()
        );
    }

    #[test]
    fn small_json_mention_inside_prose_is_not_structured_json() {
        let text = format!(
            "This prose explains an example {{\"id\":1}} without being a JSON payload. {}",
            "more prose ".repeat(20)
        );
        assert_eq!(detect_content_type(&text), ContentType::PlainText);
    }

    #[test]
    fn malformed_bracket_not_json() {
        // 以 [ 开头但解析失败 → 不是 JSON
        assert_eq!(detect_content_type("[not json at all, just text"), ContentType::PlainText);
    }

    // ─── 空内容 / 兜底 ────────────────────────────────────────────────

    #[test]
    fn empty_and_whitespace_fall_to_plain_text() {
        assert_eq!(detect_content_type(""), ContentType::PlainText);
        assert_eq!(detect_content_type("   \n\t  "), ContentType::PlainText);
    }

    #[test]
    fn plain_text_fallback() {
        assert_eq!(
            detect_content_type("Just some random text without any special structure."),
            ContentType::PlainText
        );
    }

    // ─── 搜索结果 ─────────────────────────────────────────────────────

    #[test]
    fn grep_style_search_results() {
        let content = "src/main.py:42:def process():\nsrc/util.py:13:    return None\nlib/x.py:7:class X:";
        assert_eq!(detect_content_type(content), ContentType::SearchResults);
    }

    #[test]
    fn ripgrep_style_with_line_numbers() {
        let content = "crates/core/src/lib.rs:10:pub fn run()\ncrates/core/src/lib.rs:20:pub struct Foo\ncrates/util/src/lib.rs:5:mod tests";
        assert_eq!(detect_content_type(content), ContentType::SearchResults);
    }

    #[test]
    fn sparse_search_lines_below_ratio() {
        // 只有 1/5 行匹配 file:line: → ratio 0.2 < 0.3，不判为搜索
        let content = "intro line\nrandom line\nsrc/a.py:1:x\nother\nmore text";
        assert_ne!(detect_content_type(content), ContentType::SearchResults);
    }

    #[test]
    fn colon_without_line_number_not_search() {
        assert_ne!(detect_content_type("hello: world\nfoo: bar\nbaz: qux"), ContentType::SearchResults);
    }

    // ─── git diff ─────────────────────────────────────────────────────

    #[test]
    fn git_diff_full() {
        let content = "\
diff --git a/foo.py b/foo.py
--- a/foo.py
+++ b/foo.py
@@ -1,3 +1,4 @@
 def hello():
-    print('hi')
+    print('hello')
+    print('world')
";
        assert_eq!(detect_content_type(content), ContentType::GitDiff);
    }

    #[test]
    fn single_diff_header_borderline() {
        // 1 个头 + 0 变更 = 0.7，恰好达到阈值
        assert_eq!(detect_content_type("diff --git a/x b/x\n"), ContentType::GitDiff);
    }

    #[test]
    fn combined_diff_header() {
        let content = "\
diff --cc foo.py
@@@ -1,3 -1,3 -1,4 @@@
 context
-removed
+added
";
        assert_eq!(detect_content_type(content), ContentType::GitDiff);
    }

    #[test]
    fn plain_patch_without_diff_git() {
        // 只有 --- a/ 头也认（hunk 头 + 变更行抬置信度）
        let content = "\
--- a/old.py
+++ b/new.py
@@ -1,2 +1,2 @@
-old
+new
";
        assert_eq!(detect_content_type(content), ContentType::GitDiff);
    }

    // ─── 构建/测试日志 ────────────────────────────────────────────────

    #[test]
    fn build_output_with_bracket_levels() {
        let content = "\
[INFO] Starting build
[INFO] Compiling 42 sources
[ERROR] Compilation failed
[WARN] Deprecated API
FAILED test_one
PASSED test_two
";
        assert_eq!(detect_content_type(content), ContentType::BuildOutput);
    }

    #[test]
    fn timestamped_log_lines() {
        let content = "\
2026-08-21 10:00:00 INFO boot
2026-08-21 10:00:01 ERROR crash
2026-08-21 10:00:02 WARN retry
";
        assert_eq!(detect_content_type(content), ContentType::BuildOutput);
    }

    #[test]
    fn python_traceback_detected() {
        let content = "\
Traceback (most recent call last):
  File \"main.py\", line 3, in <module>
ERROR: ValueError: boom
FATAL: aborting
";
        assert_eq!(detect_content_type(content), ContentType::BuildOutput);
    }

    #[test]
    fn npm_error_line_detected() {
        assert_eq!(
            detect_content_type("npm ERR! code ELIFECYCLE\nnpm ERR! errno 1"),
            ContentType::BuildOutput
        );
    }

    // ─── HTML ─────────────────────────────────────────────────────────

    #[test]
    fn html_full_document() {
        let content = "\
<!DOCTYPE html>
<html>
<head><title>X</title></head>
<body><div>hi</div></body>
</html>";
        assert_eq!(detect_content_type(content), ContentType::Html);
    }

    #[test]
    fn html_without_doctype_but_tags() {
        // 无 doctype：<html>+<head>+<body>（0.5）+ 结构标签封顶 0.3 → ≥ 0.7
        let content = "<html><head></head><body>\
<div>a</div><div>b</div><div>c</div><div>d</div>\
<section>e</section><article>f</article><nav>g</nav><header>h</header>\
</body></html>";
        assert_eq!(detect_content_type(content), ContentType::Html);
    }

    #[test]
    fn single_div_not_html() {
        assert_ne!(detect_content_type("<div>hello</div>"), ContentType::Html);
    }

    // ─── 源代码 ───────────────────────────────────────────────────────

    #[test]
    fn python_code_detected() {
        let content = "\
import os
from typing import Any

def process(data):
    return data

class Service:
    def __init__(self):
        pass

    @property
    def x(self):
        return 1

if __name__ == '__main__':
    process({})
";
        assert_eq!(detect_content_type(content), ContentType::SourceCode);
    }

    #[test]
    fn rust_code_detected() {
        let content = "\
use std::sync::Arc;

#[derive(Debug)]
pub struct Foo {
    bar: u32,
}

pub fn baz() -> u32 {
    42
}

impl Foo {
    pub fn new() -> Self {
        Self { bar: 0 }
    }
}
";
        assert_eq!(detect_content_type(content), ContentType::SourceCode);
    }

    #[test]
    fn go_code_detected() {
        let content = "\
package main

import \"fmt\"

func main() {
    fmt.Println(\"hello\")
}

type Service struct{}

func (s *Service) Do() {}

func helper() {}
";
        assert_eq!(detect_content_type(content), ContentType::SourceCode);
    }

    #[test]
    fn javascript_code_detected() {
        let content = "\
const fs = require('fs');

export function main() {
  return 1;
}

let x = 2;
var y = 3;
class Foo {}
";
        assert_eq!(detect_content_type(content), ContentType::SourceCode);
    }

    #[test]
    fn too_few_code_matches_not_source() {
        // 不足 3 行命中 → 不判为代码
        assert_ne!(
            detect_content_type("hello world\nimport os\njust text"),
            ContentType::SourceCode
        );
    }

    // ─── hunk 头解析单元 ─────────────────────────────────────────────

    #[test]
    fn hunk_header_parsing() {
        assert!(is_hunk_header("@@ -1,3 +1,4 @@"));
        assert!(is_hunk_header("@@ -10,1 +10,1 @@ func main"));
        assert!(!is_hunk_header("@@ -a,b +c,d @@"));
        assert!(!is_hunk_header("@@ 1,2 3,4 @@"));
        assert!(is_combined_hunk_header("@@@ -1,3 -1,3 +1,4 @@@"));
        assert!(!is_combined_hunk_header("@@@ -1,3 -1,3 -1,4 @@@"));
        assert!(is_diff_header("--- a/foo.py"));
        assert!(is_diff_header("diff --combined foo.py"));
        assert!(!is_diff_header("random text"));
    }

    #[test]
    fn search_line_parsing() {
        assert!(is_search_line("src/main.py:42:def f():"));
        assert!(!is_search_line("src/main.py:fourty:def f():"));
        assert!(!is_search_line(":42: no filename"));
        assert!(!is_search_line("plain line"));
    }
}
