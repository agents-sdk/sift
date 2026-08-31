//! SmartCrusher：JSON 数组的统计压缩。
//!
//! # 移植说明
//!
//! 参考实现追求与 Python 的字节级 parity（Kneedle 自适应 K、md5 内容哈希、
//! stash store、TOIN/feedback/telemetry 等子系统）。本实现不做 parity，只做
//! **算法行为等价**：同样的输入得到信息等价的压缩输出——
//!
//! - 错误行（error/failed/fatal/... 关键字）永不丢弃；
//! - 罕见 status 值（Pareto 80% 主流值之外的取值）保留；
//! - 结构异类（出现在 <20% 行中的字段）保留；
//! - 数值异常（>2σ 偏离均值）与变化点邻域保留；
//! - 保留 first/last/middle 采样锚点，重复行按内容哈希折叠；
//! - 被丢弃的行在输出尾部的 `_crushed` 哨兵对象中标注（计数 + 采样）。
//!
//! 相对参考实现的简化：
//! - 无 lossless compaction 阶段（CSV/buckets）、无 stash store / prose hook /
//!   observer / constraint trait 对象（错误行与结构异类保留直接内联）；
//! - 自适应 K 用简单的 `clamp(n/4, 3, max_items)` 代替 Kneedle 算法；
//! - 内容哈希用 blake3（允许依赖）代替 md5；
//! - 查询锚点提取用无 regex 的手写扫描器（UUID / 4 位以上数字 / 邮箱 /
//!   主机名 / 引号字符串），匹配语义与参考一致（小写子串包含）；
//! - relevance 打分简化为锚点精确匹配（确定性），无概率 BM25。
//!
//! 实现的是 [`OffloadTransform`]：压缩是有损的（丢弃行 + 采样标注），
//! 原文由调用方进 offload store，`apply` 返回结构化卸载结果。

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::content::ContentType;
use crate::transforms::{CompressionContext, OffloadOutput, OffloadTransform, TransformError};

// ────────────────────────────── 配置 ──────────────────────────────

/// SmartCrusher 配置（参考 `SmartCrusherConfig` 的子集，默认值一致）。
#[derive(Debug, Clone)]
pub struct SmartCrusherConfig {
    /// 低于该元素数的数组不做分析。
    pub min_items_to_analyze: usize,
    /// 压缩后输出保留的最大行数。
    pub max_items_after_crush: usize,
    /// 偏离均值多少个样本标准差算数值异常 / 变化点。
    pub variance_threshold: f64,
    /// 字段唯一值比例低于该值视为"近常量"（可安全采样）。
    pub uniqueness_threshold: f64,
    /// K 预算中分配给数组头部的比例。
    pub first_fraction: f64,
    /// K 预算中分配给数组尾部的比例。
    pub last_fraction: f64,
    /// 是否保留变化点邻域。
    pub preserve_change_points: bool,
    /// 是否在去重阶段折叠内容相同的行。
    pub dedup_identical_items: bool,
    /// dropped 采样在哨兵中最多展示的行数。
    pub max_dropped_sample: usize,
}

impl Default for SmartCrusherConfig {
    fn default() -> Self {
        // 默认值与参考实现 `SmartCrusherConfig::default()` 对齐。
        SmartCrusherConfig {
            min_items_to_analyze: 5,
            max_items_after_crush: 15,
            variance_threshold: 2.0,
            uniqueness_threshold: 0.1,
            first_fraction: 0.3,
            last_fraction: 0.15,
            preserve_change_points: true,
            dedup_identical_items: true,
            max_dropped_sample: 3,
        }
    }
}

// ────────────────────────────── 类型 ──────────────────────────────

/// 压缩策略（对应参考 `CompressionStrategy`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionStrategy {
    None,
    Skip,
    TimeSeries,
    ClusterSample,
    TopN,
    SmartSample,
}

impl CompressionStrategy {
    /// 与参考实现一致的小写策略名（出现在输出标注里）。
    pub fn as_str(self) -> &'static str {
        match self {
            CompressionStrategy::None => "none",
            CompressionStrategy::Skip => "skip",
            CompressionStrategy::TimeSeries => "time_series",
            CompressionStrategy::ClusterSample => "cluster",
            CompressionStrategy::TopN => "top_n",
            CompressionStrategy::SmartSample => "smart_sample",
        }
    }
}

/// 单个字段的跨行统计（参考 `FieldStats` 的子集）。
#[derive(Debug, Clone)]
pub struct FieldStats {
    pub name: String,
    /// `"numeric" | "string" | "boolean" | "object" | "array" | "null" | "mixed"`
    pub field_type: String,
    pub count: usize,
    pub unique_count: usize,
    pub unique_ratio: f64,
    pub min_val: Option<f64>,
    pub max_val: Option<f64>,
    pub mean_val: Option<f64>,
    pub variance: Option<f64>,
    /// 数值字段的 变化点 索引（窗口均值跳变 > variance_threshold·σ）。
    pub change_points: Vec<usize>,
}

/// 数组分析结果（参考 `ArrayAnalysis` 的子集）。
#[derive(Debug, Clone)]
pub struct ArrayAnalysis {
    pub item_count: usize,
    pub field_stats: BTreeMap<String, FieldStats>,
    pub recommended_strategy: CompressionStrategy,
    /// 不可压时的人类可读原因（对应参考 crushability.reason）。
    pub skip_reason: Option<String>,
}

/// 压缩计划：保留原始数组中的哪些下标。
#[derive(Debug, Clone)]
pub struct CompressionPlan {
    pub strategy: CompressionStrategy,
    pub keep_indices: Vec<usize>,
    pub cluster_field: Option<String>,
    pub sort_field: Option<String>,
}

/// JSON 数组元素类型分类（对应参考 `classify_array`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayType {
    DictArray,
    StringArray,
    NumberArray,
    BoolArray,
    NestedArray,
    MixedArray,
    Empty,
}

/// 遍历全部元素做类型分类（参考实现同样全量遍历，不采样）。
pub fn classify_array(items: &[Value]) -> ArrayType {
    if items.is_empty() {
        return ArrayType::Empty;
    }
    let (mut b, mut n, mut s, mut o, mut a, mut nu) = (false, false, false, false, false, false);
    for item in items {
        match item {
            Value::Bool(_) => b = true,
            Value::Number(_) => n = true,
            Value::String(_) => s = true,
            Value::Object(_) => o = true,
            Value::Array(_) => a = true,
            Value::Null => nu = true,
        }
    }
    // 纯类型判定；含 null 或混合类型 → MixedArray（与参考一致）。
    if b && !(n || s || o || a || nu) {
        ArrayType::BoolArray
    } else if o && !(b || n || s || a || nu) {
        ArrayType::DictArray
    } else if s && !(b || n || o || a || nu) {
        ArrayType::StringArray
    } else if n && !(b || s || o || a || nu) {
        ArrayType::NumberArray
    } else if a && !(b || n || s || o || nu) {
        ArrayType::NestedArray
    } else {
        ArrayType::MixedArray
    }
}

// ────────────────────────── 统计与哈希工具 ──────────────────────────

fn mean(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    Some(xs.iter().sum::<f64>() / xs.len() as f64)
}

/// 样本方差（n-1 分母，对应 Python `statistics.variance`）。
fn sample_variance(xs: &[f64]) -> Option<f64> {
    if xs.len() < 2 {
        return None;
    }
    let m = mean(xs)?;
    Some(xs.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (xs.len() - 1) as f64)
}

fn sample_stdev(xs: &[f64]) -> Option<f64> {
    sample_variance(xs).map(|v| v.sqrt())
}

/// 线性插值分位数（numpy "linear" 方法，对应参考 bug#1 修复后的实现）。
fn percentile_linear(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let idx = q * (sorted.len() - 1) as f64;
    let lo = idx.floor() as usize;
    let hi = idx.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = idx - lo as f64;
        sorted[lo] * (1.0 - frac) + sorted[hi] * frac
    }
}

/// 内容哈希：canonical JSON（key 排序）→ blake3 前 16 个 hex 字符。
/// 用于重复行折叠；参考实现用 md5[:16]，这里换用允许依赖的 blake3。
fn content_hash(item: &Value) -> String {
    let canonical = canonical_json(item);
    blake3::hash(canonical.as_bytes()).to_hex()[..16].to_string()
}

/// key 排序的紧凑 JSON 序列化，保证 {"b":2,"a":1} 与 {"a":1,"b":2} 同哈希。
fn canonical_json(v: &Value) -> String {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let body: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{}:{}", Value::String(k.clone()), canonical_json(&map[k])))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", body.join(","))
        }
        other => other.to_string(),
    }
}

/// 值的字符串化（用于唯一值集合 / 频次统计，与参考 `stringify` 对应）。
fn stringify_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ────────────────────────── 错误关键字 ──────────────────────────

/// 保留信号关键字（对应参考 `ERROR_KEYWORDS`，全小写，子串匹配）。
pub const ERROR_KEYWORDS: &[&str] = &[
    "error",
    "exception",
    "failed",
    "failure",
    "critical",
    "fatal",
    "crash",
    "panic",
    "abort",
    "timeout",
    "denied",
    "rejected",
];

/// 检测包含错误关键字的行（大小写不敏感）。参考 `detect_error_items_for_preservation`。
pub fn detect_error_items(items: &[Value]) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if !item.is_object() {
            continue;
        }
        let serialized = item.to_string().to_lowercase();
        if ERROR_KEYWORDS.iter().any(|kw| serialized.contains(kw)) {
            out.push(i);
        }
    }
    out
}

