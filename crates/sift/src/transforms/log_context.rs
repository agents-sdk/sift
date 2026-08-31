//! 日志中的执行上下文不属于可抽稀噪声；无损模板化与有损选择共用保护规则。
use std::collections::BTreeSet;

/// 返回必须原样可见的输入行下标：首个非空行、命令回显及显式续行。
pub(super) fn protected_lines(lines: &[&str]) -> BTreeSet<usize> {
    let mut protected = BTreeSet::new();
    let first = lines.iter().position(|line| !line.trim().is_empty());
    let mut continued = false;
    for (i, line) in lines.iter().enumerate() {
        if first == Some(i) || continued || is_command_echo(line) {
            protected.insert(i);
            continued = has_continuation(line);
        } else {
            continued = false;
        }
    }
    protected
}

pub(super) fn is_command_echo(line: &str) -> bool {
    let mut line = line.trim_start();
    // 忽略行首常见 ANSI SGR 颜色码，仅用于识别；输出仍保留原始字节。
    while let Some(rest) = line.strip_prefix("\u{1b}[") {
        let Some(end) = rest.find('m') else { break };
        if !rest[..end].bytes().all(|b| b.is_ascii_digit() || b == b';') {
            break;
        }
        line = rest[end + 1..].trim_start();
    }
    // POSIX 提示符、set -x、多层 shell tracing、npm script 回显。
    for marker in ["$", "#", "+", ">"] {
        let rest = line.trim_start_matches(marker);
        if rest.len() < line.len()
            && rest.starts_with(char::is_whitespace)
            && !rest.trim().is_empty()
        {
            return true;
        }
    }
    // 带工作目录的 PowerShell / cmd 提示符。
    if let Some(rest) = line.strip_prefix("PS ") {
        if let Some((cwd, command)) = rest.split_once('>') {
            if !cwd.is_empty() && !command.trim().is_empty() {
                return true;
            }
        }
    }
    if let Some((cwd, command)) = line.split_once('>') {
        let b = cwd.as_bytes();
        if b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && matches!(b[2], b'\\' | b'/')
            && !command.trim().is_empty()
        {
            return true;
        }
    }
    // user@host:cwd$ command / root@host:cwd# command。
    for marker in ["$ ", "# "] {
        if let Some((prompt, command)) = line.split_once(marker) {
            if prompt.contains('@')
                && !prompt.contains(char::is_whitespace)
                && !command.trim().is_empty()
            {
                return true;
            }
        }
    }
    false
}

fn has_continuation(line: &str) -> bool {
    let line = line.trim_end();
    let slashes = line.chars().rev().take_while(|&c| c == '\\').count();
    slashes % 2 == 1 || line.ends_with(['`', '^', '|']) || line.ends_with("&&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_common_command_echoes_but_not_ordinary_output() {
        for line in [
            "$ cargo build",
            "# make test",
            "++ npm test",
            "> app@1.0 build",
            "> vite build",
            "PS C:\\repo> npm test",
            "C:\\repo>cargo build",
            "dev@host:~/app$ npm test",
            "root@host:/app# make",
            "\u{1b}[32m$ cargo check",
        ] {
            assert!(is_command_echo(line), "{line}");
        }
        for line in [
            "Compiling libc v0.2",
            "INFO worker ready",
            "error: x > y",
            "at app.Main(main:10)",
            "cost $ 20",
            "#",
            "---",
        ] {
            assert!(!is_command_echo(line), "{line}");
        }
    }

    #[test]
    fn continuations_stop_at_the_last_command_line() {
        let lines = [
            "header",
            "noise",
            "$ cargo test \\",
            "  --workspace \\",
            "  -- --nocapture",
            "noise",
            "+ npm test &&",
            " npm run build",
            "noise",
        ];
        assert_eq!(protected_lines(&lines), BTreeSet::from([0, 2, 3, 4, 6, 7]));
        assert!(!has_continuation(r"echo \\"));
    }
}
