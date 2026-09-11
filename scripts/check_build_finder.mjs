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
  const line = () => {
    const el = document.getElementById('build-current');
    return el ? el.textContent.replace(/\\s+/g, ' ').trim() : '';
  };
  const roChips = () => Array.from(bar().querySelectorAll('.pchip.ro')).map((c) => c.dataset.name);
  const open = async (name) => {
    history.pushState({}, '', '/weapons/' + name); route(); await sleep(3500);
  };
  const out = {};

  await open('Ballistica_Prime');
  out.rows = ((typeof BOARD !== 'undefined' && BOARD) ? (BOARD['ballistica_prime'] || []) : []).length;
  out.drawn = !box().hidden && getComputedStyle(box()).display !== 'none';
  out.lineLanding = line();
  out.firstPage = listed().length;
  // THE SCOPE IS ONE CELL: every listed row shares the ruler and the mode the
  // scope segments say.
  const scoped = listed().map(byId);
  out.oneCell = scoped.length > 0 && scoped.every((p) => p && p.benchmark === finder.b && p.mode === finder.mo);

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
  out.lineOfficial = line();

  // KEPT BY IDENTITY, NOT BY RANK: a rescore that moves the build down the
  // board renumbers its id, and the bar still holds the same build.
  const p = byId(target);
  const ident = boardRowIdentity(p.board);
  // A RESCORE ARRIVES AS A NEW FILE, so the rows are replaced, never edited.
  BOARD.ballistica_prime = BOARD.ballistica_prime.map((row) => (row === p.board ? { ...row, score: -1, shown: '-1' } : row));
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
  out.lineAfterUnpin = line();

  // A COPY is your own.
  const again = listed()[0];
  box().querySelector('[data-fopen="' + again + '"]').click(); await sleep(1200);
  copyActivePreset(buildBarCfg()); await sleep(800);
  out.lineOwn = line();

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
  return out;
})()`);

// THE CASE HAS TO EXIST. The native dev server does not serve the board, and
// against it every assertion below would pass on an empty table.
check("the weapon under test actually has board rows", r.rows > 0, `${r.rows} rows — is this running against site/?`);
check("the finder is drawn in the builder", r.drawn === true);
check("...listing one page of one scope", r.firstPage > 0 && r.firstPage <= 25 && r.oneCell, `${r.firstPage} rows`);
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
check("...and leaves an unsaved build open, not another board row", r.unpinActive === "" && /unsaved|尚未保存/.test(r.lineAfterUnpin),
  `${JSON.stringify(r.unpinActive)} · ${r.lineAfterUnpin}`);

check("landing on a weapon opens an UNSAVED build", /unsaved|尚未保存/.test(r.lineLanding), r.lineLanding);
check("a board row says it is one, and cannot be edited", /read-only|cannot be edited|不能编辑|榜单行/.test(r.lineOfficial), r.lineOfficial);
check("...a copy of it says it is your own", /your own|你自己/.test(r.lineOwn), r.lineOwn);
check("the SCENARIO bar asks one question", JSON.stringify(r.scenarioShape) === JSON.stringify(["dd-bench"]),
  JSON.stringify(r.scenarioShape));
check(`a weapon with no rows shows the finder's empty state (${r.bareId || "none found"})`, !!r.bareId && r.bareEmpty === true);

process.exit(0);
