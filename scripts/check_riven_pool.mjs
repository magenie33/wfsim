// A WEAPON'S RIVEN EDITOR OFFERS THE STATS ITS RIVENS ACTUALLY ROLL.
//
// What a riven can roll is DE's own per-weapon table (MEASUREMENTS M35): it is
// published nowhere, the wiki's 25%-of-a-physical-type rule disclaims itself
// ("exceptions exist on a case by case basis"), and a count over ~12 000 live
// cards found the derivation wrong in BOTH directions on six of 26 families.
// So the engine derives the pool from the weapon and lets
// `data/rivens/exceptions.yaml` override it per family (the survey is a test's
// business, not the calculation's) — and this asserts
// the PICKER acts on that, because the failure a player sees is a stat their
// real riven carries not being in the list.
//
// It opens the popover rather than reading `rivenPool()`, and it walks BOTH
// SLOTS. The bonus list and the malus list are drawn from one pool through two
// different filters (five stats are bonus-only), so "offered" is two claims:
// the report that prompted this was about the NEGATIVE slot specifically.
//
//   node scripts/check_riven_pool.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

// weapon -> [stat, must it be offered, why]
const CASES = [
  // A REAL CARD beats everything. The Furis is hit-scan in both forms, so the
  // rules refuse flight speed, and the survey's 13-of-500 is inside the band
  // where a count proves nothing.
  ["Furis", "projectile_speed", true, "a player's card carries it"],
  ["MK1-Furis", "projectile_speed", true, "same riven, same family"],
  // THE SURVEY, adding what the rules struck out: 9% Puncture, 91% Radiation,
  // and all three physical stats roll on real cards.
  ["Ocucor", "impact", true, "49 of 500 cards"],
  ["Ocucor", "slash", true, "39 of 500 cards"],
  // ...and taking away what they allowed.
  ["Phenmor", "puncture", false, "30% of its damage, 0 of 500 cards"],
  ["Boar", "zoom", false, "no scope, and no field in the data says so"],
  ["Phantasma Prime", "projectile_speed", false, "the bomb flies, 0 of 500 cards"],
  // THE INCARNON QUESTION, settled by counting rather than by argument: these
  // three forms fire a literal travelling projectile and their families show
  // 0, 4 and 0 flight-speed listings out of 500.
  ["Latron", "projectile_speed", false, "0 of 500 cards despite the Incarnon form"],
  ["Lex Prime", "projectile_speed", false, "4 of 500 cards"],
  ["Atomos", "projectile_speed", false, "0 of 500 cards"],
];

