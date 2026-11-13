import { openApp } from './cdp.mjs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export async function checkSimulationContext(app) {
  await app.load('/weapons/Kuva_Nukor?bench=standard_multi_target&riven=1&view=library', 4500);
  const result = await app.evaluate(`(async () => {
    const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
    gainPrefs.on = false; saveGainPrefs(); localStorage.setItem('wfsim-board-review', 'off');
    await loadBoard('kuva_nukor');
    const top = builtinBuilds('standard_multi_target').find(p => p.riven && p.rank === 1);
    await openLibraryBuild(top); copyActivePreset(buildBarCfg());
    history.pushState(null, '', '/weapons/Kuva_Nukor/simulator'); await route();
    pickPreset(scenarioBarCfg(), 'standard_multi_target'); copyActiveScenario();
    setSimRuns(2); setScenarioFields({ duration: 3 });
    await sleep(700);
    const firstScene = activeScenario;
    await runSim(); await sleep(400); renderStoredSimResult();
    const first = resultLatest(presetWeapon(), activePreset);
    const firstKey = recordKey(shownResult.r);
    const read = () => ({
      score: shownResult?.r.score,
      unit: document.querySelector('#sim-results .hero-unit')?.textContent,
      head: [...document.querySelectorAll('#sim-results .fc')].find(el => el.textContent.startsWith('Headshots'))?.textContent,
      stale: !!$('sim-result-context') && !$('sim-result-context').hidden,
      selected: document.querySelector('#sim-history tr.on')?.dataset.rview,
      agent: agentResult(),
    });
    copyActiveScenario();
    const secondScene = activeScenario;
    setScenarioFields({ headshot_pct: 0, metric: 'dps' });
    const immediate = read();
    renderStoredSimResult();
    const redrawn = read(), keyAfterEdit = recordKey(shownResult.r);
    await runSim(); await sleep(400); renderStoredSimResult();
    const second = resultLatest(presetWeapon(), activePreset);
    const secondView = read();
    const choose = async id => {
      document.querySelector('[data-dd="dd-preset-simulator-scenarios"]').click();
      [...document.querySelectorAll('#dd-menu .opt')].find(el => el.dataset.v === id).click();
      await sleep(100);
    };
    await choose(firstScene); const returned = read();
    await choose(secondScene);
    [...document.querySelectorAll('#sim-history [data-rview]')].find(el => el.dataset.rview === first.id).click();
    const historical = read();
    const request = first.request, originalApi = api, calls = [];
    let legacyRefused = false, olderEngineRefused = false;
    try {
      api = async (path, body, ...args) => {
        if (path === '/api/log') { calls.push(structuredClone(body)); return { ok: true, events: [], dropped: 0 }; }
        return originalApi(path, body, ...args);
      };
      await loadRecord(shownResult.r, 0, null);
      const measuredCalls = calls.length;
      renderResults(first.r, first.at, first.build, { ...first, schema: 1, request: undefined });
      await loadRecord(shownResult.r, 0, null);
      legacyRefused = calls.length === measuredCalls && !!recordState.error;
      renderResults(first.r, first.at, first.build, { ...first, engine: 'other-engine' });
      await loadRecord(shownResult.r, 0, null);
      olderEngineRefused = calls.length === measuredCalls && !!recordState.error;
    } finally { api = originalApi; }
    const log = calls.at(-1);
    const beforeValence = simKey(), originalValence = { ...valence };
    valence = { ...valence, element: valence.element === 'toxin' ? 'magnetic' : 'toxin' };
    const valenceIdentity = simKey() !== beforeValence;
    valence = originalValence;
    return { immediate, redrawn, secondView, returned, historical, firstKey, keyAfterEdit,
      firstScore: first.r.score, secondScore: second.r.score, firstId: first.id,
      captured: !!request && request.headshot_pct === 100 && request.formation.length === 24 && request.runs === 2,
      recordRequest: !!request && !!log && log.headshot_pct === 100 && log.metric === 'kpm'
        && JSON.stringify(log.mods) === JSON.stringify(request.mods)
        && JSON.stringify(log.formation) === JSON.stringify(request.formation),
      legacyRefused, olderEngineRefused, valenceIdentity,
    };
  })()`);
  app.check('editing the scenario immediately marks the displayed measurement stale', result.immediate.stale, JSON.stringify(result.immediate));
  app.check('a redraw retains the measured headshot rate and metric', result.redrawn.head === 'Headshots 100%' && result.redrawn.unit === 'KPM' && result.redrawn.stale, JSON.stringify(result.redrawn));
  app.check('a new measurement records the complete executed request', result.captured);
  app.check('switching scenarios restores their own matching measurements', result.returned.score === result.firstScore && result.returned.selected === result.firstId
    && result.returned.unit === 'KPM' && !result.returned.stale && result.secondView.unit === 'DPS', JSON.stringify(result.returned));
  app.check('an older history row keeps its recorded conditions under another scenario', result.historical.score === result.firstScore
    && result.historical.head === 'Headshots 100%' && result.historical.unit === 'KPM' && result.historical.stale, JSON.stringify(result.historical));
  app.check('editing the scenario does not change the displayed combat record identity', result.firstKey === result.keyAfterEdit);
  app.check('reading an older combat record submits its captured build and fight', result.recordRequest);
  app.check('missing historical input and another engine cannot regenerate against the editor', result.legacyRefused && result.olderEngineRefused);
  app.check('changing the Kuva progenitor changes the measurement identity', result.valenceIdentity);
  app.check('the agent readout uses the same measurement and units as the page', result.returned.agent.metric === 'kpm'
    && result.returned.agent.fresh && result.historical.agent.metric === 'kpm' && !result.historical.agent.fresh);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const app = await openApp({ boot: 1500, lang: 'en' });
  try { await checkSimulationContext(app); await app.finish('simulation results retain their measured inputs'); }
  finally { await app.close(); }
}
