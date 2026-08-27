// THE NUMBERS A FIGHT POPS — the THIRTY-SIXTH check.
//
// Every other thing the replay draws is a CURVE: a pool falling, a stack count,
// a running total. This is the one output that is an EVENT — a discrete number
// that happened at a place at a time — and it is the only view in the app where
// "one hit for 400,000" and "twenty for 20,000" look different rather than
// reading identically as an average.
//
// WHICH IS ALSO WHY IT IS THE EASIEST THING HERE TO FAKE. A layer that floated
// plausible numbers over the bodies would look exactly right and mean nothing.
//
// SO THE PROPERTY IS ONE-TO-ONE, AND IT IS CHECKED BY NAME (owner, 2026-08-27).
// The engine used to carry a `Replay.pops` buffer beside the combat record —
// the same nine damage sites written down twice, capped by two different rules
// — so a number could float over a body with no row to explain it and a row
// could name a number that never appeared. Both were "the engine's", so a
// check that only asked "is this text one the engine produced" passed on it.
// There is ONE stream now: every drawn number carries `data-rpevent`, the id of
// the row it IS, and this asserts that the row exists, that its effective
// damage is the text on screen, and that it belongs to the frame being shown.
// An overlay drawing plausible numbers now has to forge an id as well.
//
// THE CAP IS PART OF THE FEATURE and it moved with the stream: it is a DISPLAY
// decision now, twelve a frame with the biggest kept, made where the numbers
// are drawn. A frame that dropped any must SAY so — a cap nobody is told about
// reads as "that is everyone".

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
  // reaches the neighbours. Without something that spreads only the aimed body
  // takes damage, and the assertions below about numbers landing on more than
  // one body would have nothing to be about. Status mods too, so the fight
  // produces more than one KIND of number.
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

  // ---- 0. THE REPLAY NO LONGER CARRIES A SECOND ACCOUNT -------------------
  // The negative control for the whole change: if \`pops\` came back on the wire
  // there would be two lists again, and every assertion below would pass just
  // as happily against the wrong one.
  out.noPopBuffer = rp.pops === undefined;

  // ---- 1. REACHING FOR THE REPLAY ASKS FOR THE RECORD ---------------------
  // The numbers ARE the record, so a scrub is what fetches it — deliberately
  // not the result, whose stream would be megabytes on a dense build.
  const scrub = document.getElementById('rp-scrub');
  out.beforeScrub = !(recordState && recordState.events);
  scrub.value = '0';
  scrub.dispatchEvent(new Event('input'));
  for (let i = 0; i < 60 && !(recordState && recordState.events); i++) await sleep(500);
  out.recordArrived = !!(recordState && recordState.events);
  if (!out.recordArrived) return out;

  const fs = rp.frame_seconds;
  const nf = rp.t.length;
  const frameOf = (t) => Math.min(nf - 1, Math.max(0, Math.ceil(t / fs) - 1));
  const rows = recordState.events.filter((e) =>
    e.kind === 'damage' && e.body != null && e.effective > 0);
  out.rows = rows.length;
  out.dropped = recordState.dropped || 0;
  out.kinds = [...new Set(rows.map((e) => e.pop_kind))].sort();
  out.bodies = [...new Set(rows.map((e) => e.body))].sort((a, b) => a - b);
  // EVERY NUMBER IS POSITIVE AND INSIDE THE CLOCK. A zero would be a number
  // nobody would see and a negative one would be a bug wearing a number's
  // clothes.
  out.allPositive = rows.every((e) => e.effective > 0);
  out.allInClock = rows.every((e) => e.t >= 0 && e.t <= rp.t[nf - 1] + 1e-6);
  // …AND EVERY ROW HAS A NAME OF ITS OWN, which is what the overlay points at.
  out.idsUnique = new Set(rows.map((e) => e.id)).size === rows.length;

  // ---- 2. THE PAGE DRAWS THEM, AND EACH ONE NAMES ITS ROW -----------------
  // Scrub to the busiest frame, which is the one that exercises both the
  // layout and the cap.
  const per = new Map();
  for (const e of rows) {
    const i = frameOf(e.t);
    per.set(i, (per.get(i) || 0) + 1);
  }
  let best = 0, bestN = 0;
  for (const [i, n] of per) if (n > bestN) { best = i; bestN = n; }
  out.bestFrame = best;
  out.bestCount = bestN;
  scrub.value = String(best);
  scrub.dispatchEvent(new Event('input'));
  await sleep(400);
  const layer = document.querySelector('#rp-scene .rp-pops');
  out.hasLayer = !!layer;
  const drawn = layer ? [...layer.querySelectorAll('.rp-pop')] : [];
  out.drawn = drawn.length;
  const byId = new Map(rows.map((e) => [String(e.id), e]));
  const numbers = drawn.filter((el) => !el.classList.contains('p-more'));
  out.drawnNumbers = numbers.length;
  // ONE-TO-ONE, BY NAME. Not "the text is a number the engine produced" — the
  // id has to resolve to a row, that row's damage has to be the text, and the
  // row has to belong to the frame on screen.
  out.everyDrawnNamesItsRow = numbers.length > 0 && numbers.every((el) => {
    const e = byId.get(el.dataset.rpevent || '');
    return !!e
      && el.textContent === Math.round(e.effective).toLocaleString()
      && frameOf(e.t) === best;
  });
  // …AND NO ROW IS DRAWN TWICE, which is the other direction of the same claim.
  out.noDuplicates =
    new Set(numbers.map((el) => el.dataset.rpevent)).size === numbers.length;
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
  let capped = -1;
  for (const [i, n] of per) if (n > 12) { capped = i; break; }
  out.hasCappedFrame = capped >= 0;
  out.perFrameMax = Math.max(0, ...[...per.values()]);
  if (capped >= 0) {
    scrub.value = String(capped);
    scrub.dispatchEvent(new Event('input'));
    await sleep(400);
    const shown = document.querySelectorAll('#rp-scene .rp-pops .rp-pop:not(.p-more)');
    out.cappedDrawn = shown.length;
    out.saysMore = !!document.querySelector('#rp-scene .rp-pops .p-more');
  }

  // ---- 4. SCRUBBING REPLACES, PLAYING ACCUMULATES -------------------------
  // Landing on a frame twice must not double what is on screen; without that
  // distinction a scrub either piles up hundreds of numbers or shows none.
  scrub.value = String(best);
  scrub.dispatchEvent(new Event('input'));
  await sleep(300);
  const once = document.querySelectorAll('#rp-scene .rp-pops .rp-pop').length;
  scrub.dispatchEvent(new Event('input'));
  await sleep(300);
  out.scrubReplaces =
    document.querySelectorAll('#rp-scene .rp-pops .rp-pop').length === once;

  // ---- 5. THE TABLE BELOW IS THE SAME STREAM ------------------------------
  // The claim this whole thing rests on, asserted rather than assumed: the
  // panel that LISTS the rows and the layer that DRAWS them read one array.
  const host = document.getElementById('rec-host');
  // The table shows ONE body at a time — a reload belongs to nobody and a
  // per-enemy view is a filter over the one stream — so the comparison is
  // against the body whose chip is selected.
  const sel = host && host.querySelector('.pchip.sel[data-recbody]');
  out.tablePick = sel ? Number(sel.dataset.recbody) : null;
  out.tableRows = host ? host.querySelectorAll('tr.rec-dmg').length : -1;
  out.tableIsTheStream = out.tablePick != null && out.tableRows === recordState.events
    .filter((e) => e.kind === 'damage' && e.body === out.tablePick).length;
  // …AND THE TWO VIEWS MEET ON THE ID. Every number drawn over the body the
  // table is showing must be findable as a LINE in it, by name.
  const mine = numbers.filter((el) => {
    const e = byId.get(el.dataset.rpevent || '');
    return e && e.body === out.tablePick;
  });
  out.drawnForPick = mine.length;
  out.tableHasEveryDrawn = mine.length > 0 && mine.every((el) =>
    !!host.querySelector('tr[data-recevent="' + el.dataset.rpevent + '"]'));
  return out;
})()`);

check("the run produced a replay", r.hasReplay === true);
check("the replay carries NO second account of the numbers", r.noPopBuffer === true,
  "rp.pops must be gone — one stream, not two");
check("the record is not fetched with the result", r.beforeScrub === true);
check("...and reaching for the replay asks for it", r.recordArrived === true);
check("...and the fight wrote rows", r.rows > 20, `${r.rows} rows`);
check("...every one positive", r.allPositive === true);
check("...every one inside the clock", r.allInClock === true);
check("...every one with an id of its own", r.idsUnique === true);
check("more than one KIND of number", (r.kinds || []).length >= 2, (r.kinds || []).join(","));
check("...and they land on more than one body",
  (r.bodies || []).length >= 2, `bodies ${(r.bodies || []).join(",")}`);

check("the scene has a pop layer", r.hasLayer === true,
  `scene=${r.hasScene} rollCall=${r.rollCall} bodies=${(r.bodies || []).join(",")}`);
check("...it draws this frame's numbers", r.drawnNumbers > 0,
  `${r.drawnNumbers} of ${r.bestCount}`);
check("...and every one of them NAMES the record row it is",
  r.everyDrawnNamesItsRow === true,
  "id resolves, damage matches, frame matches");
check("...and no row is drawn twice", r.noDuplicates === true);
// THE OTHER DIRECTION, and the one a shrunken list slips past. "Every number
// on screen is a row" is satisfied perfectly by drawing ONE of them, which is
// the shape of the bug this whole change was about: two lists over one fight,
// where the smaller one looked right. So the frame's rows must ALL be drawn,
// up to the display cap and not one short of it.
check("...and every row in the frame is drawn, up to the cap",
  r.drawnNumbers === Math.min(12, r.bestCount),
  `${r.drawnNumbers} drawn of ${r.bestCount} rows in frame ${r.bestFrame}`);
check("...placed inside the scene", r.allInside === true);
check("...and the layer never eats a click", r.layerIgnoresPointer === true);

check("no frame shows more than twelve",
  !r.hasCappedFrame || r.cappedDrawn <= 12, `${r.cappedDrawn}`);
check("a frame that dropped numbers SAYS so",
  !r.hasCappedFrame || r.saysMore === true,
  r.hasCappedFrame ? `busiest frame ${r.perFrameMax}` : "no frame hit the cap in this fight");
check("scrubbing REPLACES rather than piling up", r.scrubReplaces === true);
check("the table below lists the SAME stream the scene draws",
  r.tableIsTheStream === true,
  `${r.tableRows} rows in the table for body ${r.tablePick}`);
check("...and every number on the scene is a LINE in it, by id",
  r.tableHasEveryDrawn === true, `${r.drawnForPick} drawn for that body`);

// ---- AND A SINGLE-TARGET FIGHT IS THE SAME PANEL ---------------------------
//
// It was not. The whole block — scene, roll call, and the layer the numbers
// float in — was gated on there being more than one body, on the reasoning
// that a one-row roll call is not worth a picture. So the one output that is a
// discrete thing that happened at a place at a time was invisible in the
// COMMONEST fight this app runs, and nothing said why (owner, 2026-08-24).
//
// The special case was the mistake rather than the missing feature: a fight is
// a scene with N bodies and N=1 is just N=1 — it has a shooter, a target, a
// distance and an aim point, which is exactly what the scenario's own canvas
// draws for it. This asserts the panel does not change shape with the body
// count, which is the property, not the pops.
const one = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/Braton_Prime/simulator'); route();
  await sleep(6000);
  sim.formation = [];
  sim.runs = 20;
  document.getElementById('run-sim').click();
  for (let i = 0; i < 90 && !document.getElementById('rp-scene'); i++) await sleep(700);
  const sc = document.getElementById('rp-scene');
  const rp = (shownResult && shownResult.r && shownResult.r.replay) || {};
  const out = {
    bodies: ((shownResult && shownResult.r && shownResult.r.bodies) || []).length,
    scene: !!sc,
    svg: !!(sc && sc.querySelector('.ar-svg')),
    roll: document.querySelectorAll('.rp-roll tr').length,
  };
  const scrub = document.getElementById('rp-scrub');
  scrub.value = '0';
  scrub.dispatchEvent(new Event('input'));
  for (let i = 0; i < 60 && !(recordState && recordState.events); i++) await sleep(500);
  const rows = (recordState && recordState.events || [])
    .filter((e) => e.kind === 'damage' && e.body != null && e.effective > 0);
  // A FRAME THAT ACTUALLY POPPED, found rather than guessed: most frames of a
  // 20 s fight are between shots.
  const fs = rp.frame_seconds, nf = (rp.t || []).length;
  const frameOf = (t) => Math.min(nf - 1, Math.max(0, Math.ceil(t / fs) - 1));
  const idx = rows.length ? frameOf(rows[0].t) : -1;
  out.hasRows = rows.length > 0;
  if (sc && idx >= 0) {
    scrub.value = String(idx);
    scrub.dispatchEvent(new Event('input'));
    await sleep(600);
    out.drawn = sc.querySelectorAll('.rp-pop').length;
    out.engineCount = rows.filter((e) => frameOf(e.t) === idx).length;
  }
  return out;
})()`);

check("a single-target fight is one body", one.bodies === 1, String(one.bodies));
check("...and it draws the same scene", one.scene === true && one.svg === true,
  JSON.stringify({ scene: one.scene, svg: one.svg }));
check("...and the same roll call, one row long", one.roll === 1, String(one.roll));
check("...and it pops numbers like any other fight",
  one.hasRows === true && one.drawn > 0,
  `${one.drawn} drawn of ${one.engineCount} rows in that frame`);

await finish("the numbers a fight pops");
