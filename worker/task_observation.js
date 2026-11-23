// SPDX-License-Identifier: AGPL-3.0-or-later
// Read-only task health observations.
import { iso } from "./instant.js";
import { dialsOf } from "./dials.js";
import { OFFICIAL_STATES, ceilingS, OVERDUE_MAX, OVERDUE_HOURS_MS, UNSETTLED_MS, DISAGREE_MIN, DISAGREE_SHARE } from "./task_contract.js";

const UNSETTLED_POINT_LOOKUPS = 128;

export function watchdogFailure(step, error) {
  const code = error?.code === "57014" ? "timeout" : ["40P01", "55P03"].includes(error?.code) ? "lock"
    : /^08/.test(error?.code || "") ? "connection" : "error";
  console.error("watchdog", step, code);
  return { kind: `watchdog_failed:${step}`, code };
}

async function overdueRanges(db) {
  const row = await db.prepare(`SELECT c.task_cutoff::text AS cutoff FROM compute_order_cut() c
    WHERE EXISTS (SELECT 1 FROM pg_index i WHERE i.indexrelid=to_regclass('compute_scheduling_stock_epoch_due')
      AND i.indrelid='state_compute_scheduling'::regclass AND i.indisvalid AND i.indisready
      AND substring(pg_get_expr(i.indpred,i.indrelid) FROM 'task_id > ['' ]*([0-9]+)')::bigint<=c.task_cutoff)`).first();
  const cutoff = row?.cutoff;
  if (!/^[1-9][0-9]*$/.test(cutoff ?? "")) return `overdue AS MATERIALIZED (
    SELECT q.id,q.due_at FROM state_compute_effective_work q WHERE q.state='open' AND q.waits IN (0,1) AND q.due_at<?1::text
      AND EXISTS (SELECT 1 FROM relation_task_consumers d WHERE d.question=q.id AND d.state='live')
    ORDER BY q.waits DESC,q.due_at,q.id LIMIT ?2::bigint)`;
  // Bound the new orders, retained prefix and protected tail separately before merging.
  return `cut AS MATERIALIZED(SELECT id,keep_zone,keep_position,keep_id FROM compute_order_cut()),
    prefix AS MATERIALIZED (
      SELECT s.id,s.task_id,s.waits,s.due_at FROM state_compute_scheduling s
      WHERE s.state='open' AND s.task_id<=${cutoff}
        AND (coalesce(s.zone,factory_default_zone(s.producer)),coalesce(s.zone_position,s.task_id),s.id)
          <=(SELECT keep_zone,keep_position,keep_id FROM cut)
      ORDER BY coalesce(s.zone,factory_default_zone(s.producer)),coalesce(s.zone_position,s.task_id),s.id
    ),
    eligible AS NOT MATERIALIZED (
      SELECT s.id,s.task_id,s.waits,s.due_at,s.zone,s.zone_position,s.producer
      FROM state_compute_scheduling s WHERE s.state='open' AND s.waits IN(0,1) AND s.due_at<?1::text
        AND NOT EXISTS (SELECT 1 FROM record_compute_order_cancellations c
          WHERE (c.question,c.task_id,c.cut_id)=(s.id,s.task_id,(SELECT id FROM cut)))
        AND EXISTS (SELECT 1 FROM relation_task_consumers d WHERE d.question=s.id AND d.state='live')
    ),
    overdue AS MATERIALIZED (
      SELECT q.id,q.due_at FROM (
        (SELECT e.id,e.waits,e.due_at FROM eligible e WHERE e.task_id>${cutoff}
          ORDER BY e.waits DESC,e.due_at,e.id LIMIT ?2::bigint)
        UNION ALL
        (SELECT s.id,s.waits,s.due_at FROM prefix s CROSS JOIN LATERAL (
           SELECT 1 FROM eligible e WHERE (e.id,e.task_id)=(s.id,s.task_id) OFFSET 0
         ) e ORDER BY s.waits DESC,s.due_at,s.id LIMIT ?2::bigint)
        UNION ALL
        (SELECT s.id,s.waits,s.due_at FROM compute_order_cut_protections() p CROSS JOIN LATERAL (
          SELECT e.id,e.waits,e.due_at FROM eligible e WHERE e.id=p.question AND e.task_id<=${cutoff}
            AND (coalesce(e.zone,factory_default_zone(e.producer)),coalesce(e.zone_position,e.task_id),e.id)
                  >(SELECT keep_zone,keep_position,keep_id FROM cut)
          OFFSET 0
        ) s WHERE p.cut_id=(SELECT id FROM cut)
          ORDER BY s.waits DESC,s.due_at,s.id LIMIT ?2::bigint)
      ) q ORDER BY q.waits DESC,q.due_at,q.id LIMIT ?2::bigint
    )`;
}

