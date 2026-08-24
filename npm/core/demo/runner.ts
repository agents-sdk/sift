import * as assert from 'node:assert';
import * as os from 'node:os';
import * as path from 'node:path';
import type { DemoCase } from './types';

// 走用户实际使用的包入口：package.json 的 main -> dist/index.js。
// 编译后的文件位于 dist-demo/demo/runner.js，向上两级是 npm 包根目录。
process.env.SIFT_STASH_DIR ??= path.join(
  os.tmpdir(),
  `sift-demo-${process.pid}`,
);
const { siftRequest, retrieve, detectContentType } = require(
  path.resolve(__dirname, '..', '..'),
) as typeof import('../src/index');

const STASH_RE = /<<stash:([0-9a-f]{24})>>/g;

function makeBody(content: string) {
  return {
    messages: [
      {
        role: 'user',
        content: [
          {
            type: 'text',
            text: '[cached system prompt] You are a helpful assistant.',
            cache_control: { type: 'ephemeral' },
          },
        ],
      },
      {
        role: 'user',
        content: [{ type: 'tool_result', tool_use_id: 'demo-tool', content }],
      },
    ],
  };
}

function heading(label: string) {
  console.log(`\n${'='.repeat(24)} ${label} ${'='.repeat(24)}`);
}

/** 只改善终端可读性，不改变传给 compress 的原始字符串。 */
function formatOriginalForDisplay(input: string): string {
  try {
    return JSON.stringify(JSON.parse(input), null, 2);
  } catch {
    // 混合命令输出中常嵌有一段 minified JSON；完整展开这一段便于逐行对照。
    const start = input.indexOf('[');
    const end = input.lastIndexOf(']');
    if (start >= 0 && end > start) {
      try {
        const json = JSON.stringify(JSON.parse(input.slice(start, end + 1)), null, 2);
        return input.slice(0, start) + json + input.slice(end + 1);
      } catch {
        // 不是完整 JSON span，按原样展示。
      }
    }
    return input;
  }
}

export interface DemoResult {
  output: string;
  contentType: string;
  beforeBytes: number;
  afterBytes: number;
  compressionRatio: number;
  tokensSaved: number;
  blocksExamined: number;
  blocksCompressed: number;
  blocksReverted: number;
  frozenMessages: number;
  stashStored: number;
  stashKey?: string;
}

function codeBlock(content: string): string {
  const fence = content.includes('```') ? '````' : '```';
  return `${fence}text\n${content}\n${fence}`;
}

/** 把一次真实运行保存为便于并排查看的 Markdown。 */
export function renderResultMarkdown(demo: DemoCase, result: DemoResult): string {
  const lines = [
    `# ${demo.title}`,
    '',
    demo.description,
    '',
    `- 场景 ID：\`${demo.id}\``,
    `- 检测类型：\`${result.contentType}\``,
  ];
  if (demo.query) lines.push(`- 相关性 query：\`${demo.query}\``);
  lines.push(
    '',
    '## 压缩前原文',
    '',
    '> 内容完整保留；为了便于阅读，JSON 仅在展示时进行了缩进美化。',
    '',
    codeBlock(formatOriginalForDisplay(demo.input)),
    '',
    '## 压缩后输出',
    '',
    codeBlock(result.output),
    '',
    '## 运行结果',
    '',
    '| 指标 | 结果 |',
    '|---|---:|',
    `| 原文字节数 | ${result.beforeBytes} |`,
    `| 压缩后字节数 | ${result.afterBytes} |`,
    `| 压缩后占比 | ${(result.compressionRatio * 100).toFixed(1)}% |`,
    `| 节省 token（估算） | ${result.tokensSaved} |`,
    `| 检查 block | ${result.blocksExamined} |`,
    `| 压缩 block | ${result.blocksCompressed} |`,
    `| 回退 block | ${result.blocksReverted} |`,
    `| 冻结消息 | ${result.frozenMessages} |`,
    `| stash 写入 | ${result.stashStored} |`,
    '',
    `- stash 恢复：${result.stashKey ? `PASS（\`${result.stashKey}\`）` : '不适用'}`,
    '- 场景断言：PASS',
    '',
  );
  return lines.join('\n');
}

/** 完整打印一个场景的原文、压缩后文本和验证指标。 */
export function runCase(demo: DemoCase, index: number, total: number): DemoResult {
  console.log('\n' + '#'.repeat(80));
  console.log(`# 示例 ${index}/${total}：${demo.title}  [${demo.id}]`);
  console.log('#'.repeat(80));
  console.log(demo.description);
  console.log(`检测类型期望: ${demo.expectedType}`);
  if (demo.query) console.log(`相关性 query: ${demo.query}`);

  heading('压缩前原文（完整；JSON 自动美化显示）');
  console.log(formatOriginalForDisplay(demo.input));

  const body = makeBody(demo.input);
  const frozenBefore = JSON.stringify(body.messages[0]);
  const result = siftRequest(body, demo.query);
  const output = (result.body as any).messages[1].content[0].content as string;

  heading('压缩后输出（完整）');
  console.log(output);

  heading('运行结果');
  const contentType = detectContentType(demo.input);
  const beforeBytes = Buffer.byteLength(demo.input);
  const afterBytes = Buffer.byteLength(output);
  const compressionRatio = afterBytes / beforeBytes;
  console.log(`changed:          ${result.changed}`);
  console.log(`contentType:      ${contentType}`);
  console.log(`beforeBytes:      ${beforeBytes}`);
  console.log(`afterBytes:       ${afterBytes}`);
  console.log(`compressionRatio: ${(compressionRatio * 100).toFixed(1)}%`);
  console.log(`tokensSaved:      ${result.tokensSaved}`);
  console.log(`blocksExamined:   ${result.blocksExamined}`);
  console.log(`blocksCompressed: ${result.blocksCompressed}`);
  console.log(`blocksReverted:   ${result.blocksReverted}`);
  console.log(`frozenMessages:   ${result.frozenMessages}`);
  console.log(`stashStored:        ${result.stashStored}`);

  // 所有场景共同验证：类型识别正确，冻结前缀逐字不动。
  assert.strictEqual(contentType, demo.expectedType);
  assert.strictEqual(JSON.stringify((result.body as any).messages[0]), frozenBefore);
  assert.strictEqual(result.frozenMessages, 1);

  let stashKey: string | undefined;
  if (demo.expectedPath === 'lossy-stash') {
    assert.strictEqual(result.changed, true);
    const keys = [...output.matchAll(STASH_RE)].map((match) => match[1]);
    assert.ok(keys.length > 0, '有损结果必须包含合法的 stash 标记');
    stashKey = keys[keys.length - 1];
    assert.strictEqual(retrieve(stashKey), demo.input, 'stash 必须能恢复完整原文');
    console.log(`stash restore:      PASS (${stashKey})`);
  } else if (demo.expectedPath === 'changed') {
    assert.strictEqual(result.changed, true);
  } else {
    assert.strictEqual(result.changed, false);
    assert.strictEqual(output, demo.input);
  }

  for (const text of demo.mustContain ?? []) {
    assert.ok(output.includes(text), `压缩结果应保留: ${text}`);
  }
  demo.verify?.(output);
  console.log('verification:     PASS');

  return {
    output,
    contentType,
    beforeBytes,
    afterBytes,
    compressionRatio,
    tokensSaved: result.tokensSaved,
    blocksExamined: result.blocksExamined,
    blocksCompressed: result.blocksCompressed,
    blocksReverted: result.blocksReverted,
    frozenMessages: result.frozenMessages,
    stashStored: result.stashStored,
    stashKey,
  };
}
