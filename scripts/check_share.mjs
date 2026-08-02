// A SHARE LINK, end to end, in a browser that has never seen the build.
//
// Exists because this path broke twice in ways a state check could not see.
// Both times the presets landed correctly and `slots` held the right mods —
// and the visitor still stared at an empty page, because what is VISIBLE is
// decided somewhere else. So this asserts the screen, not the variables:
// the builder is shown, the home grid is not, and the stats panel the
// recipient renders is the one the sender saw, character for character.
//
//   node scripts/check_share.mjs        (serves site/, drives headless Chrome)
//
// Exits non-zero on the first failure.
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
    s.writeHead(200, { "content-type": MIME[extname(p)] || "application/octet-stream",
      "cache-control": "no-store" });
    s.end(b);
  } catch {
    s.writeHead(200, { "content-type": "text/html" });
    s.end(await readFile(join(ROOT, "index.html")));
  }
});
await new Promise((r) => srv.listen(0, "127.0.0.1", r));
const BASE = `http://127.0.0.1:${srv.address().port}`;
const PORT = 9481;
const proc = spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",
  [`--remote-debugging-port=${PORT}`, "--headless=new", "--disable-gpu", "--no-first-run",
    `--user-data-dir=${process.env.TEMP}/wfsim-share-check`, "about:blank"], { stdio: "ignore" });
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
  if (r.result?.exceptionDetails) {
    throw new Error(String(r.result.exceptionDetails.exception?.description || "").slice(0, 400));
  }
  return r.result?.result?.value;
};
await send("Page.enable"); await send("Runtime.enable");
await send("Page.navigate", { url: BASE });
await sleep(12000);

let failures = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || detail === undefined ? "" : `  — ${detail}`}`);
  if (!ok) failures++;
};

// ---- the SENDER: a build with a riven and a non-default scenario ---------
const sent = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid/rivens'); route(); await sleep(1800);
  document.querySelector('.cu-new').click(); await sleep(700);
  riven.bonuses[0] = { id: 'damage', roll: 1.05 };
  riven.bonuses[1] = { id: 'critical_damage', roll: 1.0 };
  riven.bonuses[2] = { id: 'multishot', roll: 0.95 };
  riven.malus = { id: 'zoom', roll: 1.0 };
  markRivenDirty(); await sleep(1000);
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(1400);
  ['serration','split_chamber','point_strike','vital_sense','hellfire']
    .forEach((m, i) => { if (modById(m)) { slots[i].mod = m; slots[i].rank = modById(m).max_rank; } });
  slots[6].mod = rivenMods()[0].id;
  arcanes = ['primary_deadhead'];
  evoSel = { 1: 'torid_evo1_incarnon_form', 2: 'torid_final_fusillade' };
  sim.level = 155; sim.steel_path = false; sim.headshot_pct = 40;
  markPresetDirty(); markScenarioDirty(); renderMods(); refreshPanel(); await sleep(2000);
  const panel = ['stats-rows','stats-damage']
    .map(id => (document.getElementById(id) || {}).textContent || '').join(' | ').replace(/\\s+/g,' ').trim();
  return { url: await shareUrl(), panel, mods: slots.map(s => s.mod) };
})()`);
check("a link is produced", !!sent.url, sent.url);
check("the link is under 600 characters", sent.url.length < 600, `${sent.url.length} chars`);

// ---- the RECIPIENT: a real navigation, in a browser with nothing ---------
await evaluate(`(() => { localStorage.clear(); location.href = ${JSON.stringify("__URL__").replace("__URL__", sent.url)}; })()`);
await sleep(12000);

const got = await evaluate(`(async () => {
  const q = (s) => document.querySelector(s);
  await new Promise(r => setTimeout(r, 2500));
  const panel = ['stats-rows','stats-damage']
    .map(id => (document.getElementById(id) || {}).textContent || '').join(' | ').replace(/\\s+/g,' ').trim();
  return {
    search: location.search,
    homeVisible: !q('#home-page').hidden,
    configVisible: !q('.config-page').hidden,
    slotsDrawn: document.querySelectorAll('#mod-slots .slot').length,
    mods: slots.map(s => s.mod),
    rivens: loadPresetList('rivens').map(p => p.name),
    scenarioLevel: sim.level, headshot: sim.headshot_pct,
    activeBuild: activePreset,
    panel,
  };
})()`);

check("the builder is on screen without a refresh", got.configVisible && !got.homeVisible,
  `config=${got.configVisible} home=${got.homeVisible}`);
check("the mod slots are drawn", got.slotsDrawn === 8, `${got.slotsDrawn} slots`);
check("the query is stripped", got.search === "", got.search);
check("the build is the shared one", /\(shared\)/.test(got.activeBuild || ""), got.activeBuild);
check("the riven travelled", got.rivens.length === 1, JSON.stringify(got.rivens));
check("the riven is equipped in its slot", /^riven:/.test(got.mods[6] || ""), got.mods[6]);
check("the scenario travelled", got.scenarioLevel === 155 && got.headshot === 40,
  `level=${got.scenarioLevel} headshot=${got.headshot}`);
check("the panel reproduces the sender's exactly", got.panel === sent.panel,
  got.panel === sent.panel ? "" : `\n    sent: ${sent.panel.slice(0, 140)}\n    got : ${got.panel.slice(0, 140)}`);

console.log(failures ? `\n${failures} failure(s)` : "\na shared link lands whole, on screen, first time");
ws.close(); proc.kill(); srv.close();
process.exit(failures ? 1 : 0);
