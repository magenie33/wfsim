// THE REPLAY, driven in a browser: the median engagement plays back.
//
// It exists because a replay that shows the WRONG fight is worse than none —
// the engine's own test proves the run is reproduced bit-for-bit, and this
// proves the page shows it: the curve is drawn, the pools drain as the cursor
// moves, and pressing play advances the clock at the chosen multiplier.
//
//   node scripts/check_replay.mjs
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
const BASE=`http://127.0.0.1:${srv.address().port}`, PORT=9489;
const proc=spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",[`--remote-debugging-port=${PORT}`,"--headless=new","--disable-gpu","--no-first-run",`--user-data-dir=${process.env.TEMP}/wfsim-replay-check`,"about:blank"],{stdio:"ignore"});
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
  history.pushState({},'','/weapons/Cernos_Prime'); route(); await sleep(3000);
  ['primed_cryo_rounds','serration','point_strike','vital_sense'].forEach((m,i)=>{
    if (modById(m)) { slots[i].mod=m; slots[i].rank=modById(m).max_rank; }});
  arcanes=['primary_frostbite'];
  sim.level=300; sim.steel_path=true; sim.duration=60; sim.runs=8;
  markPresetDirty(); markScenarioDirty(); renderMods(); refreshPanel(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(x=>{ if(/Sim/i.test(x.textContent)) x.click(); });
  await sleep(1200);
  document.getElementById('run-sim').click();
  for (let k=0;k<40 && !document.getElementById('rp-scrub'); k++) await sleep(1000);
  if (!document.getElementById('rp-scrub')) {
    return { fail: true, resultsHtml: (document.getElementById('sim-results')||{}).innerHTML?.slice(0,300) };
  }
  const rows=[...document.querySelectorAll('.rp-row')].map(e=>({
    name:e.querySelector('.rp-name').textContent,
    stat:e.querySelector('.rp-stat').textContent,
    now:e.querySelector('.rp-now').textContent,
    open:!e.querySelector('.rp-chart').hidden,
    pts:e.querySelector('.rp-line').getAttribute('points').split(' ').length }));
  // The panel as the FINISHED fight, which is where a replay opens.
  const read = () => ({
    kpi: Object.fromEntries([...document.querySelectorAll('[data-kpi]')].map(e=>[e.dataset.kpi, e.textContent])),
    meter: [...document.querySelectorAll('#sim-results .mrow[data-mk]:not(.sub)')].map(e=>e.querySelector('.mval').textContent.trim()),
    pools: [...document.querySelectorAll('#rp-pools .rp-cell b')].map(e=>e.textContent).join('|'),
    hero: document.querySelector('[data-hero]').textContent,
  });
  const atEnd = read();
  // ...and the replay BELOW everything it drives.
  const res = document.querySelector('#sim-results .results');
  const kids = [...res.children].map(e=>e.tagName+'.'+(e.className||''));
  const iBar = [...res.children].findIndex(e=>e.classList.contains('rp-bar'));
  const iMeter = [...res.children].findIndex(e=>e.classList.contains('meter'));
  const iTable = [...res.children].findIndex(e=>e.classList.contains('stat-table'));
  const iRow = [...res.children].findIndex(e=>e.classList.contains('rp-row'));

  // Rewind to the very start: the panel must read as a fight that has not
  // happened yet.
  const sc=document.getElementById('rp-scrub');
  sc.value=0; sc.dispatchEvent(new Event('input')); await sleep(300);
  const atZero = read();
  // ...and back to the end restores it exactly.
  sc.value=sc.max; sc.dispatchEvent(new Event('input')); await sleep(300);
  const restored = read();
  const nowAtEnd=[...document.querySelectorAll('.rp-now')].map(e=>e.textContent);

  sc.value=0; sc.dispatchEvent(new Event('input')); await sleep(200);
  document.getElementById('rp-play').click(); await sleep(1500);
  const movedTo = Number(document.getElementById('rp-scrub').value);
  document.getElementById('rp-play').click();
  return { rows, atEnd, atZero, restored, nowAtEnd, movedTo, iBar, iMeter, iTable, iRow, kids,
           clock: document.getElementById('rp-clock').textContent };
})()`);
if (r.fail) { console.log("FAIL  no replay section — sim-results:", r.resultsHtml); process.exit(1); }
check("one row per buff, drawn and open by default",
  r.rows.length === 1 && r.rows[0].open && r.rows[0].pts === 600, JSON.stringify(r.rows[0]));
// Language-agnostic: this check runs in whatever locale the browser picks, so
// it asserts the FIGURES (mean out of max, a percentage, a ramp time) rather
// than the words around them.
check("the header states average, uptime and the ramp",
  /[\d.]+\/40/.test(r.rows[0].stat) && /\d+%/.test(r.rows[0].stat) &&
  /[\d.]+s/.test(r.rows[0].stat), r.rows[0].stat);
check("the replay BAR sits above everything it drives",
  r.iBar < r.iMeter && r.iBar < r.iTable, JSON.stringify(r.kids));
check("...and the buff CURVES stay down with the other chart",
  r.iRow > r.iMeter && r.iRow < r.iTable, JSON.stringify(r.kids));
check("it opens on the finished fight", r.nowAtEnd[0] === "40/40", String(r.nowAtEnd));
check("rewinding empties the KPIs and the meter",
  r.atZero.kpi.shots === "0" && r.atZero.kpi.procs === "0" &&
  r.atZero.meter.every((v) => /^0 /.test(v)),
  JSON.stringify(r.atZero));
check("...and the pools go back to full",
  r.atZero.pools !== r.atEnd.pools && r.atZero.pools.startsWith("659,445"), r.atZero.pools);
check("the headline follows too", r.atZero.hero !== r.atEnd.hero,
  r.atEnd.hero + " -> " + r.atZero.hero);
check("the unit sits on the number's line",
  /KPM|DPS/.test(r.atEnd.hero), r.atEnd.hero);
check("returning to the end restores the panel exactly",
  JSON.stringify(r.restored) === JSON.stringify(r.atEnd),
  JSON.stringify(r.atEnd) + " vs " + JSON.stringify(r.restored));
check("play advances the clock at the chosen multiplier",
  r.movedTo > 40 && r.movedTo < 120, "frame " + r.movedTo + " after 1.5s at 5x (expect ~75)");
ws.close(); proc.kill(); srv.close();
console.log(fail ? fail + " failed" : "the whole panel replays");
process.exit(fail ? 1 : 0);
