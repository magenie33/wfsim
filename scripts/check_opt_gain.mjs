// THE SIXTEENTH CHECK — the optimizer's quick calc, and the one dimension it
// has that the builder's does not: ELEMENT PAIRING.
//
// The builder asks "what if this mod went in THIS slot", so the pairing is
// decided by the slot you opened. The optimizer has no slots, so a mod SET
// with three distinct elements is not one build but THREE — and on the Burston
// Prime the best is several times the worst. Every number the optimizer's chips
// print is therefore a MAXIMUM over pairings, against a baseline measured the
// same way, and this asserts the three things that makes true:
//
//   1. the ladder is on screen, ranked, best first — the swing between
//      pairings is larger than any single mod's, so it is not a footnote;
//   2. a candidate that lands on a DIFFERENT pairing says so on its row. That
//      label is not decoration: adding a fourth element re-pairs the other
//      three, so a mod reading "+90% Electricity" can measure NEGATIVE, and
//      without the label that looks like a bug (the shape of user, 2026-08-02:
//      "why does adding status chance LOWER the damage?");
//   3. a mod that changes no partition stays SILENT — a second mod of an
//      element the build already has POOLS (`ElementalInput::push` merges
//      them), so it adds no pairing and must not claim one. Getting that
//      distinction wrong is what shipped 5669040.
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
const PORT = 9512;
const proc = spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",
  [`--remote-debugging-port=${PORT}`, "--headless=new", "--disable-gpu", "--no-first-run",
    `--user-data-dir=${process.env.TEMP}/wfsim-optgain-check`, "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function cdp(p) {
  for (let i = 0; i < 60; i++) {
    try { const r = await fetch(`http://127.0.0.1:${PORT}${p}`); if (r.ok) return r.json(); } catch {}
    await sleep(250);
  }
  throw new Error("no CDP");
}
const page = (await cdp("/json/list")).find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0; const pend = new Map();
ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); } };
await new Promise((r) => (ws.onopen = r));
const send = (method, params = {}) => new Promise((res) => { const i = ++id; pend.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
await send("Page.enable"); await send("Runtime.enable");
await send("Page.navigate", { url: BASE });
await sleep(11000);

// Burston Prime, because its Incarnon form carries INNATE Heat — so the
// leftover column has something to be right about that no rule written over
// mod ids could have known.
const script = [
  "(async () => {",
  "  const s = (ms) => new Promise(r => setTimeout(r, ms));",
  "  history.pushState({}, '', '/weapons/Burston_Prime/optimizer'); route(); await s(3500);",
  "  ['primed_cryo_rounds','infected_clip','hellfire'].forEach(m => { opt.mods[m] = 'fixed'; });",
  "  const tiers = weaponEvos();",
  "  if (tiers[0]) opt.evos[tiers[0].tier] = { [tiers[0].options[0].id]: 'fixed' };",
  "  renderOptMods(); renderOptTools(); await s(600);",
  "  document.getElementById('opk-gain').click();",
  "  for (let i = 0; i < 300; i++) { await s(1000); if (!optGain.running && optGain.key) break; }",
  "  await s(800);",
  "  const rows = [...document.querySelectorAll('#opt-pairings .pairrow')].map(r => ({",
  "    label: r.querySelector('.pl').innerText.trim(),",
  "    value: Number(r.querySelector('.pv').innerText.trim()),",
  "    best: r.classList.contains('best') }));",
  "  const chip = (id) => { const el = document.querySelector(`#opt-mods .opt .seg[data-m='${id}']`);",
  "    const row = el && el.closest('.opt');",
  "    return row ? { gain: (row.querySelector('.gainchip')||{}).innerText || null,",
  "                   note: (row.querySelector('.pairnote')||{}).innerText || null } : null; };",
  // The other two axes are marked the same way and asked the same question,
  // so they have to answer it too (user, 2026-08-06: "mod/arcane/evo").
  //
  // BY ID, never by the name on screen. This matched `.mn` text against
  // "Forceful Finality" and passed only because that evolution had no Chinese
  // name yet — the page runs in the machine's own language, so the day the
  // string was transcribed (强制终结) the row stopped being found and the check
  // reported the SCAN broken. Same lesson `check_enemies` records about wiki
  // URLs: a display name is not an identifier.
  "  const anyChip = (sel, id) => { const el = document.querySelector(sel + ` .seg[data-e='${id}']`);",
  "    const row = el && el.closest('.opt');",
  "    return row ? ((row.querySelector('.gainchip')||{}).innerText || null) : null; };",
  "  return { done: optGain.done, total: optGain.total, base: optGain.base,",
  "           orders: optGain.orders.length, rows,",
  "           hellfire: chip('hellfire'), infected: chip('infected_clip'),",
  "           storm: chip('stormbringer'), serration: chip('serration'),",
  "           wildfire: chip('wildfire'),",
  "           arcaneScanned: [...document.querySelectorAll('#opt-arcanes .gainchip')].length,",
  "           evoOpen: anyChip('#opt-evos', 'burston_prime_forceful_finality'),",
  "           evoLocked: anyChip('#opt-evos', 'burston_prime_extended_volley') };",
  "})()",
].join("\n");
const r = await send("Runtime.evaluate", { expression: script, awaitPromise: true, returnByValue: true });
const v = r.result?.result?.value;
if (!v) {
  console.log("FAIL  the scan threw:", r.result?.exceptionDetails?.exception?.description?.slice(0, 500));
  ws.close(); proc.kill(); srv.close(); process.exit(1);
}

let bad = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? "ok  " : "FAIL"}  ${name}${ok || detail === undefined ? "" : `  — ${detail}`}`);
  if (!ok) bad++;
};

check("the scan finished every job it planned",
  v.total > 0 && v.done === v.total, `${v.done}/${v.total}`);
check("three distinct elements produce three pairings",
  v.orders === 3 && v.rows.length === 3, `${v.orders} orders, ${v.rows.length} rows`);
// Ranked, best FIRST — the whole reason the ladder is above the list.
check("the ladder is ranked, best first",
  v.rows.length === 3 && v.rows[0].best
    && v.rows[0].value >= v.rows[1].value && v.rows[1].value >= v.rows[2].value,
  JSON.stringify(v.rows.map((x) => x.value)));
// The baseline IS the best pairing, not an arbitrary one. A canonical order
// would have frozen whichever pairing the insertion order produced.
check("the baseline is the best pairing, not just some pairing",
  v.base > 0 && Math.abs(v.base - Math.max(...v.rows.map((x) => x.value))) / v.base < 0.02,
  `base ${v.base} vs rows ${JSON.stringify(v.rows.map((x) => x.value))}`);
check("every mod on screen carries a number",
  ["hellfire", "infected", "storm", "serration", "wildfire"].every((k) => v[k] && v[k].gain),
  JSON.stringify(v));

// A REQUIRED mod is measured by DROPPING it: Toxin out of Cold+Toxin+Heat
// leaves Blast, which is not the reference's pairing, so its row says so.
check("a required mod whose removal re-pairs the build says where it lands",
  !!(v.infected.note && v.infected.note.trim().startsWith("\u21e0")), JSON.stringify(v.infected));
// A POOLED element mod is measured by ADDING it, and a fourth element re-pairs
// the other three — the label is what explains a number the card cannot.
check("a pooled element mod states the pairing it lands on",
  !!(v.storm.note && v.storm.note.trim().startsWith("\u21e2")), JSON.stringify(v.storm));
// ...and the two that change NO partition stay silent. Wildfire is the one
// that matters: a second Heat mod POOLS, so it adds no pairing at all.
check("a non-element mod claims no pairing",
  v.serration.note === null, JSON.stringify(v.serration));
check("a second mod of an element the build already has POOLS, and says nothing",
  v.wildfire.note === null, JSON.stringify(v.wildfire));
// Dropping Heat still leaves the Incarnon form's INNATE Heat, so the pairing
// is unchanged and that row is silent too — the innate is the reason.
check("dropping a mod whose element the WEAPON also carries changes no pairing",
  v.hellfire.note === null, JSON.stringify(v.hellfire));

// ALL THREE AXES, because all three are marked the same way and the question
// asked of them is the same one. Arcanes and evolutions carry no element, so
// they never move a pairing — what they need is simply to be scanned at all.
check("the arcane axis is scanned too",
  v.arcaneScanned > 0, `${v.arcaneScanned} arcane chips`);
check("an evolution the LADDER has opened carries a number",
  !!v.evoOpen, JSON.stringify(v.evoOpen));
// ...and one it has not is left alone. Scanning a locked tier would rank a
// perk the builder will not let you click, measured on a build that cannot
// exist — the rule `check_gain_axes` asserts of the builder's own scan.
check("a tier the ladder has NOT opened is not ranked",
  v.evoLocked === null, JSON.stringify(v.evoLocked));

ws.close(); proc.kill(); srv.close();
console.log(bad ? `\n${bad} failed` : "\nthe optimizer's quick calc names its pairings");
process.exit(bad ? 1 : 0);
