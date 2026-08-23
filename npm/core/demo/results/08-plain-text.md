# 长纯文本 + 高熵敏感值

按段落和 query 压缩，同时强制保留疑似凭据等高熵文本。

- 场景 ID：`plain-text`
- 检测类型：`plain_text`
- 相关性 query：`rate limits`

## 压缩前原文

> 内容完整保留；为了便于阅读，JSON 仅在展示时进行了缩进美化。

```text
The service exposes a REST API for managing user accounts and preferences. Requests are authenticated via bearer tokens issued by the identity provider. Rate limits apply per tenant and per API key, with separate quotas for read and write operations. Clients should implement exponential backoff on 429.

Deployment credentials: api_key=sk-demo-Xk9mQ2vLpZ7wRtY4uHj6nB8cE5fG3aD1sWq2eRt. Do not commit this value to version control or share it in chat channels.

Historical note 0: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 1: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 2: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 3: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 4: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 5: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 6: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 7: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 8: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 9: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 10: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.

Historical note 11: the legacy stack ran on bare metal with manual deploys. Each release required SSH access and a checklist printed on paper. The team celebrated every Friday deployment that did not page anyone at night.
```

## 压缩后输出

```text
The service exposes a REST API for managing user accounts and preferences.
Requests are authenticated via bearer tokens issued by the identity provider.
Rate limits apply per tenant and per API key, with separate quotas for read and write operations.
Clients should implement exponential backoff on 429.
Deployment credentials: api_key=sk-demo-Xk9mQ2vLpZ7wRtY4uHj6nB8cE5fG3aD1sWq2eRt.
Do not commit this value to version control or share it in chat channels.
Historical note 0: the legacy stack ran on bare metal with manual deploys.
Historical note 1: the legacy stack ran on bare metal with manual deploys.
Historical note 2: the legacy stack ran on bare metal with manual deploys.
Historical note 3: the legacy stack ran on bare metal with manual deploys.
Historical note 4: the legacy stack ran on bare metal with manual deploys.
Historical note 5: the legacy stack ran on bare metal with manual deploys.
Historical note 6: the legacy stack ran on bare metal with manual deploys.
Historical note 7: the legacy stack ran on bare metal with manual deploys.
Historical note 8: the legacy stack ran on bare metal with manual deploys.
Historical note 9: the legacy stack ran on bare metal with manual deploys.
Historical note 10: the legacy stack ran on bare metal with manual deploys.
Historical note 11: the legacy stack ran on bare metal with manual deploys.
Each release required SSH access and a checklist printed on paper.
The team celebrated every Friday deployment that did not page anyone at night.<<ccr:312239449016be449d55492b>>
```

## 运行结果

| 指标 | 结果 |
|---|---:|
| 原文字节数 | 3125 |
| 压缩后字节数 | 1538 |
| 压缩后占比 | 49.2% |
| 节省 token（估算） | 486 |
| 检查 block | 1 |
| 压缩 block | 1 |
| 回退 block | 0 |
| 冻结消息 | 1 |
| CCR 写入 | 1 |

- CCR 恢复：PASS（`312239449016be449d55492b`）
- 场景断言：PASS
