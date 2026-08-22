// PROGRESS WHERE THE WORK IS BEING READ — the THIRTY-SEVENTH check.
//
// The quick calc counted itself in exactly one place: the panel at the top of
// the page. The LIST it was ranking — ninety mods, tens of seconds at a real
// run count — said nothing at all, and a list that does not move is read as
// broken rather than as busy (owner, 2026-08-22).
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

  // ---- 2. THE EVOLUTION TIERS -------------------------------------------
  // A different axis, a different list, the same component — and the evolution
  // rows scan without being opened, which is what makes them the case where a
  // silent list is most confusing.
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(3500);
  // THE RUN COUNT IS NOT IN THE SCAN'S KEY — it is the reader's precision, not
  // a property of the fight (AGENTS.md) — so raising it does not invalidate a
  // finished scan. The FIGHT is what the key carries, so the fight is what has
  // to move: set the count first, then change the fight, then re-ask.
  gainPrefs = { ...gainPrefs, on: true, runs: 400 };
  // A FIGHT HEAVY ENOUGH TO WATCH. The Torid's evolution axis is a dozen
  // candidates and a one-body fight ranks them in ~360 ms — faster than the
  // 250 ms repaint throttle, so nothing is ever drawn and this half of the
  // check would pass on a page that never draws the strip at all. A crowd is
  // also the case the owner reported: it is the expensive fight that looks
  // frozen, and the cheap one never needed a bar.
  sim.duration = 90;
  sim.formation = Array.from({ length: 24 }, (_, i) =>
    ({ at: [(i % 6) * 3 - 7.5, 0.4 + Math.floor(i / 6) * 3] }));
  markScenarioDirty();
  refreshGains();
  // Wait for the EVO axis specifically. The page may still be finishing another
  // one, and sampling whatever is running would let a mod scan's strip pass for
  // an evolution scan's.
  let evo = null;
  for (let i = 0; i < 200 && !evo; i++) {
    if (gainScan.running && (gainScan.axis || {}).kind === 'evo') evo = strip('#evo-rows');
    if (!evo) await sleep(150);
  }
  out.evo = evo;
  out.evoAxis = (gainScan.axis || {}).kind;
  out.evoRunning = gainScan.running;
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

check("the evolution rows carry the same strip", !!r.evo, `axis=${r.evoAxis} running=${r.evoRunning} ${JSON.stringify(r.evo)}`);
check("...with their own count", !!r.evo && /\d+\s*\/\s*\d+/.test(r.evo.text),
  r.evo && r.evo.text);

await finish("progress where the work is being read");
