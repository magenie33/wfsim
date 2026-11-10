// SPDX-License-Identifier: AGPL-3.0-or-later
// Read-only task health observations.
import { iso } from "./instant.js";
import { dialsOf } from "./dials.js";
import { OFFICIAL_STATES, ceilingS, OVERDUE_MAX, OVERDUE_HOURS_MS, UNSETTLED_MS, DISAGREE_MIN, DISAGREE_SHARE } from "./task_contract.js";

export function watchdogFailure(step, error) {
  const code = error?.code === "57014" ? "timeout" : ["40P01", "55P03"].includes(error?.code) ? "lock"
    : /^08/.test(error?.code || "") ? "connection" : "error";
  console.error("watchdog", step, code);
  return { kind: `watchdog_failed:${step}`, code };
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
    // Not `opened_at` — a refresh opened hours ago turns spot a minute ago. The
    // last answer is before the cutoff when none is after it. Keep both answer
    // reads on that official question; a global recent-answer set also reads
    // the volunteers' history when no official question needs it.
    await observe("unsettled", async () => {
      const cutoff = iso(now - UNSETTLED_MS);
      const unsettled = (await db.prepare(
      `SELECT count(*) AS n FROM state_compute_work q WHERE q.state IN (${OFFICIAL_STATES.map(() => "?").join(", ")})
          AND NOT EXISTS (SELECT 1 FROM (SELECT a.at FROM record_candidate_answers a WHERE a.question = q.id OFFSET 0) a WHERE a.at >= ?)
          AND (q.opened_at < ? OR EXISTS (SELECT 1 FROM record_candidate_answers a WHERE a.question = q.id OFFSET 0))`)
        .bind(...OFFICIAL_STATES, cutoff, cutoff).first()).n;
      if (unsettled) out.push({ kind: "unsettled", questions: unsettled });
    });
    // PAST DUE AND STILL OPEN, by kind. Stop once the alert threshold is crossed;
    // a large queue is a lower bound, not another full inventory scan.
    await observe("overdue", async () => {
      const { results: late } = await db.prepare(
      `WITH overdue AS MATERIALIZED (
         SELECT q.id,q.due_at FROM state_compute_work q WHERE q.state='open' AND q.waits IN (0,1) AND q.due_at<?1::text
           AND EXISTS (SELECT 1 FROM relation_task_consumers d WHERE d.question=q.id AND d.state='live')
         ORDER BY q.waits DESC,q.due_at,q.id LIMIT ?2::bigint)
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
