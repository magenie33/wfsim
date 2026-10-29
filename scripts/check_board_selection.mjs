// Board selection and labels describe one complete ruler on every surface.
import { openApp } from './cdp.mjs';
const app = await openApp({ boot: 9000, lang: 'en' });
try {
  await app.load('/benchmark', 3000);
  const result = await app.evaluate(`(async () => {
    if (typeof benchSelectorHtml !== 'function') return { shared: false };
    const out = { shared: true }, all = benchList();
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
    app.check('all cells retain their identity, order, summary and URL', result.cells.length === 0, JSON.stringify(result.cells));
    app.check('complete labels distinguish every board', result.unique);
    app.check('multiple-board selection preserves every cell and checked value', result.multiple);
    app.check('changing fight preserves enemy strength and operating conditions', result.preserve);
    app.check('the build finder uses the same selection', result.finder);
    app.check('an unmeasured selection stays available', result.empty);
    app.check('weapon boards, riven analysis and appraisal use the same hierarchy', result.surfaces);
    app.check('the simulator and optimizer share a hierarchical official-board picker', result.scenario && result.opened);
    app.check('official scenario labels use the same complete path', result.scenarioNames);
    app.check('existing task records display the same official board label', result.history);
    await app.send('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await app.load('/benchmark', 1000);
    app.check('the board selector stays within a phone viewport', await app.evaluate(`document.querySelector('#bench-picker .board-selector').getBoundingClientRect().right <= innerWidth && document.documentElement.scrollWidth <= innerWidth`));
    app.check('the official-board popover fits a phone viewport', await app.evaluate(`(async () => {
      history.pushState(null, '', '/weapons/Torid/simulator'); await route(); renderScenarioBar();
      const anchor = document.querySelector('[data-board-open]'); anchor.click();
      const rect = document.getElementById('board-choice-popover').getBoundingClientRect();
      return rect.left >= 0 && rect.right <= innerWidth && rect.top >= 0 && rect.bottom <= innerHeight;
    })()`));
  }
  await app.finish('board selection and display share one hierarchy');
} catch (error) { await app.close(); throw error; }
