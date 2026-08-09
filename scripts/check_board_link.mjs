// The SIXTEENTH check: A BOARD ROW OPENS THAT ROW.
//
// The board page lists one line per weapon per ruler, and clicking one is the
// main way anybody arrives at a build. What arrives has to be the build that
// line is about — under the ruler that line is on.
//
// The failure it exists for (owner, 2026-08-08): "torid的无瞄准榜首我记得是带了
// 衰弱的，但是现在跳转的那个并不是，是瞄准头的榜首。我现在读取并且计算，得到的
// 结果也不对". The link carried the weapon and the mode and NOT the ruler, and
// both boards call their leader "#1 · Incarnon cycle" — so the no-aim leader
// opened the aimed board's leader, under the aimed board's fight, and re-running
// it produced a number that matched neither line on either board.
//
// Two halves, because a row is two things:
//   · the BUILD — mods, arcane, evolutions, the mode it was played in
//   · the FIGHT — the ruler it was measured under, without which the number on
//     the line cannot be reproduced on the page it links to
//
// Asserted against `BOARD` itself rather than against a build written down
// here, so it keeps holding as the board moves under it.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/benchmark'); route(); await s(2500);
  const out = { rulers: [] };
  const chips = [...document.querySelectorAll('#bench-picker [data-bench]')];
  out.rulers = chips.map(c => c.dataset.bench);
  out.boardWeapons = Object.keys(BOARD || {}).length;
  // EVERY RULER, not just the second one: the bug was that one of them was
  // reachable and the rest resolved to it.
  out.each = [];
  for (const id of out.rulers) {
    const chip = [...document.querySelectorAll('#bench-picker [data-bench]')]
      .find(c => c.dataset.bench === id);
    chip.click(); await s(900);
    // A MEASURED row: the board lists the whole roster, and an unmeasured
    // weapon is a link to a page with nothing of this ruler's on it.
    const row = [...document.querySelectorAll('.bench-rows .brow')]
      .filter(a => !a.classList.contains('none'))[0];
    const rec = { ruler: id, href: row ? row.getAttribute('href') : null };
    if (row) {
      history.pushState({}, '', rec.href); route(); await s(3200);
      rec.weapon = document.getElementById('weapon').value;
      rec.scenario = activeScenario;
      const b = buildNamed(activePreset) || {};
      rec.buildRuler = b.benchmark;
      rec.buildMode = b.mode;
      rec.mods = slots.filter(x => x.mod).map(x => x.mod);
      rec.arcanes = arcanes.filter(a => a && a !== 'none');
      rec.mode = mode;
      // BY MODE, not "the first row for this weapon". A weapon can hold a
      // row per mode, so taking [0] compared the row you clicked against
      // whichever one happened to be stored first.
      const hrefMode = (rec.href.match(/mode=([^&]+)/) || [])[1] || 'base';
      const want = (BOARD[rec.weapon] || [])
        .filter(x => x.benchmark === id && (x.mode || 'base') === hrefMode)
        .sort((a, b) => b.score - a.score);
      rec.wantMods = (want[0] || {}).mods || null;
      rec.wantArcanes = (want[0] || {}).arcanes || null;
      rec.wantMode = (want[0] || {}).mode || null;
      // Back to the board for the next ruler.
      history.pushState({}, '', '/benchmark'); route(); await s(1800);
    }
    out.each.push(rec);
  }

  // ---- TWO MODES, ONE WEAPON -------------------------------------------
  //
  // The board LISTS every weapon in every mode it can be played in and always
  // has; what it has never HELD is two measured rows for one weapon, because
  // no submission has ever named a second mode. So the half of a board row's
  // identity that says HOW the weapon was played has never been told apart
  // from the other row of the same weapon — which is exactly the shape of the
  // bug that made this check exist, one level down (owner, 2026-08-09: 排查一下
  // 很多武器的其他形态是否可以正确上传显示).
  //
  // Synthetic, because waiting for a player to submit a base-form Torid is not
  // a test plan. The row is a real row: same benchmark, a real mode this
  // weapon really has, a build the engine really accepts.
  history.pushState({}, '', '/benchmark'); route(); await s(2000);
  // BACK TO THE FIRST RULER. The loop above left the picker on the LAST one,
  // and a synthetic row built for one board clicked on another is a test that
  // fails for the wrong reason.
  const ruler = out.rulers[0];
  [...document.querySelectorAll('#bench-picker [data-bench]')]
    .find(c => c.dataset.bench === ruler).click();
  await s(900);
  const twoModes = (META.weapons || []).find(w =>
    (w.modes || []).length > 1 &&
    (BOARD[w.id] || []).some(r => r.benchmark === ruler));
  out.twoModeWeapon = twoModes ? twoModes.id : null;
  if (twoModes) {
    const have = (BOARD[twoModes.id] || []).find(r => r.benchmark === ruler);
    const other = (twoModes.modes || []).find(m => m !== (have.mode || 'base'));
    out.otherMode = other;
    BOARD[twoModes.id] = (BOARD[twoModes.id] || []).concat([
      { ...have, mode: other, score: have.score / 3, shown: String(have.score / 3) },
    ]);
    renderBenchBoard(); await s(1200);
    const rows = [...document.querySelectorAll('.bench-rows .brow')]
      .filter(a => (a.getAttribute('href') || '').includes('/' + wikiSlug(twoModes) + '?'));
    out.bothListed = rows.length;
    out.bothMeasured = rows.filter(a => !a.classList.contains('none')).length;
    const mine = rows.find(a => (a.getAttribute('href') || '').includes('mode=' + other));
    out.otherHref = mine ? mine.getAttribute('href') : null;
    if (mine) {
      history.pushState({}, '', out.otherHref); route(); await s(3200);
      out.otherOpenedWeapon = document.getElementById('weapon').value;
      out.otherOpenedMode = mode;
      out.otherOpenedRuler = activeScenario;
      out.otherOpenedMods = slots.filter(x => x.mod).map(x => x.mod);
      out.otherWantMods = have.mods || [];
    }
  }
  return out;
})()`);

check("the board page knows some weapons", r.boardWeapons > 0, String(r.boardWeapons));
check("...and offers every ruler", r.rulers.length >= 1, r.rulers.join(", "));

for (const e of r.each) {
  const tag = `[${e.ruler}]`;
  check(`${tag} its top row links somewhere`, !!e.href, String(e.href));
  if (!e.href) continue;
  check(`${tag} ...and the link names the ruler`, /bench=/.test(e.href), e.href);
  check(`${tag} ...it opens that ruler's fight`, e.scenario === e.ruler,
    `${e.scenario} vs ${e.ruler}`);
  check(`${tag} ...and that ruler's build`, e.buildRuler === e.ruler,
    `${e.buildRuler} vs ${e.ruler}`);
  check(`${tag} ...with the mods that row holds`,
    JSON.stringify(e.mods) === JSON.stringify(e.wantMods),
    `${JSON.stringify(e.mods)} vs ${JSON.stringify(e.wantMods)}`);
  check(`${tag} ...and its arcane`,
    JSON.stringify(e.arcanes) === JSON.stringify(e.wantArcanes || []),
    `${JSON.stringify(e.arcanes)} vs ${JSON.stringify(e.wantArcanes)}`);
  // THE MODE, which is half the entrant's identity — a row played through the
  // cycle and one that never transmutes are two lines on the same board.
  check(`${tag} ...played the way the row was`, e.mode === (e.wantMode || "base"),
    `${e.mode} vs ${e.wantMode}`);
}

