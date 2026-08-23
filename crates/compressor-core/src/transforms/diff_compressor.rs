//! Unified-diff 压缩器：hunk 采样 + 上下文裁剪。

//!
//! 压缩流程：
//! 1. 把 unified diff 解析为 文件 + hunk 结构（手写解析器，不依赖 regex）。
//! 2. 文件数超 `max_files` 时按总变更量（+/- 行数）排序保留最重的。
//! 3. 每文件 hunk 数超 `max_hunks_per_file` 时保留 首个 + 末个 + 得分最高的
//!    中间 hunk（相关性打分：变更密度 + query 词重叠（含 CJK bigram）+
//!    优先级模式），再按 `@@` 起始行号恢复顺序。
//! 4. 每个 `+`/`-` 行两侧裁剪到 `max_context_lines`；
//!    `\ No newline at end of file` 等结构标记无条件保留。
//! 5. 压缩节省达标时以 blake3 生成 cache_key 并追加取回标记
//!    （原文由调用方经 `OffloadTransform::apply` 卸载到 CCR store）。
//!
//! trait 选择：本压缩器**有损**（丢弃 hunk 与上下文行），因此实现
//! [`crate::transforms::OffloadTransform`] 而非 `ReformatTransform`。
//!
//! 与参考实现的差异（受依赖约束）：
//! - 不用 `md5`/`regex`/`tracing`：cache_key 用 blake3（`crate::ccr`），
//!   hunk/文件头解析手写，观测统计直接随 `compress_with_stats` 返回。
//! - 优先级模式用内置关键词表 + 词边界检测替代 regex。

use std::collections::{BTreeMap, BTreeSet};

use crate::ccr;
use crate::content::ContentType;
use crate::transforms::{CompressionContext, OffloadTransform, TransformError};

// ─── 打分权重常量（与参考实现逐值一致）──────────────────────────────────

/// 变更密度基项的每行权重：`min(CAP, 变更数 * WEIGHT)`。
pub const SCORE_CHANGE_DENSITY_WEIGHT: f64 = 0.03;
/// 变更密度基项上限。
pub const SCORE_CHANGE_DENSITY_CAP: f64 = 0.3;
/// query 中每个命中词（长度 > 2）的加权。
pub const SCORE_CONTEXT_WORD_WEIGHT: f64 = 0.2;
/// query 词参与匹配的最小长度（`len(word) > 2` 过滤停用词）。
pub const SCORE_CONTEXT_MIN_WORD_LEN: usize = 2;
/// 任一优先级模式命中的加权（每个 hunk 只加一次，对齐参考的 `break`）。
pub const SCORE_PRIORITY_PATTERN_BOOST: f64 = 0.3;
/// hunk 总分上限。
pub const SCORE_TOTAL_CAP: f64 = 1.0;

// ─── 配置 ────────────────────────────────────────────────────────────────

/// 配置。默认值与参考实现 `DiffCompressorConfig` 一致。
#[derive(Debug, Clone)]
pub struct DiffCompressorConfig {
    /// 每个 `+`/`-` 行单侧保留的上下文行数。
    pub max_context_lines: usize,
    /// 每文件保留的 hunk 上限（超出则首+末+高分中间）。
    pub max_hunks_per_file: usize,
    /// 整个 diff 保留的文件上限（超出按总变更量保留最重的）。
    pub max_files: usize,
    /// 保留位（兼容字段）：算法总是保留 `+` 行。
    pub always_keep_additions: bool,
    /// 保留位：同上，针对 `-` 行。
    pub always_keep_deletions: bool,
    /// 是否在输出追加 CCR 取回标记。
    pub enable_ccr: bool,
    /// 最小输入行数：低于此值整体原样返回（不解析不压缩）。
    /// 参考实现名为 min_lines_for_ccr（名不副实地门控整条压缩路径）。
    pub min_lines_for_ccr: usize,
    /// CCR 标记的压缩比阈值：压缩后行数 < 原始行数 × 此值才追加。
    pub min_compression_ratio_for_ccr: f64,
}

impl Default for DiffCompressorConfig {
    fn default() -> Self {
        Self {
            max_context_lines: 2,
            max_hunks_per_file: 10,
            max_files: 20,
            always_keep_additions: true,
            always_keep_deletions: true,
            enable_ccr: true,
            min_lines_for_ccr: 50,
            min_compression_ratio_for_ccr: 0.8,
        }
    }
}

// ─── 结果与统计 ──────────────────────────────────────────────────────────

/// 压缩结果。
#[derive(Debug, Clone)]
pub struct DiffCompressionResult {
    pub compressed: String,
    pub original_line_count: usize,
    pub compressed_line_count: usize,
    pub files_affected: usize,
    pub additions: usize,
    pub deletions: usize,
    pub hunks_kept: usize,
    pub hunks_removed: usize,
    pub cache_key: Option<String>,
}

/// 旁路观测统计（不在压缩输出里）。
#[derive(Debug, Clone, Default)]
pub struct DiffCompressorStats {
    pub input_lines: usize,
    pub output_lines: usize,
    /// `output_lines / input_lines`，1.0 表示未压缩。
    pub compression_ratio: f64,

    pub files_total: usize,
    pub files_kept: usize,
    /// `max_files` 触发时被丢弃的文件标签（`old -> new`）。
    pub files_dropped: Vec<String>,

    pub hunks_total: usize,
    pub hunks_kept: usize,
    pub hunks_dropped: usize,
    /// 每文件 hunk 丢弃数。
    pub hunks_dropped_per_file: BTreeMap<String, usize>,

    pub context_lines_input: usize,
    pub context_lines_kept: usize,
    pub context_lines_trimmed: usize,

    /// 保留的最大 hunk 行数。
    pub largest_hunk_kept_lines: usize,
    /// 丢弃的最大 hunk 行数。
    pub largest_hunk_dropped_lines: usize,

    /// 输出时被规范化为 `100644` 的文件模式行（可执行位等信息丢失，
    /// 在此显式暴露而非静默吞掉）。
    pub file_mode_normalizations: Vec<(String, String)>,
    /// 输出时被简化为 `Binary files differ` 的二进制标记原文。
    pub binary_files_simplified: Vec<String>,

    /// CCR 标记是否被附加。
    pub cache_key_emitted: bool,
    /// 未附加标记时的原因。
    pub ccr_skipped_reason: Option<String>,
}

// ─── 压缩器 ──────────────────────────────────────────────────────────────

/// 压缩器本体，只持有配置。
#[derive(Debug, Clone)]
pub struct DiffCompressor {
    config: DiffCompressorConfig,
}

impl Default for DiffCompressor {
    fn default() -> Self {
        Self::new(DiffCompressorConfig::default())
    }
}

