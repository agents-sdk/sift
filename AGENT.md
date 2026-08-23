# AGENT.md

LLM 上下文压缩工具（Rust）：压缩 LLM 对话上下文，节省 token 与缓存成本。

## 必读

- `docs/PROJECT_MAP.md` — 工程地图：模块职责、压缩管线、发布链

## 三大不变量（任何改动不得违反）

1. 只在消息内压缩，绝不跨消息丢弃内容
2. 冻结前缀（cache_control 标记以下）字节不动
3. 有损压缩必须经 CCR 可恢复（`<<ccr:HASH>>` 标记 + store 原文）

## 常用命令

```sh
cargo test --workspace                 # Rust 测试（320 个，全部必须过）
cd npm/core && npm run build           # 本机 .node + tsc 编译 TS
cd npm/core && npm run build:cross     # 交叉编译 6 平台 .node + 生成平台子包（需 zig）
cd npm/core && npm test                # TS 冒烟测试（先构建再运行）
```

发布：推 `v*` tag 触发 `.github/workflows/release.yml`（需 NPM_TOKEN secret）：
build 矩阵编各平台 → 生成 `@compressor/core-<platform>` 子包 → 依次 publish 子包 + 根包。

## 架构

- `crates/compressor-core`：纯逻辑库，无 napi 依赖
- `crates/compressor-node`：napi-rs cdylib 桥（→ `npm/core/native/compressor.node`）
- `npm/core`：npm 包 **@compressor/core**（TypeScript），源码在 `src/`，tsc 产出到 `dist/`

## 约定

- 逻辑只进 `compressor-core`；`compressor-node` 只做类型桥接，不含压缩逻辑
- 新压缩器实现 `ReformatTransform`（无损）或 `OffloadTransform`（有损+CCR），
  并在 `transforms::dispatch_compressor` 注册
- 新特性先写测试再写实现（golden 样本放 `tests/fixtures/`）
- 注释与文档用中文，标识符用英文

> 历史：初始版本的部分设计与实现参考了 Apache-2.0 项目 headroom
> （见 `references/headroom/`，只读；归属见 `NOTICE`）。
