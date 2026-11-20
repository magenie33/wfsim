// SPDX-License-Identifier: AGPL-3.0-or-later
// Runs the foreground fleet in an isolated Chromium profile against local release assets.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { openApp } from "./cdp.mjs";

const src = file => readFile(new URL("../" + file, import.meta.url), "utf8");
const sleep = ms => new Promise(r => setTimeout(r, ms));
const digest = data => createHash("sha256").update(data).digest("hex");

// The body of this function runs in the page, alongside the actual fleet and storage module.
function installRunner() {
  let job, session, begin, measured, stats, pending;
  const rawOpen = typeof openSearchWarmCache === "function" && openSearchWarmCache;
  if (rawOpen) openSearchWarmCache = async (...args) => {
    session = await rawOpen(...args);
    if (session) {
      const lookup = session.lookup.bind(session);
      session.lookup = (keys, runs) => {
        const rows = lookup(keys, runs);
        stats.lookups += rows.length;
        stats.hits += rows.filter(Boolean).length;
        return rows;
      };
    }
    return session;
  };
  const encoder = new TextEncoder();
  async function hash(data) {
    const bytes = await crypto.subtle.digest("SHA-256", encoder.encode(JSON.stringify(data)));
    return Array.from(new Uint8Array(bytes), n => n.toString(16).padStart(2, "0")).join("");
  }
  window.spawnEngine = () => {
    const w = new Worker("/worker.js");
    window.loaded.then(([module, data]) => w.postMessage({kind: "engine", module, packs: [new Uint8Array(data)]}),
      () => w.postMessage({kind: "engine", cause: "network"}));
    const send = w.postMessage.bind(w);
    w.postMessage = (m, ...args) => {
      const q = m.body?.quick_fleet;
      if (q?.lead && window.checkRounds) pending.push(hash(q.scores.map(row => [row.key,row.score,row.order,row.sd,row.rough || false])));
      if (m.kind === "verify_final") { stats.finals++; stats.finalStart ??= performance.now() - begin; }
      send(m, ...args);
    };
    return w;
  };
  window.startRun = async (request, workers, options, audit) => {
    window.checkRounds = audit;
    pending = []; measured = null; session = null;
    stats = { hits: 0, lookups: 0, finals: 0, finalStart: null };
    begin = performance.now();
    window.loaded = Promise.all([WebAssembly.compileStreaming(fetch("/engine.wasm")), fetch("/data").then(r => r.arrayBuffer())]);
    const normalizer = window.spawnEngine();
    let deadline, frozen;
    try {
      frozen = await new Promise((resolve, reject) => {
        deadline = setTimeout(() => reject(Error("Search input normalization timed out")), 60_000);
        normalizer.onmessage = ({data}) => {
          if (data.id === 1) {
            if (data.payload?.ok) resolve(data.payload.request);
            else reject(Error(data.payload?.error || "Could not normalize the search request"));
          }
          if (data.kind === "load_failed") reject(Error("Cannot load the normalizer"));
        };
        normalizer.onerror = event => reject(Error(event.message));
        normalizer.postMessage({id: 1, kind: "api", path: "/api/computation/input", body: {kind: "optimization", request}});
      });
    } finally { clearTimeout(deadline); normalizer.terminate(); }
    job = quickFleet(frozen, workers, undefined, { searchWarm: options });
  };
  window.progress = () => {
    if (job.result && measured == null) measured = performance.now() - begin;
    return { result: !!job.result, phase: job.status?.activity, builds: job.status?.enumerated, fights: job.status?.sims_done, steps: job.status?.batches_completed };
  };
  window.outcome = async capture => {
    if (capture && session) window.exported = await session.exportPack();
    return { result: job.result, stats, wallMs: measured, scoreDigest: await hash(await Promise.all(pending)), entryCount: window.exported?.entries.length, identity: window.exported?.identity };
  };
}

