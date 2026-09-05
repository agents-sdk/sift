/**
 * @compressor/core 原文与压缩结果逐例对照。
 *
 * 运行全部：npm run demo
 * 运行单个：npm run demo -- json-array
 * 保存结果：npm run demo -- --save
 * 查看列表：npm run demo -- --list
 */
import * as fs from 'node:fs';
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
import { structuredConfigCase } from './cases/11-structured-config';
import { tabularCase } from './cases/12-tabular';
import { renderResultMarkdown, runCase } from './runner';
import type { DemoResult } from './runner';
import type { DemoCase } from './types';

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
  structuredConfigCase,
  tabularCase,
];

const args = process.argv.slice(2);
const shouldSave = args.includes('--save');
const selector = args.find((arg) => arg !== '--save');

if (selector === '--list') {
  console.log('可运行的 demo：');
  for (const demo of cases) console.log(`  ${demo.id.padEnd(16)} ${demo.title}`);
} else {
  const selected = selector ? cases.filter((demo) => demo.id === selector) : cases;
  if (selected.length === 0) {
    console.error(`未知 demo: ${selector}`);
    console.error(`可选值: ${cases.map((demo) => demo.id).join(', ')}`);
    process.exitCode = 1;
  } else {
    console.log('@compressor/core：原文与压缩结果逐例对照');
    console.log(`stash 临时目录: ${process.env.SIFT_STASH_DIR}`);
    const completed: Array<{ demo: DemoCase; result: DemoResult; fileName: string }> = [];
    selected.forEach((demo, index) => {
      const result = runCase(demo, index + 1, selected.length);
      const caseNumber = String(cases.indexOf(demo) + 1).padStart(2, '0');
      completed.push({ demo, result, fileName: `${caseNumber}-${demo.id}.md` });
    });

    if (shouldSave) {
      const resultsDir = path.resolve(__dirname, '..', '..', 'demo', 'results');
      fs.mkdirSync(resultsDir, { recursive: true });
      for (const item of completed) {
        fs.writeFileSync(
          path.join(resultsDir, item.fileName),
          renderResultMarkdown(item.demo, item.result),
        );
      }
      const indexLines = [
        '# @compressor/core demo 运行结果',
        '',
        '以下文件由 `npm run demo -- --save` 通过 npm 包公开入口实际运行生成。',
        '',
        '| 示例 | 类型 | 原文字节 | 压缩后字节 | 压缩后占比 | 节省 token | stash |',
        '|---|---|---:|---:|---:|---:|---|',
        ...completed.map(({ demo, result, fileName }) =>
          `| [${demo.title}](./${fileName}) | ${result.contentType} | ${result.beforeBytes} | ` +
          `${result.afterBytes} | ${(result.compressionRatio * 100).toFixed(1)}% | ` +
          `${result.tokensSaved} | ${result.stashKey ? 'PASS' : '—'} |`,
        ),
        '',
      ];
      fs.writeFileSync(path.join(resultsDir, 'README.md'), indexLines.join('\n'));
      console.log(`\n结果已保存到: ${resultsDir}`);
    }
    console.log(`\n${selected.length} 个示例全部验证通过。`);
  }
}
