import type { DemoCase } from '../types';

const lines: string[] = [];
for (let file = 0; file < 4; file++) {
  lines.push(`diff --git a/src/file${file}.rs b/src/file${file}.rs`);
  lines.push(`--- a/src/file${file}.rs`, `+++ b/src/file${file}.rs`);
  for (let hunk = 0; hunk < 4; hunk++) {
    const start = file * 1000 + hunk * 100 + 1;
    lines.push(
      `@@ -${start},17 +${start},17 @@ fn update_region_${hunk}`,
    );
    for (let line = 0; line < 8; line++) {
      lines.push(
        `     let item_${file}_${hunk}_${line} = load_item(${line}); ` +
          `// unchanged setup context for region ${hunk}`,
      );
    }
    lines.push(
      `-    let timeout_${file}_${hunk} = Duration::from_secs(30);`,
      `+    let timeout_${file}_${hunk} = Duration::from_secs(45);`,
    );
    for (let line = 8; line < 16; line++) {
      lines.push(
        `     assert!(item_${file}_${hunk}_${line - 8}.is_ready()); ` +
          `// unchanged verification context for region ${hunk}`,
      );
    }
  }
}

export const gitDiffCase: DemoCase = {
  id: 'git-diff',
  title: '多文件长上下文 git diff',
  description: '保留文件头、变更行及邻近上下文，省略连续的远端上下文并将完整 diff 写入 stash。',
  input: lines.join('\n'),
  query: 'timeout',
  expectedType: 'git_diff',
  expectedPath: 'lossy-stash',
  mustContain: ['diff --git', '+    let timeout_'],
};
