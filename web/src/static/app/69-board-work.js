// SPDX-License-Identifier: AGPL-3.0-or-later
/// THE BOARD'S WORK, DONE ON THIS MACHINE — one compute order at a time, in the
/// background (docs/BOARD.md §"Compute orders"). The worker hands out a build,
/// a ruler and a mode; this fights it by the scorer's own path and sends back
/// the number. Whether it is the first measurement of that row or the second
/// that confirms one, this side cannot tell and does not need to.
///
/// THE SCORER'S PATH, NOT A COPY OF IT. `/api/board/order` names the fight off
/// the library's record, `/api/board/runs` fights its runs, `/api/board/fold`
/// folds them in run order and `/api/board/score` ends it the scorer's way —
/// all `webapi::board_rows` — so the number sent is the scorer's to the bit.
///
/// SOLO AND TOGETHER, SIDE BY SIDE (docs/BOARD.md §"Contribution" rule 0): the
/// reader's own computing takes its share (06-transport.js `computePct`) and
/// this takes the together share (`communityShare`), each on workers of its own,
/// and neither pauses for the other. A piece is sized to about `PIECE_MS`, so a
/// changed share takes hold within one; what the pieces take is `communityPower`.
const PIECE_MS = 250;

/// ONE ROW, MEASURED: its runs fought on the together lanes, folded in run
/// order, then scored, with what the pieces and the folds took in ms summed over
/// the lanes (`compute_ms`) — the rests between them (`communityRest`) are not
/// in it — and how many lanes there were (`lanes`).
/// `null` when the engine refused or `live()` went false between pieces.
///
/// EVERY LANE FIGHTS, ONE MERGES. A run depends on its index alone, so lanes
/// pull pieces off one cursor (`/api/board/runs`, a shard per run) and the
/// shards are folded as `pieces` strictly in run order — the scorer's merges in
/// the scorer's order, so the bits are the scorer's however the lanes raced.
/// A lane past `communityLanes()` — the share was lowered — takes no new piece.
/// `from` is a fold cut short (`RESUME_KEY`): `{ acc, folded, spent }`, folded on
/// from run `folded`; `onPiece` is handed the same after every fold.
/// IN THE DESKTOP CLIENT THE CALLS GO NATIVE once proven (82-desktop.js
/// `nativeBoardReady`), the same pieces in the same order; a native call that
/// fails is asked of the lane's wasm instead, and the session stays there.
async function measureRow(request, ruler, live, onPiece = () => {}, from = null) {
  const runs = Number(request.runs) || 0;
  const native = await nativeBoardReady();
  const ls = await boardLanes(communityLanes());
  let cursor = from ? from.folded : 0, est = null, failed = false, spent = from ? from.spent : 0, folded = cursor;
  const ready = new Map();
  let acc = from ? from.acc : null;
  let merging = Promise.resolve();
  // ONE CALL ON A LANE, asked again on a fresh worker when a Stop or a lost
  // worker took it: the range is the same, so the answer is too.
  const ask = async (k, path, body) => {
    if (native && !nativeOff) {
      if (!live()) return null;
      const r = await nativeApi(path, body);
      if (r) return r.ok ? r : null;
    }
    for (let tries = 0; tries < 3; tries++) {
      if (!live()) return null;
      const r = await boardLaneAt(k).call(path, body, null, true);
      if (!(r && (r.cancelled || r.worker_dead))) return r && r.ok ? r : null;
    }
    return null;
  };
  const fold = () => {
    const pieces = [];
    while (ready.has(folded + pieces.length)) pieces.push(ready.get(folded + pieces.length));
    pieces.forEach((_, i) => ready.delete(folded + i));
    if (!pieces.length) return;
    folded += pieces.length;
    const upto = folded;
    merging = merging.then(async () => {
      if (failed) return;
      const began = performance.now();
      const step = await ask(0, "/api/board/fold", { request, acc, pieces });
      spent += performance.now() - began;
      if (step) { acc = step.acc; onPiece(Math.min(runs, folded), runs, { acc, folded: upto, spent }); } else failed = true;
    });
  };
  await Promise.all(ls.map(async (_, k) => {
    while (!failed && cursor < runs) {
      if (!live()) { failed = true; return; }
      if (k >= communityLanes()) { await new Promise((r) => setTimeout(r, PIECE_MS)); continue; }
      const left = runs - cursor;
      const want = est ? Math.max(1, Math.round(PIECE_MS / est)) : 1;
      const count = Math.max(1, Math.min(want, 1000, Math.ceil(left / ls.length), left));
      const from = cursor;
      cursor += count;
      const began = performance.now();
      const r = await ask(k, "/api/board/runs", { request, from, count });
      if (!r || !Array.isArray(r.shards) || r.shards.length !== count) { failed = true; return; }
      const ms = Math.max(1, performance.now() - began);
      spent += ms;
      est = est === null ? ms / count : est * 0.7 + (ms / count) * 0.3;
      r.shards.forEach((x, i) => ready.set(from + i, x));
      fold();
      const rest = communityRest(ms);
      if (rest) await boardLaneAt(k).send({ kind: "rest", ms: rest }, null, true);
    }
  }));
  await merging;
  if (failed || folded !== runs) return null;
  const began = performance.now();
  const s = await ask(0, "/api/board/score", { ruler, request, acc });
  spent += performance.now() - began;
  return s && Number.isFinite(s.score) ? { ...s, compute_ms: Math.round(spent), lanes: ls.length } : null;
}

/// WHO IS WORKING: a random id this browser makes for itself, joined to no
/// account and to no submission, sent with this work alone — so a client caught
/// once can be refused (docs/BOARD.md §"Compute orders"). A browser that cannot
/// keep it does not work: a new id every visit is a ban nobody can apply.
const VERIFIER_KEY = "wfsim-verifier";
const VERIFIED_KEY = "wfsim-board-verified";
/// How long an idle browser waits before asking again; one that just finished
/// an order asks at once.
const ASK_EVERY_MS = 10_000;

function verifierId() {
  try {
    let id = localStorage.getItem(VERIFIER_KEY);
    if (!id || !/^[a-z0-9]{16,40}$/.test(id)) {
      id = [...crypto.getRandomValues(new Uint8Array(12))].map((x) => x.toString(16).padStart(2, "0")).join("");
      localStorage.setItem(VERIFIER_KEY, id);
    }
    return localStorage.getItem(VERIFIER_KEY) === id ? id : null;
  } catch (_) { return null; }
}

/// NOTHING IS COMPUTED UNTIL THE READER SAID YES — to the statement they were
/// shown, `COMPUTE_CONSENT_V` (73-compute.js `computeAskHtml`), asked once,
/// inline. A yes or a no is kept with when it was given, and the yes travels
/// with every ask for work so the server keeps it too; a new statement asks
/// again. Running a stranger's computer without that is what the law calls
/// controlling it, whatever it computes (docs/BOARD.md §"Contribution").
const CONSENT_KEY = "wfsim-compute-consent";
const COMPUTE_CONSENT_V = 3;
function computeConsent() {
  try {
    const c = JSON.parse(localStorage.getItem(CONSENT_KEY) || "null");
    return c && c.v === COMPUTE_CONSENT_V ? c : null;
  } catch (_) { return null; }
}
function boardVerifyOn() {
  const c = computeConsent();
  return !!(c && c.on);
}
function setBoardVerify(on) {
  try {
    localStorage.setItem(CONSENT_KEY, JSON.stringify({ v: COMPUTE_CONSENT_V, on: !!on, at: new Date().toISOString() }));
  } catch (_) { /* private mode: nothing kept, so nothing runs */ }
  renderBoardConsent();
  computeRedraw();
  computeChrome();
  renderComputePicker();
}

/// HOW MUCH OF THE DEVICE: a share of its processing power the reader sets in
/// the compute menu (`COMMUNITY_SHARES`), 30% unless they do, whether or not
/// they are using it — the shape of their own share, with off beside it. The
/// statement they agreed to says so (`COMPUTE_CONSENT_V`).
const COMMUNITY_SHARE_KEY = "wfsim-community-share";
const COMMUNITY_SHARES = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
function communityShare() {
  try {
    const n = Number(localStorage.getItem(COMMUNITY_SHARE_KEY));
    return COMMUNITY_SHARES.includes(n) ? n : 30;
  } catch (_) { return 30; }
}
function setCommunityShare(pct) {
  try { localStorage.setItem(COMMUNITY_SHARE_KEY, String(pct)); } catch (_) { /* this page only */ }
}
/// THE PROCESSING POWER IT TAKES NOW, in cores, fractional. Solo and together
/// add up to no more than 100% by default and then never touch; past it, while
/// the reader's own work runs, the two share the device in proportion — solo
/// counted at what it really holds (`poolSize`, whole lanes), since a lane
/// does not rest.
function communityPower() {
  const cores = detectedCores().n, together = communityShare();
  const solo = (100 * Math.min(cores, poolSize())) / cores;
  const share = readerBusy() && solo + together > 100 ? (100 * together) / (solo + together) : together;
  return (cores * share) / 100;
}
/// …AS LANES, rounded up, each running `communityDuty()` of the time: 30% of
/// eight cores is three lanes at 80%, and 30% of one core is one lane at 30%,
/// so the share is the share on any device. A lane rests after each piece for
/// as long as the duty asks (`communityRest`), in its worker, where a page in
/// the background does not slow the clock.
function communityLanes() {
  return Math.max(1, Math.ceil(communityPower() - 1e-9));
}
const communityDuty = () => Math.min(1, communityPower() / communityLanes());
/// The rest after `ms` of work, capped well inside a lane's silence watch.
const communityRest = (ms) => {
  const d = communityDuty();
  return d >= 0.999 ? 0 : Math.min(30_000, Math.round((ms * (1 - d)) / d));
};