impl DiffCompressor {
    pub fn new(config: DiffCompressorConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &DiffCompressorConfig {
        &self.config
    }

    /// 压缩入口。`context` 为用户 query（hunk 采样打分用）。
    pub fn compress(&self, content: &str, context: &str) -> DiffCompressionResult {
        self.compress_with_stats(content, context).0
    }

    /// 同 [`compress`](Self::compress)，另返回观测统计。
    pub fn compress_with_stats(
        &self,
        content: &str,
        context: &str,
    ) -> (DiffCompressionResult, DiffCompressorStats) {
        let mut stats = DiffCompressorStats::default();

        // `split('\n')` 与 Python 语义一致：结尾换行产出空末元素。
        let lines: Vec<&str> = content.split('\n').collect();
        let original_line_count = lines.len();
        stats.input_lines = original_line_count;

        // 短路 1：输入低于阈值 → 原样返回（短 diff 不值得压缩）。
        if original_line_count < self.config.min_lines_for_ccr {
            stats.output_lines = original_line_count;
            stats.compression_ratio = 1.0;
            stats.ccr_skipped_reason = Some("input below min_lines_for_ccr".into());
            return (pass_through_result(content, original_line_count), stats);
        }

        // 解析：pre-diff 内容（commit 头、邮件头）原样保留并在输出回放。
        let parsed = parse_diff(&lines);
        let pre_diff_lines = parsed.pre_diff_lines;
        let mut diff_files = parsed.files;
        stats.files_total = diff_files.len();
        stats.hunks_total = diff_files.iter().map(|f| f.hunks.len()).sum();
        stats.context_lines_input = diff_files
            .iter()
            .flat_map(|f| f.hunks.iter())
            .map(|h| h.context_lines)
            .sum();

        // 短路 2：没解析出任何 diff 段 → 原样返回（畸形输入保真）。
        if diff_files.is_empty() {
            stats.output_lines = original_line_count;
            stats.compression_ratio = 1.0;
            stats.ccr_skipped_reason = Some("no diff sections parsed".into());
            return (pass_through_result(content, original_line_count), stats);
        }

        // hunk 相关性打分（仅在 max_hunks_per_file 触发时起作用）。
        score_hunks(&mut diff_files, context);

        // 文件上限：按总变更量降序保留前 max_files。
        if diff_files.len() > self.config.max_files {
            diff_files.sort_by(|a, b| {
                let a_changes = a.total_additions() + a.total_deletions();
                let b_changes = b.total_additions() + b.total_deletions();
                b_changes.cmp(&a_changes)
            });
            let dropped: Vec<DiffFile> = diff_files.split_off(self.config.max_files);
            stats.files_dropped = dropped
                .iter()
                .map(|f| format!("{} -> {}", f.old_file, f.new_file))
                .collect();
        }
        stats.files_kept = diff_files.len();

        // 捕获有损输出信号（模式规范化 / 二进制简化），暴露给观测。
        for file in diff_files.iter() {
            let label = format!("{} -> {}", file.old_file, file.new_file);
            if let Some(orig) = &file.original_new_file_mode_line {
                if orig != "new file mode 100644" {
                    stats
                        .file_mode_normalizations
                        .push((label.clone(), orig.clone()));
                }
            }
            if let Some(orig) = &file.original_deleted_file_mode_line {
                if orig != "deleted file mode 100644" {
                    stats
                        .file_mode_normalizations
                        .push((label.clone(), orig.clone()));
                }
            }
            if let Some(orig) = &file.original_binary_line {
                if orig != "Binary files differ" {
                    stats.binary_files_simplified.push(orig.clone());
                }
            }
        }

        // 每文件：hunk 数量上限 + 上下文裁剪。
        let mut compressed_files: Vec<DiffFile> = Vec::with_capacity(diff_files.len());
        let mut total_additions = 0usize;
        let mut total_deletions = 0usize;
        let mut hunks_kept_total = 0usize;
        let mut hunks_removed_total = 0usize;
        let mut largest_kept = 0usize;
        let mut largest_dropped = 0usize;
        let mut context_kept_total = 0usize;

        for file in diff_files {
            total_additions += file.total_additions();
            total_deletions += file.total_deletions();

            let original_hunk_count = file.hunks.len();
            let file_label = format!("{} -> {}", file.old_file, file.new_file);

            let (selected, dropped) = select_hunks(file.hunks, self.config.max_hunks_per_file);
            let dropped_count = dropped.len();
            if dropped_count > 0 {
                stats.hunks_dropped_per_file.insert(file_label, dropped_count);
                let max_dropped = dropped.iter().map(|h| h.lines.len()).max().unwrap_or(0);
                if max_dropped > largest_dropped {
                    largest_dropped = max_dropped;
                }
            }

            let mut compressed_hunks: Vec<DiffHunk> = Vec::with_capacity(selected.len());
            for hunk in selected {
                let trimmed = reduce_context(&hunk, self.config.max_context_lines);
                if trimmed.lines.len() > largest_kept {
                    largest_kept = trimmed.lines.len();
                }
                context_kept_total += trimmed.context_lines;
                compressed_hunks.push(trimmed);
            }

            hunks_kept_total += compressed_hunks.len();
            hunks_removed_total += original_hunk_count - compressed_hunks.len();

            compressed_files.push(DiffFile {
                hunks: compressed_hunks,
                ..file
            });
        }

        stats.hunks_kept = hunks_kept_total;
        stats.hunks_dropped = hunks_removed_total;
        stats.context_lines_kept = context_kept_total;
        stats.context_lines_trimmed = stats.context_lines_input.saturating_sub(context_kept_total);
        stats.largest_hunk_kept_lines = largest_kept;
        stats.largest_hunk_dropped_lines = largest_dropped;

        let files_affected = compressed_files.len();

        let mut compressed_output = format_output(
            &pre_diff_lines,
            &compressed_files,
            files_affected,
            total_additions,
            total_deletions,
            hunks_removed_total,
        );
        let compressed_line_count = count_split_lines(&compressed_output);

        // CCR 层：节省达标才追加标记。注意 compressed_line_count 在追加标记
        // **之前**捕获（标记自身的 "compressed to N" 与结果字段都用它），
        // 与参考实现一致。
        let savings_threshold = self.config.min_compression_ratio_for_ccr;
        let mut cache_key: Option<String> = None;
        if self.config.enable_ccr
            && (compressed_line_count as f64) < (original_line_count as f64) * savings_threshold
        {
            let key = ccr::compute_key(content);
            compressed_output.push('\n');
            compressed_output.push_str(&format!(
                "[{} lines compressed to {}. Retrieve full diff: {}]",
                original_line_count,
                compressed_line_count,
                ccr::marker_for(&key)
            ));
            cache_key = Some(key);
            stats.cache_key_emitted = true;
        } else if !self.config.enable_ccr {
            stats.ccr_skipped_reason = Some("ccr disabled".into());
        } else {
            stats.ccr_skipped_reason = Some(format!(
                "compression ratio {:.3} above threshold {:.3}",
                if original_line_count == 0 {
                    1.0
                } else {
                    compressed_line_count as f64 / original_line_count as f64
                },
                savings_threshold
            ));
        }

        stats.output_lines = compressed_line_count;
        stats.compression_ratio = if original_line_count == 0 {
            1.0
        } else {
            compressed_line_count as f64 / original_line_count as f64
        };

        let result = DiffCompressionResult {
            compressed: compressed_output,
            original_line_count,
            compressed_line_count,
            files_affected,
            additions: total_additions,
            deletions: total_deletions,
            hunks_kept: hunks_kept_total,
            hunks_removed: hunks_removed_total,
            cache_key,
        };

        (result, stats)
    }
}

// ─── 内部结构 ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct DiffHunk {
    header: String,
    lines: Vec<String>,
    additions: usize,
    deletions: usize,
    context_lines: usize,
    /// 相关性得分；仅当 max_hunks_per_file 触发时被消费。
    score: f64,
}

