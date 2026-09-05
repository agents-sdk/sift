# sift

**Send less context. Keep the original within reach.**

sift compresses large tool outputs before they are sent to an LLM. It reduces token usage and prompt-cache costs while keeping lossy source content recoverable from a local stash. The compression engine is written in Rust and is available to Node.js as [`@agent-context/sift`](npm/core/README.md).

English · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [Español](README.es.md)

### **73.1% less context · ~16,795 tokens saved · 8/8 lossy cases recovered**

```sh
npm install @agent-context/sift
```

Status: **Alpha** · API details may change before 1.0 · [Operational notes](#operational-notes)

## Use it with your agent

Ready-made adapters compress tool results automatically and let the agent retrieve stashed originals:

- **Pi:** `pi install npm:@agent-context/pi-sift`
- **OpenCode:** `opencode plugin @agent-context/opencode-sift`

[Setup and configuration →](https://github.com/agents-sdk/sift-plugins)

## Why sift?

Build logs, search results, diffs, source files, and JSON can quickly crowd useful context out of an agent conversation. sift keeps the signal and makes the rest recoverable:

- **Content-aware** — keeps errors, stack traces, commands, relevant matches, and structure.
- **Recoverable** — stores the original before returning lossy output, linked by `<<stash:HASH>>`.
- **Cache-safe** — leaves the Anthropic prefix through the last `cache_control` anchor untouched.
- **Easy to adopt** — one Rust-backed Node.js API supports Anthropic Messages and both OpenAI request formats.

### More useful than blind truncation, safer than a one-way summary

| Approach | Content-aware | Original recoverable | Anthropic cache prefix protected | Rejects results with no savings |
| --- | :---: | :---: | :---: | :---: |
| Blind truncation | No | No | Not necessarily | No |
| LLM summary | Partial | Usually no | Not necessarily | No |
| **sift** | **Yes** | **Yes** | **Yes** | **Yes** |

## Benchmark

Ten deterministic [demo inputs](npm/core/demo/cases), measured from the current source tree (package version `0.0.1-alpha.7`):

| Input | Output | Reduction | Est. tokens saved | Lossy recovery |
| ---: | ---: | ---: | ---: | --- |
| 76,582 B | 20,594 B | 73.1% | 16,795 | 8/8 restored |

Credential-like values remain visible while unrelated text can still be compressed; every lossy case restores the exact original. Results vary by input and tokenizer; `tokensSaved` is an estimate. [Full breakdown and methodology →](BENCHMARK.md)

## Quick start

Compress an LLM request immediately before sending it:

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

`siftRequest` changes only eligible tool outputs. System, user, and assistant prompts are protected by default.

To compress a standalone tool result or file:

```ts
import { siftText } from "@agent-context/sift";

const result = siftText(
  fileContents,
  currentUserQuestion,
  "src/services/OrderService.java", // optional: improves language detection
);

console.log(result.text);
console.log(result.tokensSaved);
```

Inputs smaller than 512 bytes are passed through unchanged, so it is safe to place sift on a general request path without pre-filtering every block.

### What the model sees

Illustratively, instead of carrying hundreds or thousands of repetitive lines into the next turn, the model keeps the useful structure and a route back to the source:

```diff
- 2,000 lines of commands, repeated status messages, and stack traces
+ $ cargo test --workspace
+ error[E0382]: borrow of moved value: `request`
+   --> src/client.rs:84:17
+ [... 1,962 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 19]
+ test result: FAILED. 127 passed; 1 failed
+ <<stash:HASH>>
```

The error and summary stay visible. The omitted lines remain available through the stash marker or exact file range.

## Get the original back

Lossy compression never returns a compressed result until the full input has been written to the stash. The output includes a marker such as:

```text
<<stash:8f1c2e...>>
```

Retrieve the entire original or only the lines you need:

```ts
import { retrieve, retrieveLines, siftText } from "@agent-context/sift";

const result = siftText(longToolOutput, currentUserQuestion);

if (result.stashKey) {
  const original = retrieve(result.stashKey);
  const slice = retrieveLines(result.stashKey, 120, 80);
}
```

For source code, logs, search results, and line-based plain text, omission notices can point directly to the stash file and exact line range:

```text
// ... 30 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 32
```

An agent that shares the filesystem can read that range directly. Diffs use a compact file/hunk summary when repeating an absolute path for every short context gap would erase the savings; the trailing stash marker still restores the complete diff. Otherwise, expose `retrieve` or `retrieveLines` through your own tool or application flow. sift does not inject a retrieval tool into the model automatically.

## Content-aware compression

| Input | What sift keeps or simplifies |
| --- | --- |
| JSON objects and arrays | Compact encoding, nested prose compression, representative samples, and important/error records; concatenated or lightly wrapped JSON is recognized |
| Build and test logs | Commands, errors, stack traces, and summaries |
| grep / ripgrep results | The most useful matches, grouped with source context |
| Unified diffs | Representative hunks and change structure; lockfile and whitespace-only churn is summarized |
| Source code | Signatures, structure, and the first five lines of complete AST statements before folding function bodies; supports Python, JavaScript, TypeScript, Go, Rust, Java, C, and C++ |
| Plain text | Query-aware extractive selection using relevance, recency, salience, and near-duplicate suppression |
| HTML | Main article content rendered as readable Markdown; scripts, styles, navigation, sidebars, ads, and footers are removed |
| Pretty JSON and repetitive logs | Lossless minification or templating when that is sufficient |

## Designed for safe adoption

sift follows three non-negotiable rules:

1. It compresses inside individual messages; it never drops whole messages across the conversation.
2. It does not modify the frozen prefix through the last Anthropic `cache_control` anchor.
3. Every lossy transformation stores the original before publishing the compressed output.

Additional safeguards protect tool-call/result pairs, custom XML tags, and high-entropy strings that may contain credentials. If compression does not save tokens, or if the stash write fails, sift returns the original content.

## Where it fits

Call `siftRequest` as the last middleware step before an outbound LLM request. It is especially useful for:

- coding agents that repeatedly carry build output, searches, and diffs;
- long-running assistants with large tool responses;
- gateways serving both Anthropic and OpenAI request formats;
- local or server-side workflows where the model can request omitted source details later.

Use `siftText` when you have one raw string rather than a complete request body.

## API at a glance

```ts
siftRequest(body, query?)
siftText(text, query?, sourcePath?)
retrieve(key)
retrieveLines(key, startLine, lineCount)
createSift({ stashDir })
detectContentType(text)
detectRequestFormat(body)
```

See the [Node.js package documentation](npm/core/README.md) for return types, request-format details, and complete behavior.

## Operational notes

- The default file stash is `~/.sift/stash`; set `SIFT_STASH_DIR` or use `createSift({ stashDir })` to choose another directory.
- Stash entries expire after 30 minutes and are removed lazily when read. Plan retrieval and retention accordingly.
- A local stash is shared by processes on one machine, not automatically across a cluster. Use a shared filesystem or implement a shared `StashStore` backend for multi-host deployments.
- `tokensSaved` is an estimate, intended for observability rather than billing reconciliation.
- The Node.js package provides prebuilt x64 and arm64 binaries for macOS, Linux (GNU and musl), and Windows. Linux GNU builds target a glibc 2.28 baseline.

## Contributing

Build instructions, architecture rules, test requirements, and the release workflow are documented in [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[Apache-2.0](LICENSE)
