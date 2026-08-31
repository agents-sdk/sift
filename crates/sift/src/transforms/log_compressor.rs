//! 日志压缩器：构建/测试输出（pytest / npm / cargo / jest / make / 通用）
//! → 保留错误、首尾错误、堆栈跟踪、摘要行，折叠运行时帧，全局预算截断。
//!
//! 构建日志压缩器。与常规日志抽取的差异点：
//! - aho-corasick → 手写字符串扫描（不新增依赖）；
//! - regex 去重归一化 → 手写字符级归一化；
//! - md5 缓存键 → 复用 [`crate::stash::compute_key`]（blake3 前 24 hex）；
//! - `adaptive_sizer::compute_optimal_k` → 内置简化版自适应预算。
//!
//! 相对 Python 参考实现修复并保留的语义（fixed_in_3e5 系列）：
//! 1. 堆栈跟踪状态机不在任意空行终止（链式异常跟踪内嵌空行得以保留）；
//! 2. 保守去重：保留消息前缀（第一个 `:` 或 `=` 之前的内容），
//!    不同地址/ID 的同类错误不会被折叠成一条；
//! 3. `LogLevel::Fail` 与 `Error` 打分等价（1.0），区分仅用于展示。
//!
//! 本压缩器为有损变换，实现 [`crate::transforms::OffloadTransform`]：
//! 原文经 `apply` 返回给调用方写入 stash store，实现端到端无损。

use std::collections::{BTreeMap, BTreeSet};

use crate::content::ContentType;
use crate::stash;
use crate::transforms::{
    CompressionContext, OffloadOutput, OffloadTransform, OmissionRange, TransformError,
};

// ─── 类型定义 ─────────────────────────────────────────────────────────────

/// 检测到的日志格式，`Generic` 为兜底。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogFormat {
    Pytest,
    Npm,
    Cargo,
    Jest,
    Make,
    Generic,
}

impl LogFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogFormat::Pytest => "pytest",
            LogFormat::Npm => "npm",
            LogFormat::Cargo => "cargo",
            LogFormat::Jest => "jest",
            LogFormat::Make => "make",
            LogFormat::Generic => "generic",
        }
    }
}

/// 单行日志级别。Error/Fail 打分等价，区分仅为展示（与参考实现保持一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogLevel {
    Error,
    Fail,
    Warn,
    Info,
    Debug,
    Trace,
    Unknown,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Fail => "fail",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
            LogLevel::Unknown => "unknown",
        }
    }
}

/// 一行已分类的日志。
///
/// 相等性 / 排序只基于 `line_number`（与参考实现的 Python dunder 语义一致），
/// 支撑选择阶段的按行号去重与有序输出。
#[derive(Debug, Clone)]
pub struct LogLine {
    pub line_number: usize,
    pub content: String,
    pub level: LogLevel,
    pub is_stack_trace: bool,
    pub is_summary: bool,
    pub score: f32,
}

impl PartialEq for LogLine {
    fn eq(&self, other: &Self) -> bool {
        self.line_number == other.line_number
    }
}
impl Eq for LogLine {}
impl PartialOrd for LogLine {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for LogLine {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.line_number.cmp(&other.line_number)
    }
}

impl LogLine {
    pub fn new(line_number: usize, content: impl Into<String>) -> Self {
        Self {
            line_number,
            content: content.into(),
            level: LogLevel::Unknown,
            is_stack_trace: false,
            is_summary: false,
            score: 0.0,
        }
    }
}

/// 压缩器配置，默认值与参考实现 `LogCompressorConfig` 一致。
#[derive(Debug, Clone)]
pub struct LogCompressorConfig {
    pub max_errors: usize,
    pub error_context_lines: usize,
    pub keep_first_error: bool,
    pub keep_last_error: bool,
    pub max_stack_traces: usize,
    pub stack_trace_max_lines: usize,
    pub max_warnings: usize,
    pub dedupe_warnings: bool,
    pub keep_summary_lines: bool,
    /// 普通诊断行预算；首行和命令上下文不受此上限裁剪。
    pub max_total_lines: usize,
    /// 是否输出 stash 取回标记（需要调用 `compress_with_store` 传入 store）。
    pub enable_stash: bool,
    pub min_lines_for_stash: usize,
    /// 触发 stash 存储的压缩率阈值（压缩后/原始字节数低于该值才卸载）。
    pub min_compression_ratio_for_stash: f64,
    /// 超长堆栈折叠运行时/标准库帧（而非盲目截断尾部）。
    pub collapse_runtime_frames: bool,
    /// 折叠时始终保留的头部帧数。
    pub trace_head_frames: usize,
    /// 头部之外保留的应用代码帧数。
    pub trace_app_frames: usize,
}

impl Default for LogCompressorConfig {
    fn default() -> Self {
        Self {
            max_errors: 10,
            error_context_lines: 3,
            keep_first_error: true,
            keep_last_error: true,
            max_stack_traces: 3,
            stack_trace_max_lines: 20,
            max_warnings: 5,
            dedupe_warnings: true,
            keep_summary_lines: true,
            max_total_lines: 100,
            enable_stash: true,
            min_lines_for_stash: 50,
            min_compression_ratio_for_stash: 0.5,
            collapse_runtime_frames: true,
            trace_head_frames: 3,
            trace_app_frames: 5,
        }
    }
}

/// 压缩结果。
#[derive(Debug, Clone)]
pub struct LogCompressionResult {
    pub compressed: String,
    pub original: String,
    pub original_line_count: usize,
    pub compressed_line_count: usize,
    pub format_detected: LogFormat,
    pub compression_ratio: f64,
    pub cache_key: Option<String>,
    pub stats: BTreeMap<String, u64>,
    /// 被丢弃的 stash 原文连续行范围（1-based）。
    pub omissions: Vec<OmissionRange>,
}

impl LogCompressionResult {
    /// 估算节省的 token 数（按 4 字符/token 粗估，下限 0）。
    pub fn tokens_saved_estimate(&self) -> i64 {
        let chars_saved = self.original.len() as i64 - self.compressed.len() as i64;
        chars_saved.max(0) / 4
    }
    pub fn lines_omitted(&self) -> usize {
        self.original_line_count
            .saturating_sub(self.compressed_line_count)
    }
}

/// 旁路诊断信息（不进入压缩输出）。
#[derive(Debug, Clone, Default)]
pub struct LogCompressorStats {
    pub format: Option<LogFormat>,
    pub stack_traces_seen: usize,
    pub stack_traces_kept: usize,
    pub warnings_dropped_by_dedupe: usize,
    pub lines_dropped_by_global_cap: usize,
    pub runtime_frames_collapsed: usize,
    pub stash_emitted: bool,
    pub stash_skip_reason: Option<&'static str>,
}

// ─── 格式检测 ─────────────────────────────────────────────────────────────

/// 内联静态表格式检测：扫描前 100 行，取标记命中数最多的格式（与参考实现一致）。
struct FormatDetector {
    matchers: Vec<(LogFormat, Vec<&'static str>)>,
}

impl FormatDetector {
    fn new() -> Self {
        Self {
            matchers: vec![
                (
                    LogFormat::Pytest,
                    vec![
                        "=== FAILURES",
                        "=== ERRORS",
                        "=== test session",
                        "=== short test summary",
                        "PASSED [",
                        "FAILED [",
                        "ERROR [",
                        "SKIPPED [",
                        "collected ",
                    ],
                ),
                (
                    LogFormat::Npm,
                    vec!["npm ERR!", "npm WARN", "npm info", "npm http"],
                ),
                (
                    LogFormat::Cargo,
                    vec![
                        "Compiling ",
                        "Finished ",
                        "Running ",
                        "warning: ",
                        "error[E",
                    ],
                ),
                (LogFormat::Jest, vec!["PASS ", "FAIL ", "Test Suites:"]),
                (
                    LogFormat::Make,
                    vec!["make[", "make:", "gcc ", "g++ ", "clang "],
                ),
            ],
        }
    }

