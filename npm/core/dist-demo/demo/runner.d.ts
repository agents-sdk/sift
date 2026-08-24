import type { DemoCase } from './types';
export interface DemoResult {
    output: string;
    contentType: string;
    beforeBytes: number;
    afterBytes: number;
    compressionRatio: number;
    tokensSaved: number;
    blocksExamined: number;
    blocksCompressed: number;
    blocksReverted: number;
    frozenMessages: number;
    stashStored: number;
    stashKey?: string;
}
/** 把一次真实运行保存为便于并排查看的 Markdown。 */
export declare function renderResultMarkdown(demo: DemoCase, result: DemoResult): string;
/** 完整打印一个场景的原文、压缩后文本和验证指标。 */
export declare function runCase(demo: DemoCase, index: number, total: number): DemoResult;
