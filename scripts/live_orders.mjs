// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SERVER'S SIDE OF COMPUTE ORDERS — docs/BOARD.md §"Compute orders". Run
// by `live_board.sh`: `rank` every cycle, `settle` in a loop beside it.
//
//   node live_orders.mjs rank <work-dir>               fresh results: top TOP → server, the rest → open
//   node live_orders.mjs settle <work-dir> <bin-dir>   top-TOP, disputed and spot-checked orders, fought here
//   node live_orders.mjs unverified <work-dir> <out>   every result not yet a fact, as fact lines
//
// THE SERVER ONLY BACKS THE CLIENTS UP: `TOP` is 0, so every result waits for a
// second client, and the server fights what they disagree on, the spot checks,
// and — through `scores.yml` after the hold — what nobody took. Raising `TOP`
// makes a result in its group's top TOP the server's alone.
// `settle` fights with the scorer itself (`wfsim-board --queue-in`),
// ships the fact through `ship_facts.sh`, and refuses whichever client its
// number disproves — only when the order's engine is the one this server runs
// (`bin/ENGINE`), since another engine proves nothing.
import { readFileSync, writeFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { d1, clientsOf, credit } from "./order_credit.mjs";

const TOP = 0;
const RANK_PER_CYCLE = 2000;
const SETTLE_PER_CYCLE = 3;
const HERE = dirname(fileURLToPath(import.meta.url));
const [mode, work, arg] = process.argv.slice(2);

const lines = (p) => (existsSync(p) ? readFileSync(p, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l)) : []);
const key = (f) => `${f.identity}|${f.ruler}|${f.mode}`;
const where = "WHERE identity = ? AND ruler = ? AND mode = ?";
const keyOf = (o) => [o.identity, o.ruler, o.mode];

/// WHERE A RESULT STANDS among the verified rows of its group on the site's
/// board — one ruler, one mode, riven or not, the grouping the page draws.
function rank(o) {
  const record = JSON.parse(o.record);
  const p = join(work, "verified", `${record.weapon}.json`);
  if (!existsSync(p)) return 1;
  const riven = !!(record.riven_pos && record.riven_pos.length);
  const group = JSON.parse(readFileSync(p, "utf8"))
    .filter((r) => r.benchmark === o.ruler && (r.mode || "base") === o.mode && !!r.riven === riven);
  return 1 + group.filter((r) => r.score > o.score).length;
}

// AS MANY ORDERS A STATEMENT AS D1'S HUNDRED BOUND PARAMETERS ALLOW: one call
// an order took a minute a few hundred, while the first results waited.
const KEYS_PER_STATEMENT = Math.floor((100 - 1) / 3);

async function rankFresh() {
  const fresh = await d1("SELECT identity, ruler, mode, record, score FROM orders WHERE state = 'fresh' LIMIT ?", [RANK_PER_CYCLE]);
  const to = { arbiter: [], open: [] };
  for (const o of fresh) to[TOP > 0 && rank(o) <= TOP ? "arbiter" : "open"].push(o);
  for (const [state, list] of Object.entries(to)) {
    for (let i = 0; i < list.length; i += KEYS_PER_STATEMENT) {
      const part = list.slice(i, i + KEYS_PER_STATEMENT);
      await d1(`UPDATE orders SET state = ? WHERE (identity, ruler, mode) IN (VALUES ${part.map(() => "(?, ?, ?)").join(", ")})
                AND state = 'fresh'`, [state, ...part.flatMap(keyOf)]);
    }
  }
  if (fresh.length) console.error(`orders: ranked ${fresh.length}, ${to.arbiter.length} kept for the server`);
}

/// ONE FIGHT MAY NOT HOLD SETTLING: past this it is killed, the row is tried
/// again on a later cycle, and the rows behind it are settled meanwhile.
const SETTLE_FIGHT_MS = 15 * 60_000;

/// THE SCORER'S OWN FACT for one order — its score and its work — shipped to
/// `scores`.
function fight(o) {
  const dir = join(work, "settle");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "lib.json"), JSON.stringify([JSON.parse(o.record)]));
  writeFileSync(join(dir, "q.ndjson"), JSON.stringify({ build_id: o.identity, ruler: o.ruler, mode: o.mode }) + "\n");
  execFileSync(join(arg, "wfsim-board"), [o.ruler, join(dir, "board"), "--facts", join(dir, "facts.ndjson"),
    "--measured-by", readFileSync(join(arg, "VERSION"), "utf8").trim(), "--queue-in", join(dir, "q.ndjson")],
  { input: readFileSync(join(dir, "lib.json")), stdio: ["pipe", "ignore", "ignore"], timeout: SETTLE_FIGHT_MS });
  const got = lines(join(dir, "facts.ndjson")).find((f) => key(f) === key(o));
  if (!got) return null;
  execFileSync("bash", [join(HERE, "ship_facts.sh"), join(dir, "facts.ndjson")], { stdio: ["ignore", "ignore", "inherit"] });
  return got;
}

