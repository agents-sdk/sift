#!/usr/bin/env node
/**
 * 生成多平台 npm 子包（等价于 `napi prepublish` 的本地实现）。
 *
 * 输入：npm/core/native/sift.<triple>.node（由 build-cross.sh 产出）
 * 输出：npm/core/platforms/<triple>/
 *   ├── package.json   name=@agent-context/sift-<platform>, files=[*.node, LICENSE, NOTICE]
 *   ├── sift.<triple>.node
 *   ├── LICENSE
 *   └── NOTICE
 *
 * 同时把根 package.json 的 optionalDependencies 更新为各平台子包，
 * npm install 时按当前平台自动装对应子包（装错平台的会被 optional 豁免）。
 *
 * 用法：node scripts/gen-platform-packages.mjs  （在仓库根或 npm/core 下运行均可）
 */
import { execSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pkgDir = path.join(root, 'npm', 'core');
const nativeDir = path.join(pkgDir, 'native');
const platformsDir = path.join(pkgDir, 'platforms');
const pkgJsonPath = path.join(pkgDir, 'package.json');
const licensePath = path.join(root, 'LICENSE');
const noticePath = path.join(root, 'NOTICE');

// rust target triple -> napi 平台名（与 index.ts 的 platformTriple() 一致）
const TRIPLE_TO_PLATFORM = {
  'aarch64-apple-darwin': 'darwin-arm64',
  'x86_64-apple-darwin': 'darwin-x64',
  'x86_64-unknown-linux-gnu': 'linux-x64-gnu',
  'aarch64-unknown-linux-gnu': 'linux-arm64-gnu',
  'x86_64-unknown-linux-musl': 'linux-x64-musl',
  'aarch64-unknown-linux-musl': 'linux-arm64-musl',
  'x86_64-pc-windows-msvc': 'win32-x64-msvc',
  'aarch64-pc-windows-msvc': 'win32-arm64-msvc',
};

const rootPkg = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf8'));
const version = rootPkg.version;
const license = rootPkg.license;

const nodes = fs
  .readdirSync(nativeDir)
  .filter((f) => /^sift\..+\.node$/.test(f));
if (nodes.length === 0) {
  console.error('错误：native/ 下没有 sift.<triple>.node。先运行 npm run build:cross。');
  process.exit(1);
}

fs.rmSync(platformsDir, { recursive: true, force: true });

const optionalDeps = {};
for (const file of nodes) {
  // sift.darwin-arm64.node -> darwin-arm64；带 rust triple 的映射到平台名
  const stem = file.replace(/\.node$/, '');
  const triple = Object.keys(TRIPLE_TO_PLATFORM).find(
    (t) => stem.endsWith(t) || stem === `sift.${TRIPLE_TO_PLATFORM[t]}`,
  );
  const platform = triple
    ? TRIPLE_TO_PLATFORM[triple]
    : stem.replace(/^sift\./, '');
  const dir = path.join(platformsDir, platform);
  fs.mkdirSync(dir, { recursive: true });
  fs.copyFileSync(path.join(nativeDir, file), path.join(dir, file));
  fs.copyFileSync(licensePath, path.join(dir, 'LICENSE'));
  fs.copyFileSync(noticePath, path.join(dir, 'NOTICE'));
  const name = `@agent-context/sift-${platform}`;
  fs.writeFileSync(
    path.join(dir, 'package.json'),
    JSON.stringify(
      {
        name,
        version,
        os: [platform.split('-')[0]],
       cpu: [platform.split('-')[1]],
        main: 'index.js',
        files: [file, 'LICENSE', 'NOTICE'],
        license,
        description: `Native ${platform} binary for @agent-context/sift`,
      },
      null,
      2,
    ) + '\n',
  );
  // 子包入口：直接导出 .node
  fs.writeFileSync(
    path.join(dir, 'index.js'),
    `module.exports = require('./${file}');\n`,
  );
  optionalDeps[name] = version;
  console.log(`✓ ${name}@${version}  (${file})`);
}

// 更新根 package.json 的 optionalDependencies
rootPkg.optionalDependencies = Object.fromEntries(
  Object.entries(optionalDeps).sort(([a], [b]) => a.localeCompare(b)),
);
fs.writeFileSync(pkgJsonPath, JSON.stringify(rootPkg, null, 2) + '\n');
console.log(`\n根包 optionalDependencies 已更新（${Object.keys(optionalDeps).length} 个平台）`);