// ────────────────────────── 异类检测 ──────────────────────────

/// 检测罕见 status 值（参考 bug#3 修复后的 Pareto 算法）：
/// 1. 字段基数在 2..=50；
/// 2. 频次降序累加，找到覆盖 ≥80% 行的最小 top-K；
/// 3. 若 K ≤ 5，top-K 之外的取值即"罕见"，含罕见值的行是异类。
pub fn detect_rare_status_values(items: &[Value], common_fields: &HashSet<String>) -> Vec<usize> {
    let mut outliers = Vec::new();
    let mut fields: Vec<&String> = common_fields.iter().collect();
    fields.sort(); // 排序迭代保证确定性

    for field in fields {
        let values: Vec<&Value> = items
            .iter()
            .filter_map(|it| it.as_object())
            .filter_map(|m| m.get(field.as_str()))
            .collect();
        // 基数统计排除 null（对应参考 `{str(v) for v in values if v is not None}`）。
        let unique: BTreeSet<String> = values
            .iter()
            .filter(|v| !v.is_null())
            .map(|v| stringify_value(v))
            .collect();
        if !(2..=50).contains(&unique.len()) {
            continue;
        }

        // 频次表：null → "__none__"。
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for v in &values {
            let key = if v.is_null() {
                "__none__".to_string()
            } else {
                stringify_value(v)
            };
            *counts.entry(key).or_insert(0) += 1;
        }
        let total = values.len();
        let mut sorted_counts: Vec<(&String, &usize)> = counts.iter().collect();
        sorted_counts.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));

        // Pareto：最小 K 使 top-K 覆盖 ≥80%。
        let threshold = (total as f64 * 0.8).ceil() as usize;
        let mut cum = 0usize;
        let mut top_k: HashSet<String> = HashSet::new();
        for (val, cnt) in &sorted_counts {
            cum += **cnt;
            top_k.insert((*val).clone());
            if cum >= threshold {
                break;
            }
        }
        if top_k.len() > 5 {
            continue; // 分布太均匀，无法定义"罕见"
        }

        for (i, item) in items.iter().enumerate() {
            if let Some(v) = item.as_object().and_then(|m| m.get(field.as_str())) {
                let key = if v.is_null() {
                    "__none__".to_string()
                } else {
                    stringify_value(v)
                };
                if !top_k.contains(&key) {
                    outliers.push(i);
                }
            }
        }
    }
    outliers
}

/// 结构异类检测：罕见字段（出现率 <20%）+ 罕见 status 值。
/// 参考 `detect_structural_outliers`。
pub fn detect_structural_outliers(items: &[Value]) -> Vec<usize> {
    if items.len() < 5 {
        return Vec::new();
    }
    let n = items.len();
    let mut field_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for item in items.iter().filter_map(|v| v.as_object()) {
        for key in item.keys() {
            *field_counts.entry(key.as_str()).or_insert(0) += 1;
        }
    }
    // 常见字段（≥80%）：status 值检测只在这些字段里找，避免把 ID 列当枚举。
    let common: HashSet<String> = field_counts
        .iter()
        .filter(|(_, &c)| c as f64 >= n as f64 * 0.8)
        .map(|(k, _)| (*k).to_string())
        .collect();
    let rare: HashSet<&str> = field_counts
        .iter()
        .filter(|(_, &c)| (c as f64) < n as f64 * 0.2)
        .map(|(k, _)| *k)
        .collect();

    let mut outliers: BTreeSet<usize> = BTreeSet::new();
    for (i, item) in items.iter().enumerate() {
        if let Some(obj) = item.as_object() {
            if obj.keys().any(|k| rare.contains(k.as_str())) {
                outliers.insert(i);
            }
        }
    }
    outliers.extend(detect_rare_status_values(items, &common));
    outliers.into_iter().collect()
}

// ────────────────────────── 数组分析 ──────────────────────────

/// 对对象数组做逐字段统计（schema 提取与去重的核心）。
/// 参考 `SmartAnalyzer::analyze_array`：字段集合是所有行的 key 并集；
/// 每个字段统计类型 / 唯一值 / 数值分布 / 变化点。
pub fn analyze_array(items: &[Value], config: &SmartCrusherConfig) -> ArrayAnalysis {
    let n = items.len();
    // 收集字段 → 每行的取值（缺失跳过）。
    let mut field_values: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for item in items {
        if let Some(obj) = item.as_object() {
            for (k, v) in obj {
                field_values.entry(k.clone()).or_default().push(v);
            }
        }
    }

    let mut field_stats: BTreeMap<String, FieldStats> = BTreeMap::new();
    for (name, values) in &field_values {
        let mut type_counts: BTreeMap<&str, usize> = BTreeMap::new();
        let mut unique: BTreeSet<String> = BTreeSet::new();
        let mut finite: Vec<f64> = Vec::new();
        for v in values {
            let t = match v {
                Value::Null => "null",
                Value::Bool(_) => "boolean",
                Value::Number(_) => {
                    if let Some(f) = v.as_f64().filter(|f| f.is_finite()) {
                        finite.push(f);
                    }
                    "numeric"
                }
                Value::String(_) => "string",
                Value::Object(_) => "object",
                Value::Array(_) => "array",
            };
            *type_counts.entry(t).or_insert(0) += 1;
            if !v.is_null() {
                unique.insert(stringify_value(v));
            }
        }
        // 主类型 = 出现次数最多的类型（并列取字典序，保证确定）。
        let field_type = type_counts
            .iter()
            .max_by_key(|(t, c)| (**c, std::cmp::Reverse(*t)))
            .map(|(t, _)| (*t).to_string())
            .unwrap_or_else(|| "null".to_string());
        let is_pure_numeric = type_counts.len() == 1 && type_counts.contains_key("numeric");

        let count = values.len();
        let (min_val, max_val, mean_val, variance) = if is_pure_numeric && !finite.is_empty() {
            let mn = finite.iter().cloned().fold(f64::INFINITY, f64::min);
            let mx = finite.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            (Some(mn), Some(mx), mean(&finite), sample_variance(&finite))
        } else {
            (None, None, None, None)
        };

        // 变化点：相邻差分序列中，偏离差分均值超过 variance_threshold·σ_d 的位置。
        // 用差分域而非原始域（阶跃信号的原始域 σ 过大，会把真正的跳变淹没）。
        let mut change_points = Vec::new();
        if is_pure_numeric && n > 10 {
            let nums: Vec<Option<f64>> = items
                .iter()
                .map(|it| {
                    it.as_object()
                        .and_then(|o| o.get(name.as_str()))
                        .and_then(|v| v.as_f64())
                })
                .collect();
            let diffs: Vec<f64> = nums
                .windows(2)
                .filter_map(|w| match (w[0], w[1]) {
                    (Some(a), Some(b)) => Some(b - a),
                    _ => None,
                })
                .collect();
            if let (Some(dm), Some(ds)) = (mean(&diffs), sample_stdev(&diffs)) {
                if ds > 0.0 {
                    let threshold = config.variance_threshold * ds;
                    for i in 1..n {
                        if let (Some(a), Some(b)) = (nums[i - 1], nums[i]) {
                            if (b - a - dm).abs() > threshold {
                                change_points.push(i);
                            }
                        }
                    }
                }
            }
        }

        let unique_ratio = if count > 0 {
            unique.len() as f64 / count as f64
        } else {
            0.0
        };
        field_stats.insert(
            name.clone(),
            FieldStats {
                name: name.clone(),
                field_type,
                count,
                unique_count: unique.len(),
                unique_ratio,
                min_val,
                max_val,
                mean_val,
                variance,
                change_points,
            },
        );
    }

    // ── 策略推断与可压性 gate（简化自参考 analyzer + field_detect）──
    let error_count = detect_error_items(items).len();
    // "数值异常"信号 = 存在超出 variance_threshold·σ 的具体行（而非仅有方差）。
    // 否则任何自增 ID 列都会误判为可压信号。
    let has_anomaly = field_stats.values().any(|s| {
        if s.field_type != "numeric" {
            return false;
        }
        let (Some(m), Some(var)) = (s.mean_val, s.variance) else {
            return false;
        };
        let sigma = var.sqrt();
        if sigma <= 0.0 {
            return false;
        }
        let threshold = config.variance_threshold * sigma;
        items.iter().any(|it| {
            it.as_object()
                .and_then(|o| o.get(s.name.as_str()))
                .and_then(|v| v.as_f64())
                .map(|num| num.is_finite() && (num - m).abs() > threshold)
                .unwrap_or(false)
        })
    });
    let score_field = detect_score_field(&field_stats);
    let has_change_points = field_stats.values().any(|s| !s.change_points.is_empty());
    // 近常量字段（唯一值比例低）→ 采样安全。
    let has_low_uniqueness = field_stats
        .values()
        .any(|s| s.unique_ratio < 0.1 && s.count >= 2);

    if n < 5 {
        return ArrayAnalysis {
            item_count: n,
            field_stats,
            recommended_strategy: CompressionStrategy::None,
            skip_reason: Some("too_small".to_string()),
        };
    }

    // 参考 "unique_entities_no_signal"：全字段近乎唯一（ID 表）且没有
    // 错误行 / 数值信号 / 打分字段 → 不可压（无信号决定哪些行重要）。
    let crushable = has_low_uniqueness || error_count > 0 || has_anomaly || score_field.is_some();
    if !crushable {
        return ArrayAnalysis {
            item_count: n,
            field_stats,
            recommended_strategy: CompressionStrategy::Skip,
            skip_reason: Some("unique_entities_no_signal".to_string()),
        };
    }

    let strategy = if score_field.is_some() {
        CompressionStrategy::TopN
    } else if has_change_points {
        CompressionStrategy::TimeSeries
    } else {
        // 高唯一度字符串字段 + 存在低基数枚举字段 → 日志形态，聚类采样。
        let has_message_field = detect_message_field(&field_stats).is_some();
        let has_enum = field_stats
            .values()
            .any(|s| s.field_type == "string" && s.unique_ratio <= 0.3 && s.count >= 5);
        if has_message_field && has_enum {
            CompressionStrategy::ClusterSample
        } else {
            CompressionStrategy::SmartSample
        }
    };

    ArrayAnalysis {
        item_count: n,
        field_stats,
        recommended_strategy: strategy,
        skip_reason: None,
    }
}

