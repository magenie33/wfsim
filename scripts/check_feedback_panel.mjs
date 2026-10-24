// FEEDBACK ON THE PAGE (51-feedback-panel.js): a mark on the bar beside the
// bell wherever accounts are offered, on a phone too; signed out its panel is
// the way to sign in, signed in it is one box. A pasted screenshot is a JPEG
// beside it, removable, three at most; Ctrl+Enter or Send sends the words with
// where the reader was and nothing asked of them; sent, the panel shuts and
// the next one starts empty, and a refusal keeps the words.
//   node scripts/check_feedback_panel.mjs [shots=<dir>]   (WFSIM_BASE to test a server)
import { openApp } from "./cdp.mjs";
import { writeFileSync } from "node:fs";
import { join } from "node:path";

const shots = (process.argv.find((a) => a.startsWith("shots=")) || "").slice(6);
const app = await openApp({ boot: 11000, base: process.env.WFSIM_BASE, lang: "zh" });
const { evaluate, check, send, sleep } = app;
const shot = async (name) => {
  if (!shots) return;
  await sleep(300);
  const m = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(shots, `${name}.png`), Buffer.from(m.result.data, "base64"));
};
const page = (body) => evaluate(`(async () => { ${body} })()`);

await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
// THE FIXTURE: accounts offered and nobody signed in; the worker's answer is
// stubbed and every body sent to it kept.
await page(`
  window.__sent = [];
  window.__answer = { ok: true, id: 7 };
  const real = window.fetch;
  window.fetch = (u, o) => String(u).endsWith('/api/feedback')
    ? (window.__sent.push(JSON.parse(o.body)), Promise.resolve(new Response(JSON.stringify(window.__answer))))
    : real(u, o);
  history.pushState({}, '', '/weapons/Rubico_Prime'); await route();
  accountState = { providers: ['google'], account: null, loaded: true };
  renderAccountEntry();
`);
await sleep(1500);

const state = () => page(`
  const bar = document.querySelector('.topbar').getBoundingClientRect();
  const box = document.getElementById('feedback'), b = document.getElementById('feedback-btn').getBoundingClientRect();
  const bell = document.getElementById('rem-bell').getBoundingClientRect();
  const panel = document.getElementById('feedback-panel'), p = panel.getBoundingClientRect();
  return { shown: !box.hidden && b.width > 0, inBar: b.top >= bar.top && b.bottom <= bar.bottom && b.right <= innerWidth,
    besideBell: bell.width === 0 || b.right <= bell.left + 1,
    open: box.classList.contains('open') && p.width > 0, panelInView: p.left >= 0 && p.right <= innerWidth,
    signIn: (panel.querySelector('a[href^="/login"]') || {}).getAttribute ? panel.querySelector('a[href^="/login"]').getAttribute('href') : null,
    box: !!document.getElementById('feedback-text'), text: (document.getElementById('feedback-text') || {}).value,
    shots: document.querySelectorAll('#feedback-shots img').length,
    add: !!document.getElementById('feedback-add') && !document.getElementById('feedback-add').hidden,
    note: (document.getElementById('feedback-note') || {}).hidden === false ? document.getElementById('feedback-note').textContent : '',
    selects: panel.querySelectorAll('select, input[type=radio], input[type=checkbox]').length };
`);
const click = (sel) => page(`document.querySelector('${sel}').click();`);
const paste = () => page(`
  const c = new OffscreenCanvas(1920, 1080);
  const g = c.getContext('2d'); g.fillStyle = '#345'; g.fillRect(0, 0, 1920, 1080); g.fillStyle = '#fc0'; g.fillRect(800, 400, 300, 200);
  const blob = await c.convertToBlob({ type: 'image/png' });
  const dt = new DataTransfer(); dt.items.add(new File([blob], 'shot.png', { type: 'image/png' }));
  document.getElementById('feedback-text').dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
`);

