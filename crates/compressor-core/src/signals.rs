//! 行重要性信号检测（单文件模块）。
//!
//! 行重要性信号（line_importance + keyword_detector + tiered）：
//! 压缩器在 token 预算下决定丢哪些行时调用本模块。信号携带
//! 类别、优先级、置信度——绝不返回裸 bool——以便未来的分层检测器
//! 可以在高置信度时短路、低优先级调用方可以继续向下问询。
//!
//! 与参考实现的差异：aho-corasick 自动机 → 手写小写化、子串扫描、
//! ASCII 词边界后过滤（不新增依赖，语义等价：词内子串不命中、
//! 大小写不敏感、长词优先）。

use std::collections::BTreeMap;

// ─── trait 与核心类型（对应 signals/line_importance.rs） ──────────────────

/// 行的来源上下文。决定哪组模式生效（如 markdown 标题在散文里是
/// 优先信号，在 diff hunk 里不是）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportanceContext {
    /// 自由散文（text_compressor）——markdown 结构有意义。
    Text,
    /// grep/ripgrep 输出（search_compressor）——error/warn 关键词优先。
    Search,
    /// git diff（diff_compressor）——error + security + importance 关键词。
    Diff,
    /// 日志输出（log_compressor）——error/warn 关键词 + 级别前缀。
    Log,
}

/// 行获得优先级的原因类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportanceCategory {
    Error,
    Warning,
    Importance,
    Security,
    /// Markdown 结构——标题、加粗、引用块。仅在 `ImportanceContext::Text` 中有意义。
    Markdown,
}

/// 单个检测器对单行的输出。
///
/// `priority` 是压缩器排序的依据；`confidence` 是 [`Tiered`] 组合器
/// 决定是否继续问询下一层的依据。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImportanceSignal {
    /// 检测器命中的类别（若有）。
    pub category: Option<ImportanceCategory>,
    /// 0.0 = 最先丢弃，1.0 = 拼命保留。
    pub priority: f32,
    /// 0.0 = 无信息，1.0 = 检测器确信。
    pub confidence: f32,
}

impl ImportanceSignal {
    /// “本行无意见”。未命中任何关键词时返回。
    pub const fn neutral() -> Self {
        Self {
            category: None,
            priority: 0.0,
            confidence: 0.0,
        }
    }

    /// 一次显式命中，带类别 / 优先级 / 置信度。
    pub const fn matched(category: ImportanceCategory, priority: f32, confidence: f32) -> Self {
        Self {
            category: Some(category),
            priority,
            confidence,
        }
    }

    /// 检测器是否识别到了东西。
    pub fn is_match(&self) -> bool {
        self.category.is_some()
    }
}

/// 单行重要性分类器。
///
/// 实现必须廉价（关键词扫描、词法特征）或可摊销（嵌入 + 分类头批量推理）。
/// 必须是 `Send + Sync`：压缩器会跨线程共享检测器实例。
pub trait LineImportanceDetector: Send + Sync {
    /// 在给定上下文中给单行打分。
    fn score(&self, line: &str, ctx: ImportanceContext) -> ImportanceSignal;
}

// ─── 关键词检测（对应 signals/keyword_detector.rs） ───────────────────────

/// 关键词层使用的置信度。低于 [`ESCALATE_THRESHOLD`]，未来的 ML 层
/// 可以在边界情形覆盖它；但足够高，无歧义的关键词命中不会被反复怀疑。
const KEYWORD_CONFIDENCE: f32 = 0.7;

/// 各类别的优先级常量。压缩器按它排序。
const ERROR_PRIORITY: f32 = 0.95;
const WARNING_PRIORITY: f32 = 0.75;
const SECURITY_PRIORITY: f32 = 0.85;
const IMPORTANCE_PRIORITY: f32 = 0.6;
const MARKDOWN_PRIORITY: f32 = 0.45;

