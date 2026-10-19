// THE BUILD FINDER FINDS, THE BUILD BAR HOLDS, AND THE LINE SAYS WHAT IS OPEN.
//
// The finder is a query over the board's builds and never a selection: every
// row it lists must satisfy the query, every row opens into the same detail,
// the comparison is against a build the reader picked, and "Open" puts that
// build in the build bar as a read-only chip and makes it current. The chip is kept by WHAT THE
// BUILD IS, so a rescore that renumbers the board leaves it on the same build.
// Asserted here too: the three states of "what is open", and the scenario bar
// keeping its single control.
//
//   node scripts/check_build_finder.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  localStorage.clear();
  const box = () => document.getElementById('build-finder');
  const bar = () => document.getElementById('preset-bar-builder-builds');
  const listed = () => Array.from(box().querySelectorAll('tr.fr[data-frow]')).map((t) => t.dataset.frow);
  // EVERY BUILD LISTED, ONCE: each tie opened, and an opened tie's head left
  // out, since its first member is the same build.
  const everyBuild = async () => {
    finder.shown = 1e4; renderBuildFinder(); await sleep(100);
    for (;;) {
      const b = [...box().querySelectorAll('[data-ftie]')].find((x) => !finder.ties.has(x.dataset.ftie));
      if (!b) break;
      b.click(); await sleep(60);
    }
    return Array.from(box().querySelectorAll('tr.fr[data-frow]:not(.tie)')).map((t) => byId(t.dataset.frow));
  };
  const byId = (id) => builtinBuilds().find((p) => presetId(p) === id);
  const selChip = () => bar().querySelector('.pchip.sel');
  const roChips = () => Array.from(bar().querySelectorAll('.pchip.ro')).map((c) => c.dataset.name);
  const open = async (name) => {
    history.pushState({}, '', '/weapons/' + name); route(); await sleep(3500);
  };
  const out = {};

  await open('Ballistica_Prime');
  out.rows = ((typeof BOARD !== 'undefined' && BOARD) ? (BOARD['ballistica_prime'] || []) : []).length;
  out.drawn = !box().hidden && getComputedStyle(box()).display !== 'none';
  // LANDING ON A WEAPON OPENS AN UNSAVED BUILD, which no chip stands for.
  out.landingActive = activePreset;
  out.landingSel = !!selChip();

  // IT FOLDS LIKE EVERY BOX, AND WITH NO BUILD OF YOUR OWN HERE IT SHIPS
  // OPEN. The state is the reader's and outlives the weapon; a click in the
  // search box is the search box's. Asked of the computed layout, not the class.
  const seen = (el) => !!el && el.offsetParent !== null;
  const head = () => box().querySelector(':scope > .fd-head');
  out.foldDefault = !box().classList.contains('shut') && seen(box().querySelector('.fd-main'));
  out.foldUnstored = JSON.parse(localStorage.getItem('wfsim-folds') || '{}')['build-finder'] == null;
  head().querySelector('.fd-title').click(); await sleep(150);
  head().querySelector('.fd-title').click(); await sleep(150);
  out.foldOpened = !box().classList.contains('shut') && seen(box().querySelector('.fd-main'));
  out.foldStored = JSON.parse(localStorage.getItem('wfsim-folds') || '{}')['build-finder'] === false;
  renderBuildFinder(); await sleep(150);
  out.foldRedrawn = !box().classList.contains('shut') && !!head().querySelector(':scope > .fold-c');
  box().querySelector('.fd-q').click(); await sleep(150);
  out.searchKept = !box().classList.contains('shut');
  jump.open = true; renderJump(); await sleep(150);
  const jr = document.querySelector('.jump-row[data-jump="build-finder"]');
  out.jumpName = jr ? jr.querySelector('.jr-n').textContent.trim() : '';
  jump.open = false; renderJump();

  out.firstPage = listed().length;
  // THE SCOPE IS ONE CELL: every listed row shares the ruler and the mode the
  // scope segments say.
  const scoped = listed().map(byId);
  out.oneCell = scoped.length > 0 && scoped.every((p) => p && p.benchmark === finder.b && p.mode === finder.mo);
  // A ROW IS THE SIMULATOR'S BUILD CARD, holding every card the build carries.
  const row0 = box().querySelector('tr.fr');
  const b0 = byId(row0.dataset.frow).board;
  const ex0 = b0.exilus && b0.exilus !== 'none' && !(b0.mods || []).includes(b0.exilus) ? [b0.exilus] : [];
  const saved = [...(b0.mods || []).filter(Boolean), ...ex0];
  const secs = [...row0.querySelectorAll('.fd-card .sb-h')];
  const chips = secs.length ? [...secs[0].nextElementSibling.querySelectorAll('.sb-chip')] : [];
  out.cardMods = secs.length ? chips.length : -1;
  out.boardMods = saved.length;
  out.cardIcons = row0.querySelectorAll('.fd-card .sb-chip img').length;
  // THE MODS RUN IN THE ORDER THE BUILD WAS SAVED: elements combine in slot
  // order, so a row sorted any other way can read as another element build.
  out.orderKept = chips.length === saved.length && saved.every((id, i) => {
    const label = chips[i].textContent.trim();
    if (id === BOARD_RIVEN_SLOT) return label.startsWith(tr('Riven'));
    const m = modById(splitRank(id)[0]);
    return !!m && label.startsWith(m.name);
  });
  out.savedOrder = saved.join(' ');
  out.shownOrder = chips.map((c) => c.textContent.trim()).join(' | ');

  // THE OVERVIEW SHIPS SHUT, and its fold is the reader's like every other.
  const ov = () => box().querySelector('.fd-ov');
  out.ovShut = !!ov() && ov().classList.contains('shut') && !seen(ov().querySelector('.fold-b'))
    && JSON.parse(localStorage.getItem('wfsim-folds') || '{}')['finder-overview'] == null;

  // EVERY ROW OPENS THE SAME WAY, in place: the whole build and, piece by
  // piece, how many here carry it and the best build without it.
  const rows = () => [...box().querySelectorAll('tr.fr[data-frow]')];
  // A MISSING CONTROL IS A FAILED ASSERTION, never a thrown probe.
  const tap = async (el) => { if (el) { el.click(); await sleep(250); } return !!el; };
  await tap(rows()[0]);
  out.detail0 = !!box().querySelector('tr.fdet .fd-why-row') && box().querySelectorAll('tr.fdet').length === 1;
  await tap(rows()[1]);
  out.detail1 = box().querySelectorAll('tr.fdet').length === 1
    && box().querySelector('tr.fr.x') === rows()[1] && !!box().querySelector('tr.fdet .fd-why-row');
  // THE COMPARISON IS THE READER'S: any build is made the reference, every
  // other row reads against it, and it is taken back as it was given.
  const refId = rows()[1].dataset.frow;
  out.refOn = await tap(box().querySelector('tr.fdet [data-fref]')) && !!box().querySelector('.fd-refbar:not(.off)') && finder.ref === refId
    && rows().filter((t) => t.dataset.frow !== refId).every((t) => !!t.querySelector('.fd-card.cmp'));
  await tap(rows()[0]);
  out.refDiff = !!box().querySelector('tr.fdet .fd-diff');
  out.refOff = await tap(box().querySelector('.fd-refbar [data-fref]')) && !box().querySelector('.fd-refbar:not(.off)') && !box().querySelector('.fd-card.cmp');
  // "I DON'T HAVE IT" on a piece is a condition like any other.
  const nh = box().querySelector('tr.fdet [data-fnohave]');
  out.noHave = nh ? nh.dataset.fnohave : '';
  if (nh) { nh.click(); await sleep(300); }
  out.noHaveKept = !!out.noHave && listed().map(byId).every((p) => !finderTokens(p).includes(out.noHave))
    && !!box().querySelector('.fd-tok.exc');
  const un = box().querySelector('[data-funtok]');
  if (un) { un.click(); await sleep(250); }
  if (box().querySelector('tr.fdet')) { box().querySelector('tr.fr.x').click(); await sleep(250); }
  // FIVE UNTIL ASKED: "more" adds rows, and "back to the top" folds them away.
  out.inScope = builtinBuilds().filter((p) => p.benchmark === finder.b && p.mode === finder.mo).length;
  const more = box().querySelector('[data-fmore]');
  if (more) { more.click(); await sleep(200); }
  out.afterMore = listed().length;
  const less = box().querySelector('[data-fless]');
  if (less) { less.click(); await sleep(200); }
  out.afterLess = listed().length;

  // REQUIRE, THEN EXCLUDE, the most-used card — by clicking it in the overview.
  out.ovOpened = !!ov() && await tap(ov().querySelector(':scope > .fold-h')) && !ov().classList.contains('shut') && seen(ov().querySelector('.fold-b'));
  const u = box().querySelector('.fd-ov .fd-use.mods [data-fcyc]') || box().querySelector('.fd-use.mods [data-fcyc]');
  out.token = u ? u.dataset.fcyc : '';
  u.click(); await sleep(300);
  const req = listed().map(byId);
  out.reqAll = req.length > 0 && req.every((p) => finderTokens(p).includes(out.token));
  out.reqChip = !!box().querySelector('.fd-tok.req');
  box().querySelector('.fd-use.mods [data-fcyc="' + out.token + '"]').click(); await sleep(300);
  const exc = listed().map(byId);
  out.excNone = exc.every((p) => !finderTokens(p).includes(out.token));
  out.excChip = !!box().querySelector('.fd-tok.exc');
  box().querySelector('.fd-use.mods [data-fcyc="' + out.token + '"]').click(); await sleep(300);
  out.cleared = !box().querySelector('.fd-tok');

  // OPEN: the build is current, and is in the bar as a read-only chip.
  const target = listed()[1] || listed()[0];
  box().querySelector('[data-fopen="' + target + '"]').click(); await sleep(1500);
  out.opened = activePreset === target;
  out.inBar = roChips().includes(target);
  out.chipSel = !!bar().querySelector('.pchip.ro.sel[data-name="' + target + '"]');
  out.finderSays = (box().querySelector('[data-fopen="' + target + '"]') || {}).className || '';
  // A BOARD ROW SAYS SO WHERE IT IS OPEN: a locked chip in the bar, and every
  // builder block inert.
  out.officialChipLocked = !!bar().querySelector('.pchip.ro.sel .plock');
  out.officialLocked = ['mod-block','arcane-block','evo-block','mode-block']
    .every((id) => (document.getElementById(id) || { classList: { contains: () => false } })
      .classList.contains('locked-hard'));

  // KEPT BY IDENTITY, NOT BY RANK: a rescore that moves the build down the
  // board renumbers its id, and the bar still holds the same build.
  const p = byId(target);
  const ident = boardRowIdentity(p.board);
  // A RESCORE ARRIVES AS A NEW FILE, so the rows are replaced, never edited.
  // UPWARDS, past the leader: a row scored DOWN far enough leaves the board's
  // depth cut altogether, which is the other rule (a build that has left the
  // board stops resolving) and would leave this one with nothing to follow.
  const top = Math.max(...BOARD.ballistica_prime.map((row) => row.score || 0)) * 1.5;
  BOARD.ballistica_prime = BOARD.ballistica_prime.map((row) => (row === p.board
    ? { ...row, score: top, shown: top.toFixed(4) } : row));
  const moved = builtinBuilds().find((x) => boardRowIdentity(x.board) === ident && x.benchmark === p.benchmark && x.mode === p.mode);
  out.renumbered = !!moved && presetId(moved) !== target;
  out.followed = openedPublished(buildBarCfg()).some((x) => presetId(x) === presetId(moved));
  // …AND THE PAGE REOPENS ON IT, not on whatever holds its old rank now.
  await open('Braton');
  await open('Ballistica_Prime');
  out.activeFollowed = activePreset === presetId(moved);
  out.chipsAfter = roChips();
  out.oneChip = out.chipsAfter.filter((id) => boardRowIdentity(byId(id).board) === ident).length === 1;

  // × TAKES IT OUT OF THE BAR, and the page lands on an unsaved build.
  const x = bar().querySelector('.pchip.ro .pop.unpin');
  const unpinned = x.dataset.unpin;
  x.click(); await sleep(1200);
  out.unpinGone = !roChips().includes(unpinned);
  out.unpinActive = activePreset;
  out.unpinSel = !!selChip();

  // A COPY is your own.
  const again = listed()[0];
  box().querySelector('[data-fopen="' + again + '"]').click(); await sleep(1200);
  copyActivePreset(buildBarCfg()); await sleep(800);
  // A COPY IS YOUR OWN: an ordinary chip, and the blocks open again.
  out.ownChip = !!bar().querySelector('.pchip.sel:not(.ro)');
  out.ownEditable = !document.getElementById('mod-block').classList.contains('locked-hard');

  // THE SCENARIO BAR ASKS ONE QUESTION.
  const sbar = document.getElementById('preset-bar-simulator-scenarios');
  out.scenarioShape = sbar
    ? Array.from(sbar.querySelectorAll('button.dd')).map((d) => d.id.replace(/-simulator-scenarios$/, ''))
    : [];

  // A WEAPON WITH NO ROWS draws the finder's empty state, not a stale table.
  // The board INDEX names every weapon that has any, so the candidates are
  // asked of it rather than opened one by one.
  const idx = await (await fetch('/board/index.json', { cache: 'no-cache' })).json();
  out.bareId = '';
  for (const w of (META.weapons || []).filter((x) => !(idx[x.id] || []).length).slice(0, 5)) {
    await open(wikiSlug(w));
    if (builtinBuilds().length) continue;
    out.bareId = w.id;
    out.bareEmpty = !!box().querySelector('.fd-empty') && !box().querySelector('tr.fr');
    break;
  }

  // MODE HAS AN "ALL": every mode of the ruler in one list, by score, and each
  // row names its mode, because its rank is still that mode's own.
  await open('Aeolak');
  const allSeg = box().querySelector('[data-fseg="mo"][data-v="*"]');
  out.allSeg = !!allSeg;
  if (allSeg) {
    allSeg.click(); await sleep(300);
    const more = () => box().querySelector('[data-fmore]');
    while (more()) { more().click(); await sleep(150); }
    const rows = await everyBuild();
    const sc = (p) => (p.board || {}).score || 0;
    const want = builtinBuilds().filter((p) => p.benchmark === finder.b);
    out.allCount = rows.length + '/' + want.length;
    out.allModes = new Set(rows.map((p) => p.mode)).size;
    out.allSorted = rows.every((p, i) => i === 0 || sc(rows[i - 1]) >= sc(p));
    out.allNamed = [...box().querySelectorAll('tr.fr:not(.tie)')].every((tr, i) =>
      tr.querySelector('.fd-sd').textContent.includes(rows[i].modeName));
  }

  // A TIE IS ONE PLACE: builds on one exact score share a rank, the next score
  // is the next rank, a place is one row, and it opens into every exact build.
  await open('Felarx');
  Object.assign(finder, { b: 'standard_single_target', mo: 'cycle', rv: 'plain', shown: 500 });
  renderBuildFinder(); await sleep(200);
  const cell = builtinBuilds().filter((p) => p.benchmark === finder.b && p.mode === finder.mo && !p.riven);
  const scores = [...new Set(cell.map((p) => p.board.score))].sort((a, b) => b - a);
  out.tiePlaces = cell.length > 0 && cell.every((p) => p.rank === scores.indexOf(p.board.score) + 1);
  out.tieRows = box().querySelectorAll('tr.fr').length + '/' + scores.length;
  const tie = box().querySelector('tr.fr.tie');
  out.tieHeads = box().querySelectorAll('tr.fr.tie').length;
  if (tie) {
    const n = cell.filter((p) => p.rank === byId(tie.dataset.frow).rank).length;
    out.tieFree = !!tie.querySelector('.fd-free .sb-chip');
    tie.querySelector('[data-ftie]').click(); await sleep(200);
    const mem = [...box().querySelectorAll('tr.fr.mem')].map((t) => byId(t.dataset.frow));
    out.tieMembers = mem.length + '/' + n;
    out.tieExact = new Set(mem.map((p) => boardRowIdentity(p.board))).size === n
      && mem.every((p) => p.board.score === mem[0].board.score);
  }

  // REFUSING A TRIGGER IS PRECISE: refusing weak-point kills drops every build
  // with a part that asks for one and keeps every other, those whose parts ask
  // for any kill included; a group refuses all of its own; the refusal is the
  // reader's and is stored.
  const pt = META.part_triggers || {};
  const asks = (p, t) => boardRowParts(p.board).some((x) =>
    ((pt[x.axis] || {})[x.axis === 'mods' ? splitRank(x.id)[0] : x.id] || []).includes(t));
  const hk = box().querySelector('[data-ftrg="headshot_kill"]');
  out.trgBtn = !!hk;
  if (hk) {
    hk.click(); await sleep(200);
    const kept = await everyBuild();
    const want = cell.filter((p) => !asks(p, 'headshot_kill'));
    out.trgCount = kept.length + '/' + want.length + ' of ' + cell.length;
    out.trgPrecise = kept.length === want.length && kept.every((p) => !asks(p, 'headshot_kill'))
      && want.length < cell.length && kept.some((p) => asks(p, 'kill'));
    out.trgStored = JSON.parse(localStorage.getItem('wfsim-finder-triggers-off') || '[]').includes('headshot_kill');
    box().querySelector('[data-ftrgg="kill"]').click(); await sleep(200);
    const none = await everyBuild();
    out.trgGroup = none.every((p) => !['kill', 'headshot_kill', 'melee_kill'].some((t) => asks(p, t)))
      && none.length === cell.filter((p) => !['kill', 'headshot_kill', 'melee_kill'].some((t) => asks(p, t))).length;
    localStorage.removeItem('wfsim-finder-triggers-off'); renderBuildFinder(); await sleep(150);
  }

  // Left SHUT, for the cross-weapon half below — which needs a real page load,
  // not this one's navigation: the section element survives a route() with its
  // class on it, so nothing there can tell a stored answer from a kept one.
  if (!box().classList.contains('shut')) { box().querySelector('.fd-title').click(); await sleep(150); }
  return out;
})()`);

// THE CASE HAS TO EXIST. The native dev server does not serve the board, and
// against it every assertion below would pass on an empty table.
check("the weapon under test actually has board rows", r.rows > 0, `${r.rows} rows — is this running against site/?`);
check("the finder is drawn in the builder", r.drawn === true);
check("...a click in its search box does not fold it", r.searchKept === true);
check("...open with no build of the reader's own on the weapon, with nothing stored yet",
  r.foldDefault && r.foldUnstored, `open ${r.foldDefault}, unstored ${r.foldUnstored}`);
check("...and shutting then opening it is what gets stored", r.foldOpened && r.foldStored,
  `opened ${r.foldOpened}, stored ${r.foldStored}`);
check("...it stays as it is, caret and all, when it redraws", r.foldRedrawn === true);
check("...the jump menu lists it by name", /Build finder|配装查找器/.test(r.jumpName), JSON.stringify(r.jumpName));
check("...listing the top five of one scope", r.firstPage === Math.min(5, r.inScope) && r.oneCell,
  `${r.firstPage} rows of ${r.inScope}`);
check("...each row drawn as the build card, one chip per card the build carries",
  r.cardMods === r.boardMods && r.cardIcons > 0, `${r.cardMods} chips for ${r.boardMods} mods, ${r.cardIcons} icons`);
check("...more on request, and folded back to five", r.inScope <= 5
  || (r.afterMore === Math.min(25, r.inScope) && r.afterLess === 5), `${r.afterMore} then ${r.afterLess}`);
check("...its mods in the order the build was saved", r.orderKept === true,
  `saved ${r.savedOrder} / shown ${r.shownOrder}`);
check("the overview ships shut, with nothing stored yet", r.ovShut === true);
check("...and opens on a click", r.ovOpened === true);
check("a row opens in place into the whole build, piece by piece", r.detail0 === true);
check("...and so does any other row, one open at a time", r.detail1 === true);
check("any build can be made the reference, and every other row reads against it", r.refOn === true);
check("...an opened row says what it has that the reference does not", r.refDiff === true);
check("...and the comparison is taken back as it was given", r.refOff === true);
check(`"I don't have it" on ${r.noHave || "a piece"} lists only builds without it`, r.noHaveKept === true);
check(`requiring ${r.token} lists only builds carrying it`, r.reqAll && r.reqChip);
check("...excluding it lists none that do", r.excNone && r.excChip);
check("...and a third click clears the condition", r.cleared);
check("Mode offers All when a ruler holds more than one mode", r.allSeg === true);
check("...listing every mode's builds in one list, best first",
  r.allSeg && r.allCount.split('/')[0] === r.allCount.split('/')[1] && r.allModes > 1 && r.allSorted,
  `${r.allCount} rows, ${r.allModes} modes, sorted ${r.allSorted}`);
check("...each row naming its mode", r.allNamed === true);
check("a tie is one place: one exact score, one rank, and the next score the next rank", r.tiePlaces === true);
check("...one row per place", r.tieHeads > 0 && r.tieRows.split('/')[0] === r.tieRows.split('/')[1], `${r.tieRows} rows/places, ${r.tieHeads} ties`);
check("...naming the positions its builds differ in", r.tieFree === true);
check("...and opening into every exact build it stands for", r.tieExact === true && r.tieMembers.split('/')[0] === r.tieMembers.split('/')[1],
  r.tieMembers);
check("refusing weak-point kills drops every build that asks for one, and only those", r.trgBtn && r.trgPrecise === true, r.trgCount);
check("...the reader's refusal is stored", r.trgStored === true);
check("...and the kill group refuses every kill", r.trgGroup === true);
check("Open makes the build current", r.opened === true);
check("...and puts it in the build bar, read-only and selected", r.inBar && r.chipSel);
check("...and the finder says it is already there", /\bin\b/.test(r.finderSays), r.finderSays);
check("a rescore that renumbers the build leaves the bar on the same build", r.renumbered && r.followed,
  `renumbered ${r.renumbered}, followed ${r.followed}`);
check("...and leaving and coming back reopens that build, not its old rank", r.activeFollowed && r.oneChip,
  `active followed ${r.activeFollowed}, chips ${JSON.stringify(r.chipsAfter)}`);
check("× takes it out of the bar", r.unpinGone);
check("...and leaves an unsaved build open, which no chip stands for",
  r.unpinActive === "" && r.unpinSel === false, JSON.stringify(r.unpinActive));

check("landing on a weapon opens an UNSAVED build, and selects no chip",
  r.landingActive === "" && r.landingSel === false, JSON.stringify(r.landingActive));
check("a board row says it is one WHERE IT IS OPEN: a locked chip, and inert blocks",
  r.officialChipLocked && r.officialLocked, `chip ${r.officialChipLocked}, locked ${r.officialLocked}`);
check("...and a copy of it is an ordinary build you can edit",
  r.ownChip && r.ownEditable, `chip ${r.ownChip}, editable ${r.ownEditable}`);
check("the SCENARIO bar asks one question: which ruler to open", JSON.stringify(r.scenarioShape) === JSON.stringify(["dd-open"]),
  JSON.stringify(r.scenarioShape));
check(`a weapon with no rows shows the finder's empty state (${r.bareId || "none found"})`, !!r.bareId && r.bareEmpty === true);
// ---- ACROSS WEAPONS, ACROSS LOADS ---------------------------------------
// What a reader folds is a habit, not a property of a gun: the state is keyed
// by fold id alone. Asserted with REAL page loads on two other weapons, because
// a route() keeps the section element — and its class — whatever is stored.
const carried = async (path, then) => {
  await app.load(path);
  return evaluate(`(async () => {
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    await sleep(3000);
    const box = document.getElementById('build-finder');
    const seen = { shut: box.classList.contains('shut'),
      stored: JSON.parse(localStorage.getItem('wfsim-folds') || '{}')['build-finder'] };
    ${then || ""}
    return seen;
  })()`);
};
const shutElsewhere = await carried("/weapons/Braton",
  "box.querySelector('.fd-title').click(); await sleep(200);");
