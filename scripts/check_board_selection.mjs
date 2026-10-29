// Board selection and labels describe one complete ruler on every surface.
import { openApp } from './cdp.mjs';
const app = await openApp({ boot: 9000, lang: 'en' });
try {
  await app.load('/benchmark', 3000);
  const result = await app.evaluate(`(async () => {
    if (typeof benchSelectorHtml !== 'function') return { shared: false };
    const out = { shared: true }, all = benchList();
    const kitguns = META.weapons.filter(w => w.assembly);
    out.kitgunSlots = kitguns.length > 0 && kitguns.every(w => {
      const pair = kitguns.filter(k => k.assembly.chamber === w.assembly.chamber);
      const entries = benchEntries(all[0], pair);
      return pair.length === 2 && new Set(entries.map(e => e.w.id)).size === 2
        && entries.every(e => benchRowHtml(all[0], e, 0).includes('(' + tr(ARC_POOL_LABEL[e.w.slot]) + ')'));
    });
    const weapons = META.weapons.slice(0, 2).map(w => ({ ...w, modes: ['base', 'cycle'] }));
    const saved = weapons.map(w => BOARD[w.id]), priorView = benchRivenView, priorMeasured = benchMeasuredOnly;
    try {
      const ruler = all[0], row = (mode, score, riven = null) => ({ benchmark: ruler.id, mode, score, mods: [], riven });
      const rows = [row('base', 30), row('cycle', 25), row('cycle', 40, { bonuses: [], malus: null })];
      BOARD[weapons[0].id] = rows; BOARD[weapons[1].id] = [];
      benchRivenView = 'all'; benchMeasuredOnly = false;
      const entries = benchEntries(ruler, weapons), best = entries.find(e => e.w.id === weapons[0].id);
      out.oneWeapon = entries.length === 2 && new Set(entries.map(e => e.w.id)).size === 2 && best.row.score === 40;
      const html = benchRowHtml(ruler, best, 0);
      out.winnerLink = html.includes('mode=cycle') && html.includes('riven=1') && !html.includes('class="bmode"');
      benchRivenView = 'plain';
      out.viewWinner = benchEntries(ruler, weapons).find(e => e.w.id === weapons[0].id).row.score === 30;
      benchRivenView = 'all'; benchMeasuredOnly = true;
      out.measuredWeapons = benchEntries(ruler, weapons).length === 1;
      out.modeRecords = BOARD[weapons[0].id] === rows && rows.length === 3
        && new Set(rankBoard(rows, [ruler.id], weapons[0].modes).map(e => e.mode)).size === 2;
    } finally {
      weapons.forEach((w, i) => { BOARD[w.id] = saved[i]; });
      benchRivenView = priorView; benchMeasuredOnly = priorMeasured;
    }
    const failures = [];
    for (const b of all) {
      benchPick = b.id; renderBenchBoard();
      const root = document.querySelector('#bench-picker .board-selector');
      const labels = [...root.querySelectorAll('.slotf-lab')].map(x => x.textContent);
      if (JSON.stringify(labels) !== JSON.stringify(['Board scenario', 'Enemy strength', 'Operating conditions', 'Other conditions'])) failures.push(b.id + ': hierarchy');
      if (root.dataset.boardSelected !== b.id || root.querySelector('.board-choice-summary').textContent !== benchmarkName(b.id)) failures.push(b.id + ': summary');
      for (const button of root.querySelectorAll('[data-board-choice]')) {
        if (!all.some(x => x.id === button.dataset.boardChoice)) failures.push(b.id + ': unknown ruler');
      }
      benchWriteUrl(); benchPick = null; benchReadUrl();
      if (benchCurrent().id !== b.id) failures.push(b.id + ': URL');
    }
    out.cells = failures;
    out.unique = new Set(all.map(b => benchShort(b.id))).size === all.length;
    const checklist = document.createElement('div');
    checklist.innerHTML = benchChecklistHtml(all.map(b => b.id), new Set([all[0].id]));
    out.multiple = checklist.querySelectorAll('[data-rb]').length === all.length
      && checklist.querySelectorAll('[data-rb]:checked').length === 1;
    const custom = all.find(b => b.ladder.find(x => x.axis === 'scene')?.tag === 'sp200' && b.scenario.headshot_pct === 50);
    const next = benchChoose(custom.id, 'base', all.find(b => b.base !== custom.base).base);
    out.preserve = benchSceneOf(all.find(b => b.id === next)) === benchSceneOf(custom);
    history.pushState(null, '', '/weapons/Torid'); await route(); await loadBoard('torid');
    finder.weapon = null; renderBuildFinder();
    out.allModes = finder.mo === FINDER_EVERY_MODE && !!document.querySelector('#build-finder [data-fseg="mo"][data-v="*"].on');
    finder.b = custom.id; renderBuildFinder();
    out.finder = document.querySelector('#build-finder [data-board-selected]')?.dataset.boardSelected === custom.id;
    const before = BOARD.torid;
    try {
      BOARD.torid = []; renderBuildFinder();
      out.empty = document.querySelector('#build-finder [data-board-selected]')?.dataset.boardSelected === custom.id;
    } finally { BOARD.torid = before; renderBuildFinder(); }
    raRuler = custom.id; renderRivenAnalyst();
    appraisalRuler = custom.id; renderRivenAppraisalControls();
    wbenchRuler = custom.id; renderWeaponBench();
    out.surfaces = ['riven-analyst', 'riven-appraisals', 'wbench'].every(id => document.querySelector('#' + id + ' [data-board-selected]')?.dataset.boardSelected === custom.id);
    renderScenarioBar();
    const anchor = document.querySelector('[data-board-open]'); anchor.click();
    const pop = document.getElementById('board-choice-popover');
    out.scenario = !!pop && !pop.hidden && pop.querySelectorAll('[data-board-axis]').length === 4;
    const target = pop.querySelector('[data-board-axis="shooting"] [data-board-choice]:last-child');
    const id = target.dataset.boardChoice; target.click(); pop.querySelector('[data-board-apply]').click();
    out.opened = scenarioNamed(activeScenario)?.builtin === id;
    out.scenarioNames = builtinScenarios().every(p => p.name === benchmarkName(p.builtin));
    const note = USER_TASK_TYPES.get('optimization').describe({ request: { weapon: 'torid' }, origin: { scenario: custom.id, scenario_name: 'old label' } }).note;
    out.history = note.includes(benchmarkName(custom.id)) && !note.includes('old label');
    return out;
  })()`);
  app.check('all board entry points use the shared hierarchy', result.shared === true);
  if (result.shared) {
    app.check('the overview lists each weapon once at its highest score', result.oneWeapon);
    app.check('winning-build links retain mode while the overview has no mode badge', result.winnerLink);
    app.check('build filters choose the highest matching score for each weapon', result.viewWinner);
    app.check('hiding unmeasured weapons retains one measured entry', result.measuredWeapons);
    app.check('the overview preserves the original per-mode build groups', result.modeRecords);
    app.check('primary and secondary Kitguns remain separate and visibly name their slots', result.kitgunSlots);
    app.check('all cells retain their identity, order, summary and URL', result.cells.length === 0, JSON.stringify(result.cells));
    app.check('complete labels distinguish every board', result.unique);
    app.check('multiple-board selection preserves every cell and checked value', result.multiple);
    app.check('changing fight preserves enemy strength and operating conditions', result.preserve);
    app.check('the build finder uses the same selection', result.finder);
    app.check('the build finder defaults to all usage modes', result.allModes);
    app.check('an unmeasured selection stays available', result.empty);
    app.check('weapon boards, riven analysis and appraisal use the same hierarchy', result.surfaces);
    app.check('the simulator and optimizer share a hierarchical official-board picker', result.scenario && result.opened);
    app.check('official scenario labels use the same complete path', result.scenarioNames);
    app.check('existing task records display the same official board label', result.history);
    await app.send('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await app.load('/benchmark', 1000);
    app.check('the board selector stays within a phone viewport', await app.evaluate(`document.querySelector('#bench-picker .board-selector').getBoundingClientRect().right <= innerWidth && document.documentElement.scrollWidth <= innerWidth`));
    const popoverBounds = () => app.evaluate(`(async () => {
      history.pushState(null, '', '/weapons/Torid/simulator'); await route(); renderScenarioBar();
      const anchor = document.querySelector('[data-board-open]'); anchor.click();
      const rect = document.getElementById('board-choice-popover').getBoundingClientRect();
      return { fits: rect.left >= 0 && rect.right <= innerWidth && rect.top >= 0 && rect.bottom <= innerHeight,
        left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom, width: innerWidth, height: innerHeight };
    })()`);
    const phone = await popoverBounds();
    app.check('the official-board popover fits a phone viewport', phone.fits, JSON.stringify(phone));
    await app.send('Emulation.setDeviceMetricsOverride', { width: 390, height: 360, deviceScaleFactor: 1, mobile: true });
    const short = await popoverBounds();
    app.check('the official-board popover fits a short viewport', short.fits, JSON.stringify(short));
  }
  await app.finish('board selection and display share one hierarchy');
} catch (error) { await app.close(); throw error; }
