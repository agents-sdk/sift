//! 搜索结果压缩器：grep / ripgrep / ag 输出抽稀。

//!
//! 典型压缩比 5-10×。输入形如：
//!
//! ```text
//! src/utils.py:42:def process_data(items):
//! src/main.py-40-context before     <- ripgrep -C 的上下文行用 `-` 分隔
//! ```
//!
//! 压缩流水线：
//! 1. 解析为 `{file: [(line, content)]}`（FileMatches / SearchMatch）。
//! 2. 按相关性打分：query 词重叠（含 CJK bigram）+ 错误关键词加权 + 配置关键词。
//! 3. 文件按总分排序，截断到 `max_files`。
//! 4. 自适应总量（简化版 `compute_optimal_k`：按 bias 等比、下限 5、上限 max_total）。
//! 5. 每文件选择：保首/保尾（可配），剩余按分数填充，同 (行号, 内容) 去重，
//!    幸存者按行号恢复输出顺序。
//! 6. 输出 `file:line:content` + `[... and N more matches in file]` 汇总行。
//! 7. 达到阈值时追加 stash 取回标记（原文由调用方经 `OffloadTransform::apply`
//!    返回值卸载到 store）。
//!
//! trait 选择：本压缩器**有损**（丢弃匹配行），因此实现
//! [`crate::transforms::OffloadTransform`] 而非 `ReformatTransform`。
//!
//! 与参考实现的差异（受依赖约束）：
//! - 不用 `md5`，cache_key 用 blake3（`crate::stash::compute_key`）。
//! - 不依赖 `signals` 模块，重要性检测用内置关键词表（Error/Warning 两档，
//!   加权 0.5 / 0.4，与参考的类别映射一致）。
//! - 不依赖 `adaptive_sizer`，内置简化版 `compute_optimal_k`。
//! - 解析器与参考实现逐行等价（含 Windows 盘符、路径内 `-`、日期路径三档扫描）。

use std::collections::{BTreeMap, BTreeSet};

use crate::content::ContentType;
use crate::stash;
use crate::transforms::{CompressionContext, OffloadOutput, OffloadTransform, TransformError};

// ─── 类型 ────────────────────────────────────────────────────────────────

/// 单条搜索命中（grep 风格的一行）。
#[derive(Debug, Clone, PartialEq)]
pub struct SearchMatch {
    pub file: String,
    pub line_number: u64,
    pub content: String,
    /// 相关性得分 [0.0, 1.0]，由 [`SearchCompressor::score_matches`] 填充。
    pub score: f32,
    /// 当前搜索输出的 0-based 行号，与命中源文件的 line_number 无关。
    pub input_line: usize,
}

impl SearchMatch {
    pub fn new(file: impl Into<String>, line_number: u64, content: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            line_number,
            content: content.into(),
            score: 0.0,
            input_line: 0,
        }
    }
}

/// 同一文件下的全部命中。
#[derive(Debug, Clone, Default)]
pub struct FileMatches {
    pub file: String,
    pub matches: Vec<SearchMatch>,
}

impl FileMatches {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            matches: Vec::new(),
        }
    }

    pub fn first(&self) -> Option<&SearchMatch> {
        self.matches.first()
    }

    pub fn last(&self) -> Option<&SearchMatch> {
        self.matches.last()
    }

    pub fn total_score(&self) -> f32 {
        self.matches.iter().map(|m| m.score).sum()
    }
}

/// 压缩器配置。默认值与参考实现 `SearchCompressorConfig` 一致。
#[derive(Debug, Clone)]
pub struct SearchCompressorConfig {
    /// 每文件最多保留的匹配数。
    pub max_matches_per_file: usize,
    /// 是否总是保留每个文件的第一条匹配。
    pub always_keep_first: bool,
    /// 是否总是保留每个文件的最后一条匹配。
    pub always_keep_last: bool,
    /// 全局匹配总数上限（软上限，受 min_k=5 下限约束）。
    pub max_total_matches: usize,
    /// 最多保留的文件数（按总分截断）。
    pub max_files: usize,
    /// 额外的上下文关键词（命中 +0.4）。
    pub context_keywords: Vec<String>,
    /// 是否启用错误关键词加权。
    pub boost_errors: bool,
    /// 是否在输出追加 stash 取回标记。
    pub enable_stash: bool,
    /// 触发 stash 标记所需的最少原始匹配数。
    pub min_matches_for_stash: usize,
    /// stash 标记的压缩比阈值：压缩后/原始 ≥ 此值则不追加标记。
    pub min_compression_ratio_for_stash: f64,
    /// 按 `rg --heading` 风格分组输出：文件路径只出现一次。
    pub group_by_file: bool,
}

impl Default for SearchCompressorConfig {
    fn default() -> Self {
        Self {
            max_matches_per_file: 5,
            always_keep_first: true,
            always_keep_last: true,
            max_total_matches: 30,
            max_files: 15,
            context_keywords: Vec::new(),
            boost_errors: true,
            enable_stash: true,
            min_matches_for_stash: 10,
            min_compression_ratio_for_stash: 0.8,
            group_by_file: false,
        }
    }
}

/// 压缩结果。`compressed` 是格式化输出（可含 stash 标记）；
/// `summaries` 记录每个文件落进输出的 `[... and N more matches]` 行。
#[derive(Debug, Clone)]
pub struct SearchCompressionResult {
    pub compressed: String,
    pub original: String,
    pub original_match_count: usize,
    pub compressed_match_count: usize,
    pub files_affected: usize,
    pub compression_ratio: f64,
    pub cache_key: Option<String>,
    pub summaries: BTreeMap<String, String>,
}

