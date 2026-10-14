// PROTOCOL 8 ON THE PAGE (web/src/static/app/69-board-work.js, docs/CLIENTS.md
// §"The cycle, protocol 8" and §"The work signal"), on the built site. The
// server is the page's own stub, installed before the app boots so it outlives
// a reload; a row is "measured" by a stub too, since the fold itself is
// check_compute's. Asserted: a result is kept in IndexedDB before anything else,
// survives a reload and goes in on the next load; the way out carries it by
// beacon; with no IndexedDB, or told `fallback: 7`, the page speaks protocol 7;
// `next_ms` holds; idle with nothing open it calls no worker; something open, it
// syncs within the jitter; a signal stale by the SERVER's clock, or unreadable,
// falls back to a direct sync every two minutes; pending points are the unconfirmed
// results' work over 1e9.
//   node scripts/check_compute_queue.mjs
//   WFSIM_BREAK=<keep|beacon|noidb|fallback|nextms|quiet|jitter|stale|pending> …  breaks one, to see it fail
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 13000 });
const { evaluate, check } = app;
const BREAK = process.env.WFSIM_BREAK || "";

// THE SERVER, before every document: sync and the signal answered by what the
// check set, every call kept in sessionStorage so a reload keeps the record.
await app.send("Page.addScriptToEvaluateOnNewDocument", { source: `(() => {
  const realFetch = window.fetch;
  const keep = () => { try { sessionStorage.setItem("q-calls", JSON.stringify(window.__calls)); } catch (_) {} };
  try { window.__calls = JSON.parse(sessionStorage.getItem("q-calls") || "[]"); } catch (_) { window.__calls = []; }
  const reply = (j) => new Response(JSON.stringify(j), { status: 200, headers: { "content-type": "application/json", date: new Date().toUTCString() } });
  window.fetch = async (url, init) => {
    const u = typeof url === "string" ? url : url.url;
    if (u.endsWith("/api/board/sync")) {
      const body = JSON.parse(init.body);
      window.__calls.push({ kind: "sync", at: Date.now(), body }); keep();
      const def = { ok: true, taken: (body.results || []).map((r) => r.question), dropped: [],
        held: (body.held || []).flatMap((h) => h.rows.map((q) => ({ question: q, held: true }))), pack: null, work: null, states: [], next_ms: 600000 };
      return reply(window.__syncReply ? window.__syncReply(body, def) : def);
    }
    if (u === "https://cdn.wfsim.app/work.json") {
      window.__calls.push({ kind: "signal", at: Date.now() }); keep();
      const s = window.__signal ? window.__signal() : null;
      if (s === "fail") throw new TypeError("unreachable");
      return reply(s || { v: 1, at: new Date().toISOString(), open: 0, urgent: 0, engine: window.ENGINE_ID, release: window.RELEASE_ID,
        protocol: 8, protocols: [8, 7, 6], computing: 1, poll_ms: 10000, jitter_ms: 2000, urgent_jitter_ms: 1000, mode: "signal" });
    }
    if (/\\/api\\/board\\/(work|answers|renew|release|points|mine)$/.test(u)) {
      window.__calls.push({ kind: u.split("/").pop(), at: Date.now() }); keep();
      return reply({ ok: true, work: null, pack: null });
    }
    return realFetch(url, init);
  };
  navigator.sendBeacon = (p, b) => { window.__calls.push({ kind: "beacon", path: p, body: JSON.parse(String(b)) }); keep(); return true; };
})();` });
await app.send("Page.reload");
await app.sleep(6000);
await evaluate(`(async () => { const end = Date.now() + 30000; while (!window.__wfsimReady && Date.now() < end) await new Promise((r) => setTimeout(r, 100)); return true; })()`);

