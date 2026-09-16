// THE GAIN SCAN OBEYS THE TIER LADDER.
//
// Tier N of an evolution set is choosable only once N-1 is filled — a tier-2
// perk with no tier 1 is not a weaker build, it is not a build. The builder
// greys those rows out, and the quick-calc gain scan must not measure them
// anyway — that ranks evolutions nobody can click, on builds that cannot
// exist.
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
  history.pushState({},'','/weapons/Torid'); route(); await sleep(3000);
  const ids = () => gainCandidates({kind:'evo',idx:0}).map(c=>c.id);
  const none = ids();
  evoSel = {1:'torid_evo1_incarnon_form'}; const one = ids();
  evoSel = {1:'torid_evo1_incarnon_form',2:'torid_final_fusillade'}; const two = ids();
  evoSel = {1:'torid_evo1_incarnon_form',2:'torid_final_fusillade',3:'torid_extended_volley'}; const three = ids();
  const swap = gainCandidates({kind:'evo',idx:0}).find(c=>c.id==='torid_plentiful_mayhem');
  return { none, one, two, three, swap: swap && swap.payload.evolutions };
})()`);

const t3 = ['torid_extended_volley','torid_renewed_horror','torid_swift_deliverance'];
const t4 = ['torid_commodores_fortune','torid_elemental_balance','torid_survivors_edge'];
check("with nothing chosen, only tier 1 is offered",
  r.none.length === 1 && r.none[0] === 'torid_evo1_incarnon_form', r.none.join(","));
check("tier 1 chosen opens tier 2, and no further",
  r.one.length === 2 && !r.one.some((x) => t3.includes(x) || t4.includes(x)), r.one.join(","));
check("tier 2 chosen opens tier 3, its own tier still swappable",
  r.two.includes('torid_plentiful_mayhem') && t3.every((x) => r.two.includes(x)) &&
  !r.two.some((x) => t4.includes(x)), r.two.join(","));
check("tier 3 chosen opens tier 4", t4.every((x) => r.three.includes(x)), r.three.join(","));
check("a swap replaces ONE tier and leaves the rest alone",
  JSON.stringify(r.swap) === JSON.stringify(['torid_evo1_incarnon_form','torid_plentiful_mayhem','torid_extended_volley']),
  JSON.stringify(r.swap));

// THE MODE AXIS IS THE CHEAPEST ONE, and this is why: a form carries no mod
// pool of its own, so switching mode leaves every mod, arcane and evolution
// exactly where it is and the candidate is ONE request field. A payload that
// grew a second field would be a build the reader did not ask for.
//
// Its one exclusion is the ladder's own: a mode a mod has taken off the weapon
// is still listed and still not measured.
const m = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  history.pushState({},'','/weapons/Torid'); route(); await sleep(3000);
  slots.forEach(s => { s.mod = null; s.rank = null; });
  const w = weaponInfo('torid');
  const cands = () => gainCandidates({kind:'mode',idx:0});
  const open = cands();
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
  const unblocked = cands().map(c => c.id);
  slots[0] = { mod: blocker, pol: slots[0].pol, rank: null };
  const blocked = cands().map(c => c.id);
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

await app.finish("the gain scan obeys the ladder");
