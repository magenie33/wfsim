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

import { witnessOf } from "./tasks.js";
import { dialsOf, clientDials, SYNC_CLIENT_DIALS } from "./dials.js";
import { facts, workOf, packTaskOf, notePause } from "./factory.js";
import { iso } from "./instant.js";
import { fairTable, throttled, accountOf } from "./fair.js";
import { devicePointsOf } from "./contribution.js";
import { askingIds } from "./tasks.js";
import { json, read, day, VERIFIER_ID, ENGINE_ID, LEASE_ID, QUESTION_ID, METRIC_ID, ISO, unrecognised, servedRelease, graceOf,
  netOf, viaRelay, askedOf, asksOf, countsOf, packSize, FAIR_SCAN, PROTOCOL, protocolsSaid } from "./verify.js";

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
/// `/api/board/mine` by question.
async function statesOf(db, device, ids) {
  if (!ids.length) return [];
  const { results } = await db.prepare(`SELECT q.id, q.state, q.fact_at,
        (SELECT max(credited) FROM answers a WHERE a.question = q.id AND a.device = ?1::text AND a.void = 0) AS credited,
        (SELECT count(*) FROM answers a WHERE a.question = q.id AND a.device = ?1::text AND a.void = 0) AS mine
      FROM questions q WHERE q.id IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb))`)
    .bind(device, JSON.stringify(ids)).all();
  return results.map((r) => {
    const state = !r.mine ? "gone" : r.credited ? "confirmed"
      : ["disputed", "nondeterministic"].includes(r.state) ? "checking" : r.state === "open" ? "waiting" : "gone";
    return { question: r.id, state, ...(state === "confirmed" && r.fact_at ? { at: r.fact_at } : {}) };
  });
}

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

  // THE CLIENT, ITS YES, ITS DAY, ITS IDLE HOURS — one batch, as `/work` writes them.
  await db.batch([
    db.prepare("INSERT INTO verifiers (id, seen) VALUES (?, ?) ON CONFLICT DO NOTHING").bind(id, day()),
    askedOf(db, { ...b, lanes, ahead: false }, now, d),
    ...(consented ? [db.prepare("UPDATE verifiers SET consent_v = ?::bigint, consent_at = ?::text WHERE id = ?::text AND (consent_v IS DISTINCT FROM ?::bigint OR consent_at IS DISTINCT FROM ?::text)")
      .bind(c.v, c.at, id, c.v, c.at)] : []),
    db.prepare("UPDATE verifiers SET banned = 0 WHERE id = ? AND banned = 1 AND (refused_until IS NULL OR refused_until <= ?)").bind(id, at),
    db.prepare("UPDATE verifiers SET seen = ?::text WHERE id = ?::text AND seen IS DISTINCT FROM ?::text").bind(day(), id, day()),
    asksOf(db, id, false, (Array.isArray(b.idle) ? b.idle : []).filter((x) => x && HOUR.test(x.hour || "")), now, viaRelay(request, env)),
  ]);

  // RESULTS, by the lease each was handed under. A row whose lease ran out still
  // takes its result while nobody took it since (`late`); a result already given
  // is taken again as given, so a sync sent twice is one answer.
  const taken = [], dropped = [];
  if (stale) for (const r of p.results) dropped.push({ question: r.question, why: "stale_engine" });
  else {
    const byLease = new Map();
    for (const r of p.results) byLease.set(r.lease, [...(byLease.get(r.lease) || []), r]);
    for (const [lease, rows] of byLease) {
      const got = await f.answerPack(env, w, { lease, engine: b.engine, served: grace ? served.engine : null, join: !!(grace && grace.join),
        answers: rows.map(({ lease: _l, ...a }) => a), late: true }, now);
      const ok = new Set([...got.taken, ...got.already]);
      for (const r of rows) (ok.has(r.question) ? taken : dropped).push(ok.has(r.question) ? r.question : { question: r.question, why: "not_held" });
    }
  }
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
  // A QUESTION SOMEONE WAITS ON goes alone, to a computer holding nothing.
  if (!grace && !f.packable(head)) {
    if (holds.questions > 0) return idle();
    const leased = await f.next(env, { ...w, most: 1 }, engine, now);
    if (!leased) return idle();
    const next = now + ms(d.sync_floor_ms);
    await db.prepare("UPDATE verifiers SET next_at = ? WHERE id = ?").bind(iso(next), id).run();
    return say({ ...base, pack: null, work: workOf(leased), next_ms: next - now });
  }
  const per = { ...d, pack_ms: Math.max(1, want), pack_max: d.pack8_max };
  const fair = await db.prepare(`WITH a AS (SELECT count(*) AS n FROM ${askingIds(3, 1)} x)
      SELECT (SELECT count(*) FROM (SELECT 1 FROM questions WHERE state = 'open' AND waits = 0
        AND (lease_until IS NULL OR lease_until < ?1::text)
        LIMIT LEAST(?2::bigint, GREATEST(1, (SELECT n FROM a)) * ?4::bigint)) o) AS open, (SELECT n FROM a) AS asking`)
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
  const tasks = got ? got.questions.map(packTaskOf).filter(Boolean) : [];
  if (!tasks.length) return idle();
  const next = now + ms(d.sync_floor_ms);
  await db.prepare("UPDATE verifiers SET next_at = ? WHERE id = ?").bind(iso(next), id).run();
  // COME BACK as the pack runs low: its rows at the client's pace, less the low mark.
  const advise = Math.max(d.sync_floor_ms, (pace ? tasks.length * pace : want) - d.sync_low_ms);
  return say({ ...base, work: null, pack: { lease: got.lease, lease_ms, tasks }, next_ms: ms(advise),
    ...(grace ? { engine: served.engine, accepted_until: grace.until, ...(grace.join ? { join: true } : {}) } : {}) });
}

export async function syncRoute(request, env) {
  if (!env.LIBRARY) return json({ ok: false, error: "the library is not configured" }, 503);
  return sync(request, env);
}
