import type { ContentType } from '../src/index';
/** 一个可独立查看的 npm 包演示场景。 */
export interface DemoCase {
    /** 命令行选择器，例如 `npm run demo -- json-array`。 */
    id: string;
    title: string;
    description: string;
    input: string;
    query?: string;
    expectedType: ContentType;
    expectedPath: 'lossy-stash' | 'changed' | 'unchanged';
    /** 压缩结果中必须保留的关键文本。 */
    mustContain?: string[];
    /** 额外验证；抛出异常即表示示例失败。 */
    verify?: (output: string) => void;
}
