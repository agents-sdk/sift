// 预生成官网演示样本:调本地 @agent-context/sift 压缩 6 类真实感输入,
// 产出 site/src/data/samples.json(离线运行,站点本身零后端依赖)。
//
// 用法:node scripts/gen-demo-samples.mjs  (或 cd site && npm run gen:samples)
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { isAbsolute } from 'node:path';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { siftText, detectContentType } from '../site/vendor/sift/dist/index.js';

const here = dirname(fileURLToPath(import.meta.url));

// ---------- 样本构造 ----------

function jsonArraySample() {
  const rows = [];
  const statuses = ['ok', 'ok', 'ok', 'degraded', 'ok', 'error'];
  for (let i = 1; i <= 48; i++) {
    rows.push({
      id: `svc-${String(i).padStart(3, '0')}`,
      name: `service-${i}`,
      region: ['us-east-1', 'us-west-2', 'eu-central-1', 'ap-southeast-1'][i % 4],
      status: statuses[i % statuses.length],
      latency_ms: 12 + ((i * 37) % 480),
      error_rate: (i % 7) * 0.15,
      replicas: 2 + (i % 5),
      version: `2.${i % 9}.${i % 13}`,
      last_deploy: `2026-08-${String(1 + (i % 21)).padStart(2, '0')}T0${i % 10}:15:00Z`,
      owner: ['team-alpha', 'team-beta', 'team-gamma'][i % 3],
    });
  }
  return JSON.stringify(rows, null, 2);
}

function buildOutputSample() {
  const lines = [
    '$ cargo build --release --workspace',
    '   Compiling libc v0.2.155',
    '   Compiling proc-macro2 v1.0.86',
    '   Compiling unicode-ident v1.0.12',
    '   Compiling quote v1.0.36',
    '   Compiling syn v2.0.68',
  ];
  for (let i = 0; i < 30; i++) {
    const crate = ['serde', 'serde_json', 'tokio', 'rayon', 'regex', 'anyhow', 'thiserror'][i % 7];
    lines.push(`   Compiling ${crate} v${1 + (i % 4)}.${i % 20}.${i % 9}`);
    if (i % 6 === 5) {
      lines.push(`warning: unused import: \`std::collections::HashMap\``);
      lines.push(` --> crates/sift/src/relevance.rs:${10 + i}:${5 + i}`);
      lines.push('  |');
      lines.push(`${10 + i} | use std::collections::HashMap;`);
      lines.push('  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^');
      lines.push('  |');
      lines.push('  = note: `#[warn(unused_imports)]` on by default');
      lines.push('  = help: remove this import');
    }
  }
  for (let i = 0; i < 12; i++) {
    lines.push(`warning: field \`legacy_${i}\` is never read`);
    lines.push(` --> crates/sift/src/policy.rs:${40 + i * 3}:9`);
    lines.push('  |');
    lines.push(`${40 + i * 3} |     legacy_${i}: Option<String>,`);
    lines.push('  |     ^^^^^^^^^^^^');
    lines.push('  |');
    lines.push('  = note: `#[warn(dead_code)]` on by default');
  }
  lines.push('   Compiling sift v0.4.2 (crates/sift)');
  lines.push('error[E0308]: mismatched types');
  lines.push('   --> crates/sift/src/tokenizer.rs:128:23');
  lines.push('    |');
  lines.push('128 |     let n: u32 = text.chars().count() as u64;');
  lines.push('    |                   ^^^^^^^^^^^^^^^^^^^^^^^^^ an `as` cast can silently truncate');
  lines.push('    |');
  lines.push('help: try: `text.chars().count().try_into().unwrap()`');
  lines.push('');
  lines.push('error: could not compile `sift` (bin "sift") due to 1 previous error');
  return lines.join('\n');
}

