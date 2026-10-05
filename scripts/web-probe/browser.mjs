// browser.mjs: what capture.mjs and drive.mjs share — serve a site directory
// on localhost (WebGPU needs a secure context) and open it in headless
// Chromium with WebGPU on SwiftShader.
//
// Playwright is found as `playwright` on the module path, or at
// $PLAYWRIGHT_MODULE (a package directory) when it is installed elsewhere.
import { createRequire } from 'module';
import http from 'http';
import fs from 'fs';
import path from 'path';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json' };

/// Open `page` (a file in `root`) at `width` x `height`. Resolves to
/// { page, logs, close }; `logs` collects the page's console and errors.
export async function open(root, file, width, height) {
  const server = http.createServer((q, r) => {
    const p = decodeURIComponent(q.url.split('?')[0]);
    const f = path.join(root, p === '/' ? file : p);
    fs.readFile(f, (e, d) => {
      if (e) { r.writeHead(404); r.end(); return; }
      r.writeHead(200, { 'content-type': types[path.extname(f)] || 'application/octet-stream' });
      r.end(d);
    });
  });
  await new Promise(res => server.listen(0, '127.0.0.1', res));
  // SwiftShader for WebGPU, and for the compositor too: a headless shell with
  // GPU compositing has no shared-image backing for a WebGPU canvas, and the
  // device is lost on the first present ("A valid external Instance reference
  // no longer exists").
  const browser = await chromium.launch({ args: [
    '--enable-unsafe-webgpu', '--use-webgpu-adapter=swiftshader', '--enable-features=Vulkan',
    '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--disable-gpu-compositing',
  ] });
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: Number(process.env.PROBE_DPR || 1) });
  const logs = [];
  page.on('console', m => logs.push(`[${m.type()}] ${m.text()}`));
  page.on('pageerror', e => logs.push(`[pageerror] ${e}`));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  return { page, logs, close: async () => { await browser.close(); server.close(); } };
}
