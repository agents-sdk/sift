//! OpenAI Chat Completions 格式适配。
//!
//! live zone：无 cache_control 前缀锚点，floor = 0，ceiling = 最后一条
//! `role:"user"` 消息。候选：
//! - content 为字符串 → 消息本身的 `content` 字段（含 `role:"tool"` 的工具输出）；
//! - content 为 parts 数组 → 各 `type:"text"` part 的 `text` 字段
//!   （image_url 等其他 part 跳过）。
//!
//! assistant 消息的 `tool_calls`（工具调用参数）不压缩——那是模型发出的
//! 结构化调用，不是历史负载。

use super::TextCandidates;
use crate::live_zone::LiveZone;
use serde_json::Value;

/// Chat Completions 策略。
pub(crate) struct ChatCompletionsFormat;

impl TextCandidates for ChatCompletionsFormat {
    fn live_zone(&self, body: &Value) -> Option<LiveZone> {
        let messages = body.get("messages")?.as_array()?;
        if messages.is_empty() {
            return None;
        }
        let ceiling = messages
            .iter()
            .rposition(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))?;
        Some(LiveZone { floor: 0, ceiling })
    }

    fn messages_mut<'a>(&self, body: &'a mut Value) -> Option<&'a mut Vec<Value>> {
        body.get_mut("messages")?.as_array_mut()
    }

    fn for_each_candidate<'a>(
        &self,
        msg: &'a mut Value,
        f: &mut dyn FnMut(&'a mut Value, &str),
    ) {
        let is_string = msg.get("content").is_some_and(|c| c.is_string());
        if is_string {
            f(msg, "content");
            return;
        }
        if let Some(Value::Array(parts)) = msg.get_mut("content") {
            for part in parts {
                if part.get("type").and_then(|t| t.as_str()) == Some("text")
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
    fn live_zone_floor_zero_ceiling_last_user() {
        let body = json!({"messages": [
            {"role": "system", "content": "sys"},
            {"role": "user", "content": "q1"},
            {"role": "assistant", "content": "a1"},
            {"role": "tool", "tool_call_id": "c1", "content": "out"},
            {"role": "user", "content": "q2"},
        ]});
        let zone = ChatCompletionsFormat.live_zone(&body).unwrap();
        assert_eq!((zone.floor, zone.ceiling), (0, 4));
    }

    #[test]
    fn no_user_message_means_no_zone() {
        let body = json!({"messages": [
            {"role": "system", "content": "sys"},
            {"role": "assistant", "content": "a1"},
        ]});
        assert!(ChatCompletionsFormat.live_zone(&body).is_none());
    }

    #[test]
    fn candidates_string_content_and_parts() {
        // 字符串 content：宿主是消息本身。
        let mut msg = json!({"role": "tool", "tool_call_id": "c1", "content": "tool output"});
        let mut seen = Vec::new();
        ChatCompletionsFormat.for_each_candidate(&mut msg, &mut |h, field| {
            seen.push((field.to_string(), h.get(field).unwrap().as_str().unwrap().to_string()));
        });
        assert_eq!(seen, vec![("content".to_string(), "tool output".to_string())]);

        // parts 数组：只取 type:"text" 的 part，image 跳过。
        let mut msg = json!({"role": "user", "content": [
            {"type": "text", "text": "look at this"},
            {"type": "image_url", "image_url": {"url": "data:..."}},
            {"type": "text", "text": "please"},
        ]});
        let mut seen = Vec::new();
        ChatCompletionsFormat.for_each_candidate(&mut msg, &mut |h, field| {
            seen.push(h.get(field).unwrap().as_str().unwrap().to_string());
        });
        assert_eq!(seen, vec!["look at this".to_string(), "please".to_string()]);
    }

    #[test]
    fn tool_calls_not_enumerated() {
        // assistant 的 tool_calls 是结构化调用参数，不是压缩候选。
        let mut msg = json!({"role": "assistant", "content": null, "tool_calls": [
            {"id": "c1", "type": "function", "function": {"name": "f", "arguments": "{\"a\":1}"}}
        ]});
        let mut called = false;
        ChatCompletionsFormat.for_each_candidate(&mut msg, &mut |_, _| called = true);
        assert!(!called);
    }
}
