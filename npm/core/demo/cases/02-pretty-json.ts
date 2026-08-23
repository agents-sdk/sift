import * as assert from 'node:assert';
import type { DemoCase } from '../types';

const rows = Array.from({ length: 60 }, (_, i) => ({
  id: i,
  name: `item-${i}`,
  ok: true,
}));
const input = JSON.stringify(rows, null, 2);

export const prettyJsonCase: DemoCase = {
  id: 'pretty-json',
  title: 'pretty JSON 无损压缩',
  description: 'JsonMinifier 只移除缩进和换行，解析后的 JSON 必须完全等价。',
  input,
  expectedType: 'json_array',
  expectedPath: 'changed',
  verify(output) {
    assert.deepStrictEqual(JSON.parse(output), JSON.parse(input));
  },
};
