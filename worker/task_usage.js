// SPDX-License-Identifier: AGPL-3.0-or-later
// Compute-cost accounting for fair scheduling.
import { PRODUCER_MEAN_SHARE } from "./task_contract.js";

async function idleUsageFloor(db) {
  const row = await db.prepare(`SELECT c.task_cutoff::text AS cutoff FROM compute_order_cut() c
    WHERE EXISTS (SELECT 1 FROM pg_index i WHERE i.indexrelid=to_regclass('compute_scheduling_stock_epoch_zone')
      AND i.indrelid='state_compute_scheduling'::regclass AND i.indisvalid AND i.indisready
      AND substring(pg_get_expr(i.indpred,i.indrelid) FROM 'task_id > ['' ]*([0-9]+)')::bigint<=c.task_cutoff)`).first();
  const cutoff = row?.cutoff;
  if (!/^[1-9][0-9]*$/.test(cutoff ?? "")) return `WITH floor AS MATERIALIZED (
    SELECT min(a.used) AS v FROM state_producer_compute_usage a WHERE EXISTS (
      SELECT 1 FROM state_compute_effective_work q WHERE q.state='open' AND q.producer=a.producer))`;

  const zone = "coalesce(s.zone,factory_default_zone(s.producer))";
  const position = "coalesce(s.zone_position,s.task_id)";
  const active = (q) => `NOT EXISTS(SELECT 1 FROM record_compute_order_cancellations c
    WHERE (c.question,c.task_id,c.cut_id)=(${q}.id,${q}.task_id,(SELECT id FROM cut)))`;
  return `WITH RECURSIVE cut AS MATERIALIZED(SELECT id,keep_zone,keep_position,keep_id FROM compute_order_cut()),
    ledger AS MATERIALIZED (SELECT producer,used FROM state_producer_compute_usage),
    lower_bound AS (SELECT min(used) AS v FROM ledger),
    prefix AS MATERIALIZED (
      SELECT s.id,s.task_id,s.producer FROM state_compute_scheduling s
      WHERE s.state='open' AND s.task_id<=${cutoff}
        AND (${zone},${position},s.id)<=(SELECT keep_zone,keep_position,keep_id FROM cut)
      ORDER BY ${zone},${position},s.id
    ),
    retained AS (
      SELECT * FROM prefix
      UNION ALL
      SELECT s.id,s.task_id,s.producer FROM compute_order_cut_protections() p CROSS JOIN LATERAL (
        SELECT * FROM state_compute_scheduling s WHERE s.id=p.question OFFSET 0
      ) s WHERE p.cut_id=(SELECT id FROM cut) AND s.state='open' AND s.task_id<=${cutoff}
        AND (${zone},${position},s.id)>(SELECT keep_zone,keep_position,keep_id FROM cut)
    ),
    retained_floor AS (SELECT min(l.used) AS v FROM retained r JOIN ledger l USING(producer) WHERE ${active("r")}),
    cursor(z,pos,id,v) AS (
      SELECT '-Infinity'::numeric,0::bigint,''::text COLLATE "C",(SELECT v FROM retained_floor)
      UNION ALL
      SELECT tail.z,tail.pos,tail.id,least(c.v,tail.v) FROM cursor c CROSS JOIN lower_bound m CROSS JOIN LATERAL (
        WITH page AS MATERIALIZED (
          SELECT s.id,s.task_id,s.producer,${zone}::numeric z,${position} pos FROM state_compute_scheduling s
          WHERE s.state='open' AND s.task_id>${cutoff} AND (${zone}::numeric,${position},s.id)>(c.z,c.pos,c.id)
          ORDER BY ${zone}::numeric,${position},s.id LIMIT 4096
        )
        SELECT z,pos,id,(SELECT min(l.used) FROM page r JOIN ledger l USING(producer) WHERE ${active("r")}) v
        FROM page ORDER BY z DESC,pos DESC,id DESC LIMIT 1
      ) tail WHERE m.v IS NOT NULL AND (c.v IS NULL OR c.v>m.v)
    ),
    floor AS (SELECT min(v) AS v FROM cursor)`;
}

function answerCosts(items) {
  const by = {};
  for (const { q, a } of items) {
    if (!Number.isFinite(a.compute_ms) || a.compute_ms <= 0) continue;
    const b = by[q.producer || ""] ||= { ms: 0, n: 0 };
    b.ms += a.compute_ms; b.n += 1;
  }
  return Object.entries(by).sort().map(([p, b]) => ({ p, ms: b.ms, n: b.n }));
}

export function createUsage() {
  /// WHAT A HAND-OUT COSTS ITS PRODUCER: the question at its producer's mean
  /// fight, over its weight (docs/BOARD.md §"Fair share between producers"); a
  /// pack is charged in the statement that leases it (`nextPack`).
  async function charge(db, q) {
    await db.prepare("UPDATE state_producer_compute_usage SET used = used + answer_ms / weight WHERE producer = ?").bind(q.producer || "").run();
  }


  /// …AND WHAT ITS ANSWERS COST, folded into each producer's mean: `k` answers
  /// move it as `k` folds of `PRODUCER_MEAN_SHARE` would. A statement for the
  /// answers' own batch, or null. worker/pg/sync.sql `factory_learn`.
  function learnCost(db, items) {
    const rows = answerCosts(items);
    if (!rows.length) return null;
    return db.prepare("SELECT factory_learn(?1::text, ?2::double precision)").bind(JSON.stringify(rows), PRODUCER_MEAN_SHARE);
  }


  /// A PRODUCER WITH NOTHING OPEN is lifted to the least any producer with open
  /// work has used, so it comes back level and never with a share banked while
  /// idle. Run by the watchdog's cron.
  async function liftIdle(env) {
    const db = env.LIBRARY;
    const floor = await idleUsageFloor(db);
    // Every value below the active minimum belongs to an idle producer.
    await db.prepare(`${floor} UPDATE state_producer_compute_usage u SET used=m.v
      FROM floor m WHERE u.used<m.v`).run();
  }


  return { charge, learnCost, answerCosts, liftIdle };
}
