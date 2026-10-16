// SPDX-License-Identifier: AGPL-3.0-or-later
// A COMPUTE LANE STAYS SMALL (docs/WASM.md §"Memory"): a fresh wasm instance
// fights the rows of tests/gas/rows.json as a board lane does, and its linear
// memory — which never shrinks, so it is the lane's for life — stays under
// `CEILING_MB`.
//   node scripts/check_lane_memory.mjs pkg=<dir>    a wasm-bindgen output in hand
//   node scripts/check_lane_memory.mjs               site/pkg, by site/release.json
import fs from "node:fs";
import path from "node:path";
import { loadEngine, measureRow } from "./wasm_engine.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/(\w:)/, "$1")), "..");
const arg = (k, d) => { const a = process.argv.find((x) => x.startsWith(`${k}=`)); return a ? a.slice(k.length + 1) : d; };

/// A RATCHET: lower it when a change frees memory, never raise it to make a red
/// run green. MEASURED at 20.2 MB; the weapon catalog holding every file's yaml
/// tree at once made it 50.0 MB, and this check failed on that build.
const CEILING_MB = 21;

const engine = loadEngine({ pkg: arg("pkg", "") });
const rows = JSON.parse(fs.readFileSync(path.join(ROOT, "tests/gas/rows.json"), "utf8"));
let failures = 0;
for (const row of rows) {
  const s = await measureRow([engine], row, { piece: 4 });
  if (s.ok === false) { console.log(`  FAIL  ${row.record.weapon} ${row.ruler} ${row.mode}: ${s.error}`); failures++; }
}
const mb = engine.memoryBytes() / 1048576;
const ok = mb <= CEILING_MB;
console.log(`  ${ok ? "ok " : "FAIL"}  a lane that fought ${rows.length} rows holds ${mb.toFixed(1)} MB (ceiling ${CEILING_MB} MB)`);
if (!ok || failures) process.exit(1);