impl SearchCompressionResult {
    /// 粗估节省 token（约 4 字符/token），与参考实现一致。
    pub fn tokens_saved_estimate(&self) -> i64 {
        let chars_saved = self.original.len() as i64 - self.compressed.len() as i64;
        chars_saved.max(0) / 4
    }

    pub fn matches_omitted(&self) -> usize {
        self.original_match_count
            .saturating_sub(self.compressed_match_count)
    }
}

/// 旁路诊断统计（不在压缩输出里，供观测用）。
#[derive(Debug, Clone, Default)]
pub struct SearchCompressorStats {
    pub lines_scanned: usize,
    pub lines_unparsed: usize,
    pub files_dropped: usize,
    pub matches_dropped_by_per_file_cap: usize,
    pub matches_dropped_by_global_cap: usize,
    pub stash_emitted: bool,
    pub stash_skip_reason: Option<&'static str>,
}

// ─── 压缩器 ──────────────────────────────────────────────────────────────

/// 搜索结果压缩器本体：解析 → 打分 → 选择 → 格式化。
pub struct SearchCompressor {
    config: SearchCompressorConfig,
}

impl SearchCompressor {
    pub fn new(config: SearchCompressorConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &SearchCompressorConfig {
        &self.config
    }

    /// 压缩入口。`context` 为用户 query（相关性打分用），`bias` ∈ (0,1]
    /// 控制自适应保留比例（1.0 = 尽量贴近上限）。
    pub fn compress(
        &self,
        content: &str,
        context: &str,
        bias: f64,
    ) -> (SearchCompressionResult, SearchCompressorStats) {
        let mut stats = SearchCompressorStats::default();
        let parsed = self.parse_search_results(content, &mut stats);

        if parsed.is_empty() {
            return (
                SearchCompressionResult {
                    compressed: content.to_string(),
                    original: content.to_string(),
                    original_match_count: 0,
                    compressed_match_count: 0,
                    files_affected: 0,
                    compression_ratio: 1.0,
                    cache_key: None,
                    summaries: BTreeMap::new(),
                },
                stats,
            );
        }

        let original_count: usize = parsed.values().map(|fm| fm.matches.len()).sum();

        let mut scored = parsed;
        self.score_matches(&mut scored, context);

        let selected = self.select_matches(&scored, bias, &mut stats);

        let (compressed_body, summaries) = self.format_output(&selected, &scored);
        let compressed_count: usize = selected.values().map(|fm| fm.matches.len()).sum();
        let ratio = compressed_body.len() as f64 / content.len().max(1) as f64;

        let mut compressed = compressed_body;
        let mut cache_key = None;
        if self.config.enable_stash {
            if original_count < self.config.min_matches_for_stash {
                stats.stash_skip_reason = Some("below min_matches_for_stash");
            } else if ratio >= self.config.min_compression_ratio_for_stash {
                stats.stash_skip_reason = Some("compression ratio too high");
            } else {
                let key = stash::compute_key(content);
                let marker = format!(
                    "\n[{} matches compressed to {}. Retrieve more: hash={}]",
                    original_count, compressed_count, key
                );
                compressed.push_str(&marker);
                cache_key = Some(key);
                stats.stash_emitted = true;
            }
        } else {
            stats.stash_skip_reason = Some("stash disabled in config");
        }

        let result = SearchCompressionResult {
            compressed,
            original: content.to_string(),
            original_match_count: original_count,
            compressed_match_count: compressed_count,
            files_affected: scored.len(),
            compression_ratio: ratio,
            cache_key,
            summaries,
        };
        (result, stats)
    }

    // ─── 各阶段（测试与复用可直接调用）────────────────────────────────

    /// 解析为按文件分组的匹配表。
    pub fn parse_search_results(
        &self,
        content: &str,
        stats: &mut SearchCompressorStats,
    ) -> BTreeMap<String, FileMatches> {
        let mut out: BTreeMap<String, FileMatches> = BTreeMap::new();
        for (input_line, raw) in content.split('\n').enumerate() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            stats.lines_scanned += 1;
            match parse_match_line(line) {
                Some((file, line_no, body)) => {
                    let mut entry = SearchMatch::new(file, line_no, body);
                    entry.input_line = input_line;
                    out.entry(file.to_string())
                        .or_insert_with(|| FileMatches::new(file))
                        .matches
                        .push(entry);
                }
                None => stats.lines_unparsed += 1,
            }
        }
        out
    }

    /// 相关性打分：query 词重叠（+0.3/词，含 CJK bigram）+
    /// 错误关键词加权（Error +0.5 / Warning +0.4）+ 配置关键词（+0.4），
    /// 总分截断到 1.0。
    pub fn score_matches(&self, files: &mut BTreeMap<String, FileMatches>, context: &str) {
        let context_lower = context.to_lowercase();
        // 去重后只保留长度 > 2（按字符数，对齐 Python 码点语义）的词；
        // 另加 CJK bigram，使无空格的中文 query 也能命中内容。
        let mut context_words: BTreeSet<String> = context_lower
            .split_whitespace()
            .filter(|w| w.chars().count() > 2)
            .map(|w| w.to_string())
            .collect();
        context_words.extend(cjk_bigrams(&context_lower));

        for fm in files.values_mut() {
            for m in &mut fm.matches {
                let mut score: f32 = 0.0;
                let content_lower = m.content.to_lowercase();

                for w in &context_words {
                    if content_lower.contains(w.as_str()) {
                        score += 0.3;
                    }
                }

                if self.config.boost_errors {
                    // 内置关键词检测，等价于参考实现的 signals 类别加权。
                    if contains_any_word(&content_lower, ERROR_KEYWORDS) {
                        score += 0.5;
                    } else if contains_any_word(&content_lower, WARNING_KEYWORDS) {
                        score += 0.4;
                    }
                }

                for kw in &self.config.context_keywords {
                    if content_lower.contains(&kw.to_lowercase()) {
                        score += 0.4;
                    }
                }

                m.score = score.min(1.0);
            }
        }
    }

