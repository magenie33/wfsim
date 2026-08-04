// THE OFFICIAL THINGS ARE READ-ONLY, AND NOTHING CAN WRITE TO THEM.
//
// Two of them, one contract: the official SCENARIO (data/benchmarks/) and the
// official BUILDS (data/benchmarks/boards/). Neither is a preset — no weapon
// owns them, nothing stores them, nobody edits them — and both sit in the bar
// that already holds their kind, marked, selectable, copyable.
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
  // THE BENCHMARK BAR, not the player's. Official entries were split out of
  // the preset bar into a bar of their own (owner, 2026-08-04) — so this also
  // asserts the split: finding the official chip in the preset bar would mean
  // it leaked back into the collection that is supposed to be yours.
  const bar = $('bench-bar-simulator-scenarios');
  const own = $('preset-bar-simulator-scenarios');
  out.barVisible = !bar.hidden;
  out.notInOwnBar = ![...own.querySelectorAll('.pchip')].some((c) => c.dataset.name === out.name);
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
  // ...and it lands in YOUR bar, which is the point of copying it.
  out.copyInOwnBar = [...$('preset-bar-simulator-scenarios').querySelectorAll('.pchip')]
    .some((c) => c.dataset.name === activeScenario);
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
  check("...in the BENCHMARK bar, not yours", r.barVisible && r.notInOwnBar);
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

// ---- THE OFFICIAL BUILDS ------------------------------------------------
const BUILDS_PROBE = `(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/Torid'); route(); await sleep(4500);
  const out = {};
  // AN EMPTY BOARD IS THE SHIPPING STATE until submissions arrive, so there is
  // nothing to click — and no chips is exactly right, not a bug to work around.
  out.emptyBoardChips = builtinBuilds().length;

  // The machinery is exercised with a row this check INJECTS. That is the
  // point: the read-only-build path has to be tested against code, not against
  // whatever the board happens to hold on the day — a check that only works
  // while data exists stops testing the moment the data is cleared, which is
  // exactly what happened when the seed was removed.
  const inject = { benchmark: 'single_target', source: 'submissions', score: 1.2345,
                   mods: ['serration','split_chamber','point_strike'],
                   evolutions: [], arcanes: ['none'] };
  BOARD = { torid: [inject] };     // the runtime board, as /board.json would give it
  renderPresetBar(); await sleep(300);

  const rows = builtinBuilds();
  out.count = rows.length;
  out.first = rows[0] ? { name: rows[0].name, id: rows[0].builtin, mods: (rows[0].board||{}).mods } : null;

  const bar = $('bench-bar-builder-builds');
  const own = $('preset-bar-builder-builds');
  out.barVisible = !bar.hidden;
  out.notInOwnBar = ![...own.querySelectorAll('.pchip')].some((c) => c.dataset.name === '#1');
  const chip = [...bar.querySelectorAll('.pchip')].find((c) => c.dataset.name === '#1');
  out.chipFound = !!chip;
  out.chipMarked = !!(chip && chip.classList.contains('ro'));
  chip.click(); await sleep(700);
  out.isOfficial = officialBuildActive();
  // ...and the BUILD on screen is the board's.
  out.slots = slots.filter((s) => s.mod).map((s) => s.mod).sort();

  // The note says what it scored and what it costs to own.
  const note = $('build-official');
  out.noteShown = !!(note && !note.hidden);
  out.noteText = (note && note.textContent || '').trim();

  // The editor is inert — pointer-events, since a slot is a div.
  out.locked = ['mod-block','arcane-block','evo-block']
    .every((id) => $(id) && $(id).classList.contains('locked-hard'));

  // NOTHING WRITES TO IT.
  const before = JSON.stringify(loadPresetList('builder-builds'));
  slots[0].mod = null; markPresetDirty();
  await sleep(900);
  out.storeUntouched = JSON.stringify(loadPresetList('builder-builds')) === before;

  // ...and ⧉ gives an ordinary editable build.
  const sel = bar.querySelector('.pchip.sel');
  out.hasCopy = !!sel.querySelector('.pop.dup');
  out.hasRename = !!sel.querySelector('.pop.ren');
  sel.querySelector('.pop.dup').click();
  await sleep(800);
  out.copyIsOwn = !officialBuildActive();
  out.copyStored = loadPresetList('builder-builds').some((p) => p.name === activePreset);
  out.copyEditable = ['mod-block','arcane-block','evo-block']
    .every((id) => $(id) && !$(id).classList.contains('locked-hard'));
  return out;
})()`;