check("shut on one weapon is shut on the next, loaded cold",
  shutElsewhere.shut === true && shutElsewhere.stored === true, JSON.stringify(shutElsewhere));
const openElsewhere = await carried("/weapons/Torid");
check("...and opening it there opens it on a third weapon too",
  openElsewhere.shut === false && openElsewhere.stored === false, JSON.stringify(openElsewhere));


// ---- WHAT IT SHIPS AS, BEFORE THE READER ANSWERS -------------------------
// Shut above a build of the reader's own on the weapon, open with none; decided
// on arriving at the weapon, so a first edit does not shut it.
const unanswered = (own) => evaluate(`(() => {
  const f = JSON.parse(localStorage.getItem('wfsim-folds') || '{}');
  delete f['build-finder'];
  localStorage.setItem('wfsim-folds', JSON.stringify(f));
  localStorage.setItem('wfsim-presets-builder-builds', JSON.stringify(${own} ? [{ id: 'own1', scope: 'torid', name: 'mine', savedAt: 1,
    state: buildState('torid', { evoSel: {}, arcane: ['none'], arcaneRank: [null], slots: [{ mod: 'serration', rank: 0 }],
      mode: null, valence: null, assembly: null, wielder: null }) }] : []));
  return true;
})()`);
await unanswered(true);
const withOwn = await carried("/weapons/Torid");
check("with a build of the reader's own on the weapon, it ships shut",
  withOwn.shut === true && withOwn.stored === undefined, JSON.stringify(withOwn));
await unanswered(false);
const firstEdit = await carried("/weapons/Torid", `
  localStorage.setItem('wfsim-presets-builder-builds', JSON.stringify([{ id: 'own2', scope: 'torid', name: 'preset 1', savedAt: 2,
    state: buildState('torid', { evoSel: {}, arcane: ['none'], arcaneRank: [null], slots: [{ mod: 'serration', rank: 0 }],
      mode: null, valence: null, assembly: null, wielder: null }) }]));
  renderPresetBar(); await sleep(200);
  seen.afterEdit = box.classList.contains('shut');`);
check("with none, it ships open, and a first build there does not shut it",
  firstEdit.shut === false && firstEdit.afterEdit === false && firstEdit.stored === undefined, JSON.stringify(firstEdit));

await app.finish("the finder finds, the bar holds");
