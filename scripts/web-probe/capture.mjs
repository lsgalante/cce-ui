// capture.mjs <site-dir> <out.rgba>: serve <site-dir> on localhost (WebGPU
// needs a secure context), open it in headless Chromium with WebGPU on
// SwiftShader, run the probe, and write the frame it read back from the GPU
// to <out.rgba> (premultiplied RGBA8), with a page screenshot beside it.
//
// Playwright is found as `playwright` on the module path, or at
// $PLAYWRIGHT_MODULE (a package directory) when it is installed elsewhere.
import { createRequire } from 'module';
import http from 'http';
import fs from 'fs';
import path from 'path';

const [root, out] = process.argv.slice(2);
if (!root || !out) { console.error('usage: capture.mjs <site-dir> <out.rgba>'); process.exit(2); }
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json', '.ttf': 'font/ttf' };
const server = http.createServer((q, r) => {
  const p = decodeURIComponent(q.url.split('?')[0]);
  const f = path.join(root, p === '/' ? 'index.html' : p);
  fs.readFile(f, (e, d) => {
    if (e) { r.writeHead(404); r.end(); return; }
    r.writeHead(200, { 'content-type': types[path.extname(f)] || 'application/octet-stream' });
    r.end(d);
  });
});
await new Promise(res => server.listen(0, '127.0.0.1', res));
const port = server.address().port;

// SwiftShader for WebGPU, and for the compositor too: a headless shell with
// GPU compositing has no shared-image backing for a WebGPU canvas, and the
// device is lost on the first present ("A valid external Instance reference
// no longer exists").
const browser = await chromium.launch({ args: [
  '--enable-unsafe-webgpu', '--use-webgpu-adapter=swiftshader', '--enable-features=Vulkan',
  '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--disable-gpu-compositing',
] });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const logs = [];
page.on('console', m => logs.push(`[${m.type()}] ${m.text()}`));
page.on('pageerror', e => logs.push(`[pageerror] ${e}`));
await page.goto(`http://127.0.0.1:${port}/`);
await page.waitForFunction(() => window.probeReady === true);
let ok = true;
try {
  const b64 = await page.evaluate(() => window.runProbe());
  fs.writeFileSync(out, Buffer.from(b64, 'base64'));
  await page.screenshot({ path: out + '.screenshot.png' });
} catch (e) { ok = false; logs.push('[probe] ' + String(e)); }
for (const l of logs) console.log(l);
await browser.close();
server.close();
process.exit(ok ? 0 : 1);
