# 命令回显 + JSON + 尾注混合输出

分段识别嵌入的大 JSON，同时保留 JSON 外部的命令文本。

- 场景 ID：`mixed-output`
- 检测类型：`plain_text`
- 相关性 query：`find unhealthy pods`

## 压缩前原文

> 内容完整保留；为了便于阅读，JSON 仅在展示时进行了缩进美化。

```text
$ kubectl get pods -o json
[
  {
    "idx": 0,
    "name": "pod-0",
    "phase": "Running"
  },
  {
    "idx": 1,
    "name": "pod-1",
    "phase": "Running"
  },
  {
    "idx": 2,
    "name": "pod-2",
    "phase": "Running"
  },
  {
    "idx": 3,
    "name": "pod-3",
    "phase": "Running"
  },
  {
    "idx": 4,
    "name": "pod-4",
    "phase": "Running"
  },
  {
    "idx": 5,
    "name": "pod-5",
    "phase": "Running"
  },
  {
    "idx": 6,
    "name": "pod-6",
    "phase": "Running"
  },
  {
    "idx": 7,
    "name": "pod-7",
    "phase": "Running"
  },
  {
    "idx": 8,
    "name": "pod-8",
    "phase": "Running"
  },
  {
    "idx": 9,
    "name": "pod-9",
    "phase": "Running"
  },
  {
    "idx": 10,
    "name": "pod-10",
    "phase": "Running"
  },
  {
    "idx": 11,
    "name": "pod-11",
    "phase": "Running"
  },
  {
    "idx": 12,
    "name": "pod-12",
    "phase": "Running"
  },
  {
    "idx": 13,
    "name": "pod-13",
    "phase": "Running"
  },
  {
    "idx": 14,
    "name": "pod-14",
    "phase": "Running"
  },
  {
    "idx": 15,
    "name": "pod-15",
    "phase": "Running"
  },
  {
    "idx": 16,
    "name": "pod-16",
    "phase": "Running"
  },
  {
    "idx": 17,
    "name": "pod-17",
    "phase": "Running"
  },
  {
    "idx": 18,
    "name": "pod-18",
    "phase": "Running"
  },
  {
    "idx": 19,
    "name": "pod-19",
    "phase": "Running"
  },
  {
    "idx": 20,
    "name": "pod-20",
    "phase": "Running"
  },
  {
    "idx": 21,
    "name": "pod-21",
    "phase": "Running"
  },
  {
    "idx": 22,
    "name": "pod-22",
    "phase": "Running"
  },
  {
    "idx": 23,
    "name": "pod-23",
    "phase": "Running"
  },
  {
    "idx": 24,
    "name": "pod-24",
    "phase": "Running"
  },
  {
    "idx": 25,
    "name": "pod-25",
    "phase": "Running"
  },
  {
    "idx": 26,
    "name": "pod-26",
    "phase": "Running"
  },
  {
    "idx": 27,
    "name": "pod-27",
    "phase": "Running"
  },
  {
    "idx": 28,
    "name": "pod-28",
    "phase": "Running"
  },
  {
    "idx": 29,
    "name": "pod-29",
    "phase": "Running"
  },
  {
    "idx": 30,
    "name": "pod-30",
    "phase": "Running"
  },
  {
    "idx": 31,
    "name": "pod-31",
    "phase": "Running"
  },
  {
    "idx": 32,
    "name": "pod-32",
    "phase": "Running"
  },
  {
    "idx": 33,
    "name": "pod-33",
    "phase": "Running"
  },
  {
    "idx": 34,
    "name": "pod-34",
    "phase": "Running"
  },
  {
    "idx": 35,
    "name": "pod-35",
    "phase": "Running"
  },
  {
    "idx": 36,
    "name": "pod-36",
    "phase": "Running"
  },
  {
    "idx": 37,
    "name": "pod-37",
    "phase": "Running"
  },
  {
    "idx": 38,
    "name": "pod-38",
    "phase": "Running"
  },
  {
    "idx": 39,
    "name": "pod-39",
    "phase": "Running"
  },
  {
    "idx": 40,
    "name": "pod-40",
    "phase": "Running"
  },
  {
    "idx": 41,
    "name": "pod-41",
    "phase": "Running"
  },
  {
    "idx": 42,
    "name": "pod-42",
    "phase": "Running"
  },
  {
    "idx": 43,
    "name": "pod-43",
    "phase": "Running"
  },
  {
    "idx": 44,
    "name": "pod-44",
    "phase": "Running"
  },
  {
    "idx": 45,
    "name": "pod-45",
    "phase": "Running"
  },
  {
    "idx": 46,
    "name": "pod-46",
    "phase": "Running"
  },
  {
    "idx": 47,
    "name": "pod-47",
    "phase": "Running"
  },
  {
    "idx": 48,
    "name": "pod-48",
    "phase": "Running"
  },
  {
    "idx": 49,
    "name": "pod-49",
    "phase": "Running"
  },
  {
    "idx": 50,
    "name": "pod-50",
    "phase": "Running"
  },
  {
    "idx": 51,
    "name": "pod-51",
    "phase": "Running"
  },
  {
    "idx": 52,
    "name": "pod-52",
    "phase": "Running"
  },
  {
    "idx": 53,
    "name": "pod-53",
    "phase": "Running"
  },
  {
    "idx": 54,
    "name": "pod-54",
    "phase": "Running"
  },
  {
    "idx": 55,
    "name": "pod-55",
    "phase": "Running"
  },
  {
    "idx": 56,
    "name": "pod-56",
    "phase": "Running"
  },
  {
    "idx": 57,
    "name": "pod-57",
    "phase": "Running"
  },
  {
    "idx": 58,
    "name": "pod-58",
    "phase": "Running"
  },
  {
    "idx": 59,
    "name": "pod-59",
    "phase": "Running"
  },
  {
    "idx": 60,
    "name": "pod-60",
    "phase": "Running"
  },
  {
    "idx": 61,
    "name": "pod-61",
    "phase": "Running"
  },
  {
    "idx": 62,
    "name": "pod-62",
    "phase": "Running"
  },
  {
    "idx": 63,
    "name": "pod-63",
    "phase": "Running"
  },
  {
    "idx": 64,
    "name": "pod-64",
    "phase": "Running"
  },
  {
    "idx": 65,
    "name": "pod-65",
    "phase": "Running"
  },
  {
    "idx": 66,
    "name": "pod-66",
    "phase": "Running"
  },
  {
    "idx": 67,
    "name": "pod-67",
    "phase": "Running"
  },
  {
    "idx": 68,
    "name": "pod-68",
    "phase": "Running"
  },
  {
    "idx": 69,
    "name": "pod-69",
    "phase": "Running"
  },
  {
    "idx": 70,
    "name": "pod-70",
    "phase": "Running"
  },
  {
    "idx": 71,
    "name": "pod-71",
    "phase": "Running"
  },
  {
    "idx": 72,
    "name": "pod-72",
    "phase": "Running"
  },
  {
    "idx": 73,
    "name": "pod-73",
    "phase": "CrashLoopBackOff"
  },
  {
    "idx": 74,
    "name": "pod-74",
    "phase": "Running"
  },
  {
    "idx": 75,
    "name": "pod-75",
    "phase": "Running"
  },
  {
    "idx": 76,
    "name": "pod-76",
    "phase": "Running"
  },
  {
    "idx": 77,
    "name": "pod-77",
    "phase": "Running"
  },
  {
    "idx": 78,
    "name": "pod-78",
    "phase": "Running"
  },
  {
    "idx": 79,
    "name": "pod-79",
    "phase": "Running"
  },
  {
    "idx": 80,
    "name": "pod-80",
    "phase": "Running"
  },
  {
    "idx": 81,
    "name": "pod-81",
    "phase": "Running"
  },
  {
    "idx": 82,
    "name": "pod-82",
    "phase": "Running"
  },
  {
    "idx": 83,
    "name": "pod-83",
    "phase": "Running"
  },
  {
    "idx": 84,
    "name": "pod-84",
    "phase": "Running"
  },
  {
    "idx": 85,
    "name": "pod-85",
    "phase": "Running"
  },
  {
    "idx": 86,
    "name": "pod-86",
    "phase": "Running"
  },
  {
    "idx": 87,
    "name": "pod-87",
    "phase": "Running"
  },
  {
    "idx": 88,
    "name": "pod-88",
    "phase": "Running"
  },
  {
    "idx": 89,
    "name": "pod-89",
    "phase": "Running"
  },
  {
    "idx": 90,
    "name": "pod-90",
    "phase": "Running"
  },
  {
    "idx": 91,
    "name": "pod-91",
    "phase": "Running"
  },
  {
    "idx": 92,
    "name": "pod-92",
    "phase": "Running"
  },
  {
    "idx": 93,
    "name": "pod-93",
    "phase": "Running"
  },
  {
    "idx": 94,
    "name": "pod-94",
    "phase": "Running"
  },
  {
    "idx": 95,
    "name": "pod-95",
    "phase": "Running"
  },
  {
    "idx": 96,
    "name": "pod-96",
    "phase": "Running"
  },
  {
    "idx": 97,
    "name": "pod-97",
    "phase": "Running"
  },
  {
    "idx": 98,
    "name": "pod-98",
    "phase": "Running"
  },
  {
    "idx": 99,
    "name": "pod-99",
    "phase": "Running"
  },
  {
    "idx": 100,
    "name": "pod-100",
    "phase": "Running"
  },
  {
    "idx": 101,
    "name": "pod-101",
    "phase": "Running"
  },
  {
    "idx": 102,
    "name": "pod-102",
    "phase": "Running"
  },
  {
    "idx": 103,
    "name": "pod-103",
    "phase": "Running"
  },
  {
    "idx": 104,
    "name": "pod-104",
    "phase": "Running"
  },
  {
    "idx": 105,
    "name": "pod-105",
    "phase": "Running"
  },
  {
    "idx": 106,
    "name": "pod-106",
    "phase": "Running"
  },
  {
    "idx": 107,
    "name": "pod-107",
    "phase": "Running"
  },
  {
    "idx": 108,
    "name": "pod-108",
    "phase": "Running"
  },
  {
    "idx": 109,
    "name": "pod-109",
    "phase": "Running"
  },
  {
    "idx": 110,
    "name": "pod-110",
    "phase": "Running"
  },
  {
    "idx": 111,
    "name": "pod-111",
    "phase": "Running"
  },
  {
    "idx": 112,
    "name": "pod-112",
    "phase": "Running"
  },
  {
    "idx": 113,
    "name": "pod-113",
    "phase": "Running"
  },
  {
    "idx": 114,
    "name": "pod-114",
    "phase": "Running"
  },
  {
    "idx": 115,
    "name": "pod-115",
    "phase": "Running"
  },
  {
    "idx": 116,
    "name": "pod-116",
    "phase": "Running"
  },
  {
    "idx": 117,
    "name": "pod-117",
    "phase": "Running"
  },
  {
    "idx": 118,
    "name": "pod-118",
    "phase": "Running"
  },
  {
    "idx": 119,
    "name": "pod-119",
    "phase": "Running"
  },
  {
    "idx": 120,
    "name": "pod-120",
    "phase": "Running"
  },
  {
    "idx": 121,
    "name": "pod-121",
    "phase": "Running"
  },
  {
    "idx": 122,
    "name": "pod-122",
    "phase": "Running"
  },
  {
    "idx": 123,
    "name": "pod-123",
    "phase": "Running"
  },
  {
    "idx": 124,
    "name": "pod-124",
    "phase": "Running"
  },
  {
    "idx": 125,
    "name": "pod-125",
    "phase": "Running"
  },
  {
    "idx": 126,
    "name": "pod-126",
    "phase": "Running"
  },
  {
    "idx": 127,
    "name": "pod-127",
    "phase": "Running"
  },
  {
    "idx": 128,
    "name": "pod-128",
    "phase": "Running"
  },
  {
    "idx": 129,
    "name": "pod-129",
    "phase": "Running"
  },
  {
    "idx": 130,
    "name": "pod-130",
    "phase": "Running"
  },
  {
    "idx": 131,
    "name": "pod-131",
    "phase": "Running"
  },
  {
    "idx": 132,
    "name": "pod-132",
    "phase": "Running"
  },
  {
    "idx": 133,
    "name": "pod-133",
    "phase": "Running"
  },
  {
    "idx": 134,
    "name": "pod-134",
    "phase": "Running"
  },
  {
    "idx": 135,
    "name": "pod-135",
    "phase": "Running"
  },
  {
    "idx": 136,
    "name": "pod-136",
    "phase": "Running"
  },
  {
    "idx": 137,
    "name": "pod-137",
    "phase": "Running"
  },
  {
    "idx": 138,
    "name": "pod-138",
    "phase": "Running"
  },
  {
    "idx": 139,
    "name": "pod-139",
    "phase": "Running"
  },
  {
    "idx": 140,
    "name": "pod-140",
    "phase": "Running"
  },
  {
    "idx": 141,
    "name": "pod-141",
    "phase": "Running"
  },
  {
    "idx": 142,
    "name": "pod-142",
    "phase": "Running"
  },
  {
    "idx": 143,
    "name": "pod-143",
    "phase": "Running"
  },
  {
    "idx": 144,
    "name": "pod-144",
    "phase": "Running"
  },
  {
    "idx": 145,
    "name": "pod-145",
    "phase": "Running"
  },
  {
    "idx": 146,
    "name": "pod-146",
    "phase": "Running"
  },
  {
    "idx": 147,
    "name": "pod-147",
    "phase": "Running"
  },
  {
    "idx": 148,
    "name": "pod-148",
    "phase": "Running"
  },
  {
    "idx": 149,
    "name": "pod-149",
    "phase": "Running"
  },
  {
    "idx": 150,
    "name": "pod-150",
    "phase": "Running"
  },
  {
    "idx": 151,
    "name": "pod-151",
    "phase": "Running"
  },
  {
    "idx": 152,
    "name": "pod-152",
    "phase": "Running"
  },
  {
    "idx": 153,
    "name": "pod-153",
    "phase": "Running"
  },
  {
    "idx": 154,
    "name": "pod-154",
    "phase": "Running"
  },
  {
    "idx": 155,
    "name": "pod-155",
    "phase": "Running"
  },
  {
    "idx": 156,
    "name": "pod-156",
    "phase": "Running"
  },
  {
    "idx": 157,
    "name": "pod-157",
    "phase": "Running"
  },
  {
    "idx": 158,
    "name": "pod-158",
    "phase": "Running"
  },
  {
    "idx": 159,
    "name": "pod-159",
    "phase": "Running"
  },
  {
    "idx": 160,
    "name": "pod-160",
    "phase": "Running"
  },
  {
    "idx": 161,
    "name": "pod-161",
    "phase": "Running"
  },
  {
    "idx": 162,
    "name": "pod-162",
    "phase": "Running"
  },
  {
    "idx": 163,
    "name": "pod-163",
    "phase": "Running"
  },
  {
    "idx": 164,
    "name": "pod-164",
    "phase": "Running"
  },
  {
    "idx": 165,
    "name": "pod-165",
    "phase": "Running"
  },
  {
    "idx": 166,
    "name": "pod-166",
    "phase": "Running"
  },
  {
    "idx": 167,
    "name": "pod-167",
    "phase": "Running"
  },
  {
    "idx": 168,
    "name": "pod-168",
    "phase": "Running"
  },
  {
    "idx": 169,
    "name": "pod-169",
    "phase": "Running"
  },
  {
    "idx": 170,
    "name": "pod-170",
    "phase": "Running"
  },
  {
    "idx": 171,
    "name": "pod-171",
    "phase": "Running"
  },
  {
    "idx": 172,
    "name": "pod-172",
    "phase": "Running"
  },
  {
    "idx": 173,
    "name": "pod-173",
    "phase": "Running"
  },
  {
    "idx": 174,
    "name": "pod-174",
    "phase": "Running"
  },
  {
    "idx": 175,
    "name": "pod-175",
    "phase": "Running"
  },
  {
    "idx": 176,
    "name": "pod-176",
    "phase": "Running"
  },
  {
    "idx": 177,
    "name": "pod-177",
    "phase": "Running"
  },
  {
    "idx": 178,
    "name": "pod-178",
    "phase": "Running"
  },
  {
    "idx": 179,
    "name": "pod-179",
    "phase": "Running"
  },
  {
    "idx": 180,
    "name": "pod-180",
    "phase": "Running"
  },
  {
    "idx": 181,
    "name": "pod-181",
    "phase": "Running"
  },
  {
    "idx": 182,
    "name": "pod-182",
    "phase": "Running"
  },
  {
    "idx": 183,
    "name": "pod-183",
    "phase": "Running"
  },
  {
    "idx": 184,
    "name": "pod-184",
    "phase": "Running"
  },
  {
    "idx": 185,
    "name": "pod-185",
    "phase": "Running"
  },
  {
    "idx": 186,
    "name": "pod-186",
    "phase": "Running"
  },
  {
    "idx": 187,
    "name": "pod-187",
    "phase": "Running"
  },
  {
    "idx": 188,
    "name": "pod-188",
    "phase": "Running"
  },
  {
    "idx": 189,
    "name": "pod-189",
    "phase": "Running"
  },
  {
    "idx": 190,
    "name": "pod-190",
    "phase": "Running"
  },
  {
    "idx": 191,
    "name": "pod-191",
    "phase": "Running"
  },
  {
    "idx": 192,
    "name": "pod-192",
    "phase": "Running"
  },
  {
    "idx": 193,
    "name": "pod-193",
    "phase": "Running"
  },
  {
    "idx": 194,
    "name": "pod-194",
    "phase": "Running"
  },
  {
    "idx": 195,
    "name": "pod-195",
    "phase": "Running"
  },
  {
    "idx": 196,
    "name": "pod-196",
    "phase": "Running"
  },
  {
    "idx": 197,
    "name": "pod-197",
    "phase": "Running"
  },
  {
    "idx": 198,
    "name": "pod-198",
    "phase": "Running"
  },
  {
    "idx": 199,
    "name": "pod-199",
    "phase": "Running"
  }
]
TOTAL: 200 pods listed
```

