// The SIXTEENTH check: A BOARD ROW OPENS THAT ROW.
//
// The board page lists one line per weapon per ruler, and clicking one is the
// main way anybody arrives at a build. What arrives has to be the build that
// line is about — under the ruler that line is on.
//
// The failure it exists for (owner, 2026-08-08). The link carried the weapon
// and the mode and NOT the ruler, and both boards call their leader "#1 ·
// Incarnon cycle" — so the no-aim leader opened the aimed board's leader,
// under the aimed board's fight, and re-running it produced a number that
// matched neither line on either board.
//
// Two halves, because a row is two things:
//   · the BUILD — mods, arcane, evolutions, the mode it was played in
//   · the FIGHT — the ruler it was measured under, without which the number on
//     the line cannot be reproduced on the page it links to
//
// Asserted against `BOARD` itself rather than against a build written down
// here, so it keeps holding as the board moves under it.
//
// A THIRD concern joined them on 2026-08-20, one level down: the board page
// lists a weapon's BEST row per mode, and the deeper ranks live in the
// builder's picker — where a rank only means something inside one way of
// playing. See the section at the bottom.
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
      // A RIVEN ROW LANDS WITH A RIVEN MADE FOR IT, so the slot holds
      // riven:<the card's name> where the board row states the bare riven.
      // Normalised rather than compared raw: the name is generated and
      // localized ("榜单 · critical_chance / …"), so a raw comparison would be
      // asserting the label rather than the build.
      rec.mods = slots.filter(x => x.mod)
        .map(x => (String(x.mod).startsWith('riven') ? 'riven' : x.mod));
      rec.arcanes = arcanes.filter(a => a && a !== 'none');
      rec.mode = mode;
      // BY MODE, not "the first row for this weapon". A weapon can hold a
      // row per mode, so taking [0] compared the row you clicked against
      // whichever one happened to be stored first.
      const hrefMode = (rec.href.match(/mode=([^&]+)/) || [])[1] || 'base';
      // …AND WHICH OF THE TWO LEADERS. A board holds a riven row and a plain
      // one per weapon and mode, both called #1, and until 2026-08-25 the link
      // carried no way to tell them apart — so clicking the riven view's
      // leader opened the plain one. Taking the highest score of the union
      // here would compare the row that WAS opened against the row that should
      // have been, and call the link correct whichever it opened.
      const hrefRiven = (rec.href.match(/riven=([01])/) || [])[1];
      const want = (BOARD[rec.weapon] || [])
        .filter(x => x.benchmark === id && (x.mode || 'base') === hrefMode
          && (hrefRiven === undefined || (!!x.riven) === (hrefRiven === '1')))
        .sort((a, b) => b.score - a.score);
      rec.hrefRiven = hrefRiven;
      rec.wantRiven = !!(want[0] || {}).riven;
      rec.openedRiven = slots.some(x => String(x.mod || '').startsWith('riven'));
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
  // BACK TO A RULER THAT HAS ROWS. The loop above left the picker on the LAST
  // one, and a synthetic row built for one board clicked on another is a test
  // that fails for the wrong reason — as is one built for a board with nothing
  // on it, which is what a NEW ruler is. rulers[0] is the PRIMARY one and
  // therefore the populated one today, but that is a fact about the board
  // rather than a guarantee, so this reads it.
  const ruler = out.rulers.find(id => Object.values(BOARD).some(rs =>
    (rs || []).some(r => r.benchmark === id))) || out.rulers[0];
  out.fixtureRuler = ruler;
  [...document.querySelectorAll('#bench-picker [data-bench]')]
    .find(c => c.dataset.bench === ruler).click();
  await s(900);
  // THE LIVE CASE FIRST, and only then the synthetic one.
  //
  // This was injection-only, and the injection went wrong the day the board
  // stopped needing it: the Larkspur Prime now holds a REAL alternate-fire row
  // under single_target, so a synthetic second-mode row landed beside it, the
  // rendered mode= link resolved to the REAL leader, and the check compared
  // that leader's build against the base row it had copied. It reported a
  // one-mod difference and blamed the link (2026-08-18).
  //
  // A fixture that duplicates something the live data already has is a fixture
  // that will collide with it. So the live rows are preferred and the
  // injection stays as the FALLBACK — the case has to keep being covered on a
  // board that loses its last two-mode weapon, which is how it was covered for
  // the nine days before one arrived.
  const modesOn = (w) => [...new Set((BOARD[w.id] || [])
    .filter(r => r.benchmark === ruler).map(r => r.mode || 'base'))];
  const live = (META.weapons || []).find(w =>
    (w.modes || []).length > 1 && modesOn(w).length > 1);
  const twoModes = live || (META.weapons || []).find(w =>
    (w.modes || []).length > 1 &&
    (BOARD[w.id] || []).some(r => r.benchmark === ruler));
  out.twoModeWeapon = twoModes ? twoModes.id : null;
  out.synthetic = !live;
  if (twoModes) {
    // THE LEADER OF EACH MODE, which is what the board lists and what the link
    // opens — board.json is stored best-first, so find is the leader.
    const base = (BOARD[twoModes.id] || [])
      .find(r => r.benchmark === ruler && (r.mode || 'base') === 'base');
    const other = live
      ? modesOn(live).find(m => m !== 'base')
      : (twoModes.modes || []).find(m => m !== ((base || {}).mode || 'base'));
    out.otherMode = other;
    if (!live) {
      const have = base || (BOARD[twoModes.id] || []).find(r => r.benchmark === ruler);
      BOARD[twoModes.id] = (BOARD[twoModes.id] || []).concat([
        { ...have, mode: other, score: have.score / 3, shown: String(have.score / 3) },
      ]);
    }
    const have = (BOARD[twoModes.id] || [])
      .find(r => r.benchmark === ruler && (r.mode || 'base') === other);
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

// A RULER WITH NO ROWS HAS NO ROW TO OPEN, and that is a real state rather
// than a failure: a benchmark exists before anyone has submitted to it. It is
// REPORTED rather than skipped in silence, because a check that quietly
// exercises nothing reads exactly like one that exercised everything.
const empty = r.each.filter((e) => !e.href).map((e) => e.ruler);
if (empty.length) console.log(`  --  no rows yet, nothing to open: ${empty.join(", ")}`);
check("at least one ruler has rows to check",
  r.each.some((e) => e.href), r.each.map((e) => e.ruler).join(", "));

for (const e of r.each.filter((x) => x.href)) {
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
  // AND CARRYING A RIVEN IF THE ROW DID. The build that lands has to be the one
  // the link named on BOTH halves of a weapon's board — the empty riven slot
  // the owner reported (2026-08-24) was this: a riven row's link opened the
  // plain leader, so the slot the row needed had nothing in it.
  check(`${tag} ...and the link says which of the two leaders it is`,
    e.hrefRiven === '0' || e.hrefRiven === '1', String(e.hrefRiven));
  check(`${tag} ...opening a build with a riven exactly when the row has one`,
    e.openedRiven === e.wantRiven, `opened ${e.openedRiven}, row ${e.wantRiven}`);
}

// ---- and the same weapon in TWO modes ----------------------------------
// WHICH CASE RAN, in the title. A live two-mode weapon and an injected one are
// two different amounts of evidence, and a check that does not say which it
// found reads as the stronger one on the day it silently becomes the weaker.
check(`a weapon with two modes is on the board twice (${r.synthetic ? "injected" : "LIVE"})`,
  r.bothListed === 2, `${r.twoModeWeapon}: ${r.bothListed} rows`);
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

// ---- and the RANK a picker shows is a rank WITHIN a mode ---------------
//
// The board page lists a weapon's BEST row per mode; the deeper ranks live in
// the builder's own picker, which groups them by mode and labels each one with
// its rank inside that mode. Both halves were right and the ORDER was not:
// `builtinBuilds` sorted by ruler only, and inside a ruler it inherited
// board.json's order — which is by SCORE and knows nothing about modes. The
// picker draws a group header where the group CHANGES, so two interleaved
// modes drew one of them TWICE with the other wedged inside it, and the ranks
// restarted mid-list.
//
// Live on nine weapons when it was found (owner, 2026-08-20): the Burston
// Prime's two `base` rows sat at positions 93 and 94 of its 100 `cycle` rows,
// so the picker read "Incarnon cycle #1..#92, base #1 #2, Incarnon cycle
// #93..#100".
//
// TWO HALVES, because either alone passes on a page that lost the other. The
// ORDER is asserted over every weapon the board holds in more than one mode —
// a property, not a weapon — and the DRAWING is asserted once in the DOM,
// since an order nothing renders is an order nobody reads.
const pick = await evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  const out = { weapons: [] };
  // Where the grouping is VISIBLE: a weapon this board holds in more than one
  // mode under one ruler. Includes the synthetic row injected above, which is
  // why this runs after it.
  const many = (META.weapons || []).filter(w => {
    const byRuler = {};
    for (const r of (BOARD[w.id] || [])) {
      (byRuler[r.benchmark] = byRuler[r.benchmark] || new Set()).add(r.mode || 'base');
    }
    return Object.values(byRuler).some(set => set.size > 1);
  });
  for (const w of many) {
    // builtinBuilds reads the weapon on screen, so this is the one input it
    // takes. Nothing else about the page decides its answer.
    document.getElementById('weapon').value = w.id;
    const ps = builtinBuilds();
    const blocks = [];
    const prev = {};
    let last = null, ok = true, why = '';
    for (const p of ps) {
      // THE GROUP IS THREE THINGS, not two. Riven rows became their own block
      // on 2026-08-24 — a rank only means something inside one — and a key that
      // stopped at the mode reported the two rankings interleaved, which is
      // exactly what a reader would see if the page had not grouped them.
      const k = p.benchmark + '#' + p.mode + '#' + (p.riven ? 'r' : 'p');
      if (k !== last) { blocks.push(k); last = k; }
      // WHAT A RANK MEANS: #1 is that mode's leader. Counting positions would
      // be vacuous — builtinBuilds numbers the rows as it walks them, so a
      // position counter and its rank agree however the list is ordered. The
      // falsifiable claim is that the ORDER is by score inside the mode.
      const sc = ((p.board || {}).score) || 0;
      if (prev[k] !== undefined && sc > prev[k] + 1e-12) {
        ok = false;
        why = k + ': #' + p.rank + ' scores ' + sc + ', above the row before it (' + prev[k] + ')';
        break;
      }
      prev[k] = sc;
    }
    out.weapons.push({
      id: w.id, rows: ps.length,
      blocks: blocks.length, groups: new Set(blocks).size,
      bestFirst: ok, why,
    });
  }
  // THE WORST-INTERLEAVED ONE for the DOM half, not the one with the most
  // rows. Picking by depth chose the Torid, whose modes happen to be
  // contiguous already — so the DOM half passed on the broken build while nine
  // weapons failed beside it, which is a check reporting the wrong thing. On a
  // healthy board every weapon ties at zero and this falls back to depth.
  const deep = out.weapons.slice().sort((a, b) =>
    (b.blocks - b.groups) - (a.blocks - a.groups) || b.rows - a.rows)[0];
  out.drawn = deep ? deep.id : null;
  if (deep) {
    const w = (META.weapons || []).find(x => x.id === deep.id);
    history.pushState({}, '', '/weapons/' + wikiSlug(w)); route(); await s(3200);
    // BY PREFIX, not by the domain's name: the row picker is the second of the
    // benchmark bar's two dropdowns and the only one whose id starts this way.
    const btn = document.querySelector('[id^="dd-bench-row-"]');
    out.hasPicker = !!btn;
    if (btn) {
      btn.click(); await s(400);
      const menu = document.getElementById('dd-menu');
      out.headers = [...(menu ? menu.children : [])]
        .filter(el => el.className.indexOf('ddgroup') >= 0)
        .map(el => el.textContent.trim());
    }
  }
  return out;
})()`);

// A BOARD WITH NO TWO-MODE WEAPON IS A REAL STATE, and it is reported rather
// than passing in silence — the injection above means it should not happen,
// so a zero here is a fault in the fixture and not in the picker.
check("some weapon is on a board in more than one mode",
  pick.weapons.length > 0, pick.weapons.map((w) => w.id).join(", "));
for (const w of pick.weapons) {
  check(`[${w.id}] its modes are contiguous — one block per mode, not more`,
    w.blocks === w.groups, `${w.blocks} blocks for ${w.groups} modes`);
  check(`[${w.id}] ...and #1 is that mode's leader, best first`,
    w.bestFirst, w.why);
}
// …AND THE PICKER DRAWS IT. The order above is invisible until something
// renders a header from it, and the header is the only thing on screen that
// says what a rank is a rank ON.
check(`[${pick.drawn}] the picker offers its rows`, pick.hasPicker !== false,
  String(pick.hasPicker));
check(`[${pick.drawn}] ...drawing each mode's header exactly once`,
  !!pick.headers && pick.headers.length > 0
    && pick.headers.length === new Set(pick.headers).size,
  JSON.stringify(pick.headers));

await app.finish("a board row opens the build it is about, under the ruler it is on");
