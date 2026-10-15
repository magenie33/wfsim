// SPDX-License-Identifier: AGPL-3.0-or-later
// PROTOCOL 8 — docs/CLIENTS.md §"The cycle, protocol 8". ONE CALL a client makes
// every pack or so does what protocol 7's `/work`, `/answers`, `/renew`,
// `/release` and `/points` did apart: it hands in the results the client kept,
// renews the rows it still computes, gives back every row it no longer names,
// reports rows its engine refused, and is handed its next pack — or told when
// to come back (`next_ms`), which the server holds it to (`verifiers.next_at`).
// Every decision is the factory's own (worker/tasks.js) and protocol 7's
// (worker/verify.js): the order, eligibility, fair share and packing are the
// same functions, so the two protocols hand out by one rule.
//
//   POST /api/board/sync { verifier, engine, protocol: 8, consent, lanes, pace_ms?, want_ms,
//        held?: [{ lease, rows: [question] }], results?: [{ lease, question, score, metric, work,
//        compute_ms?, counts?, lanes? }], keep?: { lease, question }, refused?: [{ lease, question, step }],
//        idle?, states?: [question], since? }
//     → { ok, protocol, protocols_served, release, kept, next_ms, taken, dropped, held,
//         pack: { lease, lease_ms, tasks } | null, work, states, points?, early?, banned?, until?,
//         stale?, busy?, paused?, fallback?, engine?, accepted_until?, join?, dials }

import { witnessOf, newLease, OFFICIAL_STATES, PRODUCER_MEAN_SHARE } from "./tasks.js";
import { dialsOf, clientDials, SYNC_CLIENT_DIALS } from "./dials.js";
import { facts, workOf, packTaskOf, notePause } from "./factory.js";
import { iso } from "./instant.js";
import { fairTable, throttled, accountOf } from "./fair.js";
import { devicePointsOf, RECENT_DAYS, WEEK_DAYS } from "./contribution.js";
import { json, read, day, VERIFIER_ID, ENGINE_ID, LEASE_ID, QUESTION_ID, METRIC_ID, ISO, unrecognised, servedRelease, graceOf,
  netOf, viaRelay, seenOf, askedOf, askedArgs, asksOf, asksRows, countsOf, packSize, FAIR_SCAN, PROTOCOL, protocolsSaid } from "./verify.js";

/// WHAT ONE SYNC CARRIES AT MOST: a ten-minute pack's results twice over.
export const SYNC_BODY = 262_144;
export const SYNC_RESULTS_MAX = 400;
const HELD_MAX = 400, STATES_MAX = 100, REFUSED_MAX = 50;
const STEP = /^[a-z_]{1,24}$/;
const HOUR = /^\d{4}-\d\d-\d\dT\d\d$/;

/// WHO A DEVICE IS, kept a minute per isolate: a claim takes effect within it.
const WITNESS_KEEP_MS = 60_000;
const witnesses = new Map();
async function witness(env, device, now) {
  const kept = witnesses.get(device);
  if (kept && now - kept.at < WITNESS_KEEP_MS) return kept.who;
  const who = await witnessOf(env, device);
  if (witnesses.size > 5000) witnesses.clear();
  witnesses.set(device, { at: now, who });
  return who;
}
/// …a check's moved clock and its claims are seen at once.
export const forgetWitnesses = () => witnesses.clear();

const bad = () => json({ ok: false, error: "bad request" }, 400);

