// A STANCE IS AN AURA: IT HANDS CAPACITY BACK, AND THE PAGE HAS TO SAY SO.
//
// "All Stances provide a bonus mod capacity of 5 when maxed, doubling it to 10
// when placed on the matching polarity" (wiki, Stance), and the Aura page's
// third case: a slot of a DIFFERENT polarity grants "80% of listed drain,
// rounded down" — 4. So the number beside the mod slots is the weapon's own
// capacity plus one of THREE grants, and which one is the slot's colour against
// the stance's.
//
// ASSERTED ON SCREEN, because the page owns this arithmetic: it mirrors
// `engine::mods::stance_capacity` rather than asking for it, so an engine that
// is right and a page that is not would still read wrong to everyone.
//
//   node scripts/check_stance_capacity.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Magistar'); route(); await sleep(3500);
  const cap = () => document.getElementById('capacity').textContent;
  const forma = () => document.getElementById('forma').textContent;
  const out = { slotPol: stancePolOf(document.getElementById('weapon').value) };
  out.empty = cap();
  slots[STANCE].mod = 'crushing_ruin'; slots[STANCE].pol = null;
  renderMods(); await sleep(400);
  out.mismatched = cap();
  out.mismatchedForma = forma();
  slots[STANCE].mod = 'shattering_storm';
  renderMods(); await sleep(400);
  out.matched = cap();
  out.matchedForma = forma();
  slots[STANCE].mod = 'crushing_ruin'; slots[STANCE].pol = 'Madurai';
  renderMods(); await sleep(400);
  out.polarized = cap();
  out.polarizedForma = forma();
  // …AND THE AUTO PLAN SPENDS THE GRANT. Two Umbra mods in a build that fits
  // without an Umbra Forma: planning against the weapon's capacity alone buys
  // polarizations it does not need and reaches for the scarce item to do it.
  const build = ['sacrificial_steel', 'sacrificial_pressure', 'killing_blow', 'organ_shatter',
                 'seismic_wave', 'corrupt_charge', 'gladiator_might', 'primed_fever_strike'];
  build.forEach((id, i) => { slots[i].mod = id; });
  slots[STANCE].mod = 'shattering_storm'; slots[STANCE].pol = null;
  renderMods(); await sleep(400);
  autoForma(); renderMods(); await sleep(400);
  out.autoCap = cap();
  out.autoForma = forma();
  return out;
})()`);

// The Magistar's stance slot is Vazarin, and rank 30 with a catalyst is 60.
// Crushing Ruin is Madurai, so it is the mismatched case until a Forma lands.
check("the stance slot's own polarity reaches the page", r.slotPol === "Vazarin", r.slotPol);
check("an empty stance slot grants nothing", r.empty === "0 / 60", r.empty);
check("a mismatched stance grants four", r.mismatched === "0 / 64", r.mismatched);
check("...and costs no Forma to leave alone", r.mismatchedForma === "0 Forma", r.mismatchedForma);
check("a matching stance doubles it", r.matched === "0 / 70", r.matched);
check("...and it is still free", r.matchedForma === "0 Forma", r.matchedForma);
check("polarizing the slot buys the double", r.polarized === "0 / 70", r.polarized);
check("...for exactly one Forma", r.polarizedForma === "1 Forma", r.polarizedForma);

// …AND THE AUTO PLAN AGREES WITH THE ENGINE, which answers 5 regular Forma and
// no Umbra for this build at a capacity of 70. Planning against 60 needs seven
// polarizations and spends an Umbra Forma on one of them.
check("the auto plan spends the stance's capacity", r.autoCap === "68 / 70", r.autoCap);
check("...and no Umbra Forma it does not need", r.autoForma === "5 Forma", r.autoForma);

process.exit(0);
