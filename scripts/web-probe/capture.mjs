// capture.mjs <site-dir> <out.rgba>: open the probe page (browser.mjs), run
// the probe, and write the frame it read back from the GPU to <out.rgba>
// (premultiplied RGBA8), with a page screenshot beside it.
import fs from 'fs';
import { open } from './browser.mjs';

const [root, out] = process.argv.slice(2);
if (!root || !out) { console.error('usage: capture.mjs <site-dir> <out.rgba>'); process.exit(2); }
const { page, logs, close } = await open(root, 'index.html', 1280, 800);
await page.waitForFunction(() => window.probeReady === true);
let ok = true;
try {
  const b64 = await page.evaluate(() => window.runProbe());
  fs.writeFileSync(out, Buffer.from(b64, 'base64'));
  await page.screenshot({ path: out + '.screenshot.png' });
} catch (e) { ok = false; logs.push('[probe] ' + String(e)); }
for (const l of logs) console.log(l);
await close();
process.exit(ok ? 0 : 1);