/// 打分字段检测（简化自参考 `detect_score_field_statistically`）：
/// 数值字段 + 名称含 score 类关键词，或有界 [0,1] 范围。
fn detect_score_field(stats: &BTreeMap<String, FieldStats>) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "score",
        "rating",
        "priority",
        "confidence",
        "relevance",
        "rank",
    ];
    let mut best: Option<(String, f64)> = None;
    for (name, s) in stats {
        if s.field_type != "numeric" {
            continue;
        }
        let lower = name.to_lowercase();
        let kw = KEYWORDS.iter().any(|k| lower.contains(k));
        let bounded = s.min_val.unwrap_or(f64::NAN) >= 0.0
            && s.max_val.unwrap_or(f64::NAN) <= 1.0
            && s.unique_count > 1;
        if kw || bounded {
            let span = s.max_val.unwrap_or(0.0) - s.min_val.unwrap_or(0.0);
            if best.as_ref().map(|(_, b)| span > *b).unwrap_or(true) {
                best = Some((name.clone(), span));
            }
        }
    }
    best.map(|(n, _)| n)
}

/// 消息字段检测：唯一度 >0.3 的字符串字段中基数最高者。
fn detect_message_field(stats: &BTreeMap<String, FieldStats>) -> Option<String> {
    stats
        .values()
        .filter(|s| s.field_type == "string" && s.unique_ratio > 0.3)
        .max_by_key(|s| s.unique_count)
        .map(|s| s.name.clone())
}

// ────────────────────────── 查询锚点 ──────────────────────────

/// 从用户 query 中提取锚点（简化自参考 `extract_query_anchors`，无 regex）：
/// - 引号字符串（≥2 字符）；
/// - 4 位以上连续数字；
/// - UUID 形态（8-4-4-4-12 hex）；
/// - 含 `@` 或 `.` 的 token（邮箱 / 主机名）。
///
/// 输出全部小写，用于与行的小写 JSON 做子串匹配。
pub fn extract_query_anchors(text: &str) -> HashSet<String> {
    let mut anchors = HashSet::new();
    if text.is_empty() {
        return anchors;
    }

    // 1. 引号字符串。
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let q = bytes[i];
        if q == '\'' || q == '"' {
            if let Some(end) = (i + 1..bytes.len()).find(|&j| bytes[j] == q) {
                let inner: String = bytes[i + 1..end].iter().collect();
                if inner.trim().chars().count() >= 2 {
                    anchors.insert(inner.to_lowercase());
                }
                i = end;
            }
        }
        i += 1;
    }

    // 2. 按"锚点字符"分段扫描。
    let is_anchor_char =
        |c: char| c.is_ascii_alphanumeric() || c == '@' || c == '.' || c == '-' || c == '_';
    let mut token = String::new();
    let flush = |tok: &mut String, out: &mut HashSet<String>| {
        if tok.is_empty() {
            return;
        }
        let digits = tok.chars().filter(|c| c.is_ascii_digit()).count();
        let has_at = tok.contains('@');
        let has_dot = tok.contains('.');
        // 4+ 位纯数字（ID），或含 @ / . 的 token（邮箱 / 主机名）。
        if (digits >= 4 && tok.len() == digits) || has_at || (has_dot && tok.chars().count() >= 4) {
            out.insert(tok.to_lowercase());
        }
        tok.clear();
    };
    for c in text.chars() {
        if is_anchor_char(c) {
            token.push(c);
        } else {
            flush(&mut token, &mut anchors);
        }
    }
    flush(&mut token, &mut anchors);

    // UUID 形态整串也加入（token 分段会把它拆开，这里补一次）。
    let lower = text.to_lowercase();
    for tok in lower.split_whitespace() {
        let t = tok.trim_matches(|c: char| !c.is_ascii_hexdigit() && c != '-');
        if t.len() == 36 && t.matches('-').count() == 4 {
            anchors.insert(t.to_string());
        }
    }
    anchors
}

/// 行是否命中任一锚点：小写 JSON 序列化做子串匹配
/// （对应参考 `item_matches_anchors`）。
pub fn item_matches_anchors(item: &Value, anchors: &HashSet<String>) -> bool {
    if anchors.is_empty() {
        return false;
    }
    let hay = item.to_string().to_lowercase();
    anchors.iter().any(|a| hay.contains(a))
}

/// 计算查询锚点命中的行下标集合（供 plan 与超预算裁剪共用）。
fn query_hits(items: &[Value], query: Option<&str>) -> BTreeSet<usize> {
    let Some(q) = query.filter(|q| !q.is_empty()) else {
        return BTreeSet::new();
    };
    let anchors = extract_query_anchors(q);
    if anchors.is_empty() {
        return BTreeSet::new();
    }
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| item_matches_anchors(item, &anchors))
        .map(|(i, _)| i)
        .collect()
}

// ────────────────────────── 锚点（位置采样） ──────────────────────────

/// 位置锚点选择（简化自参考 `AnchorSelector::select_anchors`）：
/// front/back/middle 三段配额（0.3 / 0.15 / 剩余），段内等距采样，
/// 内容哈希去重。n <= max 时全保留。
fn select_anchors(items: &[Value], max_items: usize) -> BTreeSet<usize> {
    let n = items.len();
    if n == 0 {
        return BTreeSet::new();
    }
    if n <= max_items {
        return (0..n).collect();
    }
    let budget = max_items.min(n);
    let front = ((budget as f64 * 0.45) as usize).max(1);
    let back = ((budget as f64 * 0.25) as usize).max(1);
    let middle = budget.saturating_sub(front + back);

    let mut anchors = BTreeSet::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut take = |start: usize, end: usize, slots: usize, seen: &mut HashSet<String>| {
        if slots == 0 || end <= start {
            return;
        }
        let span = end - start;
        let step = ((span - 1) / slots).max(1);
        let mut i = start;
        let mut picked = 0;
        while i < end && picked < slots {
            let h = content_hash(&items[i]);
            if seen.insert(h) {
                anchors.insert(i);
                picked += 1;
            }
            i += step;
        }
    };
    take(0, (front * 2).min(n / 3), front, &mut seen);
    take(
        n.saturating_sub(back * 2).max(2 * n / 3),
        n,
        back,
        &mut seen,
    );
    take(0, n, middle, &mut seen);
    anchors
}

// ────────────────────────── 编排：去重 / 补位 / 优先级 ──────────────────────────

/// 内容重复的下标折叠到最小代表（参考 `deduplicate_indices_by_content`）。
fn deduplicate_indices(keep: &BTreeSet<usize>, items: &[Value]) -> BTreeSet<usize> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for &idx in keep {
        if idx >= items.len() {
            continue;
        }
        seen.entry(content_hash(&items[idx])).or_insert(idx);
    }
    seen.values().copied().collect()
}

/// 用等距步进补足预算，跳过内容重复（参考 `fill_remaining_slots`）。
fn fill_remaining_slots(
    keep: &BTreeSet<usize>,
    items: &[Value],
    n: usize,
    effective_max: usize,
) -> BTreeSet<usize> {
    let remaining = effective_max.saturating_sub(keep.len());
    if remaining == 0 {
        return keep.clone();
    }
    let mut seen: HashSet<String> = keep
        .iter()
        .filter(|&&i| i < n)
        .map(|&i| content_hash(&items[i]))
        .collect();
    let candidates: Vec<usize> = (0..n).filter(|i| !keep.contains(i)).collect();
    if candidates.is_empty() {
        return keep.clone();
    }
    let mut result = keep.clone();
    let step = (candidates.len() / (remaining + 1)).max(1);
    let mut added = 0;
    'outer: for offset in 0..step {
        let mut i = offset;
        while i < candidates.len() {
            if added >= remaining {
                break 'outer;
            }
            let idx = candidates[i];
            if seen.insert(content_hash(&items[idx])) {
                result.insert(idx);
                added += 1;
            }
            i += step;
        }
    }
    result
}

