// THE GAS METER HOLDS (docs/WASM.md §"Gas"), in two parts:
// 1. EVERY TOP-LEVEL fn, impl AND trait in the metered sources carries
//    `#[wfsim_gas_macro::metered]`, so nothing new on a fight's path goes
//    uncounted; and no production code calls the platform's maths library
//    (`.powf(`, `.exp(`, `.sin(`…), only `rules::math`, which is one libm;
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
import { METERED, NOT_METERED } from "./metered.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/(\w:)/, "$1")), "..");
const arg = (k, d) => { const a = process.argv.find((x) => x.startsWith(`${k}=`)); return a ? a.slice(k.length + 1) : d; };
let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const ATTR = "#[wfsim_gas_macro::metered]";
const ITEM = /^(pub(\([^)]*\))? )?(unsafe )?(fn |impl[ <]|trait )/;

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
    while (j >= 0 && /^\s*(#\[|\/\/)/.test(lines[j]) && lines[j].trim() !== ATTR) j--;
    if (!(j >= 0 && lines[j].trim() === ATTR)) unmetered.push(`${file}:${i + 1}: ${line.trim().slice(0, 60)}`);
  });
}
check(`${items} top-level items carry ${ATTR}`, items > 0 && unmetered.length === 0, unmetered.slice(0, 10).join("\n        "));
/// A `macro_rules!` body is opaque to the attribute, so one that loops wraps it
/// in `metered_expr!`, or its turns would count as one expression.
const opaque = [];
for (const file of METERED.flatMap(rust).filter((f) => !NOT_METERED.includes(f))) {
  const text = fs.readFileSync(path.join(ROOT, file), "utf8");
  for (const m of text.matchAll(/macro_rules!\s*(\w+)\s*\{([\s\S]*?)\n\s*\};?\s*\n\s*\}/g)) {
    if (/\b(for|while|loop)\b[^\n]*\{/.test(m[2].replace(/\/\/.*$/gm, "")) && !m[2].includes("metered_expr!")) opaque.push(`${file}: ${m[1]}`);
  }
}
check("every macro_rules! body that loops is a metered_expr!", opaque.length === 0, opaque.join(", "));

