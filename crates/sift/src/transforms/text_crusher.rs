//! TextCrusher：纯文本的抽取式有损压缩（request-path 安全的非 ml 方案）。

//!
//! # 算法
//!
//! 把纯文本切分为「段落 / 句子」段，对每段用三因子打分：
//!
//! 1. **recency**：越靠后的段落权重越高（`(i+1)/n`）；
//! 2. **relevance**：与当前 query 的 BM25 相关性（语料统计在候选集内现算）；
//! 3. **salience**：段落中「难重建」词的比例（数字 / 错误关键字 / 全大写 /
//!    点分标识符）。
//!
//! 再按分数降序保留段落，同时用全局 3-gram shingle 索引抑制近重复
//! （已保留 shingle 覆盖比例 ≥ 阈值则跳过），直到达到目标保留比例。
//! 输出是**抽取式**的：保留的段落逐字原文（trim 后以 `\n` 重连），
//! 不编造任何词。压缩是有损的（丢弃低分段落），原文经 [`OffloadTransform`]
//! 卸载进 stash store 后通过 `<<stash:HASH>>` 标记恢复，端到端无损。
//!
//! # 相对参考实现的保留 / 简化
//!
//! 保留：分段（句子/段落）→ BM25 relevance + recency + salience 三因子打分
//! → 近重复抑制 → 按比例保留高分段的完整管线；配置默认值逐项对齐
//! （target_ratio / w_recency / w_relevance / w_salience / min_segment_chars /
//! near_dup_threshold / min_segments_for_crush）。
//!
//! 简化（受依赖约束）：
//! - **去掉 ICU**：参考实现用 `icu_segmenter`（UAX#29 句子切分 + 词典分词）
//!   处理 CJK。这里改为手写分词/分段：ASCII 按空白/标点切词，CJK 按单字符
//!   切词（全角 ASCII 折叠为半角）；句子分段按换行 + 句末标点（`.!?。！？`），
//!   并对「无标点长 CJK 段」做长度兜底切分。
//! - **统一 BM25**：参考实现把 ASCII 走共享 [`Bm25Scorer`](crate::relevance)、
//!   CJK 走本地 ICU-token BM25；本实现只有一个统一 tokenizer，因此只用一份
//!   语料级 BM25（k1=1.2, b=0.75，idf 非负，max 归一化到 [0,1]）覆盖两者。
//! - 缓存键复用 [`crate::stash::compute_key`]（blake3 前 24 hex），不引入 md5。

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::stash;
use crate::content::ContentType;
use crate::transforms::{CompressionContext, OffloadTransform, TransformError};


// ────────────────────────────── 配置 ──────────────────────────────

/// TextCrusher 配置（对应参考 `TextCrusherConfig`，默认值逐项一致）。
#[derive(Debug, Clone)]
pub struct TextCrusherConfig {
    /// 大致保留的字符比例（0.0-1.0）。
    pub target_ratio: f64,
    /// recency 项权重。
    pub w_recency: f64,
    /// relevance（BM25 与 query 相关性）项权重。
    pub w_relevance: f64,
    /// salience（难重建词比例）项权重。
    pub w_salience: f64,
    /// 短于此字节数的段被降权（× 0.25）。
    pub min_segment_chars: usize,
    /// 候选段已有该比例的词 shingle 被保留段覆盖时，视为近重复跳过。
    pub near_dup_threshold: f64,
    /// 低于该段数时整体原样透传（无压缩收益）。
    pub min_segments_for_crush: usize,
    /// 输出 token 上限（`None` 表示不限）。`apply` 中优先用
    /// [`CompressionContext::token_budget`] 覆盖本项。
    pub max_tokens: Option<usize>,
}

impl Default for TextCrusherConfig {
    fn default() -> Self {
        // 默认值与参考实现 `TextCrusherConfig::default()` 对齐。
        TextCrusherConfig {
            target_ratio: 0.5,
            w_recency: 1.0,
            w_relevance: 2.0,
            w_salience: 1.5,
            min_segment_chars: 12,
            near_dup_threshold: 0.85,
            min_segments_for_crush: 6,
            max_tokens: None,
        }
    }
}

// ────────────────────────────── 结果 ──────────────────────────────

