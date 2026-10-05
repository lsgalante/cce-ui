// ime.mjs <site-dir> [<out-dir>]: input-method composition through the
// browser shell, in the demo's text box, driven by Chromium's own IME path
// (CDP Input.imeSetComposition / Input.insertText — the events a real IME
// raises in the focused textarea). Each result is read back by copying the
// box's text to the system clipboard. With <out-dir>, the frame mid-
// composition is saved there (ime-composing.rgba).
//   1. a composition shows in the box, and the keyboard sink sits at its caret;
//   2. the commit replaces it with the committed text;
//   3. a composition cancelled leaves the box as it was;
//   4. text with no composition (an emoji panel) is typed;
//   5. plain keys still type.
import fs from 'fs';
import { open } from './browser.mjs';

const [root, out] = process.argv.slice(2);
if (!root) { console.error('usage: ime.mjs <site-dir> [<out-dir>]'); process.exit(2); }
let ok = true;
const check = (what, pass, detail) => {
  ok &&= pass;
  console.log(`${pass ? 'ok  ' : 'FAIL'} ${what}${detail ? ': ' + detail : ''}`);
};
const { page, logs, close } = await open(root, 'demo.html', 1280, 800);
await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
const cdp = await page.context().newCDPSession(page);
const m = page.mouse, k = page.keyboard, wait = ms => page.waitForTimeout(ms);
const chord = async key => { await k.down('Control'); await k.press(key); await k.up('Control'); await wait(300); };
const boxText = async () => {
  await chord('a'); await chord('c');
  const t = await page.evaluate(() => navigator.clipboard.readText());
  await k.press('End'); await wait(200);
  return t;
};
const compose = async (text, at) => {
  await cdp.send('Input.imeSetComposition', { text, selectionStart: at, selectionEnd: at });
  await wait(400);
};
try {
  await m.move(1279, 799);
  await page.waitForFunction(() => window.demoReady === true, null, { timeout: 120000 });
  await wait(3000);
  await m.move(640, 168); await m.down(); await m.up(); await wait(300);
  await k.type('ab'); await k.press('ArrowLeft'); await wait(300);

  await compose('にほ', 1);
  if (out) {
    fs.mkdirSync(out, { recursive: true });
    const b64 = await page.evaluate(() => window.capture());
    fs.writeFileSync(`${out}/ime-composing.rgba`, Buffer.from(b64, 'base64'));
  }
  const at = await page.evaluate(() => {
    const t = document.activeElement;
    return { tag: t.tagName, left: parseFloat(t.style.left), top: parseFloat(t.style.top), value: t.value };
  });
  check('the keyboard sink has the focus and holds the composition', at.tag === 'TEXTAREA' && at.value === 'にほ', JSON.stringify(at));
  // The box (the demo's, at y 155-181) and its caret after "aに".
  check('the sink sits at the caret', at.top > 150 && at.top < 185 && at.left > 40 && at.left < 120, `left ${at.left}, top ${at.top}`);

  await cdp.send('Input.insertText', { text: '日本' }); await wait(400);
  let t = await boxText();
  check('the commit replaces the composition', t === 'a日本b', JSON.stringify(t));

  await compose('か', 1);
  await compose('', 0);
  t = await boxText();
  check('a cancelled composition leaves the box as it was', t === 'a日本b', JSON.stringify(t));

  await cdp.send('Input.insertText', { text: '😀' }); await wait(400);
  t = await boxText();
  check('text with no composition is typed', t === 'a日本b😀', JSON.stringify(t));

  await k.type('xy'); await wait(300);
  t = await boxText();
  check('plain keys still type', t === 'a日本b😀xy', JSON.stringify(t));
} catch (e) { ok = false; logs.push('[ime] ' + String(e)); }
for (const l of logs) if (!l.includes('WebGPU is experimental')) console.log(l);
await close();
process.exit(ok ? 0 : 1);
