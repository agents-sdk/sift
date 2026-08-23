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
  description: 'smart_crusher 保留异常项和代表性样本，其余原文写入 CCR。',
  input: JSON.stringify(rows),
  query: 'list open issues and worker pool panics',
  expectedType: 'json_array',
  expectedPath: 'lossy-ccr',
  mustContain: ['panic in worker pool', '"state": "open"'],
};
