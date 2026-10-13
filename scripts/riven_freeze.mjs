// SPDX-License-Identifier: AGPL-3.0-or-later
// FREEZE A SURVEY'S SHAPES — docs/AGENT.md §"Riven survey". A code's search is
// built by a CLEAN page (`/appraise/<code>?freeze=1`, 81-appraisal.js) and
// stored through `/api/appraise/<code>/request`, as the chat's bot does. One
// page answering code after code is NOT clean: its earlier rivens and weapon
// leak into the next search, which is the search every volunteer then runs.
//
// A CLEAN PAGE IS A WHOLE BOOT, seconds of CPU for one search. So each
// (weapon, ruler) freezes ONE clean page as a template and the rest of its
// codes are written into it (`riven_template.mjs`); one in `CHECK_EVERY` is
// frozen clean too and must be the same search, and on the first that is not,
// that weapon's remaining codes go back to clean pages.
//
//   node scripts/riven_freeze.mjs <codes file> [lanes] [--dry]
// The bot's token is BOT_RELAY_TOKEN, or private/qq/relay.token. `--dry`
// prints each frozen search instead of storing it.
import { openApp } from "./cdp.mjs";
import { withCard, sameSearch } from "./riven_template.mjs";
import { readFileSync } from "node:fs";

const SITE = process.env.WFSIM_SITE || "https://wfsim.app";
const CHECK_EVERY = 100;
/// Stores and reads in flight at once: each is one small request to the worker.
const CALLS = 16;
const args = process.argv.slice(2).filter((a) => a !== "--dry");
const dry = process.argv.includes("--dry");
const codes = readFileSync(args[0], "utf8").split(/\s+/).filter(Boolean);
const lanes = Math.max(1, Number(args[1] || 12));
const token = dry ? "" : (process.env.BOT_RELAY_TOKEN
  || readFileSync(new URL("../private/qq/relay.token", import.meta.url), "utf8")).trim();

let ok = 0, bad = 0;
const t0 = Date.now();
const elapsed = () => `${((Date.now() - t0) / 1000).toFixed(0)}s`;
const progress = () => (ok + bad) % 500 === 0 && console.log(`${ok + bad}/${codes.length} ok ${ok} bad ${bad} ${elapsed()}`);

/// `fn` over `items`, `n` at a time.
async function pool(items, n, fn) {
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(n, items.length) }, async () => {
    while (next < items.length) await fn(items[next++]);
  }));
}

async function store(code, frozen) {
  const body = JSON.stringify(frozen);
  if (dry) { console.log(code, body); return; }
  const res = await fetch(`${SITE}/api/appraise/${code}/request`, { method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` }, body });
  if (!res.ok) throw new Error(`store ${res.status} ${await res.text()}`);
}

// ---- clean pages, a pool of lanes that freeze one code each ---------------------
// AT MOST `lanes` OPEN, however the groups and their checks overlap.
const apps = [], waiting = [];
let opened = 0;
async function cleanPage() {
  if (apps.length) return apps.pop();
  if (opened >= lanes) return new Promise((resolve) => waiting.push(resolve));
  opened++;
  const app = await openApp({ base: SITE, profile: `wfsim-riven-freeze-${Date.now()}-${Math.random().toString(36).slice(2, 6)}` });
  // NO PICTURES: a search is built from data, and they are most of a load.
  await app.send("Network.enable");
  await app.send("Network.setBlockedURLs", { urls: ["*.png", "*.webp", "*.jpg", "*.woff2"] });
  return app;
}
async function freezeClean(code) {
  const app = await cleanPage();
  try {
    await app.send("Page.navigate", { url: `${SITE}/appraise/${code}?freeze=1` });
    let got = "";
    for (let i = 0; i < 600 && !got; i++) {
      await app.sleep(100);
      try { got = await app.evaluate(`document.body && document.body.dataset.request || ""`); } catch { got = ""; }
    }
    if (!got) throw new Error("timed out");
    const r = JSON.parse(got);
    if (r.error) throw new Error(r.error);
    return r;
  } finally {
    if (waiting.length) waiting.shift()(app); else apps.push(app);
  }
}

// ---- each code's weapon, ruler and card ---------------------------------------
const jobs = new Map();
await pool(codes, CALLS, async (code) => {
  try {
    const r = await fetch(`${SITE}/api/appraise/${code}`);
    const j = r.ok && await r.json();
    if (!j || !j.ok) throw new Error(`read ${r.status}`);
    jobs.set(code, j);
  } catch (e) { bad++; console.log("FAIL", code, e.message); }
});
const groups = new Map();
for (const code of codes) {
  const j = jobs.get(code);
  if (!j) continue;
  const k = `${j.weapon} ${j.ruler}`;
  (groups.get(k) || groups.set(k, []).get(k)).push(code);
}

// ---- a template a group, then its checks, then the rest written in -------------
const clean = []; // codes back on clean pages
await pool([...groups.entries()], lanes, async ([k, list]) => {
  let template;
  try {
    template = await freezeClean(list[0]);
    await store(list[0], template);
    ok++;
  } catch (e) {
    bad++; console.log("FAIL", list[0], e.message, "- the group goes to clean pages");
    clean.push(...list.slice(1));
    return;
  }
  const rest = list.slice(1);
  const checks = rest.filter((_, i) => i % CHECK_EVERY === CHECK_EVERY - 1);
  let differs = false;
  await pool(checks, lanes, async (code) => {
    if (differs) return;
    try {
      const want = await freezeClean(code);
      if (!sameSearch(withCard(template, jobs.get(code).riven, want.request.rivens[0].id), want)) {
        differs = true;
        console.log(`MISMATCH ${k}: ${code} written into the template is not its clean search`);
      }
      await store(code, want);
      ok++;
    } catch (e) { bad++; console.log("FAIL", code, e.message); }
  });
  const written = rest.filter((c) => !checks.includes(c));
  if (differs) { clean.push(...written); return; }
  console.log(`${k}: template ${list[0]}, ${checks.length} checked clean, ${written.length} written in`);
  await pool(written, CALLS, async (code) => {
    try { await store(code, withCard(template, jobs.get(code).riven)); ok++; } catch (e) { bad++; console.log("FAIL", code, e.message); }
    progress();
  });
});

await pool(clean, lanes, async (code) => {
  try { await store(code, await freezeClean(code)); ok++; } catch (e) { bad++; console.log("FAIL", code, e.message); }
  progress();
});
await Promise.all(apps.map((a) => a.close()));
console.log(`DONE ok ${ok} bad ${bad}${clean.length ? `, ${clean.length} on clean pages` : ""} in ${elapsed()}`);
process.exitCode = bad ? 1 : 0;
