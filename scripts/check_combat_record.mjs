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
  // SCENARIO and not the mods, so it explained a fight nobody had run (owner,
  // 2026-08-27). A damage mod makes the two tell apart.
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
  // Read the factors as drawn, multiply them, and compare with the two totals
  // the same row prints. Nothing here consults the engine: if the page draws a
  // ledger that does not produce its own number, that is the bug.
  const num = (s) => Number(String(s).replace(/[^0-9.\\-]/g, ''));
  const bad = [];
  let checked = 0;
  for (const tr of rows) {
    const calc = tr.querySelector('.rec-calc');
    if (!calc) continue;
    // THE BASE IS THE LAST rec-b0 chip: where it can be opened up, the row
    // draws the ModifiedBase it started from, the factors, then the base.
    const b0 = [...calc.querySelectorAll('.rec-b0')];
    const base = num(b0[b0.length - 1].textContent);
    // …AND WHEN IT IS OPENED UP, that chain has to multiply out too.
    if (b0.length > 1) {
      const from = num(b0[0].textContent);
      const bs = [...calc.querySelectorAll('.rec-f.base')].map((f) => num(f.firstChild.textContent));
      const got = bs.reduce((a, x) => a * x, from);
      if (Math.abs(got - base) > Math.max(1.0, Math.abs(base) * 0.005)) {
        bad.push('ModifiedBase x base steps = ' + got.toFixed(1) + ' but row says ' + base);
        checked++;
        continue;
      }
    }
    const mid = num(calc.querySelector('.rec-mid').textContent);
    const eq = num(calc.querySelector('.rec-eq').textContent);
    const off = [...calc.querySelectorAll('.rec-f:not(.mit):not(.base)')].map((f) => num(f.firstChild.textContent));
    const def = [...calc.querySelectorAll('.rec-f.mit')].map((f) => num(f.firstChild.textContent));
    const raw = off.reduce((a, b) => a * b, base);
    const eff = def.reduce((a, b) => a * b, mid);
    // The page rounds what it prints, so the tolerance is the rounding and not
    // a fudge: one unit, or 0.5% on a number too big for one unit to matter.
    const near = (a, b) => Math.abs(a - b) <= Math.max(1.0, Math.abs(b) * 0.005);
    const where = () => ' [' + tr.querySelector('.rec-org').textContent + '/'
      + (tr.className.match(/rec-(shield|health|overguard)/) || [])[1] + ' '
      + calc.textContent.replace(/\s+/g, ' ').slice(0, 110) + ']';
    if (!near(raw, mid)) bad.push('base x steps = ' + raw.toFixed(1) + ' but row says ' + mid + where());
    else if (!near(eff, eq)) bad.push('raw x mitigation = ' + eff.toFixed(1) + ' but row says ' + eq + where());
    checked++;
  }
  out.checked = checked;
  out.bad = bad.slice(0, 3);
  // …AND THE SAME ARITHMETIC ON THE WIRE, so a failure says WHICH side is
  // wrong: the engine's ledger, or the way this page drew it.
  out.wireBad = (recordState.events || []).filter((e) => {
    if (e.kind !== 'damage') return false;
    const eff = (e.mitigation || []).reduce((a, [, v]) => a * v, e.raw);
    return Math.abs(eff - e.effective) > Math.max(0.05, Math.abs(e.effective) * 0.005);
  }).slice(0, 2);

  // ---- THE SPLIT ------------------------------------------------------
  // A body hit on a shielded target pops two numbers. They are two ROWS at the
  // same instant, one per pool, and the health one wears the gate.
  const pools = rows.map((tr) => (tr.className.match(/rec-(shield|health|overguard)/) || [])[1]);
  out.shieldRows = pools.filter((p) => p === 'shield').length;
  out.healthRows = pools.filter((p) => p === 'health').length;
  // BY THE FACTOR'S OWN KEY, never by its label. The label is translated and
  // this page defaults to Chinese, so matching the English sentence passed
  // only for as long as the panel was untranslated — and then failed on the
  // day it was translated, which is a check reporting the wrong thing twice
  // (2026-08-27). data-factor is the engine's own spelling and does not move.
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

  // ---- THE STATE COLUMN IS THE TARGET BEFORE THE HIT -------------------
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
check(`${tag} every row multiplies out`, r.checked > 10 && r.bad.length === 0,
  `checked ${r.checked}: ${r.bad.join(" | ") || "all consistent"}`
  + (r.wireBad.length ? ` — ENGINE: ${JSON.stringify(r.wireBad[0])}` : " — the engine's own ledger is consistent"));

// The split, and the gate on its face.
check(`${tag} a shielded body pops two numbers`, r.shieldRows > 0 && r.healthRows > 0,
  `shield ${r.shieldRows}, health ${r.healthRows}`);
check(`${tag} the half that got through wears the gate`, r.gateFactor === 0.05,
  `gate factor ${r.gateFactor} on ${r.gateRows} rows`);
check(`${tag} the two halves are one instant`, r.pairedAtOneInstant,
  "the shield row and the health row share a time");

check(`${tag} each row says what the target was before it`, r.statesDrawn === r.rows,
  `${r.statesDrawn} of ${r.rows}`);

// A NEGATIVE CONTROL for the filter: it must REMOVE things, and put them back.
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

await finish("every row of the record produces its own number");