/// …AND EVEN WITH A YES, NOT NOW: on battery unless the reader allows it
/// (`computeOnBattery`), or with the browser's data saver on. `why` says which.
/// A device that does not report its power counts as on power: the reader
/// decides whether it computes, not a guess at what kind of device it is.
const BATTERY_KEY = "wfsim-compute-battery";
function computeOnBattery() {
  try { return localStorage.getItem(BATTERY_KEY) === "yes"; } catch (_) { return false; }
}
function setComputeOnBattery(on) {
  try { localStorage.setItem(BATTERY_KEY, on ? "yes" : "no"); } catch (_) { /* this page only */ }
  computeRedraw(); computeChrome(); renderComputePicker();
}
let computeBattery = null;
try {
  if (navigator.getBattery) {
    navigator.getBattery().then((b) => {
      computeBattery = b;
      b.addEventListener("chargingchange", () => { computeRedraw(); computeChrome(); });
    }).catch(() => {});
  }
} catch (_) { /* no battery API: a desktop, as far as this can tell */ }
function computeHeld() {
  if (computeBattery && !computeBattery.charging && !computeOnBattery()) return "battery";
  if (navigator.connection && navigator.connection.saveData) return "data";
  return "";
}
function boardVerifiedCount() {
  try { return Number(localStorage.getItem(VERIFIED_KEY)) || 0; } catch (_) { return 0; }
}

/// A SIGNED-IN READER'S BROWSER IS THEIRS: claimed once for the account, so
/// the work it does counts under their name (docs/BOARD.md §"Contribution").
/// THE CLAIM IS SENT ONCE A PAGE AND ACCOUNT, and the server's answer is the only
/// record of it: a mark kept here said "claimed" for a device the server had
/// never joined — one removed, one whose id changed — and no page claimed it again.
let claimedFor = null;
async function claimDevice(id) {
  const account = accountState.account && accountState.account.id;
  if (!account || claimedFor === `${account}:${id}`) return;
  const label = await Promise.race([deviceGuessReady(), new Promise((ok) => setTimeout(() => ok(computeDeviceGuess()), 3000))]);
  const r = await accountCall("POST", "/api/account/devices/claim", { verifier: id, label });
  if (r && r.ok) { claimedFor = `${account}:${id}`; devicePointsAt = 0; }
}

/// WHAT THIS BROWSER HAS EARNED — `{ points, recent, claimed }`, asked by its
/// own secret id. A fact is made when another client agrees, which can be
/// hours later, so it is asked at most every `POINTS_EVERY_MS`.
let devicePoints = null;
let devicePointsAt = 0;
const POINTS_EVERY_MS = 5 * 60_000;
async function loadDevicePoints() {
  if (!WASM || !boardVerifyOn() || Date.now() - devicePointsAt < POINTS_EVERY_MS) return;
  const id = verifierId();
  if (!id) return;
  devicePointsAt = Date.now();
  const r = await postBoardWork("/api/board/points", { verifier: id, since: pointsSince() });
  if (r && r.ok) await takeDevicePoints(r);
}
/// …AND ITS "TODAY": THE UTC DAY, as every period of the product is, its start
/// said on the page in the reader's own zone (73-compute.js `computeDayHtml`).
function pointsSince() {
  return new Date().toISOString().slice(0, 10) + "T00";
}
async function takeDevicePoints(r) {
  devicePoints = r;
  // WHETHER THE RANKING MAY NAME THEM is asked here, where they see their points.
  if (accountState.account && !devicesState) await loadDevices();
  renderBoardConsent();
}

/// EVERY ASK HAS A CLOCK: one call that never answers — a laptop waking on a
/// dead connection — would otherwise hold the work loop for good.
const BOARD_ASK_MS = 30_000;
const boardSignal = () => timeoutSignal(BOARD_ASK_MS);
const postBoardTo = (base, path, body) => fetch(base + path, {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(body),
  signal: boardSignal(),
}).then((r) => { learnServerClock(r); return r.ok ? r.json() : null; }).catch(() => null);
/// THE SERVER'S CLOCK, as the offset of a same-origin reply's `Date` from this
/// one's: a reader's clock minutes off would read a fresh work signal as stale.
let serverSkewMs = null;
function learnServerClock(r) {
  const d = r && r.headers && Date.parse(r.headers.get("date") || "");
  if (Number.isFinite(d)) serverSkewMs = d - Date.now();
}
function serverNow() { return Date.now() + (serverSkewMs || 0); }

/// THE RELAY (docs/DISTRIBUTION.md §"The relay"): a server in Hong Kong that
/// carries the board's work for readers whose road to the edge is poor. The page
/// times both roads every `ROUTE_EVERY_MS` and takes the quicker, unless the
/// reader named one (`computeRoad`); a call the relay drops goes direct at once,
/// and the relay is left for that long.
const RELAY = "https://cn.wfsim.app", ROUTE_EVERY_MS = 10 * 60_000, ROUTE_TRIES = 3;
/// WHAT THE RELAY CARRIES — its nginx site lists the same paths, and anything
/// else it answers 404, which a call here would read as the relay dropping it.
const RELAY_PATHS = ["/api/board/sync", "/api/board/work", "/api/board/answers", "/api/board/renew", "/api/board/release",
  "/api/board/mine", "/api/board/points", "/api/board/ping"];
let routeBase = "", routeAt = 0, routing = null;
function relayServes() { return /^(https:\/\/wfsim\.app|https?:\/\/wfsim\.localhost)$/.test(location.origin); }
async function timeRoute(base) {
  const ms = [];
  for (let i = 0; i < ROUTE_TRIES; i++) {
    const t = performance.now();
    try {
      const r = await fetch(base + "/api/board/ping", { cache: "no-store", signal: timeoutSignal(5_000) });
      if (!r.ok) return Infinity;
      await r.json();
    } catch (_) { return Infinity; }
    ms.push(performance.now() - t);
  }
  return ms.sort((a, b) => a - b)[Math.floor(ms.length / 2)];
}
/// THE READER MAY NAME THE ROAD in the compute menu: "auto" times both, "direct"
/// or "relay" is taken every time and nothing falls back to the other — the
/// reader chose it, and a road that quietly changed under them is not a choice.
const ROAD_KEY = "wfsim-compute-road", ROADS = ["auto", "direct", "relay"];
function computeRoad() {
  try { const v = localStorage.getItem(ROAD_KEY); return ROADS.includes(v) ? v : "auto"; } catch (_) { return "auto"; }
}
function setComputeRoad(v) {
  try { if (v === "auto") localStorage.removeItem(ROAD_KEY); else if (ROADS.includes(v)) localStorage.setItem(ROAD_KEY, v); } catch (_) { /* this page only */ }
  routeAt = 0;
}
function chooseRoute() {
  if (!relayServes()) return Promise.resolve("");
  const road = computeRoad();
  if (road !== "auto") return Promise.resolve(road === "relay" ? RELAY : "");
  if (Date.now() - routeAt < ROUTE_EVERY_MS) return Promise.resolve(routeBase);
  routing = routing || Promise.all([timeRoute(""), timeRoute(RELAY)]).then(([direct, relay]) => {
    routeBase = relay < direct ? RELAY : "";
    routeAt = Date.now();
    routing = null;
    return routeBase;
  });
  return routing;
}
/// ONE CALL OF THE BOARD'S WORK, by the chosen road — the relay carries
/// `RELAY_PATHS` alone — its answer or null. A call one road drops goes by the
/// other at once, and the road that answered is kept until the next timing; one
/// both drop has the roads timed again on the next call. A road the reader named
/// is never left. `roadDropped` names the road that dropped the last call:
/// "relay", "direct" or "".
let roadDropped = "";
async function postBoardWork(path, body) {
  const board = RELAY_PATHS.includes(path) && relayServes();
  const base = board ? await chooseRoute() : "";
  const r = await postBoardTo(base, path, body);
  roadDropped = r || !board ? "" : base ? "relay" : "direct";
  if (r || !board || computeRoad() !== "auto") return r;
  const other = base ? "" : RELAY;
  const again = await postBoardTo(other, path, body);
  routeBase = other;
  routeAt = again ? Date.now() : 0;
  return again;
}

