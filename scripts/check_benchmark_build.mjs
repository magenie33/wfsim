// THIRTEENTH CHECK — a benchmark build is a build, on arrival and on every view.
//
// Two things it asserts, both of which were broken on 2026-08-04:
//
//  1. THE FORMA PLAN SURVIVES A COLD LOAD. A board row carries mods and no
//     polarities, so it has to be planned into a legal layout. That plan lived
//     in the build bar's apply(), and `initPresets` restores a build WITHOUT
//     going through the bar — so landing on a page whose active build was a
//     benchmark build showed full drain (91/60, red) until you clicked
//     something. The check reloads with the benchmark build already active,
//     which is the exact path that was skipped.
//  2. THE FINDER STAYS IN THE BUILDER. The build finder is the builder's alone
//     and the note is the build collection's, so the simulator shows only the
//     bar and the optimizer — which owns no build — neither. Hiding is by CSS id
//     list, which is the kind of thing a new element silently falls out of.
//
//   node scripts/check_benchmark_build.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";
const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send, BASE } = app;
// THE REAL BOARD, not an injected one: the point of the cold path is that the
// page RELOADS, and an in-memory injection does not survive that — a weapon's
// rows are fetched from /board/<id>.json on boot. The INDEX is what names the
// weapons that have any, so the check finds one that actually has a row and
// skips cleanly if the board is empty (which it is before the first
// submission, and that is an ordinary state).
const WEAPON = await evaluate(`(async () => {
  const r = await fetch('/board/index.json', {cache:'no-cache'});
  const b = r.ok ? await r.json() : {};
  const id = Object.keys(b).find(k => (b[k] || []).length) || null;
  if (!id) return null;
  const w = (META.weapons || []).find(x => x.id === id);
  return w ? { id, path: (w.name_en || w.name).replace(/ /g, '_') } : null;
})()`);
if (!WEAPON) { await app.finish("board is empty — nothing to check"); }
console.log(`[${WEAPON.id}]`);
const SETUP = `(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/${WEAPON.path}'); route(); await sleep(4500);
  // OPENED FROM THE BUILD FINDER, whose first row is the leader of the scope it
  // starts on.
  document.querySelector('#build-finder [data-fopen]').click(); await sleep(1800);
  // What it looks like when SELECTED — the path that already worked.
  return { cap: (document.getElementById('capacity')||{}).textContent,
           over: !!document.querySelector('#capacity.over, #capacity.bad'),
           pols: slots.map(s => s.pol).filter(Boolean).length,
           active: activePreset };
})()`;
const warm = await evaluate(SETUP);
check("selecting a benchmark build plans its Forma", warm.pols > 0, `polarities ${warm.pols}, ${warm.cap}`);
// THE COLD PATH: reload with that build already active. `initPresets` restores
// it without the bar ever being clicked — which is the case that was broken.
await send("Page.navigate",{url:BASE+"/weapons/"+WEAPON.path});await sleep(12000);
const cold = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  await sleep(3000);
  const out = { active: activePreset, official: officialBuildActive() };
  out.pols = slots.map(s => s.pol).filter(Boolean).length;
  out.cap = (document.getElementById('capacity')||{}).textContent;
  const capEl = document.getElementById('capacity');
  out.overCls = capEl ? capEl.className : null;
  // The two numbers the header states, parsed: used must fit the capacity.
  const m = /(\\d+)\\s*\\/\\s*(\\d+)/.exec(out.cap || '');
  out.used = m ? Number(m[1]) : null;
  out.total = m ? Number(m[2]) : null;
  // EVERY PART OF THE BUILD IS READ-ONLY, mode included. It is the one that was
  // not: switching a #1 row to its base form ran the base form, saved nothing
  // and submitted nothing, with no line anywhere saying why.
  out.locked = ['mod-block','arcane-block','evo-block','mode-block']
    .map(id => ((document.getElementById(id)||{}).className||'').includes('locked-hard'));
  // …and the page SAYS the runs are not submitted: the consent panel hides
  // itself on a board row, so it cannot be the only thing that mentions it.
  out.consentHidden = (document.getElementById('board-consent')||{}).hidden;
  // The submit path's own verdict, asked the way it asks itself.
  out.wouldSubmit = officialScenarioActive() && !officialBuildActive();
  // A WAY OUT YOU CAN CLICK: the locked block itself takes the copy, and says
  // why on hover. A slot that simply does not react teaches nothing.
  out.lockedTitle = ((document.getElementById('mod-block')||{}).title || '').length;
  const blk = document.getElementById('mod-block');
  if (blk) {
    blk.click();
    await sleep(1600);
    out.afterCopy = { official: officialBuildActive(),
      locked: ['mod-block','arcane-block','evo-block','mode-block']
        .some(id => ((document.getElementById(id)||{}).className||'').includes('locked-hard')) };
  }
  return out;
})()`);
console.log("");
check("a cold load restores the benchmark build", cold.official === true, String(cold.active));
check("...with its Forma planned, not left unpolarised", cold.pols > 0, `polarities ${cold.pols}`);
check("...so the build FITS", cold.used !== null && cold.total !== null && cold.used <= cold.total,
      `${cold.cap} (${cold.overCls})`);
check("the cold and warm plans agree", cold.pols === warm.pols, `cold ${cold.pols} vs warm ${warm.pols}`);
// ---- READ-ONLY MEANS THE WHOLE BUILD ------------------------------------
// The mode is part of the build (it left the fight on 2026-08-07), so it locks
// with the build. While it did not, it was the one control on a board row that
// still moved — and both of its consequences were silent: `markPresetDirty`
// refuses to write an official build and `offerBoardSubmit` refuses to send
// one, so a base-form test ran, saved nothing and entered nothing.
check("every part of a benchmark build is read-only, mode included",
      cold.locked.every(Boolean), JSON.stringify(cold.locked));
// …AND IT SAYS WHY, on the block a player is trying to click: the consent
// panel hides itself on a board row, so nothing else on screen mentions it.
check("...while a locked block says why on hover", cold.lockedTitle > 10,
      `${cold.lockedTitle} chars of title`);
check("...and clicking it copies the build, which frees every block",
      !!cold.afterCopy && cold.afterCopy.official === false && cold.afterCopy.locked === false,
      JSON.stringify(cold.afterCopy));
check("...and a run of the board row itself would submit nothing", cold.wouldSubmit === false);
// ---- and it belongs to the BUILDER, not to every module ----
const views = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  const vis = id => { const e = document.getElementById(id); if (!e) return null;
    const s = getComputedStyle(e); return s.display !== 'none' && !e.hidden; };
  const out = {};
  // BY URL, which is how a module is actually entered — the tabs are links and
  // \`route()\` is what sets the body class the hiding rules key off.
  for (const [name, suffix] of [['builder',''],['simulator','/simulator'],['optimizer','/optimizer']]) {
    history.pushState({}, '', '/weapons/${WEAPON.path}' + suffix); route();
    await sleep(2000);
    out[name] = { body: document.body.className,
                  finder: vis('build-finder'), own: vis('preset-bar-builder-builds') };
  }
  return out;
})()`);
console.log("");
check("the build finder shows in the builder", views.builder.finder === true, JSON.stringify(views.builder));
// THE FINDER IS THE BUILDER'S, and only the builder's: the simulator keeps the
// build bar (you test a build there) and finds nothing.
check("...and is ABSENT from the simulator, which keeps only the build bar",
      views.simulator.finder === false && views.simulator.own === true, JSON.stringify(views.simulator));
check("...and from the optimizer, which owns no build",
      views.optimizer.finder === false && views.optimizer.own === false,
      JSON.stringify(views.optimizer));

await app.finish("all good");
