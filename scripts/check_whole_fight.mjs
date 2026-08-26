// THE FORTIETH CHECK: **A FIGHT IS ONE DOCUMENT, AND A READER CAN SEE ALL OF IT.**
//
// The scenario blocks show what APPLIES to the weapon in front of you, which is
// the right default and used to leave two things invisible:
//
//   * a field this weapon FORCES — a sentinel's headshot rate is 0 whatever the
//     document carries, and the document carries whatever you set for the rest
//     of the roster. Two different numbers, one of them the one that runs, and
//     nothing on screen said so.
//   * a field it merely does not USE, which is still part of the fight and
//     still travels. That is the buff map's own rule (AGENTS.md): the whole map
//     travels because it is the FIGHT's, and pruning it to what the current
//     build can grant made the quick calc a different fight the moment a
//     candidate granted a buff the current build lacked.
//
// So "the same fight across two weapons" was a claim nobody could check. This
// asserts that it can be.
//
// AND THAT THE RULE IS THE ENGINE'S. `engine::scenario::forced_for` decides and
// `/api/meta` states the consequence per weapon; the page reads it. The three
// forcing rules used to be re-derived in `app.js` from weapon flags — two
// implementations of one rule, drifting in silence, because a forced field
// looks identical whoever forced it (owner, 2026-08-27).
//
//   node scripts/check_whole_fight.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep } = app;

/// Open the panel on a weapon and read every row out of it.
const read = async (weapon) => {
  await app.load("/weapons/" + weapon + "/simulator", 12000);
  return evaluate(`(async () => {
    const nap = (ms) => new Promise(r => setTimeout(r, ms));
    await nap(1200);
    const d = document.getElementById('sim-whole-fight');
    const wasOpen = d ? d.open : null;
    if (d) d.open = true;
    await nap(500);
    const rows = [...document.querySelectorAll('#sim-whole-fight-body .wf-row')];
    return {
      present: !!d,
      foldedByDefault: wasOpen === false,
      groups: [...document.querySelectorAll('#sim-whole-fight-body .wf-g-h')].map(e => e.textContent.trim()),
      ids: rows.map(r => r.querySelector('code').textContent),
      forced: rows.filter(r => r.classList.contains('forced')).map(r => ({
        id: r.querySelector('code').textContent,
        val: r.querySelector('.wf-val').textContent.trim(),
        why: (r.querySelector('.wf-why') || {}).textContent || '',
      })),
      axesServed: ((META && META.scenario_axes) || []).length,
      // The ENGINE's answer for this weapon, straight off meta — so the
      // assertion below compares the screen to the served rule rather than to
      // a rule this check re-states.
      served: ((META.weapons || []).find(x => x.id === document.getElementById('weapon').value) || {}).forced || {},
    };
  })()`);
};

// ---- 1. IT IS THERE, AND IT IS SHUT ---------------------------------------
//
// Folded by default because the ordinary reader is not asked to care: this is
// the escape hatch, not a second settings panel.
const laetum = await read("Laetum");
check("the whole fight is on the page", laetum.present === true);
check("...folded shut by default", laetum.foldedByDefault === true,
  `open=${!laetum.foldedByDefault}`);
check("...and the engine declares what a fight consists of",
  laetum.axesServed >= 25, `${laetum.axesServed} axes served`);
check("...grouped rather than handed over as one flat list",
  laetum.groups.length === 4, JSON.stringify(laetum.groups));

// ---- 2. THE DOCUMENT KEEPS EVERYTHING -------------------------------------
//
// The sharp one. A companion weapon hides and forces more than a rifle does,
// and the panel must still list the SAME fields — that is what "one document"
// means, and it is the assertion that fails the day somebody prunes the fight
// to the weapon in front of them.
const artax = await read("Artax");
check("a companion weapon carries the same fields as a rifle",
  JSON.stringify(artax.ids) === JSON.stringify(laetum.ids),
  `${artax.ids.length} vs ${laetum.ids.length}`);
check("...and there are enough of them to be the whole fight",
  laetum.ids.length >= 25, `${laetum.ids.length} rows`);

// ---- 3. WHAT IS FORCED, AND WHY -------------------------------------------
//
// An ordinary rifle forces nothing: the negative control, and the reason the
// assertion under it means something. A panel that marked rows forced on every
// weapon would pass a presence check just as well.
check("an ordinary rifle forces nothing at all",
  laetum.forced.length === 0, JSON.stringify(laetum.forced).slice(0, 160));
check("a companion weapon forces three fields",
  artax.forced.length === 3, artax.forced.map(f => f.id).join(", "));
// THE PAGE'S ANSWER IS THE ENGINE'S. Compared against what meta served for this
// weapon, so the check cannot pass by re-deriving the rule the page dropped.
check("...and they are exactly the ones the ENGINE says",
  JSON.stringify(artax.forced.map(f => f.id).sort())
    === JSON.stringify(Object.keys(artax.served).sort()),
  `screen ${artax.forced.map(f => f.id).sort()} vs served ${Object.keys(artax.served).sort()}`);

// THE GAP IS THE POINT. `headshot_pct` is 0 on a sentinel and the document
// keeps 100 for the rest of the roster — two numbers, one of which runs. Before
// this panel a reader could type 100 on a Verglas, watch the page accept it,
// and get a run computed at 0 with nothing saying why.
const head = artax.forced.find(f => f.id === "headshot_pct");
check("a forced row shows BOTH numbers — what runs and what the document holds",
  !!head && /\b0\b/.test(head.val) && /100/.test(head.val), head ? head.val : "(no row)");
check("...and says why in a sentence rather than a word",
  !!head && head.why.length > 25 && head.why.includes(" "),
  head ? JSON.stringify(head.why).slice(0, 140) : "(none)");

// ---- 4. THE THREE-STATE AMMO BOX, WHICH IS WHY THE RULE IS DATA ------------
//
// One flag read as the wrong one of two facts left the only adjustable weapon
// being the one weapon the game gives no way to adjust (2026-08-04). The two
// forced states are OPPOSITE values from OPPOSITE facts, so a weapon of each is
// the only way to hold it.
const ammoOn = artax.forced.find(f => f.id === "infinite_ammo");
check("a weapon with no reserve forces infinite ammo ON",
  !!ammoOn && /\b1\b|true/.test(ammoOn.val), ammoOn ? ammoOn.val : "(none)");
const larkspur = await read("Larkspur");
const ammoOff = larkspur.forced.find(f => f.id === "infinite_ammo");
check("...and an Arch-Gun, which HAS one it cannot refill, forces it OFF",
  !!ammoOff && /\b0\b|false/.test(ammoOff.val), ammoOff ? ammoOff.val : "(none)");
check("...which is the opposite value from the opposite fact",
  !!ammoOn && !!ammoOff && ammoOn.val !== ammoOff.val,
  `${ammoOn && ammoOn.val} vs ${ammoOff && ammoOff.val}`);

await app.finish("a fight is one document, and a reader can see all of it");
