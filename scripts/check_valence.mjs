// THE VALENCE BONUS — an adversary weapon's bonus element, on screen and in the
// number.
//
// The TWENTY-SEVENTH check, and the first about a build axis that is a property
// of the COPY a player owns rather than of the model. A Kuva Lich hands out
// 25–60% of base damage as one of seven elements, so two Kuva Nukors are two
// different weapons and neither is "the" Kuva Nukor (owner, 2026-08-13:
// "kuva武器有个初属性。类似evo得多一块建立").
//
// It is checked on the NUMBER rather than on the control, for the reason every
// axis here is: a dropdown that stores a value nobody reads looks exactly like
// one that works. And on the ARITHMETIC rather than on a direction, because the
// wiki states the rule exactly — "ranging from 25-60% of the weapon's base
// damage … applies as weapon base damage" — so 21 Radiation plus a 60% Toxin
// progenitor is 33.6, and nothing else.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);

  const spec = (weaponInfo('kuva_nukor') || {}).valence || null;
  const block = document.getElementById('element-block');
  const shown = !!block && !block.hidden;

  // The panel's own base, which is where a base-damage add has to land.
  const panel = async () => {
    const s = await api('/api/simulate', { ...buildPayload(), ...fightPayload(sim), runs: 2 });
    return { base: (s.panel || {}).modified_base, dmg: (s.panel || {}).damage };
  };
  const bare = await panel();

  // Pick TOXIN at the ceiling: it is not the weapon's own element, so it has to
  // arrive beside the Radiation rather than merge into it.
  valence.element = 'toxin'; valence.bonus = 0.60;
  renderValence(); refreshPanel(); await sleep(1500);
  const toxin = await panel();

  // …and RADIATION at the ceiling, which the weapon already deals: it must
  // MERGE, so the total is the same 33.6 with one element and not two.
  valence.element = 'radiation';
  renderValence(); refreshPanel(); await sleep(1500);
  const rad = await panel();

  // IT SAVES WITH THE BUILD, not the fight: a valence is a statement about one
  // weapon, and the fight is shared across the roster.
  const buildDoc = snapshotState();
  const inFight = JSON.stringify(snapshotScenario()).includes('valence');

  // …AND NOTHING CROSSES BETWEEN WEAPONS. Open an ordinary weapon and the axis
  // is gone, along with the choice.
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(2500);
  const otherShown = !!document.getElementById('element-block')
    && !document.getElementById('element-block').hidden;
  const otherValence = JSON.parse(JSON.stringify(valence));

  return { spec, shown, bare, toxin, rad,
           savedElement: (buildDoc.valence || {}).element,
           savedBonus: (buildDoc.valence || {}).bonus,
           inFight, otherShown, otherValence };
})()`);

check("the weapon declares which elements its Lich can roll",
  r.spec && r.spec.elements.length === 7 && r.spec.min === 0.25 && r.spec.max === 0.6,
  JSON.stringify(r.spec));
// PUNCTURE AND SLASH ARE NOT PROGENITOR ELEMENTS. A list that merely had seven
// entries would pass a length check and be wrong.
check("...the seven the wiki names, and not one more",
  r.spec && !r.spec.elements.includes("puncture") && !r.spec.elements.includes("slash")
    && ["impact", "heat", "cold", "electricity", "toxin", "magnetic", "radiation"]
      .every((e) => r.spec.elements.includes(e)),
  JSON.stringify(r.spec && r.spec.elements));
check("the block is drawn for an adversary weapon", r.shown === true, String(r.shown));
// 21 Radiation is the infobox's own number.
check("no element picked is the weapon's printed panel",
  Math.abs(r.bare.base - 21) < 1e-6, String(r.bare.base));
// THE ARITHMETIC, exactly: 21 + 21 × 0.60 = 33.6, as BASE damage.
check("a 60% Toxin progenitor is +12.6 Toxin beside the Radiation",
  Math.abs(r.toxin.base - 33.6) < 1e-6, `${r.bare.base} -> ${r.toxin.base}`);
// …AND THE MERGE, which is the half a "new element" implementation gets wrong.
check("...and a Radiation one merges into the 21 it already deals",
  Math.abs(r.rad.base - 33.6) < 1e-6, String(r.rad.base));
check("it saves with the BUILD, not the fight",
  r.savedElement === "radiation" && r.savedBonus === 0.6 && r.inFight === false,
  JSON.stringify({ element: r.savedElement, bonus: r.savedBonus, inFight: r.inFight }));
check("an ordinary weapon has no such axis, and inherits no choice",
  r.otherShown === false && r.otherValence.element === "",
  JSON.stringify({ shown: r.otherShown, carried: r.otherValence }));

await app.finish("an adversary weapon's Valence bonus reaches its damage");
