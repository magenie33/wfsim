// THE COMBAT RECORD — the FORTIETH check, and the only one that asserts a
// number by DOING ITS ARITHMETIC.
//
// Every other block on the result panel is an aggregate, and an aggregate hides
// an error inside an average: a factor applied twice moves a mean by a few per
// cent and reads as a build being good. A record row is the opposite — it
// carries every factor behind one number, so a reader with a calculator can
// falsify it. This check IS that reader.
//
// WHAT IT WOULD CATCH. A row whose ledger lists a factor the engine did not
// apply, or applies one it does not list, comes out of the multiplication as a
// different number from the one beside it. That is the whole class of bug the
// record exists to expose, and it is invisible in every other output here.
//
// AND IT ASSERTS THE SPLIT, which is the one thing in this app that can be laid
// beside a recording and checked: a hit on a shielded body pops TWO numbers —
// what the shield stopped and what got through it — and the second carries the
// enemy shield gate's ×0.05 on its face (MEASUREMENTS M61).

import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  localStorage.clear();
  history.pushState({}, '', '/weapons/Laetum/simulator'); route(); await sleep(3000);

  // A SCENARIO OF OUR OWN. The official rulers are pinned and their target has
  // no shields — every enemy on the board carries shield 0, which is exactly
  // how the gate went unmodelled for so long.
  const bar = document.querySelector('#preset-bar-simulator-scenarios');
  const add = bar && bar.querySelector('.pchip.add');
  if (add) { add.click(); await sleep(1200); }
  // THE MEASURED FIGHT: a level 1 Crewman is 120 shield / 90 health, and an
  // unmodded Laetum body shot is 160 — enough to break the shield and not
  // enough to make the leak hard to read.
  sim.enemy = 'crewman'; sim.level = 1; sim.steel_path = false; sim.eximus = false;
  sim.duration = 6; sim.runs = 12; sim.headshot_pct = 0;
  sim.formation = []; sim.aim_at = null;
  markScenarioDirty && markScenarioDirty();
  // A BUILD, and the check needs one. Every assertion below passed for a month
  // while the record was being computed for a BLANK build, because the fixture
  // was blank too and the two were indistinguishable — the panel sent the
  // SCENARIO and not the mods, so it explained a fight nobody had run. A damage mod makes the two tell apart.
  slots[0] = { mod: 'hornet_strike', pol: slots[0] && slots[0].pol, rank: null };
  // …AND MULTISHOT, so 'own' and 'multishot' both exist and the filter has
  // something to remove.
  slots[1] = { mod: 'barrel_diffusion', pol: slots[1] && slots[1].pol, rank: null };
  if (typeof renderAll === 'function') renderAll();
  await sleep(600);

  const run = document.querySelector('#run-sim');
  run.click();
  for (let i = 0; i < 90 && !document.querySelector('.rec, #rec-host'); i++) await sleep(400);
  await sleep(800);

  // OPEN IT, then ask for the record. It is fetched rather than carried, so
  // "the block exists" and "the record exists" are two different assertions and
  // this check needs both.
  const fold = document.querySelector('.fold[data-fold="record"]');
  out.block = !!fold;
  if (fold && fold.classList.contains('shut')) { fold.querySelector('.fold-h').click(); await sleep(200); }
  out.mods = (typeof buildPayload === 'function' ? buildPayload().mods : []) || [];
  // WHAT THE REPORT SAID THIS ENGAGEMENT WAS WORTH. The dps it prints is the
  // MEDIAN run's effective damage over the duration, and the record is that
  // same run — so the rows have to add up to it. It is the one assertion that
  // tells "the record explains this report" from "a similar fight".
  out.reportTotal = (shownResult && shownResult.r)
    ? shownResult.r.dps * shownResult.r.duration : 0;

  const load = document.querySelector('#rec-load');
  out.button = !!load;
  if (load) load.click();
  for (let i = 0; i < 60 && !document.querySelector('.rec-t'); i++) await sleep(300);
  await sleep(500);

  const rows = [...document.querySelectorAll('tr.rec-dmg')];
  out.rows = rows.length;
  out.events = document.querySelectorAll('tr.rec-evt').length;

  // ---- THE ARITHMETIC, off the SCREEN ---------------------------------
  // Walk the ledger as DRAWN, layer by layer, and compare with the totals the
  // same row prints. Nothing here consults the engine: if the page draws a
  // ledger that does not produce its own number, that is the bug.
  //
  // IT IS LAYERS NOW, NOT A FLAT PRODUCT, and that is the stronger claim: each
  // layer STATES the running total after it, so a bracket that adds its terms
  // wrongly fails here even when the end of the chain lands right. The shapes
  // are the mechanics — a bracket adds, a snap rounds, a mul multiplies — and
  // a quotient has no shape at all, which is what stops the panel printing
  // x5.706 Condition Overload again.
  const num = (x) => Number(String(x).replace(/[^0-9.-]/g, '').replace(/-(?!^)/g, ''));
  const bad = [];
  let checked = 0;
  let brackets = 0;
  for (const tr of rows) {
    const calc = tr.querySelector('.rec-calc');
    if (!calc) continue;
    const layers = [...calc.querySelectorAll('.lg')];
    if (layers.length < 3) continue;      // base + at least one layer + popped
    const outOf = (l) => num(l.querySelector('.lg-out').textContent);
    let at = outOf(layers[0]);            // the weapon's own base
    let ok = true;
    for (const l of layers.slice(1, -1)) {
      const shown = outOf(l);
      if (l.classList.contains('lg-b')) {
        const terms = [...l.querySelectorAll('.lg-term')]
          .map((t) => num(t.textContent) * (t.textContent.indexOf('−') === 0 ? -1 : 1));
        const printed = num(l.querySelector('.lg-sum').textContent);
        const sum = 1 + terms.reduce((a, b) => a + b, 0);
        if (Math.abs(sum - printed) > 0.02) {
          bad.push('bracket adds to ' + sum.toFixed(3) + ' but prints ' + printed);
          ok = false;
        }
        at *= printed;
        brackets += 1;
      } else if (l.classList.contains('lg-m')) {
        at *= num(l.querySelector('.lg-lbl').textContent);
      } else {
        at = shown;      // a snap is not a multiplier; take what it states
      }
      if (Math.abs(at - shown) > Math.max(1, Math.abs(shown) * 0.01)) {
        bad.push('layer carries ' + at.toFixed(1) + ' but states ' + shown);
        ok = false;
      }
      at = shown;
    }
    if (ok) checked += 1;
  }
  out.checked = checked;
  out.brackets = brackets;
  out.bad = bad.slice(0, 3);
  // …AND NO ROW DRAWS A QUOTIENT WITH A MULTIPLICATION SIGN. On an Adding
  // weapon Condition Overload is a TERM of the base bracket; an x in front of
  // it is the fiction this whole shape exists to make unrepresentable.
  out.coIsATerm = rows.every((tr) =>
    !tr.querySelector('.lg-m [data-factor="Condition Overload bracket"]'));
  // …AND THE SAME ARITHMETIC ON THE WIRE, so a failure says WHICH side is
  // wrong: the engine's ledger, or the way this page drew it.
  out.wireBad = (recordState.events || []).filter((e) => {
    if (e.kind !== 'damage') return false;
    const eff = (e.mitigation || []).reduce((a, [, v]) => a * v, e.raw);
    return Math.abs(eff - e.effective) > Math.max(0.05, Math.abs(e.effective) * 0.005);
  }).slice(0, 2);

  // ---- STATE, PLUS WHAT THIS ROW CHANGED, IS THE NEXT STATE -------------
  //
  // The two state columns said what was TRUE and never why it changed, so a
  // reader could not check one row against the next. A row states what it put
  // on the target (procs) and what it set off on the shooter (triggered),
  // and both are deltas against the state column beside them: the next row's
  // count for a buff this row triggered can never be LOWER.
  //
  // It is one-directional on purpose. A buff can also expire between two rows,
  // so "went up by exactly one" is not the property — "never went down after
  // being triggered" is, and that is what a bump guarantees.
  const dmg = (recordState.events || []).filter((e) => e.kind === 'damage');
  let pairs = 0, fell = 0;
  for (let i = 0; i < dmg.length - 1; i++) {
    for (const k of dmg[i].triggered || []) {
      pairs += 1;
      if (((dmg[i + 1].buffs || [])[k] || [0])[0] < ((dmg[i].buffs || [])[k] || [0])[0]) fell += 1;
    }
  }
  out.triggerPairs = pairs;
  out.triggerFell = fell;
  out.rowsWithTriggers = dmg.filter((e) => (e.triggered || []).length).length;
  // …AND A DURATION IS THE ROW'S OWN CLOCK. The wire carries an ABSOLUTE
  // expiry, so a buff seen on two rows a second apart must read one second
  // shorter — a countdown baked in at application time would read the same.
  const ticking = [];
  for (const e of dmg) {
    for (let k = 0; k < (e.debuffs || []).length; k++) {
      const [n, until] = e.debuffs[k] || [];
      if (n > 0 && typeof until === 'number') ticking.push([e.t, until, k]);
    }
  }
  out.withDuration = ticking.length;
  out.durationsAhead = ticking.every(([t, until]) => until >= t - 1e-6);

  // ---- THE SPLIT ------------------------------------------------------
  // A body hit on a shielded target pops two numbers. They are two ROWS at the
  // same instant, one per pool, and the health one wears the gate.
  const pools = rows.map((tr) => (tr.className.match(/rec-(shield|health|overguard)/) || [])[1]);
  out.shieldRows = pools.filter((p) => p === 'shield').length;
  out.healthRows = pools.filter((p) => p === 'health').length;
  // BY THE FACTOR'S OWN KEY, never by its label. The label is translated and
  // this page defaults to Chinese, so matching the English sentence passed
  // only for as long as the panel was untranslated — and then failed on the
  // day it was translated, which is a check reporting the wrong thing twice. data-factor is the engine's own spelling and does not move.
  const gated = rows.filter((tr) => tr.querySelector('.rec-calc [data-factor="shield gate"]'));
  out.gateRows = gated.length;
  out.gateFactor = gated.length
    ? num(gated[0].querySelector('.rec-f.mit[data-factor="shield gate"]').firstChild.textContent)
    : null;
  // …and the two halves are ONE instant, which is what makes them one hit.
  const t = (tr) => tr.querySelector('.rec-t').firstChild.textContent.trim();
  out.pairedAtOneInstant = gated.length
    ? rows.some((x) => x !== gated[0] && t(x) === t(gated[0]) && /rec-shield/.test(x.className))
    : false;

  // ---- A PELLET THAT WENT NOWHERE IS STILL A ROW -----------------------
  //
  // The one thing the ledger could not say. Three exits in the pellet loop
  // produce no damage at all, so "why did a three-pellet shot pop two numbers"
  // had no answer anywhere — and Kind::Miss sat declared and never emitted
  // for months because of it.
  const ev = recordState.events || [];
  const byId = new Map(ev.map((e) => [e.id, e]));
  const misses = ev.filter((e) => e.kind === 'miss');
  out.misses = misses.length;
  out.missCausesNothing = ev.every((e) => (byId.get(e.cause) || {}).kind !== 'miss');
  out.missesSayWhy = misses.every((e) => !!e.reason);
  // …AND NOTHING ELSE CREPT IN. The event kinds are the four the owner named
  // plus the miss: a shot, a reload's two ends, a transmute's two ends. An
  // ARRIVAL was tried here and taken back out the same day — against a target
  // at contact it is one row per pellet saying "it arrived, 0.00 m", which is
  // half the stream to say nothing.
  out.kinds = [...new Set(ev.map((e) => e.kind))].sort();

  // ---- THE STATE COLUMN IS THE TARGET BEFORE THE HIT -------------------  // ---- THE STATE COLUMN IS THE TARGET BEFORE THE HIT -------------------
  out.statesDrawn = rows.filter((tr) =>
    tr.querySelector('.rec-state [data-pool="health"]')).length;

  // ---- A FILTER SHOWS ONLY WHAT IT NAMES -------------------------------
  // A KIND THE FIXTURE ACTUALLY PRODUCES. Asserting on one it does not is a
  // check that fails for being unlucky rather than for being wrong.
  const want = rows.some((tr) => tr.querySelector('[data-origin="multishot"]'))
    ? 'multishot' : 'own';
  out.want = want;
  const kind = [...document.querySelectorAll('[data-reckind]')].find((b) => b.dataset.reckind === want);
  if (kind) { kind.click(); await sleep(250); }
  const after = [...document.querySelectorAll('tr.rec-dmg')];
  out.filtered = after.length;
  // BY THE ORIGIN'S OWN KEY, for the reason the gate is: the chip's LABEL is
  // translated and reading it back turned a passing check into a failing one
  // the day the panel was.
  out.filteredAllStatus = after.length > 0
    && after.every((tr) => !!tr.querySelector('[data-origin="' + want + '"]'));
  const all = [...document.querySelectorAll('[data-reckind]')].find((b) => b.dataset.reckind === 'all');
  if (all) { all.click(); await sleep(250); }
  out.restored = document.querySelectorAll('tr.rec-dmg').length;

  // ---- IT CAN BE TAKEN AWAY AS TEXT ------------------------------------
  // A record that can only be looked at cannot be DIFFED, and a diff of two
  // records is the thing that says which row moved when a number changes.
  out.copy = !!document.querySelector('#rec-copy');
  // …AND THE SUM OF EVERY ROW, over the WHOLE fight rather than a window.
  out.recordTotal = (recordState.events || [])
    .filter((e) => e.kind === 'damage')
    .reduce((a, e) => a + e.effective, 0);
  return out;
})()`);

const tag = "combat record";
check(`${tag} the block is on the result panel`, r.block, JSON.stringify(r).slice(0, 200));
check(`${tag} it is fetched on demand, not carried`, r.button, "no load control");
check(`${tag} it lists damage instances`, r.rows > 10, `${r.rows} rows`);
check(`${tag} weapon events are rows too`, r.events > 0, `${r.events} events`);

// THE ONE THAT MATTERS: every drawn ledger produces its own number.
check(`${tag} every row multiplies out, layer by layer`,
  r.checked > 5 && r.bad.length === 0,
  `checked ${r.checked}: ${(r.bad || []).join(" | ") || "all consistent"}`);
check(`${tag} ...and the base-damage bracket is drawn as a BRACKET`,
  r.brackets > 0, `${r.brackets} brackets`);
check(`${tag} ...with no quotient wearing a multiplication sign`,
  r.coIsATerm === true, "Condition Overload is a term of it, not a factor");

// The split, and the gate on its face.
check(`${tag} a shielded body pops two numbers`, r.shieldRows > 0 && r.healthRows > 0,
  `shield ${r.shieldRows}, health ${r.healthRows}`);
check(`${tag} the half that got through wears the gate`, r.gateFactor === 0.05,
  `gate factor ${r.gateFactor} on ${r.gateRows} rows`);
check(`${tag} the two halves are one instant`, r.pairedAtOneInstant,
  "the shield row and the health row share a time");

check(`${tag} each row says what the target was before it`, r.statesDrawn === r.rows,
  `${r.statesDrawn} of ${r.rows}`);

// THE STREAM IS THE FOUR THINGS A FIGHT DOES, plus a miss.
check(`${tag} the event kinds are the ones a fight actually has`,
  (r.kinds || []).every((k) => ['damage', 'shot', 'miss', 'reload_start',
    'reload_end', 'transform_start', 'transform_end'].includes(k)),
  (r.kinds || []).join(","));
check(`${tag} a pellet that went nowhere is a row that says why`,
  r.misses === 0 || r.missesSayWhy === true, `${r.misses} misses`);
check(`${tag} ...and causes nothing`, r.missCausesNothing === true);

check(`${tag} a filter shows only what it names`,
  r.filteredAllStatus && r.filtered < r.rows && r.restored === r.rows,
  `${r.want}: ${r.filtered} of ${r.rows}, restored ${r.restored}`);
check(`${tag} the record can be taken away as text`, r.copy, "no copy control");

// THE PAIRING, and it is the whole claim this panel makes: the rows are THIS
// report's own engagement, not a similar one. Anything that reaches the engine
// for the run and not for the record — a mod, a mode, an evolution, a riven —
// moves this sum and nothing else on the page would notice.
check(`${tag} the build reached it`, r.mods.length > 0, JSON.stringify(r.mods));
check(`${tag} the rows add up to the report's own engagement`,
  r.reportTotal > 0 && Math.abs(r.recordTotal - r.reportTotal) / r.reportTotal < 0.01,
  `record ${Math.round(r.recordTotal).toLocaleString()} vs report ${Math.round(r.reportTotal).toLocaleString()}`);

