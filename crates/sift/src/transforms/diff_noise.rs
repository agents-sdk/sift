//! diff 预处理：卸载 lockfile churn 与纯空白变化。
//!
//! 这里只生成压缩候选；原文存储与 `<<stash:...>>` 标记统一由 live zone 完成。

const MIN_LINES: usize = 30;
const BLOAT_THRESHOLD: f64 = 0.5;
const LOCKFILE_SUFFIXES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "poetry.lock",
    "Pipfile.lock",
    "Gemfile.lock",
    "go.sum",
    "composer.lock",
];

struct Segment<'a> {
    new_path: String,
    header_lines: Vec<&'a str>,
    body_lines: Vec<&'a str>,
}

impl Segment<'_> {
    fn is_lockfile(&self) -> bool {
        LOCKFILE_SUFFIXES.iter().any(|suffix| {
            self.new_path
                .strip_suffix(suffix)
                .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with(['/', '\\']))
        })
    }

    fn is_whitespace_only(&self) -> bool {
        let mut additions = Vec::new();
        let mut deletions = Vec::new();
        for line in &self.body_lines {
            if line.starts_with('+') && !line.starts_with("+++") {
                additions.push(strip_ascii_whitespace(&line[1..]));
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions.push(strip_ascii_whitespace(&line[1..]));
            }
        }
        !additions.is_empty() && additions == deletions
    }

    fn body_bytes(&self) -> usize {
        self.body_lines.iter().map(|line| line.len() + 1).sum()
    }
}

/// 返回比原文更小的噪声清理候选；无足够收益时返回 `None`。
pub(super) fn compact(content: &str) -> Option<String> {
    if content.lines().count() < MIN_LINES {
        return None;
    }
    let (prelude, segments) = parse_segments(content);
    if segments.is_empty() {
        return None;
    }

    let total_bytes: usize = segments.iter().map(Segment::body_bytes).sum();
    let droppable_bytes: usize = segments
        .iter()
        .filter(|segment| segment.is_lockfile() || segment.is_whitespace_only())
        .map(Segment::body_bytes)
        .sum();
    if total_bytes == 0 || droppable_bytes as f64 / (total_bytes as f64) < BLOAT_THRESHOLD {
        return None;
    }

    let mut output = String::with_capacity(content.len());
    for line in prelude {
        push_line(&mut output, line);
    }
    let mut dropped = false;
    for segment in segments {
        for line in &segment.header_lines {
            push_line(&mut output, line);
        }
        let reason = if segment.is_lockfile() {
            Some("lockfile")
        } else if segment.is_whitespace_only() {
            Some("whitespace-only")
        } else {
            None
        };
        if let Some(reason) = reason {
            output.push_str("[diff_noise: ");
            output.push_str(reason);
            output.push_str(" hunks dropped (");
            output.push_str(&segment.body_lines.len().to_string());
            output.push_str(" lines)]\n");
            dropped = true;
        } else {
            for line in &segment.body_lines {
                push_line(&mut output, line);
            }
        }
    }
    if !content.ends_with('\n') {
        output.pop();
    }
    (dropped && output.len() < content.len()).then_some(output)
}

fn push_line(output: &mut String, line: &str) {
    output.push_str(line);
    output.push('\n');
}

fn strip_ascii_whitespace(input: &str) -> String {
    input.chars().filter(|c| !c.is_ascii_whitespace()).collect()
}

fn parse_segments(content: &str) -> (Vec<&str>, Vec<Segment<'_>>) {
    let mut prelude = Vec::new();
    let mut segments = Vec::new();
    let mut current: Option<Segment<'_>> = None;
    let mut in_body = false;

    for line in content.lines() {
        if line.starts_with("diff --git ") {
            if let Some(segment) = current.take() {
                segments.push(segment);
            }
            current = Some(Segment {
                new_path: parse_new_path(line),
                header_lines: vec![line],
                body_lines: Vec::new(),
            });
            in_body = false;
            continue;
        }
        let Some(segment) = current.as_mut() else {
            prelude.push(line);
            continue;
        };
        if !in_body && line.starts_with("@@") {
            in_body = true;
        }
        if in_body {
            segment.body_lines.push(line);
        } else {
            segment.header_lines.push(line);
        }
    }
    if let Some(segment) = current {
        segments.push(segment);
    }
    (prelude, segments)
}

fn parse_new_path(header: &str) -> String {
    header
        .rfind(" b/")
        .map(|index| header[index + 3..].to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(path: &str, old: &str, new: &str) -> String {
        let mut output = format!(
            "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,30 +1,30 @@\n"
        );
        for _ in 0..26 {
            output.push_str(" context\n");
        }
        output.push('-');
        output.push_str(old);
        output.push('\n');
        output.push('+');
        output.push_str(new);
        output.push('\n');
        output
    }

    #[test]
    fn lockfile_body_is_replaced_with_summary() {
        let input = diff("crates/app/Cargo.lock", "version = 1", "version = 2");
        let output = compact(&input).expect("lockfile 应产生压缩候选");
        assert!(output.contains("[diff_noise: lockfile hunks dropped"));
        assert!(!output.contains("version = 1"));
        assert!(output.starts_with("diff --git a/crates/app/Cargo.lock"));
    }

    #[test]
    fn whitespace_only_body_is_replaced_with_summary() {
        let input = diff("src/main.rs", "  let value = 1;", "let   value = 1;");
        let output = compact(&input).expect("纯空白变化应产生压缩候选");
        assert!(output.contains("[diff_noise: whitespace-only hunks dropped"));
        assert!(!output.contains("let value"));
    }

    #[test]
    fn meaningful_change_is_not_dropped() {
        let input = diff("src/main.rs", "let value = 1;", "let value = 2;");
        assert!(compact(&input).is_none());
    }

    #[test]
    fn lockfile_suffix_requires_path_boundary() {
        let input = diff("src/MyCargo.lock", "value = 1", "value = 2");
        assert!(compact(&input).is_none());
    }
}
