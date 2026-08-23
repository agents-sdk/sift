//! 请求格式适配层：格式检测 + 各格式的 live-zone 定位与文本候选枚举。
//!
//! 设计：不把请求体转换成统一中间表示，直接在各格式的
//! [`serde_json::Value`] 上就地枚举可压缩文本候选（回调式，规避
//! 嵌套可变引用的借用检查问题），保证 Anthropic 既有路径行为不变。
//!
//! 支持的格式：
//! - [`RequestFormat::Anthropic`]：`/v1/messages` 风格（messages + content blocks）
//! - [`RequestFormat::ChatCompletions`]：OpenAI Chat Completions（messages +
//!   字符串 content 或 parts 数组；tool 输出为 `role:"tool"` 消息）
//! - [`RequestFormat::Responses`]：OpenAI Responses API（`input` 数组））

pub mod anthropic;
pub mod chat_completions;
pub mod responses;

use crate::cache_control;
use crate::live_zone::LiveZone;
use serde_json::Value;

/// 请求体格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestFormat {
    /// Anthropic /v1/messages 风格。
    Anthropic,
    /// OpenAI Chat Completions 风格。
    ChatCompletions,
    /// OpenAI Responses API 风格。
    Responses,
    /// 无法识别（无 messages 也无 input）。
    Unknown,
}

/// 检测请求体格式。
///
/// 判别顺序（先强键后弱键）：
/// 1. 顶层有 `input` → Responses；
/// 2. 有 OpenAI 强键（`role:"tool"` 消息、`tool_calls`、`tool_call_id`）→ ChatCompletions；
/// 3. 有 Anthropic 键（`cache_control`、`tool_result`/`tool_use` block、
///    顶层 `system` 字符串、顶层 `anthropic_version`）→ Anthropic；
/// 4. 均未命中（如全部 content 为纯字符串）→ 默认 Anthropic：
///    无 cache_control 时两种格式行为等价，向后兼容。
pub fn detect_request_format(body: &Value) -> RequestFormat {
    if body.get("input").is_some() {
        return RequestFormat::Responses;
    }
    let Some(messages) = body.get("messages").and_then(|m| m.as_array()) else {
        return RequestFormat::Unknown;
    };
    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str());
        if role == Some("tool") || msg.get("tool_calls").is_some() || msg.get("tool_call_id").is_some() {
            return RequestFormat::ChatCompletions;
        }
        // OpenAI parts 数组的 content part 带 "text" 字段但没有 cache_control /
        // tool_result 判别键，不足以与 Anthropic text block 区分，靠上面的强键。
    }
    // Anthropic 判别键。
    if body.get("anthropic_version").is_some() {
        return RequestFormat::Anthropic;
    }
    if body.get("system").map(|s| s.is_string()).unwrap_or(false) {
        return RequestFormat::Anthropic;
    }
    for msg in messages {
        let Some(blocks) = msg.get("content").and_then(|c| c.as_array()) else {
            continue;
        };
        for b in blocks {
            if b.get("cache_control").is_some_and(|v| !v.is_null()) {
                return RequestFormat::Anthropic;
            }
            match b.get("type").and_then(|t| t.as_str()) {
                Some("tool_result") | Some("tool_use") => return RequestFormat::Anthropic,
                _ => {}
            }
        }
    }
    RequestFormat::Anthropic
}

/// 各格式的冻结消息下界（统计用）：Anthropic 按 cache_control 标记，
/// OpenAI 格式没有显式前缀锚点，恒为 0。
pub fn frozen_message_count(body: &Value, fmt: RequestFormat) -> usize {
    match fmt {
        RequestFormat::Anthropic => cache_control::compute_frozen_count(body),
        _ => 0,
    }
}

/// 单条消息内的可压缩文本候选枚举策略。
pub(crate) trait TextCandidates: Send + Sync {
    /// 定位 live zone（floor..=ceiling 的消息下标区间）；无可压缩区间返回 None。
    fn live_zone(&self, body: &Value) -> Option<LiveZone>;