// THE PAGE'S SIDE: a row measured instantly, one call of the loop at a time.
const SETUP = `
  window.__break = ${JSON.stringify(BREAK)};
  const wait = async (ok, ms) => { const end = Date.now() + ms; while (!(await ok())) { if (Date.now() > end) return false; await new Promise((r) => setTimeout(r, 25)); } return true; };
  const idle = () => wait(() => !!computeWake, 20000);
  const kick = async () => { await idle(); const w = computeWake; if (w) w(); await new Promise((r) => setTimeout(r, 50)); };
  const syncs = () => window.__calls.filter((c) => c.kind === "sync");
  const idbResults = () => new Promise((ok) => { const rq = indexedDB.open("wfsim-compute", 1);
    rq.onsuccess = () => { const g = rq.result.transaction("results").objectStore("results").getAll(); g.onsuccess = () => ok(g.result); g.onerror = () => ok([]); };
    rq.onerror = () => ok([]); });
  const fresh = (o = {}) => { heldPack = null; packQueue = []; refusedPending = []; lostRows = new Set(); syncNotBefore = 0; syncFailures = 0;
    lastSyncAt = Date.now(); bootSyncAt = 1; window.__calls.length = 0; window.__syncReply = null; window.__signal = null; Object.assign(window, o); };
  const signal = (o = {}) => () => ({ v: 1, at: new Date().toISOString(), open: 0, urgent: 0, engine: ENGINE_ID, release: RELEASE_ID,
    protocol: 8, protocols: [8, 7, 6], computing: 1, poll_ms: 10000, jitter_ms: 2000, urgent_jitter_ms: 1000, mode: "signal", ...o });
  measureRow = async () => ({ score: 1.5, metric: "kpm", work: 7e9, compute_ms: 1000, counts: { gas: 1 }, lanes: 1 });
  if (!window.__realApi) window.__realApi = api;
  api = (p, b, x, o) => (p === "/api/board/order" ? Promise.resolve({ ok: true, request: { runs: 1 } }) : window.__realApi(p, b, x, o));
  if (window.__break === "keep") idbWrite = async () => {};
  if (window.__break === "nextms") { const real = takeSynced; takeSynced = async (r, ...a) => { r.next_ms = 0; return real(r, ...a); }; }
  if (window.__break === "quiet") { const real = readSignal; readSignal = async () => { const x = await real(); return x && { ...x, open: 1 }; }; }
  if (window.__break === "beacon") giveBack8 = () => {};
  if (window.__break === "jitter") { const real = readSignal; readSignal = async () => { const x = await real(); syncDials.jitter_ms = 30000; return x; }; }
  if (window.__break === "stale") serverNow = () => Date.now();
  if (window.__break === "pending") pendingPoints = () => Math.floor(resultsMem.reduce((t, r) => t + r.work, 0) / 1e9);
  localStorage.setItem("wfsim-verifier", "a1".repeat(12));
  localStorage.setItem("wfsim-compute-consent", JSON.stringify({ v: COMPUTE_CONSENT_V, on: true, at: new Date().toISOString() }));
  setCommunityShare(100); lastTouched = 0;
`;

