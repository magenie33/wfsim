// EVERY WEAPON SAYS WHICH CONDITION OVERLOAD RULE IT IS COMPUTED UNDER.
//
// The rules are PER WEAPON and hand-transcribed from a catalog: Adding or
// Multiplying, which attack parts take it, and what fraction of the base the
// term reads (docs/CATALOGS.md). The Burston Prime's fraction was wrong for
// months and was only caught because a player measured it — so the adopted
// rule belongs on the panel of every weapon that could ever take a GunCO
// source, not only on the ones where a source is already equipped (owner,
// 2026-08-16). Someone who owns the gun can then argue with it.
//
//   node scripts/check_gunco_stated.mjs
//
// It is a STATEMENT OF METHOD, not an admission — `unmodeled:` and the
// disclosure banner are for what the sim cannot do; this is what it does.
//
// Four claims:
//
//   · THE ROW IS THERE WITH NOTHING EQUIPPED. That is the whole point: the
//     row used to appear only once a CO card was on the build, so the one
//     thing a reader could check was invisible until they had committed to it.
//   · EVERY BEHAVIOUR IS SPELLED OUT, and the roster exercises all three —
//     Adding, Multiplying and inert are three different sentences, so a page
//     that printed one of them for everything would fail here.
//   · A NON-DEFAULT FRACTION IS NAMED WITH ITS NUMBER. A weapon whose CO reads
//     less than its whole base has to say so and say how much.
//   · …AND AN AoE PART THAT TAKES CO SAYS SO. "Direct hits only" is the rule
//     every unlisted weapon follows; the exceptions are per entry.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const out = {};
  const co = async (weapon, evolutions) => {
    const p = await api('/api/panel', { weapon, mods: [], evolutions: evolutions || [] });
    const found = [];
    const walk = (o) => {
      if (Array.isArray(o)) return o.forEach(walk);
      if (o && typeof o === 'object') {
        if (o.key === 'co') found.push(o);
        Object.values(o).forEach(walk);
      }
    };
    walk(p);
    return found[0] || null;
  };

  // 1. NOTHING EQUIPPED, and the rule is still stated.
  const bare = await co('latron');
  out.bareThere = !!bare;
  out.bareFinal = bare && bare.final;
  out.bareNote = bare && bare.note;

  // 2. ALL THREE BEHAVIOURS, from three weapons the catalog classifies
  //    differently.
  out.adding = (await co('latron'))?.note || '';
  out.multiplying = (await co('torid'))?.note || '';
  out.inert = (await co('stug'))?.note || '';

  // 3. A NON-DEFAULT FRACTION, named with its number. The Burston Prime's base
  //    form reads 46 of its evolved 88 — measured, MEASUREMENTS M48.
  out.fraction = (await co('burston_prime',
    ['burston_prime_evo1_incarnon_form', 'burston_prime_forceful_finality']))?.note || '';

  // 4. AN AoE PART THAT TAKES CO says so rather than being silently included.
  out.aoe = (await co('torid'))?.note || '';
  return out;
})()`);

check("the CO rule is stated with NOTHING equipped", r.bareThere === true);
check("...and it says there is no source yet rather than a number",
  /no source/i.test(r.bareFinal || ""), r.bareFinal);
check("...and still spells out how it WOULD be computed",
  /would be computed/i.test(r.bareNote || ""), (r.bareNote || "").slice(0, 90));

check("ADDING is named as joining the base-damage bracket",
  /base-damage bracket/i.test(r.adding), r.adding.slice(0, 70));
check("MULTIPLYING is named as an independent multiplier",
  /independent multiplier/i.test(r.multiplying), r.multiplying.slice(0, 70));
check("INERT says the bonus does not apply at all",
  /INERT/.test(r.inert), r.inert.slice(0, 70));
check("...and the three are three different sentences",
  new Set([r.adding, r.multiplying, r.inert]).size === 3);

check("a weapon whose CO reads less than its base says so, with the number",
  /ORIGINAL \d+ base only/.test(r.fraction) && /%\s*effectiveness/.test(r.fraction),
  r.fraction.slice(0, 110));

check("an AoE part that takes CO is named rather than silently included",
  /direct hits AND/.test(r.aoe), r.aoe.slice(0, 90));

await finish("every weapon states the Condition Overload rule it is computed under");
