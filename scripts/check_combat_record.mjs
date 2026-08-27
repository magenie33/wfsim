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
  await sleep(400);

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
  const gated = rows.filter((tr) => /shield gate/i.test(tr.querySelector('.rec-calc').textContent));
  out.gateRows = gated.length;
  out.gateFactor = gated.length
    ? num([...gated[0].querySelectorAll('.rec-f.mit')]
        .find((f) => /shield gate/i.test(f.textContent)).firstChild.textContent)
    : null;
  // …and the two halves are ONE instant, which is what makes them one hit.
  const t = (tr) => tr.querySelector('.rec-t').firstChild.textContent.trim();
  out.pairedAtOneInstant = gated.length
    ? rows.some((x) => x !== gated[0] && t(x) === t(gated[0]) && /rec-shield/.test(x.className))
    : false;

  // ---- THE STATE COLUMN IS THE TARGET BEFORE THE HIT -------------------
  out.statesDrawn = rows.filter((tr) => /health/i.test(tr.querySelector('.rec-state').textContent)).length;

  // ---- A FILTER SHOWS ONLY WHAT IT NAMES -------------------------------
  const kind = [...document.querySelectorAll('[data-reckind]')].find((b) => b.dataset.reckind === 'status');
  if (kind) { kind.click(); await sleep(250); }
  const after = [...document.querySelectorAll('tr.rec-dmg')];
  out.filtered = after.length;
  out.filteredAllStatus = after.length > 0
    && after.every((tr) => /status/i.test(tr.querySelector('.rec-org').textContent));
  const all = [...document.querySelectorAll('[data-reckind]')].find((b) => b.dataset.reckind === 'all');
  if (all) { all.click(); await sleep(250); }
  out.restored = document.querySelectorAll('tr.rec-dmg').length;

  // ---- IT CAN BE TAKEN AWAY AS TEXT ------------------------------------
  // A record that can only be looked at cannot be DIFFED, and a diff of two
  // records is the thing that says which row moved when a number changes.
  out.copy = !!document.querySelector('#rec-copy');
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
  `status ${r.filtered} of ${r.rows}, restored ${r.restored}`);
check(`${tag} the record can be taken away as text`, r.copy, "no copy control");

await finish("every row of the record produces its own number");
