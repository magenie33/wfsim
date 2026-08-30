// PROGRESS WHERE THE WORK IS BEING READ — the THIRTY-SEVENTH check.
//
// The quick calc counted itself in exactly one place: the panel at the top of
// the page. The LIST it was ranking — ninety mods, tens of seconds at a real
// run count — said nothing at all, and a list that does not move is read as
// broken rather than as busy.
//
// The per-row "…" chip was already there and is not the same claim: it says
// THIS row has no answer yet, and says nothing about whether anything is still
// happening. So this asserts the STRIP — a bar, a count, and the fact that it
// is inside the list rather than beside it.
//
// IT ALSO ASSERTS THE ABSENCE. A bar that is always there is furniture; one
// that appears when work starts and leaves when it stops is information, and
// the second half is the half a "does it render" test never checks.

import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  localStorage.clear();
  history.pushState({}, '', '/weapons/Braton_Prime'); route(); await sleep(3500);

  // A RUN COUNT HIGH ENOUGH THAT THE SCAN IS WATCHABLE. At the floor it can
  // finish inside one repaint and the strip would never be sampled — which
  // would make this check pass on a page that never draws it.
  gainPrefs = { ...gainPrefs, on: true, runs: 200 };

  const strip = (sel) => {
    const el = document.querySelector(sel + ' .scan-strip');
    if (!el) return null;
    const bar = el.querySelector('.scan-bar > i');
    return {
      text: el.querySelector('.scan-txt').textContent.trim(),
      width: bar ? bar.style.width : '',
      sticky: getComputedStyle(el).position,
      // INSIDE the list, not merely near it: a strip that is a sibling of the
      // menu would scroll away with the rows it is about.
      inList: !!el.closest(sel),
    };
  };

  // ---- 1. THE MOD PICKER -------------------------------------------------
  // THROUGH THE SLOT ITSELF. openPicker positions the popover against an
  // anchor, so calling it without one throws — the app never does, because the
  // slot is always what opens it (slotEl is the app's own accessor).
  openPicker(0, slotEl(0));
  await sleep(400);
  out.pickerOpen = !document.getElementById('mod-popover').hidden;
  // Sample while it runs. The scan repaints every 250 ms, so a handful of
  // looks over a couple of seconds catches it however fast the machine is.
  let seen = null;
  for (let i = 0; i < 40 && !seen; i++) {
    seen = strip('#mod-menu');
    if (!seen) await sleep(150);
  }
  out.mods = seen;
  out.modsRunning = gainScan.running;

  // …AND IT GOES AWAY. A bar that is always there is furniture.
  for (let i = 0; i < 200 && gainScan.running; i++) await sleep(200);
  renderMenu(pickerSlot, '');
  await sleep(200);
  out.modsAfter = strip('#mod-menu');
  out.chipsAfter = document.querySelectorAll('#mod-menu .gainchip').length;
  closePopovers();
  await sleep(200);

  // ---- 2. THE RANKED SLOTS -----------------------------------------------
  // THE EVOLUTION TIERS, THE VALENCE ELEMENT AND A KITGUN'S PARTS are one
  // control now (rankedSlot) reached by one path: a card with a
  // ⋯ that opens the ranked list, and the scan fires when the list OPENS. So
  // they are watched the way the mod picker above is, through the control the
  // reader actually uses, rather than through a render.
  //
  // This half must not read the strip out of #evo-rows after calling
  // refreshGains(), which was right while the tiers were rows of chips that
  // ranked themselves unopened. It went red the day they became cards, which
  // is the check doing its job: axis=mods running=false, because nothing
  // scans an evolution any more until somebody looks at one.
  const openSlot = async (id) => {
    closePopovers(); await sleep(200);
    const el = document.querySelector('[data-slot="' + id + '"]');
    if (!el) return false;
    // A FILLED card opens its list through the ⋯ menu's Swap; an EMPTY one is
    // the button itself. Both are the app's own bindings, not a shortcut.
    const dots = el.querySelector('.dots');
    if (dots) {
      dots.click(); await sleep(250);
      const sw = document.querySelector('#slot-menu [data-a="swap"]');
      if (sw) sw.click();
    } else el.click();
    // NO SETTLING SLEEP. The strip is what is being sampled, and the scan now
    // reports itself the instant it starts — so a cheap axis can open, rank and
    // clear inside a couple of hundred milliseconds. A fixed wait before the
    // first look is a race the sampler loses; the poll waits for the popover
    // itself instead.
    for (let i = 0; i < 60; i++) {
      if (!document.getElementById('dd-popover').hidden) return true;
      await sleep(20);
    }
    return false;
  };
  const pickFirst = async () => {
    const o = document.querySelector('#dd-menu .opt[data-v]:not(.dis)');
    if (o) o.click();
    await sleep(700);
  };
  // A SCENARIO OF YOUR OWN FIRST — check_arena's lesson, and this check needed
  // it for the same reason. The app lands a first-time visitor on the OFFICIAL
  // ruler, whose fight is PINNED, so every edit below was silently ignored and
  // the scans ran on a single standing target: 23 candidates in about seven
  // hundred milliseconds, which is a race against the sampler rather than a
  // fight anybody watches. Scenarios are shared across the roster, so one is
  // made once and carries to every weapon here.
  const ownScenario = async () => {
    const own = () => typeof officialScenarioActive === 'function'
      && !officialScenarioActive();
    if (own()) return true;
    const bar = document.querySelector('#preset-bar-simulator-scenarios');
    const add = bar && bar.querySelector('.pchip.add');
    if (add) { add.click(); await sleep(1800); }
    return own();
  };
  const heavy = async () => {
    out.editable = await ownScenario();
    // A FIGHT HEAVY ENOUGH TO WATCH. A one-body Torid ranks a dozen evolutions
    // in ~360 ms — faster than the 250 ms repaint throttle — so nothing is ever
    // drawn and this half would pass on a page that never draws the strip. A
    // crowd is also the case the report was about: the expensive fight is the
    // one that looks frozen, and the cheap one never needed a bar.
    gainPrefs = { ...gainPrefs, on: true, runs: 250 };
    sim.duration = 90;
    sim.formation = Array.from({ length: 24 }, (_, i) =>
      ({ at: [(i % 6) * 3 - 7.5, 0.4 + Math.floor(i / 6) * 3] }));
    markScenarioDirty();
  };
  const watch = async (id) => {
    const rec = { opened: await openSlot(id), strip: null };
    if (!rec.opened) return rec;
    for (let i = 0; i < 500 && !rec.strip; i++) {
      rec.strip = strip('#dd-menu');
      if (!rec.strip) await sleep(30);
    }
    // THE ROWS THE READER IS LOOKING AT, counted from the list itself.
    rec.rows = document.querySelectorAll('#dd-menu .opt[data-v]').length;
    rec.axis = (gainScan.axis || {}).kind;
    rec.why = 'running=' + gainScan.running + ' fresh=' + (gainScan.key === gainKey())
      + ' ' + gainScan.done + '/' + gainScan.total
      + ' open=' + !document.getElementById('dd-popover').hidden
      + ' bodies=' + ((sim.formation || []).length);
    return rec;
  };

  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(3500);
  await heavy(); await sleep(400);
  // CLIMB THE LADDER THROUGH THE CONTROL. A tier is locked until the one below
  // it is installed and EVERY weapon's tier 1 offers exactly one option (the
  // Incarnon form), so the first list with anything to rank is further up —
  // and tier 3 is also where the denominator bug shows on this axis: one scan
  // covers every open tier, so its total counts tier 2's leftover candidate
  // that tier 3's list does not show.
  if (await openSlot('dd-evo-1')) await pickFirst();
  if (await openSlot('dd-evo-2')) await pickFirst();
  out.evo = await watch('dd-evo-3');

  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);
  await heavy(); await sleep(400);
  out.valence = await watch('dd-valence');

  // THE SHARPEST DENOMINATOR CASE, and the one that produced this rule: a
  // Kitgun's grip list is five rows and reported 12/23, because ONE scan
  // covers the grip and the loader and only one of the two is ever open.
  history.pushState({}, '', '/weapons/Tombfinger'); route(); await sleep(3500);
  await heavy(); await sleep(400);
  out.grip = await watch('dd-grip');
  return out;
})()`);

check("the mod picker opened", r.pickerOpen === true);
check("the mod list carries a progress strip while it ranks",
  !!r.mods, JSON.stringify(r.mods));
check("...with a COUNT, not just a bar",
  !!r.mods && /\d+\s*\/\s*\d+/.test(r.mods.text), r.mods && r.mods.text);
check("...and a bar that has a width",
  !!r.mods && /%$/.test(r.mods.width), r.mods && r.mods.width);
check("...it is INSIDE the list it is about", !!r.mods && r.mods.inList === true);
check("...and sticky, so it survives scrolling the rows it is about",
  !!r.mods && r.mods.sticky === "sticky", r.mods && r.mods.sticky);

check("it LEAVES when the scan finishes", r.modsAfter === null,
  JSON.stringify(r.modsAfter));
check("...and the list it leaves behind is a ranked one",
  r.chipsAfter > 3, `${r.chipsAfter} chips`);

check("the run edits a fight of its own, not the pinned official ruler",
  r.editable === true,
  "an official ruler ignores every edit below, so the scans would race the sampler");

// ---- the ranked slots carry the same component, and count their own list ----
//
// ONE CONTROL, ONE COMPONENT. The reader is told these are
// all quantifiable the same way, so a strip on the mod picker and silence on
// the evolution tiers is the page contradicting its own claim.
for (const [name, k, want] of [["evolution tier", "evo", "evo"],
                               ["valence element", "valence", "valence"],
                               ["kitgun grip", "grip", "assembly"]]) {
  const x = r[k] || {};
  check(`the ${name} list opens through its own control`, x.opened === true,
    JSON.stringify(x));
  check(`...and carries the SAME strip the mod picker does`, !!x.strip,
    `axis=${x.axis} ${x.why} ${JSON.stringify(x.strip)}`);
  if (!x.strip) continue;
  check(`...it ranks the ${want} axis`, x.axis === want, `axis=${x.axis}`);
  check(`...with a COUNT, not just a bar`, /\d+\s*\/\s*\d+/.test(x.strip.text),
    x.strip.text);
  check(`...and a bar that has a width`, /%$/.test(x.strip.width), x.strip.width);
  check(`...inside the list it is about`, x.strip.inList === true);
  // THE DENOMINATOR IS THIS LIST'S OWN, which is the property the whole strip
  // rests on: a count a reader cannot check against the rows in front of them
  // is not a progress report, it is a number. `gainScan.total` is how many
  // SIMULATIONS the scan will run — it carries the refine pass, and on an axis
  // whose candidates are split across several lists it carries the other lists
  // too. Both read as a denominator larger than the list: a Kitgun grip's five
  // rows said 23, and a ninety-mod picker said 98.
  //
  // Asserted as "no more than the rows on screen" rather than as an exact
  // figure, because the CURRENT option is a row that never gets an answer and
  // how many of those a list has is the axis's business, not this check's.
  const m = x.strip.text.match(/(\d+)\s*\/\s*(\d+)/) || [];
  const done = Number(m[1]), total = Number(m[2]);
  check(`...whose denominator is no larger than the ${x.rows} rows on screen`,
    total >= 1 && total <= x.rows, `${done}/${total} over ${x.rows} rows`);
  check(`...and counts up to it, never past it`, done <= total, `${done}/${total}`);
}

await finish("progress where the work is being read");
