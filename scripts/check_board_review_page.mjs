import assert from "node:assert/strict";
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 13000, lang: "en" });
try {
  await app.load("/weapons/Laetum/simulator", 7000);
  const checks = await app.evaluate(`(async () => {
    gainPrefs.on = false; restartCalc(); await initUserTasks(); await loadBoard('laetum');
    const checks = {};
    localStorage.removeItem(SIM_RUNS_KEY); checks.defaultRuns = simRuns() === 1000;
    setSimRuns(17); checks.savedRuns = simRuns() === 17; setSimRuns(1000);
    localStorage.removeItem('wfsim-board-review'); checks.defaultAuto = boardReviewEnabled();
    localStorage.setItem('wfsim-board-review', 'off'); checks.disableAuto = !boardReviewEnabled();
    localStorage.removeItem('wfsim-board-review');
    await agentDo('shell.preset.open', { bar: 'scenario', preset: 'standard_single_target' });
    const fixture = async riven => {
      const build = builtinBuilds().find(p => p.benchmark === 'standard_single_target' && p.mode === 'cycle' && !!p.riven === riven && p.rank === 1);
      if (!build) throw new Error('Missing published Laetum fixture');
      await agentDo('shell.preset.open', { bar: 'build', preset: build.builtin });
      return { engine: ENGINE_ID, request: taskClone({ ...buildPayload(), ...theFight({ replay: true }) }),
        origin: { build: taskClone(snapshotState()), fight: taskClone(snapshotScenario()), benchmark: 'standard_single_target' } };
    };
    const plain = await fixture(false);
    checks.plain = !!await matchingBoardReview(plain);
    const riven = await fixture(true); checks.rivenCorner = !!await matchingBoardReview(riven);
    const altered = async (key, mutate) => { const task = taskClone(plain); mutate(task); checks[key] = !await matchingBoardReview(task); };
    await altered('runs', task => { task.request.runs--; });
    await altered('seed', task => { task.request.seed++; task.origin.fight.seed++; });
    await altered('duration', task => { task.request.duration--; task.origin.fight.duration--; });
    await altered('unknownTerm', task => { task.request.unknown_fight_term = 1; });
    await altered('arcaneRank', task => { task.request.arcane_rank = [0]; });
    await altered('participants', task => { task.request.also_acting = [{ weapon: 'furis' }]; });
    await altered('mods', task => { task.request.mods.reverse(); });
    const unrelated = taskClone(plain);
    unrelated.origin.fight.class_rules = { archgun: { infinite_ammo: true } };
    unrelated.request.class_rules = taskClone(unrelated.origin.fight.class_rules);
    unrelated.origin.fight.buffs = { ...unrelated.origin.fight.buffs, unrelated_weapon_buff: { stacks: 10, locked: true } };
    unrelated.request.buffs = taskClone(unrelated.origin.fight.buffs);
    checks.unrelatedGlobalSettings = !!await matchingBoardReview(unrelated);
    const buffs = (await api('/api/panel', plain.request)).buffs;
    if (!buffs.length) throw new Error('The fixture must have a relevant buff');
    await altered('relevantBuff', task => {
      task.origin.fight.buffs = { ...task.origin.fight.buffs, [buffs[0].id]: { stacks: buffs[0].default_stacks ? 0 : 1, locked: !buffs[0].default_locked } };
      task.request.buffs = taskClone(task.origin.fight.buffs);
    });
    const matchReview = matchingBoardReview, scheduler = scheduleBoardReviews, matched = await matchReview(plain);
    matchingBoardReview = async () => ({ ...matched, row: { ...matched.row, measured_by: ENGINE_ID } });
    scheduleBoardReviews = () => {};
    boardReviewTrackers.clear();
    const metric = metricOf(benchList().find(b => b.id === matched.ruler).scenario.metric);
    await requestBoardReview(plain, { duration: 60, [metric.field]: matched.row.score }, { id: 'same-number' });
    checks.currentScoreIgnored = boardReviewTrackers.size === 0;
    await requestBoardReview(plain, { duration: 60, [metric.field]: matched.row.score + 1 }, { id: 'changed-number' });
    checks.changedScoreTriggers = [...boardReviewTrackers.values()][0]?.state === 'submitting';
    boardReviewTrackers.clear(); await saveBoardReviewTrackers();
    matchingBoardReview = matchReview; scheduleBoardReviews = scheduler;
    const nativeFetch = window.fetch, refresh = refreshReviewedBoard;
    const requests = [], remote = crypto.randomUUID();
    let release;
    window.fetch = async (url, options) => {
      if (url === '/api/board/review') {
        requests.push(JSON.parse(options.body));
        return new Promise(resolve => { release = () => resolve(Response.json({ ok: true, id: remote })); });
      }
      if (url === '/api/board/review/status') return Response.json({ ok: true, reviews: [{ id: remote, state: 'succeeded', score: 12, witnesses: 2 }] });
      return nativeFetch(url, options);
    };
    const track = { id: 'test-outbox', identity: 'a'.repeat(32), weapon: 'laetum', ruler: 'standard_single_target', mode: 'cycle', engine: ENGINE_ID, source: 'test-report', sources: ['test-report'], state: 'submitting' };
    boardReviewTrackers.clear(); boardReviewTrackers.set(track.id, track); await saveBoardReviewTrackers();
    checks.outbox = (await userTasks.store.value('board-review-trackers'))[0].state === 'submitting';
    const accepting = acceptBoardReview(track);
    await Promise.resolve();
    await USER_TASK_TYPES.get('board-review').act('hide', track);
    release(); await accepting; checks.hiddenStaysHidden = boardReviewTrackers.size === 0;
    boardReviewTrackers.set(track.id, track);
    const second = acceptBoardReview(track); await Promise.resolve(); release(); await second;
    checks.targetOnly = requests.every(body => Object.keys(body).sort().join(',') === 'engine,identity,mode,ruler');
    checks.remoteObserved = boardReviewTrackers.get('board-review:' + remote)?.state === 'waiting';
    let refreshed = 0; refreshReviewedBoard = async () => { refreshed++; };
    await pollBoardReviews();
    checks.verified = boardReviewTrackers.get('board-review:' + remote)?.score === 12 && refreshed === 1;
    checks.noLocalExecutor = userTaskSources.get('board-review')()[0].state === 'succeeded';
    window.fetch = nativeFetch; refreshReviewedBoard = refresh;
    boardReviewTrackers.clear(); await saveBoardReviewTrackers(); clearTimeout(boardReviewTimer);
    return checks;
  })()`);
  for (const [name, passed] of Object.entries(checks)) { assert.equal(passed, true, name); console.log("  ok  " + name); }
  console.log(`\n${Object.keys(checks).length} board review page checks passed`);
} finally { await app.close(); }
