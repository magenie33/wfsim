// Only the engine may name a candidate or accept a carry. The request owns
// the battle context and the caller supplies its frozen UI source.
const searchWarmMiB = 1024 ** 2;
const searchWarmLimits = Object.freeze({
  manifest: 128 * 1024, gzip: 24 * searchWarmMiB, json: 128 * searchWarmMiB,
  entry: 128 * 1024, entries: 100_000, local: 32 * searchWarmMiB, disk: 128 * searchWarmMiB, rows: 96,
  captureEntries: 500_000, captureBytes: 192 * searchWarmMiB,
});
// Only search controls, UI bookkeeping and axes bound by Replay::warm_identity.
// Keep every other top-level input, including all nested seat and enemy data.
const searchWarmExcluded = new Set([
  "weapon", "__weapon", "__resume", "__collect_builds", "strategy", "search_strategy", "quick_fleet",
  "starts", "final", "limits", "finalists", "runs", "candidate_runs", "every_rank",
  "mods", "exilus", "arcane", "arcane_rank", "arcanes", "evolutions", "mode", "modes",
  "assembly", "assemblies", "valence", "valence_element", "valence_bonus", "rivens", "seed",
]);
const searchWarmDbName = "wfsim-search-warm-v1";
const searchWarmRetry = new Map();
const searchWarmManifestTtl = 5 * 60_000;
const searchWarmRetryTtl = 60_000;
const searchWarmObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const searchWarmCompare = (a, b) => a < b ? -1 : a > b ? 1 : 0;
const searchWarmIsRuns = (n) => Number.isInteger(n) && n > 0 && n <= 0xffffffff;
const searchWarmKey = (key) => typeof key === "string" && key.length > 0 && key.length <= 256;

function searchWarmCanonical(value, depth = 0) {
  if (depth > 64) throw new Error("warm cache identity is too deep");
  if (Array.isArray(value)) {
    return `[${Array.from(value, (v) => searchWarmCanonical(v, depth + 1)).join(",")}]`;
  }
  if (searchWarmObject(value)) {
    const proto = Object.getPrototypeOf(value);
    if (proto !== null && proto !== Object.prototype) throw new Error("warm cache identity is not JSON");
    const fields = Object.keys(value).sort().map((key) =>
      `${JSON.stringify(key)}:${searchWarmCanonical(value[key], depth + 1)}`);
    return `{${fields.join(",")}}`;
  }
  if (value === null || typeof value === "boolean" || typeof value === "string" || Number.isFinite(value)) {
    return JSON.stringify(value);
  }
  throw new Error("warm cache identity is not JSON");
}

async function searchWarmSha(bytes) {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (n) => n.toString(16).padStart(2, "0")).join("");
}

function searchWarmIdentityHash(value) {
  const bytes = new TextEncoder().encode(searchWarmCanonical(value));
  if (bytes.byteLength > searchWarmLimits.manifest) throw new Error("warm cache identity is too large");
  return searchWarmSha(bytes);
}

async function searchWarmIdentity(body, options = {}) {
  if (!searchWarmObject(body)) return null;
  const scene = options.scenario ?? Object.fromEntries(Object.entries(body).filter(([key]) => !searchWarmExcluded.has(key)));
  const weapon = body?.weapon;
  const official = options.official === false ? null : options.official ?? null;
  const engine = options.engine ?? (typeof ENGINE_ID === "string" ? ENGINE_ID : null);
  const wasm = options.wasm ?? (typeof ENGINE_WASM === "string"
    ? ENGINE_WASM.match(/(?:^|\/)wfsim_wasm_bg\.([a-f0-9]{12})\.wasm$/)?.[1] : null);
  const data = options.data ?? (typeof ENGINE_PACKS !== "undefined" && Array.isArray(ENGINE_PACKS)
    ? ENGINE_PACKS.map((url) => url.match(/(?:^|\/)pack\.data\.([a-f0-9]{12})\.json$/)?.[1]).find(Boolean) : null);
  const fingerprint = (id) => typeof id === "string" && /^[a-f0-9]{12,64}$/.test(id);
  const name = (id) => typeof id === "string" && id.length > 0 && id.length <= 256;
  if (!fingerprint(engine) || !fingerprint(data) || !fingerprint(wasm) || !name(weapon)
      || (official !== null && !name(official)) || !(searchWarmObject(scene) || Array.isArray(scene))) return null;
  return { engine, data, wasm, weapon, official, scenario: await searchWarmIdentityHash(scene) };
}