/// 单次压缩结果（观测字段，供测试与统计）。
#[derive(Debug, Clone, PartialEq)]
pub struct TextCrusherResult {
    /// 压缩后的抽取式文本（保留段落以 `\n` 重连）。
    pub compressed: String,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    /// `compressed_tokens / original_tokens`；无原文时为 1.0。
    pub compression_ratio: f64,
    pub kept_segments: usize,
    pub total_segments: usize,
}

// ────────────────────────────── 压缩器 ──────────────────────────────

/// TextCrusher 主体：持有配置，无状态（打分统计每次现算）。
#[derive(Debug, Clone)]
pub struct TextCrusher {
    config: TextCrusherConfig,
}

impl Default for TextCrusher {
    fn default() -> Self {
        TextCrusher::new(TextCrusherConfig::default())
    }
}

impl TextCrusher {
    pub fn new(config: TextCrusherConfig) -> Self {
        TextCrusher { config }
    }

    pub fn config(&self) -> &TextCrusherConfig {
        &self.config
    }

    /// 压缩入口：使用配置默认的 `target_ratio` 与 `max_tokens`。
    pub fn compress(&self, content: &str, context: &str) -> TextCrusherResult {
        self.compress_with(
            content,
            context,
            self.config.target_ratio,
            self.config.max_tokens,
        )
    }

    /// 压缩入口（显式给定保留比例与 token 上限）。
    pub fn compress_with(
        &self,
        content: &str,
        context: &str,
        target_ratio: f64,
        max_tokens: Option<usize>,
    ) -> TextCrusherResult {
        let cfg = &self.config;
        let ratio = target_ratio.clamp(0.05, 1.0);

        let segments = split_segments(content);
        if segments.len() < cfg.min_segments_for_crush {
            return passthrough(content, segments.len());
        }

        let n = segments.len();
        let total_chars: usize = segments.iter().map(|s| s.len()).sum();
        // `.max(1)` 避免极小输入把预算截成 0 从而什么都不保留、退化为透传。
        let target_chars = ((total_chars as f64 * ratio) as usize).max(1);

        // 预分词：统一 tokenizer（ASCII 词 / CJK 单字符）同时供 relevance 与
        // salience / shingle 复用，避免重复切分。
        let seg_tokens: Vec<Vec<String>> = segments.iter().map(|s| tokens(s)).collect();
        let relevance = relevance_scores(&seg_tokens, context);

        let mut scores = vec![0.0f64; n];
        for i in 0..n {
            let recency = (i as f64 + 1.0) / n as f64;
            let rel = relevance.get(i).copied().unwrap_or(0.0);
            // salience：难重建词占比（统一用已预分词的 token）。
            let salient = seg_tokens[i].iter().filter(|w| is_salient(w)).count();
            let word_count = seg_tokens[i].len();
            let salience = salient as f64 / (word_count as f64 + 1.0);
            let mut score =
                cfg.w_recency * recency + cfg.w_relevance * rel + cfg.w_salience * salience;
            if segments[i].len() < cfg.min_segment_chars {
                score *= 0.25;
            }
            scores[i] = score;
        }

        // 分数降序；并列按下标升序，保证确定性。
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| {
            scores[b]
                .partial_cmp(&scores[a])
                .unwrap_or(Ordering::Equal)
                .then(a.cmp(&b))
        });

        let mut kept = vec![false; n];
        let mut seen: HashSet<String> = HashSet::new();
        let mut kept_chars = 0usize;
        let mut kept_tokens = 0usize;
        let mut kept_count = 0usize;

        // 保密保底：段内任一空白切分 token 命中熵检测（`is_secret_like`）
        // → 该段强制
        // 保留（pinned），不可因低分 / 预算 / 近重复被丢弃。凭证不可重建，
        // 有损压缩丢弃即丢失（不变量 3 的前提是内容还在）。
        for (i, seg) in segments.iter().enumerate() {
            if seg.split_whitespace().any(crate::secrets::is_secret_like) {
                kept[i] = true;
                kept_count += 1;
                for s in shingles(&seg_tokens[i], 3) {
                    seen.insert(s);
                }
                kept_chars += seg.len();
                kept_tokens += seg_tokens[i].len();
            }
        }