/// THE BODY, read whole or refused whole: a client is told `bad request` for
/// anything malformed, never a part of it taken.
function parse(b) {
  if (!b || !VERIFIER_ID.test(b.verifier || "") || !ENGINE_ID.test(b.engine || "")) return null;
  const results = b.results ?? [], held = b.held ?? [], refused = b.refused ?? [], states = b.states ?? [];
  if (![results, held, refused, states].every(Array.isArray) || results.length > SYNC_RESULTS_MAX
      || refused.length > REFUSED_MAX || states.length > STATES_MAX) return null;
  const out = { results: [], held: [], refused: [], states: [], keep: null };
  for (const a of results) {
    if (!a || !LEASE_ID.test(a.lease || "") || !QUESTION_ID.test(a.question || "") || !METRIC_ID.test(a.metric || "")
        || typeof a.score !== "number" || !Number.isFinite(a.score) || a.score < 0 || !Number.isSafeInteger(a.work) || a.work < 0) return null;
    out.results.push({ lease: a.lease, question: a.question, result: { score: a.score, metric: a.metric }, work: a.work,
      compute_ms: Number.isInteger(a.compute_ms) && a.compute_ms >= 0 && a.compute_ms <= 86_400_000 ? a.compute_ms : null,
      counts: countsOf(a.counts), lanes: Number.isInteger(a.lanes) && a.lanes > 0 && a.lanes <= 256 ? a.lanes : null });
  }
  let rows = 0;
  for (const h of held) {
    if (!h || !LEASE_ID.test(h.lease || "") || !Array.isArray(h.rows) || !h.rows.every((q) => QUESTION_ID.test(q || ""))) return null;
    rows += h.rows.length;
    for (const q of h.rows) out.held.push({ lease: h.lease, question: q });
  }
  if (rows > HELD_MAX) return null;
  for (const r of refused) {
    if (!r || !LEASE_ID.test(r.lease || "") || !QUESTION_ID.test(r.question || "") || !STEP.test(r.step || "")) return null;
    out.refused.push({ lease: r.lease, question: r.question, step: r.step });
  }
  if (!states.every((q) => QUESTION_ID.test(q || ""))) return null;
  out.states = states;
  if (b.keep != null) {
    if (!LEASE_ID.test(b.keep.lease || "") || !QUESTION_ID.test(b.keep.question || "")) return null;
    out.keep = { lease: b.keep.lease, question: b.keep.question };
  }
  return out;
}

/// WHERE EACH NAMED RESULT STANDS, for the page's pending points: protocol 7's
/// `/api/board/mine` by question, read by worker/pg/sync.sql `factory_states`.
const stateOf = (r) => {
  const state = !r.mine ? "gone" : r.credited ? "confirmed"
    : ["disputed", "nondeterministic"].includes(r.state) ? "checking" : r.state === "open" ? "waiting" : "gone";
  return { question: r.id, state, ...(state === "confirmed" && r.fact_at ? { at: r.fact_at } : {}) };
};
async function statesOf(db, device, ids) {
  if (!ids.length) return [];
  const { results } = await db.prepare("SELECT * FROM factory_states(?1::text, ?2::text)").bind(device, JSON.stringify(ids)).all();
  return results.map(stateOf);
}

/// A DEVICE'S POINTS AS worker/contribution.js `devicePointsOf` says them, from
/// what worker/pg/sync.sql `device_points` summed: the work after its last span
/// (`from`, null while an account holds it, read from the accounts beforehand).
const WORK_PER_POINT = 1e9;
const pointsOf = (x) => Math.floor(x / WORK_PER_POINT);
const sinceDay = (days) => new Date(Date.now() - (days - 1) * 86_400_000).toISOString().slice(0, 10);
async function spanOf(env, id) {
  const last = env.ACCOUNTS && await env.ACCOUNTS.prepare("SELECT until_hour FROM device_spans WHERE verifier = ?1 ORDER BY from_hour DESC LIMIT 1")
    .bind(id).first();
  return last ? last.until_hour ?? null : "";
}
const pointsSaid = (x) => ({ points: pointsOf(x.work), recent: pointsOf(x.recent), week: pointsOf(x.week), claimed: x.held,
  ...(x.today ? { today: { tasks: x.today.tasks, ms: x.today.ms, points: pointsOf(x.today.work) } } : {}) });

/// WHAT THE PROCEDURE CANNOT DO raises this, and nothing it did is kept: the sync
/// runs in the worker instead (worker/pg/sync.sql `board_sync`).
const NEEDS_JS = "WS001";

