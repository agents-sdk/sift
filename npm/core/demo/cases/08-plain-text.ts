import type { DemoCase } from '../types';

const repeatedStatus =
  'Routine status: the compatibility test matrix completed successfully on all supported ' +
  'platforms. No new failures, decisions, or follow-up actions were reported in this update.';

const paragraphs = [
  'Weekly engineering status report. Exact duplicate updates should be collapsed while ' +
    'the first occurrence and all unique conclusions remain visible.',
  ...Array.from({ length: 14 }, () => repeatedStatus),
  'Conclusion: continue monitoring the release candidate and escalate only newly observed failures.',
];

export const plainTextCase: DemoCase = {
  id: 'plain-text',
  title: '重复纯文本',
  description: '折叠同一章节内逐字相同的完整段落，保留首份与独有结论。',
  input: paragraphs.join('\n\n'),
  query: 'compatibility failures',
  expectedType: 'plain_text',
  expectedPath: 'lossy-stash',
  mustContain: ['Weekly engineering status report', repeatedStatus, 'Conclusion:'],
  verify: (output) => {
    if (output.split(repeatedStatus).length - 1 !== 1) {
      throw new Error('重复状态段落应只保留首份');
    }
  },
};
