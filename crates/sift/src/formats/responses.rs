//! OpenAI Responses API 格式适配。
//!
//! 请求体的 `input` 为数组（字符串形态是当前 query，不压缩）。live zone
//! 覆盖整个 input 数组，默认候选只包括：
//! - `{type: "function_call_output", call_id, output}` item → `output`
//!   （字符串，或 content parts 数组里的文本 part）。

use super::TextCandidates;
use crate::live_zone::LiveZone;
use serde_json::Value;

/// Responses API 策略。
pub(crate) struct ResponsesFormat;

/// content part 的 type 是否为文本变体（OpenAI 惯例 `*_text` 后缀；
/// 官方 input 里实际存在的是 `input_text` / `output_text`）。
fn is_text_part(part_type: &str) -> bool {
    part_type == "input_text" || part_type == "output_text" || part_type == "summary_text"
}

impl TextCandidates for ResponsesFormat {
    fn live_zone(&self, body: &Value) -> Option<LiveZone> {
        let items = body.get("input")?.as_array()?;
        if items.is_empty() {
            return None;
        }
        Some(LiveZone {
            floor: 0,
            ceiling: items.len() - 1,
        })
    }

    fn messages_mut<'a>(&self, body: &'a mut Value) -> Option<&'a mut Vec<Value>> {
        body.get_mut("input")?.as_array_mut()
    }

    fn for_each_candidate<'a>(
        &self,
        msg: &'a mut Value,
        f: &mut dyn FnMut(&'a mut Value, &str),
    ) {
        // function_call_output：工具输出在 `output` 字段——字符串形态，
        // 或 content parts 数组（[{type:"input_text", text}, ...]）。
        if msg.get("type").and_then(|t| t.as_str()) != Some("function_call_output") {
            // user/developer/system/assistant 文本都属于 prompt 语义，默认保护。
            return;
        }
        if msg.get("output").map(|o| o.is_string()).unwrap_or(false) {
            f(msg, "output");
            return;
        }
        if let Some(Value::Array(parts)) = msg.get_mut("output") {
            for part in parts {
                if is_text_part(part.get("type").and_then(|t| t.as_str()).unwrap_or(""))
                    && part.get("text").and_then(|t| t.as_str()).is_some()
                {
                    f(part, "text");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn live_zone_ceiling_last_user_item() {
        let body = json!({"input": [
            {"type": "function_call_output", "call_id": "c1", "output": "out"},
            {"role": "user", "content": [
                {"type": "input_text", "text": "q1"}
            ]},
        ]});
        let zone = ResponsesFormat.live_zone(&body).unwrap();
        assert_eq!((zone.floor, zone.ceiling), (0, 1));
    }

    #[test]
    fn live_zone_includes_trailing_function_output() {
        let body = json!({"input": [
            {"role": "user", "content": [{"type": "input_text", "text": "run"}]},
            {"type": "function_call", "call_id": "c1", "name": "f", "arguments": "{}"},
            {"type": "function_call_output", "call_id": "c1", "output": "result"},
        ]});
        let zone = ResponsesFormat.live_zone(&body).unwrap();
        assert_eq!((zone.floor, zone.ceiling), (0, 2));
    }

    #[test]
    fn string_input_means_no_zone() {
        // input 为纯字符串 = 当前 query，不压缩。
        let body = json!({"input": "just the current prompt"});
        assert!(ResponsesFormat.live_zone(&body).is_none());
    }

    #[test]
    fn candidates_text_parts_and_function_output() {
        let mut item = json!({"role": "user", "content": [
            {"type": "input_text", "text": "hello"},
            {"type": "input_image", "image_url": "data:..."},
            {"type": "output_text", "text": "prior answer"},
        ]});
        let mut seen = Vec::new();
        ResponsesFormat.for_each_candidate(&mut item, &mut |h, field| {
            seen.push(h.get(field).unwrap().as_str().unwrap().to_string());
        });
        assert!(seen.is_empty());

        let mut item = json!({"type": "function_call_output", "call_id": "c1", "output": "tool out"});
        let mut seen = Vec::new();
        ResponsesFormat.for_each_candidate(&mut item, &mut |h, field| {
            seen.push((field.to_string(), h.get(field).unwrap().as_str().unwrap().to_string()));
        });
        assert_eq!(seen, vec![("output".to_string(), "tool out".to_string())]);
    }

    #[test]
    fn function_call_output_array_parts_enumerated() {
        // output 也可以是 content parts 数组（官方 schema 允许），文本 part 是候选。
        let mut item = json!({"type": "function_call_output", "call_id": "c1", "output": [
            {"type": "input_text", "text": "structured tool output"},
            {"type": "input_image", "image_url": "data:..."},
        ]});
        let mut seen = Vec::new();
        ResponsesFormat.for_each_candidate(&mut item, &mut |h, field| {
            seen.push(h.get(field).unwrap().as_str().unwrap().to_string());
        });
        assert_eq!(seen, vec!["structured tool output".to_string()]);
    }

    #[test]
    fn function_call_arguments_not_enumerated() {
        // function_call（模型发出的调用）不是压缩候选。
        let mut item = json!({
            "type": "function_call",
            "call_id": "c1",
            "name": "f",
            "arguments": "{\"a\":1}"
        });
        let mut called = false;
        ResponsesFormat.for_each_candidate(&mut item, &mut |_, _| called = true);
        assert!(!called);
    }
}