        for &i in &order {
            if kept[i] {
                continue;
            }
            if kept_chars >= target_chars {
                break;
            }
            if let Some(max_t) = max_tokens {
                if kept_tokens >= max_t {
                    break;
                }
            }
            // 近重复抑制：候选段的 shingle 已被保留段覆盖到阈值则跳过。
            let sh = shingles(&seg_tokens[i], 3);
            if !sh.is_empty() {
                let covered = sh.iter().filter(|s| seen.contains(s.as_str())).count() as f64
                    / sh.len() as f64;
                if covered >= cfg.near_dup_threshold {
                    continue;
                }
            }
            kept[i] = true;
            kept_count += 1;
            for s in sh {
                seen.insert(s);
            }
            kept_chars += segments[i].len();
            kept_tokens += seg_tokens[i].len();
        }

        if kept_count == 0 {
            return passthrough(content, n);
        }

        let compressed = (0..n)
            .filter(|&i| kept[i])
            .map(|i| segments[i].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let orig_tok = count_tokens(content);
        let comp_tok = count_tokens(&compressed);
        TextCrusherResult {
            compression_ratio: if orig_tok > 0 {
                comp_tok as f64 / orig_tok as f64
            } else {
                1.0
            },
            compressed,
            original_tokens: orig_tok,
            compressed_tokens: comp_tok,
            kept_segments: kept_count,
            total_segments: n,
        }
    }
}

fn passthrough(content: &str, n_segments: usize) -> TextCrusherResult {
    let toks = count_tokens(content);
    TextCrusherResult {
        compressed: content.to_string(),
        original_tokens: toks,
        compressed_tokens: toks,
        compression_ratio: 1.0,
        kept_segments: n_segments,
        total_segments: n_segments,
    }
}

// ────────────────────────────── 分段 ──────────────────────────────

/// 句子/段落分段：按换行切行，行内按句末标点（ASCII `.`/`!`/`?` 或全角
/// `。`/`！`/`？`）切分；最后对「无标点超长 CJK 段」做长度兜底。
fn split_segments(text: &str) -> Vec<String> {
    let mut segs = Vec::new();
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut cur = String::new();
        let mut prev_ascii_term = false;
        for c in trimmed.chars() {
            // ASCII 句末标点后跟空白才断句（与参考 ASCII 路径一致，
            // 避免把 `e.g.` / 小数 / 缩写误切）。
            if prev_ascii_term && c.is_whitespace() {
                push_segment(&mut segs, &mut cur);
                prev_ascii_term = false;
                continue; // 丢弃分隔空白
            }
            cur.push(c);
            if is_fullwidth_terminal(c) {
                // 全角句末标点自身即句界（CJK 无空格，不能等空白）。
                push_segment(&mut segs, &mut cur);
                prev_ascii_term = false;
            } else {
                prev_ascii_term = matches!(c, '.' | '!' | '?');
            }
        }
        push_segment(&mut segs, &mut cur);
    }
    apply_length_fallback(segs)
}

fn push_segment(segs: &mut Vec<String>, cur: &mut String) {
    let s = cur.trim();
    if !s.is_empty() {
        segs.push(s.to_string());
    }
    cur.clear();
}

/// 全角句末标点。
fn is_fullwidth_terminal(c: char) -> bool {
    matches!(c, '。' | '！' | '？')
}

/// 无标点超长 CJK 段的兜底切分：段长 > 60 字符时，在空白 / 二级标点处
/// （软切，≥20 字符）或硬上限 40 字符处强制断开，保证不会整段透传。
fn apply_length_fallback(segs: Vec<String>) -> Vec<String> {
    const SOFT_CAP: usize = 60;
    const HARD_CAP: usize = 40;
    let mut out = Vec::new();
    for s in segs {
        if s.chars().count() <= SOFT_CAP || !s.chars().any(is_han) {
            out.push(s);
            continue;
        }
        let mut piece = String::new();
        for c in s.chars() {
            piece.push(c);
            let n = piece.chars().count();
            let soft = c.is_whitespace() || matches!(c, '、' | '，' | '；' | '：' | '·' | '…');
            if (soft && n >= HARD_CAP / 2) || n >= HARD_CAP {
                let t = piece.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
                piece.clear();
            }
        }
        let t = piece.trim();
        if !t.is_empty() {
            out.push(t.to_string());
        }
    }
    out
}

// ────────────────────────────── 分词 ──────────────────────────────

