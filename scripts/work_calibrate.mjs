// SPDX-License-Identifier: AGPL-3.0-or-later
// FITS THE TWO PRICES of a fight's work — `GAS_PRICE_PER_MILLION` and
// `RUN_PRICE` in engine/src/fight/gas.rs. `.github/workflows/work.yml` runs it;
// docs/BOARD.md §"Contribution", docs/WASM.md §"Gas".
//
//   node scripts/work_calibrate.mjs measure out=<file.json> [rows=300] [repeats=2] [share=0.25] [pkg=<dir>]
//   node scripts/work_calibrate.mjs sample out=<file.json> [days=7] [rows=400] native=<wfsim-gas> [jobs=4]
//   node scripts/work_calibrate.mjs fit <measure.json>... sample=<file.json> [total=1.015] [prices=<gas>,<run>]
//
// `measure` times the engine contributors run (the site's wasm, under V8) over
// the board's rows. `sample` reads the volunteers' credited board answers of
// the last `days` from the accounts' database (scripts/db.mjs, PG_ACCOUNTS_URL)
// and fights `rows` of their questions natively for their exact count. `fit` takes the price BETWEEN the two terms from the timing,
// and the SCALE from those answers: what they would be paid now over what they
// were paid is `total`, held to 1.00–1.20. It reports that, each account's
// ratio, and the rows that move most; no number it prints names a person.
//
// THE SAMPLE NEVER LEAVES THE JOB THAT READ IT: it joins accounts to the
// questions they answered, which nothing may publish (docs/ACCOUNTS.md), so
// work.yml runs `sample` and `fit` in one job and uploads neither the file nor a
// digest — an account's digest is keyed per run, and printed only as a count.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHmac, randomBytes } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { loadEngine } from "./wasm_engine.mjs";
import { query, close } from "./db.mjs";
import { countsOf, population, addQuestion, pricedPopulation, populationDocument } from './work_population.mjs';
import { calibrationReader } from './work_reader.mjs';

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/(\w:)/, "$1")), "..");
const arg = (name, dflt) => {
  const a = process.argv.find((x) => x.startsWith(name + "="));
  return a ? a.slice(name.length + 1) : dflt;
};
const quantile = (xs, p) => { const s = [...xs].sort((a, b) => a - b); return s[Math.floor((s.length - 1) * p)]; };

/// Every published row that can be fought again from what the file states — a
/// riven's rolls are not published — taken at an even stride, so two machines
/// measuring the same site measure the same rows.
function boardRows(want) {
  const dir = path.join(ROOT, "site/board");
  const all = [];
  for (const f of fs.readdirSync(dir).filter((f) => f.endsWith(".json")).sort()) {
    const list = JSON.parse(fs.readFileSync(path.join(dir, f), "utf8"));
    if (!Array.isArray(list)) continue;
    for (const r of list) {
      if (r.riven_pos !== undefined || r.riven !== undefined) continue;
      const record = { weapon: f.slice(0, -5) };
      for (const k of ["mods", "evolutions", "arcanes", "valence", "exilus", "grip", "loader", "wielder"]) {
        if (r[k] !== undefined) record[k] = r[k];
      }
      all.push({ record, ruler: r.benchmark || "", mode: r.mode || "base" });
    }
  }
  const stride = Math.max(1, Math.floor(all.length / want));
  return all.filter((_, i) => i % stride === 0).slice(0, want);
}

function measure() {
  const pkg = arg("pkg", "");
  const engine = loadEngine({ pkg });
  const repeats = Number(arg("repeats", "2"));
  const share = Number(arg("share", "0.25"));
  const rows = [];
  for (const row of boardRows(Number(arg("rows", "300")))) {
    const order = engine.api("/api/board/order", row);
    if (!order.ok) continue;
    const runs = Math.max(1, Math.ceil(order.request.runs * share));
    let best = Infinity;
    let acc = null;
    for (let i = 0; i < repeats; i++) {
      const t0 = process.hrtime.bigint();
      const out = engine.api("/api/board/fold", { request: order.request, from: 0, count: runs });
      best = Math.min(best, Number(process.hrtime.bigint() - t0) / 1e9);
      acc = out.acc;
    }
    if (!acc) continue;
    if (!(acc.gas > 0)) throw new Error("this engine counts no gas: build it from a source that meters (docs/WASM.md §Gas)");
    const key = `${row.record.weapon}|${row.ruler}|${row.mode}|${JSON.stringify(row.record)}`;
    rows.push({ key, gas: acc.gas, runs: acc.runs, seconds: best });
    process.stderr.write(`${rows.length}\t${best.toFixed(3)}s\t${row.record.weapon} ${row.ruler} ${row.mode}\n`);
  }
  const cpu = os.cpus()[0];
  const out = {
    wasm: engine.release.wasm || pkg,
    machine: { cpu: cpu && cpu.model, cores: os.cpus().length, node: process.version, os: `${os.platform()} ${os.release()}` },
    repeats,
    share,
    rows,
  };
  fs.writeFileSync(arg("out", "work-measure.json"), JSON.stringify(out));
  console.log(`${rows.length} rows, ${rows.reduce((a, r) => a + r.seconds, 0).toFixed(1)} s, ${out.machine.cpu}`);
}

