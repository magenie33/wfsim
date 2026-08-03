// BUFF CARDS: named in the display language, opened at the right stack count,
// and honest about coverage.
//
// Three things this guards, each of which has been wrong:
//   · a buff granted by an EVOLUTION was the only card left in English,
//     because the name lookup knew about mods and arcanes and nothing else;
//   · the earned-from-zero default has to REACH the card, not just the server;
//   · uptime was rounding 99.83% up to a flat "100%", which is the one number
//     a reader will not believe (user, 2026-08-03).
//
//   node scripts/check_buff_cards.mjs
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
const BASE=`http://127.0.0.1:${srv.address().port}`, PORT=9497;
const proc=spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",[`--remote-debugging-port=${PORT}`,"--headless=new","--disable-gpu","--no-first-run",`--user-data-dir=${process.env.TEMP}/wfsim-zh-check`,"about:blank"],{stdio:"ignore"});
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
// Land in Chinese before the app boots.
await send("Page.addScriptToEvaluateOnNewDocument", { source: `localStorage.setItem("wfsim-lang","zh")` });
await send("Page.navigate",{url:BASE});await sleep(13000);
let fail=0;const check=(n,ok,d)=>{console.log(`${ok?"  ok  ":"FAIL  "}${n}${ok||d===undefined?"":`  — ${d}`}`);if(!ok)fail++;};

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  history.pushState({},'','/weapons/Laetum'); route(); await sleep(3000);
  evoSel = {1:'laetum_evo1_incarnon_form',2:'laetum_rapid_wrath',3:'laetum_lethal_rearmament',
            4:'laetum_caput_mortuum',5:'laetum_overwhelming_attrition'};
  markPresetDirty(); renderEvo(); refreshPanel(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(x=>{ if(/Sim|模拟/i.test(x.textContent)) x.click(); });
  await sleep(1200);
  const cards = [...document.querySelectorAll('#sim-buffs .buff-card')].map(e=>({
    name: e.querySelector('.bn').textContent.trim(),
    stacks: e.querySelector('input[data-f="stacks"]').value,
    inactive: !e.querySelector('.binact').classList.contains('off'),
    inactiveText: e.querySelector('.binact').textContent,
  }));
  sim.level = 300; sim.steel_path = true; sim.duration = 60; sim.runs = 6;
  markScenarioDirty(); await sleep(600);
  document.getElementById('run-sim').click();
  for (let k=0;k<40 && !document.querySelector('.rp-row'); k++) await sleep(1000);
  const rows = [...document.querySelectorAll('.rp-row')].map(e=>({
    name: e.querySelector('.rp-name').textContent.trim(),
    stat: e.querySelector('.rp-stat').textContent.replace(/\\s+/g,' ').trim(),
  }));
  return { cards, rows, lang: LANG };
})()`);

console.log("lang:", r.lang);
console.log("cards:", JSON.stringify(r.cards, null, 1));
console.log("rows :", JSON.stringify(r.rows, null, 1));
check("both evolution buffs have a card", r.cards.length === 2, JSON.stringify(r.cards.map(c=>c.name)));
check("their names are Chinese", r.cards.every(c => /[\u4e00-\u9fff]/.test(c.name)), r.cards.map(c=>c.name).join(","));
check("they open at 0 stacks", r.cards.every(c => c.stacks === "0"), r.cards.map(c=>c.stacks).join(","));
check("and say so", r.cards.every(c => c.inactive && /[\u4e00-\u9fff]/.test(c.inactiveText)), r.cards.map(c=>c.inactiveText).join(","));
check("the coverage rows are Chinese too", r.rows.every(x => /[\u4e00-\u9fff]/.test(x.name)), r.rows.map(x=>x.name).join(","));
check("uptime is never rounded up to 100%", r.rows.every(x => !/100%/.test(x.stat)), r.rows.map(x=>x.stat).join(" | "));
check("the ramp is stated", r.rows.every(x => /满层于|full at|未满层|never full/.test(x.stat)), r.rows.map(x=>x.stat).join(" | "));
ws.close(); proc.kill(); srv.close();
console.log(fail ? fail + " failed" : "the buff cards read right in Chinese");
process.exit(fail ? 1 : 0);
