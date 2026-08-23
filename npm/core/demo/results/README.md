# @compressor/core demo 运行结果

以下文件由 `npm run demo -- --save` 通过 npm 包公开入口实际运行生成。

| 示例 | 类型 | 原文字节 | 压缩后字节 | 压缩后占比 | 节省 token | CCR |
|---|---|---:|---:|---:|---:|---|
| [JSON 数组工具输出](./01-json-array.md) | json_array | 18397 | 2973 | 16.2% | 4637 | PASS |
| [pretty JSON 无损压缩](./02-pretty-json.md) | json_array | 3642 | 2201 | 60.4% | 432 | — |
| [Cargo 构建日志](./03-build-log.md) | build_output | 3073 | 1543 | 50.2% | 459 | — |
| [grep / ripgrep 搜索结果](./04-search-results.md) | search_results | 10057 | 2468 | 24.5% | 2287 | PASS |
| [多文件 git diff](./05-git-diff.md) | git_diff | 8201 | 6910 | 84.3% | 397 | PASS |
| [命令回显 + JSON + 尾注混合输出](./06-mixed-output.md) | plain_text | 9240 | 1599 | 17.3% | 2301 | PASS |
| [Rust 源代码](./07-source-code.md) | source_code | 2282 | 262 | 11.5% | 616 | PASS |
| [长纯文本 + 高熵敏感值](./08-plain-text.md) | plain_text | 3125 | 1538 | 49.2% | 486 | PASS |
