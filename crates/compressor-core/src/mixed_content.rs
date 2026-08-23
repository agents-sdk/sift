//! 混合内容分段路由：把混排内容（bash 输出里夹着命令回显、JSON、日志、
//! 文本）按类型切成 `Section` 段，供逐段分发到不同压缩器。
//!
//! 核心入口 `split_into_sections`。
//! 与参考实现的差异（简化 + 求稳）：
//! - 不处理代码围栏/搜索结果的专属优先级，统一交给 `detect_content_type`
//!   的结构化判据做类型判定；
//! - JSON 段用「平衡括号扫描 + serde 解析」提取，与参考一致但按字节
//!   精确切片（行拼接会吞掉闭合括号之后的同行尾巴）；
//! - 文本段的切分原则是「宁可不切也不错切」：只有当累积内容的检测类型
//!   发生变化时才切段；空白行不触发切段。

use crate::content::{detect_content_type, ContentType};

/// 混合内容中的一个类型化段落。行号 0 起，含两端。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// 该段的原文（字节精确切片，不含被排除的行首空白）。
    pub content: String,
    /// 该段的内容类型（由 `detect_content_type` 判定）。
    pub content_type: ContentType,
    /// 起始行（0 起，含）。
    pub start_line: usize,
    /// 结束行（0 起，含）。
    pub end_line: usize,
}

/// 把混合内容切成类型化段落。
///
/// 算法（单遍，行驱动）：
/// 1. 若某行去空白后以 `[` / `{` 开头，尝试从括号字节起做平衡扫描
///    （跟踪字符串/转义状态），提取出的片段若能被 serde_json 成功解析，
///    则作为一段 JSON（JsonArray 类型，对象也归此类）；
/// 2. 否则进入文本段累积：逐行并入，重新对累积内容做 `detect_content_type`，
///    类型一旦与当前段类型不同就关闭当前段、从该行重新开始；
///    空白行并入当前段、不参与类型判定；
/// 3. 段落类型取段内容整体的检测结果。
pub fn split_into_sections(text: &str) -> Vec<Section> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut sections: Vec<Section> = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        // ── JSON 块提取 ──
        if trimmed.starts_with('[') || trimmed.starts_with('{') {
            if let Some((span, end_line)) = extract_json_span(text, &lines, i) {
                sections.push(Section {
                    content: span,
                    content_type: ContentType::JsonArray,
                    start_line: i,
                    end_line,
                });
                i = end_line + 1;
                continue;
            }
        }

        // ── 文本段累积 ──
        let start_line = i;
        let mut buf: Vec<&str> = vec![lines[i]];
        let section_type = detect_content_type(lines[i]);
        i += 1;
        while i < lines.len() {
            let next = lines[i];
            // 空白行并入当前段，不改变类型、不触发切段
            if next.trim().is_empty() {
                buf.push(next);
                i += 1;
                continue;
            }
            // JSON 候选行交给下一轮的 JSON 提取
            let t = next.trim_start();
            if t.starts_with('[') || t.starts_with('{') {
                break;
            }
            // 重新对累积内容判型；类型变化才切（宁可不切也不错切）
            let candidate = format!("{}\n{}", buf.join("\n"), next);
            let detected = detect_content_type(&candidate);
            if detected != section_type {
                break;
            }
            buf.push(next);
            i += 1;
        }

        let content = buf.join("\n");
        // 段类型以段整体检测为准（首行判型可能被后续行修正）
        let final_type = detect_content_type(&content);
        sections.push(Section {
            content,
            content_type: final_type,
            start_line,
            end_line: i - 1,
        });
    }

    sections
}