/// ONE MATHS LIBRARY ON EVERY TARGET (engine/src/rules/math.rs): a transcendental
/// called on an f64 reaches the platform's own library natively and
/// compiler-builtins in wasm, and the two need not agree to the last bit.
/// Production code only — a test may compare against std.
export const PLATFORM_MATHS = /\.(powf|powi|exp|exp2|exp_m1|ln|ln_1p|log|log2|log10|sin|cos|tan|sin_cos|asin|acos|atan|atan2|sinh|cosh|tanh|asinh|acosh|atanh|hypot|cbrt)\(/;
const PORTABLE_SOURCES = ["engine/src", "webapi/src", "optimizer/src"];
const MATHS_HOME = "engine/src/rules/math.rs";
/// Each line outside a `tests` directory and an inline `#[cfg(test)] mod x { … }`
/// (closed by the `}` at its own indent), its comment dropped.
function productionLines(text) {
  const lines = text.split(/\r?\n/), out = [];
  for (let i = 0; i < lines.length; i++) {
    const m = /^(\s*)#\[cfg\(test\)\]\s*$/.exec(lines[i]);
    if (m && /^\s*(pub(\([^)]*\))? )?mod \w+ \{/.test(lines[i + 1] ?? "")) {
      while (i < lines.length && lines[i] !== `${m[1]}}`) i++;
      continue;
    }
    out.push([i + 1, lines[i].replace(/\/\/.*$/, "")]);
  }
  return out;
}
const direct = PORTABLE_SOURCES.flatMap(rust).filter((f) => f !== MATHS_HOME).flatMap((file) =>
  productionLines(fs.readFileSync(path.join(ROOT, file), "utf8"))
    .filter(([, l]) => PLATFORM_MATHS.test(l)).map(([n, l]) => `${file}:${n}: ${l.trim().slice(0, 60)}`));
check(`no production code calls the platform's maths, only ${MATHS_HOME}`, direct.length === 0, direct.slice(0, 10).join("\n        "));

let native = arg("native", ""), pkg = arg("pkg", "");
if (process.argv.includes("build")) {
  const run = (cmd, args, env = {}) => execFileSync(cmd, args, { cwd: ROOT, stdio: ["ignore", "inherit", "inherit"], env: { ...process.env, ...env } });
  run("cargo", ["build", "--release", "-q", "-p", "wfsim-cli", "--bin", "wfsim-gas"]);
  // THE SHIPPED MODULE, which carries no data and is handed its packs: one
  // that embedded them and was handed them too would hold the data twice.
  const apart = { WFSIM_DATA_APART: "1" };
  run("cargo", ["build", "--release", "-q", "-p", "wfsim-wasm", "--target", "wasm32-unknown-unknown"], apart);
  pkg = path.join(ROOT, "target", "gas-check-pkg");
  run(process.env.WASM_BINDGEN || "wasm-bindgen", ["target/wasm32-unknown-unknown/release/wfsim_wasm.wasm", "--target", "no-modules", "--no-typescript", "--out-dir", pkg]);
  // …AND ITS DATA, the packs the module is handed (docs/WASM.md §"Code and data").
  run(process.platform === "win32" ? "python" : "python3", ["scripts/data_packs.py", pkg], apart);
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
  // …AND THE FIRST ROW AGAIN AT THE END, in the same process: a long-lived
  // client fights row after row, and state a run fills once — a cache, a lazy
  // table — must not make a later row's count differ from its first.
  const againFile = path.join(ROOT, "target", "gas-rows-again.json");
  fs.mkdirSync(path.dirname(againFile), { recursive: true });
  fs.writeFileSync(againFile, JSON.stringify([...rows, rows[0]]));
  const nat = execFileSync(native, [againFile, String(SHARE), "1", "check"], { encoding: "utf8", maxBuffer: 1 << 26 })
    .split("\n").filter(Boolean).map((l) => JSON.parse(l));
  const bits = (s) => `${JSON.stringify(s.score)} ${s.metric} ${s.work} ${s.gas}`;
  const A = loadEngine({ pkg });
  let first = null, heaviest = null;
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
    if (i === 0) first = { req, n, w, scoreOn };
    if (!heaviest || BigInt(w.gas) > BigInt(heaviest.w.gas)) heaviest = { req, n, w, scoreOn, row };
  });
  const again = first.scoreOn(A, A.api("/api/board/fold", { request: first.req, from: 0, count: first.n }).acc);
  check(`${rows[0].record.weapon}, fought again after every other row in one process: native and wasm count what they did first`,
    nat[rows.length] && bits(nat[rows.length]) === bits(nat[0]) && bits(again) === bits(first.w),
    `native ${nat[rows.length] && bits(nat[rows.length])} vs ${bits(nat[0])}, wasm ${bits(again)} vs ${bits(first.w)}`);
  // EVERY ENGINE CALL SAYS IT IS ALIVE from inside its work (worker.js hands it
  // the heartbeat), and saying so changes no bit of what it counts.
  const B = loadEngine({ pkg });
  let beats = 0;
  B.heartbeat(() => { beats += 1; });
  const h = heaviest;
  const beat = h.scoreOn(B, B.api("/api/board/fold", { request: h.req, from: 0, count: h.n }).acc);
  check(`${h.row.record.weapon}: a board fold, which reports no progress, beats from inside the engine`, beats > 0, `${beats} beats over gas ${h.w.gas}`);
  check("...and a heartbeat changes no bit of score, work or gas", bits(beat) === bits(h.w), `${bits(beat)} vs ${bits(h.w)}`);
}
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
