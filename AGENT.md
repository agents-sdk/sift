# AGENT.md

sift — LLM 上下文压缩工具（Rust）：压缩 LLM 对话上下文，节省 token 与缓存成本。

## 必读

- `docs/PROJECT_MAP.md` — 工程地图：模块职责、压缩管线、发布链

## 三大不变量（任何改动不得违反）

1. 只在消息内压缩，绝不跨消息丢弃内容
2. 冻结前缀（cache_control 标记以下）字节不动
3. 有损压缩必须经 stash 可恢复（`<<stash:HASH>>` 标记 + store 原文）

## 常用命令

```sh
cargo test --workspace                 # Rust workspace 测试（全部必须过）
cd npm/core && npm run build           # 本机 .node + tsc 编译 TS
cd npm/core && npm run build:cross     # 交叉编译 6 平台 .node + 生成平台子包（需 zig）
cd npm/core && npm test                # TS 冒烟测试（先构建再运行）
```

发布：推 `v*` tag 触发 `.github/workflows/release.yml`（需 NPM_TOKEN secret）：
build 矩阵编各平台 → 生成 `@agent-context/sift-<platform>` 子包 → 依次 publish 子包 + 根包。

## 架构

- `crates/sift`：纯逻辑库，无 napi 依赖
- `crates/sift-node`：napi-rs cdylib 桥（→ `npm/core/native/sift.node`）
- `npm/core`：npm 包 **@agent-context/sift**（TypeScript），源码在 `src/`，tsc 产出到 `dist/`

## 核心 API 与官网联动约束

- 修改 `@agent-context/sift` 的公开 API（新增、删除、重命名、签名或行为变化）时，必须在同一任务中同步更新
  `README.md`、`npm/core/README.md`、`docs/PROJECT_MAP.md` 和 `site/src/pages/docs/` 下对应的官网文档与示例。
- 如果官网运行时代码使用了新增或变化的 API，还必须同步 `site/vendor/sift`，确保官网 serverless API
  实际加载的 vendored 包与文档一致。
- `site/` 下用于上线的内容修改完成后，必须先执行 `cd site && npm run build`，再发布到已绑定的 Vercel
  production 项目，并验证生产 URL 可访问；没有完成生产发布与验证，不得把官网更新报告为完成。

## 约定

- 逻辑只进 `sift`；`sift-node` 只做类型桥接，不含压缩逻辑
- 新压缩器实现 `ReformatTransform`（无损）或 `OffloadTransform`（有损+stash），
  并在 `transforms::dispatch_compressor` 注册
- 新特性先写测试再写实现（golden 样本放 `tests/fixtures/`）
- 注释与文档用中文，标识符用英文

> 历史：初始版本的部分设计与实现参考了 Apache-2.0 项目 headroom
> （见 `references/headroom/`，只读；归属见 `NOTICE`）。
