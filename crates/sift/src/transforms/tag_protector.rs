//! 自定义 XML 标签保护/恢复（TagProtector）。

//!
//! # 为什么需要
//!
//! LLM 消息里经常夹带应用自定义的 XML 标签（`<tool_output>`、`<result>`、
//! `<thinking>`、`<system-reminder>` 等），这些标签是下游代码解析的结构标记。
//! 文本压缩器把它们当可丢弃的噪声剥离，破坏一切依赖它们的逻辑。本模块在
//! 压缩前把已知标签替换成不透明占位符（protect），压缩后把原文拼接回去
//! （restore）。
//!
//! 标准 HTML5 元素（`<div>`、`<p>`、`<span>` 等）**不**保护 —— 它们走
//! HTML 提取层，不属于本模块职责；其余一律视为自定义标签。
//!
//! # 算法
//!
//! 对输入字节做单遍 tag-stack walker（无 regex 回溯、无 O(n²) 重启循环）：
//!
//! 1. 向前扫描 `<`。若后续字节构成合法开标签（`<name attr=…>` 或 `<name/>`），
//!    归类标签名。
//! 2. HTML 标签 → 原样输出，继续。
//! 3. 自定义自闭合标签 → 输出一个占位符，记录区间。
//! 4. 自定义开标签 → 把 `(name, start_offset)` 压栈。
//! 5. `</name>` 匹配栈顶 → 弹栈，块模式下把整个 `<name>…</name>` 区间替换为
//!    单个占位符；标记模式（`compress_tagged_content == true`）下只替换开/闭
//!    标记本身。
//! 6. 不匹配的闭标签（HTML 闭标签落在自定义栈顶之上，或无对应开标签的闭标签）
//!    → 原样输出并继续。walker 从不试图「修复」畸形输入。
//!
//! 输出按字节偏移增量切片拼接 —— 绝不使用 Python 原版的
//! `result.replace(original, placeholder, 1)`，后者在输入中出现两个相同标签块时
//! 会替换*第一个*文本匹配而非匹配到的偏移（见 `duplicate_blocks_get_distinct_placeholders`）。
//!
//! # 与参考实现的差异
//!
//! - 无 `tracing` 依赖：占位符丢失时不再发结构化日志，仅按 Hotfix-A9 语义
//!   静默丢弃整段 wrap（不注入孤儿标签、不追加、不前置）。
//! - 提供 `TagProtector` 结构体 + 配置：可指定「只保护已知标签集合」
//!   （`protected_tags`），未知标签原样不动；默认 `None` 即参考实现行为
//!   （保护所有非 HTML 标签）。

use std::collections::HashSet;
use std::sync::OnceLock;

/// HTML5 living-standard 元素名 —— 本模块**永不**保护的标签集合（它们在
/// 另一层处理；其余一律视为自定义标签）。
///
/// 取自 <https://html.spec.whatwg.org/multipage/indices.html#elements-3>，
/// 与参考实现的 `KNOWN_HTML_TAGS` 逐元素一致。
const HTML5_TAGS: &[&str] = &[
    // Main root
    "html",
    // Document metadata
    "base",
    "head",
    "link",
    "meta",
    "style",
    "title",
    // Sectioning root
    "body",
    // Content sectioning
    "address",
    "article",
    "aside",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "main",
    "nav",
    "section",
    "search",
    // Text content
    "blockquote",
    "dd",
    "div",
    "dl",
    "dt",
    "figcaption",
    "figure",
    "hr",
    "li",
    "menu",
    "ol",
    "p",
    "pre",
    "ul",
    // Inline text semantics
    "a",
    "abbr",
    "b",
    "bdi",
    "bdo",
    "br",
    "cite",
    "code",
    "data",
    "dfn",
    "em",
    "i",
    "kbd",
    "mark",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "small",
    "span",
    "strong",
    "sub",
    "sup",
    "time",
    "u",
    "var",
    "wbr",
    // Image and multimedia
    "area",
    "audio",
    "img",
    "map",
    "track",
    "video",
    // Embedded content
    "embed",
    "iframe",
    "object",
    "param",
    "picture",
    "portal",
    "source",
    // SVG and MathML
    "svg",
    "math",
    // Scripting
    "canvas",
    "noscript",
    "script",
    // Demarcating edits
    "del",
    "ins",
    // Table content
    "caption",
    "col",
    "colgroup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    // Forms
    "button",
    "datalist",
    "fieldset",
    "form",
    "input",
    "label",
    "legend",
    "meter",
    "optgroup",
    "option",
    "output",
    "progress",
    "select",
    "textarea",
    // Interactive
    "details",
    "dialog",
    "summary",
    // Web Components
    "slot",
    "template",
];

