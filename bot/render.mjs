// SPDX-License-Identifier: AGPL-3.0-or-later
// THE LONG IMAGE, TAKEN — docs/AGENT.md §"The QQ bot". A headless Chrome, up
// while renders come; each opens the site's card page (45-card-page.js) in
// Chinese, waits for `data-card-ready`, and screenshots that element alone.
// `--no-sandbox`: Ubuntu 26.04 denies the namespaces Chrome's sandbox needs,
// and this browser only ever opens wfsim.app.
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CHROME = process.env.CHROME || "chrome-headless-shell";
/// A CARD TAKES 20–35 s on the bot's server: the page ranks the weapon's whole
/// board before it draws the card. QQ holds a reply open for minutes.
const READY_MS = 60_000;
/// THE SITE'S FILES OUTLIVE A BROWSER: every browser starts with a fresh profile,
/// so without a cache kept apart from it each one fetches the wasm and app.js
/// again. Hashed files never change. A cache serves one browser at a time, so a
/// browser takes the first slot no live one holds.
const CACHE_ROOT = join(tmpdir(), "wfsim-chrome-cache");
const CACHE_BYTES = 256 << 20;
/// A BROWSER IS KEPT ONLY WHILE IT IS USED: closed once no page has been open
/// this long, and retired after this many pages, so a renderer's growth is given
/// back — on a small server a browser kept for days is what the kernel kills.
const IDLE_CLOSE_MS = 300_000;
const PAGES_PER_BROWSER = 50;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/// EACH BROWSER HAS ITS OWN PROFILE AND A PORT THE SYSTEM PICKS: a fixed port
/// let this process drive a browser some earlier run left behind, which nothing
/// then ever closed. Every browser it starts dies with it.
const browsers = new Set();
let chrome = null, starting = null;
const alive = (c) => c.proc.exitCode === null && c.proc.signalCode === null;
const quit = (c) => { clearTimeout(c.idle); if (alive(c)) c.proc.kill(); };
process.once("exit", () => { for (const c of browsers) { quit(c); try { rmSync(c.dir, { recursive: true, force: true }); } catch (_) {} } });
for (const sig of ["SIGINT", "SIGTERM"]) process.once(sig, () => process.exit(128 + (sig === "SIGINT" ? 2 : 15)));

async function launch() {
  if (chrome && !chrome.retired) { chrome.retired = true; if (!chrome.open) quit(chrome); }
  const dir = mkdtempSync(join(tmpdir(), "wfsim-chrome-"));
  const held = new Set([...browsers].map((b) => b.slot));
  let slot = 0;
  while (held.has(slot)) slot++;
  const proc = spawn(CHROME, ["--no-sandbox", "--lang=zh-CN", "--remote-debugging-port=0", `--user-data-dir=${dir}`,
    `--disk-cache-dir=${join(CACHE_ROOT, String(slot))}`, `--disk-cache-size=${CACHE_BYTES}`,
    "--hide-scrollbars", "--disable-gpu", "about:blank"], { stdio: "ignore" });
  proc.unref();
  const c = { proc, dir, slot, base: "", pages: 0, open: 0, idle: null, retired: false };
  browsers.add(c);
  proc.once("exit", () => { browsers.delete(c); rmSync(dir, { recursive: true, force: true }); });
  for (let i = 0; i < 50 && alive(c); i++) {
    try {
      const port = readFileSync(join(dir, "DevToolsActivePort"), "utf8").split("\n")[0].trim();
      if (port && (await fetch(`http://127.0.0.1:${port}/json/version`)).ok) {
        c.base = `127.0.0.1:${port}`;
        return (chrome = c);
      }
    } catch (_) {}
    await sleep(200);
  }
  quit(c);
  throw new Error("chrome did not come up");
}

async function browser() {
  if (chrome && !chrome.retired && alive(chrome)) return chrome;
  if (!starting) starting = launch().finally(() => { starting = null; });
  return starting;
}

function release(c) {
  if (--c.open) return;
  if (c.retired) quit(c);
  else c.idle = setTimeout(() => { c.retired = true; quit(c); }, IDLE_CLOSE_MS).unref();
}

/// A page opened by `open(browser)`, counted against its browser until closed.
async function use(open) {
  const c = await browser();
  clearTimeout(c.idle);
  c.open++;
  if (++c.pages >= PAGES_PER_BROWSER) c.retired = true;
  let p;
  try { p = await open(c); } catch (e) { release(c); throw e; }
  return { send: p.send, close: async () => { try { await p.close(); } finally { release(c); } } };
}

