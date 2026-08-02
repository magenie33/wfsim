// THE GAIN SCAN OBEYS THE TIER LADDER.
//
// Tier N of an evolution set is choosable only once N-1 is filled — a tier-2
// perk with no tier 1 is not a weaker build, it is not a build. The builder
// greys those rows out; the quick-calc gain scan used to measure them anyway,
// so the picker ranked evolutions nobody could click, on builds that cannot
// exist (user, 2026-08-03).
//
//   node scripts/check_gain_axes.mjs
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
const proc=spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",[`--remote-debugging-port=${PORT}`,"--headless=new","--disable-gpu","--no-first-run",`--user-data-dir=${process.env.TEMP}/wfsim-gain-check`,"about:blank"],{stdio:"ignore"});
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
  history.pushState({},'','/weapons/Torid'); route(); await sleep(3000);
  const ids = () => gainCandidates({kind:'evo',idx:0}).map(c=>c.id);
  const none = ids();
  evoSel = {1:'torid_evo1_incarnon_form'}; const one = ids();
  evoSel = {1:'torid_evo1_incarnon_form',2:'torid_final_fusillade'}; const two = ids();
  evoSel = {1:'torid_evo1_incarnon_form',2:'torid_final_fusillade',3:'torid_extended_volley'}; const three = ids();
  const swap = gainCandidates({kind:'evo',idx:0}).find(c=>c.id==='torid_plentiful_mayhem');
  return { none, one, two, three, swap: swap && swap.payload.evolutions };
})()`);

const t3 = ['torid_extended_volley','torid_renewed_horror','torid_swift_deliverance'];
const t4 = ['torid_commodores_fortune','torid_elemental_balance','torid_survivors_edge'];
check("with nothing chosen, only tier 1 is offered",
  r.none.length === 1 && r.none[0] === 'torid_evo1_incarnon_form', r.none.join(","));
check("tier 1 chosen opens tier 2, and no further",
  r.one.length === 2 && !r.one.some((x) => t3.includes(x) || t4.includes(x)), r.one.join(","));
check("tier 2 chosen opens tier 3, its own tier still swappable",
  r.two.includes('torid_plentiful_mayhem') && t3.every((x) => r.two.includes(x)) &&
  !r.two.some((x) => t4.includes(x)), r.two.join(","));
check("tier 3 chosen opens tier 4", t4.every((x) => r.three.includes(x)), r.three.join(","));
check("a swap replaces ONE tier and leaves the rest alone",
  JSON.stringify(r.swap) === JSON.stringify(['torid_evo1_incarnon_form','torid_plentiful_mayhem','torid_extended_volley']),
  JSON.stringify(r.swap));

ws.close(); proc.kill(); srv.close();
console.log(fail ? fail + " failed" : "the gain scan obeys the ladder");
process.exit(fail ? 1 : 0);
