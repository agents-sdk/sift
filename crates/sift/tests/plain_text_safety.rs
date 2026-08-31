//! 默认纯文本压缩只折叠完整的相同块，不把信息差异误判成重复。
use sift::transforms::text_crusher::TextCrusher;
use sift::transforms::{CompressionContext, OffloadTransform};

const REPEAT: &str = "例行同步：当前仍在等待依赖团队确认，处理方式保持不变。\n这是一段重复状态播报，没有新的任务、决定或处理结果。";

#[test]
fn empty_input_is_unchanged_and_has_no_bloat() {
    let crusher = TextCrusher::default();
    let result = crusher.compress("", "");
    assert_eq!(result.compressed, "");
    assert_eq!(result.compression_ratio, 1.0);
    assert_eq!(crusher.estimate_bloat(""), 0.0);
}

#[test]
fn old_meeting_preserves_every_task_and_speaker_turn() {
    let input = include_str!("fixtures/plain_text_meeting.txt");
    let result = TextCrusher::default().compress(input, "结论 冻结前缀 CCR 发布");
    assert_eq!(
        result.compressed, input,
        "旧样例没有可确认的重复发言块，不能删不同任务"
    );
    for id in 100..126 {
        assert!(result.compressed.contains(&format!("COMP-{id}")));
    }
}

#[test]
fn budgets_and_queries_do_not_delete_unique_facts() {
    let input = [
        "任务 COMP-100 已完成，重试次数为 1，结果为通过。",
        "任务 COMP-101 已完成，重试次数为 1，结果为通过。",
        "任务 COMP-101 未完成，重试次数为 2，结果为失败。",
        "Alice: 当前余额为 +1，允许继续执行操作。",
        "Bob: 当前余额为 -1，不允许继续执行操作。",
        "结论：先修复失败任务，再执行下一次发布。",
    ]
    .join("\n\n");
    let result = TextCrusher::default().compress_with(&input, "unrelated", 0.05, Some(1));
    assert_eq!(result.compressed, input);
}

#[test]
fn repeats_are_removed_as_whole_paragraphs_with_first_occurrence_kept() {
    let input = format!(
        "会议纪要\n\n{}\n\n任务 COMP-100：已完成。\n\n{}\n\n{}\n\n结论：继续按既定计划推进。",
        REPEAT, REPEAT, REPEAT
    );
    let result = TextCrusher::default().compress(&input, "结论");
    assert!(result
        .compressed
        .starts_with(&format!("会议纪要\n\n{REPEAT}")));
    assert_eq!(result.compressed.matches(REPEAT).count(), 1);
    assert!(result.compressed.contains("任务 COMP-100：已完成。"));
    assert!(result.compressed.ends_with("结论：继续按既定计划推进。"));
    assert_eq!(result.compressed.matches("这是一段重复状态播报").count(), 1);
}

#[test]
fn different_speakers_and_sections_are_not_merged() {
    let input = format!("# 本周\n\n{REPEAT}\n\n# 下周\n\n{REPEAT}\n\nAlice: 工作已完成。\nAlice: 等待后续安排。\nBob: 工作已完成。\nBob: 等待后续安排。\n\n结论：两人分别跟进。\n");
    assert_eq!(
        TextCrusher::default().compress(&input, "工作").compressed,
        input
    );
}

#[test]
fn wrapped_prose_is_not_arbitrarily_split_into_selected_lines() {
    let input = "本段描述一个完整流程，需要按顺序阅读。\n首先准备环境并校验输入。\n然后处理请求并记录结果。\n若处理失败，则回滚并报告原因。\n最后由负责人确认状态。\n以上步骤构成一个整体，不能仅保留末尾一句。";
    assert_eq!(
        TextCrusher::default().compress(input, "最后").compressed,
        input
    );
}

#[test]
fn fenced_code_and_list_continuations_stay_intact() {
    let fence = "```text\n执行准备步骤\n\n确认结果\n```";
    let item = "- 任务 COMP-124\n  当前状态：未完成\n  处理方式：等待回调";
    let input = format!("操作说明\n\n{fence}\n\n{REPEAT}\n\n{REPEAT}\n\n{item}\n\n{fence}\n\n结论：不要重复执行。\n");
    let result = TextCrusher::default().compress(&input, "确认");
    assert_eq!(result.compressed.matches(fence).count(), 2);
    assert!(result.compressed.contains(item));
    assert_eq!(result.compressed.matches(REPEAT).count(), 1);
}

#[test]
fn single_line_text_does_not_get_sentence_based_line_hints() {
    let input = "This paragraph contains several sentences. It must remain a coherent paragraph. "
        .repeat(30);
    let ctx = CompressionContext {
        stash_file_path: Some("/tmp/stash/original".into()),
        ..Default::default()
    };
    assert!(TextCrusher::default().apply(&input, &ctx).is_err());
}
