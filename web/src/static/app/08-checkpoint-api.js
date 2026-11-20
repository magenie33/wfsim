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

// ---- checkpoint / resume -------------------------------------------------
// A reload KILLS the worker, and there is no browser mechanism that avoids it
// (measured 2026-07-30: a SharedWorker is terminated too, the moment its last
// client disconnects, busy or idle). So instead of pretending a run can
// survive, make losing it cheap: the worker emits the surviving field after
// every completed round, and a fresh page rebuilds from the last one.
//
// Stored as IDENTITIES only — (mod pool indices, evo set, exilus, arcane) — so
// it fits localStorage and cannot drift from what the engine would rebuild.
// The REQUEST is stored with it and is what a resume replays: a checkpoint
// only ever means anything under the scope that produced it, so it is never
// re-derived from whatever the form happens to say later.
const OPT_CKPT = "wfsim-optimize-checkpoint";
function saveCheckpoint(body, cp, board) {
  const write = (payload) => localStorage.setItem(OPT_CKPT, JSON.stringify(payload));
  try {
    write({ body, cp, board, at: Date.now() });
  } catch (_) {
    // A screen cut is the whole surviving field — tens of thousands of pairs.
    // If it does not fit alongside the leaderboard, the RESUME POINT is worth
    // more than the display copy, so drop the board and keep the cut.
    try { write({ body, cp, at: Date.now() }); } catch (_) { /* nothing fits: no checkpoint */ }
  }
}
const clearCheckpoint = () => { try { localStorage.removeItem(OPT_CKPT); } catch (_) {} };
function loadCheckpoint() {
  try {
    const s = JSON.parse(localStorage.getItem(OPT_CKPT));
    if (!s || !s.body || !s.cp) return null;
    // A day-old checkpoint is almost certainly not what the visitor meant to
    // resume, and the data behind it may have been rebuilt since.
    if (Date.now() - (s.at || 0) > 24 * 3600 * 1000) { clearCheckpoint(); return null; }
    return s;
  } catch (_) { return null; }
}

// HOW MANY WORKERS the browser search runs on. This is the only lever the
// browser has: it is single-threaded at ~150 simulated engagements per second
// against ~5,100 on a 26-thread desktop, so coverage is scarcest exactly where
// there is least compute. A WALK gives the N workers DISJOINT STRIDES of the
// shuffled index range (`shard`, `shard + shards`, …), a partition — nothing
// is evaluated twice and nothing is missed (`shards_partition_the_shuffled_
// order_exactly`). A DESCENT gives each worker its share of the starts, so a
// worker with no start of its own returns an empty, complete envelope.
//
// THE COUNT IS THE TOPBAR'S COMPUTE SHARE, and it is the whole answer. A `CPU
// threads` box in a search preset — on the reasoning that a heavy scope might
// want more cores than a light one — is two controls for one fact, which is
// the arena's own rule, and it puts the override on the one thing most able
// to
// cook a phone, which is the last place a global heat setting should be
// ignorable. `poolSize()` is the setting; this is now only a name for it.
const woptWorkerCount = () => poolSize();

// Bound the number of trips while letting idle workers pull smaller pieces
// when scoring the preceding batch took a long time per build.
function quickScoreWidth(total, workers, msPerBuild) {
  const usual = Math.ceil(total / (2 * workers));
  if (workers === 1 || !(msPerBuild > 0) || usual * msPerBuild <= 1000) return usual;
  return Math.max(Math.ceil(total / (8 * workers)), Math.floor(1000 / msPerBuild));
}

