# grep / ripgrep 搜索结果

按文件和 query 相关性抽稀搜索命中，完整结果可从 CCR 恢复。

- 场景 ID：`search-results`
- 检测类型：`search_results`
- 相关性 query：`compute_thing`

## 压缩前原文

> 内容完整保留；为了便于阅读，JSON 仅在展示时进行了缩进美化。

```text
src/module_00.rs:0:    let value = compute_thing(context, input_0);
src/module_00.rs:1:    let value = compute_thing(context, input_1);
src/module_00.rs:2:    let value = compute_thing(context, input_2);
src/module_00.rs:3:    let value = compute_thing(context, input_3);
src/module_00.rs:4:    let value = compute_thing(context, input_4);
src/module_00.rs:5:    let value = compute_thing(context, input_5);
src/module_00.rs:6:    let value = compute_thing(context, input_6);
src/module_00.rs:7:    let value = compute_thing(context, input_7);
src/module_00.rs:8:    let value = compute_thing(context, input_8);
src/module_00.rs:9:    let value = compute_thing(context, input_9);
src/module_00.rs:10:    let value = compute_thing(context, input_10);
src/module_00.rs:11:    let value = compute_thing(context, input_11);
src/module_01.rs:37:    let value = compute_thing(context, input_0);
src/module_01.rs:38:    let value = compute_thing(context, input_1);
src/module_01.rs:39:    let value = compute_thing(context, input_2);
src/module_01.rs:40:    let value = compute_thing(context, input_3);
src/module_01.rs:41:    let value = compute_thing(context, input_4);
src/module_01.rs:42:    let value = compute_thing(context, input_5);
src/module_01.rs:43:    let value = compute_thing(context, input_6);
src/module_01.rs:44:    let value = compute_thing(context, input_7);
src/module_01.rs:45:    let value = compute_thing(context, input_8);
src/module_01.rs:46:    let value = compute_thing(context, input_9);
src/module_01.rs:47:    let value = compute_thing(context, input_10);
src/module_01.rs:48:    let value = compute_thing(context, input_11);
src/module_02.rs:74:    let value = compute_thing(context, input_0);
src/module_02.rs:75:    let value = compute_thing(context, input_1);
src/module_02.rs:76:    let value = compute_thing(context, input_2);
src/module_02.rs:77:    let value = compute_thing(context, input_3);
src/module_02.rs:78:    let value = compute_thing(context, input_4);
src/module_02.rs:79:    let value = compute_thing(context, input_5);
src/module_02.rs:80:    let value = compute_thing(context, input_6);
src/module_02.rs:81:    let value = compute_thing(context, input_7);
src/module_02.rs:82:    let value = compute_thing(context, input_8);
src/module_02.rs:83:    let value = compute_thing(context, input_9);
src/module_02.rs:84:    let value = compute_thing(context, input_10);
src/module_02.rs:85:    let value = compute_thing(context, input_11);
src/module_03.rs:111:    let value = compute_thing(context, input_0);
src/module_03.rs:112:    let value = compute_thing(context, input_1);
src/module_03.rs:113:    let value = compute_thing(context, input_2);
src/module_03.rs:114:    let value = compute_thing(context, input_3);
src/module_03.rs:115:    let value = compute_thing(context, input_4);
src/module_03.rs:116:    let value = compute_thing(context, input_5);
src/module_03.rs:117:    let value = compute_thing(context, input_6);
src/module_03.rs:118:    let value = compute_thing(context, input_7);
src/module_03.rs:119:    let value = compute_thing(context, input_8);
src/module_03.rs:120:    let value = compute_thing(context, input_9);
src/module_03.rs:121:    let value = compute_thing(context, input_10);
src/module_03.rs:122:    let value = compute_thing(context, input_11);
src/module_04.rs:148:    let value = compute_thing(context, input_0);
src/module_04.rs:149:    let value = compute_thing(context, input_1);
src/module_04.rs:150:    let value = compute_thing(context, input_2);
src/module_04.rs:151:    let value = compute_thing(context, input_3);
src/module_04.rs:152:    let value = compute_thing(context, input_4);
src/module_04.rs:153:    let value = compute_thing(context, input_5);
src/module_04.rs:154:    let value = compute_thing(context, input_6);
src/module_04.rs:155:    let value = compute_thing(context, input_7);
src/module_04.rs:156:    let value = compute_thing(context, input_8);
src/module_04.rs:157:    let value = compute_thing(context, input_9);
src/module_04.rs:158:    let value = compute_thing(context, input_10);
src/module_04.rs:159:    let value = compute_thing(context, input_11);
src/module_05.rs:185:    let value = compute_thing(context, input_0);
src/module_05.rs:186:    let value = compute_thing(context, input_1);
src/module_05.rs:187:    let value = compute_thing(context, input_2);
src/module_05.rs:188:    let value = compute_thing(context, input_3);
src/module_05.rs:189:    let value = compute_thing(context, input_4);
src/module_05.rs:190:    let value = compute_thing(context, input_5);
src/module_05.rs:191:    let value = compute_thing(context, input_6);
src/module_05.rs:192:    let value = compute_thing(context, input_7);
src/module_05.rs:193:    let value = compute_thing(context, input_8);
src/module_05.rs:194:    let value = compute_thing(context, input_9);
src/module_05.rs:195:    let value = compute_thing(context, input_10);
src/module_05.rs:196:    let value = compute_thing(context, input_11);
src/module_06.rs:222:    let value = compute_thing(context, input_0);
src/module_06.rs:223:    let value = compute_thing(context, input_1);
src/module_06.rs:224:    let value = compute_thing(context, input_2);
src/module_06.rs:225:    let value = compute_thing(context, input_3);
src/module_06.rs:226:    let value = compute_thing(context, input_4);
src/module_06.rs:227:    let value = compute_thing(context, input_5);
src/module_06.rs:228:    let value = compute_thing(context, input_6);
src/module_06.rs:229:    let value = compute_thing(context, input_7);
src/module_06.rs:230:    let value = compute_thing(context, input_8);
src/module_06.rs:231:    let value = compute_thing(context, input_9);
src/module_06.rs:232:    let value = compute_thing(context, input_10);
src/module_06.rs:233:    let value = compute_thing(context, input_11);
src/module_07.rs:259:    let value = compute_thing(context, input_0);
src/module_07.rs:260:    let value = compute_thing(context, input_1);
src/module_07.rs:261:    let value = compute_thing(context, input_2);
src/module_07.rs:262:    let value = compute_thing(context, input_3);
src/module_07.rs:263:    let value = compute_thing(context, input_4);
src/module_07.rs:264:    let value = compute_thing(context, input_5);
src/module_07.rs:265:    let value = compute_thing(context, input_6);
src/module_07.rs:266:    let value = compute_thing(context, input_7);
src/module_07.rs:267:    let value = compute_thing(context, input_8);
src/module_07.rs:268:    let value = compute_thing(context, input_9);
src/module_07.rs:269:    let value = compute_thing(context, input_10);
src/module_07.rs:270:    let value = compute_thing(context, input_11);
src/module_08.rs:296:    let value = compute_thing(context, input_0);
src/module_08.rs:297:    let value = compute_thing(context, input_1);
src/module_08.rs:298:    let value = compute_thing(context, input_2);
src/module_08.rs:299:    let value = compute_thing(context, input_3);
src/module_08.rs:300:    let value = compute_thing(context, input_4);
src/module_08.rs:301:    let value = compute_thing(context, input_5);
src/module_08.rs:302:    let value = compute_thing(context, input_6);
src/module_08.rs:303:    let value = compute_thing(context, input_7);
src/module_08.rs:304:    let value = compute_thing(context, input_8);
src/module_08.rs:305:    let value = compute_thing(context, input_9);
src/module_08.rs:306:    let value = compute_thing(context, input_10);
src/module_08.rs:307:    let value = compute_thing(context, input_11);
src/module_09.rs:333:    let value = compute_thing(context, input_0);
src/module_09.rs:334:    let value = compute_thing(context, input_1);
src/module_09.rs:335:    let value = compute_thing(context, input_2);
src/module_09.rs:336:    let value = compute_thing(context, input_3);
src/module_09.rs:337:    let value = compute_thing(context, input_4);
src/module_09.rs:338:    let value = compute_thing(context, input_5);
src/module_09.rs:339:    let value = compute_thing(context, input_6);
src/module_09.rs:340:    let value = compute_thing(context, input_7);
src/module_09.rs:341:    let value = compute_thing(context, input_8);
src/module_09.rs:342:    let value = compute_thing(context, input_9);
src/module_09.rs:343:    let value = compute_thing(context, input_10);
src/module_09.rs:344:    let value = compute_thing(context, input_11);
src/module_10.rs:370:    let value = compute_thing(context, input_0);
src/module_10.rs:371:    let value = compute_thing(context, input_1);
src/module_10.rs:372:    let value = compute_thing(context, input_2);
src/module_10.rs:373:    let value = compute_thing(context, input_3);
src/module_10.rs:374:    let value = compute_thing(context, input_4);
src/module_10.rs:375:    let value = compute_thing(context, input_5);
src/module_10.rs:376:    let value = compute_thing(context, input_6);
src/module_10.rs:377:    let value = compute_thing(context, input_7);
src/module_10.rs:378:    let value = compute_thing(context, input_8);
src/module_10.rs:379:    let value = compute_thing(context, input_9);
src/module_10.rs:380:    let value = compute_thing(context, input_10);
src/module_10.rs:381:    let value = compute_thing(context, input_11);
src/module_11.rs:407:    let value = compute_thing(context, input_0);
src/module_11.rs:408:    let value = compute_thing(context, input_1);
src/module_11.rs:409:    let value = compute_thing(context, input_2);
src/module_11.rs:410:    let value = compute_thing(context, input_3);
src/module_11.rs:411:    let value = compute_thing(context, input_4);
src/module_11.rs:412:    let value = compute_thing(context, input_5);
src/module_11.rs:413:    let value = compute_thing(context, input_6);
src/module_11.rs:414:    let value = compute_thing(context, input_7);
src/module_11.rs:415:    let value = compute_thing(context, input_8);
src/module_11.rs:416:    let value = compute_thing(context, input_9);
src/module_11.rs:417:    let value = compute_thing(context, input_10);
src/module_11.rs:418:    let value = compute_thing(context, input_11);
```

