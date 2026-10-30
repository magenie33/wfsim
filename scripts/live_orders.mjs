// SPDX-License-Identifier: AGPL-3.0-or-later
// THE OFFICIAL SIDE OF THE FACT FACTORY for board rows — docs/BOARD.md §"The
// fact factory". `settle` is run by `settle.yml`.
//
//   node live_orders.mjs settle <work-dir> <bin-dir>   disputed, spot-checked and non-deterministic rows, fought here
//     WITNESS_SITE and WITNESS stand in for the bin dir's engine and
//     `official:live`; exit 3 when there was nothing to claim. A fact records
//     the witness's engine id as `measured_by`, the version a board row states.
//
// THE OFFICIAL MACHINES ONLY BACK THE CLIENTS UP: every result waits for a
// second client, and `settle` fights what only they may settle (worker/tasks.js
// `OFFICIAL_STATES`) as an official witness. It fights on the volunteers' own
// engine — the site's wasm (scripts/wasm_witness.mjs), the only build that counts their
// work — ships the fact through `ship_facts.sh`, and tells the factory, which
// refuses whichever client the fact disproves, on that engine alone.
import { writeFileSync, mkdirSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { cpus } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { official, tell } from "./order_credit.mjs";
import { BOARD_WORK_KINDS } from "../worker/board_kinds.js";
import { witness } from "./wasm_witness.mjs";

const SETTLE_PER_CYCLE = 3;
const HERE = dirname(fileURLToPath(import.meta.url));
const [mode, work, arg] = process.argv.slice(2);
const WITNESS = process.env.WITNESS || "official:live";

/// ONE FIGHT MAY NOT HOLD SETTLING: past this the cycle stops, the row is
/// tried again on a later one, and the rows behind it are released meanwhile.
const SETTLE_FIGHT_MS = 15 * 60_000;
/// HALF THE CORES unless WITNESS_LANES says otherwise.
const LATE = new Error("took too long");
const LANES = Number(process.env.WITNESS_LANES) || Math.max(1, Math.floor(cpus().length / 2));

/// WHAT ONLY THE SERVER MAY SETTLE, claimed from the factory a few at a time,
/// fought, shipped to `scores` and told: the factory credits whom the fact
/// agrees with and refuses whom it disproves. What did not fight is released
/// for a later cycle.
async function settle() {
  const w = await witness({ lanes: LANES, site: process.env.WITNESS_SITE || join(arg, "witness") });
  const dir = join(work, "settle");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const facts = [], reviews = [];
  let claimed = 0;
  try {
    const { tasks } = await official("claim", { witness: WITNESS, engine: w.engine, states: ["disputed", "spot", "nondeterministic"],
      producers: BOARD_WORK_KINDS, max: SETTLE_PER_CYCLE, lease_ms: 2 * SETTLE_FIGHT_MS });
    if (tasks.length < SETTLE_PER_CYCLE) {
      const fallback = await official("claim", { witness: WITNESS, engine: w.engine, states: ["open"], producers: ["board_review"],
        max: SETTLE_PER_CYCLE - tasks.length, lease_ms: 2 * SETTLE_FIGHT_MS });
      tasks.push(...fallback.tasks);
    }
    claimed = tasks.length;
    for (const t of tasks) {
      const d = t.demands[0];
      if (!d) { console.error(`orders: ${t.question.slice(0, 8)} came with no demand — skipped`); continue; }
      const row = { record: t.request.record, identity: d.payload.identity, ruler: t.request.ruler, mode: t.request.mode };
      const began = new Date();
      let timer;
      const late = new Promise((_, no) => { timer = setTimeout(() => no(LATE), SETTLE_FIGHT_MS); });
      let got = null, stuck = false;
      try { got = await Promise.race([w.row(row), late]); }
      catch (e) { stuck = e === LATE; console.error(`orders: ${row.identity.slice(0, 8)} did not fight — ${e.message}`); }
      finally { clearTimeout(timer); }
      console.error(`orders: settled ${t.state} ${row.identity.slice(0, 8)} ${row.ruler}/${row.mode}${got ? "" : " — no fact"}`);
      // A FIGHT STILL RUNNING holds its lanes: the rows behind it wait for a later cycle.
      if (stuck) break;
      if (!got) continue;
      const done = new Date();
      if (d.producer === "board_review") {
        reviews.push({ ref: d.ref, result: { score: got.score, metric: got.metric }, work: got.work });
        continue;
      }
      facts.push({ identity: row.identity, ruler: row.ruler, metric: got.metric, mode: row.mode, measured_by: w.engine || "",
        score: got.score, cost_seconds: (done - began) / 1000, started_at: began.toISOString(), finished_at: done.toISOString(), work: got.work });
    }
  } finally { await w.close(); }
  if (reviews.length) await official("answer", { witness: WITNESS, engine: w.engine, producers: ["board_review"], answers: reviews });
  if (facts.length) {
    writeFileSync(join(dir, "facts.ndjson"), facts.map((f) => `${JSON.stringify(f)}\n`).join(""));
    execFileSync("bash", [join(HERE, "ship_facts.sh"), join(dir, "facts.ndjson")], { stdio: ["ignore", "ignore", "inherit"] });
    await tell(facts, w.engine, (what, body) => official(what, { ...body, witness: WITNESS }));
  }
  await official("release", { witness: WITNESS });
  return claimed;
}

if (mode === "settle") { if (!(await settle())) process.exitCode = 3; }
else if (mode === "rank") { /* nothing is held for ranking: a first result is open at once (worker/tasks.js) */ }
else { console.error("usage: live_orders.mjs settle <work-dir> <bin-dir>"); process.exit(2); }
