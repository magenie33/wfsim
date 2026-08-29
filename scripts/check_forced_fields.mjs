/// THE THIRTY-FIFTH: A FIGHT CANNOT ASK EVERY WEAPON EVERY QUESTION, AND THE
/// PANEL SAYS SO WHERE THE QUESTION IS ASKED.
///
/// Three of the scenario's technique fields are not the reader's on every
/// weapon, and each is forced for a reason the weapon itself carries:
///
///   · AIMING          — a sentinel is fired by the companion and is always aiming
///   · INFINITE AMMO   — three states, not two: no reserve to run out of
///                       (sentinel, ticked), a reserve it cannot refill (ground
///                       Arch-Gun, unticked), or yours (everything else)
///   · HEADSHOT %      — a sentinel never aims at the head, and `parse_fight`
///                       forces 0 whatever the request carries
///
/// The first two have been pinned-and-explained since 2026-08-04. The THIRD was
/// not, and nothing here could see it: the value was right from both ends while
/// the control stayed a bare editable number, so a reader could type 100 on a
/// Verglas, watch the page take it, and get a run computed at 0 with nothing on
/// screen saying why. A column that is shown and not
/// applied looks exactly like one that works.
///
/// TWO THINGS ARE ASSERTED OF EACH, because either alone passes on a broken
/// panel: that it is PINNED, and that it NAMES what pinned it. A disabled field
/// with no reason is a dead end, and a reason on an editable field is a lie.
///
/// THE NEGATIVE CONTROL IS NOT OPTIONAL HERE — a check that only asserts
/// "disabled" passes perfectly on a panel that disables everything for
/// everybody, which would be a far worse bug than the one it is looking for.
/// So an ordinary weapon must have all three back, editable.
///
/// AND THE LAST ONE IS THE REGRESSION GUARD FOR HOW THIS WAS FIXED. The pin is
/// DISPLAY-ONLY, because `headshot_pct` lives in a scenario SHARED across the
/// whole roster: writing 0 into the state would rewrite the fight every time a
/// sentinel was opened, and auto-save would store it. So the reader's own value
/// has to survive a trip through a sentinel and come back.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep } = app;

/// The three fields as the panel draws them, for whichever weapon is open.
const FIELDS = `(() => {
  // Two boxes: what the WIELDER is doing (technique) and what the fight is
  // LIMITED by (limits). Ammo is the second kind, which is why this looks in
  // both rather than assuming one panel holds all three.
  const boxes = ['sim-technique', 'sim-limits']
    .map((id) => document.getElementById(id)).filter(Boolean);
  if (!boxes.length) return { missing: true };
  const pick = (k) => {
    let el = null;
    for (const b of boxes) { el = el || b.querySelector('[data-k="' + k + '"]'); }
    if (!el) return { there: false };
    const lab = el.closest('label');
    return {
      there: true,
      disabled: !!el.disabled,
      checked: el.type === 'checkbox' ? !!el.checked : null,
      value: el.type === 'checkbox' ? null : el.value,
      why: (lab && lab.getAttribute('title')) || '',
    };
  };
  return { aiming: pick('aiming'), ammo: pick('infinite_ammo'), head: pick('headshot_pct') };
})()`;