async function sync(request, env) {
  const { b, err } = await read(request, SYNC_BODY);
  if (err) return err;
  const p = parse(b);
  if (!p) return bad();
  const refusedClient = unrecognised(b.verifier);
  if (refusedClient) return refusedClient;
  const db = env.LIBRARY, now = Date.now(), at = iso(now), id = b.verifier, d = await dialsOf(env, now);
  const scale = d.sync_scale_pct / 100, ms = (x) => Math.round(x * scale);
  const say = (body) => json({ ok: true, ...protocolsSaid(), ...body, dials: clientDials(d, SYNC_CLIENT_DIALS) });
  // THE OWNER'S SWITCH: every client falls back to protocol 7 for the session.
  if (d.sync_off) return json({ ok: true, fallback: 7, ...protocolsSaid() });
  if (b.protocol !== PROTOCOL) return say({ stale: true, next_ms: ms(d.sync_idle_ms), taken: [], dropped: [], held: [], pack: null, work: null, states: [] });
  // PAUSED WHILE THE TABLES MOVE: nothing is written, and the client keeps its results.
  if (env.FACTORY_PAUSED === "1") {
    await notePause(env, now);
    return say({ paused: true, busy: true, next_ms: ms(d.sync_idle_ms), taken: [], dropped: [], held: [], pack: null, work: null, states: [] });
  }
  const served = await servedRelease(env), { release } = served;
  // THE ENGINE UNREAD: nothing can be judged, so nothing is taken or dropped.
  if (!served.engine) return say({ release, next_ms: ms(d.sync_idle_ms), taken: [], dropped: [], held: [], pack: null, work: null, states: [] });
  const grace = b.engine !== served.engine ? graceOf(served, b.engine) : null;
  const stale = b.engine !== served.engine && !grace;
  const c = b.consent, consented = !!c && Number.isInteger(c.v) && c.v >= 1 && typeof c.at === "string" && ISO.test(c.at);
  const lanes = Number.isInteger(b.lanes) && b.lanes > 0 && b.lanes <= 256 ? b.lanes : 1;
  const pace = Number.isInteger(b.pace_ms) && b.pace_ms >= 100 && b.pace_ms <= d.sync_lease_ms ? b.pace_ms : null;
  const want = Number.isInteger(b.want_ms) && b.want_ms >= 0 ? Math.min(b.want_ms, d.pack8_ms) : d.pack8_ms;
  const who = await witness(env, id, now);
  const w = { device: id, owner: who.owner, siblings: who.siblings, net: await netOf(request, env), lanes };
  const f = facts();
  const x = { request, env, b, p, d, db, now, at, id, ms, say, served, release, grace, stale, consented, lanes, pace, want, who, w, f };

  // THE CLIENT, ITS YES, ITS DAY, ITS IDLE HOURS — one batch, as `/work` writes them.
  const seen = () => db.batch([
    seenOf(db, id, at, consented ? c : null),
    askedOf(db, { ...b, lanes, ahead: false }, now, d),
    asksOf(db, id, false, idleHours(b), now, viaRelay(request, env)),
  ]);
  // THE OWNER'S OTHER SWITCH: the database work in one call (`sync_proc`). With
  // answers settled in the request (`settle_async` 0) they are recorded here
  // first, so they are settled before anything the call decides.
  if (d.sync_proc === 1) {
    const whole = d.settle_async === 1;
    let given = null;
    if (!whole) { await seen(); given = await results(x); }
    try {
      return await viaProcedure(x, given);
    } catch (e) {
      if (!(e && e.code === NEEDS_JS)) throw e;
    }
    if (whole) { await seen(); given = await results(x); }
    return rest(x, given);
  }
  await seen();
  return rest(x, await results(x));
}
const idleHours = (b) => (Array.isArray(b.idle) ? b.idle : []).filter((x) => x && HOUR.test(x.hour || ""));

/// RESULTS, by the lease each was handed under. A row whose lease ran out still
/// takes its result while nobody took it since (`late`); a result already given
/// is taken again as given, so a sync sent twice is one answer.
async function results({ env, b, p, w, f, now, grace, stale, served }) {
  const taken = [], dropped = [];
  if (stale) for (const r of p.results) dropped.push({ question: r.question, why: "stale_engine" });
  else {
    for (const [lease, rows] of byLease(p.results)) {
      const got = await f.answerPack(env, w, { lease, engine: b.engine, served: grace ? served.engine : null, join: !!(grace && grace.join),
        answers: rows.map(({ lease: _l, ...a }) => a), late: true }, now);
      const ok = new Set([...got.taken, ...got.already]);
      for (const r of rows) (ok.has(r.question) ? taken : dropped).push(ok.has(r.question) ? r.question : { question: r.question, why: "not_held" });
    }
  }
  return { taken, dropped };
}
const byLease = (results) => {
  const out = new Map();
  for (const r of results) out.set(r.lease, [...(out.get(r.lease) || []), r]);
  return out;
};