/// 优先级收敛（参考 `prioritize_indices`）：
/// 1. 内容去重；2. 补位到预算；3. 超预算时保全部关键行（错误行 + 结构异类
///    + 数值异常），再加 first-3 / last-2，最后按升序补满。
///
/// 关键行超预算时允许超出（与参考一致的质量保证）。
fn prioritize_indices(
    config: &SmartCrusherConfig,
    keep: &BTreeSet<usize>,
    items: &[Value],
    analysis: &ArrayAnalysis,
    effective_max: usize,
    pinned: &BTreeSet<usize>,
) -> BTreeSet<usize> {
    let n = items.len();
    let mut current = if config.dedup_identical_items {
        deduplicate_indices(keep, items)
    } else {
        keep.clone()
    };
    if current.len() < effective_max && current.len() < n {
        current = fill_remaining_slots(&current, items, n, effective_max);
    }
    if current.len() <= effective_max {
        return current;
    }

    let mut prioritized: BTreeSet<usize> = BTreeSet::new();
    prioritized.extend(detect_error_items(items));
    prioritized.extend(detect_structural_outliers(items));
    // 查询命中行视为关键（参考实现里由 relevance/anchor 信号承担）。
    prioritized.extend(pinned);
    // 数值异常（>variance_threshold·σ）。
    for s in analysis.field_stats.values() {
        if s.field_type != "numeric" {
            continue;
        }
        let (Some(m), Some(var)) = (s.mean_val, s.variance) else {
            continue;
        };
        let sigma = var.sqrt();
        if sigma <= 0.0 {
            continue;
        }
        let threshold = config.variance_threshold * sigma;
        for (i, item) in items.iter().enumerate() {
            if let Some(num) = item
                .as_object()
                .and_then(|o| o.get(&s.name))
                .and_then(|v| v.as_f64())
            {
                if num.is_finite() && (num - m).abs() > threshold {
                    prioritized.insert(i);
                }
            }
        }
    }

    let mut remaining = effective_max.saturating_sub(prioritized.len());
    if remaining > 0 {
        for i in 0..3.min(n) {
            if remaining == 0 {
                break;
            }
            if prioritized.insert(i) {
                remaining -= 1;
            }
        }
        for i in n.saturating_sub(2)..n {
            if remaining == 0 {
                break;
            }
            if prioritized.insert(i) {
                remaining -= 1;
            }
        }
    }
    if remaining > 0 {
        for i in current
            .difference(&prioritized)
            .copied()
            .collect::<Vec<_>>()
        {
            if remaining == 0 {
                break;
            }
            if prioritized.insert(i) {
                remaining -= 1;
            }
        }
    }
    prioritized
}

// ────────────────────────── 计划生成 ──────────────────────────

/// 自适应 K（简化自参考 Kneedle `compute_optimal_k`）：
/// n/4 夹在 [3, max_items]，n 小于 9 时直接返回 n（不压）。
fn compute_optimal_k(n: usize, config: &SmartCrusherConfig) -> usize {
    if n <= 8 {
        return n;
    }
    (n / 4).clamp(3, config.max_items_after_crush.max(1))
}

/// 生成压缩计划（参考 `SmartCrusherPlanner::create_plan` 调度器）。
fn create_plan(
    config: &SmartCrusherConfig,
    analysis: &ArrayAnalysis,
    items: &[Value],
    query: Option<&str>,
) -> CompressionPlan {
    if analysis.recommended_strategy == CompressionStrategy::Skip
        || analysis.recommended_strategy == CompressionStrategy::None
    {
        // 不可压：保留全部（上游不会走到这里，防御式处理）。
        return CompressionPlan {
            strategy: analysis.recommended_strategy,
            keep_indices: (0..items.len()).collect(),
            cluster_field: None,
            sort_field: None,
        };
    }
    match analysis.recommended_strategy {
        CompressionStrategy::TopN => plan_top_n(config, analysis, items, query),
        CompressionStrategy::ClusterSample => plan_cluster_sample(config, analysis, items, query),
        CompressionStrategy::TimeSeries => plan_time_series(config, analysis, items, query),
        _ => plan_smart_sample(config, analysis, items, query),
    }
}

/// SMART_SAMPLE（默认策略）：锚点 + 关键行 + 数值异常 + 变化点邻域 + 查询命中。
fn plan_smart_sample(
    config: &SmartCrusherConfig,
    analysis: &ArrayAnalysis,
    items: &[Value],
    query: Option<&str>,
) -> CompressionPlan {
    let n = items.len();
    let k = compute_optimal_k(n, config);
    let mut keep: BTreeSet<usize> = select_anchors(items, k);

    // 关键行：错误关键字 + 结构异类（罕见字段 / 罕见 status 值）。
    keep.extend(detect_error_items(items));
    keep.extend(detect_structural_outliers(items));

    // 数值异常。
    for s in analysis.field_stats.values() {
        if let (Some(m), Some(var), "numeric") = (s.mean_val, s.variance, s.field_type.as_str()) {
            let sigma = var.sqrt();
            if sigma > 0.0 {
                let threshold = config.variance_threshold * sigma;
                for (i, item) in items.iter().enumerate() {
                    if let Some(num) = item
                        .as_object()
                        .and_then(|o| o.get(&s.name))
                        .and_then(|v| v.as_f64())
                    {
                        if num.is_finite() && (num - m).abs() > threshold {
                            keep.insert(i);
                        }
                    }
                }
            }
        }
    }

    // 变化点邻域（±1）。
    if config.preserve_change_points {
        for s in analysis.field_stats.values() {
            for &cp in &s.change_points {
                for off in -1isize..=1 {
                    let idx = cp as isize + off;
                    if idx >= 0 && (idx as usize) < n {
                        keep.insert(idx as usize);
                    }
                }
            }
        }
    }

    // 查询锚点命中（同时作为超预算裁剪时的 pinned 关键行）。
    let query_hits = query_hits(items, query);
    keep.extend(&query_hits);

    let final_keep = prioritize_indices(config, &keep, items, analysis, k, &query_hits);
    CompressionPlan {
        strategy: CompressionStrategy::SmartSample,
        keep_indices: final_keep.into_iter().collect(),
        cluster_field: None,
        sort_field: None,
    }
}

/// TOP_N：按打分字段取最高 K-3 行，叠加关键行与查询命中。
fn plan_top_n(
    config: &SmartCrusherConfig,
    analysis: &ArrayAnalysis,
    items: &[Value],
    query: Option<&str>,
) -> CompressionPlan {
    let Some(score_field) = detect_score_field(&analysis.field_stats) else {
        return plan_smart_sample(config, analysis, items, query);
    };
    let n = items.len();
    let k = compute_optimal_k(n, config);
    let mut keep: BTreeSet<usize> = BTreeSet::new();

    let mut scored: Vec<(usize, f64)> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let score = item
                .as_object()
                .and_then(|o| o.get(&score_field))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            (i, score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (idx, _) in scored.iter().take(k.saturating_sub(3)) {
        keep.insert(*idx);
    }

    keep.extend(detect_error_items(items));
    keep.extend(detect_structural_outliers(items));

    if let Some(q) = query.filter(|q| !q.is_empty()) {
        let anchors = extract_query_anchors(q);
        for (i, item) in items.iter().enumerate() {
            if !keep.contains(&i) && item_matches_anchors(item, &anchors) {
                keep.insert(i);
            }
        }
    }

    // top_n 不做超预算裁剪的强保证，但仍经过去重收敛。
    let final_keep = if config.dedup_identical_items {
        deduplicate_indices(&keep, items)
    } else {
        keep
    };
    CompressionPlan {
        strategy: CompressionStrategy::TopN,
        keep_indices: final_keep.into_iter().collect(),
        cluster_field: None,
        sort_field: Some(score_field),
    }
}

/// CLUSTER_SAMPLE：按消息字段前 50 字符聚类，每簇保留 2 个代表。
fn plan_cluster_sample(
    config: &SmartCrusherConfig,
    analysis: &ArrayAnalysis,
    items: &[Value],
    query: Option<&str>,
) -> CompressionPlan {
    let n = items.len();
    let k = compute_optimal_k(n, config);
    let mut keep: BTreeSet<usize> = select_anchors(items, k);
    keep.extend(detect_error_items(items));
    keep.extend(detect_structural_outliers(items));

    let message_field = detect_message_field(&analysis.field_stats);
    if let Some(field) = &message_field {
        let mut clusters: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, item) in items.iter().enumerate() {
            let msg = item
                .as_object()
                .and_then(|o| o.get(field.as_str()))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // 聚类 key 用内容哈希代替参考的 md5(前 50 字符)[:8]，语义等价。
            let key_prefix: String = msg.chars().take(50).collect();
            let key = blake3::hash(key_prefix.as_bytes()).to_hex()[..8].to_string();
            clusters.entry(key).or_default().push(i);
        }
        for idxs in clusters.values() {
            for &idx in idxs.iter().take(2) {
                keep.insert(idx);
            }
        }
    }

    let query_hits = query_hits(items, query);
    keep.extend(&query_hits);

    let final_keep = prioritize_indices(config, &keep, items, analysis, k, &query_hits);
    CompressionPlan {
        strategy: CompressionStrategy::ClusterSample,
        keep_indices: final_keep.into_iter().collect(),
        cluster_field: message_field,
        sort_field: None,
    }
}

