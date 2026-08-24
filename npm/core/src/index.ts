// 加载 Rust cdylib。构建脚本（napi build --platform）产出带平台后缀的
// `native/sift.<platform-triple>.node`，按 __dirname 解析，编译后
// dist/index.js 也能找到。
import * as path from 'path';

/** napi 平台三元组（如 `darwin-arm64`、`linux-x64-gnu`）。 */
function platformTriple(): string {
  const { platform, arch } = process;
  if (platform === 'darwin') return `darwin-${arch}`;
  if (platform === 'win32') return `win32-${arch}-msvc`;
  if (platform === 'linux') {
    const libc = detectLibc();
    return `linux-${arch}-${libc}`;
  }
  throw new Error(`@agent-context/sift: 不支持的平台 ${platform}`);
}

/** 区分 glibc / musl（仅 Linux 生效）。 */
function detectLibc(): 'gnu' | 'musl' {
  const report = (process as any).report?.getReport?.();
  if (report?.header?.glibcVersionRuntime) return 'gnu';
  return 'musl';
}

function loadNative(): NativeModule {
  // 加载顺序：
  // 1. 平台子包 @agent-context/sift-<platform>（npm 安装后由 optionalDependencies 命中）
  // 2. 本地 native/sift.<platform>.node（仓库内开发模式）
  // 3. native/sift.node（napi build 未加 --platform 的产物）
  const platform = platformTriple();
  const nativeDirs = [
    path.join(__dirname, '..', 'native'), // dist/index.js 布局
    path.join(__dirname, '..', '..', 'native'), // dist-demo/src、dist-test 布局
  ];
  const candidates: Array<() => NativeModule> = [
    () => require(`@agent-context/sift-${platform}`) as NativeModule,
    ...nativeDirs.map(
      (dir) => () => require(path.join(dir, `sift.${platform}.node`)) as NativeModule,
    ),
    ...nativeDirs.map((dir) => () => require(path.join(dir, 'sift.node')) as NativeModule),
  ];
  for (const load of candidates) {
    try {
      return load();
    } catch {
      // 尝试下一个候选
    }
  }
  throw new Error(
    `@agent-context/sift: 未找到 ${platform} 的原生模块（子包 @agent-context/sift-${platform} 或本地 native/）。先运行 npm run build。`,
  );
}

const native = loadNative();

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
  /** 写入 stash store 的原文条数 */
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
  /** 是否有损（原文已写入 stash store，可用 retrieve 取回） */
  lossy: boolean;
  /** 有损时的取回 key */
  stashKey: string | null;
  /** 估算节省的 token 数 */
  tokensSaved: number;
}

export type ContentType =
  | 'json_array'
  | 'build_output'
  | 'search_results'
  | 'git_diff'
  | 'source_code'
  | 'plain_text'
  | 'html';

interface NativeModule {
  SiftInstance: NativeSiftConstructor;
  siftRequest(body: object, query?: string): CompressResult;
  siftText(text: string, query?: string): TextCompressResult;
  retrieve(key: string): string | null;
  detectContentType(text: string): ContentType;
  detectRequestFormat(body: object): RequestFormat;
}

interface NativeSiftConstructor {
  new (stashDir: string): NativeSiftInstance;
}

interface NativeSiftInstance {
  siftRequest(body: object, query?: string): CompressResult;
  siftText(text: string, query?: string): TextCompressResult;
  retrieve(key: string): string | null;
}

/** `createSift` 的实例配置。 */
export interface SiftOptions {
  /** 该实例专用的 stash 落盘目录；相对路径按调用时的 cwd 解析。 */
  stashDir: string;
}

/** 绑定到独立 stash store 的压缩 API。 */
export interface Sift {
  siftRequest(body: object, query?: string): CompressResult;
  siftText(text: string, query?: string): TextCompressResult;
  retrieve(key: string): string | null;
  detectContentType(text: string): ContentType;
  detectRequestFormat(body: object): RequestFormat;
}

/**
 * 创建使用独立 stash 目录的压缩实例。
 * 顶层 `siftRequest` / `siftText` / `retrieve` 仍使用环境变量或默认目录。
 */
export function createSift(options: SiftOptions): Sift {
  if (!options || typeof options.stashDir !== 'string' || options.stashDir.trim() === '') {
    throw new TypeError('createSift: stashDir 必须是非空字符串');
  }
  const stashDir = path.resolve(options.stashDir);
  const instance = new native.SiftInstance(stashDir);
  return {
    siftRequest(body: object, query?: string): CompressResult {
      const r = instance.siftRequest(body, query);
      return { ...r, stashStored: r.stashStored ?? 0 };
    },
    siftText(text: string, query?: string): TextCompressResult {
      const r = instance.siftText(text, query);
      return { ...r, stashKey: r.stashKey ?? null };
    },
    retrieve(key: string): string | null {
      return instance.retrieve(key);
    },
    detectContentType(text: string): ContentType {
      return native.detectContentType(text);
    },
    detectRequestFormat(body: object): RequestFormat {
      return native.detectRequestFormat(body);
    },
  };
}

/**
 * 压缩请求 body（就地透传或压缩）。自动检测格式：
 * Anthropic /v1/messages、OpenAI Chat Completions、OpenAI Responses API。
 * 默认只压缩工具输出；system/user/assistant prompt 保持不变。
 */
export function siftRequest(body: object, query?: string): CompressResult {
  const r = native.siftRequest(body, query);
  // napi 对 Option::None 的字段会整个省略,这里补齐为 null,保证字段形状稳定
  return { ...r, stashStored: r.stashStored ?? 0 };
}

/** 压缩单个字符串（如把工具输出原文送进任意 API 之前）。 */
export function siftText(text: string, query?: string): TextCompressResult {
  const r = native.siftText(text, query);
  // napi 对 Option::None 的字段会整个省略,这里补齐为 null,保证字段形状稳定
  return { ...r, stashKey: r.stashKey ?? null };
}

/** 按取回标记 key 取回压缩时卸载的原文。 */
export function retrieve(key: string): string | null {
  return native.retrieve(key);
}

/** 内容类型检测（压缩分发键）。 */
export function detectContentType(text: string): ContentType {
  return native.detectContentType(text);
}

/** 请求体格式检测。 */
export function detectRequestFormat(body: object): RequestFormat {
  return native.detectRequestFormat(body);
}
