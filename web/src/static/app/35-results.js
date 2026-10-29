// ---- RESULTS — every measurement, kept ---------------------------------------
//
// docs/UI.md §"Results". A run of the simulator is a RECORD of its own, written
// once and never rewritten: the build and the fight as they were, the engine
// that measured them, and the summary. A preset holds no result — "this build's
// number" is the newest record naming it (`resultLatest`), so a second run does
// not erase the first and two runs can be set side by side.
//
// Stored in IndexedDB (`wfsim` / `results`), mirrored in `results` for the
// synchronous readers. With no IndexedDB (a private window) the mirror is all
// there is, and a reload forgets it.
const RESULTS_DB = "wfsim";
const RESULTS_STORE = "results";
/// WHAT A RECORD'S SHAPE IS. A record is never migrated: a field it lacks was
/// not measured when it was written, and every reader says "not recorded"
/// rather than reading it as 0. A new field raises this and nothing else.
const RESULT_SCHEMA = 1;
/// How many unpinned records one build keeps per fight; a pinned one is kept
/// until it is deleted.
const RESULT_KEEP = 5;
/// Every record, oldest first.
let results = [];
let resultsDbOpen = null;
const resultsChannel = typeof BroadcastChannel === "function" ? new BroadcastChannel("wfsim-results") : null;

function resultsDb() {
  if (resultsDbOpen) return resultsDbOpen;
  resultsDbOpen = new Promise((ok) => {
    let req;
    try { req = indexedDB.open(RESULTS_DB, 1); } catch (_) { ok(null); return; }
    req.onupgradeneeded = () => {
      if (!req.result.objectStoreNames.contains(RESULTS_STORE)) req.result.createObjectStore(RESULTS_STORE, { keyPath: "id" });
    };
    req.onsuccess = () => ok(req.result);
    req.onerror = () => ok(null);
    req.onblocked = () => ok(null);
  });
  return resultsDbOpen;
}

/// One transaction over the store; resolves true when it committed.
async function resultsTx(mode, work) {
  const db = await resultsDb();
  if (!db) return false;
  return new Promise((ok) => {
    let tx;
    try { tx = db.transaction(RESULTS_STORE, mode); } catch (_) { ok(false); return; }
    const out = work(tx.objectStore(RESULTS_STORE));
    tx.oncomplete = () => ok(out === undefined ? true : out);
    tx.onerror = () => ok(false);
    tx.onabort = () => ok(false);
  });
}

async function loadResults() {
  const got = await resultsTx("readonly", (s) => {
    const box = { rows: [] };
    s.getAll().onsuccess = (e) => { box.rows = e.target.result || []; };
    return box;
  });
  if (got && got.rows) results = got.rows.sort((a, b) => (a.at || 0) - (b.at || 0));
  await lastResultsToRecords();
  if (typeof META !== "undefined" && META) renderStoredSimResult();
}

/// THE RUN AS A RECORD. The replay stays out — it is the one part of a result
/// a button regenerates, and it is `resultMem`'s for the session.
function resultContext() {
  return { weapon: presetWeapon(), preset: activePreset, scenario: activeScenario,
    key: simKey(), fight: snapshotScenario() };
}

function resultRecord(r, build, context = resultContext(), id = presetNewId()) {
  return {
    id, schema: RESULT_SCHEMA, at: Date.now(),
    engine: ENGINE_ID, release: RELEASE_ID,
    ...context, build,
    kept: false, r: { ...r, replay: null },
  };
}

/// Every record of one build, newest first.
const resultsFor = (weapon, preset) =>
  preset ? results.filter((x) => x.weapon === weapon && x.preset === preset).reverse() : [];
/// THE BUILD'S NUMBER: its newest record, under whatever fight — `key` says
/// whether that is still the fight and the build on screen.
const resultLatest = (weapon, preset) => resultsFor(weapon, preset)[0] || null;

/// Task completion adopts this stable record id after a crash between stores.
async function resultAddCaptured(r, build, context, id, persistent) {
  const existing = results.find((record) => record.id === id);
  if (existing) return existing;
  const rec = resultRecord(r, build, context, id);
  const same = (record) => !record.kept && record.weapon === rec.weapon && record.preset === rec.preset && record.scenario === rec.scenario;
  const drop = results.filter(same).slice(0, 1 - RESULT_KEEP).map((record) => record.id);
  const stored = await resultsTx("readwrite", (store) => { store.put(rec); drop.forEach((key) => store.delete(key)); });
  if (!stored && persistent) throw new Error("The result could not be saved. Retry this task.");
  results = results.filter((record) => record.id !== id && !drop.includes(record.id)).concat(rec);
  if (stored && resultsChannel) resultsChannel.postMessage("changed");
  return rec;
}

/// Record a run, and trim what that build keeps for that fight. The blank
/// (no preset open) records nothing: no build is there for a record to name.
function resultAdd(r, build) {
  if (!activePreset) return null;
  const rec = resultRecord(r, build);
  results.push(rec);
  const same = (x) => !x.kept && x.weapon === rec.weapon && x.preset === rec.preset && x.scenario === rec.scenario;
  const drop = results.filter(same).slice(0, -RESULT_KEEP).map((x) => x.id);
  results = results.filter((x) => !drop.includes(x.id));
  resultsTx("readwrite", (s) => { s.put(rec); drop.forEach((id) => s.delete(id)); })
    .then((ok) => { if (ok && resultsChannel) resultsChannel.postMessage("changed"); });
  return rec;
}

/// Pin or unpin one record: a pinned record is never trimmed.
function resultKeep(id, kept) {
  const rec = results.find((x) => x.id === id);
  if (!rec) return;
  rec.kept = !!kept;
  resultsTx("readwrite", (s) => { s.put(rec); });
}

function resultDelete(id) {
  results = results.filter((x) => x.id !== id);
  resultsTx("readwrite", (s) => { s.delete(id); });
}

/// ONE-TIME, and again whenever an older page wrote one: a `lastResult` inside a
/// saved entry becomes a record, and leaves the entry only once the record is
/// stored. It keeps what the entry recorded and nothing it did not — no engine,
/// and the fight only as the `key` states it.
async function lastResultsToRecords() {
  const lists = Object.keys(localStorage).filter((k) => k.startsWith("wfsim-presets-"));
  const made = [], touched = new Map();
  for (const k of lists) {
    let list;
    try { list = JSON.parse(localStorage.getItem(k)); } catch (_) { continue; }
    if (!Array.isArray(list)) continue;
    for (const p of list) {
      const lr = p && p.lastResult;
      if (!lr) continue;
      if (lr.r) {
        let fight;
        try { fight = JSON.parse(lr.key)[4]; } catch (_) { fight = undefined; }
        made.push({
          id: presetNewId(), schema: RESULT_SCHEMA, at: lr.at || 0, weapon: p.scope || "", preset: presetId(p),
          key: lr.key, ...(fight ? { fight } : {}), kept: false, r: { ...lr.r, replay: null },
        });
      }
      delete p.lastResult;
      touched.set(k, list);
    }
  }
  if (!touched.size) return;
  const stored = !made.length || await resultsTx("readwrite", (s) => { made.forEach((x) => s.put(x)); });
  results = results.concat(made).sort((a, b) => (a.at || 0) - (b.at || 0));
  // NOT STORED, NOT MOVED: the entry keeps it, and the next load tries again.
  if (!stored) return;
  for (const [k, list] of touched) {
    try { localStorage.setItem(k, JSON.stringify(list)); } catch (_) { /* moved again next load */ }
  }
}

if (resultsChannel) resultsChannel.onmessage = () => { loadResults(); };
loadResults();
