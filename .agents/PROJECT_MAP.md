# 项目地图（PROJECT MAP）

LLM 上下文压缩工具：压缩 LLM 对话上下文，节省 token 与缓存成本。纯 Rust 实现。

## 三大设计不变量

1. **只在消息内压缩**，绝不跨消息丢弃内容；
2. **冻结前缀字节不动**（`cache_control` 标记以下的消息是 prompt cache 锚点）；
3. **有损压缩必须可恢复**：原文进 stash store，输出留 `<<stash:HASH>>` 标记，端到端无损。

## 工程结构

```
crates/
  sift/        # 压缩核心库（本工程主体，纯逻辑 rlib）
    src/
      tokenizer.rs        # Tokenizer trait + EstimatingCounter + registry
      policy.rs           # AuthMode / CompressionPolicy / 缓存成本乘数
      cache_control.rs    # compute_frozen_count：冻结消息下界
      safety.rs           # tool_use/tool_result 配对保护
      content.rs          # ContentType + detect_content_type（含 JSON、HTML、配置与表格）
      stash.rs              # StashStore trait + FileStashStore(落盘) / InMemoryStashStore(测试) + compute_key
      secrets.rs          # 熵检测保密（高熵候选逐次可见性校验，缺失则拒绝有损结果）
      mixed_content.rs    # 混合内容分段路由（split_into_sections）
      recursive_json.rs   # 递归 JSON 路由（块内嵌入 JSON span 平衡匹配 + 替换）
      relevance.rs        # BM25 相关性打分 + rank_by_relevance
      signals.rs          # 行重要性信号（LineImportanceDetector + Tiered）
      transforms/
        mod.rs            # ReformatTransform / OffloadTransform traits + dispatch
        line_omissions.rs # 按原始行坐标渲染共享内联提示，短空隙保留，分段行偏移换算
        log_context.rs    # 日志首行/命令回显/显式续行保护，模板化与有损选择共用
        smart_crusher.rs  # JSON 对象/数组递归压缩（无损 schema 优先，其他结构采样/prose 字段）
        json_compactor.rs # 规则/稀疏对象数组 CSV-schema 紧凑化（异构分桶、统一嵌套字段扁平化）
        log_compressor.rs # 构建/测试日志压缩（错误/堆栈/摘要保留）
        search_compressor.rs # grep/ripgrep 搜索结果抽稀
        diff_compressor.rs   # unified diff hunk 采样
        diff_noise.rs        # diff 噪声卸载（lockfile / whitespace-only）
        html_extractor.rs    # HTML 正文转 Markdown，移除脚本/样式/导航/页脚等页面噪声
        text_crusher.rs   # 默认 BM25/时序/显著性抽取；Rust 可显式 conservative=true 切换完整块去重
        text_blocks.rs    # 保守段落/发言分块，同章节完全相同块保留首份，输出原文行坐标
        code_compressor.rs   # tree-sitter AST 代码压缩（8 语言，函数体保留前 5 行完整语句后折叠）
        config_compressor.rs # YAML/TOML/INI 安全注释/空行卸载（block scalar/多行字符串保护）
        tag_protector.rs  # 自定义 XML 标签保护/恢复（压缩前 protect 后 restore）
        tabular_compressor.rs # CSV/TSV/Markdown 表格严格解析后桥接 SmartCrusher
        reformats.rs      # 无损重排：JsonMinifier + LogTemplate（Drain 模板）
      formats/           # 请求格式适配层：检测 + 三格式候选枚举
        anthropic.rs        # /v1/messages（floor = cache_control 冻结下界）
        chat_completions.rs # OpenAI Chat Completions（string content / text parts / tool 消息）
        responses.rs        # OpenAI Responses API（input_text/output_text / function_call_output）
      text_api.rs        # 裸文本压缩入口（单条字符串，如工具输出原文）
      live_zone.rs        # live zone 定位 + 压缩入口（消息内压缩 + stash 卸载）
  sift-node/        # napi-rs cdylib → Node 原生模块（@agent-context/sift 的桥）
npm/core/                 # npm 包 @agent-context/sift（TypeScript）
  src/index.ts            # 源码：类型定义 + 平台子包/native 加载
  test/smoke.test.ts      # TS 冒烟测试
  tsconfig*.json          # tsc 配置（构建 / 测试两份）
  dist/                   # tsc 产物（index.js + index.d.ts，gitignore）
  native/                 # Rust 构建产物（cp 自 target/release，gitignore）
  platforms/              # 平台子包 @agent-context/sift-<platform>（生成物，gitignore）
.agents/PROJECT_MAP.md      # 本文件
references/headroom/      # 初始版本参考的上游实现（只读，勿改）
tests/fixtures/           # 压缩输入/输出 golden 样本
.cargo/config.toml        # macOS 链接参数：napi cdylib 延迟解析 node 符号
```