/// EVERYTHING AFTER THE RESULTS, statement by statement.
async function rest(x, { taken, dropped }) {
  const { request, env, b, p, d, db, now, at, id, ms, say, served, release, grace, stale, consented, pace, want, who, w, f } = x;
  // ROWS ITS ENGINE REFUSED: given back, never handed to it again on this engine.
  const withdrawn = stale ? [] : await f.refuseRows(env, w, b.engine, served.engine, p.refused, d.refusals_to_withdraw, now);
  const done = new Set([...taken, ...p.refused.map((r) => r.question)]);
  // RENEWED: every row it names as held; ANY OTHER LIVE LEASE IT HOLDS GOES BACK.
  const heldRows = stale ? [] : p.held.filter((r) => !done.has(r.question));
  const keptIds = new Set(await f.renewMany(env, id, heldRows, d.sync_lease_ms, now));
  await f.releaseUnnamed(env, id, [...keptIds, ...(p.keep && !stale ? [p.keep.question] : [])], now);
  const held = heldRows.map((r) => ({ question: r.question, held: keptIds.has(r.question) }));
  const states = await statesOf(db, id, p.states);
  const points = typeof b.since === "string" ? await devicePointsOf(env, id, b.since).catch(() => null) : null;
  const base = { release, kept: true, taken, dropped, held, states, ...(withdrawn.length ? { withdrawn } : {}), ...(points ? { points } : {}) };

  const me = await db.prepare("SELECT banned, refused_until, last_at, next_at FROM verifiers WHERE id = ?").bind(id).first() || {};
  const idle = async (extra = {}) => {
    const next = now + ms(d.sync_idle_ms);
    await db.prepare("UPDATE verifiers SET next_at = ? WHERE id = ?").bind(iso(next), id).run();
    return say({ ...base, ...extra, pack: null, work: null, next_ms: next - now });
  };
  if (stale) return idle({ stale: true });
  if (me.banned) return say({ ...base, banned: true, until: me.refused_until, pack: null, work: null, next_ms: ms(d.sync_idle_ms) });
  if (!consented) return idle();
  // EARLY: a sync before the time it was given is served everything above and no work.
  if (me.next_at && at < iso(Date.parse(me.next_at) - 2_000)) {
    return say({ ...base, early: true, pack: null, work: null, next_ms: Math.max(0, Date.parse(me.next_at) - now) });
  }
  const holds = await db.prepare("SELECT count(*) AS questions, count(DISTINCT lease) AS packs FROM questions WHERE leased_to = ? AND lease_until > ?").bind(id, at).first();
  if (want <= 0 || holds.packs >= d.packs_held) return say({ ...base, pack: null, work: null, next_ms: ms(d.sync_floor_ms) });

  const engine = b.engine;
  const head = (await f.headOf(env, w, engine, now, d).all()).results[0];
  if (!head && !grace) return idle();
  // A QUESTION SOMEONE WAITS ON goes alone, to a computer holding nothing; one
  // holding rows is handed a pack of the rest (a pack never holds a waited row),
  // or a busy fleet idled while any waited row stood open.
  if (!grace && !f.packable(head) && holds.questions === 0) return alone(x, base, idle);
  const per = { ...d, pack_ms: Math.max(1, want), pack_max: d.pack8_max };
  const fair = await db.prepare("SELECT * FROM factory_open(?1::text, ?2::bigint, ?3::text, ?4::bigint)")
    .bind(at, FAIR_SCAN, iso(now - 2 * d.asked_every_ms), Math.max(d.pack8_max, pace ? Math.round(want / pace) : d.pack_first)).first();
  const { open, asking } = fair;
  if (d.compute_cap && asking > d.compute_cap && !(me.last_at && me.last_at > iso(now - 600_000))) return idle({ busy: true });
  // WHILE WORK IS SHORT: an account past its fair share is handed nothing this
  // sync, and a pack is a short one, leased short — the last rows go to many.
  const short = Math.floor(open / Math.max(1, asking)) < (pace ? Math.round(want / pace) : d.pack_first);
  if (short && throttled(await fairTable(request, env, served.engine, now), accountOf(who.owner, id))) return idle();
  const n = packSize(pace, open, asking, per), lease_ms = short ? d.pack_lease_ms : d.sync_lease_ms;
  const got = await f.nextPack(env, w, engine, { n, lease_ms, most: d.packs_held, exclude: grace && !grace.join ? grace.changed : null, join: grace && grace.join ? served.engine : null,
    producer: head && f.packable(head) ? head.producer : null }, now);
  return packed(x, base, idle, got && got.lease, lease_ms, got ? got.questions : []);
}

