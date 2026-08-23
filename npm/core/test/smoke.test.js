'use strict';

const assert = require('assert');
const { compress, detectContentType } = require('..');

const body = {
  messages: [
    {
      role: 'user',
      content: [
        { type: 'text', text: 'cached system stuff', cache_control: { type: 'ephemeral' } },
      ],
    },
    { role: 'assistant', content: 'old answer' },
    { role: 'user', content: [{ type: 'text', text: 'latest question' }] },
  ],
};

const result = compress(body);
assert.strictEqual(result.changed, false); // 骨架阶段：透传
assert.strictEqual(result.frozenMessages, 1);
assert.strictEqual(result.blocksExamined, 1);
assert.deepStrictEqual(result.body, body);

assert.strictEqual(detectContentType('[{"a":1}]'), 'json_array');
assert.strictEqual(detectContentType('plain words'), 'plain_text');

console.log('✓ @compressor/core smoke test passed');
