// HOW FULL A BUILD MUST BE IS A RANGE, AND BOTH ENDS TRAVEL.
//
// The optimizer used to offer only a ceiling ("max mods / build"), so
// "search only full 8-mod builds" was not a thing you could ask for — and
// every search paid for the sizes below it (user, 2026-08-03: "应该搜索器可以
// 有个设置，例如必须8个，<=8个，<=7个"). The floor is its own control now.
//
// Asserts what is on SCREEN and in the REQUEST, not what is in a variable:
// the two ends push each other, the scope estimate follows the floor, the
// pair survives a preset round-trip, and the run actually sends build_min.
//
//   node scripts/check_build_size.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/Verglas_Prime'); route(); await sleep(3000);
  document.querySelector('[data-tab="optimizer"]')?.click(); await sleep(500);
  const out = {};
  const set = async (el, v) => { const n=document.getElementById(el); n.value=String(v); n.dispatchEvent(new Event('input',{bubbles:true})); await sleep(200); };
  out.present = !!document.getElementById('opt-min') && !!document.getElementById('opt-size');

  // Pool ten mods so the scope estimate has something to count.
  const ids=['serration','split_chamber','point_strike','vital_sense','hammer_shot','cryo_rounds','infected_clip','hellfire','stormbringer','malignant_force'];
  ids.forEach(i=>{opt.mods[i]='search';});
  updateOptEstimate(); renderOptMods(); await sleep(300);

  // The floor moves the estimate: fewer sizes to enumerate, fewer builds.
  await set('opt-min', 1); const wide = document.getElementById('opt-estimate')?.textContent || '';
  out.wideJobs = Number((wide.match(/([\\d,]+)\\s*(?:candidate|build)/i)||[])[1]?.replace(/,/g,'')) || null;
  await set('opt-min', 8); const tight = document.getElementById('opt-estimate')?.textContent || '';
  out.tightJobs = Number((tight.match(/([\\d,]+)\\s*(?:candidate|build)/i)||[])[1]?.replace(/,/g,'')) || null;

  // A floor above the ceiling is not a scope: raising one pushes the other,
  // and the SCREEN shows it, not just the variable.
  out.pushedMax = document.getElementById('opt-size').value;
  await set('opt-size', 3);
  out.pulledMin = document.getElementById('opt-min').value;

  // The pair is part of the search preset, so it survives a round trip.
  await set('opt-min', 2); await set('opt-size', 6);
  const snap = snapshotOpt();
  opt.min = 1; opt.size = 8; applyOptState(snap); renderOptMods(); await sleep(200);
  out.restored = [document.getElementById('opt-min').value, document.getElementById('opt-size').value];

  // And it reaches the request.
  const seen = [];
  const realApi = window.api;
  window.api = async (path, body) => { seen.push([path, body]); throw new Error('stop'); };
  try { await runOptimize(); } catch {}
  window.api = realApi;
  const req = (seen.find(([p]) => p === '/api/optimize') || [])[1] || {};
  out.sent = [req.build_min, req.build_size];
  return out;
})()`);

check("both ends exist on screen", r.present === true);
check("the floor narrows the scope", r.wideJobs > r.tightJobs, `${r.wideJobs} -> ${r.tightJobs}`);
check("raising the floor pushes the ceiling", r.pushedMax === "8", `ceiling ${r.pushedMax}`);
check("lowering the ceiling pulls the floor", r.pulledMin === "3", `floor ${r.pulledMin}`);
check("the pair survives a preset round trip", String(r.restored) === "2,6", String(r.restored));
check("the request carries build_min", String(r.sent) === "2,6", String(r.sent));

await app.finish("build size is a range, and both ends reach the request");
