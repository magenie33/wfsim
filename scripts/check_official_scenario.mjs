// THE OFFICIAL SCENARIO IS ON EVERY WEAPON, AND NOTHING CAN WRITE TO IT.
//
// `data/benchmarks/*.yaml` is a ruler rather than a preset: no weapon owns it,
// nothing stores it, nobody edits it. Three claims that have to hold ON SCREEN,
// because each fails in its own way:
//
//   - it APPEARS, on every weapon in the roster (a per-weapon list would make
//     it a preset again, and presets never cross weapons);
//   - it is READ-ONLY where a write would actually happen — auto-save, not the
//     disabled attribute. A control that looks inert while auto-save still
//     reads it is the exact bug this guards;
//   - it can be COPIED into an ordinary scenario, which is the whole answer to
//     "but I want to change it".
//
// Run twice, in both languages: the name is translated for display but its
// IDENTITY is the benchmark id, so switching language must not orphan the
// pointer that says which scenario is open.
//
//   node scripts/check_official_scenario.mjs
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
const BASE=`http://127.0.0.1:${srv.address().port}`, PORT=9501;
const proc=spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",[`--remote-debugging-port=${PORT}`,"--headless=new","--disable-gpu","--no-first-run",`--user-data-dir=${process.env.TEMP}/wfsim-official`,"about:blank"],{stdio:"ignore"});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function cdp(path){for(let i=0;i<60;i++){try{const r=await fetch(`http://127.0.0.1:${PORT}${path}`);if(r.ok)return r.json();}catch{}await sleep(250);}throw new Error("no CDP");}
const page=(await cdp("/json/list")).find(t=>t.type==="page");
const ws=new WebSocket(page.webSocketDebuggerUrl);
let id=0;const pending=new Map();
ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.id&&pending.has(m.id)){pending.get(m.id)(m);pending.delete(m.id);}};
await new Promise(r=>ws.onopen=r);
const send=(method,params={})=>new Promise(res=>{const i=++id;pending.set(i,res);ws.send(JSON.stringify({id:i,method,params}));});
const evaluate=async expr=>{const r=await send("Runtime.evaluate",{expression:expr,awaitPromise:true,returnByValue:true});if(r.result?.exceptionDetails)throw new Error(String(r.result.exceptionDetails.exception?.description||"").slice(0,700));return r.result?.result?.value;};
await send("Page.enable");await send("Runtime.enable");
await send("Page.navigate",{url:BASE});await sleep(12000);
let fail=0;const check=(n,ok,d)=>{console.log(`${ok?"  ok  ":"FAIL  "}${n}${ok||d===undefined?"":`  — ${d}`}`);if(!ok)fail++;};

