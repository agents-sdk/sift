# Cargo 构建日志

折叠重复构建噪声，同时保留 warning、error 和源码位置。

- 场景 ID：`build-log`
- 检测类型：`build_output`

## 压缩前原文

> 内容完整保留；为了便于阅读，JSON 仅在展示时进行了缩进美化。

```text
   Compiling serde v1.0.200
   Compiling dep-000 v0.0.0
   Compiling dep-001 v0.1.0
   Compiling dep-002 v0.2.0
   Compiling dep-003 v0.3.0
   Compiling dep-004 v0.4.0
   Compiling dep-005 v0.5.0
   Compiling dep-006 v0.6.0
   Compiling dep-007 v0.7.0
   Compiling dep-008 v0.8.0
   Compiling dep-009 v0.9.0
   Compiling dep-010 v0.10.0
   Compiling dep-011 v0.11.0
   Compiling dep-012 v0.12.0
   Compiling dep-013 v0.13.0
   Compiling dep-014 v0.14.0
   Compiling dep-015 v0.15.0
   Compiling dep-016 v0.16.0
   Compiling dep-017 v0.17.0
   Compiling dep-018 v0.18.0
   Compiling dep-019 v0.19.0
   Compiling dep-020 v0.20.0
   Compiling dep-021 v0.21.0
   Compiling dep-022 v0.22.0
   Compiling dep-023 v0.23.0
   Compiling dep-024 v0.24.0
   Compiling dep-025 v0.25.0
   Compiling dep-026 v0.26.0
   Compiling dep-027 v0.27.0
   Compiling dep-028 v0.28.0
   Compiling dep-029 v0.29.0
   Compiling dep-030 v0.30.0
   Compiling dep-031 v0.31.0
   Compiling dep-032 v0.32.0
   Compiling dep-033 v0.33.0
   Compiling dep-034 v0.34.0
   Compiling dep-035 v0.35.0
   Compiling dep-036 v0.36.0
   Compiling dep-037 v0.37.0
   Compiling dep-038 v0.38.0
   Compiling dep-039 v0.39.0
warning: unused variable: `x`
note: `#[warn(unused_variables)]` on by default (line 0)
note: `#[warn(unused_variables)]` on by default (line 1)
note: `#[warn(unused_variables)]` on by default (line 2)
note: `#[warn(unused_variables)]` on by default (line 3)
note: `#[warn(unused_variables)]` on by default (line 4)
note: `#[warn(unused_variables)]` on by default (line 5)
note: `#[warn(unused_variables)]` on by default (line 6)
note: `#[warn(unused_variables)]` on by default (line 7)
note: `#[warn(unused_variables)]` on by default (line 8)
note: `#[warn(unused_variables)]` on by default (line 9)
note: `#[warn(unused_variables)]` on by default (line 10)
note: `#[warn(unused_variables)]` on by default (line 11)
note: `#[warn(unused_variables)]` on by default (line 12)
note: `#[warn(unused_variables)]` on by default (line 13)
note: `#[warn(unused_variables)]` on by default (line 14)
note: `#[warn(unused_variables)]` on by default (line 15)
note: `#[warn(unused_variables)]` on by default (line 16)
note: `#[warn(unused_variables)]` on by default (line 17)
note: `#[warn(unused_variables)]` on by default (line 18)
note: `#[warn(unused_variables)]` on by default (line 19)
note: `#[warn(unused_variables)]` on by default (line 20)
note: `#[warn(unused_variables)]` on by default (line 21)
note: `#[warn(unused_variables)]` on by default (line 22)
note: `#[warn(unused_variables)]` on by default (line 23)
note: `#[warn(unused_variables)]` on by default (line 24)
note: `#[warn(unused_variables)]` on by default (line 25)
note: `#[warn(unused_variables)]` on by default (line 26)
note: `#[warn(unused_variables)]` on by default (line 27)
note: `#[warn(unused_variables)]` on by default (line 28)
note: `#[warn(unused_variables)]` on by default (line 29)
error[E0308]: mismatched types
  --> src/main.rs:42:13
   | let x: String = 42;
error: could not compile `demo` due to 1 previous error
```

## 压缩后输出

```text
   Compiling serde v1.0.200
   Compiling dep-000 v0.0.0
   Compiling dep-001 v0.1.0
   Compiling dep-002 v0.2.0
   Compiling dep-003 v0.3.0
   Compiling dep-004 v0.4.0
   Compiling dep-005 v0.5.0
   Compiling dep-006 v0.6.0
   Compiling dep-007 v0.7.0
   Compiling dep-008 v0.8.0
   Compiling dep-009 v0.9.0
   Compiling dep-010 v0.10.0
   Compiling dep-011 v0.11.0
   Compiling dep-012 v0.12.0
   Compiling dep-013 v0.13.0
   Compiling dep-014 v0.14.0
   Compiling dep-015 v0.15.0
   Compiling dep-016 v0.16.0
   Compiling dep-017 v0.17.0
   Compiling dep-018 v0.18.0
   Compiling dep-019 v0.19.0
   Compiling dep-020 v0.20.0
   Compiling dep-021 v0.21.0
   Compiling dep-022 v0.22.0
   Compiling dep-023 v0.23.0
   Compiling dep-024 v0.24.0
   Compiling dep-025 v0.25.0
   Compiling dep-026 v0.26.0
   Compiling dep-027 v0.27.0
   Compiling dep-028 v0.28.0
   Compiling dep-029 v0.29.0
   Compiling dep-030 v0.30.0
   Compiling dep-031 v0.31.0
   Compiling dep-032 v0.32.0
   Compiling dep-033 v0.33.0
   Compiling dep-034 v0.34.0
   Compiling dep-035 v0.35.0
   Compiling dep-036 v0.36.0
   Compiling dep-037 v0.37.0
   Compiling dep-038 v0.38.0
   Compiling dep-039 v0.39.0
warning: unused variable: `x`
[Template T1: note: `#[warn(unused_variables)]` on by default (line <*>] (30 occurrences)
0)
1)
2)
3)
4)
5)
6)
7)
8)
9)
10)
11)
12)
13)
14)
15)
16)
17)
18)
19)
20)
21)
22)
23)
24)
25)
26)
27)
28)
29)
error[E0308]: mismatched types
  --> src/main.rs:42:13
   | let x: String = 42;
error: could not compile `demo` due to 1 previous error
```

## 运行结果

| 指标 | 结果 |
|---|---:|
| 原文字节数 | 3073 |
| 压缩后字节数 | 1543 |
| 压缩后占比 | 50.2% |
| 节省 token（估算） | 459 |
| 检查 block | 1 |
| 压缩 block | 1 |
| 回退 block | 0 |
| 冻结消息 | 1 |
| CCR 写入 | 0 |

- CCR 恢复：不适用
- 场景断言：PASS
