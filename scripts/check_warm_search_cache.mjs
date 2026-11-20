// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import { resolve } from "node:path";
import { Readable } from "node:stream";
import { Script } from "node:vm";
import { createGzip, gzipSync } from "node:zlib";
import { APP_PARTS_DIR, appParts, appSource } from "./app_source.mjs";
import { openApp } from "./cdp.mjs";

const part = "07-warm-search-cache.js";
const source = readFileSync(resolve(APP_PARTS_DIR, part), "utf8");
const joined = appSource();
assert.ok(appParts().includes(part) && joined.includes(source));
assert.ok(joined.indexOf(source) < joined.indexOf("function quickFleet("));
new Script(joined, { filename: "concatenated app.js" });

const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const engine = "111111111111", data = "222222222222", wasm = "333333333333", release = "444444444444";
const official = "multi_target_aimed";
const body = { weapon: "ocucor", headshot_pct: 50, enemy: "thrax", limits: { forbidden: ["example"] },
  candidate_runs: 10, __weapon: "ocucor", also_acting: [{ weapon: "torid", mods: ["one", "two"] }],
  arena: { "2": 20, "10": 10 }, next_battle_field: { enabled: true } };
const scenario = sha('{"also_acting":[{"mods":["one","two"],"weapon":"torid"}],"arena":{"10":10,"2":20},"enemy":"thrax","headshot_pct":50,"next_battle_field":{"enabled":true}}');
const identity = { engine, data, wasm, weapon: body.weapon, official, scenario };
const identityId = sha(JSON.stringify({ data, engine, official, scenario, wasm, weapon: body.weapon }));
const receipt = (key, runs, gas, pad = "") => JSON.stringify([1, [401, 8], {
  runs, gas, warm_key: key, ttks: [], by_body: [], by_combatant: [], seat_counters: [], pad,
  index: Array.from({ length: runs }, () => ({ v: 1, hi: 1, lo: 2 })),
  series: { kill_progress: [], effective: [] },
}]);
const exact = receipt("q1:a", 2, 9), full = receipt("q1:a", 10, 30);
const longEntries = Array.from({ length: 104 }, (_, i) => {
  const key = `q1:large-${i}`;
  return [key, 1, receipt(key, 1, i, "x".repeat(90_000))];
});
const pack = { version: 1, identity, entries: [["q1:a", 2, exact], ["q1:a", 10, full], ...longEntries] };
assert.ok(Buffer.byteLength(JSON.stringify(pack)) > 8 * 1024 ** 2);
const gzip = gzipSync(JSON.stringify(pack));
const goodAsset = { bytes: gzip.length, sha256: sha(gzip) };
const root = `/search-cache/v1/${release}/`;
const requests = [];
let manifest = { version: 1, packs: { [identityId]: goodAsset, ["0".repeat(64)]: { ...goodAsset, sha256: "0".repeat(64) } } };
let servedGzip = gzip;
let assetEncoding;
let manifestStatus = 200, assetStatus = 200;
let conditionalManifest = 0;
const boot = `const ENGINE_ID = ${JSON.stringify(engine)}, RELEASE_ID = ${JSON.stringify(release)};
const ENGINE_WASM = "/pkg/wfsim_wasm_bg.${wasm}.wasm", ENGINE_PACKS = ["/pkg/pack.data.${data}.json"];
const actualNow = Date.now;
window.warmClockOffset = 0;
Date.now = () => actualNow() + window.warmClockOffset;
${source}`;
const server = createServer((req, res) => {
  const url = new URL(req.url, "http://127.0.0.1");
  if (url.pathname === "/") return res.writeHead(200, { "content-type": "text/html", "set-cookie": "private=yes" }).end('<script src="/warm.js"></script>');
  if (url.pathname === "/warm.js") return res.writeHead(200, { "content-type": "application/javascript" }).end(boot);
  if (url.pathname === "/favicon.ico") return res.writeHead(404).end();
  requests.push({ path: req.url, method: req.method, cookie: req.headers.cookie, referrer: req.headers.referer });
  if (url.pathname === root + "manifest.json") {
    if (manifestStatus !== 200) return res.writeHead(manifestStatus, { "cache-control": "no-store" }).end();
    const json = JSON.stringify(manifest), etag = `"${sha(json)}"`;
    const headers = { "content-type": "application/json", "cache-control": "max-age=0, must-revalidate", etag };
    if (req.headers["if-none-match"] === etag) {
      conditionalManifest++;
      return res.writeHead(304, headers).end();
    }
    return res.writeHead(200, headers).end(json);
  }
  if (url.pathname === root + goodAsset.sha256 + ".json.gz" || url.pathname === root + manifest.packs?.[identityId]?.sha256 + ".json.gz") {
    if (assetStatus !== 200) return res.writeHead(assetStatus, { "cache-control": "no-store" }).end();
    const head = { "content-type": "application/octet-stream" };
    if (assetEncoding) head["content-encoding"] = assetEncoding;
    return res.writeHead(200, head).end(servedGzip);
  }
  return res.writeHead(404).end();
});
await new Promise((done, fail) => server.once("error", fail).listen(0, "127.0.0.1", done));