/// HOW EACH ROAD CARRIES A PUBLIC FILE, once a day per browser, as `road.probe`
/// points: whether to send public reads by the relay too is decided on these,
/// split by country, and not on a guess. `board.meta.json` is small, public and
/// what every board reader fetches first; each road is timed alone, in turn.
/// DIRECT IS THE ROAD THE PAGE READS IT BY, the board's own address.
/// It waits `ROAD_PROBE_AFTER_MS` past boot, so it neither slows the page nor is slowed by it.
const ROAD_PROBE = "wfsim-road-probe", ROAD_PROBE_MS = 10_000, ROAD_PROBE_AFTER_MS = 15_000;
async function roadProbe() {
  if (!relayServes() || !LIVE_HOSTS.includes(location.hostname) || usageOff()
      || navigator.globalPrivacyControl || navigator.doNotTrack === "1") return;
  const day = new Date().toISOString().slice(0, 10);
  try { if (localStorage.getItem(ROAD_PROBE) === day) return; localStorage.setItem(ROAD_PROBE, day); } catch (_) { return; }
  for (const [road, base] of [["direct", BOARD_ORIGIN], ["relay", RELAY]]) {
    const t = performance.now();
    let ok = false;
    try {
      const r = await fetch(`${base}/board.meta.json`, { cache: "no-store",
        signal: timeoutSignal(ROAD_PROBE_MS) });
      ok = r.ok && !!(await r.json());
    } catch (_) { /* a failed road is the point */ }
    track("road.probe", `${road}_${ok ? "ok" : "failed"}`, Math.round(performance.now() - t));
  }
}

/// WHAT THIS PAGE HOLDS, given back on every way out that is not an answer —
/// and on the page's own exit, by beacon — so neither the work nor this computer
/// waits out a lease nobody is working (worker/verify.js `release`). A riven gain
/// is held alone (`heldLease`); board rows come in a PACK (`heldPack`), whose
/// answers go back together (`sendAnswers`) every `ANSWER_EVERY_MS` and as it
/// ends, the last send giving back the rows it did not reach.
let heldLease = null, heldPack = null;
const releaseLease = (h, beacon) => {
  const path = h.code ? `/api/appraise/${encodeURIComponent(h.code)}/renew` : "/api/board/release";
  const body = { lease: h.lease, verifier: h.verifier, ...(h.code ? { release: true } : {}) };
  if (beacon && navigator.sendBeacon) navigator.sendBeacon(path, JSON.stringify(body));
  else postBoardWork(path, body);
};
const ANSWER_EVERY_MS = 30_000;
/// A PACK'S ANSWERS SO FAR, sent; `done` gives back the rest of it. Its rows
/// leave the "not sent" state in this browser's list either way: a send that
/// failed is the server's to say (`/api/board/mine`), and it says `gone`.
/// A SEND THAT GOT NO REPLY IS SENT AGAIN, a computed result being the one thing
/// a dropped connection must not cost: a reply lost after the answers landed is
/// harmless, the server taking each device's answer to a question once.
const SEND_AGAIN_MS = [3_000, 10_000, 30_000];
async function sendAgain(body) {
  for (let i = 0; ; i++) {
    const r = await postBoardWork("/api/board/answers", body);
    if (r && r.ok) return r;
    if (i >= SEND_AGAIN_MS.length) return null;
    await new Promise((res) => setTimeout(res, SEND_AGAIN_MS[i]));
  }
}
/// A SEND WITH RESULTS ASKS FOR THIS BROWSER'S POINTS IN THE SAME REPLY
/// (`points_since`); a worker that does not carry them is asked apart, as before.
function sendAnswers(p, done, beacon = false, keep = null) {
  const keys = p.unsent.map((a) => a.key);
  const body = { lease: p.lease, verifier: p.verifier, engine: ENGINE_ID, done, answers: p.unsent.map(({ key: _k, ...a }) => a),
    ...(keep ? { keep } : {}), ...(keys.length ? { points_since: pointsSince() } : {}) };
  p.unsent = [];
  p.sentAt = Date.now();
  if (beacon && navigator.sendBeacon) navigator.sendBeacon("/api/board/answers", JSON.stringify(body));
  const sent = beacon && navigator.sendBeacon ? Promise.resolve({ ok: true }) : sendAgain(body);
  return sent.then((r) => {
    computeSent(keys);
    if (r && keys.length) {
      if (r.device_points) { devicePointsAt = Date.now(); takeDevicePoints({ ok: true, ...r.device_points }); }
      else { devicePointsAt = 0; loadDevicePoints(); }
    }
    return !!r;
  });
}
async function endPack(keep = null) {
  const p = heldPack;
  heldPack = null;
  if (p) await sendAnswers(p, true, false, keep);
}
function giveBack(beacon = false) {
  const h = heldLease, p = heldPack;
  heldLease = null;
  heldPack = null;
  if (h) releaseLease(h, beacon);
  if (p && p.proto === 8) { giveBack8(p, beacon); return; }
  // …AND TURNED OFF OR HELD, THE KEPT RESULTS STILL GO IN, at the reply's pace.
  if (!p && boardProtocol === 8 && (packQueue.length || (resultsMem.length && (beacon || Date.now() >= syncNotBefore)))) {
    giveBack8(null, beacon);
    return;
  }
  if (p) sendAnswers(p, true, beacon, keptCut(p));
}

/// PROTOCOL 8 (docs/CLIENTS.md §"The cycle, protocol 8"): one door, `/api/board/sync`,
/// called when the work runs low, when a result has waited `flush_ms`, and on the
/// way out; an idle page reads the work signal instead (§"The work signal"). It
/// is spoken when this browser keeps results in IndexedDB and the server has not
/// said `fallback: 7`; otherwise the protocol 7 loop below runs unchanged.
/// `boardProtocol`: 0 not yet decided, then 8 or 7 for the rest of the page.
let boardProtocol = 0;
const SIGNAL_URL = "https://cdn.wfsim.app/work.json";
/// THE DIALS IN FORCE are the server's (`dials` in every reply, `work.json`);
/// these are its stated defaults, used only until the first reply says otherwise.
const syncDials = { pack8_ms: 600_000, sync_low_ms: 120_000, sync_lease_ms: 1_500_000, flush_ms: 600_000,
  poll_ms: 10_000, jitter_ms: 5_000, urgent_jitter_ms: 2_000, report_ms: 3_600_000 };
/// THE FALLBACK'S PACE and a signal's age past which it is not believed.
const SYNC_FALLBACK_MS = 120_000, SIGNAL_STALE_MS = 90_000;
const SYNC_BACKOFF_MS = 5_000, SYNC_BACKOFF_MAX_MS = 600_000;
/// A result kept longer than this, or another id's or engine's, is dropped.
const RESULT_KEEP_MS = 24 * 3_600_000, PENDING_KEEP_MS = 3 * 24 * 3_600_000, BEACON_BYTES = 60_000;
let syncNotBefore = 0, syncFailures = 0, lastSyncAt = 0, syncInFlight = null, bootSyncAt = 0;
let packQueue = [], refusedPending = [], lostRows = new Set();
const jittered = (ms) => ms * (0.75 + Math.random() * 0.5);

/// THE RESULTS THIS BROWSER KEEPS (`results`: computed, not yet taken) and the ones
/// the server took that no fact has confirmed yet (`pending`), in IndexedDB so a
/// reload, a closed tab or a crash costs no result — and mirrored here, so the
/// way out can send them in a beacon without waiting on the database.
const COMPUTE_DB = "wfsim-compute";
let computeDb = null, resultsMem = [], pendingMem = [];
function openResults() {
  if (!computeDb) computeDb = new Promise((ok) => {
    try {
      const rq = indexedDB.open(COMPUTE_DB, 1);
      rq.onupgradeneeded = () => {
        for (const s of ["results", "pending"]) if (!rq.result.objectStoreNames.contains(s)) rq.result.createObjectStore(s, { keyPath: "question" });
      };
      rq.onsuccess = () => ok(rq.result);
      rq.onerror = rq.onblocked = () => ok(null);
    } catch (_) { ok(null); }
  }).then(async (db) => {
    if (!db) return null;
    try { [resultsMem, pendingMem] = await Promise.all([idbAll(db, "results"), idbAll(db, "pending")]); } catch (_) { return null; }
    return db;
  });
  return computeDb;
}
const idbDone = (tx) => new Promise((ok, no) => { tx.oncomplete = () => ok(); tx.onerror = tx.onabort = () => no(tx.error); });
function idbAll(db, store) {
  return new Promise((ok, no) => {
    const rq = db.transaction(store).objectStore(store).getAll();
    rq.onsuccess = () => ok(rq.result || []);
    rq.onerror = () => no(rq.error);
  });
}
async function idbWrite(store, put = [], del = []) {
  const db = await openResults();
  if (!db || (!put.length && !del.length)) return;
  const tx = db.transaction(store, "readwrite"), os = tx.objectStore(store);
  for (const v of put) os.put(v);
  for (const k of del) os.delete(k);
  await idbDone(tx);
}
/// …THIS ID'S AND ENGINE'S, oldest first; the rest dropped where they are found.
async function keptResults(id) {
  const now = Date.now(), mine = (r) => r.verifier === id && r.engine === ENGINE_ID;
  const goneR = resultsMem.filter((r) => !mine(r) || now - r.at > RESULT_KEEP_MS).map((r) => r.question);
  const goneP = pendingMem.filter((r) => r.verifier !== id || now - r.at > PENDING_KEEP_MS).map((r) => r.question);
  if (goneR.length) { resultsMem = resultsMem.filter((r) => !goneR.includes(r.question)); await idbWrite("results", [], goneR).catch(() => {}); }
  if (goneP.length) { pendingMem = pendingMem.filter((r) => !goneP.includes(r.question)); await idbWrite("pending", [], goneP).catch(() => {}); }
  return resultsMem.slice().sort((a, b) => a.at - b.at);
}
/// POINTS NOT YET CONFIRMED: every kept or taken result's work, as points.
function pendingPoints() { return Math.floor([...resultsMem, ...pendingMem].reduce((t, r) => t + (Number(r.work) || 0), 0) / 1e9); }

