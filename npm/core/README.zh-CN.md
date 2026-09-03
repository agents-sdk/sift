# @agent-context/sift

**在每次 LLM 请求前缩小工具输出，需要时仍能找回完整原文。**

`@agent-context/sift` 是 sift Rust 压缩引擎的 Node.js 包。它识别日志、搜索结果、JSON、diff、源码和重复文本，在保留关键信息的同时减少发送给模型的上下文。

[English](README.md) · 简体中文 · [项目主页](../../README.zh-CN.md)

### **8 个内置基准场景整体缩小 60.6%**

**58,017 B → 22,859 B** · **估算节省约 10,548 tokens** · **4/4 个有损基准样例成功恢复**

测试口径与完整结果见 [BENCHMARK.md](../../BENCHMARK.md)。当前状态：**Alpha**，1.0 前 API 细节可能变化。

- 纯 Rust 核心（`sift`）+ napi-rs 桥
- 支持三种请求格式（自动检测）：Anthropic `/v1/messages`、OpenAI Chat
  Completions、OpenAI Responses API
- 另有 `siftText`：对单条字符串（如工具输出原文）直接压缩
- 消息内压缩 + 冻结前缀保护 + stash 可恢复，三大不变量见 [PROJECT_MAP](../../.agents/PROJECT_MAP.md)

## 安装

```sh
npm install @agent-context/sift
```

发布包支持 macOS、Linux 与 Windows 的 x64/arm64；Linux 同时提供 GNU 与 musl 变体。

## 运行场景演示

仓库内提供了覆盖 JSON 数组、pretty JSON、构建日志、搜索结果、git diff、混合命令
输出、源代码和纯文本的 8 个独立用例。它通过包根目录加载 `@agent-context/sift` 的公开入口，
每个用例都会完整打印「压缩前原文」「压缩后输出」「运行指标」，并验证 stash 恢复和
冻结前缀保护：

```sh
cd npm/core
npm run build:native  # 首次运行或 Rust 代码变更后执行
npm run demo -- --list       # 查看用例名称
npm run demo -- json-array   # 单独运行一个用例
npm run demo                 # 按顺序运行全部用例
npm run demo -- --save       # 运行全部并分别保存到 demo/results/
```

演示会打印每个场景压缩前后的完整内容、字节数、压缩比、节省 token 和验证结果；stash
数据写入系统临时目录，不会使用正式的 `~/.sift/stash`。

## 快速开始

```ts
import { siftRequest, siftText, retrieve, retrieveLines } from '@agent-context/sift';

// 1. 压缩（发送前）
const result = siftRequest(requestBody, userQuery);

// 2. 把压缩后的 body 发给 LLM API
const response = await client.messages.create({
  ...requestBody,
  messages: (result.body as any).messages,
});

// 3. 如需恢复原文：从压缩文本里提取 <<stash:KEY>>，取回
const key = extractStashKeys(result.body)[0];
const original = retrieve(key); // string | null
const fragment = retrieveLines(key, 120, 80); // StashSlice | null

// 源码无需路径也能生成可直接读取的 stash 文件分片
const code = siftText(fileContent, userQuery);
// // ... 30 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 32

// 可选真实路径用于通过扩展名稳定选择 grammar
const fileCode = siftText(fileContent, userQuery, 'src/services/OrderService.java');
```

## API

### `createSift({ stashDir })`

创建绑定到独立 stash 目录的 API 实例。该实例的 `siftRequest`、`siftText`、`retrieve` 和 `retrieveLines`
始终使用传入的目录，不受 `SIFT_STASH_DIR` 影响；相对路径按调用 `createSift` 时的工作目录
解析。

```ts
import { createSift } from '@agent-context/sift';

const sift = createSift({ stashDir: '/var/lib/my-app/sift-stash' });
const result = sift.siftText(toolOutput);
const original = result.stashKey ? sift.retrieve(result.stashKey) : null;
const fragment = result.stashKey ? sift.retrieveLines(result.stashKey, 120, 80) : null;
```

每次调用 `createSift` 都会创建独立实例，因此同一 Node.js 进程可以同时使用多个 stash
目录。直接导入的顶层 `siftRequest`、`siftText`、`retrieve` 和 `retrieveLines` 保持原有行为，继续使用
`SIFT_STASH_DIR`、`~/.sift/stash`、系统临时目录这一默认优先级。

### `siftRequest(body, query?)`

