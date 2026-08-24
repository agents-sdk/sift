"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.gitDiffCase = void 0;
const lines = [];
for (let file = 0; file < 8; file++) {
    lines.push(`diff --git a/src/file${file}.rs b/src/file${file}.rs`);
    lines.push(`--- a/src/file${file}.rs`, `+++ b/src/file${file}.rs`);
    for (let hunk = 0; hunk < 6; hunk++) {
        lines.push(`@@ -${file * 100 + hunk * 10},7 +${file * 100 + hunk * 10},7 @@ fn region_${hunk}`);
        for (let line = 0; line < 7; line++) {
            if (line === 3) {
                lines.push(`-    old_line_${file}_${hunk}`, `+    new_line_${file}_${hunk}`);
            }
            else {
                lines.push(` context_${file}_${hunk}_${line}`);
            }
        }
    }
}
exports.gitDiffCase = {
    id: 'git-diff',
    title: '多文件 git diff',
    description: '保留文件头和关键 hunk，抽稀上下文行，完整 diff 写入 stash。',
    input: lines.join('\n'),
    query: 'new_line',
    expectedType: 'git_diff',
    expectedPath: 'lossy-stash',
    mustContain: ['diff --git', '+    new_line_'],
};
