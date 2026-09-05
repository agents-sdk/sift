//! 规则 JSON 对象数组的 CSV-schema 紧凑化。
//!
//! 所有行都保留，只把每行重复的字段名提升为一次 schema 声明。仅当相对紧凑
//! JSON 至少节省配置比例时采用；否则交回 SmartCrusher 的有损抽样路径。

use std::collections::BTreeMap;

use serde_json::Value;

use super::DeferredStash;

const CORE_FIELD_FRACTION: f64 = 0.8;
const HETEROGENEOUS_CORE_RATIO: f64 = 0.6;
const MIN_BUCKETS: usize = 2;
const MAX_BUCKETS: usize = 8;

#[derive(Debug, Clone)]
struct Column {
    name: String,
    parent: String,
    child: Option<String>,
    type_tag: &'static str,
    nullable: bool,
}

pub(super) struct JsonCompaction {
    pub output: String,
    pub deferred_stashes: Vec<DeferredStash>,
}

pub(super) fn try_compact_csv_schema(
    items: &[Value],
    min_items: usize,
    min_savings_ratio: f64,
    require_lossless_shape: bool,
    offload_opaque: bool,
) -> Option<JsonCompaction> {
    if items.len() < min_items || !items.iter().all(Value::is_object) {
        return None;
    }
    if require_lossless_shape && !has_lossless_shape(items) {
        return None;
    }

    let mut deferred_stashes = Vec::new();
    let output = if require_lossless_shape {
        render_table(items, false, false, &mut deferred_stashes)?
    } else if let Some(bucketed) = render_buckets(items, offload_opaque, &mut deferred_stashes) {
        bucketed
    } else {
        render_table(items, true, offload_opaque, &mut deferred_stashes)?
    };

    let compact_json = serde_json::to_string(items).ok()?;
    let savings_ratio = 1.0 - output.len() as f64 / compact_json.len().max(1) as f64;
    (savings_ratio >= min_savings_ratio).then_some(JsonCompaction {
        output,
        deferred_stashes,
    })
}

