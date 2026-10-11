// THE FACT FACTORY'S TWO PROPERTIES (worker/tasks.js), on random books, with no
// network: LIVENESS — a witness is handed nothing only when nothing exists that
// it may answer, and otherwise the earliest deadline among what it may — and
// TERMINATION — with honest, lying and non-deterministic witnesses answering at
// random, every question ends a fact or withdrawn once the official machines
// have had their turn. Then the cases that once failed in production, one each.
//   node scripts/check_tasks.mjs [trials=400]
import { factory, questionId, canon, nearAnswer } from "../worker/tasks.js";
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};
const TRIALS = Number((process.argv.find((a) => a.startsWith("trials=")) || "trials=400").slice(7));

// D1's surface on node's sqlite, as check_riven_gain.mjs has it.
const d1 = () => {
  const db = new DatabaseSync(":memory:");
  db.exec(readFileSync(new URL("../worker/schema.sql", import.meta.url), "utf8"));
  const stmt = (sql, args = []) => ({
    sql, args,
    bind: (...a) => stmt(sql, a),
    first: async () => db.prepare(sql).get(...args) ?? null,
    all: async () => ({ results: db.prepare(sql).all(...args) }),
    run: async () => ({ meta: { changes: Number(db.prepare(sql).run(...args).changes) } }),
  });
  return { raw: db, prepare: (sql) => stmt(sql),
    batch: async (ss) => ss.map((s) => (/^\s*select|\breturning\b/i.test(s.sql) ? { results: db.prepare(s.sql).all(...s.args) }
      : { meta: { changes: Number(db.prepare(s.sql).run(...s.args).changes) } })) };
};