fn known_html_tags() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| HTML5_TAGS.iter().copied().collect())
}

/// 默认占位符前缀。双花括号让它看起来不像真实工作流标签会产出的东西。
/// 若输入本身包含该前缀，则改用加盐前缀（见 [`pick_placeholder_prefix`]）。
const DEFAULT_PREFIX: &str = "{{CMPR_TAG_";
const PLACEHOLDER_SUFFIX: &str = "}}";

/// 占位符 → 原文 的映射列表（按输入中从左到右的出现顺序）。
/// 传给 [`restore_tags`] 以恢复被保护区间。
pub type TagMap = Vec<(String, String)>;

/// 保护过程的旁路诊断计数 —— 与其他变换的 stats 结构同构。
#[derive(Debug, Default, Clone)]
pub struct ProtectStats {
    /// 扫描到的标签总数（开 + 闭 + 自闭合）。
    pub tags_seen: usize,
    /// 因属 HTML5 标签（或不在受保护集合内）而跳过的标签数。
    pub html_tags_skipped: usize,
    /// 块模式下被保护的整个 `<custom>…</custom>` 块数。
    pub custom_blocks_protected: usize,
    /// 被保护的自闭合自定义标签数。
    pub self_closing_protected: usize,
    /// 没匹配到任何栈内开标签的闭标签数（畸形输入或 HTML 交错）。
    /// 原样输出。非零是值得追踪的气味，但不必然是 bug。
    pub orphan_closes: usize,
    /// 占位符前缀是否因输入包含字面 `{{CMPR_TAG_` 而加盐。
    pub placeholder_collision_avoided: bool,
}

/// 保护配置。
#[derive(Debug, Clone, Default)]
pub struct TagProtectorConfig {
    /// 需要保护的自定义标签集合（内部统一小写，比较时大小写不敏感）。
    ///
    /// - `None`（默认）：保护**所有**非 HTML5 标签（与参考实现一致，
    ///   因为自定义标签无法枚举）；
    /// - `Some(set)`：只保护集合内的标签，其余自定义标签与 HTML 标签一样
    ///   原样输出（对应「只保护已知标签、未知标签不动」的场景）。
    pub protected_tags: Option<HashSet<String>>,
    /// 是否压缩被保护标签内部的内容（标记模式）。
    ///
    /// - `false`（默认）：整个 `<custom>…</custom>` 块（含嵌套子元素）替换为
    ///   单个占位符，正文不暴露给压缩器；
    /// - `true`：只把开/闭标记分别替换为占位符，正文保留给压缩器压缩。
    pub compress_tagged_content: bool,
}

impl TagProtectorConfig {
    /// 只保护给定标签集合（大小写不敏感），其余标签原样输出。
    pub fn with_protected_tags<I, S>(tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            protected_tags: Some(
                tags.into_iter()
                    .map(|s| s.into().to_ascii_lowercase())
                    .collect(),
            ),
            compress_tagged_content: false,
        }
    }

    /// 标记模式：只替换开/闭标签标记，正文保留给压缩器。
    pub fn with_marker_only(mut self) -> Self {
        self.compress_tagged_content = true;
        self
    }
}

/// 自定义标签保护器。
///
/// 用法：
/// ```ignore
/// let protector = TagProtector::default();
/// let (cleaned, tag_map) = protector.protect(input);
/// let compressed = compress(cleaned); // 任何文本压缩器
/// let restored = protector.restore(&compressed, &tag_map);
/// ```
#[derive(Debug, Clone, Default)]
pub struct TagProtector {
    config: TagProtectorConfig,
}

impl TagProtector {
    pub fn new(config: TagProtectorConfig) -> Self {
        Self { config }
    }

    /// 读取当前配置。
    pub fn config(&self) -> &TagProtectorConfig {
        &self.config
    }

    /// 保护输入中的自定义标签，返回 `(清洗后文本, 占位符映射)`。
    pub fn protect(&self, input: &str) -> (String, TagMap) {
        let (cleaned, blocks, _stats) = protect_impl(input, &self.config);
        (cleaned, blocks)
    }

    /// 同 [`TagProtector::protect`]，额外返回 [`ProtectStats`] 诊断。
    pub fn protect_with_stats(&self, input: &str) -> (String, TagMap, ProtectStats) {
        protect_impl(input, &self.config)
    }

    /// 压缩完成后，把占位符恢复为原文。
    pub fn restore(&self, protected_text: &str, tag_map: &[(String, String)]) -> String {
        restore_impl(protected_text, tag_map)
    }
}

/// 便捷入口：用默认配置保护自定义标签（块模式，保护所有非 HTML 标签）。
pub fn protect_tags(input: &str) -> (String, TagMap) {
    TagProtector::default().protect(input)
}