    fn detect(&self, lines: &[&str]) -> LogFormat {
        let sample: Vec<&str> = lines.iter().take(100).copied().collect();
        let mut best: Option<(LogFormat, usize)> = None;
        for (fmt, patterns) in &self.matchers {
            let mut score = 0usize;
            for line in &sample {
                // 每行每格式最多计一次命中（与参考实现的内层 break 一致）
                if patterns.iter().any(|p| line.contains(p)) {
                    score += 1;
                }
            }
            if score > 0 && best.map(|(_, s)| score > s).unwrap_or(true) {
                best = Some((*fmt, score));
            }
        }
        best.map(|(f, _)| f).unwrap_or(LogFormat::Generic)
    }
}

// ─── 级别分类 ─────────────────────────────────────────────────────────────

/// 词边界感知的级别分类器。参考实现用 aho-corasick + 词边界后过滤，
/// 这里用手写字符串查找：组间按级别优先级（Error 先于 Fail……），
/// 组内长词优先（`warning` 先于 `warn`），近似 LeftmostLongest 语义。
struct LevelClassifier {
    /// (级别, 关键词表)；组内关键词已按长度降序排列。
    entries: Vec<(LogLevel, Vec<&'static str>)>,
}

impl LevelClassifier {
    fn new() -> Self {
        let mut entries: Vec<(LogLevel, Vec<&'static str>)> = vec![
            (
                LogLevel::Error,
                vec![
                    "CRITICAL", "critical", "Critical", "FATAL", "fatal", "Fatal", "ERROR",
                    "error", "Error",
                ],
            ),
            (
                LogLevel::Fail,
                vec!["FAILED", "failed", "Failed", "FAIL", "fail", "Fail"],
            ),
            (
                LogLevel::Warn,
                vec!["WARNING", "warning", "Warning", "WARN", "warn", "Warn"],
            ),
            (LogLevel::Info, vec!["INFO", "info", "Info"]),
            (LogLevel::Debug, vec!["DEBUG", "debug", "Debug"]),
            (LogLevel::Trace, vec!["TRACE", "trace", "Trace"]),
        ];
        for (_, words) in &mut entries {
            words.sort_by_key(|w| std::cmp::Reverse(w.len()));
        }
        Self { entries }
    }

    fn classify(&self, line: &str) -> LogLevel {
        let bytes = line.as_bytes();
        for (level, words) in &self.entries {
            for word in words {
                // 在行内寻找该词的词边界命中
                let wb = word.as_bytes();
                let mut from = 0usize;
                while let Some(pos) = line[from..].find(word) {
                    let start = from + pos;
                    let end = start + wb.len();
                    if is_word_boundary(bytes, start, end) {
                        return *level;
                    }
                    from = end;
                }
            }
        }
        LogLevel::Unknown
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

// ─── 堆栈跟踪检测 ─────────────────────────────────────────────────────────

/// 手写堆栈跟踪状态机。每种语言变体有自己的开启标记识别，
/// 之后持续把行标记为跟踪的一部分，直到变体特定的终止规则触发
/// 或达到 `stack_trace_max_lines`。
///
/// fixed_in_3e5：Python 版在任意空行终止，导致链式异常跟踪（内嵌空行）
/// 的中段丢失。这里只有不适配内嵌空行的变体才把空行当终止符。
struct StackTraceDetector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TraceFlavor {
    PythonTraceback,
    Js,
    Java,
    RustError,
    /// Rust panic + `RUST_BACKTRACE` 转储，帧为 `N: 0x<hex>` / `N: <symbol>`。
    RustBacktrace,
    GoPanic,
    DotNet,
}

impl StackTraceDetector {
    fn flavor_for(line: &str) -> Option<TraceFlavor> {
        let trimmed = line.trim_start();
        if trimmed.starts_with("Traceback (most recent call last)")
            || Self::is_python_file_frame(trimmed)
        {
            Some(TraceFlavor::PythonTraceback)
        } else if Self::is_dotnet_opener(trimmed) {
            // 先于 Js/Java：.NET 的 `at Ns.Class.Method(...) in File.cs:line N`
            // 也满足 Java 的 `at <dotted>(` 形状。
            Some(TraceFlavor::DotNet)
        } else if Self::is_js_at_frame(trimmed) {
            Some(TraceFlavor::Js)
        } else if Self::is_java_at_frame(trimmed) {
            Some(TraceFlavor::Java)
        } else if trimmed.starts_with("--> ") && Self::has_line_col_suffix(trimmed) {
            Some(TraceFlavor::RustError)
        } else if Self::is_rust_panic_opener(trimmed)
            || trimmed.starts_with("stack backtrace:")
            || Self::is_rust_backtrace_frame(line)
        {
            Some(TraceFlavor::RustBacktrace)
        } else if Self::is_go_panic_opener(line) {
            Some(TraceFlavor::GoPanic)
        } else {
            None
        }
    }

    fn is_python_file_frame(s: &str) -> bool {
        // 形如 `File "<name>", line <N>`
        s.starts_with("File \"")
            && s.contains("\", line ")
            && s.bytes().next_back().is_some_and(|b| b.is_ascii_digit())
    }

    fn is_js_at_frame(s: &str) -> bool {
        // 形如 `at <name>(<file>:<line>:<col>)`
        s.starts_with("at ") && s.contains('(') && s.contains(')') && Self::has_line_col_suffix(s)
    }

    fn is_java_at_frame(s: &str) -> bool {
        // 形如 `at <package.Class.method>(`。`/` 允许 JPMS 模块前缀与 lambda 帧。
        if !s.starts_with("at ") || !s.contains('(') {
            return false;
        }
        let body = &s[3..s.find('(').unwrap_or(s.len())];
        !body.is_empty()
            && body
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '$' | '/'))
    }

    fn has_line_col_suffix(s: &str) -> bool {
        // 寻找 `:<数字>:<数字>`（line:col）
        let bytes = s.as_bytes();
        for i in 0..bytes.len().saturating_sub(2) {
            if bytes[i] == b':' && bytes[i + 1].is_ascii_digit() {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j < bytes.len()
                    && bytes[j] == b':'
                    && bytes.get(j + 1).copied().map(|b| b.is_ascii_digit()) == Some(true)
                {
                    return true;
                }
            }
        }
        false
    }

    fn is_rust_panic_opener(s: &str) -> bool {
        // 形如 `thread '<name>' panicked at <loc>`（任意 rustc 时代）
        s.starts_with("thread '") && s.contains("panicked at")
    }

    fn is_go_panic_opener(line: &str) -> bool {
        // `panic: <msg>` / `fatal error: <msg>`（顶格）或 goroutine 头
        if line.starts_with("panic: ") || line.starts_with("fatal error: ") {
            return true;
        }
        Self::is_goroutine_header(line)
    }

    fn is_goroutine_header(line: &str) -> bool {
        // `goroutine <N> [<state>]:`
        let Some(rest) = line.strip_prefix("goroutine ") else {
            return false;
        };
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        digits > 0 && rest[digits..].starts_with(" [")
    }

    fn is_go_file_frame(line: &str) -> bool {
        // Tab 缩进的 `<path>.go:<line> +0x<hex>`（goroutine 帧对的第二行）
        let Some(rest) = line.strip_prefix('\t') else {
            return false;
        };
        rest.contains(".go:") && rest.contains(" +0x")
    }

    fn is_go_call_frame(line: &str) -> bool {
        // goroutine 块内的 `pkg.func(...)` / `created by pkg.func` 调用行
        if line.starts_with("created by ") {
            return true;
        }
        if line.starts_with([' ', '\t']) || !line.ends_with(')') {
            return false;
        }
        let Some(open) = line.find('(') else {
            return false;
        };
        let symbol = &line[..open];
        !symbol.is_empty()
            && symbol.contains('.')
            && symbol
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '*'))
    }

    fn is_dotnet_opener(s: &str) -> bool {
        s.starts_with("Unhandled exception.") || Self::is_dotnet_frame(s)
    }

    fn is_dotnet_frame(s: &str) -> bool {
        // `at <symbol>(<args>) in <file>:line <N>` —— ` in … :line` 后缀
        // 是 .NET 帧与 Java 帧的区别所在。
        s.starts_with("at ") && s.contains(") in ") && s.contains(":line ")
    }