/// 统一 tokenizer：ASCII/拉丁按字母数字下划线 run 切词（小写），
/// CJK（汉字/假名/谚文）按单字符切词；全角 ASCII 折叠为半角后参与
/// run 合并（`ＡＰＩ` → `api`），全角空格折叠为普通空格。
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        let f = width_fold(c);
        if is_han(f) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push(f.to_string());
        } else if f.is_alphanumeric() || f == '_' {
            cur.extend(f.to_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 全角 ASCII（０-９、Ａ-Ｚ 等）与全角标点折叠为半角，表意空格折叠为空格。
/// 只影响内部 token 键，保留的输出仍逐字原文。
fn width_fold(c: char) -> char {
    match c as u32 {
        0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        0x3000 => ' ',
        _ => c,
    }
}

/// 汉字 / 假名 / 谚文判定（无空格、无 ASCII 词界的脚本）。
fn is_han(c: char) -> bool {
    matches!(
        c as u32,
        0x3040..=0x30FF // 平假名 + 片假名
            | 0x3400..=0x4DBF // CJK 扩展 A
            | 0x4E00..=0x9FFF // CJK 统一汉字
            | 0xAC00..=0xD7AF // 谚文音节
            | 0xF900..=0xFAFF // CJK 兼容汉字
            | 0x20000..=0x2FA1F // CJK 扩展 B–F + 兼容补充
    )
}

/// token 计数（压缩比报告用）：含 CJK 时按统一 token 数，否则按空白词数
/// （纯空白切分会把一整段无空格 CJK 计为 1，使压缩比失真）。
fn count_tokens(s: &str) -> usize {
    if s.chars().any(is_han) {
        tokens(s).len()
    } else {
        s.split_whitespace().count()
    }
}

// ────────────────────────────── 相关性 ──────────────────────────────

/// 语料级 BM25：在全部候选段的预分词 token 上现算 df / avgdl，对每段打分，
/// 输出 max 归一化到 [0,1]（与参考 `relevance_cjk` 的本地 BM25 语义一致，
/// 但用统一 tokenizer 覆盖 ASCII 与 CJK）。
fn relevance_scores(seg_tokens: &[Vec<String>], context: &str) -> Vec<f64> {
    let n = seg_tokens.len();
    let qtokens: HashSet<String> = tokens(context).into_iter().collect();
    if n == 0 || qtokens.is_empty() {
        return vec![0.0; n];
    }
    // 文档频率：每个 term 出现在多少段里（段内去重）。
    let mut df: HashMap<&str, usize> = HashMap::new();
    for toks in seg_tokens {
        let uniq: HashSet<&str> = toks.iter().map(|s| s.as_str()).collect();
        for t in uniq {
            *df.entry(t).or_insert(0) += 1;
        }
    }
    let nf = n as f64;
    // `+1` 保证 idf 非负。
    let idf = |t: &str| -> f64 {
        let d = *df.get(t).unwrap_or(&0) as f64;
        (((nf - d + 0.5) / (d + 0.5)) + 1.0).ln()
    };
    let (k1, b) = (1.2_f64, 0.75_f64);
    let avgdl = (seg_tokens.iter().map(|t| t.len()).sum::<usize>() as f64 / nf).max(1.0);

    let mut out = vec![0.0f64; n];
    for (i, toks) in seg_tokens.iter().enumerate() {
        let mut tf: HashMap<&str, usize> = HashMap::new();
        for t in toks {
            *tf.entry(t.as_str()).or_insert(0) += 1;
        }
        let dl = toks.len() as f64;
        let mut score = 0.0;
        for q in &qtokens {
            if let Some(&f) = tf.get(q.as_str()) {
                let f = f as f64;
                score += idf(q) * (f * (k1 + 1.0)) / (f + k1 * (1.0 - b + b * dl / avgdl));
            }
        }
        out[i] = score;
    }
    let max = out.iter().cloned().fold(0.0f64, f64::max);
    if max > 0.0 {
        for s in &mut out {
            *s /= max;
        }
    }
    out
}

// ────────────────────────────── 近重复 / salience ──────────────────────────────

/// 词 shingle（k-gram，用 `\u{1}` 连接）。段短于 k 时产出所有子窗口，
/// 让短的相同/重叠段仍能互相近重复匹配。
fn shingles(words: &[String], k: usize) -> HashSet<String> {
    let mut set = HashSet::new();
    if words.is_empty() {
        return set;
    }
    if words.len() < k {
        for size in 1..=words.len() {
            for w in words.windows(size) {
                set.insert(w.join("\u{1}"));
            }
        }
        return set;
    }
    for w in words.windows(k) {
        set.insert(w.join("\u{1}"));
    }
    set
}

const KEYWORDS: [&str; 10] = [
    "error",
    "exception",
    "failed",
    "failure",
    "fail",
    "warning",
    "traceback",
    "assert",
    "todo",
    "fixme",
];

/// 词是否携带「难重建」信息：含数字、错误/状态关键字、全大写（≥2 字母）、
/// 或点分标识符（`foo.bar`）。
fn is_salient(word: &str) -> bool {
    if word.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }
    let lower = word
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    if KEYWORDS.contains(&lower.as_str()) {
        return true;
    }
    let alpha: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    if alpha.len() >= 2 && alpha.iter().all(|c| c.is_uppercase()) {
        return true;
    }
    if let Some(dot) = word.find('.') {
        let a = &word[..dot];
        let b = &word[dot + 1..];
        if !a.is_empty()
            && !b.is_empty()
            && a.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
            && b.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            return true;
        }
    }
    false
}