/// 每个重要性类别的静态关键词数据。
#[derive(Debug, Clone)]
pub struct KeywordRegistry {
    pub error: Vec<&'static str>,
    pub warning: Vec<&'static str>,
    pub importance: Vec<&'static str>,
    pub security: Vec<&'static str>,
    /// 仅在 Text 上下文按 *行前缀* 匹配的重要性信号（markdown 标题
    /// `# `、引用块 `> ` 等），不是全行关键词。
    pub markdown_prefixes: Vec<&'static str>,
    /// 子串级错误指示词（无词边界要求），用于快速分诊
    ///（[`KeywordDetector::contains_error_indicator`]）。与 `error` 集合
    /// 不同：携带 `traceback`、不含 timeout 等四个扩展词。
    pub error_indicators: Vec<&'static str>,
}

impl KeywordRegistry {
    /// 默认关键词集：Python 版 error_detection.py 的超集，
    /// 去掉了 security 集合中的 `token`（对 LLM token 计数行误报严重），
    /// 补上了 Python 正则遗漏的 {abort, timeout, denied, rejected}。
    pub fn default_set() -> Self {
        Self {
            error: vec![
                "error",
                "exception",
                "fail",
                "failed",
                "failure",
                "fatal",
                "critical",
                "crash",
                "panic",
                "abort",
                "timeout",
                "denied",
                "rejected",
            ],
            warning: vec!["warn", "warning"],
            importance: vec![
                "important",
                "note",
                "todo",
                "fixme",
                "hack",
                "xxx",
                "bug",
                "fix",
            ],
            security: vec!["security", "auth", "password", "secret"],
            markdown_prefixes: vec!["# ", "## ", "### ", "#### ", "**", "> "],
            error_indicators: vec![
                "error",
                "fail",
                "exception",
                "traceback",
                "fatal",
                "panic",
                "crash",
            ],
        }
    }

    /// 供外部反射的快照。`BTreeMap` 保证遍历顺序确定。
    pub fn as_map(&self) -> BTreeMap<&'static str, Vec<&'static str>> {
        let mut m = BTreeMap::new();
        m.insert("error", self.error.clone());
        m.insert("warning", self.warning.clone());
        m.insert("importance", self.importance.clone());
        m.insert("security", self.security.clone());
        m.insert("markdown_prefixes", self.markdown_prefixes.clone());
        m.insert("error_indicators", self.error_indicators.clone());
        m
    }
}

/// 关键词表 + 对应类别的查找结构。匹配在小写化副本上进行，
/// 词边界检查基于返回的字节偏移（`to_ascii_lowercase` 不改变字节长度，
/// 因此偏移可直接映射回原行）。
struct CategoryWords {
    /// 关键词（小写），按长度降序（长词优先，近似 LeftmostLongest）。
    words: Vec<&'static str>,
}

impl CategoryWords {
    fn build(mut words: Vec<&'static str>) -> Self {
        words.sort_by_key(|w| std::cmp::Reverse(w.len()));
        Self { words }
    }

    /// `line` 中是否存在某关键词作为 *整词* 出现；存在则返回其类别。
    fn first_word_match(&self, line_lower: &str) -> bool {
        let bytes = line_lower.as_bytes();
        for word in &self.words {
            let mut from = 0usize;
            while let Some(pos) = line_lower[from..].find(*word) {
                let start = from + pos;
                let end = start + word.len();
                if is_word_boundary(bytes, start, end) {
                    return true;
                }
                from = end;
            }
        }
        false
    }
}

/// 基于关键词的 [`LineImportanceDetector`]（aho-corasick 的无依赖替代实现）。
///
/// 用 [`KeywordDetector::new`] 取默认关键词集，
/// 或 [`KeywordDetector::with_registry`] 定制。
pub struct KeywordDetector {
    registry: KeywordRegistry,
    /// 所有上下文都生效的类别（error/importance）。
    universal_error: CategoryWords,
    universal_importance: CategoryWords,
    /// warning 在 Search/Log/Text 上下文生效，Diff 中省略
    ///（与 Python 版 PRIORITY_PATTERNS_DIFF 形状一致）。
    warning: CategoryWords,
    /// security 仅在 Diff 上下文生效。
    security: CategoryWords,
    /// 子串级分诊指示词（无词边界要求），与打分集合刻意分离。
    indicators: Vec<&'static str>,
}

