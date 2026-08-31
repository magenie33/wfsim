// A RIVEN IS THE WEAPON FAMILY'S — the THIRTY-EIGHTH check.
//
// A riven mod belongs to a weapon FAMILY, not to one entry in it: *"Riven mods
// can be used on variants of a particular weapon, including MK1, Prime, Vandal,
// Wraith, Dex, Prisma, Mara, and Syndicate variants"*. Filed per WEAPON, a card
// built on the Burston is invisible on the Burston Prime and a player builds it
// twice — two cards for one riven, free to drift apart.
//
// THE NUMBERS FOLLOW BY THEMSELVES, which is why the fix is a storage scope
// rather than a feature: a saved riven holds ROLLS and the shown value is the
// roll against THIS weapon's disposition, so one card reads 1.45's worth on a
// Burston and 1.35's on its Prime. That RATIO is asserted here, because a list
// sharing a card and showing the wrong numbers is worse than one sharing
// nothing.
//
// THREE NEGATIVE CONTROLS, because "one big list" passes every positive above:
// an unrelated weapon does not see it; a KITGUN's two builds do not see each
// other's (one family, two cards, since a chamber built as a primary takes a
// RIFLE riven); and the MIGRATION keeps what is already on the machine, build
// references included, which is the only part that can lose a player's work.
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
  foldRivensIntoOneList();
  const read = (k) => {
    try { return JSON.parse(localStorage.getItem(k) || 'null'); } catch (e) { return null; }
  };
  out.merged = (read('wfsim-customs-rivens') || [])
    .filter((p) => (p.scope || '') === out.scopes.burston)
    .map((p) => ({ name: p.name, roll: ((p.state.bonuses || [])[0] || {}).roll }));
  out.oldKeysGone = ['wfsim-customs-burston-rivens', 'wfsim-customs-burston_prime-rivens']
    .filter((k) => localStorage.getItem(k) !== null);
  out.buildPoints = JSON.stringify(read('wfsim-presets-burston_prime-builder-builds'));

  // ---- 6. RENAME AND DELETE REACH EVERY BUILD IN THE FAMILY -------------
  // A riven's id IS its name, so both are the same operation seen from a build.
  // Touching the LIVE build only is widened across weapons by filing rivens by
  // family: rename a card on the Burston and the Burston Prime's saved builds
  // lose it silently. Driven through the page's own buttons, so the
  // wiring is under test and not just the helper.
  localStorage.clear();
  await go('/weapons/Burston/rivens');
  await mk();
  out.rn = activeRivenName();
  const equipped = () => JSON.stringify([{ name: 'b1', state: { slots: [
    { mod: 'riven:' + out.rn, rank: 3, pol: null }] } }]);
  localStorage.setItem('wfsim-presets-burston_prime-builder-builds', equipped());
  // THE NEGATIVE CONTROL: another FAMILY whose own card happens to carry the
  // same name. A sweep over every weapon would rewrite this one too.
  localStorage.setItem('wfsim-presets-braton-builder-builds', equipped());

  const renameTo = async (want) => {
    const b = document.querySelector('.cu-ren');
    if (!b) return false;
    b.click(); await sleep(350);
    const inp = document.querySelector('.cu-name');
    if (!inp) return false;
    inp.value = want;
    inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await sleep(1000);
    return true;
  };
  out.renamed = await renameTo(out.rn + ' x');
  out.nameAfter = activeRivenName();
  out.primeAfterRename = localStorage.getItem('wfsim-presets-burston_prime-builder-builds');
  out.bratonAfterRename = localStorage.getItem('wfsim-presets-braton-builder-builds');

  const del = document.querySelector('.cu-del');
  if (del) { del.click(); await sleep(1000); }
  out.primeAfterDelete = localStorage.getItem('wfsim-presets-burston_prime-builder-builds');

  // ---- 7. AN EDIT CHANGES THE CARD; IT DOES NOT DROP IT ----------------
  // The opposite of a delete, and the distinction is the whole point of a
  // riven being a REFERENCE. Editing one is the game's own reroll: the same
  // card, new numbers, everywhere it is equipped. Asked for as "deleted OR
  // CHANGED should remove it from the build" and declined
  // with this measurement, because removal here would drop a slot every time
  // somebody nudged the rank slider. Pinned so nobody 'fixes' it later.
  localStorage.clear();
  await go('/weapons/Burston/rivens');
  await mk();
  await pick('0', 'damage');
  const nm = activeRivenName();
  await go('/weapons/Burston_Prime');
  slots[0].mod = 'riven:' + nm;
  renderMods(); refreshPanel(); markPresetDirty(); await sleep(1400);
  const was = modById('riven:' + nm);
  out.editBefore = { drain: was && was.drain, eff: ((was || {}).effects || []).join(' ') };
  await go('/weapons/Burston/rivens');
  const openEl = document.querySelector('#riven-all [data-open="' + nm + '"]');
  if (openEl) { openEl.click(); await sleep(1600); }
  const rk = document.getElementById('rv-rank');
  out.rankFound = !!rk;
  if (rk) { rk.value = '0'; rk.dispatchEvent(new Event('input', { bubbles: true })); await sleep(1800); }
  await go('/weapons/Burston_Prime');
  const now = modById('riven:' + nm);
  out.editAfter = { drain: now && now.drain, eff: ((now || {}).effects || []).join(' ') };
  out.stillEquipped = ((slots[0] || {}).mod === 'riven:' + nm);
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

// ---- a rename and a delete reach every build that names the card -----------

check("the riven was renamed through the page's own control",
  r.renamed === true && r.nameAfter === r.rn + " x",
  `${r.rn} -> ${r.nameAfter}`);
// THE ONE THE OWNER'S QUESTION FOUND. Both operations touched the LIVE build
// only — already narrow for a weapon's other saved builds, and filing by family
// widened it across weapons.
check("...and a SAVED build on the other variant followed it",
  new RegExp("riven:" + r.rn + " x").test(r.primeAfterRename || ""),
  r.primeAfterRename);
check("...while another family's build with the same name did NOT move",
  new RegExp("riven:" + r.rn + "\"").test(r.bratonAfterRename || ""),
  r.bratonAfterRename);
check("deleting it clears that saved build's slot rather than orphaning it",
  !/riven:/.test(r.primeAfterDelete || "")
    && /"mod":null/.test(r.primeAfterDelete || ""),
  r.primeAfterDelete);

// ---- …but an EDIT is not a delete -------------------------------------------

check("the rank control was found, so an edit really happened",
  r.rankFound === true);
check(`editing the card on one variant changes it in the other's build `
  + `(${r.editBefore && r.editBefore.drain} -> ${r.editAfter && r.editAfter.drain} drain)`,
  !!r.editBefore && !!r.editAfter
    && r.editBefore.drain !== r.editAfter.drain
    && r.editBefore.eff !== r.editAfter.eff,
  JSON.stringify({ before: r.editBefore, after: r.editAfter }));
// AND IT IS STILL THERE. Editing a riven is the game's own reroll — the same
// card with new numbers — so a build keeps it. Dropping it on every edit would
// take a slot off the build each time somebody moved the rank slider, which is
// why "deleted or changed should remove it" was answered with only the first
// half.
check("...and the build still has it, because an edit is not a delete",
  r.stillEquipped === true);

await finish("a riven is the weapon family's");
