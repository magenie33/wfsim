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
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, resolve } from "node:path";
const ROOT = resolve("site");
const MIME = { ".html":"text/html",".js":"text/javascript",".css":"text/css",".json":"application/json",".wasm":"application/wasm",".svg":"image/svg+xml",".png":"image/png",".jpg":"image/jpeg",".ico":"image/x-icon" };
const srv = createServer(async (q,s)=>{const p=decodeURIComponent(q.url.split("?")[0]);try{const b=await readFile(join(ROOT,p));s.writeHead(200,{"content-type":MIME[extname(p)]||"application/octet-stream","cache-control":"no-store"});s.end(b);}catch{s.writeHead(200,{"content-type":"text/html"});s.end(await readFile(join(ROOT,"index.html")));}});
await new Promise(r=>srv.listen(0,"127.0.0.1",r));
const BASE=`http://127.0.0.1:${srv.address().port}`, PORT=9495;
const proc=spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",[`--remote-debugging-port=${PORT}`,"--headless=new","--disable-gpu","--no-first-run",`--user-data-dir=${process.env.TEMP}/wfsim-search-check`,"about:blank"],{stdio:"ignore"});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function cdp(path){for(let i=0;i<60;i++){try{const r=await fetch(`http://127.0.0.1:${PORT}${path}`);if(r.ok)return r.json();}catch{}await sleep(250);}throw new Error("no CDP");}
const page=(await cdp("/json/list")).find(t=>t.type==="page");
const ws=new WebSocket(page.webSocketDebuggerUrl);
let id=0;const pending=new Map();
ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.id&&pending.has(m.id)){pending.get(m.id)(m);pending.delete(m.id);}};
await new Promise(r=>ws.onopen=r);
const send=(method,params={})=>new Promise(res=>{const i=++id;pending.set(i,res);ws.send(JSON.stringify({id:i,method,params}));});
const evaluate=async expr=>{const r=await send("Runtime.evaluate",{expression:expr,awaitPromise:true,returnByValue:true});if(r.result?.exceptionDetails)throw new Error(String(r.result.exceptionDetails.exception?.description||"").slice(0,600));return r.result?.result?.value;};
await send("Page.enable");await send("Runtime.enable");
await send("Page.navigate",{url:BASE});await sleep(12000);
let fail=0;const check=(n,ok,d)=>{console.log(`${ok?"  ok  ":"FAIL  "}${n}${ok||d===undefined?"":`  — ${d}`}`);if(!ok)fail++;};

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
  optRun.threads = 1;
  const solo = await runIt({ ...req,
    mods: Object.fromEntries(wide.map(id => [id, 'search'])),
    build_size: 8, build_min: 1, max_evals: 40 });
  out.soloSampled = solo && solo.sampled;
  out.fleetSampled = big && big.sampled;
  optRun.threads = 0;

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
check("...and covered more ground than one worker would",
  r.fleetSampled > r.soloSampled * 1.5,
  `fleet ${r.fleetSampled} vs solo ${r.soloSampled} index positions`);
check("the page says it sampled", /searched .*% of this scope|搜索覆盖了/.test(r.bigText), JSON.stringify(r.bigText.slice(0, 160)));

ws.close(); srv.close(); proc.kill();
process.exit(fail ? 1 : 0);