/// TIME_SERIES：锚点 + 变化点邻域（±2）+ 关键行 + 查询命中。
fn plan_time_series(
    config: &SmartCrusherConfig,
    analysis: &ArrayAnalysis,
    items: &[Value],
    query: Option<&str>,
) -> CompressionPlan {
    let n = items.len();
    let k = compute_optimal_k(n, config);
    let mut keep: BTreeSet<usize> = select_anchors(items, k);
    keep.extend(detect_error_items(items));
    keep.extend(detect_structural_outliers(items));

    for s in analysis.field_stats.values() {
        for &cp in &s.change_points {
            for off in -2isize..=2 {
                let idx = cp as isize + off;
                if idx >= 0 && (idx as usize) < n {
                    keep.insert(idx as usize);
                }
            }
        }
    }

    let query_hits = query_hits(items, query);
    keep.extend(&query_hits);

    let final_keep = prioritize_indices(config, &keep, items, analysis, k, &query_hits);
    CompressionPlan {
        strategy: CompressionStrategy::TimeSeries,
        keep_indices: final_keep.into_iter().collect(),
        cluster_field: None,
        sort_field: None,
    }
}

// ────────────────────────── 字符串 / 数字数组压缩器 ──────────────────────────

/// 字符串数组压缩（参考 `crush_string_array`）：
/// 错误关键字行 + 长度异常行 + first/last 边界 + 等距补位去重。
fn crush_string_array(items: &[&str], config: &SmartCrusherConfig) -> (Vec<String>, String) {
    let n = items.len();
    if n <= 8 {
        return (
            items.iter().map(|s| s.to_string()).collect(),
            "string:passthrough".into(),
        );
    }
    let (k_total, k_first, k_last) = k_split(n, config);

    let mut keep: BTreeSet<usize> = BTreeSet::new();
    for (i, s) in items.iter().enumerate() {
        let lower = s.to_lowercase();
        if ERROR_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
            keep.insert(i);
        }
    }
    // 长度异常。
    let lengths: Vec<f64> = items.iter().map(|s| s.chars().count() as f64).collect();
    if let (Some(m), Some(sd)) = (mean(&lengths), sample_stdev(&lengths)) {
        if sd > 0.0 {
            let threshold = config.variance_threshold * sd;
            for (i, &l) in lengths.iter().enumerate() {
                if (l - m).abs() > threshold {
                    keep.insert(i);
                }
            }
        }
    }
    // 边界 + 等距补位。
    for i in 0..k_first.min(n) {
        keep.insert(i);
    }
    for i in n.saturating_sub(k_last)..n {
        keep.insert(i);
    }
    let mut seen: HashSet<&str> = keep.iter().map(|&i| items[i]).collect();
    let remaining = k_total.saturating_sub(keep.len());
    if remaining > 0 {
        let stride = ((n - 1) / (remaining + 1)).max(1);
        let mut i = 0;
        while i < n && keep.len() < k_total + remaining {
            if !keep.contains(&i) && seen.insert(items[i]) {
                keep.insert(i);
            }
            i += stride;
        }
    }
    let result: Vec<String> = keep.iter().map(|&i| items[i].to_string()).collect();
    let strategy = format!("string:adaptive({}->{})", n, result.len());
    (result, strategy)
}

/// 数字数组压缩（参考 `crush_number_array` 简化）：
/// 离群点 + 变化点 + first/last + 等距补位。
fn crush_number_array(items: &[Value], config: &SmartCrusherConfig) -> (Vec<Value>, String) {
    let n = items.len();
    if n <= 8 {
        return (items.to_vec(), "number:passthrough".into());
    }
    let finite: Vec<f64> = items
        .iter()
        .filter_map(|v| v.as_f64().filter(|f| f.is_finite()))
        .collect();
    if finite.is_empty() {
        return (items.to_vec(), "number:no_finite".into());
    }
    let (k_total, k_first, k_last) = k_split(n, config);
    let m = mean(&finite).unwrap_or(0.0);
    let sd = sample_stdev(&finite).unwrap_or(0.0);

    let mut keep: BTreeSet<usize> = BTreeSet::new();
    if sd > 0.0 {
        let threshold = config.variance_threshold * sd;
        for (i, v) in items.iter().enumerate() {
            if let Some(num) = v.as_f64().filter(|f| f.is_finite()) {
                if (num - m).abs() > threshold {
                    keep.insert(i);
                }
            }
        }
    }
    // 变化点（窗口均值跳变）。
    if config.preserve_change_points && n > 10 {
        let w = 5;
        for i in w..n.saturating_sub(w) {
            let left: Vec<f64> = items[i - w..i]
                .iter()
                .filter_map(|v| v.as_f64().filter(|f| f.is_finite()))
                .collect();
            let right: Vec<f64> = items[i..i + w]
                .iter()
                .filter_map(|v| v.as_f64().filter(|f| f.is_finite()))
                .collect();
            if let (Some(lm), Some(rm)) = (mean(&left), mean(&right)) {
                if sd > 0.0 && (lm - rm).abs() > 2.0 * sd {
                    keep.insert(i);
                }
            }
        }
    }
    for i in 0..k_first.min(n) {
        keep.insert(i);
    }
    for i in n.saturating_sub(k_last)..n {
        keep.insert(i);
    }
    let remaining = k_total.saturating_sub(keep.len());
    if remaining > 0 {
        let stride = ((n - 1) / (remaining + 1)).max(1);
        let mut i = 0;
        while i < n && keep.len() < k_total + remaining {
            keep.insert(i);
            i += stride;
        }
    }
    let result: Vec<Value> = keep.iter().map(|&i| items[i].clone()).collect();
    let strategy = format!(
        "number:adaptive({}->{},mean={:.4},p25={:.4},p75={:.4})",
        n,
        result.len(),
        m,
        percentile_linear(
            &{
                let mut s = finite.clone();
                s.sort_by(f64::total_cmp);
                s
            },
            0.25
        ),
        percentile_linear(
            &{
                let mut s = finite.clone();
                s.sort_by(f64::total_cmp);
                s
            },
            0.75
        ),
    );
    (result, strategy)
}

/// K 预算切分（参考 `compute_k_split`，含 bug#4 修复的 clamp）。
fn k_split(n: usize, config: &SmartCrusherConfig) -> (usize, usize, usize) {
    let k_total = compute_optimal_k(n, config);
    let k_first_raw = 1.max((k_total as f64 * config.first_fraction).round() as usize);
    let k_last_raw = 1.max((k_total as f64 * config.last_fraction).round() as usize);
    let k_first = k_first_raw.min(k_total);
    let k_last = k_last_raw.min(k_total.saturating_sub(k_first));
    (k_total, k_first, k_last)
}

/// 混合数组：按类型分组，大组走对应压缩器，小组全保留，按原序重组。
/// 参考 `crush_mixed_array`。
fn crush_mixed_array(
    items: &[Value],
    query: Option<&str>,
    config: &SmartCrusherConfig,
) -> (Vec<Value>, String) {
    let n = items.len();
    if n <= 8 {
        return (items.to_vec(), "mixed:passthrough".into());
    }
    // 按类型分桶（保持首现顺序）。
    let mut order: Vec<&'static str> = Vec::new();
    let mut buckets: HashMap<&'static str, Vec<usize>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let key = match item {
            Value::Object(_) => "dict",
            Value::String(_) => "str",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::Array(_) => "list",
            Value::Null => "none",
        };
        if !buckets.contains_key(key) {
            order.push(key);
        }
        buckets.entry(key).or_default().push(i);
    }

    let mut keep: BTreeSet<usize> = BTreeSet::new();
    let mut parts: Vec<String> = Vec::new();
    for key in order {
        let idxs = &buckets[key];
        if idxs.len() < config.min_items_to_analyze {
            keep.extend(idxs.iter().copied()); // 小组全保留
            continue;
        }
        match key {
            "dict" => {
                let values: Vec<Value> = idxs.iter().map(|&i| items[i].clone()).collect();
                let (kept, _, _) = crush_dict_array(&values, query, config);
                // 用内容哈希匹配存活行到原始下标。
                let kept_hashes: HashSet<String> = kept.iter().map(content_hash).collect();
                for &i in idxs {
                    if kept_hashes.contains(&content_hash(&items[i])) {
                        keep.insert(i);
                    }
                }
                parts.push(format!("dict:{}->{}", idxs.len(), kept.len()));
            }
            "str" => {
                let strs: Vec<&str> = idxs.iter().filter_map(|&i| items[i].as_str()).collect();
                let (kept, _) = crush_string_array(&strs, config);
                let kept_set: HashSet<&str> = kept.iter().map(|s| s.as_str()).collect();
                for &i in idxs {
                    if let Some(s) = items[i].as_str() {
                        if kept_set.contains(s) {
                            keep.insert(i);
                        }
                    }
                }
                parts.push(format!("str:{}->{}", idxs.len(), kept.len()));
            }
            "number" => {
                let values: Vec<Value> = idxs.iter().map(|&i| items[i].clone()).collect();
                let (kept, _) = crush_number_array(&values, config);
                let kept_hashes: HashSet<String> = kept.iter().map(content_hash).collect();
                for &i in idxs {
                    if kept_hashes.contains(&content_hash(&items[i])) {
                        keep.insert(i);
                    }
                }
                parts.push(format!("num:{}->{}", idxs.len(), kept.len()));
            }
            _ => keep.extend(idxs.iter().copied()),
        }
    }
    let result: Vec<Value> = keep.iter().map(|&i| items[i].clone()).collect();
    let strategy = format!(
        "mixed:adaptive({}->{},{})",
        n,
        result.len(),
        parts.join(",")
    );
    (result, strategy)
}

// ────────────────────────── 主压缩流程 ──────────────────────────

