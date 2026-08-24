export interface CompressResult {
    /** 压缩后的请求 body（格式与输入一致） */
    body: object;
    /** 是否发生了实际压缩 */
    changed: boolean;
    /** 检查过的 text block 数 */
    blocksExamined: number;
    /** 实际压缩的 block 数 */
    blocksCompressed: number;
    /** 因 token 校验未通过而回退的 block 数 */
    blocksReverted: number;
    /** 冻结前缀消息条数（cache 锚点，未被触碰；OpenAI 格式恒为 0） */
    frozenMessages: number;
    /** 写入 CCR store 的原文条数 */
    stashStored: number;
    /** 估算节省的 token 数 */
    tokensSaved: number;
}
/** 请求体格式。 */
export type RequestFormat = 'anthropic' | 'chat_completions' | 'responses' | 'unknown';
/** 裸文本（如工具输出原文）的压缩结果。 */
export interface TextCompressResult {
    /** 压缩后的文本（有损时尾部带 `<<stash:KEY>>` 标记） */
    text: string;
    /** 是否发生了实际压缩 */
    changed: boolean;
    /** 是否有损（原文已写入 CCR store，可用 retrieve 取回） */
    lossy: boolean;
    /** 有损时的取回 key */
    stashKey: string | null;
    /** 估算节省的 token 数 */
    tokensSaved: number;
}
export type ContentType = 'json_array' | 'build_output' | 'search_results' | 'git_diff' | 'source_code' | 'plain_text' | 'html';
/**
 * 压缩请求 body（就地透传或压缩）。自动检测格式：
 * Anthropic /v1/messages、OpenAI Chat Completions、OpenAI Responses API。
 */
export declare function siftRequest(body: object, query?: string): CompressResult;
/** 压缩单个字符串（如把工具输出原文送进任意 API 之前）。 */
export declare function siftText(text: string, query?: string): TextCompressResult;
/** 按取回标记 key 取回压缩时卸载的原文。 */
export declare function retrieve(key: string): string | null;
/** 内容类型检测（压缩分发键）。 */
export declare function detectContentType(text: string): ContentType;
/** 请求体格式检测。 */
export declare function detectRequestFormat(body: object): RequestFormat;
