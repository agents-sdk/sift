# pretty JSON 无损压缩

JsonMinifier 只移除缩进和换行，解析后的 JSON 必须完全等价。

- 场景 ID：`pretty-json`
- 检测类型：`json_array`

## 压缩前原文

> 内容完整保留；为了便于阅读，JSON 仅在展示时进行了缩进美化。

```text
[
  {
    "id": 0,
    "name": "item-0",
    "ok": true
  },
  {
    "id": 1,
    "name": "item-1",
    "ok": true
  },
  {
    "id": 2,
    "name": "item-2",
    "ok": true
  },
  {
    "id": 3,
    "name": "item-3",
    "ok": true
  },
  {
    "id": 4,
    "name": "item-4",
    "ok": true
  },
  {
    "id": 5,
    "name": "item-5",
    "ok": true
  },
  {
    "id": 6,
    "name": "item-6",
    "ok": true
  },
  {
    "id": 7,
    "name": "item-7",
    "ok": true
  },
  {
    "id": 8,
    "name": "item-8",
    "ok": true
  },
  {
    "id": 9,
    "name": "item-9",
    "ok": true
  },
  {
    "id": 10,
    "name": "item-10",
    "ok": true
  },
  {
    "id": 11,
    "name": "item-11",
    "ok": true
  },
  {
    "id": 12,
    "name": "item-12",
    "ok": true
  },
  {
    "id": 13,
    "name": "item-13",
    "ok": true
  },
  {
    "id": 14,
    "name": "item-14",
    "ok": true
  },
  {
    "id": 15,
    "name": "item-15",
    "ok": true
  },
  {
    "id": 16,
    "name": "item-16",
    "ok": true
  },
  {
    "id": 17,
    "name": "item-17",
    "ok": true
  },
  {
    "id": 18,
    "name": "item-18",
    "ok": true
  },
  {
    "id": 19,
    "name": "item-19",
    "ok": true
  },
  {
    "id": 20,
    "name": "item-20",
    "ok": true
  },
  {
    "id": 21,
    "name": "item-21",
    "ok": true
  },
  {
    "id": 22,
    "name": "item-22",
    "ok": true
  },
  {
    "id": 23,
    "name": "item-23",
    "ok": true
  },
  {
    "id": 24,
    "name": "item-24",
    "ok": true
  },
  {
    "id": 25,
    "name": "item-25",
    "ok": true
  },
  {
    "id": 26,
    "name": "item-26",
    "ok": true
  },
  {
    "id": 27,
    "name": "item-27",
    "ok": true
  },
  {
    "id": 28,
    "name": "item-28",
    "ok": true
  },
  {
    "id": 29,
    "name": "item-29",
    "ok": true
  },
  {
    "id": 30,
    "name": "item-30",
    "ok": true
  },
  {
    "id": 31,
    "name": "item-31",
    "ok": true
  },
  {
    "id": 32,
    "name": "item-32",
    "ok": true
  },
  {
    "id": 33,
    "name": "item-33",
    "ok": true
  },
  {
    "id": 34,
    "name": "item-34",
    "ok": true
  },
  {
    "id": 35,
    "name": "item-35",
    "ok": true
  },
  {
    "id": 36,
    "name": "item-36",
    "ok": true
  },
  {
    "id": 37,
    "name": "item-37",
    "ok": true
  },
  {
    "id": 38,
    "name": "item-38",
    "ok": true
  },
  {
    "id": 39,
    "name": "item-39",
    "ok": true
  },
  {
    "id": 40,
    "name": "item-40",
    "ok": true
  },
  {
    "id": 41,
    "name": "item-41",
    "ok": true
  },
  {
    "id": 42,
    "name": "item-42",
    "ok": true
  },
  {
    "id": 43,
    "name": "item-43",
    "ok": true
  },
  {
    "id": 44,
    "name": "item-44",
    "ok": true
  },
  {
    "id": 45,
    "name": "item-45",
    "ok": true
  },
  {
    "id": 46,
    "name": "item-46",
    "ok": true
  },
  {
    "id": 47,
    "name": "item-47",
    "ok": true
  },
  {
    "id": 48,
    "name": "item-48",
    "ok": true
  },
  {
    "id": 49,
    "name": "item-49",
    "ok": true
  },
  {
    "id": 50,
    "name": "item-50",
    "ok": true
  },
  {
    "id": 51,
    "name": "item-51",
    "ok": true
  },
  {
    "id": 52,
    "name": "item-52",
    "ok": true
  },
  {
    "id": 53,
    "name": "item-53",
    "ok": true
  },
  {
    "id": 54,
    "name": "item-54",
    "ok": true
  },
  {
    "id": 55,
    "name": "item-55",
    "ok": true
  },
  {
    "id": 56,
    "name": "item-56",
    "ok": true
  },
  {
    "id": 57,
    "name": "item-57",
    "ok": true
  },
  {
    "id": 58,
    "name": "item-58",
    "ok": true
  },
  {
    "id": 59,
    "name": "item-59",
    "ok": true
  }
]
```