    /// 文件截断 + 自适应总量 + 每文件选择（保首尾、按分填充、去重、恢复行序）。
    pub fn select_matches(
        &self,
        files: &BTreeMap<String, FileMatches>,
        bias: f64,
        stats: &mut SearchCompressorStats,
    ) -> BTreeMap<String, FileMatches> {
        // 按文件总分降序（BTreeMap 按键序遍历，须显式排序）。
        let mut by_score: Vec<(&String, &FileMatches)> = files.iter().collect();
        by_score.sort_by(|a, b| {
            b.1.total_score()
                .partial_cmp(&a.1.total_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if by_score.len() > self.config.max_files {
            stats.files_dropped += by_score.len() - self.config.max_files;
            by_score.truncate(self.config.max_files);
        }

        let all_match_strings: Vec<String> = by_score
            .iter()
            .flat_map(|(file, fm)| {
                fm.matches
                    .iter()
                    .map(move |m| format!("{}:{}:{}", file, m.line_number, m.content))
            })
            .collect();
        let all_refs: Vec<&str> = all_match_strings.iter().map(|s| s.as_str()).collect();
        let adaptive_total =
            compute_optimal_k(&all_refs, bias, 5, Some(self.config.max_total_matches));

        let mut selected: BTreeMap<String, FileMatches> = BTreeMap::new();
        let mut total_selected: usize = 0;

        for (file, fm) in by_score {
            if total_selected >= adaptive_total {
                stats.matches_dropped_by_global_cap += fm.matches.len();
                continue;
            }

            // 分数降序；同分按行号升序保证确定性。
            let mut sorted = fm.matches.clone();
            sorted.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.line_number.cmp(&b.line_number))
            });

            let mut file_selected: Vec<SearchMatch> = Vec::new();
            // BTreeSet 使"已选"检查为 O(log n)（参考实现的 Python 是 O(n²)）。
            let mut seen: BTreeSet<(u64, u64)> = BTreeSet::new();

            let remaining_cap = self
                .config
                .max_matches_per_file
                .min(adaptive_total.saturating_sub(total_selected));

            let push_unique = |m: &SearchMatch,
                               file_selected: &mut Vec<SearchMatch>,
                               seen: &mut BTreeSet<(u64, u64)>| {
                let key = (m.line_number, hash_u64(&m.content));
                if seen.insert(key) {
                    file_selected.push(m.clone());
                    true
                } else {
                    false
                }
            };

            if self.config.always_keep_first {
                if let Some(first) = fm.first() {
                    if file_selected.len() < remaining_cap {
                        push_unique(first, &mut file_selected, &mut seen);
                    }
                }
            }

            if self.config.always_keep_last && fm.matches.len() > 1 {
                if let Some(last) = fm.last() {
                    if file_selected.len() < remaining_cap {
                        push_unique(last, &mut file_selected, &mut seen);
                    }
                }
            }

            for m in &sorted {
                if file_selected.len() >= remaining_cap {
                    break;
                }
                push_unique(m, &mut file_selected, &mut seen);
            }

            // 恢复行号顺序输出。
            file_selected.sort_by_key(|m| m.line_number);

            let dropped_here = fm.matches.len().saturating_sub(file_selected.len());
            stats.matches_dropped_by_per_file_cap += dropped_here;

            total_selected += file_selected.len();
            selected.insert(
                file.clone(),
                FileMatches {
                    file: file.clone(),
                    matches: file_selected,
                },
            );
        }

        selected
    }

    /// 格式化输出：经典 `file:line:content` 或 `rg --heading` 分组风格，
    /// 并为被抽稀的文件追加 `[... and N more matches ...]` 汇总。
    pub fn format_output(
        &self,
        selected: &BTreeMap<String, FileMatches>,
        original: &BTreeMap<String, FileMatches>,
    ) -> (String, BTreeMap<String, String>) {
        let mut lines: Vec<String> = Vec::new();
        let mut summaries: BTreeMap<String, String> = BTreeMap::new();
        let grouped = self.config.group_by_file;

        for (file, fm) in selected {
            if grouped {
                if !lines.is_empty() {
                    lines.push(String::new());
                }
                lines.push(file.clone());
                for m in &fm.matches {
                    lines.push(format!("{}:{}", m.line_number, m.content));
                }
            } else {
                for m in &fm.matches {
                    lines.push(format!("{}:{}:{}", m.file, m.line_number, m.content));
                }
            }
            if let Some(orig_fm) = original.get(file) {
                if orig_fm.matches.len() > fm.matches.len() {
                    let omitted = orig_fm.matches.len() - fm.matches.len();
                    let summary = if grouped {
                        format!("[... and {} more matches]", omitted)
                    } else {
                        format!("[... and {} more matches in {}]", omitted, file)
                    };
                    lines.push(summary.clone());
                    summaries.insert(file.clone(), summary);
                }
            }
        }

        (lines.join("\n"), summaries)
    }
}

