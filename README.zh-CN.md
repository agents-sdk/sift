# sift

**发送更少的上下文，原文依然触手可及。**

sift 在请求发送给 LLM 之前压缩大型工具输出，降低 token 用量和 prompt cache 成本；有损压缩的完整原文会保存到本地 stash，随时可以恢复。压缩核心由 Rust 编写，并通过 [`@agent-context/sift`](npm/core/README.md) 提供 Node.js API。

[English](README.md) · 简体中文 · [日本語](README.ja.md) · [Español](README.es.md)

### **上下文减少 71.5%，估算节省 17,006 tokens，有损基准样例全部恢复成功。**

11 个内置基准场景合计从 **79,280 B 降至 22,588 B**，其中 **9/9 个有损样例均成功恢复完整原文**。[查看完整实测数据。](#实际能节省多少)

```sh
npm install @agent-context/sift
```

状态：**Alpha** · 1.0 前 API 细节可能变化 · [运行注意事项](#运行注意事项)

## 现成的 Agent 集成

如果使用 [Pi](https://github.com/earendil-works/pi) 或 [OpenCode](https://github.com/anomalyco/opencode)，可以直接安装对应适配器。它们会自动压缩新产生的工具输出，并注册 `sift_retrieve` 工具，让 Agent 在需要时取回 stash 中的原文：

- **Pi：**`pi install npm:@agent-context/pi-sift`
- **OpenCode：**在 `opencode.json` 的 `plugin` 数组中加入 `["@agent-context/opencode-sift", { "minLength": 200 }]`

完整安装、配置、存储和排障说明见 [agents-sdk/sift-plugins](https://github.com/agents-sdk/sift-plugins)。

## 为什么使用 sift？

Agent 对话很容易被构建日志、搜索结果、diff、源码和 JSON 响应撑大。这些内容通常重复很多，真正影响下一步推理的信息却只占一小部分。每轮都重新发送完整内容，不仅消耗 token，也会挤占更重要的上下文。

sift 的优势：

- **降低上下文成本**：内置基准样例体积减少 **71.5%**，从 79,280 字节降至 22,588 字节。
- **优先保留关键信息**：错误、堆栈、命令、相关搜索结果和结构信息优先可见。
- **有损但可恢复**：只有在完整原文成功写入 stash 后，才会返回带 `<<stash:HASH>>` 标记的有损结果。
- **保护 prompt cache**：Anthropic `cache_control` 锚点及其之前的消息完全不动。
- **一次接入，多种格式**：自动识别 Anthropic Messages、OpenAI Chat Completions 和 OpenAI Responses。
- **Rust 核心，Node.js 易用性**：编译型压缩核心通过精简的 Node.js API 提供。

### 比直接截断更有效，比单向摘要更安全

| 方案 | 内容感知 | 原文可恢复 | 保护 Anthropic 缓存前缀 | 无节省时拒绝结果 |
| --- | :---: | :---: | :---: | :---: |
| 直接截断 | 否 | 否 | 不一定 | 否 |
| LLM 摘要 | 部分 | 通常不能 | 不一定 | 否 |
| **sift** | **是** | **是** | **是** | **是** |

## 实际能节省多少？

以下数据使用仓库内 11 个固定的 [demo 输入](npm/core/demo/cases)，由当前源码实测（包版本字段为 `0.0.1-alpha.7`）。测试口径和复现方式见 [BENCHMARK.md](BENCHMARK.md)。

| 场景 | 输入 | 输出 | 体积减少 | 估算节省 token | 恢复验证 |
| --- | ---: | ---: | ---: | ---: | --- |
| JSON 数组 | 18,397 B | 1,888 B | 89.7% | 4,953 | PASS |
| Pretty JSON | 3,642 B | 2,201 B | 39.6% | 432 | 无损 |
| 构建日志 | 3,073 B | 1,543 B | 49.8% | 459 | 无损 |
| 搜索结果 | 10,057 B | 3,227 B | 67.9% | 2,049 | PASS |
| Git diff | 23,007 B | 7,795 B | 66.1% | 4,564 | PASS |
| 混合命令输出 | 9,240 B | 1,037 B | 88.8% | 2,460 | PASS |
| Rust 源代码 | 2,282 B | 572 B | 74.9% | 513 | PASS |
| 重复纯文本 | 2,723 B | 454 B | 83.3% | 680 | PASS |
| 独有事实与敏感值保护 | 3,125 B | 1,540 B | 50.7% | 476 | PASS |
| HTML 正文提取 | 1,036 B | 337 B | 67.5% | 209 | PASS |
| 结构化配置 | 2,698 B | 1,994 B | 26.1% | 211 | PASS |
| **合计** | **79,280 B** | **22,588 B** | **71.5%** | **17,006** | **9/9 有损样例恢复成功** |

这是公开样例的透明结果，不代表所有工作负载。疑似凭据会继续留在可见输出中，其余低价值内容仍可压缩；所有有损样例都能恢复完整原文。`tokensSaved` 使用 sift 内置估算器；实际 token 数和节省比例会随模型 tokenizer 与输入数据变化。

## 快速开始

```sh
npm install @agent-context/sift
```

在请求发送给模型前压缩：

```ts
import OpenAI from "openai";
import { siftRequest } from "@agent-context/sift";

const openai = new OpenAI();
const request = {
  model: "gpt-5.6-sol",
  input: conversationWithLargeToolOutputs,
};

const result = siftRequest(request, currentUserQuestion);
const response = await openai.responses.create(result.body as any);

console.log({
  changed: result.changed,
  tokensSaved: result.tokensSaved,
  blocksCompressed: result.blocksCompressed,
});
```

`siftRequest` 默认只修改符合条件的工具输出，system、user 和 assistant prompt 都会受到保护。

压缩单条工具结果或文件时使用 `siftText`：

```ts
import { siftText } from "@agent-context/sift";

const result = siftText(
  fileContents,
  currentUserQuestion,
  "src/services/OrderService.java", // 可选：帮助稳定识别语言
);

console.log(result.text);
console.log(result.tokensSaved);
```

小于 512 字节的输入会原样透传，因此可以把 sift 放在通用请求链路中，无需提前筛选每个文本块。

### 模型最终看到什么？

以下为效果示意：模型不再把成百上千行重复内容带入下一轮，而是保留关键结构，以及返回原文的路径：

```diff
- 2,000 行命令、重复状态和堆栈信息
+ $ cargo test --workspace
+ error[E0382]: borrow of moved value: `request`
+   --> src/client.rs:84:17
+ [... 1,962 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 19]
+ test result: FAILED. 127 passed; 1 failed
+ <<stash:HASH>>
```

错误和测试摘要仍然可见；省略内容可通过 stash 标记或准确的文件行范围取回。

## 恢复原文

有损压缩返回之前，完整输入一定已经写入 stash。压缩结果会带有类似下面的标记：

```text
<<stash:8f1c2e...>>
```

可以恢复全文，也可以只读取需要的行：

```ts
import { retrieve, retrieveLines, siftText } from "@agent-context/sift";

const result = siftText(longToolOutput, currentUserQuestion);

if (result.stashKey) {
  const original = retrieve(result.stashKey);
  const slice = retrieveLines(result.stashKey, 120, 80);
}
```

对于源码、日志、搜索结果、diff 和显式保守模式的整行纯文本，省略提示可以直接指向 stash 文件和准确行范围。默认纯文本按句子抽取，不会伪造无法可靠映射的行范围：

```text
// ... 30 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 32
```

与 sift 共享文件系统的 Agent 可以直接读取该片段；其他场景可通过自己的工具或应用流程暴露 `retrieve` / `retrieveLines`。sift 不会自动向模型注入取回工具。

## 针对内容类型优化

| 输入 | sift 保留或简化的内容 |
| --- | --- |
| JSON 数组 | schema、代表性样本、重要记录和错误记录 |
| 构建与测试日志 | 命令、错误、堆栈和摘要 |
| grep / ripgrep 结果 | 按源码上下文组织的高价值匹配 |
| Unified diff | 代表性 hunk 与改动结构 |
| 源代码 | 保留签名、结构和前 5 行完整 AST 语句，再折叠函数体；支持 Python、JavaScript、TypeScript、Go、Rust、Java、C、C++ |
| 纯文本 | 按 query 相关性、位置与显著信息抽取代表句段，并抑制近重复内容 |
| YAML、TOML、INI 配置 | 全部键值和顺序保持可见，只卸载安全的整行注释与空行 |
| Pretty JSON 与重复日志 | 优先尝试无损 minify 或模板化 |

HTML 会提取文章正文并转换为可读 Markdown，移除脚本、样式、导航、侧栏、广告和页脚；完整原文仍可从 stash 恢复。

## 为安全接入而设计

sift 遵守三条不可破坏的规则：

1. 只在单条消息内部压缩，不会跨对话删除整条消息。
2. 不修改 Anthropic `cache_control` 锚点及其之前的冻结前缀。
3. 任何有损转换都必须先成功保存原文，才能发布压缩结果。

此外，sift 还会保护工具调用与结果的配对、自定义 XML 标签，以及可能包含凭证的高熵字符串。如果压缩没有节省 token，或 stash 写入失败，就会返回原文。

## 适合放在哪里？

建议将 `siftRequest` 放在 LLM 出站请求前的最后一道中间件中。它尤其适合：

- 反复携带构建输出、搜索结果和 diff 的 Coding Agent；
- 工具返回内容很大的长对话助手；
- 同时服务 Anthropic 与 OpenAI 请求格式的网关；
- 模型后续可以按需取回省略内容的本地或服务端工作流。

只有一段原始字符串、没有完整请求体时，使用 `siftText`。

## API 概览

```ts
siftRequest(body, query?)
siftText(text, query?, sourcePath?)
retrieve(key)
retrieveLines(key, startLine, lineCount)
createSift({ stashDir })
detectContentType(text)
detectRequestFormat(body)
```

完整返回类型、请求格式与行为说明见 [Node.js 包文档](npm/core/README.md)。

## 运行注意事项

- 默认 stash 目录是 `~/.sift/stash`；可以设置 `SIFT_STASH_DIR`，或用 `createSift({ stashDir })` 指定其他目录。
- stash 条目 30 分钟后过期，并在读取时惰性清理；请据此设计取回和保留策略。
- 本地 stash 可供同一台机器上的多个进程共享，但不会自动跨集群同步。多机部署需要共享文件系统或共享 `StashStore` 后端。
- `tokensSaved` 是估算值，适合可观测性统计，不适合作为账单核对依据。
- Node.js 包为 macOS、Linux（GNU 与 musl）和 Windows 提供 x64 / arm64 预编译二进制。Linux GNU 构建以 glibc 2.28 为基线。

## 参与贡献

构建说明、架构约束、测试要求和发布流程统一记录在 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可证

[Apache-2.0](LICENSE)
