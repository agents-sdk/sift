# 压缩测试 fixtures

存放压缩输入/输出 golden 样本。命名约定：`<内容类型>-<场景>.json`。

- 来源 1：手工/脚本构造的典型场景样本
- 来源 2：真实 Claude Code 会话导出的 tool 输出（注意脱敏）

每个 fixture 是一个 Anthropic /v1/messages 风格的 body JSON，
配同名 `.expected.json` 表示压缩后输出。
