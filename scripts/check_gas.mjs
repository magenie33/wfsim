// THE GAS METER HOLDS (docs/WASM.md §"Gas"), in two parts:
// 1. EVERY TOP-LEVEL fn, impl AND trait in the metered sources carries
//    `#[wfsim_gas_macro::metered]`, so nothing new on a fight's path goes
//    uncounted;
// 2. NATIVE AND WASM COUNT THE SAME: the rows of tests/gas/rows.json, fought
//    by the native `wfsim-gas` and by the wasm engine as the page fights them,
//    give the same gas, work and score, however the runs are cut, on a fresh
//    instance too; and the work is engine/src/fight/gas.rs's price of the count.
//   node scripts/check_gas.mjs                          part 1 (part 2 says it was not run)
//   node scripts/check_gas.mjs native=<bin> pkg=<dir>   both parts, on builds in hand
//   node scripts/check_gas.mjs build                    both, building the pair first
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { loadEngine } from "./wasm_engine.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/(\w:)/, "$1")), "..");
const arg = (k, d) => { const a = process.argv.find((x) => x.startsWith(`${k}=`)); return a ? a.slice(k.length + 1) : d; };
let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

/// THE METERED SOURCES: what a fight runs. A module a new verb's work runs in
/// is added here, and this check then asks every item in it for the attribute.
export const METERED = ["engine/src/fight", "engine/src/rules", "engine/src/model", "engine/src/target.rs",
  "engine/src/formation.rs", "engine/src/record.rs"];
/// …less the code around the runs, which must not count: the loop that reads
/// the count, and the counter itself.
const NOT_METERED = ["engine/src/fight/monte_carlo.rs", "engine/src/fight/gas.rs"];
const ATTR = "#[wfsim_gas_macro::metered]";
const ITEM = /^(pub(\([a-z:]+\))? )?(unsafe )?(fn |impl[ <]|trait )/;

function rust(p) {
  const abs = path.join(ROOT, p);
  if (fs.statSync(abs).isFile()) return [p];
  return fs.readdirSync(abs, { withFileTypes: true }).flatMap((d) =>
    d.isDirectory() ? (d.name === "tests" ? [] : rust(`${p}/${d.name}`)) : d.name.endsWith(".rs") ? [`${p}/${d.name}`] : []);
}