// ---- AND A MISS HAS TO HAPPEN FOR ANY OF THAT TO MEAN ANYTHING ------------
//
// Every assertion about misses above passes perfectly on a fight that has none,
// which is the shape of a vacuous check this repo has been caught by before.
// So a pellet is MADE to miss: the target is pushed out to 40 m, where the
// cone is wider than a body, and the same claims are asked of a run that
// actually produces them.
const m = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  // FAR ENOUGH THAT THE CONE IS WIDER THAN A BODY. The arena is the only place
  // a position is set, so this moves the body rather than typing a distance.
  sim.target_at = [0, 40];
  sim.duration = 8; sim.runs = 8;
  markScenarioDirty && markScenarioDirty();
  await sleep(500);
  document.querySelector('#run-sim').click();
  for (let i = 0; i < 90 && !document.querySelector('#rec-host'); i++) await sleep(400);
  await sleep(800);
  const load = document.querySelector('#rec-load');
  if (load) load.click();
  for (let i = 0; i < 60 && !document.querySelector('.rec-t'); i++) await sleep(300);
  await sleep(500);
  const ev = (recordState && recordState.events) || [];
  const byId = new Map(ev.map((e) => [e.id, e]));
  const misses = ev.filter((e) => e.kind === 'miss');
  out.misses = misses.length;
  out.pellets = ev.filter((e) => e.kind === 'hit').length + misses.length;
  out.reasons = [...new Set(misses.map((e) => e.reason))];
  out.fromShots = misses.every((e) => (byId.get(e.cause) || {}).kind === 'shot');
  out.causeNothing = ev.every((e) => (byId.get(e.cause) || {}).kind !== 'miss');
  // …AND IT IS ON SCREEN, with its reason, under a filter of its own.
  const chip = [...document.querySelectorAll('[data-reckind]')].find((b) => b.dataset.reckind === 'miss');
  out.chip = !!chip;
  if (chip) { chip.click(); await sleep(250); }
  const shown = [...document.querySelectorAll('#rec-host tr.rec-miss')];
  out.drawn = shown.length;
  out.onlyMisses = shown.length === document.querySelectorAll('#rec-host tbody tr').length;
  return out;
})()`);

check(`${tag} a wide shot MISSES, and the record says so`, m.misses > 0,
  `${m.misses} of ${m.pellets} pellets at 40 m`);
check(`${tag} ...each with a reason`, m.misses > 0 && m.reasons.every(Boolean),
  (m.reasons || []).join(" / "));
check(`${tag} ...caused by the shot and causing nothing`,
  m.fromShots === true && m.causeNothing === true);
check(`${tag} ...and "only the misses" is a view`,
  m.chip === true && m.drawn > 0 && m.onlyMisses === true,
  `${m.drawn} drawn`);

// ---- A RESULT WITH NO ENGAGEMENT TO NAME SAYS SO ---------------------------
//
// The record is fetched by naming the run it explains, and a result SAVED
// before that name existed has none to give. The block must not simply vanish,
// or a reader coming back to a stored result finds the feature absent with
// nothing
// saying why — reported as "the combat record does not show". An unexplained absence reads as a missing feature, which
// is the same rule this panel already follows about its own caps.
const stale = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  // THE SHAPE STORAGE ACTUALLY HOLDS: a result with its replay stripped and,
  // for anything written before this feature, no run name either.
  const was = shownResult && shownResult.r;
  out.had = !!(was && was.run);
  const { run, ...older } = was;
  renderResults(older, (shownResult || {}).at);
  await sleep(400);
  const fold = document.querySelector('.fold[data-fold="record"]');
  out.block = !!fold;
  out.says = fold ? fold.textContent.trim().length > 20 : false;
  out.noButton = !document.getElementById('rec-load');
  // …AND RUNNING IT AGAIN BRINGS IT BACK, which is what the sentence promises.
  renderResults(was, (shownResult || {}).at);
  await sleep(400);
  // Either the button or the table it already loaded — what matters is that
  // the panel is a working one again rather than a sentence.
  out.backAgain = !!document.getElementById('rec-load')
    || !!document.querySelector('table.rec-t');
  return out;
})()`);