let app;
try {
  app = await openApp({ base: `http://127.0.0.1:${server.address().port}`, boot: 100 });
  const browser = (code, input = {}) => app.evaluate(`(${code})(${JSON.stringify(input)})`);
  const fixed = { body, official, identityId, exact, full };
  const coldMiss = ({ body, official }) => {
    searchWarmRetry.clear();
    return openSearchWarmCache(body, { official, indexedDB: null }).then(cache => cache.lookup(["q1:a"], 20));
  };
  const first = await browser(async ({ body, official }) => {
    const cache = await openSearchWarmCache(body, { official, localBytes: 1024 });
    await cache.flush();
    return { identity: cache.exportPack().identity, exact: cache.lookup(["q1:a", null], 2),
      prefix: cache.lookup(["q1:a"], 9), full: cache.lookup(["q1:a"], 10), tooLong: cache.lookup(["q1:a"], 1),
      large: cache.lookup(["q1:large-103"], 1)[0]?.length };
  }, fixed);
  assert.deepEqual(first, { identity, exact: [exact, null], prefix: [exact], full: [full], tooLong: [null], large: longEntries.at(-1)[2].length });
  assert.deepEqual(requests.splice(0), [root + "manifest.json", root + goodAsset.sha256 + ".json.gz"]
    .map(path => ({ path, method: "GET", cookie: undefined, referrer: undefined })));

  await app.load("/", 100);
  assert.deepEqual(await browser(async ({ body, official }) => {
    const cache = await openSearchWarmCache({ ...body, limits: { require: ["different"] }, starts: [{}] }, { official });
    return cache.lookup(["q1:a"], 11);
  }, fixed), [full]);
  assert.deepEqual(requests, []);

  const storage = await browser(async ({ body, official, exact, full }) => {
    const local = { remote: false };
    const [a, b] = await Promise.all([openSearchWarmCache(body, local), openSearchWarmCache(body, local)]);
    a.record(["q1:a", "wrong-key"], [exact, full]);
    const other = full.replace('"q1:a"', '"q1:b"');
    b.record(["q1:b"], [other]);
    await Promise.all([a.flush(), b.flush()]);
    const find = async (request, options = local) => (await openSearchWarmCache(request, options)).lookup(["q1:a", "q1:b", "wrong-key"], 10);
    const privateState = await find({ ...body, limits: {}, mode: "alternate", seed: 9, valence_bonus: 55, rivens: [] });
    const officialLocal = await find(body, { ...local, official });
    const fresh = await Promise.all([
      find({ ...body, next_battle_field: { enabled: false } }),
      find({ ...body, also_acting: [{ weapon: "torid", mods: ["two", "one"] }] }),
      find({ ...body, headshot_pct: 100 }), find({ ...body, weapon: "torid" }),
      find({ ...body, buffs: { frenzy: { initial_stacks: 1 } } }),
      find({ ...body, shards: [{ effect: "secondary_crit_chance", tauforged: true }] }),
      find(body, { ...local, data: "555555555555" }), find(body, { ...local, engine: "555555555555" }),
      find(body, { ...local, wasm: "555555555555" }),
    ]);
    return { privateState, officialLocal, fresh };
  }, fixed);
  assert.deepEqual(storage.privateState, [exact, full.replace('"q1:a"', '"q1:b"'), null]);
  assert.deepEqual(storage.officialLocal, [null, null, null]);
  assert.ok(storage.fresh.every(row => row.every(x => x === null)));
  assert.deepEqual(requests, []);

  const capture = await browser(async ({ body, exact }) => {
    const seen = [];
    const cache = await openSearchWarmCache(body, { indexedDB: null, localBytes: 1024, capture: true,
      onRecord: (key, runs) => seen.push([key, runs]) });
    const hi = exact.replace('"q1:a"', '"q1:high"').replace('"gas":9', '"gas":9007199254740993');
    const low = Array.from({ length: 20 }, (_, i) => `q1:low-${i}`);
    const keys = ["q1:high", ...low];
    cache.record(keys, [hi, ...low.map(key => exact.replace('"q1:a"', JSON.stringify(key)))]);
    const exported = cache.exportPack();
    exported.entries[0][2] = "modified export";
    return { seen, count: exported.entries.length, high: cache.lookup(["q1:high"], 2)[0],
      exportedHigh: cache.exportPack().entries[0][2], lowest: cache.lookup(low, 2).filter(Boolean).length };
  }, fixed);
  assert.equal(capture.seen.length, 21);
  assert.equal(capture.count, 21);
  assert.match(capture.high, /9007199254740993/);
  assert.equal(capture.exportedHigh, capture.high);
  assert.ok(capture.lowest < 20);

  const evicted = await browser(async ({ body, exact }) => {
    const opts = { localBytes: 980, diskBytes: 2000 };
    const context = (i) => ({ ...body, level: i });
    for (let i = 0; i < 3; i++) {
      const cache = await openSearchWarmCache(context(i), opts);
      cache.record(["q1:a"], [exact]);
      await cache.flush();
    }
    return await Promise.all([0, 1, 2].map(async i => (await openSearchWarmCache(context(i), opts)).lookup(["q1:a"], 2)[0]));
  }, fixed);
  assert.deepEqual(evicted, [null, null, exact]);

  manifest = { version: 1, packs: {} };
  assert.deepEqual(await browser(coldMiss, fixed), [null]);
  assert.deepEqual(requests.splice(0).map(req => req.path), [root + "manifest.json"]);
  manifest.packs[identityId] = goodAsset;
  const broken = Buffer.from(gzip);
  broken[broken.length - 12] ^= 0xff;
  for (const sample of [broken, gzip.subarray(0, -5), Buffer.concat([gzip, Buffer.of(1)])]) {
    servedGzip = sample;
    assert.deepEqual(await browser(coldMiss, fixed), [null]);
  }
  servedGzip = gzip;
  assetEncoding = "gzip";
  assert.deepEqual(await browser(coldMiss, fixed), [null]);
  assetEncoding = undefined;
  assert.equal(requests.splice(0).length, 8);

  function useGzip(bytes) {
    servedGzip = bytes;
    manifest.packs[identityId] = { bytes: bytes.length, sha256: sha(bytes) };
  }
  const wrongIdentity = { ...identity, scenario: "0".repeat(64) };
  for (const invalid of [
    { ...pack, identity: wrongIdentity },
    { ...pack, entries: [["q1:a", 10, exact]] },
    { ...pack, entries: [["q1:a", 2, exact], ["q1:a", 2, exact]] },
    { ...pack, entries: [["q1:a", 2, exact.replace('"q1:a"', '"q1:b"')]] },
  ]) {
    useGzip(gzipSync(JSON.stringify(invalid)));
    assert.deepEqual(await browser(coldMiss, fixed), [null]);
  }
  const many = Array.from({ length: 100_000 }, (_, i) => {
    const key = `q1:${i.toString(16).padStart(32, "0")}`;
    return [key, 1, receipt(key, 1, 42)];
  });
  useGzip(gzipSync(JSON.stringify({ ...pack, entries: many })));
  assert.deepEqual(await browser(async ({ body, official, key }) => {
    searchWarmRetry.clear();
    const cache = await openSearchWarmCache(body, { official, indexedDB: null });
    return cache.lookup([key], 1);
  }, { ...fixed, key: many.at(-1)[0] }), [many.at(-1)[2]]);
  useGzip(gzipSync(JSON.stringify({ ...pack, entries: [...many, ["q1:a", 2, exact]] })));
  assert.deepEqual(await browser(coldMiss, fixed), [null]);
  const megabyte = Buffer.alloc(1024 ** 2, 32);
  const zipChunks = [];
  const validJson = JSON.stringify({ ...pack, entries: [["q1:a", 2, exact]] });
  for await (const part of Readable.from([validJson, ...Array(128).fill(megabyte)]).pipe(createGzip())) zipChunks.push(part);
  // Valid JSON with trailing whitespace would hit if the decoded-size limit were absent.
  useGzip(Buffer.concat(zipChunks));
  assert.deepEqual(await browser(coldMiss, fixed), [null]);
  assert.equal(requests.splice(0).length, 14);
  useGzip(gzip);

  const rejected = await browser(async ({ body, official, identityId }) => {
    let issued = 0, aborted = false;
    const common = { official, indexedDB: null, timeoutMs: 30 };
    const hang = async (_url, { signal }) => {
      issued++;
      signal.addEventListener("abort", () => { aborted = true; });
      return new Promise(() => {});
    };
    searchWarmRetry.clear();
    const timed = await openSearchWarmCache(body, { ...common, fetch: hang });
    const cross = await openSearchWarmCache(body, { ...common, fetch: hang, baseUrl: "https://example.invalid/warm/" });
    searchWarmRetry.clear();
    const maliciousMeta = await openSearchWarmCache(body, { ...common, fetch: async () => {
      issued++;
      return Response.json({ version: 1, packs: { [identityId]: { bytes: 24, sha256: "../somewhere" } } });
    } });
    searchWarmRetry.clear();
    const tooBigManifest = await openSearchWarmCache(body, { ...common, fetch: async () => {
      issued++;
      return new Response(new Uint8Array(128 * 1024 + 1));
    } });
    return { issued, aborted, hits: [timed, cross, maliciousMeta, tooBigManifest].map(cache => cache.lookup(["q1:a"], 2)[0]) };
  }, fixed);
  assert.deepEqual(rejected, { issued: 3, aborted: true, hits: [null, null, null, null] });
  assert.deepEqual(requests, []);

  const corrupt = await browser(async ({ body, official, identityId }) => {
    await new Promise((resolve, reject) => {
      const opening = indexedDB.open(searchWarmDbName, 1);
      opening.onerror = () => reject(opening.error);
      opening.onsuccess = () => {
        const db = opening.result;
        const tx = db.transaction("values", "readwrite");
        tx.objectStore("values").put({ asset: { gzip: new Uint8Array(32) } }, `asset:${identityId}`);
        tx.oncomplete = () => { db.close(); resolve(); };
        tx.onabort = () => { db.close(); reject(tx.error); };
      };
    });
    const session = await openSearchWarmCache(body, { official });
    await session.flush();
    return session.lookup(["q1:a"], 2);
  }, fixed);
  assert.deepEqual(corrupt, [exact]);
  assert.deepEqual(requests.splice(0).map(req => req.path), [root + "manifest.json", root + goodAsset.sha256 + ".json.gz"]);

  const checkOfficial = (offset, remote = true) => browser(async ({ body, official, offset, remote }) => {
    window.warmClockOffset = offset;
    const cache = await openSearchWarmCache(body, { official, remote });
    await cache.flush();
    return cache.lookup(["q1:a", "q1:added"], 2);
  }, { ...fixed, offset, remote });
  const paths = () => requests.splice(0).map(req => req.path);
  const advance = 5 * 60_000 + 1;
  const conditionalBefore = conditionalManifest;
  assert.deepEqual(await checkOfficial(advance), [exact, null]);
  assert.deepEqual(paths(), [root + "manifest.json"]);
  assert.ok(conditionalManifest > conditionalBefore);

  const added = receipt("q1:added", 2, 12);
  const privatePrefix = receipt("q1:a", 1, 1);
  const dedup = await browser(async ({ body, official, exact, full, privatePrefix }) => {
    const cache = await openSearchWarmCache(body, { official });
    cache.record(["q1:a", "q1:a", "q1:a"], [exact, full, privatePrefix]);
    await cache.flush();
    return (await openSearchWarmCache(body, { official, remote: false })).exportPack().entries;
  }, { ...fixed, privatePrefix });
  assert.deepEqual(dedup, [["q1:a", 1, privatePrefix]]);
  assert.deepEqual(paths(), []);

  manifestStatus = 404;
  const expired = 2 * advance;
  assert.deepEqual(await checkOfficial(expired), [exact, null]);
  assert.deepEqual(await checkOfficial(expired), [exact, null]);
  assert.deepEqual(paths(), [root + "manifest.json"]);
  manifestStatus = 200;

  useGzip(gzipSync(JSON.stringify({ ...pack, entries: [["q1:added", 2, added]] })));
  const nextAsset = root + manifest.packs[identityId].sha256 + ".json.gz";
  assetStatus = 404;
  const retry = 60_001;
  assert.deepEqual(await checkOfficial(expired + retry), [privatePrefix, null]);
  assert.deepEqual(await checkOfficial(expired + retry), [privatePrefix, null]);
  assert.deepEqual(paths(), [root + "manifest.json", nextAsset]);
  assetStatus = 200;
  assert.deepEqual(await checkOfficial(expired + 2 * retry), [privatePrefix, added]);
  assert.deepEqual(paths(), [root + "manifest.json", nextAsset]);

  // Seed a byte-identical private copy written by a client without deduplication.
  await browser(async ({ body, official, added }) => {
    const local = await openSearchWarmCache(body, { official, remote: false });
    local.record(["q1:added"], [added]);
    await local.flush();
  }, { ...fixed, added });
  delete manifest.packs[identityId];
  const withdrawn = expired + 2 * retry + advance;
  assert.deepEqual(await checkOfficial(withdrawn), [privatePrefix, null]);
  assert.deepEqual(await checkOfficial(withdrawn, false), [privatePrefix, null]);
  assert.deepEqual(await checkOfficial(withdrawn), [privatePrefix, null]);
  assert.deepEqual(paths(), [root + "manifest.json"]);
  manifestStatus = 503;
  assert.deepEqual(await checkOfficial(withdrawn + retry), [privatePrefix, null]);
  assert.deepEqual(paths(), [root + "manifest.json"]);

  // A later publication is visible after the negative TTL and Gone removes it.
  manifestStatus = 200;
  useGzip(gzipSync(JSON.stringify({ ...pack, entries: [["q1:added", 2, added]] })));
  assert.deepEqual(await checkOfficial(withdrawn + 2 * retry), [privatePrefix, added]);
  assert.deepEqual(paths(), [root + "manifest.json", nextAsset]);
  manifestStatus = 410;
  assert.deepEqual(await checkOfficial(withdrawn + 2 * retry + advance), [privatePrefix, null]);
  assert.deepEqual(paths(), [root + "manifest.json"]);

  const noStorage = await browser(async ({ body, official }) => {
    searchWarmRetry.clear();
    const options = { official, indexedDB: { open() { throw Error("storage unavailable"); } } };
    const first = await openSearchWarmCache(body, options);
    const second = await openSearchWarmCache(body, options);
    return [first, second].map(cache => cache.lookup(["q1:a"], 2)[0]);
  }, fixed);
  assert.deepEqual(noStorage, [null, null]);
  assert.deepEqual(paths(), [root + "manifest.json"]);
  console.log("Warm search cache: identities, bounded downloads, revalidation, withdrawal, deduplication and Chromium IndexedDB passed.");
} finally {
  await app?.close();
  server.closeAllConnections();
  await new Promise((done) => server.close(done));
}
