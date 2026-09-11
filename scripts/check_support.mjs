// EVERY NUMBER ON THE PAGE THAT ASKS FOR SOMETHING IS COUNTED, NOT TYPED.
//
// `/support` is the one page here that asks the reader for money, and it makes
// its case in counts. A page of counts has exactly one failure mode worth
// testing for — figures that are DRAWN but not COUNTED, since a hardcoded strip
// looks identical, reads correctly, and goes stale in a week unnoticed.
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
//
// AND THE NEGATIVE CONTROL IS THE CHANNELS: an entry with no url must draw
// nothing, which is what makes an unopened Patreon safe to declare.
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
    scored: boards.reduce((t, b) => t + (b.held | 0), 0),
    injected: PROJECT_FACTS,
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
check(`${tag} the site build injected what it counted`,
  !!facts.injected && inj.rust_tests > 0 && inj.browser_checks > 0 && inj.commits > 0 && !!inj.first_commit_day,
  JSON.stringify(facts.injected));
// Compared on DIGITS rather than on a separator: the sentence is a template
// and the language decides its punctuation, so splitting it on a dash asserted
// English typography rather than the two numbers in it.
const digits = (s) => String(s).replace(/[^0-9]/g, "");
check(`${tag} ...and the page states when it started, with its commit count`,
  !facts.built.hidden
    && facts.built.text.includes(String(inj.first_commit_day))
    && digits(facts.built.text).includes(String(inj.commits)),
  JSON.stringify(facts.built));

// ---------------------------------------------------------------------------
// 3. THE CHANNELS, and the negative control that makes an unopened account safe
//    to declare: an entry with no url draws nothing at all.
const chans = await evaluate(`(() => {
  const cards = [...document.querySelectorAll("#support-channels .sup-card")].map((a) => ({
    href: a.getAttribute("href"),
    name: a.querySelector(".sup-name").textContent.trim(),
    what: a.querySelector(".sup-what").textContent.trim(),
  }));
  return {
    cards,
    declared: SUPPORT_CHANNELS.map((c) => ({ id: c.id, hasUrl: !!c.url })),
  };
})()`);

const withUrl = chans.declared.filter((c) => c.hasUrl);
const withoutUrl = chans.declared.filter((c) => !c.hasUrl);
check(`${tag} every channel that has a link is offered`,
  chans.cards.length === withUrl.length && chans.cards.length > 0,
  `${chans.cards.length} drawn, ${withUrl.length} with a url`);
check(`${tag} ...and one that does not is not (negative control)`,
  withoutUrl.length === 0 || chans.cards.length === withUrl.length,
  JSON.stringify(chans.declared));
// NO SUM ANYWHERE ON THE PAGE. Each channel shows its own minimum and ladder at
// the moment of paying; a number written here is a staler copy of one, and it
// makes the page read as a price list — the shape DE's rule does not allow. A
// digit reappearing on a card is exactly how that comes back unnoticed.
const money = await evaluate(`[...document.querySelectorAll("#support-page")]
  .map((el) => el.innerText).join(" ").match(/[$¥£€]\s?[0-9]|[0-9]+\s?(元|美元|USD|CNY)/g) || []`);
check(`${tag} ...and no card, and no sentence, names a sum`,
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
  return { hidden: $("support-usage").hidden, text: $("support-usage").textContent.trim() };
})()`);
check(`${tag} ...and the page says so, in both figures`,
  shown.hidden === false && /\b1\b/.test(shown.text) && /\b7\b/.test(shown.text),
  JSON.stringify(shown));

// ---------------------------------------------------------------------------
// 5. THE ASK, WHERE THE ANSWER LANDED. Three things a line allowed to appear
//    inside a result panel has to get right, and all three fail silently:
//    nothing before the gate, exactly one line on the run that reaches it, and
//    never again after that. The gate is read from the page rather than typed,
//    so moving `NUDGE_AFTER` moves this check with it.
const nudge = await evaluate(`(async () => {
  const line = () => document.querySelectorAll("#sim-results .sup-nudge").length;
  // THE NEGATIVE CONTROL IS THE RUN ABOVE: one run, and the flag that retires
  // the ask must still be unset — a gate that never gates asks everybody.
  const beforeGate = { asked: localStorage.getItem("wfsim-asked"), drawn: line() };
  localStorage.removeItem("wfsim-asked");
  // ONE SHORT OF THE GATE, so the run below is the one that reaches it.
  localStorage.setItem("wfsim-use", JSON.stringify({ sims: NUDGE_AFTER - 1, engagements: 99 }));
  setSimRuns(1);
  sim.duration = 4; sim.level = 30; sim.steel_path = false; sim.eximus = false;
  await runSim();
  const atGate = line();
  await runSim();
  return { beforeGate, atGate, after: line(), gate: NUDGE_AFTER, asked: localStorage.getItem("wfsim-asked") };
})()`);
check(`${tag} a reader short of the gate is never asked (negative control)`,
  nudge.beforeGate.asked === null && nudge.beforeGate.drawn === 0, JSON.stringify(nudge));
check(`${tag} ...the ask is drawn under the result that reaches it`,
  nudge.atGate === 1 && nudge.asked === "1", JSON.stringify(nudge));
check(`${tag} ...and never a second time`,
  nudge.after === 0, JSON.stringify(nudge));

// ---------------------------------------------------------------------------
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
    what: [...el.querySelectorAll(".sup-card .sup-what")].map((s) => s.textContent.trim()),
  };
})()`);
const han = (s) => /[一-鿿]/.test(s);
check(`${tag} zh the sections are named in Chinese`,
  zh.heads.length >= 3 && zh.heads.every(han), JSON.stringify(zh.heads));
check(`${tag} zh ...so are the figures' labels`,
  zh.labels.length >= 3 && zh.labels.every(han), JSON.stringify(zh.labels));
check(`${tag} zh ...and the line the build fills in`,
  han(zh.built) && /\d/.test(zh.built), zh.built);
check(`${tag} zh ...and what each channel is`,
  zh.what.length > 0 && zh.what.every(han), JSON.stringify(zh.what));

await sleep(200);
await finish("the support page counts what it claims, and keeps what is the reader's");