## 构建与发布链

```
napi build --platform --release --manifest-path ../../crates/sift-node/Cargo.toml --output-dir native
  → native/sift.<platform-triple>.node   （如 sift.darwin-arm64.node）
  → tsc 编译 src/index.ts → dist/
  → dist/index.js 按 process.platform/arch 加载对应的 .node
```

- npm 包内 `npm run build`（本机 native + tsc）、`npm test`（构建 + 冒烟测试）一条命令完成。
- 平台三元组由 `package.json` 的 `napi.targets` 声明（macOS/Linux/Windows × arm64/x64，Linux 含 gnu/musl）。
- **本机跨平台编译**：`npm run build:cross`（=`scripts/build-cross.sh`）一次产出 macOS/Linux 6 个平台的 `.node`：
  - macOS 目标（arm64/x64）由 clang 原生交叉，无需额外工具；
  - Linux 目标（x64/arm64 × gnu/musl）由 `napi build --cross-compile`（cargo-zigbuild + zig）交叉链接；GNU 目标显式使用 `.2.28` glibc 基线，并在构建后扫描 ELF 符号版本。
  - 依赖：`rustup target add`（各交叉目标 rust-std）+ zig（脚本自动找 `~/zig` 或 PATH）。
- **发布形态**：根包纯 TS（~6kB，只有 dist/）；二进制在 8 个平台子包
  `@agent-context/sift-<platform>`（`npm/core/platforms/`，由
  `scripts/gen-platform-packages.mjs` 生成，build:cross 自动执行）。
  根包 `optionalDependencies` 引用全部平台子包，`npm install` 时按当前平台自动命中，
  装错平台的被 optional 豁免。`dist/index.js` 加载顺序：平台子包 → 本地 `native/`（开发模式）。
- **发布流水线**（`.github/workflows/release.yml`）：推 `v*` tag 触发——
  build 矩阵各平台编 `.node` 上传 artifact → publish job 生成子包、依次 publish
  8 个平台包 + 根包（含 Windows x64/arm64，需 `NPM_TOKEN` secret）。CI（`ci.yml`）：PR/push 跑
  clippy + cargo test + npm 冒烟。

## npm 包对外 API（@agent-context/sift）

> 完整使用说明（安装、快速开始、调用场景、stash 恢复流程）见 [`npm/core/README.md`](../npm/core/README.md)。

- `siftRequest(body, query?) -> { body, changed, blocksExamined, blocksCompressed, blocksReverted, frozenMessages, stashStored, tokensSaved }`
  （自动检测格式：Anthropic /v1/messages、OpenAI Chat Completions、OpenAI Responses API）
- `siftText(text, query?, sourcePath?) -> { text, changed, lossy, stashKey, tokensSaved }`
  （单条字符串压缩；FileStashStore 下源码、搜索、日志及显式保守模式整行纯文本的省略点内联 stash 绝对文件路径、省略行数和
  1-based 起始行；diff 在逐段绝对路径会抵消收益时使用紧凑文件/hunk 汇总；可选 sourcePath 的扩展名用于选择 8 种 grammar）
- `retrieve(key) -> string | null`（按 `<<stash:KEY>>` 取回压缩时卸载的原文）
- `retrieveLines(key, startLine, lineCount) -> { text, startLine, lineCount, totalLines, hasMore } | null`
  （按 stash 原文的 1-based 行号分片读取，单次最多 1000 行）
- `createSift({ stashDir }) -> Sift`（创建绑定到独立 stash 目录的同构 API 实例；顶层 API
  仍按 `SIFT_STASH_DIR` / `~/.sift/stash` 使用全局 store）
- `detectContentType(text) -> 'json_array' | 'build_output' | ... | 'structured_config' | 'tabular'`
- `detectRequestFormat(body) -> 'anthropic' | 'chat_completions' | 'responses' | 'unknown'`

> token 估算不对外暴露：它在 Rust 侧 `tokenizer::EstimatingCounter`（UTF-8 字节 / 4 × 1.2）
> 内部使用，作为 live-zone 压缩后「token 不减则回退」的校验。

## 宿主集成