/// 便捷入口：压缩后恢复占位符。
pub fn restore_tags(protected_text: &str, tag_map: &[(String, String)]) -> String {
    TagProtector::default().restore(protected_text, tag_map)
}

/// 大小写不敏感的 HTML 标签判断。惰性小写，常见全小写 ASCII 场景不分配。
pub fn is_known_html_tag(tag_name: &str) -> bool {
    let set = known_html_tags();
    if set.contains(tag_name) {
        return true;
    }
    if tag_name.bytes().any(|b| b.is_ascii_uppercase()) {
        let lower = tag_name.to_ascii_lowercase();
        return set.contains(lower.as_str());
    }
    false
}

/// 迭代规范 HTML 标签列表（供上层桥接暴露 `KNOWN_HTML_TAGS` 时复用）。
pub fn known_html_tag_names() -> &'static [&'static str] {
    HTML5_TAGS
}

/// 判断某个自定义标签是否应当被保护（未命中 HTML5 且命中受保护集合）。
fn should_protect(name: &str, config: &TagProtectorConfig) -> bool {
    match &config.protected_tags {
        None => true,
        Some(set) => {
            if set.contains(name) {
                return true;
            }
            if name.bytes().any(|b| b.is_ascii_uppercase()) {
                return set.contains(name.to_ascii_lowercase().as_str());
            }
            false
        }
    }
}

/// 挑选不与 `text` 中任何内容冲突的占位符前缀。先试 `{{CMPR_TAG_`；
/// 若输入字面包含它则按调用级计数器加盐直到不冲突。盐值有界，实际几乎
/// 用不到第二次尝试。
fn pick_placeholder_prefix(text: &str) -> (String, bool) {
    if !text.contains(DEFAULT_PREFIX) {
        return (DEFAULT_PREFIX.to_string(), false);
    }
    for salt in 0u32..16 {
        let candidate = format!("{{{{CMPR_TAG_{salt}_");
        if !text.contains(&candidate) {
            return (candidate, true);
        }
    }
    // 16 次加盐都冲突 —— 退回固定 UUID 形状标记。OnceLock 缓存避免同进程
    // 内连续调用重复付格式化成本。
    static FALLBACK: OnceLock<String> = OnceLock::new();
    let prefix = FALLBACK
        .get_or_init(|| "{{CMPR_TAG_FALLBACK_a4f1c7e2_".to_string())
        .clone();
    (prefix, true)
}

#[derive(Debug)]
struct OpenTag {
    /// 小写标签名，用于闭标签大小写不敏感匹配。
    name_lower: String,
    /// 打开此标签的 `<` 的字节偏移。
    open_start: usize,
}

/// 在给定偏移处解析一次 `<…>` 的结果。
enum TagParse {
    /// 开标签（`<name attr=…>`）。`name_end` 为开区间。
    Open {
        name_end: usize,
        tag_end: usize,
        is_self_closing: bool,
    },
    /// 闭标签（`</name>`）。
    Close { name_end: usize, tag_end: usize },
    /// 不是标签（如 `<` 后跟空白或数字）。
    NotTag,
}

/// 从 `start` 开始解析一个 `<…>`。返回闭 `>` 的字节偏移（标签的排他终点）
/// 与类别。对畸形形状保守拒绝 —— 宁可原样输出一个 `<`，也不在坏输入上
/// 过度保护。
fn parse_tag_at(bytes: &[u8], start: usize) -> TagParse {
    debug_assert!(bytes[start] == b'<');
    let mut i = start + 1;
    let n = bytes.len();
    if i >= n {
        return TagParse::NotTag;
    }

    let is_close = bytes[i] == b'/';
    if is_close {
        i += 1;
    }
    // 消费可能存在的 '/' 后可能已到输入末尾（如字面 `</`）。在索引
    // `bytes[i]` 检查 name-start 前先做边界保护 —— proptest 在输入 `</`
    // 上发现了 OOB。
    if i >= n {
        return TagParse::NotTag;
    }
    let name_start = i;
    if !is_name_start(bytes[i]) {
        return TagParse::NotTag;
    }
    i += 1;
    while i < n && is_name_cont(bytes[i]) {
        i += 1;
    }
    let name_end = i;
    if name_end == name_start {
        return TagParse::NotTag;
    }

    if is_close {
        // 允许可选空白，然后是 `>`。
        while i < n && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= n || bytes[i] != b'>' {
            return TagParse::NotTag;
        }
        return TagParse::Close {
            name_end,
            tag_end: i + 1,
        };
    }

    // 开标签：跳过属性直到 `>`（处理自闭合的 `/>`）。引号内属性值可含 `>`；
    // 单遍属性词法器覆盖常见情形。
    let mut self_closing = false;
    while i < n {
        match bytes[i] {
            b'>' => {
                return TagParse::Open {
                    name_end,
                    tag_end: i + 1,
                    is_self_closing: self_closing,
                };
            }
            b'/' => {
                self_closing = true;
                i += 1;
            }
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                while i < n && bytes[i] != quote {
                    i += 1;
                }
                if i >= n {
                    return TagParse::NotTag;
                }
                i += 1;
                self_closing = false;
            }
            _ => {
                if bytes[i].is_ascii_whitespace() {
                    self_closing = false;
                }
                i += 1;
            }
        }
    }

    TagParse::NotTag
}

