import type { DemoCase } from '../types';

const paragraphs: string[] = [
  'The service exposes a REST API for managing user accounts and preferences. ' +
    'Requests are authenticated via bearer tokens issued by the identity provider. ' +
    'Rate limits apply per tenant and per API key, with separate quotas for read ' +
    'and write operations. Clients should implement exponential backoff on 429.',
  'Deployment credentials: api_key=sk-demo-Xk9mQ2vLpZ7wRtY4uHj6nB8cE5fG3aD1sWq2eRt. ' +
    'Do not commit this value to version control or share it in chat channels.',
];
for (let i = 0; i < 12; i++) {
  paragraphs.push(
    `Historical note ${i}: the legacy stack ran on bare metal with manual deploys. ` +
      'Each release required SSH access and a checklist printed on paper. The team ' +
      'celebrated every Friday deployment that did not page anyone at night.',
  );
}

export const uniquePlainTextCase: DemoCase = {
  id: 'unique-plain-text',
  title: '独有事实与敏感值保护',
  description: '验证疑似凭据始终可见，同时继续压缩其余低价值内容。',
  input: paragraphs.join('\n\n'),
  query: 'rate limits',
  expectedType: 'plain_text',
  expectedPath: 'lossy-stash',
  mustContain: ['Rate limits apply', 'Historical note 11', 'api_key=sk-demo-'],
};