// 1. A ROW'S RESULT IS KEPT BEFORE ANYTHING ELSE, AND KEPT UNTIL A REPLY TAKES IT.
const one = JSON.parse(await evaluate(`(async () => {
  ${SETUP}
  const out = { wasm: !!WASM };
  if (!WASM) return JSON.stringify(out);
  boardProtocol = 0;
  await openResults();
  fresh();
  window.__signal = signal({ open: 3 });
  let handed = false;
  window.__syncReply = (body, def) => ({ ...def, taken: [], next_ms: 0,
    pack: handed ? null : (handed = true, { lease: "b".repeat(32), lease_ms: 300000,
      tasks: [{ question: "c".repeat(32), identity: "q-row", record: { weapon: "braton_prime" }, ruler: "standard_single_target", mode: "base" }] }) });
  await kick();
  out.protocol = boardProtocol;
  out.kept = await wait(async () => (await idbResults()).some((r) => r.question === "c".repeat(32)), 20000);
  // …AND WITH ONE TAKEN, NOT YET CONFIRMED, BESIDE IT: both count.
  pendingMem.push({ question: "d".repeat(32), verifier: "a1".repeat(12), engine: ENGINE_ID, work: 3.2e9, at: Date.now() });
  out.pending = pendingPoints();
  pendingMem = pendingMem.filter((x) => x.question !== "d".repeat(32));
  // THE WAY OUT carries it by beacon, holding nothing.
  window.__syncReply = (body, def) => ({ ...def, taken: [], next_ms: 600000 });
  await idle();
  dispatchEvent(new Event("pagehide"));
  const b = window.__calls.find((c) => c.kind === "beacon" && c.path === "/api/board/sync");
  out.beacon = !!b && (b.body.results || []).some((r) => r.question === "c".repeat(32)) && Array.isArray(b.body.held) && b.body.held.length === 0;
  out.stillKept = (await idbResults()).some((r) => r.question === "c".repeat(32));
  window.__calls.length = 0;
  window.__syncReply = null;
  return JSON.stringify(out);
})()`));
check("protocol 8 is spoken where IndexedDB opens", !one.wasm || one.protocol === 8, JSON.stringify(one));
check("a measured row's result is kept in IndexedDB until a reply takes it", !one.wasm || one.kept, JSON.stringify(one));
check("the way out sends the kept results by beacon, holding nothing, and keeps them", !one.wasm || (one.beacon && one.stillKept), JSON.stringify(one));
check("pending points are the unconfirmed results' work over 1e9", !one.wasm || one.pending === 10, JSON.stringify(one));

// 2. …AND A RELOAD KEEPS IT: the next load sends it within its first moments.
await app.send("Page.reload");
await app.sleep(4000);
const two = JSON.parse(await evaluate(`(async () => {
  const end = Date.now() + 40000;
  while (Date.now() < end) {
    const s = window.__calls.find((c) => c.kind === "sync" && (c.body.results || []).some((r) => r.question === "c".repeat(32)));
    if (s) return JSON.stringify({ wasm: !!WASM, sent: true });
    await new Promise((r) => setTimeout(r, 200));
  }
  return JSON.stringify({ wasm: !!WASM, sent: false, calls: window.__calls.map((c) => c.kind) });
})()`));
check("a kept result survives a reload and goes in on the next load", !two.wasm || two.sent, JSON.stringify(two));

