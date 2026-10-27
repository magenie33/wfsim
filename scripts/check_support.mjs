// EVERY NUMBER ON /support IS COUNTED, NOT TYPED.
//
// The page states what WFSim holds in counts. A page of counts has exactly one
// failure mode worth testing for — figures that are DRAWN but not COUNTED,
// since a hardcoded strip looks identical, reads correctly, and goes stale in
// a week unnoticed.
//
// So the sharp assertions here compare what is on screen against the source it
// claims to come from: the weapons tile against `META.weapons`, the mods tile
// against the union of `META.mod_pools`, the built line against the injected
// `PROJECT_FACTS`. A typed number fails all three.
//
// THE OTHER HALF IS WHAT THE READER HAS RUN — the one line on the page that is
// about them and the one that must never travel. Asserted in both directions:
// absent on a browser that has run nothing, present and CORRECT after a real
// run, and absent from the request that run sent.
import { openApp, sleep } from "./cdp.mjs";

const app = await openApp({ boot: 20000 });
const { evaluate, check, finish } = app;
const tag = "support";

// ---------------------------------------------------------------------------
// 1. THE FACTS STRIP — drawn, and drawn from the sources it claims.
//
// ENGLISH IS FORCED, never inherited: the app boots into the browser's own
// language, so on a machine whose Chrome reports zh-CN this pass would run in
// Chinese while calling itself the English one.
await app.setLang("en", 20000);
await app.load("/support", 20000);

const facts = await evaluate(`(() => {
  const tiles = [...document.querySelectorAll("#support-facts .sup-fact")].map((el) => ({
    n: el.querySelector("b").textContent.trim(),
    what: el.querySelector("span").textContent.trim(),
  }));
  // What the page could have counted for itself, computed here the same way
  // and independently: if the strip is a literal, these will not match.
  const mods = new Set();
  for (const pool of Object.values(META.mod_pools || {})) {
    for (const m of pool || []) mods.add(m.id);
  }
  let evo = 0;
  for (const w of META.weapons || []) {
    for (const t of w.evolutions || []) evo += (t.options || []).length;
  }
  const n = (k) => (META[k] || []).length;
  const boards = Object.values((BOARD_META && BOARD_META.boards) || {});
  return {
    tiles,
    roster: n("weapons") + n("frames") + mods.size + evo
      + n("arcanes") + n("abilities") + n("auras") + n("shards") + n("enemies"),
    uploads: boards.length ? Math.max(...boards.map((b) => b.submissions | 0)) : 0,
    scored: boards.reduce((t, b) => t + (b.listed | 0), 0),
    injected: PROJECT_FACTS, dev: RELEASE_ID === "dev",
    built: {
      hidden: $("support-built").hidden,
      text: $("support-built").textContent.trim(),
    },
  };
})()`);

const num = (s) => Number(String(s).replace(/[^0-9]/g, ""));
const drawn = facts.tiles.map((t) => t.n);

check(`${tag} the facts strip draws figures`,
  facts.tiles.length >= 3, JSON.stringify(drawn));
check(`${tag} ...every one of them a real number`,
  facts.tiles.length > 0 && facts.tiles.every((t) => num(t.n) > 0)
    && !/null|undefined|NaN/.test(drawn.join(" ")),
  JSON.stringify(facts.tiles));
// THE ONES THAT A TYPED STRIP FAILS. The roster figure is a SUM over every
// category META carries, so it also fails the day a category stops being
// counted — which is the way this number goes wrong, not by being mistyped.
check(`${tag} ...the roster figure is META's own categories, summed`,
  drawn.some((n) => num(n) === facts.roster), `on screen ${JSON.stringify(drawn)} vs META ${facts.roster}`);
// SUBMISSIONS ARE ONE POOL, HELD SCORES ARE PER RULER — summing the first or
// maxing the second is the mistake this pair is here to catch.
check(`${tag} ...the uploads figure is the board's submission pool`,
  facts.uploads > 0 && drawn.some((n) => num(n) === facts.uploads),
  `on screen ${JSON.stringify(drawn)} vs board ${facts.uploads}`);
check(`${tag} ...and the evaluations figure is every ruler's, added up`,
  facts.scored > 0 && drawn.some((n) => num(n) === facts.scored),
  `on screen ${JSON.stringify(drawn)} vs board ${facts.scored}`);

// ---------------------------------------------------------------------------
// 2. THE BUILD-TIME COUNTS. These cannot come from the page at all — they are
//    about the repository around it — so the only question is whether the
//    injection reached the shipping build and whether the page prints it.
const inj = facts.injected || {};
check(`${tag} build counts are injected on releases and omitted on the dev server`,
  facts.dev ? facts.injected === null : !!facts.injected && inj.rust_tests > 0 && inj.browser_checks > 0 && inj.commits > 0 && !!inj.first_commit_day,
  JSON.stringify(facts.injected));
// Compared on DIGITS rather than on a separator: the sentence is a template
// and the language decides its punctuation, so splitting it on a dash asserted
// English typography rather than the two numbers in it.
const digits = (s) => String(s).replace(/[^0-9]/g, "");
check(`${tag} ...and the page states when it started, with its commit count`,
  facts.dev ? facts.built.hidden : !facts.built.hidden
    && facts.built.text.includes(String(inj.first_commit_day))
    && digits(facts.built.text).includes(String(inj.commits)),
  JSON.stringify(facts.built));

// ---------------------------------------------------------------------------
// 3. NO SUM ANYWHERE ON THE PAGE: a price written into prose reads as a price
//    list, and a digit coming back in a sentence is how that returns unnoticed.
const money = await evaluate(`[...document.querySelectorAll("#support-page")]
  .map((el) => el.innerText).join(" ").match(/[$¥£€]\s?[0-9]|[0-9]+\s?(元|美元|USD|CNY)/g) || []`);