// A seeded generator, so a failing trial can be run again by its number.
const rng = (seed) => () => {
  seed = (seed + 0x6D2B79F5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (r, xs) => xs[Math.floor(r() * xs.length)];
const iso = (ms) => new Date(ms).toISOString();
const NOW = Date.parse("2026-10-10T12:00:00.000Z");

const VERBS = {
  simulate: { cap: 2, lease_ms: 30 * 60_000, pack: true, canon: (r, work) => canon({ score: r.score, work }), near: nearAnswer(["work"]) },
  optimize: { cap: 3, lease_ms: 15 * 60_000, canon: (r, work) => canon({ build: r.build, score: r.score, work }) },
};
const told = [];
const PRODUCERS = {
  p: { on_fact: async (env, d, fact) => { told.push({ ref: d.ref, canon: fact.canon }); } },
  q: { on_fact: async (env, d, fact) => { told.push({ ref: d.ref, canon: fact.canon }); } },
};

console.log("liveness: handed nothing only when nothing may be answered, else the waited-on, then the earliest deadline");
{
  let bad = 0, first = "", badPack = 0, firstPack = "";
  for (let t = 0; t < TRIALS && bad < 3; t++) {
    const r = rng(t + 1), db = d1(), env = { LIBRARY: db };
    const f = factory({ verbs: VERBS, producers: PRODUCERS, random: r });
    const engines = ["e1", "e0"], owners = ["o1", "o2", "o3", "o4"], nets = ["", "n1", "n2", "n3"];
    const devices = Array.from({ length: 6 }, (_, i) => ({ device: `d${i}`, owner: pick(r, owners), net: pick(r, nets) }));
    const n = 1 + Math.floor(r() * 14);
    const qs = [];
    for (let i = 0; i < n; i++) {
      const id = `q${i}`, verb = pick(r, ["simulate", "optimize"]);
      const state = pick(r, ["open", "open", "open", "fact", "withdrawn", "disputed", "spot", "nondeterministic"]);
      const lanes = r() < 0.4 ? [{ lanes: 8, after_ms: 0 }, { lanes: 4, after_ms: 10_000 }, { lanes: 1, after_ms: 120_000 }] : null;
      const opened = NOW - Math.floor(r() * 300_000), due = NOW + Math.floor((r() - 0.5) * 7_200_000);
      const leased = r() < 0.2 ? pick(r, [NOW + 60_000, NOW - 60_000]) : null;
      const waits = r() < 0.2 ? 1 : 0;
      db.raw.prepare(`INSERT INTO questions (id, verb, request, state, due_at, opened_at, lanes, lease, lease_until, leased_to, waits)
        VALUES (?, ?, '{}', ?, ?, ?, ?, ?, ?, ?, ?)`).run(id, verb, state, iso(due), iso(opened), lanes && JSON.stringify(lanes),
        leased ? "l" : null, leased ? iso(leased) : null, leased ? "dx" : null, waits);
      db.raw.prepare("INSERT INTO demands (producer, ref, question, state, asked_at, due_at) VALUES ('p', ?, ?, 'live', ?, ?)")
        .run(id, id, iso(opened), iso(due));
      const answers = [];
      for (let k = Math.floor(r() * 3); k > 0; k--) {
        const w = pick(r, devices), a = { device: w.device, owner: w.owner, net: w.net, engine: pick(r, engines) };
        db.raw.prepare(`INSERT INTO answers (question, engine, device, owner, net, canon, result, work, at) VALUES (?, ?, ?, ?, ?, 'c', '{}', 1, ?)`)
          .run(id, a.engine, a.device, a.owner, a.net, iso(NOW));
        answers.push(a);
      }
      qs.push({ id, verb, state, lanes, opened, due, leased, answers, waits });
    }
    const me = pick(r, devices), lanes = pick(r, [0, 1, 2, 4, 8]), before = r() < 0.3 ? NOW + Math.floor((r() - 0.5) * 3_600_000) : null;
    const siblings = devices.filter((d) => d.owner === me.owner).map((d) => d.device);
    // THE RULE, said again the slow way.
    const may = (q) => q.state === "open" && !(q.leased && q.leased > NOW)
      && (!q.lanes || q.answers.length > 0
        || lanes >= Math.min(...q.lanes.filter((x) => x.after_ms <= NOW - q.opened).map((x) => x.lanes)))
      && (before === null || q.due < before)
      && !q.answers.some((a) => a.engine === "e1" && (siblings.includes(a.device) || a.owner === me.owner || (me.net && a.net === me.net)));
    const eligible = qs.filter(may).sort((a, b) => b.waits - a.waits || a.due - b.due || (a.id < b.id ? -1 : 1));
    const w = { device: me.device, owner: me.owner, siblings, net: me.net, lanes, before: before === null ? null : iso(before) };
    // A PACK, by the same rule: the first `size` it may answer of a packable
    // verb that nobody waits on — read before `next` leases one of them.
    const size = 1 + Math.floor(r() * 5);
    const packWant = eligible.filter((q) => !q.waits && q.verb === "simulate").slice(0, size).map((q) => q.id);
    const pack = await f.nextPack(env, w, "e1", { n: size, lease_ms: 60_000, most: 1 }, NOW);
    const packGot = pack ? pack.questions.map((q) => q.question) : [];
    const sameSet = (a, b) => JSON.stringify([...a].sort()) === JSON.stringify([...b].sort());
    if (!sameSet(packGot, packWant)) { badPack++; firstPack ||= `trial ${t}: got ${packGot}, want ${packWant}`; }
    if (pack) db.raw.prepare("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE lease = ?").run(pack.lease);
    const got = await f.next(env, { ...w, most: 99 }, "e1", NOW);
    const want = eligible[0] ? eligible[0].id : null;
    if ((got ? got.question : null) !== want) {
      bad++;
      first ||= `trial ${t}: got ${got && got.question}, want ${want}`;
    }
  }
  check(`${TRIALS} random books: next() is exactly the first eligible question in order, or null when there is none`, !bad, first);
  check(`...and nextPack(n) exactly the first n of them a pack may carry — none someone waits on`, !badPack, firstPack);
}

console.log("\ntermination: every question ends a fact or withdrawn");
{
  let bad = 0, first = "", liarsCredited = 0, refusedHonest = 0;
  for (let t = 0; t < Math.ceil(TRIALS / 4) && bad < 3; t++) {
    const r = rng(10_000 + t), db = d1(), env = { LIBRARY: db };
    const disproved = [];
    const f = factory({ verbs: VERBS, producers: PRODUCERS, random: r, spot: 0.2, recheck: 0.3,
      on_disproved: async (_, a) => { disproved.push(a); } });
    // Each question's true answer, and whether it is deterministic at all.
    const truth = new Map(), odd = new Set();
    const n = 1 + Math.floor(r() * 10);
    for (let i = 0; i < n; i++) {
      const verb = pick(r, ["simulate", "optimize"]);
      const id = await f.demand(env, { producer: pick(r, ["p", "q"]), ref: `r${i}`, verb, request: { i },
        due_in_ms: Math.floor(r() * 3_600_000), lanes: r() < 0.3 ? [{ lanes: 8, after_ms: 0 }, { lanes: 1, after_ms: 120_000 }] : null }, "e1", NOW);
      truth.set(id, Math.floor(r() * 1000));
      if (r() < 0.15) odd.add(id);
    }
    const owners = ["o1", "o2", "o3", "o4", "o5"];
    const people = Array.from({ length: 8 }, (_, i) => ({ device: `d${i}`, owner: pick(r, owners), net: pick(r, ["", "n1", "n2", "n3", "n4"]),
      liar: r() < 0.2, lanes: pick(r, [1, 4, 8]) }));
    const answerFor = (w, q) => {
      const v = truth.get(q.question);
      const score = w.liar ? v + 1 : v;
      const work = odd.has(q.question) && !w.official ? 100 + Math.floor(r() * 1e6) : 100;
      return { result: { score, build: "b" }, work };
    };
    let now = NOW;
    for (let step = 0; step < 2000; step++) {
      now += 1000;
      // Mostly volunteers; past every deadline, the official machines too.
      const official = step > 200 && r() < 0.3;
      const w = official ? { device: "official", owner: "official", official: true } : pick(r, people);
      const siblings = official ? ["official"] : people.filter((p) => p.owner === w.owner).map((p) => p.device);
      const q = await f.next(env, { ...w, siblings, most: 1 }, "e1", now);
      if (!q) {
        const open = db.raw.prepare("SELECT count(*) AS n FROM questions WHERE state NOT IN ('fact', 'withdrawn')").get().n;
        if (!open) break;
        if (step > 200) now += 3_600_000; // time passes; deadlines go by
        continue;
      }
      const { result, work } = answerFor(w, q);
      await f.answer(env, w, { lease: q.lease, engine: "e1", result, work }, now);
    }
    const left = db.raw.prepare("SELECT id, state FROM questions WHERE state NOT IN ('fact', 'withdrawn')").all();
    if (left.length) { bad++; first ||= `trial ${t}: ${JSON.stringify(left)}`; }
    for (const a of db.raw.prepare("SELECT device, credited FROM answers WHERE credited = 1").all()) {
      if (people.find((p) => p.device === a.device && p.liar)) liarsCredited++;
    }
    for (const d of disproved) if (people.find((p) => p.device === d.device && !p.liar)) refusedHonest++;
    // Nobody is refused for a question that was not deterministic.
    for (const id of odd) {
      const s = db.raw.prepare("SELECT state FROM questions WHERE id = ?").get(id);
      if (s.state === "nondeterministic") { bad++; first ||= `trial ${t}: ${id} stayed nondeterministic`; }
    }
  }
  check(`${Math.ceil(TRIALS / 4)} random populations: no question is left waiting`, !bad, first);
  check("an honest witness is never refused", refusedHonest === 0, `${refusedHonest} refusals of honest witnesses`);
  console.log(`  (liars credited by agreeing with each other before a spot check: ${liarsCredited} answers)`);
}

console.log("\nthe cases that failed in production");
{
  const db = d1(), env = { LIBRARY: db };
  const f = factory({ verbs: VERBS, producers: PRODUCERS, random: () => 1, on_disproved: async (_, a) => { throw new Error(`refused ${a.device}`); } });
  // A WINDOW FULL OF ANSWERED-OUT QUESTIONS: nine optimize questions
  // three different owners each answered, never agreeing, then one more.
  for (let i = 0; i < 9; i++) {
    const id = await f.demand(env, { producer: "p", ref: `full${i}`, verb: "optimize", request: { full: i }, due_in_ms: -1000 }, "e1", NOW);
    for (const [k, o] of ["x", "y", "z"].entries()) {
      const q = await f.next(env, { device: `${o}${i}`, owner: o, net: `n${o}` }, "e1", NOW);
      await f.answer(env, { device: `${o}${i}`, owner: o, net: `n${o}` }, { lease: q.lease, engine: "e1", result: { build: "b", score: 1 }, work: 100 + k }, NOW);
      void id;
    }
  }
  const late = await f.demand(env, { producer: "p", ref: "late", verb: "optimize", request: { late: 1 }, due_in_ms: 86_400_000 }, "e1", NOW);
  const g = await f.next(env, { device: "w1", owner: "w", net: "nw" }, "e1", NOW);
  check("nine answered-out questions ahead of it do not starve the tenth", g && g.question === late, JSON.stringify(g));
  const states = db.raw.prepare("SELECT state, count(*) AS n FROM questions WHERE request LIKE '%full%' GROUP BY state").all();
  check("...and three witnesses differing only in work is an engine fault, not three liars",
    states.length === 1 && states[0].state === "nondeterministic" && states[0].n === 9, JSON.stringify(states));
}
{
  const db = d1(), env = { LIBRARY: db };
  told.length = 0;
  const f = factory({ verbs: VERBS, producers: PRODUCERS, random: () => 1 });
  db.raw.exec("INSERT INTO verifiers (id, seen) VALUES ('a', 'x'), ('b', 'x')");
  // ONE COMPUTATION FOR TWO PRODUCERS: a survey shape and a chat asking the same.
  const one = await f.demand(env, { producer: "p", ref: "survey-shape", verb: "optimize", request: { riven: "x" }, due_in_ms: 86_400_000 }, "e1", NOW);
  const two = await f.demand(env, { producer: "q", ref: "chat-ask", verb: "optimize", request: { riven: "x" }, due_in_ms: 120_000 }, "e1", NOW);
  check("two producers asking one thing share one question", one === two && db.raw.prepare("SELECT count(*) AS n FROM questions").get().n === 1);
  check("...whose deadline is the earlier one", db.raw.prepare("SELECT due_at FROM questions").get().due_at === iso(NOW + 120_000));
  for (const w of [{ device: "a", owner: "oa", net: "na" }, { device: "b", owner: "ob", net: "nb" }]) {
    const q = await f.next(env, w, "e1", NOW);
    await f.answer(env, w, { lease: q.lease, engine: "e1", result: { build: "b", score: 5 }, work: 7 }, NOW);
  }
  check("...answered once, told to both", told.length === 2 && new Set(told.map((x) => x.ref)).size === 2, JSON.stringify(told));
  const work = db.raw.prepare("SELECT sum(work) AS w FROM verifiers").get().w;
  check("...and credited once: the work done, to each witness that did it", work === 14, String(work));
  told.length = 0;
  await f.demand(env, { producer: "p", ref: "asked-again", verb: "optimize", request: { riven: "x" }, due_in_ms: 1000 }, "e1", NOW);
  check("a question already a fact on the served engine is told at once, unfought", told.length === 1 && told[0].ref === "asked-again");
  await f.demand(env, { producer: "p", ref: "next-release", verb: "optimize", request: { riven: "x" }, due_in_ms: 1000 }, "e2", NOW);
  check("...and on another engine it opens again", db.raw.prepare("SELECT state FROM questions").get().state === "open");
  const c = await f.next(env, { device: "c", owner: "oc", net: "nc" }, "e2", NOW);
  await f.answer(env, { device: "c", owner: "oc", net: "nc" }, { lease: c.lease, engine: "e2", result: { build: "b", score: 5 }, work: 7 }, NOW);
  check("...where one answer of the new engine that reproduces the old bits makes the fact again",
    db.raw.prepare("SELECT state, fact_engine FROM questions").get().fact_engine === "e2");
}
{
  const db = d1(), env = { LIBRARY: db };
  const f = factory({ verbs: VERBS, producers: PRODUCERS });
  await f.demand(env, { producer: "p", ref: "gone", verb: "simulate", request: { row: 1 }, due_in_ms: 1000 }, "e1", NOW);
  await f.withdraw(env, "p", "gone", NOW);
  check("a question nobody asks for any more is withdrawn and handed to nobody",
    db.raw.prepare("SELECT state FROM questions").get().state === "withdrawn" && (await f.next(env, { device: "a" }, "e1", NOW)) === null);
  const gone = { p: { ...PRODUCERS.p, still_wanted: async () => false } };
  const f2 = factory({ verbs: VERBS, producers: gone });
  await f2.demand(env, { producer: "p", ref: "settled-elsewhere", verb: "simulate", request: { row: 2 }, due_in_ms: 1000 }, "e1", NOW);
  check("...nor is one its producer no longer wants, found where it is picked",
    (await f2.next(env, { device: "a" }, "e1", NOW)) === null);
  const id = await questionId("simulate", { row: 2 });
  check("...which is withdrawn there", db.raw.prepare("SELECT state FROM questions WHERE id = ?").get(id).state === "withdrawn");
}
{
  const db = d1(), env = { LIBRARY: db };
  const f = factory({ verbs: VERBS, producers: PRODUCERS });
  const id = await f.demand(env, { producer: "p", ref: "spotted", verb: "simulate", request: { row: 3 }, due_in_ms: 1000 }, "e1", NOW);
  db.raw.prepare("UPDATE questions SET state = 'spot' WHERE id = ?").run(id);
  await f.withdraw(env, "p", "spotted", NOW);
  const [t] = await f.claim(env, { device: "official" }, { states: ["spot"], max: 1 }, "e1", NOW);
  check("a spot nobody asks for any more is still claimed with its demand, so the official machines can settle it",
    t && t.question === id && t.demands.length === 1 && t.demands[0].ref === "spotted", JSON.stringify(t));
}

{
  const db = d1(), env = { LIBRARY: db };
  told.length = 0;
  const f = factory({ verbs: VERBS, producers: PRODUCERS, random: () => 1 });
  db.raw.exec("INSERT INTO verifiers (id, seen) VALUES ('a', 'x'), ('b', 'x')");
  // MANY ASKS AT ONCE ARE THE SAME AS EACH ALONE: past a chunk, one asked
  // twice, and one whose fact on the served engine stands is delivered at once.
  const known = await f.demand(env, { producer: "p", ref: "known", verb: "simulate", request: { n: 0 }, due_in_ms: 1000 }, "e1", NOW);
  for (const w of [{ device: "a", owner: "oa", net: "na" }, { device: "b", owner: "ob", net: "nb" }]) {
    const q = await f.next(env, w, "e1", NOW);
    await f.answer(env, w, { lease: q.lease, engine: "e1", result: { score: 2 }, work: 3 }, NOW);
  }
  told.length = 0;
  const asks = Array.from({ length: 120 }, (_, i) => ({ producer: "q", ref: `many${i}`, verb: "simulate", request: { n: i % 119 }, due_in_ms: 86_400_000 }));
  const ids = await f.demandAll(env, asks, "e1", NOW);
  check("a batch of asks is a demand each, past one chunk, and one request asked twice is one question",
    ids.length === 120 && ids[0] === known && db.raw.prepare("SELECT count(*) AS n FROM demands WHERE producer = 'q'").get().n === 120
      && db.raw.prepare("SELECT count(*) AS n FROM questions").get().n === 119);
  check("...and one whose fact on the served engine stands is told at once", told.some((t) => t.ref === "many0"), JSON.stringify(told));
}

{
  const db = d1(), env = { LIBRARY: db };
  // AN ANSWERED QUESTION IS NOBODY ELSE'S UNTIL IT IS SETTLED: it heads every
  // order, so a lease freed before the fact was handed out and answered for nothing.
  let f, during = [];
  const probe = { on_answer: async () => { during.push(await f.next(env, { device: "c", owner: "oc", net: "nc" }, "e1", NOW)); },
    on_fact: async () => {} };
  f = factory({ verbs: VERBS, producers: { ...PRODUCERS, r: probe }, random: () => 1 });
  await f.demand(env, { producer: "r", ref: "head", verb: "simulate", request: { n: 7 }, due_in_ms: 1000 }, "e1", NOW);
  for (const w of [{ device: "a", owner: "oa", net: "na" }, { device: "b", owner: "ob", net: "nb" }]) {
    const q = await f.next(env, w, "e1", NOW);
    if (q) await f.answer(env, w, { lease: q.lease, engine: "e1", result: { score: 2 }, work: 3 }, NOW);
  }
  check("a question being answered is handed to nobody else until it is settled", during.length === 2 && during.every((x) => x === null),
    JSON.stringify(during.map((x) => x && x.question)));
  check("...and is free once it is", db.raw.prepare("SELECT count(*) AS n FROM questions WHERE lease IS NOT NULL").get().n === 0);
}

{
  const db = d1(), env = { LIBRARY: db };
  // A BROWSER CLAIMED BY AN OWNER AFTER IT TOOK A QUESTION answers as that
  // owner's second: the same answer twice from one owner is no dispute.
  const f = factory({ verbs: VERBS, producers: PRODUCERS, random: () => 1 });
  const id = await f.demand(env, { producer: "p", ref: "claimed", verb: "simulate", request: { n: 8 }, due_in_ms: 1000 }, "e1", NOW);
  const a = { device: "a", owner: "oa", net: "na" }, d = { device: "d", owner: "d", net: "nd" };
  const qa = await f.next(env, a, "e1", NOW);
  await f.answer(env, a, { lease: qa.lease, engine: "e1", result: { score: 2 }, work: 3 }, NOW);
  const qd = await f.next(env, d, "e1", NOW);
  await f.answer(env, { ...d, owner: "oa" }, { lease: qd.lease, engine: "e1", result: { score: 2 }, work: 3 }, NOW);
  check("one owner's two answers fill no cap, and the question stays open for a second witness",
    db.raw.prepare("SELECT state FROM questions WHERE id = ?").get(id).state === "open");
}

console.log("\npacks");
{
  // A PACK'S ANSWERS TOGETHER ARE THE SAME AS EACH ALONE: two witnesses answer
  // the same rows — agreeing on some, not on others — once by packs and once one
  // by one; every state, fact and credit comes out equal.
  const run = async (packed) => {
    const db = d1(), env = { LIBRARY: db };
    told.length = 0;
    const f = factory({ verbs: VERBS, producers: PRODUCERS, random: () => 1 });
    db.raw.exec("INSERT INTO verifiers (id, seen) VALUES ('a', 'x'), ('b', 'x')");
    for (let i = 0; i < 9; i++) await f.demand(env, { producer: "p", ref: `row${i}`, verb: "simulate", request: { row: i }, due_in_ms: 1000 + i }, "e1", NOW);
    for (const w of [{ device: "a", owner: "oa", net: "na" }, { device: "b", owner: "ob", net: "nb" }]) {
      const score = (q) => (w.device === "b" && JSON.parse(db.raw.prepare("SELECT request FROM questions WHERE id = ?").get(q).request).row % 3 === 0 ? 2 : 1);
      if (packed) {
        // …SENT IN TWO PARTS, as a page sends them every half minute.
        const pack = await f.nextPack(env, w, "e1", { n: 9, lease_ms: 60_000 }, NOW);
        const rows = pack.questions.map((q) => ({ question: q.question, result: { score: score(q.question) }, work: 5 }));
        await f.answerPack(env, w, { lease: pack.lease, engine: "e1", answers: rows.slice(0, 4) }, NOW);
        await f.answerPack(env, w, { lease: pack.lease, engine: "e1", answers: rows.slice(4), done: true }, NOW);
      } else {
        for (let q; (q = await f.next(env, w, "e1", NOW));) await f.answer(env, w, { lease: q.lease, engine: "e1", result: { score: score(q.question) }, work: 5 }, NOW);
      }
    }
    return JSON.stringify({
      questions: db.raw.prepare("SELECT id, state, canon, lease FROM questions ORDER BY id").all(),
      paid: db.raw.prepare("SELECT id, work, agreed FROM verifiers ORDER BY id").all(),
      hours: db.raw.prepare("SELECT verifier, tasks, work FROM verifier_hours ORDER BY verifier").all(),
      told: [...told].sort((x, y) => (x.ref < y.ref ? -1 : 1)),
    });
  };
  const alone = await run(false), together = await run(true);
  check("a pack's answers together make every state, fact and credit that each alone makes", alone === together, `${alone}\n  ${together}`);
  check("...three disputes and six facts among them", (JSON.parse(together).questions.filter((q) => q.state === "disputed").length === 3)
    && JSON.parse(together).questions.filter((q) => q.state === "fact").length === 6);
}
{
  // TWO WITNESSES SETTLING ONE QUESTION AT THE SAME MOMENT pay once: both read
  // the two agreeing answers before either writes.
  const db = d1(), env = { LIBRARY: db };
  const f = factory({ verbs: VERBS, producers: PRODUCERS, random: () => 1 });
  db.raw.exec("INSERT INTO verifiers (id, seen) VALUES ('a', 'x'), ('b', 'x')");
  const id = await f.demand(env, { producer: "p", ref: "race", verb: "simulate", request: { race: 1 }, due_in_ms: 1000 }, "e1", NOW);
  for (const [d, o] of [["a", "oa"], ["b", "ob"]]) {
    db.raw.prepare(`INSERT INTO answers (question, engine, device, owner, net, canon, result, work, at) VALUES (?, 'e1', ?, ?, ?, ?, '{"score":1}', 5, ?)`)
      .run(id, d, o, `n${d}`, canon({ score: 1, work: 5 }), iso(NOW));
  }
  told.length = 0;
  await Promise.all([f.settle(env, id, "e1", NOW), f.settle(env, id, "e1", NOW)]);
  const paid = db.raw.prepare("SELECT sum(work) AS w, sum(agreed) AS n FROM verifiers").get();
  check("two settlings of one question at once credit each witness once", paid.w === 10 && paid.n === 2, JSON.stringify(paid));
}

console.log("\nthe watchdog");
{
  // A STALL: a survey's shapes overdue, volunteers at work on
  // other things, and not one answer to any shape for over an hour.
  const db = d1(), env = { LIBRARY: db };
  const f = factory({ verbs: VERBS, producers: PRODUCERS });
  const later = NOW + 3_600_000;
  db.raw.prepare("INSERT INTO verifiers (id, seen, last_at) VALUES ('busy', 'x', ?)").run(iso(later - 60_000));
  await f.demand(env, { producer: "p", ref: "shape", verb: "optimize", request: { shape: 1 }, due_in_ms: 1000 }, "e1", NOW);
  await f.demand(env, { producer: "q", ref: "row", verb: "simulate", request: { row: 1 }, due_in_ms: 10 * 3_600_000 }, "e1", NOW);
  const found = await f.audit(env, later);
  check("a kind whose overdue questions got no answer in half an hour, while witnesses worked, is a stall",
    found.length === 1 && found[0].kind === "stalled:p", JSON.stringify(found));
  const q = await f.next(env, { device: "busy", owner: "o", net: "n" }, "e1", later - 5 * 60_000);
  await f.answer(env, { device: "busy", owner: "o", net: "n" }, { lease: q.lease, engine: "e1", result: { build: "b", score: 1 }, work: 1 }, later - 5 * 60_000);
  check("...and one answer to it is not", !(await f.audit(env, later)).some((x) => x.kind === "stalled:p"));
  check("a healthy factory has nothing to say", (await f.audit(env, later)).length === 0, JSON.stringify(await f.audit(env, later)));
  db.raw.prepare("UPDATE questions SET state = 'disputed', opened_at = ?").run(iso(NOW - 2 * 3_600_000));
  check("a dispute nobody settled in an hour is said", (await f.audit(env, later)).some((x) => x.kind === "unsettled"));
  // SAID ONCE, then again only once it has stood six hours.
  const said = [];
  const { watchdog } = await import("../worker/factory.js");
  const tell = { LIBRARY: db, CLOUD: { fetch: async (r) => { said.push(await r.json()); return new Response('{"ok":true}'); } } };
  await watchdog(tell, later);
  await watchdog(tell, later + 600_000);
  check("the watchdog tells the owner once", said.length === 1 && /unsettled/.test(said[0].text), JSON.stringify(said));
  await watchdog(tell, later + 7 * 3_600_000);
  check("...and again once it has stood six hours", said.length === 2);
}

console.log("\na score a few ULP off, its work equal, is unpaid but never refused");
{
  const db = d1(), env = { LIBRARY: db }, refused = [];
  const f = factory({ verbs: VERBS, producers: PRODUCERS, random: () => 1, on_disproved: async (_, a) => { refused.push(a.device); } });
  const S = 6.785217593620141;
  for (const [ref, off] of [["ulp", { score: 6.785217593620138, work: 100 }], ["work", { score: S, work: 101 }], ["far", { score: S * (1 + 1e-9), work: 100 }]]) {
    const id = await f.demand(env, { producer: "p", ref, verb: "simulate", request: { ref }, due_in_ms: 3_600_000 }, "e1", NOW);
    for (const [who, r] of [[`a-${ref}`, { score: S, work: 100 }], [`b-${ref}`, off]]) {
      const w = { device: who, owner: who, net: `n${who}` };
      const q = await f.next(env, w, "e1", NOW);
      await f.answer(env, w, { lease: q.lease, engine: "e1", result: { score: r.score }, work: r.work }, NOW);
    }
    await f.answerOfficial(env, { device: "official", owner: "official" }, { question: id, engine: "e1", result: { score: S }, work: 100 }, NOW);
  }
  check("an ULP off with equal work is not refused", !refused.includes("b-ulp"), JSON.stringify(refused));
  check("...nor paid", !db.raw.prepare("SELECT credited FROM answers WHERE device = 'b-ulp'").get().credited);
  check("...while another work, or a score further off, is refused", refused.includes("b-work") && refused.includes("b-far"), JSON.stringify(refused));
}

console.log("\nan official answer without work witnesses the score alone");
{
  const db = d1(), env = { LIBRARY: db }, refused = [];
  const f = factory({ verbs: VERBS, producers: PRODUCERS, random: () => 1, on_disproved: async (_, a) => { refused.push(a.device); } });
  const S = 3.25, OFF = { device: "official", owner: "official" };
  const state = (id) => db.raw.prepare("SELECT state FROM questions WHERE id = ?").get(id).state;
  const paid = (who) => !!db.raw.prepare("SELECT credited FROM answers WHERE device = ?").get(who).credited;
  const ask = async (ref) => f.demand(env, { producer: "p", ref, verb: "simulate", request: { ref }, due_in_ms: 3_600_000 }, "e1", NOW);
  const volunteer = async (who, work) => {
    const w = { device: who, owner: who, net: `n${who}` };
    const q = await f.next(env, w, "e1", NOW);
    await f.answer(env, w, { lease: q.lease, engine: "e1", result: { score: S }, work }, NOW);
  };
  const alone = await ask("alone");
  check("with nobody else's answer it is the fact", await f.answerOfficial(env, OFF, { question: alone, engine: "e1", result: { score: S }, work: null }, NOW) === "fact");
  const owed = await ask("owed");
  await volunteer("v-right", 100);
  check("a volunteer who gave its score is owed a check of the work: spot",
    await f.answerOfficial(env, OFF, { question: owed, engine: "e1", result: { score: S }, work: null }, NOW) === "spot" && !paid("v-right"));
  await f.answerOfficial(env, OFF, { question: owed, engine: "e1", result: { score: S }, work: 100 }, NOW);
  check("...and the witness that counts work then pays them", state(owed) === "fact" && paid("v-right"));
  const lied = await ask("lied");
  await volunteer("v-lied", 999);
  await f.answerOfficial(env, OFF, { question: lied, engine: "e1", result: { score: S }, work: null }, NOW);
  await f.answerOfficial(env, OFF, { question: lied, engine: "e1", result: { score: S }, work: 100 }, NOW);
  check("...or refuses one whose work it disproves", refused.includes("v-lied") && !paid("v-lied"), JSON.stringify(refused));
}

console.log("\na recheck: one more independent volunteer before a fact");
{
  const db = d1(), env = { LIBRARY: db };
  let roll = 0;
  const f = factory({ verbs: VERBS, producers: PRODUCERS, spot: 0, recheck: 0.5, random: () => roll, on_disproved: async () => {} });
  const ask = async (ref, due = 3_600_000) => f.demand(env, { producer: "p", ref, verb: "simulate", request: { ref }, due_in_ms: due }, "e1", NOW);
  const row = (id) => db.raw.prepare("SELECT * FROM questions WHERE id = ?").get(id);
  const paid = (id) => db.raw.prepare("SELECT device FROM answers WHERE question = ? AND credited = 1 ORDER BY device").all(id).map((a) => a.device).join();
  const delivered = (ref) => told.filter((x) => x.ref === ref).length;
  const who = (device, owner = device, net = `n${device}`, siblings = [device]) => ({ device, owner, net, siblings });
  const volunteer = async (w, score, at = NOW) => {
    const q = await f.next(env, w, "e1", at);
    if (!q) return null;
    await f.answer(env, w, { lease: q.lease, engine: "e1", result: { score }, work: 100 }, at);
    return q.question;
  };
  const agreed = await ask("agreed");
  await volunteer(who("r1"), 7);
  await volunteer(who("r2"), 7);
  check("two agreeing on a row the roll picks leave it open, needing a third, nobody credited and nothing delivered",
    row(agreed).state === "open" && row(agreed).needs === 3 && paid(agreed) === "" && !delivered("agreed"), JSON.stringify(row(agreed)));
  await f.settle(env, agreed, "e1", NOW);
  check("...and settled again before a third comes, it is still open, not a dispute", row(agreed).state === "open", JSON.stringify(row(agreed)));
  check("...and no device, owner or network of either is the third",
    !(await f.next(env, who("r1-alt", "r1", "nother", ["r1-alt", "r1"]), "e1", NOW))
      && !(await f.next(env, who("r3", "r3", "nr2"), "e1", NOW)));
  roll = 0.9;
  const p = await f.nextPack(env, who("r4"), "e1", { n: 5, lease_ms: 60_000 }, NOW);
  await f.answerPack(env, who("r4"), { lease: p.lease, engine: "e1", answers: [{ question: agreed, result: { score: 7 }, work: 100 }], done: true }, NOW);
  check("a third agreeing, from a pack, makes it a fact: all three credited once, delivered once",
    row(agreed).state === "fact" && paid(agreed) === "r1,r2,r4" && delivered("agreed") === 1
      && db.raw.prepare("SELECT count(*) AS n FROM answers WHERE credited = 1").get().n === 3, `${JSON.stringify(row(agreed))} ${paid(agreed)}`);
  roll = 0;
  const lied = await ask("lied");
  await volunteer(who("l1"), 5);
  await volunteer(who("l2"), 5);
  await volunteer(who("l3"), 6);
  check("a third who differs makes a dispute, nobody credited", row(lied).state === "disputed" && paid(lied) === "", JSON.stringify(row(lied)));
  const late = await ask("late", 1000);
  await volunteer(who("t1"), 9);
  await volunteer(who("t2"), 9);
  roll = 0.9;
  const claimed = await f.claim(env, { device: "official" }, { states: ["open"], max: 5 }, "e1", NOW + 2000);
  await f.answerOfficial(env, { device: "official", owner: "official" }, { question: late, engine: "e1", result: { score: 9 }, work: 100 }, NOW + 2000);
  check("a rechecked row past its deadline is the official machines', and they credit the two who agreed",
    claimed.some((t) => t.question === late) && row(late).needs === 3 && row(late).state === "fact" && paid(late) === "t1,t2",
    `${JSON.stringify(row(late))} ${paid(late)}`);
}

console.log(failures ? `\n${failures} failed` : "\nevery question becomes a fact, and nobody waits on one that cannot");
process.exitCode = failures ? 1 : 0;