/// An account inside one run: a digest under a key this run alone holds, so two
/// runs' samples cannot be joined on it.
const RUN_KEY = randomBytes(32);
const hidden = (id) => createHmac("sha256", RUN_KEY).update(String(id)).digest("hex").slice(0, 10);

async function sample() {
  const days = Number(arg("days", "7"));
  const until = new Date().toISOString(), since = new Date(Date.parse(until) - days * 86_400_000).toISOString();
  const want = Number(arg("rows", "400")), all = population(), reader = await calibrationReader();
  const owner = new Map();
  let after = '', size = 2000;
  try {
    for (;;) {
      let page;
      try { page = await reader.page(since, until, after, size); }
      catch (error) {
        if (error.code === '57014' && /statement timeout/.test(error.message) && size > 1) { size = Math.max(1, Math.floor(size / 2)); continue; }
        throw error;
      }
      const devices = [...new Set(page.flatMap(q => JSON.parse(q.devices)))].filter(d => !owner.has(d));
      for (let i = 0; i < devices.length; i += 500) {
        const chunk = devices.slice(i, i + 500);
        const got = await query("SELECT verifier, account FROM relation_account_compute_devices WHERE verifier IN (SELECT value FROM jsonb_array_elements_text((?1::text)::jsonb))", [JSON.stringify(chunk)], 'accounts');
        chunk.forEach(device => owner.set(device, 'unclaimed'));
        got.forEach(device => owner.set(device.verifier, hidden(device.account)));
      }
      for (const row of page) {
        if (row.id <= after) throw new Error('calibration cursor did not advance');
        after = row.id;
        addQuestion(all, { id: row.id, work: row.work, counts: row.counts ? JSON.parse(row.counts) : null,
          accounts: JSON.parse(row.devices).map(device => owner.get(device) || 'unclaimed') }, want);
      }
      if (page.length < size) break;
      if (all.questions % 10000 === 0) console.log(`calibration: ${all.questions} paid questions read`);
    }
    console.log(`calibration: ${all.questions} paid questions; loading ${all.sample.length} sample bodies`);
    for (const q of all.sample) {
      const [row] = await reader.page(since, until, q.id, 1, true, true);
      q.request = row?.id === q.id ? JSON.parse(row.request) : null;
    }
  } finally {
    await reader.close(); await close();
  }
  // Counter sums price the whole population; only a bounded uniform sample
  // retains requests. Exact sample counts replace their predicted contribution.
  const picked = all.sample.filter(q => q.request);
  const jobs = Number(arg("jobs", String(os.cpus().length)));
  const run = promisify(execFile);
  const parts = Array.from({ length: jobs }, (_, j) => picked.filter((_, i) => i % jobs === j));
  console.log(`calibration: fighting ${picked.length} native samples across ${jobs} jobs`);
  const results = await Promise.all(parts.map(async (part, j) => {
    const file = path.join(os.tmpdir(), `work-sample-${process.pid}-${j}.json`);
    fs.writeFileSync(file, JSON.stringify(part.map((q) => ({ record: q.request.record, ruler: q.request.ruler, mode: q.request.mode }))));
    try {
      const { stdout } = await run(arg("native", "wfsim-gas"), [file, "1", "1"], { maxBuffer: 1 << 28 });
      const lines = stdout.split("\n").filter(Boolean);
      if (lines.length !== part.length) throw new Error('native calibration sample count changed');
      return lines.map((line, i) => ({ id: part[i].id, ...JSON.parse(line) }));
    } finally { fs.rmSync(file, { force: true }); }
  }));
  const exact = new Map(results.flat().filter((r) => r.gas > 0).map((r) => [r.id, { gas: r.gas, runs: r.runs, key: r.key }]));
  for (const q of all.sample) q.exact = exact.get(q.id) || null;
  fs.writeFileSync(arg("out", "work-sample.json"), JSON.stringify(populationDocument(all, { since, until, days })));
  console.log(`${all.questions} questions paid since ${since}, ${owner.size} devices, ${exact.size} fought`);
}