function searchWarmReceipt(key, packed) {
  if (typeof packed !== "string" || !packed.length || packed.length > searchWarmLimits.entry) return null;
  try {
    const receipt = JSON.parse(packed);
    if (!Array.isArray(receipt) || receipt.length !== 3 || receipt[0] !== 1) return null;
    const [, widths, shard] = receipt;
    if (!Array.isArray(widths) || widths.length !== 2 || widths[0] !== 401 || widths[1] !== 8
        || !searchWarmObject(shard) || shard.warm_key !== key || !searchWarmIsRuns(shard.runs)
        || !Number.isInteger(shard.gas) || shard.gas < 0
        || !Array.isArray(shard.index) || shard.index.length !== shard.runs
        || !Array.isArray(shard.ttks) || shard.ttks.length > shard.runs
        || !Array.isArray(shard.by_body) || shard.by_body.length > widths[0]
        || !Array.isArray(shard.by_combatant) || shard.by_combatant.length > widths[1]
        || !Array.isArray(shard.seat_counters) || shard.seat_counters.length > widths[1]
        || !Array.isArray(shard.series?.kill_progress) || shard.series.kill_progress.length !== 0
        || !Array.isArray(shard.series?.effective) || shard.series.effective.length !== 0) return null;
    // Keep the original string; JavaScript cannot round-trip its u64 fields.
    return [key, shard.runs, packed, shard.gas];
  } catch (_) { return null; }
}

// A min-heap keeps the lowest-gas receipt cheap to evict even in a full cache.
function searchWarmRank() {
  const heap = [], positions = new WeakMap();
  const cheaper = (a, b) => a[3] < b[3];
  function swap(a, b) {
    [heap[a], heap[b]] = [heap[b], heap[a]];
    positions.set(heap[a], a);
    positions.set(heap[b], b);
  }
  function fix(i) {
    let parent = (i - 1) >> 1;
    if (i > 0 && cheaper(heap[i], heap[parent])) {
      for (; i > 0 && cheaper(heap[i], heap[parent]); parent = (i - 1) >> 1) { swap(i, parent); i = parent; }
      return;
    }
    for (let child = 2 * i + 1; child < heap.length; child = 2 * i + 1) {
      if (child + 1 < heap.length && cheaper(heap[child + 1], heap[child])) child++;
      if (!cheaper(heap[child], heap[i])) break;
      swap(i, child);
      i = child;
    }
  }
  return {
    least: () => heap[0],
    add(row) { positions.set(row, heap.length); heap.push(row); fix(heap.length - 1); },
    remove(row) {
      const i = positions.get(row), last = heap.pop();
      positions.delete(row);
      if (last !== row) { heap[i] = last; positions.set(last, i); fix(i); }
    },
  };
}

function searchWarmEntries(budget = 0) {
  const order = new Set(), byKey = new Map();
  const rank = budget ? searchWarmRank() : null;
  // Charge UTF-16 strings and row overhead conservatively for IndexedDB.
  const charge = (row) => 64 + 2 * (row[0].length + row[2].length);
  let bytes = 128;
  const drop = (row) => {
    if (!order.delete(row)) return;
    const versions = byKey.get(row[0]);
    versions.delete(row[1]);
    if (!versions.size) byKey.delete(row[0]);
    bytes -= charge(row);
    rank?.remove(row);
  };
  return {
    get bytes() { return bytes; },
    get size() { return order.size; },
    rows: () => Array.from(order),
    remove: drop,
    exact: (key, runs) => byKey.get(key)?.get(runs),
    best(key, runs) {
      let best = null;
      for (const row of byKey.get(key)?.values() ?? []) {
        if (row[1] === runs) return row;
        if (row[1] < runs && (!best || row[1] > best[1])) best = row;
      }
      return best;
    },
    add(row) {
      const [key, runs] = row;
      const prior = byKey.get(key)?.get(runs);
      if (prior) drop(prior);
      if (!byKey.has(key)) byKey.set(key, new Map());
      byKey.get(key).set(runs, row);
      order.add(row);
      bytes += charge(row);
      if (rank) {
        rank.add(row);
        while (order.size && (bytes > budget || order.size > searchWarmLimits.entries)) drop(rank.least());
      }
    },
  };
}