const b = await evaluate(BUILDS_PROBE);
console.log("");
console.log("[board]");
check("an empty board shows no chips at all", b.emptyBoardChips === 0, String(b.emptyBoardChips));
check("a board row becomes a chip", b.count === 1, JSON.stringify(b.first));
check("its chip is marked read-only", b.chipFound && b.chipMarked);
check("...in the BENCHMARK bar, not yours", b.barVisible && b.notInOwnBar);
check("opening it puts the board's build on screen",
  b.isOfficial === true
    && JSON.stringify(b.slots) === JSON.stringify(((b.first || {}).mods || []).slice().sort()),
  JSON.stringify(b.slots));
check("a note says what it is and what it scored",
  b.noteShown && /1\.2345/.test(b.noteText), JSON.stringify(b.noteText.slice(0, 90)));
check("the editor is inert", b.locked === true);
check("EDITING THE BUILD WRITES NOTHING", b.storeUntouched === true);
check("it offers copy and not rename", b.hasCopy === true && b.hasRename === false);
check("...and the copy is an ordinary editable build",
  b.copyIsOwn === true && b.copyStored === true && b.copyEditable === true,
  `own ${b.copyIsOwn}, stored ${b.copyStored}, editable ${b.copyEditable}`);

// ---- CONSENT: nothing leaves before it is given -------------------------
const CONSENT_PROBE = `(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/Torid'); route(); await sleep(4500);
  const out = {};
  // Watch the WIRE, not the function: what matters is whether a request left.
  const real = window.fetch;
  let posts = [];
  window.fetch = (u, o) => { if (String(u).includes('/api/board/')) posts.push({ u: String(u), body: o && o.body }); return real(u, o); };

  // Under an ordinary scenario the question is not even asked.
  out.askedOffOfficial = !$('board-consent').hidden;

  // Open the official scenario.
  const bar = $('bench-bar-simulator-scenarios');
  // By its READ-ONLY mark, not by name: the name is translated and this probe
  // runs after the language ones, so matching on it couples two checks.
  const off = bar.querySelector('.pchip.ro');
  out.chipSeen = !!off;
  if (off) off.click();
  await sleep(1500);
  renderBoardConsent();
  out.askedOnOfficial = !$('board-consent').hidden;
  out.asksFirst = /board|榜单/.test($('board-consent').textContent || '');
  out.saysWhatIsSent = /mod/i.test($('board-consent').textContent || '');
  // THE NOTICE IS UP BEFORE ANY RUN. Submission defaults to ON, so what has to
  // be true is not that nothing leaves — it is that nothing leaves UNSAID.
  out.statesDefaultOn = /added to the official board|加入官方榜单/.test(
    $('board-consent').textContent || '');
  out.hasOptOut = !!$('board-no') || !!$('board-flip');

  // AN INCOMPLETE BUILD IS NOT SENT. This is the first visit's actual state —
  // the default build is empty — so it is also the state the default-on setting
  // would otherwise fire a pointless request from.
  out.modCount = slots.filter((s) => s.mod).length;
  await offerBoardSubmit();
  await sleep(600);
  out.postsWhileIncomplete = posts.length;
  out.incompleteText = ($('board-consent').textContent || '').trim().slice(0, 400);

  // Fill the build to the floor, from this weapon's own pool.
  const pool = (weaponInfo($('weapon').value) || {}).mods || [];
  const need = (META.board_build_mods || 8);
  out.floor = need;
  let k = 0;
  for (const id of pool) {
    if (k >= need) break;
    if (!modById(id)) continue;
    slots[k].mod = id; slots[k].rank = modById(id).max_rank; k++;
  }
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(1200);
  out.modCountAfter = slots.filter((s) => s.mod).length;

  // A complete build under the default DOES go — that is the change.
  await offerBoardSubmit(); await sleep(800);
  out.postsOnDefault = posts.length;

  out.boxHtml = ($('board-consent').innerHTML || '').slice(0, 200);
  out.hasNo = !!$('board-no') || !!$('board-flip');
  // Opt OUT: nothing further leaves, and the line says so.
  const optOut = $('board-no') || $('board-flip');
  if (!optOut) { window.fetch = real; return out; }
  optOut.click(); await sleep(300);
  const before = posts.length;
  await offerBoardSubmit(); await sleep(600);
  out.postsAfterNo = posts.length - before;
  out.declinedText = ($('board-consent').textContent || '').trim().slice(0, 60);

  // Back on. Flipping the setting is NOT itself a submission — turning a
  // preference on should not fire a request — so the next RUN is what sends.
  const back = posts.length;
  $('board-flip').click(); await sleep(400);
  out.postsOnFlip = posts.length - back;
  await offerBoardSubmit(); await sleep(700);
  out.postsAfterYes = posts.length - back;
  const sent = posts.length ? JSON.parse(posts[posts.length - 1].body || '{}') : null;
  out.sentKeys = sent ? Object.keys(sent).sort() : null;
  out.sentHasScore = sent ? ('score' in sent || 'dps' in sent) : null;
  out.sentBenchmark = sent && sent.benchmark;

  window.fetch = real;
  return out;
})()`;

