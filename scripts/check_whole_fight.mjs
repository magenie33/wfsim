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
// AND THAT THE RULE IS THE ENGINE'S. `engine::scenario::settled_for` decides and
// `/api/meta` states the consequence per weapon; the page reads it. The three
// forcing rules used to be re-derived in `app.js` from weapon flags — two
// implementations of one rule, drifting in silence, because a forced field
// looks identical whoever forced it.
//
// AND WHAT THE FIGHT ITSELF RULES, per weapon class (§5-6). A scenario carries
// the rules for classes it is not pointed at, which is what makes it one
// document any weapon can be measured against — and OVERRIDES SIT BEHIND
// LEGALITY, so the check's sharpest assertion is a NEGATIVE one: a Companion
// settles three fields and may be ruled on for none of them, because all three
// are the game's rule rather than the sim's simplification. An editor that drew
// a box beside every settled row would pass everything else here.
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
      // The house-rules editor, and what it is offering.
      house: [...document.querySelectorAll('#sim-whole-fight-body .hr-col')].map(c => ({
        head: c.querySelector('.hr-h').textContent.trim(),
        here: c.classList.contains('here'),
        offers: [...c.querySelectorAll('[data-cr]')].map(i => i.dataset.cr),
      })),
      overridable: ((META.class_rules || {}).overridable || []).map(p => p.join('|')),
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
      served: ((META.weapons || []).find(x => x.id === document.getElementById('weapon').value) || {}).settled || {},
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
// being the one weapon the game gives no way to adjust. The two
// forced states are OPPOSITE values from OPPOSITE facts, so a weapon of each is
// the only way to hold it.
// THE VALUE THAT RUNS IS THE FIRST THING IN THE CELL, and these anchor on it.
// A settled row prints BOTH numbers — what runs, then what the document holds —
// so a loose `/true/` finds the "true" in "document says true" and passes while
// the row reports `false`. It did, for one revision.
const runs = (r) => (r ? String(r.val).split(/\s+/)[0] : "(none)");
const ammoOn = artax.forced.find(f => f.id === "infinite_ammo");
check("a weapon with no reserve forces infinite ammo ON", runs(ammoOn) === "true", runs(ammoOn));
const larkspur = await read("Larkspur");
const ammoOff = larkspur.forced.find(f => f.id === "infinite_ammo");
check("...and an Arch-Gun, which HAS one it cannot refill, forces it OFF",
  runs(ammoOff) === "false", runs(ammoOff));
check("...which is the opposite value from the opposite fact",
  runs(ammoOn) !== runs(ammoOff), `${runs(ammoOn)} vs ${runs(ammoOff)}`);

// ---- 5. THE HOUSE RULES: WHAT A FIGHT MAY SAY, AND ABOUT WHOM -------------
//
// A scenario is one document that ANY weapon can be measured against, so it
// carries rules for the classes it is not pointed at — which is what makes
// "in my fight, Arch-Guns have infinite ammo" something you write on a
// Burston's page.
check("every weapon class gets a column, on a weapon in none of the others",
  laetum.house.length === 4, JSON.stringify(laetum.house.map(h => h.head)));
check("...and the one you are standing on is marked",
  laetum.house.filter(h => h.here).length === 1,
  JSON.stringify(laetum.house.map(h => [h.head, h.here])));

// THE GUARD, AND IT IS THE WHOLE FEATURE. A control exists exactly where the
// ENGINE says a fight may argue — which is where the sim's own simplification
// is being argued with, never the game's rule. So the Arch-Gun column offers
// its ammo and the Companion column offers NOTHING: a sentinel's three settled
// fields are all game facts, and a page that drew a box for them would let a
// reader publish a number no one can reproduce in game.
const offered = laetum.house.flatMap(h => h.offers).sort();
check("a control exists exactly where the engine says a fight may argue",
  JSON.stringify(offered) === JSON.stringify([...laetum.overridable].sort()),
  `screen ${JSON.stringify(offered)} vs served ${JSON.stringify(laetum.overridable)}`);
check("...and the engine says exactly one thing today: the Arch-Gun's ammo",
  JSON.stringify(laetum.overridable) === JSON.stringify(["archgun|infinite_ammo"]),
  JSON.stringify(laetum.overridable));
// THE NEGATIVE CONTROL, and it is the sharp one: a companion settles THREE
// fields and may be ruled on for none of them. An editor that simply drew a box
// beside every settled row would pass every assertion above.
const companionCol = laetum.house.find(h => /Companion|守护/.test(h.head));
check("a companion's settled fields are the GAME's, so none is offered",
  !!companionCol && companionCol.offers.length === 0,
  companionCol ? JSON.stringify(companionCol.offers) : "(no column)");