console.log("every item on a fight's path is metered");
const unmetered = [];
let items = 0;
for (const file of METERED.flatMap(rust).filter((f) => !NOT_METERED.includes(f))) {
  const lines = fs.readFileSync(path.join(ROOT, file), "utf8").split(/\r?\n/);
  lines.forEach((line, i) => {
    if (!ITEM.test(line) || /\bconst fn\b/.test(line)) return;
    items++;
    // The attribute may sit anywhere in the item's run of attributes and doc comments.
    let j = i - 1;
    while (j >= 0 && /^\s*(#\[|\/\/\/|\/\/!)/.test(lines[j]) && lines[j].trim() !== ATTR) j--;
    if (!(j >= 0 && lines[j].trim() === ATTR)) unmetered.push(`${file}:${i + 1}: ${line.trim().slice(0, 60)}`);
  });
}
check(`${items} top-level items carry ${ATTR}`, items > 0 && unmetered.length === 0, unmetered.slice(0, 10).join("\n        "));

let native = arg("native", ""), pkg = arg("pkg", "");
if (process.argv.includes("build")) {
  const run = (cmd, args, env = {}) => execFileSync(cmd, args, { cwd: ROOT, stdio: ["ignore", "inherit", "inherit"], env: { ...process.env, ...env } });
  run("cargo", ["build", "--release", "-q", "-p", "wfsim-cli", "--bin", "wfsim-gas"]);
  run("cargo", ["build", "--release", "-q", "-p", "wfsim-wasm", "--target", "wasm32-unknown-unknown"]);
  pkg = path.join(ROOT, "target", "gas-check-pkg");
  run(process.env.WASM_BINDGEN || "wasm-bindgen", ["target/wasm32-unknown-unknown/release/wfsim_wasm.wasm", "--target", "no-modules", "--no-typescript", "--out-dir", pkg]);
  native = path.join(ROOT, "target", "release", process.platform === "win32" ? "wfsim-gas.exe" : "wfsim-gas");
}

console.log("native and wasm count the same");
if (!native || !pkg) {
  console.log("  --   not run: give native=<bin> pkg=<dir>, or build");
} else {
  const SRC = fs.readFileSync(path.join(ROOT, "engine/src/fight/gas.rs"), "utf8");
  const price = (name) => BigInt(SRC.match(new RegExp(`pub const ${name}: u64 = ([0-9_]+);`))[1].replace(/_/g, ""));
  const [PER_MILLION, PER_RUN] = [price("GAS_PRICE_PER_MILLION"), price("RUN_PRICE")];
  const SHARE = 0.1;
  const rowsFile = path.join(ROOT, "tests/gas/rows.json");
  const rows = JSON.parse(fs.readFileSync(rowsFile, "utf8"));
  const nat = execFileSync(native, [rowsFile, String(SHARE), "1", "check"], { encoding: "utf8", maxBuffer: 1 << 26 })
    .split("\n").filter(Boolean).map((l) => JSON.parse(l));
  const bits = (s) => `${JSON.stringify(s.score)} ${s.metric} ${s.work} ${s.gas}`;
  const A = loadEngine({ pkg });
  rows.forEach((row, i) => {
    const name = `${row.record.weapon} ${row.ruler} ${row.mode}`;
    const order = A.api("/api/board/order", row), req = order.request;
    const n = Math.max(1, Math.ceil(req.runs * SHARE));
    const scoreOn = (e, acc) => { const s = e.api("/api/board/score", { ruler: row.ruler, request: req, acc }); return { ...s, gas: s.counts && s.counts.gas }; };
    const pieces = (lanes, piece, backward = false) => {
      const at = [];
      for (let k = 0; k < n; k += piece) at.push(k);
      if (backward) at.reverse();
      const got = at.map((k, j) => [k, lanes[j % lanes.length].api("/api/board/runs", { request: req, from: k, count: Math.min(piece, n - k) }).shards]);
      got.sort((x, y) => x[0] - y[0]);
      return scoreOn(A, A.api("/api/board/fold", { request: req, acc: null, pieces: got.flatMap((g) => g[1]) }).acc);
    };
    const w = scoreOn(A, A.api("/api/board/fold", { request: req, from: 0, count: n }).acc);
    check(`${name}: native and wasm give the same gas, work and score`, nat[i] && bits(nat[i]) === bits(w), `${nat[i] && bits(nat[i])} vs ${bits(w)}`);
    check(`${name}: native, five ways of cutting its runs, the same`, nat[i] && nat[i].same === true);
    const fresh = loadEngine({ pkg });
    const half = A.api("/api/board/fold", { request: req, from: 0, count: n >> 1 }).acc;
    const ways = [scoreOn(fresh, fresh.api("/api/board/fold", { request: req, from: 0, count: n }).acc), pieces([loadEngine({ pkg }), A, fresh], 1),
      pieces([fresh, A], 7), pieces([loadEngine({ pkg }), A], 1, true),
      scoreOn(A, A.api("/api/board/fold", { request: req, from: n >> 1, count: n - (n >> 1), acc: half }).acc)];
    check(`${name}: wasm, a fresh instance and five ways of cutting its runs, the same`, ways.every((x) => bits(x) === bits(w)), ways.map(bits).join(" | "));
    const want = (BigInt(w.gas) * PER_MILLION) / 1_000_000n + BigInt(n) * PER_RUN;
    check(`${name}: the work is gas.rs's price of the count`, BigInt(w.work) === want, `${w.work} vs ${want}`);
    check(`${name}: a thousand full rows of it is an exact JSON number`, Number.isSafeInteger(w.gas * 10000) && Number.isSafeInteger(w.work * 10000));
  });
}
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
