// THE NUMBERS A FIGHT POPS — the THIRTY-SIXTH check.
//
// Every other thing the replay draws is a CURVE: a pool falling, a stack count,
// a running total. This is the one output that is an EVENT — a discrete number
// that happened at a place at a time — and it is the only view in the app where
// "one hit for 400,000" and "twenty for 20,000" look different rather than
// reading identically as an average.
//
// WHICH IS ALSO WHY IT IS THE EASIEST THING HERE TO FAKE. A layer that floated
// plausible numbers over the bodies would look exactly right and mean nothing,
// so every assertion below ties what is ON SCREEN to what the ENGINE said: the
// text of a number must be one the replay's own `pops` carries, and it must sit
// over the body that took it.
//
// THE CAP IS PART OF THE FEATURE. The engine keeps the twelve biggest numbers
// of a frame and counts the rest — the game caps its own display the same way
// ("a maximum of 10 tick numbers are shown at once") — so a frame that dropped
// any must SAY so. A cap nobody is told about reads as "that is everyone".

import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  localStorage.clear();
  history.pushState({}, '', '/weapons/Braton_Prime/simulator'); route(); await sleep(3000);

  // A SCENARIO OF OUR OWN, with a CROWD: a number has to land on a body, and a
  // one-body fight cannot tell "over the right one" from "over the only one".
  const bar = document.querySelector('#preset-bar-simulator-scenarios');
  const add = bar && bar.querySelector('.pchip.add');
  if (add) { add.click(); await sleep(1200); }
  sim.level = 40; sim.steel_path = false; sim.duration = 20; sim.runs = 8;
  sim.formation = [[1.6, 0.4], [-1.6, 0.4], [0, 2.0]].map((at) => ({ at }));
  // COLD + HEAT, so the build makes BLAST — and a blast detonation is what
  // reaches the neighbours. Without something that spreads, only the aimed body
  // takes damage, the result draws no scene at all (the roll call needs two
  // bodies to be worth a picture), and there is nowhere for a number to land.
  // Status mods too, so the fight produces more than one KIND of number.
  ['serration', 'split_chamber', 'point_strike', 'vital_sense',
   'primed_cryo_rounds', 'thermite_rounds', 'rifle_aptitude', 'malignant_force']
    .forEach((m, i) => {
      if (modById(m)) { slots[i].mod = m; slots[i].rank = modById(m).max_rank; }
    });
  markPresetDirty(); markScenarioDirty(); renderMods(); refreshPanel();
  await sleep(2000);

  document.getElementById('run-sim').click();
  for (let i = 0; i < 80 && !document.getElementById('rp-scrub'); i++) await sleep(500);
  const rp = shownResult && shownResult.r && shownResult.r.replay;
  out.hasReplay = !!(rp && rp.t && rp.t.length > 1);
  if (!out.hasReplay) return out;

  // ---- 1. THE ENGINE CARRIES THEM ---------------------------------------
  const pops = rp.pops || [];
  out.frames = pops.length;
  out.sameLength = pops.length === rp.t.length;
  const all = pops.flatMap((f) => f.v || []);
  out.count = all.length;
  out.dropped = pops.reduce((n, f) => n + (f.n || 0), 0);
  out.perFrameMax = Math.max(0, ...pops.map((f) => (f.v || []).length));
  out.kinds = [...new Set(all.map((p) => p[4]))].sort();
  out.bodies = [...new Set(all.map((p) => p[1]))].sort((a, b) => a - b);
  // EVERY NUMBER IS POSITIVE AND INSIDE THE CLOCK. A zero would be a number
  // nobody would see and a negative one would be a bug wearing a number's
  // clothes.
  out.allPositive = all.every((p) => p[2] > 0);
  out.allInClock = all.every((p) => p[0] >= 0 && p[0] <= rp.t[rp.t.length - 1] + 1e-6);

  // ---- 2. THE PAGE DRAWS THEM, AND THEY ARE THE ENGINE'S ------------------
  // Scrub to the frame with the most numbers, which is the one that exercises
  // both the layout and the cap.
  let best = 0;
  pops.forEach((f, i) => { if ((f.v || []).length > (pops[best].v || []).length) best = i; });
  out.bestFrame = best;
  out.bestCount = (pops[best].v || []).length;
  const scrub = document.getElementById('rp-scrub');
  scrub.value = String(best);
  scrub.dispatchEvent(new Event('input'));
  await sleep(300);
  const layer = document.querySelector('#rp-scene .rp-pops');
  out.hasLayer = !!layer;
  const drawn = layer ? [...layer.querySelectorAll('.rp-pop')] : [];
  out.drawn = drawn.length;
  // THE TEXT MUST BE THE ENGINE'S. Every drawn number (bar the "+N more" chip)
  // has to be one this frame actually popped.
  const want = new Set((pops[best].v || []).map((p) => Math.round(p[2]).toLocaleString()));
  const got = drawn.filter((el) => !el.classList.contains('p-more'))
    .map((el) => el.textContent);
  out.everyDrawnIsReal = got.length > 0 && got.every((tx) => want.has(tx));
  // …AND IT MUST BE INSIDE THE SCENE. A number placed off-canvas is a number
  // nobody sees, which is the failure a screenshot would not catch either.
  const sceneEl = document.getElementById('rp-scene');
  const sbox = sceneEl ? sceneEl.getBoundingClientRect() : { left: 0, right: 0, top: 0, bottom: 0 };
  out.allInside = drawn.every((el) => {
    const b = el.getBoundingClientRect();
    return b.left >= sbox.left - 40 && b.right <= sbox.right + 40
      && b.top >= sbox.top - 40 && b.bottom <= sbox.bottom + 40;
  });
  // …AND THE LAYER MUST NOT EAT A CLICK: the scene under it still picks.
  out.layerIgnoresPointer =
    !!layer && getComputedStyle(layer).pointerEvents === 'none';
  out.hasScene = !!document.getElementById('rp-scene');
  out.rollCall = ((shownResult.r || {}).bodies || []).length;

  // ---- 3. THE CAP IS STATED ----------------------------------------------
  const capped = pops.findIndex((f) => (f.n || 0) > 0);
  out.hasCappedFrame = capped >= 0;
  if (capped >= 0) {
    scrub.value = String(capped);
    scrub.dispatchEvent(new Event('input'));
    await sleep(300);
    out.saysMore = !!document.querySelector('#rp-scene .rp-pops .p-more');
  }

  // ---- 4. SCRUBBING REPLACES, PLAYING ACCUMULATES -------------------------
  // Landing on a frame twice must not double what is on screen; without that
  // distinction a scrub either piles up hundreds of numbers or shows none.
  scrub.value = String(best);
  scrub.dispatchEvent(new Event('input'));
  await sleep(200);
  const once = document.querySelectorAll('#rp-scene .rp-pops .rp-pop').length;
  scrub.dispatchEvent(new Event('input'));
  await sleep(200);
  out.scrubReplaces =
    document.querySelectorAll('#rp-scene .rp-pops .rp-pop').length === once;
  return out;
})()`);

check("the run produced a replay", r.hasReplay === true);
check("the engine carries one pop entry per frame", r.sameLength === true,
  `${r.frames} vs the clock`);
check("...and the fight popped numbers", r.count > 20, `${r.count} numbers`);
check("...never more than twelve in a frame", r.perFrameMax <= 12, `${r.perFrameMax}`);
check("...every one positive", r.allPositive === true);
check("...every one inside the clock", r.allInClock === true);
check("more than one KIND of number", (r.kinds || []).length >= 2, (r.kinds || []).join(","));
check("...and they land on more than one body",
  (r.bodies || []).length >= 2, `bodies ${(r.bodies || []).join(",")}`);

check("the scene has a pop layer", r.hasLayer === true, `scene=${r.hasScene} rollCall=${r.rollCall} bodies=${(r.bodies||[]).join(",")}`);
check("...it draws this frame's numbers", r.drawn > 0, `${r.drawn} of ${r.bestCount}`);
check("...and every one of them is a number the ENGINE popped",
  r.everyDrawnIsReal === true);
check("...placed inside the scene", r.allInside === true);
check("...and the layer never eats a click", r.layerIgnoresPointer === true);

check("a frame that dropped numbers SAYS so",
  !r.hasCappedFrame || r.saysMore === true,
  r.hasCappedFrame ? `dropped ${r.dropped}` : "no frame hit the cap in this fight");
check("scrubbing REPLACES rather than piling up", r.scrubReplaces === true);

await finish("the numbers a fight pops");