function searchWarmRow(row) {
  if (!Array.isArray(row) || row.length !== 3 || !searchWarmKey(row[0]) || !searchWarmIsRuns(row[1])) return null;
  const receipt = searchWarmReceipt(row[0], row[2]);
  return receipt?.[1] === row[1] ? receipt : null;
}

function searchWarmRestore(rows, budget) {
  const entries = searchWarmEntries(budget);
  if (Array.isArray(rows) && rows.length <= searchWarmLimits.entries) {
    for (const row of rows) {
      const receipt = searchWarmRow(row);
      if (receipt) entries.add(receipt);
    }
  }
  return entries;
}

async function searchWarmPublishedRows(rows, signal) {
  if (!Array.isArray(rows) || rows.length > searchWarmLimits.entries) return null;
  const entries = searchWarmEntries();
  for (let i = 0; i < rows.length; i++) {
    if (i > 0 && i % 1024 === 0) {
      await new Promise((resolve) => setTimeout(resolve, 0));
      signal.throwIfAborted();
    }
    const row = searchWarmRow(rows[i]);
    if (!row || entries.best(row[0], row[1])?.[1] === row[1]) return null;
    entries.add(row);
  }
  return entries;
}

async function searchWarmDeadline(ms, run) {
  const controller = new AbortController();
  let timer;
  try {
    return await Promise.race([
      Promise.resolve().then(() => run(controller.signal)),
      new Promise((resolve) => { timer = setTimeout(() => { controller.abort(); resolve(null); }, ms); }),
    ]);
  } catch (_) { return null; }
  finally { clearTimeout(timer); }
}

async function searchWarmRead(stream, limit, signal, consume) {
  if (!stream) throw new Error("warm cache needs a readable body");
  signal.throwIfAborted();
  const reader = stream.getReader();
  let total = 0, done = false;
  const cancel = () => { reader.cancel().catch(() => {}); };
  signal.addEventListener("abort", cancel, { once: true });
  try {
    while (!done) {
      const part = await reader.read();
      signal.throwIfAborted();
      done = part.done;
      if (done) break;
      total += part.value.byteLength;
      if (total > limit) throw new Error("warm cache byte limit exceeded");
      if (part.value.byteLength) consume(part.value, total - part.value.byteLength);
    }
    return total;
  } finally {
    signal.removeEventListener("abort", cancel);
    if (!done) cancel();
    reader.releaseLock();
  }
}

function searchWarmBase(baseUrl) {
  try {
    const origin = globalThis.location?.origin;
    const release = typeof RELEASE_ID === "string" && /^[a-f0-9]{12,64}$/.test(RELEASE_ID) ? RELEASE_ID : null;
    const url = new URL(baseUrl ?? (release ? `/search-cache/v1/${release}/` : "/search-cache/"), origin);
    if (!/^https?:$/.test(url.protocol) || (origin && url.origin !== origin)
        || url.username || url.password || url.search || url.hash) return null;
    if (!url.pathname.endsWith("/")) url.pathname += "/";
    return url;
  } catch (_) { return null; }
}

async function searchWarmGet(fetcher, url, limit, signal, asset = false) {
  signal.throwIfAborted();
  const response = await fetcher(url.href, {
    method: "GET", mode: "same-origin", credentials: "omit", redirect: "error", referrerPolicy: "no-referrer",
    cache: asset ? "default" : "no-cache", signal,
  });
  const declared = response.headers.get("content-length");
  const encoding = response.headers.get("content-encoding");
  const direct = !response.redirected && (!response.url || response.url === url.href);
  if (response.status !== 200 || !direct
      || (declared !== null && (!/^\d+$/.test(declared) || Number(declared) > limit))
      || (asset && encoding && encoding.toLowerCase() !== "identity")) {
    response.body?.cancel().catch(() => {});
    return direct && !asset && response.status === 410 ? 410 : null;
  }
  const bytes = new Uint8Array(limit);
  const length = await searchWarmRead(response.body, limit, signal, (part, offset) => bytes.set(part, offset));
  return bytes.subarray(0, length);
}

const searchWarmParse = (bytes) => JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
const searchWarmValidAsset = (meta) => searchWarmObject(meta) && typeof meta.sha256 === "string"
  && /^[a-f0-9]{64}$/.test(meta.sha256) && Number.isInteger(meta.bytes)
  && meta.bytes >= 18 && meta.bytes <= searchWarmLimits.gzip;

