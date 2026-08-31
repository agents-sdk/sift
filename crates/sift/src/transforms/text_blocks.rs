//! 保守纯文本选择：只删除同一章节内完整相同的块，不推断语义等价。
use std::collections::HashSet;

pub(super) struct BlockSelection {
    pub compressed: String,
    pub kept_lines: Vec<usize>,
    pub total_blocks: usize,
    pub kept_blocks: usize,
}

pub(super) fn select(input: &str) -> BlockSelection {
    let lines: Vec<_> = input.split_inclusive('\n').collect();
    let mut keep = vec![true; lines.len()];
    let mut seen = HashSet::new();
    let mut total_blocks = 0;
    let mut kept_blocks = 0;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        let start = i;
        let heading = is_heading(lines[i]) || (i + 1 < lines.len() && is_underline(lines[i + 1]));
        let fence = fence_start(lines[i]);
        if let Some((ch, width)) = fence {
            i += 1;
            while i < lines.len() {
                let line = lines[i].trim();
                i += 1;
                if line.chars().take_while(|&c| c == ch).count() >= width
                    && line.trim_matches(ch).is_empty()
                {
                    break;
                }
            }
        } else if heading {
            i += 1;
            if i < lines.len() && is_underline(lines[i]) {
                i += 1;
            }
            seen.clear(); // 不跨章节删除相同内容。
        } else {
            let speaker = speaker_prefix(lines[i]);
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() {
                let next = lines[i];
                if is_heading(next)
                    || fence_start(next).is_some()
                    || (i + 1 < lines.len() && is_underline(lines[i + 1]))
                {
                    break;
                }
                // 缩进续行属于当前块，不单独抽走列表说明或发言中的细节。
                if !next.starts_with([' ', '\t']) {
                    if is_list_start(next) {
                        break;
                    }
                    if let Some(next_speaker) = speaker_prefix(next) {
                        if speaker != Some(next_speaker) {
                            break;
                        }
                    }
                }
                i += 1;
            }
        }
        let content_end = i;
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        let text = lines[start..content_end].concat();
        let key = text.trim_end_matches(['\r', '\n']);
        let protected = heading
            || fence.is_some()
            || lines[start..content_end]
                .iter()
                .any(|line| super::log_context::is_command_echo(line) || is_decision(line))
            || crate::secrets::contains_secret_token(&text);
        total_blocks += 1;
        if protected || seen.insert(key.to_owned()) {
            kept_blocks += 1;
        } else {
            keep[start..i].fill(false);
        }
    }
    BlockSelection {
        compressed: lines
            .iter()
            .enumerate()
            .filter(|(i, _)| keep[*i])
            .map(|(_, line)| *line)
            .collect(),
        kept_lines: keep
            .iter()
            .enumerate()
            .filter_map(|(i, &kept)| kept.then_some(i))
            .collect(),
        total_blocks,
        kept_blocks,
    }
}

fn fence_start(line: &str) -> Option<(char, usize)> {
    let line = line.trim_start();
    let ch = line.chars().next()?;
    if !matches!(ch, '`' | '~') {
        return None;
    }
    let width = line.chars().take_while(|&c| c == ch).count();
    (width >= 3).then_some((ch, width))
}

fn is_heading(line: &str) -> bool {
    let line = line.trim();
    let hashes = line.chars().take_while(|&c| c == '#').count();
    (1..=6).contains(&hashes) && line[hashes..].starts_with(char::is_whitespace)
        || line.ends_with([':', '：'])
}

fn is_underline(line: &str) -> bool {
    let line = line.trim();
    line.len() >= 3 && (line.chars().all(|c| c == '=') || line.chars().all(|c| c == '-'))
}

fn is_list_start(line: &str) -> bool {
    if ["- ", "* ", "+ "]
        .iter()
        .any(|prefix| line.starts_with(prefix))
    {
        return true;
    }
    let digits = line.bytes().take_while(|c| c.is_ascii_digit()).count();
    digits > 0
        && [". ", ") "]
            .iter()
            .any(|suffix| line[digits..].starts_with(suffix))
}

fn speaker_prefix(line: &str) -> Option<&str> {
    let (prefix, body) = line.split_once([':', '：'])?;
    // 这里只识别短标签，不把 URL、文件路径或长句子的冒号当成人名边界。
    if body.starts_with(['/', '\\']) || prefix.is_empty() || prefix.chars().count() > 32 {
        return None;
    }
    prefix
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'))
        .then_some(prefix)
}

fn is_decision(line: &str) -> bool {
    let line = line.trim_start().to_lowercase();
    [
        "结论",
        "决定",
        "决策",
        "注意",
        "风险",
        "待办",
        "conclusion",
        "decision",
        "warning",
        "todo",
    ]
    .iter()
    .any(|prefix| {
        line.strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with([':', '：']))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_is_literal_not_token_based() {
        let input = "value = +1\n\nvalue = -1\n\nvalue = +1\n\n";
        let result = select(input);
        assert_eq!(result.compressed, "value = +1\n\nvalue = -1\n\n");
        assert_eq!(result.kept_lines, vec![0, 1, 2, 3]);
    }

    #[test]
    fn crlf_and_unicode_bytes_are_preserved() {
        let block = "例行同步：继续等待确认。\r\n下一步仍由原负责人跟进。\r\n\r\n";
        let result = select(&format!("{block}{block}结论：保持现状。"));
        assert_eq!(result.compressed, format!("{block}结论：保持现状。"));
        assert_eq!(result.kept_lines, vec![0, 1, 2, 6]);
    }
}
