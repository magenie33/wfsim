// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SCORER'S FACTS, told to the fact factory as the official witness's answers
// — docs/BOARD.md §"The fact factory", §"Contribution". An official answer is a
// fact alone; a client whose bits it reproduces is credited exactly as if a
// second client had agreed, and the factory alone decides that and pays it.
//
//   node order_credit.mjs <facts.ndjson> <engine>   a scorer run's facts, as answers of `official:scorer`
//
// The run reads from `<facts>.credited` and advances only after acknowledgement.
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { BOARD_KINDS } from "../worker/board_kinds.js";

const ORIGIN = process.env.FACTORY_ORIGIN || "https://wfsim.app";
const WITNESS = process.env.WITNESS || "official:scorer";
/// The board's producers, whose demands a fact line answers.
export { BOARD_KINDS };
const PER_CALL = 200;

/// ONE CALL TO THE OFFICIAL WITNESS'S DOOR (worker/factory.js `officialRoute`).
export async function official(what, body) {
  const r = await fetch(`${ORIGIN}/api/tasks/official/${what}`, {
    method: "POST",
    headers: { authorization: `Bearer ${process.env.FACTORY_TOKEN}`, "content-type": "application/json", "user-agent": "wfsim-scorer" },
    body: JSON.stringify({ witness: WITNESS, ...body }),
    signal: AbortSignal.timeout(180000),
  });
  const j = await r.json().catch(() => null);
  if (!r.ok || !j || !j.ok) throw new Error(`factory ${what} ${r.status}: ${JSON.stringify(j)}`);
  return j;
}

/// EVERY FACT LINE AS AN OFFICIAL ANSWER: the row's score, its metric and the
/// work the scorer counted. Returns how many questions it made facts of.
export async function tell(facts, engine, call = official, claims = null) {
  let made = 0;
  for (let i = 0; i < facts.length; i += PER_CALL) {
    const part = facts.slice(i, i + PER_CALL);
    const answers = part.map(f => {
      const ref = `${f.identity}|${f.ruler}|${f.mode}`, receipt = claims?.get(ref);
      if (claims && !receipt) throw Error(`result has no captured claim: ${ref}`);
      if (claims && (!Number.isFinite(f.score) || typeof f.metric !== 'string' || f.measured_by !== engine
          || !Number.isFinite(f.cost_seconds) || f.cost_seconds < 0 || !Number.isFinite(Date.parse(f.started_at))
          || !Number.isFinite(Date.parse(f.finished_at)) || f.started_at > f.finished_at)) throw Error(`invalid official measurement: ${ref}`);
      return { ref, ...(receipt || {}), result: { score: f.score, metric: f.metric,
        ...(claims ? { measurement: { measured_by: f.measured_by, cost_seconds: f.cost_seconds, started_at: f.started_at, finished_at: f.finished_at } } : {}) },
      // A FACT WITHOUT WORK — a native build's — witnesses the score alone.
        work: Number.isSafeInteger(f.work) ? f.work : null };
    });
    const { states } = await call("answer", { engine, producers: BOARD_KINDS, answers });
    if (!Array.isArray(states) || states.length !== part.length || claims && states.some(s => !['fact','spot'].includes(s))) throw Error('official results not fully acknowledged');
    made += states.filter((s) => s === "fact").length;
  }
  return made;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [path, engine, claimsPath] = process.argv.slice(2);
  if (!path || !engine) { console.error("usage: order_credit.mjs <facts.ndjson> <engine> [claimed-calculations.ndjson]"); process.exit(2); }
  if (!process.env.FACTORY_TOKEN) { if (claimsPath) throw Error('FACTORY_TOKEN required'); console.log("credit: no factory token"); process.exit(0); }
  const claims = claimsPath ? new Map(readFileSync(claimsPath,'utf8').split('\n').filter(Boolean).map(line => {
    const { build_id, ruler, mode, question, task_id, lease } = JSON.parse(line);
    if (typeof question !== 'string' || !Number.isSafeInteger(task_id) || task_id <= 0 || typeof lease !== 'string' || !lease) throw Error('invalid captured official claim');
    return [`${build_id}|${ruler}|${mode}`, { question, task_id, lease }];
  })) : null;
  const cursor = `${path}.credited`;
  const from = existsSync(cursor) ? Number(readFileSync(cursor, "utf8")) || 0 : 0;
  const all = existsSync(path) ? readFileSync(path, "utf8").split("\n").filter(Boolean) : [];
  if (all.length > from) {
    const n = await tell(all.slice(from).map((l) => JSON.parse(l)), engine, official, claims);
    writeFileSync(cursor, String(all.length));
    console.log(`credit: ${all.length - from} fact(s) told to the factory, ${n} question(s) settled by them`);
  }
}