fn render_table(
    items: &[Value],
    flatten_nested: bool,
    offload_opaque: bool,
    deferred_stashes: &mut Vec<DeferredStash>,
) -> Option<String> {
    let columns = build_columns(items, flatten_nested);
    if columns.is_empty() {
        return None;
    }

    let mut output = String::new();
    output.push('[');
    output.push_str(&items.len().to_string());
    output.push_str("]{");
    output.push_str(
        &columns
            .iter()
            .map(|column| {
                format!(
                    "{}:{}{}",
                    column.name,
                    column.type_tag,
                    if column.nullable { "?" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join(","),
    );
    output.push_str("}\n");

    for item in items {
        let object = item.as_object()?;
        let cells = columns
            .iter()
            .map(|column| {
                let value = object.get(&column.parent).and_then(|value| {
                    column
                        .child
                        .as_ref()
                        .map_or(Some(value), |child| value.as_object()?.get(child))
                });
                format_cell(value, offload_opaque, deferred_stashes)
            })
            .collect::<Vec<_>>();
        output.push_str(&cells.join(","));
        output.push('\n');
    }

    Some(output)
}

fn render_buckets(
    items: &[Value],
    offload_opaque: bool,
    deferred_stashes: &mut Vec<DeferredStash>,
) -> Option<String> {
    let frequencies = key_frequencies(items);
    let core_threshold = (items.len() as f64 * CORE_FIELD_FRACTION).ceil() as usize;
    let core_count = frequencies
        .values()
        .filter(|frequency| **frequency >= core_threshold)
        .count();
    let core_ratio = if frequencies.is_empty() {
        1.0
    } else {
        core_count as f64 / frequencies.len() as f64
    };
    if core_ratio >= HETEROGENEOUS_CORE_RATIO {
        return None;
    }

    let discriminator = detect_discriminator(items, &frequencies)?;
    let mut groups = BTreeMap::<String, Vec<Value>>::new();
    for item in items {
        let key = item.as_object()?.get(&discriminator)?.as_str()?.to_string();
        groups.entry(key).or_default().push(item.clone());
    }

    let mut output = format!("__buckets:{discriminator}\n");
    for (key, group) in groups {
        output.push_str("__key:");
        output.push_str(&csv_quote_if_needed(&key));
        output.push('\n');
        if group.len() < 2 {
            output.push_str(&render_raw_value_table(&group)?);
        } else {
            output.push_str(&render_table(
                &group,
                true,
                offload_opaque,
                deferred_stashes,
            )?);
        }
    }
    Some(output)
}

fn render_raw_value_table(items: &[Value]) -> Option<String> {
    let mut output = format!("[{}]{{value:json}}\n", items.len());
    for item in items {
        output.push_str(&csv_quote(&serde_json::to_string(item).ok()?));
        output.push('\n');
    }
    Some(output)
}

fn key_frequencies(items: &[Value]) -> BTreeMap<String, usize> {
    let mut frequencies = BTreeMap::new();
    for object in items.iter().filter_map(Value::as_object) {
        for key in object.keys() {
            *frequencies.entry(key.clone()).or_default() += 1;
        }
    }
    frequencies
}

fn detect_discriminator(items: &[Value], frequencies: &BTreeMap<String, usize>) -> Option<String> {
    let total = items.len();
    let mut best = None::<(String, usize)>;

    for (key, frequency) in frequencies {
        if *frequency != total {
            continue;
        }
        let Some(values) = items
            .iter()
            .map(|item| item.as_object()?.get(key)?.as_str())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let distinct = values
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        if !(MIN_BUCKETS..=MAX_BUCKETS).contains(&distinct) || distinct as f64 / total as f64 > 0.7
        {
            continue;
        }
        if match &best {
            None => true,
            Some((_, previous)) => distinct > *previous,
        } {
            best = Some((key.clone(), distinct));
        }
    }

    best.map(|(key, _)| key)
}

fn has_lossless_shape(items: &[Value]) -> bool {
    let Some(first) = items.first().and_then(Value::as_object) else {
        return false;
    };
    let keys = first.keys().collect::<std::collections::BTreeSet<_>>();
    if keys.is_empty()
        || keys.iter().any(|key| {
            key.is_empty()
                || !key
                    .chars()
                    .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        })
    {
        return false;
    }
    let same_shape = items.iter().all(|item| {
        let Some(object) = item.as_object() else {
            return false;
        };
        object.keys().collect::<std::collections::BTreeSet<_>>() == keys
            && object.values().all(|value| !value.is_null())
    });
    same_shape
        && keys.iter().all(|key| {
            let expected = type_tag(&first[*key]);
            items.iter().all(|item| {
                item.as_object()
                    .and_then(|object| object.get(*key))
                    .is_some_and(|value| type_tag(value) == expected)
            })
        })
}

fn build_columns(items: &[Value], flatten_nested: bool) -> Vec<Column> {
    let frequencies = key_frequencies(items);
    let mut keys = frequencies.keys().cloned().collect::<Vec<_>>();
    keys.sort_by(|left, right| {
        frequencies[right]
            .cmp(&frequencies[left])
            .then_with(|| left.cmp(right))
    });

    let mut columns = Vec::new();
    for key in keys {
        if let Some(children) = flatten_nested
            .then(|| uniform_nested_keys(items, &key))
            .flatten()
        {
            for child in children {
                let values = items.iter().map(|item| {
                    item.as_object()
                        .and_then(|object| object.get(&key))
                        .and_then(Value::as_object)
                        .and_then(|object| object.get(&child))
                });
                let (type_tag, nullable) = infer_type(values);
                columns.push(Column {
                    name: format!("{key}.{child}"),
                    parent: key.clone(),
                    child: Some(child),
                    type_tag,
                    nullable,
                });
            }
        } else {
            let values = items
                .iter()
                .map(|item| item.as_object().and_then(|object| object.get(&key)));
            let (type_tag, nullable) = infer_type(values);
            columns.push(Column {
                name: key.clone(),
                parent: key,
                child: None,
                type_tag,
                nullable,
            });
        }
    }
    columns
}

fn uniform_nested_keys(items: &[Value], key: &str) -> Option<Vec<String>> {
    let mut expected: Option<Vec<String>> = None;
    for item in items {
        let Some(value) = item.as_object()?.get(key) else {
            continue;
        };
        let object = value.as_object()?;
        let mut keys = object.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        if keys.is_empty() || keys.len() > 6 {
            return None;
        }
        match &expected {
            Some(previous) if previous != &keys => return None,
            None => expected = Some(keys),
            _ => {}
        }
    }
    expected
}

fn infer_type<'a>(values: impl Iterator<Item = Option<&'a Value>>) -> (&'static str, bool) {
    let mut inferred = None;
    let mut nullable = false;
    for value in values {
        let Some(value) = value else {
            nullable = true;
            continue;
        };
        if value.is_null() {
            nullable = true;
            continue;
        }
        let current = type_tag(value);
        inferred = match inferred {
            None => Some(current),
            Some(previous) if previous == current => Some(previous),
            Some(_) => Some("json"),
        };
    }
    (inferred.unwrap_or("string"), nullable)
}

fn type_tag(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(number) if number.is_i64() || number.is_u64() => "int",
        Value::Number(_) => "float",
        Value::String(_) => "string",
        Value::Array(_) | Value::Object(_) => "json",
    }
}

fn format_cell(
    value: Option<&Value>,
    offload_opaque: bool,
    deferred_stashes: &mut Vec<DeferredStash>,
) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::String(value)) => {
            if offload_opaque {
                if let Some(kind) = classify_opaque_string(value) {
                    let key = crate::stash::compute_key(value);
                    deferred_stashes.push(DeferredStash {
                        key: key.clone(),
                        content: value.clone(),
                    });
                    return format!(
                        "{}[{kind},{}]",
                        crate::stash::marker_for(&key),
                        humanize_bytes(value.len())
                    );
                }
            }
            csv_quote_if_needed(value)
        }
        Some(value) => csv_quote(&serde_json::to_string(value).unwrap_or_default()),
    }
}

