"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
/**
 * @compressor/core 原文与压缩结果逐例对照。
 *
 * 运行全部：npm run demo
 * 运行单个：npm run demo -- json-array
 * 保存结果：npm run demo -- --save
 * 查看列表：npm run demo -- --list
 */
const fs = __importStar(require("node:fs"));
const path = __importStar(require("node:path"));
const _01_json_array_1 = require("./cases/01-json-array");
const _02_pretty_json_1 = require("./cases/02-pretty-json");
const _03_build_log_1 = require("./cases/03-build-log");
const _04_search_results_1 = require("./cases/04-search-results");
const _05_git_diff_1 = require("./cases/05-git-diff");
const _06_mixed_output_1 = require("./cases/06-mixed-output");
const _07_source_code_1 = require("./cases/07-source-code");
const _08_plain_text_1 = require("./cases/08-plain-text");
const runner_1 = require("./runner");
const cases = [
    _01_json_array_1.jsonArrayCase,
    _02_pretty_json_1.prettyJsonCase,
    _03_build_log_1.buildLogCase,
    _04_search_results_1.searchResultsCase,
    _05_git_diff_1.gitDiffCase,
    _06_mixed_output_1.mixedOutputCase,
    _07_source_code_1.sourceCodeCase,
    _08_plain_text_1.plainTextCase,
];
const args = process.argv.slice(2);
const shouldSave = args.includes('--save');
const selector = args.find((arg) => arg !== '--save');
if (selector === '--list') {
    console.log('可运行的 demo：');
    for (const demo of cases)
        console.log(`  ${demo.id.padEnd(16)} ${demo.title}`);
}
else {
    const selected = selector ? cases.filter((demo) => demo.id === selector) : cases;
    if (selected.length === 0) {
        console.error(`未知 demo: ${selector}`);
        console.error(`可选值: ${cases.map((demo) => demo.id).join(', ')}`);
        process.exitCode = 1;
    }
    else {
        console.log('@compressor/core：原文与压缩结果逐例对照');
        console.log(`CCR 临时目录: ${process.env.COMPRESSOR_CCR_DIR}`);
        const completed = [];
        selected.forEach((demo, index) => {
            const result = (0, runner_1.runCase)(demo, index + 1, selected.length);
            const caseNumber = String(cases.indexOf(demo) + 1).padStart(2, '0');
            completed.push({ demo, result, fileName: `${caseNumber}-${demo.id}.md` });
        });
        if (shouldSave) {
            const resultsDir = path.resolve(__dirname, '..', '..', 'demo', 'results');
            fs.mkdirSync(resultsDir, { recursive: true });
            for (const item of completed) {
                fs.writeFileSync(path.join(resultsDir, item.fileName), (0, runner_1.renderResultMarkdown)(item.demo, item.result));
            }
            const indexLines = [
                '# @compressor/core demo 运行结果',
                '',
                '以下文件由 `npm run demo -- --save` 通过 npm 包公开入口实际运行生成。',
                '',
                '| 示例 | 类型 | 原文字节 | 压缩后字节 | 压缩后占比 | 节省 token | CCR |',
                '|---|---|---:|---:|---:|---:|---|',
                ...completed.map(({ demo, result, fileName }) => `| [${demo.title}](./${fileName}) | ${result.contentType} | ${result.beforeBytes} | ` +
                    `${result.afterBytes} | ${(result.compressionRatio * 100).toFixed(1)}% | ` +
                    `${result.tokensSaved} | ${result.ccrKey ? 'PASS' : '—'} |`),
                '',
            ];
            fs.writeFileSync(path.join(resultsDir, 'README.md'), indexLines.join('\n'));
            console.log(`\n结果已保存到: ${resultsDir}`);
        }
        console.log(`\n${selected.length} 个示例全部验证通过。`);
    }
}