#[inline]
fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

#[inline]
fn is_name_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':')
}

/// 一个被识别为值得替换的区间。
///
/// 块模式下每个匹配的自定义标签区间（open..=close）成为一个 Span 并被单个
/// 占位符替换；自闭合自定义标签成为一个仅覆盖标签字节的 Span。
///
/// 标记模式下每个开标签和每个闭标签各自成为一个 Span（两者之间的正文保持
/// 对压缩器可见）。
#[derive(Debug, Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
    kind: SpanKind,
}

#[derive(Debug, Clone, Copy)]
enum SpanKind {
    /// 整个 `<custom>…</custom>` 块（块模式）。
    Block,
    /// 自闭合 `<custom/>`（块模式）。
    SelfClosing,
    /// 开标签 `<custom>` 标记（标记模式）。
    OpenMarker,
    /// 闭标签 `</custom>` 标记（标记模式）。
    CloseMarker,
}

fn protect_impl(text: &str, config: &TagProtectorConfig) -> (String, TagMap, ProtectStats) {
    let mut stats = ProtectStats::default();
    if text.is_empty() || !text.contains('<') {
        return (text.to_string(), Vec::new(), stats);
    }

    let (prefix, salted) = pick_placeholder_prefix(text);
    stats.placeholder_collision_avoided = salted;

    // 阶段一：单遍扫描，归类每个标签，产出值得替换的区间列表。此阶段不产出
    // 任何输出 —— 纯发现，以便决定要交换哪些字节区间。
    let spans = identify_spans(text, config, &mut stats);

    // 阶段二：产出。再走一遍输入，为区间字节拼接占位符，其余原样复制。
    // `spans` 按从左到右排序且不重叠（块模式把嵌套匹配折叠进最外层区间；
    // 标记模式产出的开/闭标记按构造字节不相交），因此是直接扫描。
    match emit_output(text, &spans, &prefix) {
        Some((cleaned, blocks)) => (cleaned, blocks, stats),
        // 理论上不可达 —— `identify_spans` 返回的区间都是 `text` 的切片。
        // 若拼接失败，退回输出原文。
        None => (text.to_string(), Vec::new(), stats),
    }
}

fn identify_spans(text: &str, config: &TagProtectorConfig, stats: &mut ProtectStats) -> Vec<Span> {
    let bytes = text.as_bytes();
    let n = bytes.len();
    let mut spans: Vec<Span> = Vec::new();
    let mut stack: Vec<OpenTag> = Vec::new();

    let mut i = 0;
    while i < n {
        let b = bytes[i];
        if b != b'<' {
            // 跳到下一个 `<`。非标签字节与区间识别无关，阶段二会原样复制。
            i = memchr(b'<', &bytes[i..]).map(|j| i + j).unwrap_or(n);
            continue;
        }

        match parse_tag_at(bytes, i) {
            TagParse::NotTag => {
                i += 1;
            }
            TagParse::Open {
                name_end,
                tag_end,
                is_self_closing,
            } => {
                stats.tags_seen += 1;
                let name = &text[i + 1..name_end];
                if is_known_html_tag(name) || !should_protect(name, config) {
                    stats.html_tags_skipped += 1;
                    i = tag_end;
                    continue;
                }
                if is_self_closing {
                    spans.push(Span {
                        start: i,
                        end: tag_end,
                        kind: SpanKind::SelfClosing,
                    });
                    stats.self_closing_protected += 1;
                    i = tag_end;
                    continue;
                }
                if config.compress_tagged_content {
                    // 标记模式：开标签作为独立区间，同时压栈以便闭标签被
                    // 匹配并作为独立区间输出。
                    spans.push(Span {
                        start: i,
                        end: tag_end,
                        kind: SpanKind::OpenMarker,
                    });
                }
                // 两种模式都压栈，闭标签匹配依赖它。
                stack.push(OpenTag {
                    name_lower: name.to_ascii_lowercase(),
                    open_start: i,
                });
                i = tag_end;
            }
            TagParse::Close { name_end, tag_end } => {
                stats.tags_seen += 1;
                let close_name = &text[i + 2..name_end];
                if is_known_html_tag(close_name) || !should_protect(close_name, config) {
                    stats.html_tags_skipped += 1;
                    i = tag_end;
                    continue;
                }
                let close_name_lower = close_name.to_ascii_lowercase();
                let matching = stack
                    .iter()
                    .rposition(|open| open.name_lower == close_name_lower);

                match matching {
                    Some(stack_idx) => {
                        if config.compress_tagged_content {
                            // 弹出其上的所有元素（匹配区间内的孤儿开标签 ——
                            // 它们的开标记已记录为区间，予以保留）。
                            stack.truncate(stack_idx);
                            let _ = stack.pop();
                            spans.push(Span {
                                start: i,
                                end: tag_end,
                                kind: SpanKind::CloseMarker,
                            });
                        } else {
                            // 块模式：把 [open..close] 折叠为单个区间。丢弃
                            // 区间内未匹配的开标签（它们是本区间正文的一部分）。
                            // 同时丢弃已记录、现被此外层块吞并的内层区间 ——
                            // 嵌套自定义标签由此折叠为单个占位符。
                            let open_start = stack[stack_idx].open_start;
                            stack.truncate(stack_idx);
                            spans.retain(|s| s.start < open_start);
                            spans.push(Span {
                                start: open_start,
                                end: tag_end,
                                kind: SpanKind::Block,
                            });
                            stats.custom_blocks_protected += 1;
                        }
                        i = tag_end;
                    }
                    None => {
                        stats.orphan_closes += 1;
                        i = tag_end;
                    }
                }
            }
        }
    }

    // 栈内残留是孤儿开标签（从未等到匹配闭标签）。不保护它们 —— 它们会以
    // 原始 `<name>` 字节落到压缩器，与 Python 原版行为一致。块模式下它们
    // 内侧已记录的自闭合区间仍安全保留：位于未匹配外层开标签之下，从未被
    // 折叠。由于 walk 单调，区间按 start 升序排列；阶段二依赖此性质。
    spans
}

