"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.mixedOutputCase = void 0;
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
exports.mixedOutputCase = {
    id: 'mixed-output',
    title: '命令回显 + JSON + 尾注混合输出',
    description: '分段识别嵌入的大 JSON，同时保留 JSON 外部的命令文本。',
    input,
    query: 'find unhealthy pods',
    expectedType: 'plain_text',
    expectedPath: 'lossy-stash',
    mustContain: ['$ kubectl get pods -o json', 'TOTAL: 200 pods listed', 'CrashLoopBackOff'],
};