/// A SCENARIO OF THE READER'S OWN, BEFORE ANYTHING IS ASKED ABOUT A FIELD.
///
/// The app lands a first-time visitor on an OFFICIAL RULER, and a ruler pins
/// its whole fight — `lockOfficialScenario` sweeps every `input,select,button,
/// textarea` in the panel. So on a fresh profile EVERY field reads disabled,
/// for a reason that has nothing to do with the weapon, and the first version
/// of this check passed all three sentinel assertions on exactly that. It was
/// caught only because the NEGATIVE CONTROL failed: an ordinary Torid came back
/// with all three pinned too, which is the whole reason a negative control is
/// not optional here.
///
/// Third time this trap has been paid for in this repo — `check_arena.mjs`
/// names it, and `check_share.mjs` hit it the same week.
await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(3000);
  const bar = document.querySelector('#preset-bar-simulator-scenarios');
  const add = bar && bar.querySelector('.pchip.add');
  if (add) { add.click(); await sleep(1600); }
})()`);
const editable = await evaluate(
  `typeof officialScenarioActive === 'function' ? !officialScenarioActive() : null`);
check("the fight under test is the reader's own, not a pinned ruler", editable === true,
  `officialScenarioActive() says ${editable === true ? "no" : "YES — every field would read disabled"}`);

const open = async (path) => {
  await evaluate(`(async () => {
    const sleep = (ms) => new Promise(r => setTimeout(r, ms));
    history.pushState({}, '', ${JSON.stringify(path)}); route(); await sleep(2600);
  })()`);
  return evaluate(FIELDS);
};

// ---- a SENTINEL: none of the three is the reader's -----------------------
const sent = await open("/weapons/Verglas_Prime/simulator");
check("a sentinel's aim is pinned", sent.aiming.there && sent.aiming.disabled
  && sent.aiming.checked === true, JSON.stringify(sent.aiming));
check("...and its ammo is pinned ON — there is no reserve to run out of",
  sent.ammo.disabled && sent.ammo.checked === true, JSON.stringify(sent.ammo));
check("...and its headshot rate is pinned at 0",
  sent.head.disabled && Number(sent.head.value) === 0, JSON.stringify(sent.head));
// EACH NAMES WHAT PINNED IT. A dead control with no reason is the half of this
// bug that survives a fix to the other half.
check("...and each says why, in words", [sent.aiming, sent.ammo, sent.head]
  .every((f) => f.there && f.why && f.why.length >= 8),
  [sent.aiming, sent.ammo, sent.head]
    .map((f) => `"${String(f.why || (f.there ? "" : "ABSENT")).slice(0, 40)}"`).join(" | "));
check("...and the headshot reason is the SENTINEL one, not the generic blurb",
  /sentinel|companion|守护/.test(sent.head.why), `"${sent.head.why.slice(0, 90)}"`);

// ---- a ground ARCH-GUN: the third ammo state ----------------------------
// Unticked AND disabled — the setting stands in for PICKUPS, and this is the
// one weapon that can receive none. Its headshot rate is its own, though.
const ag = await open("/weapons/Larkspur_Prime/simulator");
check("a ground Arch-Gun's ammo is pinned OFF — pickups it cannot receive",
  ag.ammo.disabled && ag.ammo.checked === false, JSON.stringify(ag.ammo));
check("...but its head is a head", ag.head.there && ag.head.disabled === false,
  JSON.stringify(ag.head));

// ---- the NEGATIVE CONTROL: an ordinary weapon has all three back --------
const ord = await open("/weapons/Torid/simulator");
check("an ordinary weapon owns all three", ord.aiming.disabled === false
  && ord.ammo.disabled === false && ord.head.disabled === false,
  `aim=${ord.aiming.disabled} ammo=${ord.ammo.disabled} head=${ord.head.disabled}`);

// ---- THE PIN IS DISPLAY-ONLY, and the shared fight proves it ------------
// A scenario is shared across the roster, so a pin that WROTE its value would
// destroy the reader's own setting for every other weapon the moment a
// sentinel was opened — silently, through auto-save.
const kept = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  sim.headshot_pct = 40; markScenarioDirty(); await sleep(1600);
  const before = sim.headshot_pct;
  history.pushState({}, '', '/weapons/Verglas_Prime/simulator'); route(); await sleep(2600);
  const onSentinel = sim.headshot_pct;
  const shown = (document.querySelector('#sim-technique [data-k="headshot_pct"]') || {}).value;
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(2600);
  return { before, onSentinel, shown, after: sim.headshot_pct };
})()`);
check("the reader's own headshot rate survives a trip through a sentinel",
  kept.before === 40 && kept.after === 40,
  `set ${kept.before} -> on the sentinel ${kept.onSentinel} -> back ${kept.after}`);
check("...while the sentinel still SHOWED 0", Number(kept.shown) === 0, `showed ${kept.shown}`);

await app.finish("a question this weapon cannot be asked is pinned, and says why");