check(`${tag} no sentence on the page names a sum`,
  money.length === 0, JSON.stringify(money));

// ---------------------------------------------------------------------------
// 4. WHAT THE READER HAS RUN. Absent on a fresh browser, then a real run.
const fresh = await evaluate(`(() => {
  localStorage.removeItem("wfsim-use");
  renderSupport();
  return { hidden: $("support-usage").hidden, text: $("support-usage").textContent };
})()`);
check(`${tag} a browser that has run nothing is told nothing about itself`,
  fresh.hidden === true, JSON.stringify(fresh));

// A REAL RUN, at a run count this check chooses, so the engagement figure is
// checkable rather than merely present.
await app.load("/weapons/Torid", 22000);
const ran = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  setSimRuns(7);
  sim.duration = 8; sim.level = 30; sim.steel_path = false; sim.eximus = false;
  // WHAT THE RUN SENT, captured the way check_pace_and_hits does: Run Sim goes
  // through the worker fleet and never touches api, so hooking one alone comes
  // back with nothing.
  let body = null;
  const real = window.api;
  window.api = async (p, b) => { if (p === "/api/simulate") body = b; return real(p, b); };
  const realFleet = window.simulateFleet;
  window.simulateFleet = async (b, onp) => { body = b; return realFleet(b, onp); };
  await runSim();
  window.api = real;
  window.simulateFleet = realFleet;
  for (let i = 0; i < 60 && !body; i++) await sleep(400);
  return {
    stored: localStorage.getItem("wfsim-use"),
    sent: body ? JSON.stringify(body) : "",
  };
})()`);

let use = {};
try { use = JSON.parse(ran.stored || "{}"); } catch (_) { use = {}; }
check(`${tag} a run counts itself, once, at the runs it was given`,
  use.sims === 1 && use.engagements === 7, ran.stored);
// IT NEVER TRAVELS. The claim printed beside the number is that it is in this
// browser and has been sent nowhere, so the request that produced it is the
// one place that claim can be checked.
check(`${tag} ...and the fight it ran carries no part of it`,
  !!ran.sent && !ran.sent.includes("wfsim-use") && !/"sims"|"engagements"/.test(ran.sent),
  ran.sent.slice(0, 200));

const shown = await evaluate(`(async () => {
  history.pushState({}, "", "/support");
  route();
  await new Promise((r) => setTimeout(r, 400));
  return { hidden: $("support-usage").hidden, text: $("support-usage").textContent.trim(),
    counts: [...document.querySelectorAll("#support-use-facts [data-support-count]")].map((e) => [e.dataset.supportCount, Number(e.dataset.value)]) };
})()`);
check(`${tag} ...and the page says so, in both figures`,
  shown.hidden === false && JSON.stringify(shown.counts) === JSON.stringify([["sims", 1], ["engagements", 7]]),
  JSON.stringify(shown));

// ---------------------------------------------------------------------------
// 5. THE SHARED TOTALS, with a repeated submission pool and reduced motion.
await app.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
const shared = await evaluate(`(() => {
  const oldBoard = BOARD_META, oldTally = tallyTo;
  BOARD_META = { boards: { a: { submissions: 17, listed: 2 }, b: { submissions: 17, listed: 9 } } };
  tallyTo = { volunteers: 24680, official: 4321, computing: 3 };
  renderSupportCounts();
  const counts = [...document.querySelectorAll("#support-community-facts [data-support-count]")].map((e) => [e.dataset.supportCount, Number(e.dataset.value)]);
  const reduced = [...document.querySelectorAll("#support-community-facts .count-value")].map((e) => e.textContent);
  const personal = supportUse();
  BOARD_META = oldBoard; tallyTo = oldTally;
  return { counts, personal, reduced };
})()`);
check(`${tag} shared builds count the pool once and device scores use the verified tally`,
  JSON.stringify(shared.counts) === JSON.stringify([["builds", 17], ["volunteers", 24680]]), JSON.stringify(shared));
check(`${tag} reduced motion shows final values immediately`, shared.reduced.join("|") === "17|24,680", JSON.stringify(shared.reduced));
check(`${tag} shared totals do not change personal history`, shared.personal.sims === 1 && shared.personal.engagements === 7, JSON.stringify(shared.personal));

// 6. THE WHOLE PAGE IN CHINESE. A number filled into an untranslated template
//    is invisible in English and is half an English sentence on a zh page,
//    which is `check_mode_def`'s own lesson one page over.
await app.setLang("zh", 20000);
await app.load("/support", 20000);
const zh = await evaluate(`(() => {
  const el = $("support-page");
  const labels = [...el.querySelectorAll("#support-facts .sup-fact span")].map((s) => s.textContent.trim());
  return {
    heads: [...el.querySelectorAll(".bh h2")].map((h) => h.textContent.trim()),
    labels,
    built: $("support-built").textContent.trim(),
  };
})()`);
const han = (s) => /[一-鿿]/.test(s);
check(`${tag} zh the sections are named in Chinese`,
  zh.heads.length >= 2 && zh.heads.every(han), JSON.stringify(zh.heads));
check(`${tag} zh ...so are the figures' labels`,
  zh.labels.length >= 3 && zh.labels.every(han), JSON.stringify(zh.labels));
check(`${tag} zh ...and the line the build fills in`,
  facts.dev ? !zh.built : han(zh.built) && /\d/.test(zh.built), zh.built);

await sleep(200);
await finish("the support page counts what it claims, and keeps what is the reader's");
