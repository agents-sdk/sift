//! sift：LLM 上下文压缩核心库。
//!
//! 设计不变量（见 .agents/PROJECT_MAP.md）：
//! 1. 只在消息内压缩，绝不跨消息丢弃内容；
//! 2. 冻结前缀（cache_control 标记以下）字节不动；
//! 3. 有损压缩必须可通过 stash 卸载恢复端到端无损。
//!
//! 模块总览：
//! - [`tokenizer`]：token 计数 trait 与实现（一切预算判断的基础）
//! - [`policy`]：AuthMode / CompressionPolicy（压缩激进度决策）
//! - [`cache_control`]：冻结消息下界计算
//! - [`safety`]：tool_use/tool_result 配对保护
//! - [`content`]：内容类型检测（压缩分发键）
//! - [`stash`]：内容暂存(store)（有损压缩的恢复通道）
//! - [`transforms`]：具体压缩变换与管线 trait
//! - [`formats`]：请求格式适配层（Anthropic / Chat Completions / Responses）
//! - [`live_zone`]：live-zone 压缩入口（字节区间手术）

pub mod cache_control;
pub mod stash;
pub mod content;
pub mod formats;
pub mod live_zone;
pub mod mixed_content;
pub mod policy;
pub mod recursive_json;
pub mod relevance;
pub mod safety;
pub mod secrets;
pub mod signals;
pub mod text_api;
pub mod tokenizer;
pub mod transforms;
