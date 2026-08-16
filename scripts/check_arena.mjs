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
//   · IT DRAWS — two bodies, a distance label, at the real 0.25 m radius.
//   · DRAGGING MOVES THE FIGHT — the label, the scenario state and a real
//     `/api/simulate` in the shipping wasm build all follow the finger.
//   · BODIES DO NOT PASS THROUGH EACH OTHER. Dragging the enemy onto the
//     player leaves them 0.4 m apart — CONTACT, twice the measured 0.2 m body
//     radius (M46) — that is the closest two circles go and the floor the engine
//     clamps to as well. It is the one rule the scene exists to make visible.
//   · THE TYPED BOX AND THE DRAG ARE ONE THING. Typing a distance moves the
//     target ALONG the line it already stands on rather than snapping it to an
//     axis, so neither input undoes the other's other axis.
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

  const svg = () => document.querySelector('#sim-target-arena .ar-svg');
  const dist = () => {
    const t = document.querySelector('#sim-target-arena .ar-dist');
    return t ? parseFloat(t.textContent) : null;
  };
  const state = () => Math.hypot(sim.target_at[0] - sim.player_at[0],
                                 sim.target_at[1] - sim.player_at[1]);

  // 1. IT DRAWS, and it starts at contact.
  out.drew = !!svg();
  out.bodies = document.querySelectorAll('#sim-target-arena .ar-body').length;
  out.startLabel = dist();
  out.startState = state();

  // 2. DRAGGING MOVES THE FIGHT. Pointer events on the enemy body, in the
  //    SVG's own pixel space, so this is the gesture a finger makes.
  const drag = (sel, dx, dy) => {
    const el = document.querySelector(sel);
    const b = el.getBoundingClientRect();
    const x = b.left + b.width / 2, y = b.top + b.height / 2;
    const ev = (type, cx, cy, target) => target.dispatchEvent(
      new PointerEvent(type, { clientX: cx, clientY: cy, bubbles: true, cancelable: true }));
    ev('pointerdown', x, y, el);
    ev('pointermove', x + dx, y + dy, window);
    ev('pointerup', x + dx, y + dy, window);
  };
  // Up the screen is further away (y grows away from the viewer). Twice, so
  // this also covers the repaint: the scene rebuilds its own markup on every
  // move, and a listener bound to the circle rather than to the host would
  // make the scene draggable exactly once.
  drag('#sim-target-arena .ar-foe', 0, -60);
  await sleep(600);
  const oneDrag = state();
  drag('#sim-target-arena .ar-foe', 0, -60);
  await sleep(600);
  out.oneDrag = oneDrag;
  out.afterDrag = state();
  out.afterDragLabel = dist();

  // …and the fight that gets RUN is the one on screen. The wasm build is the
  // one shipped, so this is the real path.
  const body = { ...buildPayload(), ...fightPayload(), runs: 6, seed: 7, duration: 8 };
  out.sentPlayer = body.player_at;
  out.sentTarget = body.target_at;
  out.sentMatches = JSON.stringify(body.target_at) === JSON.stringify(sim.target_at)
    && JSON.stringify(body.player_at) === JSON.stringify(sim.player_at);

  // …and out where the cone is wider than a body, the shipping build drops
  // pellets. A Braton's aimed cone is 2 degrees, so a 0.25 m body is missed
  // past about 7 m; 25 m is unambiguously out there.
  sim.target_at = [0, 25]; markScenarioDirty(); renderSim(); await sleep(700);
  const farRun = await api('/api/simulate',
    { ...buildPayload(), ...fightPayload(), runs: 6, seed: 7, duration: 8 });
  out.farHitRate = farRun.pellets / Math.max(farRun.shots, 1);
  out.farLabel = dist();

  // 3. TWO BODIES DO NOT OVERLAP. Drag the enemy right onto the player.
  const you = document.querySelector('#sim-target-arena .ar-you').getBoundingClientRect();
  const foe = document.querySelector('#sim-target-arena .ar-foe');
  const fb = foe.getBoundingClientRect();
  const ev = (type, cx, cy, target) => target.dispatchEvent(
    new PointerEvent(type, { clientX: cx, clientY: cy, bubbles: true, cancelable: true }));
  ev('pointerdown', fb.left + fb.width / 2, fb.top + fb.height / 2, foe);
  ev('pointermove', you.left + you.width / 2, you.top + you.height / 2, window);
  ev('pointerup', you.left + you.width / 2, you.top + you.height / 2, window);
  await sleep(900);
  out.overlapped = state();
  out.overlapLabel = dist();
  // …and at contact nothing misses, which is what makes it the boards' fight.
  const nearRun = await api('/api/simulate',
    { ...buildPayload(), ...fightPayload(), runs: 6, seed: 7, duration: 8 });
  out.nearHitRate = nearRun.pellets / Math.max(nearRun.shots, 1);

  // 4. THE TYPED BOX IS THE SAME ONE THING. Put the target off-axis first, so
  //    a snap-to-axis implementation is visible rather than lucky.
  sim.player_at = [0, 0]; sim.target_at = [6, 8]; markScenarioDirty();
  renderSim(); await sleep(900);
  const box = document.querySelector('#sim-target [data-k="arena_distance"]');
  out.typedShows = parseFloat(box.value);
  box.value = '20';
  box.dispatchEvent(new Event('change', { bubbles: true }));
  await sleep(900);
  out.typedState = state();
  // ALONG THE SAME LINE: (6,8) is 3-4-5 scaled, so 20 m must land on (12,16).
  out.typedAt = sim.target_at.map(v => Math.round(v * 100) / 100);

  // 5. THE OPTIMIZER SHOWS IT READ-ONLY.
  history.pushState({}, '', '/weapons/Braton/optimizer'); route(); await sleep(2200);
  const oh = document.querySelector('#opt-target-arena');
  out.optDrew = !!(oh && oh.querySelector('.ar-svg'));
  out.optReadonly = !!(oh && oh.classList.contains('ar-ro'));
  return out;
})()`);

check("the arena draws, with two bodies", r.drew && r.bodies === 2, `${r.bodies} bodies`);
check("...and the fight starts at CONTACT — 0.4 m, twice the measured 0.2 m radius",
  Math.abs(r.startState - 0.4) < 1e-6 && Math.abs(r.startLabel - 0.4) < 0.01,
  `state ${r.startState}, label ${r.startLabel}`);
check("dragging the enemy moves it away", r.oneDrag > r.startState + 0.3,
  `${r.startState} -> ${r.oneDrag} m`);
check("...and it is still draggable after the repaint", r.afterDrag > r.oneDrag + 0.3,
  `a second drag went ${r.oneDrag} -> ${r.afterDrag} m`);
check("...and the label follows the finger",
  Math.abs(r.afterDragLabel - r.afterDrag) < 0.02,
  `label ${r.afterDragLabel} vs ${r.afterDrag}`);
check("...and what is dragged is EXACTLY what gets sent", r.sentMatches === true,
  `player ${JSON.stringify(r.sentPlayer)} target ${JSON.stringify(r.sentTarget)}`);
check("...and out there the shipping build MISSES", r.farHitRate < 0.9,
  `${(r.farHitRate * 100).toFixed(0)}% of pellets landed at ${r.farLabel} m`);
check("two bodies cannot pass through each other",
  Math.abs(r.overlapped - 0.4) < 1e-6,
  `dragged onto the player and landed at ${r.overlapped} m`);
check("...and the label says so", Math.abs(r.overlapLabel - 0.4) < 0.01, `${r.overlapLabel}`);
check("...and at contact nothing misses", r.nearHitRate > 0.99,
  `${(r.nearHitRate * 100).toFixed(0)}%`);
check("the typed box reads the distance the scene shows",
  Math.abs(r.typedShows - 10) < 0.02, `${r.typedShows} for (6,8)`);
check("...and typing moves the target ALONG its own line, not onto an axis",
  Math.abs(r.typedState - 20) < 1e-6 && Math.abs(r.typedAt[0] - 12) < 0.01
    && Math.abs(r.typedAt[1] - 16) < 0.01,
  `${r.typedState} m at ${JSON.stringify(r.typedAt)}`);
check("the optimizer draws the same scene", r.optDrew === true);
check("...read-only, because a fight is edited in one place", r.optReadonly === true);

await finish("the arena is a place you can drag");
