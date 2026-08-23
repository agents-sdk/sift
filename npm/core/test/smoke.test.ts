import * as assert from 'node:assert';
import {
  compress,
  compressText,
  retrieve,
  detectContentType,
  detectRequestFormat,
} from '../dist/index';

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

const result = compress(body);

// 冻结前缀未触碰
assert.strictEqual(result.frozenMessages, 1);
// 大 JSON 工具结果被压缩
assert.strictEqual(result.changed, true);
assert.ok(result.blocksCompressed >= 1, `blocksCompressed=${result.blocksCompressed}`);
assert.ok(result.ccrStored >= 1, `ccrStored=${result.ccrStored}`);
assert.ok(result.tokensSaved > 0, `tokensSaved=${result.tokensSaved}`);

// 压缩后的 body 含取回标记，且可通过 retrieve 恢复原文
const compressedContent = (result.body as any).messages[2].content[0].content as string;
assert.ok(compressedContent.includes('<<ccr:'), '应含 <<ccr: 标记');
const key = compressedContent.slice(
  compressedContent.lastIndexOf('<<ccr:') + '<<ccr:'.length,
  compressedContent.length - 2,
);
assert.strictEqual(retrieve(key), bigJson);

// 内容检测
assert.strictEqual(detectContentType('[{"a":1}]'), 'json_array');
assert.strictEqual(detectContentType('plain words'), 'plain_text');

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
    { role: 'user', content: 'summarize' },
  ],
};
assert.strictEqual(detectRequestFormat(chatBody), 'chat_completions');
const chatResult = compress(chatBody);
assert.strictEqual(chatResult.frozenMessages, 0, 'OpenAI 格式无冻结前缀');
assert.ok(chatResult.changed, 'tool 消息应被压缩');
const chatToolContent = (chatResult.body as any).messages[2].content as string;
assert.ok(chatToolContent.includes('<<ccr:'), 'tool content 应含取回标记');
const chatKey = chatToolContent.slice(
  chatToolContent.lastIndexOf('<<ccr:') + '<<ccr:'.length,
  chatToolContent.length - 2,
);
assert.strictEqual(retrieve(chatKey), bigJson);

// ── OpenAI Responses API 格式 ──
const responsesBody = {
  model: 'gpt-5',
  input: [
    { role: 'user', content: [{ type: 'input_text', text: 'fetch items' }] },
    { type: 'function_call', call_id: 'c1', name: 'fetch', arguments: '{}' },
    { type: 'function_call_output', call_id: 'c1', output: bigJson },
    { role: 'user', content: [{ type: 'input_text', text: 'summarize' }] },
  ],
};
assert.strictEqual(detectRequestFormat(responsesBody), 'responses');
const responsesResult = compress(responsesBody);
assert.strictEqual(responsesResult.frozenMessages, 0);
assert.ok(responsesResult.changed, 'function_call_output 应被压缩');
const fnOutput = (responsesResult.body as any).input[2].output as string;
assert.ok(fnOutput.includes('<<ccr:'), 'output 应含取回标记');
const fnKey = fnOutput.slice(
  fnOutput.lastIndexOf('<<ccr:') + '<<ccr:'.length,
  fnOutput.length - 2,
);
assert.strictEqual(retrieve(fnKey), bigJson);
// function_call（模型发出的调用）未被触碰
assert.strictEqual((responsesResult.body as any).input[1].arguments, '{}');

// ── 裸文本压缩 ──
const textResult = compressText(bigJson);
if (textResult.lossy) {
  assert.ok(textResult.ccrKey, '有损结果应带 ccrKey');
  assert.strictEqual(retrieve(textResult.ccrKey!), bigJson);
} else {
  assert.ok(textResult.changed, '大 JSON 应至少无损压缩');
  assert.ok(!textResult.text.includes('<<ccr:'), '无损结果不应含标记');
}
// 小文本透传
assert.strictEqual(compressText('tiny').changed, false);

// Anthropic 格式检测（含 cache_control）
assert.strictEqual(detectRequestFormat(body), 'anthropic');

console.log('✓ @compressor/core smoke test passed');