check(`${tag} a result saved before the record still shows the block`,
  stale.had === true && stale.block === true, "the block is drawn either way");
check(`${tag} ...and says why it cannot be read`, stale.says === true && stale.noButton === true);
check(`${tag} ...and running the fight again brings it back`, stale.backAgain === true);

// ---- STATE, PLUS WHAT THIS ROW CHANGED, IS THE NEXT STATE ------------------
//
// The two state columns said what was TRUE and never why it changed, so a
// reader could not check one row against the next. A row states what it put on
// the target and what it set off on the shooter, and both are deltas against
// the state column beside them.
//
// ITS OWN FIXTURE, because the bare one above has neither: no stacking buff to
// set off and a level-1 target whose statuses are over before the next row.
// Both claims pass perfectly on a fight that has none of either, which is the
// shape of vacuous this file has already been caught by twice.
const live = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  history.pushState({}, '', '/weapons/Laetum/simulator'); route();
  await sleep(6000);
  // THE BOARD'S OWN BUILD, which carries the stacking buffs a bare one cannot.
  const b = builtinBuilds();
  const row = b.find((x) => !x.riven && x.mode === 'cycle') || b[0];
  if (row) pickPreset(buildBarCfg(), presetId(row));
  await sleep(2500);
  // A LONG ENOUGH FIGHT TO NEED PAGING. The scenario above is six seconds,
  // which is right for reading one instance's arithmetic and is nowhere near
  // the fights this panel had to be paged for — and the paging assertions at
  // the bottom of this file pass VACUOUSLY on a record that fits on one page.
  // A real board build over a minute is thousands of rows.
  sim.duration = 60; sim.runs = 3;
  markScenarioDirty && markScenarioDirty();
  await sleep(400);
  document.getElementById('run-sim').click();
  for (let i = 0; i < 300 && !document.getElementById('rp-scrub'); i++) await sleep(500);
  const btn = document.getElementById('rec-load');
  if (btn) btn.click();
  for (let i = 0; i < 120 && !document.querySelector('table.rec-t'); i++) await sleep(300);
  await sleep(600);
  const dmg = (recordState.events || []).filter((e) => e.kind === 'damage');
  out.rows = dmg.length;
  let pairs = 0, fell = 0;
  for (let i = 0; i < dmg.length - 1; i++) {
    for (const k of dmg[i].triggered || []) {
      pairs += 1;
      if (((dmg[i + 1].buffs || [])[k] || [0])[0] < ((dmg[i].buffs || [])[k] || [0])[0]) fell += 1;
    }
  }
  out.pairs = pairs;
  out.fell = fell;
  out.withTriggers = dmg.filter((e) => (e.triggered || []).length).length;
  // …AND A DURATION IS THE ROW'S OWN CLOCK. The wire carries an ABSOLUTE
  // expiry, so a chip on two rows a second apart reads one second shorter — a
  // countdown frozen at application time would read the same on both.
  const clocked = [];
  for (const e of dmg) {
    for (const side of ['debuffs', 'buffs']) {
      for (const pair of e[side] || []) {
        if (pair && pair[0] > 0 && typeof pair[1] === 'number') clocked.push([e.t, pair[1]]);
      }
    }
  }
  out.clocked = clocked.length;
  out.ahead = clocked.every(([t, until]) => until >= t - 1e-6);
  // …AND IT IS ON SCREEN as a countdown beside the count.
  out.drawn = document.querySelectorAll('#rec-host .rec-s em').length;
  return out;
})()`);

check(`${tag} a row says what it set off on the shooter, not just on the target`,
  live.withTriggers > 0, `${live.withTriggers} of ${live.rows} rows carry a trigger`);
check(`${tag} ...and a buff it triggered never goes DOWN on the next row`,
  live.pairs > 0 && live.fell === 0, `${live.fell} of ${live.pairs} fell`);
check(`${tag} ...and a stack carries an expiry still ahead of its own row`,
  live.clocked > 0 && live.ahead === true, `${live.clocked} with a clock`);
check(`${tag} ...drawn as a countdown beside the count`,
  live.drawn > 0, `${live.drawn} chips with a time`);

// ---------------------------------------------------------------------------
// A LONG RECORD IS PAGED, AND IT CAN LEAVE THIS WINDOW.
//
// The record covers the whole fight, and the fights people argue about are tens
// of thousands of rows — which the browser lays out again on every repaint of
// the result panel, so picking an enemy or scrubbing the replay froze the page
// for seconds. Two answers, and this asserts both on the
// SAME fixture as above, which is a real board build and long enough to reach
// them: the table draws one screenful, and the whole thing can be moved into a
// window of its own.
//
// THE WINDOW NEEDS A GESTURE. `window.open` outside one is blocked, so the
// click is evaluated as the reader's — without it the button would look broken
// for a reason nobody using the app would ever hit.
const paged = await evaluate(`(() => ({
  rows: document.querySelectorAll('#rec-host > .rec-scroll > table.rec-t > tbody > tr').length,
  total: (recordState && recordState.events || []).length,
  pager: !!document.querySelector('.rec-pager'),
  page: REC_PAGE,
}))()`);

check(`${tag} the table draws one screenful, however long the fight is`,
  paged.total > paged.page && paged.rows <= paged.page,
  `${paged.rows} rows of ${paged.total}`);
check(`${tag} ...and says which screenful it is`,
  paged.pager === true, JSON.stringify(paged));

const win = await evaluate(`(() => {
  const b = document.querySelector('#rec-pop');
  if (!b) return { noButton: true };
  b.click();
  const d = recWin && recWin.document;
  return {
    opened: !!(recWin && !recWin.closed),
    childRows: d ? d.querySelectorAll('#rec-host > .rec-scroll > table.rec-t > tbody > tr').length : -1,
    styled: d ? !!d.querySelector('link[rel="stylesheet"]') : false,
    // …AND OUT OF THIS ONE, which is the whole point: a table that is drawn in
    // both places has moved nothing off the page that was freezing.
    handedOver: !document.querySelector('#rec-host table.rec-t'),
    back: !!document.querySelector('#rec-back'),
  };
})()`, { userGesture: true });

check(`${tag} ...and the whole record opens in a window of its own`,
  win.opened === true && win.childRows > 0 && win.styled === true, JSON.stringify(win));
check(`${tag} ...drawn THERE and not here`,
  win.handedOver === true && win.back === true, JSON.stringify(win));

const back = await evaluate(`(() => {
  // GUARDED, so a build where the window never opened FAILS the assertion
  // rather than throwing out of the check — a crash reports the wrong thing.
  const b = document.querySelector('#rec-back');
  if (b) b.click();
  return { clicked: !!b, table: !!document.querySelector('#rec-host table.rec-t'),
           closed: !recWin || recWin.closed };
})()`);
check(`${tag} ...and comes back when it is closed`,
  back.clicked === true && back.table === true && back.closed === true, JSON.stringify(back));

await finish("every row of the record produces its own number");