    fn is_rust_backtrace_frame(s: &str) -> bool {
        // 形如 `<数字>:<空格>0x<hex>`
        let trimmed = s.trim_start();
        let mut chars = trimmed.chars().peekable();
        let mut saw_digit = false;
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                saw_digit = true;
                chars.next();
            } else {
                break;
            }
        }
        if !saw_digit || chars.next() != Some(':') {
            return false;
        }
        while chars.peek() == Some(&' ') {
            chars.next();
        }
        let rest: String = chars.collect();
        rest.starts_with("0x")
            && rest[2..]
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .count()
                > 0
    }

    /// `line` 是否应终止当前变体的跟踪。`lines_so_far` 是当前跟踪已
    /// 占用的行数（1 = 只有开启行）—— RustBacktrace 用它保住
    /// `panicked at <loc>:` 后面的自由文本 panic 消息行。
    fn terminates(flavor: TraceFlavor, line: &str, lines_so_far: usize) -> bool {
        let trimmed = line.trim_start();
        match flavor {
            TraceFlavor::PythonTraceback => {
                // 空行（链式异常修复）与已知延续标记不终止；
                // 非缩进行中，大写开头的 `ExceptionType: message` 终止符保留在跟踪内。
                let is_indented_or_blank = line.starts_with([' ', '\t']) || line.is_empty();
                let is_continuation = trimmed.starts_with("Traceback")
                    || trimmed.starts_with("File ")
                    || trimmed.starts_with("During handling")
                    || trimmed.starts_with("The above exception");
                if is_indented_or_blank || is_continuation {
                    false
                } else {
                    !trimmed.starts_with(char::is_uppercase)
                }
            }
            TraceFlavor::Js => !trimmed.starts_with("at ") && !line.is_empty(),
            TraceFlavor::Java => {
                // `Caused by:` / `Suppressed:` / `... N more` 是链头，
                // 在这里终止会把一条链式异常拆成多条并在 max_stack_traces 下丢失后段。
                let is_chain = trimmed.starts_with("Caused by:")
                    || trimmed.starts_with("Suppressed:")
                    || Self::is_java_more_summary(trimmed);
                !trimmed.starts_with("at ") && !is_chain && !line.is_empty()
            }
            TraceFlavor::DotNet => {
                if line.is_empty() {
                    return false;
                }
                let continues = trimmed.starts_with("at ")
                    || trimmed.starts_with("--->")
                    || trimmed.starts_with("--- End of")
                    || Self::is_dotnet_exception_head(trimmed);
                !continues
            }
            TraceFlavor::RustError => !trimmed.starts_with("--> ") && !line.is_empty(),
            TraceFlavor::RustBacktrace => {
                if line.is_empty() || lines_so_far == 1 {
                    // 开启行后面的非缩进 panic 消息行保留
                    return false;
                }
                let is_frame = trimmed.chars().next().is_some_and(|c| c.is_ascii_digit());
                let is_continuation = line.starts_with([' ', '\t'])
                    || trimmed.starts_with("stack backtrace:")
                    || trimmed.starts_with("note: run with");
                !is_frame && !is_continuation
            }
            TraceFlavor::GoPanic => {
                // goroutine 转储 = 若干 `goroutine N [state]:` 头、调用行、
                // tab 缩进 `.go:` 行，块间空行分隔。信号行与链式 `panic:` 行延续。
                if line.is_empty() {
                    return false;
                }
                let continues = line.starts_with('\t')
                    || Self::is_goroutine_header(line)
                    || Self::is_go_call_frame(line)
                    || line.starts_with("panic: ")
                    || line.starts_with("fatal error: ")
                    || line.starts_with("[signal ");
                !continues
            }
        }
    }

    fn is_dotnet_exception_head(trimmed: &str) -> bool {
        // `System.InvalidOperationException: message`（点分类型以 Exception 结尾 + 冒号）
        let Some(colon) = trimmed.find(':') else {
            return false;
        };
        let head = &trimmed[..colon];
        head.ends_with("Exception")
            && head.contains('.')
            && head
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '`' | '+'))
    }

    fn is_java_more_summary(trimmed: &str) -> bool {
        // `... 17 more`
        let Some(rest) = trimmed.strip_prefix("... ") else {
            return false;
        };
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        digits > 0 && rest[digits..].trim() == "more"
    }
}

// ─── 帧折叠 ───────────────────────────────────────────────────────────────

/// 超长堆栈折叠运行时帧的结果。
struct CollapsedTrace {
    kept: Vec<LogLine>,
    /// 被丢弃帧的原始行号——上下文补行阶段排除它们，防止作为邻居混回。
    dropped_indices: Vec<usize>,
}

/// 是否为栈帧（区别于异常消息 / 链头行）。
fn is_frame_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("at ")
        || (trimmed.starts_with("File \"") && trimmed.contains("\", line "))
        || StackTraceDetector::is_rust_backtrace_frame(line)
        || StackTraceDetector::is_go_file_frame(line)
        || StackTraceDetector::is_go_call_frame(line)
}

/// 折叠中必须存活的链头 / 跟踪间标记。
fn is_chain_head_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("Caused by:")
        || trimmed.starts_with("Suppressed:")
        || trimmed.starts_with("... ")
        || trimmed.starts_with("--->")
        || trimmed.starts_with("--- End of")
        || trimmed.starts_with("During handling")
        || trimmed.starts_with("The above exception")
}

/// 运行时/标准库帧标记：前缀匹配（trimmed 行）与包含匹配（路径/点分符号）。
const RUNTIME_FRAME_PREFIXES: &[&str] = &[
    "at java.",
    "at jdk.",
    "at sun.",
    "at javax.",
    "at scala.",
    "at System.",
    "at Microsoft.",
    "runtime.",
    "created by runtime.",
];
const RUNTIME_FRAME_MARKERS: &[&str] = &[
    "site-packages/",
    "/usr/lib/python",
    "lib/python3.",
    "node:internal/",
    "node_modules/",
    "(internal/",
    "core::",
    "std::",
    "alloc::",
    "rust_begin_unwind",
    "__rust_",
    "/rustc/",
    "/usr/local/go/src/",
    "/libexec/src/runtime/",
];

fn is_runtime_frame(line: &str) -> bool {
    let trimmed = line.trim_start();
    RUNTIME_FRAME_PREFIXES
        .iter()
        .any(|p| trimmed.starts_with(p))
        || RUNTIME_FRAME_MARKERS.iter().any(|m| line.contains(m))
}

/// 折叠超长跟踪中的运行时帧：保留所有消息/链头行、前 `head_frames` 帧、
/// 以及最多 `app_frames` 个应用代码帧；每段连续丢弃区折叠为一个
/// `[... N frames collapsed]` 标记。被丢弃帧的缩进续行（如 Python 源码回显）
/// 随帧一起丢弃。
fn collapse_trace_frames(
    stack: &[LogLine],
    head_frames: usize,
    app_frames: usize,
) -> CollapsedTrace {
    let mut kept: Vec<LogLine> = Vec::with_capacity(stack.len().min(64));
    let mut dropped_indices: Vec<usize> = Vec::new();
    let mut frames_seen = 0usize;
    let mut app_kept = 0usize;
    let mut run_start: Option<usize> = None;
    let mut run_len = 0usize;
    let mut prev_dropped = false;

    fn flush_run(kept: &mut Vec<LogLine>, run_start: &mut Option<usize>, run_len: &mut usize) {
        if let Some(ln) = run_start.take() {
            let mut marker = LogLine::new(ln, format!("      [... {run_len} frames collapsed]"));
            // 标记代表多行，不能在分数排序的全局截断里最先被丢
            marker.score = 0.8;
            marker.is_stack_trace = true;
            kept.push(marker);
            *run_len = 0;
        }
    }

    for line in stack {
        if is_frame_line(&line.content) && !is_chain_head_line(&line.content) {
            frames_seen += 1;
            let runtime = is_runtime_frame(&line.content);
            let keep = frames_seen <= head_frames || (!runtime && app_kept < app_frames);
            if keep {
                if !runtime {
                    app_kept += 1;
                }
                flush_run(&mut kept, &mut run_start, &mut run_len);
                kept.push(line.clone());
                prev_dropped = false;
            } else {
                if run_start.is_none() {
                    run_start = Some(line.line_number);
                }
                run_len += 1;
                dropped_indices.push(line.line_number);
                prev_dropped = true;
            }
        } else if prev_dropped
            && line.content.starts_with([' ', '\t'])
            && !is_chain_head_line(&line.content)
        {
            // 被丢弃帧的缩进续行（源码回显等）
            run_len += 1;
            dropped_indices.push(line.line_number);
        } else {
            flush_run(&mut kept, &mut run_start, &mut run_len);
            kept.push(line.clone());
            prev_dropped = false;
        }
    }
    flush_run(&mut kept, &mut run_start, &mut run_len);
    CollapsedTrace {
        kept,
        dropped_indices,
    }
}