#[derive(Debug, Clone)]
struct DiffFile {
    header: String,
    old_file: String,
    new_file: String,
    hunks: Vec<DiffHunk>,
    is_binary: bool,
    is_new_file: bool,
    is_deleted_file: bool,
    is_renamed: bool,
    /// rename / similarity / dissimilarity / copy 标记行（原样保留回放，
    /// 否则输出会被误读为原地修改）。
    rename_lines: Vec<String>,
    /// 原始 `new file mode <NNNNNN>` 行（检测输出规范化损失）。
    original_new_file_mode_line: Option<String>,
    /// 原始 `deleted file mode <NNNNNN>` 行。
    original_deleted_file_mode_line: Option<String>,
    /// 原始 `Binary files X and Y differ` 行。
    original_binary_line: Option<String>,
}

impl DiffFile {
    fn total_additions(&self) -> usize {
        self.hunks.iter().map(|h| h.additions).sum()
    }
    fn total_deletions(&self) -> usize {
        self.hunks.iter().map(|h| h.deletions).sum()
    }
}

// ─── 解析器（手写，替代参考实现的 regex 集）─────────────────────────────

/// 解析输出：首个 diff 头之前的内容 + 解析出的文件结构。
struct ParsedDiff {
    pre_diff_lines: Vec<String>,
    files: Vec<DiffFile>,
}

/// 识别任意 diff 文件段头：`diff --git a/X b/Y`、`diff --combined <path>`、
/// `diff --cc <path>`（后两者是 merge commit 形态，漏识别会把整个 merge
/// diff 当作 pre-diff 内容原样放过）。
fn is_diff_header(line: &str) -> bool {
    if let Some(rest) = line.strip_prefix("diff --git ") {
        // `a/<old> b/<new>`；简单校验 `a/` 前缀与 ` b/` 分隔存在即可。
        return rest.starts_with("a/") && rest[2..].contains(" b/");
    }
    line.starts_with("diff --combined ") || line.starts_with("diff --cc ")
}

/// `--- a/<file>` 或 `--- /dev/null`。
fn is_old_file_line(line: &str) -> bool {
    (line.starts_with("--- a/") && line.len() > 6) || line == "--- /dev/null"
}

/// `+++ b/<file>` 或 `+++ /dev/null`。
fn is_new_file_line(line: &str) -> bool {
    (line.starts_with("+++ b/") && line.len() > 6) || line == "+++ /dev/null"
}

/// `Binary files ... differ`。
fn is_binary_line(line: &str) -> bool {
    line.starts_with("Binary files ") && line.ends_with(" differ")
}

/// hunk 头判定：2-4 个 `@` 开头，后跟若干 `[-+]N[,M]` 段（至少一个 `-`
/// 段和一个 `+` 段），再以同数 `@` 收尾；收尾 `@` 之后允许 trailing
/// context（`@@ ... @@ fn main() {`）。支持 3/4 路 merge 的 `@@@`/`@@@@`；
/// 更高阶的 n 路 merge（5+ 个 `@`）极端罕见，按非 hunk 行处理。
fn is_hunk_header(line: &str) -> bool {
    let ats = line.chars().take_while(|&c| c == '@').count();
    if !(2..=4).contains(&ats) || line.len() < 2 * ats {
        return false;
    }
    let b = line.as_bytes();
    // `@@` 之后、首段 `-` 之前有一个空格。
    if b.get(ats) != Some(&b' ') {
        return false;
    }
    let mut i = ats + 1;
    // 段序列：`-N[,M]` 或 `+N[,M]`，段间以单个空格分隔。
    let mut saw_plus = false;
    let mut saw_minus = false;
    loop {
        if i >= b.len() {
            return false;
        }
        match b[i] {
            b'-' => saw_minus = true,
            b'+' => saw_plus = true,
            _ => return false,
        }
        i += 1;
        let ds = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == ds {
            return false; // 符号后必须有数字。
        }
        if i < b.len() && b[i] == b',' {
            i += 1;
            let cs = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i == cs {
                return false;
            }
        }
        if i < b.len() && b[i] == b' ' {
            // 空格后跟 `-`/`+` 是下一段；否则是收尾 `@` 串前的分隔。
            if i + 1 < b.len() && (b[i + 1] == b'-' || b[i + 1] == b'+') {
                i += 1;
                continue;
            }
            i += 1;
        }
        break;
    }
    saw_plus && saw_minus && line[i..].starts_with(&"@".repeat(ats))
}

/// 从 hunk 头提取新文件起始行号（`+N`），普通与 combined 头通用。
fn extract_line_number(header: &str) -> Option<usize> {
    let b = header.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
            let start = i + 1;
            let mut end = start;
            while end < b.len() && b[end].is_ascii_digit() {
                end += 1;
            }
            return header[start..end].parse::<usize>().ok();
        }
        i += 1;
    }
    None
}

