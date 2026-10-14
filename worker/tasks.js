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
import { dialsOf, DIAL_DEFAULTS } from "./dials.js";

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
const ceilingS = (ms) => Math.round(ms / 1000);
const SUPERSEDE = `(lease_prev, leased_to_prev, lease_prev_until) = (SELECT * FROM factory_supersede(lease_prev, leased_to_prev,
  lease_prev_until, lease, leased_to, lease_until, ?1::text))`;

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

const hex = (buf) => [...new Uint8Array(buf)].map((x) => x.toString(16).padStart(2, "0")).join("");
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

/// THE FACTORY, given what varies: `verbs` — how each is answered
/// ({ cap, lease_ms, canon(result, work), spot? }) — `producers` — who asks and who is
/// told ({ still_wanted?, on_answer?, on_fact }) — the trust a fact needs
/// (`threshold`, a volunteer counting one and the official machines all of it),
/// the share of facts the official machines check again (`spot`), and what
/// happens to a witness they disprove (`on_disproved`).
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
const CLAIM_STALE_MS = 60_000;

/// ROW LOCKS ON QUESTIONS ARE TAKEN IN ID ORDER by every statement that takes
/// many: a lease given back and a settlement locked one set of rows in two
/// orders, and deadlocked each other several times an hour.
export const inIdOrder = (where) => `id IN (SELECT id FROM questions WHERE ${where} ORDER BY id FOR UPDATE)`;

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
  /// `waits`: a person is waiting on it now — §"Order". `initiator`: who asked,
  /// docs/BOARD.md §"Who asked".
  async function demand(env, ask, engine, now = Date.now()) {
    return (await demandAll(env, [ask], engine, now))[0];
  }

  /// MANY ASKS AT ONCE, a chunk in two statements whatever its size: through
  /// Hyperdrive a batch's statements go one round trip each, so a statement a
  /// row is a round trip a row and a reconciliation's asks outlast its hour.
  /// A chunk names each question and each demand once, folded in ask order the
  /// way one write a row folded them: the earliest deadline, the larger
  /// `waits`, the first lanes given; a demand asked twice is its last asking.
  const DEMAND_CHUNK = 250;
  async function demandAll(env, asks, engine, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), ids = [];
    for (const a of asks) {
      if (!verbs[a.verb]) throw new Error(`no verb ${a.verb}`);
      producerOf(a.producer);
      ids.push(await questionId(a.verb, a.request));
    }
    for (let i = 0; i < asks.length; i += DEMAND_CHUNK) {
      const qs = new Map(), ds = new Map();
      asks.slice(i, i + DEMAND_CHUNK).forEach(({ producer, ref, verb, request, due_in_ms, payload = {}, lanes = null, waits = false, initiator = null }, k) => {
        const id = ids[i + k], due_at = iso(now + due_in_ms), w = waits ? 1 : 0, l = lanes ? JSON.stringify(lanes) : null;
        const q = qs.get(id);
        if (!q) qs.set(id, { id, verb, request: canon(request), due_at, lanes: l, waits: w, producer, initiator });
        else Object.assign(q, { ...(due_at < q.due_at ? { due_at, producer, initiator } : {}), waits: Math.max(q.waits, w), lanes: q.lanes ?? l });
        ds.set(JSON.stringify([producer, ref]), { producer, ref, question: id, due_at, waits: w, payload: JSON.stringify(payload), initiator });
      });
      // IN KEY ORDER, so two asks writing the same rows take their locks alike.
      const sorted = (m, key) => [...m.values()].sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0));
      // A QUESTION THE CLIENTS' ENGINE REFUSED (`refuseRows`) stays withdrawn on
      // that engine however often it is asked again; a new engine tries it anew.
      const reopen = `t.state = 'withdrawn' AND NOT EXISTS (SELECT 1 FROM refusal_withdrawals rw WHERE rw.question = t.id AND rw.engine = ?3::text)`;
      await db.batch([
        db.prepare(`INSERT INTO questions AS t (id, verb, request, state, due_at, opened_at, lanes, waits, producer, initiator)
                      SELECT x.id, x.verb, x.request, 'open', x.due_at, ?1::text, x.lanes, x.waits, x.producer, x.initiator
                      FROM jsonb_to_recordset((?2::text)::jsonb) AS x(id text, verb text, request text, due_at text, lanes text, waits bigint, producer text, initiator text)
                    ON CONFLICT (id) DO UPDATE SET
                      state = CASE WHEN ${reopen} THEN 'open' ELSE t.state END,
                      opened_at = CASE WHEN ${reopen} THEN excluded.opened_at ELSE t.opened_at END,
                      due_at = CASE WHEN ${reopen} OR excluded.due_at < t.due_at THEN excluded.due_at ELSE t.due_at END,
                      producer = CASE WHEN ${reopen} OR excluded.due_at < t.due_at THEN excluded.producer ELSE t.producer END,
                      initiator = CASE WHEN ${reopen} OR excluded.due_at < t.due_at THEN excluded.initiator ELSE t.initiator END,
                      waits = CASE WHEN ${reopen} THEN excluded.waits ELSE GREATEST(t.waits, excluded.waits) END,
                      lanes = COALESCE(t.lanes, excluded.lanes)`)
          .bind(at, JSON.stringify(sorted(qs, (q) => q.id)), engine || ""),
        db.prepare(`INSERT INTO demands (producer, ref, question, state, asked_at, due_at, waits, payload, initiator)
                      SELECT x.producer, x.ref, x.question, 'live', ?1::text, x.due_at, x.waits, x.payload, x.initiator
                      FROM jsonb_to_recordset((?2::text)::jsonb) AS x(producer text, ref text, question text, due_at text, waits bigint, payload text, initiator text)
                    ON CONFLICT (producer, ref) DO UPDATE SET question = excluded.question, state = 'live', asked_at = excluded.asked_at,
                      due_at = excluded.due_at, waits = excluded.waits, payload = excluded.payload, served_at = NULL, initiator = excluded.initiator`)
          .bind(at, JSON.stringify(sorted(ds, (d) => `${d.producer}\u0000${d.ref}`))),
        // A PRODUCER NEVER SEEN starts at the least any has used: no credit banked.
        db.prepare(`INSERT INTO producer_use (producer, used)
                      SELECT x.p, COALESCE((SELECT min(used) FROM producer_use), 0) FROM jsonb_array_elements_text((?1::text)::jsonb) AS x(p)
                    ON CONFLICT (producer) DO NOTHING`)
          .bind(JSON.stringify([...new Set([...ds.values()].map((d) => d.producer))].sort())),
      ]);
    }
    const { results } = await db.prepare(`SELECT * FROM questions WHERE id IN ${listOf(db, "?")} AND state IN ('fact', 'spot')`)
      .bind(JSON.stringify([...new Set(ids)])).all();
    // A FACT OF ANOTHER ENGINE answers nothing now: the question opens again,
    // its answers carried for the served engine to reproduce or replace.
    const stale = results.filter((q) => q.fact_engine !== engine);
    if (stale.length) {
      await db.prepare(`UPDATE questions SET state = 'open', opened_at = ?1::text
          FROM jsonb_to_recordset((?2::text)::jsonb) AS x(id text, state text) WHERE questions.id = x.id AND questions.state = x.state`)
        .bind(at, JSON.stringify(stale.map((q) => ({ id: q.id, state: q.state })))).run();
    }
    await deliverMany(env, results.filter((q) => q.fact_engine === engine), now);
    return ids;
  }

  /// A PRODUCER NO LONGER ASKS. A question nobody asks for any more is withdrawn,
  /// and one still asked for keeps the earliest deadline still standing.
  async function withdraw(env, producer, ref, now = Date.now()) {
    const db = env.LIBRARY;
    const d = await db.prepare("SELECT question FROM demands WHERE producer = ? AND ref = ?").bind(producer, ref).first();
    if (!d) return;
    await db.prepare("UPDATE demands SET state = 'withdrawn' WHERE producer = ? AND ref = ? AND state = 'live'").bind(producer, ref).run();
    await restate(db, d.question);
  }

  /// worker/pg/sync.sql `factory_restate`.
  async function restate(db, id) {
    await db.prepare("SELECT factory_restate(?::text)").bind(id).run();
  }

  /// THE QUESTION THIS WITNESS ANSWERS NEXT, leased, or null — §"Order" among
  /// exactly the questions it may answer. `w`: { device, owner,
  /// siblings, net, lanes, official, most, before }: `lanes` 0 is a witness with
  /// no cores free now (asking ahead), which a question someone waits on is never
  /// handed; `before` keeps to questions due before then.
  async function next(env, w, engine, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), most = w.most ?? 1, d = await dialsOf(env, now);
    for (let tries = 0; tries < 8; tries++) {
      const q = await db.prepare(HEAD).bind(...eligibleArgs(w, engine, at, d)).first();
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
  /// ONE WHOSE CODE MOVED (`join`, the served engine) is handed only a row a
  /// served answer of another owner and network already leads: its own answers
  /// never start a fact, so handed every row, every stale page answers the head of
  /// the queue again and none of it counts.
  /// The pick, the lease and the charge are worker/pg/sync.sql `factory_lease_pack`.
  async function nextPack(env, w, engine, { n, lease_ms, most = 1, exclude = null, join = null, producer = null }, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), lease = newLease(), d = await dialsOf(env, now);
    const [, , , , , , , , ceiling, grace] = eligibleArgs(w, engine, at, d);
    const [taken, ds] = await db.batch([
      db.prepare(`SELECT * FROM factory_lease_pack(?1::text, ?2::bigint, ?3::text, ?4::text, ?5::text, ?6::text, ?7::text, ?8::bigint, ?9::bigint,
          ?10::text, ?11::text, ?12::text, ?13::text, ?14::bigint, ?15::text, ?16::text, ?17::text, ?18::bigint)`)
        .bind(at, w.lanes ?? 1, engine, JSON.stringify(w.siblings || [w.device]), w.owner || w.device, w.net || "", w.before ?? null, ceiling, grace,
          PACKABLE, exclude ? JSON.stringify(exclude) : null, join, producer, n, lease, iso(now + lease_ms), w.device, most),
      // IN ONE ORDER, which the sync's procedure walks too (`board_sync`).
      db.prepare(`SELECT d.question, d.producer, d.ref, d.payload FROM demands d JOIN questions q ON q.id = d.question
          WHERE q.lease = ? AND q.leased_to = ? AND d.state = 'live' ORDER BY d.question, d.producer, d.ref`).bind(lease, w.device),
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
      await restate(db, d.question);
    }
    const wanted = demands.filter((d) => !unwanted.has(d));
    const gone = taken.results.filter((q) => !wanted.some((d) => d.question === q.id)).map((q) => q.id);
    if (gone.length) {
      await db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
          WHERE ${inIdOrder(`id IN ${listOf(db, "?")} AND lease = ?`)}`).bind(JSON.stringify(gone), lease).run();
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
  /// …AS ONE STATEMENT, and the head of a witness's order as another, for a
  /// caller to read in its own batch: an ask is then one round trip, not five.
  const releaseAllOf = (env, device, now = Date.now()) => env.LIBRARY.prepare(
    `UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE ${inIdOrder("leased_to = ? AND lease_until > ?")}`).bind(device, iso(now));
  const headOf = (env, w, engine, now = Date.now(), d = DIAL_DEFAULTS) => env.LIBRARY.prepare(HEAD)
    .bind(...eligibleArgs(w, engine, iso(now), d));

  /// WHAT A HAND-OUT COSTS ITS PRODUCER: the question at its producer's mean
  /// fight, over its weight (docs/BOARD.md §"Fair share between producers"); a
  /// pack is charged in the statement that leases it (`nextPack`).
  async function charge(db, q) {
    await db.prepare("UPDATE producer_use SET used = used + answer_ms / weight WHERE producer = ?").bind(q.producer || "").run();
  }
  /// …AND WHAT ITS ANSWERS COST, folded into each producer's mean: `k` answers
  /// move it as `k` folds of `PRODUCER_MEAN_SHARE` would. A statement for the
  /// answers' own batch, or null. worker/pg/sync.sql `factory_learn`.
  function learnCost(db, items) {
    const by = {};
    for (const { q, a } of items) {
      if (!Number.isFinite(a.compute_ms) || a.compute_ms <= 0) continue;
      const b = by[q.producer || ""] ||= { ms: 0, n: 0 };
      b.ms += a.compute_ms; b.n += 1;
    }
    const rows = Object.entries(by).sort().map(([p, b]) => ({ p, ms: b.ms, n: b.n }));
    if (!rows.length) return null;
    return db.prepare("SELECT factory_learn(?1::text, ?2::double precision)").bind(JSON.stringify(rows), PRODUCER_MEAN_SHARE);
  }
  /// A PRODUCER WITH NOTHING OPEN is lifted to the least any producer with open
  /// work has used, so it comes back level and never with a share banked while
  /// idle. Run by the watchdog's cron.
  async function liftIdle(env) {
    await env.LIBRARY.prepare(`UPDATE producer_use u SET used = m.v
        FROM (SELECT min(a.used) AS v FROM producer_use a WHERE EXISTS (SELECT 1 FROM questions q WHERE q.state = 'open' AND q.producer = a.producer)) m
        WHERE m.v IS NOT NULL AND u.used < m.v AND NOT EXISTS (SELECT 1 FROM questions q WHERE q.state = 'open' AND q.producer = u.producer)`).run();
  }
  /// WHETHER A HEAD GOES IN A PACK, or alone (`next`).
  const packable = (q) => !q.waits && !!(verbs[q.verb] || {}).pack;
  /// AN ANSWER THE SETTLER MAY TAKE (dial `settle_async`): to a packable verb,
  /// asked by a producer that is not told each answer (`on_answer` runs here).
  const queueable = (q) => !!(verbs[q.verb] || {}).pack && !!producers[q.producer] && !producers[q.producer].on_answer;
  /// …THE SAME RULES as data for the sync's procedure (worker/sync.js): the
  /// producers whose demands it may judge in SQL (`wanted_in_sql`, none told of
  /// a lease), and each verb's canonical answer, which only JS writes byte-exact.
  const syncRules = {
    packable: Object.keys(verbs).filter((v) => verbs[v].pack),
    queueable: Object.keys(producers).filter((p) => !producers[p].on_answer),
    owed: Object.keys(producers).filter((p) => producers[p].wanted_in_sql && !producers[p].on_lease),
    canons: (result, work) => Object.fromEntries(Object.keys(verbs).map((v) => [v, verbs[v].canon(result, work)])),
  };

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
  /// `late` (protocol 8): a row whose lease ran out still takes its answer while
  /// nobody took it since — the page keeps results and sends them later.
  async function answerPack(env, w, { lease, engine, served = null, join = false, answers, done = false, keep = null, late = false }, now = Date.now()) {
    const db = env.LIBRARY;
    const { results: held } = await db.prepare("SELECT * FROM factory_held(?1::text, ?2::text, ?4::text, ?3::text, ?5::bigint)")
      .bind(lease, w.device, iso(now), JSON.stringify(answers.map((a) => a.question)), late ? 1 : 0).all();
    const byId = new Map(held.map((q) => [q.id, q]));
    // A PAGE SENDS AGAIN what got no reply, and the reply may be all that was lost:
    // an answer this device already gave on this engine is not a second witness
    // (the ask never hands it the same question twice, `nextPack`).
    // An answer already recorded is `already` whether or not the row is still held:
    // settling it gave the lease back, and the reply that said so was lost.
    const already = [];
    if (answers.length) {
      const { results: given } = await db.prepare("SELECT g AS question FROM factory_given(?1::text, ?2::text, ?3::text) g")
        .bind(w.device, engine, JSON.stringify(answers.map((a) => a.question))).all();
      for (const r of given) { byId.delete(r.question); already.push(r.question); }
    }
    const items = answers.filter((a) => byId.has(a.question)).map(({ question, ...a }) => ({ q: byId.get(question), a: { ...a, engine } }));
    if (items.length) await recordMany(env, w, items, now, served, join);
    if (done) await release(env, w.device, lease, keep);
    return { taken: items.map((x) => x.q.id), already };
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
    // ONE DEVICE'S SENDS ARE TAKEN ONE AT A TIME, and an answer it already gave on
    // this engine is not written again: a beacon on the way out raced a sync still
    // in flight, both checked before either wrote, and both rows landed.
    const lock = db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?1::text, 0))").bind(w.device);
    const writes = items.map(({ q, a }) => db.prepare(`SELECT factory_answer(?1::text, ?2::text, ?3::text, ?4::text, ?5::text, ?6::bigint, ?7::text,
        ?8::text, ?9::bigint, ?10::bigint, ?11::text, ?12::bigint, ?13::text)`)
      .bind(q.id, a.engine, w.device, w.owner || w.device, w.net || "", official, verbs[q.verb].canon(a.result, a.work), JSON.stringify(a.result),
        a.work ?? 0, a.compute_ms ?? null, a.counts ? JSON.stringify(a.counts) : null, a.lanes ?? null, at));
    const learnt = official ? null : learnCost(db, items);
    // QUEUED FOR THE SETTLER (dial `settle_async`): a volunteer's answers to a
    // packable verb whose producer is not told each answer (`on_answer` runs in
    // the request), in the answers' own transaction, so one written is one queued.
    // An answer on a row a settler pass holds clears its claim (`settleQueued`).
    const peer = mine !== engine && !join ? mine : null;
    const queued = !official && items.every(({ q }) => queueable(q)) && (await dialsOf(env, now)).settle_async === 1;
    const queue = queued ? db.prepare("SELECT factory_queue(?1::text, ?2::text, ?3::text, ?4::bigint, ?5::text)")
      .bind(JSON.stringify(items.map(({ q }) => q.id)), engine, peer || "", join ? 1 : 0, at) : null;
    await db.batch([lock, ...writes, ...(learnt ? [learnt] : []), ...(queue ? [queue] : [])]);
    if (!official) {
      // …AND WHEN IT LAST ANSWERED and what it spent, in its hour (docs/BOARD.md
      // §"Contribution") — a statistic, so outside the answers' transaction:
      // inside it, it held the device's row against every credit.
      const ms = items.reduce((t, { a }) => t + (a.compute_ms || 0), 0);
      await db.prepare("SELECT verifier_worked(?1::text, ?2::bigint, ?3::bigint, ?4::text)").bind(w.device, ms, items.length, at).run();
    }
    // …AND SETTLED THERE, the lease given back with it. A handle that asks for it
    // (a check's database, scripts/pg/test_db.mjs) has the pass run at once.
    if (queued) {
      if (db.settleAtOnce) await settleQueued(env, now);
      return;
    }
    const answered = new Map(items.map(({ q, a }) => [q.id,
      { question: q.id, device: w.device, net: w.net || "", engine: a.engine, result: a.result, work: a.work, official: !!official, at }]));
    const ids = JSON.stringify([...answered.keys()]);
    // THE LEASE GOES ONLY ONCE THE QUESTION IS SETTLED, in the settling's own
    // write: the answered question heads every witness's order, so freed before
    // it became a fact it was taken at once and answered again for nothing.
    await settleMany(env, [...answered.keys()], engine, now, { answered, peer, after: [
      db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
                   WHERE ${inIdOrder(`id IN ${listOf(db, "?")} AND leased_to = ?`)}`).bind(ids, w.device)] });
  }

  /// THE SETTLER'S PASS (worker/settler.js `settleBatch`): up to `max` queued
  /// questions claimed, each settled exactly as an answer in the request settles
  /// it (`settleMany`), the lease of a holder that answered given back, and the
  /// rows taken off — but a row an answer re-armed meanwhile, which waits for
  /// the next pass. A pass that dies is claimed again after `CLAIM_STALE_MS`;
  /// every write in a settlement is guarded, so settling twice counts once.
  async function settleQueued(env, now = Date.now(), max = null) {
    const db = env.LIBRARY, claim = newLease(), n = max ?? (await dialsOf(env, now)).settle_batch;
    const { results: rows } = await db.prepare(`UPDATE settle_queue SET claim = ?1::text, claimed_at = ?2::text
        WHERE (question, engine, peer) IN (SELECT question, engine, peer FROM settle_queue
          WHERE claim IS NULL OR claimed_at < ?3::text ORDER BY question, engine, peer LIMIT ?4::bigint FOR UPDATE SKIP LOCKED)
        RETURNING question, engine, peer`).bind(claim, iso(now), iso(now - CLAIM_STALE_MS), n).all();
    if (!rows.length) return 0;
    const groups = new Map();
    for (const r of rows) {
      const k = `${r.engine} ${r.peer}`;
      if (!groups.has(k)) groups.set(k, { engine: r.engine, peer: r.peer || null, ids: [] });
      groups.get(k).ids.push(r.question);
    }
    for (const g of [...groups.values()].sort((a, b) => (a.engine + a.peer < b.engine + b.peer ? -1 : 1))) {
      const ids = [...new Set(g.ids)].sort(), list = JSON.stringify(ids);
      await settleMany(env, ids, g.engine, now, { peer: g.peer, after: [
        db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE ${inIdOrder(`id IN ${listOf(db, "?")}
            AND leased_to IS NOT NULL AND EXISTS (SELECT 1 FROM answers a WHERE a.question = questions.id AND a.device = questions.leased_to)`)}`)
          .bind(list)] });
    }
    await db.prepare("DELETE FROM settle_queue WHERE claim = ?").bind(claim).run();
    return rows.length;
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
    for (const q of [...qs.results].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))) {
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
    // …ITS QUESTIONS LOCKED FIRST, IN ID ORDER (`inIdOrder`): each write after
    // finds its row already held, so the commit never waits holding one.
    if (writes.length) writes.unshift(db.prepare(`SELECT id FROM questions WHERE ${inIdOrder(`id IN ${ids_}`)}`).bind(list));
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
      // ANOTHER ENGINE AGREES BY ITS RESULT ALONE: `work` counts the engine's own
      // instructions, so a release that moved its code moved every `work`, and
      // compared whole, a `join` answer never agrees with a served one.
      const same = sameAs(lead);
      const equal = all.filter(same);
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
      credits.push(...group, ...joinersParted(all, served, group, same, engine));
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
  function joinersParted(all, served, group, same, engine) {
    const parted = all.filter((a) => a.engine !== engine && !a.official && !served.includes(a) && !group.includes(a) && !same(a));
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

  /// ONLY A FACT SCORES, once per answer: its work to its device, in its hour,
  /// and one fact toward forgiving a refusal (docs/BOARD.md §"Contribution").
  /// A CREDIT IS ONE TRANSACTION, one round trip: its devices locked in device
  /// order, then the answers flipped to credited (`credited = 0`, so each is paid
  /// once) and each device paid what flipped. In two steps, a pass dying between
  /// them left answers marked paid whose work nobody was ever given.
  async function credit(db, answers, now) {
    // THE WORK PAID IS THE ANSWER'S AS GIVEN HERE, which may be the official work
    // in its place (`settleOfficial`), never re-read from the table.
    const owed = answers.filter((a) => !a.official && !a.credited).map((a) => ({ id: Number(a.id), device: a.device, work: a.work }));
    if (!owed.length) return;
    await db.batch([
      db.prepare(`SELECT id FROM verifiers WHERE id IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb)) ORDER BY id FOR UPDATE`)
        .bind(JSON.stringify([...new Set(owed.map((a) => a.device))])),
      db.prepare(`WITH owed AS (SELECT (e->>'id')::bigint AS id, e->>'device' AS device, (e->>'work')::bigint AS work
            FROM jsonb_array_elements((?1::text)::jsonb) e),
          paid AS (UPDATE answers a SET credited = 1 FROM owed WHERE a.id = owed.id AND a.credited = 0 RETURNING owed.device, owed.work),
          sums AS (SELECT device, sum(work)::bigint AS work, count(*)::bigint AS n FROM paid GROUP BY device),
          v AS (UPDATE verifiers SET work = verifiers.work + sums.work, agreed = agreed + sums.n,
              refusals = CASE WHEN refusals > 0 AND clean + sums.n >= refusals * ?2::bigint THEN refusals - 1 ELSE refusals END,
              clean = CASE WHEN refusals = 0 OR clean + sums.n >= refusals * ?2::bigint THEN 0 ELSE clean + sums.n END
            FROM sums WHERE verifiers.id = sums.device RETURNING 1)
        INSERT INTO verifier_hours AS h (verifier, hour, work) SELECT device, ?3::text, work FROM sums
          ON CONFLICT (verifier, hour) DO UPDATE SET work = h.work + excluded.work`)
        .bind(JSON.stringify(owed), FACTS_PER_REFUSAL_FORGIVEN, iso(now).slice(0, 13)),
    ]);
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
  /// …MANY AT ONCE, each told what `deliver` tells it, in three round trips
  /// however many: their answers, their live demands, one commit.
  async function deliverMany(env, qs, now) {
    if (!qs.length) return;
    const db = env.LIBRARY, keys = JSON.stringify(qs.map((q) => ({ id: q.id, canon: q.canon })));
    const { results: answers } = await db.prepare(`SELECT DISTINCT ON (a.question) a.* FROM answers a
        JOIN jsonb_to_recordset((?::text)::jsonb) AS x(id text, canon text) ON a.question = x.id AND a.canon = x.canon
        ORDER BY a.question, a.official DESC, a.id`).bind(keys).all();
    const { results: demands } = await db.prepare(`SELECT * FROM demands WHERE question IN ${listOf(db, "?")} AND state = 'live'`)
      .bind(JSON.stringify(qs.map((q) => q.id))).all();
    const writes = [], later = [];
    for (const q of qs) {
      deliverPlan(env, q, now, answers.find((a) => a.question === q.id) || null, demands.filter((d) => d.question === q.id), writes, later);
    }
    await commit(env, writes);
    for (const f of later) await f();
  }
  /// A FACT WHOSE ASKER WAS NEVER TOLD, told again: a delivery that failed half
  /// way (a database too busy for the producer's own write) left its demand live
  /// on a settled question, which nothing else ever revisits. Up to `max` a call,
  /// oldest first, each on its own so one that fails again stops none behind it.
  async function redeliver(env, now = Date.now(), max = 100) {
    const db = env.LIBRARY;
    // FROM THE LIVE DEMANDS, never from the settled questions: walked by
    // `fact_at`, every fact ever made was probed for a demand to find none.
    const { results } = await db.prepare(`WITH live AS MATERIALIZED (SELECT DISTINCT question FROM demands WHERE state = 'live')
        SELECT q.* FROM live JOIN questions q ON q.id = live.question
        WHERE q.state IN ('fact', 'spot') AND q.fact_at < ?::text ORDER BY q.fact_at LIMIT ?::bigint`)
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
  /// AN ANSWER WHOSE OWNER OR NETWORK WAS ALREADY PAID FOR THE QUESTION is no
  /// witness (`independent`) and is never owed: picked, the oldest of them held
  /// every call's places and nobody behind them was ever paid.
  async function repay(env, now = Date.now(), max = 200) {
    const db = env.LIBRARY;
    const { results: qs } = await db.prepare(`SELECT DISTINCT q.id, q.canon, q.fact_engine, q.fact_at FROM answers a JOIN questions q ON q.id = a.question
        WHERE a.credited = 0 AND a.void = 0 AND a.official = 0 AND a.canon = q.canon AND q.state IN ('fact', 'spot')
          AND a.at <= q.fact_at AND q.fact_at < ?::text
          AND NOT EXISTS (SELECT 1 FROM answers c WHERE c.question = a.question AND c.credited = 1 AND c.void = 0 AND c.official = 0
                          AND (c.owner = a.owner OR (coalesce(a.net, '') <> '' AND c.net = a.net)))
        ORDER BY q.fact_at LIMIT ?::bigint`)
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
    const db = env.LIBRARY, at = iso(now), recent = iso(now - 30 * 60_000), ceiling = ceilingS((await dialsOf(env, now)).row_ceiling_ms);
    const active = (await db.prepare("SELECT count(*) AS n FROM verifiers WHERE last_at > ?").bind(recent).first()).n;
    // A PRODUCER WHOSE OVERDUE QUESTIONS GOT NO ANSWER AT ALL in half an hour
    // while witnesses were at work: something hands them out to nobody. One
    // overdue question waiting its turn is a queue; a whole kind silent is a stall.
    // Only a question nobody holds counts: one a page is computing and renewing
    // is handed out, however long a single row takes.
    const { results: stalled } = await db.prepare(`SELECT d.producer, count(DISTINCT d.question) AS questions, min(d.due_at) AS oldest_due
         FROM demands d JOIN questions q ON q.id = d.question
        WHERE d.state = 'live' AND q.state = 'open' AND d.due_at < ?1::text
          AND factory_takeable(q.lease_until, q.leased_at, ?1::text, ?3::bigint)
        GROUP BY d.producer
       HAVING NOT EXISTS (SELECT 1 FROM demands e JOIN answers a ON a.question = e.question
                           WHERE e.producer = d.producer AND a.at > ?2::text)`).bind(at, recent, ceiling).all();
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
    // QUESTIONS THE CLIENTS' ENGINE REFUSED, withdrawn (`refuseRows`) in the last
    // six hours: a build no client can fight came in, so something let it in.
    const { results: refusedRows } = await db.prepare(`SELECT question, engine, owners, at FROM refusal_withdrawals WHERE at > ? ORDER BY at DESC LIMIT 20`)
      .bind(iso(now - 6 * 3_600_000)).all();
    const out = [];
    if (refusedRows.length) out.push({ kind: "refused_rows", questions: refusedRows.length, sample: refusedRows.slice(0, 5) });
    if (active) for (const p of stalled) out.push({ kind: `stalled:${p.producer}`, ...p, witnesses_active: active });
    if (odd) out.push({ kind: "nondeterministic", questions: odd });
    if (unsettled) out.push({ kind: "unsettled", questions: unsettled });
    if (lateTotal >= OVERDUE_MAX || late.some((p) => Date.parse(p.oldest_due) < now - OVERDUE_HOURS_MS)) {
      out.push({ kind: "overdue", questions: lateTotal, producers: late });
    }
    if (disagreeing) out.push({ kind: "disagreeing", ...disagreeing });
    return out;
  }

  return { demand, demandAll, withdraw, next, nextPack, headOf, packable, syncRules, liftIdle, renew, renewRow, renewMany, release, releaseAll, releaseAllOf, releaseUnnamed,
    answer, answerPack, answerOfficial, refuseRows, claim, refuse, settle, settleQueued, audit, redeliver, repay };
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
/// deadline and what only they may settle. A volunteer's pick is each producer's
/// head, of those the waited-on first, then the producer that has used least
/// (docs/BOARD.md §"Fair share between producers"), else the plain order, so
/// nothing eligible is ever left unhanded. worker/pg/sync.sql `factory_head`.
const HEAD = `SELECT * FROM factory_head(?1::text, ?2::bigint, ?3::bigint, ?4::text, ?5::text, ?6::text, ?7::text, ?8::text,
  ?9::bigint, ?10::bigint, ?11::text)`;
function eligibleArgs(w, engine, at, d = DIAL_DEFAULTS) {
  return [at, w.official ? 1 : 0, w.lanes ?? 1, engine, JSON.stringify(w.siblings || [w.device]), w.owner || w.device, w.net || "", w.before ?? null,
    ceilingS(d.row_ceiling_ms), ceilingS(d.settle_grace_ms), JSON.stringify(OFFICIAL_STATES)];
}

/// WHO IS ASKING, one definition for every reader (worker/verify.js, worker/fair.js,
/// worker/settler.js): worker/pg/sync.sql `factory_asking`, since ?`since` at ?`now`.
export const askingIds = (since, now) => `(SELECT a.id FROM factory_asking(?${since}::text, ?${now}::text) AS a(id))`;