// Pilot gas is a scheduling hint. Nearby unmeasured candidates can share a
// costly neighborhood, so spread them across the existing slices first.
function quickScoreSlices(items, workers, width) {
  const count = Math.ceil(items.length / width);
  if (workers < 2 || count <= workers) return null;
  const measured = [], unmeasured = [];
  let shortRuns;
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    // Workers return one contiguous group per run count. The leader emits
    // full items before a single short run count, so ascending indices preserve
    // the positional reply even when a slice contains both kinds.
    if (item?.runs != null) {
      if (!Number.isInteger(item.runs) || item.runs < 1 || item.runs > 0xffffffff ||
          item.shards != null || (shortRuns != null && item.runs !== shortRuns)) return null;
      shortRuns = item.runs;
    } else if (shortRuns != null) return null;
    if (item?.shards == null) {
      unmeasured.push(i);
      continue;
    }
    if (!Array.isArray(item.shards)) return null;
    let weight = 0;
    for (const carried of item.shards) {
      if (carried == null) continue;
      let shard = carried;
      if (typeof carried === "string") {
        try {
          const packed = JSON.parse(carried);
          if (!Array.isArray(packed) || packed[0] !== 1) return null;
          shard = packed[2];
        } catch (_) { return null; }
      }
      if (!Number.isSafeInteger(shard?.gas) || shard.gas < 0) return null;
      weight += shard.gas;
    }
    if (!Number.isSafeInteger(weight)) return null;
    measured.push({ i, weight });
  }
  if (!unmeasured.length && measured.every(row => row.weight === measured[0].weight)) return null;
  if (!measured.length && width === 1) return null;
  const slices = Array.from({ length: count }, () => ({ weight: 0, indices: [] }));
  measured.sort((a, b) => b.weight - a.weight || a.i - b.i);
  const unknownWeight = measured[Math.floor(measured.length / 2)]?.weight ?? 0;
  unmeasured.forEach((i, j) => {
    const slice = slices[j % count];
    slice.indices.push(i);
    slice.weight += unknownWeight;
  });
  for (const { i, weight } of measured) {
    let lightest;
    for (const slice of slices) {
      if (slice.indices.length < width && (!lightest || slice.weight < lightest.weight)) lightest = slice;
    }
    lightest.indices.push(i);
    lightest.weight += weight;
  }
  return slices.sort((a, b) => b.weight - a.weight).map(slice => slice.indices.sort((a, b) => a - b));
}

// Merge the fleet's leaderboards into one. Each worker ran its own funnel over
// its own elites, so every row here is measured at the SAME run count under the
// SAME scenario and the scores are directly comparable — the merge is a sort.
//
// Deduplicate first: two descents can reach the same build from different
// starts. Counting it twice would push a real alternative off the board.
function woptMerge(parts, request) {
  const bad = parts.find((p) => p && p.ok === false);
  if (bad) return bad;
  // A shard that owned no ground returns an empty but complete envelope, so
  // any part is a valid head — prefer one that actually ranked something.
  const rows = [];
  const seen = new Map();
  for (const p of parts) {
    for (const r of p.results || []) {
      const key = JSON.stringify([[...(r.mods || [])].sort(), r.arcane, r.evolutions, r.exilus, r.mode, r.valence]);
      // Two shards' starts that settled on one build are ONE answer from both.
      const had = seen.get(key);
      if (had) { had.from_starts = [...(had.from_starts || []), ...(r.from_starts || [])]; continue; }
      const row = { ...r };
      seen.set(key, row);
      rows.push(row);
    }
  }
  rows.sort((a, b) => (b.kill_progress ?? b.kills ?? 0) - (a.kill_progress ?? a.kills ?? 0));
  const head = parts.find((p) => (p.results || []).length) || parts[0] || {};
  const finalists = head.finalists || rows.length;
  const space = head.space || 0;
  const sampled = parts.reduce((n, p) => n + (p.sampled || 0), 0);
  const searchStrategy = request?.search_strategy || parts.find(p => p.search_strategy)?.search_strategy;
  const answers = new Map(), origins = new Set(), succeeded = new Set();
  for (const p of parts) {
    for (const answer of p.start_results || []) {
      const had = answers.get(answer.start);
      if (!had || (answer.result.kill_progress ?? answer.result.kills ?? 0) > (had.result.kill_progress ?? had.result.kills ?? 0)) answers.set(answer.start, answer);
      origins.add(answer.start);
      succeeded.add(answer.start);
    }
    for (const failed of p.failed_starts || []) origins.add(failed.start);
    for (const result of p.results || []) {
      for (const lane of result.from_starts || []) { origins.add(lane.start); succeeded.add(lane.start); }
      for (const start of result.near?.starts || []) { origins.add(start); succeeded.add(start); }
    }
  }
  return {
    ...head,
    // EVERY shard must have finished its stride for the union to be the space.
    exhaustive: parts.length > 0 && parts.every((p) => p.exhaustive),
    // Branches of one user start can belong to several shards.
    strategy: (parts.find((p) => p.strategy) || head).strategy,
    starts: searchStrategy ? request?.starts?.length || Math.max(origins.size, ...parts.map(p => p.starts || 0))
      : parts.reduce((n, p) => n + (p.starts || 0), 0),
    ...(searchStrategy ? { search_strategy: searchStrategy, branches: parts.reduce((n, p) => n + (p.branches || 0), 0) } : {}),
    start_results: [...answers.values()].sort((a, b) => a.start - b.start),
    cut: parts.some((p) => p.cut),
    // Coverage of the FLEET, not of one worker: the strides are disjoint, so
    // the positions add up.
    coverage: space > 0 ? Math.min(1, sampled / space) : 0,
    sampled,
    searched: parts.reduce((n, p) => n + (p.searched || 0), 0),
    candidates: parts.reduce((n, p) => n + (p.candidates || 0), 0),
    jobs: parts.reduce((n, p) => n + (p.jobs || 0), 0),
    cancelled: parts.some((p) => p.cancelled),
    failed_starts: parts.flatMap((p) => p.failed_starts || []).filter(f => !searchStrategy || !succeeded.has(f.start)),
    failed_branches: parts.flatMap((p) => p.failed_branches || []),
    results: rows.slice(0, finalists).map((r, i) => ({ ...r, rank: i + 1 })),
  };
}

