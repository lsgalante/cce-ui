// capture-app.mjs <site-dir> <app> <out.rgba> [settle-ms] [entry]: open
// demo.html with ?app=<app> (and &entry=<entry>, the app's start function in
// place of `start`) through browser.mjs, let the app run for settle-ms
// (default 3000), and write the frame `capture()` reads back from the GPU to
// <out.rgba> (premultiplied RGBA8, 1280x800).
import fs from 'fs';
import { open } from './browser.mjs';

const [root, app, out, settle, entry] = process.argv.slice(2);
if (!root || !app || !out) { console.error('usage: capture-app.mjs <site-dir> <app> <out.rgba> [settle-ms]'); process.exit(2); }
const { page, logs, close } = await open(root, 'demo.html', 1280, 800, `?app=${app}` + (entry ? `&entry=${entry}` : ''));
let ok = true;
try {
  await page.waitForFunction(() => window.demoReady === true, null, { timeout: 120000 });
  await page.waitForTimeout(Number(settle || 3000));
  const b64 = await page.evaluate(() => window.capture());
  fs.writeFileSync(out, Buffer.from(b64, 'base64'));
} catch (e) { ok = false; logs.push('[capture] ' + String(e)); }
for (const l of logs) console.log(l);
await close();
process.exit(ok ? 0 : 1);