/// 从第 `line_idx` 行的首个 `[`/`{` 字节起做平衡括号扫描（跟踪字符串与
/// 转义状态），返回 `(JSON 片段, 结束行号)`；片段需能被 serde_json 成功
/// 解析才算命中，否则返回 None。
fn extract_json_span(text: &str, lines: &[&str], line_idx: usize) -> Option<(String, usize)> {
    // 定位起始括号的字节偏移（行首字节 + 前导空白）
    let mut line_start = 0usize;
    for l in &lines[..line_idx] {
        line_start += l.len() + 1;
    }
    let lead = lines[line_idx].len() - lines[line_idx].trim_start().len();
    let start = line_start + lead;

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut end = None;

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
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + offset + ch.len_utf8());
                    break;
                }
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
    }
    let end = end?;

    let span = &text[start..end];
    if serde_json::from_str::<serde_json::Value>(span).is_err() {
        return None;
    }
    let end_line = line_idx + text[start..end].matches('\n').count();
    Some((span.to_string(), end_line))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 辅助：拼一个大 JSON 数组（≥ 阈值不影响分段，仅让内容更真实）。
    fn big_json_array() -> String {
        let items: Vec<String> = (0..40)
            .map(|i| format!(r#"{{"id": {i}, "name": "item-{i}", "desc": "some padding text {i}"}}"#))
            .collect();
        format!("[\n  {}\n]", items.join(",\n  "))
    }

    #[test]
    fn single_plain_text_returns_one_section() {
        let text = "hello world\nsecond line\nthird line";
        let s = split_into_sections(text);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].content_type, ContentType::PlainText);
        assert_eq!(s[0].start_line, 0);
        assert_eq!(s[0].end_line, 2);
        assert_eq!(s[0].content, text);
    }

    #[test]
    fn single_json_block_is_one_section() {
        let text = big_json_array();
        let s = split_into_sections(&text);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].content_type, ContentType::JsonArray);
        assert_eq!(s[0].start_line, 0);
        assert_eq!(s[0].end_line, text.split('\n').count() - 1);
    }

    #[test]
    fn text_then_json_then_text() {
        let json = big_json_array();
        let text = format!("running query...\n\n{json}\n\ndone, 40 rows");
        let s = split_into_sections(&text);
        assert_eq!(s.len(), 3, "sections: {s:?}");
        assert_eq!(s[0].content_type, ContentType::PlainText);
        assert!(s[0].content.contains("running query"));
        assert_eq!(s[1].content_type, ContentType::JsonArray);
        assert_eq!(s[1].content, json);
        // JSON 段起止行
        let json_start = text.lines().position(|l| l.trim_start().starts_with('[')).unwrap();
        assert_eq!(s[1].start_line, json_start);
        assert_eq!(s[2].content_type, ContentType::PlainText);
        assert!(s[2].content.contains("done, 40 rows"));
        // 行号连续且覆盖全文
        assert_eq!(s[0].start_line, 0);
        assert_eq!(s[2].end_line, text.lines().count() - 1);
        assert_eq!(s[0].end_line + 1, s[1].start_line);
        assert_eq!(s[1].end_line + 1, s[2].start_line);
    }

    #[test]
    fn text_then_log_lines_split() {
        let text = "intro prose here\n2026-08-21 10:00:00 INFO boot\n2026-08-21 10:00:01 ERROR crash";
        let s = split_into_sections(text);
        assert_eq!(s.len(), 2, "sections: {s:?}");
        assert_eq!(s[0].content_type, ContentType::PlainText);
        assert_eq!(s[1].content_type, ContentType::BuildOutput);
        assert!(s[1].content.contains("ERROR crash"));
    }

    #[test]
    fn json_object_span_also_extracted() {
        // JSON 对象（detect_content_type 不认，但分段提取认）
        let obj = "{\"items\": [1, 2, 3], \"msg\": \"hello\"}".to_string()
            + &"\n".repeat(0);
        let text = format!("curl http://api\n{obj}\nOK");
        let s = split_into_sections(&text);
        assert_eq!(s.len(), 3, "sections: {s:?}");
        assert_eq!(s[1].content_type, ContentType::JsonArray);
        assert_eq!(s[1].content, obj);
    }

    #[test]
    fn brace_inside_string_not_misjudged() {
        // 字符串字面量里的 `}` 不应提前闭合
        let json = r#"{"a": "contain } brace", "b": [1, 2]}"#;
        let text = format!("head\n{json}\ntail");
        let s = split_into_sections(&text);
        assert_eq!(s.len(), 3, "sections: {s:?}");
        assert_eq!(s[1].content, json);
    }

    #[test]
    fn unbalanced_bracket_falls_back_to_text() {
        let text = "some text\n[not json at all, just an open bracket\ntail line";
        let s = split_into_sections(text);
        // 不切错：全部并入文本段（允许 1 或 2 段，但绝不能出现 JsonArray）
        assert!(s.iter().all(|x| x.content_type == ContentType::PlainText), "{s:?}");
        assert!(s[0].content.contains("some text"));
    }

    #[test]
    fn blank_lines_do_not_fragment_sections() {
        let text = "line one\n\n\nline two\n";
        let s = split_into_sections(text);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].content_type, ContentType::PlainText);
        // 末尾空行并入段内，内容逐字节保留
        assert_eq!(s[0].content, text);
    }

    #[test]
    fn empty_text_yields_single_empty_section() {
        let s = split_into_sections("");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].content_type, ContentType::PlainText);
        assert_eq!(s[0].content, "");
    }

    #[test]
    fn multiline_json_with_nested_braces() {
        let json = r#"{
  "data": {
    "items": [
      {"id": 1, "tags": ["a", "b"]},
      {"id": 2, "tags": ["c"]}
    ]
  },
  "ok": true
}"#;
        let text = format!("prefix\n{json}\nsuffix");
        let s = split_into_sections(&text);
        assert_eq!(s.len(), 3, "sections: {s:?}");
        assert_eq!(s[1].content, json);
        assert_eq!(s[1].content_type, ContentType::JsonArray);
    }

    #[test]
    fn trailing_text_after_close_brace_on_same_line_stays_outside() {
        // 闭合括号后的同行文本不被吞进 JSON 段
        let json = r#"{"a": 1, "b": 2}"#;
        let text = format!("{json} # comment");
        let s = split_into_sections(&text);
        // 行首就是括号：JSON 段只切到 `}` 为止
        assert_eq!(s.len(), 1, "sections: {s:?}");
        assert_eq!(s[0].content_type, ContentType::JsonArray);
        assert_eq!(s[0].content, json);
    }
}
