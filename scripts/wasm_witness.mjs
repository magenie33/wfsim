// SPDX-License-Identifier: AGPL-3.0-or-later
// THE OFFICIAL WITNESS FIGHTS ON THE ENGINE THE VOLUNTEERS RUN — the site's
// own wasm, under node, along the page's own path (69-board-work.js
// `measureRow`): `/api/board/order`, `/api/board/runs` a shard per run on
// every lane, `/api/board/fold` in run order, `/api/board/score`. Its answer is
// the page's to the bit, work included, which a native build cannot give
// (docs/BOARD.md §"Contribution").
//
//   node wasm_witness.mjs <rows.ndjson> [lanes=<n>] [pkg=<dir>] [site=<dir|url>]
//     each line { record, ruler, mode, identity? } → { identity, ruler, mode, score, metric, work, counts }
//
// `pkg` is a wasm-bindgen output. `site` is a site's root — a directory holding
// `release.json` and `pkg/` as the live server keeps one beside the scorer, or
// an origin (wfsim.app unless set), its engine fetched once into `.wasm-witness/`;
// either way a module whose name is not its hash is never run.
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import vm from "node:vm";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { fileURLToPath, pathToFileURL } from "node:url";

const SITE = process.env.WFSIM_SITE || "https://wfsim.app";
const HERE = path.dirname(fileURLToPath(import.meta.url));
const CACHE = path.join(HERE, "..", ".wasm-witness");

/// THE ENGINE AS SOURCE: its glue and its module, from a directory or from the site.
export async function enginePackage(pkg = "", site = SITE) {
  if (pkg) {
    const wasm = fs.readFileSync(path.join(pkg, "wfsim_wasm_bg.wasm"));
    return { js: fs.readFileSync(path.join(pkg, "wfsim_wasm.js"), "utf8"), wasm,
      id: createHash("sha256").update(wasm).digest("hex").slice(0, 12), engine: null };
  }
  const local = !/^https?:/.test(site);
  const rel = local ? JSON.parse(fs.readFileSync(path.join(site, "release.json"), "utf8"))
    : await (await fetch(`${site}/release.json`, { cache: "no-store" })).json();
  fs.mkdirSync(CACHE, { recursive: true });
  const get = async (name) => {
    if (local) return fs.readFileSync(path.join(site, "pkg", name));
    const at = path.join(CACHE, name);
    if (!fs.existsSync(at)) {
      const r = await fetch(`${site}/pkg/${name}`);
      if (!r.ok) throw new Error(`${name}: ${r.status}`);
      fs.writeFileSync(at, Buffer.from(await r.arrayBuffer()));
    }
    return fs.readFileSync(at);
  };
  const wasm = await get(`wfsim_wasm_bg.${rel.wasm}.wasm`);
  // A FILE IS ITS HASH: a cached or served one that is not is never run.
  if (createHash("sha256").update(wasm).digest("hex").slice(0, 12) !== rel.wasm) throw new Error(`wasm ${rel.wasm}: hash differs`);
  return { js: (await get(`wfsim_wasm.${rel.wasm}.js`)).toString("utf8"), wasm, id: rel.wasm, engine: rel.engine, commit: rel.commit || null };
}

/// ONE INSTANCE, called as the page's lane calls its worker.
function instance({ js, wasm }) {
  const ctx = vm.createContext({ console, TextEncoder, TextDecoder, WebAssembly });
  vm.runInContext(js + "\nthis.wasm_bindgen = wasm_bindgen;", ctx);
  ctx.bytes = wasm;
  vm.runInContext("wasm_bindgen.initSync({ module: bytes })", ctx);
  return (p, body) => {
    const r = JSON.parse(ctx.wasm_bindgen.api(p, JSON.stringify(body)));
    if (!r || r.ok === false) throw new Error(`${p}: ${(r && r.error) || "failed"}`);
    return r;
  };
}

if (!isMainThread) {
  const api = instance(workerData);
  parentPort.on("message", ({ id, request, from, count }) => {
    try { parentPort.postMessage({ id, shards: api("/api/board/runs", { request, from, count }).shards }); }
    catch (e) { parentPort.postMessage({ id, error: e.message }); }
  });
}

/// THE WITNESS: an engine on `lanes` workers, and `row(r)` fighting one row.
export async function witness({ lanes = os.cpus().length, pkg = "", site = SITE } = {}) {
  const src = await enginePackage(pkg, site);
  const api = instance(src);
  const workers = Array.from({ length: Math.max(1, lanes) }, () => new Worker(fileURLToPath(import.meta.url), { workerData: src }));
  let next = 0;
  const waiting = new Map();
  for (const w of workers) {
    w.on("message", (m) => { const p = waiting.get(m.id); waiting.delete(m.id); m.error ? p.reject(new Error(m.error)) : p.resolve(m.shards); });
  }
  const runs = (w, request, from, count) => new Promise((resolve, reject) => {
    const id = next++;
    waiting.set(id, { resolve, reject });
    w.postMessage({ id, request, from, count });
  });
  async function row({ record, ruler, mode, identity }) {
    const { request } = api("/api/board/order", { record, ruler, mode });
    const total = Number(request.runs) || 0;
    // A SHARD PER RUN, whatever lane fought it: how the runs are split never
    // moves a bit, the order they are folded in does.
    const shards = new Array(total);
    let cursor = 0;
    const step = Math.max(1, Math.ceil(total / (workers.length * 8)));
    await Promise.all(workers.map(async (w) => {
      while (cursor < total) {
        const from = cursor, count = Math.min(step, total - from);
        cursor += count;
        const got = await runs(w, request, from, count);
        if (!Array.isArray(got) || got.length !== count) throw new Error("runs: wrong shard count");
        got.forEach((s, i) => { shards[from + i] = s; });
      }
    }));
    let acc = null;
    for (let i = 0; i < total; i += 256) acc = api("/api/board/fold", { request, acc, pieces: shards.slice(i, i + 256) }).acc;
    const s = api("/api/board/score", { ruler, request, acc });
    return { identity, ruler, mode, score: s.score, metric: s.metric, work: s.work, counts: s.counts };
  }
  return { row, id: src.id, engine: src.engine, close: () => Promise.all(workers.map((w) => w.terminate())) };
}

if (isMainThread && process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const file = process.argv[2];
  const opt = (k, d) => (process.argv.find((a) => a.startsWith(`${k}=`)) || `${k}=${d}`).slice(k.length + 1);
  if (!file) { console.error("usage: wasm_witness.mjs <rows.ndjson> [lanes=<n>] [pkg=<dir>]"); process.exit(2); }
  const w = await witness({ lanes: Number(opt("lanes", os.cpus().length)), pkg: opt("pkg", ""), site: opt("site", SITE) });
  try {
    for (const line of fs.readFileSync(file, "utf8").split("\n").filter(Boolean)) console.log(JSON.stringify(await w.row(JSON.parse(line))));
  } finally { await w.close(); }
}
