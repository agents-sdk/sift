//! 熵检测保密：识别 API key / token / 密码等高熵字符串。
//!
//! - **归一化熵**：字符频率的 Shannon 熵除以「当前字符集大小的最大熵」
//!   `log2(unique_chars)`，输出 [0,1]；`should_preserve` 阈值为 **0.85**。
//!   （trufflehog / detect-secrets 用 ~4.0 的原始 bits/char 阈值，在 62 字符
//!   字母表上等价于约 4/log2(62) ≈ 0.67；这里取更保守的 0.85，宁可过度保留。）
//! - **长度地板** `SECRET_ENTROPY_MIN_LENGTH = 20`：归一化熵对短但字符多样的
//!   英文词（如 "detailed"）几乎与真 secret 一样高，长度才是区分信号；该值
//!   与 secret 扫描器的熵地板一致。
//!
//! 用途：TextCrusher 在段落选择时强制保留 secret-like token；统一压缩管线还会
//! 在发布有损结果前逐次校验全部候选仍可见，任何缺失都会使该结果回退。

/// secret-like 判定的归一化熵阈值（对齐参考 `EntropyScore.compute` 默认值）。
pub const SECRET_ENTROPY_THRESHOLD: f64 = 0.85;

/// secret-like 判定的最小长度（字符数），对齐 `SECRET_ENTROPY_MIN_LENGTH`。
pub const SECRET_ENTROPY_MIN_LENGTH: usize = 20;

/// 归一化 Shannon 熵：`H / log2(去重字符数)`，范围 [0,1]。
///
/// 空串 / 单一字符集（唯一字符 ≤ 1）返回 0.0（与参考实现一致：
/// `max_entropy = log2(len(counter)) if len(counter) > 1 else 1.0`，
/// 单字符熵为 0）。
pub fn normalized_shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    // 字符频率统计（纯 std，BTreeMap 保证确定性）。
    let mut freq: std::collections::BTreeMap<char, usize> = std::collections::BTreeMap::new();
    let total = s.chars().count() as f64;
    for c in s.chars() {
        *freq.entry(c).or_insert(0) += 1;
    }
    // Shannon 熵（bits/char）。
    let mut entropy = 0.0f64;
    for &count in freq.values() {
        let p = count as f64 / total;
        entropy -= p * p.log2();
    }
    // 归一化：除以当前字符集的最大可能熵。
    let unique = freq.len();
    let max_entropy = if unique > 1 {
        (unique as f64).log2()
    } else {
        1.0
    };
    if max_entropy > 0.0 {
        entropy / max_entropy
    } else {
        0.0
    }
}

/// token（按空白切出的「词」）是否像 secret：长度 ≥ [`SECRET_ENTROPY_MIN_LENGTH`]
/// 且归一化熵 ≥ [`SECRET_ENTROPY_THRESHOLD`]。
pub fn is_secret_like(s: &str) -> bool {
    s.chars().count() >= SECRET_ENTROPY_MIN_LENGTH
        && normalized_shannon_entropy(s) >= SECRET_ENTROPY_THRESHOLD
}

/// 返回含 secret-like token 的行号（0 起，按 `\n` 切行、行内按空白切 token）。
///
/// 对齐参考实现的词级扫描语义（`re.finditer(r"\S+", content)`）：
/// 只看完整空白分隔的词，不做子串检测。
pub fn find_secret_lines(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.split_whitespace().any(is_secret_like))
        .map(|(i, _)| i)
        .collect()
}

/// 在结构化或混合内容中寻找 secret-like 连续 token。
///
/// 与 [`find_secret_lines`] 的空白切分不同，这里只把常见凭证字符
/// `[A-Za-z0-9_-]` 视为一个候选，避免把整行 compact JSON、grep 结果或 URL
/// 当成一个超长高熵 token 而误判。
pub fn contains_secret_token(text: &str) -> bool {
    credential_candidates(text).any(is_secret_like)
}