/// 对象数组压缩主管道（参考 `crush_array` 有损路径）。
/// 返回 (保留行, 策略串, dropped 采样)。调用方负责拼哨兵。
fn crush_dict_array(
    items: &[Value],
    query: Option<&str>,
    config: &SmartCrusherConfig,
) -> (Vec<Value>, String, Vec<Value>) {
    let n = items.len();
    let k = compute_optimal_k(n, config);
    if n <= k {
        return (items.to_vec(), "none:adaptive_at_limit".into(), Vec::new());
    }

    let analysis = analyze_array(items, config);
    if analysis.recommended_strategy == CompressionStrategy::Skip {
        let reason = analysis
            .skip_reason
            .clone()
            .unwrap_or_else(|| "unknown".into());
        return (items.to_vec(), format!("skip:{reason}"), Vec::new());
    }

    let plan = create_plan(config, &analysis, items, query);
    let mut keep_indices: Vec<usize> = plan
        .keep_indices
        .iter()
        .copied()
        .filter(|&i| i < n)
        .collect();
    keep_indices.sort_unstable();
    let kept: Vec<Value> = keep_indices.iter().map(|&i| items[i].clone()).collect();
    let dropped: Vec<Value> = (0..n)
        .filter(|i| !keep_indices.contains(i))
        .map(|i| items[i].clone())
        .collect();

    let mut strategy = analysis.recommended_strategy.as_str().to_string();
    if let Some(f) = &plan.sort_field {
        strategy.push_str(&format!("({f})"));
    } else if let Some(f) = &plan.cluster_field {
        strategy.push_str(&format!("({f})"));
    }
    (kept, strategy, dropped)
}

/// 递归处理任意 JSON 值：在每个深度上压缩可压数组。
/// 参考 `process_value`（深度上限 50）。
fn process_value(
    value: &Value,
    depth: usize,
    query: Option<&str>,
    config: &SmartCrusherConfig,
) -> (Value, String) {
    if depth >= 50 {
        return (value.clone(), String::new());
    }
    match value {
        Value::Array(arr) => {
            let n = arr.len();
            let mut infos: Vec<String> = Vec::new();
            if n >= config.min_items_to_analyze {
                match classify_array(arr) {
                    ArrayType::DictArray => {
                        let (mut kept, strat, dropped) = crush_dict_array(arr, query, config);
                        infos.push(format!("{strat}({}->{})", n, kept.len()));
                        if !dropped.is_empty() {
                            // dropped 采样哨兵：保留代表性被丢弃行 + 计数标注。
                            kept.push(dropped_sentinel(&strat, n, &dropped, config));
                        }
                        return (Value::Array(kept), infos.join(","));
                    }
                    ArrayType::StringArray => {
                        let strs: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                        let (kept, strat) = crush_string_array(&strs, config);
                        infos.push(format!("{strat}({}->{})", n, kept.len()));
                        return (
                            Value::Array(kept.into_iter().map(Value::String).collect()),
                            infos.join(","),
                        );
                    }
                    ArrayType::NumberArray => {
                        let (kept, strat) = crush_number_array(arr, config);
                        infos.push(format!("{strat}({}->{})", n, kept.len()));
                        return (Value::Array(kept), infos.join(","));
                    }
                    ArrayType::MixedArray => {
                        let (kept, strat) = crush_mixed_array(arr, query, config);
                        infos.push(format!("{strat}({}->{})", n, kept.len()));
                        return (Value::Array(kept), infos.join(","));
                    }
                    // Nested/Bool/Empty → 递归下钻。
                    _ => {}
                }
            }
            let mut out = Vec::with_capacity(n);
            for item in arr {
                let (p, info) = process_value(item, depth + 1, query, config);
                out.push(p);
                if !info.is_empty() {
                    infos.push(info);
                }
            }
            (Value::Array(out), infos.join(","))
        }
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            let mut infos: Vec<String> = Vec::new();
            for (k, v) in map {
                let (p, info) = process_value(v, depth + 1, query, config);
                out.insert(k.clone(), p);
                if !info.is_empty() {
                    infos.push(info);
                }
            }
            (Value::Object(out), infos.join(","))
        }
        other => (other.clone(), String::new()),
    }
}

/// 构造 dropped 标注哨兵：策略 / 原始行数 / 丢弃行数 / 丢弃行采样 /
/// 被丢弃行的低基数字段取值分布（status 类字段去重后的统计信息）。
fn dropped_sentinel(
    strategy: &str,
    original_count: usize,
    dropped: &[Value],
    config: &SmartCrusherConfig,
) -> Value {
    let mut sentinel = serde_json::Map::new();
    sentinel.insert("_crushed".to_string(), Value::String(strategy.to_string()));
    sentinel.insert("_original_count".to_string(), Value::from(original_count));
    sentinel.insert("_dropped_count".to_string(), Value::from(dropped.len()));
    let sample: Vec<Value> = dropped
        .iter()
        .take(config.max_dropped_sample)
        .cloned()
        .collect();
    sentinel.insert("_dropped_sample".to_string(), Value::Array(sample));

    // 对低基数字段输出丢弃行的取值直方图（如 status: {"ok": 40}）。
    let mut field_counts: BTreeMap<&str, BTreeMap<String, usize>> = BTreeMap::new();
    let mut field_presence: HashMap<&str, usize> = HashMap::new();
    for item in dropped.iter().filter_map(|v| v.as_object()) {
        for (k, v) in item {
            *field_presence.entry(k.as_str()).or_insert(0) += 1;
            if !v.is_null() {
                *field_counts
                    .entry(k.as_str())
                    .or_default()
                    .entry(stringify_value(v))
                    .or_insert(0) += 1;
            }
        }
    }
    let histograms: serde_json::Map<String, Value> = field_counts
        .into_iter()
        .filter(|(k, hist)| {
            // 只保留"status 类"字段：出现在全部丢弃行且取值数 ≤ 10。
            let present = field_presence.get(*k).copied().unwrap_or(0);
            present == dropped.len() && !hist.is_empty() && hist.len() <= 10
        })
        .map(|(k, hist)| {
            let total: usize = hist.values().sum();
            (
                k.to_string(),
                Value::Object(
                    hist.into_iter()
                        .map(|(v, c)| (v, Value::from(format!("{c}/{total}"))))
                        .collect(),
                ),
            )
        })
        .collect();
    if !histograms.is_empty() {
        sentinel.insert(
            "_dropped_field_summary".to_string(),
            Value::Object(histograms),
        );
    }
    Value::Object(sentinel)
}

// ────────────────────────── SmartCrusher 主体 ──────────────────────────

/// SmartCrusher：JSON 数组统计压缩器。
#[derive(Default)]
pub struct SmartCrusher {
    config: SmartCrusherConfig,
}

impl SmartCrusher {
    pub fn new(config: SmartCrusherConfig) -> Self {
        SmartCrusher { config }
    }

    /// 压缩入口：解析 → 递归处理 → 序列化（紧凑 JSON，人类可读由
    /// pretty 输出保证在 `apply` 层）。
    pub fn crush(
        &self,
        content: &str,
        query: Option<&str>,
    ) -> Result<(String, bool), TransformError> {
        let parsed: Value =
            serde_json::from_str(content.trim()).map_err(|_| TransformError::InvalidInput)?;
        let (processed, _info) = process_value(&parsed, 0, query, &self.config);
        let compact = serde_json::to_string(&processed)
            .map_err(|e| TransformError::Internal(e.to_string()))?;
        let was_modified = compact != content.trim();
        Ok((compact, was_modified))
    }
}

/// 实现 [`OffloadTransform`]：SmartCrusher 是有损压缩（行丢弃 + 采样标注），
/// 被丢弃的原文需要由调用方存入 offload store，因此返回结构化卸载结果。
/// 而非 ReformatTransform（后者要求输出可完全重建原文）。
impl OffloadTransform for SmartCrusher {
    fn name(&self) -> &'static str {
        "smart_crusher"
    }

    fn applies_to(&self) -> ContentType {
        ContentType::JsonArray
    }

    /// 膨胀度估算：元素数相对压缩后保留上限的倍数（0.0 = 无膨胀）。
    fn estimate_bloat(&self, input: &str) -> f64 {
        let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(input.trim()) else {
            return 0.0;
        };
        if arr.len() <= self.config.min_items_to_analyze {
            return 0.0;
        }
        arr.len() as f64 / self.config.max_items_after_crush.max(1) as f64
    }

    /// cache key：与 [`crate::stash::compute_key`] 统一（blake3 前 24 hex），
    /// 保证 marker 与 store 跨压缩器键格式一致。
    fn cache_key(&self, input: &str) -> String {
        crate::stash::compute_key(input)
    }

    fn apply(
        &self,
        input: &str,
        ctx: &CompressionContext,
    ) -> Result<OffloadOutput, TransformError> {
        let (compact, was_modified) = self.crush(input, ctx.query.as_deref())?;
        if !was_modified {
            // 无压缩空间（太小 / 不可压 / 输出等价）：交给上层走原样。
            return Err(TransformError::Skipped);
        }
        // 人类可读：pretty-print 输出，原文原样返回给 offload store。
        let parsed: Value =
            serde_json::from_str(&compact).map_err(|e| TransformError::Internal(e.to_string()))?;
        let pretty = serde_json::to_string_pretty(&parsed)
            .map_err(|e| TransformError::Internal(e.to_string()))?;
        Ok(OffloadOutput::new(pretty, input.to_string()))
    }
}