impl KeywordDetector {
    pub fn new() -> Self {
        Self::with_registry(KeywordRegistry::default_set())
    }

    pub fn with_registry(registry: KeywordRegistry) -> Self {
        let indicators = registry.error_indicators.clone();
        Self {
            universal_error: CategoryWords::build(registry.error.clone()),
            universal_importance: CategoryWords::build(registry.importance.clone()),
            warning: CategoryWords::build(registry.warning.clone()),
            security: CategoryWords::build(registry.security.clone()),
            indicators,
            registry,
        }
    }

    /// 快速“是否包含任何错误形状的东西”检查（原
    /// `content_has_error_indicators` 调用点）。子串匹配、无词边界要求，
    /// 保留 Python 版的宽松语义。关键词集与 [`Self::score`] 不同
    ///（携带 `traceback`、不含 timeout 等扩展词）——分诊调用点关心的
    /// 是 Python 风格异常输出而非连接状态。
    pub fn contains_error_indicator(&self, text: &str) -> bool {
        let lower = text.to_ascii_lowercase();
        self.indicators.iter().any(|i| lower.contains(i))
    }

    pub fn registry(&self) -> &KeywordRegistry {
        &self.registry
    }

    fn match_in_context(
        &self,
        line: &str,
        ctx: ImportanceContext,
    ) -> Option<(ImportanceCategory, f32)> {
        // 大小写不敏感：在小写副本上扫描；ASCII 小写化保字节长度，
        // 词边界偏移可直接复用。
        let lower = line.to_ascii_lowercase();

        // 通用类别（error / importance）——error 优先。
        if self.universal_error.first_word_match(&lower) {
            return Some((ImportanceCategory::Error, ERROR_PRIORITY));
        }
        if self.universal_importance.first_word_match(&lower) {
            return Some((ImportanceCategory::Importance, IMPORTANCE_PRIORITY));
        }
        match ctx {
            ImportanceContext::Diff => {
                if self.security.first_word_match(&lower) {
                    return Some((ImportanceCategory::Security, SECURITY_PRIORITY));
                }
            }
            ImportanceContext::Text | ImportanceContext::Search | ImportanceContext::Log => {
                if self.warning.first_word_match(&lower) {
                    return Some((ImportanceCategory::Warning, WARNING_PRIORITY));
                }
            }
        }
        // markdown 结构前缀只在 Text 上下文生效
        if matches!(ctx, ImportanceContext::Text)
            && self
                .registry
                .markdown_prefixes
                .iter()
                .any(|p| line.starts_with(p))
        {
            return Some((ImportanceCategory::Markdown, MARKDOWN_PRIORITY));
        }
        None
    }
}

impl Default for KeywordDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl LineImportanceDetector for KeywordDetector {
    fn score(&self, line: &str, ctx: ImportanceContext) -> ImportanceSignal {
        match self.match_in_context(line, ctx) {
            Some((category, priority)) => {
                ImportanceSignal::matched(category, priority, KEYWORD_CONFIDENCE)
            }
            None => ImportanceSignal::neutral(),
        }
    }
}

#[cfg(test)]
const fn priority_for(category: ImportanceCategory) -> f32 {
    match category {
        ImportanceCategory::Error => ERROR_PRIORITY,
        ImportanceCategory::Warning => WARNING_PRIORITY,
        ImportanceCategory::Security => SECURITY_PRIORITY,
        ImportanceCategory::Importance => IMPORTANCE_PRIORITY,
        ImportanceCategory::Markdown => MARKDOWN_PRIORITY,
    }
}

/// `[start..end)` 两侧是否均为非词字符（或串边界）。ASCII 词字符：`[A-Za-z0-9_]`。
fn is_word_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    let left_ok = start == 0 || !is_word_byte(bytes[start - 1]);
    let right_ok = end == bytes.len() || !is_word_byte(bytes[end]);
    left_ok && right_ok
}

#[inline]
fn is_word_byte(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_')
}

// ─── 分层组合（对应 signals/tiered.rs） ───────────────────────────────────

