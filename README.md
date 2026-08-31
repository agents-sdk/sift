# sift

LLM 上下文压缩工具：在发送给 LLM 之前压缩对话上下文，节省 token 与缓存成本。核心为纯 Rust 实现，通过 napi-rs 以 npm 包 **`@agent-context/sift`** 提供 Node.js API。

## 特性

- **多请求格式**（自动检测）：Anthropic `/v1/messages`、OpenAI Chat Completions、OpenAI Responses API；`siftRequest` 默认只压缩工具输出并保护 system/user/assistant prompt，另有 `siftText` 显式压缩单条字符串
- **按内容类型分发压缩器**：JSON 数组统计压缩、构建/测试日志、搜索结果、unified diff、纯文本完整块去重（同章节完全相同的段落/发言块保留首份）、tree-sitter AST 代码压缩（8 语言）
- **无损优先**：先尝试无损重排（JSON minify、日志模板化），缩小到 ≤80% 即短路，不引入任何信息损失
- **有损可恢复**：有损压缩的原文确认写入 stash store 后，输出才留下 `<<stash:HASH>>` 标记，可按 key 取回原文
- **安全兜底**：冻结前缀（prompt cache 锚点）字节不动；tool_use/tool_result 配对保护；熵检测识别 API key / 凭证并强制保留；自定义 XML 标签占位保护

## 三大不变量

任何改动不得违反：

1. 只在消息内压缩，绝不跨消息丢弃内容
2. 冻结前缀（`cache_control` 标记以下）字节不动
3. 有损压缩必须经 stash 可恢复（`<<stash:HASH>>` 标记 + store 原文）

## 安装与使用

```sh
npm install @agent-context/sift
```

```ts
import { createSift, siftRequest, siftText, retrieve, retrieveLines } from "@agent-context/sift";

// 请求体压缩（自动检测 Anthropic / OpenAI 格式）
const { body, changed, tokensSaved } = siftRequest(requestBody, "用户当前的问题");

// 或直接压缩源码；折叠点直接指向已落盘的 stash 文件
const r = siftText(fileContent, "用户当前的问题");
// // ... 30 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 32

// 若知道真实路径，可额外传入以通过扩展名稳定选择语言 grammar
const withPath = siftText(fileContent, undefined, "src/services/OrderService.java");

// 有损压缩的原文可按标记取回
const original = retrieve(r.stashKey!);

// 也可从 stash 只取回需要的分片
const fragment = retrieveLines(r.stashKey!, 120, 80);

// 如需独立 stash 目录，创建绑定到该目录的实例
const isolatedSift = createSift({ stashDir: "/var/lib/my-app/sift-stash" });
const isolated = isolatedSift.siftText(toolOutput);
const isolatedOriginal = isolatedSift.retrieve(isolated.stashKey!);
```

完整 API 说明见 [`npm/core/README.md`](npm/core/README.md)。

纯文本默认保守去重，不按 query、位置或目标比例删除独有内容。不同编号、数字、状态或发言人的块不会因为句式相似而合并；标题、代码围栏、可识别的命令和结论块保留。没有明确重复或提示抵消收益时原样返回。重复次数/位置仍属于原文信息，因此折叠后仍是有损压缩，原文保存在 stash。

落盘 stash 的行片段提示覆盖源码折叠、搜索结果抽稀、日志、Git Diff 和整行纯文本：
`[... 30 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 32]`。
提示就地放在省略位置，坐标指向 stash 文件本身，Agent 可直接分片读文件；不依赖原始文件路径或 Sift API。
可精确验证的混合内容整行分段也支持。无损 minify 不产生 stash；JSON 结构采样、行内片段或标签保护导致映射变化时，暂不伪造行号。
日志首个非空行、可识别的命令回显及显式续行强制保留，不受日志行数预算截断，也不会被模板化隐藏。

## 工程结构

```
crates/
  sift/          # 压缩核心库（纯逻辑，无 napi 依赖）
  sift-node/     # napi-rs cdylib 桥
npm/core/        # npm 包 @agent-context/sift（TypeScript）
.agents/PROJECT_MAP.md  # 工程地图：模块职责、压缩管线、发布链
tests/fixtures/      # 压缩输入/输出 golden 样本
```

## 开发

```sh
cargo test --workspace                 # Rust 测试（全部必须过）
cd npm/core && npm run build           # 本机 .node + tsc 编译 TS
cd npm/core && npm run build:cross     # 本机交叉编译 macOS/Linux 6 平台（需 zig）
cd npm/core && npm test                # TS 冒烟测试（先构建再运行）
```

Linux GNU 预编译包以 glibc 2.28 为最高 ABI 基线，可运行于 Oracle Linux 8.4 及 glibc 更新的发行版。

- CI（PR/push）：clippy + cargo test + npm 冒烟
- 发布：推 `v*` tag 触发 release 流水线，编 macOS/Linux/Windows 8 平台二进制并 publish 平台子包 + 根包

## License

[Apache-2.0](LICENSE)。初始版本的部分设计与实现参考了 Apache-2.0 项目
[headroom](https://github.com/chopratejas/headroom)，归属说明见 [NOTICE](NOTICE)。
