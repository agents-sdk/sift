import type { DemoCase } from '../types';

const lines = ['   Compiling serde v1.0.200'];
for (let i = 0; i < 40; i++) {
  lines.push(`   Compiling dep-${String(i).padStart(3, '0')} v0.${i}.0`);
}
lines.push('warning: unused variable: `x`');
for (let i = 0; i < 30; i++) {
  lines.push(`note: \`#[warn(unused_variables)]\` on by default (line ${i})`);
}
lines.push(
  'error[E0308]: mismatched types',
  '  --> src/main.rs:42:13',
  '   | let x: String = 42;',
  'error: could not compile `demo` due to 1 previous error',
);

export const buildLogCase: DemoCase = {
  id: 'build-log',
  title: 'Cargo 构建日志',
  description: '折叠重复构建噪声，同时保留 warning、error 和源码位置。',
  input: lines.join('\n'),
  expectedType: 'build_output',
  expectedPath: 'changed',
  mustContain: [
    'warning: unused variable',
    'error[E0308]: mismatched types',
    'src/main.rs:42:13',
  ],
};
