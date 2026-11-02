// SPDX-License-Identifier: AGPL-3.0-or-later
// Idempotent contribution settlement and repayment.
import { iso } from "./instant.js";
import { independent, FACTS_PER_REFUSAL_FORGIVEN } from "./task_contract.js";

export function createCredit() {
  const REDELIVER_BACK_MS = 7 * 86_400_000, REPAY_SLICE_MS = 3_600_000;
  /// The verifier records its exact payment decision with the fact. Recovery
  /// never reconstructs adjusted work or witness selection from a later state.
  function prepareCredit(db, answers, now) {
    const owed = answers.filter(a => !a.official && !a.credited).map(a => ({ id: Number(a.id), work: a.work }));
    return db.prepare(`UPDATE answers a SET credit_work=x.work,credit_at=?2::text
      FROM jsonb_to_recordset((?1::text)::jsonb) AS x(id bigint,work bigint)
      WHERE a.id=x.id AND a.credited=0 AND a.credit_work IS NULL`).bind(JSON.stringify(owed), iso(now));
  }
  /// ONLY A FACT SCORES, once per answer: its work to its device, in its hour,
  /// and one fact toward forgiving a refusal (docs/BOARD.md §"Contribution").
  /// A CREDIT IS ONE TRANSACTION, one round trip: its devices locked in device
  /// order, then the answers flipped to credited (`credited = 0`, so each is paid
  /// once) and each device paid what flipped. In two steps, a pass dying between
  /// them left answers marked paid whose work nobody was ever given.
  async function credit(db, answers, now) {
    // A durable payment decision wins; legacy repayment uses the work supplied
    // here, never the answer's original uncorrected work from the table.
    const owed = answers.filter((a) => !a.official && !a.credited).map((a) => ({ id: Number(a.id), device: a.device, work: a.work }));
    if (!owed.length) return;
    const questions = JSON.stringify([...new Set(answers.map((a) => a.question).filter(Boolean))]);
    await db.batch([
      db.prepare(`SELECT id FROM verifiers WHERE id IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb)) ORDER BY id FOR UPDATE`)
        .bind(JSON.stringify([...new Set(owed.map((a) => a.device))])),
      db.prepare(`WITH owed AS (SELECT a.id,a.device,COALESCE(a.credit_work,(e->>'work')::bigint) AS work,
              COALESCE(a.credit_at,?3::text) AS at
            FROM jsonb_array_elements((?1::text)::jsonb) e JOIN answers a ON a.id=(e->>'id')::bigint),
          paid AS (UPDATE answers a SET credited = 1,credit_work=NULL,credit_at=NULL FROM owed
            WHERE a.id = owed.id AND a.credited = 0 AND a.void=0 RETURNING owed.device, owed.work, owed.at),
          sums AS (SELECT device, sum(work)::bigint AS work, count(*)::bigint AS n FROM paid GROUP BY device),
          v AS (UPDATE verifiers SET work = verifiers.work + sums.work, agreed = agreed + sums.n,
              refusals = CASE WHEN refusals > 0 AND clean + sums.n >= refusals * ?2::bigint THEN refusals - 1 ELSE refusals END,
              clean = CASE WHEN refusals = 0 OR clean + sums.n >= refusals * ?2::bigint THEN 0 ELSE clean + sums.n END
            FROM sums WHERE verifiers.id = sums.device RETURNING 1)
        INSERT INTO verifier_hours AS h (verifier, hour, work)
          SELECT device,left(at,13),sum(work)::bigint FROM paid GROUP BY device,left(at,13)
          ON CONFLICT (verifier, hour) DO UPDATE SET work = h.work + excluded.work`)
        .bind(JSON.stringify(owed), FACTS_PER_REFUSAL_FORGIVEN, iso(now)),
      // …AND WHAT NO WITNESS RULE CAN EVER PAY, marked covered (worker/pg/sync.sql `factory_cover`).
      db.prepare("SELECT factory_cover(?::text) AS n").bind(questions),
    ]);
  }


  async function repay(env, now = Date.now(), max = 200) {
    const db = env.LIBRARY, floor = iso(now - REDELIVER_BACK_MS);
    const { results: pending } = await db.prepare(`SELECT a.*,a.credit_work AS work FROM answers a
      WHERE a.credit_work IS NOT NULL AND a.credited=0 AND a.void=0 AND a.official=0
      ORDER BY a.credit_at,a.id LIMIT ?::bigint`).bind(max).all();
    let recovered = 0;
    for (const at of new Set(pending.map(a => a.credit_at))) {
      const group = pending.filter(a => a.credit_at === at);
      await credit(db, group, Date.parse(at)); recovered += group.length;
    }
    const kept = await db.prepare("SELECT through FROM factory_cursors WHERE name = 'repay'").first();
    const from = kept ? (kept.through > floor ? kept.through : floor) : iso(now - 10 * 60_000 - REPAY_SLICE_MS);
    const until = iso(Math.min(now - 10 * 60_000, Date.parse(from) + REPAY_SLICE_MS));
    const { results: qs } = await db.prepare(`SELECT q.id, q.canon, q.fact_engine, q.fact_at FROM questions q
        WHERE q.fact_at >= ?1::text AND q.fact_at < ?2::text AND q.state IN ('fact', 'spot')
          AND EXISTS (SELECT 1 FROM answers a WHERE a.question = q.id AND a.credited = 0 AND a.void = 0 AND a.official = 0
                        AND a.canon = q.canon AND a.at <= q.fact_at
                        AND NOT EXISTS (SELECT 1 FROM answers c WHERE c.question = a.question AND c.credited = 1 AND c.void = 0 AND c.official = 0
                                        AND (c.owner = a.owner OR (coalesce(a.net, '') <> '' AND c.net = a.net))))
        ORDER BY q.fact_at, q.id LIMIT ?3::bigint`)
      .bind(from, until, max).all();
    let paid = 0, stuck = null;
    for (const q of qs) {
      const { results: all } = await db.prepare("SELECT * FROM answers WHERE question = ? AND void = 0 AND official = 0 AND canon = ? AND at <= ? ORDER BY id")
        .bind(q.id, q.canon, q.fact_at).all();
      const group = independent([...all.filter((a) => a.engine === q.fact_engine), ...all.filter((a) => a.engine !== q.fact_engine)]);
      const owed = group.filter((a) => !a.credited);
      if (!owed.length) continue;
      try { await credit(db, owed, Date.parse(q.fact_at)); paid += owed.length; } catch (_) { stuck ??= q.fact_at; }
    }
    const through = stuck ?? (qs.length === max ? qs[qs.length - 1].fact_at : until);
    if (through > from || !kept) {
      await db.prepare(`INSERT INTO factory_cursors (name, through) VALUES ('repay', ?1::text)
          ON CONFLICT (name) DO UPDATE SET through = excluded.through`).bind(through).run();
    }
    return paid + recovered;
  }


  return { prepareCredit, credit, repay };
}
