// SPDX-License-Identifier: AGPL-3.0-or-later
// Shared task identity, witness rules and lifecycle constants.
import { DIAL_DEFAULTS } from "./dials.js";

export const OFFICIAL_STATES = ["disputed", "spot", "nondeterministic"];
/// HOW MANY CANDIDATES A CLAIM READS FOR EACH QUESTION IT MAY TAKE, per state:
/// rows another claim holds are skipped, and a branch that ran dry would hand
/// back fewer than `max` while takeable ones sat further along.
export const CLAIM_SPARE = 3;

/// A PACK'S LEASE IS A HEARTBEAT, not a deadline: a page still computing a row
/// renews it (`renewRow`) and is never cut off; only a silent one loses it.
/// Each renewal holds the row this much longer — a few missed beats' worth.
export const RENEW_MS = DIAL_DEFAULTS.renew_ms;
/// WHAT AN OLDER ENGINE'S ANSWER EARNS WHEN ITS ENGINE PARTED FROM THE FACT
/// (`joinersParted`): at most what an answer on the served engine earns, so
/// staying on an old engine never pays better than updating.
export const GRACE_SHARE = 1;
/// HOW FAR ONE ANSWER MOVES ITS PRODUCER'S MEAN FIGHT (`learnCost`): about the
/// last sixty answers, so a release that changes what a row costs is followed
/// within a pack or two.
export const PRODUCER_MEAN_SHARE = 1 / 60;
/// …AND PAST THIS LONG FROM THE LEASE another computer may take it too, the
/// first holder computing on (`lease_prev`): whichever answers first makes it.
/// Nobody is cut off, and a page that beats but never answers holds nothing.
export const ROW_CEILING_MS = DIAL_DEFAULTS.row_ceiling_ms;
/// A LEASE ANOTHER MAY TAKE: none, run out, or held past the ceiling (a dial,
/// worker/dials.js), and TAKING ONE keeps the holder still computing as the one
/// before — worker/pg/sync.sql `factory_takeable`, `factory_supersede`.
export const ceilingS = (ms) => Math.round(ms / 1000);
export const SUPERSEDE = `(lease_prev, leased_to_prev, lease_prev_until) = (SELECT * FROM factory_supersede(lease_prev, leased_to_prev,
  lease_prev_until, lease, leased_to, lease_until, ?1::text))`;

