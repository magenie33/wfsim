// THE SEARCH RUNS IN THE BROWSER, AND SAYS WHAT IT COVERED.
//
// The optimizer's enumeration is no longer a depth-first walk of the whole
// scope: it walks a shuffled index range, so running to the end is an
// exhaustive enumeration and stopping early is a uniform sample
// (optimizer/src/space.rs). Two claims that have to hold ON SCREEN, in the
// single-threaded wasm build that actually ships:
//
//   - a scope small enough to finish reports `exhaustive` and says so;
//   - the run produces a real leaderboard either way.
//
// This is the end-to-end check for the whole path — parse → search → funnel →
// render — in the host where it is slowest and least like the tests.
//
//   node scripts/check_search.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/Verglas_Prime'); route(); await sleep(3500);
  const out = {};
  // A scope the browser can exhaust in seconds: six mods, full builds only.
  const req = {
    weapon: 'verglas_prime',
    mods: Object.fromEntries(['serration','split_chamber','point_strike','vital_sense','cryo_rounds','hellfire'].map(id => [id, 'search'])),
    build_size: 6, build_min: 6,
    enemy: 'thrax_centurion', level: 200, steel_path: true,
    duration: 8, runs: 3, finalists: 5,
  };
  // The optimize is a BACKGROUND JOB — post, then poll for the result, which
  // is also exactly the path the page takes.
  const runIt = async (body) => {
    await api('/api/optimize', body);
    for (let i = 0; i < 600; i++) {
      const st = await api('/api/optimize/status', {});
      if (st && (st.phase === 'done' || st.phase === 'cancelled' || st.phase === 'error')) return st.result;
      await sleep(500);
    }
    return null;
  };
  const small = await runIt(req);
  out.smallOk = !!(small && small.ok);
  out.exhaustive = small && small.exhaustive;
  out.coverage = small && small.coverage;
  out.space = small && small.space;
  out.results = small && (small.results || []).length;
  out.top = small && small.results && small.results[0] ? small.results[0].mods : null;

  // ...and the same scope with a budget so small it cannot finish: it must
  // come back with a real ranking and admit what it covered.
  // A fleet finishes a tiny space instantly, so the budgeted case needs a
  // scope it cannot: twelve mods, any size, a handful of evaluations each.
  const wide = ['serration','split_chamber','point_strike','vital_sense','cryo_rounds','hellfire',
                'infected_clip','stormbringer','malignant_force','rime_rounds','thermite_rounds','hammer_shot'];
  const big = await runIt({ ...req,
    mods: Object.fromEntries(wide.map(id => [id, 'search'])),
    build_size: 8, build_min: 1, max_evals: 40 });
  out.bigWorkers = woptWorkerCount();
  out.bigOk = !!(big && big.ok);
  out.bigExhaustive = big && big.exhaustive;
  out.bigCoverage = big && big.coverage;
  out.bigResults = big && (big.results || []).length;

  // THE POINT OF THE FLEET: N workers over disjoint strides must cover N
  // times the ground of one. Same scope, same per-worker budget, one worker.
  //
  // THE LANE COUNT COMES FROM THE TOPBAR NOW (owner, 2026-08-29): the search's
  // own CPU-threads box is gone, so one worker is asked for by shrinking the
  // page's compute share rather than by overriding it. Stubbing detectedCores
  // is what makes that exact — the share is a percentage of whatever THIS
  // machine reports, so 10% is one lane on eight cores and three on twenty-six.
  const pctWas = computePct;
  setComputePct(computeSteps()[0].pct);   // the narrowest share this machine offers
  out.soloWorkers = woptWorkerCount();
  const solo = await runIt({ ...req,
    mods: Object.fromEntries(wide.map(id => [id, 'search'])),
    build_size: 8, build_min: 1, max_evals: 40 });
  out.soloSampled = solo && solo.sampled;
  out.fleetSampled = big && big.sampled;
  setComputePct(pctWas);

  // What the page SAYS about each — the numbers are worth nothing if the
  // difference between "sampled" and "proven" never reaches the screen.
  try { renderOptResults(small); } catch (e) { out.renderErr = String(e).slice(0,200); }
  await sleep(100);
  out.smallText = ($('opt-results').querySelector('.opt-meta') || {}).textContent || '';
  try { renderOptResults(big); } catch (e) { out.renderErr2 = String(e).slice(0,200); }
  await sleep(100);
  out.bigText = ($('opt-results').querySelector('.opt-meta') || {}).textContent || '';
  return out;
})()`);

check("a small scope runs to a result", r.smallOk === true && r.results > 0, JSON.stringify(r.results));
check("...and reports itself EXHAUSTIVE", r.exhaustive === true, `coverage ${r.coverage}`);
check("...over a counted space", r.space > 0, String(r.space));
check("...with a build in it", Array.isArray(r.top) && r.top.length === 6, JSON.stringify(r.top));
check("the page says every build was searched", /every build|每一套/.test(r.smallText), JSON.stringify(r.smallText.slice(0, 160)));

check("a budgeted run still ranks", r.bigOk === true && r.bigResults > 0);
check("...and does NOT claim to be exhaustive", r.bigExhaustive === false);
check("...reporting a coverage below 1", r.bigCoverage > 0 && r.bigCoverage < 1, String(r.bigCoverage));
check(`the fleet ran ${r.bigWorkers} workers`, r.bigWorkers > 1, String(r.bigWorkers));
// …AND THE TOPBAR IS WHAT SET THAT (owner, 2026-08-29). The search's own CPU
// box is gone, so the only way a run can be made to use fewer lanes is the
// page's compute share — and if that were ignored, the two runs below would
// have covered the same ground and the next assertion would pass for the
// wrong reason.
check(`...and the compute share moved it (${r.bigWorkers} → ${r.soloWorkers})`,
  r.soloWorkers < r.bigWorkers, `${r.bigWorkers} vs ${r.soloWorkers}`);
check("...and covered more ground than one worker would",
  r.fleetSampled > r.soloSampled * 1.5,
  `fleet ${r.fleetSampled} at ${r.bigWorkers} lanes vs solo ${r.soloSampled} at ${r.soloWorkers}`);
check("the page says it sampled", /searched .*% of this scope|搜索覆盖了/.test(r.bigText), JSON.stringify(r.bigText.slice(0, 160)));

await app.finish("the search reports the ground it actually covered");