async function searchWarmUnpack(asset, id, signal) {
  const { meta, gzip } = asset;
  if (!searchWarmValidAsset(meta) || !(gzip instanceof Uint8Array) || gzip.byteLength !== meta.bytes
      || gzip[0] !== 0x1f || gzip[1] !== 0x8b || await searchWarmSha(gzip) !== meta.sha256) return null;
  signal.throwIfAborted();
  const stream = new Blob([gzip]).stream().pipeThrough(new DecompressionStream("gzip"));
  const decoder = new TextDecoder("utf-8", { fatal: true });
  const text = [];
  await searchWarmRead(stream, searchWarmLimits.json, signal, (part) => text.push(decoder.decode(part, { stream: true })));
  text.push(decoder.decode());
  const pack = JSON.parse(text.join(""));
  text.length = 0;
  if (!searchWarmObject(pack) || pack.version !== 1 || await searchWarmIdentityHash(pack.identity) !== id) return null;
  return searchWarmPublishedRows(pack.entries, signal);
}

// An absent key (or HTTP Gone) withdraws a pack. An unavailable or invalid
// manifest cannot withdraw it. Fetch revalidates the browser's HTTP entry.
async function searchWarmManifest(fetcher, base, id, signal) {
  const raw = await searchWarmGet(fetcher, new URL("manifest.json", base), searchWarmLimits.manifest, signal);
  if (!raw) return null;
  if (raw === 410) return { meta: null };
  const manifest = searchWarmParse(raw);
  if (manifest?.version !== 1 || !searchWarmObject(manifest.packs)) return null;
  if (!Object.hasOwn(manifest.packs, id)) return { meta: null };
  const meta = manifest.packs[id];
  return searchWarmValidAsset(meta) ? { meta } : null;
}

async function searchWarmDownload(fetcher, base, id, signal, meta) {
  meta ??= (await searchWarmManifest(fetcher, base, id, signal))?.meta;
  if (!meta) return null;
  const gzip = await searchWarmGet(fetcher, new URL(`${meta.sha256}.json.gz`, base), meta.bytes, signal, true);
  if (!gzip) return null;
  const asset = { meta: { bytes: meta.bytes, sha256: meta.sha256 }, gzip };
  const entries = await searchWarmUnpack(asset, id, signal);
  return entries ? { entries, asset } : null;
}

async function searchWarmRefresh(saved, fetcher, base, id, timeoutMs) {
  const checkedAt = Date.now();
  const retryKey = `${base.href}:${id}`;
  const unpack = (asset) => searchWarmDeadline(timeoutMs, (signal) => searchWarmUnpack(asset, id, signal));
  const old = saved?.asset;
  let savedEntries;
  const readOld = () => savedEntries ??= unpack(old);
  if (saved?.base === base.href && saved.refreshAt > checkedAt && saved.refreshAt <= checkedAt + searchWarmManifestTtl) {
    if (!old) return null;
    const entries = await readOld();
    if (entries) return { entries };
  }
  if (!old && searchWarmRetry.get(retryKey) > checkedAt) return null;

  const manifest = await searchWarmDeadline(Math.min(timeoutMs, 5000), (signal) => searchWarmManifest(fetcher, base, id, signal));
  let asset = null, entries = null;
  const matches = manifest?.meta && old?.meta?.sha256 === manifest.meta.sha256 && old.meta.bytes === manifest.meta.bytes;
  if (old && (!manifest || matches)) {
    entries = await readOld();
    if (entries) asset = old;
  }
  if (!entries && manifest?.meta) {
    const got = await searchWarmDeadline(timeoutMs, (signal) => searchWarmDownload(fetcher, base, id, signal, manifest.meta));
    if (got) ({ entries, asset } = got);
  }
  const refreshAt = Date.now() + (manifest?.meta && entries ? searchWarmManifestTtl : searchWarmRetryTtl);
  searchWarmRetry.delete(retryKey);
  if (!entries) {
    searchWarmRetry.set(retryKey, refreshAt);
    if (searchWarmRetry.size > searchWarmLimits.rows) searchWarmRetry.delete(searchWarmRetry.keys().next().value);
  }
  const state = { asset, base: base.href, checkedAt, refreshAt, bytes: (asset?.meta.bytes ?? 0) + 256 + 2 * base.href.length };
  return { entries, state, retired: manifest && !matches && old ? readOld : null };
}

