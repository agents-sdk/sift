//! Anthropic /v1/messages 格式适配。
//!
//! live zone：floor = 冻结条数（cache_control 标记），ceiling = 最后一条
//! user 消息。候选：content block 数组内 tool_result 的 `content` 字段
//! 与其余 block 的 `text` 字段；content 为纯字符串的消息不处理（既有行为）。

use super::TextCandidates;
use crate::cache_control::compute_frozen_count;
use crate::live_zone::LiveZone;
use serde_json::Value;

/// Anthropic /v1/messages 策略。
pub(crate) struct AnthropicFormat;

impl TextCandidates for AnthropicFormat {
    fn live_zone(&self, body: &Value) -> Option<LiveZone> {
        let messages = body.get("messages")?.as_array()?;
        if messages.is_empty() {
            return None;
        }
        let floor = compute_frozen_count(body);
        let ceiling = messages
            .iter()
            .rposition(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))?;
        (ceiling >= floor && floor < messages.len()).then_some(LiveZone { floor, ceiling })
    }

    fn messages_mut<'a>(&self, body: &'a mut Value) -> Option<&'a mut Vec<Value>> {
        body.get_mut("messages")?.as_array_mut()
    }

    fn for_each_candidate<'a>(
        &self,
        msg: &'a mut Value,
        f: &mut dyn FnMut(&'a mut Value, &str),
    ) {
        let Some(blocks) = msg.get_mut("content").and_then(|c| c.as_array_mut()) else {
            return;
        };
        for block in blocks {
            // 候选文本来源：text block 的 `text` 字段，或 tool_result block
            // 的 `content` 字段（字符串形态）。不改 block 结构，
            // tool_use/tool_result 配对不受影响。
            let kind = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let text_field = if kind == "tool_result" { "content" } else { "text" };
            if block.get(text_field).and_then(|t| t.as_str()).is_some() {
                f(block, text_field);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn live_zone_above_frozen_floor() {
        let body = json!({"messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "cached", "cache_control": {"type": "ephemeral"}}
            ]},
            {"role": "assistant", "content": "old answer"},
            {"role": "user", "content": "latest question"},
        ]});
        let zone = AnthropicFormat.live_zone(&body).unwrap();
        assert_eq!((zone.floor, zone.ceiling), (1, 2));
    }

    #[test]
    fn candidates_cover_text_and_tool_result() {
        let mut msg = json!({"role": "user", "content": [
            {"type": "text", "text": "hello"},
            {"type": "tool_result", "tool_use_id": "t1", "content": "output"},
            {"type": "image", "source": {"type": "base64"}},
        ]});
        let mut seen: Vec<(String, String)> = Vec::new();
        AnthropicFormat.for_each_candidate(&mut msg, &mut |holder, field| {
            let v = holder.get(field).and_then(|v| v.as_str()).unwrap().to_string();
            seen.push((field.to_string(), v));
        });
        assert_eq!(
            seen,
            vec![
                ("text".to_string(), "hello".to_string()),
                ("content".to_string(), "output".to_string()),
            ]
        );
    }

    #[test]
    fn string_content_message_has_no_candidates() {
        // Anthropic 允许 content 为纯字符串；既有行为是跳过。
        let mut msg = json!({"role": "user", "content": "plain string"});
        let mut called = false;
        AnthropicFormat.for_each_candidate(&mut msg, &mut |_, _| called = true);
        assert!(!called);
    }
}