## 压缩后输出

```text
[{"id":0,"name":"item-0","ok":true},{"id":1,"name":"item-1","ok":true},{"id":2,"name":"item-2","ok":true},{"id":3,"name":"item-3","ok":true},{"id":4,"name":"item-4","ok":true},{"id":5,"name":"item-5","ok":true},{"id":6,"name":"item-6","ok":true},{"id":7,"name":"item-7","ok":true},{"id":8,"name":"item-8","ok":true},{"id":9,"name":"item-9","ok":true},{"id":10,"name":"item-10","ok":true},{"id":11,"name":"item-11","ok":true},{"id":12,"name":"item-12","ok":true},{"id":13,"name":"item-13","ok":true},{"id":14,"name":"item-14","ok":true},{"id":15,"name":"item-15","ok":true},{"id":16,"name":"item-16","ok":true},{"id":17,"name":"item-17","ok":true},{"id":18,"name":"item-18","ok":true},{"id":19,"name":"item-19","ok":true},{"id":20,"name":"item-20","ok":true},{"id":21,"name":"item-21","ok":true},{"id":22,"name":"item-22","ok":true},{"id":23,"name":"item-23","ok":true},{"id":24,"name":"item-24","ok":true},{"id":25,"name":"item-25","ok":true},{"id":26,"name":"item-26","ok":true},{"id":27,"name":"item-27","ok":true},{"id":28,"name":"item-28","ok":true},{"id":29,"name":"item-29","ok":true},{"id":30,"name":"item-30","ok":true},{"id":31,"name":"item-31","ok":true},{"id":32,"name":"item-32","ok":true},{"id":33,"name":"item-33","ok":true},{"id":34,"name":"item-34","ok":true},{"id":35,"name":"item-35","ok":true},{"id":36,"name":"item-36","ok":true},{"id":37,"name":"item-37","ok":true},{"id":38,"name":"item-38","ok":true},{"id":39,"name":"item-39","ok":true},{"id":40,"name":"item-40","ok":true},{"id":41,"name":"item-41","ok":true},{"id":42,"name":"item-42","ok":true},{"id":43,"name":"item-43","ok":true},{"id":44,"name":"item-44","ok":true},{"id":45,"name":"item-45","ok":true},{"id":46,"name":"item-46","ok":true},{"id":47,"name":"item-47","ok":true},{"id":48,"name":"item-48","ok":true},{"id":49,"name":"item-49","ok":true},{"id":50,"name":"item-50","ok":true},{"id":51,"name":"item-51","ok":true},{"id":52,"name":"item-52","ok":true},{"id":53,"name":"item-53","ok":true},{"id":54,"name":"item-54","ok":true},{"id":55,"name":"item-55","ok":true},{"id":56,"name":"item-56","ok":true},{"id":57,"name":"item-57","ok":true},{"id":58,"name":"item-58","ok":true},{"id":59,"name":"item-59","ok":true}]
```

## 运行结果

| 指标 | 结果 |
|---|---:|
| 原文字节数 | 3642 |
| 压缩后字节数 | 2201 |
| 压缩后占比 | 60.4% |
| 节省 token（估算） | 432 |
| 检查 block | 1 |
| 压缩 block | 1 |
| 回退 block | 0 |
| 冻结消息 | 1 |
| CCR 写入 | 0 |

- CCR 恢复：不适用
- 场景断言：PASS
