// The TENNO block, on screen: the fields exist, they change the panel, and
// they survive a share link.
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
const PORT = 9484;
const proc = spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",
  [`--remote-debugging-port=${PORT}`, "--headless=new", "--disable-gpu", "--no-first-run",
    `--user-data-dir=${process.env.TEMP}/wfsim-tenno-check`, "about:blank"], { stdio: "ignore" });
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
  if (r.result?.exceptionDetails) throw new Error(String(r.result.exceptionDetails.exception?.description || "").slice(0, 500));
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

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(t => { if (/Sim/i.test(t.textContent)) t.click(); });
  await sleep(1200);
  const box = document.getElementById('sim-technique');
  const keys = [...box.querySelectorAll('[data-k]')].map(e => e.dataset.k);

  // Equip Primary Bulwark, then give the Warframe armor.
  arcanes = ['primary_bulwark'];
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(2200);
  // Bulwark is NOT a buff card — its value is a Warframe stat, not a stack
  // anyone earns. It lists as a CONDITIONAL, the panel's channel for "this
  // pays and here is what decides it".
  const conds = () => [...document.querySelectorAll('#stats-conditionals .scond')]
    .map(e => e.textContent).join(' | ');
  const before = conds();

  const armor = box.querySelector('[data-k="wf_armor"]');
  armor.value = '1500'; armor.dispatchEvent(new Event('change'));
  await sleep(2500);
  const after = conds();
  return { keys, before: before.replace(/\\s+/g,' ').trim().slice(0,120),
           after: after.replace(/\\s+/g,' ').trim().slice(0,120),
           simArmor: sim.wf_armor, url: await shareUrl() };
})()`);

check("the Tenno block carries every player field",
  ["aiming", "headshot_pct", "invisible", "airborne", "wf_armor", "wf_energy"].every((k) => r.keys.includes(k)),
  r.keys.join(","));
check("typing armor reaches the scenario state", r.simArmor === 1500, String(r.simArmor));
check("no frame: Bulwark says nothing", !/Bulwark/i.test(r.before), r.before || "(no conditionals)");
check("1,500 armor: the panel states Bulwark's +500%", /Bulwark/i.test(r.after) && /500/.test(r.after), r.after || "(no conditionals)");

await evaluate(`(() => { localStorage.clear(); location.href = ${JSON.stringify(r.url)}; })()`);
await sleep(12000);
const got = await evaluate(`(async () => { await new Promise(r=>setTimeout(r,2500)); return { armor: sim.wf_armor }; })()`);
check("the Warframe travels in a share link", got.armor === 1500, String(got.armor));

ws.close(); proc.kill(); srv.close();
console.log(failures ? `\n${failures} failed` : "\nthe Tenno is on the field, and the page knows it");
process.exit(failures ? 1 : 0);
