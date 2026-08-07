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
//  2. IT STAYS IN THE BUILDER. The benchmark bar and its note are the build
//     collection's read-only half, so the optimizer — which owns no build —
//     must not show them. Hiding is by CSS id list, which is the kind of thing
//     a new element silently falls out of.
//
//   node scripts/check_benchmark_build.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";
const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send, BASE } = app;
// THE REAL BOARD, not an injected one: the point of the cold path is that the
// page RELOADS, and an in-memory injection does not survive that — `BOARD` is
// fetched from /board.json on boot. So the check finds the weapon that actually
// has a row and skips cleanly if the board is empty (which it is before the
// first submission, and that is an ordinary state).
const WEAPON = await evaluate(`(async () => {
  const r = await fetch('/board.json', {cache:'no-cache'});
  const b = r.ok ? await r.json() : {};
  const id = Object.keys(b)[0] || null;
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
  const bar = document.getElementById('bench-bar-builder-builds');
  const chip = [...bar.querySelectorAll('.pchip')].find(c => c.dataset.name === '#1');
  chip.click(); await sleep(1500);
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
  // And the note is on screen, naming the benchmark.
  const note = document.getElementById('build-official');
  out.noteShown = note && !note.hidden;
  out.noteText = note ? (note.textContent||'').replace(/\\s+/g,' ').trim().slice(0,120) : null;
  return out;
})()`);
console.log("");
check("a cold load restores the benchmark build", cold.official === true, String(cold.active));
check("...with its Forma planned, not left unpolarised", cold.pols > 0, `polarities ${cold.pols}`);
check("...so the build FITS", cold.used !== null && cold.total !== null && cold.used <= cold.total,
      `${cold.cap} (${cold.overCls})`);
check("...and the note names its benchmark", !!cold.noteShown && /Single Target|单体/.test(cold.noteText||""),
      cold.noteText);
check("the cold and warm plans agree", cold.pols === warm.pols, `cold ${cold.pols} vs warm ${warm.pols}`);
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
                  bar: vis('bench-bar-builder-builds'), note: vis('build-official'),
                  own: vis('preset-bar-builder-builds') };
  }
  return out;
})()`);
console.log("");
check("the benchmark bar shows in the builder", views.builder.bar === true, JSON.stringify(views.builder));
// The build bar is deliberately visible on the simulator (you test a build
// there), so its benchmark half follows it — the rule is that the two travel
// together, not that the bar is builder-only.
check("...and follows the build bar on the simulator",
      views.simulator.bar === views.simulator.own, JSON.stringify(views.simulator));
check("...and is ABSENT from the optimizer, which owns no build",
      views.optimizer.bar === false && views.optimizer.note === false && views.optimizer.own === false,
      JSON.stringify(views.optimizer));

await app.finish("all good");