/// Lawson–Hanson non-negative least squares, `min |A w - b|` with `w >= 0`.
function nnls(A, b) {
  const n = A[0].length;
  const w = new Array(n).fill(0);
  const P = new Set();
  const grad = () => A[0].map((_, j) => A.reduce((s, row, i) => s + row[j] * (b[i] - row.reduce((t, a, k) => t + a * w[k], 0)), 0));
  const solveOn = (set) => {
    const idx = [...set];
    const m = idx.length;
    const M = idx.map((p) => [...idx.map((q) => A.reduce((s, row) => s + row[p] * row[q], 0)), A.reduce((s, row, i) => s + row[p] * b[i], 0)]);
    for (let c = 0; c < m; c++) {
      let piv = c;
      for (let r = c + 1; r < m; r++) if (Math.abs(M[r][c]) > Math.abs(M[piv][c])) piv = r;
      [M[c], M[piv]] = [M[piv], M[c]];
      for (let r = 0; r < m; r++) {
        if (r === c || !M[c][c]) continue;
        const f = M[r][c] / M[c][c];
        for (let k = c; k <= m; k++) M[r][k] -= f * M[c][k];
      }
    }
    const z = new Array(n).fill(0);
    idx.forEach((p, r) => { z[p] = M[r][r] ? M[r][m] / M[r][r] : 0; });
    return z;
  };
  for (let iter = 0; iter < 100; iter++) {
    const g = grad();
    const scale = Math.max(...g.map(Math.abs), 1e-300);
    const cand = [...Array(n).keys()].filter((j) => !P.has(j) && g[j] > 1e-10 * scale);
    if (!cand.length) break;
    P.add(cand.reduce((a, j) => (g[j] > g[a] ? j : a)));
    for (;;) {
      const z = solveOn(P);
      const bad = [...P].filter((j) => z[j] <= 0);
      if (!bad.length) { for (let j = 0; j < n; j++) w[j] = z[j]; break; }
      const alpha = Math.min(...bad.map((j) => w[j] / (w[j] - z[j])));
      for (let j = 0; j < n; j++) w[j] += alpha * (z[j] - w[j]);
      for (const j of [...P]) if (w[j] <= 1e-15) { P.delete(j); w[j] = 0; }
    }
  }
  return w;
}

const predict = (terms, w) => terms.reduce((s, c, j) => s + c * w[j], 0);
const relErrors = (rows, w) => rows.map((r) => Math.abs(predict(r.terms, w) - r.seconds) / r.seconds);
/// Each row divided by its own time, so the fit minimises RELATIVE error and a
/// long row does not set every price.
const fitRows = (rows) => nnls(rows.map((r) => r.terms.map((c) => c / r.seconds)), rows.map(() => 1));

