// SPDX-License-Identifier: AGPL-3.0-or-later
// FITS THE TWO PRICES of a fight's work — `GAS_PRICE_PER_MILLION` and
// `RUN_PRICE` in engine/src/fight/gas.rs. `.github/workflows/work.yml` runs it;
// docs/BOARD.md §"Contribution", docs/WASM.md §"Gas".
//
//   node scripts/work_calibrate.mjs measure out=<file.json> [rows=300] [repeats=2] [share=0.25] [pkg=<dir>]
//   node scripts/work_calibrate.mjs sample out=<file.json> [days=7] [rows=400] native=<wfsim-gas> [jobs=4]
//   node scripts/work_calibrate.mjs fit <measure.json>... sample=<file.json> [total=1.015]
//
// `measure` times the engine contributors run (the site's wasm, under V8) over
// the board's rows. `sample` reads the volunteers' credited board answers of
// the last `days` from the database (CF_ACCOUNT, CF_TOKEN, CF_D1_DATABASE,
// CF_D1_ACCOUNTS) and fights `rows` of their questions natively for their
// exact count. `fit` takes the price BETWEEN the two terms from the timing,
// and the SCALE from those answers: what they would be paid now over what they
// were paid is `total`, held to 1.00–1.03. It reports that, each account's
// ratio, and the rows that move most; no number it prints names a person.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { loadEngine } from "./wasm_engine.mjs";

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

/// ONE QUERY TO A DATABASE over Cloudflare's API, the way scripts/fetch_facts.sh asks.
async function d1(database, sql, params = []) {
  const url = `https://api.cloudflare.com/client/v4/accounts/${process.env.CF_ACCOUNT}/d1/database/${database}/query`;
  const r = await fetch(url, { method: "POST", headers: { authorization: `Bearer ${process.env.CF_TOKEN}`, "content-type": "application/json" },
    body: JSON.stringify({ sql, params }), signal: AbortSignal.timeout(180_000) });
  const j = await r.json();
  if (!j.success) throw new Error(`d1: ${JSON.stringify(j.errors)}`);
  return j.result[0].results;
}

/// An account as the report shows it: a digest, never the id.
const hidden = (id) => createHash("sha256").update(String(id)).digest("hex").slice(0, 10);

