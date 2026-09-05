import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import {
  createSift,
  siftRequest,
  siftText,
  retrieve,
  retrieveLines,
  detectContentType,
  detectRequestFormat,
} from '../dist/index';

function globalStashFile(key: string): string {
  const configured = process.env.SIFT_STASH_DIR?.trim();
  const dir = configured ? path.resolve(configured) : path.join(os.homedir(), '.sift', 'stash');
  return fs.realpathSync(path.join(dir, key));
}

// 一个含大 JSON 工具输出的请求体，验证真实压缩 + CCR 取回链路。
const rows = Array.from({ length: 200 }, (_, i) => ({
  id: i,
  name: `item-${i}`,
  status: 'ok',
}));
const bigJson = JSON.stringify(rows);

const body = {
  messages: [
    {
      role: 'user',
      content: [
        { type: 'text', text: 'cached system stuff', cache_control: { type: 'ephemeral' } },
      ],
    },
    { role: 'assistant', content: 'old answer' },
    {
      role: 'user',
      content: [{ type: 'tool_result', tool_use_id: 't1', content: bigJson }],
    },
  ],
};

const result = siftRequest(body);

// 冻结前缀未触碰
assert.strictEqual(result.frozenMessages, 1);
// 大 JSON 工具结果被压缩
assert.strictEqual(result.changed, true);
assert.ok(result.blocksCompressed >= 1, `blocksCompressed=${result.blocksCompressed}`);
assert.strictEqual(result.stashStored, 0, `stashStored=${result.stashStored}`);
assert.ok(result.tokensSaved > 0, `tokensSaved=${result.tokensSaved}`);

// 规则对象数组无损转为 CSV-schema，保留全部行且无需 stash。
const compressedContent = (result.body as any).messages[2].content[0].content as string;
assert.ok(compressedContent.startsWith('[200]{id:int,name:string,status:string}\n'));
assert.ok(compressedContent.includes('199,'), '最后一行必须保留');
assert.ok(!compressedContent.includes('<<stash:'), '无损 schema 不应含 stash 标记');
assert.strictEqual(retrieve('../outside-stash'), null, '非法 stash key 必须被拒绝');

// 长纪要按相关性抽取；有损结果必须能从 stash 逐字恢复。
const oldMeeting = fs.readFileSync(
  path.resolve(__dirname, '../../../crates/sift/tests/fixtures/plain_text_meeting.txt'),
  'utf8',
);
const meetingResult = siftText(oldMeeting, '结论 冻结前缀 CCR 发布');
assert.strictEqual(meetingResult.changed, true);
assert.strictEqual(meetingResult.lossy, true);
assert.ok(meetingResult.stashKey);
assert.ok(meetingResult.text.includes('结论'));
assert.strictEqual(retrieve(meetingResult.stashKey!), oldMeeting);

// 重复纯文本按句段抽取，不为句子级删除伪造整行坐标。
const plainRepeat = '例行同步：当前仍在等待依赖团队确认，处理方式保持不变。\n这是一段重复状态播报，没有新的任务、决定或处理结果。';
const repeatedProse = [
  '周会纪要', plainRepeat, plainRepeat, plainRepeat, plainRepeat,
  'Alice: COMP-100 已完成，失败数为 0。',
  'Bob: COMP-101 未完成，失败数为 2。',
  '结论：修复失败后再发布。',
].join('\n\n');
const proseResult = siftText(repeatedProse, 'COMP-101 结论');
assert.strictEqual(proseResult.lossy, true);
assert.ok(proseResult.stashKey);
assert.ok(proseResult.text.includes('COMP-101'));
assert.ok(proseResult.text.includes('结论'));
assert.ok(!proseResult.text.includes('lines omitted from file'));
assert.strictEqual(retrieve(proseResult.stashKey!), repeatedProse);

