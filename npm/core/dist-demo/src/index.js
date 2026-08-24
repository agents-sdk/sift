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
exports.siftRequest = siftRequest;
exports.siftText = siftText;
exports.retrieve = retrieve;
exports.detectContentType = detectContentType;
exports.detectRequestFormat = detectRequestFormat;
// 加载 Rust cdylib。构建脚本（napi build --platform）产出带平台后缀的
// `native/sift.<platform-triple>.node`，按 __dirname 解析，编译后
// dist/index.js 也能找到。
const path = __importStar(require("path"));
/** napi 平台三元组（如 `darwin-arm64`、`linux-x64-gnu`）。 */
function platformTriple() {
    const { platform, arch } = process;
    if (platform === 'darwin')
        return `darwin-${arch}`;
    if (platform === 'win32')
        return `win32-${arch}-msvc`;
    if (platform === 'linux') {
        const libc = detectLibc();
        return `linux-${arch}-${libc}`;
    }
    throw new Error(`@agent-context/sift: 不支持的平台 ${platform}`);
}
/** 区分 glibc / musl（仅 Linux 生效）。 */
function detectLibc() {
    const report = process.report?.getReport?.();
    if (report?.header?.glibcVersionRuntime)
        return 'gnu';
    return 'musl';
}
function loadNative() {
    // 加载顺序：
    // 1. 平台子包 @agent-context/sift-<platform>（npm 安装后由 optionalDependencies 命中）
    // 2. 本地 native/sift.<platform>.node（仓库内开发模式）
    // 3. native/sift.node（napi build 未加 --platform 的产物）
    const platform = platformTriple();
    const nativeDirs = [
        path.join(__dirname, '..', 'native'), // dist/index.js 布局
        path.join(__dirname, '..', '..', 'native'), // dist-demo/src、dist-test 布局
    ];
    const candidates = [
        () => require(`@agent-context/sift-${platform}`),
        ...nativeDirs.map((dir) => () => require(path.join(dir, `sift.${platform}.node`))),
        ...nativeDirs.map((dir) => () => require(path.join(dir, 'sift.node'))),
    ];
    for (const load of candidates) {
        try {
            return load();
        }
        catch {
            // 尝试下一个候选
        }
    }
    throw new Error(`@agent-context/sift: 未找到 ${platform} 的原生模块（子包 @agent-context/sift-${platform} 或本地 native/）。先运行 npm run build。`);
}
const native = loadNative();
/**
 * 压缩请求 body（就地透传或压缩）。自动检测格式：
 * Anthropic /v1/messages、OpenAI Chat Completions、OpenAI Responses API。
 */
function siftRequest(body, query) {
    return native.siftRequest(body, query);
}
/** 压缩单个字符串（如把工具输出原文送进任意 API 之前）。 */
function siftText(text, query) {
    return native.siftText(text, query);
}
/** 按取回标记 key 取回压缩时卸载的原文。 */
function retrieve(key) {
    return native.retrieve(key);
}
/** 内容类型检测（压缩分发键）。 */
function detectContentType(text) {
    return native.detectContentType(text);
}
/** 请求体格式检测。 */
function detectRequestFormat(body) {
    return native.detectRequestFormat(body);
}
