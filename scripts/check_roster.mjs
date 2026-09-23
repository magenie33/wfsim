// A FIGHT IS n AGAINST m, AND THE PAGE CAN SET BOTH HALVES.
//
// The enemy half has been a list since the formation existed. This asserts the
// other one: the roster in block 2 puts another gun in the fight, and what it
// puts there reaches the engine, the answer and the ledger as its own seat.
//
//   · THE ROSTER IS A LIST — the open build as seat 1, and a weapon plus one
//     of its presets per seat after it. Seat 1 has no remove: it is the build
//     the page is about and the only one a board can take.
//   · A SEAT REACHES THE ANSWER — `also_acting` on the wire, one `combatants`
//     row back per seat, each naming the weapon it brought.
//   · …AND ITS BUILD IS ITS OWN. Two seats on the same weapon with different
//     presets deal different numbers, which is the whole of what a seat IS.
//   · THE RECORD REPLAYS THE SAME FIGHT. `/api/log` is a second reader of one
//     engagement, and a record drawn from the wielder alone is a TRUE record
//     of a fight nobody ran — every row in it correct, the fight wrong.
//   · A RULER IS ONE GUN. An official scenario never mentions a roster, so
//     opening one empties it rather than carrying the last fight's squad in.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const out = {};
  localStorage.clear();
  history.pushState({}, '', '/weapons/Cernos_Prime/simulator'); route(); await sleep(3000);

  // A SCENARIO OF YOUR OWN. A first-time visitor lands on the OFFICIAL ruler,
  // whose fight is pinned — which the last assertion is about, and which every
  // one before it needs not to be.
  await window.wfsim.do('shell.preset.copy', { bar: 'scenario' });
  await sleep(800);

  // ---- THE LIST -----------------------------------------------------------
  out.youOnly = document.querySelectorAll('#sim-roster .rs-row').length;
  out.youHaveNoRemove = !document.querySelector('#sim-roster .rs-you .rs-x');
  document.getElementById('rs-add').click(); await sleep(500);
  out.afterAdd = document.querySelectorAll('#sim-roster .rs-row').length;
  out.seatHasWeapon = !!document.getElementById('dd-seat-0');
  out.seatHasPreset = !!document.getElementById('dd-seat-preset-0');
  out.seatHasRemove = !!document.querySelector('#sim-roster [data-drop="0"]');
  // …and the new seat starts on the weapon in front of you, unmodded.
  out.bornOn = (sim.also_acting || [])[0];

  document.querySelector('#sim-roster [data-drop="0"]').click(); await sleep(400);
  out.afterDrop = document.querySelectorAll('#sim-roster .rs-row').length;

  // ---- A SEAT REACHES THE ANSWER -----------------------------------------
  const run = async () => {
    const body = { ...buildPayload(), ...theFight({ runs: 3, seed: 7 }) };
    const s = await simulateFleet(body);
    return { body, s };
  };
  const solo = await run();
  out.soloSeats = (solo.s.combatants || []).length;
  out.soloWire = (solo.body.also_acting || []).length;

  await window.wfsim.do('simulator.roster.add', { weapon: 'braton_prime' });
  await sleep(300);
  const pair = await run();
  out.pairSeats = (pair.s.combatants || []).map(c => [c.id, c.weapon]);
  out.pairDamage = (pair.s.combatants || []).map(c => Math.round(c.damage));
  out.wireIsWholeBuilds = (pair.body.also_acting || []).map(x => x.weapon);

  // ---- …AND ITS BUILD IS ITS OWN -----------------------------------------
  //
  // The same weapon twice, one seat unmodded and one carrying Serration: two
  // seats of one gun that deal DIFFERENT numbers is the only proof that a seat
  // resolves its own build rather than borrowing the one being reported on.
  const ps = presetListWithIds(BUILDS, 'braton_prime');
  storePresetList(BUILDS, [...ps, { id: 'armed', name: 'armed', savedAt: Date.now(),
    state: { weapon: 'braton_prime', slots: [{ mod: 'serration', pol: null, rank: 10 }] } }], 'braton_prime');
  await window.wfsim.do('simulator.roster.add', { weapon: 'braton_prime', preset: 'armed' });
  await sleep(300);
  const three = await run();
  out.threeSeats = (three.s.combatants || []).length;
  const d = (three.s.combatants || []).map(c => Math.round(c.damage));
  out.blankSeat = d[1];
  out.armedSeat = d[2];
  out.armedCarries = ((three.body.also_acting || [])[1] || {}).mods;

  // ---- THE RECORD REPLAYS THE SAME FIGHT ---------------------------------
  const log = await api('/api/log', { ...three.body, runs: 1, from: 0, to: 2 });
  out.logSeats = (log.combatants || []).map(c => [c.id, c.weapon]);
  out.logDealers = [...new Set((log.events || []).map(e => e.combatant || 0))].sort();

  // ---- …AND THE READER SEES IT -------------------------------------------
  //
  // Through the page's own Run, not a call beside it: the per-seat table is
  // drawn only where there is more than one seat, so until now it had never
  // been on screen at all.
  setSimRuns(3);
  document.getElementById('run-sim').click();
  for (let k = 0; k < 40 && !document.querySelector('#sim-results .seat-r'); k++) await sleep(1000);
  out.seatRows = [...document.querySelectorAll('#sim-results .seat-r')]
    .map(e => [e.dataset.combatant, e.querySelector('.nm').textContent.trim()]);
  // THE NAMES THE PAGE ITSELF GIVES THOSE WEAPONS, so the assertion is about
  // the seat being named by what it brought rather than about a locale.
  // WHICH ONE IS YOURS IS SAID IN FRONT OF IT, and every row still names a gun.
  out.want = [['wielder', 'cernos_prime'], ['seat2', 'braton_prime'], ['seat3', 'braton_prime']]
    .map(([id, w]) => [id, (id === 'wielder' ? tr('You') + ' · ' : '') + tf(weaponInfo(w).name)]);
  out.meterRows = [...document.querySelectorAll('#sim-results .mrow[data-combatant]')]
    .map(e => e.dataset.combatant);

  // ---- A RULER IS ONE GUN -------------------------------------------------
  const rows = await window.wfsim.do('shell.presets.list', { bar: 'scenario' });
  const ruler = ((rows || {}).rows || []).find(x => x.read_only || x.builtin);
  out.ruler = ruler && ruler.id;
  if (ruler) { await window.wfsim.do('shell.preset.open', { bar: 'scenario', preset: ruler.id }); }
  await sleep(1200);
  out.onRuler = typeof officialScenarioActive === 'function' && officialScenarioActive();
  out.rulerRoster = (sim.also_acting || []).length;
  out.rulerWire = (theFight().also_acting || []).length;
  out.rulerRows = document.querySelectorAll('#sim-roster .rs-row').length;
  return out;
})()`);

check("the roster opens as you alone", r.youOnly === 1, `${r.youOnly} rows`);
check("...and you cannot be taken out of your own fight", r.youHaveNoRemove === true);
check("+ another gun adds a seat", r.afterAdd === 2, `${r.afterAdd} rows`);
check("...with a weapon, one of its builds, and a way out",
  r.seatHasWeapon && r.seatHasPreset && r.seatHasRemove,
  JSON.stringify([r.seatHasWeapon, r.seatHasPreset, r.seatHasRemove]));
check("...born on the weapon in front of you, unmodded",
  r.bornOn && r.bornOn.weapon === "cernos_prime" && r.bornOn.preset === "default",
  JSON.stringify(r.bornOn));
check("...and the remove removes it", r.afterDrop === 1, `${r.afterDrop} rows`);

check("one seat sends no roster and gets one row back",
  r.soloWire === 0 && r.soloSeats === 1, JSON.stringify([r.soloWire, r.soloSeats]));
check("a second seat reaches the engine as a WHOLE build, not a weapon id",
  Array.isArray(r.wireIsWholeBuilds) && r.wireIsWholeBuilds[0] === "braton_prime",
  JSON.stringify(r.wireIsWholeBuilds));
check("...and comes back as its own seat, named by what it brought",
  JSON.stringify(r.pairSeats) === JSON.stringify([["wielder", "cernos_prime"], ["seat2", "braton_prime"]]),
  JSON.stringify(r.pairSeats));
check("...with a damage figure of its own", r.pairDamage[1] > 0, JSON.stringify(r.pairDamage));

check("a third seat is a third seat, not a second 'second'", r.threeSeats === 3, `${r.threeSeats}`);
check("a seat resolves ITS OWN build — the mods it names are on the wire",
  Array.isArray(r.armedCarries) && r.armedCarries.length === 1,
  JSON.stringify(r.armedCarries));
check("...so one gun in two builds deals two numbers",
  r.armedSeat > r.blankSeat * 1.2, `blank ${r.blankSeat}, armed ${r.armedSeat}`);

check("the record names the same seats the answer does",
  JSON.stringify(r.logSeats) === JSON.stringify([["wielder", "cernos_prime"],
    ["seat2", "braton_prime"], ["seat3", "braton_prime"]]),
  JSON.stringify(r.logSeats));
check("...and replays the fight all of them were in",
  JSON.stringify(r.logDealers) === JSON.stringify([0, 1, 2]), JSON.stringify(r.logDealers));

check("the panel draws a row per seat, named by the weapon it brought",
  r.want.length === 3 && JSON.stringify(r.seatRows) === JSON.stringify(r.want),
  `${JSON.stringify(r.seatRows)} against ${JSON.stringify(r.want)}`);
check("...and the damage cut names the same three",
  JSON.stringify(r.meterRows) === JSON.stringify(["wielder", "seat2", "seat3"]),
  JSON.stringify(r.meterRows));

check("an official ruler is the active scenario for this part", r.onRuler === true);
check("...and a ruler is ONE gun, whatever the last fight held",
  r.rulerRoster === 0 && r.rulerWire === 0 && r.rulerRows === 1,
  JSON.stringify([r.rulerRoster, r.rulerWire, r.rulerRows]));

await finish("a fight is n against m, and the page sets both halves");
