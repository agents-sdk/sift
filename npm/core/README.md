# @agent-context/sift

**Shrink LLM tool output before every request—and recover the original when you need it.**

`@agent-context/sift` is the Node.js package for sift's Rust compression engine. It recognizes logs, search results, JSON, diffs, source code, and repetitive text, then keeps the useful signal while reducing the context sent to your model.

English · [简体中文](README.zh-CN.md) · [Project overview](../../README.md)

### **62.3% smaller across the nine bundled benchmark scenarios**

**75,546 B → 28,447 B** · **~14,129 estimated tokens saved** · **6/6 lossy benchmark cases restored successfully**

See the [methodology and complete results](../../BENCHMARK.md). Status: **Alpha**; API details may change before 1.0.

## Install

```sh
npm install @agent-context/sift
```

Prebuilt x64 and arm64 binaries are available for macOS, Linux (GNU and musl), and Windows.

### Pi and OpenCode

Ready-made adapters are available for [Pi](https://github.com/earendil-works/pi) (`@agent-context/pi-sift`) and [OpenCode](https://github.com/anomalyco/opencode) (`@agent-context/opencode-sift`). They compress tool results automatically and expose `sift_retrieve` to the agent. See [agents-sdk/sift-plugins](https://github.com/agents-sdk/sift-plugins) for setup and configuration.

## Quick start

Place `siftRequest` immediately before your existing LLM API call:

```ts
import OpenAI from "openai";
import { siftRequest } from "@agent-context/sift";

const openai = new OpenAI();
const request = {
  model: "gpt-5.6-sol",
  input: conversationWithLargeToolOutputs,
};

const compressed = siftRequest(request, currentUserQuestion);
const response = await openai.responses.create(compressed.body as any);

console.log(`Estimated tokens saved: ${compressed.tokensSaved}`);
```

The call style above follows the current [OpenAI Responses API TypeScript interface](https://developers.openai.com/api/reference/typescript/resources/beta/subresources/responses/methods/create). The same `siftRequest` function also detects Anthropic Messages and OpenAI Chat Completions request bodies.

For one raw tool result or file, use `siftText`:

```ts
import { siftText } from "@agent-context/sift";

const result = siftText(
  fileContents,
  currentUserQuestion,
  "src/services/OrderService.java", // optional language hint
);

console.log(result.text);
console.log(result.tokensSaved);
```

Inputs smaller than 512 bytes pass through unchanged.

## Why sift instead of truncation?

| Approach | Content-aware | Original recoverable | Anthropic cache prefix protected | Rejects results with no savings |
| --- | :---: | :---: | :---: | :---: |
| Blind truncation | No | No | Not necessarily | No |
| LLM summary | Partial | Usually no | Not necessarily | No |
| **sift** | **Yes** | **Yes** | **Yes** | **Yes** |

sift tries lossless JSON minification and log templating first. Lossy compression is returned only after the full original has been stored. If the result does not save tokens or the stash write fails, the input is returned unchanged.

## Supported request bodies

`siftRequest(body, query?)` detects the request format and modifies only eligible tool output:

| Format | Compression candidates |
| --- | --- |
| Anthropic `/v1/messages` | Non-error `tool_result` content after the frozen cache prefix |
| OpenAI Chat Completions | String content or text parts in `role: "tool"` messages |
| OpenAI Responses API | String output or text parts in `function_call_output` items |

System, user, and assistant prompts are protected by default. Structured model tool calls are not modified. OpenAI formats do not use the Anthropic `cache_control` prefix anchor, so `frozenMessages` is always `0` for those formats.

## Content-aware compression

| Input | What remains visible |
| --- | --- |
| JSON objects and arrays | Compact encoding, nested prose compression, representative samples, important and error records; concatenated or lightly wrapped JSON is recognized |
| Build/test logs | Commands, errors, stack traces, summaries |
| grep/ripgrep output | High-value matches grouped with source context |
| Unified diffs | Representative hunks and change structure; lockfile and whitespace-only churn is summarized |
| Source code | Signatures and structure; supports Python, JavaScript, TypeScript, Go, Rust, Java, C, and C++ |
| Plain text | Exact duplicate blocks within a section; unique facts remain visible |

Pretty JSON and repetitive logs may be reformatted losslessly. HTML currently passes through unchanged.

## Recovery

Lossy output contains a marker such as `<<stash:8f1c2e...>>`. Use its key to retrieve the entire original or a line range:

```ts
import { retrieve, retrieveLines, siftText } from "@agent-context/sift";

const result = siftText(longToolOutput, currentUserQuestion);

if (result.stashKey) {
  const original = retrieve(result.stashKey);
  const slice = retrieveLines(result.stashKey, 120, 80);
}
```

For source code, logs, search output, and line-based text, omission notices can point directly to the local stash:

```text
// ... 30 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 32
```

An agent sharing the filesystem can read that range directly. Diffs use their compact file/hunk summary when per-gap absolute paths would erase the savings; the trailing stash marker still restores the complete diff. Otherwise, expose `retrieve` or `retrieveLines` through your own application. sift does not inject a retrieval tool into the model.

## API

### `siftRequest(body, query?)`

Returns:

| Field | Meaning |
| --- | --- |
| `body` | Compressed request body with the same request format |
| `changed` | Whether any content changed |
| `blocksExamined` | Number of text blocks inspected |
| `blocksCompressed` | Number of blocks compressed |
| `blocksReverted` | Number of attempted results rejected by token validation |
| `frozenMessages` | Number of messages protected by the cache anchor |
| `stashStored` | Number of originals written to the stash |
| `tokensSaved` | Estimated tokens saved |

### `siftText(text, query?, sourcePath?)`

Compresses one string and returns `{ text, changed, lossy, stashKey, tokensSaved }`. `sourcePath` is optional and uses the extension to select a source-code grammar reliably. Lossy results include a stash marker and non-null `stashKey`; lossless results do not.

### `retrieve(key)`

Returns the stashed original as `string | null`. `null` means that the key does not exist or has expired.

### `retrieveLines(key, startLine, lineCount)`

Reads a 1-based range from the original and returns `{ text, startLine, lineCount, totalLines, hasMore } | null`. `lineCount` must be between 1 and 1,000. Original LF or CRLF line endings are preserved.

### `createSift({ stashDir })`

Creates an isolated API instance bound to a specific stash directory:

```ts
import { createSift } from "@agent-context/sift";

const sift = createSift({ stashDir: "/var/lib/my-app/sift-stash" });
const result = sift.siftText(toolOutput);
const original = result.stashKey ? sift.retrieve(result.stashKey) : null;
```

Each instance has independent storage. Top-level functions continue to use `SIFT_STASH_DIR` or the default directory.

### Detection helpers

```ts
detectContentType(text)
// 'json_array' | 'build_output' | 'search_results' | 'git_diff'
// | 'source_code' | 'plain_text' | 'html'

detectRequestFormat(body)
// 'anthropic' | 'chat_completions' | 'responses' | 'unknown'
```

## Storage and production notes

- The default stash directory is `~/.sift/stash`; override it with `SIFT_STASH_DIR` or `createSift({ stashDir })`.
- Entries expire after 1,800 seconds and are deleted lazily when read. Recovery is guaranteed only while the stash entry remains available.
- Processes on one machine can share a local stash. Multi-host deployments need a shared filesystem or a shared `StashStore` backend.
- `tokensSaved` uses a byte-based estimator and is not a provider billing measurement.
- sift modifies parsed request objects, not raw HTTP bytes. Byte-level proxy surgery must be handled by the proxy layer.

## Local development

See the repository's [contribution guide](../../CONTRIBUTING.md) for build, test, cross-compilation, and release instructions.

## License

[Apache-2.0](LICENSE)
