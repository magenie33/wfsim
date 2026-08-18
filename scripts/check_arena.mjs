// THE ARENA IS A PLACE YOU CAN DRAG, and what you drag is what gets simulated.
//
// A fight is two bodies on a floor, so the panel draws two bodies on a floor
// and you move them with your finger (owner, 2026-08-15). The picture is not a
// decoration and that is the whole reason this check exists: a scene that
// looked right and did not reach `/api/simulate` would be the most convincing
// wrong thing on the page.
//
//   node scripts/check_arena.mjs
//
// Four claims, and the last two are the sharp ones:
//
//   · IT DRAWS — two bodies, a MUZZLE on the shooter's own circumference with
//     the arrow that says which way they face, and a distance label.
//   · DRAGGING MOVES THE FIGHT — the label, the scenario state and a real
//     `/api/simulate` in the shipping wasm build all follow the finger.
//   · BODIES DO NOT PASS THROUGH EACH OTHER, and CONTACT READS ZERO. Dragging
//     the enemy onto the player leaves their CENTRES 0.4 m apart — twice the
//     measured 0.2 m radius (M47) — and the number on screen is the GAP between
//     their surfaces, which is 0 (owner, 2026-08-16). What a player calls point
//     blank is zero; the 0.4 m is the model's business.
//   · …AND AT CONTACT NOTHING MISSES. The shot leaves the muzzle, one radius
//     forward, so its closest approach to the target's centre is `r·sin(θ) ≤ r`
//     for every θ. Asserted here for the weapon on screen and for the WHOLE
//     roster in `space`, where a cone is a number rather than a page.
//   · AN OFFICIAL RULER'S FIGHT DOES NOT MOVE. The benchmark pins its
//     distance, so the scene refuses the gesture there — and it has to refuse
//     it ITSELF, because the official lock disables inputs and the bodies are
//     SVG circles that sweep never reaches.
//   · THE CANVAS IS THE ONLY PLACE A POSITION IS SET (owner, 2026-08-16). The
//     typed Distance box is gone; the shortcuts that replaced it live INSIDE
//     the scene, and a quick-set moves the target ALONG the line it already
//     stands on rather than snapping it to an axis — same rule the drag obeys,
//     because they move the same body.
//
// …and the negative control: the OPTIMIZER draws the same scene read-only,
// because it runs the simulator's fight and a preset is edited in one place.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const out = {};
  localStorage.clear();
  history.pushState({}, '', '/weapons/Braton/simulator'); route(); await sleep(3000);

  // A SCENARIO OF YOUR OWN FIRST. The app lands a first-time visitor on the
  // OFFICIAL ruler, whose fight is pinned and therefore not draggable — which
  // is asserted at the end. Everything before it needs an editable fight, so
  // the run starts by making one from the preset bar's "+ new".
  const bar0 = document.querySelector('#preset-bar-simulator-scenarios');
  const add = bar0 && bar0.querySelector('.pchip.add');
  if (add) { add.click(); await sleep(1500); }
  out.startedEditable = typeof officialScenarioActive === 'function' && !officialScenarioActive();

  // THE SCENE IS A CANVAS as of 2026-08-18, so there is no circle to grab and
  // no label element to read. Two things replace them, and both are STRONGER
  // than what they replace:
  //
  //   · at(i) asks the scene itself where it drew body i (host.__arena.px
  //     is the renderer's own metres-to-pixels map), so a drag starts exactly
  //     where the body is on screen rather than where a DOM box says it is.
  //   · ink(x, y) reads the PIXELS BACK. "It is drawn" used to mean "an
  //     element exists"; on a canvas it can mean the thing a reader actually
  //     sees, which is a better question to be asking.
  const cvEl = () => document.querySelector('#sim-target-arena .arc-cv');
  const arena = () => document.querySelector('#sim-target-arena').__arena;
  const at = (i) => {
    const b = i === 0 ? sim.target_at : sim.formation[i - 1].at;
    const [x, y] = arena().px(b);
    const box = cvEl().getBoundingClientRect();
    return [box.left + x, box.top + y];
  };
  const atPoint = (p) => {
    const [x, y] = arena().px(p);
    const box = cvEl().getBoundingClientRect();
    return [box.left + x, box.top + y];
  };
  // Anything that is not the floor. The floor is one flat colour, so a pixel
  // that differs from the corner is something the renderer put there.
  const ink = (cx, cy) => {
    const c = cvEl(), box = c.getBoundingClientRect();
    const g = c.getContext('2d');
    const sx = (cx - box.left) * (c.width / box.width);
    const sy = (cy - box.top) * (c.height / box.height);
    const bg = g.getImageData(2, 2, 1, 1).data;
    const px = g.getImageData(Math.round(sx), Math.round(sy), 1, 1).data;
    return Math.abs(px[0] - bg[0]) + Math.abs(px[1] - bg[1]) + Math.abs(px[2] - bg[2]) > 12;
  };
  const dist = () => {
    const t = document.querySelector('#sim-target-arena .arc-read');
    const m = t && t.textContent.match(/([0-9.]+) m/);
    return m ? parseFloat(m[1]) : null;
  };
  // The model holds CENTRES; the page shows the GAP. Both, so the check can
  // say which one an assertion is about.
  const state = () => Math.hypot(sim.target_at[0] - sim.player_at[0],
                                 sim.target_at[1] - sim.player_at[1]);
  const gap = () => Math.max(0, state() - 0.4);

  // 1. IT DRAWS, and it starts at contact.
  out.drew = !!cvEl();
  out.bodies = arenaBodies(sim).length + 1;   // the enemies, and you
  // …and both of them are INK on the floor, at the places the scene says it
  // put them. An element could exist and be drawn nowhere; a pixel cannot.
  out.bodyInk = ink(...at(0));
  out.youInk = ink(...atPoint(sim.player_at));
  out.startLabel = dist();
  out.startState = state();
  out.startGap = gap();
  // THE MUZZLE IS DRAWN, and it sits on the shooter's own circumference.
  // THE MUZZLE IS ON THE SHOOTER'S OWN CIRCUMFERENCE, facing the aim, and both
  // halves are read off the picture: ink one body radius along the aim line
  // from the player's centre, and ink again further along it where the facing
  // arrow reaches. Nothing is drawn a radius BEHIND, which is the control that
  // stops "the player is a big blob" from passing this.
  {
    const a = arenaAim(sim), you = sim.player_at;
    const d = Math.hypot(a[0] - you[0], a[1] - you[1]) || 1;
    const u = [(a[0] - you[0]) / d, (a[1] - you[1]) / d];
    const along = (m) => atPoint([you[0] + u[0] * m, you[1] + u[1] * m]);
    out.muzzle = ink(...along(0.2));
    out.facing = ink(...along(0.55));
  }

  // 2. DRAGGING MOVES THE FIGHT. Pointer events on the enemy body, in the
  //    SVG's own pixel space, so this is the gesture a finger makes.
  const drag = (i, dx, dy) => {
    const c = cvEl();
    c.setPointerCapture = () => {};
    const [x, y] = at(i);
    const ev = (type, cx, cy) => c.dispatchEvent(new PointerEvent(type,
      { clientX: cx, clientY: cy, bubbles: true, cancelable: true, pointerId: 1, button: 0 }));
    ev('pointerdown', x, y);
    ev('pointermove', x + dx, y + dy);
    ev('pointerup', x + dx, y + dy);
  };
  // Up the screen is further away (y grows away from the viewer). Twice, so
  // this also covers the repaint: the scene rebuilds its own markup on every
  // move, and a listener bound to the circle rather than to the host would
  // make the scene draggable exactly once.
  drag(0, 0, -60);
  await sleep(600);
  const oneDrag = state();
  drag(0, 0, -60);
  await sleep(600);
  out.oneDrag = oneDrag;
  out.afterDrag = state();
  out.afterDragLabel = dist();
  out.afterDragGap = gap();

  // …and the fight that gets RUN is the one on screen. The wasm build is the
  // one shipped, so this is the real path.
  const body = { ...buildPayload(), ...theFight(), runs: 6, seed: 7, duration: 8 };
  out.sentPlayer = body.player_at;
  out.sentTarget = body.target_at;
  out.sentMatches = JSON.stringify(body.target_at) === JSON.stringify(sim.target_at)
    && JSON.stringify(body.player_at) === JSON.stringify(sim.player_at);

  // …and out where the cone is wider than a body, the shipping build drops
  // pellets. A Braton's aimed cone is 2 degrees, so a 0.25 m body is missed
  // past about 7 m; 25 m is unambiguously out there.
  sim.target_at = [0, 25]; markScenarioDirty(); renderSim(); await sleep(700);
  const farRun = await api('/api/simulate',
    { ...buildPayload(), ...theFight(), runs: 6, seed: 7, duration: 8 });
  out.farHitRate = farRun.pellets / Math.max(farRun.shots, 1);
  out.farLabel = dist();

  // 3. TWO BODIES DO NOT OVERLAP, AND CONTACT IS ZERO. Drag the enemy right
  //    onto the player.
  {
    const c = cvEl();
    c.setPointerCapture = () => {};
    const [fx, fy] = at(0);
    const [yx, yy] = atPoint(sim.player_at);
    const ev = (type, cx, cy) => c.dispatchEvent(new PointerEvent(type,
      { clientX: cx, clientY: cy, bubbles: true, cancelable: true, pointerId: 1, button: 0 }));
    ev('pointerdown', fx, fy);
    ev('pointermove', yx, yy);
    ev('pointerup', yx, yy);
  }
  await sleep(900);
  out.overlapped = state();
  out.overlapLabel = dist();
  // …and at contact nothing misses, which is what makes it the boards' fight.
  const nearRun = await api('/api/simulate',
    { ...buildPayload(), ...theFight(), runs: 6, seed: 7, duration: 8 });
  out.nearHitRate = nearRun.pellets / Math.max(nearRun.shots, 1);

  // 4. THE QUICK SETS ARE IN THE CANVAS, and there is no second control.
  out.noTypedBox = !document.querySelector('#sim-target [data-k="arena_distance"]');
  out.jumps = [...document.querySelectorAll('#sim-target-arena .ar-jump[data-jump]')].map(b => b.textContent.trim());
  // Put the target OFF-AXIS first, so a snap-to-axis implementation is visible
  // rather than lucky: (6,8) is 3-4-5 scaled, so a 20 m GAP must land on
  // (12.24, 16.32) — 20.4 m between centres, since the quick sets set the gap.
  sim.player_at = [0, 0]; sim.target_at = [6, 8]; markScenarioDirty();
  renderSim(); await sleep(900);
  document.querySelector('#sim-target-arena .ar-jump[data-jump="20"]').click();
  await sleep(900);
  out.typedState = gap();
  out.typedAt = sim.target_at.map(v => Math.round(v * 100) / 100);
  // …and the chip for the distance you are AT is marked.
  out.marked = [...document.querySelectorAll('#sim-target-arena .ar-jump.on')].map(b => b.dataset.jump);

  // 5. AN OFFICIAL RULER'S FIGHT IS PINNED. Switch to the benchmark scenario
  //    and try to drag: nothing may move.
  // Pick the ruler the way the app does. The benchmark bar is a searchable
  // dropdown rather than a chip strip, so the check drives the app's own
  // picker instead of guessing at its markup.
  // The official rulers are BUILTINS and are not in the user's preset list —
  // they are addressed by their own id, which is the benchmark's.
  pickPreset(scenarioBarCfg(), 'single_target'); await sleep(1800);
  out.officialDistance = gap();
  out.official = typeof officialScenarioActive === 'function' && officialScenarioActive();
  const before = state();
  drag(0, 0, -70);
  await sleep(700);
  out.officialMoved = Math.abs(state() - before) > 1e-9;
  out.officialLooksLocked = !!document.querySelector('#sim-target-arena.ar-ro');

  // 6. THE OPTIMIZER SHOWS IT READ-ONLY.
  history.pushState({}, '', '/weapons/Braton/optimizer'); route(); await sleep(2200);
  const oh = document.querySelector('#opt-target-arena');
  out.optDrew = !!(oh && oh.querySelector('.arc-cv'));
  out.optReadonly = !!(oh && oh.classList.contains('ar-ro'));
  return out;
})()`);

check("a scenario of your own is open before anything is dragged",
  r.startedEditable === true,
  "the app lands on the official ruler, whose fight is pinned");
check("the arena draws, with two bodies", r.drew && r.bodies === 2, `${r.bodies} bodies`);
check("...and both are INK on the floor, where the scene says it put them",
  r.bodyInk === true && r.youInk === true, `enemy ${r.bodyInk}, you ${r.youInk}`);
check("...and a MUZZLE on the shooter's own circumference, with its facing",
  r.muzzle === true && r.facing === true, `muzzle ${r.muzzle} facing ${r.facing}`);
check("...and the fight starts at CONTACT — centres 0.4 m apart, and it READS 0 m",
  Math.abs(r.startState - 0.4) < 1e-6 && Math.abs(r.startLabel) < 0.01,
  `centres ${r.startState}, label ${r.startLabel}`);
check("dragging the enemy moves it away", r.oneDrag > r.startState + 0.3,
  `${r.startState} -> ${r.oneDrag} m`);
check("...and it is still draggable after the repaint", r.afterDrag > r.oneDrag + 0.3,
  `a second drag went ${r.oneDrag} -> ${r.afterDrag} m`);
check("...and the label follows the finger, one contact behind the centres",
  Math.abs(r.afterDragLabel - r.afterDragGap) < 0.02,
  `label ${r.afterDragLabel} vs gap ${r.afterDragGap} (centres ${r.afterDrag})`);
check("...and what is dragged is EXACTLY what gets sent", r.sentMatches === true,
  `player ${JSON.stringify(r.sentPlayer)} target ${JSON.stringify(r.sentTarget)}`);
check("...and out there the shipping build MISSES", r.farHitRate < 0.9,
  `${(r.farHitRate * 100).toFixed(0)}% of pellets landed at ${r.farLabel} m`);
check("two bodies cannot pass through each other",
  Math.abs(r.overlapped - 0.4) < 1e-6,
  `dragged onto the player and landed at ${r.overlapped} m`);
check("...and the label says ZERO, because that is what point blank means",
  Math.abs(r.overlapLabel) < 0.01, `${r.overlapLabel} m`);
check("...and at contact nothing misses", r.nearHitRate > 0.99,
  `${(r.nearHitRate * 100).toFixed(0)}%`);
check("there is no second control for a position — the canvas is the only one",
  r.noTypedBox === true);
check("...and the quick sets are in the scene", (r.jumps || []).length >= 4, (r.jumps || []).join(" "));
check("...and one click moves the target ALONG its own line, not onto an axis",
  Math.abs(r.typedState - 20) < 1e-6 && Math.abs(r.typedAt[0] - 12.24) < 0.01
    && Math.abs(r.typedAt[1] - 16.32) < 0.01,
  `${r.typedState} m gap at ${JSON.stringify(r.typedAt)}`);
check("an official ruler is the active scenario for this part", r.official === true);
check("...and it opens at the distance the ruler pins — contact, a zero gap",
  Math.abs(r.officialDistance) < 1e-6, `${r.officialDistance} m`);
check("...and its fight cannot be dragged", r.officialMoved === false);
check("...and the scene says so rather than silently ignoring the finger",
  r.officialLooksLocked === true);
check("...and the distance you are at is marked", (r.marked || []).includes("20"),
  (r.marked || []).join(","));
check("the optimizer draws the same scene", r.optDrew === true);
check("...read-only, because a fight is edited in one place", r.optReadonly === true);

await finish("the arena is a place you can drag");