function searchResultsSample() {
  const files = [
    'crates/sift/src/transforms/mod.rs',
    'crates/sift/src/transforms/smart_crusher.rs',
    'crates/sift/src/transforms/log_compressor.rs',
    'crates/sift/src/stash.rs',
    'crates/sift/src/relevance.rs',
    'npm/core/src/index.ts',
    '.agents/PROJECT_MAP.md',
  ];
  const terms = ['siftText', 'StashStore', 'ReformatTransform', 'frozen', 'tokensSaved'];
  const lines = ['$ rg -n "siftText|StashStore|frozen" crates/ npm/ .agents/'];
  for (let i = 0; i < 60; i++) {
    const f = files[i % files.length];
    const t = terms[(i / 7) % terms.length | 0];
    lines.push(`${f}:${20 + i * 13}:${t} ${(t + ' ').repeat(i % 5)}related handler ${i}`);
  }
  return lines.join('\n');
}

function diffSample() {
  const hunks = [];
  for (let h = 0; h < 10; h++) {
    const start = 30 + h * 40;
    hunks.push(`@@ -${start},${12 + h} +${start},${10 + h} @@ fn transform_${h}(input: &str) -> String {`);
    for (let i = 0; i < 8 + h; i++) {
      hunks.push(` context line ${h}.${i} of the function body that stays the same`);
    }
    hunks.push('-    let old_impl = format!("{}-legacy", input);');
    hunks.push('-    return old_impl;');
    hunks.push('+    let new_impl = crush(input);');
    hunks.push('+    new_impl');
  }
  return ['diff --git a/src/lib.rs b/src/lib.rs', 'index 3f2a9c1..b7e4d02 100644', '--- a/src/lib.rs', '+++ b/src/lib.rs', ...hunks].join('\n');
}

// Java Spring Service:AST 感知压缩(imports/类/方法签名保留,长方法体折叠)
function sourceCodeSample() {
  return readFileSync(
    new URL('../crates/sift/tests/fixtures/order_service.java', import.meta.url),
    'utf8',
  ).replace(/\n$/, '');
}

function plainTextSample() {
  const broadcast = '例行同步：当前仍在等待依赖团队确认，处理方式保持不变。\n这是一段重复状态播报，没有新的任务、决定或处理结果。';
  return [
    '周会纪要：上下文压缩项目（2026-08-18）',
    '## 项目进展',
    'Alice: COMP-100 已完成，冻结前缀的边界测试通过。\nAlice: 这项任务可以关闭，但不代表其他任务已经完成。',
    broadcast, broadcast, broadcast, broadcast,
    'Bob: COMP-101 未完成，恢复测试有 2 项失败。\nBob: 需要先修复失败用例，不能按已完成处理。',
    'Carol: COMP-102 已完成，恢复测试有 0 项失败。\nCarol: 和 COMP-101 是不同任务，需要分别保留记录。',
    '## 发布决定',
    '结论：冻结前缀算法保持不变；等 COMP-101 修复并验收后再发布。',
  ].join('\n\n');
}

// ---------- 元数据 ----------

const SAMPLES = [
  { type: 'json_array', label: 'JSON 数组', query: 'error degraded service status', desc: '结构化监控数据(schema 去重 + 采样,关键行保留)', make: jsonArraySample },
  { type: 'build_output', label: '构建日志', query: 'error mismatched types could not compile', desc: 'cargo/npm 构建输出(错误与堆栈保留,重复 warning 折叠)', make: buildOutputSample },
  { type: 'search_results', label: '搜索结果', query: 'siftText StashStore frozen', desc: 'grep / ripgrep 输出(重复行抽稀,匹配项保留)', make: searchResultsSample },
  { type: 'git_diff', label: 'Git Diff', query: 'new_impl crush', desc: 'unified diff(hunk 采样,改动行保留)', make: diffSample },
  { type: 'source_code', label: 'Java 源码', query: 'cancel refund 支付', sourcePath: 'src/main/java/com/example/orders/service/OrderService.java', desc: 'AST 感知代码压缩(imports/签名/类型保留,长方法体折叠)', make: sourceCodeSample },
  { type: 'plain_text', label: '纯文本', query: '', desc: '完整文本块去重：4 次相同播报保留首份，不同任务、状态和结论完整保留', make: plainTextSample },
];