// ────────────────────────────── OffloadTransform ──────────────────────────────

/// TextCrusher 是有损压缩（丢弃低分段落），原文由调用方写入 stash store，
/// `apply` 返回 `(compressed, original)`。
impl OffloadTransform for TextCrusher {
    fn name(&self) -> &'static str {
        "text_crusher"
    }

    fn applies_to(&self) -> ContentType {
        ContentType::PlainText
    }

    /// 膨胀度估算：不可压（段数不足）时 0.0；否则为「可丢弃比例」
    /// （`1 - target_ratio`），确定性。
    fn estimate_bloat(&self, input: &str) -> f64 {
        let segments = split_segments(input);
        if segments.len() < self.config.min_segments_for_crush {
            0.0
        } else {
            1.0 - self.config.target_ratio
        }
    }

    /// cache key：与 [`crate::stash::compute_key`] 统一（blake3 前 24 hex），
    /// 保证 marker 与 store 跨压缩器键格式一致。
    fn cache_key(&self, input: &str) -> String {
        stash::compute_key(input)
    }

    fn apply(
        &self,
        input: &str,
        ctx: &CompressionContext,
    ) -> Result<(String, String), TransformError> {
        let query = ctx.query.as_deref().unwrap_or("");
        // token 预算优先取调用方上下文，其次取配置。
        let max_tokens = ctx.token_budget.or(self.config.max_tokens);
        let result = self.compress_with(input, query, self.config.target_ratio, max_tokens);
        // 没丢弃任何段落（透传/太短/比例不满）→ 无压缩空间，交给上层原样。
        if result.kept_segments >= result.total_segments || result.compressed == input {
            return Err(TransformError::Skipped);
        }
        Ok((result.compressed, input.to_string()))
    }
}

