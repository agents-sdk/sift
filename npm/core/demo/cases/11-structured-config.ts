import type { DemoCase } from '../types';

const components = Array.from({ length: 14 }, (_, index) => `
# Component ${index} keeps explicit runtime settings.
component_${index}:
  image: registry.example.com/service-${index}:2026.09.04
  replicas: 3
  port: ${8100 + index}
  healthPath: /components/${index}/health
`).join('');

const config = `---
# Production service configuration.
service:
  name: context-platform
  namespace: production
  owner: context-team
${components}
# The comments remain available through stash recovery.
rollout:
  strategy: RollingUpdate
  maxUnavailable: 0
  maxSurge: 1
`;

export const structuredConfigCase: DemoCase = {
  id: 'structured-config',
  title: '结构化配置',
  description: 'YAML 保留全部键值与顺序，只卸载可恢复的整行注释和空行。',
  input: config,
  expectedType: 'structured_config',
  expectedPath: 'lossy-stash',
  mustContain: ['name: context-platform', 'component_13:', 'maxSurge: 1'],
  verify: (output) => {
    if (output.includes('Production service configuration')) {
      throw new Error('YAML 整行注释应被卸载');
    }
    if (!output.includes('comment/blank lines elided')) {
      throw new Error('YAML 输出应说明省略的注释与空行数');
    }
  },
};