/// `Tiered` 接受某层信号而不继续问询后续层的置信度阈值。
/// KeywordDetector 输出 0.7，默认即胜出；一个校准置信度 ≥ 0.8 的
/// ML 层可以反过来短路关键词层。
pub const ESCALATE_THRESHOLD: f32 = 0.7;

/// 分层检测器组合器。按顺序链式调用各层：第一层置信度达到
/// [`ESCALATE_THRESHOLD`] 即胜出；否则跳过（继续问下一层）。
/// 若没有层达标，返回见到的最高置信度信号——调用方仍能拿到
/// 最佳猜测，置信度反映整个栈有多不确定。
///
/// 分层是 *组合* 而非继承：`KeywordDetector` 对未来的 ML 检测器
/// 一无所知，反之亦然；两者都实现 trait，由 `Tiered` 排定顺序。
pub struct Tiered {
    tiers: Vec<Box<dyn LineImportanceDetector>>,
}

impl Tiered {
    pub fn new() -> Self {
        Self { tiers: Vec::new() }
    }

    /// 压入一层。顺序重要：最精确的在前。
    pub fn with(mut self, tier: Box<dyn LineImportanceDetector>) -> Self {
        self.tiers.push(tier);
        self
    }

    /// 便捷方法：接收所有权检测器并装箱，调用点免写 `as Box<dyn …>`。
    pub fn with_detector<D: LineImportanceDetector + 'static>(self, detector: D) -> Self {
        self.with(Box::new(detector))
    }

    pub fn len(&self) -> usize {
        self.tiers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiers.is_empty()
    }
}

impl Default for Tiered {
    fn default() -> Self {
        Self::new()
    }
}

impl LineImportanceDetector for Tiered {
    fn score(&self, line: &str, ctx: ImportanceContext) -> ImportanceSignal {
        let mut best = ImportanceSignal::neutral();
        for tier in &self.tiers {
            let signal = tier.score(line, ctx);
            if signal.confidence >= ESCALATE_THRESHOLD {
                return signal;
            }
            if signal.confidence > best.confidence {
                best = signal;
            }
        }
        best
    }
}

