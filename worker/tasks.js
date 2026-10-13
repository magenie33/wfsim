// SPDX-License-Identifier: AGPL-3.0-or-later
// THE FACT FACTORY — docs/BOARD.md §"Compute orders". One thing is done here:
// a QUESTION (a verb on a canonical request) becomes a FACT (enough independent
// witnesses with equal canonical answers). Board rows, riven gains in a chat and
// a survey's shapes are PRODUCERS: each asks for questions (`demand`) and is told
// the facts (`on_fact`); none of them is named below. A new kind of task is a new
// producer, and this file does not change.
//
// Two properties hold, and scripts/check_tasks.mjs holds them on random books:
//   LIVENESS     `next` returns null only when no question exists this witness
//                may answer — eligibility is ONE query, never a filter after a
//                LIMIT, which is how eight unanswerable rows once starved a survey;
//   TERMINATION  every question ends `fact`, `withdrawn` or with the official
//                machines (`disputed`, `spot`, `nondeterministic`): none waits on
//                answers that will never come.
//
// ORDER: a question a person is waiting on now first, then earliest deadline
// first. A deadline alone is not enough: a survey of a thousand shapes asked at
// once is a thousand deadlines passed together, and by deadline alone a person
// asking in a chat would wait behind every one of them.
import { iso } from "./instant.js";

/// HOW STATES LEAVE (§"States"). `open` takes volunteers' answers; the cap on
/// them reached without agreement is `disputed`, or `nondeterministic` when every
/// answer differs from every other — independent witnesses all disagreeing is an
/// engine whose answer is not deterministic, not a liar, and nobody is refused.
/// `spot` is a fact the official machines check again. Those three are the
/// official machines' alone.
export const OFFICIAL_STATES = ["disputed", "spot", "nondeterministic"];

/// A PACK'S LEASE IS A HEARTBEAT, not a deadline: a page still computing a row
/// renews it (`renewRow`) and is never cut off; only a silent one loses it.
/// Each renewal holds the row this much longer — a few missed beats' worth.
export const RENEW_MS = 5 * 60_000;
/// WHAT AN OLDER ENGINE'S ANSWER EARNS WHEN ITS ENGINE PARTED FROM THE FACT
/// (`joinersParted`): at most what an answer on the served engine earns, so
/// staying on an old engine never pays better than updating.
export const GRACE_SHARE = 1;
/// …AND PAST THIS LONG FROM THE LEASE another computer may take it too, the
/// first holder computing on (`lease_prev`): whichever answers first makes it.
/// Nobody is cut off, and a page that beats but never answers holds nothing.
export const ROW_CEILING_MS = 2 * 3_600_000;
/// A LEASE ANOTHER MAY TAKE: none, run out, or held past the ceiling. In SQL,
/// against the instant bound as ?1.
const TAKEABLE = `(lease_until IS NULL OR lease_until < ?1::text
  OR leased_at < to_char(((?1::text)::timestamptz AT TIME ZONE 'UTC') - interval '${ROW_CEILING_MS / 1000} seconds', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))`;
/// …AND TAKING ONE keeps the holder computing longest as the one before, its
/// answer still counted: one still beating there stays, else the one taken
/// from if it still beats; a lease run out keeps nobody.
const SUPERSEDE = `
  lease_prev = CASE WHEN lease_prev_until >= ?1::text THEN lease_prev WHEN lease_until >= ?1::text THEN lease END,
  leased_to_prev = CASE WHEN lease_prev_until >= ?1::text THEN leased_to_prev WHEN lease_until >= ?1::text THEN leased_to END,
  lease_prev_until = CASE WHEN lease_prev_until >= ?1::text THEN lease_prev_until WHEN lease_until >= ?1::text THEN lease_until END`;

