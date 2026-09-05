//! 规则 JSON 对象数组的 CSV-schema 紧凑化。
//!
//! 所有行都保留，只把每行重复的字段名提升为一次 schema 声明。仅当相对紧凑
//! JSON 至少节省配置比例时采用；否则交回 SmartCrusher 的有损抽样路径。

use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Debug, Clone)]
struct Column {
    name: String,
    parent: String,
    child: Option<String>,
    type_tag: &'static str,
    nullable: bool,
}

pub(super) fn try_compact_csv_schema(
    items: &[Value],
    min_items: usize,
    min_savings_ratio: f64,
    require_lossless_shape: bool,
) -> Option<String> {
    if items.len() < min_items || !items.iter().all(Value::is_object) {
        return None;
    }
    if require_lossless_shape && !has_lossless_shape(items) {
        return None;
    }
    let columns = build_columns(items, !require_lossless_shape);
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
                format_cell(value)
            })
            .collect::<Vec<_>>();
        output.push_str(&cells.join(","));
        output.push('\n');
    }

    let compact_json = serde_json::to_string(items).ok()?;
    let savings_ratio = 1.0 - output.len() as f64 / compact_json.len().max(1) as f64;
    (savings_ratio >= min_savings_ratio).then_some(output)
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
    let mut frequencies = BTreeMap::<String, usize>::new();
    for object in items.iter().filter_map(Value::as_object) {
        for key in object.keys() {
            *frequencies.entry(key.clone()).or_default() += 1;
        }
    }
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

fn format_cell(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::String(value)) => csv_quote_if_needed(value),
        Some(value) => csv_quote(&serde_json::to_string(value).unwrap_or_default()),
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
        let output = try_compact_csv_schema(&items, 5, 0.15, false).unwrap();
        assert!(output.starts_with("[20]{id:int,meta.region:string,meta.tier:string}\n"));
    }
}
