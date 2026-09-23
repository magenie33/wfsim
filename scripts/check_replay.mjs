// THE REPLAY, driven in a browser: the benchmark fight plays back, and the
// average above it holds still.
//
// It exists because a replay that shows the WRONG fight is worse than none —
// the engine's own test proves the run is reproduced bit-for-bit, and this
// proves the page shows it: the curve is drawn, the pools drain as the cursor
// moves, and pressing play advances the clock at the chosen multiplier.
//
//   node scripts/check_replay.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/Cernos_Prime'); route(); await sleep(3000);
  ['primed_cryo_rounds','serration','point_strike','vital_sense'].forEach((m,i)=>{
    if (modById(m)) { slots[i].mod=m; slots[i].rank=modById(m).max_rank; }});
  arcanes=['primary_frostbite'];
  sim.level=300; sim.steel_path=true; sim.duration=60; sim.runs=8;
  markPresetDirty(); markScenarioDirty(); renderMods(); refreshPanel(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(x=>{ if(/Sim/i.test(x.textContent)) x.click(); });
  await sleep(1200);
  document.getElementById('run-sim').click();
  for (let k=0;k<40 && !document.getElementById('rp-scrub'); k++) await sleep(1000);
  if (!document.getElementById('rp-scrub')) {
    return { fail: true, resultsHtml: (document.getElementById('sim-results')||{}).innerHTML?.slice(0,300) };
  }
  // THE BUFF SIDE ONLY. The target debuffs draw with the same component and the
  // same class since 2026-08-11 — they are told apart by which side of the
  // fight they came from, which is what data-buff says.
  const rp0 = replayState.data;
  const rows=[...document.querySelectorAll('.rp-row[data-buff]')].map(e=>({
    name:e.querySelector('.rp-name').textContent,
    stat:e.querySelector('.rp-stat').textContent,
    now:e.querySelector('.rp-now').textContent,
    open:!e.querySelector('.rp-chart').hidden,
    pts:e.querySelector('.rp-line').getAttribute('points').split(' ').length }));
  // The panel as the FINISHED fight, which is where a replay opens.
  const read = () => ({
    kpi: Object.fromEntries([...document.querySelectorAll('[data-kpi]')].map(e=>[e.dataset.kpi, e.textContent])),
    meter: [...document.querySelectorAll('#sim-results .mrow[data-mk]:not(.sub)')].map(e=>e.querySelector('.mval').textContent.trim()),
    // The BAR beside each figure, as the inline width the page wrote — the
    // block is un-laid-out here, so a measured width is zero whatever it says.
    bars: [...document.querySelectorAll('#sim-results .mrow[data-mk]:not(.sub)')].map(e=>parseFloat(((e.querySelector('.mbar i')||{}).style||{}).width) || 0),
    // The TYPE composition, read off the legend — the shares the segments are
    // drawn from, keyed by the type each line belongs to.
    types: [...document.querySelectorAll('#sim-results .legend .li[data-dk]')].map(e=>e.dataset.dk+':'+e.querySelector('.lv').textContent),
    // TWO ROWS, and which is which is the point: the top one is the FIGHT
    // (damage, kills) and the pools sit with the bodies, whose they are.
    // The rail fills behind the thumb, so the played fraction is readable as
    // one inline width rather than out of the range input's paint.
    done: (((document.getElementById('rp-done')||{}).style)||{}).width || '',
    pools: [...document.querySelectorAll('#rp-pools .rp-cell b')].map(e=>e.textContent).join('|'),
    foePools: [...document.querySelectorAll('#rp-foe-pools .rp-cell b')].map(e=>e.textContent).join('|'),
    hero: document.querySelector('[data-hero]').textContent,
    mean: document.querySelector('#sim-results .hero-num').textContent,
  });
  const atEnd = read();
  // THE LIVE STACK COUNTS AS THE PANEL OPENS, before anything is scrubbed —
  // the other half of "it opens on the finished fight".
  const nowAtOpen = [...document.querySelectorAll('.rp-now')].map(e => e.textContent);
  // ...and the replay BELOW everything it drives.
  const res = document.querySelector('#sim-results .results');
  const kids = [...res.children].map(e=>e.tagName+'.'+(e.className||''));
  // ORDER IN THE DOCUMENT, not among the direct children: every block folds
  // now, so most of them sit one level down inside a fold wrapper. What is
  // being asserted is where things READ, and that is document order.
  const pos = (sel) => {
    const el = res.querySelector(sel);
    if (!el) return -1;
    const all = [...res.querySelectorAll('*')];
    return all.indexOf(el);
  };
  // WHAT THE RAIL SAYS HAPPENED, against the series it is derived from. The
  // marks are frames where a cumulative counter went up, thinned to 0.3% apart
  // — so the assertion recomputes that here rather than trusting a count.
  const rises = (sr) => {
    const n = rp0.t.length - 1;
    let k = 0, prev = -0.3;
    for (let i = 1; i <= n; i++) {
      if (!((sr[i] || 0) > (sr[i - 1] || 0))) continue;
      const p = (i / n) * 100;
      if (p - prev < 0.3) continue;
      k++; prev = p;
    }
    return k;
  };
  const rail = {
    killMarks: document.querySelectorAll('.scrub-kill').length,
    reloadMarks: document.querySelectorAll('.scrub-reload').length,
    killRises: rises(rp0.kills || []),
    reloadRises: rises((rp0.kpi || {}).reloads || []),
    legend: document.querySelectorAll('.scrub-legend .sl').length,
    // The PLATFORM thumb switched off is what lets the page draw its own;
    // a pseudo-element's computed style is not readable, but this is.
    appearance: getComputedStyle(document.getElementById('rp-scrub')).appearance,
    railBg: getComputedStyle(document.querySelector('.scrub-rail')).backgroundColor,
  };
  // THE TWO CUTS OF ONE FIGHT: who dealt it and who took it. Both are booked
  // through one door in the engine (ledger::settle), so they answer with one
  // number or one of them is counting something the other is not.
  // (No backticks in this comment: it lives inside a template literal.)
  const cuts = {
    attackers: (shownResult.r.attackers || []).map(a => a.id),
    dealt: (shownResult.r.attackers || []).reduce((a, x) => a + (x.damage || 0), 0),
    taken: (shownResult.r.bodies || []).reduce((a, x) => a + (x.damage || 0), 0),
    rows: [...document.querySelectorAll('.mrow[data-attacker]')].map(e => e.dataset.attacker),
    recRoster: ((recordState || {}).attackers || []).length,
    tagged: [...document.querySelectorAll('.rec-peek tr[data-attacker]')].length,
    untagged: [...document.querySelectorAll('.rec-peek tr.rec-dmg')]
      .filter(e => !e.dataset.attacker).length,
    whoColumns: document.querySelectorAll('.rec-who').length,
  };
  const iBar = pos('.rp-bar');
  const iMeter = pos('.meter');
  // THE DPS CURVE, the anchor both order assertions read against: the replay
  // bar sits above it and the buff curves below. pos() answers -1 for a block
  // the panel does not draw, so an anchor has to be one the results carry.
  const iChart = pos('.tl-wrap');
  const iRow = pos('.rp-row');

  // Rewind to the very start: the panel must read as a fight that has not
  // happened yet.
  const sc=document.getElementById('rp-scrub');
  // HALF WAY FIRST, which is where a fixed scale is visible at all: at both
  // ends every scheme agrees, and only the middle says whether the bars grow.
  sc.value=Math.floor(Number(sc.max)/2); sc.dispatchEvent(new Event('input')); await sleep(300);
  const atMid = read();
  sc.value=0; sc.dispatchEvent(new Event('input')); await sleep(300);
  const atZero = read();
  // ...and back to the end restores it exactly.
  sc.value=sc.max; sc.dispatchEvent(new Event('input')); await sleep(300);
  const restored = read();
  const nowAtEnd=[...document.querySelectorAll('.rp-now')].map(e=>e.textContent);

  // STOP, FROM THE MIDDLE OF A PLAYBACK: it ends the playback and lands the
  // panel back on the finished fight, which is the state it opens in.
  sc.value=0; sc.dispatchEvent(new Event('input')); await sleep(200);
  document.getElementById('rp-play').click(); await sleep(600);
  document.getElementById('rp-stop').click(); await sleep(400);
  const afterStop = read();
  const playLabel = document.getElementById('rp-play').textContent.trim();

  sc.value=0; sc.dispatchEvent(new Event('input')); await sleep(200);
  document.getElementById('rp-play').click(); await sleep(1500);
  const movedTo = Number(document.getElementById('rp-scrub').value);
  document.getElementById('rp-play').click();
  // COLLAPSE, then expand, then collapse.
  //
  // Measured as the RESOLVED display, not as the hidden attribute and not as a
  // height. The attribute is what hid this bug for so long: it was set
  // correctly the whole time and changed nothing, because .mrow is
  // display:grid and an author rule beats the UA's [hidden] rule.
  // Height would be the most honest measure and cannot be used here — this
  // check ends with the results block un-laid-out (the on-simulator body class
  // is off), so everything inside it measures zero whatever it is doing.
  // (No backticks in this comment: it lives inside a template literal.)
  const head0 = document.querySelector('.mrow.exp[data-mk]');
  const sub0 = document.querySelector('.mrow.sub[data-mk]');
  const disp = () => getComputedStyle(sub0).display;
  const collapse = { start: disp() };
  head0.click(); await sleep(350); collapse.toggled = disp();
  head0.click(); await sleep(350); collapse.back = disp();

  // THE DAMAGE METER'S COLOURS, gathered with every expandable source open so
  // the same damage TYPE appears under more than one of them.
  document.querySelectorAll('.mrow.exp').forEach(e => e.click());
  await sleep(700);
  // THE DAMAGE METER'S ROWS, and only those. The zone draws more than one
  // meter now — the attacker cut is the same component with a roster id
  // instead of a damage key — so a bare .mrow sweeps in rows that are not
  // about a damage type and asks them for a damage type's glyph.
  const meterRows = [...document.querySelectorAll('.mrow[data-mk]')].map(el => {
    const bar = el.querySelector('.mbar i');
    return {
      key: el.getAttribute('data-mk') || '',
      // The RESOLVED colour, not the var() name — a variable that resolves to
      // nothing would still compare equal to itself.
      color: bar ? getComputedStyle(bar).backgroundColor : '',
      icon: !!el.querySelector('.dt-ico'),
      sub: el.classList.contains('sub'),
    };
  });
  // THE COMPOSITION BAR, and the meter it has to agree with. Read as SHARES
  // (flex-grow), because that is what the bar is drawn from — measuring pixel
  // widths would test the layout engine instead.
  const segs = [...document.querySelectorAll('.dmg-bar .dmg-seg')]
    .map(e => ({ share: parseFloat(getComputedStyle(e).flexGrow),
                 color: getComputedStyle(e).backgroundColor }));
  const legend = [...document.querySelectorAll('.legend .li')]
    .map(e => ({ text: e.textContent.trim(), icon: !!e.querySelector('.dt-ico') }));
  // What the METER says each type totalled, to reconcile against.
  const meterByType = {};
  for (const el of document.querySelectorAll('.mrow[data-mk]')) {
    const k = el.getAttribute('data-mk') || '';
    const ty = (k.split('::')[1] || k).toLowerCase();
    if (['direct','radial','field','arcane','syndicate'].includes(ty)) continue;
    if (!k.includes('::') && el.classList.contains('sub')) continue;
    const v = parseFloat((el.querySelector('.mval')||{}).textContent?.replace(/[^\d.]/g,'') || '0');
    meterByType[ty] = (meterByType[ty] || 0) + v;
  }
  return { rows, atEnd, atMid, atZero, restored, afterStop, playLabel, rail, cuts, nowAtOpen, nowAtEnd, movedTo, iBar, iMeter, iChart, iRow, kids,
           meterRows, segs, legend, collapse, meterTypes: Object.keys(meterByType).sort(),
           clock: document.getElementById('rp-clock').textContent };
})()`);
if (r.fail) { console.log("FAIL  no replay section — sim-results:", r.resultsHtml); process.exit(1); }
check("one row per buff, drawn and open by default",
  r.rows.length === 1 && r.rows[0].open && r.rows[0].pts === 600, JSON.stringify(r.rows[0]));
// Language-agnostic: this check runs in whatever locale the browser picks, so
// it asserts the FIGURES (mean out of max, a percentage, a ramp time) rather
// than the words around them.
// THE RAMP IS THE THIRD FIGURE and it is CONDITIONAL: a run that never fills
// the bar has no ramp to state, and the header says so in words instead
// ("not full"). Asserting a ramp time unconditionally made this a test of the
// median run's LUCK — it passed for months and moved the day the seed scheme
// did, with nothing about the header changed.
check("the header states average, uptime and either the ramp or why there is none",
  /[\d.]+\/40/.test(r.rows[0].stat) && /\d+%/.test(r.rows[0].stat) &&
  (/[\d.]+s/.test(r.rows[0].stat) || /full|满层/.test(r.rows[0].stat)), r.rows[0].stat);
// THE METER IS COLOURED BY DAMAGE TYPE, NOT BY ROW POSITION. A colour from
// `(i % 8) + 1` makes the same element one colour under a direct hit and
// another under a lingering field — and neither is the element's. DE publishes
// a colour per type
// (`Module:DamageTypes/data`), and the point of using it is that it is
// the SAME everywhere.
//
// Asserted on the RESOLVED colour: a `var()` that resolved to nothing would
// still equal itself, so comparing the declarations would pass on a missing
// palette.
{
  const byType = {};
  for (const row of r.meterRows) {
    const ty = (row.key.split("::")[1] || row.key).toLowerCase();
    if (!row.sub && !row.key.includes("::") && ["direct","radial","field","arcane","syndicate"].includes(ty)) continue;
    (byType[ty] ||= []).push(row);
  }
  const shared = Object.entries(byType).filter(([, v]) => v.length > 1);
  check("a damage type has ONE colour wherever it appears",
    shared.length > 0 && shared.every(([, v]) => new Set(v.map((x) => x.color)).size === 1),
    JSON.stringify(shared.map(([k, v]) => [k, v.map((x) => x.color)])));
  // ...and it is a real colour, not an unresolved variable falling back to
  // transparent.
  check("...and that colour actually resolves",
    Object.values(byType).flat().every((x) => /^rgba?\(/.test(x.color) && x.color !== "rgba(0, 0, 0, 0)"),
    JSON.stringify(Object.values(byType).flat().map((x) => x.color).slice(0, 6)));
  check("every damage-type row carries DE's own glyph",
    Object.values(byType).flat().every((x) => x.icon),
    JSON.stringify(Object.entries(byType).map(([k, v]) => [k, v.every((x) => x.icon)])));
}

// EXPANDING A SOURCE ACTUALLY SHOWS AND HIDES IT. The
// handler was always correct — it toggled the attribute and flipped the
// caret — and nothing happened, because `.mrow` is `display:grid` and an
// author rule beats the UA's `[hidden]{display:none}`. So the rows were
// permanently expanded and the caret lied about it.
//
// Asserted on the rendered HEIGHT, not on the attribute: reading `.hidden`
// back is what made this invisible for so long, since the attribute was right
// the whole time.
check("a meter source expands and collapses for real",
  r.collapse.start === "none" && r.collapse.toggled === "grid" && r.collapse.back === "none",
  JSON.stringify(r.collapse));

// THE COMPOSITION BAR — the same damage counted a second way. The meter answers where damage came FROM; this answers what
// it was MADE OF, and the two are the same total, so the shares must come to
// one and cover the same types the meter listed.
//
// Aggregated in the page from the meter's own rows precisely so it cannot
// drift; this asserts that it did not.
{
  const sum = r.segs.reduce((a, x) => a + x.share, 0);
  check("the type composition covers the whole engagement",
    r.segs.length > 1 && Math.abs(sum - 1) < 0.01, `${r.segs.length} segments summing to ${sum}`);
  check("...one legend entry per segment, each with its glyph",
    r.legend.length === r.segs.length && r.legend.every((l) => l.icon),
    JSON.stringify(r.legend));
  // Same colours as the meter: one palette, keyed on the type, used twice.
  const meterColors = new Set(r.meterRows.filter((x) => x.sub).map((x) => x.color));
  check("...and its colours are the meter's",
    r.segs.every((x) => meterColors.has(x.color)),
    JSON.stringify([r.segs.map((x) => x.color), [...meterColors]]));
}

check("the replay BAR sits above everything it drives",
  r.iBar >= 0 && r.iBar < r.iMeter && r.iBar < r.iChart, JSON.stringify([r.iBar, r.iMeter, r.iChart, r.iRow]));
check("...and the buff CURVES stay down with the other chart",
  r.iRow > r.iMeter && r.iRow > r.iChart, JSON.stringify([r.iBar, r.iMeter, r.iChart, r.iRow]));
// IT OPENS ON THE FINISHED FIGHT — the cursor is at the LAST frame, which is
// what this is about. Asserting "40/40" adds the claim that the buff happened
// to fill in this particular run: two facts in one assertion, and only one of
// them is the panel's doing.
check("it opens on the finished fight",
  r.nowAtOpen.length > 0 && r.nowAtOpen.join() === r.nowAtEnd.join(),
  `${r.nowAtOpen} vs ${r.nowAtEnd}`);
// THE METER'S BARS ARE ONE FIXED SCALE — the biggest source at the END. A bar
// therefore only grows, and the composition fills in as the fight runs.
// Rescaling each frame against that frame's own leader kept the top bar full
// from the first shot to the last, so the chart said the same thing at every
// instant and only the text moved. Asserted in the MIDDLE, because at both
// ends the two schemes agree.
check("the meter's bars grow with the playhead",
  Math.max(...r.atEnd.bars) === 100 && Math.max(...r.atZero.bars) === 0 &&
  Math.max(...r.atMid.bars) > 0 && Math.max(...r.atMid.bars) < 100,
  JSON.stringify([r.atZero.bars, r.atMid.bars, r.atEnd.bars]));
// THE TYPE BAR FOLLOWS TOO. It is the meter's damage counted a second way, so
// one of them frozen on the finished fight while the other walks the fight is
// two answers to one question. The ORDER holds — the lines are the ones the
// panel drew — and the shares are what move.
check("the type composition follows the playhead",
  r.atEnd.types.length > 1 &&
  r.atEnd.types.map((x) => x.split(':')[0]).join() === r.atMid.types.map((x) => x.split(':')[0]).join() &&
  r.atMid.types.join() !== r.atEnd.types.join(),
  JSON.stringify([r.atMid.types, r.atEnd.types]));
// THE RAIL CARRIES THE FIGHT'S OWN EVENTS, and they are DERIVED rather than
// shipped: a mark is a frame where a cumulative counter went up. Asserted
// against a recomputation of that rule, so it holds whatever this fight does —
// a run that kills nothing draws no kill marks and is still correct.
check("the scrubber's marks are the fight's own events",
  r.rail.killMarks === r.rail.killRises && r.rail.reloadMarks === r.rail.reloadRises
  && (r.rail.killMarks + r.rail.reloadMarks) > 0,
  JSON.stringify(r.rail));
// ...and the key names only the marks that are on the rail. A legend entry for
// a mark this fight never drew is a key to nothing.
check("...and the key names only the marks it drew",
  r.rail.legend === (r.rail.killMarks > 0 ? 1 : 0) + (r.rail.reloadMarks > 0 ? 1 : 0),
  JSON.stringify(r.rail));
// THE THUMB IS THE PAGE'S, not the platform's: a bare range input paints a
// browser thumb that ignores the theme, and the rail underneath it would be
// the only styled half.
check("the scrubber's thumb is the page's own",
  r.rail.appearance === "none" && /^rgba?\(/.test(r.rail.railBg)
    && r.rail.railBg !== "rgba(0, 0, 0, 0)",
  `${r.rail.appearance} · ${r.rail.railBg}`);
// THE RAIL FILLS BEHIND IT, from nothing to the whole width. Read as a NUMBER:
// the browser normalises "0.00%" to "0%", so comparing the string would assert
// the serialiser rather than the fill.
{
  const w = (x) => parseFloat(x);
  check("the rail fills behind the playhead",
    w(r.atZero.done) === 0 && w(r.atEnd.done) === 100
    && w(r.atMid.done) > 20 && w(r.atMid.done) < 80,
    `${r.atZero.done} -> ${r.atMid.done} -> ${r.atEnd.done}`);
}
// ONE FIGHT, TWO CUTS: who DEALT the damage and who TOOK it. Both are booked
// through the same door in the engine, so they come to one number — any drift
// at all means a damage site reached a total without naming one of the two.
{
  const c = r.cuts;
  check("the attacker cut and the body cut are the same total",
    c.dealt > 0 && Math.abs(c.dealt - c.taken) < 1,
    `dealt ${c.dealt} vs taken ${c.taken}`);
  // …AND THE PANEL DRAWS THE ROSTER, whatever its length. One attacker is a
  // list of one, not a hidden block: the markup that draws a squad is the
  // markup that draws you alone.
  check("...and the panel draws a row for every seat",
    c.rows.length === c.attackers.length && c.rows.join() === c.attackers.join(),
    JSON.stringify([c.rows, c.attackers]));
  // EVERY ROW NAMES ITS DEALER even where nothing would be drawn from it. The
  // attribution is the ledger's; the visible column is the page's, and it
  // follows the same rule the foe chips do — a control with one option is not
  // a control.
  check("every record row names who dealt it, and the column waits for a second",
    c.recRoster > 0 && c.tagged > 0 && c.untagged === 0
    && c.whoColumns === (c.recRoster > 1 ? c.tagged : 0),
    JSON.stringify(c));
}
check("rewinding empties the KPIs and the meter",
  r.atZero.kpi.shots === "0" && r.atZero.kpi.procs === "0" &&
  r.atZero.meter.every((v) => /^0 /.test(v)),
  JSON.stringify(r.atZero));
check("...and the pools go back to full",
  r.atZero.foePools !== r.atEnd.foePools && r.atZero.foePools.startsWith("659,445"), r.atZero.foePools);
// THE TOP ROW IS THE FIGHT'S. Damage and kills are true of a crowd; a pool
// belongs to ONE body, and a body's number on the fight's own row reads as the
// crowd's. So the pools sit with the bodies, under the heading that names whose.
check("the fight's row carries no single body's pools",
  r.atEnd.pools.split("|").length === 2 && r.atEnd.foePools.split("|").length === 3,
  `${r.atEnd.pools} :: ${r.atEnd.foePools}`);
// ...AND STOP IS ONE CLICK BACK TO THE ANSWER. Pause leaves the playhead where
// it stands; this is the state the panel opens in, and before the button the
// only way there was dragging the scrubber to its far end.
check("stop ends the playback on the finished fight",
  JSON.stringify(r.afterStop) === JSON.stringify(r.atEnd) && /^▶/.test(r.playLabel),
  `${r.playLabel} · ${JSON.stringify(r.afterStop)}`);
check("the benchmark fight's headline follows too", r.atZero.hero !== r.atEnd.hero,
  r.atEnd.hero + " -> " + r.atZero.hero);
// THE AVERAGE IS EVERY RUN AT ONCE, and a replay of one of them must not
// rewrite it.
check("...while the average above it holds still",
  r.atEnd.mean.length > 0 && r.atZero.mean === r.atEnd.mean,
  r.atEnd.mean + " -> " + r.atZero.mean);
check("the unit sits on the number's line",
  /KPM|DPS/.test(r.atEnd.hero), r.atEnd.hero);
check("returning to the end restores the panel exactly",
  JSON.stringify(r.restored) === JSON.stringify(r.atEnd),
  JSON.stringify(r.atEnd) + " vs " + JSON.stringify(r.restored));
check("play advances the clock at the chosen multiplier",
  r.movedTo > 40 && r.movedTo < 120, "frame " + r.movedTo + " after 1.5s at 5x (expect ~75)");
await app.finish("the whole panel replays");
