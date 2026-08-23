export interface CompressResult {
    /** 压缩后的 messages body */
    body: object;
    /** 是否发生了实际压缩 */
    changed: boolean;
    /** 检查过的 text block 数 */
    blocksExamined: number;
    /** 实际压缩的 block 数 */
    blocksCompressed: number;
    /** 因 token 校验未通过而回退的 block 数 */
    blocksReverted: number;
    /** 冻结前缀消息条数（cache 锚点，未被触碰） */
    frozenMessages: number;
    /** 写入 CCR store 的原文条数 */
    ccrStored: number;
    /** 估算节省的 token 数 */
    tokensSaved: number;
}
export type ContentType = 'json_array' | 'build_output' | 'search_results' | 'git_diff' | 'source_code' | 'plain_text' | 'html';
/** 压缩一条 Anthropic /v1/messages 风格的请求 body（就地透传或压缩）。 */
export declare function compress(body: object, query?: string): CompressResult;
/** 按取回标记 key 取回压缩时卸载的原文。 */
export declare function retrieve(key: string): string | null;
/** 内容类型检测（压缩分发键）。 */
export declare function detectContentType(text: string): ContentType;
