// 自定义粘贴内容的压缩入口:Vercel serverless function 调 @agent-context/sift。
// 有损压缩时返回 stashKey——原文留在服务端 CCR store,可通过 retrieve 取回。
import type { APIRoute } from 'astro';
import { siftText, detectContentType } from '@agent-context/sift';

const SOURCE_EXTENSIONS = /\.(?:py|pyi|js|jsx|mjs|cjs|ts|tsx|mts|cts|go|rs|java|c|cc|cpp|cxx|hpp|hh|hxx)$/i;

export const POST: APIRoute = async ({ request }) => {
  let text: string;
  let query: string;
  let sourcePath: string | undefined;
  try {
    const body = await request.json();
    text = typeof body?.text === 'string' ? body.text : '';
    query = typeof body?.query === 'string' ? body.query : '';
    sourcePath = typeof body?.sourcePath === 'string' ? body.sourcePath : undefined;
  } catch {
    return Response.json({ error: '请求体必须是 JSON' }, { status: 400 });
  }
  if (!text.trim()) {
    return Response.json({ error: 'text 不能为空' }, { status: 400 });
  }
  if (text.length > 200_000) {
    return Response.json({ error: 'text 过长(上限 200,000 字符)' }, { status: 413 });
  }
  try {
    const detected = sourcePath && SOURCE_EXTENSIONS.test(sourcePath)
      ? 'source_code'
      : detectContentType(text);
    const r = siftText(text, query, sourcePath);
    return Response.json({
      detected,
      changed: r.changed,
      lossy: r.lossy,
      stashKey: r.stashKey,
      tokensSaved: r.tokensSaved,
      text: r.text,
    });
  } catch (err) {
    return Response.json({ error: `压缩失败:${String(err)}` }, { status: 500 });
  }
};