/// Text whose bytes two equal values share: keys sorted, at every depth.
export const canon = (v) => (Array.isArray(v) ? `[${v.map(canon).join(",")}]`
  : v && typeof v === "object" ? `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canon(v[k])}`).join(",")}}`
  : JSON.stringify(v));

/// AN ANSWER A FEW ULP OFF ANOTHER, everything else equal: a score folded in
/// another grouping (docs/WASM.md "one grouping is one answer"). Bits are still
/// the witness — it is never paid as agreeing — but it is not a lie, so the
/// official machines refuse nobody for it (`settleOfficial`).
const SCORE_NEAR = 1e-12;
/// WHEN THE WATCHDOG SPEAKS of a backlog past due: this many questions, or any
/// left this long past its deadline; settling runs hourly, so less is a queue.
const OVERDUE_MAX = 1000, OVERDUE_HOURS_MS = 3 * 3_600_000;
/// …AND OF A QUESTION WAITING ON THEM: settling is an hourly cron GitHub
/// starts up to ~45 min late, so under two hours is the next run not yet come.
const UNSETTLED_MS = 2 * 3_600_000;
/// …AND OF CLIENTS DISAGREEING: this many answers in the hour, and this share
/// of all of them. A healthy hour runs under one in a hundred.
const DISAGREE_MIN = 30, DISAGREE_SHARE = 0.03;
const sansWork = (c) => { const v = JSON.parse(c); delete v.work; return canon(v); };
export const nearAnswer = (same) => (x, y) => {
  let a, b;
  try { a = JSON.parse(x); b = JSON.parse(y); } catch (_) { return false; }
  if (!a || !b || typeof a.score !== "number" || typeof b.score !== "number") return false;
  if (!same.every((k) => canon(a[k]) === canon(b[k]))) return false;
  return Math.abs(a.score - b.score) <= SCORE_NEAR * Math.max(Math.abs(a.score), Math.abs(b.score));
};

const hex = (buf) => [...new Uint8Array(buf)].map((x) => x.toString(16).padStart(2, "0")).join("");
/// A QUESTION'S ID IS WHAT IT ASKS: two producers asking one thing share it.
export async function questionId(verb, request) {
  return hex(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`${verb}\n${canon(request)}`))).slice(0, 32);
}
const newLease = () => hex(crypto.getRandomValues(new Uint8Array(16)));

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

/// THE FACTORY, given what varies: `verbs` — how each is answered
/// ({ cap, lease_ms, canon(result, work), spot? }) — `producers` — who asks and who is
/// told ({ still_wanted?, on_answer?, on_fact }) — the trust a fact needs
/// (`threshold`, a volunteer counting one and the official machines all of it),
/// the share of facts the official machines check again (`spot`), and what
/// happens to a witness they disprove (`on_disproved`).
export function factory({ verbs, producers, threshold = 2, spot = 0.05, recheck = 0, random = () => Math.random(), on_disproved = null, refusing = true }) {
  /// THE VERBS A PACK MAY CARRY (`pack: true`): short, many, nobody waiting on one.
  const PACKABLE = JSON.stringify(Object.keys(verbs).filter((v) => verbs[v].pack));
  const producerOf = (name) => {
    const p = producers[name];
    if (!p) throw new Error(`no producer ${name}`);
    return p;
  };

  /// A JSON ARRAY BOUND AS PARAMETER `p`, as a set of values.
  const listOf = (db, p) => `(SELECT value FROM jsonb_array_elements_text((${p}::text)::jsonb))`;

  async function liveDemands(db, id) {
    return (await db.prepare("SELECT * FROM demands WHERE question = ? AND state = 'live'").bind(id).all()).results;
  }

  /// A PRODUCER ASKS. The question is opened, or shared when another producer
  /// asked it already; a fact the served engine already holds is told at once.
  /// `waits`: a person is waiting on it now — §"Order".
  async function demand(env, ask, engine, now = Date.now()) {
    return (await demandAll(env, [ask], engine, now))[0];
  }

  /// MANY ASKS AT ONCE, written in a few round trips rather than three each — a
  /// producer asking thousands an hour is otherwise slower than its own clock.
  const DEMAND_CHUNK = 50;
  async function demandAll(env, asks, engine, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), ids = [];
    for (const a of asks) {
      if (!verbs[a.verb]) throw new Error(`no verb ${a.verb}`);
      producerOf(a.producer);
      ids.push(await questionId(a.verb, a.request));
    }
    for (let i = 0; i < asks.length; i += DEMAND_CHUNK) {
      const writes = [];
      asks.slice(i, i + DEMAND_CHUNK).forEach(({ producer, ref, verb, request, due_in_ms, payload = {}, lanes = null, waits = false }, k) => {
        const id = ids[i + k], due = iso(now + due_in_ms);
        writes.push(
          db.prepare(`INSERT INTO questions AS t (id, verb, request, state, due_at, opened_at, lanes, waits) VALUES (?, ?, ?, 'open', ?, ?, ?, ?)
                      ON CONFLICT (id) DO UPDATE SET
                        state = CASE WHEN t.state = 'withdrawn' THEN 'open' ELSE t.state END,
                        opened_at = CASE WHEN t.state = 'withdrawn' THEN excluded.opened_at ELSE t.opened_at END,
                        due_at = CASE WHEN t.state = 'withdrawn' OR excluded.due_at < t.due_at THEN excluded.due_at ELSE t.due_at END,
                        waits = CASE WHEN t.state = 'withdrawn' THEN excluded.waits ELSE GREATEST(t.waits, excluded.waits) END,
                        lanes = COALESCE(t.lanes, excluded.lanes)`)
            .bind(id, verb, canon(request), due, at, lanes ? JSON.stringify(lanes) : null, waits ? 1 : 0),
          db.prepare(`INSERT INTO demands (producer, ref, question, state, asked_at, due_at, waits, payload) VALUES (?, ?, ?, 'live', ?, ?, ?, ?)
                      ON CONFLICT (producer, ref) DO UPDATE SET question = excluded.question, state = 'live', asked_at = excluded.asked_at,
                        due_at = excluded.due_at, waits = excluded.waits, payload = excluded.payload, served_at = NULL`)
            .bind(producer, ref, id, at, due, waits ? 1 : 0, JSON.stringify(payload)));
      });
      await db.batch(writes);
    }
    const { results } = await db.prepare(`SELECT * FROM questions WHERE id IN ${listOf(db, "?")} AND state IN ('fact', 'spot')`)
      .bind(JSON.stringify([...new Set(ids)])).all();
    for (const q of results) {
      // A FACT OF ANOTHER ENGINE answers nothing now: the question opens again,
      // its answers carried for the served engine to reproduce or replace.
      if (q.fact_engine !== engine) {
        await db.prepare("UPDATE questions SET state = 'open', opened_at = ? WHERE id = ? AND state = ?").bind(at, q.id, q.state).run();
      } else {
        await deliver(env, q, now);
      }
    }
    return ids;
  }

  /// A PRODUCER NO LONGER ASKS. A question nobody asks for any more is withdrawn,
  /// and one still asked for keeps the earliest deadline still standing.
  async function withdraw(env, producer, ref, now = Date.now()) {
    const db = env.LIBRARY;
    const d = await db.prepare("SELECT question FROM demands WHERE producer = ? AND ref = ?").bind(producer, ref).first();
    if (!d) return;
    await db.prepare("UPDATE demands SET state = 'withdrawn' WHERE producer = ? AND ref = ? AND state = 'live'").bind(producer, ref).run();
    await restate(db, d.question, now);
  }

  async function restate(db, id, now) {
    const left = await db.prepare("SELECT min(due_at) AS due, max(waits) AS waits, count(*) AS n FROM demands WHERE question = ? AND state = 'live'")
      .bind(id).first();
    if (left.n) {
      await db.prepare("UPDATE questions SET due_at = ?, waits = ? WHERE id = ?").bind(left.due, left.waits, id).run();
    } else {
      await db.prepare("UPDATE questions SET state = 'withdrawn', lease = NULL, lease_until = NULL, leased_to = NULL WHERE id = ? AND state = 'open'")
        .bind(id).run();
    }
  }

  /// THE QUESTION THIS WITNESS ANSWERS NEXT, leased, or null — §"Order" among
  /// exactly the questions it may answer. `w`: { device, owner,
  /// siblings, net, lanes, official, most, before }: `lanes` 0 is a witness with
  /// no cores free now (asking ahead), which a question someone waits on is never
  /// handed; `before` keeps to questions due before then.
  async function next(env, w, engine, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), most = w.most ?? 1;
    for (let tries = 0; tries < 8; tries++) {
      const q = await db.prepare(eligibleSql(w.official)).bind(...eligibleArgs(w, engine, at)).first();
      if (!q) return null;
      // A ROW NOBODY OWES ANY MORE is withdrawn where it is found, and the next
      // one is asked for.
      const live = await liveDemands(db, q.id);
      const wanted = [];
      for (const d of live) {
        const p = producerOf(d.producer);
        if (!p.still_wanted || await p.still_wanted(env, { ...d, payload: JSON.parse(d.payload) })) wanted.push(d);
        else await db.prepare("UPDATE demands SET state = 'withdrawn' WHERE producer = ? AND ref = ?").bind(d.producer, d.ref).run();
      }
      if (q.state === "open" && !wanted.length) {
        await restate(db, q.id, now);
        continue;
      }
      const lease = newLease();
      // THE RACE IS THE DATABASE'S, and so is the cap on what one witness holds.
      const took = await db.prepare(`UPDATE questions SET ${SUPERSEDE}, lease = ?2::text, lease_until = ?3::text, leased_to = ?4::text, leased_at = ?1::text
          WHERE id = ?5::text AND state = ?6::text AND ${TAKEABLE}
            AND (SELECT count(*) FROM questions h WHERE h.leased_to = ?4::text AND h.lease_until > ?1::text) < ?7::bigint`)
        .bind(at, lease, iso(now + verbs[q.verb].lease_ms), w.device, q.id, q.state, most).run();
      if (took.meta.changes) {
        for (const d of wanted) {
          const p = producerOf(d.producer);
          if (p.on_lease) await p.on_lease(env, { ...d, payload: JSON.parse(d.payload) }, now);
        }
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
  /// ON POSTGRES THE PICK SKIPS ROWS ANOTHER PICK HOLDS LOCKED (`SKIP LOCKED`):
  /// two asks at once took the same head rows and deadlocked.
  async function nextPack(env, w, engine, { n, lease_ms, most = 1, exclude = null }, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), lease = newLease();
    const [taken, ds] = await db.batch([
      db.prepare(`UPDATE questions SET ${SUPERSEDE}, lease = ?9::text, lease_until = ?10::text, leased_to = ?11::text, leased_at = ?1::text
          WHERE id IN (SELECT q.id ${eligibleFrom(false)} AND q.waits = 0 AND q.verb IN (SELECT value FROM jsonb_array_elements_text((?12::text)::jsonb))
            AND (?15::text IS NULL OR (q.request::jsonb #>> '{record,weapon}') NOT IN (SELECT value FROM jsonb_array_elements_text((?15::text)::jsonb))) ${ORDER} LIMIT ?13::bigint FOR UPDATE OF q SKIP LOCKED)
            AND (SELECT count(DISTINCT h.lease) FROM questions h WHERE h.leased_to = ?11::text AND h.lease_until > ?1::text AND h.lease <> ?9::text) < ?14::bigint
          RETURNING id, verb, request, due_at`)
        .bind(...eligibleArgs(w, engine, at), lease, iso(now + lease_ms), w.device, PACKABLE, n, most, exclude ? JSON.stringify(exclude) : null),
      db.prepare(`SELECT d.question, d.producer, d.ref, d.payload FROM demands d JOIN questions q ON q.id = d.question
          WHERE q.lease = ? AND q.leased_to = ? AND d.state = 'live'`).bind(lease, w.device),
    ]);
    if (!taken.results.length) return null;
    const demands = ds.results.map((d) => ({ ...d, payload: JSON.parse(d.payload) }));
    // A ROW NOBODY OWES ANY MORE, asked of its producer for the whole pack at once.
    const unwanted = new Set();
    for (const name of new Set(demands.map((d) => d.producer))) {
      const p = producerOf(name), mine = demands.filter((d) => d.producer === name);
      if (p.wanted_many) {
        const keep = await p.wanted_many(env, mine);
        for (const d of mine) if (!keep.has(d.ref)) unwanted.add(d);
      } else if (p.still_wanted) {
        for (const d of mine) if (!(await p.still_wanted(env, d))) unwanted.add(d);
      }
    }
    for (const d of unwanted) {
      await db.prepare("UPDATE demands SET state = 'withdrawn' WHERE producer = ? AND ref = ?").bind(d.producer, d.ref).run();
      await restate(db, d.question, now);
    }
    const wanted = demands.filter((d) => !unwanted.has(d));
    const gone = taken.results.filter((q) => !wanted.some((d) => d.question === q.id)).map((q) => q.id);
    if (gone.length) {
      await db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
          WHERE id IN ${listOf(db, "?")} AND lease = ?`).bind(JSON.stringify(gone), lease).run();
    }
    for (const d of wanted) {
      const p = producerOf(d.producer);
      if (p.on_lease) await p.on_lease(env, d, now);
    }
    const questions = taken.results.filter((q) => !gone.includes(q.id))
      .sort((a, b) => (a.due_at < b.due_at ? -1 : a.due_at > b.due_at ? 1 : a.id < b.id ? -1 : 1))
      .map((q) => ({ question: q.id, verb: q.verb, request: JSON.parse(q.request), due_at: q.due_at,
        demands: wanted.filter((d) => d.question === q.id).map(({ producer, ref, payload }) => ({ producer, ref, payload })) }));
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
    const db = env.LIBRARY, most = "GREATEST";
    const [kept, before] = await db.batch([
      db.prepare(`UPDATE questions SET lease_until = ${most}(lease_until, ?4)
                   WHERE id = ?1 AND lease = ?2 AND leased_to = ?3 RETURNING id`)
        .bind(question, lease, device, iso(now + RENEW_MS)),
      db.prepare(`UPDATE questions SET lease_prev_until = ${most}(lease_prev_until, ?4)
                   WHERE id = ?1 AND lease_prev = ?2 AND leased_to_prev = ?3 RETURNING id`)
        .bind(question, lease, device, iso(now + RENEW_MS)),
    ]);
    return { held: kept.results.length > 0 || before.results.length > 0 };
  }
  /// `keep` is a row the page will resume (69-board-work.js `RESUME_KEY`): left
  /// under the lease, which runs out unless the page comes back to renew it.
  async function release(env, device, lease, keep = null) {
    const db = env.LIBRARY;
    await db.prepare("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE lease = ?::text AND leased_to = ?::text AND id IS DISTINCT FROM ?::text")
      .bind(lease, device, keep).run();
  }
  /// EVERYTHING A WITNESS HOLDS goes back: one that asks for work again is
  /// working on nothing (worker/verify.js, asking ahead).
  async function releaseAll(env, device, now = Date.now()) {
    await releaseAllOf(env, device, now).run();
  }
  /// …AS ONE STATEMENT, and the head of a witness's order as another, for a
  /// caller to read in its own batch: an ask is then one round trip, not five.
  const releaseAllOf = (env, device, now = Date.now()) => env.LIBRARY.prepare(
    "UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE leased_to = ? AND lease_until > ?").bind(device, iso(now));
  const headOf = (env, w, engine, now = Date.now()) => env.LIBRARY.prepare(eligibleSql(w.official)).bind(...eligibleArgs(w, engine, iso(now)));
  /// WHETHER A HEAD GOES IN A PACK, or alone (`next`).
  const packable = (q) => !q.waits && !!(verbs[q.verb] || {}).pack;

  /// A WITNESS ANSWERS, under its lease. `w`: { device, owner, net, official }.
  /// `counts`, `lanes` and `compute_ms` are what the answer cost and on how many
  /// cores — kept beside it to fit what a point is worth, never credited.
  /// Returns `{ taken }`: whether the lease was still its own.
  async function answer(env, w, { lease, ...a }, now = Date.now()) {
    const q = await env.LIBRARY.prepare("SELECT * FROM questions WHERE lease = ? AND leased_to = ? AND lease_until >= ?")
      .bind(lease, w.device, iso(now)).first();
    if (!q) return { taken: false };
    await recordMany(env, w, [{ q, a }], now);
    return { taken: true };
  }

  /// A PACK'S ANSWERS, together: those whose question the pack still holds are
  /// recorded and settled in a few round trips, whatever their number; `done`
  /// gives back the rest of the pack but `keep`. Returns the questions taken.
  /// AN OLDER ENGINE'S ANSWERS (`served` the engine the site serves) settle
  /// under the served one: its rows were handed out because their bits are the same
  /// — or, `join`, because they may be, and then each only joins a served one's.
  async function answerPack(env, w, { lease, engine, served = null, join = false, answers, done = false, keep = null }, now = Date.now()) {
    const db = env.LIBRARY;
    const { results: held } = await db.prepare(`SELECT * FROM questions WHERE id IN (SELECT value FROM jsonb_array_elements_text((?4::text)::jsonb))
         AND ((lease = ?1::text AND leased_to = ?2::text AND lease_until >= ?3::text) OR (lease_prev = ?1::text AND leased_to_prev = ?2::text))`)
      .bind(lease, w.device, iso(now), JSON.stringify(answers.map((a) => a.question))).all();
    const byId = new Map(held.map((q) => [q.id, q]));
    // A PAGE SENDS AGAIN what got no reply, and the reply may be all that was lost:
    // an answer this device already gave on this engine is not a second witness
    // (the ask never hands it the same question twice, `nextPack`).
    if (byId.size) {
      const { results: given } = await db.prepare("SELECT question FROM answers WHERE device = ?1::text AND engine = ?2::text AND question IN (SELECT jsonb_array_elements_text((?3::text)::jsonb))")
        .bind(w.device, engine, JSON.stringify([...byId.keys()])).all();
      for (const r of given) byId.delete(r.question);
    }
    const items = answers.filter((a) => byId.has(a.question)).map(({ question, ...a }) => ({ q: byId.get(question), a: { ...a, engine } }));
    if (items.length) await recordMany(env, w, items, now, served, join);
    if (done) await release(env, w.device, lease, keep);
    return { taken: items.map((x) => x.q.id) };
  }

  /// THE OFFICIAL MACHINES ANSWER WITHOUT A LEASE: they are trusted, and what they
  /// measured is the fact whoever held the question. Returns the question's
  /// state after it, or null for a question that does not exist.
  async function answerOfficial(env, w, { question, ...a }, now = Date.now()) {
    const db = env.LIBRARY;
    const q = await db.prepare("SELECT * FROM questions WHERE id = ?").bind(question).first();
    if (!q) return null;
    await recordMany(env, { ...w, official: true }, [{ q, a }], now);
    return (await db.prepare("SELECT state FROM questions WHERE id = ?").bind(question).first()).state;
  }

  /// ONE WITNESS'S ANSWERS `[{ q, a }]`, all of one engine: written in one round
  /// trip, then settled together (`settleMany`).
  async function recordMany(env, w, items, now, served = null, join = false) {
    const db = env.LIBRARY, at = iso(now), official = w.official ? 1 : 0, mine = items[0].a.engine, engine = served || mine;
    const writes = items.map(({ q, a }) => db.prepare(
      `INSERT INTO answers (question, engine, device, owner, net, official, canon, result, work, compute_ms, counts, lanes, at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
      .bind(q.id, a.engine, w.device, w.owner || w.device, w.net || "", official, verbs[q.verb].canon(a.result, a.work), JSON.stringify(a.result),
        a.work ?? 0, a.compute_ms ?? null, a.counts ? JSON.stringify(a.counts) : null, a.lanes ?? null, at));
    await db.batch(writes);
    if (!official) {
      // …AND WHEN IT LAST ANSWERED and what it spent, in its hour (docs/BOARD.md
      // §"Contribution") — a statistic, so each on its own and outside the answers'
      // transaction: inside it, it held the device's row against every credit.
      const ms = items.reduce((t, { a }) => t + (a.compute_ms || 0), 0);
      await db.prepare("UPDATE verifiers SET compute_ms = compute_ms + ?, last_at = ? WHERE id = ?").bind(ms, at, w.device).run();
      await db.prepare(`INSERT INTO verifier_hours AS h (verifier, hour, tasks, ms) VALUES (?, ?, ?, ?)
                    ON CONFLICT (verifier, hour) DO UPDATE SET tasks = h.tasks + excluded.tasks, ms = h.ms + excluded.ms`)
        .bind(w.device, at.slice(0, 13), items.length, ms).run();
    }
    const answered = new Map(items.map(({ q, a }) => [q.id,
      { question: q.id, device: w.device, net: w.net || "", engine: a.engine, result: a.result, work: a.work, official: !!official, at }]));
    const ids = JSON.stringify([...answered.keys()]);
    // THE LEASE GOES ONLY ONCE THE QUESTION IS SETTLED, in the settling's own
    // write: the answered question heads every witness's order, so freed before
    // it became a fact it was taken at once and answered again for nothing.
    await settleMany(env, [...answered.keys()], engine, now, { answered, peer: mine !== engine && !join ? mine : null, after: [
      db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
                   WHERE id IN ${listOf(db, "?")} AND leased_to = ?`).bind(ids, w.device)] });
  }

  /// WHAT THE ANSWERS NOW MAKE OF A QUESTION (§"States").
  async function settle(env, id, engine, now) {
    await settleMany(env, [id], engine, now);
  }

  /// …OF MANY QUESTIONS AT ONCE: read in one round trip, decided here, written in
  /// one more. Every write is guarded — a state moves only from `open`, an answer
  /// is credited only once — so two witnesses settling one question at the same
  /// moment cannot both count. `answered` tells each question's producers of the
  /// answer just recorded (`on_answer`); `after` is written with the rest.
  async function settleMany(env, ids, engine, now, { answered = null, after = [], peer = null } = {}) {
    const db = env.LIBRARY, list = JSON.stringify([...new Set(ids)]), ids_ = listOf(db, "?");
    const [qs, as, ds] = await db.batch([
      db.prepare(`SELECT * FROM questions WHERE id IN ${ids_}`).bind(list),
      db.prepare(`SELECT * FROM answers WHERE question IN ${ids_} AND void = 0 ORDER BY id`).bind(list),
      db.prepare(`SELECT * FROM demands WHERE question IN ${ids_} AND state = 'live'`).bind(list),
    ]);
    const writes = [], later = [], credits = [];
    for (const q of qs.results) {
      const demands = ds.results.filter((d) => d.question === q.id);
      const told = answered && answered.get(q.id);
      if (told) {
        for (const d of demands) {
          const p = producerOf(d.producer);
          if (p.on_answer) await p.on_answer(env, { ...d, payload: JSON.parse(d.payload) }, told);
        }
      }
      const all = as.results.filter((a) => a.question === q.id);
      const served = all.filter((a) => a.engine === engine);
      const officialSays = served.filter((a) => a.official).at(-1);
      if (officialSays) await settleOfficial(env, q, all, served, officialSays, engine, now);
      else if (q.state === "open") decide(env, q, all, peer ? all.filter((a) => a.engine === engine || a.engine === peer) : served, demands, engine, now, writes, later, credits);
    }
    // CREDITS AFTER THE COMMIT (`credit`): two settlements crediting the same
    // devices in different orders deadlock, so no settlement holds a
    // device's row.
    writes.push(...after);
    await commit(env, writes);
    await credit(db, credits, now);
    for (const f of later) await f();
  }

  /// THE FACTORY'S WRITES AND ITS PRODUCERS' (`{ side }`, worker/tasks.js
  /// `deliverPlan`) in one transaction.
  async function commit(env, writes) {
    if (!writes.length) return;
    await env.LIBRARY.batch(writes.map((w) => w.side || w));
  }

  /// AN OPEN QUESTION'S VOLUNTEER ANSWERS, judged — a fact, the cap reached, or
  /// still open — its writes added to `writes`.
  function decide(env, q, all, served, demands, engine, now, writes, later, credits) {
    const db = env.LIBRARY, needs = q.needs || threshold;
    // A GROUP OF EQUAL ANSWERS holds one of the served engine; an older engine's
    // answer joins one (a release that did not move the bits) and never starts one
    // — unless it is still accepted on this row (`peer`), when it leads as one.
    for (const lead of served) {
      // The served engine's answers are taken first, so the witnesses it
      // reproduced are credited beside it rather than instead of it.
      const equal = all.filter((a) => a.canon === lead.canon);
      const group = independent([...equal.filter((a) => a.engine === engine), ...equal.filter((a) => a.engine !== engine)]);
      if (group.length < needs) continue;
      // A RECHECK: one more independent volunteer before it is a fact, nothing
      // credited meanwhile. One who differs makes it a dispute, which the
      // official machines settle and refuse the liar in — so a recheck catches
      // a pair agreeing on a lie whenever the third is honest.
      if (!q.needs && group.length === threshold && random() < (verbs[q.verb].recheck ?? recheck)) {
        writes.push(db.prepare("UPDATE questions SET needs = ? WHERE id = ? AND state = 'open' AND needs IS NULL").bind(threshold + 1, q.id));
        return;
      }
      const state = random() < (verbs[q.verb].spot ?? spot) ? "spot" : "fact";
      writes.push(db.prepare("UPDATE questions SET state = ?, canon = ?, fact_engine = ?, fact_at = ? WHERE id = ? AND state = 'open'")
        .bind(state, lead.canon, engine, iso(now), q.id));
      credits.push(...group, ...joinersParted(all, served, group, lead, engine));
      deliverPlan(env, { ...q, canon: lead.canon }, now, lead, demands, writes, later);
      return;
    }
    // ONLY WITNESSES FILL THE CAP: a browser claimed by an owner after it took
    // the question answers as that owner's second, which disagrees with nobody.
    const cap = Math.max(verbs[q.verb].cap, needs), witnesses = independent(served);
    if (witnesses.length >= cap) {
      const distinct = new Set(witnesses.map((a) => a.canon)).size === witnesses.length;
      writes.push(db.prepare("UPDATE questions SET state = ? WHERE id = ? AND state = 'open'")
        .bind(distinct && witnesses.length >= 3 ? "nondeterministic" : "disputed", q.id));
    }
  }

  /// AN OLDER ENGINE'S ANSWERS THAT PARTED FROM THE FACT, paid `GRACE_SHARE`
  /// of their work: computed in good faith on the engine its owner had, and
  /// less, so an engine is worth updating. Only where two independent answers
  /// of that engine agree with each other — the engine's own bits, which one
  /// alone cannot tell from a made-up number.
  function joinersParted(all, served, group, lead, engine) {
    const parted = all.filter((a) => a.engine !== engine && !a.official && !served.includes(a) && !group.includes(a) && a.canon !== lead.canon);
    const out = [];
    for (const key of new Set(parted.map((a) => `${a.engine}|${a.canon}`))) {
      const agreeing = independent(parted.filter((a) => `${a.engine}|${a.canon}` === key));
      if (agreeing.length >= 2) out.push(...agreeing.map((a) => ({ ...a, work: Math.floor((a.work || 0) * GRACE_SHARE) })));
    }
    return out;
  }

  async function settleOfficial(env, q, all, served, officialSays, engine, now) {
    const db = env.LIBRARY, id = q.id;
    // AN OFFICIAL ANSWER WITHOUT WORK — a native build's, which counts no gas —
    // witnesses the score alone: it is the fact, and a volunteer who gave the
    // score is owed a check of the work by the witness that counts it (the
    // wasm, scripts/live_orders.mjs), so the question is spot-checked.
    if (JSON.parse(officialSays.canon).work === null) {
      const owed = all.some((a) => !a.official && sansWork(a.canon) === sansWork(officialSays.canon));
      await db.prepare("UPDATE questions SET state = ?, canon = ?, fact_engine = ?, fact_at = ?, odd = 0 WHERE id = ?")
        .bind(owed ? "spot" : "fact", officialSays.canon, engine, iso(now), id).run();
      if (q.state !== "spot") await deliver(env, { ...q, canon: officialSays.canon }, now, officialSays);
      return;
    }
    // THE OFFICIAL MACHINES' ANSWER IS THE FACT, whatever state it was in. The
    // volunteers who gave its bits are credited; those it disproves on its own
    // engine are refused — unless the question was not deterministic.
    // BITS ARE THE WITNESS: a volunteer of any engine that gave their bits
    // agreed; a refusal needs the same engine, since another may differ honestly.
    // THE SCORE IS THE WITNESS AND THE OFFICIAL WORK IS THE PAY: a volunteer
    // who gave the official score agreed, whatever work it counted, and on the
    // official engine is paid the official work - an engine counting other work
    // is a client to fix, not a liar, and over-counting earns nothing. Another
    // engine prices work in its own units, so there it keeps its own.
    const gave = (a) => sansWork(a.canon) === sansWork(officialSays.canon);
    const officialWork = JSON.parse(officialSays.canon).work;
    const agree = independent(all.filter((a) => !a.official && gave(a)))
      .map((a) => (a.engine === engine && Number.isSafeInteger(officialWork) ? { ...a, work: officialWork } : a));
    const near = verbs[q.verb].near || (() => false);
    const wrong = served.filter((a) => !a.official && !gave(a) && !near(a.canon, officialSays.canon));
    const moved = q.canon !== null && q.canon !== officialSays.canon;
    // A DISPUTE THE OFFICIAL MACHINES SIDE WITH NOBODY IN is not a liar found:
    // every answer differs from every other, theirs too, and that is an engine
    // whose answer is not deterministic. Nobody is refused; the watchdog says so.
    const odd = q.state === "nondeterministic" || (q.state === "disputed" && !agree.length);
    await credit(db, agree, now);
    await db.prepare("UPDATE questions SET state = 'fact', canon = ?, fact_engine = ?, fact_at = ?, odd = ? WHERE id = ?")
      .bind(officialSays.canon, engine, iso(now), odd ? 1 : 0, id).run();
    // ONLY A DISPUTE OR A SPOT CHECK REFUSES: an open question past its deadline
    // that they answered differently is measured again, its answer unpaid —
    // one witness against them is not yet a witness found lying.
    if (refusing && !odd && (q.state === "disputed" || q.state === "spot")) {
      for (const a of wrong) await (on_disproved ? on_disproved(env, a) : refuse(env, a.device, now));
    }
    if (q.state !== "spot" || moved) await deliver(env, { ...q, canon: officialSays.canon }, now, officialSays);
  }

  /// THE ANSWERS THAT COUNT AS WITNESSES, in the order they came: no owner and no
  /// network twice. Two browsers on one desk are one witness.
  function independent(answers) {
    const owners = new Set(), nets = new Set(), out = [];
    for (const a of answers) {
      if (owners.has(a.owner) || (a.net && nets.has(a.net))) continue;
      owners.add(a.owner);
      if (a.net) nets.add(a.net);
      out.push(a);
    }
    return out;
  }

  /// ONLY A FACT SCORES, once per answer: its work to its device, in its hour,
  /// and one fact toward forgiving a refusal (docs/BOARD.md §"Contribution").
  /// A CREDIT IS PAID IN TWO SHORT STEPS, each its own statement:
  /// the answers flip to credited (their RETURNING is what was not yet paid, so
  /// each is paid once), then each device is paid their sum in one update, in
  /// device order. A device's row is held for one statement, never a settlement.
  async function credit(db, answers, now) {
    // THE WORK PAID IS THE ANSWER'S AS GIVEN HERE, which may be the official work
    // in its place (`settleOfficial`), never re-read from the table.
    const owed = new Map(answers.filter((a) => !a.official && !a.credited).map((a) => [Number(a.id), a]));
    const ids = [...owed.keys()];
    if (!ids.length) return;
    const { results: paid } = await db.prepare(
      `UPDATE answers SET credited = 1 WHERE id IN (SELECT value::bigint FROM jsonb_array_elements_text((?::text)::jsonb)) AND credited = 0
        RETURNING id`).bind(JSON.stringify(ids)).all();
    const by = new Map();
    for (const p of paid) {
      const a = owed.get(Number(p.id));
      const d = by.get(a.device) || { work: 0, n: 0 };
      d.work += a.work; d.n += 1; by.set(a.device, d);
    }
    const hour = iso(now).slice(0, 13);
    for (const [device, d] of [...by].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) {
      await db.prepare(`UPDATE verifiers SET work = work + ?::bigint, agreed = agreed + ?::bigint,
          refusals = CASE WHEN refusals > 0 AND clean + ?::bigint >= refusals * ?::bigint THEN refusals - 1 ELSE refusals END,
          clean = CASE WHEN refusals = 0 OR clean + ?::bigint >= refusals * ?::bigint THEN 0 ELSE clean + ?::bigint END
        WHERE id = ?::text`).bind(d.work, d.n, d.n, FACTS_PER_REFUSAL_FORGIVEN, d.n, FACTS_PER_REFUSAL_FORGIVEN, d.n, device).run();
      await db.prepare(`INSERT INTO verifier_hours AS h (verifier, hour, work) VALUES (?::text, ?::text, ?::bigint)
          ON CONFLICT (verifier, hour) DO UPDATE SET work = h.work + excluded.work`).bind(device, hour, d.work).run();
    }
  }

  /// EVERY LIVE DEMAND IS TOLD THE FACT, once.
  async function deliver(env, q, now, by = null) {
    const db = env.LIBRARY;
    const answerOf = by || await db.prepare("SELECT * FROM answers WHERE question = ? AND canon = ? ORDER BY official DESC, id LIMIT 1")
      .bind(q.id, q.canon).first();
    const writes = [], later = [];
    deliverPlan(env, q, now, answerOf, await liveDemands(db, q.id), writes, later);
    await commit(env, writes);
    for (const f of later) await f();
  }
  /// A FACT WHOSE ASKER WAS NEVER TOLD, told again: a delivery that failed half
  /// way (a database too busy for the producer's own write) left its demand live
  /// on a settled question, which nothing else ever revisits. Up to `max` a call,
  /// oldest first, each on its own so one that fails again stops none behind it.
  async function redeliver(env, now = Date.now(), max = 100) {
    const db = env.LIBRARY;
    const { results } = await db.prepare(`SELECT DISTINCT q.* FROM demands d JOIN questions q ON q.id = d.question
        WHERE d.state = 'live' AND q.state IN ('fact', 'spot') AND q.fact_at < ?::text ORDER BY q.fact_at LIMIT ?::bigint`)
      .bind(iso(now - 10 * 60_000), max).all();
    let told = 0;
    for (const q of results) {
      try { await deliver(env, q, now); told += 1; } catch (_) { /* told again next run */ }
    }
    return told;
  }

  /// A FACT'S WITNESSES NEVER PAID, paid: the answers agreeing with a settled
  /// question's canon by the time it formed (a later answer is unpaid, as ever),
  /// its independent witnesses as `decide` counts them, still uncredited — a
  /// settlement whose credit failed after its commit. Paid in the
  /// hour the fact formed, as they would have been. Up to `max` questions a call.
  async function repay(env, now = Date.now(), max = 200) {
    const db = env.LIBRARY;
    const { results: qs } = await db.prepare(`SELECT DISTINCT q.id, q.canon, q.fact_engine, q.fact_at FROM answers a JOIN questions q ON q.id = a.question
        WHERE a.credited = 0 AND a.void = 0 AND a.official = 0 AND a.canon = q.canon AND q.state IN ('fact', 'spot')
          AND a.at <= q.fact_at AND q.fact_at < ?::text ORDER BY q.fact_at LIMIT ?::bigint`)
      .bind(iso(now - 10 * 60_000), max).all();
    let paid = 0;
    for (const q of qs) {
      const { results: all } = await db.prepare("SELECT * FROM answers WHERE question = ? AND void = 0 AND official = 0 AND canon = ? AND at <= ? ORDER BY id")
        .bind(q.id, q.canon, q.fact_at).all();
      const group = independent([...all.filter((a) => a.engine === q.fact_engine), ...all.filter((a) => a.engine !== q.fact_engine)]);
      const owed = group.filter((a) => !a.credited);
      if (!owed.length) continue;
      try { await credit(db, owed, Date.parse(q.fact_at)); paid += owed.length; } catch (_) { /* paid next run */ }
    }
    return paid;
  }

  /// …AS WRITES where the producer states its own (`on_fact_writes`), so a whole
  /// pack's facts land in one round trip; a producer that must call out is told
  /// after them (`later`).
  function deliverPlan(env, q, now, answerOf, demands, writes, later) {
    const db = env.LIBRARY;
    const fact = { question: q.id, verb: q.verb, request: JSON.parse(q.request), canon: q.canon,
      result: answerOf ? JSON.parse(answerOf.result) : null, work: answerOf ? answerOf.work : 0,
      engine: answerOf ? answerOf.engine : null, official: !!(answerOf && answerOf.official) };
    for (const d of demands) {
      const p = producerOf(d.producer), told = { ...d, payload: JSON.parse(d.payload) };
      const served = db.prepare("UPDATE demands SET state = 'served', served_at = ? WHERE producer = ? AND ref = ?").bind(iso(now), d.producer, d.ref);
      if (p.on_fact_writes) writes.push(...p.on_fact_writes(env, told, fact).map((side) => ({ side })), served);
      else later.push(async () => { await p.on_fact(env, told, fact); await served.run(); });
    }
  }

  /// THE OFFICIAL MACHINES TAKE A BATCH: up to `max` questions past their
  /// deadline (`open`) or theirs alone (`OFFICIAL_STATES`), as `states` says, of
  /// the given producers, leased for `lease_ms` in one statement. Each comes
  /// with its demands, the live ones first, which say what to compute — a fact
  /// checked again has only served ones.
  async function claim(env, w, { states = ["open", ...OFFICIAL_STATES], producers = null, max = 100, lease_ms = 6 * 3_600_000 }, engine, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now);
    const open = states.includes("open") ? 1 : 0;
    const theirs = states.filter((x) => OFFICIAL_STATES.includes(x));
    const { results } = await db.prepare(`UPDATE questions SET lease = replace(gen_random_uuid()::text, '-', ''), lease_until = ?1::text, leased_to = ?2::text, leased_at = NULL
        WHERE id IN (SELECT q.id FROM questions q
          WHERE ((?3::bigint = 1 AND q.state = 'open' AND q.due_at < ?4::text) OR q.state IN (SELECT value FROM jsonb_array_elements_text((?5::text)::jsonb)))
            AND (q.lease_until IS NULL OR q.lease_until < ?4::text)
            AND (?6::text IS NULL OR EXISTS (SELECT 1 FROM demands d WHERE d.question = q.id
                                       AND d.producer IN (SELECT value FROM jsonb_array_elements_text((?6::text)::jsonb))))
            AND NOT EXISTS (SELECT 1 FROM answers a WHERE a.question = q.id AND a.device = ?2::text AND a.engine = ?7::text)
          ORDER BY q.waits DESC, q.due_at, q.id LIMIT ?8::bigint FOR UPDATE OF q SKIP LOCKED)
        RETURNING id, verb, request, state, lease, waits, due_at`)
      .bind(iso(now + lease_ms), w.device, open, at, JSON.stringify(theirs), producers ? JSON.stringify(producers) : null, engine, max).all();
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

  /// A WITNESS THE OFFICIAL MACHINES DISPROVED, refused — for a cool-down, never
  /// for good (`coolDownMs`). Its answers stop counting; what it was credited is
  /// taken back, in its hour; a fact it witnessed that the official machines
  /// never checked is opened again, and its producers told it no longer stands
  /// (`on_retract`).
  async function refuse(env, device, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now);
    const v = await db.prepare("SELECT refusals FROM verifiers WHERE id = ?").bind(device).first();
    const cool = coolDownMs(v && v.refusals);
    const paid = (await db.prepare("SELECT coalesce(sum(work), 0) AS w FROM answers WHERE device = ? AND credited = 1 AND official = 0")
      .bind(device).first()).w;
    const { results: touched } = await db.prepare(
      `SELECT DISTINCT q.id, q.state FROM answers a JOIN questions q ON q.id = a.question
        WHERE a.device = ? AND a.void = 0 AND a.official = 0 AND q.state IN ('fact', 'spot')
          AND NOT EXISTS (SELECT 1 FROM answers o WHERE o.question = q.id AND o.official = 1 AND o.void = 0)`).bind(device).all();
    await db.batch([
      db.prepare("UPDATE verifiers SET banned = 1, refusals = refusals + 1, clean = 0, refused_until = ? WHERE id = ?").bind(iso(now + cool), device),
      db.prepare("UPDATE verifiers SET work = GREATEST(0, work - ?::bigint) WHERE id = ?::text").bind(paid, device),
      db.prepare(`INSERT INTO verifier_hours AS h (verifier, hour, work) VALUES (?, ?, ?)
                  ON CONFLICT (verifier, hour) DO UPDATE SET work = h.work + excluded.work`).bind(device, at.slice(0, 13), -paid),
      db.prepare("UPDATE answers SET void = 1, credited = 0 WHERE device = ? AND official = 0").bind(device),
      // WHAT IT TAKES BACK GOES TO THE OFFICIAL POOL: compute is never uncounted.
      db.prepare("INSERT INTO points_pool (at, source, work) SELECT ?::text, 'refused', ?::bigint WHERE ?::bigint > 0").bind(at, paid, paid),
    ]);
    for (const q of touched) {
      await db.prepare("UPDATE questions SET state = 'open', canon = NULL, fact_engine = NULL, fact_at = NULL WHERE id = ?").bind(q.id).run();
      const { results: served } = await db.prepare("SELECT * FROM demands WHERE question = ? AND state = 'served'").bind(q.id).all();
      for (const d of served) {
        const p = producerOf(d.producer);
        if (p.on_retract) await p.on_retract(env, { ...d, payload: JSON.parse(d.payload) });
        await db.prepare("UPDATE demands SET state = 'live', served_at = NULL WHERE producer = ? AND ref = ?").bind(d.producer, d.ref).run();
      }
    }
    return { cool_ms: cool, taken_back: paid, reopened: touched.length };
  }

  /// WHAT IS WRONG RIGHT NOW, for the watchdog: a kind whose overdue questions
  /// went unanswered while witnesses were at work, questions that were not
  /// deterministic, disputes nobody settled, a backlog past due, and clients
  /// disagreeing with the facts on the served `engine`. Empty when healthy.
  async function audit(env, now = Date.now(), engine = null) {
    const db = env.LIBRARY, at = iso(now), recent = iso(now - 30 * 60_000);
    const active = (await db.prepare("SELECT count(*) AS n FROM verifiers WHERE last_at > ?").bind(recent).first()).n;
    // A PRODUCER WHOSE OVERDUE QUESTIONS GOT NO ANSWER AT ALL in half an hour
    // while witnesses were at work: something hands them out to nobody. One
    // overdue question waiting its turn is a queue; a whole kind silent is a stall.
    // Only a question nobody holds counts: one a page is computing and renewing
    // is handed out, however long a single row takes.
    const { results: stalled } = await db.prepare(`SELECT d.producer, count(DISTINCT d.question) AS questions, min(d.due_at) AS oldest_due
         FROM demands d JOIN questions q ON q.id = d.question
        WHERE d.state = 'live' AND q.state = 'open' AND d.due_at < ?1::text AND ${TAKEABLE.replaceAll("lease", "q.lease")}
        GROUP BY d.producer
       HAVING NOT EXISTS (SELECT 1 FROM demands e JOIN answers a ON a.question = e.question
                           WHERE e.producer = d.producer AND a.at > ?2::text)`).bind(at, recent).all();
    const odd = (await db.prepare("SELECT count(*) AS n FROM questions WHERE state = 'nondeterministic' OR (odd = 1 AND fact_at > ?)")
      .bind(iso(now - 24 * 3_600_000)).first()).n;
    // WAITING ON THE OFFICIAL MACHINES since its last answer put it there: a
    // dispute nobody settles is the one place a question can still wait for ever.
    // Not `opened_at` — a refresh opened hours ago turns spot a minute ago.
    const unsettled = (await db.prepare(
      `SELECT count(*) AS n FROM questions q WHERE q.state IN (${OFFICIAL_STATES.map(() => "?").join(", ")})
          AND coalesce((SELECT max(a.at) FROM answers a WHERE a.question = q.id), q.opened_at) < ?`)
      .bind(...OFFICIAL_STATES, iso(now - UNSETTLED_MS)).first()).n;
    // PAST DUE AND STILL OPEN, by kind: the official machines take what is past
    // due within the hour, so a pile of it, or one left hours late, is a fleet
    // and a backstop both behind. A refresh past due is a row past fifteen days.
    const { results: late } = await db.prepare(
      `SELECT d.producer, count(*) AS questions, min(q.due_at) AS oldest_due
         FROM questions q JOIN demands d ON d.question = q.id AND d.state = 'live'
        WHERE q.state = 'open' AND q.waits IN (0, 1) AND q.due_at < ?
        GROUP BY d.producer`).bind(at).all();
    const lateTotal = late.reduce((n, p) => n + p.questions, 0);
    // CLIENTS DISAGREEING WITH THE FACTS of the last hour on the served engine,
    // and whose they are: a release or a client build that moved a number shows
    // here before anybody complains.
    let disagreeing = null;
    if (engine) {
      const { results: by } = await db.prepare(
        `SELECT a.owner, count(*) AS answers, sum(CASE WHEN a.canon <> q.canon THEN 1 ELSE 0 END) AS differ, count(DISTINCT a.device) AS devices
           FROM answers a JOIN questions q ON q.id = a.question
          WHERE a.engine = ? AND a.at > ? AND a.official = 0 AND q.canon IS NOT NULL
          GROUP BY a.owner`).bind(engine, iso(now - 3_600_000)).all();
      const answers = by.reduce((n, o) => n + o.answers, 0), differ = by.reduce((n, o) => n + o.differ, 0);
      if (differ >= DISAGREE_MIN && differ >= DISAGREE_SHARE * answers) {
        disagreeing = { engine, answers, differ, owners: by.filter((o) => o.differ).sort((x, y) => y.differ - x.differ).slice(0, 5) };
      }
    }
    const out = [];
    if (active) for (const p of stalled) out.push({ kind: `stalled:${p.producer}`, ...p, witnesses_active: active });
    if (odd) out.push({ kind: "nondeterministic", questions: odd });
    if (unsettled) out.push({ kind: "unsettled", questions: unsettled });
    if (lateTotal >= OVERDUE_MAX || late.some((p) => Date.parse(p.oldest_due) < now - OVERDUE_HOURS_MS)) {
      out.push({ kind: "overdue", questions: lateTotal, producers: late });
    }
    if (disagreeing) out.push({ kind: "disagreeing", ...disagreeing });
    return out;
  }

  return { demand, demandAll, withdraw, next, nextPack, headOf, packable, renew, renewRow, release, releaseAll, releaseAllOf, answer, answerPack, answerOfficial, claim, refuse, settle, audit, redeliver, repay };
}

