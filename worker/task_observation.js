// SPDX-License-Identifier: AGPL-3.0-or-later
// Read-only task health observations.
import { iso } from "./instant.js";
import { dialsOf } from "./dials.js";
import { OFFICIAL_STATES, ceilingS, OVERDUE_MAX, OVERDUE_HOURS_MS, UNSETTLED_MS, DISAGREE_MIN, DISAGREE_SHARE } from "./task_contract.js";

export function createObservation() {
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
    // is handed out, however long a single row takes. Each past-due demand looks
    // up its own question (LATERAL): joined, the planner guesses most live rows
    // past due and walks every open question, minutes past the role's timeout.
    const { results: stalled } = await db.prepare(`SELECT d.producer, count(DISTINCT d.question) AS questions, min(d.due_at) AS oldest_due
         FROM demands d CROSS JOIN LATERAL (SELECT 1 FROM questions q WHERE q.id = d.question AND q.state = 'open'
                AND factory_takeable(q.lease_until, q.leased_at, ?1::text, ?3::bigint) LIMIT 1) q
        WHERE d.state = 'live' AND d.due_at < ?1::text
        GROUP BY d.producer
       HAVING NOT EXISTS (SELECT 1 FROM demands e JOIN answers a ON a.question = e.question
                           WHERE e.producer = d.producer AND a.at > ?2::text)`).bind(at, recent, ceiling).all();
    const odd = (await db.prepare(`SELECT SUM(n) AS n FROM (
      SELECT count(*) AS n FROM questions WHERE state = 'nondeterministic'
      UNION ALL
      SELECT count(*) AS n FROM questions WHERE odd = 1 AND fact_at > ? AND state != 'nondeterministic'
    ) x`)
      .bind(iso(now - 24 * 3_600_000)).first()).n;
    // WAITING ON THE OFFICIAL MACHINES since its last answer put it there: a
    // dispute nobody settles is the one place a question can still wait for ever.
    // Not `opened_at` — a refresh opened hours ago turns spot a minute ago. The
    // last answer is before the cutoff when none is after it: the hours' answers
    // are one range of `answers_by_at`, where a latest per question read every
    // answer of tens of thousands of questions from the table.
    const cutoff = iso(now - UNSETTLED_MS);
    const unsettled = (await db.prepare(
      `SELECT count(*) AS n FROM questions q WHERE q.state IN (${OFFICIAL_STATES.map(() => "?").join(", ")})
          AND q.id NOT IN (SELECT a.question FROM answers a WHERE a.at >= ?)
          AND (q.opened_at < ? OR EXISTS (SELECT 1 FROM answers a WHERE a.question = q.id))`)
      .bind(...OFFICIAL_STATES, cutoff, cutoff).first()).n;
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


  return { audit };
}