export async function runSearchCache({ site = fileURLToPath(new URL("../site/", import.meta.url)), request, official = null,
  workers = 10, mode = "cold", cacheRoot, wasm, bind, engine, audit = false, localRepeat = false, onProgress, timeoutMs = 6 * 3600000, storageOptions = {} }) {
  if (!Number.isInteger(workers) || workers < 1 || workers > 10 || !["cold", "capture", "official", "local"].includes(mode)) throw Error("Invalid cache run configuration");
  const readSite = name => readFile(resolve(site, name));
  const rel = JSON.parse(await readSite("release.json"));
  const module = wasm ? await readFile(wasm) : await readSite("pkg/wfsim_wasm_bg." + rel.wasm + ".wasm");
  const binding = bind ? await readFile(bind) : await readSite("pkg/wfsim_wasm." + rel.wasm + ".js");
  const binary = digest(module).slice(0, 12);
  const data = await readSite("pkg/" + rel.packs.find(name => name.startsWith("pack.data.")));
  if (binary !== rel.wasm || digest(data).slice(0, 12) !== rel.data) {
    throw Error("The WASM or battle data does not match the local release manifest");
  }
  const transport = await src("web/src/static/app/06-transport.js");
  const watch = transport.slice(transport.indexOf("const LANE_WATCHDOG"), transport.indexOf("/// WHERE A WORKER"));
  const fleet = await src("web/src/static/app/08-checkpoint-api.js");
  const cache = mode === "cold" ? "" : await src("web/src/static/app/07-warm-search-cache.js");
  const gen = JSON.parse(await readSite("pkg/generation.json"));
  const m = JSON.parse(await readSite("pkg/" + gen.names.find(n => /^meta\./.test(n))));
  const meta = JSON.stringify({ benchmarks: m.benchmarks });
  const assets = new Map([
    ["/", ["text/html", '<script src="/h.js"></script>']],
    ["/engine.wasm", ["application/wasm", module]],
    ["/pkg/wfsim_wasm.js", ["application/javascript", binding]],
    ["/data", ["application/json", data]],
    ["/worker.js", ["application/javascript", await src("web/src/static/worker.js")]],
    ["/h.js", ["application/javascript", ['let woptNextId=1; const computeRun=async(_,call,live)=>live()?call():{cancelled:true};',
      'const META=' + meta + ';',watch,cache,fleet,'(' + installRunner.toString() + ')()'].join("\n")]]
  ]);
  let requests = [];
  const server = createServer(async (req, res) => {
    if (req.method !== "GET") return res.writeHead(405).end();
    const url = new URL(req.url, "http://localhost");
    const fixed = assets.get(url.pathname);
    if (fixed) { res.writeHead(200, {"content-type": fixed[0]}); return res.end(fixed[1]); }
    if (url.pathname.startsWith("/search-cache/") && cacheRoot) {
      const key = url.pathname.slice("/search-cache/".length);
      const file = resolve(cacheRoot, key);
      if (/^[a-zA-Z0-9_.\/-]+$/.test(key) && file.startsWith(resolve(cacheRoot) + sep)) {
        try {
          const bytes = await readFile(file);
          requests.push({key, status: 200, bytes: bytes.length});
          res.writeHead(200, {"content-type": key.endsWith(".json") ? "application/json" : "application/octet-stream"});
          return res.end(bytes);
        } catch { /* an unpublished scenario runs cold */ }
      }
    }
    if (url.pathname !== "/favicon.ico") requests.push({key: url.pathname, status: 404, bytes: 0});
    res.writeHead(404).end();
  });
  await new Promise((resolve, reject) => server.once("error", reject).listen(0, "127.0.0.1", resolve));
  let app;
  try {
    app = await openApp({base: "http://127.0.0.1:" + server.address().port, boot: 0});
    const run = async () => {
      requests = [];
      const options = mode === "cold" ? null : { official, data: rel.data, wasm: binary,
        engine: engine || rel.engine, baseUrl: "/search-cache/", capture: mode === "capture", ...storageOptions };
      const from = Date.now();
      while (!await app.evaluate('typeof startRun === "function"')) {
        if (Date.now() - from > 15000) throw Error("The benchmark page failed to load.");
        await sleep(40);
      }
      await app.evaluate("startRun(" + [request, workers, options, audit].map(x => JSON.stringify(x)).join(",") + ")");
      let nextUpdate = 0;
      for (;;) {
        const p = await app.evaluate("progress()");
        if (p.result) break;
        if (Date.now() > nextUpdate) { onProgress?.({...p, ms: Date.now()-from}); nextUpdate = Date.now() + 15000; }
        if (Date.now() - from > timeoutMs) throw Error("Search deadline exceeded.");
        await sleep(100);
      }
      const capture = mode === "capture";
      const out = await app.evaluate("outcome(" + capture + ")");
      if (!out.result?.ok) throw Error(out.result?.error || "Optimizer returned no result");
      if (capture) {
        if (!out.entryCount) throw Error("The capture produced no search shards. Check the WASM and cache identity.");
        const entries = [];
        for (let i=0; i<out.entryCount; i+=256) entries.push(...await app.evaluate("window.exported.entries.slice(" + i + "," + (i + 256) + ")"));
        out.pack = {version: 1, identity: out.identity, entries};
      }
      return {...out, requests: [...requests], engine: engine || rel.engine, data: rel.data, wasm: binary};
    };
    const first = await run();
    if (localRepeat) { await app.load("/", 0); first.repeat = await run(); }
    return first;
  } finally {
    try { await app?.close(); } finally { server.closeAllConnections(); await new Promise(r => server.close(r)); }
  }
}
