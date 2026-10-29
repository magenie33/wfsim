import { openApp } from "./cdp.mjs";
import { taskServiceFixture } from "./user_task_service_fixture.mjs";
const fixture = await taskServiceFixture();
const app = await openApp({ base: fixture.base, lang: 'en', boot: 13000 });
try {
  await app.load('/weapons/Torid/simulator', 7000);
  const out = await app.evaluate(`(async () => {
    const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
    const until = async condition => { for (let i = 0; i < 600; i++) { if (await condition()) return; await sleep(100); } throw new Error('community check timeout'); };
    await initUserTasks(); gainPrefs.on = false; restartCalc(); workOnce = async () => false;
    localStorage.setItem(CONSENT_KEY, JSON.stringify({ on: true, v: COMPUTE_CONSENT_V, at: new Date().toISOString() }));
    computeFrozen = false;
    const original = simulateFleet; let entered = false;
    simulateFleet = (body, progress, options) => new Promise(resolve => { entered = true; options.signal.addEventListener('abort', () => resolve({ cancelled: true }), { once: true }); });
    activePreset = ''; sim.duration = 1; setSimRuns(129);
    const submitted = runSim();
    await until(() => entered);
    const task = userTasks.list().find(row => row.kind === 'simulation' && row.state === 'running');
    await userTasks.transfer(task.id, 'community'); simulateFleet = original;
    const transferred = userTasks.list().find(row => row.id === task.id);
    const out = { identity: transferred.id === task.id && transferred.destination === 'community', input: JSON.stringify(transferred.request) === JSON.stringify(task.request) };
    await until(() => foregroundHeld === 0);
    const originalLanes = communityLanes;
    for (const digit of ['2','3']) {
      communityLanes = () => digit === '2' ? 1 : 2;
      const verifier = digit.repeat(24);
      const work = await fetch('/test/claim', { method: 'POST', body: JSON.stringify({ verifier }) }).then(response => response.json());
      if (!work) throw new Error('no work for verifier ' + digit);
      await userCalculationOnce(work, verifier);
    }
    communityLanes = originalLanes;
    const done = await userTasks.wait(task.id);
    await submitted;
    out.completed = done.state === 'succeeded';
    out.record = !!results.find(record => record.id === done.result_ref && record.r.runs === 129);
    out.single = userTasks.list().filter(row => row.id === task.id).length === 1;
    out.outbox = !(await userTasks.store.value('community-reports')).length;
    const limits = normalizeLimits(null);
    limits.mods = 1; limits.exilus = false; limits.arcane_seats = limits.arcane_seats.map(() => false);
    limits.exclude.mods = buildPool().filter(mod => mod.id !== 'serration').map(mod => mod.id);
    applyOptState({ starts: [{ build: stateFromBuild({ mods: ['serration'] }, 'torid'), fixed: ['mods:0'] }], limits, candidate_runs: 1, finalists: 2 });
    const body = nameableOptimizeBody(optimizeBody()), origin = captureOptimizationOrigin();
    const searchTask = await userTasks.create('optimization', body, origin, 'community');
    await until(() => userTasks.list().find(row => row.id === searchTask.id)?.remote_command);
    for (const digit of ['4', '5']) {
      const verifier = digit.repeat(24);
      const work = await fetch('/test/claim', { method: 'POST', body: JSON.stringify({ verifier }) }).then(response => response.json());
      if (!work) throw new Error('No search work for verifier ' + digit);
      await userCalculationOnce(work, verifier);
    }
    const searchDone = await userTasks.wait(searchTask.id);
    const artifact = await userTasks.store.value('artifact:' + searchDone.result_ref);
    out.search = searchDone.state === 'succeeded' && artifact.result.results.length > 0;
    out.finalists = artifact.result.results.every(row => !!row.replay && !!row.forma) && artifact.result.target && artifact.request.finalists === 2;
    return out;
  })()`);
  for (const [key,label] of Object.entries({ identity:'transfer keeps one task identity', input:'transfer retains the frozen calculation', completed:'two real WASM witnesses complete the remote task', record:'the verified report enters the existing simulation result store', single:'handoff produces a single user-visible completion', outbox:'durable volunteer reports are acknowledged', search:'two real WASM searches agree on a complete optimization report', finalists:'the remote artifact preserves every finalist and its build-opening fields' })) app.check(label,out[key],JSON.stringify(out));
} finally { await fixture.close(); await app.close(); }
process.exitCode = app.failures ? 1 : 0;
console.log(app.failures ? `${app.failures} failed` : "community task execution works end to end");
