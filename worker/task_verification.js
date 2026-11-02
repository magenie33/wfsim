// SPDX-License-Identifier: AGPL-3.0-or-later
// Answer recording and independent-witness verification.
import { iso } from "./instant.js";
import { dialsOf } from "./dials.js";
import { GRACE_SHARE, sansWork, sameAs, newLease, independent, CLAIM_STALE_MS, inIdOrder, coolDownMs, KEPT_MS } from "./task_contract.js";

export function createVerification({ verbs, producers, producerOf, threshold, spot, recheck, random, on_disproved, refusing, learnCost, prepareCredit, credit, deliver, deliverPlan, flush, commit, readyFact, release } = {}) {
  const listOf = (db, p) => `(SELECT value FROM jsonb_array_elements_text((${p}::text)::jsonb))`;
  /// A WITNESS ANSWERS, under its lease. `w`: { device, owner, net, official }.
  /// `counts`, `lanes` and `compute_ms` are what the answer cost and on how many
  /// cores — kept beside it to fit what a point is worth, never credited.
  /// Returns `{ taken }`: whether the lease was still its own.
  async function answer(env, w, { lease, ...a }, now = Date.now()) {
    const q = await env.LIBRARY.prepare("SELECT * FROM questions WHERE lease = ? AND leased_to = ? AND lease_until >= ?")
      .bind(lease, w.device, iso(now)).first();
    if (!q) return { taken: false };
    await recordMany(env, w, [{ q, a }], now);
    return { taken: true };
  }


  /// A PACK'S ANSWERS, together: those whose question the pack still holds are
  /// recorded and settled in a few round trips, whatever their number; `done`
  /// gives back the rest of the pack but `keep`. Returns the questions taken.
  /// AN OLDER ENGINE'S ANSWERS (`served` the engine the site serves) settle
  /// under the served one: its rows were handed out because their bits are the same
  /// — or, `join`, because they may be, and then each only joins a served one's.
  /// `late` (protocol 8): a row whose lease ran out still takes its answer while
  /// nobody took it since — the page keeps results and sends them later.
  async function answerPack(env, w, { lease, engine, served = null, join = false, answers, done = false, keep = null, late = false }, now = Date.now()) {
    const db = env.LIBRARY;
    const { results: held } = await db.prepare("SELECT * FROM factory_held(?1::text, ?2::text, ?4::text, ?3::text, ?5::bigint)")
      .bind(lease, w.device, iso(now), JSON.stringify(answers.map((a) => a.question)), late ? 1 : 0).all();
    const byId = new Map(held.map((q) => [q.id, q]));
    // A PAGE SENDS AGAIN what got no reply, and the reply may be all that was lost:
    // an answer this device already gave on this engine is not a second witness
    // (the ask never hands it the same question twice, `nextPack`).
    // An answer already recorded is `already` whether or not the row is still held:
    // settling it gave the lease back, and the reply that said so was lost.
    const already = [];
    if (answers.length) {
      const { results: given } = await db.prepare("SELECT g AS question FROM factory_given(?1::text, ?2::text, ?3::text) g")
        .bind(w.device, engine, JSON.stringify(answers.map((a) => a.question))).all();
      for (const r of given) { byId.delete(r.question); already.push(r.question); }
    }
    const items = answers.filter((a) => byId.has(a.question)).map(({ question, ...a }) => ({ q: byId.get(question), a: { ...a, engine } }));
    if (items.length) await recordMany(env, w, items, now, served, join);
    if (done) await release(env, w.device, lease, keep);
    return { taken: items.map((x) => x.q.id), already };
  }


  /// THE OFFICIAL MACHINES ANSWER WITHOUT A LEASE: they are trusted, and what they
  /// measured is the fact whoever held the question. Returns the question's
  /// state after it, or null for a question that does not exist.
  async function answerOfficial(env, w, { question, ...a }, now = Date.now()) {
    const db = env.LIBRARY;
    const q = await db.prepare("SELECT * FROM questions WHERE id = ?").bind(question).first();
    if (!q) return null;
    await recordMany(env, { ...w, official: true }, [{ q, a }], now);
    return (await db.prepare("SELECT state FROM questions WHERE id = ?").bind(question).first()).state;
  }


  /// ONE WITNESS'S ANSWERS `[{ q, a }]`, all of one engine: written in one round
  /// trip, then settled together (`settleMany`).
  async function recordMany(env, w, items, now, served = null, join = false) {
    const db = env.LIBRARY, at = iso(now), official = w.official ? 1 : 0, mine = items[0].a.engine, engine = served || mine;
    // ONE DEVICE'S SENDS ARE TAKEN ONE AT A TIME, and an answer it already gave on
    // this engine is not written again: a beacon on the way out raced a sync still
    // in flight, both checked before either wrote, and both rows landed.
    const lock = db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?1::text, 0))").bind(w.device);
    const writes = items.map(({ q, a }) => db.prepare(`SELECT factory_answer(?1::text, ?2::text, ?3::text, ?4::text, ?5::text, ?6::bigint, ?7::text,
        ?8::text, ?9::bigint, ?10::bigint, ?11::text, ?12::bigint, ?13::text)`)
      .bind(q.id, a.engine, w.device, w.owner || w.device, w.net || "", official, verbs[q.verb].canon(a.result, a.work), JSON.stringify(a.result),
        a.work ?? 0, a.compute_ms ?? null, a.counts ? JSON.stringify(a.counts) : null, a.lanes ?? null, at));
    const learnt = official ? null : learnCost(db, items);
    // QUEUED FOR THE SETTLER (dial `settle_async`): a volunteer's answers to a
    // packable verb whose producer is not told each answer (`on_answer` runs in
    // the request), in the answers' own transaction, so one written is one queued.
    // An answer on a row a settler pass holds clears its claim (`settleQueued`).
    const peer = mine !== engine && !join ? mine : null;
    const queued = !official && items.every(({ q }) => queueable(q)) && (await dialsOf(env, now)).settle_async === 1;
    const queue = queued ? db.prepare("SELECT factory_queue(?1::text, ?2::text, ?3::text, ?4::bigint, ?5::text)")
      .bind(JSON.stringify(items.map(({ q }) => q.id)), engine, peer || "", join ? 1 : 0, at) : null;
    // PRODUCER_USE LAST, as `board_sync` takes it: a batch holding it while it
    // waits on a queue row the procedure holds deadlocks against it.
    await db.batch([lock, ...writes, ...(queue ? [queue] : []), ...(learnt ? [learnt] : [])]);
    if (!official) {
      // …AND WHEN IT LAST ANSWERED and what it spent, in its hour (docs/BOARD.md
      // §"Contribution") — a statistic, so outside the answers' transaction:
      // inside it, it held the device's row against every credit.
      const ms = items.reduce((t, { a }) => t + (a.compute_ms || 0), 0);
      await db.prepare("SELECT verifier_worked(?1::text, ?2::bigint, ?3::bigint, ?4::text)").bind(w.device, ms, items.length, at).run();
    }
    // …AND SETTLED THERE, the lease given back with it. A handle that asks for it
    // (a check's database, scripts/pg/test_db.mjs) has the pass run at once.
    if (queued) {
      if (db.settleAtOnce) await settleQueued(env, now);
      return;
    }
    const answered = new Map(items.map(({ q, a }) => [q.id,
      { question: q.id, device: w.device, net: w.net || "", engine: a.engine, result: a.result, work: a.work, official: !!official, at }]));
    const ids = JSON.stringify([...answered.keys()]);
    // THE LEASE GOES ONLY ONCE THE QUESTION IS SETTLED, in the settling's own
    // write: the answered question heads every witness's order, so freed before
    // it became a fact it was taken at once and answered again for nothing.
    await settleMany(env, [...answered.keys()], engine, now, { answered, peer, after: [
      db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
                   WHERE ${inIdOrder(`id IN ${listOf(db, "?")} AND leased_to = ?`)}`).bind(ids, w.device)] });
  }


  /// THE SETTLER'S PASS (worker/settler.js `settleBatch`): up to `max` queued
  /// questions claimed, each settled exactly as an answer in the request settles
  /// it (`settleMany`), the lease of a holder that answered given back, and the
  /// rows taken off — but a row an answer re-armed meanwhile, which waits for
  /// the next pass. A pass that dies is claimed again after `CLAIM_STALE_MS`;
  /// every write in a settlement is guarded, so settling twice counts once.
  async function settleQueued(env, now = Date.now(), max = null) {
    const db = env.LIBRARY, claim = newLease(), n = max ?? (await dialsOf(env, now)).settle_batch;
    const { results: rows } = await db.prepare(`UPDATE settle_queue SET claim = ?1::text, claimed_at = ?2::text
        WHERE (question, engine, peer) IN (SELECT question, engine, peer FROM settle_queue
          WHERE claim IS NULL OR claimed_at < ?3::text ORDER BY question, engine, peer LIMIT ?4::bigint FOR UPDATE SKIP LOCKED)
        RETURNING question, engine, peer`).bind(claim, iso(now), iso(now - CLAIM_STALE_MS), n).all();
    if (!rows.length) return 0;
    const groups = new Map();
    for (const r of rows) {
      const k = `${r.engine} ${r.peer}`;
      if (!groups.has(k)) groups.set(k, { engine: r.engine, peer: r.peer || null, ids: [] });
      groups.get(k).ids.push(r.question);
    }
    for (const g of [...groups.values()].sort((a, b) => (a.engine + a.peer < b.engine + b.peer ? -1 : 1))) {
      const ids = [...new Set(g.ids)].sort(), list = JSON.stringify(ids);
      await settleMany(env, ids, g.engine, now, { peer: g.peer, after: [
        db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE ${inIdOrder(`id IN ${listOf(db, "?")}
            AND leased_to IS NOT NULL AND EXISTS (SELECT 1 FROM answers a WHERE a.question = questions.id AND a.device = questions.leased_to)`)}`)
          .bind(list)] });
    }
    await db.prepare("DELETE FROM settle_queue WHERE claim = ?").bind(claim).run();
    return rows.length;
  }


  /// WHAT THE ANSWERS NOW MAKE OF A QUESTION (§"States").
  async function settle(env, id, engine, now) {
    await settleMany(env, [id], engine, now);
  }


  /// …OF MANY QUESTIONS AT ONCE: read in one round trip, decided here, written in
  /// one more. Every write is guarded — a state moves only from `open`, an answer
  /// is credited only once — so two witnesses settling one question at the same
  /// moment cannot both count. `answered` tells each question's producers of the
  /// answer just recorded (`on_answer`); `after` is written with the rest.
  async function settleMany(env, ids, engine, now, { answered = null, after = [], peer = null } = {}) {
    const db = env.LIBRARY, list = JSON.stringify([...new Set(ids)]), ids_ = listOf(db, "?");
    const [qs, as, ds] = await db.batch([
      db.prepare(`SELECT * FROM questions WHERE id IN ${ids_}`).bind(list),
      db.prepare(`SELECT * FROM answers WHERE question IN ${ids_} AND void = 0 ORDER BY id`).bind(list),
      db.prepare(`SELECT * FROM demands WHERE question IN ${ids_} AND state = 'live'`).bind(list),
    ]);
    const writes = [], later = [], credits = [], errors = [];
    for (const q of [...qs.results].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))) {
      const demands = ds.results.filter((d) => d.question === q.id);
      const told = answered && answered.get(q.id);
      if (told) {
        for (const d of demands) {
          const p = producerOf(d.producer);
          if (p.on_answer) {
            try { await p.on_answer(env, { ...d, payload: JSON.parse(d.payload) }, told); }
            catch (error) { errors.push(error); }
          }
        }
      }
      const all = as.results.filter((a) => a.question === q.id);
      const served = all.filter((a) => a.engine === engine);
      const officialSays = served.filter((a) => a.official).at(-1);
      if (officialSays) {
        try { await settleOfficial(env, q, all, served, officialSays, engine, now); }
        catch (error) { errors.push(error); }
      }
      else if (q.state === "open") decide(env, q, all, peer ? all.filter((a) => a.engine === engine || a.engine === peer) : served, demands, engine, now, writes, later, credits);
    }
    // CREDITS AFTER THE COMMIT (`credit`): two settlements crediting the same
    // devices in different orders deadlock, so no settlement holds a
    // device's row.
    writes.push(...after);
    if (credits.length) writes.push(prepareCredit(db, credits, now));
    // …ITS QUESTIONS LOCKED FIRST, IN ID ORDER (`inIdOrder`): each write after
    // finds its row already held, so the commit never waits holding one.
    if (writes.length) writes.unshift(db.prepare(`SELECT id FROM questions WHERE ${inIdOrder(`id IN ${ids_}`)}`).bind(list));
    await commit(env, writes);
    try { await credit(db, credits, now); } catch (error) { errors.push(error); }
    try { await flush(env, later, now); } catch (error) { errors.push(error); }
    if (errors.length) throw errors[0];
  }


  /// AN OPEN QUESTION'S VOLUNTEER ANSWERS, judged — a fact, the cap reached, or
  /// still open — its writes added to `writes`.
  function decide(env, q, all, served, demands, engine, now, writes, later, credits) {
    const db = env.LIBRARY, needs = q.needs || threshold;
    // A GROUP OF EQUAL ANSWERS holds one of the served engine; an older engine's
    // answer joins one (a release that did not move the bits) and never starts one
    // — unless it is still accepted on this row (`peer`), when it leads as one.
    for (const lead of served) {
      // The served engine's answers are taken first, so the witnesses it
      // reproduced are credited beside it rather than instead of it.
      // ANOTHER ENGINE AGREES BY ITS RESULT ALONE: `work` counts the engine's own
      // instructions, so a release that moved its code moved every `work`, and
      // compared whole, a `join` answer never agrees with a served one.
      const same = sameAs(lead);
      const equal = all.filter(same);
      const group = independent([...equal.filter((a) => a.engine === engine), ...equal.filter((a) => a.engine !== engine)]);
      if (group.length < needs) continue;
      // A RECHECK: one more independent volunteer before it is a fact, nothing
      // credited meanwhile. One who differs makes it a dispute, which the
      // official machines settle and refuse the liar in — so a recheck catches
      // a pair agreeing on a lie whenever the third is honest.
      if (!q.needs && group.length === threshold && random() < (verbs[q.verb].recheck ?? recheck)) {
        writes.push(db.prepare("UPDATE questions SET needs = ? WHERE id = ? AND state = 'open' AND needs IS NULL").bind(threshold + 1, q.id));
        return;
      }
      const state = random() < (verbs[q.verb].spot ?? spot) ? "spot" : "fact";
      // A FACT IS NO OLDER THAN ITS LAST WITNESS: an answer stamped by its request
      // can land after a settler pass took its time, and every reader of a fact
      // counts only the answers at or before `fact_at` (`repay`, the shadow).
      const formed = [iso(now), ...group.map((a) => a.at)].sort().pop();
      writes.push(db.prepare("UPDATE questions SET state = ?, canon = ?, fact_engine = ?, fact_at = ? WHERE id = ? AND state = 'open'")
        .bind(state, lead.canon, engine, formed, q.id));
      writes.push(readyFact(db, q.id, lead.canon, engine, formed));
      credits.push(...group, ...joinersParted(all, served, group, same, engine));
      deliverPlan(env, { ...q, canon: lead.canon, fact_at: formed, fact_engine: engine }, now, lead, demands, writes, later);
      return;
    }
    // ONLY WITNESSES FILL THE CAP: a browser claimed by an owner after it took
    // the question answers as that owner's second, which disagrees with nobody.
    const cap = Math.max(verbs[q.verb].cap, needs), witnesses = independent(served);
    if (witnesses.length >= cap) {
      const distinct = new Set(witnesses.map((a) => a.canon)).size === witnesses.length;
      writes.push(db.prepare("UPDATE questions SET state = ? WHERE id = ? AND state = 'open'")
        .bind(distinct && witnesses.length >= 3 ? "nondeterministic" : "disputed", q.id));
    }
  }


  /// AN OLDER ENGINE'S ANSWERS THAT PARTED FROM THE FACT, paid `GRACE_SHARE`
  /// of their work: computed in good faith on the engine its owner had, and
  /// less, so an engine is worth updating. Only where two independent answers
  /// of that engine agree with each other — the engine's own bits, which one
  /// alone cannot tell from a made-up number.
  function joinersParted(all, served, group, same, engine) {
    const parted = all.filter((a) => a.engine !== engine && !a.official && !served.includes(a) && !group.includes(a) && !same(a));
    const out = [];
    for (const key of new Set(parted.map((a) => `${a.engine}|${a.canon}`))) {
      const agreeing = independent(parted.filter((a) => `${a.engine}|${a.canon}` === key));
      if (agreeing.length >= 2) out.push(...agreeing.map((a) => ({ ...a, work: Math.floor((a.work || 0) * GRACE_SHARE) })));
    }
    return out;
  }


  async function settleOfficial(env, q, all, served, officialSays, engine, now) {
    const db = env.LIBRARY, id = q.id;
    // AN OFFICIAL ANSWER WITHOUT WORK — a native build's, which counts no gas —
    // witnesses the score alone: it is the fact, and a volunteer who gave the
    // score is owed a check of the work by the witness that counts it (the
    // wasm, scripts/live_orders.mjs), so the question is spot-checked.
    if (JSON.parse(officialSays.canon).work === null) {
      const owed = all.some((a) => !a.official && sansWork(a.canon) === sansWork(officialSays.canon));
      await db.batch([
        db.prepare("UPDATE questions SET state = ?, canon = ?, fact_engine = ?, fact_at = ?, odd = 0 WHERE id = ?")
          .bind(owed ? "spot" : "fact", officialSays.canon, engine, iso(now), id),
        readyFact(db, id, officialSays.canon, engine, iso(now)),
      ]);
      if (q.state !== "spot") await deliver(env, { ...q, canon: officialSays.canon, fact_at: iso(now), fact_engine: engine }, now, officialSays);
      return;
    }
    // THE OFFICIAL MACHINES' ANSWER IS THE FACT, whatever state it was in. The
    // volunteers who gave its bits are credited; those it disproves on its own
    // engine are refused — unless the question was not deterministic.
    // BITS ARE THE WITNESS: a volunteer of any engine that gave their bits
    // agreed; a refusal needs the same engine, since another may differ honestly.
    // THE SCORE IS THE WITNESS AND THE OFFICIAL WORK IS THE PAY: a volunteer
    // who gave the official score agreed, whatever work it counted, and on the
    // official engine is paid the official work - an engine counting other work
    // is a client to fix, not a liar, and over-counting earns nothing. Another
    // engine prices work in its own units, so there it keeps its own.
    const gave = (a) => sansWork(a.canon) === sansWork(officialSays.canon);
    const officialWork = JSON.parse(officialSays.canon).work;
    const agree = independent(all.filter((a) => !a.official && gave(a)))
      .map((a) => (a.engine === engine && Number.isSafeInteger(officialWork) ? { ...a, work: officialWork } : a));
    const near = verbs[q.verb].near || (() => false);
    const wrong = served.filter((a) => !a.official && !gave(a) && !near(a.canon, officialSays.canon));
    const moved = q.canon !== null && q.canon !== officialSays.canon;
    // A DISPUTE THE OFFICIAL MACHINES SIDE WITH NOBODY IN is not a liar found:
    // every answer differs from every other, theirs too, and that is an engine
    // whose answer is not deterministic. Nobody is refused; the watchdog says so.
    const odd = q.state === "nondeterministic" || (q.state === "disputed" && !agree.length);
    await db.batch([
      db.prepare("UPDATE questions SET state = 'fact', canon = ?, fact_engine = ?, fact_at = ?, odd = ? WHERE id = ?")
        .bind(officialSays.canon, engine, iso(now), odd ? 1 : 0, id),
      readyFact(db, id, officialSays.canon, engine, iso(now)),
      prepareCredit(db, agree, now),
    ]);
    const errors = [];
    try { await credit(db, agree, now); } catch (error) { errors.push(error); }
    // ONLY A DISPUTE OR A SPOT CHECK REFUSES: an open question past its deadline
    // that they answered differently is measured again, its answer unpaid —
    // one witness against them is not yet a witness found lying.
    if (refusing && !odd && (q.state === "disputed" || q.state === "spot")) {
      for (const a of wrong) {
        try { await (on_disproved ? on_disproved(env, a) : refuse(env, a.device, now)); }
        catch (error) { errors.push(error); }
      }
    }
    if (q.state !== "spot" || moved) {
      try { await deliver(env, { ...q, canon: officialSays.canon, fact_at: iso(now), fact_engine: engine }, now, officialSays); }
      catch (error) { errors.push(error); }
    }
    if (errors.length) throw errors[0];
  }


  /// A WITNESS THE OFFICIAL MACHINES DISPROVED, refused — for a cool-down, never
  /// for good (`coolDownMs`). Its answers stop counting; what it was credited is
  /// taken back, in its hour; a fact it witnessed that the official machines
  /// never checked is opened again, and its producers told it no longer stands
  /// (`on_retract`). All of it within `KEPT_MS`: an answer older has been
  /// forgotten, and what it earned and witnessed stands.
  async function refuse(env, device, now = Date.now()) {
    const db = env.LIBRARY, at = iso(now), since = iso(now - KEPT_MS);
    const v = await db.prepare("SELECT refusals FROM verifiers WHERE id = ?").bind(device).first();
    const cool = coolDownMs(v && v.refusals);
    const paid = (await db.prepare("SELECT coalesce(sum(work), 0) AS w FROM answers WHERE device = ? AND credited = 1 AND official = 0 AND at >= ?")
      .bind(device, since).first()).w;
    const { results: touched } = await db.prepare(
      `SELECT DISTINCT q.id, q.state FROM answers a JOIN questions q ON q.id = a.question
        WHERE a.device = ? AND a.at >= ? AND a.void = 0 AND a.official = 0 AND q.state IN ('fact', 'spot')
          AND NOT EXISTS (SELECT 1 FROM answers o WHERE o.question = q.id AND o.official = 1 AND o.void = 0)`).bind(device, since).all();
    await db.batch([
      db.prepare("UPDATE verifiers SET banned = 1, refusals = refusals + 1, clean = 0, refused_until = ? WHERE id = ?").bind(iso(now + cool), device),
      db.prepare("UPDATE verifiers SET work = GREATEST(0, work - ?::bigint) WHERE id = ?::text").bind(paid, device),
      db.prepare(`INSERT INTO verifier_hours AS h (verifier, hour, work) VALUES (?, ?, ?)
                  ON CONFLICT (verifier, hour) DO UPDATE SET work = h.work + excluded.work`).bind(device, at.slice(0, 13), -paid),
      db.prepare("UPDATE answers SET void = 1, credited = 0 WHERE device = ? AND official = 0").bind(device),
      // WHAT IT TAKES BACK GOES TO THE OFFICIAL POOL: compute is never uncounted.
      db.prepare("INSERT INTO points_pool (at, source, work) SELECT ?::text, 'refused', ?::bigint WHERE ?::bigint > 0").bind(at, paid, paid),
    ]);
    for (const q of touched) {
      await db.prepare("UPDATE questions SET state = 'open', canon = NULL, fact_engine = NULL, fact_at = NULL WHERE id = ?").bind(q.id).run();
      const { results: served } = await db.prepare("SELECT * FROM demands WHERE question = ? AND state = 'served'").bind(q.id).all();
      for (const d of served) {
        const p = producerOf(d.producer);
        if (p.on_retract) await p.on_retract(env, { ...d, payload: JSON.parse(d.payload) });
        await db.prepare("UPDATE demands SET state = 'live', served_at = NULL, delivery_at=NULL WHERE producer = ? AND ref = ?").bind(d.producer, d.ref).run();
      }
    }
    return { cool_ms: cool, taken_back: paid, reopened: touched.length };
  }

  const queueable = (q) => !!(verbs[q.verb] || {}).pack && !!producers[q.producer] && !producers[q.producer].on_answer;
  const syncRules = {
    packable: Object.keys(verbs).filter((v) => verbs[v].pack),
    queueable: Object.keys(producers).filter((p) => !producers[p].on_answer),
    owed: Object.keys(producers).filter((p) => producers[p].wanted_in_sql && !producers[p].on_lease),
    canons: (result, work) => Object.fromEntries(Object.keys(verbs).map((v) => [v, verbs[v].canon(result, work)])),
  };
  return { answer, answerPack, answerOfficial, recordMany, settleQueued, settle, settleMany, decide, joinersParted, settleOfficial, refuse, syncRules };
}
