// A COLOUR CANNOT BE PUT IN A DRAWER, AND THE AUTO PLAN HAS TO SAY WHERE IT WENT.
//
// The weapon has nine slots and every innate polarity sits on one of them, so a
// colour no mod wants is not simply absent: it lands on a mod-less slot (free,
// and it changes no number) or on a modded one (+25%) — unless a Forma spent
// elsewhere overwrites it, which each one bought does for nothing, because the
// bill is `max(added, removed)`.
//
// Two ways to get that wrong, and the page has had both:
//
//   - PARKING A COLOUR NOBODY WANTS on a modded slot when dropping it was free.
//     The reader pays 25% of that mod for nothing.
//   - PLANNING AS IF IT VANISHED. The fit loop then measures a drain the
//     layout does not have, declares a fit, and the panel prints 64 / 60.
//
// Ballistica Prime is the shape that exposes both: a pool of four colours
// ({Naramon, Madurai} x2) over nine slots, so a nine-mod build has no mod-less
// slot to park anything on.
//
//   node scripts/check_forma_plan.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Ballistica_Prime'); route(); await sleep(3500);
  const w = document.getElementById('weapon').value;
  const cap = () => capOf(w) + stanceGrant();
  const bill = () => { const f = formaCount(); return f.regular + f.umbra + f.omni; };
  // A slot wearing a colour its own mod does not want.
  const reds = () => slots.slice(0, 9)
    .map((s, i) => ({ i, m: modById(s.mod), p: s.pol }))
    .filter((x) => x.m && x.p && x.p !== x.m.polarity
                   && !(x.p === 'Omni' && x.m.polarity !== 'Umbra'));

  const out = { pool: innate.slice(0, 9).filter(Boolean), cases: [], pageForma: null };

  // ALL-MADURAI BUILDS, so the two innate Naramon are leftovers with nowhere to
  // go, and a window over the pool so the answer is not one lucky build.
  const mad = currentPool.filter((m) => m.polarity === 'Madurai' && !m.exilus)
    .sort((a, b) => b.drain - a.drain);
  const exm = currentPool.filter((m) => m.exilus && m.polarity === 'Madurai')
    .sort((a, b) => b.drain - a.drain)[0];
  out.exilusMod = exm ? exm.id : null;
  for (let off = 0; off + 8 <= mad.length; off++) {
    for (let i = 0; i < 10; i++) { slots[i].mod = null; slots[i].pol = null; }
    mad.slice(off, off + 8).forEach((m, i) => { slots[i].mod = m.id; });
    slots[8].mod = exm.id;
    autoForma();
    const planned = { forma: bill(), used: capacityUsed(), cap: cap() };
    // EVERY MOD MATCHED is the end of the road: nothing is left to buy, so a
    // build still over capacity there is impossible rather than mis-planned.
    planned.allMatched = slots.slice(0, 9)
      .every((s) => !s.mod || s.pol === modById(s.mod).polarity);
    // What those red slots are BUYING: drop them and see whether the bill rises.
    const red = reds();
    const keep = red.map((x) => x.p);
    red.forEach((x) => { slots[x.i].pol = null; });
    const dropped = { forma: bill(), used: capacityUsed() };
    red.forEach((x, k) => { slots[x.i].pol = keep[k]; });
    out.cases.push({ off, reds: red.length, planned, dropped });
  }

  // THE BILL IS A STATE, NOT A HISTORY. The exilus slot's colour, off and back
  // on, has to land on the number it started from — the owner read the same
  // slot as charging twice for one polarity.
  for (let i = 0; i < 10; i++) { slots[i].mod = null; slots[i].pol = null; }
  mad.slice(0, 8).forEach((m, i) => { slots[i].mod = m.id; });
  slots[8].mod = exm.id;
  autoForma(); await sleep(200);
  const pick = (idx, label) => {
    openPolMenu(idx);
    const rows = Array.from(document.getElementById('slot-menu').querySelectorAll('.mi'));
    (label === null ? rows[rows.length - 1] : rows.find((x) => x.dataset.p === label)).click();
  };
  const start = { pol: slots[8].pol, forma: bill() };
  pick(8, null); await sleep(200);
  const blanked = bill();
  pick(8, start.pol); await sleep(200);
  out.trip = { start: start.forma, blanked, back: bill() };

  // …AND THE PAGE ANSWERS WHAT THE ENGINE ANSWERS. The plan is mirrored in JS
  // rather than asked for, so the two drift silently: same build, same bill.
  //
  // ONE FAMILY PER BUILD, AND IT HAS TO FIT: the server answers an error rather
  // than a plan for a build carrying two of a family (the biggest-drain eight
  // walk straight into Barrel Diffusion and its Amalgam) or for one no amount
  // of Forma can seat. So this walks down to the first legal build that costs
  // something — a parity check on a free build asserts 0 === 0.
  const taken = new Set(exm.family ? [exm.family] : []);
  const fam = [];
  for (const m of mad) {
    if (m.family && taken.has(m.family)) continue;
    if (m.family) taken.add(m.family);
    fam.push(m);
  }
  for (let off = 0; off + 8 <= fam.length && out.pageForma == null; off++) {
    for (let i = 0; i < 10; i++) { slots[i].mod = null; slots[i].pol = null; }
    fam.slice(off, off + 8).forEach((m, i) => { slots[i].mod = m.id; });
    slots[8].mod = exm.id;
    autoForma(); renderMods(); await sleep(50);
    if (capacityUsed() <= cap() && bill() > 0) {
      out.parityMods = slots.slice(0, 9).map((s) => s.mod);
      out.pageForma = bill();
    }
  }
  const sim = await api('/api/simulate', { ...buildPayload(), ...theFight(), runs: 1 });
  out.engine = (sim || {}).forma || null;
  return out;
})()`);

const cases = r.cases || [];
check("the weapon's pool is the four colours this check is about",
  (r.pool || []).join() === "Naramon,Madurai,Madurai,Naramon", JSON.stringify(r.pool));
check("there are builds to judge", cases.length > 0, String(cases.length));

// A red slot is a PURCHASE: it is there because dropping it would cost a Forma.
const gratuitous = cases.filter((c) => c.reds > 0 && c.dropped.forma <= c.planned.forma);
check("no slot wears a colour it could shed for free",
  gratuitous.length === 0, JSON.stringify(gratuitous.slice(0, 3)));

// …and every red one really is saving something, or it would not be there.
const saving = cases.filter((c) => c.reds > 0);
check("the reds that remain each save a Forma",
  saving.every((c) => c.dropped.forma > c.planned.forma), JSON.stringify(saving.slice(0, 3)));

// THE PLAN'S OWN ARITHMETIC. It counted matched-or-blank and never the +25% it
// was about to create, so it declared a fit at 64 / 60.
const overflowing = cases.filter((c) => c.planned.used > c.planned.cap && !c.planned.allMatched);
check("the plan never stops over capacity with a polarization still to buy",
  overflowing.length === 0, JSON.stringify(overflowing.slice(0, 3)));

check("a polarity off and back on comes back to the same bill",
  r.trip && r.trip.start === r.trip.back, JSON.stringify(r.trip));
check("...and taking it off moved the bill by one",
  r.trip && Math.abs(r.trip.blanked - r.trip.start) === 1, JSON.stringify(r.trip));

check("the engine answers the page's bill",
  r.engine && r.engine.used === r.pageForma,
  JSON.stringify({ page: r.pageForma, engine: r.engine }));

await app.finish("the auto plan puts every innate colour somewhere, and bills for it");
