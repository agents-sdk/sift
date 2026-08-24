//! Anthropic /v1/messages 格式适配。
//!
//! live zone：floor = 冻结条数（cache_control 标记），ceiling = 最后一条
//! user 消息。默认只压缩 content block 数组内 tool_result 的 `content` 字段；
//! system/user/assistant 的普通文本属于 prompt 语义，不做有损压缩。

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
            // 默认只压工具输出。普通 text block 可能是 system/user/assistant
            // prompt，压缩会改变指令语义，应由调用方显式走 siftText。
            let kind = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if kind == "tool_result"
                && block.get("is_error").and_then(|v| v.as_bool()) != Some(true)
                && block.get("content").and_then(|t| t.as_str()).is_some()
            {
                f(block, "content");
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
            {"type": "tool_result", "tool_use_id": "t2", "is_error": true, "content": "failed"},
            {"type": "image", "source": {"type": "base64"}},
        ]});
        let mut seen: Vec<(String, String)> = Vec::new();
        AnthropicFormat.for_each_candidate(&mut msg, &mut |holder, field| {
            let v = holder.get(field).and_then(|v| v.as_str()).unwrap().to_string();
            seen.push((field.to_string(), v));
        });
        assert_eq!(seen, vec![("content".to_string(), "output".to_string())]);
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