/// EVERY ROW THIS PAGE HOLDS and has not answered, by lease — named in full in
/// every sync, since a live lease a sync leaves out is given back.
function heldRows() {
  const out = new Map();
  for (const p of [heldPack, ...packQueue]) {
    if (!p || p.proto !== 8) continue;
    const rows = p.tasks.filter((t) => !p.done.has(t.question) && !lostRows.has(t.question)).map((t) => t.question);
    if (rows.length) out.set(p.lease, [...(out.get(p.lease) || []), ...rows]);
  }
  return [...out].map(([lease, rows]) => ({ lease, rows }));
}
/// …AND THE WORK LEFT IN THEM, at this computer's pace.
function workLeftMs() {
  const rows = heldRows().reduce((t, h) => t + h.rows.length, 0);
  return rows * (boardPaceMs || 60_000);
}
function boardSyncBody(id, { want, keep = null, results }) {
  const c = computeConsent(), idle = idleLoad();
  return { verifier: id, engine: ENGINE_ID, protocol: 8, ...(c ? { consent: { v: c.v, at: c.at } } : {}),
    lanes: communityLanes(), ...(boardPaceMs ? { pace_ms: Math.min(syncDials.sync_lease_ms, Math.max(100, Math.round(boardPaceMs))) } : {}),
    want_ms: c && c.on ? Math.max(0, Math.round(want)) : 0, held: heldRows(),
    results: results.map(({ verifier: _v, engine: _e, at: _a, key: _k, ...r }) => r),
    ...(keep ? { keep } : {}), ...(refusedPending.length ? { refused: refusedPending.slice(0, 50) } : {}),
    ...(Object.keys(idle).length ? { idle: Object.entries(idle).map(([hour, x]) => ({ hour, ...x })) } : {}),
    states: pendingMem.slice().sort((a, b) => a.at - b.at).slice(0, 100).map((r) => r.question), since: pointsSince() };
}

/// ONE SYNC, its reply heard: results the server took leave the queue for the
/// pending list, a new pack waits its turn, and `next_ms` is when this browser
/// may next be handed work. `want` null: what the held work leaves of a pack.
async function boardSync(id, { want = null } = {}) {
  if (syncInFlight) return syncInFlight;
  syncInFlight = (async () => {
    const results = (await keptResults(id)).slice(0, 400);
    const early = Date.now() < syncNotBefore;
    const ask = early ? 0 : want == null ? Math.max(0, syncDials.pack8_ms - workLeftMs()) : want;
    const body = boardSyncBody(id, { want: ask, results }), sentAt = Date.now(), idle = body.idle ? idleLoad() : {};
    const refusedSent = (body.refused || []).map((r) => r.question);
    const r = await postBoardWork("/api/board/sync", body);
    lastSyncAt = Date.now();
    if (!r || !r.ok) {
      syncFailures += 1;
      const cap = Math.min(SYNC_BACKOFF_MAX_MS, SYNC_BACKOFF_MS * 2 ** (syncFailures - 1));
      syncNotBefore = Date.now() + Math.random() * cap;
      idleCount(sentAt, false, true, communityLanes(), roadDropped);
      return null;
    }
    if (r.fallback === 7) { boardProtocol = 7; return r; }
    syncFailures = 0;
    if (r.dials) Object.assign(syncDials, r.dials);
    if (r.kept) idleKept(idle);
    refusedPending = refusedPending.filter((x) => !refusedSent.includes(x.question));
    await takeSynced(r, results, sentAt, body.held);
    if (Number.isFinite(r.next_ms)) syncNotBefore = Date.now() + r.next_ms;
    noteHandOut(r);
    if (r.pack && r.pack.tasks && r.pack.tasks.length) {
      packQueue.push({ proto: 8, lease: r.pack.lease, verifier: id, tasks: r.pack.tasks, next: 0, done: new Set(),
        until: sentAt + r.pack.lease_ms });
    }
    return r;
  })().finally(() => { syncInFlight = null; });
  return syncInFlight;
}
/// …WHAT THE REPLY SAYS OF EACH RESULT, ROW AND QUESTION ASKED AFTER.
async function takeSynced(r, sent, sentAt, held) {
  const byQ = new Map(sent.map((x) => [x.question, x]));
  const taken = (r.taken || []).filter((q) => byQ.has(q)), dropped = (r.dropped || []).map((d) => d.question).filter((q) => byQ.has(q));
  const gone = new Set([...taken, ...dropped]);
  if (gone.size) {
    resultsMem = resultsMem.filter((x) => !gone.has(x.question));
    const moved = taken.map((q) => { const x = byQ.get(q); return { question: q, verifier: x.verifier, engine: x.engine, work: x.work, at: Date.now() }; });
    pendingMem = [...pendingMem.filter((x) => !gone.has(x.question)), ...moved];
    await idbWrite("results", [], [...gone]).catch(() => {});
    await idbWrite("pending", moved, []).catch(() => {});
    computeSent([...gone].map((q) => byQ.get(q).key).filter(Boolean));
  }
  for (const h of r.held || []) {
    if (h.held === false) { lostRows.add(h.question); if (heldPack && heldPack.row === h.question) heldPack.lost = h.question; }
  }
  // RENEWED: every pack this sync named runs `sync_lease_ms` from when it was sent.
  const named = new Set(held.map((h) => h.lease));
  for (const p of [heldPack, ...packQueue]) if (p && p.proto === 8 && named.has(p.lease)) p.until = sentAt + syncDials.sync_lease_ms;
  const settled = (r.states || []).filter((s) => s.state === "confirmed" || s.state === "gone").map((s) => s.question);
  if (settled.length) {
    pendingMem = pendingMem.filter((x) => !settled.includes(x.question));
    await idbWrite("pending", [], settled).catch(() => {});
  }
  if (r.points) { devicePointsAt = Date.now(); await takeDevicePoints({ ok: true, ...r.points }); }
  else if (gone.size) renderBoardConsent();
}
/// A REPLY'S WORD ON THE ENGINE, THE RELEASE AND THIS BROWSER — protocol 7's too.
function noteHandOut(ask) {
  if (ask.stale && !boardStale) { boardStale = true; renderBoardConsent(); }
  if (ask.release && RELEASE_ID !== "dev" && ask.release !== RELEASE_ID) releaseNewer = true;
  if (!!ask.banned !== boardBanned || (ask.until || 0) !== boardBannedUntil) {
    boardBanned = !!ask.banned; boardBannedUntil = ask.until || 0; computeRedraw(); computeChrome();
  }
}

/// THE WAY OUT: one sync naming nothing held — so every lease goes back — but the
/// row to resume (`keep`), carrying the kept results, by beacon on a page that is
/// going. The results stay kept until a later reply lists them.
function giveBack8(p, beacon) {
  const id = verifierId();
  const keep = p && keptCut(p) ? { lease: p.lease, question: p.row } : null;
  heldPack = null;
  packQueue = [];
  if (!id) return;
  const results = [];
  let bytes = 0;
  for (const r of resultsMem.filter((x) => x.verifier === id && x.engine === ENGINE_ID).sort((a, b) => a.at - b.at)) {
    bytes += JSON.stringify(r).length;
    if (bytes > BEACON_BYTES || results.length >= 400) break;
    results.push(r);
  }
  const body = boardSyncBody(id, { want: 0, keep, results });
  body.held = [];
  if (beacon && navigator.sendBeacon) { navigator.sendBeacon("/api/board/sync", JSON.stringify(body)); return; }
  syncNotBefore = Math.max(syncNotBefore, Date.now() + SYNC_FALLBACK_MS);
  postBoardWork("/api/board/sync", body).then((r) => {
    if (!r || !r.ok || r.fallback) return;
    if (Number.isFinite(r.next_ms)) syncNotBefore = Date.now() + r.next_ms;
    return takeSynced(r, results, Date.now(), []);
  });
}