const c = await evaluate(CONSENT_PROBE);
console.log("");
console.log("[consent]");
if (!c.hasNo) console.log("      [diag] " + JSON.stringify({ chipSeen: c.chipSeen, asked: c.askedOnOfficial, html: c.boxHtml }));
check("the notice is absent under an ordinary scenario", c.askedOffOfficial === false);
check("...and present under the official one", c.askedOnOfficial === true);
check("it says what would be sent", c.saysWhatIsSent === true);
// THE CONTRACT CHANGED (2026-08-05): submission is default-ON, so the property
// worth asserting is no longer "nothing leaves" — it is that nothing leaves
// UNSAID. The notice states the default and carries a working opt-out, both
// visible before any run.
check("...states that runs are submitted, BEFORE any run", c.statesDefaultOn === true,
  c.incompleteText);
check("...and offers a way out in the same view", c.hasOptOut === true);
check("an INCOMPLETE build is not sent", c.postsWhileIncomplete === 0,
  `${c.modCount} mods, ${c.postsWhileIncomplete} posts`);
check("...and the line says why", /complete|装满/.test(c.incompleteText || ""), c.incompleteText);
check("a COMPLETE build goes under the default", c.postsOnDefault === 1,
  `${c.modCountAfter}/${c.floor} mods, ${c.postsOnDefault} posts`);
check("nothing leaves after opting out", c.postsAfterNo === 0, String(c.postsAfterNo));
check("...and the line says nothing is sent", /not|nothing|不会/.test(c.declinedText), JSON.stringify(c.declinedText));
check("turning it back on is not itself a submission", c.postsOnFlip === 0, String(c.postsOnFlip));
check("...and the next run sends exactly one", c.postsAfterYes === 1, String(c.postsAfterYes));
check("...carrying the BUILD and no score",
  JSON.stringify(c.sentKeys) === JSON.stringify(["arcanes","benchmark","evolutions","mods","weapon"]) && c.sentHasScore === false,
  JSON.stringify(c.sentKeys));
check("...against the official benchmark", c.sentBenchmark === "single_target", String(c.sentBenchmark));

ws.close(); srv.close(); proc.kill();
process.exit(fail ? 1 : 0);