// ─── 内置重要性关键词（替代参考实现的 signals 模块）────────────────────

const ERROR_KEYWORDS: &[&str] = &[
    "error",
    "exception",
    "fatal",
    "critical",
    "crash",
    "panic",
    "failed",
    "failure",
];

const WARNING_KEYWORDS: &[&str] = &["warning", "warn", "deprecated", "caution"];

/// 大小写不敏感的整词包含检测（近似 `\b(word)s?\b`：允许复数后缀，
/// 词边界取非字母数字）。
fn contains_any_word(lower_text: &str, words: &[&str]) -> bool {
    let b = lower_text.as_bytes();
    for w in words {
        let wb = w.as_bytes();
        let mut start = 0;
        while let Some(pos) = lower_text[start..].find(w) {
            let s = start + pos;
            let mut e = s + wb.len();
            // 允许复数后缀：`errors` 命中 `error`。
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

// ─── 自适应总量（简化版 adaptive_sizer::compute_optimal_k）──────────────

/// 按 `bias` 等比给出保留数，下限 `min_k`（对齐参考实现的硬下限 5），
/// 上限 `max_k`。条目不足时退化为条目数。
fn compute_optimal_k(items: &[&str], bias: f64, min_k: usize, max_k: Option<usize>) -> usize {
    let n = items.len();
    if n == 0 {
        return 0;
    }
    let bias = bias.clamp(0.0, 1.0);
    let proportional = (n as f64 * bias).round() as usize;
    let mut k = proportional.max(min_k).min(n);
    if let Some(max) = max_k {
        k = k.min(max);
    }
    k.max(1).min(n)
}

// ─── CJK bigram（对齐参考实现，使中文 query 可命中）────────────────────

/// CJK 字符（假名、汉字、谚文）判定，码点范围与参考实现一致。
fn is_cjk_char(c: char) -> bool {
    matches!(
        c as u32,
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF
    )
}

/// 取小写 query 中 CJK 连续段的二元组，使无空格中文 query 可部分命中。
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

// ─── 解析器（与参考实现逐行等价的三档扫描）──────────────────────────────

/// 把一行 grep/ripgrep 输出解析为 `(file, line_number, content)`。
///
/// 策略：
/// 1. 若行首是 Windows 盘符（`C:\` 或 `C:/`），行号扫描从盘符冒号之后开始。
/// 2. 找最左的 `<sep><digits><sep>` 三元组（sep 为 `:` 或 `-`）。
/// 3. 分三档扫描（详见 [`ScanTier`]），修复 Python 正则的两类误判：
///    Windows 路径被盘符冒号截断、路径内含 `-`（日期目录、CVE 编号等）
///    被误读为行号标记。
fn parse_match_line(line: &str) -> Option<(&str, u64, &str)> {
    scan_match_line(line, ScanTier::Colon)
        .or_else(|| scan_match_line(line, ScanTier::Dash))
        .or_else(|| scan_match_line(line, ScanTier::Permissive))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanTier {
    /// 左起的 `:` 三元组（grep 匹配行分隔符，路径几乎不含 `:`）。
    Colon,
    /// `-` 三元组（ripgrep 上下文行），需正向证据判断路径是否继续。
    Dash,
    /// 兜底：任意 sep 的最左三元组。
    Permissive,
}

/// token 中是否存在形似文件扩展名的点：后跟 1-8 位字母数字（至少一个字母）。
/// 字母要求把 `v1.2.3` 这类点分版本号排除在外。
fn has_extension_dot(tok: &str) -> bool {
    let b = tok.as_bytes();
    b.iter().enumerate().any(|(i, &c)| {
        if c != b'.' || i + 1 == b.len() {
            return false;
        }
        let tail = &b[i + 1..];
        let end = tail
            .iter()
            .position(|c| !c.is_ascii_alphanumeric())
            .unwrap_or(tail.len());
        let ext = &tail[..end];
        (1..=8).contains(&ext.len()) && ext.iter().any(|c| c.is_ascii_alphabetic())
    })
}

/// 路径最后一段是否带扩展名。Dash 档据此区分"数字还在路径里"
/// （`logs/2026` 无扩展名 → 继续扫）与"数字是行号标记"
/// （`logs/2026-05-03/app.log` → 停）。
fn last_segment_has_extension(path: &str) -> bool {
    has_extension_dot(path.rsplit(['/', '\\']).next().unwrap_or(path))
}

/// 标记之后的 token 是否仍带路径结构（`/` 或扩展名点）——
/// 即"数字仍在路径中"的正向证据。
fn path_continues(rest: &str) -> bool {
    rest.split_whitespace()
        .next()
        .is_some_and(|tok| tok.contains('/') || tok.contains('\\') || has_extension_dot(tok))
}

fn scan_match_line(line: &str, tier: ScanTier) -> Option<(&str, u64, &str)> {
    let bytes = line.as_bytes();
    // Windows 盘符前缀：[A-Za-z]:[\\/] —— 跳过盘符冒号避免被误认为标记分隔符。
    let scan_start = if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        2
    } else {
        0
    };

    // 候选三元组 (path_end, digits_start, digits_end)。
    // `first` 是最左标记（兜底答案），`chosen` 是被正向确认的边界。
    let mut first: Option<(usize, usize, usize)> = None;
    let mut chosen: Option<(usize, usize, usize)> = None;

    let mut i = scan_start;
    while i < bytes.len() {
        let tier_sep = match tier {
            ScanTier::Colon => bytes[i] == b':',
            ScanTier::Dash => bytes[i] == b'-',
            ScanTier::Permissive => bytes[i] == b':' || bytes[i] == b'-',
        };
        if tier_sep {
            // 相邻分隔符（`::` / `:-`）折叠：负行号等形态按不可解析拒绝。
            if i > 0 && (bytes[i - 1] == b':' || bytes[i - 1] == b'-') {
                i += 1;
                continue;
            }
            // 前两档只考虑首个无空白片段内的标记——grep 路径不含空白，正文常含。
            if tier != ScanTier::Permissive && bytes[..i].iter().any(|b| b.is_ascii_whitespace()) {
                break;
            }
            let digits_start = i + 1;
            let mut j = digits_start;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            // 闭合分隔符必须与开启的一致：grep 只发 `file:12:body` / `file-12-body`。
            let closes = j > digits_start
                && j < bytes.len()
                && match tier {
                    ScanTier::Permissive => bytes[j] == b':' || bytes[j] == b'-',
                    _ => bytes[j] == bytes[i],
                };
            if closes {
                // 拒绝空路径（行首即分隔符）。
                if i == 0 {
                    return None;
                }
                if first.is_none() {
                    first = Some((i, digits_start, j));
                }
                if tier != ScanTier::Dash {
                    chosen = Some((i, digits_start, j));
                    break;
                }
                // Dash 档：仅在正向证据下越过该标记——
                // (a) 路径末段已有扩展名 → 此标记即边界；
                // (b) 标记后 token 无进一步路径结构 → 停在此处；
                // 否则数字仍在路径中，继续向右扫。
                if last_segment_has_extension(&line[..i]) || !path_continues(&line[j + 1..]) {
                    chosen = Some((i, digits_start, j));
                    break;
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }

    // 走完整段都无法确认边界：本行本质歧义，回退最左标记。
    let (path_end, digits_start, digits_end) = chosen.or(first)?;
    let line_no = std::str::from_utf8(&bytes[digits_start..digits_end])
        .ok()
        .and_then(|s| s.parse::<u64>().ok())?;
    Some((&line[..path_end], line_no, &line[digits_end + 1..]))
}

// ─── 内部工具 ────────────────────────────────────────────────────────────

fn hash_u64(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

// ─── OffloadTransform 适配 ───────────────────────────────────────────────

/// `OffloadTransform` 适配器：路由层按 `ContentType::SearchResults` 分发。
/// apply 返回结构化卸载结果，由调用方把原文写入 stash store。
pub struct SearchCompressorTransform {
    compressor: SearchCompressor,
}

impl SearchCompressorTransform {
    pub fn new(config: SearchCompressorConfig) -> Self {
        Self {
            compressor: SearchCompressor::new(config),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(SearchCompressorConfig::default())
    }

    pub fn compressor(&self) -> &SearchCompressor {
        &self.compressor
    }
}

impl OffloadTransform for SearchCompressorTransform {
    fn name(&self) -> &'static str {
        "search_compressor"
    }

    fn applies_to(&self) -> ContentType {
        ContentType::SearchResults
    }

    /// 膨胀度 = 可解析匹配行占比（无匹配则 0，不值得压缩）。
    fn estimate_bloat(&self, input: &str) -> f64 {
        let mut stats = SearchCompressorStats::default();
        let parsed = self.compressor.parse_search_results(input, &mut stats);
        if stats.lines_scanned == 0 {
            return 0.0;
        }
        let matches: usize = parsed.values().map(|fm| fm.matches.len()).sum();
        matches as f64 / stats.lines_scanned as f64
    }

    fn cache_key(&self, input: &str) -> String {
        stash::compute_key(input)
    }

    fn apply(
        &self,
        input: &str,
        ctx: &CompressionContext,
    ) -> Result<OffloadOutput, TransformError> {
        let context = ctx.query.as_deref().unwrap_or("");
        if let Some(path) =
            super::line_omissions::actionable_file_path(ctx.stash_file_path.as_deref())
        {
            let mut stats = SearchCompressorStats::default();
            let mut parsed = self.compressor.parse_search_results(input, &mut stats);
            if parsed.is_empty() {
                return Err(TransformError::Skipped);
            }
            self.compressor.score_matches(&mut parsed, context);
            let selected = self.compressor.select_matches(&parsed, 1.0, &mut stats);
            let mut kept: BTreeSet<usize> = selected
                .values()
                .flat_map(|f| f.matches.iter().map(|m| m.input_line))
                .collect();
            // 命令回显、标题等未解析行原样保留；不把它们误当作搜索命中删除。
            for (i, line) in input.lines().enumerate() {
                if parse_match_line(line.trim()).is_none() {
                    kept.insert(i);
                }
            }
            return Ok(super::line_omissions::render(
                input,
                kept,
                path,
                ctx.stash_line_offset,
            ));
        }
        let (result, _) = self.compressor.compress(input, context, 1.0);
        Ok(OffloadOutput::new(result.compressed, result.original))
    }
}

// ─── 测试 ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_line(line: &str) -> Option<(String, u64, String)> {
        parse_match_line(line).map(|(f, n, c)| (f.to_string(), n, c.to_string()))
    }

    // ─── 解析器 ──────────────────────────────────────────────────────

    #[test]
    fn parses_standard_grep_line() {
        assert_eq!(
            parse_line("src/main.py:42:def main():"),
            Some(("src/main.py".into(), 42, "def main():".into()))
        );
    }

    #[test]
    fn parses_ripgrep_context_line() {
        assert_eq!(
            parse_line("src/main.py-43-context after match"),
            Some(("src/main.py".into(), 43, "context after match".into()))
        );
    }

    #[test]
    fn handles_windows_path_with_backslash() {
        assert_eq!(
            parse_line(r"C:\Users\foo\bar.py:42:def main():"),
            Some((r"C:\Users\foo\bar.py".into(), 42, "def main():".into()))
        );
    }

    #[test]
    fn handles_windows_path_with_forward_slash() {
        assert_eq!(
            parse_line("C:/Users/foo/bar.py:42:def main():"),
            Some(("C:/Users/foo/bar.py".into(), 42, "def main():".into()))
        );
    }

    #[test]
    fn handles_dashes_in_filename_with_ripgrep_context() {
        assert_eq!(
            parse_line("pre-commit-config.yaml-42-fail_fast: true"),
            Some((
                "pre-commit-config.yaml".into(),
                42,
                "fail_fast: true".into()
            ))
        );
    }

    #[test]
    fn date_stamped_path_is_not_misread_as_line_number_marker() {
        // `logs/2026-05-03/...` 的路径内部含 `-05-` 三元组，最左优先会静默
        // 产出不存在的文件与错误行号。
        assert_eq!(
            parse_line("logs/2026-05-03/app.log:12:ERROR boom"),
            Some(("logs/2026-05-03/app.log".into(), 12, "ERROR boom".into()))
        );
        assert_eq!(
            parse_line("advisories/CVE-2021-44228.md:8:Log4Shell"),
            Some(("advisories/CVE-2021-44228.md".into(), 8, "Log4Shell".into()))
        );
    }

    #[test]
    fn dash_tier_does_not_run_past_the_path_into_the_body() {
        assert_eq!(
            parse_line("notes.md-3-see-4-here"),
            Some(("notes.md".into(), 3, "see-4-here".into()))
        );
    }

    #[test]
    fn body_line_reference_does_not_hijack_a_context_line() {
        // `-` 分隔的上下文行，正文里引用了 `file:line:`——必须绑定到
        // 上下文标记而非引用。
        assert_eq!(
            parse_line("src/main.py-44-see foo.rs:12:bar"),
            Some(("src/main.py".into(), 44, "see foo.rs:12:bar".into()))
        );
    }

    #[test]
    fn rejects_lines_without_line_number_marker() {
        assert!(parse_line("just a normal line of prose").is_none());
        assert!(parse_line("file.py:not-a-number:something").is_none());
        assert!(parse_line(":42:something").is_none());
    }

    #[test]
    fn rejects_negative_line_numbers() {
        assert!(parse_line("src/file.py:-1:invalid").is_none());
        assert!(parse_line("src/file.py--1-invalid").is_none());
    }

    // ─── 分组与统计 ──────────────────────────────────────────────────

    #[test]
    fn parser_groups_by_file_and_counts() {
        let compressor = SearchCompressor::new(SearchCompressorConfig::default());
        let content = "\
src/main.py:42:def main():
src/main.py:43:    pass
src/utils.py:15:def util():
just prose, no marker
src/main.py-44-context line";
        let mut stats = SearchCompressorStats::default();
        let parsed = compressor.parse_search_results(content, &mut stats);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed["src/main.py"].matches.len(), 3);
        assert_eq!(parsed["src/utils.py"].matches.len(), 1);
        assert_eq!(stats.lines_unparsed, 1);
        assert_eq!(stats.lines_scanned, 5);
    }

    // ─── 打分 ────────────────────────────────────────────────────────

    #[test]
    fn scoring_boosts_error_lines_in_search_context() {
        let compressor = SearchCompressor::new(SearchCompressorConfig {
            context_keywords: vec!["auth".into()],
            ..Default::default()
        });
        let mut files = BTreeMap::new();
        let mut fm = FileMatches::new("src/auth.py");
        fm.matches
            .push(SearchMatch::new("src/auth.py", 10, "ERROR auth failed"));
        fm.matches
            .push(SearchMatch::new("src/auth.py", 11, "plain auth line"));
        files.insert("src/auth.py".into(), fm);

        compressor.score_matches(&mut files, "find auth error");
        let scored = &files["src/auth.py"].matches;
        // ERROR 关键词 + auth 配置关键词 + query 词 error/auth 全命中，截断 1.0。
        assert_eq!(scored[0].score, 1.0);
        // 普通行只有 query 词 + 关键词加权（无 error）。
        assert!(scored[1].score > 0.0 && scored[1].score < 1.0);
    }

    #[test]
    fn cjk_bigrams_from_runs() {
        let b = cjk_bigrams("认证令牌");
        assert!(b.contains("认证") && b.contains("证令") && b.contains("令牌") && b.len() == 3);
        assert!(cjk_bigrams("hello").is_empty());
        assert!(cjk_bigrams("a认b证").is_empty()); // 孤立汉字不成对
    }

    #[test]
    fn cjk_query_words_can_score_matches() {
        let compressor = SearchCompressor::new(SearchCompressorConfig::default());
        let mut files = BTreeMap::new();
        let mut fm = FileMatches::new("a.py");
        fm.matches
            .push(SearchMatch::new("a.py", 1, "数据库连接失败"));
        files.insert("a.py".into(), fm);
        compressor.score_matches(&mut files, "数据库连接");
        assert!(files["a.py"].matches[0].score > 0.0);
    }

    #[test]
    fn word_boundary_matching_avoids_substring_false_positive() {
        // "errors" 含 "error" 整词成立；但 "terrorist" 中 "error" 不是整词。
        assert!(contains_any_word("errors happened", ERROR_KEYWORDS));
        assert!(!contains_any_word("terrorist", ERROR_KEYWORDS));
        assert!(contains_any_word("panic in worker", ERROR_KEYWORDS));
        assert!(contains_any_word("deprecation warning", WARNING_KEYWORDS));
    }

    // ─── 选择：抽稀关键行为 ──────────────────────────────────────────

    #[test]
    fn select_respects_per_file_cap_and_global_cap() {
        // min_k=5 硬下限使 max_total_matches 是软上限，配 6 才能触到上限路径。
        let compressor = SearchCompressor::new(SearchCompressorConfig {
            max_matches_per_file: 2,
            max_total_matches: 6,
            max_files: 2,
            always_keep_first: true,
            always_keep_last: true,
            ..Default::default()
        });
        let mut files = BTreeMap::new();
        for (file, n) in [("a.py", 5), ("b.py", 4), ("c.py", 3)] {
            let mut fm = FileMatches::new(file);
            for i in 0..n {
                fm.matches
                    .push(SearchMatch::new(file, i + 1, format!("line {}", i + 1)));
            }
            files.insert(file.into(), fm);
        }

        let mut stats = SearchCompressorStats::default();
        let selected = compressor.select_matches(&files, 1.0, &mut stats);

        // max_files=2 截断，三文件丢一。
        assert_eq!(selected.len(), 2);
        assert!(stats.files_dropped >= 1);
        // 幸存文件每文件 ≤ 2 条，且按行号升序输出。
        for fm in selected.values() {
            assert!(fm.matches.len() <= 2);
            assert!(fm
                .matches
                .windows(2)
                .all(|w| w[0].line_number < w[1].line_number));
        }
    }

    #[test]
    fn always_keep_first_and_last_survive_per_file_cap() {
        let compressor = SearchCompressor::new(SearchCompressorConfig {
            max_matches_per_file: 2,
            max_total_matches: 10,
            ..Default::default()
        });
        let mut files = BTreeMap::new();
        let mut fm = FileMatches::new("a.py");
        for i in 1..=8 {
            fm.matches
                .push(SearchMatch::new("a.py", i, format!("l{i}")));
        }
        files.insert("a.py".into(), fm);

        let mut stats = SearchCompressorStats::default();
        let selected = compressor.select_matches(&files, 1.0, &mut stats);
        let ms = &selected["a.py"].matches;
        assert_eq!(ms.len(), 2);
        assert_eq!(ms.first().unwrap().line_number, 1); // 保首
        assert_eq!(ms.last().unwrap().line_number, 8); // 保尾
    }

    #[test]
    fn duplicate_matches_are_folded() {
        // 同一行被 grep 重复输出（或 -C 上下文重叠）时按 (行号, 内容) 去重。
        let compressor = SearchCompressor::new(SearchCompressorConfig {
            max_matches_per_file: 5,
            max_total_matches: 10,
            ..Default::default()
        });
        let content = "a.py:3:dup line\na.py:3:dup line\na.py:4:other";
        let mut stats = SearchCompressorStats::default();
        let files = compressor.parse_search_results(content, &mut stats);
        let mut sel_stats = SearchCompressorStats::default();
        let selected = compressor.select_matches(&files, 1.0, &mut sel_stats);
        assert_eq!(selected["a.py"].matches.len(), 2);
        assert_eq!(selected["a.py"].matches[0].line_number, 3);
        assert_eq!(selected["a.py"].matches[1].line_number, 4);
    }

    #[test]
    fn compute_optimal_k_respects_floor_and_cap() {
        let items: Vec<&str> = vec!["a"; 100];
        assert_eq!(compute_optimal_k(&items, 1.0, 5, Some(30)), 30); // 上限生效
        assert_eq!(compute_optimal_k(&items, 0.01, 5, Some(30)), 5); // 下限生效
        let small: Vec<&str> = vec!["a"; 3];
        assert_eq!(compute_optimal_k(&small, 1.0, 5, Some(30)), 3); // 条目不足退化为 n
        assert_eq!(compute_optimal_k(&[], 1.0, 5, None), 0);
    }

    // ─── 端到端 ──────────────────────────────────────────────────────

    #[test]
    fn multi_file_results_are_thinned_with_summaries() {
        let compressor = SearchCompressor::new(SearchCompressorConfig {
            max_matches_per_file: 2,
            max_total_matches: 10,
            min_matches_for_stash: 100, // 关掉标记便于断言正文
            ..Default::default()
        });
        let mut content = String::new();
        for i in 1..=6 {
            content.push_str(&format!("src/a.py:{}:alpha line {i}\n", i * 10));
        }
        for i in 1..=4 {
            content.push_str(&format!("src/b.py:{}:beta line {i}\n", i * 10));
        }
        let (result, stats) = compressor.compress(&content, "alpha beta", 1.0);

        assert_eq!(result.original_match_count, 10);
        // 每文件被抽到 2 条 + 各一条汇总行。
        assert!(result
            .compressed
            .contains("[... and 4 more matches in src/a.py]"));
        assert!(result
            .compressed
            .contains("[... and 2 more matches in src/b.py]"));
        assert_eq!(result.summaries.len(), 2);
        assert!(result.compressed.len() < result.original.len());
        assert!(result.compression_ratio < 1.0);
        assert!(stats.matches_dropped_by_per_file_cap >= 6);
        assert!(result.matches_omitted() >= 6);
        assert!(result.tokens_saved_estimate() > 0);
    }

    #[test]
    fn group_by_file_output_uses_heading_style() {
        let compressor = SearchCompressor::new(SearchCompressorConfig {
            group_by_file: true,
            max_matches_per_file: 1,
            max_total_matches: 10,
            min_matches_for_stash: 100,
            ..Default::default()
        });
        let content = "a.py:1:one\na.py:2:two\nb.py:1:first";
        let (result, _) = compressor.compress(content, "", 1.0);
        // 路径只作为标题行出现一次，正文行是 `line:content`。
        assert!(result.compressed.starts_with("a.py\n1:one\n"));
        assert!(!result.compressed.contains("a.py:1:"));
        assert!(result.compressed.contains("[... and 1 more matches]"));
    }

    #[test]
    fn empty_input_returns_unchanged() {
        let compressor = SearchCompressor::new(SearchCompressorConfig::default());
        let (result, _) = compressor.compress("plain text only", "", 1.0);
        assert_eq!(result.original_match_count, 0);
        assert_eq!(result.compressed, "plain text only");
        assert_eq!(result.compression_ratio, 1.0);
    }

    #[test]
    fn stash_reference_emitted_when_thresholds_clear() {
        let compressor = SearchCompressor::new(SearchCompressorConfig {
            max_matches_per_file: 2,
            max_total_matches: 4,
            min_matches_for_stash: 5,
            min_compression_ratio_for_stash: 0.95, // 测试用宽松阈值
            ..Default::default()
        });
        let mut content = String::new();
        for i in 1..=12 {
            content.push_str(&format!("src/main.py:{}:line content {i}\n", i));
        }
        let (result, stats) = compressor.compress(&content, "", 1.0);
        assert!(result.cache_key.is_some());
        assert!(stats.stash_emitted);
        assert!(result.compressed.contains("[12 matches compressed to"));
        // 折叠标记内只用纯哈希引用，不内嵌 `<<stash:KEY>>`（后者由框架在末尾追加一次）。
        assert!(result
            .compressed
            .contains(&format!("hash={}", result.cache_key.as_ref().unwrap())));
        assert!(!result.compressed.contains("<<stash:"));
    }

    #[test]
    fn stash_skipped_when_below_min_matches() {
        let compressor = SearchCompressor::new(SearchCompressorConfig {
            min_matches_for_stash: 100,
            ..Default::default()
        });
        let content = "src/main.py:1:hi\nsrc/main.py:2:bye\n";
        let (result, stats) = compressor.compress(content, "", 1.0);
        assert!(result.cache_key.is_none());
        assert_eq!(stats.stash_skip_reason, Some("below min_matches_for_stash"));
    }

    #[test]
    fn stash_skipped_when_disabled() {
        let compressor = SearchCompressor::new(SearchCompressorConfig {
            enable_stash: false,
            ..Default::default()
        });
        let mut content = String::new();
        for i in 1..=20 {
            content.push_str(&format!("src/main.py:{}:line\n", i));
        }
        let (result, stats) = compressor.compress(&content, "", 1.0);
        assert!(result.cache_key.is_none());
        assert_eq!(stats.stash_skip_reason, Some("stash disabled in config"));
    }

    // ─── OffloadTransform 适配 ───────────────────────────────────────

    #[test]
    fn transform_metadata_and_bloat() {
        let t = SearchCompressorTransform::with_defaults();
        assert_eq!(t.name(), "search_compressor");
        assert_eq!(t.applies_to(), ContentType::SearchResults);

        let good = "a.py:1:x\na.py:2:y\nb.py:3:z";
        assert!(t.estimate_bloat(good) > 0.9);
        assert_eq!(t.estimate_bloat("just prose\nmore prose"), 0.0);

        let key = t.cache_key(good);
        assert_eq!(key.len(), 24);
        assert_eq!(key, stash::compute_key(good));
    }

    #[test]
    fn transform_apply_returns_compressed_and_original() {
        let t = SearchCompressorTransform::new(SearchCompressorConfig {
            max_matches_per_file: 2,
            max_total_matches: 6,
            min_matches_for_stash: 100,
            ..Default::default()
        });
        let mut content = String::new();
        for i in 1..=10 {
            content.push_str(&format!("src/m.py:{}:content {i}\n", i));
        }
        let ctx = CompressionContext {
            query: Some("content".into()),
            token_budget: None,
            source_path: None,
            stash_file_path: None,
            stash_line_offset: 0,
        };
        let result = t.apply(&content, &ctx).unwrap();
        assert_eq!(result.original, content); // 原文完整返回供卸载
        assert!(result.compressed.len() < content.len());
        assert!(result
            .compressed
            .contains("[... and 8 more matches in src/m.py]"));

        // 空 query 也能工作。
        let result = t.apply(&content, &CompressionContext::default()).unwrap();
        assert!(result.compressed.len() < content.len());
    }
}
