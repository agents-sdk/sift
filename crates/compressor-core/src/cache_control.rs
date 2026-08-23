//! 冻结前缀计算。
//!
//! 扫描 messages[*].content[*].cache_control 标记，返回冻结消息下界：
//! 下界以下的消息是 prompt cache 的锚点，字节不可动。

use serde_json::Value;

/// 返回最后一个带 cache_control 标记的消息的下标 + 1（即"冻结条数"）。
/// 没有任何标记时返回 0。
pub fn compute_frozen_count(body: &Value) -> usize {
    let Some(messages) = body.get("messages").and_then(|m| m.as_array()) else {
        return 0;
    };
    let mut frozen = 0;
    for (i, msg) in messages.iter().enumerate() {
        let has_marker = msg
            .get("content")
            .and_then(|c| c.as_array())
            .map(|blocks| {
                blocks
                    .iter()
                    .any(|b| b.get("cache_control").is_some_and(|v| !v.is_null()))
            })
            .unwrap_or(false);
        if has_marker {
            frozen = i + 1;
        }
    }
    frozen
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn no_marker_means_zero() {
        let body = json!({"messages": [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "hello"},
        ]});
        assert_eq!(compute_frozen_count(&body), 0);
    }

    #[test]
    fn frozen_count_after_last_marker() {
        let body = json!({"messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "system stuff", "cache_control": {"type": "ephemeral"}}
            ]},
            {"role": "assistant", "content": "ok"},
            {"role": "user", "content": "live"},
        ]});
        assert_eq!(compute_frozen_count(&body), 1);
    }
}
