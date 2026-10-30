// A RESULT IS A RECORD, AND A REPLAY IS NOT WHAT IT COSTS.
//
//   node scripts/check_storage.mjs
//
// The only check about what the app keeps on a reader's own machine. A replay
// is tens of times the summary it belongs to, and the one part a button
// regenerates (docs/UI.md §Results). Claims:
//
//   · A RUN IS RECORDED, with the engine that measured it, and the stored
//     record holds no replay …
//   · …while the panel still draws one from `resultMem`, because a fix that
//     removed the replay would pass the first assertion and break the feature.
//   · A SECOND RUN DOES NOT ERASE THE FIRST; a build keeps its newest few
//     unpinned records per scenario, and a pinned one past them.
//   · THE RECORDS SURVIVE A RELOAD, and a `lastResult` an older page left in a
//     saved build becomes a record and leaves the build.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const out = {};
  localStorage.clear();
  history.pushState({}, '', '/weapons/Phantasma_Prime/simulator'); route(); await sleep(3000);

  // A scenario of our own — an official ruler is pinned — with a CROWD on it,
  // because a replay follows up to REPLAY_TRACKED bodies and one target is
  // the cheapest case rather than the representative one.
  // A BUILD OF OUR OWN, FIRST. This check is about what a saved result COSTS,
  // and nothing is saved without something to save it into: since
  // "nothing is owned until it is made" a first-time visitor has
  // NO build, activePreset is blank, and saveSimResult returns having written
  // neither disk nor resultMem. That state is real and has its own check
  // (check_zero_presets.mjs); it is simply not the one this file is about, and
  // the file failed here rather than reporting it.
  const bbar = document.querySelector('#preset-bar-builder-builds');
  const badd = bbar && bbar.querySelector('.pchip.add');
  if (badd) { badd.click(); await sleep(1200); }
  out.hasBuild = loadPresetList(BUILDS).length > 0 && !!activePreset;

  const bar0 = document.querySelector('#preset-bar-simulator-scenarios');
  const add = bar0 && bar0.querySelector('.pchip.add');
  if (add) { add.click(); await sleep(1200); }
  sim.duration = 5; sim.runs = 20; sim.formation = [];
  for (let i = 0; i < 24; i++)
    sim.formation.push({ id: 'e' + (i + 2), at: [10 + (i % 6), -5 + Math.floor(i / 6) * 2] });
  renderSim(); await sleep(400);
  await runSim(); await sleep(2500);

  // WHAT IS STORED is read back from IndexedDB itself, not the mirror.
  const stored = () => new Promise((ok) => {
    const q = indexedDB.open('wfsim', 1);
    q.onsuccess = () => { const g = q.result.transaction('results').objectStore('results').getAll();
      g.onsuccess = () => { q.result.close(); ok(g.result); }; };
    q.onerror = () => ok(null);
  });
  const rows = (await stored()) || [];
  out.ran = (document.getElementById('sim-results') || {}).textContent.length > 200;
  out.recorded = rows.length;
  out.engine = rows[0] && 'engine' in rows[0] && rows[0].schema === RESULT_SCHEMA && rows[0].preset === activePreset;
  out.storedReplay = rows.some((x) => x.r && x.r.replay);
  out.storedChars = JSON.stringify(rows).length;

  // WHAT IT WOULD HAVE COST, so the assertion carries its own evidence.
  const mem = [...resultMem.values()][0];
  out.memEntries = resultMem.size;
  out.replayChars = JSON.stringify((mem && mem.r && mem.r.replay) || null).length;
  out.summaryChars = JSON.stringify({ ...(mem && mem.r), replay: null }).length;
  out.memHasReplay = !!(mem && mem.r && mem.r.replay);
  // …and the panel drew it. The .rp- prefix is the replay's own markup; the
  // hit account is the other thing that lives only in there.
  out.panelDrewReplay = !!document.querySelector(
    '#sim-results [class^="rp-"], #sim-results [class*=" rp-"]');

  // ---- A SECOND RUN DOES NOT ERASE THE FIRST -----------------------------
  const first = resultLatest(presetWeapon(), activePreset);
  const fake = (n) => ({ ...first.r, score: n });
  for (let i = 0; i < RESULT_KEEP + 2; i++) resultAdd(fake(i));
  out.kept = resultsFor(presetWeapon(), activePreset).length;
  out.latestIsNewest = resultLatest(presetWeapon(), activePreset).r.score === RESULT_KEEP + 1;
  resultKeep(resultsFor(presetWeapon(), activePreset).at(-1).id, true);
  out.pinnedId = resultsFor(presetWeapon(), activePreset).at(-1).id;
  for (let i = 0; i < RESULT_KEEP; i++) resultAdd(fake(100 + i));
  out.pinnedSurvives = resultsFor(presetWeapon(), activePreset).some((x) => x.id === out.pinnedId);
  out.weapon = presetWeapon(); out.preset = activePreset;

  // ---- THE HISTORY OPENS AN OLDER RUN IN THE BLOCK ABOVE -----------------
  renderStoredSimResult(); await sleep(300);
  const hrows = [...document.querySelectorAll('#sim-history [data-rview]')];
  out.historyRows = hrows.length;
  const older = hrows[2];
  if (older) { older.click(); await sleep(400); }
  const want = results.find((x) => x.id === resultViewing);
  out.viewing = !document.getElementById('sim-viewing').hidden && !!want && !!older
    && want.id === older.dataset.rview && shownResult.r.score === want.r.score;
  const latest = document.querySelector('#sim-viewing [data-rlatest]');
  if (latest) { latest.click(); await sleep(300); }
  out.backToNewest = document.getElementById('sim-viewing').hidden && resultViewing === null
    && shownResult.r.score === resultLatest(presetWeapon(), activePreset).r.score;
  await sleep(500);
  return out;
})()`);

check("the run produced a result", r.ran === true, JSON.stringify(r.ran));
// The precondition, stated: without a build there is nothing to record against,
// and every measurement below would be measuring zero.
check("...into a build of our own, which is what a result is recorded against",
  r.hasBuild === true && r.memEntries === 1,
  `build ${r.hasBuild}, resultMem ${r.memEntries}`);
check("the run is a stored record, naming its build and the engine that measured it",
  r.recorded === 1 && r.engine === true, JSON.stringify([r.recorded, r.engine]));
check(`a replay is ${Math.round(r.replayChars / 1024)} KB against a `
  + `${Math.round(r.summaryChars / 1024 * 10) / 10} KB summary — `
  + `${Math.round(r.replayChars / r.summaryChars)}x`,
  r.replayChars > 8000 && r.replayChars > r.summaryChars * 5,
  `${r.replayChars} / ${r.summaryChars}`);
check("…and the record never takes it",
  r.storedReplay === false && r.storedChars < 40000,
  `replay stored ${r.storedReplay}, ${r.storedChars} chars`);
check("…while the session still has it, and the panel drew it",
  r.memHasReplay === true && r.panelDrewReplay === true,
  `mem ${r.memHasReplay}, drawn ${r.panelDrewReplay}`);
check("a second run does not erase the first, and a build keeps its newest few",
  r.kept === 5 && r.latestIsNewest === true, JSON.stringify([r.kept, r.latestIsNewest]));
check("…and a pinned record outlives them", r.pinnedSurvives === true);
check("the history lists every run kept", r.historyRows === 6, JSON.stringify(r.historyRows));
check("…a row opens that run in the result block, and says so", r.viewing === true);
check("…and one click goes back to the newest", r.backToNewest === true);

// THE RECORDS SURVIVE A RELOAD, and a result an older page kept inside a build
// becomes a record of its own.
await evaluate(`(() => {
  localStorage.setItem('wfsim-presets-torid-builder-builds', JSON.stringify([{ id: 'old', name: 'old', savedAt: 1,
    state: {}, lastResult: { at: 7, key: JSON.stringify([[], [], [], {}, { enemy: 'x' }]), r: { score: 42, replay: { big: 1 } } } }]));
})()`);
await app.load("/");
const back = await evaluate(`(async () => {
  await new Promise((ok) => setTimeout(ok, 1500));
  const mine = resultsFor(${JSON.stringify(r.weapon)}, ${JSON.stringify(r.preset)});
  const old = resultLatest('torid', 'old');
  return {
    survived: mine.length, pinned: mine.some((x) => x.id === ${JSON.stringify(r.pinnedId)}),
    old: old && [old.r.score, old.at, old.fight && old.fight.enemy, 'engine' in old, old.r.replay],
    leftInBuild: /lastResult/.test(localStorage.getItem('wfsim-presets-builder-builds') || ''),
  };
})()`);
check("the records survive a reload, the pinned one with them",
  back.survived === 6 && back.pinned === true, JSON.stringify(back));
check("a result an older page kept in a build becomes a record, with what it held and no more",
  JSON.stringify(back.old) === JSON.stringify([42, 7, "x", false, null]) && back.leftInBuild === false,
  JSON.stringify([back.old, back.leftInBuild]));

// ---------------------------------------------------------------------------
// A RIVEN IS ADDRESSED BY WHAT IT IS AND FILTERED BY WHAT IT IS ABOUT.
//
// One list, each card carrying its own `scope`. What is asserted is the FOLD,
// because it is the only part that can still lose work — and it is asked of
// every key shape this app has ever written, at once:
//
//   burston_prime   the pre-family shape, a weapon id
//   burston-rifle   the family shape
//   burston_rifle   the shape the bug produced by slugging the family one
//   laetum-pistol   a second family holding a card of the SAME NAME
//   not_a_scope     a token nothing answers to
const card = (n, r) => ({ name: n, state: { bonuses: ["damage"], malus: null, rolls: [r] } });
const riven = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const go = async (p) => { history.pushState({}, '', p); route(); await sleep(2500);
    presetParseCache.clear(); return loadPresetList('rivens').map(x => x.name).sort(); };
  localStorage.clear();
  await go('/weapons/Burston_Prime/builder');
  const put = (k, v) => localStorage.setItem(k, JSON.stringify(v));
  put('wfsim-customs-burston_prime-rivens', [${JSON.stringify(card("from the prime", 0.1))}]);
  put('wfsim-customs-burston-rifle-rivens', [${JSON.stringify(card("family card", 0.2))}]);
  put('wfsim-customs-burston_rifle-rivens', [
    ${JSON.stringify(card("stranded", 0.3))}, ${JSON.stringify(card("family card", 0.2))}]);
  put('wfsim-customs-not_a_scope-rivens', [${JSON.stringify(card("nobody's", 0.4))}]);
  put('wfsim-customs-rivens', [{ scope: 'laetum-pistol', ...${JSON.stringify(card("family card", 0.9))} }]);
  foldRivensIntoOneList();
  presetParseCache.clear();
  return {
    keys: Object.keys(localStorage).filter(k => k.includes('riven')).sort(),
    whole: loadPresetWhole('rivens').map(x => (x.scope || '?') + ' / ' + x.name).sort(),
    onPrime: await go('/weapons/Burston_Prime/builder'),
    onBurston: await go('/weapons/Burston/builder'),
    onLaetum: await go('/weapons/Laetum/builder'),
    onTorid: await go('/weapons/Torid/builder'),
  };
})()`);