fn classify_opaque_string(value: &str) -> Option<&'static str> {
    if value.len() <= 256 || value.contains("<<stash:") {
        return None;
    }
    let trimmed = value.trim_start();
    if matches!(trimmed.as_bytes().first(), Some(b'{') | Some(b'['))
        && serde_json::from_str::<Value>(value)
            .is_ok_and(|parsed| matches!(parsed, Value::Object(_) | Value::Array(_)))
    {
        return None;
    }
    if looks_like_base64(value) {
        Some("base64")
    } else if looks_like_html(value) {
        Some("html")
    } else {
        Some("string")
    }
}

fn looks_like_base64(value: &str) -> bool {
    if value.len() < 64 || value.contains(['<', '>']) || value.chars().any(char::is_whitespace) {
        return false;
    }
    let alphabet = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '_' | '-'))
        .count();
    if alphabet as f64 / (value.len() as f64) < 0.95 {
        return false;
    }
    value
        .chars()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        >= 16
}

fn looks_like_html(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes
        .iter()
        .enumerate()
        .filter(|(index, byte)| {
            **byte == b'<'
                && bytes
                    .get(index + 1)
                    .is_some_and(|next| next.is_ascii_alphabetic() || matches!(*next, b'/' | b'!'))
        })
        .take(3)
        .count()
        >= 3
}

fn humanize_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn csv_quote_if_needed(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        csv_quote(value)
    } else {
        value.to_string()
    }
}

