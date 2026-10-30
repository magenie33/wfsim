// THE GAIN SCAN SWAPS ONE TIER.
//
// Every tier is always installed — a finished Genesis cannot be emptied — so
// an evolution candidate is the same build with ONE tier's perk replaced, and
// a weapon opened fresh holds each tier's first option.
//
//   node scripts/check_gain_axes.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/Torid/builder'); route(); await sleep(3000);
  const held = { ...evoSel };
  const cands = await gainCandidates({kind:'evo',idx:0});
  const swap = cands.find(c=>c.id==='torid_plentiful_mayhem');
  return { held, ids: cands.map(c=>c.id), swap: swap && swap.payload.evolutions };
})()`);

const defaults = ['torid_evo1_incarnon_form','torid_final_fusillade','torid_swift_deliverance','torid_commodores_fortune'];
const others = ['torid_plentiful_mayhem','torid_renewed_horror','torid_extended_volley','torid_survivors_edge','torid_elemental_balance'];
check("a weapon opens holding each tier's first option",
  JSON.stringify(Object.values(r.held)) === JSON.stringify(defaults), JSON.stringify(r.held));
check("every other option of every tier is a candidate, and no installed one",
  others.every((x) => r.ids.includes(x)) && !r.ids.some((x) => defaults.includes(x)), r.ids.join(","));
check("a swap replaces ONE tier and leaves the rest alone",
  JSON.stringify(r.swap) === JSON.stringify(['torid_evo1_incarnon_form','torid_plentiful_mayhem','torid_swift_deliverance','torid_commodores_fortune']),
  JSON.stringify(r.swap));

// THE MODE AXIS IS THE CHEAPEST ONE, and this is why: a form carries no mod
// pool of its own, so switching mode leaves every mod, arcane and evolution
// exactly where it is and the candidate is ONE request field. A payload that
// grew a second field would be a build the reader did not ask for.
//
// Its one exclusion: a mode a mod has taken off the weapon is still listed
// and still not measured.
const m = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  history.pushState({},'','/weapons/Torid/builder'); route(); await sleep(3000);
  slots.forEach(s => { s.mod = null; s.rank = null; });
  const w = weaponInfo('torid');
  const cands = () => gainCandidates({kind:'mode',idx:0});
  const open = await cands();
  const blocker = ((w.evo_forbids || {})[w.unlock_evo] || [])[0];
  // THE CONTROL IS A PLAIN DROPDOWN AND STILL AN AXIS: what makes a list
  // measure is the declaration, not the shape of the thing that opens it.
  const trigger = document.querySelector('#mode-row [data-dd]');
  const declared = ((ddReg.get('dd-mode') || {}).axis || {}).kind || '';
  // READ FROM BASE, BOTH SIDES OF THE BLOCKER. A weapon with a cycle OPENS in
  // it, and the current mode is never a candidate — so from there the cycle
  // is missing for the wrong reason and the exclusion is never reached.
  const opened = mode;
  mode = 'base';
  const unblocked = (await cands()).map(c => c.id);
  slots[0] = { mod: blocker, pol: slots[0].pol, rank: null };
  const blocked = (await cands()).map(c => c.id);
  mode = opened;
  return {
    unblocked,
    declared,
    isButton: !!trigger && trigger.tagName === 'BUTTON',
    all: (w.modes || []).slice(),
    cur: mode,
    cycles: (w.modes || []).filter(isCycleMode),
    open: open.map(c => c.id),
    fields: [...new Set(open.flatMap(c => Object.keys(c.payload)))],
    carried: open.every(c => c.payload.mode === c.id),
    blocker,
    blocked,
  };
})()`);

check("the mode control declares the axis and stays one click away",
  m.declared === "mode" && m.isButton, `${m.declared} / button=${m.isButton}`);
check("every other mode is a candidate, and the current one is not",
  m.open.length === m.all.length - 1 && !m.open.includes(m.cur) &&
  m.all.filter((x) => x !== m.cur).every((x) => m.open.includes(x)),
  `${m.cur} -> ${m.open.join(",")}`);
check("a mode candidate overrides ONE field, and it is its own id",
  JSON.stringify(m.fields) === JSON.stringify(["mode"]) && m.carried,
  m.fields.join(","));
check("from base, with nothing blocking it, a cycle IS measured",
  !!m.blocker && m.unblocked.some((x) => m.cycles.includes(x)),
  `${m.blocker} / ${m.unblocked.join(",")}`);
check("a mod that takes the Incarnon form off leaves no cycle to measure",
  !m.blocked.some((x) => m.cycles.includes(x)), m.blocked.join(","));

await app.finish("the gain scan swaps one tier");
