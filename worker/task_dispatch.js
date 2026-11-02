// SPDX-License-Identifier: AGPL-3.0-or-later
// Task selection and leases; verification and result delivery are separate.
import { iso } from "./instant.js";
import { dialsOf, DIAL_DEFAULTS } from "./dials.js";
import { OFFICIAL_STATES, CLAIM_SPARE, ceilingS, SUPERSEDE, PICK_ROUNDS, newLease, inIdOrder } from "./task_contract.js";

export function createDispatch({ verbs, onLease, liveDemands, sift, restate, restateAll, charge } = {}) {
  const listOf = (db, p) => `(SELECT value FROM jsonb_array_elements_text((${p}::text)::jsonb))`;
  const PACKABLE = JSON.stringify(Object.keys(verbs).filter((v) => verbs[v].pack));
  /// THE QUESTION THIS WITNESS ANSWERS NEXT, leased, or null — §"Order" among
  /// exactly the questions it may answer. `w`: { device, owner,
  /// siblings, net, lanes, official, most, before }: `lanes` 0 is a witness with
  /// no cores free now (asking ahead), which a question someone waits on is never
  /// handed; `before` keeps to questions due before then.
  async function next(env, w, engine, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), most = w.most ?? 1, d = await dialsOf(env, now);
    for (let tries = 0; tries < PICK_ROUNDS; tries++) {
      const q = await db.prepare(HEAD).bind(...eligibleArgs(w, engine, at, d)).first();
      if (!q) return null;
      // A ROW NOBODY OWES ANY MORE is withdrawn where it is found, and the next
      // one is asked for.
      const live = await liveDemands(db, q.id);
      const active = await sift(env, live.map(d => ({ ...d, payload: JSON.parse(d.payload) })));
      const wanted = live.filter(d => active.some(x => x.producer === d.producer && x.ref === d.ref));
      if (q.state === "open" && !wanted.length) {
        await restate(db, q.id);
        continue;
      }
      const lease = newLease();
      // THE RACE IS THE DATABASE'S, and so is the cap on what one witness holds.
      const took = await db.prepare(`UPDATE questions SET ${SUPERSEDE}, lease = ?2::text, lease_until = ?3::text, leased_to = ?4::text, leased_at = ?1::text
          WHERE id = ?5::text AND state = ?6::text AND factory_takeable(lease_until, leased_at, ?1::text, ?8::bigint)
            AND (SELECT count(*) FROM questions h WHERE h.leased_to = ?4::text AND h.lease_until > ?1::text) < ?7::bigint`)
        .bind(at, lease, iso(now + verbs[q.verb].lease_ms), w.device, q.id, q.state, most, ceilingS(d.row_ceiling_ms)).run();
      if (took.meta.changes) {
        if (!w.official) await charge(db, q);
        for (const d of wanted) await onLease(env, { ...d, payload: JSON.parse(d.payload) }, now);
        return { lease, question: q.id, verb: q.verb, request: JSON.parse(q.request), due_at: q.due_at,
          demands: wanted.map((d) => ({ producer: d.producer, ref: d.ref, payload: JSON.parse(d.payload) })) };
      }
      const held = await db.prepare("SELECT count(*) AS n FROM questions WHERE leased_to = ? AND lease_until > ?").bind(w.device, at).first();
      if (held.n >= most) return null;
    }
    return null;
  }


  /// A PACK: up to `n` questions of a packable verb, in the order `next` hands
  /// them, leased together under one lease for `lease_ms` — one statement, so a
  /// pack costs the database what one question did. A question someone waits on
  /// is never in one: it goes alone to a computer free now (`next`). `most` is
  /// how many packs the witness may then hold. Each comes with its live demands;
  /// a row no longer wanted is withdrawn where it is found and left out.
  ///
  /// AN OLDER ENGINE STILL ACCEPTED (`exclude`, worker/verify.js `graceOf`) is
  /// handed only rows of a weapon whose data it shares with the served engine.
  /// ONE WHOSE CODE MOVED (`join`, the served engine) is handed only a row a
  /// served answer of another owner and network already leads: its own answers
  /// never start a fact, so handed every row, every stale page answers the head of
  /// the queue again and none of it counts.
  /// The pick, the lease and the charge are worker/pg/sync.sql `factory_lease_pack`.
  async function nextPack(env, w, engine, { n, lease_ms, most = 1, exclude = null, join = null, producer = null }, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), lease = newLease(), d = await dialsOf(env, now);
    const [, , , , , , , , ceiling, grace] = eligibleArgs(w, engine, at, d);
    const kept = [], wanted = [];
    // A PICK THAT WAS DEAD IS PICKED AGAIN, under the same lease, until the pack
    // is full, a pick finds nothing dead, or `PICK_ROUNDS` picks: rows cancelled
    // at the head of the order are withdrawn a pack's worth a pick, and the
    // client is handed the live rows behind them, never nothing.
    for (let round = 0; round < PICK_ROUNDS && kept.length < n; round++) {
      const [taken, ds] = await db.batch([
        db.prepare(`SELECT * FROM factory_lease_pack(?1::text, ?2::bigint, ?3::text, ?4::text, ?5::text, ?6::text, ?7::text, ?8::bigint, ?9::bigint,
            ?10::text, ?11::text, ?12::text, ?13::text, ?14::bigint, ?15::text, ?16::text, ?17::text, ?18::bigint)`)
          .bind(at, w.lanes ?? 1, engine, JSON.stringify(w.siblings || [w.device]), w.owner || w.device, w.net || "", w.before ?? null, ceiling, grace,
            PACKABLE, exclude ? JSON.stringify(exclude) : null, join, producer, n - kept.length, lease, iso(now + lease_ms), w.device, most),
        // IN ONE ORDER, which the sync's procedure walks too (`board_sync`).
        db.prepare(`SELECT d.question, d.producer, d.ref, d.payload FROM demands d JOIN questions q ON q.id = d.question
            WHERE q.lease = ? AND q.leased_to = ? AND d.state = 'live' ORDER BY d.question, d.producer, d.ref`).bind(lease, w.device),
      ]);
      if (!taken.results.length) break;
      const ids = new Set(taken.results.map((q) => q.id));
      const owed = await sift(env, ds.results.filter((x) => ids.has(x.question)).map((x) => ({ ...x, payload: JSON.parse(x.payload) })));
      const gone = taken.results.filter((q) => !owed.some((x) => x.question === q.id)).map((q) => q.id);
      if (gone.length) {
        await db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
            WHERE ${inIdOrder(`id IN ${listOf(db, "?")} AND lease = ?`)}`).bind(JSON.stringify(gone), lease).run();
        // A ROW LEFT WITH NO LIVE DEMAND is withdrawn, not only given back: still
        // open, it heads every pack's pick again and every ask is handed nothing.
        await restateAll(db, gone);
      }
      kept.push(...taken.results.filter((q) => !gone.includes(q.id)));
      wanted.push(...owed);
      if (!gone.length) break;
    }
    for (const x of wanted) await onLease(env, x, now);
    const questions = kept
      .sort((a, b) => (a.due_at < b.due_at ? -1 : a.due_at > b.due_at ? 1 : a.id < b.id ? -1 : 1))
      .map((q) => ({ question: q.id, verb: q.verb, request: JSON.parse(q.request), due_at: q.due_at,
        demands: wanted.filter((x) => x.question === q.id).map(({ producer, ref, payload }) => ({ producer, ref, payload })) }));
    return questions.length ? { lease, lease_ms, questions } : null;
  }


  /// A LEASE KEPT while its witness is still at it, or given back.
  async function renew(env, device, lease, now = Date.now()) {
    const db = env.LIBRARY;
    const q = await db.prepare("SELECT verb FROM questions WHERE lease = ? AND leased_to = ? AND lease_until >= ?")
      .bind(lease, device, iso(now)).first();
    if (!q) return false;
    await db.prepare("UPDATE questions SET lease_until = ? WHERE lease = ? AND leased_to = ?")
      .bind(iso(now + verbs[q.verb].lease_ms), lease, device).run();
    return true;
  }


  /// ONE ROW OF A PACK KEPT while its page is computing it — renewed even once
  /// its lease ran out, as long as nobody took it since, and as the holder
  /// before (`SUPERSEDE`) once another took it past the ceiling. Returns `{ held }`.
  async function renewRow(env, device, lease, question, now = Date.now()) {
    const db = env.LIBRARY, most = "GREATEST", renewMs = (await dialsOf(env, now)).renew_ms;
    const [kept, before] = await db.batch([
      db.prepare(`UPDATE questions SET lease_until = ${most}(lease_until, ?4)
                   WHERE id = ?1 AND lease = ?2 AND leased_to = ?3 RETURNING id`)
        .bind(question, lease, device, iso(now + renewMs)),
      db.prepare(`UPDATE questions SET lease_prev_until = ${most}(lease_prev_until, ?4)
                   WHERE id = ?1 AND lease_prev = ?2 AND leased_to_prev = ?3 RETURNING id`)
        .bind(question, lease, device, iso(now + renewMs)),
    ]);
    return { held: kept.results.length > 0 || before.results.length > 0 };
  }


  /// PROTOCOL 8'S RENEWAL: every row a sync names as held, `ms` longer — as its
  /// holder, or as the holder before (`renewRow`, many at once). Returns the ids
  /// still held.
  async function renewMany(env, device, rows, ms, now = Date.now()) {
    if (!rows.length) return [];
    const list = JSON.stringify(rows.map(({ lease, question }) => ({ lease, question })));
    const { results } = await env.LIBRARY.prepare("SELECT k AS id FROM factory_renew(?1::text, ?2::text, ?3::text) k")
      .bind(list, device, iso(now + ms)).all();
    return [...new Set(results.map((r) => r.id))];
  }


  /// `keep` is a row the page will resume (69-board-work.js `RESUME_KEY`): left
  /// under the lease, which runs out unless the page comes back to renew it.
  async function release(env, device, lease, keep = null) {
    const db = env.LIBRARY;
    await db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
        WHERE ${inIdOrder("lease = ?::text AND leased_to = ?::text AND id IS DISTINCT FROM ?::text")}`)
      .bind(lease, device, keep).run();
  }


  /// EVERYTHING A WITNESS HOLDS goes back: one that asks for work again is
  /// working on nothing (worker/verify.js, asking ahead).
  async function releaseAll(env, device, now = Date.now()) {
    await releaseAllOf(env, device, now).run();
  }


  /// …AND A SYNC STATES EVERYTHING ITS ID HOLDS: a live lease on a row it names
  /// nowhere is one its page lost, given back here rather than waited out.
  async function releaseUnnamed(env, device, named, now = Date.now()) {
    await env.LIBRARY.prepare("SELECT factory_release_unnamed(?1::text, ?2::text, ?3::text)").bind(device, iso(now), JSON.stringify(named)).run();
  }


  /// ROWS A CLIENT'S ENGINE REFUSED, kept so they are not handed to it again and
  /// given back now; one refused by `withdrawAt` owners on the served engine is
  /// withdrawn — no client can answer it — and written down for the watchdog.
  /// worker/pg/sync.sql `factory_refuse`.
  async function refuseRows(env, w, engine, served, rows, withdrawAt, now = Date.now()) {
    if (!rows.length) return [];
    const { results } = await env.LIBRARY.prepare("SELECT f AS question FROM factory_refuse(?1::text, ?2::text, ?3::text, ?4::text, ?5::text, ?6::bigint, ?7::text) f")
      .bind(JSON.stringify(rows), w.device, w.owner || w.device, engine, served, withdrawAt, iso(now)).all();
    return results.map((r) => r.question);
  }


  /// THE OFFICIAL MACHINES TAKE A BATCH: up to `max` questions past their
  /// deadline (`open`) or theirs alone (`OFFICIAL_STATES`), as `states` says, of
  /// the given producers, leased for `lease_ms` in one statement. Each comes
  /// with its demands, the live ones first, which say what to compute — a fact
  /// checked again has only served ones.
  async function claim(env, w, { states = ["open", ...OFFICIAL_STATES], producers = null, max = 100, lease_ms = 6 * 3_600_000 }, engine, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now);
    max = Math.trunc(Number(max));
    const wanted = ["open", ...OFFICIAL_STATES].filter((state) => states.includes(state));
    if (!Number.isSafeInteger(max) || max <= 0 || !wanted.length) return [];
    // EACH WHITELISTED STATE HAS ITS OWN ORDERED INDEX READ. A bound state
    // array lets a generic prepared plan scan and sort the whole table. Limits
    // are safe integers in the SQL so that plan knows the candidate budget.
    // Spare candidates cover held rows; the lock rechecks eligibility.
    const stateOf = (state) => `q.state = '${state}'${state === "open" ? " AND q.due_at < ?3::text" : ""}`;
    const blocked = Object.keys(verbs).filter(verb => verbs[verb].official_producers
      && !verbs[verb].official_producers.some(p => producers?.includes(p)));
    const takeable = `q.verb NOT IN (SELECT value FROM jsonb_array_elements_text((?6::text)::jsonb))
            AND (q.lease_until IS NULL OR q.lease_until < ?3::text)
            AND (?4::text IS NULL OR EXISTS (SELECT 1 FROM demands d WHERE d.question = q.id
                                       AND d.producer IN (SELECT value FROM jsonb_array_elements_text((?4::text)::jsonb))))
            AND NOT EXISTS (SELECT 1 FROM answers a WHERE a.question = q.id AND a.device = ?2::text AND a.engine = ?5::text)`;
    const take = (n) => {
      const candidates = wanted.map((state) => `(SELECT q.id FROM questions q WHERE ${stateOf(state)} AND ${takeable}
            ORDER BY q.waits DESC, q.due_at, q.id LIMIT ${n * CLAIM_SPARE})`).join(" UNION ALL ");
      return db.prepare(`WITH c AS (${candidates})
        UPDATE questions SET lease = replace(gen_random_uuid()::text, '-', ''), lease_until = ?1::text, leased_to = ?2::text, leased_at = NULL
        WHERE id IN (SELECT q.id FROM questions q
          WHERE q.id IN (SELECT id FROM c)
            AND (${wanted.map((state) => `(${stateOf(state)})`).join(" OR ")})
            AND ${takeable}
          ORDER BY q.waits DESC, q.due_at, q.id LIMIT ${n} FOR UPDATE OF q SKIP LOCKED)
        RETURNING id, verb, request, state, lease, waits, due_at`)
        .bind(iso(now + lease_ms), w.device, at, producers ? JSON.stringify(producers) : null, engine, JSON.stringify(blocked)).all();
    };
    // AN OPEN ROW NOBODY OWES ANY MORE is withdrawn where it is found and the
    // claim picks again (`PICK_ROUNDS`), as a pack does.
    const results = [];
    for (let round = 0; round < PICK_ROUNDS && results.length < max; round++) {
      const { results: got } = await take(max - results.length);
      if (!got.length) break;
      const opened = got.filter((q) => q.state === "open").map((q) => q.id);
      const { results: live } = opened.length ? await db.prepare(`SELECT question, producer, ref, payload FROM demands
          WHERE question IN ${listOf(db, "?")} AND state = 'live' ORDER BY question, producer, ref`).bind(JSON.stringify(opened)).all() : { results: [] };
      const owed = await sift(env, live.map((x) => ({ ...x, payload: JSON.parse(x.payload) })));
      const gone = opened.filter((id) => !owed.some((x) => x.question === id));
      if (gone.length) {
        await db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
            WHERE ${inIdOrder(`id IN ${listOf(db, "?")} AND leased_to = ?`)}`).bind(JSON.stringify(gone), w.device).run();
        await restateAll(db, gone);
      }
      results.push(...got.filter((q) => !gone.includes(q.id)));
      if (!gone.length) break;
    }
    if (!results.length) return [];
    // IN THE ORDER IT WAS CHOSEN: an UPDATE returns its rows in none.
    results.sort((a, b) => b.waits - a.waits || (a.due_at < b.due_at ? -1 : a.due_at > b.due_at ? 1 : a.id < b.id ? -1 : 1));
    // A SPOT NOBODY ASKS FOR ANY MORE still owes its volunteers a verdict, so it
    // comes with its withdrawn demands: without one it is claimed, skipped and
    // released forever, ahead of every question behind it.
    const { results: ds } = await db.prepare(
      `SELECT question, producer, ref, payload, state FROM demands WHERE question IN ${listOf(db, "?")}
        ORDER BY state = 'live' DESC, state <> 'withdrawn' DESC, producer, ref`)
      .bind(JSON.stringify(results.map((q) => q.id))).all();
    return results.map((q) => {
      const mine = ds.filter((d) => d.question === q.id);
      const kept = mine.some((d) => d.state !== "withdrawn") ? mine.filter((d) => d.state !== "withdrawn") : mine;
      return { question: q.id, verb: q.verb, state: q.state, lease: q.lease, request: JSON.parse(q.request),
        demands: kept.map((d) => ({ producer: d.producer, ref: d.ref, payload: JSON.parse(d.payload) })) };
    });
  }


  const releaseAllOf = (env, device, now = Date.now()) => env.LIBRARY.prepare(
    `UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE ${inIdOrder("leased_to = ? AND lease_until > ?")}`).bind(device, iso(now));
  const headOf = (env, w, engine, now = Date.now(), d = DIAL_DEFAULTS) => env.LIBRARY.prepare(HEAD)
    .bind(...eligibleArgs(w, engine, iso(now), d));
  const packable = (q) => !q.waits && !!(verbs[q.verb] || {}).pack;

  const HEAD = `SELECT * FROM factory_head(?1::text, ?2::bigint, ?3::bigint, ?4::text, ?5::text, ?6::text, ?7::text, ?8::text,
    ?9::bigint, ?10::bigint, ?11::text)`;
  function eligibleArgs(w, engine, at, d = DIAL_DEFAULTS) {
    return [at, w.official ? 1 : 0, w.lanes ?? 1, engine, JSON.stringify(w.siblings || [w.device]), w.owner || w.device, w.net || "", w.before ?? null,
      ceilingS(d.row_ceiling_ms), ceilingS(d.settle_grace_ms), JSON.stringify(OFFICIAL_STATES)];
  }


  return { next, nextPack, renew, renewRow, renewMany, release, releaseAll, releaseUnnamed, refuseRows, claim, releaseAllOf, headOf, packable };
}