// ────────────────────────── 单元测试 ──────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn crusher() -> SmartCrusher {
        SmartCrusher::default()
    }

    fn ctx(query: Option<&str>) -> CompressionContext {
        CompressionContext {
            query: query.map(|q| q.to_string()),
            token_budget: None,
            source_path: None,
            stash_file_path: None,
            stash_line_offset: 0,
        }
    }

    // ---------- classify_array ----------

    #[test]
    fn classify_variants() {
        assert_eq!(classify_array(&[]), ArrayType::Empty);
        assert_eq!(classify_array(&[json!({"a": 1})]), ArrayType::DictArray);
        assert_eq!(
            classify_array(&[json!("a"), json!("b")]),
            ArrayType::StringArray
        );
        assert_eq!(
            classify_array(&[json!(1), json!(2.5)]),
            ArrayType::NumberArray
        );
        assert_eq!(
            classify_array(&[json!(true), json!(false)]),
            ArrayType::BoolArray
        );
        assert_eq!(
            classify_array(&[json!([1]), json!([2])]),
            ArrayType::NestedArray
        );
        assert_eq!(
            classify_array(&[json!({"a": 1}), json!("s")]),
            ArrayType::MixedArray
        );
        assert_eq!(
            classify_array(&[json!({"a": 1}), json!(null)]),
            ArrayType::MixedArray
        );
    }

    // ---------- 罕见 status / 错误行保留 ----------

    #[test]
    fn rare_status_dominant_value() {
        // 95 ok + 5 异常 → 5 行全被标为异类（可搬自参考 outliers 测试）。
        let mut items: Vec<Value> = (0..95).map(|_| json!({"status": "ok"})).collect();
        for s in ["error", "timeout", "error", "timeout", "fail"] {
            items.push(json!({"status": s}));
        }
        let common: HashSet<String> = ["status".to_string()].into_iter().collect();
        assert_eq!(detect_rare_status_values(&items, &common).len(), 5);
    }

    #[test]
    fn rare_status_bimodal_high_cardinality() {
        // 参考 bug#3 用例：60 INFO + 25 WARN + 15 个单例错误码 → 15 行异类。
        let mut items: Vec<Value> = Vec::new();
        for _ in 0..60 {
            items.push(json!({"code": "INFO"}));
        }
        for _ in 0..25 {
            items.push(json!({"code": "WARN"}));
        }
        for i in 0..15 {
            items.push(json!({"code": format!("ERR_{i}")}));
        }
        let common: HashSet<String> = ["code".to_string()].into_iter().collect();
        assert_eq!(detect_rare_status_values(&items, &common).len(), 15);
    }

    #[test]
    fn rare_status_uniform_distribution_no_outliers() {
        let items: Vec<Value> = (0..50)
            .map(|i| json!({"code": format!("CAT_{i}")}))
            .collect();
        let common: HashSet<String> = ["code".to_string()].into_iter().collect();
        assert!(detect_rare_status_values(&items, &common).is_empty());
    }

    #[test]
    fn error_keywords_detected_case_insensitive() {
        let items = vec![
            json!({"msg": "all good"}),
            json!({"msg": "FATAL: out of memory"}),
            json!({"status": "ok"}),
            json!({"status": "error"}),
        ];
        assert_eq!(detect_error_items(&items), vec![1, 3]);
    }

    #[test]
    fn structural_outlier_rare_field() {
        let mut items: Vec<Value> = (0..9).map(|i| json!({"a": i})).collect();
        items.push(json!({"a": 9, "stack": "rare"}));
        assert!(detect_structural_outliers(&items).contains(&9));
    }

    // ---------- 对象数组主流程 ----------

    #[test]
    fn object_array_crushes_and_keeps_errors() {
        // 100 行 ok + 2 行 error。
        let mut items: Vec<Value> = (0..100)
            .map(|i| json!({"id": i, "status": "ok", "msg": format!("row {i}")}))
            .collect();
        items.push(json!({"id": 100, "status": "error", "msg": "boom"}));
        items.push(json!({"id": 101, "status": "error", "msg": "fatal crash"}));
        let (kept, strat, dropped) = crush_dict_array(&items, None, &SmartCrusherConfig::default());
        assert!(kept.len() < items.len(), "应显著压缩: {strat}");
        assert!(kept.len() <= 20, "保留行数应受预算约束, got {}", kept.len());
        // 错误行永不丢弃。
        for item in &items {
            if item["status"] == json!("error") {
                assert!(kept.contains(item), "错误行必须保留: {item}");
            }
        }
        assert!(!dropped.is_empty());
    }

    #[test]
    fn apply_output_annotates_dropped() {
        let mut items: Vec<Value> = (0..60).map(|i| json!({"id": i, "status": "ok"})).collect();
        items.push(json!({"id": 60, "status": "error"}));
        let input = serde_json::to_string(&items).unwrap();
        let result = crusher().apply(&input, &ctx(None)).expect("应可压缩");
        assert_eq!(result.original, input, "原文应原样返回（offload store 用）");
        assert!(
            result.compressed.contains("_dropped_count"),
            "必须标注 dropped 计数"
        );
        assert!(
            result.compressed.contains("_dropped_sample"),
            "必须带 dropped 采样"
        );
        assert!(
            result.compressed.contains("\"status\": \"error\""),
            "错误行必须在输出中"
        );
        // status 直方图：丢弃行全是 ok。
        assert!(result.compressed.contains("_dropped_field_summary"));
        // 输出是合法 JSON 数组。
        let parsed: Value = serde_json::from_str(&result.compressed).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn identical_rows_collapse() {
        // 100 行完全相同 → 折叠为极少数代表。
        let items: Vec<Value> = (0..100).map(|_| json!({"status": "ok", "v": 1})).collect();
        let (kept, _, _) = crush_dict_array(&items, None, &SmartCrusherConfig::default());
        assert!(kept.len() <= 5, "重复行应折叠, got {}", kept.len());
    }

    #[test]
    fn unique_entities_skip() {
        // 全唯一 ID + 名字，无任何信号 → skip（对应参考测试可搬运用例）。
        let items: Vec<Value> = (0..30)
            .map(|i| json!({"id": i, "name": format!("user_{i}")}))
            .collect();
        let (kept, strat, dropped) = crush_dict_array(&items, None, &SmartCrusherConfig::default());
        assert_eq!(kept.len(), 30);
        assert!(dropped.is_empty());
        assert!(strat.starts_with("skip:"), "got {strat}");
    }

    // ---------- 查询锚点 ----------

    #[test]
    fn query_anchor_pins_matching_item() {
        // 可搬自参考 planning 测试：查询 UUID → 命中行必保留。
        let items: Vec<Value> = (0..30)
            .map(|i| {
                json!({
                    "id": i,
                    "uuid": format!("550e8400-e29b-41d4-a716-44665544{i:04x}"),
                    "status": "ok",
                })
            })
            .collect();
        let target = "550e8400-e29b-41d4-a716-446655440011";
        let (kept, _, _) = crush_dict_array(
            &items,
            Some(&format!("find record {target}")),
            &SmartCrusherConfig::default(),
        );
        assert!(
            kept.iter().any(|it| it["uuid"] == json!(target)),
            "查询命中的行必须保留"
        );
    }

    #[test]
    fn anchors_extraction_variants() {
        let a = extract_query_anchors(
            r#"see "user_name" and 550E8400-E29B-41D4-A716-446655440000, user 12345, bob@Example.COM, api.example.com"#,
        );
        assert!(a.contains("user_name")); // 引号字符串
        assert!(a.contains("550e8400-e29b-41d4-a716-446655440000")); // UUID
        assert!(a.contains("12345")); // 4+ 位数字 ID
        assert!(!a.iter().any(|x| x == "123")); // 3 位不算
        assert!(a.contains("bob@example.com")); // 邮箱
        assert!(a.contains("api.example.com")); // 主机名
    }

    #[test]
    fn item_anchor_match_substring() {
        let anchors: HashSet<String> = ["alice".to_string()].into_iter().collect();
        assert!(item_matches_anchors(&json!({"name": "Alice"}), &anchors));
        assert!(!item_matches_anchors(&json!({"name": "bob"}), &anchors));
    }

    // ---------- TopN / Cluster / TimeSeries ----------

    #[test]
    fn top_n_keeps_highest_scores() {
        // score 字段触发 TopN；最高分行必保留。
        let items: Vec<Value> = (0..30)
            .map(|i| json!({"id": i, "score": (29 - i) as f64 * 0.05, "tag": "t"}))
            .collect();
        let (kept, strat, _) = crush_dict_array(&items, None, &SmartCrusherConfig::default());
        assert!(strat.starts_with("top_n"), "got {strat}");
        assert!(kept.contains(&items[0]), "最高分行（id=0）必须保留");
    }

    #[test]
    fn cluster_sample_keeps_representatives() {
        // 日志形态：3 类消息模板 + level 枚举 → 每类至少留代表。
        let items: Vec<Value> = (0..30)
            .map(|i| {
                let tmpl = match i % 3 {
                    0 => "connection reset by peer",
                    1 => "request processed successfully",
                    _ => "cache miss for key",
                };
                json!({
                    "msg": format!("{tmpl} entry {i}"),
                    "level": if i % 2 == 0 { "INFO" } else { "ERROR" },
                })
            })
            .collect();
        let (kept, strat, _) = crush_dict_array(&items, None, &SmartCrusherConfig::default());
        assert!(strat.starts_with("cluster"), "got {strat}");
        // 三类模板都应有代表（按前 50 字符聚类，前缀不同 → 不同簇）。
        for tmpl in ["connection reset", "request processed", "cache miss"] {
            assert!(
                kept.iter().any(|it| {
                    it["msg"]
                        .as_str()
                        .map(|m| m.contains(tmpl))
                        .unwrap_or(false)
                }),
                "簇代表缺失: {tmpl}"
            );
        }
    }

    #[test]
    fn time_series_keeps_change_point_region() {
        // 前 30 行 value=1，后 30 行 value=100 → 变化点附近必保留。
        let items: Vec<Value> = (0..60)
            .map(|i| json!({"id": i, "value": if i < 30 { 1.0 } else { 100.0 }}))
            .collect();
        let (kept, strat, _) = crush_dict_array(&items, None, &SmartCrusherConfig::default());
        assert!(strat.starts_with("time_series"), "got {strat}");
        let ids: Vec<i64> = kept.iter().filter_map(|it| it["id"].as_i64()).collect();
        assert!(
            (27..=33).any(|i| ids.contains(&i)),
            "变化点邻域必须保留, ids={ids:?}"
        );
    }

    // ---------- 字符串 / 数字 / 混合数组 ----------

    #[test]
    fn string_array_keeps_errors_and_boundaries() {
        let mut strs: Vec<String> = (0..30).map(|i| format!("line {i} ok")).collect();
        strs.push("FATAL: out of memory".to_string());
        let refs: Vec<&str> = strs.iter().map(|s| s.as_str()).collect();
        let (kept, strat) = crush_string_array(&refs, &SmartCrusherConfig::default());
        assert!(strat.starts_with("string:adaptive"));
        assert!(kept.contains(&"FATAL: out of memory".to_string()));
        assert!(kept.first().map(|s| s == "line 0 ok").unwrap_or(false));
        assert!(kept
            .last()
            .map(|s| s == "FATAL: out of memory")
            .unwrap_or(false));
        assert!(kept.len() < strs.len());
    }

    #[test]
    fn string_array_small_passthrough() {
        let strs = ["a", "b", "c"];
        let (kept, strat) = crush_string_array(&strs, &SmartCrusherConfig::default());
        assert_eq!(kept, vec!["a", "b", "c"]);
        assert_eq!(strat, "string:passthrough");
    }

    #[test]
    fn number_array_keeps_outliers() {
        let mut nums: Vec<Value> = (0..30).map(|i| json!(i)).collect();
        nums.push(json!(10_000)); // 极端离群
        let (kept, strat) = crush_number_array(&nums, &SmartCrusherConfig::default());
        assert!(strat.starts_with("number:adaptive"), "got {strat}");
        assert!(kept.contains(&json!(10_000)), "离群值必须保留");
        assert!(kept.len() < nums.len());
    }

    #[test]
    fn mixed_array_small_groups_kept() {
        // 25 dict（大组压缩）+ 5 string（小组全保留）。
        let mut items: Vec<Value> = (0..25).map(|i| json!({"id": i, "status": "ok"})).collect();
        for i in 0..5 {
            items.push(json!(format!("string_{i}")));
        }
        let (kept, strat) = crush_mixed_array(&items, None, &SmartCrusherConfig::default());
        assert!(strat.starts_with("mixed:adaptive"), "got {strat}");
        assert_eq!(kept.iter().filter(|v| v.is_string()).count(), 5);
    }

    #[test]
    fn mixed_array_at_threshold_passthrough() {
        let items = vec![
            json!(1),
            json!("two"),
            json!({"k": "v"}),
            json!([1]),
            json!(null),
        ];
        let (kept, strat) = crush_mixed_array(&items, None, &SmartCrusherConfig::default());
        assert_eq!(kept.len(), 5);
        assert_eq!(strat, "mixed:passthrough");
    }

    // ---------- Trait 行为 / 错误处理 / 边界 ----------

    #[test]
    fn apply_non_json_invalid_input() {
        let err = crusher().apply("not json at all", &ctx(None)).unwrap_err();
        assert_eq!(err, TransformError::InvalidInput);
    }

    #[test]
    fn apply_empty_array_skipped() {
        let err = crusher().apply("[]", &ctx(None)).unwrap_err();
        assert_eq!(err, TransformError::Skipped);
    }

    #[test]
    fn apply_small_array_skipped() {
        let input = r#"[{"a":1},{"a":2}]"#;
        let err = crusher().apply(input, &ctx(None)).unwrap_err();
        assert_eq!(err, TransformError::Skipped);
    }

    #[test]
    fn apply_nested_array_inside_object() {
        let mut inner: Vec<Value> = (0..40).map(|i| json!({"id": i, "status": "ok"})).collect();
        inner.push(json!({"id": 40, "status": "error", "msg": "failed"}));
        let input = serde_json::to_string(&json!({"data": inner, "total": 41})).unwrap();
        let result = crusher()
            .apply(&input, &ctx(None))
            .expect("嵌套数组应被压缩");
        let parsed: Value = serde_json::from_str(&result.compressed).unwrap();
        assert!(result.compressed.contains("_dropped_count"));
        assert!(
            result.compressed.contains("\"error\""),
            "嵌套的错误行必须保留"
        );
        assert_eq!(parsed["total"], json!(41));
    }

    #[test]
    fn estimate_bloat_and_cache_key() {
        let big: Vec<Value> = (0..100).map(|i| json!({"id": i})).collect();
        let input = serde_json::to_string(&big).unwrap();
        let c = crusher();
        assert!(c.estimate_bloat(&input) > 1.0, "100 行 / 上限 15 应 >1");
        assert_eq!(c.estimate_bloat("[]"), 0.0);
        assert_eq!(c.estimate_bloat("not json"), 0.0);
        // cache key 确定且随输入变化。
        assert_eq!(c.cache_key(&input), c.cache_key(&input));
        assert_ne!(c.cache_key(&input), c.cache_key("[1]"));
        assert_eq!(c.name(), "smart_crusher");
        assert_eq!(c.applies_to(), ContentType::JsonArray);
    }

    // ---------- 统计工具 ----------

    #[test]
    fn stats_helpers() {
        assert_eq!(mean(&[1.0, 2.0, 3.0]), Some(2.0));
        assert_eq!(mean(&[]), None);
        let var = sample_variance(&[2.0, 4.0]).unwrap();
        assert!((var - 2.0).abs() < 1e-12);
        assert_eq!(sample_variance(&[1.0]), None);
        let sorted = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile_linear(&sorted, 0.25), 1.75);
        assert_eq!(percentile_linear(&sorted, 0.5), 2.5);
    }

    #[test]
    fn content_hash_key_order_independent() {
        // key 顺序不同 → 同哈希（重复行折叠依赖此性质）。
        let a = json!({"b": 2, "a": 1});
        let b = json!({"a": 1, "b": 2});
        assert_eq!(content_hash(&a), content_hash(&b));
        assert_ne!(content_hash(&a), content_hash(&json!({"a": 1, "b": 3})));
    }

    #[test]
    fn dedup_and_fill_orchestration() {
        let items = vec![
            json!({"name": "alice"}),
            json!({"name": "alice"}),
            json!({"name": "bob"}),
        ];
        let keep: BTreeSet<usize> = [0, 1, 2].into_iter().collect();
        let deduped = deduplicate_indices(&keep, &items);
        assert_eq!(deduped, [0, 2].into_iter().collect::<BTreeSet<_>>());

        // fill 补位：预算 5、只有 2 个 → 补满所有可用唯一项。
        let items20: Vec<Value> = (0..20).map(|i| json!({"id": i})).collect();
        let keep2: BTreeSet<usize> = [0, 5].into_iter().collect();
        let filled = fill_remaining_slots(&keep2, &items20, 20, 5);
        assert_eq!(filled.len(), 5);
        assert!(filled.contains(&0) && filled.contains(&5));
    }

    // ---------- 参考实现可搬运用例（crusher.rs 测试） ----------

    #[test]
    fn reference_passthrough_when_below_adaptive_k() {
        let items: Vec<Value> = (0..3).map(|i| json!({"id": i})).collect();
        let (kept, strat, dropped) = crush_dict_array(&items, None, &SmartCrusherConfig::default());
        assert_eq!(kept.len(), 3);
        assert_eq!(strat, "none:adaptive_at_limit");
        assert!(dropped.is_empty());
    }

    #[test]
    fn reference_low_uniqueness_compresses() {
        // 30 行 status=ok → 低唯一度，可安全采样。
        let items: Vec<Value> = (0..30).map(|_| json!({"status": "ok"})).collect();
        let (kept, strat, _) = crush_dict_array(&items, None, &SmartCrusherConfig::default());
        assert!(kept.len() < 30, "低唯一度必须压缩, strat={strat}");
    }

    #[test]
    fn reference_crush_error_item_survives() {
        let mut items: Vec<Value> = (0..30).map(|i| json!({"id": i, "status": "ok"})).collect();
        items.push(json!({"id": 30, "status": "error", "msg": "FATAL"}));
        let (kept, _, _) = crush_dict_array(&items, None, &SmartCrusherConfig::default());
        assert!(
            kept.iter()
                .any(|it| it.get("status").and_then(|v| v.as_str()) == Some("error")),
            "错误行必须在 crush 中存活"
        );
    }
}