// 即使 siftText 没有文件路径，行范围明确的代码压缩也会直接引用 stash，
// 并可只召回对应分片。
const sourceCode = [
  'use std::collections::HashMap;',
  '',
  'fn build() -> usize {',
  ...Array.from({ length: 40 }, (_, i) => `    let value_${i} = ${i};`),
  '    value_39',
  '}',
  '',
].join('\n');
const sourceResult = siftText(sourceCode);
assert.strictEqual(sourceResult.lossy, true, '长代码应走有损压缩');
assert.ok(sourceResult.stashKey);
assert.ok(
  sourceResult.text.includes(
    `// ... 36 lines omitted from file ${JSON.stringify(globalStashFile(sourceResult.stashKey!))}, starting at line 9`,
  ),
);
assert.ok(!sourceResult.text.includes('[sift: omitted'));
assert.ok(!sourceResult.text.includes('retrieveLines'));
const sourceSlice = retrieveLines(sourceResult.stashKey!, 9, 36);
assert.ok(sourceSlice);
assert.strictEqual(sourceSlice!.startLine, 9);
assert.strictEqual(sourceSlice!.lineCount, 36);
assert.strictEqual(sourceSlice!.totalLines, 45);
assert.ok(sourceSlice!.text.startsWith('    let value_5 = 5;\n'));
assert.ok(sourceSlice!.text.endsWith('    value_39\n'));
assert.throws(() => retrieveLines(sourceResult.stashKey!, 0, 1), /startLine/);
assert.throws(() => retrieveLines(sourceResult.stashKey!, 1, 1001), /lineCount/);

// 官网 Java 样例含长 PascalCase/camelCase 标识符，不应被误判为凭证后整块跳过。
const javaDemo = fs.readFileSync(
  path.resolve(__dirname, '../../../crates/sift/tests/fixtures/order_service.java'),
  'utf8',
);
const javaResult = siftText(
  javaDemo,
  'cancel refund 支付',
  'src/main/java/com/example/orders/service/OrderService.java',
);
assert.strictEqual(javaResult.changed, true, '官网 Java 样例应产生压缩结果');
assert.strictEqual(javaResult.lossy, true);
assert.ok(javaResult.text.includes('public class OrderService'));
assert.ok(
  javaResult.text.includes(
    `// ... 26 lines omitted from file ${JSON.stringify(globalStashFile(javaResult.stashKey!))}, starting at line 36`,
  ),
);
assert.ok(!javaResult.text.includes('[sift: omitted'));
assert.ok(!javaResult.text.includes('retrieveLines'));
const javaHints = [...javaResult.text.matchAll(/(\d+) lines omitted from file ".+?", starting at line (\d+)/g)];
assert.strictEqual(javaHints.length, 4, '仅超出完整语句预算的方法应带切片坐标');
const firstLimit = Number(javaHints[0][1]);
const firstOffset = Number(javaHints[0][2]);
const firstSlice = javaDemo.split('\n').slice(firstOffset - 1, firstOffset - 1 + firstLimit);
assert.strictEqual(firstSlice.length, firstLimit);
assert.strictEqual(firstSlice[0].trim(), 'order.setAmount(request.getItems().stream()');
assert.strictEqual(firstSlice[firstSlice.length - 1]?.trim(), 'return saved;');

