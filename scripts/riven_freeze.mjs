// SPDX-License-Identifier: AGPL-3.0-or-later
// FREEZE A SURVEY'S SHAPES — docs/AGENT.md §"Riven survey". Each code's search
// is built by a CLEAN page (`/appraise/<code>?freeze=1`, 81-appraisal.js) and
// stored through `/api/appraise/<code>/request`, as the chat's bot does. One
// page answering code after code is NOT clean: its earlier rivens and weapon
// leak into the next search, which is the search every volunteer then runs.
//
//   node scripts/riven_freeze.mjs <codes file> [lanes] [--dry]
// The bot's token is BOT_RELAY_TOKEN, or private/qq/relay.token. `--dry`
// prints each frozen search instead of storing it.
import { openApp } from "./cdp.mjs";
import { readFileSync } from "node:fs";

const SITE = process.env.WFSIM_SITE || "https://wfsim.app";
const args = process.argv.slice(2).filter((a) => a !== "--dry");
const dry = process.argv.includes("--dry");
const codes = readFileSync(args[0], "utf8").split(/\s+/).filter(Boolean);
const lanes = Math.max(1, Math.min(Number(args[1] || 12), codes.length));
const token = dry ? "" : (process.env.BOT_RELAY_TOKEN
  || readFileSync(new URL("../private/qq/relay.token", import.meta.url), "utf8")).trim();

let next = 0, ok = 0, bad = 0;
const t0 = Date.now();
const elapsed = () => `${((Date.now() - t0) / 1000).toFixed(0)}s`;

async function freezeOne(app, code) {
  await app.send("Page.navigate", { url: `${SITE}/appraise/${code}?freeze=1` });
  let got = "";
  for (let i = 0; i < 600 && !got; i++) {
    await app.sleep(100);
    try { got = await app.evaluate(`document.body && document.body.dataset.request || ""`); } catch { got = ""; }
  }
  if (!got) throw new Error("timed out");
  const r = JSON.parse(got);
  if (r.error) throw new Error(r.error);
  if (dry) { console.log(code, got); return; }
  const res = await fetch(`${SITE}/api/appraise/${code}/request`, { method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` }, body: got });
  if (!res.ok) throw new Error(`store ${res.status} ${await res.text()}`);
}

async function lane(id) {
  const app = await openApp({ base: SITE, profile: `wfsim-riven-freeze-${id}` });
  try {
    // NO PICTURES: a search is built from data, and they are most of a load.
    await app.send("Network.enable");
    await app.send("Network.setBlockedURLs", { urls: ["*.png", "*.webp", "*.jpg", "*.woff2"] });
    while (next < codes.length) {
      const code = codes[next++];
      try { await freezeOne(app, code); ok++; } catch (e) { bad++; console.log("FAIL", code, e.message); }
      if ((ok + bad) % 100 === 0) console.log(`${ok + bad}/${codes.length} ok ${ok} bad ${bad} ${elapsed()}`);
    }
  } finally {
    await app.close();
  }
}

await Promise.all(Array.from({ length: lanes }, (_, i) => lane(i)));
console.log(`DONE ok ${ok} bad ${bad} in ${elapsed()}`);
process.exitCode = bad ? 1 : 0;
