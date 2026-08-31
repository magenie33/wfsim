// A STANCE IS AN AURA: IT HANDS CAPACITY BACK, AND THE PAGE HAS TO SAY SO.
//
// "All Stances provide a bonus mod capacity of 5 when maxed, doubling it to 10
// when placed on the matching polarity" (wiki, Stance) — so the number beside
// the mod slots is the weapon's own capacity PLUS the grant, and which grant
// depends on the slot's colour against the stance's.
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
  return out;
})()`);

// The Magistar's stance slot is Vazarin, and rank 30 with a catalyst is 60.
check("the stance slot's own polarity reaches the page", r.slotPol === "Vazarin", r.slotPol);
check("an empty stance slot grants nothing", r.empty === "0 / 60", r.empty);
check("a mismatched stance grants five", r.mismatched === "0 / 65", r.mismatched);
check("...and costs no Forma to leave alone", r.mismatchedForma === "0 Forma", r.mismatchedForma);
check("a matching stance doubles it", r.matched === "0 / 70", r.matched);
check("...and it is still free", r.matchedForma === "0 Forma", r.matchedForma);
check("polarizing the slot buys the double", r.polarized === "0 / 70", r.polarized);
check("...for exactly one Forma", r.polarizedForma === "1 Forma", r.polarizedForma);

process.exit(0);
