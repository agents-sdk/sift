import type { DemoCase } from '../types';

const rows = Array.from({ length: 200 }, (_, i) => ({
  idx: i,
  name: `pod-${i}`,
  phase: i === 73 ? 'CrashLoopBackOff' : 'Running',
}));
const input = [
  '$ kubectl get pods -o json',
  JSON.stringify(rows),
  `TOTAL: ${rows.length} pods listed`,
].join('\n');

export const mixedOutputCase: DemoCase = {
  id: 'mixed-output',
  title: '命令回显 + JSON + 尾注混合输出',
  description: '识别轻量 wrapper 中的大 JSON，以 CSV-schema 保留全部行和外部命令文本。',
  input,
  query: 'find unhealthy pods',
  expectedType: 'json_array',
  expectedPath: 'changed',
  mustContain: ['$ kubectl get pods -o json', 'TOTAL: 200 pods listed', 'CrashLoopBackOff'],
};