/// THE WORK SIGNAL (`work.json`), read by an idle page instead of asking: the
/// edge answers it, so waiting costs the server nothing. Unreadable, older than
/// `SIGNAL_STALE_MS` by the server's clock, or `mode: "ask"`, it is not used.
async function readSignal() {
  if (serverSkewMs == null) {
    await fetch("/release.json", { method: "HEAD", cache: "no-store" }).then(learnServerClock).catch(() => {});
  }
  try {
    const r = await fetch(SIGNAL_URL, { cache: "no-cache", signal: boardSignal() });
    const s = r.ok ? await r.json() : null;
    if (!s || s.v !== 1 || serverNow() - Date.parse(s.at) > SIGNAL_STALE_MS || s.mode === "ask") return null;
    for (const k of ["poll_ms", "jitter_ms", "urgent_jitter_ms"]) if (Number.isFinite(s[k])) syncDials[k] = s[k];
    // A NEWER ENGINE OR RELEASE is followed as `/release.json` would be.
    if (RELEASE_ID !== "dev" && ((s.release && s.release !== RELEASE_ID) || (s.engine && s.engine !== ENGINE_ID))) releaseNewer = true;
    return s;
  } catch (_) { return null; }
}
/// HOW LONG THE LOOP WAITS between looks while nothing is held.
const idleWaitMs = () => (boardProtocol === 8 ? jittered(syncDials.poll_ms) : ASK_EVERY_MS);

/// ONE STEP OF PROTOCOL 8 — `true` when a row or a riven gain ran. A row comes
/// from the pack held or the next one queued; with none, the kept results go in
/// when due, and work is asked for only when the signal says there is some (or,
/// with no signal, every `SYNC_FALLBACK_MS`), and never before `next_ms`.
async function workOnce8(id) {
  const kept = await keptResults(id);
  const now = Date.now();
  if (!bootSyncAt) bootSyncAt = now + (kept.length ? Math.random() * 5_000 : 0);
  const cut = !heldPack && loadCut(id);
  if (cut) heldPack = { proto: 8, lease: cut.lease, verifier: id, tasks: [cut.task], next: 0, done: new Set(), until: 0, from: cut.cut };
  if (heldPack && heldPack.next >= heldPack.tasks.length) heldPack = null;
  if (!heldPack && packQueue.length) heldPack = packQueue.shift();
  // A ROW TO RESUME IS RENEWED BEFORE IT RUNS: the sync names it held.
  if (heldPack && cut) { await boardSync(id, { want: 0 }); if (!heldPack || lostRows.has(cut.task.question)) { dropCut(); return true; } }
  if (heldPack) return packRowOnce(id, heldPack);
  const due = kept.length && (now >= bootSyncAt && lastSyncAt < bootSyncAt || now - kept[0].at >= syncDials.flush_ms);
  let r = null;
  if (now >= syncNotBefore) {
    const s = await readSignal();
    if (s) {
      const wait = s.urgent > 0 ? syncDials.urgent_jitter_ms : s.open > 0 ? syncDials.jitter_ms : -1;
      if (wait >= 0) { await new Promise((ok) => setTimeout(ok, Math.random() * wait)); r = await boardSync(id); }
      else if (due || (syncDials.report_ms > 0 && now - lastSyncAt >= syncDials.report_ms)) r = await boardSync(id, { want: 0 });
      else idleCount(now, true, false, communityLanes(), "");
    } else if (due || now - lastSyncAt >= jittered(SYNC_FALLBACK_MS)) r = await boardSync(id);
    else idleCount(now, true, false, communityLanes(), "");
  } else if (due) r = await boardSync(id, { want: 0 });
  maybeReloadForRelease();
  if (boardProtocol !== 8) return false;
  if (r && r.work && r.work.kind === "riven_gain") return rivenGainOnce(r.work, id);
  if (!heldPack && packQueue.length) { heldPack = packQueue.shift(); return packRowOnce(id, heldPack); }
  return false;
}
/// …DURING A PACK: work asked for as it runs low (`sync_low_ms` left), results
/// sent once the oldest has waited `flush_ms` — never more than one sync at a time.
function refill8(id) {
  if (syncInFlight) return;
  const now = Date.now(), oldest = resultsMem.length ? Math.min(...resultsMem.map((r) => r.at)) : now;
  const low = !packQueue.length && workLeftMs() < syncDials.sync_low_ms && now >= syncNotBefore;
  if (low || now - oldest >= syncDials.flush_ms) boardSync(id, low ? {} : { want: 0 });
}
/// …AND A ROW STILL RUNNING AS ITS LEASE NEARS ITS END is renewed by a sync that
/// asks for nothing, the row named held.
function keepRow8(p, t) {
  if (lostRows.has(t.question)) { p.lost = t.question; return Promise.resolve(); }
  if (!p.until || Date.now() < p.until - RENEW_BEFORE_MS || syncInFlight) return Promise.resolve();
  return boardSync(p.verifier, { want: 0 }).then(() => { if (lostRows.has(t.question)) p.lost = t.question; });
}

/// A ROW CUT SHORT RESUMES — a reload, a closed tab, a crash, a hold — from its
/// fold as it stood, kept here with the row's lease and the engine that folded
/// it, the row kept back from what goes back (`keep`). The next ask renews it and
/// folds on from run `folded` in run order, so the bits are the uncut row's; a
/// row taken since, or another engine's fold, is dropped. Turning computing off
/// drops it. Saved every `RESUME_SAVE_MS` and on the way out.
const RESUME_KEY = "wfsim-board-resume";
const RESUME_SAVE_MS = 15_000;
let resumeCut = null, resumeSavedAt = 0;
function saveCut() {
  resumeSavedAt = Date.now();
  try { localStorage.setItem(RESUME_KEY, JSON.stringify(resumeCut)); } catch (_) { /* this page only */ }
}
function dropCut() {
  resumeCut = null;
  try { localStorage.removeItem(RESUME_KEY); } catch (_) { /* nothing kept */ }
}
function loadCut(id) {
  let c = null;
  try { c = JSON.parse(localStorage.getItem(RESUME_KEY) || "null"); } catch (_) { /* none */ }
  if (c && c.engine === ENGINE_ID && c.verifier === id && c.lease && c.task && c.cut && c.cut.folded > 0) return c;
  if (c) dropCut();
  return null;
}
/// …THE ROW A PACK GIVEN BACK KEEPS, its fold saved: the one it was computing, if any of it folded.
function keptCut(p) {
  const c = resumeCut;
  if (!c || c.lease !== p.lease || c.task.question !== p.row || p.lost === p.row) return null;
  saveCut();
  return c.task.question;
}
addEventListener("pagehide", () => { giveBack(true); giveBackAhead(true); });
/// A PAGE LEFT OPEN COMPUTES, in front or not: a hidden page's timers are slowed
/// by the browser and its workers are not, which is why a lane's rest is kept in
/// the worker. Only a page the browser FREEZES computes nothing until it wakes,
/// so what it holds goes back at once — a task someone waits on never sleeps on
/// a sleeping device for a lease's length — and woken, it asks for work at once
/// (`computeWake`), the task it was on dropped with its lease. That the browser
/// put it to sleep, here or by discarding it, is said on /compute
/// (`computeSlept`), with how to keep the site awake.
let computeFrozen = false, computeSlept = !!document.wasDiscarded, computeWake = null;
document.addEventListener("freeze", () => { computeFrozen = true; giveBack(true); giveBackAhead(true); });
document.addEventListener("resume", () => { computeSlept = true; if (computeWake) computeWake(); computeRedraw(); });

