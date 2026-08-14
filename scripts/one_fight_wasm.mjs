// `one_fight_wasm` — the same fight, timed in the thing the product actually
// ships.
//
// `cargo run --release --bin one_fight` measures NATIVE Rust, which is a proxy:
// a change can be faster there and slower in the browser, and the native number
// alone would never say so. This runs the identical fight through the shipping
// wasm build in a real browser, so the two can be compared and the proxy can be
// CALIBRATED — the ratio it prints is what makes every native measurement mean
// something for a player.
//
//   node scripts/one_fight_wasm.mjs
//   node scripts/one_fight_wasm.mjs runs=2000 weapon=torid
//
// Regenerate `site/` first (`python scripts/build_site_app.py`) or you are
// timing yesterday's engine.
//
// WHAT IS IN THE NUMBER: everything a player waits for — the worker round trip,
// the JSON on both sides, and the simulation. That is deliberate. The native
// tool isolates the engine; this one measures the product, and the gap between
// them is itself a finding.
import { openApp, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const arg = (k, d) => {
  const hit = args.find((a) => a.startsWith(`${k}=`));
  return hit ? hit.slice(k.length + 1) : d;
};
const RUNS = Number(arg("runs", 1000));
const REPEATS = Number(arg("repeats", 3));
// The same three shapes the native harness uses, for the same reason: a change
// to the inner loop rarely moves them together.
const SHAPES = arg("weapon", "torid,gotva_prime,scourge").split(",");
// THE SAME FIGHT AS THE NATIVE TOOL, stated rather than inherited. Using
// the app's live scenario made the first run compare a Thrax at 9999 Steel
// Path against whatever was in `sim` — kill progress 1.48 where the native
// baseline had 0.186 — and a "ratio" between two different fights is not a
// ratio.
const ENEMY = arg("enemy", "thrax_centurion");
const LEVEL = Number(arg("level", 9999));
const STEEL = arg("steel_path", "1") !== "0";
const DURATION = Number(arg("duration", 180));
const MODS = arg(
  "mods",
  "serration,split_chamber,point_strike,vital_sense,hellfire,cryo_rounds,infected_clip,stormbringer",
).split(",");

const app = await openApp({ boot: 12000 });

const out = await app.evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const shapes = ${JSON.stringify(SHAPES)};
  const mods = ${JSON.stringify(MODS)};
  const rows = [];
  for (const w of shapes) {
    switchWeapon(w);
    await sleep(600);
    // The same eight mods, and anything this weapon cannot hold is NAMED —
    // a silently different build is a silently different measurement.
    const dropped = [];
    slots.forEach(s => { s.mod = null; s.rank = null; });
    let i = 0;
    for (const id of mods) {
      const m = modById(id);
      if (!m) { dropped.push(id); continue; }
      slots[i].mod = id; slots[i].rank = m.max_rank; i++;
    }
    autoForma(); renderMods();
    await sleep(300);
    const body = { ...buildPayload(), ...fightPayload(sim),
      enemy: ${JSON.stringify(ENEMY)}, level: ${LEVEL},
      steel_path: ${STEEL}, duration: ${DURATION}, runs: ${RUNS} };
    // Warm: the first call pays for the worker waking up and the wasm's own
    // first-touch, which is not what the tenth call pays.
    await api('/api/simulate', body);
    let best = Infinity, worst = 0, last = null;
    for (let k = 0; k < ${REPEATS}; k++) {
      const t0 = performance.now();
      last = await api('/api/simulate', body);
      const el = performance.now() - t0;
      best = Math.min(best, el); worst = Math.max(worst, el);
    }
    rows.push({
      weapon: w, dropped,
      total_ms: best,
      ms_per_run: best / ${RUNS},
      spread: best > 0 ? (worst - best) / best : 0,
      shots: last && last.shots,
      score_mean: last && last.score_mean,
      dps_mean: last && last.dps_mean,
    });
  }
  return rows;
})()`, 600000);

console.log(`${MODS.length} mods · ${ENEMY} lv ${LEVEL}${STEEL ? " SP" : ""} · ${DURATION} s · ${RUNS} runs × ${REPEATS} · wasm, in a browser\n`);
console.log(
  `${"shape".padEnd(14)}${"total".padStart(9)}${"ms/run".padStart(10)}${"noise".padStart(8)}  answer`,
);
for (const r of out) {
  if (r.dropped.length) console.log(`  ! ${r.weapon} cannot equip ${r.dropped.join(", ")}`);
  console.log(
    `${r.weapon.padEnd(14)}${(r.total_ms / 1000).toFixed(2).padStart(8)}s${
      r.ms_per_run.toFixed(3).padStart(10)}${
      `±${(r.spread * 100).toFixed(1)}%`.padStart(8)}  kill ${r.score_mean?.toFixed(6)}`,
  );
}

// THE CALIBRATION, which is the reason this exists. A native baseline is on
// disk whenever somebody ran the other tool; without one the ratio is simply
// not printed rather than guessed at.
const fs = await import("node:fs");
let base = [];
try {
  base = fs.readFileSync("target/one_fight.baseline", "utf8")
    .split("\n").filter((l) => l && !l.startsWith("#"))
    .map((l) => l.split("\t"));
} catch { /* no native baseline: the ratio is skipped, not invented */ }
if (base.length) {
  console.log("\nagainst the native baseline on disk:");
  for (const r of out) {
    const b = base.find((x) => x[0] === r.weapon);
    if (!b) continue;
    // THE SAME ANSWER OR NO RATIO. The baseline stores the native run's
    // kill progress; if this one differs the two tools measured different
    // fights and their costs are not comparable — which is how the first
    // version of this script reported "0.9x native" for a fight it had
    // never run.
    if (Math.abs(Number(b[3]) - r.score_mean) > 1e-9) {
      console.log(
        `  ${r.weapon.padEnd(14)} NOT COMPARABLE — different fights` +
        `   (native kill ${Number(b[3]).toFixed(6)}, here ${r.score_mean?.toFixed(6)})`,
      );
      continue;
    }
    console.log(
      `  ${r.weapon.padEnd(14)} wasm is ${(r.ms_per_run / Number(b[1])).toFixed(1)}× native` +
      `   (${Number(b[1]).toFixed(3)} → ${r.ms_per_run.toFixed(3)} ms/run)`,
    );
  }
  console.log(
    "\n  That ratio is what makes a native measurement mean something here.\n" +
    "  It includes the worker round trip, so it is the product's cost and not\n" +
    "  the engine's — the gap between the two is itself worth knowing.",
  );
} else {
  console.log(
    "\nno native baseline on disk — `cargo run --release --bin one_fight -- save`\n" +
    "to make one, and this will print the wasm/native ratio next time.",
  );
}

await app.finish("wasm cost measured");