// ─── 摘要行检测 ───────────────────────────────────────────────────────────

fn is_summary_line(line: &str) -> bool {
    // 对应参考实现的 _SUMMARY_PATTERNS（行首锚定）
    if line.starts_with("===") || line.starts_with("---") {
        return true;
    }
    let bytes = line.as_bytes();
    let leading_digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if leading_digits > 0 && line[leading_digits..].starts_with(' ') {
        let rest = &line[leading_digits + 1..];
        for kw in &["passed", "failed", "skipped", "error", "warning"] {
            if rest.starts_with(kw) {
                return true;
            }
        }
    }
    for prefix in &[
        "Test ", "Tests ", "Tests:", "Test:", "Suite ", "Suites ", "Suites:", "Suite:",
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return rest
                .chars()
                .find(|c| !c.is_whitespace())
                .is_some_and(|c| c.is_ascii_digit());
        }
    }
    for prefix in &["TOTAL", "Total", "Summary"] {
        if line.starts_with(prefix) {
            return true;
        }
    }
    for prefix in &["Build", "Compile", "Test"] {
        if line.starts_with(prefix) {
            for outcome in &["succeeded", "failed", "complete"] {
                if line.contains(outcome) {
                    return true;
                }
            }
        }
    }
    false
}

// ─── 压缩器 ───────────────────────────────────────────────────────────────

/// 日志压缩器。有损变换：调用方负责把原文写入 stash store。
pub struct LogCompressor {
    config: LogCompressorConfig,
    formats: FormatDetector,
    levels: LevelClassifier,
}

impl LogCompressor {
    pub fn new(config: LogCompressorConfig) -> Self {
        Self {
            config,
            formats: FormatDetector::new(),
            levels: LevelClassifier::new(),
        }
    }

    pub fn config(&self) -> &LogCompressorConfig {
        &self.config
    }

    /// 压缩（不带 stash store；需要卸载标记时用 `compress_with_store`）。
    pub fn compress(&self, content: &str, bias: f64) -> (LogCompressionResult, LogCompressorStats) {
        self.compress_with_store(content, bias, None)
    }

    /// 压缩并可选把原文卸载进 store（压缩率达标时在输出尾部附取回标记）。
    pub fn compress_with_store(
        &self,
        content: &str,
        bias: f64,
        store: Option<&dyn stash::StashStore>,
    ) -> (LogCompressionResult, LogCompressorStats) {
        let mut stats = LogCompressorStats::default();
        let lines: Vec<&str> = content.split('\n').collect();
        let original_line_count = lines.len();

        if original_line_count < self.config.min_lines_for_stash {
            // 与参考实现一致：短日志原样返回
            return (
                LogCompressionResult {
                    compressed: content.to_string(),
                    original: content.to_string(),
                    original_line_count,
                    compressed_line_count: original_line_count,
                    format_detected: LogFormat::Generic,
                    compression_ratio: 1.0,
                    cache_key: None,
                    stats: BTreeMap::new(),
                    omissions: Vec::new(),
                },
                stats,
            );
        }

        let format = self.formats.detect(&lines);
        stats.format = Some(format);

        let log_lines = self.parse_lines(&lines);
        let selected = self.select_lines(&log_lines, bias, &mut stats);
        let logical_line_count = content.split_inclusive('\n').count();
        let mut omissions = omitted_ranges(&selected, logical_line_count);

        let (compressed_body, output_stats) = self.format_output(&selected, &log_lines);
        let mut compressed = compressed_body;
        let mut compressed_line_count = selected.len();
        let ratio = compressed.len() as f64 / content.len().max(1) as f64;

        let mut cache_key = None;
        if self.config.enable_stash {
            if ratio >= self.config.min_compression_ratio_for_stash {
                stats.stash_skip_reason = Some("compression ratio too high");
            } else if let Some(store) = store {
                let key = stash::compute_key(content);
                if store.put(&key, content).is_ok() {
                    let marker = format!(
                        "\n[{} lines compressed to {}. Retrieve more: hash={}]",
                        original_line_count,
                        selected.len(),
                        key
                    );
                    compressed.push_str(&marker);
                    cache_key = Some(key);
                    stats.stash_emitted = true;
                } else {
                    stats.stash_skip_reason = Some("store write failed");
                    // 调用方明确提供了 store，却无法持久化原文：必须回退原样，
                    // 不能返回一个无 marker、不可恢复的有损日志。
                    compressed = content.to_string();
                    compressed_line_count = original_line_count;
                    omissions.clear();
                }
            } else {
                stats.stash_skip_reason = Some("no store provided");
            }
        } else {
            stats.stash_skip_reason = Some("stash disabled in config");
        }

        let final_ratio = compressed.len() as f64 / content.len().max(1) as f64;
        let result = LogCompressionResult {
            compressed,
            original: content.to_string(),
            original_line_count,
            compressed_line_count,
            format_detected: format,
            compression_ratio: final_ratio,
            cache_key,
            stats: output_stats,
            omissions,
        };
        (result, stats)
    }

    // ─── 阶段辅助（测试与后续管线复用） ──────────────────────────────

    pub fn detect_format(&self, lines: &[&str]) -> LogFormat {
        self.formats.detect(lines)
    }

    /// 逐行分类：级别、摘要标记、堆栈跟踪状态机、打分。
    pub fn parse_lines(&self, lines: &[&str]) -> Vec<LogLine> {
        let mut out: Vec<LogLine> = Vec::with_capacity(lines.len());
        let mut active: Option<TraceFlavor> = None;
        let mut trace_lines = 0usize;

        for (i, line) in lines.iter().enumerate() {
            let mut entry = LogLine::new(i, *line);
            entry.level = self.levels.classify(line);
            entry.is_summary = is_summary_line(line);

            if let Some(flavor) = active {
                if trace_lines >= self.config.stack_trace_max_lines
                    || StackTraceDetector::terminates(flavor, line, trace_lines)
                {
                    let cap_hit = trace_lines >= self.config.stack_trace_max_lines;
                    active = None;
                    trace_lines = 0;
                    // 当前行可能是链式跟踪的新开启行：重查开启标记
                    if let Some(new_flavor) = StackTraceDetector::flavor_for(line) {
                        active = Some(new_flavor);
                        trace_lines = 1;
                        entry.is_stack_trace = true;
                    } else if cap_hit && !StackTraceDetector::terminates(flavor, line, 2) {
                        // 行数上限命中在跟踪中段：该行自身不是开启行但仍在
                        // 延续当前变体，继续标记，让帧折叠（而非任意对齐截断）
                        // 决定去留。
                        active = Some(flavor);
                        trace_lines = 1;
                        entry.is_stack_trace = true;
                    }
                } else {
                    entry.is_stack_trace = true;
                    trace_lines += 1;
                }
            } else if let Some(flavor) = StackTraceDetector::flavor_for(line) {
                active = Some(flavor);
                trace_lines = 1;
                entry.is_stack_trace = true;
            }

            entry.score = score_log_line(&entry);
            out.push(entry);
        }
        out
    }

    pub fn score_line(&self, line: &LogLine) -> f32 {
        score_log_line(line)
    }

