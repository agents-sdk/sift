//! 所有按行抽取的压缩器共用同一套省略提示；坐标来自解析阶段，绝不反向猜测。
use super::{OffloadOutput, OmissionRange};
use std::collections::BTreeSet;

pub(super) fn actionable_file_path(path: Option<&str>) -> Option<&str> {
    path.filter(|p| !p.is_empty() && !p.contains(['\r', '\n']))
}

pub(super) fn hint(path: &str, range: &OmissionRange, line_offset: usize) -> String {
    let path = serde_json::to_string(path).expect("路径字符串可序列化");
    format!(
        "{} lines omitted from file {path}, starting at line {}",
        range.line_count,
        range.start_line + line_offset
    )
}

/// 保留行使用输入中的 0-based 下标；输出始终按原文顺序，不受打分/分组重排影响。
/// 很短的空隙若比提示还便宜则直接保留，避免为一行引入更长的路径说明。
pub(super) fn render(
    input: &str,
    kept: impl IntoIterator<Item = usize>,
    path: &str,
    line_offset: usize,
) -> OffloadOutput {
    let lines: Vec<_> = input.split_inclusive('\n').collect();
    let kept: BTreeSet<_> = kept.into_iter().collect();
    let mut output = OffloadOutput::new(String::new(), input.into());
    let mut i = 0;
    while i < lines.len() {
        if kept.contains(&i) {
            output.compressed.push_str(lines[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && !kept.contains(&i) {
            i += 1;
        }
        let range = OmissionRange {
            start_line: start + 1,
            line_count: i - start,
        };
        let marker = format!("[... {}]\n", hint(path, &range, line_offset));
        let original_bytes: usize = lines[start..i].iter().map(|line| line.len()).sum();
        if marker.len() >= original_bytes {
            for line in &lines[start..i] {
                output.compressed.push_str(line);
            }
        } else {
            output.compressed.push_str(&marker);
            output.omissions.push(range);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_gaps_stay_and_trailing_newline_is_not_a_line() {
        let input = format!(
            "head\r\nx\r\nkeep\r\n{}\r\ntail\r\n",
            "long removed content ".repeat(20)
        );
        let output = render(&input, [0, 2, 4], "/tmp/a \"quoted\" stash", 0);
        assert!(output.compressed.starts_with("head\r\nx\r\nkeep\r\n"));
        assert_eq!(
            output.omissions,
            vec![OmissionRange {
                start_line: 4,
                line_count: 1
            }]
        );
        assert!(output.compressed.ends_with("tail\r\n"));
        assert!(output.compressed.contains("\\\"quoted\\\""));
    }

    #[test]
    fn section_offset_only_changes_display_not_local_ranges() {
        let input = format!("head\n{}\ntail", "removed ".repeat(40));
        let output = render(&input, [0, 2], "/tmp/stash", 100);
        assert!(output.compressed.contains("starting at line 102"));
        assert_eq!(output.omissions[0].start_line, 2);
        assert!(output.compressed.ends_with("tail"));
    }
}