fn parse_diff(lines: &[&str]) -> ParsedDiff {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current_file: Option<DiffFile> = None;
    let mut current_hunk: Option<DiffHunk> = None;
    let mut pre_diff_lines: Vec<String> = Vec::new();

    for &line in lines {
        if is_diff_header(line) {
            if let Some(h) = current_hunk.take() {
                if let Some(f) = current_file.as_mut() {
                    f.hunks.push(h);
                }
            }
            if let Some(f) = current_file.take() {
                files.push(f);
            }
            current_file = Some(DiffFile {
                header: line.to_string(),
                old_file: String::new(),
                new_file: String::new(),
                hunks: Vec::new(),
                is_binary: false,
                is_new_file: false,
                is_deleted_file: false,
                is_renamed: false,
                rename_lines: Vec::new(),
                original_new_file_mode_line: None,
                original_deleted_file_mode_line: None,
                original_binary_line: None,
            });
            continue;
        }

        // 首个 diff 头之前的内容：commit 元数据、邮件头等，原样保留。
        if current_file.is_none() {
            pre_diff_lines.push(line.to_string());
            continue;
        }

        if let Some(f) = current_file.as_mut() {
            if line.starts_with("new file mode") {
                f.is_new_file = true;
                f.original_new_file_mode_line = Some(line.to_string());
                continue;
            } else if line.starts_with("deleted file mode") {
                f.is_deleted_file = true;
                f.original_deleted_file_mode_line = Some(line.to_string());
                continue;
            } else if line.starts_with("rename ")
                || line.starts_with("similarity ")
                || line.starts_with("copy ")
                || line.starts_with("dissimilarity ")
            {
                f.is_renamed = true;
                f.rename_lines.push(line.to_string());
                continue;
            } else if is_binary_line(line) {
                f.is_binary = true;
                f.original_binary_line = Some(line.to_string());
                continue;
            }
        }

        if is_old_file_line(line) {
            if let Some(f) = current_file.as_mut() {
                f.old_file = line.to_string();
            }
            continue;
        }
        if is_new_file_line(line) {
            if let Some(f) = current_file.as_mut() {
                f.new_file = line.to_string();
            }
            continue;
        }

        if is_hunk_header(line) {
            if let Some(h) = current_hunk.take() {
                if let Some(f) = current_file.as_mut() {
                    f.hunks.push(h);
                }
            }
            current_hunk = Some(DiffHunk {
                header: line.to_string(),
                lines: Vec::new(),
                additions: 0,
                deletions: 0,
                context_lines: 0,
                score: 0.0,
            });
            continue;
        }

        // hunk 内容行。
        if let Some(h) = current_hunk.as_mut() {
            if line.starts_with('+') && !line.starts_with("+++") {
                h.additions += 1;
                h.lines.push(line.to_string());
            } else if line.starts_with('-') && !line.starts_with("---") {
                h.deletions += 1;
                h.lines.push(line.to_string());
            } else if line.starts_with(' ') || line.is_empty() {
                h.context_lines += 1;
                h.lines.push(line.to_string());
            } else {
                // 其他行（`\ No newline at end of file` 等）原样追加，
                // 是否在裁剪中幸存由其与最近 `+`/`-` 行的距离决定。
                h.lines.push(line.to_string());
            }
        }
    }

    if let Some(h) = current_hunk.take() {
        if let Some(f) = current_file.as_mut() {
            f.hunks.push(h);
        }
    }
    if let Some(f) = current_file.take() {
        files.push(f);
    }

    ParsedDiff {
        pre_diff_lines,
        files,
    }
}

// ─── 打分 ────────────────────────────────────────────────────────────────

/// 优先级关键词组（近似参考实现的 PRIORITY_PATTERNS_DIFF 三组 regex）。
const PRIORITY_WORD_GROUPS: &[&[&str]] = &[
    &[
        "error",
        "exception",
        "failed",
        "failure",
        "fatal",
        "critical",
        "crash",
        "panic",
    ],
    &["important", "note", "todo", "fixme", "hack", "xxx", "bug", "fix"],
    &["security", "auth", "password", "secret", "token"],
];

/// 大小写不敏感的整词包含检测（近似 `\b(word)s?\b`：允许复数后缀）。
fn contains_any_word(lower_text: &str, words: &[&str]) -> bool {
    let b = lower_text.as_bytes();
    for w in words {
        let mut start = 0;
        while let Some(pos) = lower_text[start..].find(w) {
            let s = start + pos;
            let mut e = s + w.len();
            if e < b.len() && b[e] == b's' {
                e += 1;
            }
            let before_ok = s == 0 || !b[s - 1].is_ascii_alphanumeric();
            let after_ok = e >= b.len() || !b[e].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
            start = s + 1;
        }
    }
    false
}

/// CJK 字符（假名、汉字、谚文）判定，码点范围与参考实现一致。
fn is_cjk_char(c: char) -> bool {
    matches!(
        c as u32,
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF
    )
}

/// 小写 query 的 CJK 连续段二元组：无空格中文 query 靠 bigram 部分命中 hunk。
fn cjk_bigrams(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut run: Vec<char> = Vec::new();
    for c in text.chars() {
        if is_cjk_char(c) {
            run.push(c);
        } else {
            for w in run.windows(2) {
                out.insert(w.iter().collect::<String>());
            }
            run.clear();
        }
    }
    for w in run.windows(2) {
        out.insert(w.iter().collect::<String>());
    }
    out
}

fn score_hunks(files: &mut [DiffFile], context: &str) {
    let context_lower = context.to_lowercase();
    let context_words: Vec<&str> = context_lower.split_whitespace().collect();
    let cjk_bg = cjk_bigrams(&context_lower);

    for file in files.iter_mut() {
        for hunk in file.hunks.iter_mut() {
            let mut score: f64 = 0.0;
            // 变更密度基项（截断）。
            score += (hunk.additions as f64 + hunk.deletions as f64) * SCORE_CHANGE_DENSITY_WEIGHT;
            if score > SCORE_CHANGE_DENSITY_CAP {
                score = SCORE_CHANGE_DENSITY_CAP;
            }

            let hunk_content_lower = hunk.lines.join("\n").to_lowercase();

            for word in &context_words {
                if word.chars().count() > SCORE_CONTEXT_MIN_WORD_LEN
                    && hunk_content_lower.contains(word)
                {
                    score += SCORE_CONTEXT_WORD_WEIGHT;
                }
            }
            for bg in &cjk_bg {
                if hunk_content_lower.contains(bg.as_str()) {
                    score += SCORE_CONTEXT_WORD_WEIGHT;
                }
            }

            for group in PRIORITY_WORD_GROUPS {
                if contains_any_word(&hunk_content_lower, group) {
                    score += SCORE_PRIORITY_PATTERN_BOOST;
                    break; // 每个 hunk 只加一次。
                }
            }

            if score > SCORE_TOTAL_CAP {
                score = SCORE_TOTAL_CAP;
            }
            hunk.score = score;
        }
    }
}

// ─── hunk 采样（max_hunks_per_file 触发）────────────────────────────────