// ---- 6. …AND A RULE REACHES THE ENGINE ------------------------------------
//
// The half that cannot be faked. An editor that stored a rule the wire never
// carried — or that the server read and did not apply — would look exactly like
// a working one. So: tick the Arch-Gun rule ON A LAETUM, open the Larkspur, and
// assert its ammo box has changed its mind. Same document, different weapon,
// which is the entire claim.
//
// A SCENARIO OF ITS OWN FIRST, and the reason is the negative control below: a
// first-time visitor lands on an OFFICIAL RULER, which is PINNED and whose
// edits are never written down. A house rule is part of the fight, so a ruler
// must refuse one exactly as it refuses a dragged enemy — and this assertion
// would otherwise have been running against a fight that discards everything
// written to it, which is a check that can only ever report the last thing it
// typed. (`check_arena.mjs` learned the same lesson on 2026-08-16.)
const ruled = await evaluate(`(async () => {
  const nap = (ms) => new Promise(r => setTimeout(r, ms));
  const grab = () =>
    document.querySelector('#sim-whole-fight-body [data-cr="archgun|infinite_ammo"]');
  const onRuler = grab();
  const refusedByRuler = !!onRuler && onRuler.disabled
    && typeof officialScenarioActive === 'function' && officialScenarioActive();

  const bar = document.querySelector('#preset-bar-simulator-scenarios');
  const add = bar && bar.querySelector('.pchip.add');
  if (add) { add.click(); await nap(1800); }
  const d = document.getElementById('sim-whole-fight');
  if (d) d.open = true;
  await nap(400);

  const box = grab();
  if (!box) return { refusedByRuler, error: 'no control' };
  box.checked = true;
  box.dispatchEvent(new Event('change', { bubbles: true }));
  // Past the scenario auto-save debounce, so the rule is on DISK before the
  // next weapon is opened — which is what the reload below actually reads.
  await nap(1200);
  return {
    refusedByRuler,
    editable: typeof officialScenarioActive === 'function' && !officialScenarioActive(),
    stored: JSON.parse(JSON.stringify(sim.class_rules || null)),
    // ON THE WIRE, through the page's ONE spelling of the fight. A rule stored
    // in page state and dropped on the way out is the failure this catches.
    sent: (theFight().class_rules || null),
  };
})()`);
// THE NEGATIVE CONTROL. A ruler pins its fight, and a house rule is part of one
// — so the control is there (the document HAS the field, and the panel's whole
// job is to show it) and cannot be operated. Without this a reader could turn
// an official Arch-Gun board into a different measurement in one click.
check("an official ruler refuses a house rule, as it refuses every other edit",
  ruled.refusedByRuler === true, `disabled=${ruled.refusedByRuler}`);
check("...and a scenario of your own accepts one", ruled.editable === true);
check("ticking an Arch-Gun rule on a Laetum stores it against Arch-Guns",
  !!ruled.stored && ruled.stored.archgun && ruled.stored.archgun.infinite_ammo === true,
  JSON.stringify(ruled.stored));
check("...and it travels on the wire, in the fight every module sends",
  !!ruled.sent && ruled.sent.archgun && ruled.sent.archgun.infinite_ammo === true,
  JSON.stringify(ruled.sent));

const after = await read("Larkspur");
const ammoRuled = after.forced.find(f => f.id === "infinite_ammo");
check("...and the Arch-Gun, opened next, now runs with the reserve topped up",
  runs(ammoRuled) === "true", ammoRuled ? ammoRuled.val : "(none)");
check("...saying it was THIS FIGHT that ruled so, not the weapon",
  !!ammoRuled && /ruled|改写/.test(ammoRuled.val), ammoRuled ? ammoRuled.val : "(none)");
// AND THE MEASUREMENT MOVES, which is the only thing that proves the server
// read it. A finite 400-round reserve against an infinite one over a three
// minute engagement is not a subtle difference.
const moved = await evaluate(`(async () => {
  const one = async (rule) => {
    const f = theFight();
    if (rule) f.class_rules = { archgun: { infinite_ammo: true } };
    else delete f.class_rules;
    f.runs = 12;
    const r = await api('/api/simulate', { ...f, weapon: document.getElementById('weapon').value,
      mods: [], build_size: 0 });
    return r && r.dps;
  };
  return { off: await one(false), on: await one(true) };
})()`);
check("a class rule the server HONOURS moves the Arch-Gun's own number",
  moved.on > moved.off * 1.05,
  `off ${moved.off} vs on ${moved.on}`);

await app.finish("a fight is one document, and a reader can see all of it");