function fit() {
  const files = process.argv.slice(3).filter((a) => !a.includes("="));
  const runs = files.map((f) => JSON.parse(fs.readFileSync(f, "utf8")));
  if (new Set(runs.map((r) => r.wasm)).size !== 1) throw new Error("the files measured different engines");
  const byKey = new Map();
  for (const run of runs) for (const r of run.rows) {
    if (!byKey.has(r.key)) byKey.set(r.key, { ...r, times: [] });
    byKey.get(r.key).times.push(r.seconds);
  }
  // A ROW'S TWO TERMS, in the units the engine prices: millions of gas, and runs.
  const rows = [...byKey.values()]
    .filter((r) => r.times.length === runs.length)
    .map((r) => ({ ...r, terms: [r.gas / 1e6, r.runs], seconds: quantile(r.times, 0.5) }))
    .filter((r) => r.seconds > 1e-3);
  const w = fitRows(rows);
  const errs = relErrors(rows, w);
  // OUT OF SAMPLE: fit on half the rows, judge on the other half.
  const held = relErrors(rows.filter((_, i) => i % 2), fitRows(rows.filter((_, i) => i % 2 === 0)));
  console.log(`engine ${runs[0].wasm}, ${rows.length} rows, ${runs.length} machines:`);
  for (const r of runs) console.log(`  ${r.machine.cpu} · ${r.machine.cores} cores · node ${r.machine.node}`);
  console.log(`  a million gas ${(w[0] * 1e9).toFixed(1)} ns, a run ${(w[1] * 1e9).toFixed(1)} ns`);
  console.log(`  time error: in sample median ${(quantile(errs, 0.5) * 100).toFixed(1)}%  held-out median ${(quantile(held, 0.5) * 100).toFixed(1)}%  p90 ${(quantile(held, 0.9) * 100).toFixed(1)}%  max ${(quantile(held, 1) * 100).toFixed(1)}%`);

  // THE SCALE: what the last days' paid answers would be paid now, over what
  // they were. A question fought here counts exactly; the rest are predicted
  // from the counts their answers carried, by a fit on the fought ones.
  const population = JSON.parse(fs.readFileSync(arg("sample", "work-sample.json"), "utf8"));
  if (population.v !== 2) throw new Error('calibration sample must contain bounded population totals');
  const { since } = population, fought = population.sample.filter(q => q.exact && q.counts);
  if (fought.length < 2) throw new Error('not enough exact native counts to fit calibration');
  const gasFit = (qs) => nnls(qs.map((q) => countsOf(q).map((c) => c / q.exact.gas)), qs.map(() => 1));
  const gw = gasFit(fought);
  const heldGas = fought.filter((_, i) => i % 2).map((q) => Math.abs(predict(countsOf(q), gasFit(fought.filter((_, i) => i % 2 === 0))) / q.exact.gas - 1));
  const priced = pricedPopulation(population, gw), accounts = [...priced.accounts.values()];
  const oldTotal = accounts.reduce((sum, q) => sum + q.old, 0);
  const rawTotal = accounts.reduce((sum, q) => sum + q.gas / 1e6 * w[0] + q.runs * w[1], 0);
  const target = Number(arg("total", "1.015"));
  if (!(target >= 1 && target <= 1.2)) throw new Error("total must be within 1.00–1.20");
  if (!(oldTotal > 0 && rawTotal > 0)) throw new Error('calibration population has no priced work');
  const k = target * oldTotal / rawTotal;
  // `prices=<gas>,<run>` judges chosen prices, e.g. the fit's rounded, by the same report.
  const prices = arg("prices", "") ? arg("prices", "").split(",").map(Number) : w.map((x) => Math.round(x * k));
  const now = q => q.gas * prices[0] / 1e6 + q.runs * prices[1];
  const total = accounts.reduce((sum, q) => sum + now(q), 0) / oldTotal;
  console.log("");
  console.log(`answers paid since ${since}: ${population.questions} questions, ${priced.priced} priced, ${fought.length} fought exactly`);
  console.log(`  a question's gas from its counts: held-out median ${(quantile(heldGas, 0.5) * 100).toFixed(1)}%  p90 ${(quantile(heldGas, 0.9) * 100).toFixed(1)}%`);
  console.log(`  NEW TOTAL / OLD TOTAL: ${total.toFixed(4)}`);
  const byAccount = new Map(accounts.map(q => [q.account, { ...q, now: now(q) }]));
  const ratios = [...byAccount].filter(([a]) => a !== "unclaimed").map(([a, x]) => ({ a, r: x.now / x.old, n: x.answers }));
  console.log(`  per account (${ratios.length}): p05 ${quantile(ratios.map((x) => x.r), 0.05).toFixed(3)}  median ${quantile(ratios.map((x) => x.r), 0.5).toFixed(3)}  p95 ${quantile(ratios.map((x) => x.r), 0.95).toFixed(3)}`);
  const u = byAccount.get("unclaimed");
  if (u) console.log(`  unclaimed devices together: ${(u.now / u.old).toFixed(3)} over ${u.answers} answers`);
  const below = ratios.filter((x) => x.r < 0.95).sort((p, q) => p.r - q.r);
  if (below.length) console.log(`  BELOW 0.95: ${below.length} account(s), the lowest ${below[0].r.toFixed(3)} over ${below[0].n} answers`);
  console.log("  the rows that move most (fought exactly):");
  const exactNow = q => now({ gas: q.exact.gas, runs: q.exact.runs });
  for (const q of [...fought].sort((p, q2) => Math.abs(Math.log(exactNow(q2) / q2.work)) - Math.abs(Math.log(exactNow(p) / p.work))).slice(0, 20)) {
    console.log(`    ${(exactNow(q) / q.work).toFixed(3)}  ${q.exact.key}  (was ${q.work}, now ${Math.round(exactNow(q))})`);
  }
  console.log("");
  console.log(`pub const GAS_PRICE_PER_MILLION: u64 = ${prices[0].toLocaleString("en").replace(/,/g, "_")};`);
  console.log(`pub const RUN_PRICE: u64 = ${prices[1].toLocaleString("en").replace(/,/g, "_")};`);
}

if (process.argv[2] === "measure") measure();
else if (process.argv[2] === "sample") await sample();
else if (process.argv[2] === "fit") fit();
else { console.error("usage: work_calibrate.mjs measure|sample|fit …"); process.exit(2); }
