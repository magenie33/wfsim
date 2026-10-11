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

/// HOW STATES LEAVE (§"States"). `open` takes volunteers' answers; the cap on
/// them reached without agreement is `disputed`, or `nondeterministic` when every
/// answer differs from every other — independent witnesses all disagreeing is an
/// engine whose answer is not deterministic, not a liar, and nobody is refused.
/// `spot` is a fact the official machines check again. Those three are the
/// official machines' alone.
export const OFFICIAL_STATES = ["disputed", "spot", "nondeterministic"];

/// Text whose bytes two equal values share: keys sorted, at every depth.
export const canon = (v) => (Array.isArray(v) ? `[${v.map(canon).join(",")}]`
  : v && typeof v === "object" ? `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canon(v[k])}`).join(",")}}`
  : JSON.stringify(v));

const hex = (buf) => [...new Uint8Array(buf)].map((x) => x.toString(16).padStart(2, "0")).join("");
/// A QUESTION'S ID IS WHAT IT ASKS: two producers asking one thing share it.
export async function questionId(verb, request) {
  return hex(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`${verb}\n${canon(request)}`))).slice(0, 32);
}
const newLease = () => hex(crypto.getRandomValues(new Uint8Array(16)));

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
export function factory({ verbs, producers, threshold = 2, spot = 0.05, random = () => Math.random(), on_disproved = null }) {
  const producerOf = (name) => {
    const p = producers[name];
    if (!p) throw new Error(`no producer ${name}`);
    return p;
  };

  async function liveDemands(db, id) {
    return (await db.prepare("SELECT * FROM demands WHERE question = ? AND state = 'live'").bind(id).all()).results;
  }

  /// A PRODUCER ASKS. The question is opened, or shared when another producer
  /// asked it already; a fact the served engine already holds is told at once.
  /// `waits`: a person is waiting on it now — §"Order".
  async function demand(env, ask, engine, now = Date.now()) {
    return (await demandAll(env, [ask], engine, now))[0];
  }

  /// MANY ASKS AT ONCE, written in a few round trips rather than three each — a
  /// producer asking thousands an hour is otherwise slower than its own clock.
  const DEMAND_CHUNK = 50;
  async function demandAll(env, asks, engine, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), ids = [];
    for (const a of asks) {
      if (!verbs[a.verb]) throw new Error(`no verb ${a.verb}`);
      producerOf(a.producer);
      ids.push(await questionId(a.verb, a.request));
    }
    for (let i = 0; i < asks.length; i += DEMAND_CHUNK) {
      const writes = [];
      asks.slice(i, i + DEMAND_CHUNK).forEach(({ producer, ref, verb, request, due_in_ms, payload = {}, lanes = null, waits = false }, k) => {
        const id = ids[i + k], due = iso(now + due_in_ms);
        writes.push(
          db.prepare(`INSERT INTO questions (id, verb, request, state, due_at, opened_at, lanes, waits) VALUES (?, ?, ?, 'open', ?, ?, ?, ?)
                      ON CONFLICT (id) DO UPDATE SET
                        state = CASE WHEN state = 'withdrawn' THEN 'open' ELSE state END,
                        opened_at = CASE WHEN state = 'withdrawn' THEN excluded.opened_at ELSE opened_at END,
                        due_at = CASE WHEN state = 'withdrawn' OR excluded.due_at < due_at THEN excluded.due_at ELSE due_at END,
                        waits = CASE WHEN state = 'withdrawn' THEN excluded.waits ELSE max(waits, excluded.waits) END,
                        lanes = COALESCE(lanes, excluded.lanes)`)
            .bind(id, verb, canon(request), due, at, lanes ? JSON.stringify(lanes) : null, waits ? 1 : 0),
          db.prepare(`INSERT INTO demands (producer, ref, question, state, asked_at, due_at, waits, payload) VALUES (?, ?, ?, 'live', ?, ?, ?, ?)
                      ON CONFLICT (producer, ref) DO UPDATE SET question = excluded.question, state = 'live', asked_at = excluded.asked_at,
                        due_at = excluded.due_at, waits = excluded.waits, payload = excluded.payload, served_at = NULL`)
            .bind(producer, ref, id, at, due, waits ? 1 : 0, JSON.stringify(payload)));
      });
      await db.batch(writes);
    }
    const { results } = await db.prepare("SELECT * FROM questions WHERE id IN (SELECT value FROM json_each(?)) AND state IN ('fact', 'spot')")
      .bind(JSON.stringify([...new Set(ids)])).all();
    for (const q of results) {
      // A FACT OF ANOTHER ENGINE answers nothing now: the question opens again,
      // its answers carried for the served engine to reproduce or replace.
      if (q.fact_engine !== engine) {
        await db.prepare("UPDATE questions SET state = 'open', opened_at = ? WHERE id = ? AND state = ?").bind(at, q.id, q.state).run();
      } else {
        await deliver(env, q, now);
      }
    }
    return ids;
  }

  /// A PRODUCER NO LONGER ASKS. A question nobody asks for any more is withdrawn,
  /// and one still asked for keeps the earliest deadline still standing.
  async function withdraw(env, producer, ref, now = Date.now()) {
    const db = env.LIBRARY;
    const d = await db.prepare("SELECT question FROM demands WHERE producer = ? AND ref = ?").bind(producer, ref).first();
    if (!d) return;
    await db.prepare("UPDATE demands SET state = 'withdrawn' WHERE producer = ? AND ref = ? AND state = 'live'").bind(producer, ref).run();
    await restate(db, d.question, now);
  }

  async function restate(db, id, now) {
    const left = await db.prepare("SELECT min(due_at) AS due, max(waits) AS waits, count(*) AS n FROM demands WHERE question = ? AND state = 'live'")
      .bind(id).first();
    if (left.n) {
      await db.prepare("UPDATE questions SET due_at = ?, waits = ? WHERE id = ?").bind(left.due, left.waits, id).run();
    } else {
      await db.prepare("UPDATE questions SET state = 'withdrawn', lease = NULL, lease_until = NULL, leased_to = NULL WHERE id = ? AND state = 'open'")
        .bind(id).run();
    }
  }

  /// THE QUESTION THIS WITNESS ANSWERS NEXT, leased, or null — §"Order" among
  /// exactly the questions it may answer. `w`: { device, owner,
  /// siblings, net, lanes, official, most, before }: `lanes` 0 is a witness with
  /// no cores free now (asking ahead), which a question someone waits on is never
  /// handed; `before` keeps to questions due before then.
  async function next(env, w, engine, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), most = w.most ?? 1;
    for (let tries = 0; tries < 8; tries++) {
      const q = await db.prepare(eligibleSql(w.official)).bind(...eligibleArgs(w, engine, at)).first();
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
        await restate(db, q.id, now);
        continue;
      }
      const lease = newLease();
      // THE RACE IS THE DATABASE'S, and so is the cap on what one witness holds.
      const took = await db.prepare(
        `UPDATE questions SET lease = ?, lease_until = ?, leased_to = ?
          WHERE id = ? AND state = ? AND (lease_until IS NULL OR lease_until < ?)
            AND (SELECT count(*) FROM questions h WHERE h.leased_to = ? AND h.lease_until > ?) < ?`)
        .bind(lease, iso(now + verbs[q.verb].lease_ms), w.device, q.id, q.state, at, w.device, at, most).run();
      if (took.meta.changes) {
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

  /// A LEASE KEPT while its witness is still at it, or given back.
  async function renew(env, device, lease, now = Date.now()) {
    const q = await env.LIBRARY.prepare("SELECT verb FROM questions WHERE lease = ? AND leased_to = ? AND lease_until >= ?")
      .bind(lease, device, iso(now)).first();
    if (!q) return false;
    await env.LIBRARY.prepare("UPDATE questions SET lease_until = ? WHERE lease = ? AND leased_to = ?")
      .bind(iso(now + verbs[q.verb].lease_ms), lease, device).run();
    return true;
  }
  async function release(env, device, lease) {
    await env.LIBRARY.prepare("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE lease = ? AND leased_to = ?")
      .bind(lease, device).run();
  }
  /// EVERYTHING A WITNESS HOLDS goes back: one that asks for work again is
  /// working on nothing (worker/verify.js, asking ahead).
  async function releaseAll(env, device, now = Date.now()) {
    await env.LIBRARY.prepare("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE leased_to = ? AND lease_until > ?")
      .bind(device, iso(now)).run();
  }

  /// A WITNESS ANSWERS, under its lease. `w`: { device, owner, net, official }.
  /// `counts`, `lanes` and `compute_ms` are what the answer cost and on how many
  /// cores — kept beside it to fit what a point is worth, never credited.
  /// Returns `{ taken }`: whether the lease was still its own.
  async function answer(env, w, { lease, ...a }, now = Date.now()) {
    const q = await env.LIBRARY.prepare("SELECT * FROM questions WHERE lease = ? AND leased_to = ? AND lease_until >= ?")
      .bind(lease, w.device, iso(now)).first();
    if (!q) return { taken: false };
    await record(env, q, w, a, now);
    return { taken: true };
  }

  /// THE OFFICIAL MACHINES ANSWER WITHOUT A LEASE: they are trusted, and what they
  /// measured is the fact whoever held the question. Returns the question's
  /// state after it, or null for a question that does not exist.
  async function answerOfficial(env, w, { question, ...a }, now = Date.now()) {
    const q = await env.LIBRARY.prepare("SELECT * FROM questions WHERE id = ?").bind(question).first();
    if (!q) return null;
    await record(env, q, { ...w, official: true }, a, now);
    return (await env.LIBRARY.prepare("SELECT state FROM questions WHERE id = ?").bind(question).first()).state;
  }

  async function record(env, q, w, { engine, result, work, compute_ms = null, counts = null, lanes = null }, now) {
    const db = env.LIBRARY, at = iso(now);
    const c = verbs[q.verb].canon(result, work);
    const official = w.official ? 1 : 0;
    const writes = [
      db.prepare(`INSERT INTO answers (question, engine, device, owner, net, official, canon, result, work, compute_ms, counts, lanes, at)
                  VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
        .bind(q.id, engine, w.device, w.owner || w.device, w.net || "", official, c, JSON.stringify(result), work, compute_ms,
          counts ? JSON.stringify(counts) : null, lanes, at),
    ];
    if (!official) {
      // …AND WHEN IT LAST ANSWERED and what it spent, in its hour (docs/BOARD.md §"Contribution").
      writes.push(
        db.prepare("UPDATE verifiers SET compute_ms = compute_ms + ?, last_at = ? WHERE id = ?").bind(compute_ms || 0, at, w.device),
        db.prepare(`INSERT INTO verifier_hours (verifier, hour, tasks, ms) VALUES (?, ?, 1, ?)
                    ON CONFLICT (verifier, hour) DO UPDATE SET tasks = tasks + 1, ms = ms + excluded.ms`)
          .bind(w.device, at.slice(0, 13), compute_ms || 0));
    }
    await db.batch(writes);
    for (const d of await liveDemands(db, q.id)) {
      const p = producerOf(d.producer);
      if (p.on_answer) {
        await p.on_answer(env, { ...d, payload: JSON.parse(d.payload) },
          { question: q.id, device: w.device, net: w.net || "", engine, result, work, official: !!official, at });
      }
    }
    await settle(env, q.id, engine, now);
    // THE LEASE GOES ONLY ONCE THE QUESTION IS SETTLED: the answered question
    // heads every witness's order, so freed before it became a fact it was
    // taken at once and answered again for nothing.
    await db.prepare("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE id = ? AND leased_to = ?").bind(q.id, w.device).run();
  }

  /// WHAT THE ANSWERS NOW MAKE OF A QUESTION (§"States").
  async function settle(env, id, engine, now) {
    const db = env.LIBRARY;
    const q = await db.prepare("SELECT * FROM questions WHERE id = ?").bind(id).first();
    const all = (await db.prepare("SELECT * FROM answers WHERE question = ? AND void = 0 ORDER BY id").bind(id).all()).results;
    const served = all.filter((a) => a.engine === engine);
    const officialSays = served.filter((a) => a.official).at(-1);

    if (officialSays) {
      // THE OFFICIAL MACHINES' ANSWER IS THE FACT, whatever state it was in. The
      // volunteers who gave its bits are credited; those it disproves on its own
      // engine are refused — unless the question was not deterministic.
      // BITS ARE THE WITNESS: a volunteer of any engine that gave their bits
      // agreed; a refusal needs the same engine, since another may differ honestly.
      const agree = independent(all.filter((a) => !a.official && a.canon === officialSays.canon));
      const wrong = served.filter((a) => !a.official && a.canon !== officialSays.canon);
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
      if (!odd && (q.state === "disputed" || q.state === "spot")) {
        for (const a of wrong) await (on_disproved ? on_disproved(env, a) : refuse(env, a.device, now));
      }
      if (q.state !== "spot" || moved) await deliver(env, { ...q, canon: officialSays.canon }, now, officialSays);
      return;
    }
    if (q.state !== "open") return;

    // A GROUP OF EQUAL ANSWERS holds one of the served engine; an older engine's
    // answer joins one (a release that did not move the bits) and never starts one.
    for (const lead of served) {
      // The served engine's answers are taken first, so the witnesses it
      // reproduced are credited beside it rather than instead of it.
      const equal = all.filter((a) => a.canon === lead.canon);
      const group = independent([...equal.filter((a) => a.engine === engine), ...equal.filter((a) => a.engine !== engine)]);
      if (group.length < threshold) continue;
      await credit(db, group, now);
      const state = random() < (verbs[q.verb].spot ?? spot) ? "spot" : "fact";
      await db.prepare("UPDATE questions SET state = ?, canon = ?, fact_engine = ?, fact_at = ? WHERE id = ?")
        .bind(state, lead.canon, engine, iso(now), id).run();
      await deliver(env, { ...q, canon: lead.canon }, now, lead);
      return;
    }
    const cap = verbs[q.verb].cap;
    if (served.length >= cap) {
      const distinct = new Set(served.map((a) => a.canon)).size === served.length;
      await db.prepare("UPDATE questions SET state = ? WHERE id = ? AND state = 'open'")
        .bind(distinct && served.length >= 3 ? "nondeterministic" : "disputed", id).run();
    }
  }

  /// THE ANSWERS THAT COUNT AS WITNESSES, in the order they came: no owner and no
  /// network twice. Two browsers on one desk are one witness.
  function independent(answers) {
    const owners = new Set(), nets = new Set(), out = [];
    for (const a of answers) {
      if (owners.has(a.owner) || (a.net && nets.has(a.net))) continue;
      owners.add(a.owner);
      if (a.net) nets.add(a.net);
      out.push(a);
    }
    return out;
  }

  /// ONLY A FACT SCORES, once per answer: its work to its device, in its hour,
  /// and one fact toward forgiving a refusal (docs/BOARD.md §"Contribution").
  async function credit(db, answers, now) {
    const due = answers.filter((a) => !a.official && !a.credited);
    if (!due.length) return;
    const hour = iso(now).slice(0, 13);
    await db.batch(due.flatMap((a) => [
      db.prepare("UPDATE answers SET credited = 1 WHERE id = ?").bind(a.id),
      db.prepare("UPDATE verifiers SET work = work + ?, agreed = agreed + 1 WHERE id = ?").bind(a.work, a.device),
      db.prepare(`UPDATE verifiers SET
          refusals = CASE WHEN refusals > 0 AND clean + 1 >= refusals * ? THEN refusals - 1 ELSE refusals END,
          clean = CASE WHEN refusals = 0 OR clean + 1 >= refusals * ? THEN 0 ELSE clean + 1 END WHERE id = ?`)
        .bind(FACTS_PER_REFUSAL_FORGIVEN, FACTS_PER_REFUSAL_FORGIVEN, a.device),
      db.prepare(`INSERT INTO verifier_hours (verifier, hour, work) VALUES (?, ?, ?)
                  ON CONFLICT (verifier, hour) DO UPDATE SET work = work + excluded.work`).bind(a.device, hour, a.work),
    ]));
  }

  /// EVERY LIVE DEMAND IS TOLD THE FACT, once.
  async function deliver(env, q, now, by = null) {
    const db = env.LIBRARY;
    const answerOf = by || await db.prepare("SELECT * FROM answers WHERE question = ? AND canon = ? ORDER BY official DESC, id LIMIT 1")
      .bind(q.id, q.canon).first();
    const fact = { question: q.id, verb: q.verb, request: JSON.parse(q.request), canon: q.canon,
      result: answerOf ? JSON.parse(answerOf.result) : null, work: answerOf ? answerOf.work : 0,
      engine: answerOf ? answerOf.engine : null, official: !!(answerOf && answerOf.official) };
    for (const d of await liveDemands(db, q.id)) {
      await producerOf(d.producer).on_fact(env, { ...d, payload: JSON.parse(d.payload) }, fact);
      await db.prepare("UPDATE demands SET state = 'served', served_at = ? WHERE producer = ? AND ref = ?").bind(iso(now), d.producer, d.ref).run();
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
    const { results } = await db.prepare(
      `UPDATE questions SET lease = lower(hex(randomblob(16))), lease_until = ?1, leased_to = ?2
        WHERE id IN (SELECT q.id FROM questions q
          WHERE ((?3 = 1 AND q.state = 'open' AND q.due_at < ?4) OR q.state IN (SELECT value FROM json_each(?5)))
            AND (q.lease_until IS NULL OR q.lease_until < ?4)
            AND (?6 IS NULL OR EXISTS (SELECT 1 FROM demands d WHERE d.question = q.id
                                       AND d.producer IN (SELECT value FROM json_each(?6))))
            AND NOT EXISTS (SELECT 1 FROM answers a WHERE a.question = q.id AND a.device = ?2 AND a.engine = ?7)
          ORDER BY q.waits DESC, q.due_at, q.id LIMIT ?8)
        RETURNING id, verb, request, state, lease`)
      .bind(iso(now + lease_ms), w.device, open, at, JSON.stringify(theirs), producers ? JSON.stringify(producers) : null, engine, max).all();
    if (!results.length) return [];
    const { results: ds } = await db.prepare(
      `SELECT question, producer, ref, payload FROM demands WHERE state != 'withdrawn' AND question IN (SELECT value FROM json_each(?))
        ORDER BY state = 'live' DESC`)
      .bind(JSON.stringify(results.map((q) => q.id))).all();
    return results.map((q) => ({ question: q.id, verb: q.verb, state: q.state, lease: q.lease, request: JSON.parse(q.request),
      demands: ds.filter((d) => d.question === q.id).map((d) => ({ producer: d.producer, ref: d.ref, payload: JSON.parse(d.payload) })) }));
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
      db.prepare("UPDATE verifiers SET work = max(0, work - ?) WHERE id = ?").bind(paid, device),
      db.prepare(`INSERT INTO verifier_hours (verifier, hour, work) VALUES (?, ?, ?)
                  ON CONFLICT (verifier, hour) DO UPDATE SET work = work + excluded.work`).bind(device, at.slice(0, 13), -paid),
      db.prepare("UPDATE answers SET void = 1, credited = 0 WHERE device = ? AND official = 0").bind(device),
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
  /// deterministic, and disputes nobody settled. Empty when the factory is healthy.
  async function audit(env, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), recent = iso(now - 30 * 60_000);
    const active = (await db.prepare("SELECT count(*) AS n FROM verifiers WHERE last_at > ?").bind(recent).first()).n;
    // A PRODUCER WHOSE OVERDUE QUESTIONS GOT NO ANSWER AT ALL in half an hour
    // while witnesses were at work: something hands them out to nobody. One
    // overdue question waiting its turn is a queue; a whole kind silent is a stall.
    const { results: stalled } = await db.prepare(
      `SELECT d.producer, count(DISTINCT d.question) AS questions, min(d.due_at) AS oldest_due
         FROM demands d JOIN questions q ON q.id = d.question
        WHERE d.state = 'live' AND q.state = 'open' AND d.due_at < ?
        GROUP BY d.producer
       HAVING NOT EXISTS (SELECT 1 FROM demands e JOIN answers a ON a.question = e.question
                           WHERE e.producer = d.producer AND a.at > ?)`).bind(at, recent).all();
    const odd = (await db.prepare("SELECT count(*) AS n FROM questions WHERE state = 'nondeterministic' OR (odd = 1 AND fact_at > ?)")
      .bind(iso(now - 24 * 3_600_000)).first()).n;
    // WAITING ON THE OFFICIAL MACHINES for over an hour: a dispute nobody settles
    // is the one place a question can still wait for ever.
    const unsettled = (await db.prepare(
      `SELECT count(*) AS n FROM questions WHERE state IN (${OFFICIAL_STATES.map(() => "?").join(", ")}) AND opened_at < ?`)
      .bind(...OFFICIAL_STATES, iso(now - 3_600_000)).first()).n;
    const out = [];
    if (active) for (const p of stalled) out.push({ kind: `stalled:${p.producer}`, ...p, witnesses_active: active });
    if (odd) out.push({ kind: "nondeterministic", questions: odd });
    if (unsettled) out.push({ kind: "unsettled", questions: unsettled });
    return out;
  }

  return { demand, demandAll, withdraw, next, renew, release, releaseAll, answer, answerOfficial, claim, refuse, settle, audit };
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
/// deadline and what only they may settle.
function eligibleSql(official) {
  return `SELECT q.id, q.verb, q.request, q.state, q.due_at FROM questions q
    WHERE ${official ? `((q.state = 'open' AND q.due_at < ?1) OR q.state IN (${OFFICIAL_STATES.map((s) => `'${s}'`).join(", ")}))`
      : "q.state = 'open'"}
      AND (q.lease_until IS NULL OR q.lease_until < ?1)
      AND (?2 = 1 OR q.lanes IS NULL OR EXISTS (SELECT 1 FROM answers x WHERE x.question = q.id)
           OR ?3 >= (SELECT min(json_extract(t.value, '$.lanes')) FROM json_each(q.lanes) t
                     WHERE json_extract(t.value, '$.after_ms') <= (julianday(?1) - julianday(q.opened_at)) * 86400000.0))
      AND NOT EXISTS (SELECT 1 FROM answers a WHERE a.question = q.id AND a.engine = ?4
            AND (a.device IN (SELECT value FROM json_each(?5)) OR a.owner = ?6 OR (?7 != '' AND a.net = ?7)))
      AND (?8 IS NULL OR q.due_at < ?8)
    ORDER BY q.waits DESC, q.due_at, q.id LIMIT 1`;
}
function eligibleArgs(w, engine, at) {
  return [at, w.official ? 1 : 0, w.lanes ?? 1, engine, JSON.stringify(w.siblings || [w.device]), w.owner || w.device, w.net || "", w.before ?? null];
}
