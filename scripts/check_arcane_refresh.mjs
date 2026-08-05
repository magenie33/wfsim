// FOURTEENTH CHECK — changing an arcane reaches the panel and the buff bar.
//
// Reported 2026-08-05: "切换赋能不会刷新缓存，需要切换一下mod才能刷新可以正确
// 显示". The arcane picker redrew its own slots and stopped — `refreshPanel` is
// the funnel every build change is supposed to go through, and the arcane path
// skipped it, so the stats and the SIM'S BUFF BAR kept showing the previous
// arcane until an unrelated edit happened to refresh them. Toggling a mod was
// the usual accident.
//
// The fix put the refresh inside the mutation, so this asserts the OBSERVABLE:
// pick an arcane, touch nothing else, and its buff card is on screen with its
// name in the display language.
//
//   node scripts/check_arcane_refresh.mjs
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
const BASE=`http://127.0.0.1:${srv.address().port}`, PORT=9521;
const proc=spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",[`--remote-debugging-port=${PORT}`,"--headless=new","--disable-gpu","--no-first-run",`--user-data-dir=${process.env.TEMP}/wfsim-arcref`,"about:blank"],{stdio:"ignore"});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function cdp(path){for(let i=0;i<60;i++){try{const r=await fetch(`http://127.0.0.1:${PORT}${path}`);if(r.ok)return r.json();}catch{}await sleep(250);}throw new Error("no CDP");}
const t=(await cdp("/json/list")).find(x=>x.type==="page");
const ws=new WebSocket(t.webSocketDebuggerUrl);
await new Promise(r=>ws.onopen=r);
let id=0;const waits=new Map();
ws.onmessage=e=>{const m=JSON.parse(e.data);if(waits.has(m.id)){waits.get(m.id)(m);waits.delete(m.id);}};
const send=(method,params={})=>new Promise(r=>{const i=++id;waits.set(i,r);ws.send(JSON.stringify({id:i,method,params}));});
const evaluate=async expr=>{const r=await send("Runtime.evaluate",{expression:expr,awaitPromise:true,returnByValue:true});if(r.result?.exceptionDetails)throw new Error(String(r.result.exceptionDetails.exception?.description||"").slice(0,700));return r.result?.result?.value;};
let bad=0;
const check=(what,ok,detail="")=>{console.log(`${ok?"  ok":"FAIL"}  ${what}${ok||!detail?"":"  — "+detail}`);if(!ok)bad++;};
await send("Page.enable");
await send("Page.navigate",{url:BASE});await sleep(12000);

for (const lang of ["en", "zh"]) {
  // A FULL RELOAD per language. The display language is read from storage at
  // BOOT, and the buff bar keeps whatever the last pass left in it — running
  // both passes in one page made the second one compare a card against itself.
  await evaluate(`localStorage.clear(); localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)})`);
  await send("Page.navigate", { url: BASE }); await sleep(11000);
  const r = await evaluate(`(async () => {
    const sleep=ms=>new Promise(r=>setTimeout(r,ms));
    // A SHOTGUN, because Shotgun Vendetta is class-gated to one.
    history.pushState({},'','/weapons/Boar_Prime'); route(); await sleep(4500);
    const out = { lang: '${lang}' };

    // BY ID, not by rendered text: the card's .bn carries the grants too, so
    // matching on text compares a name against a sentence.
    const ids = () => (buffList || []).map(b => b.id);
    out.before = ids();

    // PICK THE ARCANE AND TOUCH NOTHING ELSE. No mod edit, no tab switch —
    // the whole bug was that something else had to happen.
    setArcane('shotgun_vendetta', 0);
    renderArcanes();
    await sleep(1800);
    out.after = ids();
    out.appeared = out.after.filter(n => !out.before.includes(n));
    // THE GENERAL MECHANISM, proved by BYPASSING the fix. Assign the state
    // directly — no setArcane, no render call, nothing that could have been
    // taught to refresh — and then do the one thing a user always does: click.
    // The panel must catch up on its own, because the trigger is derived from
    // the build rather than fired by whoever changed it.
    arcanes[0] = 'none';
    document.body.click();
    await sleep(1600);
    out.afterRawEdit = ids();
    out.watchdogCaughtIt = !out.afterRawEdit.includes('arcane:shotgun_vendetta');
    // Put it back for the language assertion below.
    setArcane('shotgun_vendetta', 0); renderArcanes(); await sleep(1500);

    // ...and what that card actually READS on screen, for the language check.
    const el = document.querySelector('#sim-buffs [data-b="arcane:shotgun_vendetta"]');
    out.shown = el ? (el.closest('.buff-card') || el).textContent.replace(/\s+/g,' ').trim() : '';

    // ...and the card the panel built for it.
    const b = (buffList || []).find(x => x.id === 'arcane:shotgun_vendetta');
    out.buff = b ? { name: b.name, grants: b.grants, max: b.max_stacks, kind: b.kind } : null;
    return out;
  })()`);

  console.log(`\n[${lang}]`);
  check("the arcane's buff card appears with no other edit",
    r.appeared.length >= 1, `before ${JSON.stringify(r.before)} after ${JSON.stringify(r.after)}`);
  check("the panel built a card for it", !!r.buff, JSON.stringify(r.buff));
  if (r.buff) {
    check("...granting both halves", /Multishot/i.test(r.buff.grants) && /Reload/i.test(r.buff.grants),
      r.buff.grants);
    check("...as a single toggle", r.buff.max === 1, String(r.buff.max));
  }
  // The NAME is DE's, in the display language — 霰弹·仇杀 in Chinese.
  check("a RAW state edit is caught too — the trigger is derived, not fired",
    r.watchdogCaughtIt === true, JSON.stringify(r.afterRawEdit));
  check("its card is named in the display language",
    lang === "zh" ? /霰弹|仇杀/.test(r.shown) : /Vendetta/i.test(r.shown), r.shown);
}

ws.close(); proc.kill(); srv.close();
console.log(bad ? `\n${bad} failed` : "\nall good");
process.exit(bad ? 1 : 0);