// sourcePath 扩展名应让所有 8 种 AST grammar 稳定进入源码压缩，避免长函数因
// 代码特征行占比低而被误分发到 plain_text。
const languageCases = [
  ['demo.py', 'def build(x):\n', '    value_$i = x + $i', '    return value_0\n', '#'],
  ['demo.js', 'function build(x) {\n', '  const value_$i = x + $i;', '  return value_0;\n}\n', '//'],
  ['demo.ts', 'function build(x: number): number {\n', '  const value_$i: number = x + $i;', '  return value_0;\n}\n', '//'],
  ['demo.go', 'package main\n\nfunc build(x int) int {\n', '    value$i := x + $i', '    return value0\n}\n', '//'],
  ['demo.rs', 'fn build(x: usize) -> usize {\n', '    let value_$i = x + $i;', '    value_0\n}\n', '//'],
  ['Demo.java', 'public class Demo {\n    public int build(int x) {\n', '        int value$i = x + $i;', '        return value0;\n    }\n}\n', '//'],
  ['demo.c', 'int build(int x) {\n', '    int value$i = x + $i;', '    return value0;\n}\n', '//'],
  ['demo.cpp', 'int build(int x) {\n', '    int value$i = x + $i;', '    return value0;\n}\n', '//'],
] as const;
for (const [file, prefix, bodyLine, suffix, comment] of languageCases) {
  const languageSource =
    prefix +
    Array.from({ length: 35 }, (_, i) => bodyLine.split('$i').join(String(i))).join('\n') +
    '\n' +
    suffix;
  const sourcePath = `/workspace/src/${file}`;
  const result = siftText(languageSource, undefined, sourcePath);
  assert.strictEqual(result.changed, true, `${file} 应进入源码压缩`);
  assert.ok(result.stashKey);
  assert.ok(result.text.includes(bodyLine.split('$i').join('0')), `${file} 应保留开头完整语句`);
  assert.ok(result.text.includes(bodyLine.split('$i').join('4')), `${file} 应保留 5 行默认预算`);
  assert.ok(!result.text.includes(bodyLine.split('$i').join('5')), `${file} 不应超出默认预算`);
  assert.match(
    result.text,
    new RegExp(`${comment.replace('/', '\\/')} \\.\\.\\. \\d+ lines omitted from file`),
    `${file} 缺少内联文件切片提示:\n${result.text}`,
  );
  assert.strictEqual(retrieve(result.stashKey!), languageSource);
}

const pastedTypeScript =
  'function resolveWorkerPath(): string {\n' +
  Array.from(
    { length: 18 },
    (_, i) => `  const candidate_${i} = resolve(process.cwd(), 'worker-${i}.js');`,
  ).join('\n') +
  '\n  return candidate_0;\n}\n';
const pastedTypeScriptResult = siftText(pastedTypeScript);
assert.strictEqual(pastedTypeScriptResult.lossy, true);
assert.ok(pastedTypeScriptResult.stashKey);
assert.ok(
  pastedTypeScriptResult.text.includes(
    `// ... 14 lines omitted from file ${JSON.stringify(globalStashFile(pastedTypeScriptResult.stashKey!))}, starting at line 7`,
  ),
  `无文件路径的 TypeScript 粘贴内容缺少 stash 分片提示:\n${pastedTypeScriptResult.text}`,
);
// 模拟 Agent 的普通文件读取：只使用内联提示，不调用 retrieve/retrieveLines。
const fileHint = /(\d+) lines omitted from file ("(?:\\.|[^"\\])*"), starting at line (\d+)/.exec(
  pastedTypeScriptResult.text,
);
assert.ok(fileHint);
const stashFilePath = JSON.parse(fileHint![2]) as string;
assert.ok(path.isAbsolute(stashFilePath));
const stashFileContent = fs.readFileSync(stashFilePath, 'utf8');
assert.strictEqual(stashFileContent, pastedTypeScript);
const omittedStart = Number(fileHint![3]);
const omittedCount = Number(fileHint![1]);
assert.strictEqual(
  stashFileContent.split('\n').slice(omittedStart - 1, omittedStart - 1 + omittedCount).join('\n'),
  pastedTypeScript.split('\n').slice(6, 20).join('\n'),
);