// Loading timers prove contact; engine callbacks and returns prove calculation feedback.
function optimizerWorkerSignals() {
  const workers = [];
  let engineAt = null, responseAt = null;
  return {
    start(i, activity, warm = false) {
      workers[i] = { loaded: workers[i]?.loaded || warm, activity };
    },
    heard(i, message) {
      responseAt = Date.now();
      const worker = workers[i];
      if (!worker) return;
      const engine = (message.kind === "alive" && message.source === "engine")
        || ["progress", "result", "board", "builds", "checkpoint"].includes(message.kind);
      if (engine) engineAt = responseAt;
      if (engine || message.kind === "rested") worker.loaded = true;
    },
    finish(i) { if (workers[i]) workers[i].activity = null; },
    read(activity) {
      const pending = workers.filter((w) => w?.activity);
      const active = pending.filter((w) => w.loaded && w.activity !== "resting");
      const loading = pending.filter((w) => !w.loaded);
      return { activity: !active.length && loading.length ? "loading"
        : pending.length && pending.every((w) => w.activity === "resting") ? "resting" : activity,
        workers_active: active.length, workers_loading: loading.length,
        engine_last_at: engineAt, response_last_at: responseAt };
    },
  };
}

/// THE QUICK DESCENT ACROSS THE FLEET (docs/WASM.md). A worker is one thread
/// and cannot wait on another, so worker 0 LEADS: it runs the descent on the
/// scores it holds and answers with the builds its starts wait for; every
/// worker scores a slice of them, and the scores go back to the leader, until
/// it answers with the result. Every worker is busy on every batch, whatever
/// the number of starts.
function woptQuickFleet(body, onBuilds, searchWarm) {
  const job = quickFleet(body, () => woptWorkerCount(), undefined, { onBuilds, searchWarm });
  wopt = job;
  return { ok: true, job_id: job.id };
}

/// WORKERS A BACKGROUND SEARCH HANDS BACK, for the next one to start on: a
/// new worker does nothing until its 10 MB engine has loaded, and a
/// volunteer's searches come one after another. Kept a minute idle, then let go.
const FLEET_KEEP_MS = 60_000;
const fleetKept = [];
let fleetReap = null;
function fleetKeep(workers) {
  workers.forEach((w) => { w.onmessage = null; w.onerror = null; fleetKept.push(w); });
  clearTimeout(fleetReap);
  fleetReap = setTimeout(() => fleetKept.splice(0).forEach((w) => w.terminate()), FLEET_KEEP_MS);
}