// ---------- 生成 ----------

// 在生成环节直接按提示读文件并回填切片，防止官网静态示例落后于实际运行时。
function validateLineHints(sample, original, result) {
  if (!result.lossy || sample.type === 'json_array') return;
  const hintRE = /(\d+) lines omitted from file ("(?:\\.|[^"\\])*"), starting at line (\d+)/g;
  const hints = [...result.text.matchAll(hintRE)];
  assert.ok(hints.length, `${sample.label}: 有损样例应显示内联行提示`);
  const rows = original.split(/\r?\n/);
  if (original.endsWith('\n')) rows.pop();
  for (const [, count, quotedPath, start] of hints) {
    const file = JSON.parse(quotedPath);
    assert.ok(isAbsolute(file));
    assert.equal(readFileSync(file, 'utf8'), original, '提示必须指向完整 stash 原文');
    assert.ok(Number(start) >= 1 && Number(count) > 0 && Number(start) - 1 + Number(count) <= rows.length);
  }
  if (sample.type === 'source_code') return; // AST 输出可调整注释/缩进，不按全文逐行回放断言。
  const body = result.text.replace(/<<stash:[a-f0-9]+>>$/, '');
  const compressedRows = body.split(/\r?\n/);
  if (body.endsWith('\n')) compressedRows.pop();
  const restored = [];
  for (const row of compressedRows) {
    const match = /^\[\.\.\. (\d+) lines omitted from file ("(?:\\.|[^"\\])*"), starting at line (\d+)\]$/.exec(row);
    if (!match) { restored.push(row); continue; }
    const start = Number(match[3]);
    assert.equal(start, restored.length + 1, '提示必须就地出现');
    restored.push(...rows.slice(start - 1, start - 1 + Number(match[1])));
  }
  assert.deepEqual(restored, rows, `${sample.label}: 切片回填应逐行重建原文`);
}

const results = [];
for (const s of SAMPLES) {
  const original = s.make();
  const detected = detectContentType(original);
  const r = siftText(original, s.query, s.sourcePath);
  if (s.type === 'build_output') {
    assert.ok(r.text.startsWith('$ cargo build --release --workspace\n'), '构建日志必须首先显示执行命令，不能用 omit 取代');
    assert.ok(r.text.includes('error[E0308]: mismatched types'), '构建错误必须可见');
  }
  if (s.type === 'plain_text') {
    assert.equal(detected, 'plain_text');
    assert.ok(r.lossy, '重复完整段落应折叠');
    assert.ok(r.text.startsWith('周会纪要：'));
    assert.equal(r.text.split('例行同步：').length - 1, 1, '相同播报只保留首份');
    for (const fact of ['COMP-100 已完成', 'COMP-101 未完成', '有 2 项失败', 'COMP-102 已完成', '有 0 项失败', '结论：冻结前缀算法保持不变；等 COMP-101 修复并验收后再发布。']) {
      assert.ok(r.text.includes(fact), `独有事实必须可见: ${fact}`);
    }
  }
  validateLineHints(s, original, r);
  results.push({
    type: s.type,
    label: s.label,
    desc: s.desc,
    query: s.query,
    detected,
    changed: r.changed,
    lossy: r.lossy,
    stashKey: r.stashKey,
    tokensSaved: r.tokensSaved,
    original,
    compressed: r.text,
  });
  console.log(
    `${s.label}: detected=${detected} changed=${r.changed} lossy=${r.lossy} saved=${r.tokensSaved} ` +
      `${Buffer.byteLength(original)}B -> ${Buffer.byteLength(r.text)}B`,
  );
}

const outDir = join(here, '..', 'site', 'src', 'data');
mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, 'samples.json'), JSON.stringify(results));
console.log(`\n已写入 ${outDir}/samples.json`);