// Read, merge and evict in one transaction. Usage metadata keeps eviction
// from cloning every cached pack into memory.
function searchWarmStorage(factory, timeoutMs, localBytes, diskBytes) {
  if (!factory) return null;
  function transact(key, change, weapon) {
    return searchWarmDeadline(timeoutMs, (signal) => new Promise((resolve, reject) => {
      const opening = factory.open(searchWarmDbName, 1);
      opening.onupgradeneeded = () => {
        if (signal.aborted) { opening.transaction.abort(); return; }
        opening.result.createObjectStore("values");
        opening.result.createObjectStore("usage", { keyPath: "key" });
      };
      opening.onerror = () => reject(opening.error);
      opening.onsuccess = () => {
        const db = opening.result;
        if (signal.aborted) { db.close(); resolve(null); return; }
        let tx;
        try { tx = db.transaction(["values", "usage"], "readwrite"); }
        catch (error) { reject(error); return; }
        finally { db.close(); }
        const values = tx.objectStore("values"), usage = tx.objectStore("usage");
        const stop = () => { try { tx.abort(); } catch (_) {} };
        signal.addEventListener("abort", stop, { once: true });
        let value = null;
        const done = () => signal.removeEventListener("abort", stop);
        tx.onabort = () => { done(); reject(tx.error); };
        tx.oncomplete = () => { done(); resolve(value); };
        const reading = values.get(key);
        reading.onsuccess = () => {
          value = reading.result ?? null;
          if (!change) {
            if (value) {
              const held = usage.get(key);
              held.onsuccess = () => {
                if (held.result) usage.put({ ...held.result, at: Date.now() });
              };
            }
            return;
          }
          try { value = change(value); }
          catch (_) { stop(); return; }
          if (value.bytes > diskBytes) { stop(); return; }
          const checking = usage.getAll();
          checking.onsuccess = () => {
            const prior = checking.result.filter((row) => row.key !== key);
            prior.sort((a, b) => a.at - b.at || searchWarmCompare(a.key, b.key));
            let total = value.bytes + prior.reduce((sum, row) => sum + row.bytes, 0);
            let count = prior.length + 1;
            const evict = (row) => {
              values.delete(row.key);
              usage.delete(row.key);
              total -= row.bytes;
              count--;
            };
            const removed = new Set();
            if (weapon) {
              const sameWeapon = prior.filter((row) => row.weapon === weapon);
              let used = value.bytes + sameWeapon.reduce((sum, row) => sum + row.bytes, 0);
              for (const row of sameWeapon) {
                if (used <= localBytes) break;
                evict(row);
                removed.add(row.key);
                used -= row.bytes;
              }
            }
            for (const row of prior) {
              if (total <= diskBytes && count <= searchWarmLimits.rows) break;
              if (!removed.has(row.key)) evict(row);
            }
            values.put(value, key);
            usage.put({ key, bytes: value.bytes, weapon, at: Date.now() });
          };
        };
      };
    }));
  }
  return { read: (key) => transact(key), update: transact };
}