    /// 取消息数组容器（Anthropic/ChatCompletions 为 `messages`，Responses 为 `input`）。
    fn messages_mut<'a>(&self, body: &'a mut Value) -> Option<&'a mut Vec<Value>>;

    /// 枚举一条消息内的可压缩文本候选：对每个候选调用 `f(holder, field)`，
    /// 其中 `holder[field]` 是待压缩（且压缩后就地写回）的字符串字段。
    fn for_each_candidate<'a>(
        &self,
        msg: &'a mut Value,
        f: &mut dyn FnMut(&'a mut Value, &str),
    );
}

/// 按格式返回策略实例；Unknown 无策略。
pub(crate) fn strategy_for(fmt: RequestFormat) -> Option<&'static dyn TextCandidates> {
    match fmt {
        RequestFormat::Anthropic => Some(&anthropic::AnthropicFormat),
        RequestFormat::ChatCompletions => Some(&chat_completions::ChatCompletionsFormat),
        RequestFormat::Responses => Some(&responses::ResponsesFormat),
        RequestFormat::Unknown => None,
    }
}

// ────────────────────────────── 单元测试 ──────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---------- detect_request_format ----------

    #[test]
    fn detects_responses_by_input_key() {
        let body = json!({"model": "gpt-5", "input": [
            {"role": "user", "content": "hi"}
        ]});
        assert_eq!(detect_request_format(&body), RequestFormat::Responses);
    }

    #[test]
    fn detects_chat_completions_by_tool_role() {
        let body = json!({"messages": [
            {"role": "user", "content": "run the tool"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "f", "arguments": "{}"}}
            ]},
            {"role": "tool", "tool_call_id": "c1", "content": "output"}
        ]});
        assert_eq!(detect_request_format(&body), RequestFormat::ChatCompletions);
    }

    #[test]
    fn detects_anthropic_by_cache_control() {
        let body = json!({"messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "cached", "cache_control": {"type": "ephemeral"}}
            ]}
        ]});
        assert_eq!(detect_request_format(&body), RequestFormat::Anthropic);
    }

    #[test]
    fn detects_anthropic_by_tool_result_block() {
        let body = json!({"messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "output"}
            ]}
        ]});
        assert_eq!(detect_request_format(&body), RequestFormat::Anthropic);
    }

    #[test]
    fn detects_anthropic_by_top_level_system_string() {
        let body = json!({
            "system": "you are helpful",
            "messages": [{"role": "user", "content": "hi"}]
        });
        assert_eq!(detect_request_format(&body), RequestFormat::Anthropic);
    }

    #[test]
    fn ambiguous_string_content_defaults_to_anthropic() {
        // 无任何判别键：默认 Anthropic（无 cache_control 时行为与 ChatCompletions 等价）。
        let body = json!({"messages": [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "hello"},
        ]});
        assert_eq!(detect_request_format(&body), RequestFormat::Anthropic);
    }

    #[test]
    fn unknown_when_no_messages_or_input() {
        assert_eq!(detect_request_format(&json!({})), RequestFormat::Unknown);
        assert_eq!(
            detect_request_format(&json!({"prompt": "hi"})),
            RequestFormat::Unknown
        );
    }

    // ---------- frozen_message_count ----------

    #[test]
    fn frozen_count_per_format() {
        let anthropic = json!({"messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "cached", "cache_control": {"type": "ephemeral"}}
            ]},
            {"role": "user", "content": "live"}
        ]});
        assert_eq!(
            frozen_message_count(&anthropic, RequestFormat::Anthropic),
            1
        );
        assert_eq!(
            frozen_message_count(&anthropic, RequestFormat::ChatCompletions),
            0
        );
        assert_eq!(frozen_message_count(&anthropic, RequestFormat::Responses), 0);
    }

    // ---------- strategy_for ----------

    #[test]
    fn strategy_for_all_formats() {
        assert!(strategy_for(RequestFormat::Anthropic).is_some());
        assert!(strategy_for(RequestFormat::ChatCompletions).is_some());
        assert!(strategy_for(RequestFormat::Responses).is_some());
        assert!(strategy_for(RequestFormat::Unknown).is_none());
    }
}
