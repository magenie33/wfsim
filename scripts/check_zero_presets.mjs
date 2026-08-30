/// THE THIRTY-SIXTH: YOU OWN NOTHING UNTIL YOU MAKE SOMETHING.
///
/// Opening a weapon must not write a blank "build 1" into storage, opening the
/// app must not write a "scenario 1", and opening the optimizer must not write
/// a "search 1" — or a reader who browsed forty weapons owns forty builds they
/// never made. That is invisible while a preset is listed only in the bar of
/// the weapon you are standing on, and stops being invisible the moment one
/// page lists everything you own, where the answer becomes "everything".
///
/// THE RULE THIS LOOKS LIKE IT BREAKS, AND DOES NOT: "the modules always have a
/// state, and 'no build' is not something the builder can show". Still true. It
/// conflated the LIVE state with the SAVED one — the builder opens on
/// `blankBuildState()` and the simulator on an official ruler, which is a
/// BUILTIN and never was in these lists.
///
/// THE SHARP ONE IS THE LAST PAIR. A search is born from `updateOptEstimate`,
/// which — unlike the build's marker — also runs on every RENDER, including the
/// first paint of a tab nobody has touched. So "the funnel ran" is not evidence
/// of an edit and the state has to be. Opening the optimizer must create
/// nothing; changing the scope must create exactly one.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

const counts = `({
  builds: loadPresetList('builder-builds').length,
  scenarios: loadPresetList('simulator-scenarios').length,
  searches: loadPresetList('optimizer').length,
})`;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(3000);
  const fresh = Object.assign(${counts}, {
    active: activePreset,
    bareWeapon: slots.every(s => !s.mod) && (arcanes || []).every(a => !a || a === 'none'),
    // The bar must SURVIVE an empty collection: it is the only deliberate way
    // to make one, and a collection you can only fill by accident is worse
    // than one that fills itself.
    addThere: !!document.querySelector('#preset-bar-builder-builds .pchip.add'),
    // …and the simulator still has a fight, because a ruler is a builtin.
    fightThere: !!sim && typeof sim.level === 'number',
  });

  // BROWSING IS NOT OWNING.
  for (const w of ['Braton', 'Lex', 'Boltor']) {
    history.pushState({}, '', '/weapons/' + w); route(); await sleep(2200);
  }
  const browsed = ${counts};

  // OPENING THE OPTIMIZER IS NOT EDITING IT — the funnel runs on render.
  history.pushState({}, '', '/weapons/Torid/optimizer'); route(); await sleep(2600);
  updateOptEstimate(); await sleep(1200);
  const opened = ${counts};

  // …CHANGING THE SCOPE IS.
  opt.mods = { serration: 'search' };
  updateOptEstimate(); await sleep(1200);
  const scoped = Object.assign(${counts}, { active: activeOptPreset });

  // AND THE FIRST BUILD EDIT MAKES A BUILD.
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(2600);
  slots[0].mod = 'serration'; slots[0].rank = 10;
  markPresetDirty(); renderMods(); await sleep(1400);
  const edited = Object.assign(${counts}, {
    active: activePreset,
    kept: ((((loadPresetList('builder-builds')[0] || {}).state || {}).slots || [])[0] || {}).mod,
  });

  // THE LAST ONE IS DELETABLE, and what is left is a bare weapon rather than a
  // broken page — "there is always one" was only true while one was made for
  // you.
  const del = document.querySelector('#preset-bar-builder-builds .pchip.sel .pop.del');
  if (del) del.click();
  await sleep(1600);
  const deleted = Object.assign(${counts}, {
    active: activePreset,
    bareWeapon: slots.every(s => !s.mod),
    stillDrawn: document.querySelectorAll('#mod-slots .slot').length,
  });
  return { fresh, browsed, opened, scoped, edited, deleted };
})()`);

check("a fresh reader owns nothing at all",
  r.fresh.builds === 0 && r.fresh.scenarios === 0 && r.fresh.searches === 0,
  JSON.stringify(r.fresh));
check("...but the builder still shows a bare weapon", r.fresh.bareWeapon === true
  && r.fresh.active === "", `active="${r.fresh.active}"`);
check("...and the simulator still has a fight (a ruler is a builtin)",
  r.fresh.fightThere === true);
check("...and there is still a deliberate way to make one", r.fresh.addThere === true);

check("browsing three more weapons creates nothing", r.browsed.builds === 0,
  `${r.browsed.builds} builds after three weapons`);
check("opening the optimizer creates nothing", r.opened.searches === 0,
  `${r.opened.searches} searches from a render`);
check("...but changing the scope creates exactly one", r.scoped.searches === 1
  && r.scoped.active === "search 1", `${r.scoped.searches} · active="${r.scoped.active}"`);

check("the first build edit creates exactly one build", r.edited.builds === 1
  && r.edited.active === "build 1", `${r.edited.builds} · active="${r.edited.active}"`);
check("...carrying the edit that created it", r.edited.kept === "serration", String(r.edited.kept));

check("the last build can be deleted", r.deleted.builds === 0,
  `${r.deleted.builds} left · active="${r.deleted.active}"`);
check("...and what is left is a bare weapon, not a broken page",
  r.deleted.bareWeapon === true && r.deleted.stillDrawn === 8,
  `bare=${r.deleted.bareWeapon} slots=${r.deleted.stillDrawn}`);

await app.finish("nothing is owned until it is made");