const PROBE = (lang) => `(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear(); localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)});
  // LANG is read once at module load, so setting the key alone changes nothing
  // in an already-booted page — switch the live pair the way the picker does.
  LANG = ${JSON.stringify(lang)};
  I18N = LANG === 'en' ? null : ((await api('/api/i18n'))[LANG] || null);
  history.pushState({},'','/weapons/Boar_Prime'); route(); await sleep(4500);
  const out = { lang: ${JSON.stringify(lang)} };

  // 1. IT EXISTS, and its identity is the benchmark id rather than its name.
  const official = builtinScenarios();
  out.count = official.length;
  out.name = (official[0] || {}).name;
  out.id = (official[0] || {}).builtin;

  // 2. ON EVERY WEAPON. A preset collection is per weapon; a ruler is not.
  out.everyWeapon = [];
  for (const w of META.weapons) {
    switchWeapon(w.id); await sleep(150);
    out.everyWeapon.push({ id: w.id, has: scenarioList().some((p) => p.builtin === out.id) });
  }
  switchWeapon('boar_prime'); await sleep(250);

  // 3. OPEN IT — by id, which is what a stored pointer holds.
  const bar = $('preset-bar-simulator-scenarios');
  const chip = [...bar.querySelectorAll('.pchip')].find((c) => c.dataset.name === out.name);
  out.chipFound = !!chip;
  out.chipMarked = !!(chip && chip.classList.contains('ro'));
  chip.click(); await sleep(600);
  out.active = activeScenario;
  out.isOfficial = officialScenarioActive();

  // ...the fight on screen IS the benchmark's.
  out.level = sim.level; out.duration = sim.duration; out.metric = sim.metric; out.enemy = sim.enemy;

  // 4. THE NOTE, in the display language.
  const note = $('sim-official');
  out.noteShown = !!(note && !note.hidden);
  out.noteText = (note && note.textContent || '').trim().slice(0, 120);

  // 5. CONTROLS INERT.
  const inputs = ['sim-target','sim-technique','sim-limits','sim-run']
    .flatMap((b) => [...($(b) ? $(b).querySelectorAll('input,select') : [])]);
  out.inputs = inputs.length;
  out.allDisabled = inputs.length > 0 && inputs.every((el) => el.disabled);
  // How many this LOCKED, as against how many were already unavailable for
  // their own reason (a weapon with no ammo reserve has its infinite-ammo box
  // ticked and disabled whatever scenario is open — asserting on that one
  // would test the mechanic, not the lock).
  out.lockedByUs = document.querySelectorAll('[data-official-lock]').length;

  // 6. NOTHING WRITES TO IT — the assertion that matters, because auto-save is
  //    what would make a disabled control a lie. Edit the live fight and wait
  //    out the debounce; the stored list must not have grown an entry, and the
  //    official scenario's own state must be untouched.
  const before = JSON.stringify(loadPresetList('simulator-scenarios'));
  const wasLevel = sim.level;
  sim.level = 55; markScenarioDirty();
  await sleep(900);
  out.storeUntouched = JSON.stringify(loadPresetList('simulator-scenarios')) === before;
  out.officialStateIntact = scenarioNamed(out.id).state.level === wasLevel;

  // 7. AND IT CAN BE COPIED into an ordinary, editable scenario.
  const chip2 = bar.querySelector('.pchip.sel');
  out.hasCopy = !!chip2.querySelector('.pop.dup');
  out.hasRename = !!chip2.querySelector('.pop.ren');
  out.hasDelete = !!chip2.querySelector('.pop.del');
  chip2.querySelector('.pop.dup').click();
  await sleep(700);
  out.copyIsOwn = !officialScenarioActive();
  out.copyStored = loadPresetList('simulator-scenarios').some((p) => p.name === activeScenario);
  // Everything this locked is released, and nothing it did not touch moved.
  out.stillLocked = document.querySelectorAll('[data-official-lock]').length;
  const lvl = document.querySelector('#sim-target input[data-k="level"], #sim-target input');
  out.copyEditable = !!lvl && !lvl.disabled;

  return out;
})()`;

for (const lang of ["en", "zh"]) {
  const r = await evaluate(PROBE(lang));
  console.log(`\n[${lang}] ${r.name}`);
  check("the official scenario is served", r.count > 0 && !!r.id, JSON.stringify(r.id));
  const missing = (r.everyWeapon || []).filter((w) => !w.has).map((w) => w.id);
  check(`it is on all ${r.everyWeapon.length} weapons`, missing.length === 0, missing.join(","));
  check("its chip is marked read-only", r.chipFound && r.chipMarked);
  check("opening it makes it the active fight", r.isOfficial === true, r.active);
  check("...and that fight is the benchmark's", r.level === 9999 && r.duration === 300 && r.metric === "kpm" && r.enemy === "thrax_centurion",
    `lv ${r.level}, ${r.duration}s, ${r.metric}, ${r.enemy}`);
  check("a note says what it is", r.noteShown === true, JSON.stringify(r.noteText.slice(0, 60)));
  if (lang === "zh") check("...in Chinese", /官方/.test(r.noteText), JSON.stringify(r.noteText.slice(0, 40)));
  check(`its ${r.inputs} controls are inert`, r.allDisabled === true);
  check(`...${r.lockedByUs} of them locked BY the official scenario`, r.lockedByUs > 0, String(r.lockedByUs));
  check("EDITING THE FIGHT WRITES NOTHING", r.storeUntouched === true && r.officialStateIntact === true,
    `store untouched ${r.storeUntouched}, state intact ${r.officialStateIntact}`);
  check("it offers copy, and neither rename nor delete",
    r.hasCopy === true && r.hasRename === false && r.hasDelete === false);
  check("...and the copy is an ordinary editable scenario",
    r.copyIsOwn === true && r.copyStored === true && r.copyEditable === true && r.stillLocked === 0,
    `own ${r.copyIsOwn}, stored ${r.copyStored}, editable ${r.copyEditable}, still locked ${r.stillLocked}`);
}

ws.close(); srv.close(); proc.kill();
process.exit(fail ? 1 : 0);