let s = await state();
check("the mark is on the bar, beside the bell", s.shown && s.inBar && s.besideBell, JSON.stringify(s));
await click("#feedback-btn");
s = await state();
check("signed out, the panel is the way to sign in, back to this page", s.open && !s.box && s.signIn === "/login?return=%2Fweapons%2FRubico_Prime", JSON.stringify(s));
await shot("signed-out");
await click("body");
check("a click elsewhere shuts it", !(await state()).open);

await page(`accountState = { ...accountState, account: { username: 'ann' } }; renderAccountEntry();`);
await click("#feedback-btn");
s = await state();
check("signed in, it is one box and nothing to choose", s.open && s.box && s.selects === 0 && s.add, JSON.stringify(s));
await page(`document.getElementById('feedback-text').value = 'Hunter Munitions here reads 42.5%, the game says 40%';`);
await paste();
await sleep(800);
s = await state();
check("a pasted screenshot sits beside the box", s.shots === 1, JSON.stringify(s));
await paste(); await paste();
await sleep(1500);
s = await state();
check("three is the most, and the add button goes", s.shots === 3 && !s.add, JSON.stringify(s));
await shot("signed-in");
await click('[data-fbk-drop="1"]');
s = await state();
check("one is removed from its own cross", s.shots === 2 && s.add && s.open, JSON.stringify(s));

await click("body");
await click("#feedback-btn");
s = await state();
check("the words and the screenshots survive the panel shutting", s.text.startsWith("Hunter") && s.shots === 2, JSON.stringify(s));

await page(`window.__answer = { ok: false, reason: 'rate_limited' };`);
await click("#feedback-send");
await sleep(500);
s = await state();
check("a refusal is said in the panel, and the words stay", s.open && s.note.length > 0 && s.text.startsWith("Hunter"), JSON.stringify(s));

await page(`window.__answer = { ok: true, id: 7 };
  document.getElementById('feedback-text').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, bubbles: true }));`);
await sleep(800);
const sent = await page(`return window.__sent.at(-1);`);
check("Ctrl+Enter sends the words with where the reader was", sent && sent.text.startsWith("Hunter") && sent.route === "/weapons/Rubico_Prime"
  && typeof sent.release === "string" && sent.lang === "zh" && /\/weapons\/Rubico_Prime\?/.test(sent.share || ""), JSON.stringify(sent).slice(0, 300));
check("…and the screenshots as JPEG", sent && sent.images.length === 2 && sent.images.every((u) => u.startsWith("data:image/jpeg;base64,")));
check("…and nothing that asks what it is about", sent && Object.keys(sent).sort().join() === "images,lang,release,route,share,text", sent && Object.keys(sent).join());
s = await state();
check("sent, the panel shuts", !s.open, JSON.stringify(s));
await click("#feedback-btn");
s = await state();
check("…and the next one starts empty", s.text === "" && s.shots === 0, JSON.stringify(s));
await click("body");

await page(`accountState = { providers: [], account: null, loaded: true }; renderAccountEntry();`);
check("where accounts are not offered, there is no mark", !(await state()).shown);
await page(`accountState = { providers: ['google'], account: { username: 'ann' }, loaded: true }; renderAccountEntry();`);

await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 800, deviceScaleFactor: 2, mobile: true });
await sleep(500);
s = await state();
check("on a phone the mark stays on the bar", s.shown && s.inBar, JSON.stringify(s));
await click("#feedback-btn");
s = await state();
check("…and its panel stays on the screen", s.open && s.panelInView, JSON.stringify(s));
const covered = await page(`
  const p = document.getElementById('feedback-panel').getBoundingClientRect();
  const pts = [];
  for (let y = p.top + 8; y < p.bottom - 4; y += 24) for (let x = p.left + 8; x < p.right - 4; x += 40) pts.push([x, y]);
  return pts.filter(([x, y]) => !document.getElementById('feedback-panel').contains(document.elementFromPoint(x, y))).length;
`);
check("…and nothing floating covers it", covered === 0, covered + " points covered");
await shot("phone");

await app.close();
