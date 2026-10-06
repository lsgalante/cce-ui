// compute.mjs <site-dir>: open the compute probe page (browser.mjs), run
// the shared jobs on WebGPU, and print one line each — the format
// `cargo run --example compute_native` prints, so the two can be diffed.
import { open } from './browser.mjs';

const [root] = process.argv.slice(2);
if (!root) { console.error('usage: compute.mjs <site-dir>'); process.exit(2); }
const { page, logs, close } = await open(root, 'compute.html', 320, 200);
await page.waitForFunction(() => window.probeReady === true);
let ok = true;
try {
  console.log(await page.evaluate(() => window.runJobs()));
} catch (e) { ok = false; logs.push('[probe] ' + String(e)); }
for (const l of logs) console.error(l);
await close();
process.exit(ok ? 0 : 1);