/// …ONE SUCH SEARCH, as a job object of its own: the reader's optimizer holds
/// one (`wopt`), and a riven gain run for someone else holds another, so
/// neither waits on the other. `pace` is awaited before every round — the
/// background's way to let the reader go first. The result carries `work`:
/// what every scorer's fights and the final round cost (`Shard::work`).
///
/// `n` IS A NUMBER OR A FUNCTION asked before every round, so a background
/// search takes the cores the reader leaves it now, not the ones it had when it
/// began. A round's builds are dealt in slices a free worker pulls, so one slow
/// slice does not hold every other core idle; the scores go back in the order
/// of the builds, which is all the leader reads, so the answer is the same
/// however many workers scored it. `keep` hands the workers back (`fleetKeep`);
/// `rest(ms)` is how long a worker rests after a call that took `ms`, so a
/// background search runs the share of the time its owner set.
function quickFleet(body, n, pace = async () => {}, { keep = false, rest = () => 0, kind = "solo", onBuilds = async () => {}, searchWarm = null } = {}) {
  const lanesNow = () => Math.max(1, Math.floor(typeof n === "function" ? n() : n) || 1);
  const job = { id: woptNextId++, workers: [], status: null, result: null, board: null,
    cancelled: false, shards: lanesNow(), t0: Date.now() };
  // …AND WHAT IT TOOK: every call's own time summed over the workers, the rests
  // between calls not in it (`cpu_ms`), and how many rounds the leader ran.
  const done = { builds: 0, fights: 0, work: 0, cpu_ms: 0, steps: 0 };
  const live = [];
  const signals = optimizerWorkerSignals();
  const warm = [];
  const pendingCalls = new Set();
  let activity = "planning", finalStatus = null;
  let scoreMsPerBuild = 0;
  // Where each start stands, as the leader last reported it.
  let starts = [];
  const emptyRound = () => ({ builds: 0, fights: 0, total: 0, slices_done: 0, slices_total: 0 });
  let round = emptyRound();
  const show = () => {
    const cur = live.filter(Boolean);
    job.status = { phase: "searching", round: 0, rounds: 0, round_jobs: 0, round_runs: 0, sims_planned: 0, notes: [], ...finalStatus,
      workers: job.workers.length, starts,
      enumerated: done.builds + round.builds + cur.reduce((a, s) => a + (s.enumerated || 0), 0),
      sims_done: done.fights + round.fights + cur.reduce((a, s) => a + (s.sims_done || 0), 0) + (finalStatus?.sims_done || 0),
      batches_completed: done.steps, batch_builds_done: round.builds, batch_builds_total: round.total,
      batch_slices_done: round.slices_done, batch_slices_total: round.slices_total,
      ...signals.read(activity) };
  };
  const stop = (finished) => {
    pendingCalls.forEach(finish => finish(null));
    const ws = job.workers;
    job.workers = [];
    if (finished && keep) fleetKeep(ws); else ws.forEach((w) => w && w.terminate());
  };
  job.cancel = () => {
    if (job.cancelled || job.result) return;
    job.cancelled = true;
    stop(false);
    show();
  };
  const fail = (why, workerDead = true) => {
    if (job.cancelled || job.result) return;
    job.result = { ok: false, error: why, worker_dead: workerDead };
    stop(false);
    show();
  };
  const saveFailed = error => fail(String(error?.message || error), false);
  // One call to one worker; its progress, if any, to `onProgress`.
  const call = (i, message, onProgress) => new Promise((resolve) => {
    const resting = message.kind === "rest";
    const w = job.workers[i];
    if (!w) { resolve(null); return; }
    const wd = watchSilence(() => fail("worker stopped answering"));
    let finished = false, deliveries = null;
    const finish = payload => { if (finished) return; finished = true; pendingCalls.delete(finish); wd.clear(); signals.finish(i); resolve(payload); };
    pendingCalls.add(finish);
    w.onmessage = (e) => {
      if (finished) return;
      try {
        wd.heard();
        signals.heard(i, e.data);
        if (e.data.kind === "progress" && onProgress) onProgress(e.data.payload);
        if (e.data.kind === "builds") {
          const builds = Array.isArray(e.data.payload) ? e.data.payload : e.data.payload?.builds;
          if (builds?.length) {
            deliveries = (deliveries || Promise.resolve()).then(() => onBuilds(builds));
            deliveries.catch(saveFailed);
          }
        }
        if (e.data.kind === (resting ? "rested" : "result")) {
          // The worker has answered even if saving its builds is still pending.
          wd.clear();
          signals.finish(i);
          const payload = e.data.payload;
          if (Array.isArray(payload?.builds)) {
            const builds = payload.builds; delete payload.builds;
            if (builds.length) deliveries = (deliveries || Promise.resolve()).then(() => onBuilds(builds));
          }
          if (payload?.ok === false) {
            if (deliveries) deliveries.catch(saveFailed);
            fail(payload.error || "The worker could not finish.", false);
          } else if (deliveries) deliveries.then(() => finish(payload), saveFailed);
          else finish(payload);
        }
        show();
      } catch (error) { fail(String(error?.message || error)); }
    };
    w.onerror = (e) => { e?.preventDefault?.(); fail(String(e?.message || "worker error")); };
    signals.start(i, resting ? "resting" : activity, resting || warm[i]);
    show();
    try {
      wd.start();
      w.postMessage(message);
    } catch (error) { fail(String(error?.message || error)); }
  });
  const pause = (i, ms) => {
    const r = rest(ms);
    if (!(r > 0) || !job.workers[i]) return Promise.resolve();
    return call(i, { kind: "rest", ms: r }, null);
  };
  const timed = async (i, message, onProgress) => {
    let ms = 0;
    const o = await computeRun(kind, async () => {
      const t = performance.now();
      const result = await call(i, message, onProgress);
      ms = performance.now() - t;
      done.cpu_ms += ms;
      return result;
    }, () => !job.cancelled && !job.result);
    if (o && !o.cancelled) await pause(i, ms);
    return job.cancelled || job.result ? null : o;
  };
  const grow = (k) => {
    while (job.workers.length < k) {
      const held = fleetKept.pop();
      warm.push(!!held);
      job.workers.push(held || spawnEngine());
    }
  };
  // Each replay stays whole: worker count cannot regroup floating-point sums or reroll a seed.
  const verify = async ({ verify: requests, generation, runs }) => {
    const total = requests.length;
    const k = Math.min(lanesNow(), total);
    if (!total || !Number.isInteger(generation) || !Number.isInteger(runs) || runs < 1) throw Error("Invalid finalist round.");
    grow(k);
    activity = "final_simulation";
    finalStatus = { phase: "running", jobs: total, round: 1, rounds: 1, sims_done: 0, sims_planned: total * runs };
    round = emptyRound();
    const receipts = new Array(total);
    let next = 0;
    await Promise.all(Array.from({ length: k }, async (_, i) => {
      while (!job.cancelled && !job.result && next < total) {
        const index = next++;
        await pace();
        if (job.cancelled || job.result) return;
        const out = await timed(i, { kind: "verify_final", generation, index, replay: requests[index] },
          st => { live[i] = st; show(); });
        live[i] = null;
        if (!out) return;
        if (!out.receipt) throw Error("Missing finalist measurement.");
        receipts[index] = out.receipt;
        finalStatus.sims_done += runs;
        show();
      }
    }));
    if (job.cancelled || job.result) return null;
    return timed(0, { kind: "finish_finals", generation, receipts });
  };
  (async () => {
    grow(job.shards);
    show();
    const cache = searchWarm && kind === "solo" ? await openSearchWarmCache(body, searchWarm).catch(() => null) : null;
    const cacheFlag = cache ? { warm_cache: 1 } : {};
    const candidateRuns = Math.max(1, Math.min(1000, body.candidate_runs ?? 10));
    let scores = [];
    let lastBatch = null;
    for (let step = 0; ; step++) {
      activity = "resting";
      show();
      await pace();
      if (job.cancelled || job.result) return;
      activity = "planning";
      let r = await timed(0, { kind: "optimize", body: { ...body, quick_fleet: { ...cacheFlag, lead: true, fresh: step === 0, scores, split_finals: job.workers.length > 1 } } },
        (p) => {
          if (p.phase === "running" && p.jobs != null) { finalStatus = p; activity = "final_simulation"; }
        });
      if (!r || job.cancelled) return;
      if (Array.isArray(r.verify)) {
        r = await verify(r);
        if (!r || job.cancelled) return;
      }
      if (!r.pending) {
        if (cache) await cache.flush().catch(() => {});
        if (job.cancelled) return;
        job.result = { ...r, work: done.work + (r.final_work || 0), fights: done.fights, builds: done.builds, steps: step, cpu_ms: Math.round(done.cpu_ms) };
        stop(true);
        return;
      }
      if (!Array.isArray(r.pending) || !r.pending.length) {
        fail("The optimizer requested an empty batch.", false);
        return;
      }
      // The build and run count identify a measurement. Carried runs are evidence.
      const batch = JSON.stringify(r.pending.map(item => [item.build ?? item, item.runs ?? null]));
      if (batch === lastBatch) {
        fail("The optimizer requested the same batch twice.", false);
        return;
      }
      lastBatch = batch;
      starts = r.progress || starts;
      const k = Math.min(lanesNow(), r.pending.length);
      grow(k);
      const size = quickScoreWidth(r.pending.length, k, scoreMsPerBuild);
      const positions = quickScoreSlices(r.pending, k, size);
      const slices = positions ? positions.map(ids => ids.map(i => r.pending[i])) : [];
      if (!positions) for (let at = 0; at < r.pending.length; at += size) slices.push(r.pending.slice(at, at + size));
      const outs = new Array(slices.length).fill(null);
      let next = 0, broken = false;
      const scoreStartMs = done.cpu_ms;
      activity = "scoring";
      round = { ...emptyRound(), total: r.pending.length, slices_total: slices.length };
      show();
      await Promise.all(Array.from({ length: k }, async (_, i) => {
        while (!broken && !job.cancelled && next < slices.length) {
          const at = next++;
          const items = slices[at];
          const score = cache ? items.map(item => {
            if (!item.warm_keys) return item;
            const { warm_keys, ...plain } = item;
            const carried = cache.lookup(warm_keys, item.runs ?? candidateRuns);
            return carried.some(Boolean) ? { ...plain, warm_shards: carried } : plain;
          }) : items;
          const o = await timed(i, { kind: "optimize", body: { ...body, quick_fleet: { ...cacheFlag, score, compact_shards: 1 } } }, (st) => { live[i] = st; show(); });
          live[i] = null;
          if (!o) { broken = true; return; }
          if (!Array.isArray(o.scores) || o.scores.length !== slices[at].length) {
            broken = true;
            fail("The optimizer worker returned an incomplete batch.", false);
            return;
          }
          outs[at] = o;
          if (cache) o.scores.forEach((row, j) => {
            if (row.warm_shards && items[j].warm_keys) cache.record(items[j].warm_keys, row.warm_shards);
            delete row.warm_shards;
          });
          round.builds += o.scores.length;
          round.fights += o.fights || 0;
          round.slices_done += 1;
          show();
        }
      }));
      if (broken || outs.some((o) => !o) || job.cancelled) return;
      scoreMsPerBuild = (done.cpu_ms - scoreStartMs) / r.pending.length;
      const bad = outs.find((o) => o.ok === false);
      if (bad) { fail(bad.error || "a worker could not score its share", false); return; }
      if (positions) {
        scores = new Array(r.pending.length);
        positions.forEach((ids, slice) => ids.forEach((i, j) => { scores[i] = outs[slice].scores[j]; }));
      } else scores = outs.flatMap((o) => o.scores || []);
      done.builds += scores.length;
      done.fights += outs.reduce((a, o) => a + (o.fights || 0), 0);
      done.work += outs.reduce((a, o) => a + (o.work || 0), 0);
      done.steps += 1;
      round = emptyRound();
      live.fill(null);
      show();
    }
  })().catch(error => fail(String(error?.message || error)));
  return job;
}

