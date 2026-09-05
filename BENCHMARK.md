# sift benchmark

本文档记录 README 宣传数据的测试口径和复现方式。它衡量的是压缩后的请求载荷大小，不是运行速度基准。

## 当前源码快照

- 被测代码：当前工作树（包版本字段为 `@agent-context/sift@0.0.1-alpha.7`）
- 样例：`npm/core/demo/cases` 下全部 11 个固定输入
- 输入：带一个冻结前缀和一个工具结果的 Anthropic Messages 请求体
- stash 路径：固定为 `/tmp/sift-benchmark-stash`，避免内联绝对路径长度影响输出字节数
- 校验：内容类型、冻结前缀、stash 标记与有损原文恢复
- token 数据：sift 内置估算器，不是服务商账单 token

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

全部样例都计入合计，避免只展示有利结果。sift 在结果没有变小的时候回退原文；敏感值样例会保留疑似凭据，同时压缩其余内容，并可从 stash 恢复完整原文。

## 复现已发布版本

先把指定版本安装到独立临时目录，再让 benchmark runner 加载该目录：

```sh
bench_dir=$(mktemp -d /tmp/sift-benchmark.XXXXXX)
npm install --prefix "$bench_dir" --no-audit --no-fund @agent-context/sift@0.0.1-alpha.7

cd npm/core
SIFT_BENCH_PACKAGE="$bench_dir/node_modules/@agent-context/sift" npm run benchmark
```

## 测试当前源码

当前源码必须先重新编译原生模块，避免加载旧的 `.node` 文件：

```sh
cd npm/core
npm run build
npm run benchmark
```

压缩策略或 demo 输入变化后，README 数据不会自动保持不变。更新结果时应注明新的包版本或 commit，并同步所有语言版本。

## 如何解读

- “体积减少”按 UTF-8 字节计算：`1 - output_bytes / input_bytes`。
- `tokensSaved` 使用项目内置的 `UTF-8 bytes / 4 × 1.2` 估算器，仅用于趋势观察。
- `PASS` 表示该场景进行了有损压缩，并验证 `retrieve(key)` 与完整输入逐字相同。
- “无损”表示内容发生无损重排且不需要 stash；“原样返回”表示压缩没有收益或命中了安全保护。
- 实际收益取决于输入内容、模型 tokenizer、请求格式和工具输出的重复程度。
