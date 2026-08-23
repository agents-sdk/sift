# JSON 数组工具输出

smart_crusher 保留异常项和代表性样本，其余原文写入 CCR。

- 场景 ID：`json-array`
- 检测类型：`json_array`
- 相关性 query：`list open issues and worker pool panics`

## 压缩前原文

> 内容完整保留；为了便于阅读，JSON 仅在展示时进行了缩进美化。

```text
[
  {
    "id": 0,
    "title": "Issue #0: panic in worker pool",
    "state": "open",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 1,
    "title": "Issue #1: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 2,
    "title": "Issue #2: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 3,
    "title": "Issue #3: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 4,
    "title": "Issue #4: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 5,
    "title": "Issue #5: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 6,
    "title": "Issue #6: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 7,
    "title": "Issue #7: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 8,
    "title": "Issue #8: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 9,
    "title": "Issue #9: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 10,
    "title": "Issue #10: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 11,
    "title": "Issue #11: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 12,
    "title": "Issue #12: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 13,
    "title": "Issue #13: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 14,
    "title": "Issue #14: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 15,
    "title": "Issue #15: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 16,
    "title": "Issue #16: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 17,
    "title": "Issue #17: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 18,
    "title": "Issue #18: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 19,
    "title": "Issue #19: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 20,
    "title": "Issue #20: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 21,
    "title": "Issue #21: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 22,
    "title": "Issue #22: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 23,
    "title": "Issue #23: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 24,
    "title": "Issue #24: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 25,
    "title": "Issue #25: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 26,
    "title": "Issue #26: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 27,
    "title": "Issue #27: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 28,
    "title": "Issue #28: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 29,
    "title": "Issue #29: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 30,
    "title": "Issue #30: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 31,
    "title": "Issue #31: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 32,
    "title": "Issue #32: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 33,
    "title": "Issue #33: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 34,
    "title": "Issue #34: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 35,
    "title": "Issue #35: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 36,
    "title": "Issue #36: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 37,
    "title": "Issue #37: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 38,
    "title": "Issue #38: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 39,
    "title": "Issue #39: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 40,
    "title": "Issue #40: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 41,
    "title": "Issue #41: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 42,
    "title": "Issue #42: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 43,
    "title": "Issue #43: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 44,
    "title": "Issue #44: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 45,
    "title": "Issue #45: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 46,
    "title": "Issue #46: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 47,
    "title": "Issue #47: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 48,
    "title": "Issue #48: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 49,
    "title": "Issue #49: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 50,
    "title": "Issue #50: minor typo in docs",
    "state": "open",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 51,
    "title": "Issue #51: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 52,
    "title": "Issue #52: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 53,
    "title": "Issue #53: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 54,
    "title": "Issue #54: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 55,
    "title": "Issue #55: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 56,
    "title": "Issue #56: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 57,
    "title": "Issue #57: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 58,
    "title": "Issue #58: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 59,
    "title": "Issue #59: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 60,
    "title": "Issue #60: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 61,
    "title": "Issue #61: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 62,
    "title": "Issue #62: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 63,
    "title": "Issue #63: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 64,
    "title": "Issue #64: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 65,
    "title": "Issue #65: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 66,
    "title": "Issue #66: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 67,
    "title": "Issue #67: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 68,
    "title": "Issue #68: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 69,
    "title": "Issue #69: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 70,
    "title": "Issue #70: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 71,
    "title": "Issue #71: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 72,
    "title": "Issue #72: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 73,
    "title": "Issue #73: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 74,
    "title": "Issue #74: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 75,
    "title": "Issue #75: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 76,
    "title": "Issue #76: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 77,
    "title": "Issue #77: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 78,
    "title": "Issue #78: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 79,
    "title": "Issue #79: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 80,
    "title": "Issue #80: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 81,
    "title": "Issue #81: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 82,
    "title": "Issue #82: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 83,
    "title": "Issue #83: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 84,
    "title": "Issue #84: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 85,
    "title": "Issue #85: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 86,
    "title": "Issue #86: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 87,
    "title": "Issue #87: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 88,
    "title": "Issue #88: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 89,
    "title": "Issue #89: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 90,
    "title": "Issue #90: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 91,
    "title": "Issue #91: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 92,
    "title": "Issue #92: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 93,
    "title": "Issue #93: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 94,
    "title": "Issue #94: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 95,
    "title": "Issue #95: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 96,
    "title": "Issue #96: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 97,
    "title": "Issue #97: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 98,
    "title": "Issue #98: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 99,
    "title": "Issue #99: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 100,
    "title": "Issue #100: minor typo in docs",
    "state": "open",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 101,
    "title": "Issue #101: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 102,
    "title": "Issue #102: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 103,
    "title": "Issue #103: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 104,
    "title": "Issue #104: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 105,
    "title": "Issue #105: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 106,
    "title": "Issue #106: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 107,
    "title": "Issue #107: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 108,
    "title": "Issue #108: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 109,
    "title": "Issue #109: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 110,
    "title": "Issue #110: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 111,
    "title": "Issue #111: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 112,
    "title": "Issue #112: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 113,
    "title": "Issue #113: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 114,
    "title": "Issue #114: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 115,
    "title": "Issue #115: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 116,
    "title": "Issue #116: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 117,
    "title": "Issue #117: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 118,
    "title": "Issue #118: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 119,
    "title": "Issue #119: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 120,
    "title": "Issue #120: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 121,
    "title": "Issue #121: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 122,
    "title": "Issue #122: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 123,
    "title": "Issue #123: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 124,
    "title": "Issue #124: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 125,
    "title": "Issue #125: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 126,
    "title": "Issue #126: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 127,
    "title": "Issue #127: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 128,
    "title": "Issue #128: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 129,
    "title": "Issue #129: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 130,
    "title": "Issue #130: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 131,
    "title": "Issue #131: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 132,
    "title": "Issue #132: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 133,
    "title": "Issue #133: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 134,
    "title": "Issue #134: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 135,
    "title": "Issue #135: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 136,
    "title": "Issue #136: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 137,
    "title": "Issue #137: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 138,
    "title": "Issue #138: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 139,
    "title": "Issue #139: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 140,
    "title": "Issue #140: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 141,
    "title": "Issue #141: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 142,
    "title": "Issue #142: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 143,
    "title": "Issue #143: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 144,
    "title": "Issue #144: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 145,
    "title": "Issue #145: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 146,
    "title": "Issue #146: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 147,
    "title": "Issue #147: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 148,
    "title": "Issue #148: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 149,
    "title": "Issue #149: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 150,
    "title": "Issue #150: minor typo in docs",
    "state": "open",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 151,
    "title": "Issue #151: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 152,
    "title": "Issue #152: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 153,
    "title": "Issue #153: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 154,
    "title": "Issue #154: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 155,
    "title": "Issue #155: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 156,
    "title": "Issue #156: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 157,
    "title": "Issue #157: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 158,
    "title": "Issue #158: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 159,
    "title": "Issue #159: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 160,
    "title": "Issue #160: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 161,
    "title": "Issue #161: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 162,
    "title": "Issue #162: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 163,
    "title": "Issue #163: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 164,
    "title": "Issue #164: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 165,
    "title": "Issue #165: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 166,
    "title": "Issue #166: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 167,
    "title": "Issue #167: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 168,
    "title": "Issue #168: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 169,
    "title": "Issue #169: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 170,
    "title": "Issue #170: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 171,
    "title": "Issue #171: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 172,
    "title": "Issue #172: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 173,
    "title": "Issue #173: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 174,
    "title": "Issue #174: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 175,
    "title": "Issue #175: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 176,
    "title": "Issue #176: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 177,
    "title": "Issue #177: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 178,
    "title": "Issue #178: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 179,
    "title": "Issue #179: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 180,
    "title": "Issue #180: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 181,
    "title": "Issue #181: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 182,
    "title": "Issue #182: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 183,
    "title": "Issue #183: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 184,
    "title": "Issue #184: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 185,
    "title": "Issue #185: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 186,
    "title": "Issue #186: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 187,
    "title": "Issue #187: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 188,
    "title": "Issue #188: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 189,
    "title": "Issue #189: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 190,
    "title": "Issue #190: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 191,
    "title": "Issue #191: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 192,
    "title": "Issue #192: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 193,
    "title": "Issue #193: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 194,
    "title": "Issue #194: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 195,
    "title": "Issue #195: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 196,
    "title": "Issue #196: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 197,
    "title": "Issue #197: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 198,
    "title": "Issue #198: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 199,
    "title": "Issue #199: minor typo in docs",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  }
]
```

