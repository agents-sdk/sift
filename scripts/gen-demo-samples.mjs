// 预生成官网演示样本:调本地 @compressor/core 压缩 6 类真实感输入,
// 产出 site/src/data/samples.json(离线运行,站点本身零后端依赖)。
//
// 用法:node scripts/gen-demo-samples.mjs  (或 cd site && npm run gen:samples)
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compressText, detectContentType } from '../site/vendor/compressor-core/dist/index.js';

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
      lines.push(` --> crates/compressor-core/src/relevance.rs:${10 + i}:${5 + i}`);
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
    lines.push(` --> crates/compressor-core/src/policy.rs:${40 + i * 3}:9`);
    lines.push('  |');
    lines.push(`${40 + i * 3} |     legacy_${i}: Option<String>,`);
    lines.push('  |     ^^^^^^^^^^^^');
    lines.push('  |');
    lines.push('  = note: `#[warn(dead_code)]` on by default');
  }
  lines.push('   Compiling compressor-core v0.4.2 (crates/compressor-core)');
  lines.push('error[E0308]: mismatched types');
  lines.push('   --> crates/compressor-core/src/tokenizer.rs:128:23');
  lines.push('    |');
  lines.push('128 |     let n: u32 = text.chars().count() as u64;');
  lines.push('    |                   ^^^^^^^^^^^^^^^^^^^^^^^^^ an `as` cast can silently truncate');
  lines.push('    |');
  lines.push('help: try: `text.chars().count().try_into().unwrap()`');
  lines.push('');
  lines.push('error: could not compile `compressor-core` (bin "compressor-core") due to 1 previous error');
  return lines.join('\n');
}

function searchResultsSample() {
  const files = [
    'crates/compressor-core/src/transforms/mod.rs',
    'crates/compressor-core/src/transforms/smart_crusher.rs',
    'crates/compressor-core/src/transforms/log_compressor.rs',
    'crates/compressor-core/src/ccr.rs',
    'crates/compressor-core/src/relevance.rs',
    'npm/core/src/index.ts',
    'docs/PROJECT_MAP.md',
  ];
  const terms = ['compressText', 'CcrStore', 'ReformatTransform', 'frozen', 'tokensSaved'];
  const lines = ['$ rg -n "compressText|CcrStore|frozen" crates/ npm/ docs/'];
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

// 注:source_code 压缩器当前为 no-op(B4 决策),不纳入演示样本。


function plainTextSample() {
  const speakers = ['Alice', 'Bob', 'Carol', 'Dave'];
  const out = ['周会纪要:上下文压缩项目(2026-08-18)', ''];
  for (let i = 0; i < 26; i++) {
    const s = speakers[i % 4];
    const topic = ['冻结前缀', 'CCR 恢复', 'token 估算', '发布节奏', '文档', '压测'][i % 6];
    out.push(`${s}:关于${topic},我觉得目前方案整体可以,细节上还有几个点需要确认一下,第一是边界条件,第二是回退路径,第三是和缓存成本的折中。`);
    out.push(`${s}:另外上次的 action item 我这边已经完成了,详情见 ticket COMP-${100 + i}。`);
    if (i % 5 === 4) out.push(`${s}:这个问题我们下次会议再深入讨论吧,先把结论记下来:${topic}维持现状。`);
  }
  out.push('', '结论:1) 冻结前缀下界算法保持不变;2) CCR store 换成内存 LRU;3) 下周二发布 0.5.0。');
  return out.join('\n');
}

// ---------- 元数据 ----------

const SAMPLES = [
  { type: 'json_array', label: 'JSON 数组', query: 'error degraded service status', desc: '结构化监控数据(schema 去重 + 采样,关键行保留)', make: jsonArraySample },
  { type: 'build_output', label: '构建日志', query: 'error mismatched types could not compile', desc: 'cargo/npm 构建输出(错误与堆栈保留,重复 warning 折叠)', make: buildOutputSample },
  { type: 'search_results', label: '搜索结果', query: 'compressText CcrStore frozen', desc: 'grep / ripgrep 输出(重复行抽稀,匹配项保留)', make: searchResultsSample },
  { type: 'git_diff', label: 'Git Diff', query: 'new_impl crush', desc: 'unified diff(hunk 采样,改动行保留)', make: diffSample },
  { type: 'plain_text', label: '纯文本', query: '结论 冻结前缀 CCR 发布', desc: '中英文抽取式摘要(BM25 相关性 + 近重复折叠)', make: plainTextSample },
];

// ---------- 生成 ----------

const results = [];
for (const s of SAMPLES) {
  const original = s.make();
  const detected = detectContentType(original);
  const r = compressText(original, s.query);
  results.push({
    type: s.type,
    label: s.label,
    desc: s.desc,
    query: s.query,
    detected,
    changed: r.changed,
    lossy: r.lossy,
    ccrKey: r.ccrKey,
    tokensSaved: r.tokensSaved,
    original,
    compressed: r.text,
  });
  console.log(
    `${s.label}: detected=${detected} changed=${r.changed} lossy=${r.lossy} saved=${r.tokensSaved} ` +
      `${original.length}B -> ${r.text.length}B`,
  );
}

const outDir = join(here, '..', 'site', 'src', 'data');
mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, 'samples.json'), JSON.stringify(results));
console.log(`\n已写入 ${outDir}/samples.json`);
