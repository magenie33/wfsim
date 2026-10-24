// SPDX-License-Identifier: AGPL-3.0-or-later
// FREEZE A SURVEY'S SHAPES — docs/AGENT.md §"Riven survey". A code's search is
// `headlessRivenGainRequest` (87-headless.js), the one its appraisal page sends
// and the chat's bot freezes, built on this checkout's engine (`site/pkg`, the
// release `site/release.json` names) and stored through
// `/api/appraise/<code>/request`.
//
//   node scripts/riven_freeze.mjs <codes file> [calls] [--dry]
// The bot's token is BOT_RELAY_TOKEN, or private/qq/relay.token. `--dry`
// prints each frozen search instead of storing it.
import { loadEngine } from "../bot/engine.mjs";
import { readFileSync } from "node:fs";

const SITE = process.env.WFSIM_SITE || "https://wfsim.app";
const args = process.argv.slice(2).filter((a) => a !== "--dry");
const dry = process.argv.includes("--dry");
const codes = readFileSync(args[0], "utf8").split(/\s+/).filter(Boolean);
/// Reads and stores in flight at once: each is one small request to the worker.
const calls = Math.max(1, Number(args[1] || 16));
const token = dry ? "" : (process.env.BOT_RELAY_TOKEN
  || readFileSync(new URL("../private/qq/relay.token", import.meta.url), "utf8")).trim();
const { engine } = JSON.parse(readFileSync(new URL("../site/release.json", import.meta.url), "utf8"));
const { meta, headless } = await loadEngine();

let ok = 0, bad = 0;
const t0 = Date.now();
const elapsed = () => `${((Date.now() - t0) / 1000).toFixed(0)}s`;

/// `fn` over `items`, `n` at a time.
async function pool(items, n, fn) {
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(n, items.length) }, async () => {
    while (next < items.length) await fn(items[next++]);
  }));
}

await pool(codes, calls, async (code) => {
  try {
    const r = await fetch(`${SITE}/api/appraise/${code}`);
    const job = r.ok && await r.json();
    if (!job || !job.ok) throw new Error(`read ${r.status}`);
    const asked = headless.headlessRivenGainRequest(meta, { weapon: job.weapon, ruler: job.ruler, card: job.riven || {} });
    if (!asked) throw new Error(`no search for ${job.weapon} under ${job.ruler}`);
    const body = JSON.stringify({ engine, ...asked });
    if (dry) {
      console.log(code, body);
    } else {
      const res = await fetch(`${SITE}/api/appraise/${code}/request`, { method: "POST",
        headers: { "content-type": "application/json", authorization: `Bearer ${token}` }, body });
      if (!res.ok) throw new Error(`store ${res.status} ${await res.text()}`);
    }
    ok++;
  } catch (e) { bad++; console.log("FAIL", code, e.message); }
  if ((ok + bad) % 500 === 0) console.log(`${ok + bad}/${codes.length} ok ${ok} bad ${bad} ${elapsed()}`);
});
console.log(`DONE ok ${ok} bad ${bad} in ${elapsed()}`);
process.exitCode = bad ? 1 : 0;
