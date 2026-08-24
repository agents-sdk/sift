//! 递归 JSON 路由：在 block 任意位置找**平衡的** JSON span
//! （`gh api` dump、MCP result、`curl | jq` 尾巴），逐段路由，
//! 非 span 字节逐字保留。
//!
//! stash 安全性：本模块只负责定位与替换；replace 回调走的是与整块 JSON
//! 相同的分发路径，stash 按 hash 取键、与位置无关，故嵌入 span 内产生的
//! `<<stash:HASH>>` 标记同样可恢复。非 span 字节逐字保留，绝不跨边界丢内容。

use serde_json::Value;

/// span 参与路由的最小字节数（对齐 `content::MIN_BLOCK_BYTES`）。
const MIN_SPAN_BYTES: usize = 512;

/// 找出 text 中所有顶层平衡 JSON span 的 `(start, end)` 字节区间。
///
/// 规则：
/// - 从任意 `{` / `[` 起做栈式平衡匹配，跟踪字符串字面量与转义状态
///   （字符串里的括号不参与计数）；
/// - span 必须能被 serde_json 成功解析，且字节数 ≥ `MIN_SPAN_BYTES`；
/// - 嵌套 span 不单独返回（接受一个 span 后跳过其内部）；
/// - 非平衡 / 解析失败则只前进一个字符继续找（内部嵌套的合法 span
///   仍有机会被找到）。
pub fn find_json_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'{' || bytes[i] == b'[' {
            if let Some(end) = match_span(text, i) {
                if end - i >= MIN_SPAN_BYTES
                    && serde_json::from_str::<Value>(&text[i..end]).is_ok()
                {
                    out.push((i, end));
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// 对每个 span 调用 `replace`（返回 `None` 则原样保留），其余字节逐字
/// 保留，按序拼接返回。span 首尾边界不会切在 UTF-8 字符中间
/// （起点是 ASCII 括号，终点由括号闭合决定）。
pub fn replace_json_spans<F: Fn(&str) -> Option<String>>(text: &str, replace: F) -> String {
    let spans = find_json_spans(text);
    if spans.is_empty() {
        return text.to_string();
    }
    let mut parts: Vec<Cow> = Vec::new();
    let mut last = 0usize;
    for (a, b) in spans {
        parts.push(Cow::Borrowed(&text[last..a]));
        let chunk = &text[a..b];
        match replace(chunk) {
            Some(r) => parts.push(Cow::Owned(r)),
            None => parts.push(Cow::Borrowed(chunk)),
        }
        last = b;
    }
    parts.push(Cow::Borrowed(&text[last..]));
    let mut out = String::with_capacity(text.len());
    for p in parts {
        match p {
            Cow::Borrowed(s) => out.push_str(s),
            Cow::Owned(s) => out.push_str(&s),
        }
    }
    out
}

enum Cow<'a> {
    Borrowed(&'a str),
    Owned(String),
}

/// 从 `start`（必须是 `{` 或 `[`）起做平衡匹配，返回闭合括号之后的
/// 字节偏移；不闭合、错配或提前出现反向括号则返回 None。
fn match_span(text: &str, start: usize) -> Option<usize> {
    let mut stack: Vec<u8> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in text[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => stack.push(ch as u8),
            '}' | ']' => {
                let open = stack.pop()?;
                let expect = if ch == '}' { b'{' } else { b'[' };
                if open != expect {
                    return None;
                }
                if stack.is_empty() {
                    return Some(start + offset + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个 ≥ MIN_SPAN_BYTES 的合法 JSON 对象文本。
    fn big_json() -> String {
        let items: Vec<String> = (0..30)
            .map(|i| format!(r#"{{"id": {i}, "name": "row-{i}", "note": "padding text for row {i}"}}"#))
            .collect();
        format!(r#"{{"total": 30, "items": [{}]}}"#, items.join(","))
    }

    #[test]
    fn embedded_json_in_bash_output() {
        let json = big_json();
        let text = format!("$ gh api repos/x/y\n{json}\nDone in 0.3s");
        let spans = find_json_spans(&text);
        assert_eq!(spans.len(), 1);
        let (a, b) = spans[0];
        assert_eq!(&text[a..b], json);
        // 前后回显文本的偏移正确
        assert_eq!(a, "$ gh api repos/x/y\n".len());
        assert_eq!(b, text.len() - "\nDone in 0.3s".len());
    }

    #[test]
    fn brace_in_string_literal_not_misjudged() {
        // 字符串值里的 `}` / `]` 不参与平衡计数
        let obj = format!(
            r#"{{"a": "close }} brace", "b": "close ] bracket", "esc": "quote \" and }} inside", "pad": "{}"}}"#,
            "y".repeat(512)
        );
        let text = format!("echo start\n{obj}\necho end");
        assert!(serde_json::from_str::<Value>(&obj).is_ok());
        let spans = find_json_spans(&text);
        assert_eq!(spans.len(), 1, "spans: {spans:?}");
        assert_eq!(&text[spans[0].0..spans[0].1], obj);
    }

    #[test]
    fn multiline_json_span() {
        let items: Vec<String> = (0..30)
            .map(|i| format!("  {{\"id\": {i}, \"name\": \"n{i}\"}}"))
            .collect();
        let json = format!("[\n{}\n]", items.join(",\n"));
        assert!(json.len() >= MIN_SPAN_BYTES);
        let text = format!("header text\n{json}\nfooter text");
        let spans = find_json_spans(&text);
        assert_eq!(spans.len(), 1);
        assert_eq!(&text[spans[0].0..spans[0].1], json);
    }

    #[test]
    fn unbalanced_brackets_produce_no_span() {
        let text = format!("text before [{{\"a\": 1, \"b\": 2\nmore text without close");
        assert!(find_json_spans(&text).is_empty());
        // 反向括号开头同样不匹配
        let text2 = "]}".to_string() + &"{\"a\": ".repeat(200);
        assert!(find_json_spans(&text2).is_empty());
    }

    #[test]
    fn small_span_skipped() {
        let small = r#"{"a": 1, "b": [1, 2]}"#;
        assert!(small.len() < MIN_SPAN_BYTES);
        let text = format!("prefix\n{small}\nsuffix");
        assert!(find_json_spans(&text).is_empty());
    }

    #[test]
    fn two_spans_both_found() {
        let text = format!("a\n{}\nb\n{}\nc", big_json(), big_json());
        let spans = find_json_spans(&text);
        assert_eq!(spans.len(), 2);
        for (a, b) in &spans {
            assert_eq!(&text[*a..*b], big_json());
        }
    }

    #[test]
    fn nested_spans_not_returned_separately() {
        // 外层 span 被接受后跳过内部，嵌套不单独返回
        let text = big_json();
        let spans = find_json_spans(&text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0], (0, text.len()));
    }

    #[test]
    fn outer_invalid_inner_valid_found() {
        // 外层括号平衡但解析失败 → 只前进 1 字符，内部合法数组仍可找到
        let inner: Vec<String> = (0..30)
            .map(|i| format!(r#"{{"id": {i}, "pad": "xxxxxxxx"}}"#))
            .collect();
        let inner_arr = format!("[{}]", inner.join(","));
        assert!(inner_arr.len() >= MIN_SPAN_BYTES);
        assert!(serde_json::from_str::<Value>(&inner_arr).is_ok());
        // 外层加了尾随垃圾 → 不合法；但尾随垃圾在闭合括号之后，外层切片
        // 其实合法……改为用未加引号的键构造外层不合法：
        let text = format!("pre {{broken: {inner_arr}}} post");
        let spans = find_json_spans(&text);
        assert_eq!(spans.len(), 1, "spans: {spans:?}");
        assert_eq!(&text[spans[0].0..spans[0].1], inner_arr);
    }

    #[test]
    fn replace_none_keeps_bytes_exact() {
        let json = big_json();
        let text = format!("before\n{json}\nafter");
        let out = replace_json_spans(&text, |_| None);
        assert_eq!(out, text);
    }

    #[test]
    fn replace_splices_and_preserves_rest() {
        let json = big_json();
        let text = format!("cmd output line\n{json}\nnext cmd line");
        let out = replace_json_spans(&text, |chunk| {
            assert_eq!(chunk, json);
            Some("<<stash:ABC123>>".to_string())
        });
        assert_eq!(out, format!("cmd output line\n<<stash:ABC123>>\nnext cmd line"));
    }

    #[test]
    fn replace_partial_none_keeps_that_span() {
        let text = format!("x\n{}\ny\n{}\nz", big_json(), big_json());
        // 先验证「回调按内容判断」的行为：两个 span 内容相同，返回值也相同
        let all_replaced = replace_json_spans(&text, |chunk| {
            assert_eq!(chunk, big_json());
            Some("R".to_string())
        });
        assert_eq!(all_replaced.matches('R').count(), 2);
        // 再用计数器语义验证「None 的 span 原样保留」
        // （replace 回调是 Fn，用 Cell 计数）
        let seen = std::cell::Cell::new(0);
        let out = replace_json_spans(&text, |_| {
            seen.set(seen.get() + 1);
            if seen.get() == 1 {
                Some("REPLACED".to_string())
            } else {
                None
            }
        });
        assert!(out.contains("REPLACED"));
        assert!(out.contains(&big_json()));
        assert_eq!(out.matches("REPLACED").count(), 1);
    }

    #[test]
    fn stash_marker_span_left_alone() {
        // 已含 stash 标记的内容照样传给回调（由调用方决定跳过）；
        // 这里验证回调返回 None 时字节不变
        let text = format!("keep\n{}`", big_json());
        assert_eq!(replace_json_spans(&text, |_| None), text);
    }

    #[test]
    fn no_spans_returns_text_unchanged() {
        let text = "just plain text\nno json here";
        assert_eq!(replace_json_spans(text, |_| Some("X".into())), text);
        assert!(find_json_spans(text).is_empty());
    }
}
