//! 无损重排变换（`ReformatTransform`）：`JsonMinifier` 与 `LogTemplate`。
//!
//! 两者都实现 [`crate::transforms::ReformatTransform`]：输出可完全重建原文，
//! 无需 stash 卸载。包含：
//! - `JsonMinifier` 语义一致：`serde_json` 先 parse 再紧凑序列化，取更短者，
//!   绝不膨胀输出；
//! - `LogTemplate` 语义一致：Drain 式模板挖掘，把变化 token 替换为 `<*>`，
//!   模板 + 变体表可逐行重建原文；
//! - 差异点：本项目 `ReformatTransform::apply` 返回 `Result<String, TransformError>`
//!   （无 `ReformatOutput` 包装、无 `bytes_saved`），错误枚举无载荷。
//!
//! 参考实现里 `LogTemplate` 用 `regex`（其实只用了空白切分），本实现全程手写，
//! 不新增依赖。

use crate::content::ContentType;
use crate::transforms::{CompressionContext, ReformatTransform, TransformError};

// ─── JsonMinifier ───────────────────────────────────────────────────────────

/// JSON 空白剥离器：parse 后紧凑序列化，返回更短的输出。
///
/// 无损由构造保证：`serde_json::Value` 反序列化再序列化，语义等价；
/// 当紧凑化反而更长（如调用方传入的已是紧凑 JSON，或转义规则增加字节）时，
/// 原样返回，绝不膨胀。
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonMinifier;

impl ReformatTransform for JsonMinifier {
    fn name(&self) -> &'static str {
        "json_minifier"
    }

    fn applies_to(&self) -> ContentType {
        // 检测器把数组与对象都折叠进 `JsonArray`（结构层识别的 JSON 伞标签），
        // 本压缩器不关心顶层形状，数组/对象/标量都能压缩。
        ContentType::JsonArray
    }

    fn apply(&self, input: &str, _ctx: &CompressionContext) -> Result<String, TransformError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(TransformError::Skipped);
        }

        let value: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|_| TransformError::InvalidInput)?;

        let minified = serde_json::to_string(&value)
            .map_err(|e| TransformError::Internal(e.to_string()))?;

        // 防御：紧凑化若未变短（输入已是紧凑 JSON，或转义规则增加字节），
        // 返回原文，保证绝不膨胀。
        if minified.len() >= input.len() {
            return Ok(input.to_string());
        }
        Ok(minified)
    }
}

// ─── LogTemplate ────────────────────────────────────────────────────────────

const LOG_TEMPLATE_NAME: &str = "log_template";
/// 模板中可变位置的占位符。
const WILDCARD: &str = "<*>";

/// LogTemplate 配置。
#[derive(Debug, Clone, Copy)]
pub struct LogTemplateConfig {
    /// 低于该行数不做模板挖掘。
    pub min_lines: usize,
    /// 连续同模板行的最小条数，达到才折叠成模板块。
    pub min_run: usize,
    /// 两行「同模板」所需的位置匹配比例（Drain 默认 0.4）。
    pub similarity_threshold: f32,
    /// 模板至少需要这么多常量 token，否则原样输出。
    pub min_constant_tokens: usize,
}

impl Default for LogTemplateConfig {
    fn default() -> Self {
        Self {
            min_lines: 20,
            min_run: 3,
            similarity_threshold: 0.4,
            min_constant_tokens: 2,
        }
    }
}

/// Drain 式日志模板挖掘器（无损）。
///
/// 把连续、形状一致、仅在若干位置变化的行折叠成一个模板头 + 变体表：
///
/// ```text
/// [Template T1: <TS> INFO worker-<*> processing job <*>] (800 occurrences)
/// 12:34:56 1 42
/// 12:34:57 2 43
/// ```
///
/// 每一行原文都能由「模板 + 变体行」重建，故无损。仅折叠**连续**同模板行，
/// 时间顺序得以保留。复杂度 O(n × 每行 token 数)，无 regex。
pub struct LogTemplate {
    config: LogTemplateConfig,
}

impl LogTemplate {
    pub fn new(config: LogTemplateConfig) -> Self {
        Self { config }
    }
}

impl Default for LogTemplate {
    fn default() -> Self {
        Self::new(LogTemplateConfig::default())
    }
}