/// A REFUSAL IS FORGIVEN BY HONEST WORK: a client with k refusals on record has
/// one taken off by k times this many facts it is part of afterwards.
export const FACTS_PER_REFUSAL_FORGIVEN = 1000;
/// HOW LONG A REFUSAL LASTS: a day, three times as long at each one before it.
export const coolDownMs = (refusals) => 86_400_000 * 3 ** Math.max(0, refusals || 0);

/// ONE QUERY DECIDES WHAT A WITNESS MAY ANSWER (the LIVENESS property): a
/// volunteer takes an open question it has not answered — nor any device of its
/// owner, nor its network — on the served engine, with the cores a first answer
/// needs at the question's age; the official machines take what is past its
/// deadline and what only they may settle.
function eligibleFrom(official) {
  return `FROM questions q
  WHERE ${official ? `((q.state = 'open' AND q.due_at < ?1::text) OR q.state IN (${OFFICIAL_STATES.map((s) => `'${s}'`).join(", ")}))`
    : "q.state = 'open'"}
    AND ${TAKEABLE.replaceAll("lease", "q.lease")}
    AND (?2::bigint = 1 OR q.lanes IS NULL OR EXISTS (SELECT 1 FROM answers x WHERE x.question = q.id)
         OR ?3::bigint >= (SELECT min((t ->> 'lanes')::numeric) FROM jsonb_array_elements(q.lanes::jsonb) t
                   WHERE (t ->> 'after_ms')::numeric <= extract(epoch FROM ((?1::text)::timestamptz - q.opened_at::timestamptz)) * 1000))
    AND NOT EXISTS (SELECT 1 FROM answers a WHERE a.question = q.id AND a.engine = ?4::text
          AND (a.device IN (SELECT value FROM jsonb_array_elements_text((?5::text)::jsonb)) OR a.owner = ?6::text OR (?7::text <> '' AND a.net = ?7::text)))
    AND (?8::text IS NULL OR q.due_at < ?8::text)`;
}
const ORDER = "ORDER BY q.waits DESC, q.due_at, q.id";
function eligibleSql(official) {
  return `SELECT q.id, q.verb, q.request, q.state, q.due_at, q.waits ${eligibleFrom(official)} ${ORDER} LIMIT 1`;
}
function eligibleArgs(w, engine, at) {
  return [at, w.official ? 1 : 0, w.lanes ?? 1, engine, JSON.stringify(w.siblings || [w.device]), w.owner || w.device, w.net || "", w.before ?? null];
}
