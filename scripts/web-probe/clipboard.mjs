// clipboard.mjs <site-dir>: the clipboard through the browser shell, in the
// demo's text box (src/main.rs on demo_web). Four checks, each read back
// from the system clipboard:
//   1. copy — Ctrl+A, Ctrl+C — puts the box's text there;
//   2. paste — Ctrl+V with other text there — takes it (copied back out);
//   3. copy with the page's writeText refused: the `copy` event alone
//      carries it (what an insecure context has);
//   4. paste with the `paste` event swallowed but its default kept: the text
//      the browser pastes into the keyboard sink is the clipboard's;
//   5. paste with the `paste` event swallowed and cancelled: the held key
//      goes through on its timer and pastes the page's own last copy or
//      paste (4's read-back copy of the box).
import { open } from './browser.mjs';

const [root] = process.argv.slice(2);
if (!root) { console.error('usage: clipboard.mjs <site-dir>'); process.exit(2); }
let ok = true;
const check = (what, got, want) => {
  const pass = got === want;
  ok &&= pass;
  console.log(`${pass ? 'ok  ' : 'FAIL'} ${what}: ${JSON.stringify(got)}${pass ? '' : ` (want ${JSON.stringify(want)})`}`);
};

async function session(body) {
  const { page, logs, close } = await open(root, 'demo.html', 1280, 800);
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  const m = page.mouse, k = page.keyboard, wait = ms => page.waitForTimeout(ms);
  const t = {
    page, wait,
    read: () => page.evaluate(() => navigator.clipboard.readText()),
    write: s => page.evaluate(s => (window.realWrite || (t => navigator.clipboard.writeText(t)))(s), s),
    chord: async key => { await k.down('Control'); await k.press(key); await k.up('Control'); await wait(400); },
    focusBox: async () => { await m.move(640, 168); await m.down(); await m.up(); await wait(300); },
    type: async s => { await k.type(s); await wait(300); },
    key: async s => { await k.press(s); await wait(100); },
  };
  try {
    await m.move(1279, 799);
    await page.waitForFunction(() => window.demoReady === true, null, { timeout: 120000 });
    await wait(3000);
    await body(t);
  } catch (e) { ok = false; logs.push('[clipboard] ' + String(e)); }
  for (const l of logs) if (!l.includes('WebGPU is experimental')) console.log(l);
  await close();
}

await session(async t => {
  await t.write('before');
  await t.focusBox(); await t.type('hello web');
  await t.chord('a'); await t.chord('c');
  check('copy', await t.read(), 'hello web');
  await t.write('from the system');
  await t.chord('a'); await t.chord('v');
  await t.chord('a'); await t.chord('c');
  check('paste', await t.read(), 'from the system');
});

await session(async t => {
  await t.write('before');
  await t.page.evaluate(() => {
    const real = Clipboard.prototype.writeText;
    window.realWrite = s => real.call(navigator.clipboard, s);
    Clipboard.prototype.writeText = () => Promise.reject(new Error('refused'));
  });
  await t.focusBox(); await t.type('via the event');
  await t.chord('a'); await t.chord('c');
  check('copy, the copy event alone', await t.read(), 'via the event');
  // Ahead of the shell's listener: swallow the event, and with
  // `cancelPaste` set cancel its default too.
  await t.page.evaluate(() => window.addEventListener('paste', e => {
    e.stopImmediatePropagation();
    if (window.cancelPaste) e.preventDefault();
  }, true));
  await t.write(' and the system');
  await t.focusBox(); await t.key('End'); await t.chord('v');
  await t.chord('a'); await t.chord('c');
  check('paste, the event swallowed', await t.read(), 'via the event and the system');
  await t.page.evaluate(() => { window.cancelPaste = true; });
  await t.write('system text the page never sees');
  await t.focusBox(); await t.key('End'); await t.chord('v');
  await t.chord('a'); await t.chord('c');
  check('paste, no paste event at all', await t.read(), 'via the event and the systemvia the event and the system');
});

process.exit(ok ? 0 : 1);