async function openSearchWarmCache(body, options = {}) {
  try {
    const identity = await searchWarmIdentity(body, options);
    if (!identity) return null;
    const id = await searchWarmIdentityHash(identity);
    const bounded = (value, fallback, min, max) => Number.isFinite(value) ? Math.max(min, Math.min(value, max)) : fallback;
    const diskBytes = bounded(options.diskBytes, searchWarmLimits.disk, 256, searchWarmLimits.disk);
    const localBytes = Math.min(diskBytes, bounded(options.localBytes, searchWarmLimits.local, 256, 64 * searchWarmMiB));
    let factory;
    try { factory = options.indexedDB === undefined ? globalThis.indexedDB : options.indexedDB; } catch (_) {}
    const storage = searchWarmStorage(factory, bounded(options.storageTimeoutMs, 1500, 1, 5000), localBytes, diskBytes);
    const localKey = `local:${id}`;
    const localLoad = storage?.read(localKey);
    let officialEntries, officialState, assetKey, retired;
    if (identity.official !== null && options.remote !== false && typeof DecompressionStream === "function") {
      const base = searchWarmBase(options.baseUrl);
      const fetcher = options.fetch ?? globalThis.fetch?.bind(globalThis);
      if (base && fetcher) {
        const networkMs = bounded(options.timeoutMs, 60_000, 1, 120_000);
        const defaultBase = searchWarmBase();
        assetKey = base.href === defaultBase?.href ? `asset:${id}` : `asset:${await searchWarmIdentityHash(base.href)}:${id}`;
        const saved = await storage?.read(assetKey);
        const refreshed = await searchWarmRefresh(saved, fetcher, base, id, networkMs);
        officialEntries = refreshed?.entries;
        officialState = refreshed?.state;
        retired = refreshed?.retired;
      }
    }
    const local = searchWarmRestore((await localLoad)?.entries, localBytes);
    // Remove copies left by older clients before persisting a withdrawal or replacement.
    const duplicates = new Set();
    if (local.size && (officialEntries || retired)) {
      const prior = await retired?.();
      for (const row of local.rows()) {
        const [key, runs, packed] = row;
        if (officialEntries?.exact(key, runs)?.[2] !== packed && prior?.exact(key, runs)?.[2] !== packed) continue;
        duplicates.add(packed);
        local.remove(row);
      }
    }
    const writeLocal = (rows, omit) => storage.update(localKey, (prior) => {
      const existing = Array.isArray(prior?.entries) ? prior.entries : [];
      const merged = searchWarmRestore(omit.size ? existing.filter((row) => !omit.has(row?.[2])) : existing, localBytes);
      for (const row of rows) merged.add(row);
      return { entries: merged.rows().map((row) => row.slice(0, 3)), bytes: merged.bytes };
    }, identity.weapon);
    const assetWrite = storage && (officialState || duplicates.size) ? (async () => {
      if (duplicates.size && !await writeLocal([], duplicates)) return;
      if (officialState) await storage.update(assetKey, (prior) =>
        prior?.checkedAt > officialState.checkedAt ? prior : officialState);
    })() : null;
    const pending = searchWarmEntries(localBytes);
    // The baking host can collect before ranking and trimming for publication.
    const captured = options.capture === true ? searchWarmEntries() : null;
    const encoder = captured && new TextEncoder();
    let captureBytes = captured ? encoder.encode(JSON.stringify({ version: 1, identity, entries: [] })).length : 0;
    let captureOverflow = false;
    let saving = Promise.resolve();
    return {
      lookup(keys, runs) {
        if (!Array.isArray(keys)) return [];
        return Array.from(keys, (key) => {
          if (!searchWarmKey(key) || !searchWarmIsRuns(runs)) return null;
          const own = local.best(key, runs), baked = officialEntries?.best(key, runs);
          return ((own?.[1] ?? 0) >= (baked?.[1] ?? 0) ? own : baked)?.[2] ?? null;
        });
      },
      record(keys, shards) {
        if (!Array.isArray(keys) || !Array.isArray(shards)) return;
        for (let i = 0; i < Math.min(keys.length, shards.length); i++) {
          if (!searchWarmKey(keys[i])) continue;
          const row = searchWarmReceipt(keys[i], shards[i]);
          if (!row) continue;
          const [key, runs, packed] = row;
          if (typeof options.onRecord === "function") {
            try { options.onRecord(key, runs, packed); } catch (_) {}
          }
          if (captured && !captureOverflow) {
            const held = captured.best(key, runs);
            if (held?.[1] !== runs || held[2] !== packed) {
              if (held?.[1] === runs) captureBytes -= encoder.encode(JSON.stringify(held.slice(0, 3))).length + 1;
              captureBytes += encoder.encode(JSON.stringify(row.slice(0, 3))).length + 1;
              captureOverflow = captureBytes > searchWarmLimits.captureBytes
                || (held?.[1] !== runs && captured.size >= searchWarmLimits.captureEntries);
              if (!captureOverflow) captured.add(row);
            }
          }
          if (local.exact(key, runs)?.[2] === packed || officialEntries?.exact(key, runs)?.[2] === packed) continue;
          local.add(row);
          if (storage) pending.add(row);
        }
      },
      flush() {
        saving = saving.then(async () => {
          await assetWrite;
          const rows = pending.rows();
          if (!rows.length) return;
          const saved = await writeLocal(rows, duplicates);
          if (saved) for (const row of rows) pending.remove(row);
        });
        return saving;
      },
      exportPack() {
        if (captureOverflow) throw new Error("warm cache capture exceeded collection limits");
        const merged = captured ?? searchWarmEntries();
        if (!captured) {
          for (const row of officialEntries?.rows() ?? []) merged.add(row);
          for (const row of local.rows()) merged.add(row);
        }
        const entries = merged.rows().map((row) => row.slice(0, 3));
        entries.sort((a, b) => searchWarmCompare(a[0], b[0]) || a[1] - b[1]);
        return { version: 1, identity: { ...identity }, entries };
      },
    };
  } catch (_) { return null; }
}