    /// 选择阶段：分类 → 首尾错误 → 警告去重 → 堆栈（含折叠）→ 摘要 →
    /// 上下文补行 → 全局自适应截断。
    pub fn select_lines(
        &self,
        log_lines: &[LogLine],
        bias: f64,
        stats: &mut LogCompressorStats,
    ) -> Vec<LogLine> {
        let adaptive_max = self.adaptive_budget(log_lines.len(), bias);
        let contents: Vec<_> = log_lines.iter().map(|line| line.content.as_str()).collect();
        let protected: BTreeSet<LogLine> = super::log_context::protected_lines(&contents)
            .into_iter()
            .map(|i| log_lines[i].clone())
            .collect();

        let mut errors: Vec<LogLine> = Vec::new();
        let mut fails: Vec<LogLine> = Vec::new();
        let mut warnings: Vec<LogLine> = Vec::new();
        let mut summaries: Vec<LogLine> = Vec::new();
        let mut stack_traces: Vec<Vec<LogLine>> = Vec::new();
        let mut current_stack: Vec<LogLine> = Vec::new();

        for line in log_lines {
            match line.level {
                LogLevel::Error => errors.push(line.clone()),
                LogLevel::Fail => fails.push(line.clone()),
                LogLevel::Warn => warnings.push(line.clone()),
                _ => {}
            }
            if line.is_stack_trace {
                current_stack.push(line.clone());
            } else if !current_stack.is_empty() {
                stack_traces.push(std::mem::take(&mut current_stack));
            }
            if line.is_summary {
                summaries.push(line.clone());
            }
        }
        if !current_stack.is_empty() {
            stack_traces.push(current_stack);
        }
        stats.stack_traces_seen = stack_traces.len();

        // BTreeSet 按 line_number 排序，天然得到行号有序输出
        let mut selected = protected.clone();

        for line in self.select_with_first_last(&errors, self.config.max_errors) {
            selected.insert(line);
        }
        for line in self.select_with_first_last(&fails, self.config.max_errors) {
            selected.insert(line);
        }

        let warnings = if self.config.dedupe_warnings {
            let deduped = self.dedupe_similar(warnings);
            stats.warnings_dropped_by_dedupe = warnings_dropped(log_lines, &deduped);
            deduped
        } else {
            warnings
        };
        for line in warnings.into_iter().take(self.config.max_warnings) {
            selected.insert(line);
        }

        let mut collapsed_frame_indices: BTreeSet<usize> = BTreeSet::new();
        for stack in stack_traces.iter().take(self.config.max_stack_traces) {
            stats.stack_traces_kept += 1;
            if self.config.collapse_runtime_frames
                && stack.len() > self.config.stack_trace_max_lines
            {
                let collapsed = collapse_trace_frames(
                    stack,
                    self.config.trace_head_frames,
                    self.config.trace_app_frames,
                );
                stats.runtime_frames_collapsed += collapsed.dropped_indices.len();
                collapsed_frame_indices.extend(collapsed.dropped_indices);
                for line in collapsed
                    .kept
                    .into_iter()
                    .take(self.config.stack_trace_max_lines)
                {
                    selected.insert(line);
                }
            } else {
                for line in stack.iter().take(self.config.stack_trace_max_lines) {
                    selected.insert(line.clone());
                }
            }
        }

        if self.config.keep_summary_lines {
            for line in summaries {
                selected.insert(line);
            }
        }

        // 每个已选条目周围的上下文窗口
        let selected_indices: BTreeSet<usize> = selected.iter().map(|l| l.line_number).collect();
        let mut context_indices: BTreeSet<usize> = BTreeSet::new();
        for &idx in &selected_indices {
            let lo = idx.saturating_sub(self.config.error_context_lines);
            let hi = (idx + self.config.error_context_lines + 1).min(log_lines.len());
            for i in lo..hi {
                if i != idx {
                    context_indices.insert(i);
                }
            }
        }
        for idx in context_indices {
            // 被有意折叠的运行时帧不能以“上下文”身份混回——那会撤销折叠
            if !selected_indices.contains(&idx)
                && idx < log_lines.len()
                && !collapsed_frame_indices.contains(&idx)
            {
                selected.insert(log_lines[idx].clone());
            }
        }

        // 命令上下文不参与去重或全局预算竞争；避免加了分仍被截断，或挤掉错误行。
        let mut ordered: Vec<LogLine> = selected.difference(&protected).cloned().collect();
        if ordered.len() > adaptive_max {
            stats.lines_dropped_by_global_cap += ordered.len() - adaptive_max;
            // 按分数降序取前 adaptive_max，再恢复行序
            ordered.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.line_number.cmp(&b.line_number))
            });
            ordered.truncate(adaptive_max);
        }
        ordered.extend(protected);
        ordered.sort_by_key(|l| l.line_number);
        ordered
    }

    /// 简化版自适应预算（参考实现为 adaptive_sizer::compute_optimal_k）：
    /// bias ∈ (0,1] 越大保留越多；下限 10 行，上限 max_total_lines 与总行数。
    fn adaptive_budget(&self, total_lines: usize, bias: f64) -> usize {
        let bias = bias.clamp(0.1, 1.0);
        let scaled = (self.config.max_total_lines as f64 * bias).round() as usize;
        scaled
            .max(10)
            .min(self.config.max_total_lines)
            .min(total_lines.max(1))
    }

    /// 首条 + 末条 + 按分数补足到 max_count。
    pub fn select_with_first_last(&self, lines: &[LogLine], max_count: usize) -> Vec<LogLine> {
        if lines.len() <= max_count {
            return lines.to_vec();
        }
        let mut out: Vec<LogLine> = Vec::with_capacity(max_count);
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        let push = |line: LogLine, out: &mut Vec<LogLine>, seen: &mut BTreeSet<usize>| {
            if seen.insert(line.line_number) {
                out.push(line);
            }
        };
        if self.config.keep_first_error {
            push(lines[0].clone(), &mut out, &mut seen);
        }
        if self.config.keep_last_error {
            let last = lines.last().unwrap().clone();
            push(last, &mut out, &mut seen);
        }
        let remaining = max_count.saturating_sub(out.len());
        if remaining > 0 {
            let mut by_score = lines.to_vec();
            by_score.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.line_number.cmp(&b.line_number))
            });
            for line in by_score.into_iter() {
                if !seen.contains(&line.line_number) {
                    push(line, &mut out, &mut seen);
                    if out.len() >= max_count {
                        break;
                    }
                }
            }
        }
        out
    }

    /// 保守去重（fixed_in_3e5）：保留消息前缀（第一个 `:` 或 `=` 之前），
    /// 只归一化尾部可变区（数字、hex 地址、路径），避免把不同地址/ID
    /// 的同类错误折叠成一条。
    pub fn dedupe_similar(&self, lines: Vec<LogLine>) -> Vec<LogLine> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut out: Vec<LogLine> = Vec::with_capacity(lines.len());
        for line in lines {
            let key = normalize_for_dedupe(&line.content);
            if seen.insert(key) {
                out.push(line);
            }
        }
        out
    }

    /// 输出格式化：选中行 + 省略统计摘要。
    pub fn format_output(
        &self,
        selected: &[LogLine],
        all_lines: &[LogLine],
    ) -> (String, BTreeMap<String, u64>) {
        let mut stats: BTreeMap<String, u64> = BTreeMap::new();
        stats.insert("errors".into(), count_level(all_lines, LogLevel::Error));
        stats.insert("fails".into(), count_level(all_lines, LogLevel::Fail));
        stats.insert("warnings".into(), count_level(all_lines, LogLevel::Warn));
        stats.insert("info".into(), count_level(all_lines, LogLevel::Info));
        stats.insert("total".into(), all_lines.len() as u64);
        stats.insert("selected".into(), selected.len() as u64);

        let mut output: Vec<String> = selected.iter().map(|l| l.content.clone()).collect();

        let omitted = all_lines.len().saturating_sub(selected.len());
        if omitted > 0 {
            let mut summary_parts: Vec<String> = Vec::new();
            for (label, key) in [
                ("ERROR", "errors"),
                ("FAIL", "fails"),
                ("WARN", "warnings"),
                ("INFO", "info"),
            ] {
                let n = stats.get(key).copied().unwrap_or(0);
                if n > 0 {
                    summary_parts.push(format!("{} {}", n, label));
                }
            }
            if !summary_parts.is_empty() {
                output.push(format!(
                    "[{} lines omitted: {}]",
                    omitted,
                    summary_parts.join(", ")
                ));
            }
        }
        (output.join("\n"), stats)
    }
}

/// 对已保留行求补集，并把相邻的被丢弃行合并成连续区间。
fn omitted_ranges(selected: &[LogLine], logical_line_count: usize) -> Vec<OmissionRange> {
    let kept: BTreeSet<usize> = selected.iter().map(|line| line.line_number).collect();
    let mut ranges = Vec::new();
    let mut start = None;
    for idx in 0..logical_line_count {
        if kept.contains(&idx) {
            if let Some(first) = start.take() {
                ranges.push(OmissionRange {
                    start_line: first + 1,
                    line_count: idx - first,
                });
            }
        } else if start.is_none() {
            start = Some(idx);
        }
    }
    if let Some(first) = start {
        ranges.push(OmissionRange {
            start_line: first + 1,
            line_count: logical_line_count - first,
        });
    }
    ranges
}

fn count_level(lines: &[LogLine], level: LogLevel) -> u64 {
    lines.iter().filter(|l| l.level == level).count() as u64
}

