// SPDX-License-Identifier: AGPL-3.0-or-later
// Retryable delivery of committed facts to registered consumers.
import { iso } from "./instant.js";

export function createDelivery({ producerOf, liveDemands, commit } = {}) {
  const listOf = (db, p) => `(SELECT value FROM jsonb_array_elements_text((${p}::text)::jsonb))`;
  const REDELIVER_BACK_MS = 7 * 86_400_000;
  /// EVERY LIVE DEMAND IS TOLD THE FACT, once.
  async function deliver(env, q, now, by = null) {
    const db = env.LIBRARY;
    const answerOf = by || await db.prepare("SELECT * FROM answers WHERE question = ? AND canon = ? ORDER BY official DESC, id LIMIT 1")
      .bind(q.id, q.canon).first();
    await deliverGroups(env, [{ q, answerOf, demands: await liveDemands(db, q.id) }], now);
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
    await deliverGroups(env, qs.map(q => ({ q, answerOf: answers.find(a => a.question === q.id) || null,
      demands: demands.filter(d => d.question === q.id) })), now);
  }

  /// A committed fact is delivered in a separate transaction. Failed consumers
  /// leave their demands live; a successful consumer never waits for their retry.
  async function deliverGroups(env, groups, now) {
    const prepared = [], callbacks = [], errors = [];
    const failed = async (q, demand, error) => {
      if (error.code === "WS002") return;
      errors.push(error);
      try {
        await env.LIBRARY.prepare("UPDATE demands SET delivery_at=? WHERE producer=? AND ref=? AND question=? AND asked_at=? AND payload=? AND state='live'")
          .bind(iso(now + 60000), demand.producer, demand.ref, q.id, demand.asked_at, demand.payload).run();
      } catch (error) { errors.push(error); }
    };
    const ordered = [...groups].sort((a, b) => a.q.id < b.q.id ? -1 : a.q.id > b.q.id ? 1 : 0);
    for (const { q, answerOf, demands } of ordered) for (const demand of [...demands].sort((a, b) =>
      a.producer < b.producer ? -1 : a.producer > b.producer ? 1 : a.ref < b.ref ? -1 : a.ref > b.ref ? 1 : 0)) {
      const writes = [], later = [];
      try {
        deliverPlan(env, q, now, answerOf, [demand], writes, later);
        if (writes.length) prepared.push({ writes, q, demand });
        callbacks.push(...later.map(run => ({ run, q, demand })));
      } catch (error) { await failed(q, demand, error); }
    }
    if (prepared.length) {
      try { await commit(env, prepared.flatMap(x => x.writes)); }
      catch (_) {
        for (const { writes, q, demand } of prepared) {
          try { await commit(env, writes); } catch (error) { await failed(q, demand, error); }
        }
      }
    }
    for (const { run, q, demand } of callbacks) {
      try { await run(); } catch (error) { await failed(q, demand, error); }
    }
    if (errors.length) throw errors[0];
  }

  /// Verification commits only task state. Delivery runs afterwards, and reads
  /// the demands still live so a concurrent successful delivery is not repeated.
  function schedule(env, q, now, answerOf, demands, writes, later) {
    later.push({ q, answerOf });
  }

  /// A pack's delivery reads and commits together, independent of its row count.
  /// Only the fact actually committed may be delivered to its live consumers.
  async function flush(env, pending, now) {
    if (!pending.length) return;
    const keys = pending.map(({ q }) => ({ id: q.id, canon: q.canon, engine: q.fact_engine }));
    const { results: live } = await env.LIBRARY.prepare(`SELECT d.*, q.fact_at FROM demands d JOIN questions q ON q.id = d.question
      JOIN jsonb_to_recordset((?::text)::jsonb) AS x(id text, canon text, engine text)
        ON x.id = q.id AND x.canon = q.canon AND x.engine = q.fact_engine
      WHERE d.state = 'live' AND q.state IN ('fact', 'spot')`)
      .bind(JSON.stringify(keys)).all();
    await deliverGroups(env, pending.map(({ q, answerOf }) => {
      const demands = live.filter(d => d.question === q.id);
      return { q: { ...q, fact_at: demands[0]?.fact_at || q.fact_at }, answerOf, demands };
    }), now);
  }


  async function redeliver(env, now = Date.now(), max = 100) {
    const db = env.LIBRARY, until = iso(now - 10 * 60_000), floor = iso(now - REDELIVER_BACK_MS);
    const { results: ready } = await db.prepare(`WITH pending AS (
      SELECT d.question,min(d.delivery_at) retry_at FROM demands d WHERE d.state='live' AND d.delivery_at<=?1::text
        AND EXISTS (SELECT 1 FROM questions q WHERE q.id=d.question AND q.state IN ('fact','spot'))
      GROUP BY d.question ORDER BY retry_at,d.question LIMIT ?2::bigint)
      SELECT q.* FROM pending p JOIN questions q ON q.id=p.question ORDER BY p.retry_at,q.id`).bind(iso(now), max).all();
    let told = 0, stuck = null;
    for (const q of ready) {
      try { await deliver(env, q, now); told++; } catch (_) { /* demands retain their retry time */ }
    }
    if (ready.length >= max) return told;
    // Pre-migration pending facts retain the bounded cursor path.
    const kept = await db.prepare("SELECT through FROM factory_cursors WHERE name = 'redeliver'").first();
    const from = kept && kept.through > floor ? kept.through : floor;
    const { results } = await db.prepare(`SELECT q.* FROM questions q WHERE q.fact_at >= ?1::text AND q.fact_at < ?2::text
        AND q.state IN ('fact', 'spot') AND EXISTS (SELECT 1 FROM demands d WHERE d.question = q.id AND d.state = 'live')
        AND NOT EXISTS (SELECT 1 FROM demands d WHERE d.question=q.id AND d.state='live' AND d.delivery_at IS NOT NULL)
        ORDER BY q.fact_at LIMIT ?3::bigint`).bind(from, until, max - ready.length).all();
    for (const q of results) {
      try { await deliver(env, q, now); told += 1; } catch (_) { stuck ??= q.fact_at; }
    }
    const through = stuck ?? (results.length === max - ready.length ? results[results.length - 1].fact_at : until);
    await db.prepare(`INSERT INTO factory_cursors (name, through) VALUES ('redeliver', ?1::text)
        ON CONFLICT (name) DO UPDATE SET through = excluded.through`).bind(through).run();
    return told;
  }


  /// …AS WRITES where the producer states its own (`on_fact_writes`), so a whole
  /// pack's facts land in one round trip; a producer that must call out is told
  /// after them (`later`).
  function deliverPlan(env, q, now, answerOf, demands, writes, later) {
    const db = env.LIBRARY;
    const fact = { question: q.id, verb: q.verb, request: JSON.parse(q.request), canon: q.canon, at: q.fact_at || iso(now),
      result: answerOf ? JSON.parse(answerOf.result) : null, work: answerOf ? answerOf.work : 0,
      engine: answerOf ? answerOf.engine : null, official: !!(answerOf && answerOf.official) };
    for (const d of demands) {
      const p = producerOf(d.producer), told = { ...d, payload: JSON.parse(d.payload) };
      const guard = db.prepare("SELECT factory_delivery_guard(?::text,?::text,?::text,?::text,?::text,?::text,?::text)")
        .bind(q.id, q.canon, q.fact_engine, d.producer, d.ref, d.asked_at, d.payload);
      const served = db.prepare("UPDATE demands SET state='served',served_at=?,delivery_at=NULL WHERE producer=? AND ref=? AND question=? AND asked_at=? AND payload=? AND state='live'")
        .bind(iso(now), d.producer, d.ref, q.id, d.asked_at, d.payload);
      if (p.on_fact_writes) writes.push(guard, ...p.on_fact_writes(env, told, fact).map((side) => ({ side })), served);
      else later.push(async () => { await guard.run(); await p.on_fact(env, told, fact); await served.run(); });
    }
  }


  return { deliver, deliverMany, redeliver, schedule, flush };
}