/// THE NEXT WORK, ASKED BEFORE THIS ENDS (worker/verify.js, asking ahead): once
/// what is left would take under `REFILL_MS` — the pack's rows left at this
/// computer's pace, or `AHEAD_AT` of a search past the last one's time — so the
/// next starts the moment this ends and no core waits on a round trip. Once per
/// pack or search; the server holds a computer to `PACKS_HELD`.
const AHEAD_AT = 0.8;
const REFILL_MS = 15_000;
/// A ROW'S WALL TIME HERE, rests included, smoothed — the `pace_ms` a pack is sized by.
let boardPaceMs = 0;
let aheadAsk = null, aheadHeld = null;
function askAhead(id) {
  if (boardProtocol === 8) { refill8(id); return; }
  if (aheadAsk || !boardVerifyOn() || computeHeld()) return;
  aheadAsk = askWork(id, true).then((r) => {
    aheadHeld = r && r.pack ? { lease: r.pack.lease, verifier: id }
      : r && r.work ? { code: r.work.kind === "riven_gain" ? r.work.code : null, lease: r.work.lease, verifier: id } : null;
    return r;
  });
}
/// …GIVEN BACK on every way out that does not start it.
function giveBackAhead(beacon = false) {
  const h = aheadHeld;
  aheadAsk = null;
  aheadHeld = null;
  if (h) releaseLease(h, beacon);
}
/// THE ASKS THAT BROUGHT NOTHING, by the UTC hour — `{ hour: { asks, failed,
/// lanes } }`, kept across reloads and sent with every ask until a reply says
/// `kept` (worker/verify.js `asksOf`): an outage's compensation is the cores it
/// left waiting, and the asks that never landed only this page can count.
const IDLE_KEY = "wfsim-compute-idle", IDLE_HOURS = 48;
function idleLoad() {
  try { return JSON.parse(localStorage.getItem(IDLE_KEY) || "{}") || {}; } catch (_) { return {}; }
}
function idleSave(m) {
  const hours = Object.keys(m).sort().slice(-IDLE_HOURS);
  try { localStorage.setItem(IDLE_KEY, JSON.stringify(Object.fromEntries(hours.map((h) => [h, m[h]])))); } catch (_) { /* this page only */ }
}
function idleCount(at, idle, failed, lanes, dropped) {
  const m = idleLoad(), h = new Date(at).toISOString().slice(0, 13);
  const x = { asks: 0, failed: 0, lanes: 0, relay_failed: 0, direct_failed: 0, ...m[h] };
  m[h] = { asks: x.asks + (idle ? 1 : 0), failed: x.failed + (failed ? 1 : 0), lanes: x.lanes + (idle ? lanes : 0),
    relay_failed: x.relay_failed + (dropped === "relay" ? 1 : 0), direct_failed: x.direct_failed + (dropped === "direct" ? 1 : 0) };
  idleSave(m);
}
/// …LESS WHAT A REPLY KEPT, and only that: an ask counted while it was in flight stays.
function idleKept(sent) {
  const m = idleLoad();
  for (const [h, x] of Object.entries(sent)) {
    const y = m[h];
    if (!y) continue;
    const left = { asks: y.asks - x.asks, failed: y.failed - x.failed, lanes: y.lanes - x.lanes,
      relay_failed: (y.relay_failed || 0) - (x.relay_failed || 0), direct_failed: (y.direct_failed || 0) - (x.direct_failed || 0) };
    if (left.asks > 0 || left.relay_failed > 0 || left.direct_failed > 0) m[h] = left; else delete m[h];
  }
  idleSave(m);
}

/// ONE ASK FOR WORK, its answer heard: a newer engine or release, a refusal.
async function askWork(id, ahead = false) {
  const c = computeConsent();
  if (!c) return null;
  const at = Date.now(), lanes = communityLanes();
  const idle = ahead ? {} : idleLoad();
  const sent = await postBoardWork("/api/board/work", { verifier: id, engine: ENGINE_ID, protocol: 7, consent: { v: c.v, at: c.at },
    lanes, ...(boardPaceMs ? { pace_ms: Math.round(boardPaceMs) } : {}), ...(ahead ? { ahead: true } : {}),
    ...(Object.keys(idle).length ? { idle: Object.entries(idle).map(([hour, x]) => ({ hour, ...x })) } : {}) });
  const ask = sent, dropped = roadDropped;
  if (ask && ask.kept) idleKept(idle);
  const idleNow = !ahead && !(ask && (ask.pack || ask.work));
  if (idleNow || dropped) idleCount(at, idleNow, !ask, lanes, dropped);
  if (ask && ask.stale && !boardStale) { boardStale = true; renderBoardConsent(); }
  if (ask && ask.release && RELEASE_ID !== "dev" && ask.release !== RELEASE_ID) releaseNewer = true;
  if (ask && (!!ask.banned !== boardBanned || (ask.until || 0) !== boardBannedUntil)) {
    boardBanned = !!ask.banned; boardBannedUntil = ask.until || 0; computeRedraw(); computeChrome();
  }
  return ask && { ...ask, asked_at: at };
}
/// A PACK'S LEASE IS A HEARTBEAT (worker/tasks.js `renewRow`): the row being
/// computed is renewed once its lease is this close to its end — before it
/// starts, and as its pieces come in — so a page still computing is never cut
/// off and one that went silent loses only that row. Left minutes before the
/// end, so a slow round trip or a few missed beats lose nothing.
const RENEW_BEFORE_MS = 2 * 60_000;
/// …AND A RENEWAL THAT GOT NO ANSWER is tried again this much later: a dropped
/// connection is not a no, and the row keeps computing meanwhile.
const RENEW_RETRY_MS = 30_000;
function keepRow(p, t) {
  if (p.renewing) return p.renewing.then(() => (p.renewedFor === t.question ? null : keepRow(p, t)));
  if (Date.now() < Math.max(p.rowUntil - RENEW_BEFORE_MS, p.retryAt || 0)) return Promise.resolve();
  const sent = Date.now();
  p.renewedFor = t.question;
  p.renewing = postBoardWork("/api/board/renew", { lease: p.lease, verifier: p.verifier, question: t.question }).then((r) => {
    p.renewing = null;
    if (r && r.ok && r.held === false) p.lost = t.question;
    else if (p.row !== t.question) return;
    else if (r && r.ok) p.rowUntil = sent + r.renew_ms;
    else p.retryAt = Date.now() + RENEW_RETRY_MS;
  });
  return p.renewing;
}

/// A NEWER ENGINE IS OUT (`stale`): this page is given no more work, so waiting
/// helps nobody and it reloads itself — the build kept as a language switch
/// keeps it — as soon as nothing the reader started is running. A newer release
/// on the same engine still works, and waits until nobody has touched the page
/// for `IDLE_RELOAD_MS`. Not in the desktop shell, whose updater swaps the files.
const IDLE_RELOAD_MS = 10 * 60_000;
let boardStale = false;
/// …OR ONLY A NEWER RELEASE, the engine unchanged: this page still works, and
/// reloads into it on the same terms, so a fix reaches a machine left alone.
let releaseNewer = false;
/// THE SERVER REFUSED THIS BROWSER (worker/verify.js `admit`): its results
/// disagreed with the server's own, so it is handed nothing, and the page says so.
let boardBanned = false;
/// …and until when, the refusal being a cool-down (worker/verify.js `admit`).
let boardBannedUntil = 0;
let lastTouched = Date.now();
for (const ev of ["pointerdown", "keydown", "wheel", "touchstart"]) {
  addEventListener(ev, () => { lastTouched = Date.now(); }, { passive: true, capture: true });
}
/// THE READER IS COMPUTING — anything, here or in another WFSim tab. The calls on
/// the pool are counted where they run (`readerInFlight`); the jobs that live
/// between calls, or on workers of their own, are named — each by whether it is
/// still WORKING (`scanIsLive`, `shapleyIsLive`), never by its `running` flag,
/// which an exit nobody thought of latches: a latched flag held this tab and,
/// through the channel below, every other one off the community's work for good.
function ownTabBusy() {
  return foregroundHeld > 0 || readerInFlight > 0 || optIsLive() || scanIsLive() || shapleyIsLive();
}
/// …AND ANOTHER TAB SAYS SO every second while it is, each word good for two:
/// a Run in one tab holds the community's work in all of them.
const READER_CHANNEL = (() => { try { return new BroadcastChannel("wfsim-reader-busy"); } catch (_) { return null; } })();
let otherTabBusyUntil = 0;
if (READER_CHANNEL) {
  READER_CHANNEL.onmessage = (e) => {
    const until = Number(e.data && e.data.until);
    if (Number.isFinite(until)) otherTabBusyUntil = Math.max(otherTabBusyUntil, Math.min(until, Date.now() + 2000));
  };
  setInterval(() => { if (ownTabBusy()) READER_CHANNEL.postMessage({ until: Date.now() + 2000 }); }, 1000);
}
function readerBusy() {
  return ownTabBusy() || Date.now() < otherTabBusyUntil;
}
/// `how` is what the next page's `app.boot` says it arrived by (`usageArrival`):
/// `idle` when the page reloaded itself, `asked` when the reader clicked.
function reloadForRelease(how = "asked") {
  try {
    sessionStorage.setItem("wfsim-lang-stash", JSON.stringify(snapshotState()));
    sessionStorage.setItem(USAGE_ARRIVAL, `release_${how}`);
  } catch (_) { /* nothing to keep */ }
  forgetKeptPages().finally(() => location.reload());
}
function maybeReloadForRelease() {
  if (!boardStale && !releaseNewer) return;
  // THE DESKTOP CLIENT'S UPDATER SWAPS THE FILES (82-desktop.js), told to look now.
  if (window.__WFSIM_DESKTOP__) { desktopFollowRelease(); return; }
  if (readerBusy()) return;
  if (!boardStale && !releaseLeftBehind && Date.now() - lastTouched < IDLE_RELOAD_MS) return;
  // ONE TRY AN HOUR: a CDN still serving the old files must not make a loop.
  try {
    if (Date.now() - Number(sessionStorage.getItem("wfsim-release-reload") || 0) < 3_600_000) return;
    sessionStorage.setItem("wfsim-release-reload", String(Date.now()));
  } catch (_) { return; }
  reloadForRelease("idle");
}

