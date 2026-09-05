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
  title: 'pretty JSON 无损紧凑化',
  description: '规则对象数组提取 CSV-schema，全部 60 条记录保持可见且无需 stash。',
  input,
  expectedType: 'json_array',
  expectedPath: 'changed',
  verify(output) {
    assert.match(output, /^\[60\]\{id:int,name:string,ok:bool\}/);
    assert.ok(output.includes('59,item-59,true'), '最后一条记录必须保留');
    assert.ok(!output.includes('<<stash:'), '无损 schema 不应写 stash');
  },
};
