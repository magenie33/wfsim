// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SCORER'S FACTS, told to the fact factory as the official witness's answers
// — docs/BOARD.md §"The fact factory", §"Contribution". An official answer is a
// fact alone; a client whose bits it reproduces is credited exactly as if a
// second client had agreed, and the factory alone decides that and pays it.
//
//   node order_credit.mjs <facts.ndjson> <engine>   a scorer run's facts, as answers of `official:scorer`
//
// The run reads the facts file from its last cursor (`<facts>.credited`), so
// running it beside `ship_facts.sh` every minute tells the factory each fact once.
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
  });
  const j = await r.json().catch(() => null);
  if (!r.ok || !j || !j.ok) throw new Error(`factory ${what} ${r.status}: ${JSON.stringify(j)}`);
  return j;
}

/// EVERY FACT LINE AS AN OFFICIAL ANSWER: the row's score, its metric and the
/// work the scorer counted. Returns how many questions it made facts of.
export async function tell(facts, engine, call = official) {
  let made = 0;
  for (let i = 0; i < facts.length; i += PER_CALL) {
    const part = facts.slice(i, i + PER_CALL);
    const { states } = await call("answer", { engine, producers: BOARD_KINDS, answers: part.map((f) => ({
      ref: `${f.identity}|${f.ruler}|${f.mode}`, result: { score: f.score, metric: f.metric },
      // A FACT WITHOUT WORK — a native build's — witnesses the score alone.
      work: Number.isSafeInteger(f.work) ? f.work : null })) });
    made += states.filter((s) => s === "fact").length;
  }
  return made;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [path, engine] = process.argv.slice(2);
  if (!path || !engine) { console.error("usage: order_credit.mjs <facts.ndjson> <engine>"); process.exit(2); }
  if (!process.env.FACTORY_TOKEN) { console.log("credit: no factory token"); process.exit(0); }
  const cursor = `${path}.credited`;
  const from = existsSync(cursor) ? Number(readFileSync(cursor, "utf8")) || 0 : 0;
  const all = existsSync(path) ? readFileSync(path, "utf8").split("\n").filter(Boolean) : [];
  if (all.length > from) {
    const n = await tell(all.slice(from).map((l) => JSON.parse(l)), engine);
    writeFileSync(cursor, String(all.length));
    console.log(`credit: ${all.length - from} fact(s) told to the factory, ${n} question(s) settled by them`);
  }
}