export function createObservation() {
  /// WHAT IS WRONG RIGHT NOW, for the watchdog: a kind whose overdue questions
  /// went unanswered while witnesses were at work, questions that were not
  /// deterministic, disputes nobody settled, a backlog past due, and clients
  /// disagreeing with the facts on the served `engine`. Empty when healthy.
  async function audit(env, now = Date.now(), engine = null) {
    const db = env.LIBRARY, at = iso(now), recent = iso(now - 30 * 60_000), out = [];
    const observe = async (step, read, fallback) => {
      try { return await read(); } catch (error) { out.push(watchdogFailure(step, error)); return fallback; }
    };
    // A PRODUCER WHOSE OVERDUE QUESTIONS GOT NO ANSWER AT ALL in half an hour
    // while witnesses were at work: something hands them out to nobody. One
    // overdue question waiting its turn is a queue; a whole kind silent is a stall.
    // Only a question nobody holds counts: one a page is computing and renewing
    // is handed out, however long a single row takes. Enumerate producer keys
    // along demands_live, exclude those answered recently, then count only the
    // silent producers' overdue rows; healthy producers need no question scan.
    await observe("stalled", async () => {
      const active = (await db.prepare("SELECT count(*) AS n FROM entity_compute_devices WHERE last_at > ?").bind(recent).first()).n;
      if (!active) return;
      const ceiling = ceilingS((await dialsOf(env, now)).row_ceiling_ms);
      const { results: producers } = await db.prepare(`WITH RECURSIVE producers(producer) AS (
          (SELECT producer FROM relation_task_consumers WHERE state = 'live' ORDER BY producer LIMIT 1)
          UNION ALL
          SELECT (SELECT d.producer FROM relation_task_consumers d WHERE d.state = 'live' AND d.producer > p.producer ORDER BY d.producer LIMIT 1)
          FROM producers p WHERE p.producer IS NOT NULL),
        answered AS MATERIALIZED (SELECT DISTINCT e.producer FROM record_candidate_answers a JOIN relation_task_consumers e ON e.question = a.question WHERE a.at > ?1::text),
        silent AS MATERIALIZED (SELECT p.producer FROM producers p WHERE p.producer IS NOT NULL
          AND NOT EXISTS (SELECT 1 FROM answered a WHERE a.producer = p.producer))
        SELECT producer FROM silent`).bind(recent).all();
      // Each producer is a bound constant, so rare kinds use demands_live rather than a whole-table estimate.
      for (const p of producers) {
        const row = await db.prepare(`WITH sample AS MATERIALIZED (
          SELECT d.question,d.due_at FROM relation_task_consumers d CROSS JOIN LATERAL (
            SELECT 1 FROM state_compute_work q WHERE q.id=d.question AND q.state='open'
              AND factory_takeable(q.lease_until,q.leased_at,?1::text,?3::bigint) LIMIT 1) q
          WHERE d.state='live' AND d.due_at<?1::text AND d.producer=?2::text LIMIT ?4::bigint)
          SELECT count(DISTINCT d.question) AS questions,min(d.due_at) AS oldest_due,
            count(*)>=?4::bigint AS at_least FROM sample d`)
          .bind(at, p.producer, ceiling, OVERDUE_MAX + 1).first();
        if (row.questions) out.push({ kind: `stalled:${p.producer}`, producer: p.producer, ...row, witnesses_active: active });
      }
    });
    await observe("nondeterministic", async () => {
      const odd = (await db.prepare(`SELECT SUM(n) AS n FROM (
      SELECT count(*) AS n FROM state_compute_work WHERE state = 'nondeterministic'
      UNION ALL
      SELECT count(*) AS n FROM state_compute_work WHERE odd = 1 AND fact_at > ? AND state != 'nondeterministic'
    ) x`)
        .bind(iso(now - 24 * 3_600_000)).first()).n;
      if (odd) out.push({ kind: "nondeterministic", questions: odd });
    });
    // WAITING ON THE OFFICIAL MACHINES since its last answer put it there: a
    // dispute nobody settles is the one place a question can still wait for ever.
    // A recent answer can move an old open question to spot. Keyed lookups
    // serve small sets. Larger sets share a recent range when it is cheaper.
    // An official state can only be hidden by an explicit order cancellation.
    await observe("unsettled", async () => {
      const cutoff = iso(now - UNSETTLED_MS);
      const boundary = `?${OFFICIAL_STATES.length + 1}::text`;
      const unsettled = (await db.prepare(
      `WITH settlement_cut AS MATERIALIZED (SELECT id FROM compute_order_cut()),
        official AS MATERIALIZED (
          SELECT s.id,s.opened_at FROM state_compute_scheduling s
          WHERE s.state IN (${OFFICIAL_STATES.map((_, i) => `?${i + 1}`).join(", ")})
            AND NOT EXISTS (SELECT 1 FROM record_compute_order_cancellations c
              WHERE (c.question,c.task_id,c.cut_id)=(s.id,s.task_id,(SELECT id FROM settlement_cut)))
        ),
        scope AS MATERIALIZED (
          SELECT count(*) AS questions FROM official
        ),
        recent_sample AS MATERIALIZED (
          SELECT a.question FROM record_candidate_answers a
          WHERE a.at>=${boundary} AND (SELECT questions>${UNSETTLED_POINT_LOOKUPS} FROM scope)
          ORDER BY a.at LIMIT (SELECT questions+1 FROM scope)
        ),
        strategy AS MATERIALIZED (
          SELECT CASE WHEN questions>${UNSETTLED_POINT_LOOKUPS}
            THEN (SELECT count(*) FROM recent_sample)<=questions ELSE false END AS bulk FROM scope
        ),
        answered AS MATERIALIZED (
          SELECT DISTINCT question FROM recent_sample WHERE (SELECT bulk FROM strategy)
        ),
        quiet AS MATERIALIZED (
          SELECT q.id,q.opened_at FROM official q LEFT JOIN answered a ON a.question=q.id WHERE a.question IS NULL
            AND CASE WHEN (SELECT bulk FROM strategy) THEN true ELSE NOT EXISTS (
              SELECT 1 FROM (SELECT at FROM record_candidate_answers WHERE question=q.id OFFSET 0) recent
              WHERE recent.at>=${boundary}) END
        )
        SELECT count(*) AS n FROM quiet q WHERE CASE WHEN q.opened_at<${boundary} THEN true ELSE
          EXISTS (SELECT 1 FROM record_candidate_answers a WHERE a.question=q.id LIMIT 1 OFFSET 0) END`)
        .bind(...OFFICIAL_STATES, cutoff).first()).n;
      if (unsettled) out.push({ kind: "unsettled", questions: unsettled });
    });
    // PAST DUE AND STILL OPEN, by kind. Stop once the alert threshold is crossed;
    // a large queue is a lower bound, not another full inventory scan.
    await observe("overdue", async () => {
      const ranges = await overdueRanges(db);
      const { results: late } = await db.prepare(
      `WITH ${ranges}
       SELECT d.producer,count(DISTINCT q.id) AS questions,min(q.due_at) AS oldest_due,
         (SELECT count(*) FROM overdue) AS total,
         (SELECT count(*) FROM overdue)>=?2::bigint AS at_least
       FROM overdue q CROSS JOIN LATERAL (
         SELECT producer FROM relation_task_consumers WHERE question=q.id AND state='live' OFFSET 0) d
       GROUP BY d.producer`).bind(at, OVERDUE_MAX + 1).all();
      const lateTotal = late[0]?.total || 0;
      if (lateTotal >= OVERDUE_MAX || late.some((p) => Date.parse(p.oldest_due) < now - OVERDUE_HOURS_MS)) {
        out.push({ kind: "overdue", questions: lateTotal, at_least: late.some(p => p.at_least), producers: late });
      }
    });
    // CLIENTS DISAGREEING WITH THE FACTS of the last hour on the served engine,
    // and whose they are: a release or a client build that moved a number shows
    // here before anybody complains.
    if (engine) await observe("disagreeing", async () => {
      const { results: by } = await db.prepare(
        `SELECT a.owner, count(*) AS answers, sum(CASE WHEN a.canon <> q.canon THEN 1 ELSE 0 END) AS differ, count(DISTINCT a.device) AS devices
           FROM record_candidate_answers a JOIN state_compute_work q ON q.id = a.question
          WHERE a.engine = ? AND a.at > ? AND a.official = 0 AND q.canon IS NOT NULL
          GROUP BY a.owner`).bind(engine, iso(now - 3_600_000)).all();
      const answers = by.reduce((n, o) => n + o.answers, 0), differ = by.reduce((n, o) => n + o.differ, 0);
      if (differ >= DISAGREE_MIN && differ >= DISAGREE_SHARE * answers) {
        out.push({ kind: "disagreeing", engine, answers, differ, owners: by.filter((o) => o.differ).sort((x, y) => y.differ - x.differ).slice(0, 5) });
      }
    });
    // QUESTIONS THE CLIENTS' ENGINE REFUSED, withdrawn (`refuseRows`) in the last
    // six hours: a build no client can fight came in, so something let it in.
    await observe("refused_rows", async () => {
      const { results: refusedRows } = await db.prepare(`SELECT question, engine, owners, at FROM record_refusal_withdrawals WHERE at > ? ORDER BY at DESC LIMIT 20`)
        .bind(iso(now - 6 * 3_600_000)).all();
      if (refusedRows.length) out.push({ kind: "refused_rows", questions: refusedRows.length, sample: refusedRows.slice(0, 5) });
    });
    return out;
  }


  return { audit };
}
