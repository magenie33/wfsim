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
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send } = app;

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
  // THE OFFICIAL BAR IS ONE DROPDOWN now — a benchmark is picked from a list
  // rather than scanned along a row, because there are rulers now and will be
  // dozens. Read-only is still a property of the DATA (the bar's store filters
  // builtins), which the later steps here assert.
  bar.querySelector('[data-dd]').click(); await sleep(800);
  const chip = [...document.querySelectorAll('#dd-menu .opt[data-v]')]
    .find((c) => c.dataset.v === out.name);
  out.chipFound = !!chip;
  // READ-ONLY IS NOT A CLASS ANY MORE. The bar offers a COPY and nothing else
  // — no new, no rename, no delete — and the collection's store refuses to
  // write a builtin at all. Both are asserted here and below; a marking on a
  // dropdown row would only have been decoration over them.
  out.chipMarked = !!bar.querySelector('.pop.dup')
    && !bar.querySelector('.pop.ren') && !bar.querySelector('.pop.del');
  chip.click(); await sleep(700);
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
  // Copy is offered BESIDE the dropdown, and nothing else is: no rename, no
  // delete, because none of it is yours.
  out.hasCopy = !!bar.querySelector('.pop.dup');
  out.hasRename = !!bar.querySelector('.pop.ren');
  out.hasDelete = !!bar.querySelector('.pop.del');
  bar.querySelector('.pop.dup').click();
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
  check("it offers a copy and nothing that would edit it", r.chipFound && r.chipMarked);
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
  // AN EMPTY BOARD IS A REAL STATE — a weapon nobody has submitted for shows no
  // chips, which is right rather than a bug to work around.
  //
  // CONSTRUCTED, not borrowed. This used to read the live board and rely on
  // Torid having no rows; players submitted some (2026-08-05) and the check
  // started failing on the board WORKING. What it means to assert is "empty
  // board -> no chips", so it empties the board and asks.
  BOARD = {};
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
  out.notInOwnBar = ![...own.querySelectorAll('.pchip')].some((c) => /^#1/.test(c.dataset.name || ''));
  bar.querySelector('[data-dd]').click(); await sleep(800);
  // The FIRST row of the list — its name carries the mode now, so it is picked
  // by position rather than by a rank that no longer names it alone.
  const chip = document.querySelector('#dd-menu .opt[data-v]');
  out.chipFound = !!chip;
  chip.click(); await sleep(700);
  // AFTER picking: the copy is offered for the one you are ON, which is the
  // only build a copy could mean.
  out.chipMarked = !!bar.querySelector('.pop.dup')
    && !bar.querySelector('.pop.ren') && !bar.querySelector('.pop.del');
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

  // ...and the copy button beside the dropdown gives an ordinary editable build.
  const sel = bar;
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
check("it offers a copy and nothing that would edit it", b.chipFound && b.chipMarked);
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
  bar.querySelector('[data-dd]').click(); await sleep(800);
  const off = document.querySelector('#dd-menu .opt[data-v]');
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
  // ...and the rest of what THIS benchmark asks for. "Full" is per weapon, so
  // the probe reads the requirement rather than assuming eight mods is it:
  // Torid has four evolution tiers and one arcane seat.
  const w = weaponInfo($('weapon').value) || {};
  for (let t = 1; t <= (w.evo_tiers || 0); t++) {
    const opts = (weaponAxes(w.id).evolutions[t - 1] || {}).options || [];
    if (opts.length) evoSel[t] = opts[0].id;
  }
  const seats = (w.arcane_pools || []).length;
  if (seats) {
    const ax = weaponAxes(w.id).arcanes;
    arcanes = arcanes.slice();
    for (let i = 0; i < seats; i++) {
      const o = ((ax[i] || {}).options || []).filter((x) => x.id !== 'none');
      if (o.length) arcanes[i] = o[0].id;
    }
  }
  markPresetDirty(); renderMods(); renderEvo(); renderArcanes(); refreshPanel(); await sleep(1500);
  out.modCountAfter = slots.filter((s) => s.mod).length;
  out.shortfalls = buildShortfalls();

  // AN EXILUS MOD ON TOP. The build is complete at 8 main slots; filling the
  // exilus slot must not make it "9 mods" and must not be sent — the most
  // thoroughly built players were the ones this refused (2026-08-05).
  const exi = pool.find((id) => modById(id) && modById(id).exilus && !slots.some((s) => s.mod === id));
  if (exi) { slots[8].mod = exi; slots[8].rank = modById(exi).max_rank; markPresetDirty(); renderMods(); await sleep(900); }
  out.exilusEquipped = !!exi && !!slots[8].mod;
  out.stillComplete = buildIsComplete();

  // A complete build under the default DOES go — that is the change.
  await offerBoardSubmit(); await sleep(800);
  out.postsOnDefault = posts.length;
  const first = posts.length ? JSON.parse(posts[posts.length - 1].body || '{}') : null;
  out.sentModCount = first ? first.mods.length : null;
  out.sentHasExilus = first ? first.mods.includes(exi) : null;

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
check("...and the line says why", /as far as it goes|最努力/.test(c.incompleteText || ""), c.incompleteText);
check("a COMPLETE build goes under the default", c.postsOnDefault === 1,
  `${c.modCountAfter}/${c.floor} mods, missing ${JSON.stringify(c.shortfalls)}, ${c.postsOnDefault} posts`);
check("an EXILUS mod does not make it incomplete", c.exilusEquipped === true && c.stillComplete === true,
  `equipped ${c.exilusEquipped}, complete ${c.stillComplete}`);
check("...and never travels", c.sentModCount === c.floor && c.sentHasExilus === false,
  `sent ${c.sentModCount} mods, exilus among them: ${c.sentHasExilus}`);
check("nothing leaves after opting out", c.postsAfterNo === 0, String(c.postsAfterNo));
check("...and the line says nothing is sent", /not|nothing|不会/.test(c.declinedText), JSON.stringify(c.declinedText));
check("turning it back on is not itself a submission", c.postsOnFlip === 0, String(c.postsOnFlip));
check("...and the next run sends exactly one", c.postsAfterYes === 1, String(c.postsAfterYes));
check("...carrying the BUILD and no score",
  JSON.stringify(c.sentKeys) === JSON.stringify(["arcanes","benchmark","evolutions","mods","weapon"]) && c.sentHasScore === false,
  JSON.stringify(c.sentKeys));
check("...against the official benchmark", c.sentBenchmark === "single_target", String(c.sentBenchmark));

await app.finish("the official benchmark is the fight, and it is locked");