fn warnings_dropped(all: &[LogLine], deduped: &[LogLine]) -> usize {
    let original_warnings = all.iter().filter(|l| l.level == LogLevel::Warn).count();
    original_warnings.saturating_sub(deduped.len())
}

/// 行打分：级别基分 + 堆栈加成 + 摘要加成，封顶 1.0。
/// Error 与 Fail 等价（均为 1.0，参考实现文档化行为）。
fn score_log_line(line: &LogLine) -> f32 {
    let level_score: f32 = match line.level {
        LogLevel::Error | LogLevel::Fail => 1.0,
        LogLevel::Warn => 0.5,
        LogLevel::Info | LogLevel::Unknown => 0.1,
        LogLevel::Debug => 0.05,
        LogLevel::Trace => 0.02,
    };
    let stack_boost: f32 = if line.is_stack_trace { 0.3 } else { 0.0 };
    let summary_boost: f32 = if line.is_summary { 0.4 } else { 0.0 };
    (level_score + stack_boost + summary_boost).min(1.0_f32)
}

/// 去重归一化（无 regex 版）：
/// - 前缀（第一个 `:` 或 `=` 之前）原样保留；
/// - 尾部可变区：`0x<hex>` → `ADDR`，数字串 → `N`，`/路径/` → `/PATH/`。
fn normalize_for_dedupe(content: &str) -> String {
    let split_at = content.find([':', '=']).unwrap_or(content.len());
    let prefix = &content[..split_at];
    let suffix = &content[split_at..];

    let chars: Vec<char> = suffix.chars().collect();
    let mut out = String::with_capacity(suffix.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            // 0x<hex> 先于纯数字检查（0 会被当数字）
            if c == '0' && chars.get(i + 1) == Some(&'x') {
                let mut j = i + 2;
                while j < chars.len() && chars[j].is_ascii_hexdigit() {
                    j += 1;
                }
                if j > i + 2 {
                    out.push_str("ADDR");
                    i = j;
                    continue;
                }
            }
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            out.push('N');
            continue;
        }
        if c == '/' {
            // `/[\w/]+/` → `/PATH/`
            let mut j = i + 1;
            let mut saw_word = false;
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric() || chars[j] == '_' || chars[j] == '/')
            {
                if chars[j] != '/' {
                    saw_word = true;
                }
                j += 1;
            }
            // `/[\w/]+/` 正则可回溯：匹配到 run 内 *最后一个* 内部 `/`。
            // 模拟：找 run 内最后的 `/`，且它与首 `/` 之间至少一个词字符。
            if saw_word {
                let last_slash = (i + 1..j).rev().find(|&k| chars[k] == '/');
                if let Some(k) = last_slash {
                    out.push_str("/PATH/");
                    i = k + 1;
                    continue;
                }
            }
            out.push('/');
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    format!("{}{}", prefix, out)
}

/// 折叠连续重复行（保留首次出现并标注重复次数）——快速无损预处理。
pub fn fold_repeated_lines(input: &str) -> String {
    let mut out = String::with_capacity(input.len() / 2);
    let mut last: Option<&str> = None;
    let mut repeat = 1usize;
    for line in input.lines() {
        match last {
            Some(l) if l == line => repeat += 1,
            Some(l) => {
                out.push_str(l);
                if repeat > 1 {
                    out.push_str(&format!("  [x{}]\n", repeat));
                } else {
                    out.push('\n');
                }
                repeat = 1;
            }
            None => {}
        }
        last = Some(line);
    }
    if let Some(l) = last {
        out.push_str(l);
        if repeat > 1 {
            out.push_str(&format!("  [x{}]", repeat));
        }
    }
    out
}

// ─── trait 接入 ───────────────────────────────────────────────────────────

impl OffloadTransform for LogCompressor {
    fn name(&self) -> &'static str {
        "log_compressor"
    }

    fn applies_to(&self) -> ContentType {
        ContentType::BuildOutput
    }

    fn estimate_bloat(&self, input: &str) -> f64 {
        // 行数低于门槛不处理 → 无膨胀可压
        let line_count = input.split('\n').count();
        if line_count < self.config.min_lines_for_stash {
            return 0.0;
        }
        // 粗估：日志类内容大量重复行/低分行，默认视为 50% 可压
        0.5
    }

    fn cache_key(&self, input: &str) -> String {
        stash::compute_key(input)
    }

    fn apply(
        &self,
        input: &str,
        ctx: &CompressionContext,
    ) -> Result<OffloadOutput, TransformError> {
        if let Some(path) =
            super::line_omissions::actionable_file_path(ctx.stash_file_path.as_deref())
        {
            let lines: Vec<_> = input.split('\n').collect();
            if lines.len() < self.config.min_lines_for_stash {
                return Err(TransformError::Skipped);
            }
            let parsed = self.parse_lines(&lines);
            let selected = self.select_lines(&parsed, 1.0, &mut LogCompressorStats::default());
            // 折叠帧生成的合成 marker 不算原文保留行，由统一的连续行提示替代。
            let kept = selected
                .iter()
                .filter(|line| lines.get(line.line_number) == Some(&line.content.as_str()))
                .map(|line| line.line_number);
            return Ok(super::line_omissions::render(
                input,
                kept,
                path,
                ctx.stash_line_offset,
            ));
        }
        let (result, _) = self.compress(input, 1.0);
        if result.original_line_count < self.config.min_lines_for_stash
            && result.compressed != input
        {
            return Err(TransformError::Internal("short log changed".into()));
        }
        // 返回结构化卸载结果：调用方把原文写入 stash store 完成卸载
        Ok(OffloadOutput {
            compressed: result.compressed,
            original: input.to_string(),
            omissions: result.omissions,
        })
    }
}