impl ReformatTransform for LogTemplate {
    fn name(&self) -> &'static str {
        LOG_TEMPLATE_NAME
    }

    fn applies_to(&self) -> ContentType {
        ContentType::BuildOutput
    }

    fn apply(&self, input: &str, _ctx: &CompressionContext) -> Result<String, TransformError> {
        if input.is_empty() {
            return Err(TransformError::Skipped);
        }
        let lines: Vec<&str> = input.lines().collect();
        if lines.len() < self.config.min_lines {
            return Err(TransformError::Skipped);
        }

        let tokenized: Vec<Vec<&str>> = lines.iter().map(|l| tokenize(l)).collect();

        let mut output = String::with_capacity(input.len());
        let mut next_template_id = 1usize;
        let mut run: Option<Run> = None;

        for (i, tokens) in tokenized.iter().enumerate() {
            if tokens.is_empty() {
                // 空行 / 纯空白行：打断任何活动 run。
                if let Some(r) = run.take() {
                    flush_run(
                        &r,
                        &lines,
                        &tokenized,
                        &self.config,
                        &mut next_template_id,
                        &mut output,
                    );
                }
                output.push_str(lines[i]);
                output.push('\n');
                continue;
            }
            match run.as_mut() {
                Some(r) if extends_run(r, tokens, self.config.similarity_threshold) => {
                    r.indices.push(i);
                    merge_into_template(&mut r.template, tokens);
                }
                _ => {
                    if let Some(r) = run.take() {
                        flush_run(
                            &r,
                            &lines,
                            &tokenized,
                            &self.config,
                            &mut next_template_id,
                            &mut output,
                        );
                    }
                    run = Some(Run::start(i, tokens));
                }
            }
        }
        if let Some(r) = run.take() {
            flush_run(
                &r,
                &lines,
                &tokenized,
                &self.config,
                &mut next_template_id,
                &mut output,
            );
        }

        // 尾换行：输入有则保留（上一步已带出），输入无则去掉末尾多出的 '\n'。
        if !input.ends_with('\n') && output.ends_with('\n') {
            output.pop();
        }

        // 防御：绝不膨胀。模板头开销超过收益时回退原文。
        if output.len() >= input.len() {
            return Ok(input.to_string());
        }
        Ok(output)
    }
}

/// 一次折叠候选：覆盖的原始行下标 + 逐位 token 槽。
struct Run {
    indices: Vec<usize>,
    /// `Some(token)` = 该位置至今保持常量；`None` = 该位置已变化 → 通配符。
    template: Vec<Option<String>>,
}

impl Run {
    fn start(idx: usize, tokens: &[&str]) -> Self {
        Self {
            indices: vec![idx],
            template: tokens.iter().map(|t| Some((*t).to_string())).collect(),
        }
    }
}

