//! 行提示必须指向 stash 输入本身，而不是搜索命中的源文件或 diff 的目标行号。
use sift::content::ContentType;
use sift::transforms::diff_compressor::{DiffCompressorConfig, DiffCompressorTransform};
use sift::transforms::log_compressor::{LogCompressor, LogCompressorConfig};
use sift::transforms::reformats::LogTemplate;
use sift::transforms::search_compressor::{SearchCompressorConfig, SearchCompressorTransform};
use sift::transforms::{compressor_for, CompressionContext, OffloadTransform, ReformatTransform};

const PATH: &str = "/tmp/stash with spaces/0123456789abcdef01234567";

fn context() -> CompressionContext {
    CompressionContext {
        stash_file_path: Some(PATH.into()),
        ..Default::default()
    }
}

/// 把每处提示对应的原文切片插回，必须逐行完整重建输入。
fn assert_line_roundtrip(input: &str, output: &str) {
    let lines: Vec<_> = input.lines().collect();
    let mut restored = Vec::new();
    let mut hints = 0;
    for line in output.lines() {
        if let Some(hint) = line.strip_prefix("[... ") {
            let (count, tail) = hint.split_once(" lines omitted from file ").expect(line);
            let (path, start) = tail.split_once(", starting at line ").expect(line);
            assert_eq!(serde_json::from_str::<String>(path).unwrap(), PATH);
            let start: usize = start.strip_suffix(']').unwrap().parse().unwrap();
            let count: usize = count.parse().unwrap();
            assert_eq!(start, restored.len() + 1, "{line}");
            assert!(count > 0 && start + count - 1 <= lines.len());
            restored.extend_from_slice(&lines[start - 1..start - 1 + count]);
            hints += 1;
        } else {
            restored.push(line);
        }
    }
    assert!(hints > 0, "缺少内联省略提示: {output}");
    assert_eq!(restored, lines);
    assert!(!output.contains("retrieveLines"));
}

#[test]
fn search_hints_use_input_rows_even_when_files_interleave_and_repeat() {
    let input = std::iter::once("$ rg -n handler src/".into())
        .chain((0..120).map(|i| {
            format!(
                "src/{}.ts:{}:handler {}",
                i % 3,
                800 + i * 19,
                "payload ".repeat(16)
            )
        }))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n";
    let transform = SearchCompressorTransform::new(SearchCompressorConfig {
        max_matches_per_file: 3,
        max_total_matches: 6,
        max_files: 2,
        ..Default::default()
    });
    let result = transform.apply(&input, &context()).unwrap();
    assert_line_roundtrip(&input, &result.compressed);
    assert!(!result.omissions.is_empty());
}

