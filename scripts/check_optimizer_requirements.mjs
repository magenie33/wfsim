// Global final-build requirements and each modular part survive start editing.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const r = await app.evaluate(`(async () => {
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  for (let i = 0; i < 300 && !META; i++) await wait(100);
  if (!META) throw new Error('the app did not finish loading metadata');
  const out = {};
  const open = async (weapon) => {
    history.pushState({}, '', weaponPath(weapon) + '/optimizer'); route(); await wait(1400);
    renderOpt(); copyActivePreset(optBarCfg()); await wait(200);
    opt.starts = [blankStart()]; renderOptStarts();
  };
  out.parts = [];
  for (const weapon of ['catchmoon_primary', 'plague_kripath_two_handed']) {
    await open(weapon);
    const before = JSON.stringify(snapshotState());
    editStart(0); await wait(500);
    const pins = [...document.querySelectorAll('#assembly-row .start-pin')];
    const original = { ...assembly };
    pins.forEach(p => p.click());
    renderAssembly(); await wait(200);
    const redraw = document.querySelectorAll('#assembly-row .start-pin.on').length;
    finishStartEdit(true); await wait(400);
    const payload = startPayload(opt.starts[0]);
    const board = headlessBoardPayloadFromResult({ mods: [], assembly: original }, { weapon, grip: 'wrong', loader: 'wrong', link: 'wrong' });
    out.parts.push({ weapon, count: pins.length, redraw,
      pair: JSON.stringify(payload.assembly) === JSON.stringify(original),
      board: board.grip === original.grip && (board.link || board.loader) === (original.link || original.loader),
      fixed: payload.fixed.filter(p => p.kind === 'assembly').map(p => p.idx),
      chips: document.querySelectorAll('#opt-starts .sb-chip.fixed').length,
      restored: JSON.stringify(snapshotState()) === before });
  }
  await open('kuva_bramma');
  editStart(0); await wait(400);
  document.querySelector('#element-block .start-pin').click();
  finishStartEdit(true); await wait(300);
  out.valence = startPayload(opt.starts[0]).fixed.some(p => p.kind === 'valence');
  out.valenceChip = !!document.querySelector('#opt-starts .sb-chip.fixed');

  await open('braton_prime');
  renderOptLimits();
  document.querySelector('#opt-limit-mods [data-require="serration"]').click();
  out.required = opt.limits.require.mods.slice();
  out.containsRequirement = optimizeBody().limits.require.mods.includes('serration');
  const saved = snapshotOpt();
  opt.limits.require.mods = []; applyOptState(saved);
  out.restoredRequirement = opt.limits.require.mods.includes('serration');
  opt.starts = [{ build: stateFromBuild({ mods: ['hellfire', 'cryo_rounds'] }, 'braton_prime'), fixed: [] }];
  opt.limits.mods = 2; limitsChanged();
  out.fullUnfixedAllowed = !startsBlocked();
  opt.starts[0].fixed = ['mods:0', 'mods:1']; limitsChanged();
  out.pinsConflict = !!startsBlocked() && $('run-opt').disabled;
  opt.starts[0].fixed = [];
  toggleLimit('mods', 'serration');
  out.exclusionConflict = startConflicts(opt.starts[0]).blocked.some(x => x.includes(requiredLabel('serration')));
  out.message = $('opt-estimate').textContent;
  document.querySelector('[data-require="serration"]').click();
  out.clearWorks = !opt.limits.require.mods.length && !startsBlocked();
  await open('dual_toxocyst');
  renderOptLimits();
  document.querySelector('#opt-limit-mods [data-require="semi_pistol_cannonade"]').click();
  out.evolutionConflict = !!startsBlocked();
  return out;
})()`);
console.log(JSON.stringify(r, null, 1));
for (const p of r.parts) {
  app.check(`${p.weapon}: both parts have independent pins after redraw`, p.count === 2 && p.redraw === 2);
  app.check(`${p.weapon}: payload and summary retain the pinned pair`, p.pair && p.fixed.join() === "0,1" && p.chips === 2);
  app.check(`${p.weapon}: editing restores the player's build`, p.restored);
  app.check(`${p.weapon}: board submissions use the searched parts`, p.board);
}
app.check("Kuva valence is pinnable and marked on its start card", r.valence && r.valenceChip);
app.check("a final requirement reaches the request and survives saved search state", r.required.join() === "serration" && r.containsRequirement && r.restoredRequirement);
app.check("an unfixed full start can make room for a required mod", r.fullUnfixedAllowed);
app.check("pinned slots and exclusions report contradictions and block running", r.pinsConflict && r.exclusionConflict, r.message);
app.check("removing the conflicting requirement enables running", r.clearWorks);
app.check("requirements use the whole weapon pool and report evolution conflicts", r.evolutionConflict);
await app.finish("optimizer requirements and part pins hold");
