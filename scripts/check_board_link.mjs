// The SIXTEENTH check: A BOARD ROW OPENS THAT ROW.
//
// The board page lists one line per weapon per ruler, and clicking one is the
// main way anybody arrives at a build. The failure it exists for: a link that
// carried the weapon and the mode and NOT the ruler, where both boards call
// their leader "#1 · Incarnon cycle" — so the no-aim leader opened the aimed
// board's leader, under the aimed board's fight, and re-running it matched
// neither line on either board.
//
// Two halves, because a row is two things:
//   · the BUILD — mods, arcane, evolutions, the mode it was played in
//   · the FIGHT — the ruler it was measured under, without which the number on
//     the line cannot be reproduced on the page it links to
//
// Asserted against `BOARD` itself rather than against a build written down
// here, so it keeps holding as the board moves under it.
//
// A THIRD concern, one level down: the board page lists a weapon's BEST row
// per mode and the deeper ranks live in the builder's picker, where a rank only
// means something inside one way of playing. See the section at the bottom.
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
  // bug that made this check exist, one level down.
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
  // one-mod difference and blamed the link.
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
  // the owner reported was this: a riven row's link opened the
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
// the builder's own picker, where the mode is a CONTROL of its own and the row
// list under it is one ranking. The failure: `builtinBuilds` sorted by ruler
// only and inherited board.json's order, which is by SCORE and knows nothing
// about modes — so two modes arrived interleaved and the ranks restarted
// mid-list, live on nine weapons when it was found.
//
// AND THE STRONGEST GROUP LEADS, by its best row: which mode wins here is the
// question the control exists to answer.
//
// TWO HALVES, because either alone passes on a page that lost the other. The
// ORDER is asserted over every weapon the board holds in more than one mode —
// a property, not a weapon — and the CONTROL is asserted once in the DOM,
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
    // THE BEST ROW OF EACH BLOCK, in the order the mode control is drawn in.
    const top = {};
    let last = null, ok = true, why = '';
    for (const p of ps) {
      // THE GROUP IS THREE THINGS, not two. Riven rows became their own block
      // on 2026-08-24 — a rank only means something inside one — and a key that
      // stopped at the mode reported the two rankings interleaved, which is
      // exactly what a reader would see if the page had not split them.
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
      if (top[k] === undefined) top[k] = sc;
    }
    // …AND THE BLOCKS THEMSELVES ARE STRONGEST-FIRST, compared by their
    // leaders — two rankings of different depth compare no other way.
    //
    // ONE LEVEL AT A TIME, because each level is a CONTROL: a ruler's modes are
    // ordered against each other and the two kinds INSIDE a mode against each
    // other. Flat this is false by design — a mode's riven-less ranking sits
    // under its own riven one and over nothing else.
    let led = true, ledWhy = '';
    const leadOf = {};   // a mode's leader is the first block of it in order
    for (const k of blocks) {
      const m = k.split('#').slice(0, 2).join('#');
      if (leadOf[m] === undefined) leadOf[m] = top[k];
    }
    const descends = (seq, val, label) => {
      for (let i = 1; i < seq.length && led; i++) {
        if (val(seq[i]) > val(seq[i - 1]) + 1e-12) {
          led = false;
          ledWhy = label + ': ' + seq[i] + ' leads with ' + val(seq[i])
            + ', above ' + seq[i - 1] + ' (' + val(seq[i - 1]) + ') before it';
        }
      }
    };
    const grouped = (keys, of) => {
      const g = {};
      for (const k of keys) (g[of(k)] = g[of(k)] || []).push(k);
      return g;
    };
    const modeKey = (k) => k.split('#').slice(0, 2).join('#');
    for (const [r, ms] of Object.entries(grouped(Object.keys(leadOf), m => m.split('#')[0]))) {
      descends(ms, (m) => leadOf[m], 'ruler ' + r);
    }
    for (const [m, ks] of Object.entries(grouped(blocks, modeKey))) {
      descends(ks, (k) => top[k], 'mode ' + m);
    }
    out.weapons.push({
      id: w.id, rows: ps.length,
      blocks: blocks.length, groups: new Set(blocks).size,
      bestFirst: ok, why, strongestFirst: led, ledWhy,
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
    // BY PREFIX, not by the domain's name: each control is the only one whose
    // id starts this way.
    const btn = document.querySelector('[id^="dd-bench-row-"]');
    out.hasPicker = !!btn;
    const mbtn = document.querySelector('[id^="dd-bench-mode-"]');
    out.hasModes = !!mbtn;
    if (mbtn) {
      mbtn.click(); await s(400);
      const menu = document.getElementById('dd-menu');
      out.modeOpts = [...menu.querySelectorAll('.opt[data-v]')].map(el => el.dataset.v);
      out.modeShown = mbtn.value;
      document.getElementById('dd-popover').hidden = true;
      // THE RULER THE BAR IS ON, off its own first control (a shared suffix).
      const onRuler = (document.getElementById(
        'dd-bench-' + mbtn.id.slice('dd-bench-mode-'.length)) || {}).value;
      out.onRuler = onRuler;
      out.wantModes = [...new Set(builtinBuilds()
        .filter(p => p.benchmark === onRuler).map(p => p.mode))];
    }
    if (btn) {
      btn.click(); await s(400);
      const menu = document.getElementById('dd-menu');
      out.groups = [...menu.children]
        .filter(el => el.className.indexOf('ddgroup') >= 0).length;
      out.rowLabels = [...menu.querySelectorAll('.opt .mn')].map(el => el.textContent.trim());
      document.getElementById('dd-popover').hidden = true;
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
  check(`[${w.id}] ...and the strongest way of playing it leads`,
    w.strongestFirst, w.ledWhy);
}
// …AND THE PICKER DRAWS IT. The order above is invisible until something
// renders a control from it, and that control is the only thing on screen
// saying what a rank is a rank ON.
check(`[${pick.drawn}] the picker offers its rows`, pick.hasPicker !== false,
  String(pick.hasPicker));
check(`[${pick.drawn}] ...and a control of its own for the mode`,
  pick.hasModes === true, String(pick.hasModes));
check(`[${pick.drawn}] ...offering each mode exactly once`,
  !!pick.modeOpts && pick.modeOpts.length > 1
    && pick.modeOpts.length === new Set(pick.modeOpts).size,
  JSON.stringify(pick.modeOpts));
// IN THE ORDER `builtinBuilds` PUT THEM IN, asserted above to be strongest first.
check(`[${pick.drawn}] ...in the board's own order, strongest first`,
  JSON.stringify(pick.modeOpts) === JSON.stringify(pick.wantModes),
  `${JSON.stringify(pick.modeOpts)} vs ${JSON.stringify(pick.wantModes)}`);
check(`[${pick.drawn}] ...and it opens on that one`,
  pick.modeShown === (pick.modeOpts || [])[0],
  `${pick.modeShown} vs ${JSON.stringify(pick.modeOpts)}`);
// A ROW IS A NUMBER AGAIN: the control beside it answers the mode.
check(`[${pick.drawn}] ...and its rows are bare ranks, ungrouped`,
  pick.groups === 0 && (pick.rowLabels || []).length > 0
    && pick.rowLabels.every((t) => /^#\d+$/.test(t)),
  `${pick.groups} headers, ${JSON.stringify((pick.rowLabels || []).slice(0, 4))}`);

// ---- and the page you land on starts at the top ----------------------------
//
// THE SAME GESTURE'S OTHER HALF. Everything above asks WHERE a click on a row
// takes you; this asks what you see when you get there. `pushState` does not
// touch the scroll position, so a click from halfway down a 500-row board
// landed on a weapon page already scrolled into the middle of a panel nobody
// chose — every in-app link had it.
//
// A REAL CLICK, not `pushState`: the fix lives in `nav()`, which is what a link
// click goes through and what the rest of this file deliberately bypasses.
{
  const sc = await evaluate(`(async () => {
    const s = (ms) => new Promise(r => setTimeout(r, ms));
    const out = {};
    history.pushState({}, '', '/benchmark'); route(); await s(3000);
    window.scrollTo(0, document.documentElement.scrollHeight); await s(400);
    out.before = Math.round(window.scrollY);
    const a = document.querySelector('.bench-rows .brow');
    out.href = a ? a.getAttribute('href') : null;
    if (a) { a.click(); await s(2600); }
    out.after = Math.round(window.scrollY);
    out.path = location.pathname;
    // BACK KEEPS ITS PLACE. popstate never reaches nav(), so the browser's own
    // restoration stands - which is why history.scrollRestoration is left alone
    // rather than turned off.
    history.back(); await s(2200);
    out.backPath = location.pathname;
    out.backScroll = Math.round(window.scrollY);
    // ...and a RE-RENDER is not a navigation. route() is called on its own for
    // a language switch, a share import and every check in this directory.
    window.scrollTo(0, 400); await s(300);
    route(); await s(900);
    out.afterRoute = Math.round(window.scrollY);
    return out;
  })()`);
  check("the board was scrolled before the click", sc.before > 200, String(sc.before));
  check("...and the weapon page it opens starts at the top",
    sc.after === 0 && String(sc.path || "").indexOf("/weapons/") === 0,
    `${sc.after} on ${sc.path}`);
  check("...while BACK returns to where the reader was",
    sc.backPath === "/benchmark" && Math.abs(sc.backScroll - sc.before) < 40,
    `${sc.backScroll} vs ${sc.before} on ${sc.backPath}`);
  check("...and a re-render is not a navigation", sc.afterRoute === 400,
    String(sc.afterRoute));
}

await app.finish("a board row opens the build it is about, under the ruler it is on");