#[test]
fn logs_have_inline_gaps_not_only_a_trailing_summary() {
    let input = (0..150)
        .map(|i| {
            let level = if i == 30 || i == 120 { "ERROR" } else { "INFO" };
            format!(
                "{level}: worker {i} {}",
                "processed diagnostic payload ".repeat(5)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let result = compressor_for(ContentType::BuildOutput)
        .unwrap()
        .apply(&input, &context())
        .unwrap();
    assert_line_roundtrip(&input, &result.compressed);
}

#[test]
fn build_commands_and_continuations_survive_even_a_tiny_log_budget() {
    let mut lines = vec![
        "".to_string(),
        "$ cargo build --release --workspace".to_string(),
    ];
    lines.extend((0..80).map(|i| {
        format!(
            "   Compiling package_{i} v0.2.155 {}",
            "routine dependency output ".repeat(5)
        )
    }));
    let commands = [
        "$ cargo test --workspace \\",
        "  --all-features \\",
        "  -- --nocapture",
        "+ npm run build",
        "> web@1.0.0 build",
        "> vite build",
        "PS C:\\repo> npm test",
        "$ cargo build --release --workspace",
    ];
    lines.extend(commands.iter().map(|s| s.to_string()));
    lines.push("ERROR compilation failed at src/main.rs:52".into());
    let input = lines.join("\r\n");
    let compressor = LogCompressor::new(LogCompressorConfig {
        max_total_lines: 1,
        error_context_lines: 0,
        min_lines_for_stash: 1,
        ..Default::default()
    });
    for ctx in [CompressionContext::default(), context()] {
        let output = compressor.apply(&input, &ctx).unwrap();
        for command in commands {
            assert_eq!(
                output
                    .compressed
                    .lines()
                    .filter(|line| *line == command)
                    .count(),
                input.lines().filter(|line| *line == command).count(),
                "命令必须逐次保留: {command}"
            );
        }
        assert!(output.compressed.contains("ERROR compilation failed"));
        if ctx.stash_file_path.is_some() {
            assert_line_roundtrip(&input, &output.compressed);
            assert!(output.omissions.iter().all(|r| r.start_line > 2));
        }
    }
}

#[test]
fn log_preamble_is_preserved_without_a_recognized_prompt() {
    let input = format!(
        "cargo build --release --workspace\n{}\nERROR build failed",
        "   Compiling libc v0.2.155\n".repeat(80)
    );
    let compressor = LogCompressor::new(LogCompressorConfig {
        max_total_lines: 0,
        error_context_lines: 0,
        min_lines_for_stash: 1,
        ..Default::default()
    });
    let output = compressor.apply(&input, &context()).unwrap();
    assert!(output
        .compressed
        .starts_with("cargo build --release --workspace\n"));
    assert_line_roundtrip(&input, &output.compressed);
}

#[test]
fn lossless_log_templates_also_leave_repeated_command_echoes_visible() {
    let input = (0..30)
        .map(|i| format!("$ cargo build --release --package service-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let output = LogTemplate::default()
        .apply(&input, &CompressionContext::default())
        .unwrap();
    assert_eq!(output, input, "命令不应隐藏在模板/参数表内");
}

#[test]
fn diff_hints_cover_context_hunks_and_dropped_files() {
    let mut input = "commit metadata\n".to_string();
    for f in 0..4 {
        input.push_str(&format!(
            "diff --git a/{f}.rs b/{f}.rs\nindex abc..def 100755\n--- a/{f}.rs\n+++ b/{f}.rs\n"
        ));
        for h in 0..8 {
            input.push_str(&format!(
                "@@ -{},40 +{},40 @@\n",
                900 + h * 40,
                999 + h * 40
            ));
            for i in 0..30 {
                input.push_str(&format!(
                    " context {f}.{h}.{i} {}\n",
                    "unchanged ".repeat(10)
                ));
            }
            input.push_str("-old implementation\n+new implementation\n");
        }
    }
    let transform = DiffCompressorTransform::new(DiffCompressorConfig {
        max_files: 2,
        max_hunks_per_file: 2,
        max_context_lines: 2,
        ..Default::default()
    });
    let result = transform.apply(&input, &context()).unwrap();
    assert_line_roundtrip(&input, &result.compressed);
    assert!(!result.omissions.is_empty());
}

#[test]
fn prose_maps_whole_lines_without_treating_sentences_as_lines() {
    let input = (0..60)
        .map(|_| {
            format!(
                "Meeting entry: {}. Another sentence follows. 中文也在同一行。",
                "ordinary progress report details ".repeat(8)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let result = compressor_for(ContentType::PlainText)
        .unwrap()
        .apply(&input, &context())
        .unwrap();
    assert_line_roundtrip(&input, &result.compressed);
}

#[test]
fn file_backed_pipeline_and_mixed_sections_use_the_complete_stash_coordinates() {
    use sift::stash::{FileStashStore, StashStore};
    use sift::text_api::compress_text;
    let dir = std::env::temp_dir().join(format!(
        "sift-line-hints-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = FileStashStore::new(&dir).unwrap();
    let mut input = (0..800)
        .map(|i| {
            format!(
                "Meeting progress {i}: {}",
                "ordinary project context ".repeat(4)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    input.push_str("\n{}");
    for i in 0..70 {
        input.push_str(&format!(
            "\nINFO: processing worker {i} {}",
            "diagnostic details ".repeat(6)
        ));
    }
    assert_eq!(
        sift::content::detect_content_type(&input),
        ContentType::PlainText
    );
    let result = compress_text(&input, Some(&store), None);
    assert!(result.lossy, "{result:?}");
    let key = result.stash_key.unwrap();
    let path = store.file_path(&key).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), input);
    let body = result
        .text
        .strip_suffix(&format!("<<stash:{key}>>"))
        .unwrap();
    let normalized = body.replace(
        &serde_json::to_string(path.to_str().unwrap()).unwrap(),
        &serde_json::to_string(PATH).unwrap(),
    );
    assert_line_roundtrip(&input, &normalized);
    assert!(
        normalized.contains("starting at line 806"),
        "日志分段必须使用完整 stash 的行号"
    );
    assert!(
        normalized.contains(input.lines().nth(801).unwrap()),
        "日志分段首行也应保留"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn single_line_prose_does_not_invent_sentence_line_numbers() {
    let input = "A long prose sentence with routine progress details. ".repeat(50);
    let output = compressor_for(ContentType::PlainText)
        .unwrap()
        .apply(&input, &context());
    assert!(output.is_err(), "单行内句子删除不应冒充整行省略");
}

#[test]
fn whole_line_chinese_prose_is_not_mistaken_for_a_secret() {
    let input = (0..80).map(|_| "周会记录：本周项目进展正常，边界条件已经确认，下一步继续完善文档和测试，相关工作按照计划进行。")
        .collect::<Vec<_>>().join("\n\n");
    let output = compressor_for(ContentType::PlainText)
        .unwrap()
        .apply(&input, &context())
        .unwrap();
    assert!(
        output.compressed.len() < input.len() / 2,
        "普通中文不应被当作密钥全部强制保留"
    );
    assert_line_roundtrip(&input, &output.compressed);
}