## 压缩后输出

```text
src/module_00.rs:0:    let value = compute_thing(context, input_0);
src/module_00.rs:1:    let value = compute_thing(context, input_1);
src/module_00.rs:2:    let value = compute_thing(context, input_2);
src/module_00.rs:3:    let value = compute_thing(context, input_3);
src/module_00.rs:11:    let value = compute_thing(context, input_11);
[... and 7 more matches in src/module_00.rs]
src/module_01.rs:37:    let value = compute_thing(context, input_0);
src/module_01.rs:38:    let value = compute_thing(context, input_1);
src/module_01.rs:39:    let value = compute_thing(context, input_2);
src/module_01.rs:40:    let value = compute_thing(context, input_3);
src/module_01.rs:48:    let value = compute_thing(context, input_11);
[... and 7 more matches in src/module_01.rs]
src/module_02.rs:74:    let value = compute_thing(context, input_0);
src/module_02.rs:75:    let value = compute_thing(context, input_1);
src/module_02.rs:76:    let value = compute_thing(context, input_2);
src/module_02.rs:77:    let value = compute_thing(context, input_3);
src/module_02.rs:85:    let value = compute_thing(context, input_11);
[... and 7 more matches in src/module_02.rs]
src/module_03.rs:111:    let value = compute_thing(context, input_0);
src/module_03.rs:112:    let value = compute_thing(context, input_1);
src/module_03.rs:113:    let value = compute_thing(context, input_2);
src/module_03.rs:114:    let value = compute_thing(context, input_3);
src/module_03.rs:122:    let value = compute_thing(context, input_11);
[... and 7 more matches in src/module_03.rs]
src/module_04.rs:148:    let value = compute_thing(context, input_0);
src/module_04.rs:149:    let value = compute_thing(context, input_1);
src/module_04.rs:150:    let value = compute_thing(context, input_2);
src/module_04.rs:151:    let value = compute_thing(context, input_3);
src/module_04.rs:159:    let value = compute_thing(context, input_11);
[... and 7 more matches in src/module_04.rs]
src/module_05.rs:185:    let value = compute_thing(context, input_0);
src/module_05.rs:186:    let value = compute_thing(context, input_1);
src/module_05.rs:187:    let value = compute_thing(context, input_2);
src/module_05.rs:188:    let value = compute_thing(context, input_3);
src/module_05.rs:196:    let value = compute_thing(context, input_11);
[... and 7 more matches in src/module_05.rs]
[144 matches compressed to 30. Retrieve more: <<ccr:f401a82db89e7e35295464fa>>]<<ccr:f401a82db89e7e35295464fa>>
```

## 运行结果

| 指标 | 结果 |
|---|---:|
| 原文字节数 | 10057 |
| 压缩后字节数 | 2468 |
| 压缩后占比 | 24.5% |
| 节省 token（估算） | 2287 |
| 检查 block | 1 |
| 压缩 block | 1 |
| 回退 block | 0 |
| 冻结消息 | 1 |
| CCR 写入 | 1 |

- CCR 恢复：PASS（`f401a82db89e7e35295464fa`）
- 场景断言：PASS
