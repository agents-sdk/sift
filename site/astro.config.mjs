// Vercel adapter:静态页面走 CDN,/api/compress 走 serverless function。
// includeFiles:把 vendored 原生二进制打进 function 包(napi .node 不在依赖图里,需显式带上)。
import { defineConfig } from 'astro/config';
import react from '@astrojs/react';
import vercel from '@astrojs/vercel';

export default defineConfig({
  output: 'server',
  adapter: vercel({
    includeFiles: [
      './vendor/compressor-core/dist/index.js',
      './vendor/compressor-core/native/compressor.linux-x64-gnu.node',
      './vendor/compressor-core/native/compressor.darwin-arm64.node',
    ],
  }),
  integrations: [react()],
  vite: {
    ssr: {
      // napi 动态加载 native 路径,保持 external,勿打包
      external: ['@compressor/core'],
    },
  },
});
