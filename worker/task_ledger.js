// SPDX-License-Identifier: AGPL-3.0-or-later
// Task registration and durable demand state.
import { iso } from "./instant.js";
import { canon, questionId } from "./task_contract.js";

export function createLedger({ verbs, producerOf } = {}) {
  const listOf = (db, p) => `(SELECT value FROM jsonb_array_elements_text((${p}::text)::jsonb))`;
  const DEMAND_CHUNK = 250;
  async function liveDemands(db, id) {
    return (await db.prepare("SELECT * FROM demands WHERE question = ? AND state = 'live'").bind(id).all()).results;
  }


  /// A PRODUCER ASKS. The question is opened, or shared when another producer
  /// asked it already; a fact the served engine holds is returned for delivery.
  /// `waits`: a person is waiting on it now — §"Order". `initiator`: who asked,
  /// docs/BOARD.md §"Who asked".
  async function demand(env, ask, engine, now = Date.now()) {
    return (await demandAll(env, [ask], engine, now)).ids[0];
  }


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
        db.prepare(`INSERT INTO demands (producer, ref, question, state, asked_at, due_at, waits, payload, initiator, delivery_at)
                      SELECT x.producer, x.ref, x.question, 'live', ?1::text, x.due_at, x.waits, x.payload, x.initiator,
                        CASE WHEN EXISTS (SELECT 1 FROM questions q WHERE q.id=x.question AND q.state IN ('fact','spot') AND q.fact_engine=?3::text) THEN ?1::text END
                      FROM jsonb_to_recordset((?2::text)::jsonb) AS x(producer text, ref text, question text, due_at text, waits bigint, payload text, initiator text)
                    ON CONFLICT (producer, ref) DO UPDATE SET question = excluded.question, state = 'live', asked_at = excluded.asked_at,
                      due_at = excluded.due_at, waits = excluded.waits, payload = excluded.payload, served_at = NULL,
                      initiator = excluded.initiator, delivery_at=excluded.delivery_at`)
          .bind(at, JSON.stringify(sorted(ds, (d) => `${d.producer}\u0000${d.ref}`)), engine || ""),
        // A PRODUCER NEVER SEEN starts at the least any has used: no credit banked.
        db.prepare(`INSERT INTO producer_use (producer, used)
                      SELECT x.p, COALESCE((SELECT min(used) FROM producer_use), 0) FROM jsonb_array_elements_text((?1::text)::jsonb) AS x(p)
                    ON CONFLICT (producer) DO NOTHING`)
          .bind(JSON.stringify([...new Set([...ds.values()].map((d) => d.producer))].sort())),
      ]);
    }
    const { results } = await db.prepare(`SELECT q.*, EXISTS (SELECT 1 FROM answers a WHERE a.question = q.id AND a.engine = ?2::text) AS served_answered
        FROM questions q WHERE q.id IN ${listOf(db, "?1")} AND q.state IN ('fact', 'spot', 'disputed', 'nondeterministic')`)
      .bind(JSON.stringify([...new Set(ids)]), engine || "").all();
    // A FACT OF ANOTHER ENGINE answers nothing now: the question opens again,
    // its answers carried for the served engine to reproduce or replace. So
    // does a dispute no answer of the served engine is in: the official
    // machines run that engine alone, and could neither settle it nor refuse
    // anyone (worker/pg/sync.sql `factory_moot_checks`).
    // WAITED ON ONLY IF A LIVE DEMAND WAITS: a question's `waits` is its live
    // demands' (`factory_restate`), never a served one's. Kept from a demand
    // long served, a re-asked row goes alone to a free computer as though a
    // player waited, and a fleet handed one row a sync idles out the floor.
    const settled = (q) => q.state === "fact" || q.state === "spot";
    const stale = results.filter((q) => (settled(q) ? q.fact_engine !== engine : !q.served_answered));
    if (stale.length) {
      // AN OFFICIAL LEASE GOES WITH THE DISPUTE: held, it kept the volunteers off
      // the row for the claim's hours.
      await db.prepare(`UPDATE questions SET state = 'open', opened_at = ?1::text, lease = NULL, lease_until = NULL, leased_to = NULL,
          waits = (SELECT COALESCE(max(l.waits), 0) FROM demands l WHERE l.question = questions.id AND l.state = 'live')
          FROM jsonb_to_recordset((?2::text)::jsonb) AS x(id text, state text) WHERE questions.id = x.id AND questions.state = x.state`)
        .bind(at, JSON.stringify(stale.map((q) => ({ id: q.id, state: q.state })))).run();
    }
    return { ids, ready: results.filter(q => settled(q) && q.fact_engine === engine) };
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


  /// THE LIVE DEMANDS `ds` (payload parsed) STILL OWED, asked of each producer
  /// for all of its at once; the rest withdrawn where they are found, in key
  /// order as an ask writes them, and their questions restated.
  async function sift(env, ds) {
    const db = env.LIBRARY, unwanted = new Set();
    for (const name of new Set(ds.map((x) => x.producer))) {
      const p = producerOf(name), mine = ds.filter((x) => x.producer === name);
      if (p.wanted_many) {
        const keep = await p.wanted_many(env, mine);
        for (const x of mine) if (!keep.has(x.ref)) unwanted.add(x);
      } else if (p.still_wanted) {
        for (const x of mine) if (!(await p.still_wanted(env, x))) unwanted.add(x);
      }
    }
    if (unwanted.size) {
      await db.prepare(`UPDATE demands SET state = 'withdrawn' WHERE (producer, ref) IN (SELECT d.producer, d.ref FROM demands d
          JOIN jsonb_to_recordset((?::text)::jsonb) AS x(producer text, ref text) ON d.producer = x.producer AND d.ref = x.ref
          ORDER BY d.producer, d.ref FOR UPDATE OF d)`)
        .bind(JSON.stringify([...unwanted].map(({ producer, ref }) => ({ producer, ref })))).run();
      await restateAll(db, [...unwanted].map((x) => x.question));
    }
    return ds.filter((x) => !unwanted.has(x));
  }


  /// …AND MANY QUESTIONS RESTATED in one statement, in id order.
  async function restateAll(db, ids) {
    await db.prepare(`SELECT factory_restate(g.id) FROM (SELECT DISTINCT value AS id FROM jsonb_array_elements_text((?::text)::jsonb) ORDER BY 1) g`)
      .bind(JSON.stringify(ids)).run();
  }


  /// One module's writes, including a consumer's explicit `{ side }`, commit
  /// in one transaction.
  async function commit(env, writes) {
    if (!writes.length) return;
    await env.LIBRARY.batch(writes.map((w) => w.side || w));
  }

  /// Consumer readiness is committed with the fact, not inferred from its age.
  const readyFact = (db, id, answer, engine, at) => db.prepare(`UPDATE demands SET delivery_at=?4::text
    WHERE question=?1::text AND state='live' AND EXISTS (SELECT 1 FROM questions q WHERE q.id=?1::text
      AND q.state IN ('fact','spot') AND q.canon=?2::text AND q.fact_engine=?3::text)`)
    .bind(id, answer, engine, at);


  return { liveDemands, demand, demandAll, withdraw, restate, sift, restateAll, commit, readyFact };
}