// ---- and the same weapon in TWO modes ----------------------------------
check("a weapon with two modes is on the board twice", r.bothListed === 2,
  `${r.twoModeWeapon}: ${r.bothListed} rows`);
check("...both measured, so both are rows and not placeholders",
  r.bothMeasured === 2, `${r.bothMeasured} measured`);
check("...and the second one's link names ITS mode",
  !!r.otherHref && r.otherHref.includes(`mode=${r.otherMode}`), String(r.otherHref));
check("...it opens that weapon", r.otherOpenedWeapon === r.twoModeWeapon,
  `${r.otherOpenedWeapon} vs ${r.twoModeWeapon}`);
// THE POINT. Two rows of one weapon differ in exactly one thing, and clicking
// the second must not land on the first.
check("...played the way THAT row was, not the way the other one was",
  r.otherOpenedMode === r.otherMode, `${r.otherOpenedMode} vs ${r.otherMode}`);
check("...under the ruler it was measured on", r.otherOpenedRuler === r.rulers[0],
  `${r.otherOpenedRuler} vs ${r.rulers[0]}`);
check("...carrying that row's build",
  JSON.stringify(r.otherOpenedMods) === JSON.stringify(r.otherWantMods),
  `${JSON.stringify(r.otherOpenedMods)} vs ${JSON.stringify(r.otherWantMods)}`);

await app.finish("a board row opens the build it is about, under the ruler it is on");
