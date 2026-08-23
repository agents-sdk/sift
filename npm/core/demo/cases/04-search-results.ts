import type { DemoCase } from '../types';

const lines: string[] = [];
for (let file = 0; file < 12; file++) {
  for (let hit = 0; hit < 12; hit++) {
    lines.push(
      `src/module_${String(file).padStart(2, '0')}.rs:${file * 37 + hit}:` +
      `    let value = compute_thing(context, input_${hit});`,
    );
  }
}

export const searchResultsCase: DemoCase = {
  id: 'search-results',
  title: 'grep / ripgrep 搜索结果',
  description: '按文件和 query 相关性抽稀搜索命中，完整结果可从 CCR 恢复。',
  input: lines.join('\n'),
  query: 'compute_thing',
  expectedType: 'search_results',
  expectedPath: 'lossy-ccr',
  mustContain: ['src/module_00.rs', 'compute_thing'],
};
