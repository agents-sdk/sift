import type { DemoCase } from '../types';

const html = `<!DOCTYPE html>
<html lang="en">
<head>
  <title>Sift HTML Extraction</title>
  <meta name="author" content="Context Team">
  <style>
    body { color: #222; }
    .advertisement { display: block; }
  </style>
  <script>
    window.analyticsToken = "tracking-only-noise";
  </script>
</head>
<body>
  <header>
    <nav><a href="/">Home</a><a href="/pricing">Pricing</a></nav>
  </header>
  <main>
    <article>
      <h1>Compress HTML without losing the article</h1>
      <p>Sift keeps the main explanation &amp; removes page chrome.</p>
      <h2>Important details</h2>
      <ul>
        <li>Article paragraphs remain visible.</li>
        <li>Lists retain their readable structure.</li>
      </ul>
      <pre><code>const result = siftText(html);</code></pre>
      <p>The complete original document remains recoverable from stash.</p>
    </article>
  </main>
  <aside class="advertisement">Buy unrelated products now.</aside>
  <footer>Copyright 2026 Example Corp.</footer>
  <script>sendTrackingBeacon();</script>
</body>
</html>`;

export const htmlCase: DemoCase = {
  id: 'html',
  title: 'HTML 正文提取',
  description: '保留文章正文与结构，移除脚本、样式、导航、广告和页脚。',
  input: html,
  expectedType: 'html',
  expectedPath: 'lossy-stash',
  mustContain: [
    '# Compress HTML without losing the article',
    'Article paragraphs remain visible.',
    'const result = siftText(html);',
  ],
  verify: (output) => {
    for (const noise of ['analyticsToken', 'Buy unrelated products', 'Copyright 2026']) {
      if (output.includes(noise)) throw new Error(`HTML 输出仍包含页面噪声: ${noise}`);
    }
  },
};
