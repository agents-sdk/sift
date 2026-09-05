/**
 * 用固定 demo 样例输出可复现的压缩数据；可通过 SIFT_BENCH_PACKAGE 指向已发布包。
 *
 * 当前源码：npm run build && npm run benchmark
 * 指定包：SIFT_BENCH_PACKAGE=/path/to/package npm run benchmark
 */
import * as assert from 'node:assert';
import * as os from 'node:os';
import * as path from 'node:path';
import { jsonArrayCase } from './cases/01-json-array';
import { prettyJsonCase } from './cases/02-pretty-json';
import { buildLogCase } from './cases/03-build-log';
import { searchResultsCase } from './cases/04-search-results';
import { gitDiffCase } from './cases/05-git-diff';
import { mixedOutputCase } from './cases/06-mixed-output';
import { sourceCodeCase } from './cases/07-source-code';
import { plainTextCase } from './cases/08-plain-text';
import { uniquePlainTextCase } from './cases/09-unique-plain-text';
import { htmlCase } from './cases/10-html';
import type { DemoCase } from './types';
import type * as SiftApi from '../src/index';

// 固定路径长度，避免内联绝对路径使输出字节数随 PID 或随机目录名波动。
process.env.SIFT_STASH_DIR ??= path.join(os.tmpdir(), 'sift-benchmark-stash');

const packageDir = process.env.SIFT_BENCH_PACKAGE
  ? path.resolve(process.env.SIFT_BENCH_PACKAGE)
  : path.resolve(__dirname, '..', '..');
const sift = require(packageDir) as typeof SiftApi;
const packageVersion = require(path.join(packageDir, 'package.json')).version as string;

const cases: DemoCase[] = [
  jsonArrayCase,
  prettyJsonCase,
  buildLogCase,
  searchResultsCase,
  gitDiffCase,
  mixedOutputCase,
  sourceCodeCase,
  plainTextCase,
  uniquePlainTextCase,
  htmlCase,
];

const STASH_RE = /<<stash:([0-9a-f]{24})>>/g;

function makeBody(content: string) {
  return {
    messages: [
      {
        role: 'user',
        content: [
          {
            type: 'text',
            text: '[cached benchmark prefix]',
            cache_control: { type: 'ephemeral' },
          },
        ],
      },
      {
        role: 'user',
        content: [{ type: 'tool_result', tool_use_id: 'benchmark-tool', content }],
      },
    ],
  };
}

interface Row {
  scenario: string;
  before: number;
  after: number;
  reduction: number;
  tokensSaved: number;
  recovery: string;
}

const rows: Row[] = cases.map((demo) => {
  const body = makeBody(demo.input);
  const frozenBefore = JSON.stringify(body.messages[0]);
  const result = sift.siftRequest(body, demo.query);
  const output = (result.body as any).messages[1].content[0].content as string;

  assert.strictEqual(sift.detectContentType(demo.input), demo.expectedType);
  assert.strictEqual(JSON.stringify((result.body as any).messages[0]), frozenBefore);
  assert.strictEqual(result.frozenMessages, 1);

  const keys = [...output.matchAll(STASH_RE)].map((match) => match[1]);
  let recovery = result.changed ? 'Lossless' : 'Unchanged';
  if (keys.length > 0) {
    assert.strictEqual(sift.retrieve(keys[keys.length - 1]), demo.input);
    recovery = 'PASS';
  }

  for (const expected of demo.mustContain ?? []) {
    assert.ok(output.includes(expected), `${demo.id}: 压缩结果缺少关键文本 ${expected}`);
  }
  demo.verify?.(output);
  if (demo.expectedPath === 'lossy-stash') {
    assert.ok(result.changed, `${demo.id}: 预期有损压缩，实际原样返回`);
    assert.ok(keys.length > 0, `${demo.id}: 预期生成 stash 标记`);
  } else if (demo.expectedPath === 'changed') {
    assert.ok(result.changed, `${demo.id}: 预期内容发生变化`);
  } else {
    assert.ok(!result.changed, `${demo.id}: 预期原样返回`);
  }

  const before = Buffer.byteLength(demo.input);
  const after = Buffer.byteLength(output);
  return {
    scenario: demo.id,
    before,
    after,
    reduction: 1 - after / before,
    tokensSaved: result.tokensSaved,
    recovery,
  };
});

const total = rows.reduce(
  (sum, row) => ({
    before: sum.before + row.before,
    after: sum.after + row.after,
    tokensSaved: sum.tokensSaved + row.tokensSaved,
  }),
  { before: 0, after: 0, tokensSaved: 0 },
);

console.log(`@agent-context/sift ${packageVersion}`);
console.log('| Scenario | Input | Output | Size reduction | Estimated tokens saved | Recovery |');
console.log('| --- | ---: | ---: | ---: | ---: | --- |');
for (const row of rows) {
  console.log(
    `| ${row.scenario} | ${row.before} B | ${row.after} B | ` +
      `${(row.reduction * 100).toFixed(1)}% | ${row.tokensSaved} | ${row.recovery} |`,
  );
}
console.log(
  `| **Total** | **${total.before} B** | **${total.after} B** | ` +
    `**${((1 - total.after / total.before) * 100).toFixed(1)}%** | ` +
    `**${total.tokensSaved}** | |`,
);
console.log('\nAll content-type, frozen-prefix, and stash-recovery checks passed.');
