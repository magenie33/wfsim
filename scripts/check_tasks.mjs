// THE FACT FACTORY'S TWO PROPERTIES (worker/tasks.js), on random books, with no
// network: LIVENESS — a witness is handed nothing only when nothing exists that
// it may answer, and otherwise the earliest deadline among what it may — and
// TERMINATION — with honest, lying and non-deterministic witnesses answering at
// random, every question ends a fact or withdrawn once the official machines
// have had their turn. Then the cases that once failed in production, one each.
//   node scripts/check_tasks.mjs [trials=400]
import { factory, questionId, canon } from "../worker/tasks.js";
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
    batch: async (ss) => ss.map((s) => (/^\s*select/i.test(s.sql) ? { results: db.prepare(s.sql).all(...s.args) }
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
  simulate: { cap: 2, lease_ms: 30 * 60_000, canon: (r, work) => canon({ score: r.score, work }) },
  optimize: { cap: 3, lease_ms: 15 * 60_000, canon: (r, work) => canon({ build: r.build, score: r.score, work }) },
};
const told = [];
const PRODUCERS = {
  p: { on_fact: async (env, d, fact) => { told.push({ ref: d.ref, canon: fact.canon }); } },
  q: { on_fact: async (env, d, fact) => { told.push({ ref: d.ref, canon: fact.canon }); } },
};

console.log("liveness: handed nothing only when nothing may be answered, else the waited-on, then the earliest deadline");
{
  let bad = 0, first = "";
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
      qs.push({ id, state, lanes, opened, due, leased, answers, waits });
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
    const got = await f.next(env, { device: me.device, owner: me.owner, siblings, net: me.net, lanes, most: 99, before: before === null ? null : iso(before) }, "e1", NOW);
    const want = eligible[0] ? eligible[0].id : null;
    if ((got ? got.question : null) !== want) {
      bad++;
      first ||= `trial ${t}: got ${got && got.question}, want ${want}`;
    }
  }
  check(`${TRIALS} random books: next() is exactly the first eligible question in order, or null when there is none`, !bad, first);
}

console.log("\ntermination: every question ends a fact or withdrawn");
{
  let bad = 0, first = "", liarsCredited = 0, refusedHonest = 0;
  for (let t = 0; t < Math.ceil(TRIALS / 4) && bad < 3; t++) {
    const r = rng(10_000 + t), db = d1(), env = { LIBRARY: db };
    const disproved = [];
    const f = factory({ verbs: VERBS, producers: PRODUCERS, random: r, spot: 0.2,
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

console.log(failures ? `\n${failures} failed` : "\nevery question becomes a fact, and nobody waits on one that cannot");
process.exitCode = failures ? 1 : 0;
