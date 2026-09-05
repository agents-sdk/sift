import type { DemoCase } from '../types';

const rows = Array.from({ length: 160 }, (_, index) => {
  const status = index === 137 ? 'degraded' : 'healthy';
  const latency = index === 137 ? 480 : 40 + (index % 30);
  const service = index === 159 ? '"service, edge"' : `service-${index}`;
  return `| ${index} | ${service} | us-east-1 | ${status} | platform | ${latency} |`;
});

export const tabularCase: DemoCase = {
  id: 'tabular',
  title: 'Markdown 表格',
  description: '严格解析列结构后复用 SmartCrusher，以 CSV-schema 无损保留全部记录。',
  input: [
    '| id | service | region | status | owner | latency_ms |',
    '| --- | --- | --- | --- | --- | --- |',
    ...rows,
  ].join('\n'),
  query: 'degraded latency',
  expectedType: 'tabular',
  expectedPath: 'lossy-stash',
  mustContain: ['service', 'status', 'degraded', '480'],
  verify: (output) => {
    if (!output.includes('service-136') || !output.includes('service-138')) {
      throw new Error('无损 schema 必须保留异常行相邻的普通记录');
    }
  },
};