- [agents-sdk/sift-plugins](https://github.com/agents-sdk/sift-plugins) 提供已发布的 Pi 扩展
  `@agent-context/pi-sift` 与 OpenCode 插件 `@agent-context/opencode-sift`。
- 两个适配器负责在宿主的工具结果 hook 中调用 `siftText`，并注册 `sift_retrieve` 供 Agent
  按需恢复原文；压缩算法仍只位于本仓库的 `crates/sift`。

## 压缩管线（已实现）

```
请求 body（serde_json::Value；Anthropic / OpenAI Chat Completions / OpenAI Responses）
  → formats::detect_request_format          （格式检测 + 策略分发）
  → 各格式策略定位 live zone                （Anthropic：冻结下界至最后一条 user；
                                              OpenAI：floor = 0，覆盖完整数组）
  → 遍历 live zone 内的工具输出候选（Anthropic tool_result、Chat role=tool、
    Responses function_call_output；system/user/assistant prompt 默认保护），每候选：
      0. tag_protector::protect             （自定义 XML 标签 → 占位符）
      1. 无损 reformat（reformat_for：JsonReformatter / LogTemplate）
         ├ JSON 规则对象数组：CSV-schema，异构记录按分类字段分桶；达到 ≥15% 收益即保留全部行
         └ 其他 JSON minify / 日志模板缩到 ≤80% 即短路——不写 stash、无标记
      2. 整块检测 → compressor_for
          JsonArray     → 规则对象数组已由无损 CSV-schema 优先处理；其余进入 smart_crusher
                          （对象/数组递归处理，连续对象规范化，长 prose 字段抽取）
         BuildOutput   → log_compressor
                         （首个非空行、可识别命令及续行强制保留，不受普通行数预算截断或模板化）
          SearchResults → search_compressor
          GitDiff       → diff_noise（lockfile / whitespace-only）→ diff_compressor
          PlainText     → 先试混合内容路由，无收益再 text_crusher
                          （默认按 BM25 relevance、recency、salience 与近重复抑制抽取；
                           配置和固定样本输出与 Headroom TextCrusher 对齐。
                           Rust 可显式 conservative=true 切换为同章节完整块去重）
          SourceCode    → code_compressor（tree-sitter 8 语言）
          Html          → html_extractor（正文转 Markdown；完整 HTML 经 stash 恢复）
          StructuredConfig → config_compressor（键值/顺序保留，安全注释与空行卸载）
          Tabular       → tabular_compressor（严格列解析 → SmartCrusher）
         混合内容路由（整块落 PlainText 时）：
           mixed_content::split_into_sections → 逐段独立分发压缩
           + recursive_json::replace_json_spans（段内嵌入 JSON span 平衡匹配替换）
      3. 落盘模式有损阶段使用原文视图，避免沿用 reformat 后的坐标；源码、搜索、日志、
         保守整行纯文本在省略点内联 stash 绝对文件路径、省略行数和 1-based 起始行；
         默认纯文本按句子抽取，无法映射成可靠行范围，因此不伪造行提示。
         搜索在解析时记录输入行号，按原序回放；短空隙保留，可验证整行混合分段累加行偏移。
         diff 比较逐行提示与原生文件/hunk 汇总的体积，避免重复绝对路径抵消压缩收益；完整原文仍由全局 stash marker 恢复。
         tag_protector::restore 后追加 <<stash:HASH>>。JSON 结构采样、行内片段、标签映射变化、
         内存/远程 store 暂不输出行片段文件提示；不猜测行号或伪造路径
      4. secrets 校验有损输出仍逐次包含全部高熵候选；缺失任一凭据则回退
      5. tokenizer 校验最终文本（含 marker；token ≥ 原值则回退）
      6. 原文确认写入 stash store 后才发布有损结果；写入失败原样回退
         （text_crusher 会提前钉住含凭据段，其余压缩器由统一出口校验兜底）
  → LiveZoneOutcome（changed / blocks / tokens_saved / stash_stored）
```

> 边界说明：本 crate 的输入是已解析的 `serde_json::Value`，压缩就地修改 text 字段
> 后序列化重建，冻结区的 Value 不被触碰。真字节区间手术（保持原始字节与 cache SHA
> 不变）需在更高层（HTTP 代理）实现，见待办。

## 待办

- [x] token 估算用简单方法（Rust `tokenizer::EstimatingCounter` = 字节/4 × 1.2），无需 tiktoken/HF 精确后端
- [ ] HTTP 层的字节区间手术（RawValue 偏移 + cache SHA 不变），供真实代理使用
- [ ] parity golden 测试（可参考 `references/headroom/tests/parity/` 的样本格式）
- [x] napi-rs CLI 集成（`napi build --platform` + 按平台加载）
- [x] 跨平台编译与发布（本机产出 macOS/Linux 6 平台；release matrix 另产出 Windows x64/arm64，共 8 平台包）
- [x] stash store 落盘持久化（`FileStashStore`，目录 `SIFT_STASH_DIR`/`~/.sift/stash`，重启不丢）
- [x] 多平台发布流水线（平台子包 + GitHub Actions 矩阵，`v*` tag 触发 release）
- [ ] 集群共享存储后端（Redis / 对象存储，`StashStore` trait 已抽象好，替换 `FileStashStore` 即可）
