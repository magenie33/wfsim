// SPDX-License-Identifier: AGPL-3.0-or-later
// Task registration and durable demand state.
import { iso } from "./instant.js";
import { canon, questionId } from "./task_contract.js";
import { computationInput } from "./computation_input.js";

export function createLedger({ verbs, producerOf } = {}) {
  const listOf = (db, p) => `(SELECT value FROM jsonb_array_elements_text((${p}::text)::jsonb))`;
  const DEMAND_CHUNK = 250;
  async function liveDemands(db, id) {
    return (await db.prepare("SELECT * FROM relation_task_consumers WHERE question = ? AND state = 'live'").bind(id).all()).results;
  }


  /// A PRODUCER ASKS. The question is opened, or shared when another producer
  /// asked it already; a fact the served engine holds is returned for delivery.
  /// `waits`: a person is waiting on it now — §"Order". `initiator`: who asked,
  /// docs/BOARD.md §"Who asked".
  async function demand(env, ask, engine, now = Date.now(), options = {}) {
    return (await demandAll(env, [ask], engine, now, options)).ids[0];
  }


  async function demandAll(env, asks, engine, now = Date.now(), {before=[],after=[]} = {}) {
    if ((before.length || after.length) && asks.length>DEMAND_CHUNK) throw new Error("atomic task admission exceeds one batch");
    const db = env.LIBRARY, at = iso(now), ids = [];
    for (const a of asks) {
      if (!verbs[a.verb]) throw new Error(`no verb ${a.verb}`);
      producerOf(a.producer);
      if (typeof a.ref !== "string" || !a.ref.trim()) throw new Error("a compute task requires a source");
      if (a.zone !== undefined && a.zone !== null && (!Number.isInteger(a.zone) || a.zone < 0 || a.zone > 99))
        throw new Error("queue zone must be an integer from00 to99");
      const payload = a.payload ?? {}, trigger = a.trigger_id ?? (payload.review || payload.batch || a.ref);
      if (!payload || typeof payload !== "object" || Array.isArray(payload) || typeof trigger !== "string" || !trigger.trim())
        throw new Error("a compute task requires a source");
      ids.push(await questionId(a.verb, a.request));
    }
    for (let i = 0; i < asks.length; i += DEMAND_CHUNK) {
      const qs = new Map(), ds = new Map();
      asks.slice(i, i + DEMAND_CHUNK).forEach(({ producer, ref, verb, request, due_in_ms, payload = {}, trigger_id = payload.review || payload.batch || ref, force = false, invalidate = false, reason = "requested_recalculation", lanes = null, waits = false, initiator = null, zone = null, capability = null }, k) => {
        const id = ids[i + k], due_at = iso(now + due_in_ms), w = waits ? 1 : 0, l = lanes ? JSON.stringify(lanes) : null;
        const q = qs.get(id);
        if (!q) qs.set(id, { id, ordinal: k, zone, capability, verb, request: canon(computationInput(verb, request)), due_at, lanes: l, waits: w, producer, initiator, force, invalidate, reason });
        else Object.assign(q, { ...(due_at < q.due_at ? { due_at, producer, initiator } : {}), waits: Math.max(q.waits, w), lanes: q.lanes ?? l, force: q.force || force, invalidate: q.invalidate || invalidate, ...(force ? {reason} : {}) });
        ds.set(JSON.stringify([producer, ref, trigger_id]), { producer, ref, trigger_id, ordinal: k, question: id, due_at, waits: w, payload: JSON.stringify(payload), initiator, zone });
      });
      // IN KEY ORDER, so two asks writing the same rows take their locks alike.
      const sorted = (m, key) => [...m.values()].sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0));
      // A QUESTION THE CLIENTS' ENGINE REFUSED (`refuseRows`) stays withdrawn on
      // that engine however often it is asked again; a new engine tries it anew.
      await db.batch([
        ...(i===0 ? before : []),
        db.prepare(`SELECT compute_admit_tasks(?1::text,?2::text,?3::text,?4::text)`)
          .bind(JSON.stringify(sorted(qs, (q) => q.id)), JSON.stringify(sorted(ds, (d) => `${d.producer}\u0000${d.ref}\u0000${d.trigger_id}`)), engine || "", at),
        ...after,
        // A PRODUCER NEVER SEEN starts at the least any has used: no credit banked.
        db.prepare(`INSERT INTO state_producer_compute_usage (producer, used)
                      SELECT x.p, COALESCE((SELECT min(used) FROM state_producer_compute_usage), 0) FROM jsonb_array_elements_text((?1::text)::jsonb) AS x(p)
                    ON CONFLICT (producer) DO NOTHING`)
          .bind(JSON.stringify([...new Set([...ds.values()].map((d) => d.producer))].sort())),
      ]);
    }
    const { results } = await db.prepare(`SELECT q.*
        FROM state_compute_work q WHERE q.id IN ${listOf(db, "?1")} AND q.state IN ('fact', 'spot', 'disputed', 'nondeterministic')`)
      .bind(JSON.stringify([...new Set(ids)])).all();
    // Explicit retirement opens a new round. A different deployment alone
    // changes neither the entity inputs nor the validity of an accepted fact.
    const settled = (q) => q.state === "fact" || q.state === "spot";
    return { ids, ready: results.filter(settled) };
  }


  /// A PRODUCER NO LONGER ASKS. A question nobody asks for any more is withdrawn,
  /// and one still asked for keeps the earliest deadline still standing.
  async function withdraw(env, producer, ref, now = Date.now()) {
    const db = env.LIBRARY;
    const d = await db.prepare("SELECT question FROM relation_task_consumers WHERE producer = ? AND ref = ?").bind(producer, ref).first();
    if (!d) return;
    await db.prepare("UPDATE relation_task_consumers SET state = 'withdrawn' WHERE producer = ? AND ref = ? AND state = 'live'").bind(producer, ref).run();
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
      await db.prepare(`UPDATE relation_task_consumers SET state = 'withdrawn' WHERE (producer, ref) IN (SELECT d.producer, d.ref FROM relation_task_consumers d
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
  const readyFact = (db, id, answer, engine, at) => db.prepare(`UPDATE relation_task_consumers SET delivery_at=?4::text
    WHERE question=?1::text AND state='live' AND EXISTS (SELECT 1 FROM state_compute_work q WHERE q.id=?1::text
      AND q.state IN ('fact','spot') AND q.canon=?2::text AND q.fact_engine=?3::text)`)
    .bind(id, answer, engine, at);


  return { liveDemands, demand, demandAll, withdraw, restate, sift, restateAll, commit, readyFact };
}
