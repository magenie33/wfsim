// THE GAIN SCAN OBEYS THE TIER LADDER.
//
// Tier N of an evolution set is choosable only once N-1 is filled — a tier-2
// perk with no tier 1 is not a weaker build, it is not a build. The builder
// greys those rows out; the quick-calc gain scan used to measure them anyway,
// so the picker ranked evolutions nobody could click, on builds that cannot
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

await app.finish("the gain scan obeys the ladder");
