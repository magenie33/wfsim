// HOW FULL A BUILD MUST BE IS A RANGE, AND BOTH ENDS TRAVEL.
//
// The optimizer used to offer only a ceiling ("max mods / build"), so
// "search only full 8-mod builds" was not a thing you could ask for — and
// every search paid for the sizes below it (user, 2026-08-03). The
// floor is its own control now.
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
  // …AND ON ITS OWN ROW, not sharing a flex line with the search box, where a
  // column-stacked label made that row four lines tall and read as a setting
  // ON the filter (owner, 2026-08-29).
  out.ownRow = !!document.getElementById('opt-min')?.closest('#opt-size-row')
    && !document.getElementById('opt-min')?.closest('.opt-mods-head');

  // AN EMPTY SCOPE IS THE BARE WEAPON, and it is a legal search. Every other
  // axis reads "nothing marked" as the EMPTY option; this one answered it with
  // an error, because the floor was clamped to 1. Asserted FIRST, on the state
  // an untouched tab is actually in.
  out.emptyFloor = document.getElementById('opt-min').value;
  out.emptyRuns = !document.getElementById('run-opt').disabled;
  out.emptyEst = (document.getElementById('opt-estimate')?.textContent || '').trim();

  // Pool ten mods so the scope estimate has something to count.
  const ids=['serration','split_chamber','point_strike','vital_sense','hammer_shot','cryo_rounds','infected_clip','hellfire','stormbringer','malignant_force'];
  ids.forEach(i=>{opt.mods[i]='search';});
  updateOptEstimate(); renderOptMods(); await sleep(300);

  // THE MARKS RAISE THEIR OWN FLOOR, AND THE ROW SAYS SO. With a required mod
  // and pooled ones, the box may read 0 over a search that never looks below 2
  // — a control that lies about what it does unless the difference is stated.
  opt.mods['serration'] = 'fixed';
  await set('opt-min', 0);
  out.effWhenRaised = (document.getElementById('opt-size-eff')?.textContent || '').trim();
  // …and NOTHING when the two agree: a line repeating the two numbers beside it
  // distinguishes nothing. This is the half that catches a permanent label.
  await set('opt-min', 8);
  out.effWhenAgreed = (document.getElementById('opt-size-eff')?.textContent || '').trim();
  opt.mods['serration'] = 'search';
  updateOptEstimate(); renderOptMods(); await sleep(200);

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
  await set('opt-size', 8);
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
check("...on a row of their own, off the search box", r.ownRow === true);
check("an untouched scope floors at 0", r.emptyFloor === "0", `"${r.emptyFloor}"`);
check("...so the bare weapon is a legal search, not an error",
  r.emptyRuns === true && r.emptyEst.includes("~1"),
  `run enabled ${r.emptyRuns} ` + JSON.stringify(r.emptyEst.slice(0, 60)));
check("...and when the marks raise the floor, the row says what to",
  /2/.test(r.effWhenRaised) && r.effWhenRaised.length > 0, JSON.stringify(r.effWhenRaised));
check("...and says nothing when they agree with it", r.effWhenAgreed === "",
  JSON.stringify(r.effWhenAgreed));
check("the floor narrows the scope", r.wideJobs > r.tightJobs, `${r.wideJobs} -> ${r.tightJobs}`);
check("raising the floor pushes the ceiling", r.pushedMax === "8", `ceiling ${r.pushedMax}`);
check("lowering the ceiling pulls the floor", r.pulledMin === "3", `floor ${r.pulledMin}`);
check("the pair survives a preset round trip", String(r.restored) === "2,6", String(r.restored));
check("the request carries build_min", String(r.sent) === "2,6", String(r.sent));

await app.finish("build size is a range, and both ends reach the request");
