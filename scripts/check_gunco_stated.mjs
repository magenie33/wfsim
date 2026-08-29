// EVERY WEAPON SAYS WHICH CONDITION OVERLOAD RULE IT IS COMPUTED UNDER.
//
// The rules are PER WEAPON and hand-transcribed from a catalog: Adding or
// Multiplying, which attack parts take it, and what fraction of the base the
// term reads (docs/CATALOGS.md). The Burston Prime's fraction was wrong for
// months and was only caught because a player measured it — so the adopted
// rule belongs on the panel of every weapon that could ever take a GunCO
// source, not only on the ones where a source is already equipped. Someone who owns the gun can then argue with it.
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
  // …and EVERY form and EVERY part, which is the completeness claim.
  const allParts = async (weapon, evolutions) => {
    const p = await api('/api/panel', { weapon, mods: [], evolutions: evolutions || [] });
    const out = [];
    (p.forms || []).forEach((f, fi) => (f.parts || []).forEach((pt) => {
      const row = (pt.stats || []).find((s) => s.key === 'co');
      out.push({ form: fi, part: pt.id, rule: row && row.rule });
    }));
    return out;
  };

  // 1. NOTHING EQUIPPED, and the rule is still stated.
  const bare = await co('latron');
  out.bareThere = !!bare;
  out.bareFinal = bare && bare.final;
  out.bareNote = bare && bare.note;
  out.bareRule = bare && bare.rule;

  // 2. ALL THREE BEHAVIOURS, from three weapons the catalog classifies
  //    differently.
  out.adding = (await co('latron'))?.rule || '';
  out.multiplying = (await co('torid'))?.rule || '';
  out.inert = (await co('stug'))?.rule || '';
  // …and an ordinary weapon carries NO prose beside the slots.
  out.plainNote = (await co('latron'))?.note ?? null;

  // 3. A NON-DEFAULT FRACTION, named with its number. The Burston Prime's base
  //    form reads 46 of its evolved 88 — measured, MEASUREMENTS M48.
  const bp = await co('burston_prime',
    ['burston_prime_evo1_incarnon_form', 'burston_prime_forceful_finality']);
  out.fraction = bp?.rule || '';
  out.fractionNote = bp?.note || '';

  // 4. AN AoE PART THAT TAKES CO says so rather than being silently included.
  out.aoe = (await co('torid'))?.rule || '';

  // 5. COMPLETE: every form, every part, no gaps — including the ordinary
  //    100% ones, because a blank slot cannot be told from an unfilled one.
  out.burston = await allParts('burston_prime',
    ['burston_prime_evo1_incarnon_form', 'burston_prime_forceful_finality']);
  out.torid = await allParts('torid');
  return out;
})()`);

check("the CO rule is stated with NOTHING equipped", r.bareThere === true);
check("...and it says there is no source yet rather than a number",
  /no source/i.test(r.bareFinal || ""), r.bareFinal);
check("...and still spells out how it WOULD be computed",
  /would be computed/i.test(r.bareFinal || "") || /parts =/.test(r.bareRule || ""), r.bareRule);

// THE THREE SLOTS, in fixed order, so two weapons compare column for column.
const SLOTS = /^(additive|multiplying|inert) · base = .+ · parts = .+$/;
check("the rule is three fixed slots, not a sentence", SLOTS.test(r.adding), r.adding);
check("ADDING is named", /^additive · /.test(r.adding), r.adding);
check("MULTIPLYING is named", /^multiplying · /.test(r.multiplying), r.multiplying);
check("INERT is named", /^inert · /.test(r.inert), r.inert);
check("...and an ordinary weapon carries no prose beside the slots",
  r.plainNote === null, String(r.plainNote).slice(0, 60));
check("...and the three are three different sentences",
  new Set([r.adding, r.multiplying, r.inert]).size === 3);

check("a reduced base is printed as ORIGINAL of EVOLVED, with the percentage",
  /base = 52% \(46 of 88\)/.test(r.fraction), r.fraction);
check("...and the WHY is in the note beside it, not in the slots",
  /evolution raised/i.test(r.fractionNote), r.fractionNote.slice(0, 80));

check("an AoE part that takes CO is named in the parts slot",
  /parts = direct \+ field/.test(r.aoe), r.aoe);

// COMPLETENESS, which is the point of the whole row: an ordinary attack part
// states its ordinary 100% rather than staying silent, so "no line" always
// means "nobody filled this in" and never "nothing to say here".
const every = [...r.burston, ...r.torid];
check("every form and every part carries a rule",
  every.length >= 5 && every.every((p) => !!p.rule),
  every.map((p) => `${p.form}/${p.part}=${p.rule ? "ok" : "MISSING"}`).join(" "));
check("...including the ordinary ones, which say 100% rather than nothing",
  every.some((p) => /base = 100%/.test(p.rule)),
  every.map((p) => p.rule).join(" | ").slice(0, 120));
check("...and two forms of one weapon state their OWN bases",
  new Set(r.burston.map((p) => p.rule)).size >= 2,
  r.burston.map((p) => `${p.form}/${p.part}: ${p.rule}`).join("  ||  "));

await finish("every weapon states the Condition Overload rule it is computed under");