/// 保留 首个 + 末个 + 得分最高的中间 hunk，再按 hunk 头起始行号恢复顺序。
/// 返回 (按原顺序的幸存者, 被丢弃者)。
fn select_hunks(hunks: Vec<DiffHunk>, max_per_file: usize) -> (Vec<DiffHunk>, Vec<DiffHunk>) {
    if hunks.len() <= max_per_file {
        return (hunks, Vec::new());
    }
    if hunks.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut indexed: Vec<(usize, DiffHunk)> = hunks.into_iter().enumerate().collect();

    let first = indexed.remove(0);
    let last = if !indexed.is_empty() {
        Some(indexed.pop().unwrap())
    } else {
        None
    };
    let middle: Vec<(usize, DiffHunk)> = indexed;

    let remaining_slots = if last.is_some() {
        max_per_file.saturating_sub(2)
    } else {
        max_per_file.saturating_sub(1)
    };

    // 中间 hunk 按分数降序取前 remaining_slots 个。
    let mut middle_sorted = middle;
    middle_sorted.sort_by(|a, b| {
        b.1.score
            .partial_cmp(&a.1.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let (kept_middle, dropped_middle): (Vec<_>, Vec<_>) = middle_sorted
        .into_iter()
        .enumerate()
        .partition(|(rank, _)| *rank < remaining_slots);
    let kept_middle: Vec<(usize, DiffHunk)> = kept_middle.into_iter().map(|(_, x)| x).collect();
    let dropped_middle: Vec<DiffHunk> = dropped_middle.into_iter().map(|(_, (_, h))| h).collect();

    // 按捕获的原始索引重组，再按 @@ 起始行号排序恢复出现顺序。
    let mut selected: Vec<(usize, DiffHunk)> = Vec::with_capacity(max_per_file);
    selected.push(first);
    selected.extend(kept_middle);
    if let Some(l) = last {
        selected.push(l);
    }
    selected.sort_by_key(|(_, h)| extract_line_number(&h.header).unwrap_or(0));

    (
        selected.into_iter().map(|(_, h)| h).collect(),
        dropped_middle,
    )
}

// ─── 上下文裁剪 ──────────────────────────────────────────────────────────

fn reduce_context(hunk: &DiffHunk, max_context: usize) -> DiffHunk {
    // `+`/`-` 行索引。
    let change_positions: Vec<usize> = hunk
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            if l.starts_with('+') || l.starts_with('-') {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    if change_positions.is_empty() {
        // 无变更 hunk：保留开头至多 max_context 行（对齐参考实现）。
        let take = max_context.min(hunk.lines.len());
        let lines: Vec<String> = hunk.lines.iter().take(take).cloned().collect();
        return DiffHunk {
            header: hunk.header.clone(),
            lines,
            additions: 0,
            deletions: 0,
            context_lines: take,
            score: hunk.score,
        };
    }

    // 保留集：每个变更行 ± max_context。
    let mut keep = BTreeSet::new();
    for &pos in &change_positions {
        keep.insert(pos);
        let lo = pos.saturating_sub(max_context);
        for i in lo..pos {
            keep.insert(i);
        }
        let hi = (pos + max_context + 1).min(hunk.lines.len());
        for i in (pos + 1)..hi {
            keep.insert(i);
        }
    }

    // `\ No newline at end of file` 等结构标记无条件保留——它们是补丁
    // 结构语义，丢失会破坏可回放的 patch。
    for (i, line) in hunk.lines.iter().enumerate() {
        if line.starts_with('\\') {
            keep.insert(i);
        }
    }

    let mut new_lines: Vec<String> = Vec::with_capacity(keep.len());
    let mut additions = 0usize;
    let mut deletions = 0usize;
    let mut context_lines = 0usize;
    for &i in &keep {
        let line = &hunk.lines[i];
        new_lines.push(line.clone());
        if line.starts_with('+') {
            additions += 1;
        } else if line.starts_with('-') {
            deletions += 1;
        } else {
            context_lines += 1;
        }
    }

    DiffHunk {
        header: hunk.header.clone(),
        lines: new_lines,
        additions,
        deletions,
        context_lines,
        score: hunk.score,
    }
}

// ─── 输出格式化 ──────────────────────────────────────────────────────────

fn format_output(
    pre_diff_lines: &[String],
    files: &[DiffFile],
    files_affected: usize,
    total_additions: usize,
    total_deletions: usize,
    hunks_removed: usize,
) -> String {
    let mut out_lines: Vec<String> = Vec::new();

    // pre-diff 内容（commit 头、邮件头）原样回放在最前。
    for l in pre_diff_lines {
        out_lines.push(l.clone());
    }

    for f in files {
        out_lines.push(f.header.clone());

        // rename/similarity 标记紧跟 `diff --git`，与 git 原生顺序一致。
        for l in &f.rename_lines {
            out_lines.push(l.clone());
        }

        if f.is_new_file {
            out_lines.push("new file mode 100644".into());
        } else if f.is_deleted_file {
            out_lines.push("deleted file mode 100644".into());
        }

        if f.is_binary {
            out_lines.push("Binary files differ".into());
            continue;
        }

        if !f.old_file.is_empty() {
            out_lines.push(f.old_file.clone());
        }
        if !f.new_file.is_empty() {
            out_lines.push(f.new_file.clone());
        }

        for h in &f.hunks {
            out_lines.push(h.header.clone());
            for l in &h.lines {
                out_lines.push(l.clone());
            }
        }
    }

    // 汇总脚注：只要碰过至少一个文件就输出。
    if hunks_removed > 0 || files_affected > 0 {
        let mut parts = Vec::with_capacity(3);
        parts.push(format!("{} files changed", files_affected));
        parts.push(format!("+{} -{} lines", total_additions, total_deletions));
        if hunks_removed > 0 {
            parts.push(format!("{} hunks omitted", hunks_removed));
        }
        out_lines.push(format!("[{}]", parts.join(", ")));
    }

    out_lines.join("\n")
}

// ─── 工具 ────────────────────────────────────────────────────────────────

fn pass_through_result(content: &str, line_count: usize) -> DiffCompressionResult {
    DiffCompressionResult {
        compressed: content.to_string(),
        original_line_count: line_count,
        compressed_line_count: line_count,
        files_affected: 0,
        additions: 0,
        deletions: 0,
        hunks_kept: 0,
        hunks_removed: 0,
        cache_key: None,
    }
}

/// `s.split('\n').count()` —— 与 Python `len(content.split("\n"))` 一致。
fn count_split_lines(s: &str) -> usize {
    s.split('\n').count()
}

// ─── OffloadTransform 适配 ───────────────────────────────────────────────

/// `OffloadTransform` 适配器：路由层按 `ContentType::GitDiff` 分发。
pub struct DiffCompressorTransform {
    compressor: DiffCompressor,
}

impl DiffCompressorTransform {
    pub fn new(config: DiffCompressorConfig) -> Self {
        Self {
            compressor: DiffCompressor::new(config),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DiffCompressorConfig::default())
    }

    pub fn compressor(&self) -> &DiffCompressor {
        &self.compressor
    }
}

impl OffloadTransform for DiffCompressorTransform {
    fn name(&self) -> &'static str {
        "diff_compressor"
    }

    fn applies_to(&self) -> ContentType {
        ContentType::GitDiff
    }

    /// 膨胀度：上下文行占 hunk 总行数的比例（无 diff 结构则 0）。
    fn estimate_bloat(&self, input: &str) -> f64 {
        let lines: Vec<&str> = input.split('\n').collect();
        let parsed = parse_diff(&lines);
        let mut total = 0usize;
        let mut context = 0usize;
        for f in &parsed.files {
            for h in &f.hunks {
                total += h.lines.len();
                context += h.context_lines;
            }
        }
        if total == 0 {
            0.0
        } else {
            context as f64 / total as f64
        }
    }

    fn cache_key(&self, input: &str) -> String {
        ccr::compute_key(input)
    }

    fn apply(
        &self,
        input: &str,
        ctx: &CompressionContext,
    ) -> Result<(String, String), TransformError> {
        let context = ctx.query.as_deref().unwrap_or("");
        let result = self.compressor.compress(input, context);
        Ok((result.compressed, input.to_string()))
    }
}

// ─── 测试 ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── 解析器原语 ──────────────────────────────────────────────────

    #[test]
    fn hunk_header_detection() {
        assert!(is_hunk_header("@@ -1,10 +1,12 @@"));
        assert!(is_hunk_header("@@ -1 +1 @@"));
        assert!(is_hunk_header("@@@ -1,3 -1,3 +1,4 @@@"));
        assert!(is_hunk_header("@@ -5,7 +5,8 @@ fn main() {"));
        assert!(!is_hunk_header("not a header"));
        assert!(!is_hunk_header("@"));
        assert!(!is_hunk_header("@@ no numbers @@"));
    }

    #[test]
    fn extract_line_number_finds_plus_segment() {
        assert_eq!(extract_line_number("@@ -1,10 +1,12 @@"), Some(1));
        assert_eq!(extract_line_number("@@ -100 +200 @@"), Some(200));
        assert_eq!(extract_line_number("@@@ -1,3 -1,3 +1,4 @@@"), Some(1));
        assert_eq!(extract_line_number("garbage"), None);
    }

    #[test]
    fn diff_header_variants() {
        assert!(is_diff_header("diff --git a/x.py b/x.py"));
        assert!(is_diff_header("diff --combined merge.py"));
        assert!(is_diff_header("diff --cc merge.py"));
        assert!(!is_diff_header("diff --git broken"));
        assert!(!is_diff_header("random text"));
    }

    #[test]
    fn count_split_lines_matches_python_split_semantics() {
        assert_eq!(count_split_lines(""), 1);
        assert_eq!(count_split_lines("a"), 1);
        assert_eq!(count_split_lines("a\n"), 2);
        assert_eq!(count_split_lines("a\nb"), 2);
    }

    // ─── 短路路径 ────────────────────────────────────────────────────

    #[test]
    fn short_input_passes_through() {
        let c = DiffCompressor::default();
        let input = "diff --git a/x b/x\n@@ -1 +1 @@\n-a\n+b";
        let r = c.compress(input, "");
        // 4 行 < min_lines_for_ccr(50) → 原样返回。
        assert_eq!(r.compressed, input);
        assert_eq!(r.original_line_count, 4);
        assert_eq!(r.compressed_line_count, 4);
        assert_eq!(r.files_affected, 0);
        assert!(r.cache_key.is_none());
    }

    #[test]
    fn non_diff_input_passes_through() {
        let c = DiffCompressor::default();
        let input = "this is not a diff\n".repeat(60);
        let (r, stats) = c.compress_with_stats(&input, "");
        assert_eq!(r.compressed, input);
        assert_eq!(r.files_affected, 0);
        assert_eq!(stats.ccr_skipped_reason.as_deref(), Some("no diff sections parsed"));
    }

    #[test]
    fn stats_emitted_for_pass_through() {
        let c = DiffCompressor::default();
        let input = "noise\n".repeat(60);
        let (_r, stats) = c.compress_with_stats(&input, "");
        assert_eq!(stats.input_lines, 61); // 60 个换行 split 出 61 个元素
        assert_eq!(stats.output_lines, 61);
        assert_eq!(stats.compression_ratio, 1.0);
    }

    // ─── 测试夹具 ────────────────────────────────────────────────────

    /// 构造 n 文件的合成 diff（对齐参考实现的 parity 夹具形态）。
    fn build_synthetic_diff(n_files: usize) -> String {
        let mut s = String::new();
        for i in 0..n_files {
            s.push_str(&format!(
                "diff --git a/file_{i}.py b/file_{i}.py\n--- a/file_{i}.py\n+++ b/file_{i}.py\n@@ -1,10 +1,12 @@\n",
            ));
            for k in 0..5 {
                s.push_str(&format!(" context_{k}_{i}\n"));
            }
            for k in 0..3 {
                s.push_str(&format!("-removed_{k}_{i}\n"));
            }
            for k in 0..5 {
                s.push_str(&format!("+added_{k}_{i}\n"));
            }
            for k in 0..5 {
                s.push_str(&format!(" tail_{k}_{i}\n"));
            }
        }
        s.push_str("# variant 1");
        s
    }

    /// 单文件 n hunk，每 hunk 2 上下文 + 1 删 + 1 增 + 2 上下文，
    /// hunk 起始行号相隔 100 保证互不影响。
    fn build_n_hunk_diff(n: usize) -> String {
        let mut s = String::from("diff --git a/big.py b/big.py\n--- a/big.py\n+++ b/big.py\n");
        for i in 0..n {
            let start = i * 100 + 1;
            s.push_str(&format!("@@ -{start},6 +{start},6 @@\n"));
            s.push_str(&format!(" ctx_a_{i}\n"));
            s.push_str(&format!(" ctx_b_{i}\n"));
            s.push_str(&format!("-old_{i}\n"));
            s.push_str(&format!("+new_{i}\n"));
            s.push_str(&format!(" ctx_c_{i}\n"));
            s.push_str(&format!(" ctx_d_{i}\n"));
        }
        s
    }

    // ─── 端到端 ──────────────────────────────────────────────────────

    #[test]
    fn synthetic_eight_file_diff_matches_known_shape() {
        // 数值断言对齐参考实现的同名测试。
        let c = DiffCompressor::default();
        let r = c.compress(&build_synthetic_diff(8), "");
        assert_eq!(r.original_line_count, 177);
        assert_eq!(r.files_affected, 8);
        assert_eq!(r.additions, 40);
        assert_eq!(r.deletions, 24);
        assert_eq!(r.hunks_kept, 8);
        assert_eq!(r.hunks_removed, 0);
        assert_eq!(r.compressed_line_count, 129);
        assert!(r.cache_key.is_some());
    }

    #[test]
    fn max_hunks_per_file_cap_keeps_first_and_last() {
        // 15 hunk、上限 10 → 丢 5；首 + 末 + 8 个高分中间。
        let cfg = DiffCompressorConfig::default();
        let input = build_n_hunk_diff(15);
        let (result, stats) = DiffCompressor::new(cfg).compress_with_stats(&input, "");

        assert_eq!(result.hunks_kept, 10);
        assert_eq!(result.hunks_removed, 5);
        assert_eq!(stats.hunks_total, 15);
        assert_eq!(stats.hunks_dropped, 5);
        let per_file_total: usize = stats.hunks_dropped_per_file.values().sum();
        assert_eq!(per_file_total, 5);
        assert!(stats.largest_hunk_dropped_lines >= 6);

        // 保首保末：首尾 hunk 的变更行必须出现在输出中。
        assert!(result.compressed.contains("-old_0\n"));
        assert!(result.compressed.contains("+new_0\n"));
        assert!(result.compressed.contains(&format!("-old_14\n")));
        assert!(result.compressed.contains("+new_14\n"));
        // 汇总脚注带 hunk 丢弃数。
        assert!(result.compressed.contains("5 hunks omitted"));
    }

    #[test]
    fn max_files_cap_drops_lightest_files() {
        // 25 文件、上限 20 → 丢 5 个最轻的，名字进 stats。
        let cfg = DiffCompressorConfig::default();
        let input = build_synthetic_diff(25);
        let (_r, stats) = DiffCompressor::new(cfg).compress_with_stats(&input, "");
        assert_eq!(stats.files_total, 25);
        assert_eq!(stats.files_kept, 20);
        assert_eq!(stats.files_dropped.len(), 5);
        for label in &stats.files_dropped {
            assert!(label.contains("-> "), "标签 `{label}` 应为 `old -> new` 形态");
        }
    }

    #[test]
    fn context_trim_keeps_lines_within_window() {
        // max_context_lines=2：变更行远端的上下文被裁掉。
        let cfg = DiffCompressorConfig {
            max_context_lines: 2,
            min_lines_for_ccr: 5,
            min_compression_ratio_for_ccr: 0.1, // 关掉标记方便断言正文
            ..Default::default()
        };
        let mut input = String::from(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,10 +1,10 @@\n",
        );
        for k in 0..4 {
            input.push_str(&format!(" far_top_{k}\n"));
        }
        input.push_str(" near_a\n near_b\n-old\n+new\n near_c\n near_d\n");
        for k in 0..4 {
            input.push_str(&format!(" far_bottom_{k}\n"));
        }
        let r = DiffCompressor::new(cfg).compress(&input, "");
        for k in 0..4 {
            assert!(!r.compressed.contains(&format!("far_top_{k}")));
            assert!(!r.compressed.contains(&format!("far_bottom_{k}")));
        }
        for kept in ["near_a", "near_b", "near_c", "near_d", "-old", "+new"] {
            assert!(r.compressed.contains(kept), "窗口内行被误裁: {kept}");
        }
        assert!(r.compressed.len() < input.len());
    }

    #[test]
    fn min_compression_ratio_for_ccr_is_configurable() {
        // 默认 0.8：177→129（0.729）达标 → 发标记。
        let r = DiffCompressor::default().compress(&build_synthetic_diff(8), "");
        assert!(r.cache_key.is_some());

        // 0.5：同一压缩比 0.729 不达标 → 不发标记。
        let cfg = DiffCompressorConfig {
            min_compression_ratio_for_ccr: 0.5,
            ..Default::default()
        };
        let (r2, stats) =
            DiffCompressor::new(cfg).compress_with_stats(&build_synthetic_diff(8), "");
        assert!(r2.cache_key.is_none());
        assert!(!stats.cache_key_emitted);
        assert!(stats.ccr_skipped_reason.is_some());
    }

    // ─── 有损输出信号 ────────────────────────────────────────────────

    #[test]
    fn file_mode_normalization_is_recorded_for_executable_bit() {
        // 100755 输出时被规范化为 100644（对齐参考实现的输出格式），
        // 原始模式行必须进 stats 暴露该损失。
        let mut input = String::from(
            "diff --git a/script.sh b/script.sh\n\
             new file mode 100755\n\
             --- /dev/null\n\
             +++ b/script.sh\n\
             @@ -0,0 +1,3 @@\n\
             +#!/bin/sh\n\
             +echo hi\n\
             +exit 0\n",
        );
        for _ in 0..50 {
            input.push_str("# pad\n");
        }
        let (_r, stats) = DiffCompressor::default().compress_with_stats(&input, "");
        assert_eq!(stats.file_mode_normalizations.len(), 1, "{stats:?}");
        let (label, original) = &stats.file_mode_normalizations[0];
        assert!(label.contains("script.sh"));
        assert_eq!(original, "new file mode 100755");
    }

    #[test]
    fn binary_files_simplification_is_recorded() {
        let mut input = String::from(
            "diff --git a/img.png b/img.png\n\
             Binary files a/img.png and b/img.png differ\n",
        );
        for _ in 0..60 {
            input.push_str("# pad\n");
        }
        let (r, stats) = DiffCompressor::default().compress_with_stats(&input, "");
        assert!(r.compressed.contains("Binary files differ"));
        assert_eq!(stats.binary_files_simplified.len(), 1);
        assert_eq!(
            stats.binary_files_simplified[0],
            "Binary files a/img.png and b/img.png differ"
        );
    }

    // ─── 关键行为修复测试 ────────────────────────────────────────────

    #[test]
    fn rename_markers_are_preserved_in_output() {
        // rename/similarity 标记必须进入输出，否则 rename 被误读为原地修改。
        let input = "diff --git a/old.py b/new.py\n\
                     similarity index 92%\n\
                     rename from old.py\n\
                     rename to new.py\n\
                     --- a/old.py\n\
                     +++ b/new.py\n\
                     @@ -1,3 +1,3 @@\n\
                      ctx_a\n\
                     -old_line\n\
                     +new_line\n\
                      ctx_b\n";
        let cfg = DiffCompressorConfig {
            min_lines_for_ccr: 5,
            min_compression_ratio_for_ccr: 0.1,
            ..Default::default()
        };
        let r = DiffCompressor::new(cfg).compress(input, "");
        assert!(r.compressed.contains("similarity index 92%"), "{}", r.compressed);
        assert!(r.compressed.contains("rename from old.py"));
        assert!(r.compressed.contains("rename to new.py"));
    }

    #[test]
    fn combined_diff_3way_content_is_parsed_and_emitted() {
        // `@@@` 3 路 merge hunk 不能被静默丢弃。
        let input = "diff --git a/merge.py b/merge.py\n\
                     --- a/merge.py\n\
                     +++ b/merge.py\n\
                     @@@ -1,3 -1,3 +1,4 @@@\n\
                       unchanged_a\n\
                      -old_branch_1\n\
                     - old_branch_2\n\
                     ++new_in_merge\n\
                      +new_added\n\
                       unchanged_b\n";
        let cfg = DiffCompressorConfig {
            min_lines_for_ccr: 5,
            ..Default::default()
        };
        let (r, stats) = DiffCompressor::new(cfg).compress_with_stats(input, "");
        assert!(
            r.compressed.contains("@@@ -1,3 -1,3 +1,4 @@@"),
            "@@@ 头未被保留:\n{}",
            r.compressed
        );
        assert!(r.compressed.contains("++new_in_merge"));
        assert!(stats.files_total > 0, "combined-diff 仍未被解析");
    }

    #[test]
    fn diff_combined_header_starts_a_file() {
        // merge commit 的 `diff --combined <path>` 头必须开新文件段。
        let input = "diff --combined merge.py\n\
                     index abc..def..ghi 100644\n\
                     --- a/merge.py\n\
                     +++ b/merge.py\n\
                     @@@ -1,3 -1,3 +1,4 @@@\n\
                       ctx_a\n\
                     - removed_p1\n\
                      -removed_p2\n\
                     ++added_in_merge\n\
                       ctx_b\n";
        let cfg = DiffCompressorConfig {
            min_lines_for_ccr: 5,
            ..Default::default()
        };
        let r = DiffCompressor::new(cfg).compress(input, "");
        assert_eq!(r.files_affected, 1);
        assert!(r.compressed.contains("diff --combined merge.py"));
        assert!(r.compressed.contains("++added_in_merge"));
    }

    #[test]
    fn no_newline_marker_preserved_despite_distance() {
        // `\ No newline at end of file` 距变更行再远也必须保留。
        let input = "diff --git a/last.txt b/last.txt\n\
                     --- a/last.txt\n\
                     +++ b/last.txt\n\
                     @@ -1,8 +1,8 @@\n\
                     -old_first\n\
                     +new_first\n\
                      ctx_a\n\
                      ctx_b\n\
                      ctx_c\n\
                      ctx_d\n\
                      ctx_e\n\
                      ctx_f\n\
                     \\ No newline at end of file\n";
        let cfg = DiffCompressorConfig {
            min_lines_for_ccr: 5,
            min_compression_ratio_for_ccr: 0.1,
            ..Default::default()
        };
        let r = DiffCompressor::new(cfg).compress(input, "");
        assert!(
            r.compressed.contains("\\ No newline at end of file"),
            "no-newline 标记被裁剪:\n{}",
            r.compressed
        );
    }

    #[test]
    fn pre_diff_content_is_preserved() {
        // `git log -p` 的 commit 头不能被静默丢弃。
        let input = "commit abc1234567890\n\
                     Author: Tester <t@example.com>\n\
                     Date:   Mon Apr 25 12:00:00 2026\n\
                     \n    Refactor: rename and modify\n\n\
                     diff --git a/x.py b/x.py\n\
                     --- a/x.py\n\
                     +++ b/x.py\n\
                     @@ -1 +1 @@\n\
                     -a\n\
                     +b\n";
        let cfg = DiffCompressorConfig {
            min_lines_for_ccr: 5,
            ..Default::default()
        };
        let r = DiffCompressor::new(cfg).compress(input, "");
        assert!(r.compressed.starts_with("commit abc1234567890"));
        assert!(r.compressed.contains("Author: Tester"));
        assert!(r.compressed.contains("Refactor: rename and modify"));
        assert!(r.compressed.contains("diff --git a/x.py b/x.py"));
    }

    // ─── 相关性打分 ──────────────────────────────────────────────────

    #[test]
    fn cjk_bigrams_from_query_runs() {
        let b = cjk_bigrams("数据库连接");
        assert!(b.contains("数据") && b.contains("库连") && b.contains("连接"));
        assert_eq!(b.len(), 4);
        assert!(cjk_bigrams("hello world").is_empty());
        assert!(cjk_bigrams("a数b据").is_empty());
    }

    #[test]
    fn query_boosts_matching_hunk_into_kept_set() {
        // 两个中间 hunk 竞争一个名额：无 query 时高密度普通 hunk 胜出；
        // 中文 query 命中的 hunk 被加权后反超。
        let input = "diff --git a/svc.py b/svc.py\n\
                     --- a/svc.py\n\
                     +++ b/svc.py\n\
                     @@ -1,2 +1,2 @@\n\
                     -first_old\n\
                     +first_new\n\
                     @@ -10,4 +10,4 @@\n\
                     -plain_a\n\
                     +plain_b\n\
                     -plain_c\n\
                     +plain_d\n\
                     @@ -20,2 +20,2 @@\n\
                     -数据库连接失败重试\n\
                     +数据库连接成功\n\
                     @@ -30,2 +30,2 @@\n\
                     -last_old\n\
                     +last_new\n";
        let mk = || DiffCompressorConfig {
            max_hunks_per_file: 3,
            min_lines_for_ccr: 5,
            min_compression_ratio_for_ccr: 0.1,
            ..Default::default()
        };
        let with_cjk = DiffCompressor::new(mk()).compress(input, "数据库连接超时排查");
        let no_query = DiffCompressor::new(mk()).compress(input, "");
        assert!(
            with_cjk.compressed.contains("数据库连接失败重试"),
            "中文 query 应把命中 hunk 保进幸存集:\n{}",
            with_cjk.compressed
        );
        assert!(
            !no_query.compressed.contains("数据库连接失败重试"),
            "无 query 时高密度普通 hunk 应拿走中间名额:\n{}",
            no_query.compressed
        );
    }

    #[test]
    fn word_boundary_priority_matching() {
        assert!(contains_any_word("error happened", PRIORITY_WORD_GROUPS[0]));
        assert!(!contains_any_word("terrorist", PRIORITY_WORD_GROUPS[0]));
        assert!(contains_any_word("fix the bug", PRIORITY_WORD_GROUPS[1]));
        assert!(contains_any_word("auth token", PRIORITY_WORD_GROUPS[2]));
    }

    #[test]
    fn score_constants_match_reference_values() {
        // 钉住常量，调参时必须同步更新文档。
        assert_eq!(SCORE_CHANGE_DENSITY_WEIGHT, 0.03);
        assert_eq!(SCORE_CHANGE_DENSITY_CAP, 0.3);
        assert_eq!(SCORE_CONTEXT_WORD_WEIGHT, 0.2);
        assert_eq!(SCORE_CONTEXT_MIN_WORD_LEN, 2);
        assert_eq!(SCORE_PRIORITY_PATTERN_BOOST, 0.3);
        assert_eq!(SCORE_TOTAL_CAP, 1.0);
    }

    // ─── OffloadTransform 适配 ───────────────────────────────────────

    #[test]
    fn transform_metadata_and_bloat() {
        let t = DiffCompressorTransform::with_defaults();
        assert_eq!(t.name(), "diff_compressor");
        assert_eq!(t.applies_to(), ContentType::GitDiff);

        // 大量上下文行 → 高膨胀度。
        let mut diff = String::from("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,9 +1,9 @@\n");
        for _ in 0..8 {
            diff.push_str(" context line here\n");
        }
        diff.push_str("-old\n+new\n");
        assert!(t.estimate_bloat(&diff) > 0.7);
        // 非 diff → 0。
        assert_eq!(t.estimate_bloat("plain text\n".repeat(10).as_str()), 0.0);

        let key = t.cache_key(&diff);
        assert_eq!(key.len(), 24);
        assert_eq!(key, ccr::compute_key(&diff));
    }

    #[test]
    fn transform_apply_returns_compressed_and_original() {
        let t = DiffCompressorTransform::with_defaults();
        let input = build_synthetic_diff(8);
        let ctx = CompressionContext {
            query: Some("added".into()),
            token_budget: None,
        };
        let (compressed, original) = t.apply(&input, &ctx).unwrap();
        assert_eq!(original, input); // 原文完整返回供卸载
        assert!(compressed.len() < input.len());
        assert!(compressed.contains("[8 files changed, +40 -24 lines]"));

        // 短输入原样返回。
        let short = "diff --git a/x b/x\n@@ -1 +1 @@\n-a\n+b";
        let (c2, o2) = t.apply(short, &CompressionContext::default()).unwrap();
        assert_eq!(c2, short);
        assert_eq!(o2, short);
    }
}