const r = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  const out = [];
  for (const [wiki, stat] of ${JSON.stringify(CASES.map(([w, s]) => [w, s]))}) {
    history.pushState({}, '', '/weapons/' + wiki.replace(/ /g, '_') + '/rivens');
    route(); await sleep(2200);
    // A weapon with no riven yet has nothing to pick INTO, and the editor
    // stands down rather than showing a document that is not there — customs
    // are optional by nature. So make one by CLICKING the page's own button.
    if (!riven) {
      document.querySelector('#riven-tools .cu-new').click();
      await sleep(800);
    }
    const slots = {};
    for (const slot of ['0', 'malus']) {
      // The malus SLOT has to exist before it can be filled. The default shape
      // is 3+1, so it does — and if that ever changes, click the shape that
      // has one rather than reaching past the UI for it.
      if (slot === 'malus' && !riven.malus) {
        document.querySelector('#riven-shape [data-rv="2+1"]').click();
        await sleep(400);
      }
      const anchor = document.querySelector('#riven-stats .rv-pick[data-slot="' + slot + '"]');
      if (!anchor) { slots[slot] = null; continue; }
      openRivenPicker(anchor, slot);
      await sleep(150);
      // What is ON SCREEN, not what the pool function returns.
      slots[slot] = [...document.querySelectorAll('#riven-menu [data-rvid]')]
        .map(el => el.dataset.rvid);
      closePopovers();
    }
    out.push({ wiki, stat, weapon: $('weapon').value, slots });
  }
  return out;
})()`);

for (const [i, [wiki, stat, want, why]] of CASES.entries()) {
  const got = r[i];
  check(`${wiki}: the editor drew both slots`,
    Array.isArray(got.slots["0"]) && Array.isArray(got.slots.malus),
    JSON.stringify(Object.keys(got.slots)));
  const bonus = got.slots["0"].includes(stat);
  const malus = got.slots.malus.includes(stat);
  check(`${wiki}: ${stat} is ${want ? "offered" : "refused"} as a BONUS (${why})`,
    bonus === want, `${got.slots["0"].length} stats offered`);
  check(`${wiki}: ${stat} is ${want ? "offered" : "refused"} as the MALUS (${why})`,
    malus === want, `${got.slots.malus.length} stats offered`);
}

// A pool that narrowed to nothing would pass every "is refused" line above.
for (const [i, [wiki]] of CASES.entries()) {
  check(`${wiki}: the pool is still a pool`, r[i].slots["0"].length >= 15,
    `${r[i].slots["0"].length} stats`);
}

// ---------------------------------------------------------------------------
// THE LIST SHOWS A CARD'S NUMBERS THE MOMENT THE CARD EXISTS.
//
// Its values are the ENGINE's — a roll against this weapon's disposition, asked
// for over `/api/riven` — and they were keyed by NAME and fetched only at boot.
// So a card that had just been copied, or one whose name had just changed, had
// no entry: the row drew its shape and its capacity and no stats at all, until
// something else happened to refresh. Dragging a value was doing that
// invisibly, which is why it looked like the numbers needed a nudge.
//
// Copying is the case worth seeding rather than fetching: an identical spec
// cannot answer differently, so the copy shows the original's numbers with no
// round trip at all.
const shown = await evaluate(`(async () => {
  const s = ms => new Promise(r => setTimeout(r, ms));
  const rows = () => [...document.querySelectorAll('#riven-all .rv-all')]
    .map(el => el.textContent.replace(/\\s+/g, ' ').trim());
  const press = async (sel) => { const b = document.querySelector(sel); if (b) b.click(); await s(1800); };
  localStorage.clear();
  // A card with stats, so the list has numbers it can get wrong.
  localStorage.setItem('wfsim-customs-rivens', JSON.stringify([{
    id: 'seed1', name: 'seeded', scope: rivenScope('burston_prime'),
    state: { shape: '2', rank: 8, polarity: 'madurai',
      bonuses: [{ id: 'damage', roll: 0.9 }], malus: null } }]));
  history.pushState({}, '', '/weapons/Burston_Prime/rivens'); route(); await s(5000);
  const out = { atStart: rows() };
  // OPENED THE WAY A READER OPENS IT — clicking the row, which is also the
  // path that has to keep working now that a row is addressed by identity.
  await press('#riven-all [data-open="seed1"]');
  out.opened = activeRivenId();
  await press('.cu-dup');
  out.afterCopy = rows();
  document.querySelector('.cu-ren').click(); await s(400);
  const inp = document.querySelector('.cu-name');
  inp.value = 'a new label';
  inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  await s(1200);
  out.afterRename = rows();
  return out;
})()`);

const hasStats = (rows) => rows.length > 0 && rows.every((t) => /%/.test(t));
check("a saved riven's row shows its numbers", hasStats(shown.atStart),
  JSON.stringify(shown.atStart));
check("...and clicking it opens that card", shown.opened === "seed1", shown.opened);
check("...a COPY shows them the moment it appears, with no round trip",
  shown.afterCopy.length === 2 && hasStats(shown.afterCopy),
  JSON.stringify(shown.afterCopy));
// A RENAME CANNOT LOSE THEM, because the values are keyed by the card and a
// name is not the card.
check("...and a RENAME keeps them", shown.afterRename.length === 2
  && hasStats(shown.afterRename)
  && shown.afterRename.some((t) => t.includes("a new label")),
  JSON.stringify(shown.afterRename));

await app.finish("a riven editor offers the stats that weapon's rivens roll");