fn emit_output(text: &str, spans: &[Span], prefix: &str) -> Option<(String, TagMap)> {
    let mut out = String::with_capacity(text.len());
    let mut blocks: TagMap = Vec::new();
    let mut cursor: usize = 0;

    for (counter, span) in (0_u64..).zip(spans.iter()) {
        if span.start < cursor {
            // 按我们的嵌套折叠方式不应出现重叠，但若出现则大声失败 ——
            // 静默产出错误输出比测试失败更糟。
            return None;
        }
        out.push_str(&text[cursor..span.start]);
        let placeholder = format!("{prefix}{counter}{PLACEHOLDER_SUFFIX}");
        let original = &text[span.start..span.end];
        blocks.push((placeholder.clone(), original.to_string()));
        out.push_str(&placeholder);
        cursor = span.end;
        let _ = span.kind; // SpanKind 在此层仅作信息用途
    }
    out.push_str(&text[cursor..]);
    Some((out, blocks))
}

/// 压缩器处理完清洗文本后，恢复被保护的标签区间。
///
/// # Hotfix-A9 —— 丢弃 wrap 语义
///
/// 若某个占位符在压缩中丢失（被压缩器剥离或改写），整段 wrap 被**丢弃**：
/// 压缩后的文本原样流出，原标签字节**不会**被重新注入任何位置。这是相对
/// 「把孤儿标签追加到尾部」回退的有意行为变化 —— 后者会在生产上产出静默
/// 畸形的 XML（只有开标签没有闭标签没有正文）。
///
/// 不变量：
/// 1. 对称性 —— 绝不输出不对称的标签计数（要么通过成功替换同时保住开闭，
///    要么通过丢弃同时失去开闭）。
/// 2. 不注入孤儿标签 —— `restore` 只通过占位符替换增加字节，无追加、无前置、
///    无占位符替换之外的空白插入。
/// 3. 缺占位符时的幂等性 —— 若所有占位符都缺席，函数逐字节原样返回输入。
fn restore_impl(text: &str, blocks: &[(String, String)]) -> String {
    if blocks.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();
    for (placeholder, original) in blocks {
        if result.contains(placeholder.as_str()) {
            result = result.replace(placeholder.as_str(), original);
        }
        // 占位符丢失 → 丢弃整段 wrap（不注入孤儿标签），Hotfix-A9 语义。
        // 无 tracing 依赖，此处不记日志；调用方可凭 stats/对比自行告警。
    }
    result
}

// ─── 微小字节搜索辅助 ──────────────────────────────────────────────────