/// Text whose bytes two equal values share: keys sorted, at every depth.
export const canon = (v) => (Array.isArray(v) ? `[${v.map(canon).join(",")}]`
  : v && typeof v === "object" ? `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canon(v[k])}`).join(",")}}`
  : JSON.stringify(v));

/// AN ANSWER A FEW ULP OFF ANOTHER, everything else equal: a score folded in
/// another grouping (docs/WASM.md "one grouping is one answer"). Bits are still
/// the witness — it is never paid as agreeing — but it is not a lie, so the
/// official machines refuse nobody for it (`settleOfficial`).
export const SCORE_NEAR = 1e-12;
/// WHEN THE WATCHDOG SPEAKS of a backlog past due: this many questions, or any
/// left this long past its deadline; settling runs hourly, so less is a queue.
export const OVERDUE_MAX = 1000, OVERDUE_HOURS_MS = 3 * 3_600_000;
/// …AND OF A QUESTION WAITING ON THEM: settling is an hourly cron GitHub
/// starts up to ~45 min late, so under two hours is the next run not yet come.
export const UNSETTLED_MS = 2 * 3_600_000;
/// …AND OF CLIENTS DISAGREEING: this many answers in the hour, and this share
/// of all of them. A healthy hour runs under one in a hundred.
export const DISAGREE_MIN = 30, DISAGREE_SHARE = 0.03;
/// HOW MANY PICKS A HAND-OUT MAKES past rows found dead (`next`, `nextPack`,
/// `claim`): each withdraws what it found, so a cancelled head costs a client
/// one slower ask, never an empty one. worker/pg/sync.sql `board_sync_steps`.
export const PICK_ROUNDS = 8;
export const sansWork = (c) => { const v = JSON.parse(c); delete v.work; return canon(v); };
/// AN ANSWER EQUAL TO `lead`: byte for byte on its engine, by everything but
/// `work` on another (`decide`, and the settler's shadow).
export const sameAs = (lead) => {
  let key = null;
  try { key = sansWork(lead.canon); } catch (_) { /* not an object: bytes alone */ }
  return (a) => {
    if (a.canon === lead.canon) return true;
    if (a.engine === lead.engine || key === null) return false;
    try { return sansWork(a.canon) === key; } catch (_) { return false; }
  };
};
export const nearAnswer = (same) => (x, y) => {
  let a, b;
  try { a = JSON.parse(x); b = JSON.parse(y); } catch (_) { return false; }
  if (!a || !b || typeof a.score !== "number" || typeof b.score !== "number") return false;
  if (!same.every((k) => canon(a[k]) === canon(b[k]))) return false;
  return Math.abs(a.score - b.score) <= SCORE_NEAR * Math.max(Math.abs(a.score), Math.abs(b.score));
};

export const hex = (buf) => [...new Uint8Array(buf)].map((x) => x.toString(16).padStart(2, "0")).join("");
/// A QUESTION'S ID IS WHAT IT ASKS: two producers asking one thing share it.
export async function questionId(verb, request) {
  return hex(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`${verb}\n${canon(request)}`))).slice(0, 32);
}
export const newLease = () => hex(crypto.getRandomValues(new Uint8Array(16)));

/// WHO A DEVICE IS, as eligibility needs it: its owner — the account that claimed
/// it, or the device itself — and every device of that owner, so a further answer
/// never comes from the same person's other computer.
export async function witnessOf(env, device) {
  const row = env.ACCOUNTS
    && await env.ACCOUNTS.prepare("SELECT account FROM devices WHERE verifier = ?").bind(device).first();
  if (!row) return { owner: device, siblings: [device] };
  const { results } = await env.ACCOUNTS.prepare("SELECT verifier FROM devices WHERE account = ?").bind(row.account).all();
  return { owner: row.account, siblings: [...new Set([device, ...results.map((r) => r.verifier)])] };
}

/// THE ANSWERS THAT COUNT AS WITNESSES, in the order they came: no owner and no
/// network twice. Two browsers on one desk are one witness. The settler's
/// shadow (worker/settler.js) judges with this same rule.
export function independent(answers) {
  const owners = new Set(), nets = new Set(), out = [];
  for (const a of answers) {
    if (owners.has(a.owner) || (a.net && nets.has(a.net))) continue;
    owners.add(a.owner);
    if (a.net) nets.add(a.net);
    out.push(a);
  }
  return out;
}

/// A SETTLER PASS THAT DIES leaves its claim, taken again by a pass this much later.
export const CLAIM_STALE_MS = 60_000;

/// ROW LOCKS ON QUESTIONS ARE TAKEN IN ID ORDER by every statement that takes
/// many: a lease given back and a settlement locked one set of rows in two
/// orders, and deadlocked each other several times an hour.
export const inIdOrder = (where) => `id IN (SELECT id FROM questions WHERE ${where} ORDER BY id FOR UPDATE)`;


export const FACTS_PER_REFUSAL_FORGIVEN = 1000;
/// HOW LONG A REFUSAL LASTS: a day, three times as long at each one before it.
export const coolDownMs = (refusals) => 86_400_000 * 3 ** Math.max(0, refusals || 0);
/// Settled task detail expires after this window, across engines and producers.
/// Live delivery, unpaid credit, recent activity and refusals remain protected
/// (worker/pg/sync.sql `factory_forget_past`); scores and credit aggregates stay.
export const KEPT_MS = 14 * 86_400_000;

export const askingIds = (since, now) => `(SELECT a.id FROM factory_asking(?${since}::text, ?${now}::text) AS a(id))`;
