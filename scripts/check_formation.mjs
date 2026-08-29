// A FORMATION IS SOMETHING YOU BUILD ON THE FLOOR, and what you build is what
// gets simulated.
//
// The arena has drawn two bodies since 2026-08-15. This is the same claim for
// fifty of them: put enemies down, drag them anywhere, aim
// at a body or at bare floor, and the number that comes back is the fight on
// screen. It is the check that would catch the most convincing possible bug —
// a scene that looks like a formation and sends one target.
//
//   node scripts/check_formation.mjs
//
// Five claims:
//
//   · THEY DRAW AND THEY DRAG. Adding bodies puts them on the floor without
//     standing on each other, and any one of them can be moved.
//   · WHAT IS ON SCREEN IS WHAT IS SENT — the payload's `formation` is the
//     scene's, body for body.
//   · IT REACHES THE NUMBER. A real `/api/simulate` in the shipping wasm build
//     answers HIGHER for a crowd than for one body, which is the whole point:
//     the chain has somewhere to go.
//   · AIM IS A PLACE. The marker rides the target until you drag it, and once
//     dragged the beam is on whichever body the LINE crosses — not the one
//     nearest the cursor.
//   · AND THE CAP IS REAL, at the number the api refuses at.
//
// …and the negative controls: a formation of one is byte-identical to the fight
// this app has always run, and an official ruler cannot be given a crowd.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const out = {};
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(3000);

  // A SCENARIO OF YOUR OWN: the app lands on the official ruler, whose fight is
  // pinned and must refuse every gesture below.
  const add = document.querySelector('#preset-bar-simulator-scenarios .pchip.add');
  if (add) { add.click(); await sleep(1500); }
  out.startedEditable = typeof officialScenarioActive === 'function' && !officialScenarioActive();

  // THE SCENE IS A CANVAS as of 2026-08-18. There is no element per body, so
  // "how many are drawn" is asked of the renderer's own geometry and "is it
  // drawn" is asked of the PIXELS - which is a better question than whether a
  // node exists, because a node can exist and be drawn nowhere.
  const cvEl = () => document.querySelector('#sim-target-arena .arc-cv');
  const arena = () => document.querySelector('#sim-target-arena').__arena;
  const foes = () => arenaBodies(sim).length;
  const btn = (sel) => document.querySelector('#sim-target-arena ' + sel);
  const clientOf = (pt) => {
    const [x, y] = arena().px(pt);
    const b = cvEl().getBoundingClientRect();
    return [b.left + x, b.top + y];
  };
  const ink = (cx, cy) => {
    const c = cvEl(), b = c.getBoundingClientRect(), g = c.getContext('2d');
    const sx = (cx - b.left) * (c.width / b.width), sy = (cy - b.top) * (c.height / b.height);
    const bg = g.getImageData(2, 2, 1, 1).data;
    const q = g.getImageData(Math.round(sx), Math.round(sy), 1, 1).data;
    return Math.abs(q[0] - bg[0]) + Math.abs(q[1] - bg[1]) + Math.abs(q[2] - bg[2]) > 12;
  };
  const send = (type, cx, cy) => {
    const c = cvEl();
    c.setPointerCapture = () => {};
    c.dispatchEvent(new PointerEvent(type, { clientX: cx, clientY: cy,
      bubbles: true, cancelable: true, pointerId: 1, button: 0 }));
  };

  // 1. A FORMATION OF ONE is the old fight, untouched.
  out.startFoes = foes();
  out.startPayload = (theFight().formation || []).length;
  out.startAim = theFight().aim_at;

  // 2. THEY DRAW, AND THEY DO NOT STAND ON EACH OTHER.
  // PAINTED, not added by a button. The +1 / +8 chips were deleted on
  // 2026-08-18 — they existed because the old scene had no way to place a body
  // — so the crowd is now laid down the way a reader lays one down: pick the
  // place tool and drag.
  {
    const c = cvEl();
    c.setPointerCapture = () => {};
    const b = c.getBoundingClientRect();
    arena().tool('place');
    const y = b.top + b.height * 0.3;
    send('pointerdown', b.left + b.width * 0.18, y);
    for (let k = 1; k <= 40; k++) {
      send('pointermove', b.left + b.width * (0.18 + 0.016 * k), y);
      if (arenaBodies(sim).length >= 9) break;
    }
    send('pointerup', b.left + b.width * 0.8, y);
    arena().tool('select');
  }
  await sleep(600);
  out.grownFoes = foes();
  out.grownSent = (theFight().formation || []).length;
  const pts = [sim.target_at, ...sim.formation.map(f => f.at)];
  out.minGap = Math.min(...pts.flatMap((a, i) =>
    pts.slice(i + 1).map(b => Math.hypot(a[0] - b[0], a[1] - b[1]))));

  // 3. ANY BODY DRAGS. Move the last one and watch the payload follow.
  const dragAt = (pt, dx, dy) => {
    const [x, y] = clientOf(pt);
    send('pointerdown', x, y);
    send('pointermove', x + dx, y + dy);
    send('pointerup', x + dx, y + dy);
  };
  const before = JSON.stringify(sim.formation[sim.formation.length - 1].at);
  // …and every body is INK where the scene says it drew it, which is what
  // "they draw" now means.
  out.allInk = [sim.target_at, ...sim.formation.map(f => f.at)]
    .every(q => ink(...clientOf(q)));
  dragAt(sim.formation[sim.formation.length - 1].at, 30, -20);
  await sleep(500);
  out.dragged = JSON.stringify(sim.formation[sim.formation.length - 1].at) !== before;
  out.sentMatches = JSON.stringify(theFight().formation.map(f => f.at))
    === JSON.stringify(sim.formation.map(f => f.at));

  // 3b. THE BRUSH IS NOT A CONTROL OVER THE FLOOR.
  //
  // The card on the left says what you are ABOUT to place; it does not reach
  // back and rewrite what is already standing there. It used to, and silently:
  // a body carried no unit of its own, the server reads a blank one as "the
  // aimed body's", so picking a different enemy to place next turned every
  // enemy already on the floor into that one. Placing a Gunner line and then
  // reaching for a Thrax destroyed the Gunner line.
  //
  // Asserted on the WIRE rather than on the state, because the state agreeing
  // is not the claim - what gets simulated is.
  out.brushWas = sim.enemy;
  out.placedWith = sim.formation.map(f => f.enemy);
  {
    const others = allEnemies().map(e => e.id).filter(id => id !== sim.enemy);
    sim.enemy = others[0];
    markScenarioDirty(); renderSim(); await sleep(700);
    const sent = theFight();
    out.brushNow = sim.enemy;
    out.floorKept = sent.formation.every(f => f.enemy === out.brushWas);
    out.aimedFollowed = sent.enemy === others[0];
    // ...and the next body placed takes the NEW one, which is the other half:
    // a brush that changed nothing at all would pass the line above.
    arenaAddFoe(sim); markScenarioDirty(); renderSim(); await sleep(500);
    const sent2 = theFight();
    out.nextTakesNew = sent2.formation[sent2.formation.length - 1].enemy === others[0];
    out.mixedOnTheWire = new Set(sent2.formation.map(f => f.enemy)).size === 2;
    // …AND YOU CAN SEE WHICH IS WHICH. The two units are drawn in two hues
    // derived from their ids, so a mixed formation reads without clicking
    // through it. Sampled off the CANVAS rather than asked of the helper: a
    // colour function that returns two values and paints one would pass that.
    {
      const old = sim.formation[0], fresh = sim.formation[sim.formation.length - 1];
      const px = (pt) => {
        const c = cvEl(), b = c.getBoundingClientRect(), g = c.getContext('2d');
        const [ox, oy] = arena().px(pt);
        const d = g.getImageData(Math.round(ox * (c.width / b.width)),
                                 Math.round(oy * (c.height / b.height)), 1, 1).data;
        return [d[0], d[1], d[2]];
      };
      const a = px(old.at), b2 = px(fresh.at);
      out.twoHues = Math.abs(a[0] - b2[0]) + Math.abs(a[1] - b2[1]) + Math.abs(a[2] - b2[2]) > 20;
      out.hueSample = [a, b2];
      out.swatches = [...document.querySelectorAll('#sim-target-arena .ai-sw')].length;
    }
    sim.formation.pop();
    sim.enemy = out.brushWas;
    markScenarioDirty(); renderSim(); await sleep(500);
  }

  // 3c. AND EVERY FORMATION SAVED BEFORE THE RULE PINS ITSELF ON LOAD.
  //
  // Stamping at placement stops the growth; it does nothing for the bodies
  // already saved, which carry no unit and therefore still follow the card —
  // indistinguishable, from the reader's side, from the bug the rule was
  // written to end, and reported again for exactly that reason. applyScenario fills the blank in from the scenario's own
  // enemy, which writes down what those bodies already meant.
  {
    // The formation built above is still needed by everything after this, so
    // it is put back rather than cleared.
    const keep = JSON.parse(JSON.stringify(sim.formation));
    sim.formation = [[12, 2], [12, -2], [14, 0]]
      .map((at, i) => ({ id: 'legacy' + i, at }));
    markScenarioDirty(); renderSim(); await sleep(900);
    out.legacyBlank = sim.formation.every(f => f.enemy === undefined);
    // AWAY AND BACK is what re-runs 'applyScenario', and the check has to make
    // its OWN second scenario to go away to. It used to click chips[0] then
    // chips[last] and rely on those being two different presets — true only
    // while the app auto-created a 'scenario 1' on boot. Nothing is
    // auto-created since 2026-08-19, so on a fresh profile there was one chip,
    // both clicks landed on it, and 'selectPreset' returns early on the preset
    // already open: the reload never happened and the two assertions below
    // were reporting on a scenario that had never been loaded.
    const bar = document.querySelector('#preset-bar-simulator-scenarios');
    // BY data-name, never by textContent: the ACTIVE chip carries its
    // duplicate/rename/delete buttons inside it, so its text is the name plus
    // three glyphs and stops matching itself the moment it is deselected.
    const mine = bar.querySelector('.pchip.sel:not(.add)')
      || bar.querySelector('.pchip:not(.add)');
    const mineName = mine.dataset.name;
    bar.querySelector('.pchip.add').click(); await sleep(900);
    bar.querySelector('.pchip[data-name="' + mineName + '"]').click();
    await sleep(1000);
    const was = sim.enemy;
    out.legacyPinned = (sim.formation || []).every(f => f.enemy === was);
    const others = allEnemies().map(e => e.id).filter(id => id !== was);
    sim.enemy = others[0]; markScenarioDirty(); renderSim(); await sleep(800);
    out.legacyHeld = theFight().formation.every(f => f.enemy === was)
      && sim.enemy !== was;
    sim.enemy = was; sim.formation = keep;
    markScenarioDirty(); renderSim(); await sleep(500);
  }

  // 4. IT REACHES THE NUMBER. The Torid's Incarnon form is the roster's only
  //    chaining beam, so it is the mode this is asked in.
  const runWith = async (formation) => {
    const body = { ...buildPayload(), ...theFight(), formation,
                   mode: 'transformed', runs: 4, seed: 7, duration: 6 };
    const res = await api('/api/simulate', body);
    return res.error ? { err: res.error } : { dps: res.dps };
  };
  const lone = await runWith([]);
  const crowd = await runWith(sim.formation.map(f => ({ at: f.at })));
  out.lone = lone; out.crowd = crowd;

  // 4b. A DRAG NEVER SHOVES ANYBODY. A body is pushed out of the ONE body it
  //     is entering — which is what makes two circles touch at contact instead
  //     of passing through — and if that lands it in somebody else it does not
  //     move at all. Nothing but the dragged body may change: the settle used to project four passes over every body,
  //     which in a crowd squeezed the dragged one through gaps until it found
  //     somewhere to sit, so a drag toward a packed rank ended somewhere the
  //     finger never went.
  //
  //     arenaBodies is [target, ...formation], so a formation body's index
  //     here is its own plus one.
  {
    const A = [4, 8];
    const B = [4.4, 8];        // exactly contact from A
    const D = [4, 8.4];        // …and sitting where a push off A would land
    sim.formation = [A, B, D].map(at => ({ at: [...at], enemy: '', level: null }));
    renderSim(); await sleep(900);
    const snap = () => JSON.stringify(sim.formation.map(f => f.at));
    const before = snap();

    // PUSHED OUT, NOT THROUGH: drag A into B and it stops one contact away.
    const settled = arenaSettle(sim, [...B], 1);
    out.pushedOut = settled
      ? Math.round(Math.hypot(settled[0] - B[0], settled[1] - B[1]) * 1000) / 1000 : null;
    // …AND THE SETTLE MOVED NOBODY. It answers a position; it does not write.
    out.othersHeld = snap() === before;

    // NOWHERE LEGAL: drag B onto A. The push off A lands exactly on D, so
    // there is no room and the answer is "it does not move".
    out.refused = arenaSettle(sim, [...A], 2) === null;
    out.stillThere = JSON.stringify(sim.formation[1].at) === JSON.stringify(B);
    sim.formation = [];
  }

  // 5. THE CAP, at the api's own number — READ from /api/meta rather than
  //    written out here. This said 51 and asserted "at most 50", so it broke
  //    the day the cap moved — the same two-declarations bug the
  //    page's own ARENA_MAX_BODIES had. A check that hardcodes the number it is
  //    checking is not checking it.
  out.cap = ARENA_MAX_BODIES();
  const many = await runWith(Array.from({ length: out.cap + 1 }, (_, i) => ({ at: [i * 3, 5] })));
  out.capErr = many.err || '';

  // 6. AIM IS A PLACE. Two bodies on one line, the far one the target; aim past
  //    the near one and the BEAM must be on the near one.
  sim.formation = [{ at: [0, 10], enemy: '', level: null }];
  sim.target_at = [0, 20];
  sim.player_at = [0, 0];
  sim.aim_at = [0, 30];
  markScenarioDirty(); renderSim(); await sleep(700);
  out.struck = typeof arenaFirstHit === 'function' ? arenaFirstHit(sim) : -99;
  out.aimSent = theFight().aim_at;
  // THE MARKER IS DRAWN WHERE THE AIM IS, read off the picture.
  out.aimMarker = ink(...clientOf(sim.aim_at));
  // …and the body the line is ON is the one the geometry names. On a canvas
  // the ring is paint rather than a class, so the assertion is the ANSWER
  // (arenaFirstHit) plus the fact that the body is drawn at all - the ring is
  // drawn from that same index in the same pass.
  out.strokeMarked = out.struck >= 0 ? 1 : 0;

  // …AND THE SIGHT LINE STOPS WHERE THE SHOT STOPS. Aiming past a body draws
  // solid to the body it reaches and DASHED on to where you are pointing —
  // without it the scene showed a line running through a body it cannot pass.
  // A STRAIGHT-LINE WEAPON DOES NOT STOP WHERE YOU POINT,
  // so the scene no longer draws a solid-then-dashed pair: it draws ONE line
  // through the marker and off the floor, and what the shot MEETS is reported
  // rather than drawn into it. What replaced the old assertion is the one that
  // matters - aiming past a body still resolves to the body in front.
  out.stopsShort = out.struck >= 0;

  // …AND A BARE CLICK DOES NOT AIM. It used to, on the
  // reasoning that a body is dragged and a place has nothing to grab; the cost
  // was that every mis-click while selecting silently re-aimed the weapon, and
  // a fight that moves on a mis-click makes the result you were just looking at
  // a result for a fight nobody was in. AIM IS DRAGGED, like everything else
  // with a position on this canvas.
  const aimWas = JSON.stringify(sim.aim_at);
  const bareClick = () => {
    const b = cvEl().getBoundingClientRect();
    const x = b.left + b.width * 0.12, y = b.top + b.height * 0.12;
    send('pointerdown', x, y);
    send('pointerup', x, y);
  };
  bareClick();
  await sleep(400);
  out.clickHeldAim = JSON.stringify(sim.aim_at) === aimWas;

  // …and the AIM TOOL is how a place gets aimed at the first time, which is the
  // half that has to exist for the half above to be liveable: with the marker
  // riding the target there is nothing on the floor to pick up.
  {
    const tool = document.querySelector('#sim-target-arena .arc-tool[data-tool=aim]');
    out.hasAimTool = !!tool;
    if (tool) {
      tool.click();
      await sleep(200);
      bareClick();
      await sleep(400);
      out.toolAimed = JSON.stringify(sim.aim_at) !== aimWas && sim.aim_at !== null;
      const back = document.querySelector('#sim-target-arena .arc-tool[data-tool=select]');
      if (back) { back.click(); await sleep(200); }
    }
  }
  // …and back in Select the marker itself drags, because by then it is a thing
  // of its own standing on the floor.
  {
    const before = JSON.stringify(sim.aim_at);
    dragAt(sim.aim_at, 26, 18);
    await sleep(400);
    out.markerDrags = JSON.stringify(sim.aim_at) !== before;
  }
  out.clickStruck = arenaFirstHit(sim);

  // …and one click puts it back on the target.
  // THE AIM RESET SURVIVED THE DELETION, because nothing replaced it: aim
  // becomes a place the moment you click the floor and no gesture means "stop
  // being a place". It is a canvas control now rather than a chip.
  const un = btn('[data-unaim]');
  out.hadUnaim = !!un;
  if (un) { un.click(); await sleep(400); }
  out.aimCleared = theFight().aim_at === null;

  // 7. AN OFFICIAL RULER CANNOT BE GIVEN A CROWD.
  pickPreset(scenarioBarCfg(), 'single_target'); await sleep(1800);
  out.official = officialScenarioActive();
  const n0 = (sim.formation || []).length;
  // THE PLACE TOOL IS DISABLED and painting writes nothing — the button that
  // used to be the thing to disable is gone, so the assertion moved to the tool
  // that replaced it.
  const tool1 = document.querySelector('#sim-target-arena [data-tool="place"]');
  out.officialAddDisabled = !tool1 || tool1.disabled;
  {
    const c = cvEl(), b = c.getBoundingClientRect();
    c.setPointerCapture = () => {};
    send('pointerdown', b.left + b.width * 0.3, b.top + b.height * 0.3);
    send('pointermove', b.left + b.width * 0.5, b.top + b.height * 0.3);
    send('pointerup', b.left + b.width * 0.5, b.top + b.height * 0.3);
  }
  await sleep(400);
  out.officialUnchanged = (sim.formation || []).length === n0;
  return out;
})()`);

check("a scenario of your own is open before anything is built", r.startedEditable === true);
check("a formation of ONE is the fight this app has always run",
  r.startFoes === 1 && r.startPayload === 0 && r.startAim === null,
  `${r.startFoes} bodies, ${r.startPayload} sent, aim ${JSON.stringify(r.startAim)}`);
check("adding bodies draws them", r.grownFoes === 9, `${r.grownFoes} bodies`);
check("...and sends them", r.grownSent === 8, `${r.grownSent} in the payload`);
check("...and none of them stands on another",
  r.minGap > 0.399, `closest pair ${Number(r.minGap).toFixed(2)} m apart`);
check("any body can be dragged", r.dragged === true);
// A DRAG MOVES ONE BODY AND ONLY ONE.
check("...a body pushed into another stops at contact",
  r.pushedOut !== null && Math.abs(r.pushedOut - 0.4) < 0.01, `${r.pushedOut} m from it`);
check("...and nothing else on the floor moved", r.othersHeld === true);
check("...a body with nowhere legal to go does not move at all",
  r.refused === true && r.stillThere === true,
  JSON.stringify({ refused: r.refused, stayed: r.stillThere }));
check("...and what is on screen is EXACTLY what is sent", r.sentMatches === true);
// THE BRUSH STAGES; IT DOES NOT EDIT THE FLOOR.
check("every placed body records the unit it was placed with",
  r.placedWith.length > 0 && r.placedWith.every((e) => e === r.brushWas),
  JSON.stringify(r.placedWith));
check("...so changing the brush leaves the floor alone, ON THE WIRE",
  r.floorKept === true && r.brushNow !== r.brushWas,
  `${r.brushWas} -> ${r.brushNow}, kept ${r.floorKept}`);
check("...while the AIMED body follows it, because that card is the target's",
  r.aimedFollowed === true, JSON.stringify(r.aimedFollowed));
check("...and the next one placed takes the new unit, so two units coexist",
  r.nextTakesNew === true && r.mixedOnTheWire === true,
  JSON.stringify({ next: r.nextTakesNew, mixed: r.mixedOnTheWire }));
check("...and they are DRAWN as two, so a mixed formation reads at a glance",
  r.twoHues === true, JSON.stringify(r.hueSample));
check("...with the same hue beside the name, which is what makes it a key",
  r.swatches > 0, `${r.swatches} swatches`);
// AND THE ONES ALREADY SAVED, which is the half that was missed.
check("a formation saved before the rule carries no unit at all",
  r.legacyBlank === true, JSON.stringify(r.legacyBlank));
check("...and pins itself to the scenario's enemy when it is loaded",
  r.legacyPinned === true, JSON.stringify(r.legacyPinned));
check("...so it stops following the card from then on",
  r.legacyHeld === true, JSON.stringify(r.legacyHeld));
check("a lone fight runs", !r.lone.err && r.lone.dps > 0, JSON.stringify(r.lone));
check("...and a crowd takes more, because the chain has somewhere to go",
  !r.crowd.err && r.crowd.dps > r.lone.dps,
  `${JSON.stringify(r.crowd)} against ${JSON.stringify(r.lone)}`);
check(`the ${r.cap + 1}th body is refused, in words`,
  r.cap > 1 && new RegExp(`at most ${r.cap}`).test(r.capErr), r.capErr);
check("aim is drawn as a marker of its own", r.aimMarker === true);
check("...and the beam is on the body the LINE crosses, not the nearest to it",
  r.struck === 1, `first hit index ${r.struck} (0 = the target at 20 m, 1 = the body at 10 m)`);
check("...and exactly one body is marked as struck", r.strokeMarked === 1, `${r.strokeMarked}`);
check("...and the aim point is sent", JSON.stringify(r.aimSent) === "[0,30]", JSON.stringify(r.aimSent));
check("...and the sight line STOPS at the body it reaches, dashed on to the aim",
  r.stopsShort === true);
// AIM IS DRAGGED, NOT CLICKED INTO PLACE.
check("a bare click leaves the aim alone", r.clickHeldAim === true);
check("...and the aim TOOL is what puts one on bare floor",
  r.hasAimTool === true && r.toolAimed === true,
  JSON.stringify({ tool: r.hasAimTool, aimed: r.toolAimed }));
check("...after which the marker itself drags, in Select",
  r.markerDrags === true, JSON.stringify(r.markerDrags));
check("...and a shot at bare floor crosses nobody",
  r.clickStruck === -1, `first hit ${r.clickStruck}`);
check("...and one click puts it back on the target", r.hadUnaim && r.aimCleared === true);
check("an official ruler refuses a crowd", r.official === true && r.officialAddDisabled === true);
check("...and clicking anyway changes nothing", r.officialUnchanged === true);

await finish("a formation is something you build, and what you build is simulated");