/// 源代码中的凭证检测。
///
/// 源码里常见很长且字符多样的纯字母标识符（例如 Java 异常类名和 camelCase
/// repository 方法），仅凭归一化熵会产生大量误报。源码候选因此还必须包含数字、
/// `_` 或 `-`；常见 API key、UUID、hex token 仍会命中。
pub fn contains_secret_token_in_source(text: &str) -> bool {
    credential_candidates(text).any(|candidate| {
        is_secret_like(candidate)
            && candidate
                .bytes()
                .any(|byte| byte.is_ascii_digit() || byte == b'_' || byte == b'-')
    })
}

/// 校验压缩结果仍逐次包含原文中的全部疑似凭据。
///
/// Headroom 用字符 mask 钉住高熵词；Sift 的各类型压缩器结构不同，因此在统一
/// 管线出口做等价的不变量校验。任何一个候选缺失都会拒绝该有损结果。
pub(crate) fn preserves_secret_tokens(original: &str, compressed: &str, source: bool) -> bool {
    let is_secret = |candidate: &&str| {
        is_secret_like(candidate)
            && (!source
                || candidate
                    .bytes()
                    .any(|byte| byte.is_ascii_digit() || byte == b'_' || byte == b'-'))
    };
    let mut required = std::collections::BTreeMap::<&str, usize>::new();
    for candidate in credential_candidates(original).filter(is_secret) {
        *required.entry(candidate).or_default() += 1;
    }
    if required.is_empty() {
        return true;
    }
    for candidate in credential_candidates(compressed).filter(is_secret) {
        if let Some(count) = required.get_mut(candidate) {
            *count = count.saturating_sub(1);
        }
    }
    required.values().all(|count| *count == 0)
}

fn credential_candidates(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
}

