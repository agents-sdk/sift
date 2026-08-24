//! 安全不变量：tool_use / tool_result 配对保护。


/// 一对必须同进退的 tool_use（assistant）与 tool_result（user）消息下标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolPair {
    pub tool_use_index: usize,
    pub tool_result_index: usize,
}

/// 找出 messages 中所有 tool_use/tool_result 配对。
/// 压缩决策时，配对两侧要么都动、要么都不动，否则 API 会拒绝请求。
pub fn tool_pair_indices(messages: &[serde_json::Value]) -> Vec<ToolPair> {
    let mut pairs = Vec::new();
    for (i, msg) in messages.iter().enumerate() {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let Some(blocks) = msg.get("content").and_then(|c| c.as_array()) else {
            continue;
        };
        let has_tool_use = blocks
            .iter()
            .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"));
        if role == "assistant" && has_tool_use {
            // 对应的 tool_result 在紧随其后的 user 消息中
            if let Some(next) = messages.get(i + 1) {
                let next_has_result = next
                    .get("content")
                    .and_then(|c| c.as_array())
                    .map(|bs| {
                        bs.iter().any(|b| {
                            b.get("type").and_then(|t| t.as_str()) == Some("tool_result")
                        })
                    })
                    .unwrap_or(false);
                if next_has_result {
                    pairs.push(ToolPair {
                        tool_use_index: i,
                        tool_result_index: i + 1,
                    });
                }
            }
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_adjacent_pairs() {
        let msgs = vec![
            json!({"role": "user", "content": "q"}),
            json!({"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "bash"}
            ]}),
            json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "out"}
            ]}),
        ];
        assert_eq!(
            tool_pair_indices(&msgs),
            vec![ToolPair { tool_use_index: 1, tool_result_index: 2 }]
        );
    }
}
