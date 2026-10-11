// ---- FAIR SHARE WHILE WORK IS SHORT --------------------------------------------
//
// When the open rows are fewer than the computers asking would take, each
// account aims at a TARGET share of what is handed out: (1 - α) of it by the
// cores it has asking and α of it equal among established accounts. An account
// that received more than its target over the window is handed nothing on an
// ask with probability 1 − target / received; while work is plentiful nothing
// here runs. docs/BOARD.md §"Fair share while work is short".
import { iso } from "./instant.js";

/// HOW MUCH OF THE HAND-OUT IS EQUAL PER ACCOUNT, the rest by cores asking.
export const FAIR_ALPHA = 0.3;
/// WHAT AN ACCOUNT RECEIVED is its credited-or-not `answers.work` over this
/// window, on the served engine only: a new engine is a new unit of work.
export const FAIR_WINDOW_MS = 10 * 60_000;
/// THE TABLE IS WORKED OUT ONCE A MINUTE, per isolate and at the edge.
export const FAIR_KEEP_MS = 60_000;
/// EVERY UNCLAIMED BROWSER IS ONE ACCOUNT, or a fresh id per page would each
/// draw an equal share.
export const WALK_INS = "walk-ins";
/// AN ACCOUNT SHARES THE EQUAL PART once it is this old and has been credited:
/// ten new accounts would otherwise draw ten equal shares the day they are made.
export const ESTABLISHED_MS = 24 * 3_600_000;
/// ASKING NOW is having asked within this (verify.js records an ask once a minute).
const ASKING_MS = 2 * 60_000;

export const accountOf = (owner, device) => (!owner || owner === device ? WALK_INS : owner);

/// THE TARGETS, from what each account has asking and what it received.
/// `accounts`: [{ account, lanes, received, established }]. Returns
/// { [account]: { target, share, ratio } }, `ratio` = share / target.
export function shares(accounts, alpha = FAIR_ALPHA) {
  const lanes = accounts.reduce((n, a) => n + a.lanes, 0);
  const received = accounts.reduce((n, a) => n + a.received, 0);
  const equal = accounts.filter((a) => a.established).length;
  const out = {};
  for (const a of accounts) {
    const byLanes = lanes ? a.lanes / lanes : 0;
    // WITH NOBODY ESTABLISHED there is no equal part to hand out.
    const target = equal ? (1 - alpha) * byLanes + (a.established ? alpha / equal : 0) : byLanes;
    const share = received ? a.received / received : 0;
    out[a.account] = { target, share, ratio: target ? share / target : share ? Infinity : 0 };
  }
  return out;
}

/// NOTHING THIS ASK, with probability 1 − 1/ratio for an account over its target.
export function throttled(table, account, random = Math.random) {
  const a = table && table[account];
  return !!a && a.ratio > 1 && random() > 1 / a.ratio;
}

async function tableOf(env, engine, now) {
  const db = env.LIBRARY;
  const { results: asking } = await db.prepare("SELECT id, asked_lanes FROM verifiers WHERE asked_at > ?")
    .bind(iso(now - ASKING_MS)).all();
  const owned = new Map(), created = new Map();
  if (env.ACCOUNTS) {
    const { results } = await env.ACCOUNTS.prepare(
      `SELECT d.verifier, d.account, a.created_at FROM devices d JOIN accounts a ON a.id = d.account
        WHERE d.account IN (SELECT account FROM devices WHERE verifier IN (SELECT value FROM json_each(?)))`)
      .bind(JSON.stringify(asking.map((r) => r.id))).all();
    for (const r of results) { owned.set(r.verifier, r.account); created.set(r.account, r.created_at); }
  }
  const devices = [...new Set([...asking.map((r) => r.id), ...owned.keys()])];
  const [got, paid] = await db.batch([
    db.prepare(`SELECT device, sum(work) AS work FROM answers WHERE device IN (SELECT value FROM json_each(?1))
                 AND at > ?2 AND engine = ?3 AND official = 0 AND void = 0 GROUP BY 1`)
      .bind(JSON.stringify(devices), iso(now - FAIR_WINDOW_MS), engine),
    db.prepare("SELECT id FROM verifiers WHERE id IN (SELECT value FROM json_each(?)) AND work > 0").bind(JSON.stringify([...owned.keys()])),
  ]);
  const credited = new Set(paid.results.map((r) => owned.get(r.id)));
  const by = new Map();
  const of = (device) => {
    const account = owned.get(device) || WALK_INS;
    if (!by.has(account)) {
      const born = created.get(account);
      by.set(account, { account, lanes: 0, received: 0,
        established: account === WALK_INS || (credited.has(account) && !!born && Date.parse(born) < now - ESTABLISHED_MS) });
    }
    return by.get(account);
  };
  for (const r of asking) of(r.id).lanes += r.asked_lanes || 1;
  for (const r of got.results) of(r.device).received += r.work || 0;
  return shares([...by.values()]);
}

let kept = null;
/// THE TABLE AS OF THE LAST MINUTE, for the engine served.
export async function fairTable(request, env, engine, now = Date.now()) {
  if (kept && kept.engine === engine && now - kept.at < FAIR_KEEP_MS) return kept.table;
  const cache = typeof caches !== "undefined" ? caches.default : null;
  const key = new Request(new URL(`/api/board/fair/${engine}`, request.url).toString());
  const hit = cache && await cache.match(key);
  const table = hit ? await hit.json() : await tableOf(env, engine, now);
  if (cache && !hit) await cache.put(key, new Response(JSON.stringify(table),
    { headers: { "content-type": "application/json", "cache-control": `public, max-age=${FAIR_KEEP_MS / 1000}` } }));
  kept = { engine, at: now, table };
  return table;
}
