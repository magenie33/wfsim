// THE QUICK CALC SURVIVES BEING USED, and always ends with an answer.
//
// The checks beside this one each pin one fault. This one asks the question a
// reader actually asks: after doing the impatient things people do — switching
// slots faster than a scan can finish, flicking the switch, editing the fight
// mid-scan, losing workers — does it still converge on a complete ranking?
//
// EVERY ROUND ENDS THE SAME WAY: `gainScan.key` stamped, equal to the fight on
// screen, with an answer for every candidate and nothing still running. A round
// that ends any other way is a calculator a player would call stuck.
//
//   node scripts/check_calc_survives_use.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

await evaluate("localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')");
await send("Page.navigate", { url: BASE });
await sleep(12000);

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/Ballistica_Prime'); route(); await sleep(4500);
  const out = { rounds: [] };

  // A MATURE BUILD, because an empty one is the cheap case and the reports are
  // all from full ones.
  ['hornet_strike', 'galvanized_diffusion', 'galvanized_shot', 'lethal_torrent',
   'primed_convulsion', 'primed_heated_charge', 'galvanized_crosshairs']
    .forEach((id, i) => { if (slots[i + 1]) slots[i + 1].mod = id; });
  out.filled = slots.filter((s) => s.mod).length;

  const anchor = () => document.querySelector('.slot') || document.body;
  // SETTLED MEANS ANSWERED: stamped, current, complete, and stopped. Anything
  // less is what a reader calls stuck, so nothing weaker is accepted here.
  const settled = async (limit) => {
    for (let i = 0; i < limit; i++) {
      if (!gainScan.running && gainScan.key !== null && gainScan.key === gainKey()) {
        const want = gainScan.ids ? gainScan.ids.size : 0;
        // A REFUSAL COUNTS AS AN ANSWER. An option the weapon cannot take with
        // these evolutions is measured and settled; requiring a gain for it is
        // requiring the impossible, which is the state the Torid was stuck in.
        const got = Object.keys(gainScan.by).length
          + Object.keys(gainScan.refused || {}).length;
        if (want > 0 && got >= want) return true;
      }
      await sleep(500);
    }
    return false;
  };
  const note = (name, ok) => {
    out.rounds.push({
      name, ok,
      running: gainScan.running,
      stamped: gainScan.key !== null,
      current: gainScan.key === gainKey(),
      ranked: Object.keys(gainScan.by || {}).length,
      want: gainScan.ids ? gainScan.ids.size : 0,
      lost: gainScan.lanesLost,
      why: String(gainScan.note || '').slice(0, 80),
    });
    return ok;
  };

  // 1. IMPATIENT SWITCHING. Three slots opened faster than any of them can be
  //    measured — the gesture behind "I clicked another slot and it hung".
  openPicker(0, anchor()); await sleep(120);
  openPicker(1, anchor()); await sleep(120);
  openPicker(2, anchor()); await sleep(120);
  openPicker(3, anchor());
  note('rapid slot switching', await settled(80));

  // 2. THE SWITCH, flicked mid-scan and back. The first thing anyone does to
  //    something that looks stuck.
  openPicker(4, anchor()); await sleep(150);
  // RE-QUERIED EACH TIME. Rendering the box replaces the element, so a handle
  // kept across the first flick is detached and the second one lands nowhere —
  // which looks exactly like the calculator refusing to come back on.
  const flick = async (on) => {
    const b = document.getElementById('gp-on');
    if (!b) return;
    b.checked = on;
    b.dispatchEvent(new Event('change'));
    await sleep(250);
  };
  await flick(false);
  await flick(true);
  out.backOn = gainPrefs.on;
  openPicker(4, anchor());
  note('switched off and on mid-scan', await settled(80));

  // 3. THE FIGHT MOVES under a running scan. The answers on screen become
  //    answers to a question nobody asked, and it has to re-ask on its own.
  openPicker(5, anchor()); await sleep(150);
  sim.level = 875;
  refreshGains();
  note('the fight edited mid-scan', await settled(80));

  // 4. WORKERS LOST under a running scan.
  openPicker(6, anchor()); await sleep(200);
  let killed = 0;
  for (const l of pool) if (l && !l.dead) { l.abandon(); killed++; }
  out.killed = killed;
  note('every worker killed mid-scan', await settled(100));

  // 5. THE EXILUS SLOT, whose whole pool measures flat under a single-target
  //    ruler — the case that was reported as "it will not calculate".
  openPicker(EXILUS, anchor());
  const okEx = await settled(80);
  out.exilusAllFlat = Object.values(gainScan.by || {}).every((g) => g.pct === 0);
  out.exilusRanked = Object.keys(gainScan.by || {}).length;
  note('the exilus slot', okEx);

  // 6. STRAIGHT BACK to an ordinary slot, to prove nothing was left latched.
  openPicker(0, anchor());
  note('an ordinary slot after all of that', await settled(80));

  // 7. A WEAPON WHOSE POOL CONTAINS A MOD IT CANNOT TAKE. The Torid's Incarnon
  //    evolutions change the trigger, so one of the eighty candidates is
  //    legally refused — and a refusal counted as a hole meant that list could
  //    never be complete, never stamped, and re-ran for ever.
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(4500);
  openPicker(0, anchor());
  const okT = await settled(120);
  out.toridRefused = Object.keys(gainScan.refused || {}).length;
  out.toridRanked = Object.keys(gainScan.by || {}).length;
  out.toridWant = gainScan.ids ? gainScan.ids.size : 0;
  note('a weapon with a mod it cannot equip', okT);

  out.allOk = out.rounds.every((x) => x.ok);
  return out;
})()`);

check("the build under test is a mature one", r.filled >= 7, `filled ${r.filled}`);
check("the pool really was killed in round 4", r.killed > 0, `killed ${r.killed}`);

for (const round of r.rounds) {
  check(
    `it converges on a complete answer: ${round.name}`,
    round.ok === true,
    `running ${round.running}, stamped ${round.stamped}, current ${round.current}, `
    + `ranked ${round.ranked}/${round.want}, lost ${round.lost}, note ${JSON.stringify(round.why)}`,
  );
}

check(
  "the exilus slot measures every option rather than none",
  r.exilusRanked > 0,
  `ranked ${r.exilusRanked}, all flat ${r.exilusAllFlat}`,
);
check(
  "a legally refused option is recorded rather than left as a hole",
  r.toridRefused > 0 && r.toridRanked + r.toridRefused >= r.toridWant,
  `refused ${r.toridRefused}, ranked ${r.toridRanked}, of ${r.toridWant}`,
);
check("every round ended with an answer", r.allOk === true);

process.exit(0);