function woptStart(body, checkpoint, onCheckpoint, onBuilds = builds => keepOptimizationBuilds(builds, { kind: "public" }), searchWarm = null) {
  if (wopt && wopt.workers && wopt.workers.length) {
    return { ok: false, error: "an optimization is already running — cancel it or wait", job_id: wopt.id };
  }
  const { __resume, ...req } = body ?? {}; // the resume marker is transport, not scope
  body = req;
  if (body.strategy === "quick" && !checkpoint) return woptQuickFleet(body, onBuilds, searchWarm);
  // A CHECKPOINT is one worker's field, so it can only resume a run that had
  // one worker. Rather than resume a fraction of a fleet, a resume runs
  // unsharded — slower, but it is continuing a search that already exists.
  const shards = checkpoint ? 1 : woptWorkerCount();
  const job = {
    id: woptNextId++, workers: [], status: null, statuses: new Array(shards).fill(null),
    parts: new Array(shards).fill(null), result: null, board: null, boards: new Array(shards).fill(null),
    cancelled: false, shards, t0: Date.now(),
  };
  const signals = optimizerWorkerSignals();
  // One STATUS out of many: the fleet's progress is the sum of its workers,
  // and the phase is the least advanced of them — a run is still searching
  // while any worker still is.
  const rollup = () => {
    const live = job.statuses.filter(Boolean);
    const sum = (k) => live.reduce((n, s) => n + (Number(s[k]) || 0), 0);
    const searching = live.some((s) => s.phase === "searching") || live.length < shards;
    return {
      round: 0, rounds: 0, round_jobs: 0, round_runs: 0, notes: [],
      ...live[0],
      ...(body.search_strategy ? { starts: [...new Map(live.flatMap((s, shard) => (s.starts || []).map((lane, i) =>
        [lane.branch_index ?? `${shard}:${i}`, lane]))).values()] } : {}),
      phase: searching ? "searching" : "running",
      sims_done: sum("sims_done"), sims_planned: sum("sims_planned"),
      enumerated: sum("enumerated"),
      round_jobs: sum("round_jobs"),
      workers: shards, workers_done: job.parts.filter(Boolean).length,
      ...signals.read("scoring"),
    };
  };
  const watchdogs = [];
  const budgetReleases = [];
  job.releaseBudget = () => budgetReleases.forEach(release => release());
  const failJob = (i, why) => {
    if (job.parts[i] || job.result || job.cancelled) return;
    job.parts[i] = { ok: false, error: why };
    job.result = job.parts[i];
    job.releaseBudget();
    watchdogs.forEach(wd => wd.clear());
    job.workers.forEach((worker, index) => { signals.finish(index); worker?.terminate(); });
    job.workers = [];
    job.status = rollup();
  };
  for (let i = 0; i < shards && !job.result; i++) {
    const gone = why => failJob(i, why);
    let w;
    try { w = spawnEngine(); } catch (error) { gone(String(error?.message || error)); break; }
    job.workers.push(w);
    // THE SEARCH'S WORKERS ARE WATCHED TOO. They are not lanes — they are made
    // here and run one long call — but the failure is the same: a reclaimed
    // worker neither answers nor errors, and this fleet had only `onerror`. A
    // search that can never finish and can never fail leaves the reader with a
    // progress bar that has stopped and no way to tell it from a slow one.
    const wd = watchSilence(() => gone("worker stopped answering"));
    watchdogs.push(wd);
    let releaseBudget;
    const returned = new Promise(resolve => { releaseBudget = resolve; });
    budgetReleases.push(releaseBudget);
    let deliveries = null;
    w.onmessage = (e) => {
      wd.heard();
      signals.heard(i, e.data);
      if (e.data.kind === "progress") job.statuses[i] = e.data.payload;
      if (e.data.kind === "board") { job.boards[i] = e.data.payload; job.board = woptMerge(job.boards.filter(Boolean), body); }
      if (e.data.kind === "builds") {
        const builds = Array.isArray(e.data.payload) ? e.data.payload : e.data.payload?.builds;
        if (builds?.length) {
          deliveries = (deliveries || Promise.resolve()).then(() => onBuilds(builds));
          deliveries.catch(error => gone(String(error?.message || error)));
        }
      }
      if (e.data.kind === "checkpoint") {
        const { board, ...cp } = e.data.payload;
        if (board) { job.boards[i] = board; job.board = woptMerge(job.boards.filter(Boolean), body); }
        // Only an UNSHARDED run can be resumed from a checkpoint — see above.
        if (shards === 1) {
          if (onCheckpoint) onCheckpoint(cp); else saveCheckpoint(body, cp, board || job.board);
        }
      }
      if (e.data.kind === "result") {
        releaseBudget();
        wd.done();
        signals.finish(i);
        w.terminate();
        job.workers[i] = null;
        const payload = e.data.payload;
        if (Array.isArray(payload?.builds)) {
          const builds = payload.builds; delete payload.builds;
          if (builds.length) deliveries = (deliveries || Promise.resolve()).then(() => onBuilds(builds));
        }
        const complete = () => {
          if (job.cancelled || job.result) return;
          job.parts[i] = payload;
          if (job.parts.every(Boolean)) {
            job.result = woptMerge(job.parts, body);
            if (!onCheckpoint) clearCheckpoint();
            job.workers = [];
          }
        };
        if (payload?.ok === false) {
          if (deliveries) deliveries.catch(error => gone(String(error?.message || error)));
          gone(payload.error || "The worker could not finish.");
        } else if (deliveries) deliveries.then(complete).catch(error => gone(String(error?.message || error)));
        else complete();
      }
      job.status = rollup();
    };
    w.onerror = (e) => { e?.preventDefault?.(); gone(String(e?.message || "worker error")); };
    signals.start(i, "resting");
    job.status = rollup();
    computeRun("solo", () => {
      if (job.cancelled || job.result) return;
      signals.start(i, "scoring"); job.status = rollup();
      try {
        wd.start();
        w.postMessage({ kind: "optimize", body: { ...body, shard: i, shards }, checkpoint: checkpoint || null });
      } catch (error) { gone(String(error?.message || error)); }
      return returned;
    }, () => !job.cancelled && !job.result).catch(error => gone(String(error?.message || error)));
  }
  wopt = job;
  return { ok: true, job_id: job.id };
}
function woptStatus() {
  if (!wopt) return { ok: false, error: "no such optimize job" };
  const st = wopt.status || { round: 0, rounds: 0, round_jobs: 0, round_runs: 0, sims_done: 0, sims_planned: 0, notes: [] };
  const out = { ...st, ok: true, job_id: wopt.id, elapsed_s: (Date.now() - wopt.t0) / 1000,
    // The worker's heartbeat carries its own phase (enumerating/running) —
    // keep it; the fallbacks cover the moments before the first message.
    phase: (wopt.status && wopt.status.phase) || (wopt.status ? "running" : "enumerating") };
  if (wopt.cancelled) {
    out.phase = "cancelled";
    // Cancel KILLED the worker, so there is no returned result and never will
    // be — hand back the last best-so-far it pushed out instead. It is already
    // result-shaped and flagged `cancelled`, so the normal renderer labels it
    // lower-precision without knowing where it came from.
    if (!wopt.result && wopt.board) out.result = wopt.board;
  }
  if (wopt.result) {
    out.result = wopt.result;
    out.phase = wopt.result.ok === false ? "error" : (wopt.result.cancelled ? "cancelled" : "done");
  }
  return out;
}
function woptCancel() {
  if (!wopt) return { ok: false, error: "no such optimize job" };
  // Cancel kills the WHOLE fleet. Each worker's last pushed board is already
  // here and merged, which is what a cancel has to show.
  if (wopt.cancel) wopt.cancel();
  else if (wopt.workers && wopt.workers.length) {
    wopt.workers.forEach((w) => w && w.terminate());
    wopt.workers = [];
    wopt.cancelled = true;
    wopt.releaseBudget?.();
  }
  return { ok: true, job_id: wopt.id };
}