// 搜索结果的内联坐标属于 stash 文件，而不是 file:line 中的源文件行号。
const searchInput = Array.from({ length: 100 }, (_, i) =>
  `src/worker.ts:${1000 + i * 17}:handler ${i} ${'repeated diagnostic payload '.repeat(5)}`,
).join('\n');
const searchResult = siftText(searchInput);
assert.strictEqual(searchResult.lossy, true);
const searchHints = [...searchResult.text.matchAll(/(\d+) lines omitted from file ("(?:\\.|[^"\\])*"), starting at line (\d+)/g)];
assert.ok(searchHints.length > 0, '无 sourcePath 的搜索结果也应有内联切片提示');
for (const [, count, quotedPath, start] of searchHints) {
  const stashFile = fs.readFileSync(JSON.parse(quotedPath), 'utf8');
  assert.strictEqual(stashFile, searchInput);
  assert.ok(Number(start) + Number(count) - 1 <= 100, '不可使用源文件中的 1000+ 行号');
}

// 日志首命令及中间命令必须直接可见，而非仅能从 stash 恢复。
const buildCommand = '$ cargo build --release --workspace';
const testCommand = '$ cargo test --workspace';
const buildInput = [buildCommand,
  ...Array.from({ length: 80 }, (_, i) => `   Compiling crate_${i} v0.2.${i}`),
  testCommand,
  ...Array.from({ length: 80 }, (_, i) => `INFO: running test case ${i}`),
  'ERROR: test case failed at src/main.rs:52',
].join('\n');
const buildResult = siftText(buildInput);
assert.ok(buildResult.changed);
assert.ok(buildResult.text.startsWith(buildCommand + '\n'), '命令不能被起始 omit 替代');
assert.ok(buildResult.text.includes(testCommand), '中间执行的命令也不能被省略');
assert.ok(buildResult.text.includes('ERROR: test case failed'));
if (buildResult.lossy) {
  assert.strictEqual(fs.readFileSync(globalStashFile(buildResult.stashKey!), 'utf8'), buildInput);
}

// 内容检测
assert.strictEqual(detectContentType('[{"a":1}]'), 'json_array');
assert.strictEqual(detectContentType('plain words'), 'plain_text');
const configInput = `---\n# production settings\nservice:\n  name: api\n  namespace: production\n${Array.from(
  { length: 24 },
  (_, i) => `\n# component ${i} settings\ncomponent_${i}:\n  replicas: 3\n  port: ${8000 + i}`,
).join('\n')}\n`;
assert.strictEqual(detectContentType(configInput), 'structured_config');
const configResult = siftText(configInput);
assert.ok(configResult.changed && configResult.lossy);
assert.ok(configResult.text.includes('component_23:'));
assert.ok(!configResult.text.includes('component 23 settings'));
assert.strictEqual(
  fs.readFileSync(globalStashFile(configResult.stashKey!), 'utf8'),
  configInput,
);
const csvInput = [
  '| id | service | region | status | latency_ms |',
  '| --- | --- | --- | --- | --- |',
  ...Array.from({ length: 120 }, (_, i) =>
    `| ${i} | service-${i} | us-east-1 | ${i === 97 ? 'degraded' : 'healthy'} | ${i === 97 ? 480 : 40 + i} |`,
  ),
].join('\n');
assert.strictEqual(detectContentType(csvInput), 'tabular');
const csvResult = siftText(csvInput, 'degraded latency');
assert.ok(csvResult.changed && csvResult.lossy);
assert.ok(csvResult.text.includes('degraded'));
assert.strictEqual(fs.readFileSync(globalStashFile(csvResult.stashKey!), 'utf8'), csvInput);

// ── OpenAI Chat Completions 格式 ──
const chatBody = {
  model: 'gpt-5',
  messages: [
    { role: 'user', content: 'list the items' },
    {
      role: 'assistant',
      content: null,
      tool_calls: [
        { id: 'c1', type: 'function', function: { name: 'list', arguments: '{}' } },
      ],
    },
    { role: 'tool', tool_call_id: 'c1', content: bigJson },
  ],
};
assert.strictEqual(detectRequestFormat(chatBody), 'chat_completions');
const chatResult = siftRequest(chatBody);
assert.strictEqual(chatResult.frozenMessages, 0, 'OpenAI 格式无冻结前缀');
assert.ok(chatResult.changed, 'tool 消息应被压缩');
const chatToolContent = (chatResult.body as any).messages[2].content as string;
assert.ok(chatToolContent.startsWith('[200]{id:int,name:string,status:string}\n'));
assert.ok(!chatToolContent.includes('<<stash:'), '无损 schema 不应含 stash 标记');

// ── OpenAI Responses API 格式 ──
const responsesBody = {
  model: 'gpt-5',
  input: [
    { role: 'user', content: [{ type: 'input_text', text: 'fetch items' }] },
    { type: 'function_call', call_id: 'c1', name: 'fetch', arguments: '{}' },
    { type: 'function_call_output', call_id: 'c1', output: bigJson },
  ],
};
assert.strictEqual(detectRequestFormat(responsesBody), 'responses');
const responsesResult = siftRequest(responsesBody);
assert.strictEqual(responsesResult.frozenMessages, 0);
assert.ok(responsesResult.changed, 'function_call_output 应被压缩');
const fnOutput = (responsesResult.body as any).input[2].output as string;
assert.ok(fnOutput.startsWith('[200]{id:int,name:string,status:string}\n'));
assert.ok(!fnOutput.includes('<<stash:'), '无损 schema 不应含 stash 标记');
// function_call（模型发出的调用）未被触碰
assert.strictEqual((responsesResult.body as any).input[1].arguments, '{}');

// ── 裸文本压缩 ──
const textResult = siftText(bigJson);
if (textResult.lossy) {
  assert.ok(textResult.stashKey, '有损结果应带 stashKey');
  assert.strictEqual(retrieve(textResult.stashKey!), bigJson);
} else {
  assert.ok(textResult.changed, '大 JSON 应至少无损压缩');
  assert.ok(!textResult.text.includes('<<stash:'), '无损结果不应含标记');
}

// 已含 stash marker 的结果再次进入管线必须幂等，不能形成递归 marker 链。
const repeatedTextResult = siftText(textResult.text);
assert.strictEqual(repeatedTextResult.changed, false);
assert.strictEqual(repeatedTextResult.text, textResult.text);

// system/user/assistant prompt 默认保护；有损压缩只针对工具输出。
const protectedPromptBody = {
  model: 'gpt-5',
  messages: [
    { role: 'system', content: bigJson },
    { role: 'assistant', content: null, tool_calls: [] },
    { role: 'user', content: bigJson },
  ],
};
const protectedPromptResult = siftRequest(protectedPromptBody);
assert.strictEqual(protectedPromptResult.changed, false);
assert.deepStrictEqual(protectedPromptResult.body, protectedPromptBody);
// 小文本透传
assert.strictEqual(siftText('tiny').changed, false);

// Anthropic 格式检测（含 cache_control）
assert.strictEqual(detectRequestFormat(body), 'anthropic');

// createSift 使用调用方指定的独立 stash 目录，不影响顶层默认 store。
const customStashDir = fs.mkdtempSync(path.join(os.tmpdir(), 'sift-custom-stash-'));
try {
  const customSift = createSift({ stashDir: customStashDir });
  const customJson = JSON.stringify(
    Array.from({ length: 200 }, (_, i) => `custom-store diagnostic value ${i % 8}`),
  );
  const { siftText: customSiftText } = customSift;
  const customResult = customSiftText(customJson);
  assert.strictEqual(customResult.lossy, true, '自定义 store 用例应走有损压缩');
  assert.ok(customResult.stashKey, '自定义 store 的有损结果应带 stashKey');
  assert.strictEqual(customSift.retrieve(customResult.stashKey!), customJson);
  assert.strictEqual(customSift.retrieveLines(customResult.stashKey!, 1, 1)?.text, customJson);
  assert.strictEqual(
    retrieve(customResult.stashKey!),
    null,
    '顶层 retrieve 不应读取 createSift 的独立 store',
  );
  assert.strictEqual(
    fs.existsSync(path.join(customStashDir, customResult.stashKey!)),
    true,
    '原文应写入 createSift 指定的目录',
  );
  assert.strictEqual(customSift.detectContentType('[{"a":1}]'), 'json_array');
  assert.strictEqual(customSift.detectRequestFormat(chatBody), 'chat_completions');
} finally {
  fs.rmSync(customStashDir, { recursive: true, force: true });
}

assert.throws(() => createSift({ stashDir: '   ' }), /stashDir/);

console.log('✓ @agent-context/sift smoke test passed');