// ─── 测试 ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(line: &str, ctx: ImportanceContext) -> ImportanceSignal {
        KeywordDetector::new().score(line, ctx)
    }

    // ─── 关键词检测 ─────────────────────────────────────────────────

    #[test]
    fn fires_on_uppercase_error_in_search() {
        let s = detect("ERROR: connection refused", ImportanceContext::Search);
        assert_eq!(s.category, Some(ImportanceCategory::Error));
        assert!(s.priority > 0.9);
        assert_eq!(s.confidence, KEYWORD_CONFIDENCE);
    }

    #[test]
    fn case_insensitive_matching() {
        let s = detect("Error: something went wrong", ImportanceContext::Log);
        assert_eq!(s.category, Some(ImportanceCategory::Error));
        let s2 = detect("FaTaL crash imminent", ImportanceContext::Log);
        assert_eq!(s2.category, Some(ImportanceCategory::Error));
    }

    #[test]
    fn timeout_now_classified_as_error_in_diff() {
        // fixed_in_3e1：Python 的 ERROR_PATTERN 正则遗漏 "timeout"，
        // 该行尽管命中规范关键词表也被判为 neutral。
        let s = detect("FATAL: timeout connecting upstream", ImportanceContext::Diff);
        assert_eq!(s.category, Some(ImportanceCategory::Error));
    }

    #[test]
    fn rejected_now_classified_as_error() {
        // fixed_in_3e1：与 Python 的 parity 差距
        let s = detect("auth request rejected", ImportanceContext::Diff);
        assert_eq!(s.category, Some(ImportanceCategory::Error));
    }

    #[test]
    fn token_no_longer_flags_security_in_llm_proxy_context() {
        // fixed_in_3e1：security 集合去掉 "token" 后，
        // LLM 指标行不再被误报为安全信号
        let s = detect("input_tokens=512 output_tokens=256", ImportanceContext::Diff);
        assert!(!s.is_match());
    }

    #[test]
    fn auth_still_flags_security_in_diff() {
        let s = detect("missing auth header", ImportanceContext::Diff);
        assert_eq!(s.category, Some(ImportanceCategory::Security));
        assert_eq!(s.priority, SECURITY_PRIORITY);
    }

    #[test]
    fn security_does_not_fire_outside_diff() {
        // security 只在 Diff 上下文生效
        let s = detect("missing auth header", ImportanceContext::Log);
        assert!(!s.is_match());
    }

    #[test]
    fn warning_fires_in_search_but_not_diff() {
        let in_search = detect("warning: deprecated API", ImportanceContext::Search);
        assert_eq!(in_search.category, Some(ImportanceCategory::Warning));

        // Python 版 PRIORITY_PATTERNS_DIFF 不含 WARNING_PATTERN；保持一致
        let in_diff = detect(
            "warning: deprecated API alone with no errors",
            ImportanceContext::Diff,
        );
        assert_ne!(in_diff.category, Some(ImportanceCategory::Warning));
    }

    #[test]
    fn markdown_header_fires_only_in_text() {
        let prefix_only = detect("# Section", ImportanceContext::Text);
        assert_eq!(prefix_only.category, Some(ImportanceCategory::Markdown));
        assert_eq!(prefix_only.priority, MARKDOWN_PRIORITY);
        let same_line_in_diff = detect("# Section", ImportanceContext::Diff);
        assert!(!same_line_in_diff.is_match());
    }

    #[test]
    fn markdown_prefixes_cover_bold_and_quote() {
        assert_eq!(
            detect("**key point**", ImportanceContext::Text).category,
            Some(ImportanceCategory::Markdown)
        );
        assert_eq!(
            detect("> quoted text", ImportanceContext::Text).category,
            Some(ImportanceCategory::Markdown)
        );
    }

    #[test]
    fn importance_keywords_fire_universally() {
        let s = detect("TODO: refactor this later", ImportanceContext::Log);
        assert_eq!(s.category, Some(ImportanceCategory::Importance));
        assert_eq!(s.priority, IMPORTANCE_PRIORITY);
    }

    #[test]
    fn error_outranks_importance_on_same_line() {
        // "error" 与 "fix" 同行：error 优先
        let s = detect("fix the error handler", ImportanceContext::Search);
        assert_eq!(s.category, Some(ImportanceCategory::Error));
    }

    #[test]
    fn word_boundary_excludes_substring_matches() {
        // 词边界检查："panicker" 内的 "panic" 不是独立词，不应命中
        let s = detect("the panicker showed up late", ImportanceContext::Search);
        assert!(!s.is_match());
    }

    #[test]
    fn warning_word_beats_warn_substring() {
        // "warning" 整词命中 Warning，而不是被 "warn" 抢先
        let s = detect("WARNING: deprecation", ImportanceContext::Search);
        assert_eq!(s.category, Some(ImportanceCategory::Warning));
    }

    #[test]
    fn neutral_line_returns_zero_confidence() {
        let s = detect("the quick brown fox", ImportanceContext::Text);
        assert!(!s.is_match());
        assert_eq!(s.confidence, 0.0);
        assert_eq!(s.priority, 0.0);
    }

    #[test]
    fn contains_error_indicator_is_lax_substring_match() {
        // 保留 Python `content_has_error_indicators` 语义：
        // "errored" 命中 "error"（快速分诊故意宽松；严格版是 `score()`）
        let det = KeywordDetector::new();
        assert!(det.contains_error_indicator("the request errored out"));
        assert!(det.contains_error_indicator("Traceback follows"));
        assert!(!det.contains_error_indicator("everything is fine"));
    }

    #[test]
    fn registry_snapshot_has_token_dropped() {
        let reg = KeywordRegistry::default_set();
        assert!(!reg.security.contains(&"token"));
        assert!(reg.security.contains(&"auth"));
        assert!(reg.error.contains(&"timeout"));
        assert!(reg.error.contains(&"abort"));
        // 快照接口
        let map = reg.as_map();
        assert!(map.contains_key("error"));
        assert_eq!(map["markdown_prefixes"].len(), 6);
    }

    #[test]
    fn signal_constructors() {
        let n = ImportanceSignal::neutral();
        assert!(!n.is_match());
        let m = ImportanceSignal::matched(ImportanceCategory::Error, 0.9, 0.8);
        assert!(m.is_match());
        assert_eq!(m.category, Some(ImportanceCategory::Error));
    }

    // ─── 分层组合 ───────────────────────────────────────────────────

    /// 测试短路行为的高置信度合成检测器：固定输出 Security 信号，
    /// 用以证明 Tiered 先于关键词层咨询它。
    struct AlwaysFiresHigh;
    impl LineImportanceDetector for AlwaysFiresHigh {
        fn score(&self, _line: &str, _ctx: ImportanceContext) -> ImportanceSignal {
            ImportanceSignal::matched(ImportanceCategory::Security, 0.99, 0.95)
        }
    }

    /// 低置信度合成检测器：置信度 0.5 低于阈值，Tiered 必须落到下一层。
    struct AlwaysFiresLow;
    impl LineImportanceDetector for AlwaysFiresLow {
        fn score(&self, _line: &str, _ctx: ImportanceContext) -> ImportanceSignal {
            ImportanceSignal::matched(ImportanceCategory::Importance, 0.4, 0.5)
        }
    }

    #[test]
    fn high_confidence_tier_short_circuits() {
        let tiered = Tiered::new()
            .with_detector(AlwaysFiresHigh)
            .with_detector(KeywordDetector::new());
        let s = tiered.score("ERROR: connection refused", ImportanceContext::Diff);
        // AlwaysFiresHigh 报 Security；若关键词层跑了会报 Error
        assert_eq!(s.category, Some(ImportanceCategory::Security));
    }

    #[test]
    fn low_confidence_tier_falls_through_to_keyword() {
        let tiered = Tiered::new()
            .with_detector(AlwaysFiresLow)
            .with_detector(KeywordDetector::new());
        let s = tiered.score("ERROR: connection refused", ImportanceContext::Diff);
        assert_eq!(s.category, Some(ImportanceCategory::Error));
    }

    #[test]
    fn no_tier_matches_returns_best_seen() {
        let tiered = Tiered::new()
            .with_detector(AlwaysFiresLow)
            .with_detector(KeywordDetector::new());
        let s = tiered.score("the quick brown fox", ImportanceContext::Text);
        // 关键词层 neutral（置信度 0.0）；AlwaysFiresLow 的 Importance @0.5
        // 作为“见到的最佳”胜出
        assert_eq!(s.category, Some(ImportanceCategory::Importance));
        assert_eq!(s.confidence, 0.5);
    }

    #[test]
    fn empty_stack_returns_neutral() {
        let tiered = Tiered::new();
        assert!(tiered.is_empty());
        let s = tiered.score("anything", ImportanceContext::Text);
        assert!(!s.is_match());
    }

    #[test]
    fn tiered_reports_len() {
        let tiered = Tiered::new().with_detector(KeywordDetector::new());
        assert_eq!(tiered.len(), 1);
        assert!(!tiered.is_empty());
    }

    #[test]
    fn tiered_is_send_sync() {
        // 编译期断言：压缩器跨线程共享检测器实例
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<KeywordDetector>();
        assert_send_sync::<Tiered>();
        assert_send_sync::<dyn LineImportanceDetector>();
    }

    #[test]
    fn priority_for_covers_all_categories() {
        assert_eq!(
            priority_for(ImportanceCategory::Error),
            ERROR_PRIORITY
        );
        assert_eq!(
            priority_for(ImportanceCategory::Warning),
            WARNING_PRIORITY
        );
        assert!(ERROR_PRIORITY > SECURITY_PRIORITY);
        assert!(SECURITY_PRIORITY > WARNING_PRIORITY);
        assert!(WARNING_PRIORITY > IMPORTANCE_PRIORITY);
        assert!(IMPORTANCE_PRIORITY > MARKDOWN_PRIORITY);
    }
}