/// A LANE THAT WILL NOT LOAD MAY BE A PAGE LEFT BEHIND: a release keeps only
/// the previous generation's files, so a page two releases old asks for a
/// worker the site no longer serves, and nothing it computes can finish. The
/// release is asked at most once a minute, since a dropped network fails lanes
/// in bursts; a newer one is said on the page and reloaded into at once, since
/// a page that cannot load its worker computes nothing by waiting.
/// THE ANSWER NAMES THE FAILURE in `engine.fail`: `worker_load_stale` when a newer
/// release left the page behind, `worker_load_offline` when the site itself did
/// not answer, else `worker_load_<cause>` where the worker said its `LOAD_CAUSES`
/// one, and `worker_load` where it said nothing. `cause` is read after a wait:
/// the worker's word and its error event arrive by different task sources.
let releaseAskedAt = 0, releaseLeftBehind = false;
async function releaseAfterLaneFailed(ms, cause) {
  const said = () => (releaseLeftBehind ? "worker_load_stale"
    : cause() ? `worker_load_${cause()}` : "worker_load");
  if (!WASM || RELEASE_ID === "dev" || window.__WFSIM_DESKTOP__ || Date.now() - releaseAskedAt < 60_000) {
    await new Promise((res) => setTimeout(res, 0));
    track("engine.fail", said(), ms);
    return;
  }
  releaseAskedAt = Date.now();
  let j = null;
  try {
    j = await fetch("/release.json", { cache: "no-store" }).then((r) => (r.ok ? r.json() : null));
  } catch (_) {
    track("engine.fail", "worker_load_offline", ms);
    return;
  }
  const newer = !!j && typeof j.release === "string" && j.release !== RELEASE_ID;
  releaseLeftBehind = newer;
  track("engine.fail", said(), ms);
  if (!newer || releaseNewer) return;
  releaseNewer = true;
  releaseNotice();
  setInterval(maybeReloadForRelease, 60_000);
}
/// SAID AT THE TOP OF WHATEVER PAGE IS OPEN, with the reload one click away.
function releaseNotice() {
  if (!document.body || $("release-note")) return;
  const el = document.createElement("div");
  el.id = "release-note";
  el.className = "page-note rem-note";
  el.innerHTML = `${escHtml(tr("A new version is out; this page refreshes itself once what you started ends."))}<a href="#">${escHtml(tr("refresh now"))}</a>`;
  el.addEventListener("click", (e) => {
    if (e.target.closest("a")) { e.preventDefault(); reloadForRelease(); } else el.remove();
  });
  document.body.appendChild(el);
}

/// A RIVEN GAIN, run here for someone waiting in a chat (worker/appraise.js
/// §"Volunteer work"): the frozen request through a search of its own
/// (`quickFleet`, one worker, the reader's optimizer untouched), paced so the
/// reader goes first, and its winner sent back under the lease — as the build
/// the page that froze it would have sent — with the search's work. Turning
/// computing off stops it, and the lease lapses to another computer.
const RIVEN_RENEW_MS = 2 * 60_000;
let rivenTookMs = 0;
async function rivenGainOnce(w, id) {
  heldLease = { code: w.code, lease: w.lease, verifier: id };
  computeStart({ kind: "riven_gain", code: w.code, weapon: w.weapon, ruler: w.ruler });
  const began = performance.now();
  // THE CORES IT MAY TAKE ARE ASKED EVERY ROUND, and its workers kept for the next one (`quickFleet`).
  const job = quickFleet(w.request, () => communityLanes(), undefined, { keep: true, rest: communityRest });
  // STILL AT IT, said every `RIVEN_RENEW_MS` so the lease runs on while the
  // search does; told the task went elsewhere, it stops (worker/appraise.js `renew`).
  let lost = false, said = performance.now();
  while (!job.result) {
    if (performance.now() - said > RIVEN_RENEW_MS) {
      said = performance.now();
      const r = await postBoardWork(`/api/appraise/${encodeURIComponent(w.code)}/renew`, { lease: w.lease, verifier: id });
      if (r && r.held === false) { lost = true; heldLease = null; }
    }
    // …AND IT STOPS WHEN COMPUTING IS TURNED OFF OR HELD, the lease handed back.
    if (computeFrozen) { lost = true; heldLease = null; }
    if (lost || !boardVerifyOn() || computeHeld()) {
      job.cancelled = true;
      job.workers.forEach((x) => x.terminate());
      giveBack();
      computeEnd(null);
      return true;
    }
    const s = job.status || {};
    computeBeats += 1;
    computeProgress(s.sims_done || 0, 0, s);
    // A SEARCH'S LENGTH IS NOT KNOWN AHEAD, so the last one's stands for it.
    if (rivenTookMs && performance.now() - began > AHEAD_AT * rivenTookMs) askAhead(id);
    await new Promise((r) => setTimeout(r, 1000));
  }
  rivenTookMs = performance.now() - began;
  const r = job.result;
  const best = r && r.ok !== false && (r.results || []).find((x) => x && (x.mods || []).length);
  if (!best) { giveBack(); computeEnd(null); return true; }
  const sent = await fetch(`/api/appraise/${encodeURIComponent(w.code)}/result`, {
    method: "POST",
    signal: boardSignal(),
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ build: boardPayloadFromResult(best, w.context), lease: w.lease, verifier: id, engine: ENGINE_ID,
      score: Number(best.kill_progress) || 0, work: r.work || 0,
      // WHAT THE SEARCH COST, kept beside the answer to fit what a point is worth (never credited).
      compute_ms: r.cpu_ms, lanes: communityLanes(), counts: { builds: r.builds || 0, fights: r.fights || 0, steps: r.steps || 0 } }),
  }).then((x) => x.ok).catch(() => false);
  if (sent) { heldLease = null; computeFinished += 1; } else giveBack();
  // …AND WHAT IT FOUND, for this browser's own list: the build, its number, what the search took.
  // A search row carries `kill_progress`, never `score`, and the duration is the answer's.
  const v = kpm(best.kill_progress ?? best.kills, r.duration);
  computeEnd(sent ? { ms: Math.round(performance.now() - began), work: r.work || 0, ...(Number.isFinite(v) ? { score: v, metric: "kpm" } : {}),
    record: boardPayloadFromResult(best, w.context), search: { builds: (job.status || {}).enumerated || 0, fights: r.fights || 0 } } : null);
  return true;
}

/// ONE ROW, fought here — `true` when there was one. A row comes from the pack
/// this page holds, or a new pack is asked for (the one asked ahead first); a
/// question that goes alone (a riven gain) runs as itself. The answer never
/// says whether it agreed; whatever the pack did not reach is given back.
async function workOnce() {
  computeFrozen = false;
  if (!boardVerifyOn()) dropCut();
  if (!boardVerifyOn() || computeHeld()) { giveBack(); giveBackAhead(); return false; }
  const id = verifierId();
  if (!id) return false;
  await claimDevice(id);
  if (!boardProtocol) boardProtocol = (await openResults()) ? 8 : 7;
  if (boardProtocol === 8) return workOnce8(id);
  const cut = !heldPack && loadCut(id);
  if (cut) {
    // RENEWED BEFORE ANYTHING IS ASKED: an ask gives back what this computer holds.
    heldPack = { lease: cut.lease, verifier: id, tasks: [cut.task], next: 0, unsent: [], sentAt: Date.now(), until: 0, from: cut.cut };
  } else if (!heldPack || heldPack.next >= heldPack.tasks.length) {
    await endPack();
    let ask = null;
    if (aheadAsk) { ask = await aheadAsk; aheadAsk = null; aheadHeld = null; }
    if (!(ask && (ask.pack || ask.work))) ask = await askWork(id);
    maybeReloadForRelease();
    if (ask && ask.work && ask.work.kind === "riven_gain") return rivenGainOnce(ask.work, id);
    if (!(ask && ask.pack)) return false;
    heldPack = { lease: ask.pack.lease, verifier: id, tasks: ask.pack.tasks, next: 0, unsent: [], sentAt: Date.now(),
      until: ask.asked_at + ask.pack.lease_ms };
  }
  return packRowOnce(id, heldPack);
}
async function packRowOnce(id, p) {
  const t = p.tasks[p.next++];
  // A ROW NOT STARTED is held by the pack's lease alone, which may have run out
  // while the row before ran long: renewed first, and passed by if it was taken.
  p.row = t.question;
  p.rowUntil = p.until;
  const from = p.from;
  p.from = null;
  if (!from) dropCut();
  const p8 = p.proto === 8, keep = p8 ? keepRow8 : keepRow;
  await keep(p, t);
  if (p.lost === t.question || (p8 && lostRows.has(t.question))) { dropCut(); if (p8) p.done.add(t.question); return true; }
  const live = () => boardVerifyOn() && !computeHeld() && !computeFrozen && heldPack === p && p.lost !== t.question;
  const refill = p8 ? () => refill8(id) : () => { if ((p.tasks.length - p.next) * boardPaceMs <= REFILL_MS) askAhead(id); };
  const order = await api("/api/board/order", { record: t.record, ruler: t.ruler, mode: t.mode }, null, { community: true });
  // A ROW THIS ENGINE REFUSES is said so in the next sync (`refused`): given back,
  // never handed here again, and withdrawn once enough owners refused it.
  if (p8 && order && order.ok === false) {
    refusedPending.push({ lease: p.lease, question: t.question, step: "order" });
    p.done.add(t.question);
    return true;
  }
  if (!order || !order.ok) return true;
  computeStart({ kind: "board", weapon: t.record.weapon, ruler: t.ruler, mode: t.mode, identity: t.identity, record: t.record });
  const began = Date.now();
  // A BEAT WITHOUT PIECES TOO: a share lowered to nothing folds nothing for a while.
  const beat = setInterval(() => { if (live()) keep(p, t); }, RENEW_RETRY_MS);
  const s = await measureRow(order.request, t.ruler, live, (done, total, cut) => {
    // A FOLD LANDING AFTER THE ROW STOPPED is not this page's to keep or ask on.
    if (!live()) return;
    computeBeats += 1;
    keep(p, t);
    computeProgress(done, total);
    resumeCut = { engine: ENGINE_ID, verifier: id, lease: p.lease, task: t, cut };
    if (Date.now() - resumeSavedAt >= RESUME_SAVE_MS) saveCut();
    if (total && done >= AHEAD_AT * total) refill();
  }, from).finally(() => clearInterval(beat));
  if (!s) {
    computeEnd(null);
    // A ROW THE ENGINE REFUSED is left to the pack's end, and a row taken since
    // is passed by; a pack that may no longer run ends here — held, its row kept
    // to resume (`keptCut`), turned off, given back whole.
    if (live() || p.lost === t.question || !boardVerifyOn()) dropCut();
    if (p8) {
      if (p.lost === t.question) p.done.add(t.question);
      if (!live() && p.lost !== t.question && heldPack === p) giveBack8(p, false);
      return true;
    }
    if (!live() && p.lost !== t.question && heldPack === p) await endPack(keptCut(p));
    return true;
  }
  dropCut();
  const ms = Date.now() - began;
  boardPaceMs = boardPaceMs ? boardPaceMs * 0.7 + ms * 0.3 : ms;
  const answer = { question: t.question, score: s.score, metric: s.metric, work: s.work, compute_ms: s.compute_ms, counts: s.counts, lanes: s.lanes };
  if (p8) {
    // KEPT BEFORE ANYTHING ELSE HAPPENS: a result in IndexedDB survives the page.
    const kept = { ...answer, lease: p.lease, verifier: id, engine: ENGINE_ID, at: Date.now(), key: taskKey(t) };
    resultsMem = [...resultsMem.filter((x) => x.question !== t.question), kept];
    await idbWrite("results", [kept], []).catch(() => {});
    p.done.add(t.question);
  } else p.unsent.push({ key: taskKey(t), ...answer });
  computeFinished += 1;
  computeEnd({ ms, cpu_ms: s.compute_ms, work: s.work, score: s.score, metric: s.metric, unsent: true });
  try { localStorage.setItem(VERIFIED_KEY, String(boardVerifiedCount() + 1)); } catch (_) { /* private mode */ }
  renderBoardConsent();
  refill();
  if (p8) {
    if (p.next >= p.tasks.length && heldPack === p) heldPack = null;
    return true;
  }
  if (p.next >= p.tasks.length) await endPack();
  else if (Date.now() - p.sentAt >= ANSWER_EVERY_MS) await sendAnswers(p, false);
  return true;
}

