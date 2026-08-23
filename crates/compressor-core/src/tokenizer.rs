//! Token 计数抽象。
//!
//! 不追求精确（不需要 tiktoken / HF 后端），用简单的字节级估算：
//! UTF-8 字节数 / 4 × 安全系数。语义与「Buffer.byteLength(text, "utf8") / 4」
//! 一致（Rust `String::len()` 即 UTF-8 字节数）。

/// token 计数器 trait，所有预算判断（阈值、回退校验）都经过它。
pub trait Tokenizer: Send + Sync {
    /// 后端名（如 "estimating"）。
    fn backend(&self) -> &'static str;
    /// 估算一段文本的 token 数。
    fn count_text(&self, text: &str) -> usize;
}

/// token 估算安全系数：补偿非英语文本（多字节字符）导致的低估。
const SAFETY_MARGIN: f64 = 1.2;

/// 字节级估算计数器：UTF-8 字节数 / 4 × 安全系数。
#[derive(Debug, Clone, Copy, Default)]
pub struct EstimatingCounter;

impl EstimatingCounter {
    pub const fn new() -> Self {
        Self
    }
}

impl Tokenizer for EstimatingCounter {
    fn backend(&self) -> &'static str {
        "estimating"
    }

    fn count_text(&self, text: &str) -> usize {
        ((text.len() as f64 / 4.0) * SAFETY_MARGIN).ceil() as usize
    }
}

/// registry：按模型名选后端（目前统一返回估算计数器）。
pub fn get_tokenizer(_model: &str) -> Box<dyn Tokenizer> {
    Box::new(EstimatingCounter::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimating_counter_sanity() {
        let t = EstimatingCounter::new();
        assert_eq!(t.count_text(""), 0);
        // 4 字节 → (4/4)*1.2 = 1.2 → ceil 2
        assert_eq!(t.count_text("abcd"), 2);
        // 100 字节 → 25*1.2 = 30
        assert_eq!(t.count_text(&"a".repeat(100)), 30);
    }

    #[test]
    fn multibyte_chars_counted_by_bytes() {
        let t = EstimatingCounter::new();
        // 中文「你好」= 6 字节 → (6/4)*1.2 = 1.8 → ceil 2
        assert_eq!(t.count_text("你好"), 2);
    }
}