fn csv_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn flattens_uniform_nested_objects() {
        let items = (0..20)
            .map(|index| json!({"id": index, "meta": {"region": "us", "tier": "gold"}}))
            .collect::<Vec<_>>();
        let output = try_compact_csv_schema(&items, 5, 0.15, false, false)
            .unwrap()
            .output;
        assert!(output.starts_with("[20]{id:int,meta.region:string,meta.tier:string}\n"));
    }

    #[test]
    fn buckets_heterogeneous_rows_by_string_discriminator() {
        let items = (0..40)
            .map(|index| {
                if index % 2 == 0 {
                    json!({
                        "type": "user",
                        "id": index,
                        "display_name": format!("user-{index}"),
                        "email_address": format!("user-{index}@example.com")
                    })
                } else {
                    json!({
                        "type": "order",
                        "id": index,
                        "currency_code": "USD",
                        "total_amount_cents": index * 100
                    })
                }
            })
            .collect::<Vec<_>>();

        let output = try_compact_csv_schema(&items, 5, 0.15, false, false)
            .unwrap()
            .output;

        assert!(output.starts_with("__buckets:type\n"), "got: {output}");
        assert!(output.contains("__key:order\n[20]{"));
        assert!(output.contains("__key:user\n[20]{"));
        assert!(output.contains("USD,39,3900,order"));
        assert!(output.contains("user-38,user-38@example.com,38,user"));
    }

    #[test]
    fn bucket_discriminator_prefers_more_categories_and_rejects_ids() {
        let items = (0..40)
            .map(|index| {
                let mut item = serde_json::Map::new();
                item.insert("record_id".into(), json!(format!("record-{index}")));
                item.insert("kind".into(), json!(if index % 2 == 0 { "a" } else { "b" }));
                item.insert("phase".into(), json!(format!("phase-{}", index % 4)));
                item.insert(format!("field_{}", index % 4), json!(index));
                Value::Object(item)
            })
            .collect::<Vec<_>>();

        let output = try_compact_csv_schema(&items, 5, 0.0, false, false)
            .unwrap()
            .output;

        assert!(output.starts_with("__buckets:phase\n"), "got: {output}");
        assert!(!output.starts_with("__buckets:record_id\n"));
    }

    #[test]
    fn homogeneous_rows_are_not_bucketed_by_categorical_field() {
        let items = (0..40)
            .map(|index| {
                json!({
                    "id": index,
                    "type": if index % 2 == 0 { "user" } else { "order" },
                    "status": "ready"
                })
            })
            .collect::<Vec<_>>();

        let output = try_compact_csv_schema(&items, 5, 0.0, false, false)
            .unwrap()
            .output;

        assert!(output.starts_with("[40]{id:int,status:string,type:string}\n"));
    }

    #[test]
    fn singleton_bucket_falls_back_to_raw_json_value_table() {
        let items = vec![
            json!({"type": "bulk", "id": 1, "alpha": "a"}),
            json!({"type": "bulk", "id": 2, "bravo": "b"}),
            json!({"type": "bulk", "id": 3, "charlie": "c"}),
            json!({"type": "bulk", "id": 4, "delta": "d"}),
            json!({"type": "single", "id": 5, "exception_detail": "only once"}),
        ];

        let output = render_buckets(&items, false, &mut Vec::new()).unwrap();

        assert!(output.contains("__key:single\n[1]{value:json}\n"));
        assert!(
            output
                .contains(r#""{""type"":""single"",""id"":5,""exception_detail"":""only once""}""#),
            "got: {output}"
        );
    }

    #[test]
    fn opaque_classifier_matches_reference_priority_and_kinds() {
        let base64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=".repeat(5);
        let html = format!("<html><body><p>{}</p></body></html>", "content ".repeat(40));
        let plain = "diagnostic paragraph with ordinary words ".repeat(10);
        let stringified = serde_json::to_string(
            &(0..30)
                .map(|index| json!({"id": index, "status": "ready"}))
                .collect::<Vec<_>>(),
        )
        .unwrap();

        assert_eq!(classify_opaque_string(&base64), Some("base64"));
        assert_eq!(classify_opaque_string(&html), Some("html"));
        assert_eq!(classify_opaque_string(&plain), Some("string"));
        assert_eq!(classify_opaque_string(&stringified), None);
        assert_eq!(classify_opaque_string("short"), None);
    }
}