/// DOES THIS BROWSER KEEP COMPUTING WITH THE PAGE OUT OF SIGHT — measured, not
/// assumed: many freeze or throttle a hidden tab, and that decides what leaving
/// a machine computing is worth. Judged when the page comes back after at least
/// `BACKGROUND_JUDGED_MS` hidden in the tab that holds the work, as
/// `compute.background`: `kept` (a task was finished), `frozen` (none, and the
/// browser froze the page), `stopped` (none, at most one beat — an ask, a folded
/// piece, a search's round — so it was held without a word), `idle` (it kept
/// beating and finished nothing); `n` minutes.
const BACKGROUND_JUDGED_MS = 10 * 60_000;
let computeHolder = false, computeBeats = 0, computeFinished = 0, computeFreezes = 0, hiddenFrom = null;
document.addEventListener("freeze", () => { computeFreezes += 1; });
function judgeBackground() {
  if (document.hidden) {
    hiddenFrom = computeHolder && boardVerifyOn() ? { at: Date.now(), beats: computeBeats, done: computeFinished, freezes: computeFreezes } : null;
    return;
  }
  const h = hiddenFrom;
  hiddenFrom = null;
  if (!h || Date.now() - h.at < BACKGROUND_JUDGED_MS) return;
  const how = computeFinished > h.done ? "kept" : computeFreezes > h.freezes ? "frozen"
    : computeBeats - h.beats <= 1 ? "stopped" : "idle";
  track("compute.background", how, Math.min(1440, Math.round((Date.now() - h.at) / 60_000)));
}

/// ONLY THE DEPLOYED SITE WORKS: the dev server has no orders to hand out.
if (WASM) {
  loadDevicePoints();
  // THE QUESTION ONCE THE PAGE HAS STARTED — its words come with the language's
  // strings, which arrive with the engine, so asked any sooner a slow network
  // reads it in English — and again on every route: a card page a bot
  // photographs must not carry it.
  (function askWhenReady() {
    if (window.__wfsimReady) computeChrome();
    else setTimeout(askWhenReady, 500);
  })();
  addEventListener("popstate", () => setTimeout(computeChrome, 0));
  // ONE TAB OF A BROWSER COMPUTES: its tabs share one id, and the server takes
  // back whatever an asking id still holds, so two tabs asking would take each
  // other's orders. The tab holding the lock works until it closes; the next waits.
  addEventListener("visibilitychange", judgeBackground);
  const computeLoop = async () => {
    computeHolder = true;
    for (;;) {
      let worked = false;
      computeBeats += 1;
      try { worked = await workOnce(); } catch (_) { giveBack(); /* the next ask tries again */ }
      if (!worked) await new Promise((r) => { computeWake = r; setTimeout(r, idleWaitMs()); });
      computeWake = null;
    }
  };
  if (navigator.locks && navigator.locks.request) navigator.locks.request("wfsim-community-compute", computeLoop);
  else computeLoop();
}

/// THE SWITCH, stated beside the board's own in the consent box.
function boardVerifyHtml() {
  const on = boardVerifyOn();
  const text = on
    ? tr("This device computes WFSim together with the volunteers, in the background ({n} so far).")
      .replace("{n}", String(boardVerifiedCount()))
    : tr("Your browser does not compute the board's scores.");
  const stale = on && boardStale
    ? ` <span class="board-state">${escHtml(tr("A new version is out; this page refreshes itself once what you started ends."))}</span>
      <button class="ghost-btn small" id="board-reload">${escHtml(tr("refresh now"))}</button>`
    : "";
  return ` <span class="board-state">${escHtml(text)}</span>${stale}` +
    ` <button class="ghost-btn small" id="board-verify-flip">${escHtml(on ? tr("stop computing") : tr("start computing"))}</button>` +
    (accountState.providers.length ? boardPointsHtml(on) : "");
}
/// WHAT IT EARNED, and where it counts: under the account that claimed this
/// browser, or — signed out — the one line saying signing in puts it there.
/// Signed in with points and never asked, the one question: show the name?
function boardPointsHtml(on) {
  const d = on && devicePoints && !devicePoints.claimed && devicePoints;
  const earned = d ? ` <span class="board-state">${escHtml(tr("This browser: {n} points.")
    .replace("{n}", d.points.toLocaleString(accountLocale())))}</span>` : "";
  // POINTS NOT YET CONFIRMED: the work of every result kept or taken that no fact holds yet.
  const pend = on && boardProtocol === 8 ? pendingPoints() : 0;
  const pending = pend > 0 ? ` <span class="board-state">${escHtml(tr("Pending verification: +{n} points.")
    .replace("{n}", pend.toLocaleString(accountLocale())))}</span>` : "";
  const join = on && !accountState.account
    ? ` <a href="/login?return=${encodeURIComponent("/contributors")}">${escHtml(tr("Sign in to count it under your name"))}</a> ·`
    : "";
  const mine = accountState.account && devicesState;
  const ask = on && mine && !mine.decided && mine.points > 0
    ? ` <span class="board-state">${escHtml(tr("You are on the contributors' ranking without your name. Show it?"))}</span>
      <button class="ghost-btn small" id="board-name-yes">${escHtml(tr("Show my name"))}</button>
      <button class="ghost-btn small" id="board-name-no">${escHtml(tr("Keep it anonymous"))}</button> ·`
    : "";
  return `${earned}${pending}${join}${ask} <a href="/compute">${escHtml(tr("Compute together"))}</a> · <a href="/contributors">${escHtml(tr("Contributors"))}</a>`;
}
function wireBoardVerify() {
  const b = $("board-verify-flip");
  if (b) b.onclick = () => setBoardVerify(!boardVerifyOn());
  const again = $("board-reload");
  if (again) again.onclick = reloadForRelease;
  for (const [id, named] of [["board-name-yes", true], ["board-name-no", false]]) {
    const el = $(id);
    if (el) el.onclick = async () => {
      const r = await accountCall("POST", "/api/account/contribution", { named });
      if (r && r.ok) { await loadDevices(); contributorsState = null; renderBoardConsent(); }
    };
  }
}
