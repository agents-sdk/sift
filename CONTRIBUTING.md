# 参与贡献

感谢你参与改进 sift。本文档集中说明本地环境、架构边界、测试要求、文档联动和发布流程。

## 开始之前

请先阅读 [`.agents/PROJECT_MAP.md`](.agents/PROJECT_MAP.md)。模块职责、压缩管线、公开 API 和发布链均以该文档为准。

任何改动都必须遵守三条不变量：

1. 只在单条消息内部压缩，绝不跨消息丢弃内容。
2. `cache_control` 最后一个标记及其之前的冻结前缀必须保持字节不变。
3. 所有有损转换都必须可恢复：保存原文并输出 `<<stash:HASH>>` 标记。

## 仓库结构

```text
crates/sift/       纯 Rust 压缩逻辑
crates/sift-node/  精简的 napi-rs 类型桥
npm/core/          @agent-context/sift Node.js 包
tests/fixtures/    Golden 测试样本
site/              文档网站与 serverless 集成
```

压缩行为只能放在 `crates/sift`。`crates/sift-node` 只负责 Node.js 类型桥接。

## 环境要求

- 当前稳定版 Rust 工具链与 Cargo
- Node.js 与 npm
- Zig，仅在本机交叉编译全部平台二进制时需要

发布包支持 macOS、Linux 和 Windows 的 x64 / arm64 平台；Linux 同时提供 GNU 与 musl 版本。

## 本地初始化

```sh
git clone git@github.com:agents-sdk/sift.git
cd sift
cargo test --workspace

cd npm/core
npm install
npm run build
npm test
```

`npm run build` 会先为当前机器编译原生模块，再编译 TypeScript。

## 开发流程

新增功能或修改压缩行为时，先写测试再实现。可复用的 golden 样本放在 `tests/fixtures/`。

新增压缩器必须实现以下接口之一：

- 无损转换实现 `ReformatTransform`；
- 依赖 stash 恢复的有损转换实现 `OffloadTransform`。

在 `transforms::dispatch_compressor` 中注册新压缩器，并验证无结果、无节省以及 stash 写入失败时都会返回原始输入。

工程约定：

- 注释与文档使用中文；面向国际用户的 README 以英文为主并维护翻译版本。
- 标识符使用英文。
- 保留工作区内已有的用户修改。
- 改动保持聚焦，不要在特性或修复中混入无关重构。

## 测试

提交 Pull Request 前，运行与改动相关的检查：

```sh
# 完整 Rust workspace
cargo test --workspace

# 当前平台的原生模块与 TypeScript 构建
cd npm/core && npm run build

# Node.js 冒烟测试；该命令会先执行构建
cd npm/core && npm test

# 交互式压缩示例
cd npm/core && npm run demo

# 固定样例量化结果
cd npm/core && npm run benchmark
```

本机完整交叉编译：

```sh
cd npm/core && npm run build:cross
```

该命令需要 Zig，会生成 macOS/Linux 二进制和对应平台子包。Windows 二进制由 release workflow 构建。

## 文档联动要求

修改 `@agent-context/sift` 的公开 API 时——包括名称、签名、返回值或行为——必须在同一个改动中更新：

- `README.md` 及其翻译版本；
- `npm/core/README.md`；
- `.agents/PROJECT_MAP.md`；
- `site/src/pages/docs/` 下对应的页面。

如果官网运行时代码使用了变化后的 API，还必须同步 `site/vendor/sift`。

`site/` 下的生产内容修改后，必须先执行：

```sh
cd site && npm run build
```

随后发布到已绑定的 Vercel production 项目，并验证生产 URL 可访问。

更新 README 中的量化数据时，应使用 `npm/core/demo/cases` 下的固定样例，注明被测包版本或 commit，包含未发生压缩的样例，并明确 token 数据属于估算值。

## Pull Request

Pull Request 应说明：

- 面向用户的问题和预期结果；
- 受影响的压缩路径或请求格式；
- 三条不变量如何继续得到保证；
- 新增或运行了哪些测试；
- 对兼容性、stash 或 prompt cache 的影响。

## 发布

维护者通过推送 `v*` tag 发起发布。[`.github/workflows/release.yml`](.github/workflows/release.yml) 会构建平台矩阵、生成 `@agent-context/sift-<platform>` 子包，并先发布平台包，再发布根包。该流程需要仓库的 `NPM_TOKEN` secret。

除非是在处理失败的发布，并且已经确认包顺序和版本，否则不要手动发布。

## 许可证

提交贡献即表示你同意按仓库的 [Apache-2.0 许可证](LICENSE)发布这些内容。
