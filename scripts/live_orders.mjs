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
// work — submits the captured execution round to the factory, which
// refuses whichever client the fact disproves, on that engine alone.
import { writeFileSync, mkdirSync, rmSync } from "node:fs";
import { cpus } from "node:os";
import { join } from "node:path";
import { official, tell, releaseClaims } from "./order_credit.mjs";
import { BOARD_WORK_KINDS } from "../worker/board_kinds.js";
import { witness } from "./wasm_witness.mjs";

const SETTLE_PER_CYCLE = 3;
const [mode, work, arg] = process.argv.slice(2);
const WITNESS = process.env.WITNESS || "official:live";

/// ONE FIGHT MAY NOT HOLD SETTLING: past this the cycle stops, the row is
/// tried again on a later one, and the rows behind it are released meanwhile.
const SETTLE_FIGHT_MS = 15 * 60_000;
/// HALF THE CORES unless WITNESS_LANES says otherwise.
const LATE = new Error("took too long");
const LANES = Number(process.env.WITNESS_LANES) || Math.max(1, Math.floor(cpus().length / 2));

/// WHAT ONLY THE SERVER MAY SETTLE, claimed from the factory a few at a time,
/// fought and submitted with their captured round: only an accepted fact is
/// delivered. The factory credits whom it agrees with and refuses whom it
/// disproves. What did not fight is released for a later cycle.
async function settle() {
  const w = await witness({ lanes: LANES, site: process.env.WITNESS_SITE || join(arg, "witness") });
  const dir = join(work, "settle");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const facts = [], reviews = [], workflows = [], held = [], claims = new Map();
  const capture = tasks => {
    let invalid = false;
    const rows = [];
    for (const { question, task_id, lease } of tasks) {
      if (typeof question !== "string" || !question || !Number.isSafeInteger(task_id) || task_id <= 0
          || typeof lease !== "string" || !lease) { invalid = true; continue; }
      held.push({ question, task_id, lease });
      rows.push({ question, task_id, lease });
    }
    writeFileSync(join(work, "claimed-leases.ndjson"), rows.map(row => JSON.stringify(row) + "\n").join(""), { flag: "a", flush: true });
    if (invalid) throw Error("invalid captured official claim");
  };
  const answer = async (producer, answers) => {
    if (!answers.length) return;
    const { states } = await official("answer", { witness: WITNESS, engine: w.engine, producers: [producer], answers });
    if (!Array.isArray(states) || states.length !== answers.length || states.some(state => !["fact", "spot"].includes(state)))
      throw Error("official results not fully acknowledged");
  };
  let claimed = 0;
  try {
    try {
      const { tasks } = await official("claim", { witness: WITNESS, engine: w.engine, states: ["disputed", "spot", "nondeterministic"],
        producers: [...BOARD_WORK_KINDS,"build_workflow"], capabilities:["fixed-board-input-v1"], max: SETTLE_PER_CYCLE, lease_ms: 2 * SETTLE_FIGHT_MS });
      capture(tasks);
      if (tasks.length < SETTLE_PER_CYCLE) {
        const fallback = await official("claim", { witness: WITNESS, engine: w.engine, states: ["open"], producers: ["board_review","build_workflow"], capabilities:["fixed-board-input-v1"],
          max: SETTLE_PER_CYCLE - tasks.length, lease_ms: 2 * SETTLE_FIGHT_MS });
        capture(fallback.tasks);
        tasks.push(...fallback.tasks);
      }
      claimed = tasks.length;
      for (const t of tasks) {
        const receipt = { question: t.question, task_id: t.task_id, lease: t.lease };
        const d = t.demands[0];
        if (!d) { console.error(`orders: ${t.question.slice(0, 8)} came with no demand — skipped`); continue; }
        const row = d.producer==="build_workflow"
          ? {...d.payload,fight:t.request} : {record:t.request.record,identity:d.payload.identity,ruler:t.request.ruler,mode:t.request.mode};
        const began = new Date();
        let timer;
        const late = new Promise((_, no) => { timer = setTimeout(() => no(LATE), SETTLE_FIGHT_MS); });
        let got = null, stuck = false;
        try { got = await Promise.race([w.row(row), late]); }
        catch (e) { stuck = e === LATE; console.error(`orders: ${row.identity.slice(0, 8)} did not fight — ${e.message}`); }
        finally { clearTimeout(timer); }
        console.error(`orders: computed ${t.state} ${row.identity.slice(0, 8)} ${row.ruler}/${row.mode}${got ? "" : " — no fact"}`);
        // A FIGHT STILL RUNNING holds its lanes: the rows behind it wait for a later cycle.
        if (stuck) break;
        if (!got) continue;
        const done = new Date();
        if (d.producer === "build_workflow") {
          workflows.push({ref:d.ref,...receipt,result:{score:got.score,metric:got.metric},work:got.work});
          continue;
        }
        if (d.producer === "board_review") {
          reviews.push({ ref: d.ref, ...receipt, result: { score: got.score, metric: got.metric }, work: got.work });
          continue;
        }
        claims.set(`${row.identity}|${row.ruler}|${row.mode}`, receipt);
        facts.push({ identity: row.identity, ruler: row.ruler, metric: got.metric, mode: row.mode, measured_by: w.engine || "",
          score: got.score, cost_seconds: (done - began) / 1000, started_at: began.toISOString(), finished_at: done.toISOString(), work: got.work });
      }
    } finally { await w.close(); }
    await answer("build_workflow", workflows);
    await answer("board_review", reviews);
    if (facts.length) {
      writeFileSync(join(dir, "facts.ndjson"), facts.map((f) => `${JSON.stringify(f)}\n`).join(""));
      await tell(facts, w.engine, (what, body) => official(what, { ...body, witness: WITNESS }), claims);
      console.error(`orders: recorded ${facts.length} facts`);
    }
    return claimed;
  } finally { await releaseClaims(held, (what, body) => official(what, { ...body, witness: WITNESS })); }
}

if (mode === "settle") { if (!(await settle())) process.exitCode = 3; }
else if (mode === "rank") { /* nothing is held for ranking: a first result is open at once (worker/tasks.js) */ }
else { console.error("usage: live_orders.mjs settle <work-dir> <bin-dir>"); process.exit(2); }
