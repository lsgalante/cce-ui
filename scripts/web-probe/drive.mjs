// drive.mjs <site-dir> <out-dir>: open the demo page (browser.mjs) and run
// the native harness's input script against it — the same pointer moves,
// presses, wheel notches and keys, with the same waits, and one screenshot
// per step (00-start.rgba … 23-titlebar-band.rgba: premultiplied RGBA8, the
// frame read back from the GPU — compare.py composites it). A native run of the same
// script under headless sway (the pointer parked at the corner, the cursor
// hidden, CCE_UI_MENU_POPUP=0 so the menu is drawn in the window as it is
// here) is what each screenshot is compared with.
import fs from 'fs';
import { open } from './browser.mjs';

const [root, out] = process.argv.slice(2);
if (!root || !out) { console.error('usage: drive.mjs <site-dir> <out-dir>'); process.exit(2); }
fs.mkdirSync(out, { recursive: true });
const { page, logs, close } = await open(root, 'demo.html', 1280, 800);
const m = page.mouse, k = page.keyboard;
const wait = ms => page.waitForTimeout(ms);
// One pointer command, then the 150 ms the native script's `v` waits.
const v = async f => { await f(); await wait(150); };
const shot = async name => {
  await wait(900);
  const b64 = await page.evaluate(() => window.capture());
  fs.writeFileSync(`${out}/${name}.rgba`, Buffer.from(b64, 'base64'));
};
const BTN = { 272: 'left', 273: 'right' };
const click = b => v(async () => { await m.down({ button: BTN[b] }); await m.up({ button: BTN[b] }); });
const abs = (x, y) => v(() => m.move(x, y));

let ok = true;
try {
  await abs(1279, 799);
  await page.waitForFunction(() => window.demoReady === true, null, { timeout: 120000 });
  await wait(5000);
  await shot('00-start');
  await abs(85, 91); await shot('01-hover-button');
  await click(272); await shot('02-click-button');
  await abs(202, 91); await click(272); await shot('03-toggle');
  await abs(332, 91); await click(272); await shot('04-dropdown-open');
  await abs(332, 122); await click(272); await shot('05-dropdown-pick');
  await abs(640, 168); await click(272); await shot('06-textbox-focus');
  await k.type('hello web'); await shot('07-typed');
  await k.down('Control'); await k.press('z'); await k.up('Control'); await shot('08-undo');
  for (let i = 0; i < 3; i++) await k.press('Backspace');
  await shot('09-backspace');
  await abs(900, 130); await v(() => m.down()); await abs(1000, 130); await abs(1100, 131); await v(() => m.up());
  await shot('10-slider-drag');
  await abs(517, 131); await v(() => m.wheel(0, -300)); await shot('11-slider-wheel');
  await abs(600, 500); await click(273); await shot('12-context-menu');
  await abs(640, 525); await shot('13-menu-hover');
  await abs(1200, 750); await click(272); await shot('14-menu-dismiss');
  await abs(600, 500); await click(273); await abs(640, 525); await click(272); await shot('15-menu-run');
  await k.press('Tab'); await shot('16-tab');
  await k.press('Escape'); await shot('17-esc');
  await abs(640, 168); await click(272); await k.type('xyzxyz'); await shot('18-refocus-typed');
  await k.down('Backspace'); await wait(1500); await k.up('Backspace'); await shot('19-hold-backspace');
  await k.type('abcdef'); await k.down('ArrowLeft'); await wait(1500); await k.up('ArrowLeft'); await k.type('Z');
  await shot('20-hold-left-then-type');
  await abs(3, 400); await click(272); await shot('21-csd-left-edge');
  await abs(640, 3); await click(272); await shot('22-csd-top-edge');
  await abs(300, 20); await click(272); await shot('23-titlebar-band');
} catch (e) { ok = false; logs.push('[drive] ' + String(e)); }
for (const l of logs) console.log(l);
await close();
process.exit(ok ? 0 : 1);