// ────────────────────────────── 单元测试 ──────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- 归一化熵 ----------

    #[test]
    fn entropy_empty_and_single_char() {
        assert_eq!(normalized_shannon_entropy(""), 0.0);
        assert_eq!(normalized_shannon_entropy("aaaaaaaaaa"), 0.0);
    }

    #[test]
    fn entropy_uniform_alphabet_is_one() {
        // 每个字符等频：熵 = log2(n)，归一化后恰为 1.0。
        let s = "abcdefghijklmnopqrstuvwxyz";
        let e = normalized_shannon_entropy(s);
        assert!(
            (e - 1.0).abs() < 1e-12,
            "等频字符集归一化熵应为 1.0, got {e}"
        );
    }

    #[test]
    fn entropy_low_for_repetitive_string() {
        // 少数字符高度重复：归一化熵远低于阈值。
        assert!(normalized_shannon_entropy("aaaaaaaaaaaaaaaaaaaab") < 0.85);
    }

    #[test]
    fn entropy_high_for_random_hex() {
        let hex = "3f9a2c8e71d4b06a5e93c1f7d82b4a6e";
        assert!(normalized_shannon_entropy(hex) >= 0.85);
    }

    // ---------- is_secret_like ----------

    #[test]
    fn detects_anthropic_style_api_key() {
        let key = "sk-ant-api03-9Xq2mB7vLpK4tRnZ8WjE5fHc3DsY6uA1bGvT0iOx";
        assert!(is_secret_like(key), "长随机 API key 应命中");
    }

    #[test]
    fn detects_long_hex_token() {
        assert!(is_secret_like("a1b2c3d4e5f60718293a4b5c6d7e8f901a2b3c4d"));
        assert!(is_secret_like("ghp_48xKq2mN7vJz3pLw9RtY5bEcVdXfGaHiQw"));
    }

    #[test]
    fn detects_uuid_like() {
        assert!(is_secret_like("550e8400-e29b-41d4-a716-446655440000"));
    }

    #[test]
    fn english_sentence_words_not_secret() {
        // 普通英文句子里的每个词都不应命中。
        for w in "The quick brown fox jumps over the lazy dog again and again".split_whitespace() {
            assert!(!is_secret_like(w), "普通词不得命中: {w}");
        }
    }

    #[test]
    fn long_low_entropy_english_word_not_secret() {
        // 长（≥20 字符）但字符重复、或普通英文长词：熵不足以过阈值。
        assert!(!is_secret_like("aaaaaaaaaaaaaaaaaaaa")); // 熵 0
        assert!(!is_secret_like("characterization")); // 普通英文长词
    }

    #[test]
    fn short_strings_never_secret() {
        // 长度地板：即便高熵（如 UUID 前半），短于 20 字符不命中。
        assert!(!is_secret_like("3f9a2c8e71d4b06a5e9")); // 19 字符
        assert!(!is_secret_like("a1b2c3d4e5f6"));
        assert!(!is_secret_like(""));
    }

    #[test]
    fn code_line_tokens_not_secret() {
        // 常见代码 token：关键字、短标识符、短十六进制常量等（长度或熵不足）。
        assert!(!is_secret_like("0xFFFFFFFF"));
        assert!(!is_secret_like("const"));
        assert!(!is_secret_like("my_var_name_01"));
        // 注：字符高度多样的长 token（如 `std::collections::HashMap`、
        // `https://example.com/api`，归一化熵 0.93-0.96）会命中阈值——这是
        // 启发式「宁可过度保留」的固有保守性
        //（误保留只是损失少量压缩率，漏保留才会丢失凭证）。
        assert!(is_secret_like("https://example.com/api"));
    }

    // ---------- find_secret_lines ----------

    #[test]
    fn find_secret_lines_reports_correct_indices() {
        let text = "first ordinary line\n\
                    export API_KEY=sk-ant-api03-9Xq2mB7vLpK4tRnZ8WjE5fHc\n\
                    another plain sentence here\n\
                    token: 91f0d3ab62c4e8577a3b9c1d4e5f6071\n\
                    trailing filler";
        assert_eq!(find_secret_lines(text), vec![1, 3]);
    }

    #[test]
    fn find_secret_lines_empty_when_none() {
        let text = "just some words\nand more words without any secrets";
        assert!(find_secret_lines(text).is_empty());
    }

    #[test]
    fn find_secret_lines_on_empty_text() {
        assert!(find_secret_lines("").is_empty());
    }

    #[test]
    fn find_secret_lines_multiple_tokens_same_line() {
        let text = format!("prefix {}\nplain", "91f0d3ab62c4e8577a3b9c1d4e5f6071");
        assert_eq!(find_secret_lines(&text), vec![0]);
    }

    #[test]
    fn structured_secret_scan_avoids_compact_json_false_positive() {
        let json =
            r#"[{"id":1,"name":"item-1","status":"ok"},{"id":2,"name":"item-2","status":"ok"}]"#;
        assert!(!contains_secret_token(json));

        let with_key = r#"{"token":"ghp_48xKq2mN7vJz3pLw9RtY5bEcVdXfGaHiQw"}"#;
        assert!(contains_secret_token(with_key));
    }

    #[test]
    fn source_scan_ignores_long_identifiers_but_keeps_common_credentials() {
        let java = "OrderNotFoundException findByUserIdOrderByCreatedAtDesc";
        assert!(contains_secret_token(java));
        assert!(!contains_secret_token_in_source(java));

        let with_key = r#"String token = "ghp_48xKq2mN7vJz3pLw9RtY5bEcVdXfGaHiQw";"#;
        assert!(contains_secret_token_in_source(with_key));
    }

    #[test]
    fn preservation_check_requires_every_secret_occurrence() {
        let key = "ghp_48xKq2mN7vJz3pLw9RtY5bEcVdXfGaHiQw";
        let original = format!("first {key}\nsecond {key}\nordinary filler");
        assert!(preserves_secret_tokens(&original, &original, false));
        assert!(!preserves_secret_tokens(
            &original,
            &format!("only one {key}"),
            false
        ));
    }

    #[test]
    fn source_preservation_ignores_long_identifiers() {
        let original = "OrderNotFoundException findByUserIdOrderByCreatedAtDesc";
        assert!(preserves_secret_tokens(original, "folded", true));
    }
}
