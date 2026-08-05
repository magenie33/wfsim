// The FIFTEENTH check: the page FITS THE SCREEN IT IS ON.
//
// The failure it exists for, reported on a phone (owner, 2026-08-05): the mod
// grid was two columns at every width, and `grid-template-columns:repeat(2,1fr)`
// floors each track at its MIN-CONTENT. A slot's min-content is 198px, so two
// of them plus the gap is 404px inside a 326px column — the right-hand slots
// (2, 4, 6, 8) hung ~55px past the screen edge and their ⋯ button could only be
// reached by panning the page sideways.
//
// Why a check and not a one-line fix left to stand: horizontal overflow is
// INVISIBLE on the machine it is written on. Every desktop width has room, the
// browser silently allows the pan, and nothing in the other fourteen checks
// looks at geometry at all — they assert what the DOM SAYS, and this is a class
// of bug where the DOM says everything correctly and the layout is still wrong.
//
// It asserts two things at each width, for the builder's mod grid and for the
// three preset bars, which are the other wide row on the page:
//   1. nothing sticks out past the viewport, and
//   2. the page does not scroll sideways at all.
// Plus one thing that is not about overflow: that a mod's NAME still has room
// to be a name, because the cheapest way to stop an overflow is to squeeze a
// column to nothing and call it fixed.
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, resolve } from "node:path";

const ROOT = resolve("site");
const MIME = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css",
  ".json": "application/json", ".wasm": "application/wasm", ".svg": "image/svg+xml",
  ".png": "image/png", ".jpg": "image/jpeg", ".ico": "image/x-icon" };
const srv = createServer(async (q, s) => {
  const p = decodeURIComponent(q.url.split("?")[0]);
  try {
    const b = await readFile(join(ROOT, p));
    s.writeHead(200, { "content-type": MIME[extname(p)] || "application/octet-stream", "cache-control": "no-store" });
    s.end(b);
  } catch {
    s.writeHead(200, { "content-type": "text/html" });
    s.end(await readFile(join(ROOT, "index.html")));
  }
});
await new Promise((r) => srv.listen(0, "127.0.0.1", r));
const BASE = `http://127.0.0.1:${srv.address().port}`;
const PORT = 9496;
const proc = spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",
  [`--remote-debugging-port=${PORT}`, "--headless=new", "--disable-gpu", "--no-first-run",
    `--user-data-dir=${process.env.TEMP}/wfsim-mobile-check`, "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function cdp(path) {
  for (let i = 0; i < 60; i++) {
    try { const r = await fetch(`http://127.0.0.1:${PORT}${path}`); if (r.ok) return r.json(); } catch {}
    await sleep(250);
  }
  throw new Error("no CDP");
}
const page = (await cdp("/json/list")).find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
};
await new Promise((r) => (ws.onopen = r));
const send = (method, params = {}) => new Promise((res) => {
  const i = ++id; pending.set(i, res);
  ws.send(JSON.stringify({ id: i, method, params }));
});
const evaluate = async (expr) => {
  const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(String(r.result.exceptionDetails.exception?.description || "").slice(0, 600));
  return r.result?.result?.value;
};
await send("Page.enable"); await send("Runtime.enable");

let failures = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || detail === undefined ? "" : `  — ${detail}`}`);
  if (!ok) failures++;
};

// The narrow end of the range, then a tablet and a desktop so the fix is shown
// not to have cost the wide layouts anything.
const SCREENS = [
  ["iPhone SE", 375, 667, true],
  ["Android", 360, 800, true],
  ["iPhone 14", 390, 844, true],
  ["tablet", 768, 1024, false],
  ["desktop", 1280, 900, false],
];

for (const [label, w, h, mobile] of SCREENS) {
  await send("Emulation.setDeviceMetricsOverride",
    { width: w, height: h, deviceScaleFactor: mobile ? 2 : 1, mobile });
  await send("Page.navigate", { url: BASE });
  await sleep(mobile ? 11000 : 9000);

  const r = await evaluate(`(async () => {
    const sleep = (ms) => new Promise(r => setTimeout(r, ms));
    history.pushState({}, '', '/weapons/Ocucor'); route(); await sleep(2600);
    // A FULL build, because an empty slot is narrow and proves nothing: the
    // overflow came from the content of a filled card.
    const pool = (META.weapons.find(w => w.id === 'ocucor') || {}).mods || [];
    for (let i = 0; i < 8 && i < pool.length; i++) slots[i] = { mod: pool[i], pol: null, rank: null };
    renderMods(); await sleep(1400);

    const vw = document.documentElement.clientWidth;
    // Everything that could stick out, measured where the reader meets it.
    const widest = (sel) => [...document.querySelectorAll(sel)]
      .filter(el => el.getBoundingClientRect().width > 0)
      .reduce((m, el) => Math.max(m, el.getBoundingClientRect().right), 0);
    const names = [...document.querySelectorAll('#mod-slots .slot .mn')]
      .map(el => Math.round(el.getBoundingClientRect().width)).filter(x => x > 0);
    return {
      vw,
      // The page's own sideways scroll — the symptom a reader actually feels.
      scrollW: document.documentElement.scrollWidth,
      slotsRight: Math.round(widest('#mod-slots .slot')),
      barsRight: Math.round(widest('.preset-bar, .pbar, #build-bar, #scenario-bar')),
      cols: getComputedStyle(document.getElementById('mod-slots')).gridTemplateColumns,
      narrowestName: names.length ? Math.min(...names) : 0,
    };
  })()`);

  const tag = `[${label} ${w}px]`;
  check(`${tag} the mod grid stays on screen`, r.slotsRight <= r.vw + 0.5,
    `rightmost slot edge ${r.slotsRight} vs viewport ${r.vw}`);
  check(`${tag} nothing else sticks out either`, r.barsRight <= r.vw + 0.5,
    `rightmost bar edge ${r.barsRight} vs viewport ${r.vw}`);
  check(`${tag} the page does not scroll sideways`, r.scrollW <= r.vw + 0.5,
    `scrollWidth ${r.scrollW} vs clientWidth ${r.vw}`);
  // A column squeezed to nothing is not a fixed layout. 90px is about a dozen
  // characters — enough to tell two mods apart, which is the job.
  check(`${tag} a mod name still has room to be a name`, r.narrowestName >= 90,
    `narrowest name column ${r.narrowestName}px (grid: ${r.cols})`);
}

ws.close(); proc.kill(); srv.close();
console.log(failures ? `\n${failures} FAILED` : "\nthe page fits every screen it was measured on");
process.exit(failures ? 1 : 0);