/// A QUESTION SOMEONE WAITS ON, leased alone (worker/tasks.js `next`): its
/// producer may be told of the lease, so it is the worker's on either path.
async function alone(x, base, idle) {
  const { env, db, now, id, ms, say, w, f, b, d } = x;
  const leased = await f.next(env, { ...w, most: 1 }, b.engine, now);
  if (!leased) return idle();
  // A BOARD ROW GOES AS A PACK OF ONE: a protocol 8 page runs a pack, and a
  // `work` only when it is a riven gain, so a waited board row handed as work
  // was given back unrun and went round.
  if (packTaskOf(leased)) return packed(x, base, idle, leased.lease, d.pack_lease_ms, [leased]);
  const next = now + ms(d.sync_floor_ms);
  await db.prepare("UPDATE verifiers SET next_at = ? WHERE id = ?").bind(iso(next), id).run();
  return say({ ...base, pack: null, work: workOf(leased), next_ms: next - now });
}

/// THE PACK HANDED OUT, and when to come back.
async function packed({ db, now, id, ms, say, d, pace, want, grace, served }, base, idle, lease, lease_ms, questions, wrote = false) {
  const tasks = questions.map(packTaskOf).filter(Boolean);
  if (!tasks.length) return idle();
  const next = now + ms(d.sync_floor_ms);
  if (!wrote) await db.prepare("UPDATE verifiers SET next_at = ? WHERE id = ?").bind(iso(next), id).run();
  // COME BACK as the pack runs low: its rows at the client's pace, less the low mark.
  const advise = Math.max(d.sync_floor_ms, (pace ? tasks.length * pace : want) - d.sync_low_ms);
  return say({ ...base, work: null, pack: { lease, lease_ms, tasks }, next_ms: ms(advise),
    ...(grace ? { engine: served.engine, accepted_until: grace.until, ...(grace.join ? { join: true } : {}) } : {}) });
}