async function sample() {
  const days = Number(arg("days", "7"));
  const since = new Date(Date.now() - days * 86_400_000).toISOString();
  // EVERY QUESTION A VOLUNTEER WAS PAID FOR, with what each answer was paid
  // and the counts it was weighed by, a page at a time.
  const questions = [];
  for (let offset = 0; ; offset += 2000) {
    const page = await d1(process.env.CF_D1_DATABASE,
      `SELECT q.id AS id, q.request AS request, max(a.work) AS work, max(a.counts) AS counts, json_group_array(a.device) AS devices
         FROM answers a JOIN questions q ON q.id = a.question
        WHERE a.credited = 1 AND a.official = 0 AND a.void = 0 AND q.verb = 'simulate' AND a.at > ?1
        GROUP BY q.id ORDER BY q.id LIMIT 2000 OFFSET ?2`, [since, offset]);
    questions.push(...page.map((q) => ({ id: q.id, request: JSON.parse(q.request), work: q.work, counts: q.counts ? JSON.parse(q.counts) : null, devices: JSON.parse(q.devices) })));
    if (page.length < 2000) break;
  }
  const devices = [...new Set(questions.flatMap((q) => q.devices))];
  const owner = new Map();
  for (let i = 0; i < devices.length; i += 500) {
    const got = await d1(process.env.CF_D1_ACCOUNTS, "SELECT verifier, account FROM devices WHERE verifier IN (SELECT value FROM json_each(?1))",
      [JSON.stringify(devices.slice(i, i + 500))]);
    for (const d of got) owner.set(d.verifier, hidden(d.account));
  }
  // A BROWSER'S ID IS ITS SECRET (docs/BOARD.md §"Contribution"): what leaves
  // here is how many were paid and their accounts' digests, never an id.
  for (const q of questions) {
    q.accounts = q.devices.map((d) => owner.get(d) || "unclaimed");
    delete q.devices;
  }
  // THE EXACT COUNT of an even sample of them, fought natively: the native
  // engine counts what the wasm does (check_gas), and a full row's runs.
  const want = Number(arg("rows", "400"));
  const stride = Math.max(1, Math.floor(questions.length / want));
  const picked = questions.filter((_, i) => i % stride === 0).slice(0, want);
  const jobs = Number(arg("jobs", String(os.cpus().length)));
  const run = promisify(execFile);
  const parts = Array.from({ length: jobs }, (_, j) => picked.filter((_, i) => i % jobs === j));
  const results = await Promise.all(parts.map(async (part, j) => {
    const file = path.join(os.tmpdir(), `work-sample-${process.pid}-${j}.json`);
    fs.writeFileSync(file, JSON.stringify(part.map((q) => ({ record: q.request.record, ruler: q.request.ruler, mode: q.request.mode }))));
    const { stdout } = await run(arg("native", "wfsim-gas"), [file, "1", "1"], { maxBuffer: 1 << 28 });
    return stdout.split("\n").filter(Boolean).map((l, i) => ({ id: part[i].id, ...JSON.parse(l) }));
  }));
  const exact = new Map(results.flat().filter((r) => r.gas > 0).map((r) => [r.id, { gas: r.gas, runs: r.runs, key: r.key }]));
  for (const q of questions) q.exact = exact.get(q.id) || null;
  fs.writeFileSync(arg("out", "work-sample.json"), JSON.stringify({ since, days, questions }));
  console.log(`${questions.length} questions paid since ${since}, ${devices.length} devices, ${exact.size} fought`);
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
const COUNTS = ["runs", "shots", "pellets", "procs", "dot_ticks", "field_ticks", "kills", "hits", "body_shots"];

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
  const { questions, since } = JSON.parse(fs.readFileSync(arg("sample", "work-sample.json"), "utf8"));
  const fought = questions.filter((q) => q.exact && q.counts);
  const countsOf = (q) => COUNTS.map((c) => (q.counts && q.counts[c]) || 0);
  const gasFit = (qs) => nnls(qs.map((q) => countsOf(q).map((c) => c / q.exact.gas)), qs.map(() => 1));
  const gw = gasFit(fought);
  const heldGas = fought.filter((_, i) => i % 2).map((q) => Math.abs(predict(countsOf(q), gasFit(fought.filter((_, i) => i % 2 === 0))) / q.exact.gas - 1));
  const gasOf = (q) => (q.exact ? q.exact.gas : q.counts ? predict(countsOf(q), gw) : null);
  const runsOf = (q) => (q.exact ? q.exact.runs : (q.counts && q.counts.runs) || 0);
  const priced = questions.filter((q) => gasOf(q) !== null && q.work > 0);
  const raw = (q) => (gasOf(q) / 1e6) * w[0] + runsOf(q) * w[1];
  const paid = (q) => q.work * q.accounts.length;
  const target = Number(arg("total", "1.015"));
  if (!(target >= 1 && target <= 1.03)) throw new Error("total must be within 1.00–1.03");
  const k = (target * priced.reduce((s, q) => s + paid(q), 0)) / priced.reduce((s, q) => s + raw(q) * q.accounts.length, 0);
  const prices = w.map((x) => Math.round(x * k));
  const now = (q) => (gasOf(q) * prices[0]) / 1e6 + runsOf(q) * prices[1];
  const total = priced.reduce((s, q) => s + now(q) * q.accounts.length, 0) / priced.reduce((s, q) => s + paid(q), 0);
  console.log("");
  console.log(`answers paid since ${since}: ${questions.length} questions, ${priced.length} priced, ${fought.length} fought exactly`);
  console.log(`  a question's gas from its counts: held-out median ${(quantile(heldGas, 0.5) * 100).toFixed(1)}%  p90 ${(quantile(heldGas, 0.9) * 100).toFixed(1)}%`);
  console.log(`  NEW TOTAL / OLD TOTAL: ${total.toFixed(4)}`);
  const byAccount = new Map();
  for (const q of priced) q.accounts.forEach((a) => {
    const x = byAccount.get(a) || { old: 0, now: 0, answers: 0 };
    x.old += q.work; x.now += now(q); x.answers++;
    byAccount.set(a, x);
  });
  const ratios = [...byAccount].filter(([a]) => a !== "unclaimed").map(([a, x]) => ({ a, r: x.now / x.old, n: x.answers }));
  console.log(`  per account (${ratios.length}): p05 ${quantile(ratios.map((x) => x.r), 0.05).toFixed(3)}  median ${quantile(ratios.map((x) => x.r), 0.5).toFixed(3)}  p95 ${quantile(ratios.map((x) => x.r), 0.95).toFixed(3)}`);
  const u = byAccount.get("unclaimed");
  if (u) console.log(`  unclaimed devices together: ${(u.now / u.old).toFixed(3)} over ${u.answers} answers`);
  for (const x of ratios.filter((x) => x.r < 0.95).sort((p, q) => p.r - q.r)) console.log(`  BELOW 0.95: account ${x.a}  ${x.r.toFixed(3)} over ${x.n} answers`);
  console.log("  the rows that move most (fought exactly):");
  for (const q of [...fought].sort((p, q2) => Math.abs(Math.log(now(q2) / q2.work)) - Math.abs(Math.log(now(p) / p.work))).slice(0, 20)) {
    console.log(`    ${(now(q) / q.work).toFixed(3)}  ${q.exact.key}  (was ${q.work}, now ${Math.round(now(q))})`);
  }
  console.log("");
  console.log(`pub const GAS_PRICE_PER_MILLION: u64 = ${prices[0].toLocaleString("en").replace(/,/g, "_")};`);
  console.log(`pub const RUN_PRICE: u64 = ${prices[1].toLocaleString("en").replace(/,/g, "_")};`);
}

if (process.argv[2] === "measure") measure();
else if (process.argv[2] === "sample") await sample();
else if (process.argv[2] === "fit") fit();
else { console.error("usage: work_calibrate.mjs measure|sample|fit …"); process.exit(2); }