check("every riven list this app has written folds into one",
  riven.keys.join() === "wfsim-customs-rivens", riven.keys.join(" "));
// THE PRE-FAMILY SHAPE AND THE SLUGGED ONE BOTH RESOLVE, and the card the two
// old keys held twice is one card — a player who built the Burston's and the
// Prime's separately built one riven.
check("…every shape resolves, duplicates collapse, and a stranger is kept",
  riven.whole.join("|") === [
    "burston-rifle / family card",
    "burston-rifle / from the prime",
    "burston-rifle / stranded",
    "laetum-pistol / family card",
    "not_a_scope / nobody's",
  ].join("|"), riven.whole.join(" | "));
// A NAME IS NOT AN ADDRESS: two families each holding a "family card" is two
// cards, and the pool only ever offers one weapon's, so they never meet.
check("…a family sees its own, and a variant sees the same",
  riven.onPrime.join() === "family card,from the prime,stranded"
    && riven.onBurston.join() === riven.onPrime.join(),
  `${riven.onPrime.join(" ")} / ${riven.onBurston.join(" ")}`);
check("…another family sees only its own", riven.onLaetum.join() === "family card",
  riven.onLaetum.join(" "));
check("…and a weapon with no cards sees none", riven.onTorid.join() === "",
  riven.onTorid.join(" "));

await finish("storage is bounded by the summary, not by how hard you measured");
