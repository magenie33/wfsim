// THE BUILD FINDER FINDS, THE BUILD BAR HOLDS, AND THE LINE SAYS WHAT IS OPEN.
//
// The finder is a query over the board's builds and never a selection: every
// row it lists must satisfy the query, and "Open" puts that build in the build
// bar as a read-only chip and makes it current. The chip is kept by WHAT THE
// BUILD IS, so a rescore that renumbers the board leaves it on the same build.
// Asserted here too: the three states of "what is open", and the scenario bar
// keeping its single control.
//
//   node scripts/check_build_finder.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  localStorage.clear();
  const box = () => document.getElementById('build-finder');
  const bar = () => document.getElementById('preset-bar-builder-builds');
  const listed = () => Array.from(box().querySelectorAll('tr.fr[data-frow]')).map((t) => t.dataset.frow);
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

  // IT FOLDS LIKE EVERY BOX, AND IT SHIPS SHUT. The state is the reader's and
  // outlives the weapon; a click in the search box is the search box's.
  // Asked of the computed layout, not the class.
  const seen = (el) => !!el && el.offsetParent !== null;
  const head = () => box().querySelector(':scope > .fd-head');
  out.foldDefault = box().classList.contains('shut') && !seen(box().querySelector('.fd-main'))
    && seen(box().querySelector('.fd-title'));
  out.foldUnstored = JSON.parse(localStorage.getItem('wfsim-folds') || '{}')['build-finder'] == null;
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
  const mods0 = (byId(row0.dataset.frow).board.mods || []).filter(Boolean).length;
  const secs = [...row0.querySelectorAll('.fd-card .sb-h')];
  out.cardMods = secs.length ? secs[0].nextElementSibling.querySelectorAll('.sb-chip').length : -1;
  out.boardMods = mods0;
  out.cardIcons = row0.querySelectorAll('.fd-card .sb-chip img').length;
  // FIVE UNTIL ASKED: "more" adds rows, and "back to the top" folds them away.
  out.inScope = builtinBuilds().filter((p) => p.benchmark === finder.b && p.mode === finder.mo).length;
  const more = box().querySelector('[data-fmore]');
  if (more) { more.click(); await sleep(200); }
  out.afterMore = listed().length;
  const less = box().querySelector('[data-fless]');
  if (less) { less.click(); await sleep(200); }
  out.afterLess = listed().length;

  // REQUIRE, THEN EXCLUDE, the most-used card — by clicking it in the usage rail.
  const u = box().querySelector('.fd-use.mods [data-fcyc]');
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
  out.followed = openedBoardBuilds().some((x) => presetId(x) === presetId(moved));
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
  const sbar = document.getElementById('bench-bar-simulator-scenarios');
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
check("...shut until the reader opens it, with nothing stored yet",
  r.foldDefault && r.foldUnstored, `shut ${r.foldDefault}, unstored ${r.foldUnstored}`);
check("...and opening it is what gets stored", r.foldOpened && r.foldStored,
  `opened ${r.foldOpened}, stored ${r.foldStored}`);
check("...it stays as it is, caret and all, when it redraws", r.foldRedrawn === true);
check("...the jump menu lists it by name", /Build finder|配装查找器/.test(r.jumpName), JSON.stringify(r.jumpName));
check("...listing the top five of one scope", r.firstPage === Math.min(5, r.inScope) && r.oneCell,
  `${r.firstPage} rows of ${r.inScope}`);
check("...each row drawn as the build card, one chip per card the build carries",
  r.cardMods === r.boardMods && r.cardIcons > 0, `${r.cardMods} chips for ${r.boardMods} mods, ${r.cardIcons} icons`);
check("...more on request, and folded back to five", r.inScope <= 5
  || (r.afterMore === Math.min(25, r.inScope) && r.afterLess === 5), `${r.afterMore} then ${r.afterLess}`);
check(`requiring ${r.token} lists only builds carrying it`, r.reqAll && r.reqChip);
check("...excluding it lists none that do", r.excNone && r.excChip);
check("...and a third click clears the condition", r.cleared);
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
check("the SCENARIO bar asks one question", JSON.stringify(r.scenarioShape) === JSON.stringify(["dd-bench"]),
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

process.exit(0);