/// A CLIENT ITS NUMBER DISPROVED, refused. What it agreed to is withdrawn and
/// opened again; what it measured first and nobody has confirmed is measured
/// again. A result of its that an independent client confirmed stands.
async function ban(id) {
  if (!id) return;
  await d1("UPDATE verifiers SET banned = 1 WHERE id = ?", [id]);
  const named = await d1(`SELECT identity, ruler, mode, state, produced_by, verifier, clients, clients_compute_ms FROM orders
                          WHERE (',' || clients || ',') LIKE ? AND produced_by != ? AND state IN ('verified', 'spot', 'open')`,
  [`%,${id},%`, id]);
  const agreed = named.filter((o) => o.state !== "open");
  for (const o of named) {
    const ms = (o.clients_compute_ms || "").split(",");
    const kept = clientsOf(o).map((c, i) => [c, ms[i] || ""]).filter(([c]) => c !== id);
    if (o.state !== "open") await d1(`DELETE FROM scores ${where} AND measured_by LIKE 'verified:%'`, keyOf(o));
    await d1(`UPDATE orders SET state = 'open', verifier = NULL, clients = ?, clients_compute_ms = ? ${where}`,
      [kept.map(([c]) => c).join(","), kept.map(([, m]) => m).join(","), ...keyOf(o)]);
  }
  await d1(`UPDATE orders SET state = 'todo', engine = '', score = NULL, metric = NULL, work = NULL, produced_by = NULL, clients = '', clients_compute_ms = ''
            WHERE produced_by = ? AND state IN ('fresh', 'open', 'arbiter', 'dispute')`, [id]);
  console.error(`orders: refused client ${id.slice(0, 6)}…, ${agreed.length} agreement(s) withdrawn`);
}

async function settle() {
  const engine = existsSync(join(arg, "ENGINE")) ? readFileSync(join(arg, "ENGINE"), "utf8").trim() : "";
  // A TOP-TEN ROW NOBODY OWES ANY MORE — the scorer measured it after the
  // clients' hold — is settled unfought, as a lease settles one: fighting it
  // again queued every new top-ten row behind a day of facts already banked.
  const gone = await d1(`UPDATE orders SET state = 'settled' WHERE state = 'arbiter' AND NOT EXISTS
                         (SELECT 1 FROM queue q WHERE q.build_id = orders.identity AND q.ruler = orders.ruler
                          AND q.mode = orders.mode) RETURNING identity`);
  if (gone.length) console.error(`orders: settled ${gone.length} top-ten order(s) the scorer already measured`);
  const todo = await d1(`SELECT identity, ruler, mode, record, score, work, engine, state, produced_by, verifier, disputed, clients
                         FROM orders WHERE state IN ('arbiter', 'dispute', 'spot') ORDER BY state, random() LIMIT ?`, [SETTLE_PER_CYCLE]);
  for (const o of todo) {
    let got = null;
    try { got = fight(o); } catch (e) { console.error(`orders: ${o.identity.slice(0, 8)} did not fight — ${e.message}`); }
    if (got === null) continue;
    const truth = got.score;
    // …AND THE WORK THE CLIENTS CLAIMED, which is what they are credited
    // (docs/BOARD.md §"Contribution"). A scorer that did not count it says 0.
    const claimed = truth === o.score && !(got.work && o.work != null && got.work !== o.work);
    // WHOEVER THE SERVER DISAGREES WITH WAS WRONG: every client that sent the
    // order's number, the one that disputed it, or all of them. A dispute's
    // client is the last to have answered.
    // A REFUSAL NEEDS THE SAME ENGINE — another one may differ honestly — but
    // AGREEMENT DOES NOT: bits the server reproduces are its own answer.
    let paid = [];
    const ids = clientsOf(o);
    const disputer = o.state === "dispute" ? ids.pop() : null;
    if (o.engine === engine) {
      if (!claimed) for (const id of ids) await ban(id);
      if (disputer && truth !== o.disputed) await ban(disputer);
    }
    // …AND WHOEVER IT AGREES WITH EARNED IT, work included: a spot was
    // credited when it became a fact, an arbiter or disputed order never was.
    if (claimed && o.state !== "spot" && got.work && got.work === o.work) paid = ids;
    const state = claimed ? "verified" : "rejected";
    await d1(`UPDATE orders SET state = ? ${where}`, [state, ...keyOf(o)]);
    await credit(d1, paid, o.work);
    console.error(`orders: settled ${o.state} ${o.identity.slice(0, 8)} ${o.ruler}/${o.mode} — ${state}`);
  }
}

/// EVERY RESULT NOT YET A FACT, as the fact lines the owner's board projects
/// (`board::CLIENT_MEASURED` marks them unverified).
async function unverified() {
  const rows = await d1(`SELECT identity, ruler, mode, score, metric, engine FROM orders
                         WHERE state IN ('fresh', 'open', 'arbiter', 'dispute') AND score IS NOT NULL`);
  const at = new Date().toISOString().slice(0, 19) + "Z";
  writeFileSync(arg, rows.map((o) => JSON.stringify({
    identity: o.identity, ruler: o.ruler, mode: o.mode, metric: o.metric, measured_by: `client:${o.engine}`,
    score: o.score, cost_seconds: 0, started_at: at, finished_at: at,
  }) + "\n").join(""));
}

if (mode === "rank") await rankFresh();
else if (mode === "settle") await settle();
else if (mode === "unverified") await unverified();
else { console.error("usage: live_orders.mjs rank|settle|unverified <work-dir> [bin-dir|out]"); process.exit(2); }
