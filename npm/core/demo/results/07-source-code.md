# Rust 源代码

识别 Rust 结构并折叠长函数体，保留签名，完整源码写入 CCR。

- 场景 ID：`source-code`
- 检测类型：`source_code`

## 压缩前原文

> 内容完整保留；为了便于阅读，JSON 仅在展示时进行了缩进美化。

```text
use std::collections::HashMap;
use serde::Serialize;

pub struct Config {
    pub name: String,
    pub retries: u32,
}

pub fn build_index(cfg: &Config, entries: &[String]) -> HashMap<String, usize> {
    let slot_0 = cfg.name.clone() + "_0";
    consume(&slot_0);
    let slot_1 = cfg.name.clone() + "_1";
    consume(&slot_1);
    let slot_2 = cfg.name.clone() + "_2";
    consume(&slot_2);
    let slot_3 = cfg.name.clone() + "_3";
    consume(&slot_3);
    let slot_4 = cfg.name.clone() + "_4";
    consume(&slot_4);
    let slot_5 = cfg.name.clone() + "_5";
    consume(&slot_5);
    let slot_6 = cfg.name.clone() + "_6";
    consume(&slot_6);
    let slot_7 = cfg.name.clone() + "_7";
    consume(&slot_7);
    let slot_8 = cfg.name.clone() + "_8";
    consume(&slot_8);
    let slot_9 = cfg.name.clone() + "_9";
    consume(&slot_9);
    let slot_10 = cfg.name.clone() + "_10";
    consume(&slot_10);
    let slot_11 = cfg.name.clone() + "_11";
    consume(&slot_11);
    let slot_12 = cfg.name.clone() + "_12";
    consume(&slot_12);
    let slot_13 = cfg.name.clone() + "_13";
    consume(&slot_13);
    let slot_14 = cfg.name.clone() + "_14";
    consume(&slot_14);
    let slot_15 = cfg.name.clone() + "_15";
    consume(&slot_15);
    let slot_16 = cfg.name.clone() + "_16";
    consume(&slot_16);
    let slot_17 = cfg.name.clone() + "_17";
    consume(&slot_17);
    let slot_18 = cfg.name.clone() + "_18";
    consume(&slot_18);
    let slot_19 = cfg.name.clone() + "_19";
    consume(&slot_19);
    let slot_20 = cfg.name.clone() + "_20";
    consume(&slot_20);
    let slot_21 = cfg.name.clone() + "_21";
    consume(&slot_21);
    let slot_22 = cfg.name.clone() + "_22";
    consume(&slot_22);
    let slot_23 = cfg.name.clone() + "_23";
    consume(&slot_23);
    let slot_24 = cfg.name.clone() + "_24";
    consume(&slot_24);
    let slot_25 = cfg.name.clone() + "_25";
    consume(&slot_25);
    let slot_26 = cfg.name.clone() + "_26";
    consume(&slot_26);
    let slot_27 = cfg.name.clone() + "_27";
    consume(&slot_27);
    let slot_28 = cfg.name.clone() + "_28";
    consume(&slot_28);
    let slot_29 = cfg.name.clone() + "_29";
    consume(&slot_29);
    let mut map = HashMap::new();
    map.insert(cfg.name.clone(), cfg.retries as usize);
    map
}

```

## 压缩后输出

```text
use std::collections::HashMap;
use serde::Serialize;
pub struct Config {
    pub name: String,
    pub retries: u32,
}
pub fn build_index(cfg: &Config, entries: &[String]) -> HashMap<String, usize> {
    // ... 62 lines omitted
}
<<ccr:adeceedbed9361d8b4079fe0>>
```

## 运行结果

| 指标 | 结果 |
|---|---:|
| 原文字节数 | 2282 |
| 压缩后字节数 | 262 |
| 压缩后占比 | 11.5% |
| 节省 token（估算） | 616 |
| 检查 block | 1 |
| 压缩 block | 1 |
| 回退 block | 0 |
| 冻结消息 | 1 |
| CCR 写入 | 1 |

- CCR 恢复：PASS（`adeceedbed9361d8b4079fe0`）
- 场景断言：PASS