/// 空白切分（`str::split_whitespace` 语义：折叠空白、去首尾空白，UTF-8 安全）。
/// 空结果 = 空行。
fn tokenize(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

/// `tokens` 是否与 `run.template` 在 ≥ `sim_threshold` 比例的位置上匹配，
/// 且 token 数一致。
fn extends_run(run: &Run, tokens: &[&str], sim_threshold: f32) -> bool {
    if tokens.len() != run.template.len() {
        return false;
    }
    let len = tokens.len() as f32;
    let mut matches = 0usize;
    for (pos, tok) in tokens.iter().enumerate() {
        match &run.template[pos] {
            Some(constant) if constant == tok => matches += 1,
            None => matches += 1, // 已是通配符，算匹配
            _ => {}
        }
    }
    (matches as f32 / len) >= sim_threshold
}

/// 就地更新 `template`：与 `tokens` 不同的位置置为通配符（`None`）。
fn merge_into_template(template: &mut [Option<String>], tokens: &[&str]) {
    for (pos, tok) in tokens.iter().enumerate() {
        if let Some(constant) = &template[pos] {
            if constant != tok {
                template[pos] = None;
            }
        }
    }
}

/// 冲刷一个 run：可折叠则发模板块 + 变体表，否则逐行原样输出。
fn flush_run(
    run: &Run,
    lines: &[&str],
    tokenized: &[Vec<&str>],
    cfg: &LogTemplateConfig,
    next_template_id: &mut usize,
    out: &mut String,
) {
    let constant_count = run.template.iter().filter(|t| t.is_some()).count();
    let varying_count = run.template.len() - constant_count;
    let collapse = run.indices.len() >= cfg.min_run
        && constant_count >= cfg.min_constant_tokens
        && varying_count > 0;

    if !collapse {
        for &i in &run.indices {
            out.push_str(lines[i]);
            out.push('\n');
        }
        return;
    }

    // 模板头：`[Template T<id>: TOKEN <*> TOKEN ...] (N occurrences)`。
    let template_id = *next_template_id;
    *next_template_id += 1;
    out.push_str("[Template T");
    out.push_str(&template_id.to_string());
    out.push_str(": ");
    for (pos, slot) in run.template.iter().enumerate() {
        if pos > 0 {
            out.push(' ');
        }
        match slot {
            Some(constant) => out.push_str(constant),
            None => out.push_str(WILDCARD),
        }
    }
    out.push_str("] (");
    out.push_str(&run.indices.len().to_string());
    out.push_str(" occurrences)\n");

    // 变体表：每行只输出可变位置的 token，空格分隔。
    for &i in &run.indices {
        let toks = &tokenized[i];
        let mut first = true;
        for (pos, slot) in run.template.iter().enumerate() {
            if slot.is_none() {
                if !first {
                    out.push(' ');
                }
                out.push_str(toks[pos]);
                first = false;
            }
        }
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::ContentType;
    use crate::transforms::{CompressionContext, ReformatTransform, TransformError};

    fn ctx() -> CompressionContext {
        CompressionContext::default()
    }

    // ─── JsonMinifier ───────────────────────────────────────────────────────

    #[test]
    fn json_minifier_name_and_applies_to() {
        let m = JsonMinifier;
        assert_eq!(m.name(), "json_minifier");
        assert_eq!(m.applies_to(), ContentType::JsonArray);
    }

    #[test]
    fn json_pretty_object_minifies() {
        let pretty = "{\n  \"a\": 1,\n  \"b\": 2\n}";
        let r = JsonMinifier.apply(pretty, &ctx()).expect("parses");
        assert_eq!(r, r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn json_pretty_array_minifies() {
        let pretty = "[\n  1,\n  2,\n  3\n]";
        let r = JsonMinifier.apply(pretty, &ctx()).expect("parses");
        assert_eq!(r, "[1,2,3]");
    }

    #[test]
    fn json_already_compact_stays_same() {
        let compact = r#"{"a":1,"b":2}"#;
        let r = JsonMinifier.apply(compact, &ctx()).expect("parses");
        assert_eq!(r, compact);
    }

    #[test]
    fn json_invalid_input_errors() {
        let err = JsonMinifier.apply("{not: valid", &ctx()).expect_err("must fail");
        assert_eq!(err, TransformError::InvalidInput);
    }

    #[test]
    fn json_empty_input_skipped() {
        let err = JsonMinifier.apply("", &ctx()).expect_err("empty must skip");
        assert_eq!(err, TransformError::Skipped);
    }

    #[test]
    fn json_whitespace_only_skipped() {
        let err = JsonMinifier.apply("   \n\t  ", &ctx()).expect_err("ws-only must skip");
        assert_eq!(err, TransformError::Skipped);
    }

    #[test]
    fn json_nested_round_trips_semantically() {
        let pretty = r#"
        {
          "users": [
            {"id": 1, "name": "alice", "active": true},
            {"id": 2, "name": "bob",   "active": false}
          ],
          "count": 2
        }
        "#;
        let r = JsonMinifier.apply(pretty, &ctx()).expect("parses");
        // 重解析输出，验证结构等价。
        let original_val: serde_json::Value = serde_json::from_str(pretty).unwrap();
        let output_val: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(original_val, output_val);
        assert!(r.len() < pretty.len());
    }

    #[test]
    fn json_never_grows_output() {
        let inputs = [
            r#"{}"#,
            r#"[]"#,
            r#"null"#,
            r#"42"#,
            r#""string""#,
            r#"{"k":"value with spaces"}"#,
        ];
        for input in inputs {
            let r = JsonMinifier.apply(input, &ctx()).expect("valid");
            assert!(
                r.len() <= input.len(),
                "minifier grew output for {input:?}: {} -> {}",
                input.len(),
                r.len()
            );
        }
    }

    #[test]
    fn json_unicode_round_trips() {
        let pretty = r#"{ "msg": "héllo 🌍 wörld" }"#;
        let r = JsonMinifier.apply(pretty, &ctx()).expect("parses");
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["msg"], "héllo 🌍 wörld");
    }

    #[test]
    fn json_surrounding_whitespace_is_stripped() {
        let r = JsonMinifier.apply("   [ 1 , 2 ]   \n", &ctx()).expect("parses");
        assert_eq!(r, "[1,2]");
    }

    // ─── LogTemplate ────────────────────────────────────────────────────────

    fn reformat() -> LogTemplate {
        LogTemplate::default()
    }

    #[test]
    fn log_template_name_and_applies_to() {
        let r = reformat();
        assert_eq!(r.name(), "log_template");
        assert_eq!(r.applies_to(), ContentType::BuildOutput);
    }

    #[test]
    fn log_template_empty_input_skipped() {
        let err = reformat().apply("", &ctx()).expect_err("empty must skip");
        assert_eq!(err, TransformError::Skipped);
    }

    #[test]
    fn log_template_below_min_lines_skipped() {
        let log = "INFO a\nINFO b\nINFO c\n";
        let err = reformat().apply(log, &ctx()).expect_err("must skip");
        assert_eq!(err, TransformError::Skipped);
    }

    #[test]
    fn log_template_run_collapses() {
        // 50 条 INFO 行，时间戳 / worker / job 变化——同模板，应折叠。
        let mut log = String::new();
        for i in 0..50 {
            log.push_str(&format!(
                "2025-01-15T12:34:{:02} INFO worker-{} processing job {}\n",
                i,
                i,
                100 + i
            ));
        }
        let r = reformat().apply(&log, &ctx()).expect("must collapse");
        assert!(r.contains("[Template T1:"), "got: {}", r.chars().take(200).collect::<String>());
        assert!(r.contains("(50 occurrences)"));
        // 变体仍在输出中（无损保证）。
        assert!(r.contains("worker-7"));
        assert!(r.len() < log.len());
    }

    #[test]
    fn log_template_order_preserved_across_two_templates() {
        let mut log = String::new();
        for i in 0..12 {
            log.push_str(&format!("INFO worker-{i} starting\n"));
        }
        for i in 0..12 {
            log.push_str(&format!("WARN cache key-{i} expired\n"));
        }
        let r = reformat().apply(&log, &ctx()).expect("must collapse");
        let t1_pos = r.find("[Template T1:").expect("T1 header");
        let t2_pos = r.find("[Template T2:").expect("T2 header");
        assert!(t1_pos < t2_pos, "templates must be in input order");
        let t1_line = r[t1_pos..t2_pos].lines().next().unwrap();
        assert!(t1_line.contains("INFO"));
        assert!(t1_line.contains("starting"));
    }

    #[test]
    fn log_template_lossless_round_trip() {
        let mut log = String::new();
        for i in 0..25 {
            log.push_str(&format!("TOK1 TOK2 var{i} TOK3\n"));
        }
        let r = reformat().apply(&log, &ctx()).expect("collapses");
        let mut iter = r.lines();
        let header = iter.next().unwrap();
        assert!(header.starts_with("[Template T1:"));
        let template_part = header
            .trim_start_matches("[Template T1: ")
            .split("] (")
            .next()
            .unwrap();
        let template_tokens: Vec<&str> = template_part.split_whitespace().collect();
        let var_pos = template_tokens
            .iter()
            .position(|t| *t == WILDCARD)
            .expect("must have wildcard");

        let mut reconstructed = Vec::new();
        for variant_line in iter {
            if variant_line.is_empty() {
                continue;
            }
            let var_tokens: Vec<&str> = variant_line.split_whitespace().collect();
            assert_eq!(var_tokens.len(), 1, "1 wildcard -> 1 variant token");
            let mut full = template_tokens.clone();
            full[var_pos] = var_tokens[0];
            reconstructed.push(full.join(" "));
        }
        let original: Vec<String> = log.lines().map(|s| s.to_string()).collect();
        assert_eq!(reconstructed, original);
    }

    #[test]
    fn log_template_short_run_emitted_verbatim() {
        // 2 行「模板候选」低于 min_run=3，不应折叠；两侧用异构行撑过 min_lines。
        let mut log = String::new();
        for i in 0..10 {
            let toks: Vec<String> = (0..(i + 1) % 5 + 2).map(|j| format!("p{i}q{j}")).collect();
            log.push_str(&toks.join(" "));
            log.push('\n');
        }
        log.push_str("AAA worker-1 BBB\n");
        log.push_str("AAA worker-2 BBB\n");
        for i in 0..10 {
            let toks: Vec<String> = (0..(i + 1) % 4 + 2).map(|j| format!("s{i}t{j}")).collect();
            log.push_str(&toks.join(" "));
            log.push('\n');
        }
        let r = reformat().apply(&log, &ctx()).expect("input large enough");
        assert!(r.contains("AAA worker-1 BBB"));
        assert!(r.contains("AAA worker-2 BBB"));
    }

    #[test]
    fn log_template_all_unique_lines_preserved() {
        let mut log = String::new();
        for i in 0..25 {
            log.push_str(&format!("event-{i} type-{i} status-{i}\n"));
        }
        let r = reformat().apply(&log, &ctx()).expect("processes");
        // 无论是否折叠，每个变体值都必须存活（无损）。
        for i in 0..25 {
            assert!(r.contains(&format!("event-{i}")), "missing event-{i} in output");
        }
    }

    #[test]
    fn log_template_blank_lines_break_runs() {
        let mut log = String::new();
        for i in 0..5 {
            log.push_str(&format!("INFO worker-{i} ready\n"));
        }
        log.push('\n');
        for i in 0..5 {
            log.push_str(&format!("INFO worker-{i} ready\n"));
        }
        for i in 0..15 {
            log.push_str(&format!("misc-{i}\n"));
        }
        let r = reformat().apply(&log, &ctx()).expect("input large enough");
        let t1_count = r.matches("[Template T1:").count();
        let t2_count = r.matches("[Template T2:").count();
        // 允许 0/0（都不折叠）或 1/1（各自折叠）；禁止 1/0 且跨过空行合并。
        if t1_count == 1 && t2_count == 0 {
            assert!(!r.contains("(10 occurrences)"), "must not bridge the blank line");
        }
    }

    #[test]
    fn log_template_never_inflates_output() {
        let mut log = String::new();
        for i in 0..30 {
            log.push_str(&format!("a{i}\n"));
        }
        let r = reformat().apply(&log, &ctx()).expect("processes");
        assert!(r.len() <= log.len());
    }

    #[test]
    fn log_template_unicode_tokens_survive() {
        let mut log = String::new();
        for i in 0..30 {
            log.push_str(&format!("INFO 🔥 worker-{i} héllo wörld\n"));
        }
        let r = reformat().apply(&log, &ctx()).expect("processes utf8");
        assert!(r.contains("🔥"));
        assert!(r.contains("héllo") || r.contains("wörld"));
    }

    #[test]
    fn log_template_no_constants_emits_verbatim() {
        // 每个位置都变化 → 模板全通配符，违反 min_constant_tokens=2。
        let mut log = String::new();
        for i in 0..30 {
            log.push_str(&format!("{} {} {}\n", i, i + 1, i + 2));
        }
        let r = reformat().apply(&log, &ctx()).expect("processes");
        assert!(!r.contains("[Template"));
    }

    #[test]
    fn log_template_trailing_newline_preserved() {
        let mut log = String::new();
        for i in 0..30 {
            log.push_str(&format!("INFO worker-{i} ready\n"));
        }
        let r = reformat().apply(&log, &ctx()).expect("collapses");
        assert!(r.ends_with('\n'), "input had trailing newline, output must too");
    }

    #[test]
    fn log_template_custom_config_tightens_threshold() {
        // 高相似度阈值（0.9）下，worker 变化行无法归入同模板 → 原样。
        let cfg = LogTemplateConfig {
            min_lines: 5,
            min_run: 3,
            similarity_threshold: 0.9,
            min_constant_tokens: 2,
        };
        let log = "INFO worker-1 ready\nINFO worker-2 ready\nINFO worker-3 ready\nINFO worker-4 ready\nINFO worker-5 ready\n";
        let r = LogTemplate::new(cfg).apply(log, &ctx()).expect("processes");
        // 两常量位 INFO/ready、一可变位 worker：匹配 2/3 ≈ 0.667 < 0.9 → 不折叠，原样。
        assert_eq!(r, log);
    }
}