// ────────────────────────────── 单元测试 ──────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(n: usize) -> String {
        (0..n)
            .map(|i| format!("Sentence number {i} describes a distinct topic {i} in some detail."))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn ctx(query: Option<&str>) -> CompressionContext {
        CompressionContext {
            query: query.map(|q| q.to_string()),
            token_budget: None,
        }
    }

    // ---------- 压缩与抽取式性质 ----------

    #[test]
    fn extractive_and_compresses() {
        let content = doc(40);
        let r = TextCrusher::default().compress_with(&content, "", 0.3, None);
        assert!(r.compressed_tokens < r.original_tokens, "长文本必须变短");
        // 抽取式：输出每个词都出现在输入里。
        let orig: HashSet<&str> = content.split_whitespace().collect();
        assert!(
            r.compressed.split_whitespace().all(|w| orig.contains(w)),
            "输出不得编造新词"
        );
    }

    #[test]
    fn deterministic() {
        let content = doc(40);
        let tc = TextCrusher::default();
        assert_eq!(
            tc.compress_with(&content, "", 0.4, None).compressed,
            tc.compress_with(&content, "", 0.4, None).compressed
        );
    }

    #[test]
    fn long_text_compresses_via_apply() {
        let content = doc(40);
        let (compressed, _) = TextCrusher::default()
            .apply(&content, &ctx(None))
            .expect("长文本应可压缩");
        assert!(compressed.len() < content.len());
    }

    // ---------- 关键信息保留（query 相关性） ----------

    #[test]
    fn query_related_segment_survives_ascii() {
        let mut parts: Vec<String> = (0..30)
            .map(|i| format!("Routine status report for item {i}. All systems nominal."))
            .collect();
        parts.insert(
            15,
            "The secret code is salamander. Authorize immediately.".to_string(),
        );
        let content = parts.join(" ");
        let r = TextCrusher::default().compress_with(&content, "salamander", 0.4, None);
        assert!(r.compressed_tokens < r.original_tokens, "应压缩");
        assert!(
            r.compressed.contains("salamander"),
            "query 命中的段落必须保留: {}",
            r.compressed
        );
    }

    #[test]
    fn cjk_relevance_keeps_query_match() {
        let needle = "认证令牌的缓存策略采用最近最少使用淘汰算法来管理过期。";
        let filler = "今天天气很好。我们去公园散步。然后回家吃饭。下午还要开会。\
                      晚上看电影。明天继续工作。周末去爬山。后天有个会议。";
        let doc = format!("{filler}{needle}{filler}");
        let r = TextCrusher::default().compress_with(&doc, "认证令牌缓存策略", 0.3, None);
        assert!(r.compressed_tokens < r.original_tokens, "应压缩");
        assert!(
            r.compressed.contains("认证令牌"),
            "query 命中的 CJK 段落必须保留: {}",
            r.compressed
        );
    }

    #[test]
    fn mixed_cjk_latin_keeps_ascii_terms() {
        let content = "系统启动失败。认证模块超时。\nERROR: connection refused at host.\n\
                       数据库连接池耗尽。重试机制触发。服务降级处理完成。请检查日志。";
        let r = TextCrusher::default().compress_with(content, "ERROR connection", 0.5, None);
        assert!(r.compression_ratio < 1.0, "混合内容必须压缩");
        assert!(r.original_tokens > 5, "CJK 不得塌缩成单个 token");
        assert!(
            r.compressed.contains("ERROR"),
            "query 相关的 ASCII 术语必须保留: {}",
            r.compressed
        );
    }

    // ---------- 空 / 短文本透传 ----------

    #[test]
    fn passthrough_when_small() {
        let r = TextCrusher::default().compress("one. two. three.", "");
        assert_eq!(r.compressed, "one. two. three.");
        assert_eq!(r.compression_ratio, 1.0);
    }

    #[test]
    fn empty_passthrough() {
        let r = TextCrusher::default().compress("", "");
        assert_eq!(r.compressed, "");
        assert_eq!(r.compression_ratio, 1.0);
        assert_eq!(r.total_segments, 0);
    }

    #[test]
    fn apply_skips_empty_and_short() {
        let tc = TextCrusher::default();
        assert_eq!(
            tc.apply("", &CompressionContext::default()).unwrap_err(),
            TransformError::Skipped
        );
        assert_eq!(
            tc.apply("one. two. three.", &CompressionContext::default())
                .unwrap_err(),
            TransformError::Skipped
        );
    }

    // ---------- CJK 分段 / 分词 ----------

    #[test]
    fn cjk_splits_on_full_width_terminators() {
        let zh = "今天天气很好。我们去公园散步。然后回家吃饭。下午还要开会。晚上看电影。";
        let segs = split_segments(zh);
        assert!(segs.len() >= 4, "应切出多个 CJK 句子，got {segs:?}");
        for s in &segs {
            assert!(zh.contains(s.as_str()), "段必须逐字原文: {s}");
        }
    }

    #[test]
    fn cjk_terminator_sparse_length_fallback() {
        let zh = "机器学习模型从数据中学习特征并识别模式进行预测的系统会不断地调整参数\
                  以最小化误差从而提升准确率这是一段很长的没有任何标点的中文用来测试兜底\
                  切分是否生效以及能否产生多个段落供后续打分与去重使用确保不会整段透传";
        assert!(split_segments(zh).len() >= 2, "无标点长 CJK 段必须仍能切分");
    }

    #[test]
    fn cjk_tokens_not_one_giant_token() {
        assert!(tokens("数据库连接失败重试三次").len() >= 3);
    }

    #[test]
    fn fullwidth_ascii_folds_to_halfwidth() {
        let toks = tokens("认证ＡＰＩ密钥");
        assert!(
            toks.iter().any(|t| t == "api"),
            "全角 ASCII 应折叠为 'api': {toks:?}"
        );
        assert!(tokens("端口８０８０").iter().any(|t| t == "8080"));
    }

    #[test]
    fn korean_and_japanese_tokenize_by_char() {
        // 无 ICU 词典：谚文/假名按单字符切词，不得塌缩成一个 token。
        assert!(tokens("인증 토큰의 캐시 전략은").len() >= 4);
        assert!(tokens("認証トークンのキャッシュ戦略").len() >= 5);
    }

    // ---------- 熵检测保密（secret 段强制保留） ----------

    #[test]
    fn low_score_segment_with_api_key_is_pinned() {
        // 低分（无 query 相关、无 salient 词、位置靠前）但含 API key 的段落
        // 必须被强制保留，不可因预算耗尽被丢弃。
        let key = "sk-ant-api03-9Xq2mB7vLpK4tRnZ8WjE5fHc3DsY6uA1";
        let mut parts: Vec<String> = (0..40)
            .map(|i| format!("Routine housekeeping note {i} with ordinary words and filler."))
            .collect();
        parts.insert(3, format!("My api key is {key} please keep it safe."));
        let content = parts.join(" ");
        let r = TextCrusher::default().compress_with(&content, "unrelated query", 0.15, None);
        assert!(r.compressed_tokens < r.original_tokens, "整体仍应压缩");
        assert!(
            r.compressed.contains(key),
            "含 API key 的低分段必须保留: {}",
            r.compressed
        );
    }

    #[test]
    fn low_score_segment_with_hex_token_is_pinned() {
        let hex = "91f0d3ab62c4e8577a3b9c1d4e5f6071aabbccdd";
        let filler = "Ordinary prose paragraph without any special tokens at all. "
            .repeat(20);
        let content = format!("token: {hex}\n{filler}");
        let r = TextCrusher::default().compress_with(&content, "", 0.1, None);
        assert!(r.compressed_tokens < r.original_tokens, "整体仍应压缩");
        assert!(r.compressed.contains(hex), "含 hex token 的段落必须保留");
    }

    #[test]
    fn pinned_secret_segment_survives_near_dup_and_budget() {
        // secret 段本身是大量重复段落之一（近重复抑制环境），且预算极小，
        // 仍必须保留。
        let key = "ghp_48xKq2mN7vJz3pLw9RtY5bEcVdXfGaHiQw";
        let sentence = "This is a highly repetitive sentence that just keeps repeating itself.";
        let mut parts: Vec<String> = std::iter::repeat(sentence.to_string())
            .take(30)
            .collect();
        parts.insert(0, format!("Credential {key} stored here."));
        let content = parts.join(" ");
        let r = TextCrusher::default().compress_with(&content, "", 0.05, None);
        assert!(
            r.compressed.contains(key),
            "secret 段不得被近重复抑制或预算丢弃: {}",
            r.compressed
        );
    }

    // ---------- 近重复抑制 ----------

    #[test]
    fn near_duplicates_collapse() {
        // 大量重复段落应被近重复抑制折叠，输出远小于原文。
        let sentence = "This is a highly repetitive sentence that just keeps repeating itself.";
        let content = std::iter::repeat(sentence)
            .take(40)
            .collect::<Vec<_>>()
            .join(" ");
        let r = TextCrusher::default().compress_with(&content, "", 0.5, None);
        assert!(r.kept_segments < r.total_segments);
        assert!(r.compressed.len() < content.len());
    }

    // ---------- trait / 元数据 ----------

    #[test]
    fn estimate_bloat_and_cache_key_deterministic() {
        let tc = TextCrusher::default();
        assert_eq!(tc.name(), "text_crusher");
        assert_eq!(tc.applies_to(), ContentType::PlainText);

        let long = doc(40);
        assert!(tc.estimate_bloat(&long) > 0.0);
        assert_eq!(tc.estimate_bloat("one. two."), 0.0);
        assert_eq!(tc.estimate_bloat(&long), tc.estimate_bloat(&long));

        assert_eq!(tc.cache_key(&long), tc.cache_key(&long));
        assert_eq!(tc.cache_key(&long).len(), 24);
        assert_ne!(tc.cache_key(&long), tc.cache_key("one. two."));
        assert_eq!(tc.cache_key(&long), stash::compute_key(&long));
    }

    #[test]
    fn apply_returns_original_for_stash() {
        let content = doc(40);
        let (compressed, original) = TextCrusher::default()
            .apply(&content, &ctx(Some("topic 7")))
            .expect("长文本应可压缩");
        assert_eq!(original, content, "原文应原样返回供 stash 卸载");
        assert!(compressed.len() < content.len());
        assert!(compressed.contains("topic 7"), "query 相关段落应保留");
    }
}