/// THE SAME SYNC AS ONE CALL (worker/pg/sync.sql `board_sync`): everything the
/// procedure cannot read is worked out here and handed in — the canonical
/// answers, the fair-share dice, every instant and dial — and its reply is said
/// as `rest` says it. `given`: the results already recorded here, or null for
/// the procedure to record them.
async function viaProcedure(x, given) {
  const { request, env, b, p, d, db, now, at, id, ms, say, served, release, grace, stale, consented, pace, want, who, w, f } = x;
  const rules = f.syncRules, a = askedArgs({ ...b, lanes: w.lanes, ahead: false }, now, d);
  const span = typeof b.since === "string" ? await spanOf(env, id).then((from) => ({ from }), () => null) : null;
  const throttle = throttled(await fairTable(request, env, served.engine, now), accountOf(who.owner, id));
  const call = {
    id, at, day: day(), seen: !given, consent_v: consented ? b.consent.v : null, consent_at: consented ? b.consent.at : null,
    asked: { engine: a.engine, lanes: a.lanes, before: a.before }, asks: asksRows(id, false, idleHours(b), now, viaRelay(request, env)),
    owner: w.owner || id, siblings: JSON.stringify(w.siblings || [id]), net: w.net || "", lanes: w.lanes,
    engine: b.engine, served: served.engine, stale,
    groups: given || stale ? [] : [...byLease(p.results)].map(([lease, rows]) => ({ lease, rows: rows.map((r) => ({ question: r.question,
      canons: rules.canons(r.result, r.work), result: JSON.stringify(r.result), work: r.work, compute_ms: r.compute_ms,
      counts: r.counts ? JSON.stringify(r.counts) : null, lanes: r.lanes })) })),
    taken: given ? given.taken : [],
    dropped: given ? given.dropped : stale ? p.results.map((r) => ({ question: r.question, why: "stale_engine" })) : [],
    record: { engine: grace ? served.engine : b.engine, peer: grace && !grace.join ? b.engine : "", joined: grace && grace.join ? 1 : 0 },
    packable: rules.packable, queueable: rules.queueable, owed: rules.owed, share: PRODUCER_MEAN_SHARE,
    refused: p.refused.map(({ question, step }) => ({ question, step })), withdraw_at: d.refusals_to_withdraw,
    held: p.held, keep: p.keep ? p.keep.question : null, renew_until: iso(now + d.sync_lease_ms), states: p.states,
    points: span && { from: span.from, recent_from: sinceDay(RECENT_DAYS), week_from: sinceDay(WEEK_DAYS),
      since_hour: HOUR.test(b.since) ? b.since : null },
    consented, idle_at: iso(now + ms(d.sync_idle_ms)), floor_at: iso(now + ms(d.sync_floor_ms)), want, packs_held: d.packs_held,
    ceiling_s: Math.round(d.row_ceiling_ms / 1000), grace_s: Math.round(d.settle_grace_ms / 1000), official_states: JSON.stringify(OFFICIAL_STATES),
    grace: grace ? { exclude: grace.join ? null : JSON.stringify(grace.changed), join: grace.join ? served.engine : null } : null,
    fair: { scan: FAIR_SCAN, asking_since: iso(now - 2 * d.asked_every_ms), per: Math.max(d.pack8_max, pace ? Math.round(want / pace) : d.pack_first),
      short_rows: pace ? Math.round(want / pace) : d.pack_first, cap: d.compute_cap, cap_since: iso(now - 600_000), throttle },
    pack: { by_pace: pace ? Math.round(Math.max(1, want) / pace) : d.pack_first, max: d.pack8_max, lease: newLease(),
      short_until: iso(now + d.pack_lease_ms), long_until: iso(now + d.sync_lease_ms) },
  };
  const r = await callSync(db, call);
  // A CHECK'S DATABASE SETTLES WHAT WAS QUEUED AT ONCE (scripts/pg/test_db.mjs).
  if (r.queued && db.settleAtOnce) await f.settleQueued(env, now);
  const points = r.points ? pointsSaid(r.points) : null;
  // IN THE SHAPE `rest` SAYS IT, key for key: jsonb keeps its keys in its own order.
  const base = { release, kept: true, taken: r.taken, dropped: r.dropped.map(({ question, why }) => ({ question, why })),
    held: r.held.map(({ question, held }) => ({ question, held })), states: r.states.map(stateOf),
    ...(r.withdrawn.length ? { withdrawn: r.withdrawn } : {}), ...(points ? { points } : {}) };
  const idle = async (extra = {}, wrote = true) => {
    const next = now + ms(d.sync_idle_ms);
    if (!wrote) await db.prepare("UPDATE verifiers SET next_at = ? WHERE id = ?").bind(iso(next), id).run();
    return say({ ...base, ...extra, pack: null, work: null, next_ms: next - now });
  };
  switch (r.outcome) {
    case "stale": return idle({ stale: true });
    case "banned": return say({ ...base, banned: true, until: r.until, pack: null, work: null, next_ms: ms(d.sync_idle_ms) });
    case "early": return say({ ...base, early: true, pack: null, work: null, next_ms: Math.max(0, Date.parse(r.next_at) - now) });
    case "full": return say({ ...base, pack: null, work: null, next_ms: ms(d.sync_floor_ms) });
    case "busy": return idle({ busy: true });
    case "alone": return alone(x, base, () => idle({}, false));
    case "pack": return packed(x, base, () => idle({}, false), r.lease, r.short ? d.pack_lease_ms : d.sync_lease_ms,
      r.questions.map((q) => ({ question: q.question, verb: q.verb, request: JSON.parse(q.request), due_at: q.due_at,
        demands: q.demands.map((m) => ({ producer: m.producer, ref: m.ref, payload: JSON.parse(m.payload) })) })), true);
    default: return idle();
  }
}

/// ONE CALL, tried again on a deadlock or a serialization failure: the
/// procedure's transaction rolled back whole, so running it again is running it once.
async function callSync(db, call) {
  for (let i = 0; ; i++) {
    try {
      const row = await db.prepare("CALL board_sync((?1::text)::jsonb, NULL)").bind(JSON.stringify(call)).first();
      return typeof row.r === "string" ? JSON.parse(row.r) : row.r;
    } catch (e) {
      if (i >= 2 || !["40P01", "40001"].includes(e && e.code)) throw e;
      await new Promise((r) => setTimeout(r, 25 * (i + 1) + Math.random() * 50));
    }
  }
}

export async function syncRoute(request, env) {
  if (!env.LIBRARY) return json({ ok: false, error: "the library is not configured" }, 503);
  return sync(request, env);
}