// ─── 测试 ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stash::{InMemoryStashStore, StashStore};

    struct FailingStore;

    impl StashStore for FailingStore {
        fn put(&self, _key: &str, _value: &str) -> std::io::Result<()> {
            Err(std::io::Error::other("injected write failure"))
        }

        fn get(&self, _key: &str) -> Option<String> {
            None
        }

        fn len(&self) -> usize {
            0
        }
    }

    fn cmp() -> LogCompressor {
        LogCompressor::new(LogCompressorConfig::default())
    }

    // ─── 格式检测 ───────────────────────────────────────────────────

    #[test]
    fn detects_pytest_format() {
        let lines = [
            "============================= test session starts =============================",
            "collected 15 items",
            "tests/test_foo.py::test_basic PASSED [  6%]",
            "FAILED tests/test_foo.py::test_edge",
        ];
        assert_eq!(cmp().detect_format(&lines), LogFormat::Pytest);
    }

    #[test]
    fn detects_npm_format() {
        let lines = ["npm WARN deprecated x", "npm ERR! something"];
        assert_eq!(cmp().detect_format(&lines), LogFormat::Npm);
    }

    #[test]
    fn detects_cargo_format() {
        let lines = ["   Compiling app v0.1.0", "warning: unused variable"];
        assert_eq!(cmp().detect_format(&lines), LogFormat::Cargo);
    }

    #[test]
    fn detects_jest_format() {
        let lines = ["PASS src/app.test.js", "Test Suites: 1 failed"];
        assert_eq!(cmp().detect_format(&lines), LogFormat::Jest);
    }

    #[test]
    fn detects_make_format() {
        let lines = ["make[1]: Entering directory", "gcc -c main.c"];
        assert_eq!(cmp().detect_format(&lines), LogFormat::Make);
    }

    #[test]
    fn detects_generic_for_unrecognised_input() {
        let lines = ["INFO Starting application", "DEBUG Initializing"];
        assert_eq!(cmp().detect_format(&lines), LogFormat::Generic);
    }

    // ─── 级别分类 ───────────────────────────────────────────────────

    #[test]
    fn level_classifier_word_boundary_matches() {
        let lines =
            cmp().parse_lines(&["ERROR: critical", "warning: x", "INFO: x", "no level here"]);
        assert_eq!(lines[0].level, LogLevel::Error);
        assert_eq!(lines[1].level, LogLevel::Warn);
        assert_eq!(lines[2].level, LogLevel::Info);
        assert_eq!(lines[3].level, LogLevel::Unknown);
    }

    #[test]
    fn level_classifier_does_not_overfire_on_substrings() {
        // 词边界检查：另一词内部的级别子串不应命中
        let lines = cmp().parse_lines(&["informant arrested", "errorless code", "warned-off"]);
        assert_eq!(lines[0].level, LogLevel::Unknown);
        assert_eq!(lines[1].level, LogLevel::Unknown);
        assert_eq!(lines[2].level, LogLevel::Unknown);
    }

    #[test]
    fn level_classifier_prefers_warning_over_warn() {
        let lines = cmp().parse_lines(&["WARNING: deprecated"]);
        assert_eq!(lines[0].level, LogLevel::Warn);
    }

    // ─── 堆栈跟踪状态机 ─────────────────────────────────────────────

    fn trace_flags(c: &LogCompressor, lines: &[&str]) -> Vec<bool> {
        c.parse_lines(lines)
            .iter()
            .map(|l| l.is_stack_trace)
            .collect()
    }

    #[test]
    fn fixed_in_3e5_chained_exception_traces_survive_blank_lines() {
        // Python 版在空行终止堆栈跟踪，链式异常中段丢失；这里空行延续。
        let c = cmp();
        let lines = [
            "Traceback (most recent call last):",
            "  File \"a.py\", line 1, in <module>",
            "ValueError: x",
            "",
            "During handling of the above exception, another exception occurred:",
            "",
            "Traceback (most recent call last):",
            "  File \"b.py\", line 2, in <module>",
            "RuntimeError: y",
        ];
        let flags = trace_flags(&c, &lines);
        for (i, &expect) in flags.iter().enumerate() {
            assert_eq!(flags[i], expect, "line {}: '{}'", i, lines[i]);
        }
    }

    #[test]
    fn go_panic_and_goroutine_dump_detected() {
        let c = cmp();
        let lines = [
            "some build output",
            "panic: runtime error: index out of range [3] with length 3",
            "",
            "goroutine 1 [running]:",
            "main.lookup(0x1, 0x2)",
            "\t/app/pkg/lookup.go:42 +0x1d",
            "main.main()",
            "\t/app/main.go:10 +0x20",
            "exit status 2",
        ];
        let flags = trace_flags(&c, &lines);
        assert!(!flags[0]);
        assert!(flags[1..8].iter().all(|&f| f), "flags: {:?}", flags);
        assert!(!flags[8]);
    }

    #[test]
    fn rust_panic_backtrace_detected_with_message_line() {
        let c = cmp();
        let lines = [
            "thread 'main' panicked at src/main.rs:5:5:",
            "index out of bounds: the len is 3 but the index is 99",
            "stack backtrace:",
            "   0: rust_begin_unwind",
            "             at /rustc/abc123/library/std/src/panicking.rs:645:5",
            "   1: core::panicking::panic_fmt",
            "   2: app::run",
            "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
            "done",
        ];
        let flags = trace_flags(&c, &lines);
        // panic 消息自由文本行保留在跟踪内
        assert!(flags[..8].iter().all(|&f| f), "flags: {:?}", flags);
        assert!(!flags[8]);
    }

    #[test]
    fn dotnet_trace_continues_across_inner_exception() {
        let c = cmp();
        let lines = [
            "Unhandled exception. System.InvalidOperationException: outer failed",
            " ---> System.ArgumentNullException: inner value was null",
            "   at App.Data.Load(String path) in /src/App/Data.cs:line 42",
            "   --- End of inner exception stack trace ---",
            "   at App.Program.Main(String[] args) in /src/App/Program.cs:line 12",
            "Build finished.",
        ];
        let flags = trace_flags(&c, &lines);
        assert!(flags[..5].iter().all(|&f| f), "flags: {:?}", flags);
        assert!(!flags[5]);
    }

    #[test]
    fn java_chain_continues_across_caused_by() {
        let c = cmp();
        let lines = [
            "at com.example.Service.call(Service.java:10)",
            "at com.example.Main.run(Main.java:5)",
            "Caused by: java.io.IOException: disk gone",
            "at com.example.Disk.read(Disk.java:77)",
            "... 17 more",
            "INFO next request",
        ];
        let parsed = c.parse_lines(&lines);
        let flags: Vec<bool> = parsed.iter().map(|l| l.is_stack_trace).collect();
        assert!(flags[..5].iter().all(|&f| f), "flags: {:?}", flags);
        assert!(!flags[5]);
        // 选择阶段把它归为一条跟踪而非三条
        let mut stats = LogCompressorStats::default();
        let _ = c.select_lines(&parsed, 1.0, &mut stats);
        assert_eq!(stats.stack_traces_seen, 1);
    }

    #[test]
    fn js_trace_terminates_on_non_at_line() {
        let c = cmp();
        let lines = [
            "Error: boom",
            "    at fnA (app.js:10:5)",
            "    at fnB (app.js:20:7)",
            "INFO next",
        ];
        let flags = trace_flags(&c, &lines);
        assert!(flags[1] && flags[2]);
        assert!(!flags[3]);
    }

    // ─── 去重 ───────────────────────────────────────────────────────

    #[test]
    fn fixed_in_3e5_dedupe_preserves_distinct_messages() {
        // 不同消息前缀 = 不同条目（Python 版会把它们折叠成一条）
        let c = cmp();
        let warnings = vec![
            LogLine::new(0, "segfault at 0xdeadbeef in thread main"),
            LogLine::new(1, "heap overflow at 0xcafef00d in thread worker"),
        ];
        assert_eq!(c.dedupe_similar(warnings).len(), 2);
    }

    #[test]
    fn dedupe_collapses_genuinely_repeated_warnings() {
        let c = cmp();
        let warnings = vec![
            LogLine::new(0, "warning: file /tmp/a/123 issue"),
            LogLine::new(1, "warning: file /tmp/b/999 issue"),
        ];
        assert_eq!(c.dedupe_similar(warnings).len(), 1);
    }

    // ─── 选择与全局预算 ─────────────────────────────────────────────

    #[test]
    fn select_with_first_last_keeps_both_endpoints() {
        let c = cmp();
        let lines: Vec<LogLine> = (0..5)
            .map(|i| {
                let mut l = LogLine::new(i, format!("line {}", i));
                l.score = if i == 2 { 0.9 } else { 0.1 };
                l
            })
            .collect();
        let kept = c.select_with_first_last(&lines, 3);
        let nums: Vec<_> = kept.iter().map(|l| l.line_number).collect();
        assert!(nums.contains(&0));
        assert!(nums.contains(&4));
        // 第三个名额给高分中间行
        assert!(nums.contains(&2));
    }

    #[test]
    fn select_lines_caps_global_total() {
        let c = LogCompressor::new(LogCompressorConfig {
            max_total_lines: 12,
            stack_trace_max_lines: 2,
            min_lines_for_stash: 1,
            ..Default::default()
        });
        let mut content = String::new();
        for i in 0..60 {
            content.push_str(&format!("INFO line {}\n", i));
        }
        content.push_str("ERROR something exploded\n");
        content.push_str("ERROR another failure\n");
        let (result, stats) = c.compress(&content, 1.0);
        assert!(result.compressed_line_count <= 12);
        assert_eq!(stats.format, Some(LogFormat::Generic));
        // 两条 ERROR 高分行必须存活
        assert!(result.compressed.contains("ERROR something exploded"));
        assert!(result.compressed.contains("ERROR another failure"));
    }

    #[test]
    fn errors_survive_and_output_shrinks() {
        let c = LogCompressor::new(LogCompressorConfig {
            min_lines_for_stash: 1,
            max_total_lines: 100,
            ..Default::default()
        });
        let mut content = String::new();
        for i in 0..60 {
            content.push_str(&format!("INFO line {}\n", i));
        }
        content.push_str("ERROR boom at step 30\n");
        let (result, _) = c.compress(&content, 1.0);
        assert!(result.compressed.contains("ERROR boom at step 30"));
        assert!(result.compressed.len() < content.len());
    }

    #[test]
    fn short_input_returns_unchanged() {
        let (result, _) = cmp().compress("a\nb\nc", 1.0);
        assert_eq!(result.compressed, "a\nb\nc");
        assert_eq!(result.compression_ratio, 1.0);
    }

    // ─── stash 卸载 ───────────────────────────────────────────────────

    #[test]
    fn stash_marker_emitted_when_thresholds_clear() {
        let c = LogCompressor::new(LogCompressorConfig {
            max_total_lines: 5,
            min_lines_for_stash: 5,
            min_compression_ratio_for_stash: 0.95,
            ..Default::default()
        });
        let mut content = String::new();
        for i in 0..50 {
            content.push_str(&format!("INFO line {}\n", i));
        }
        content.push_str("ERROR boom\n");
        let store = InMemoryStashStore::new();
        let (result, stats) = c.compress_with_store(&content, 1.0, Some(&store));
        assert!(result.cache_key.is_some());
        assert!(stats.stash_emitted);
        let key = result.cache_key.as_ref().unwrap();
        assert_eq!(store.get(key).unwrap(), content);
    }

    #[test]
    fn stash_skipped_without_store() {
        let c = LogCompressor::new(LogCompressorConfig {
            max_total_lines: 5,
            min_lines_for_stash: 5,
            min_compression_ratio_for_stash: 0.95,
            ..Default::default()
        });
        let mut content = String::new();
        for i in 0..50 {
            content.push_str(&format!("INFO line {}\n", i));
        }
        content.push_str("ERROR boom\n");
        let (result, stats) = c.compress(&content, 1.0);
        assert!(result.cache_key.is_none());
        assert_eq!(stats.stash_skip_reason, Some("no store provided"));
    }

    #[test]
    fn stash_write_failure_reverts_original() {
        let c = LogCompressor::new(LogCompressorConfig {
            max_total_lines: 5,
            min_lines_for_stash: 5,
            min_compression_ratio_for_stash: 0.95,
            ..Default::default()
        });
        let mut content = String::new();
        for i in 0..50 {
            content.push_str(&format!("INFO line {}\n", i));
        }
        content.push_str("ERROR boom\n");

        let (result, stats) = c.compress_with_store(&content, 1.0, Some(&FailingStore));

        assert_eq!(result.compressed, content);
        assert_eq!(result.compressed_line_count, result.original_line_count);
        assert_eq!(result.compression_ratio, 1.0);
        assert!(result.cache_key.is_none());
        assert!(!stats.stash_emitted);
        assert_eq!(stats.stash_skip_reason, Some("store write failed"));
    }

    // ─── 输出格式 ───────────────────────────────────────────────────

    #[test]
    fn format_output_emits_summary_with_omitted_count() {
        let c = cmp();
        let all_lines: Vec<LogLine> = ["ERROR a", "WARN b", "INFO c", "INFO d"]
            .iter()
            .enumerate()
            .map(|(i, content)| {
                let mut l = LogLine::new(i, *content);
                l.level = if content.contains("ERROR") {
                    LogLevel::Error
                } else if content.contains("WARN") {
                    LogLevel::Warn
                } else {
                    LogLevel::Info
                };
                l
            })
            .collect();
        let selected = vec![all_lines[0].clone()];
        let (output, stats) = c.format_output(&selected, &all_lines);
        assert!(output.contains("[3 lines omitted: 1 ERROR, 1 WARN, 2 INFO]"));
        assert_eq!(stats["errors"], 1);
        assert_eq!(stats["info"], 2);
    }

    #[test]
    fn omitted_ranges_are_exact_and_coalesced() {
        let selected = vec![LogLine::new(0, "first"), LogLine::new(4, "fifth")];
        assert_eq!(
            omitted_ranges(&selected, 6),
            vec![
                OmissionRange {
                    start_line: 2,
                    line_count: 3,
                },
                OmissionRange {
                    start_line: 6,
                    line_count: 1,
                },
            ]
        );
    }

    #[test]
    fn score_line_caps_at_one_point_zero() {
        let line = LogLine {
            line_number: 0,
            content: "ERROR summary".into(),
            level: LogLevel::Error,
            is_stack_trace: true,
            is_summary: true,
            score: 0.0,
        };
        assert_eq!(score_log_line(&line), 1.0);
    }

    // ─── 帧折叠 ─────────────────────────────────────────────────────

    fn java_chained_trace(runtime_frames: usize) -> String {
        let mut lines =
            vec!["Exception in thread \"main\" java.lang.IllegalStateException: boom".to_string()];
        lines.push("at com.example.App.handle(App.java:10)".into());
        lines.push("at com.example.App.dispatch(App.java:20)".into());
        for i in 0..runtime_frames {
            lines.push(format!(
                "at java.base/java.util.stream.Op{}.eval(Op{}.java:{})",
                i,
                i,
                i + 1
            ));
        }
        lines.push("Caused by: java.io.IOException: disk gone".into());
        lines.push("at com.example.Disk.read(Disk.java:77)".into());
        for i in 0..runtime_frames {
            lines.push(format!(
                "at java.base/java.lang.Thread{}.run(Thread.java:{})",
                i,
                i + 1
            ));
        }
        lines.push("... 17 more".into());
        lines.join("\n")
    }

    #[test]
    fn collapse_keeps_chain_heads_and_app_frames() {
        let (result, stats) = cmp().compress(&java_chained_trace(30), 1.0);
        assert!(stats.runtime_frames_collapsed > 0);
        assert!(result.compressed.contains("Caused by: java.io.IOException"));
        assert!(result.compressed.contains("com.example.Disk.read"));
        assert!(result.compressed.contains("... 17 more"));
        assert!(result.compressed.contains("frames collapsed]"));
        // 深层运行时尾部被折叠
        assert!(!result.compressed.contains("Thread25.run"));
    }

    #[test]
    fn collapse_beats_blind_truncation_on_chain_heads() {
        let content = java_chained_trace(30);
        let mut cfg = LogCompressorConfig::default();
        cfg.collapse_runtime_frames = false;
        let (off, _) = LogCompressor::new(cfg).compress(&content, 1.0);
        assert!(!off.compressed.contains("com.example.Disk.read"));
        let (on, _) = cmp().compress(&content, 1.0);
        assert!(on.compressed.contains("com.example.Disk.read"));
    }

    #[test]
    fn small_traces_not_collapsed() {
        let (result, stats) = cmp().compress(&java_chained_trace(2), 1.0);
        assert_eq!(stats.runtime_frames_collapsed, 0);
        assert!(!result.compressed.contains("frames collapsed]"));
    }

    // ─── trait 接入与折叠 ───────────────────────────────────────────

    #[test]
    fn offload_transform_roundtrip() {
        let c = cmp();
        let mut content = String::new();
        for i in 0..80 {
            content.push_str(&format!("INFO line {}\n", i));
        }
        content.push_str("ERROR boom\n");
        let ctx = CompressionContext::default();
        let result = c.apply(&content, &ctx).unwrap();
        assert_eq!(result.original, content);
        assert!(result.compressed.len() < content.len());
        assert!(!result.omissions.is_empty());
        assert_eq!(c.estimate_bloat("short"), 0.0);
        assert_eq!(c.name(), "log_compressor");
        assert_eq!(c.applies_to(), ContentType::BuildOutput);
        let k = c.cache_key(&content);
        assert_eq!(k.len(), 24);
    }

    #[test]
    fn folds_repeats() {
        let input = "compiling a\ncompiling a\ncompiling a\ndone";
        assert_eq!(fold_repeated_lines(input), "compiling a  [x3]\ndone");
    }

    #[test]
    fn no_repeat_unchanged() {
        assert_eq!(fold_repeated_lines("a\nb"), "a\nb");
    }

    // ─── 归一化 ─────────────────────────────────────────────────────

    #[test]
    fn normalize_for_dedupe_examples() {
        // 无 `:`/`=` 时整行是“前缀”，原样保留（与参考实现一致）
        assert_eq!(normalize_for_dedupe("retry 3 times"), "retry 3 times");
        // 数字 → N
        assert_eq!(normalize_for_dedupe("retry: 3 times"), "retry: N times");
        // hex → ADDR
        assert_eq!(normalize_for_dedupe("at: 0xdeadbeef now"), "at: ADDR now");
        // 路径 + 数字归一化：路径段（回溯到最后一个内部 `/`）→ /PATH/，
        // 其后数字 → N——与参考实现的 regex 替换序列等价
        assert_eq!(
            normalize_for_dedupe("warn: /tmp/a/123 issue"),
            "warn: /PATH/N issue"
        );
        assert_eq!(
            normalize_for_dedupe("warn: /tmp/b/999 issue"),
            "warn: /PATH/N issue"
        );
        // 前缀（`:` 之前）原样保留
        assert_eq!(
            normalize_for_dedupe("segfault: at 0x1 in t1"),
            "segfault: at ADDR in tN"
        );
        assert_eq!(
            normalize_for_dedupe("heap overflow: at 0x2 in t1"),
            "heap overflow: at ADDR in tN"
        );
    }
}
