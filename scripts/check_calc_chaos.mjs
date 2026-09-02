// THE QUICK CALC UNDER RANDOM USE, and it still ends with an answer.
//
// `check_calc_survives_use.mjs` walks a sequence someone thought of. This one
// exists because the sequences that broke it were the ones nobody thought of:
// a seeded random walk over everything a reader can do to the calculator, with
// the same demand at the end of every round — stamped, current, complete,
// stopped.
//
// SEEDED, so a failure is a sequence rather than a mood. The action log comes
// back with the failure and replays exactly.
//
// IT BITES, and it found the reported shape on its own. With the lane watchdog
// disabled, three seeds in four go red, and one of them reads:
//
//     wedge a worker into silence (wedged 1)
//     open slot 0 and wait
//     left: running true, stamped false, ranked 65+0/66, lost 0
//
// which is the report word for word — a ranking permanently one short, with
// nothing lost and nothing failed. No fixed sequence anyone wrote found that;
// the random walk did, twice, from a standing start.
//
//   node scripts/check_calc_chaos.mjs [seed]
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const SEED = Number(process.argv[2] || 20260902);
const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

await evaluate("localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')");
await send("Page.navigate", { url: BASE });
await sleep(12000);

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = { rounds: [], seed: ${SEED} };

  // A SMALL DETERMINISTIC PRNG. Math.random would make a red run unreplayable,
  // which is the one thing a chaos test cannot afford.
  let s = ${SEED} >>> 0;
  const rnd = () => (s = (s * 1664525 + 1013904223) >>> 0) / 4294967296;
  const pick = (xs) => xs[Math.floor(rnd() * xs.length)];

  // SHORT WINDOWS, so a wedged worker is given up on inside a round rather
  // than outliving the whole check. The product default is 45 s / 90 s.
  LANE_WATCHDOG.loading = 4000;
  LANE_WATCHDOG.stall = 4000;
  history.pushState({}, '', '/weapons/Ballistica_Prime'); route(); await sleep(4500);
  ['hornet_strike', 'galvanized_diffusion', 'galvanized_shot', 'lethal_torrent',
   'primed_convulsion', 'primed_heated_charge', 'galvanized_crosshairs']
    .forEach((id, i) => { if (slots[i + 1]) slots[i + 1].mod = id; });

  const anchor = () => document.querySelector('.slot') || document.body;
  const flick = async (on) => {
    const b = document.getElementById('gp-on');
    if (b) { b.checked = on; b.dispatchEvent(new Event('change')); }
  };

  // EVERY LEVER A READER HAS. Each returns what it did, for the log.
  const actions = [
    { name: 'open a mod slot', run: () => { const i = Math.floor(rnd() * 8); openPicker(i, anchor()); return 'slot ' + i; } },
    { name: 'open the exilus slot', run: () => { openPicker(EXILUS, anchor()); return 'exilus'; } },
    { name: 'switch off then on', run: async () => { await flick(false); await sleep(80); await flick(true); return 'off/on'; } },
    { name: 'edit the level', run: () => { sim.level = 100 + Math.floor(rnd() * 900); refreshGains(); return 'level ' + sim.level; } },
    { name: 'change the mode', run: () => {
        const el = document.getElementById('mode');
        if (!el || el.options.length < 2) return 'no modes';
        const i = Math.floor(rnd() * el.options.length);
        el.value = el.options[i].value; el.dispatchEvent(new Event('change'));
        return 'mode ' + el.value; } },
    { name: 'kill one worker', run: () => {
        const live = pool.filter((l) => l && !l.dead);
        if (!live.length) return 'none alive';
        live[Math.floor(rnd() * live.length)].abandon(); return 'killed 1'; } },
    // THE FAILURE THAT PRODUCES A REAL LATCH: a worker that is alive,
    // un-abandoned, and will never reply — what a browser reclaiming memory
    // leaves behind. Nothing settles its waiters, so without a watchdog the
    // scan awaits an answer that cannot come, for ever.
    { name: 'wedge a worker into silence', run: () => {
        const live = pool.filter((l) => l && !l.dead);
        if (!live.length) return 'none alive';
        const w = live[Math.floor(rnd() * live.length)].worker;
        w.postMessage = () => {};
        return 'wedged 1'; } },
    { name: 'kill every worker', run: () => {
        let n = 0; for (const l of pool) if (l && !l.dead) { l.abandon(); n++; }
        return 'killed ' + n; } },
    { name: 'press rebuild', run: () => { restartCalc(); refreshGains(); return 'rebuilt'; } },
    { name: 'change the compute share', run: () => {
        const was = poolSize();
        setComputePct(pick([10, 30, 50, 100]));
        return was + ' -> ' + poolSize(); } },
    { name: 'switch weapon', run: async () => {
        const w = pick(['Torid', 'Braton', 'Ballistica_Prime']);
        history.pushState({}, '', '/weapons/' + w); route(); await sleep(2500);
        return w; } },
  ];

  // SETTLED MEANS ANSWERED, and a refusal counts as one.
  const settled = async (limit) => {
    for (let i = 0; i < limit; i++) {
      if (!gainScan.running && gainScan.key !== null && gainScan.key === gainKey()) {
        const want = gainScan.ids ? gainScan.ids.size : 0;
        const got = Object.keys(gainScan.by).length + Object.keys(gainScan.refused || {}).length;
        if (want > 0 && got >= want) return true;
      }
      await sleep(500);
    }
    return false;
  };

  for (let round = 0; round < 6; round++) {
    const log = [];
    // FOUR RANDOM SHOVES, none of them given time to finish.
    for (let k = 0; k < 4; k++) {
      const a = pick(actions);
      let what = '';
      try { what = await a.run(); } catch (e) { what = 'threw: ' + (e && e.message); }
      log.push(a.name + ' (' + what + ')');
      await sleep(Math.floor(rnd() * 400));
    }
    // …then a reader opens a list and waits, which is all they ever do.
    const slot = Math.floor(rnd() * 8);
    log.push('open slot ' + slot + ' and wait');
    const b = document.getElementById('gp-on');
    if (b && !b.checked) { b.checked = true; b.dispatchEvent(new Event('change')); await sleep(200); }
    openPicker(slot, anchor());
    const ok = await settled(120);
    out.rounds.push({
      round, ok, log,
      weapon: document.getElementById('weapon') ? document.getElementById('weapon').value : '?',
      running: gainScan.running,
      stamped: gainScan.key !== null,
      current: gainScan.key === gainKey(),
      ranked: Object.keys(gainScan.by || {}).length,
      refused: Object.keys(gainScan.refused || {}).length,
      want: gainScan.ids ? gainScan.ids.size : 0,
      lost: gainScan.lanesLost,
      why: String(gainScan.note || '').slice(0, 70),
      prefsOn: gainPrefs.on,
    });
  }
  out.allOk = out.rounds.every((x) => x.ok);
  return out;
})()`);

for (const x of r.rounds) {
  check(
    `round ${x.round} converges (${x.weapon})`,
    x.ok === true,
    `\n      did: ${x.log.join("\n      then: ")}`
    + `\n      left: running ${x.running}, stamped ${x.stamped}, current ${x.current}, `
    + `ranked ${x.ranked}+${x.refused}/${x.want}, lost ${x.lost}, on ${x.prefsOn}, note ${JSON.stringify(x.why)}`,
  );
}
check(`every round converged (seed ${r.seed})`, r.allOk === true);

process.exit(0);
