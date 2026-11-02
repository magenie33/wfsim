// SPDX-License-Identifier: AGPL-3.0-or-later
// Compute-cost accounting for fair scheduling.
import { PRODUCER_MEAN_SHARE } from "./task_contract.js";

export function createUsage() {
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


  return { charge, learnCost, liftIdle };
}
