import * as assert from 'node:assert';
import type { DemoCase } from '../types';

// 模拟 API / 数据库工具返回的大型 issue 列表。
const rows = Array.from({ length: 200 }, (_, i) => ({
  id: i,
  title: `Issue #${i}: ${i % 17 === 0 ? 'panic in worker pool' : 'minor typo in docs'}`,
  state: i % 50 === 0 ? 'open' : 'closed',
  labels: ['bug', 'docs'],
}));

export const jsonArrayCase: DemoCase = {
  id: 'json-array',
  title: 'JSON 数组工具输出',
  description: '规则对象数组先提取 CSV-schema，字段名只声明一次并保留全部记录。',
  input: JSON.stringify(rows),
  query: 'list open issues and worker pool panics',
  expectedType: 'json_array',
  expectedPath: 'changed',
  mustContain: ['panic in worker pool'],
  verify(output) {
    assert.match(output, /^\[200\]\{/);
    assert.ok(output.includes('199,'), '最后一条记录必须保留');
    assert.ok(!output.includes('<<stash:'), '无损 schema 不应写 stash');
  },
};