## 压缩后输出

```text
[
  {
    "id": 0,
    "title": "Issue #0: panic in worker pool",
    "state": "open",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 17,
    "title": "Issue #17: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 34,
    "title": "Issue #34: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 50,
    "title": "Issue #50: minor typo in docs",
    "state": "open",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 51,
    "title": "Issue #51: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 68,
    "title": "Issue #68: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 85,
    "title": "Issue #85: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 100,
    "title": "Issue #100: minor typo in docs",
    "state": "open",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 102,
    "title": "Issue #102: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 119,
    "title": "Issue #119: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 136,
    "title": "Issue #136: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 150,
    "title": "Issue #150: minor typo in docs",
    "state": "open",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 153,
    "title": "Issue #153: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 170,
    "title": "Issue #170: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "id": 187,
    "title": "Issue #187: panic in worker pool",
    "state": "closed",
    "labels": [
      "bug",
      "docs"
    ]
  },
  {
    "_crushed": "cluster(title)",
    "_original_count": 200,
    "_dropped_count": 185,
    "_dropped_sample": [
      {
        "id": 1,
        "title": "Issue #1: minor typo in docs",
        "state": "closed",
        "labels": [
          "bug",
          "docs"
        ]
      },
      {
        "id": 2,
        "title": "Issue #2: minor typo in docs",
        "state": "closed",
        "labels": [
          "bug",
          "docs"
        ]
      },
      {
        "id": 3,
        "title": "Issue #3: minor typo in docs",
        "state": "closed",
        "labels": [
          "bug",
          "docs"
        ]
      }
    ],
    "_dropped_field_summary": {
      "labels": {
        "[\"bug\",\"docs\"]": "185/185"
      },
      "state": {
        "closed": "185/185"
      }
    }
  }
]<<ccr:09b57ccbdd17b7775af1194f>>
```

## 运行结果

| 指标 | 结果 |
|---|---:|
| 原文字节数 | 18397 |
| 压缩后字节数 | 2973 |
| 压缩后占比 | 16.2% |
| 节省 token（估算） | 4637 |
| 检查 block | 1 |
| 压缩 block | 1 |
| 回退 block | 0 |
| 冻结消息 | 1 |
| CCR 写入 | 1 |

- CCR 恢复：PASS（`09b57ccbdd17b7775af1194f`）
- 场景断言：PASS
