// A RIVEN IS THE WEAPON FAMILY'S — the THIRTY-EIGHTH check.
//
// A riven mod belongs to a weapon FAMILY, not to one entry in it: *"Riven mods
// can be used on variants of a particular weapon, including MK1, Prime,
// Vandal, Wraith, Dex, Prisma, Mara, and Syndicate variants"* (wiki `Riven
// Mods`). The app filed them per WEAPON, so a card built on the Burston was
// invisible on the Burston Prime and a player had to build it twice — two
// cards for one riven, free to drift apart (owner, 2026-08-25).
//
// THE NUMBERS FOLLOW BY THEMSELVES, which is why the fix is a storage scope
// rather than a feature: a saved riven holds ROLLS, and the shown value is the
// roll against THIS weapon's disposition. So the same card reads 1.45's worth
// on a Burston and 1.35's on its Prime, which is what the game does — *"the
// cycling screen allows players to view the Riven stats on every owned variant
// of said weapon"*. That RATIO is asserted here, because a list that shares a
// card and shows the wrong numbers for it is worse than one that shares
// nothing.
//
// THREE NEGATIVE CONTROLS, because "one big list" passes every positive above:
//   * an unrelated weapon does not see it;
//   * a KITGUN's two builds do not see each other's — one family, two cards (a
//     chamber built as a primary takes a RIFLE riven, as a secondary a PISTOL
//     one), which an engine test holds from the data side;
//   * and the MIGRATION keeps what is already on the machine, build references
//     included, which is the only part of this that can lose a player's work.
//
//   node scripts/check_riven_family.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  localStorage.clear();

  const go = async (path) => { history.pushState({}, '', path); route(); await sleep(2400); };
  const mk = async () => {
    // A weapon with no riven has nothing to edit and the editor stands down —
    // customs are optional by nature. Make one by clicking the page's button.
    if (!riven) { document.querySelector('#riven-tools .cu-new').click(); await sleep(900); }
  };
  const pick = async (slot, stat) => {
    const anchor = document.querySelector('#riven-stats .rv-pick[data-slot="' + slot + '"]');
    if (!anchor) return false;
    openRivenPicker(anchor, slot); await sleep(250);
    const el = document.querySelector('#riven-menu [data-rvid="' + stat + '"]');
    if (!el) { closePopovers(); return false; }
    el.click(); await sleep(1200); closePopovers();
    return true;
  };
  const firstStat = () => {
    const s = ((rivenResolved || {}).stats || [])[0];
    return s ? { slot: s.slot, roll: s.roll, value: s.value, shown: s.shown } : null;
  };
  const openFirst = async (name) => {
    const el = document.querySelector('#riven-all [data-open="' + name + '"]');
    if (!el) return false;
    el.click(); await sleep(1600);
    return true;
  };
  const backToList = async () => {
    const b = document.querySelector('.cu-back');
    if (b) { b.click(); await sleep(900); }
  };
  const listed = () => [...document.querySelectorAll('#riven-all [data-open]')]
    .map((e) => e.dataset.open);

  // ---- 1. MADE ON THE BURSTON --------------------------------------------
  await go('/weapons/Burston/rivens');
  await mk();
  out.picked = await pick('0', 'damage');
  out.name = activeRivenName();
  out.burstonStat = firstStat();
  out.burstonDisposition = weaponInfo('burston').disposition;
  out.scopes = {
    burston: rivenScope('burston'),
    burston_prime: rivenScope('burston_prime'),
    braton: rivenScope('braton'),
  };
  await backToList();
  out.burstonList = listed();

  // ---- 2. IS ON THE BURSTON PRIME, AT THE PRIME'S DISPOSITION ------------
  await go('/weapons/Burston_Prime/rivens');
  await backToList();
  out.primeList = listed();
  out.primeOpened = await openFirst(out.name);
  out.primeStat = firstStat();
  out.primeDisposition = weaponInfo('burston_prime').disposition;
  out.primeSub = (document.getElementById('riven-sub') || {}).textContent || '';
  out.burstonName = weaponInfo('burston').name;

  // ---- 3. AN UNRELATED WEAPON DOES NOT SEE IT ---------------------------
  await go('/weapons/Braton/rivens');
  await backToList();
  out.bratonList = listed();

  // ---- 4. A KITGUN'S TWO BUILDS ARE TWO CARDS ---------------------------
  const kit = (META.weapons || []).filter((w) => w.riven_family === 'Tombfinger');
  out.kitIds = kit.map((w) => w.id);
  out.kitScopes = kit.map((w) => rivenScope(w.id));
  out.kitClasses = kit.map((w) => w.riven_class || w.mod_class);

  // ---- 5. THE MIGRATION KEEPS WHAT IS ALREADY THERE ---------------------
  // Two variants of one family, each with a card called 'riven 1' — the
  // collision that cannot simply be renamed, because the Prime's BUILD is
  // pointing at that name and would silently equip the other one's card.
  localStorage.clear();
  const card = (which) => ({ name: 'riven 1', state: {
    shape: '2', rank: 8, polarity: 'madurai',
    bonuses: [{ id: 'damage', roll: which === 'a' ? 0.9 : 0.4 }], malus: null } });
  localStorage.setItem('wfsim-customs-burston-rivens', JSON.stringify([card('a')]));
  localStorage.setItem('wfsim-customs-burston_prime-rivens', JSON.stringify([card('b')]));
  localStorage.setItem('wfsim-presets-burston_prime-builder-builds', JSON.stringify(
    [{ name: 'b1', state: { slots: [{ mod: 'riven:riven 1', pol: null }] } }]));
  mergeRivenFamilyLists();
  const read = (k) => {
    try { return JSON.parse(localStorage.getItem(k) || 'null'); } catch (e) { return null; }
  };
  out.merged = (read('wfsim-customs-' + out.scopes.burston + '-rivens') || [])
    .map((p) => ({ name: p.name, roll: ((p.state.bonuses || [])[0] || {}).roll }));
  out.oldKeysGone = ['wfsim-customs-burston-rivens', 'wfsim-customs-burston_prime-rivens']
    .filter((k) => localStorage.getItem(k) !== null);
  out.buildPoints = JSON.stringify(read('wfsim-presets-burston_prime-builder-builds'));
  return out;
})()`);

// ---- the card is the family's -----------------------------------------------

check("a riven was made on the Burston", r.picked === true && !!r.name,
  JSON.stringify({ picked: r.picked, name: r.name }));
check("the Burston and its Prime file rivens under ONE scope",
  r.scopes.burston === r.scopes.burston_prime && !!r.scopes.burston,
  JSON.stringify(r.scopes));
check("...and it is in the Burston Prime's list without being made again",
  (r.primeList || []).includes(r.name),
  `${JSON.stringify(r.primeList)} vs made ${JSON.stringify(r.burstonList)}`);
check("...and it opens there", r.primeOpened === true);

// THE NUMBERS ARE THE VARIANT'S OWN. Same roll, two dispositions, and the shown
// value has to move by exactly their ratio — the whole reason this is a storage
// change and not a conversion. A list that shares a card and shows the maker's
// numbers on the other weapon passes every assertion above and is wrong.
const a = r.burstonStat || {};
const b = r.primeStat || {};
const va = a.value ?? a.shown;
const vb = b.value ?? b.shown;
check("the two variants carry DIFFERENT dispositions, so the ratio means something",
  r.burstonDisposition !== r.primeDisposition,
  `${r.burstonDisposition} vs ${r.primeDisposition}`);
check("...the same roll travelled", Math.abs((a.roll ?? 0) - (b.roll ?? -1)) < 1e-9,
  `${a.roll} vs ${b.roll}`);
check("...and the value follows the Prime's disposition "
  + `(${va} -> ${vb}, expected x${(r.primeDisposition / r.burstonDisposition).toFixed(4)})`,
  Number.isFinite(va) && Number.isFinite(vb) && va > 0
    && Math.abs((vb / va) - (r.primeDisposition / r.burstonDisposition)) < 0.02,
  `${va} -> ${vb}`);

// SILENT SHARING READS AS A BUG. A riven the player never made on this weapon
// turning up in its list needs a sentence, and the sentence has to NAME the
// other variant — "also fits 1 other" is the one thing a reader cannot act on.
// It names it in the reader's language, by borrowing the localized name of the
// family's base member: the family string itself is DE's module field and is
// always English, so a Chinese page said "Burston" beside 伯斯顿 Prime.
check("...and the page SAYS whose card it is, naming the other variant",
  (r.primeSub || "").includes(r.burstonName)
    && r.primeSub.length > (r.burstonName || "").length + 12,
  r.primeSub);

// ---- and only the family's --------------------------------------------------

check("an unrelated weapon does not see it",
  !(r.bratonList || []).includes(r.name) && r.scopes.braton !== r.scopes.burston,
  `${JSON.stringify(r.bratonList)} under ${r.scopes.braton}`);
// A KITGUN IS THE SHARP ONE: one family, two riven classes, so scoping by
// family alone would put a rifle riven in a pistol's list — offered by the
// editor and refused by the board.
check("a Kitgun's primary and secondary are two cards, not one",
  (r.kitIds || []).length === 2 && r.kitScopes[0] !== r.kitScopes[1]
    && r.kitClasses[0] !== r.kitClasses[1],
  JSON.stringify({ ids: r.kitIds, scopes: r.kitScopes, classes: r.kitClasses }));

// ---- the migration loses nothing --------------------------------------------

check("both variants' existing cards survive the move",
  (r.merged || []).length === 2, JSON.stringify(r.merged));
check("...the colliding name was renamed rather than overwritten",
  new Set((r.merged || []).map((x) => x.name)).size === 2
    && new Set((r.merged || []).map((x) => x.roll)).size === 2,
  JSON.stringify(r.merged));
check("...the old per-weapon keys are gone", (r.oldKeysGone || []).length === 0,
  JSON.stringify(r.oldKeysGone));
// THE ONE THAT CAN LOSE WORK SILENTLY: a build equipping the renamed card must
// follow it. Left alone it keeps the old name, which the OTHER variant's card
// now owns — so the build comes back equipping a riven the player never put on
// it, with nothing on screen saying so.
check("...and the build that equipped the renamed card follows it",
  /riven:riven 1 \(burston_prime\)/.test(r.buildPoints || ""),
  r.buildPoints);

await finish("a riven is the weapon family's");