就地压缩 body 中冻结前缀之外的**工具输出**。system/user/assistant prompt 默认保护；
若调用方明确要压缩一段普通文本，应使用 `siftText`。
格式自动检测（[`detectRequestFormat`](#detectrequestformatbody)）：

| 格式 | 压缩候选 |
|---|---|
| Anthropic `/v1/messages` | 非冻结区内、且 `is_error != true` 的 tool_result `content` |
| OpenAI Chat Completions | `role:"tool"` 消息的字符串 content 或 text parts |
| OpenAI Responses API | `function_call_output` 的字符串 output 或 text parts |

模型发出的结构化调用（`tool_calls`、`function_call`）不会被动。OpenAI 格式没有
`cache_control` 前缀锚点，`frozenMessages` 恒为 0（live zone 从头开始）。

- `body`：请求体对象（上述三种格式之一）
- `query?`：当前用户 query，供相关性锚点压缩器优先保留相关行（可空）

返回 `CompressResult`：

| 字段 | 含义 |
|---|---|
| `body` | 压缩后的 messages body（压缩文本尾部带 `<<stash:KEY>>` 标记） |
| `changed` | 是否发生实际压缩 |
| `blocksExamined` / `blocksCompressed` / `blocksReverted` | 检查 / 压缩 / 回退的 block 数 |
| `frozenMessages` | 冻结前缀条数（cache 锚点，未触碰） |
| `stashStored` | 写入 stash store 的原文条数 |
| `tokensSaved` | 估算节省的 token 数 |

### `retrieve(key)`

按 `<<stash:KEY>>` 里的 key 取回压缩时卸载的原文，返回 `string | null`。
`null` 表示 key 不存在或已过期（见「限制」）。

### `retrieveLines(key, startLine, lineCount)`

按 stash 原文的行号读取连续分片，返回
`{ text, startLine, lineCount, totalLines, hasMore } | null`。`startLine` 从 1 开始，
`lineCount` 必须在 1–1000 之间；返回的 `text` 保留命中范围内的原始 LF / CRLF。
越过原文末尾时返回实际可用行数，起始行越界、key 不存在或已过期时返回 `null`。

使用落盘 stash 时，源码、搜索结果、日志、Git Diff 和整行纯文本的实际省略点会就地输出
`[... 30 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 32]`，代码使用注释形式：
`// ... 30 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 32`。
路径是规范化后的绝对路径，`starting at line` 是 1-based 起始行，前面的数字是连续省略行数；
Coding Agent 可以直接把它们换成 `read_file` 等工具的起始行和读取数量。提示不要求理解 stash
hash 或调用 Sift API。搜索结果的源码行号和 diff 的 hunk 行号不是这里的 stash 行号。
落盘模式有损阶段直接使用原文视图，避免沿用无损重排后的坐标；可验证的混合内容整行分段
会累加其在完整 stash 中的行偏移。过短空隙若不值得添加提示则原样保留。
日志的首个非空行、可识别的命令回显（如 `$ cargo build`、shell tracing、npm script、PowerShell 提示符）
及显式续行强制原样可见，不参与去重、模板化或普通日志行数预算竞争；不会仅因可从 stash 恢复就省略命令。
JSON 结构采样、行内片段和 tag protect 改变映射的情况暂不标注；内存或远程 stash 后端也不伪造本地路径。

### `siftText(text, query?, sourcePath?)`

压缩单个字符串（不包请求体），适合在把工具输出原文送进任意 API / 存储之前处理。
行片段提示不依赖 `sourcePath`：有损管线从 `FileStashStore` 取得完整原文的绝对文件路径，并在
省略点写明省略行数和 1-based 起始行，不把同一行内的句子计作多行。
纯文本默认按完整段落/发言块保守去重，只折叠同章节完全相同的块，保留第一份及全部独有内容。
不同编号、数字、状态或发言人的内容不因句式相似而合并；标题、代码围栏、可识别的命令和结论块保留。
query 和目标比例不会让纯文本删除独有事实；没有明确重复，或省略提示抵消收益时，原样返回。
折叠会省略重复次数和位置，因此仍属于有损压缩，完整原文保存在 stash 中。
可选的 `sourcePath` 仅用于通过扩展名直接选择对应的
tree-sitter grammar，支持 Python、JavaScript、TypeScript、Go、Rust、Java、C、C++；因此
只有一个长函数、特征行占比很低的文件也能稳定进入源码压缩。
返回 `{ text, changed, lossy, stashKey, tokensSaved }`：有损时 `text` 尾部带
`<<stash:KEY>>` 标记、`stashKey` 非空，可用 `retrieve(stashKey)` 取回原文；无损压缩
（如 JSON minify）时 `lossy` 为 `false` 且无标记。小于 512 字节的输入直接透传。

### `detectRequestFormat(body)`

返回 `'anthropic' | 'chat_completions' | 'responses' | 'unknown'`。
无法判别时（如全部 content 为纯字符串）默认按 Anthropic 处理；这类请求没有明确工具
输出候选，因此 `siftRequest` 会安全透传。需要压缩单条字符串时使用 `siftText`。

### `detectContentType(text)`

返回内容类型：`json_array | build_output | search_results | git_diff | source_code | plain_text | html`，便于诊断。

## 什么时候调用 `siftRequest`

一句话：**在任何 LLM 客户端把请求发给 API 之前，对 messages 做一次拦截压缩**。

收益最大的场景，是消息里含以下**大型工具输出**（这几类有专门压缩器）：

| 内容 | 示例 | 压缩器 |
|---|---|---|
| JSON 数组 | `ls`/API 返回的对象列表、数据库查询结果 | smart_crusher（schema 去重、采样、错误行保留） |
| 构建/测试日志 | `pytest`/`npm`/`cargo`/`jest` 输出 | log_compressor（错误/堆栈/摘要保留） |
| grep/ripgrep 结果 | 代码搜索结果 | search_compressor（按文件/分数抽稀） |
| git diff | `git diff` / PR diff | diff_compressor（hunk 采样） |

适合的接入位置：

- **代理 / 中间件**：放在 LLM 客户端与 API 之间对出站请求统一压缩。
- **应用层发送前**：直接 SDK 调用前，把 `messages` 传给 `siftRequest`。
- **边缘函数 / 无服务器**：发送前压缩，降低首字节与计费 token。

**不需要调用**的情况：

- 消息都很小：单个文本块 < 512 字节会自动跳过（`MIN_BLOCK_BYTES`）。
- HTML：当前没有专用压缩器；纯文本和源代码已有各自压缩器，但 `siftRequest` 仍只会
  对工具输出候选启用它们，普通文本请显式调用 `siftText`。
- stash 目录不可创建或原文无法落盘：有损结果会回退原文，不会留下不可恢复的压缩内容。

## 有损压缩与恢复（stash）

压缩是有损的（丢弃了部分行/样本），但原文会被卸载进 stash store，压缩文本尾部追加
`<<stash:KEY>>` 标记，其中 `KEY` 是原文的 BLAKE3 哈希（24 hex）。

```
原文 ──compress──▶ 压缩文本 + <<stash:KEY>>        （发给 API，省 token）
                        │
                        └── store.put(KEY, 原文)   （默认落盘）

需要完整原文时：从消息里找 <<stash:KEY>> → retrieve(KEY) → 原文
只需局部时：根据省略提示中的 stash 文件路径、起始行和行数 → 直接分片读文件
```

典型恢复流程：模型看到压缩内容后说「我需要看完整数据」，应用从最近的压缩消息里
提取 `<<stash:KEY>>`，用 `retrieve` 取回全文或用 `retrieveLines` 取回分片，再补发给模型。
若 Agent 与 Sift 共享文件系统，则可直接读取提示里的 stash 文件，不必接入 Sift API。
Sift 当前不会自动向模型注入 retrieve tool；需要从 stash 自主取回时，调用方必须显式提供相应工具。

提取 key 的辅助：

```ts
const STASH_RE = /<<stash:([0-9a-f]+)>>/g;
function extractStashKeys(body: any): string[] {
  const text = JSON.stringify(body);
  return [...text.matchAll(STASH_RE)].map((m) => m[1]);
}
```

## 与 prompt cache 的关系（cache 安全）

`siftRequest` 只改冻结前缀之后的工具输出候选。
`cache_control` 标记以下的**冻结前缀字节不动**，因此不会破坏 prompt cache 的命中。
`result.frozenMessages` 报告了被保护的冻结条数。

## 限制

- **stash store 是落盘文件**（`FileStashStore`）：每个 key 一个文件，默认目录
  `~/.sift/stash`（环境变量 `SIFT_STASH_DIR` 可覆盖），TTL 1800 秒（按文件
  mtime 判定，`get` 时惰性删除过期项）；也可以通过 `createSift({ stashDir })` 为实例指定
  独立目录。单机重启不丢、同机多进程互见。
  **多实例 / 集群**需把该目录挂到共享文件系统（NFS / 对象存储），或改用外部
  store 后端（Redis 等，`StashStore` trait 已抽象好），否则不同机器取不到对方的原文。
- 压缩的是**已解析的 JSON 对象**，不是原始 HTTP 字节。若在代理层做「字节级 cache SHA
  不变」的区间手术，需在更高层处理（见 PROJECT_MAP 待办）。
- 当前 `tokensSaved` 用字节/4 × 1.2 粗估，仅作参考。