// 3. THE LOOP'S OTHER RULES, one at a time.
const three = JSON.parse(await evaluate(`(async () => {
  ${SETUP}
  const out = { wasm: !!WASM };
  if (!WASM) return JSON.stringify(out);
  await openResults();
  boardProtocol = 8;
  // IDLE, NOTHING OPEN: the signal is read, no worker is called.
  fresh(); window.__signal = signal({ open: 0 });
  for (let i = 0; i < 3; i++) await kick();
  await new Promise((r) => setTimeout(r, 300));
  out.quietSignal = window.__calls.filter((c) => c.kind === "signal").length;
  out.quietWorker = window.__calls.filter((c) => c.kind !== "signal").length;
  // SOMETHING OPEN: a sync within the signal's jitter.
  fresh(); window.__signal = signal({ open: 5, jitter_ms: 1500 });
  window.__syncReply = (body, def) => ({ ...def, next_ms: 600000 });
  const t0 = Date.now();
  await kick();
  out.jitterSynced = await wait(() => syncs().length > 0, 4000);
  out.jitterMs = syncs().length ? syncs()[0].at - t0 : null;
  // NEXT_MS HOLDS: told to come back in a minute, it does not sync again for work.
  fresh(); window.__signal = signal({ open: 5, jitter_ms: 0 });
  window.__syncReply = (body, def) => ({ ...def, next_ms: 60000 });
  for (let i = 0; i < 4; i++) { await kick(); await new Promise((r) => setTimeout(r, 300)); }
  out.nextMsSyncs = syncs().length;
  // A STALE SIGNAL BY THE SERVER'S CLOCK — this one's three minutes fast — falls back.
  fresh(); lastSyncAt = Date.now() - 200000;
  const behind = new Date(Date.now() - 180000).toISOString();
  window.__signal = signal({ open: 0, at: behind });
  serverSkewMs = -180000;
  await kick(); await new Promise((r) => setTimeout(r, 300));
  out.freshByServer = syncs().length;
  fresh(); lastSyncAt = Date.now() - 200000; serverSkewMs = 0;
  window.__signal = signal({ open: 0, at: behind });
  await kick();
  out.staleFellBack = await wait(() => syncs().length > 0, 4000);
  // UNREADABLE: the same fallback, and not before two minutes since the last sync.
  fresh(); lastSyncAt = Date.now() - 200000; window.__signal = () => "fail";
  await kick();
  out.failFellBack = await wait(() => syncs().length > 0, 4000);
  fresh(); window.__signal = () => "fail";
  await kick(); await new Promise((r) => setTimeout(r, 300));
  out.failHeld = syncs().length;
  // TOLD FALLBACK: protocol 7 for the rest of the page.
  fresh(); window.__signal = signal({ open: 5, jitter_ms: 0 });
  window.__syncReply = () => ({ ok: true, fallback: 7 });
  if (window.__break === "fallback") window.__syncReply = (body, def) => ({ ...def, next_ms: 0 });
  await kick();
  await wait(() => boardProtocol === 7, 4000);
  window.__calls.length = 0;
  await kick();
  out.fellBackTo = boardProtocol;
  out.askedWork = await wait(() => window.__calls.some((c) => c.kind === "work"), 4000);
  return JSON.stringify(out);
})()`));
check("idle with nothing open, the page reads the signal and calls no worker",
  !three.wasm || (three.quietSignal >= 1 && three.quietWorker === 0), JSON.stringify(three));
check("with work open, it syncs within the signal's jitter", !three.wasm || (three.jitterSynced && three.jitterMs <= 1500 + 600), JSON.stringify(three));
check("next_ms holds: no second sync for work before it", !three.wasm || three.nextMsSyncs === 1, JSON.stringify(three));
check("a signal is aged by the server's clock: fresh to a page whose clock is fast, stale and fallen back from by the server's",
  !three.wasm || (three.freshByServer === 0 && three.staleFellBack), JSON.stringify(three));
check("an unreadable signal falls back to a direct sync, at most every two minutes", !three.wasm || (three.failFellBack && three.failHeld === 0), JSON.stringify(three));
check("told fallback: 7, the page speaks protocol 7 for the rest of the session", !three.wasm || (three.fellBackTo === 7 && three.askedWork), JSON.stringify(three));

// 4. NO INDEXEDDB: protocol 7 from the start.
const four = JSON.parse(await evaluate(`(async () => {
  ${SETUP}
  const out = { wasm: !!WASM };
  if (!WASM) return JSON.stringify(out);
  fresh();
  boardProtocol = 0; computeDb = null;
  if (window.__break !== "noidb") Object.defineProperty(window, "indexedDB", { value: undefined, configurable: true });
  await kick();
  out.protocol = boardProtocol;
  out.askedWork = await wait(() => window.__calls.some((c) => c.kind === "work"), 4000);
  out.synced = syncs().length;
  return JSON.stringify(out);
})()`));
check("with no IndexedDB the page speaks protocol 7", !four.wasm || (four.protocol === 7 && four.askedWork && four.synced === 0), JSON.stringify(four));

await evaluate(`(() => { localStorage.removeItem("wfsim-compute-consent"); localStorage.removeItem("wfsim-community-share"); sessionStorage.removeItem("q-calls"); return true; })()`);
await app.finish("protocol 8 on the page keeps every result, waits on the signal, and holds to the server's clock and next_ms");