async function api(path, body, onProgress, opts) {
  if (!WASM) {
    // GET endpoints (the rest are POST-with-body):
    if (path === "/api/meta" || path === "/api/i18n") return (await fetch(path)).json();
    return (await fetch(path, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body ?? {}), signal: opts && opts.signal,
    })).json();
  }
  // A FILE THE BUILD WROTE answers before any lane would; the engine is asked
  // only where the file did not come.
  if (ENGINE_VIEWS[path]) {
    try {
      const r = await fetch(ENGINE_VIEWS[path]);
      if (r.ok) return await r.json();
    } catch (_) { /* the engine answers it */ }
  }
  if (path === "/api/optimize") return woptStart(body, body && body.__resume, opts?.checkpoint, opts?.builds, opts?.searchWarm);
  if (path === "/api/optimize/status" || path === "/api/optimize/cancel") {
    if (body?.id != null && wopt?.id !== body.id) return { ok: false, error: "no such optimize job" };
    return path.endsWith("/cancel") ? woptCancel() : woptStatus();
  }
  // A LANE THAT DIED MID-CALL IS DROPPED AND THE CALL RE-ASKED ON A FRESH ONE.
  //
  // ONCE WAS NOT ENOUGH, and the assumption behind once was wrong: a refusal
  // was read as "the deployment is broken, and it will be broken again", which
  // is true of a module that will not download and false of a WORKER the
  // browser reclaimed. Reclaims are not surgical — the renderer takes several
  // at a time — so two dead lanes in a row is the ordinary shape of it, and a
  // single retry handed the caller a failure with twelve healthy lanes idle.
  // For the quick calc that caller is the BASELINE, and a failed baseline shows
  // the reader an empty list.
  //
  // STILL BOUNDED, and low: each attempt costs the watchdog's window, and a
  // pool that is genuinely gone must end in an answer rather than a loop.
  let r = null;
  for (let attempt = 0; attempt < 3; attempt++) {
    r = await freeLane().call(path, body, onProgress, !!(opts && opts.community),
      () => !opts?.signal?.aborted && (!opts?.live || opts.live()));
    if (!r || !r.worker_dead) return r;
    const i = pool.findIndex((l) => l && l.dead);
    if (i >= 0) pool[i] = null;
  }
  return r;
}