## 压缩后输出

```text
$ kubectl get pods -o json
[
  {
    "idx": 0,
    "name": "pod-0",
    "phase": "Running"
  },
  {
    "idx": 1,
    "name": "pod-1",
    "phase": "Running"
  },
  {
    "idx": 2,
    "name": "pod-2",
    "phase": "Running"
  },
  {
    "idx": 3,
    "name": "pod-3",
    "phase": "Running"
  },
  {
    "idx": 4,
    "name": "pod-4",
    "phase": "Running"
  },
  {
    "idx": 5,
    "name": "pod-5",
    "phase": "Running"
  },
  {
    "idx": 6,
    "name": "pod-6",
    "phase": "Running"
  },
  {
    "idx": 7,
    "name": "pod-7",
    "phase": "Running"
  },
  {
    "idx": 8,
    "name": "pod-8",
    "phase": "Running"
  },
  {
    "idx": 9,
    "name": "pod-9",
    "phase": "Running"
  },
  {
    "idx": 10,
    "name": "pod-10",
    "phase": "Running"
  },
  {
    "idx": 11,
    "name": "pod-11",
    "phase": "Running"
  },
  {
    "idx": 73,
    "name": "pod-73",
    "phase": "CrashLoopBackOff"
  },
  {
    "idx": 198,
    "name": "pod-198",
    "phase": "Running"
  },
  {
    "idx": 199,
    "name": "pod-199",
    "phase": "Running"
  },
  {
    "_crushed": "cluster(name)",
    "_original_count": 200,
    "_dropped_count": 185,
    "_dropped_sample": [
      {
        "idx": 12,
        "name": "pod-12",
        "phase": "Running"
      },
      {
        "idx": 13,
        "name": "pod-13",
        "phase": "Running"
      },
      {
        "idx": 14,
        "name": "pod-14",
        "phase": "Running"
      }
    ],
    "_dropped_field_summary": {
      "phase": {
        "Running": "185/185"
      }
    }
  }
]
TOTAL: 200 pods listed<<ccr:e0d7ca9523dc1b63c7565348>>
```

## 运行结果

| 指标 | 结果 |
|---|---:|
| 原文字节数 | 9240 |
| 压缩后字节数 | 1599 |
| 压缩后占比 | 17.3% |
| 节省 token（估算） | 2301 |
| 检查 block | 1 |
| 压缩 block | 1 |
| 回退 block | 0 |
| 冻结消息 | 1 |
| CCR 写入 | 1 |

- CCR 恢复：PASS（`e0d7ca9523dc1b63c7565348`）
- 场景断言：PASS