/// One page over the DevTools protocol: `send(method, params)` and close.
function page() {
  return use(async (c) => {
    const t = await (await fetch(`http://${c.base}/json/new?about:blank`, { method: "PUT" })).json();
    return attach(t.webSocketDebuggerUrl, () => fetch(`http://${c.base}/json/close/${t.id}`).catch(() => {}));
  });
}

/// EVERY COMMAND ANSWERS OR FAILS: a renderer the kernel kills under memory
/// pressure never replies, and an unbounded wait stops the bot reading any chat.
const SEND_MS = 30_000;

async function attach(url, closed) {
  const ws = new WebSocket(url);
  await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = no; });
  let seq = 0;
  const waiting = new Map();
  const settle = (id, m, error) => {
    const w = waiting.get(id);
    if (!w) return;
    waiting.delete(id);
    clearTimeout(w.timer);
    if (error) w.no(error); else w.ok(m);
  };
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id) settle(m.id, m);
  };
  ws.onclose = () => { for (const id of [...waiting.keys()]) settle(id, null, new Error("devtools connection closed")); };
  const send = (method, params = {}) => new Promise((ok, no) => {
    const id = ++seq;
    const timer = setTimeout(() => settle(id, null, new Error(`${method}: no answer in ${SEND_MS / 1000} s`)), SEND_MS);
    waiting.set(id, { ok, no, timer });
    ws.send(JSON.stringify({ id, method, params }));
  });
  const close = async () => { ws.close(); await closed(); };
  return { send, close };
}

/// A PAGE'S ANSWER TO `expr`, where a busy page is not a failed one: a card
/// ranks a weapon's whole board on its main thread for longer than one call
/// waits, so an unanswered call is "not yet" until the page's own deadline.
function asker(p, until) {
  return async (expr) => {
    try {
      return ((await p.send("Runtime.evaluate", { expression: expr, returnByValue: true })).result || {}).result?.value;
    } catch (e) {
      if (Date.now() > until || !/no answer in/.test(String(e && e.message))) throw e;
      return undefined;
    }
  };
}

/// The card at `url` as a PNG buffer in the page's language `lang`, or an Error.
export async function renderCard(url, lang = "zh") {
  return (await renderCardWith(url, { lang })).png;
}

/// …and what the page says about it: `{ png, verdict }`, the verdict an
/// appraisal's answer writes to `body[data-verdict]` once its replay has run.
/// `wait` is how long a card may take — a replay is a whole official fight.
export async function renderCardWith(url, { lang = "zh", wait = READY_MS } = {}) {
  const p = await page();
  try {
    await p.send("Emulation.setDeviceMetricsOverride", { width: 760, height: 1200, deviceScaleFactor: 1.5, mobile: false });
    // THE READERS' CLOCK, not the server's: a card's "updated" time is printed
    // in the time zone of the chat it is sent to.
    await p.send("Emulation.setTimezoneOverride", { timezoneId: lang === "zh" ? "Asia/Shanghai" : "UTC" });
    // THE LANGUAGE BEFORE THE PAGE'S FIRST SCRIPT, which reads it once at load;
    // an injected script only runs on a page whose Page domain is enabled.
    await p.send("Page.enable");
    await p.send("Page.addScriptToEvaluateOnNewDocument", { source: `try { localStorage.setItem("wfsim-lang", ${JSON.stringify(lang)}); } catch (_) {}` });
    const until = Date.now() + wait;
    await p.send("Page.navigate", { url });
    const ask = asker(p, until);
    while (!(await ask(`document.body && document.body.dataset.cardReady === "1"`))) {
      if (Date.now() > until) throw new Error(`card not ready: ${url}`);
      await sleep(250);
    }
    const r = await ask(`(() => { const b = document.getElementById("card-page").getBoundingClientRect(); return { x: b.left, y: b.top + scrollY, w: b.width, h: b.height }; })()`);
    const shot = await p.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true,
      clip: { x: r.x, y: r.y, width: r.w, height: r.h, scale: 1 } });
    const verdict = await ask(`document.body.dataset.verdict || ""`);
    return { png: Buffer.from(shot.result.data, "base64"), verdict: verdict ? JSON.parse(verdict) : null };
  } finally {
    await p.close();
  }
}
