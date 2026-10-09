// SPDX-License-Identifier: AGPL-3.0-or-later
// WORK A CLIENT DID THAT THE SERVER CONFIRMED, credited — docs/BOARD.md
// §"Contribution". A client's result that the scorer reproduced bit for bit is
// a fact with two independent witnesses, the server one of them, so the
// clients earn it exactly as if a second client had agreed.
//
//   node order_credit.mjs <facts.ndjson> <engine>   a scorer run's facts against the orders it claimed
//
// The run reads the facts file from its last cursor (`<facts>.credited`), so
// running it beside `ship_facts.sh` every minute asks D1 about each fact once.
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { pathToFileURL } from "node:url";

/// ONE STATEMENT AGAINST D1's HTTP API, its rows back.
export async function d1(sql, params = []) {
  const { CF_ACCOUNT, CF_TOKEN, CF_D1_DATABASE } = process.env;
  const r = await fetch(`https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT}/d1/database/${CF_D1_DATABASE}/query`, {
    method: "POST",
    headers: { authorization: `Bearer ${CF_TOKEN}`, "content-type": "application/json" },
    body: JSON.stringify({ sql, params }),
  });
  const j = await r.json().catch(() => null);
  if (!r.ok || !j || !j.success) throw new Error(`d1 ${r.status}: ${JSON.stringify(j && j.errors)}`);
  return j.result[0].results;
}

/// EVERY CLIENT THAT MEASURED AN ORDER, in the order their results came.
export const clientsOf = (o) => (o.clients || [o.produced_by, o.verifier].filter(Boolean).join(",")).split(",").filter(Boolean);

/// `work` TO EACH OF `ids`, in the total and in the day — the two writes
/// worker/verify.js `fact` makes.
export async function credit(q, ids, work) {
  if (!work) return;
  const day = new Date().toISOString().slice(0, 10);
  for (const id of ids) {
    await q("UPDATE verifiers SET work = work + ? WHERE id = ?", [work, id]);
    await q(`INSERT INTO verifier_days (verifier, day, work) VALUES (?, ?, ?)
             ON CONFLICT (verifier, day) DO UPDATE SET work = work + excluded.work`, [id, day, work]);
  }
}

const KEYS_PER_STATEMENT = Math.floor((100 - 1) / 3);

/// EVERY CLAIMED OPEN OR UNRANKED ORDER THE SCORER MEASURED TO THE SAME BITS — score,
/// metric and work — becomes `verified` and its clients are credited. The
/// engine id is not asked: it moves with any edit to the engine's sources, and
/// a client whose bits the scorer reproduces computed the scorer's own answer.
/// The state moves before the credit, so a retry never credits twice; one that
/// differs is left for the release and settled unpaid, like any row nobody owes.
export async function creditConfirmed(q, facts, engine) {
  const byKey = new Map(facts.map((f) => [`${f.identity}|${f.ruler}|${f.mode}`, f]));
  const keys = [...byKey.values()];
  let credited = 0;
  for (let i = 0; i < keys.length; i += KEYS_PER_STATEMENT) {
    const part = keys.slice(i, i + KEYS_PER_STATEMENT);
    const orders = await q(`SELECT identity, ruler, mode, score, metric, work, engine, produced_by, verifier, clients FROM orders
                            WHERE state IN ('scoring:open', 'scoring:fresh') AND (identity, ruler, mode) IN (VALUES ${part.map(() => "(?, ?, ?)").join(", ")})`,
    part.flatMap((f) => [f.identity, f.ruler, f.mode]));
    for (const o of orders) {
      const f = byKey.get(`${o.identity}|${o.ruler}|${o.mode}`);
      if (f.score !== o.score || f.metric !== o.metric || !f.work || f.work !== o.work) continue;
      const took = await q(`UPDATE orders SET state = 'verified' WHERE identity = ? AND ruler = ? AND mode = ?
                            AND state IN ('scoring:open', 'scoring:fresh') RETURNING identity`, [o.identity, o.ruler, o.mode]);
      if (!took.length) continue;
      await credit(q, clientsOf(o), o.work);
      credited++;
    }
  }
  return credited;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [path, engine] = process.argv.slice(2);
  if (!path || !engine) { console.error("usage: order_credit.mjs <facts.ndjson> <engine>"); process.exit(2); }
  const { CF_ACCOUNT, CF_TOKEN, CF_D1_DATABASE } = process.env;
  if (!CF_ACCOUNT || !CF_TOKEN || !CF_D1_DATABASE) { console.log("credit: no database configured"); process.exit(0); }
  const cursor = `${path}.credited`;
  const from = existsSync(cursor) ? Number(readFileSync(cursor, "utf8")) || 0 : 0;
  const all = existsSync(path) ? readFileSync(path, "utf8").split("\n").filter(Boolean) : [];
  if (all.length > from) {
    const n = await creditConfirmed(d1, all.slice(from).map((l) => JSON.parse(l)), engine);
    writeFileSync(cursor, String(all.length));
    console.log(`credit: ${all.length - from} fact(s) read, ${n} client result(s) the scorer confirmed`);
  }
}
