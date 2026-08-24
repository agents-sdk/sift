//! 压缩策略。

/// 计费/授权模式，决定压缩激进度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// 按量付费：可容忍更高的有损比例。
    Payg,
    /// 订阅（Claude Max 等）：配额敏感，压缩更保守。
    Subscription,
}

/// 压缩策略参数集。
#[derive(Debug, Clone, Copy)]
pub struct CompressionPolicy {
    /// 只压缩 live zone（冻结前缀之外的可变区）。
    pub live_zone_only: bool,
    /// 易变 token 阈值：低于此值不触发压缩。
    pub volatile_token_threshold: usize,
    /// 单次有损压缩的最大比例（0.0-1.0）。
    pub max_lossy_ratio: f64,
}

impl CompressionPolicy {
    pub fn for_mode(mode: AuthMode) -> Self {
        match mode {
            AuthMode::Payg => Self {
                live_zone_only: true,
                volatile_token_threshold: 128,
                max_lossy_ratio: 0.45,
            },
            AuthMode::Subscription => Self {
                live_zone_only: true,
                volatile_token_threshold: 32,
                max_lossy_ratio: 0.25,
            },
        }
    }
}

/// 缓存成本乘数（Anthropic 计费模型）。
pub const CACHE_WRITE_MULTIPLIER: f64 = 1.25;
pub const CACHE_WRITE_1H_MULTIPLIER: f64 = 2.0;
pub const CACHE_READ_MULTIPLIER: f64 = 0.1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_is_more_conservative() {
        let payg = CompressionPolicy::for_mode(AuthMode::Payg);
        let sub = CompressionPolicy::for_mode(AuthMode::Subscription);
        assert!(sub.volatile_token_threshold < payg.volatile_token_threshold);
        assert!(sub.max_lossy_ratio < payg.max_lossy_ratio);
    }
}
