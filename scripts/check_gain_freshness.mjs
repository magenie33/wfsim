// A SCENARIO EDIT REACHES THE QUICK CALC IMMEDIATELY.
//
// The gain scan is measured under a scenario, so when the scenario changes the
// numbers on screen are answers to a question nobody is asking any more. The
// cache key used to name the scenario's fields ONE BY ONE and the list had
// drifted: `buffs` was missing, so raising a buff's starting stacks changed
// what the scan would measure without changing the key, and a stale ranking
// stayed on screen looking current (user, 2026-08-03: "我如果把某些buff初始调高
// 了，没有立刻生效").
//
// Asserts the two halves separately, because they fail separately: the key
// must MOVE when the fight changes, and something must then RE-RUN.
//
//   node scripts/check_gain_freshness.mjs
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
const BASE=`http://127.0.0.1:${srv.address().port}`, PORT=9493;
const proc=spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",[`--remote-debugging-port=${PORT}`,"--headless=new","--disable-gpu","--no-first-run",`--user-data-dir=${process.env.TEMP}/wfsim-gain-fresh`,"about:blank"],{stdio:"ignore"});
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
  history.pushState({},'','/weapons/Torid'); route(); await sleep(3500);
  const out = {};

  // Every scenario field must reach the key, so walk a representative set of
  // them rather than the one that broke — the point is that NOTHING is left out.
  const keyNow = () => gainKey();
  const moved = async (label, mutate) => {
    const before = keyNow();
    mutate();
    markScenarioDirty();
    await sleep(700);            // the scenario auto-save debounce, then the refresh
    return { label, changed: keyNow() !== before };
  };

  out.buffs = await moved('buffs', () => {
    const id = (buffList[0] || {}).id || 'test_buff';
    sim.buffs = { ...sim.buffs, [id]: { stacks: 7, locked: false } };
  });
  out.metric = await moved('metric', () => { sim.metric = sim.metric === 'dps' ? 'kpm' : 'dps'; });
  out.level  = await moved('level',  () => { sim.level = sim.level === 100 ? 200 : 100; });
  out.dur    = await moved('duration', () => { sim.duration = sim.duration === 60 ? 90 : 60; });

  // ...and a field nobody has invented yet: the key is DERIVED from the
  // scenario payload, so an unknown one counts too.
  out.future = await moved('a field added later', () => { sim.some_future_knob = 42; });

  // The second half: after the edit something must actually re-run, so the
  // scan's own key catches up with the fight instead of sitting stale.
  gainPrefs = { ...gainPrefs, on: true };
  await sleep(200);
  openPicker(0, slotEl(0));   // (slotIdx, anchor) — the order the app uses
  for (let i = 0; i < 60 && (gainScan.running || gainScan.key === null); i++) await sleep(500);
  out.scanned = gainScan.key !== null;
  out.freshBefore = gainScan.key === gainKey();
  const keyBefore = gainScan.key;
  // Edit the fight and touch NOTHING else — no reopening, no clicking.
  sim.level = sim.level === 500 ? 600 : 500;
  markScenarioDirty();
  for (let i = 0; i < 60; i++) { await sleep(500); if (!gainScan.running && gainScan.key === gainKey()) break; }
  out.freshAgain = gainScan.key === gainKey();
  // ...and it is a NEW scan under the NEW fight, not the old one still sitting
  // there because the key never noticed. This is the assertion that fails if
  // the key goes back to naming scenario fields by hand.
  out.rescanned = gainScan.key !== keyBefore;

  // AN EDIT MID-SCAN DOES NOT WAIT FOR THE STALE SCAN.
  //
  // A scan is ~90 serial simulate calls. The old guard was "a scan is running,
  // so ignore this edit", so a change made midway was DROPPED: the stale scan
  // ran to the end under the config you had just left and only then did the
  // refresh restart it. Measured against the cost of a full scan, which is the
  // only honest reference — a fixed millisecond budget would pass on a fast
  // machine and fail on a slow one for no reason.
  const settle = async () => {
    for (let i = 0; i < 200 && (gainScan.running || gainScan.key !== gainKey()); i++) await sleep(100);
    return !gainScan.running && gainScan.key === gainKey();
  };
  await settle();

  // THE SCAN RUNS WIDE. ~80 candidates x 10 runs is ~800 engagements, and they
  // all went down ONE worker until 2026-08-03 while the optimizer next door ran
  // a fleet — which is why a rich build made the quick calc feel like a search.
  const lanes = gainLanes();
  out.laneCount = lanes.length;

  // ...and it is INTERRUPTIBLE. Forced down a single lane so the scan is slow
  // enough to be caught mid-flight on purpose — a timing race against an
  // 800 ms scan proves nothing on a fast machine.
  gainPool = [lanes[0]];
  try {
    // A RICH build, because that is what makes a scan slow enough to interrupt
    // — and slow is the case that matters. An empty build's fight is over in
    // microseconds; seven mods means multishot, crit, status and elements all
    // live, and one engagement generates several times the procs and DoT ticks.
    ['serration','split_chamber','point_strike','vital_sense','hellfire','infected_clip','stormbringer']
      .forEach((m, i) => { slots[i] = { ...(slots[i] || {}), mod: m, rank: null }; });
    await sleep(100);
    // On the MOD axis — ~80 candidates, and one lane makes each one slow
    // enough that "midway" is a real place rather than a race.
    openPicker(0, slotEl(0));
    await settle();
    const axis = { kind: 'mods', idx: 0 };
    out.modAxisTotal = gainScan.total;

    const p1 = scanGains(axis, () => {});          // measuring fight A
    for (let i = 0; i < 400 && !(gainScan.running && gainScan.done > 0); i++) await sleep(25);
    out.caughtMidScan = gainScan.running && gainScan.done > 0;
    out.abandonedAt = gainScan.done;
    out.staleTotal = gainScan.total;
    const staleKey = gainScan.key;

    // The fight moves — written straight into the PRESET, because that is what
    // a scan resolves and waiting out the auto-save debounce would let the
    // scan we are trying to interrupt finish first.
    const ps = loadPresetList(SCENARIOS);
    const at = ps.findIndex((x) => x.name === activeScenario);
    ps[at] = { ...ps[at], state: { ...ps[at].state, level: 900 } };
    storePresetList(SCENARIOS, ps);
    out.stillRunningAtCut = gainScan.running;
    const t0 = Date.now();
    const p2 = scanGains(axis, () => {});          // ...and B takes over at once
    out.supersedeMs = Date.now() - t0;
    out.supersededKey = gainScan.key !== staleKey;
    await Promise.all([p1, p2]);
    // B finished as itself: a superseded A that kept writing would push its done
    // past B's total and scribble A's answers into B's table.
    out.overrun = gainScan.done > gainScan.total;
    out.finishedClean = gainScan.done === gainScan.total;

    // NO PING-PONG. refreshGains ends in renderEvo, so the evolution axis asks
    // for a scan on EVERY repaint while the picker's own scan is still running.
    // If the newest request always won, the two would cancel each other forever
    // and neither would ever finish. Only a stale fight supersedes; a different
    // axis waits its turn.
    const p3 = scanGains({ kind: 'mods', idx: 0 }, () => {});
    for (let i = 0; i < 400 && !(gainScan.running && gainScan.done > 0); i++) await sleep(25);
    const heldAxis = JSON.stringify(gainScan.axis);
    ensureGains({ kind: 'evo', idx: 0 }, () => {});
    await sleep(50);
    out.axisHeld = JSON.stringify(gainScan.axis) === heldAxis && gainScan.running === true;
    out.heldAxis = heldAxis;
    await p3;
  } finally {
    gainPool = lanes;
  }
  await settle();

  // The wording: the block says what the numbers in it MEAN.
  const hint = document.querySelector('#sim-buffs')?.previousElementSibling?.querySelector('.sim-hint');
  out.hint = (hint?.textContent || '').trim();
  return out;
})()`);

for (const k of ["buffs", "metric", "level", "dur", "future"]) {
  check(`a change to ${r[k].label} moves the scan key`, r[k].changed === true);
}
check("a scan ran at all", r.scanned === true);
check("its key matched the fight it measured", r.freshBefore === true);
check("editing the fight re-runs it without reopening anything", r.freshAgain === true);
check("and what is on screen was measured under the NEW fight", r.rescanned === true);
check(`the scan runs over ${r.laneCount} lanes, not one`, r.laneCount > 1, String(r.laneCount));
check("a scan was caught mid-flight to interrupt", r.caughtMidScan === true);
check("an edit mid-scan supersedes it instead of queueing behind it",
  r.supersededKey === true, `took ${r.supersedeMs} ms`);
check("...the scan it interrupted was genuinely still going", r.stillRunningAtCut === true);
check("...and it gave way early rather than running to its end",
  r.abandonedAt < r.staleTotal, `abandoned at ${r.abandonedAt} of ${r.staleTotal}`);
check("...leaving the new scan's own tally intact", r.overrun === false && r.finishedClean === true,
  `done ${r.abandonedAt}, overrun ${r.overrun}`);
check("a different axis does not preempt a running scan", r.axisHeld === true,
  `axis was ${r.heldAxis}`);
check("the buff block says the stacks are the START", /START|开战/.test(r.hint), JSON.stringify(r.hint));

ws.close(); srv.close(); proc.kill();
process.exit(fail ? 1 : 0);