#[inline]
fn memchr(needle: u8, haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|&b| b == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protect(text: &str) -> (String, TagMap) {
        protect_tags(text)
    }

    fn marker_only() -> TagProtector {
        TagProtector::new(TagProtectorConfig::default().with_marker_only())
    }

    #[test]
    fn passthrough_when_no_angle_bracket() {
        let (cleaned, blocks) = protect("Just plain text");
        assert_eq!(cleaned, "Just plain text");
        assert!(blocks.is_empty());
    }

    #[test]
    fn html_tags_emitted_verbatim() {
        let text = "<div>Some content</div>";
        let (cleaned, blocks) = protect(text);
        assert_eq!(cleaned, text);
        assert!(blocks.is_empty());
    }

    #[test]
    fn html_tag_check_case_insensitive() {
        assert!(is_known_html_tag("DIV"));
        assert!(is_known_html_tag("Span"));
        assert!(!is_known_html_tag("system-reminder"));
        assert!(!is_known_html_tag("EXTREMELY_IMPORTANT"));
    }

    #[test]
    fn custom_tag_replaced_with_placeholder() {
        let text = "Before <system-reminder>Important</system-reminder> After";
        let (cleaned, blocks) = protect(text);
        assert!(!cleaned.contains("<system-reminder>"));
        assert!(!cleaned.contains("Important"));
        assert!(cleaned.contains("Before"));
        assert!(cleaned.contains("After"));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, "<system-reminder>Important</system-reminder>");
    }

    #[test]
    fn custom_tag_with_attributes() {
        let text = r#"<context key="session" type="persistent">user data</context>"#;
        let (_cleaned, blocks) = protect(text);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].1.contains(r#"key="session""#));
    }

    #[test]
    fn self_closing_custom_tag() {
        let text = "Text <marker/> more text";
        let (_cleaned, blocks) = protect(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, "<marker/>");
    }

    #[test]
    fn self_closing_html_tag_not_protected() {
        let text = "Text <br/> more <hr/> text";
        let (cleaned, blocks) = protect(text);
        assert_eq!(cleaned, text);
        assert!(blocks.is_empty());
    }

    #[test]
    fn nested_custom_tags_collapse_to_outer_span() {
        let text = "<outer><inner>deep</inner></outer>";
        let (cleaned, blocks) = protect(text);
        assert!(!cleaned.contains("<outer>"));
        assert!(!cleaned.contains("<inner>"));
        // 外层区间吞并内层 —— 单个占位符。
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, "<outer><inner>deep</inner></outer>");
    }

    #[test]
    fn mixed_html_and_custom() {
        let text = "<div>HTML</div> <system-reminder>Rule</system-reminder> <p>HTML2</p>";
        let (cleaned, blocks) = protect(text);
        assert!(cleaned.contains("<div>"));
        assert!(cleaned.contains("<p>"));
        assert!(!cleaned.contains("<system-reminder>"));
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn real_workflow_tags() {
        let cases = [
            "<tool_call>search({query: 'test'})</tool_call>",
            "<thinking>Let me analyze this</thinking>",
            "<EXTREMELY_IMPORTANT>Never skip validation</EXTREMELY_IMPORTANT>",
            "<user-prompt-submit-hook>check perms</user-prompt-submit-hook>",
            "<system-reminder>Rules</system-reminder>",
            "<result>Success: 42 items</result>",
        ];
        for tag in cases {
            let text = format!("Before {tag} After");
            let (_cleaned, blocks) = protect(&text);
            assert_eq!(blocks.len(), 1, "failed to protect: {tag}");
            assert_eq!(blocks[0].1, tag);
        }
    }

    #[test]
    fn empty_input_returns_empty() {
        let (cleaned, blocks) = protect("");
        assert!(cleaned.is_empty());
        assert!(blocks.is_empty());
    }

    #[test]
    fn marker_only_mode_emits_marker_placeholders() {
        let text = "Before <system-reminder>Compressible content</system-reminder> After";
        let (cleaned, blocks, _stats) = marker_only().protect_with_stats(text);
        assert!(!cleaned.contains("<system-reminder>"));
        assert!(!cleaned.contains("</system-reminder>"));
        assert!(cleaned.contains("Compressible content"));
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn restore_basic() {
        let original = "Before <system-reminder>Rule</system-reminder> After";
        let (cleaned, blocks) = protect(original);
        let restored = restore_tags(&cleaned, &blocks);
        assert_eq!(restored, original);
    }

    #[test]
    fn restore_empty_blocks_passthrough() {
        assert_eq!(restore_tags("untouched", &[]), "untouched");
    }

    #[test]
    fn restore_lost_placeholder_discards_wrap() {
        // Hotfix-A9：占位符从压缩文本中丢失时，wrap 被丢弃 —— 压缩文本原样
        // 返回，不注入孤儿标签。
        let blocks = vec![("{{CMPR_TAG_0}}".to_string(), "<tag>data</tag>".to_string())];
        let compressed = "text without placeholder";
        let restored = restore_tags(compressed, &blocks);
        assert_eq!(restored, compressed);
        assert!(!restored.contains("<tag>"));
        assert!(!restored.contains("</tag>"));
    }

    #[test]
    fn restore_partial_loss_keeps_present_drops_lost() {
        let blocks = vec![
            ("{{CMPR_TAG_0}}".to_string(), "<a>1</a>".to_string()),
            ("{{CMPR_TAG_1}}".to_string(), "<lost>x</lost>".to_string()),
        ];
        let compressed = "head {{CMPR_TAG_0}} tail";
        let restored = restore_tags(compressed, &blocks);
        assert_eq!(restored, "head <a>1</a> tail");
        assert!(!restored.contains("<lost"));
        assert!(!restored.contains("</lost>"));
    }

    #[test]
    fn restore_roundtrip_preserves_content() {
        let original = "Start <system-reminder>Rule 1: validate</system-reminder> middle \
             <tool_call>search(q='test')</tool_call> end";
        let (cleaned, blocks) = protect(original);
        let restored = restore_tags(&cleaned, &blocks);
        assert_eq!(restored, original);
    }

    // ─── 保护 → 压缩 → 恢复 的完整 roundtrip ────────────────────────────

    #[test]
    fn protect_compress_restore_roundtrip() {
        let input = "Intro text <result>42 items found</result> tail text";
        let (protected, tag_map) = protect_tags(input);
        // 模拟一个「压缩器」：改写正文长词，但占位符原样保留。
        let compressed = protected
            .replace("Intro text", "intro")
            .replace("tail text", "tail");
        let restored = restore_tags(&compressed, &tag_map);
        assert_eq!(restored, "intro <result>42 items found</result> tail");
        assert!(restored.contains("<result>42 items found</result>"));
    }

    // ─── 非 ASCII 内容 ──────────────────────────────────────────────────

    #[test]
    fn non_ascii_content_roundtrip() {
        let input = "前缀 <result>你好，世界！🚀 找到 42 条结果</result> 后缀";
        let (protected, tag_map) = protect(input);
        assert!(!protected.contains("<result>"));
        assert!(!protected.contains("你好"));
        let restored = restore_tags(&protected, &tag_map);
        assert_eq!(restored, input);
    }

    #[test]
    fn non_ascii_attribute_roundtrip() {
        let input = r#"<tool_output lang="中文" note='数值'>有效负载</tool_output>"#;
        let (protected, tag_map) = protect(input);
        assert_eq!(tag_map.len(), 1);
        assert_eq!(tag_map[0].1, input);
        assert_eq!(restore_tags(&protected, &tag_map), input);
    }

    // ─── 未知标签不动（protect_only 配置） ──────────────────────────────

    #[test]
    fn unknown_tags_unchanged_with_protect_only_config() {
        let protector = TagProtector::new(TagProtectorConfig::with_protected_tags(["result"]));
        let input = "<result>keep me</result> <other>leave me</other>";
        let (protected, tag_map) = protector.protect(input);
        assert!(!protected.contains("<result>"));
        assert!(protected.contains("<other>leave me</other>"));
        assert_eq!(tag_map.len(), 1);
        assert_eq!(protector.restore(&protected, &tag_map), input);
    }

    #[test]
    fn protect_only_is_case_insensitive() {
        let protector = TagProtector::new(TagProtectorConfig::with_protected_tags(["Result"]));
        let input = "<result>a</result> <RESULT>b</RESULT> <other>c</other>";
        let (protected, tag_map) = protector.protect(input);
        assert!(!protected.contains("<result>"));
        assert!(!protected.contains("<RESULT>"));
        assert!(protected.contains("<other>c</other>"));
        assert_eq!(tag_map.len(), 2);
    }

    // ─── bug 修复测试（对应参考实现 fixed_in_3e4） ──────────────────────

    #[test]
    fn duplicate_blocks_get_distinct_placeholders() {
        // Bug #2：`str.replace(.., .., 1)` 替换第一个文本匹配而非匹配偏移。
        // 两个相同标签块必须各自换成不同占位符。
        let text = "<system-reminder>same</system-reminder> middle \
             <system-reminder>same</system-reminder>";
        let (cleaned, blocks) = protect(text);
        assert_eq!(blocks.len(), 2);
        assert!(!cleaned.contains("<system-reminder>"));
        assert!(!cleaned.contains("</system-reminder>"));
        assert_ne!(blocks[0].0, blocks[1].0);
        assert_eq!(restore_tags(&cleaned, &blocks), text);
    }

    #[test]
    fn deeply_nested_50_plus_collapses_to_single_span() {
        // Bug #3：Python 原版有硬编码 50 次迭代上限，深嵌套时静默截断保护。
        // 构造 60 层嵌套，验证全部被最外层区间捕获。
        let depth = 60;
        let mut text = String::new();
        for _ in 0..depth {
            text.push_str("<lvl>");
        }
        text.push_str("core");
        for _ in 0..depth {
            text.push_str("</lvl>");
        }
        let (cleaned, blocks, _stats) = TagProtector::default().protect_with_stats(&text);
        assert!(!cleaned.contains("<lvl>"));
        assert!(!cleaned.contains("</lvl>"));
        assert_eq!(blocks.len(), 1);
        assert_eq!(restore_tags(&cleaned, &blocks), text);
    }

    #[test]
    fn self_closing_duplicates_get_distinct_placeholders() {
        // Bug #4：自闭合标签同样的 first-occurrence-replace 问题。
        let text = "<marker/> middle <marker/>";
        let (cleaned, blocks) = protect(text);
        assert_eq!(blocks.len(), 2);
        assert_ne!(blocks[0].0, blocks[1].0);
        assert!(!cleaned.contains("<marker/>"));
        assert_eq!(restore_tags(&cleaned, &blocks), text);
    }

    #[test]
    fn placeholder_collision_is_avoided() {
        // Bug #5：输入包含字面 `{{CMPR_TAG_…}}`。walker 应选用加盐前缀
        // 并在 stats 中报告冲突。
        let text = "User wrote {{CMPR_TAG_0}} on purpose. \
             <system-reminder>real one</system-reminder>";
        let (_cleaned, blocks, stats) = TagProtector::default().protect_with_stats(text);
        assert!(stats.placeholder_collision_avoided);
        assert_eq!(blocks.len(), 1);
        assert_ne!(blocks[0].0, "{{CMPR_TAG_0}}");
    }

    // ─── 边界情况 ──────────────────────────────────────────────────────

    #[test]
    fn orphan_close_tag_emitted_verbatim() {
        let text = "no opener </ghost> here";
        let (cleaned, blocks, stats) = TagProtector::default().protect_with_stats(text);
        assert_eq!(blocks.len(), 0);
        assert!(cleaned.contains("</ghost>"));
        assert_eq!(stats.orphan_closes, 1);
    }

    #[test]
    fn orphan_open_tag_emitted_verbatim() {
        let text = "<ghost>dangling content with no close";
        let (cleaned, blocks) = protect(text);
        assert!(blocks.is_empty());
        assert_eq!(cleaned, text);
    }

    #[test]
    fn malformed_lone_lt_emitted_verbatim() {
        let text = "if a < b then c";
        let (cleaned, blocks) = protect(text);
        assert_eq!(cleaned, text);
        assert!(blocks.is_empty());
    }

    #[test]
    fn truncated_markers_do_not_panic() {
        // Hotfix-A9：proptest 种子 `</` 会越界。修复后返回 NotTag 并原样输出。
        for text in ["</", "<", "<a/", "<a", "<a /", "</a"] {
            let (cleaned, blocks) = protect(text);
            assert_eq!(cleaned, text, "input: {text:?}");
            assert!(blocks.is_empty());
        }
    }

    #[test]
    fn attribute_with_gt_inside_quotes() {
        let text = r#"<context attr="a > b">payload</context>"#;
        let (cleaned, blocks) = protect(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, text);
        assert!(!cleaned.contains("payload"));
    }

    #[test]
    fn html_close_inside_custom_block_does_not_pop_stack() {
        // HTML 闭标签落在自定义栈顶之上时不应搅乱栈：HTML 闭标签原样输出，
        // 自定义区间仍等自己的闭标签到来才闭合。
        let text = "<custom>x</div> y</custom>";
        let (cleaned, blocks, stats) = TagProtector::default().protect_with_stats(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, "<custom>x</div> y</custom>");
        assert!(!cleaned.contains("<custom>"));
        // `</div>` 是 HTML，不是孤儿。
        assert_eq!(stats.html_tags_skipped, 1);
        assert_eq!(stats.orphan_closes, 0);
    }

    #[test]
    fn stats_accounting_exact() {
        // div(open) div(close) custom(open) custom(close) self(open) ghost(close)
        // = 6 个标签。
        let text = "<div>h</div> <custom>x</custom> <self/> </ghost>";
        let (_cleaned, blocks, stats) = TagProtector::default().protect_with_stats(text);
        assert_eq!(stats.tags_seen, 6);
        assert_eq!(stats.html_tags_skipped, 2); // <div> 与 </div>
        assert_eq!(stats.custom_blocks_protected, 1); // <custom>…</custom>
        assert_eq!(stats.self_closing_protected, 1); // <self/>
        assert_eq!(stats.orphan_closes, 1); // </ghost>
        assert_eq!(blocks.len(), 2);
    }
}
