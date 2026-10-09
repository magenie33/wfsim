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
/// THE READER GOES FIRST. Every piece waits on `yieldToReader` — ANY computing
/// of the reader's, in this tab or another — and is sized to about `PIECE_MS`,
/// so whatever the reader starts waits for one piece at most.
/// Not on a phone: minutes of background work on a battery is a cost the reader
/// never agreed to.
const PIECE_MS = 250;
const onPhone = () => !!(window.matchMedia && matchMedia("(pointer: coarse)").matches);

/// ONE ROW, MEASURED: its runs fought on the community's lanes, folded in run
/// order, then scored, with what the pieces took in ms summed over the lanes
/// (`compute_ms`) — the waits between them are the reader's. `null` when the
/// engine refused or `live()` went false between pieces.
///
/// EVERY LANE FIGHTS, ONE MERGES. A run depends on its index alone, so lanes
/// pull pieces off one cursor (`/api/board/runs`, a shard per run) and the
/// shards are folded as `pieces` strictly in run order — the scorer's merges in
/// the scorer's order, so the bits are the scorer's however the lanes raced.
/// A lane past `communityLanes()` — the reader came back — takes no new piece.
async function measureRow(request, ruler, live, onPiece = () => {}) {
  const runs = Number(request.runs) || 0;
  const ls = await lanes(communityLanes(true));
  let cursor = 0, est = null, failed = false, spent = 0, folded = 0;
  const ready = new Map();
  let acc = null;
  let merging = Promise.resolve();
  // ONE CALL ON A LANE, asked again on a fresh worker when a Stop or a lost
  // worker took it: the range is the same, so the answer is too.
  const ask = async (k, path, body) => {
    for (let tries = 0; tries < 3; tries++) {
      await yieldToReader(live);
      if (!live()) return null;
      const r = await laneAt(k).call(path, body, null, true);
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
    merging = merging.then(async () => {
      if (failed) return;
      const step = await ask(0, "/api/board/fold", { request, acc, pieces });
      if (step) { acc = step.acc; onPiece(Math.min(runs, folded), runs); } else failed = true;
    });
  };
  await Promise.all(ls.map(async (_, k) => {
    while (!failed && cursor < runs) {
      await yieldToReader(live);
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
    }
  }));
  await merging;
  if (failed || folded !== runs) return null;
  const began = performance.now();
  const s = await ask(0, "/api/board/score", { ruler, request, acc });
  spent += performance.now() - began;
  return s && Number.isFinite(s.score) ? { ...s, compute_ms: Math.round(spent) } : null;
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
const COMPUTE_CONSENT_V = 2;
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

/// HOW MUCH OF THE COMPUTER, when it is idle: a share of its cores the reader
/// picks in the compute menu (`COMMUNITY_SHARES`), 30% unless they do. While the
/// reader is using it — touched in the last minute with the page in view, or
/// running something of their own — it takes one core. The statement they
/// agreed to says both.
const COMMUNITY_SHARE_KEY = "wfsim-community-share";
const COMMUNITY_SHARES = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
const COMMUNITY_IDLE_MS = 60_000;
function communityShare() {
  try {
    const n = Number(localStorage.getItem(COMMUNITY_SHARE_KEY));
    return COMMUNITY_SHARES.includes(n) ? n : 30;
  } catch (_) { return 30; }
}
function setCommunityShare(pct) {
  try { localStorage.setItem(COMMUNITY_SHARE_KEY, String(pct)); } catch (_) { /* this page only */ }
}
/// ROUNDED UP, so every share is at least one core and 30% of eight is three.
/// `ceiling` asks for the lanes the share buys whether or not the reader is here.
function communityLanes(ceiling = false) {
  const idle = ceiling || ((document.hidden || Date.now() - lastTouched > COMMUNITY_IDLE_MS) && !readerBusy());
  return idle ? Math.max(1, Math.ceil((detectedCores().n * communityShare()) / 100)) : 1;
}

/// …AND EVEN WITH A YES, NOT NOW: paused for this tab by the reader, on a
/// battery, or with the browser's data saver on. `why` says which.
let computePaused = false;
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
  if (computePaused) return "paused";
  if (computeBattery && !computeBattery.charging) return "battery";
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
  const r = await accountCall("POST", "/api/account/devices/claim", { verifier: id, label: computeDeviceGuess() });
  if (r && r.ok) claimedFor = `${account}:${id}`;
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
  // …AND ITS OWN "TODAY", from this reader's midnight, as the UTC hour it falls in.
  const midnight = new Date(); midnight.setHours(0, 0, 0, 0);
  const r = await postBoardWork("/api/board/points", { verifier: id, since: midnight.toISOString().slice(0, 13) });
  if (!(r && r.ok)) return;
  devicePoints = r;
  // WHETHER THE RANKING MAY NAME THEM is asked here, where they see their points.
  if (accountState.account && !devicesState) await loadDevices();
  renderBoardConsent();
}

/// EVERY ASK HAS A CLOCK: one call that never answers — a laptop waking on a
/// dead connection — would otherwise hold the work loop for good.
const BOARD_ASK_MS = 30_000;
const boardSignal = () => (typeof AbortSignal !== "undefined" && AbortSignal.timeout ? AbortSignal.timeout(BOARD_ASK_MS) : undefined);
const postBoardWork = (path, body) => fetch(path, {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(body),
  signal: boardSignal(),
}).then((r) => (r.ok ? r.json() : null)).catch(() => null);

/// THE LEASE THIS PAGE HOLDS, given back on every way out that is not an
/// answer — and on the page's own exit, by beacon — so neither the order nor
/// this computer waits out a lease nobody is working (worker/verify.js
/// `release`). Every release reloads every computing page, which made that
/// wait everyone's at once.
let heldLease = null;
function giveBack(beacon = false) {
  const h = heldLease;
  heldLease = null;
  if (!h) return;
  const path = h.code ? `/api/appraise/${encodeURIComponent(h.code)}/renew` : "/api/board/release";
  const body = { lease: h.lease, verifier: h.verifier, ...(h.code ? { release: true } : {}) };
  if (beacon && navigator.sendBeacon) navigator.sendBeacon(path, JSON.stringify(body));
  else postBoardWork(path, body);
}
addEventListener("pagehide", () => giveBack(true));
/// A LEASE RUNS `LEASE_MS` (30 min) on the server; past this share of it the
/// fight stops and the lease is given back, since an answer after it is dropped.
const LEASE_SAFE_MS = 27 * 60_000;

/// A NEWER RELEASE IS OUT (`stale`): this page's engine is given no more work,
/// so a machine left computing would idle for ever. It reloads itself — the
/// build kept as a language switch keeps it — once nobody has touched it for
/// `IDLE_RELOAD_MS` and nothing the reader started is running; until then the
/// switch says so. Not in the desktop shell, whose updater swaps the files.
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
async function yieldToReader(live) {
  while (readerBusy() && (!live || live())) await new Promise((r) => setTimeout(r, 25));
}
function reloadForRelease() {
  try { sessionStorage.setItem("wfsim-lang-stash", JSON.stringify(snapshotState())); } catch (_) { /* nothing to keep */ }
  location.reload();
}
function maybeReloadForRelease() {
  if ((!boardStale && !releaseNewer) || window.__WFSIM_DESKTOP__ || readerBusy() || Date.now() - lastTouched < IDLE_RELOAD_MS) return;
  // ONE TRY AN HOUR: a CDN still serving the old files must not make a loop.
  try {
    if (Date.now() - Number(sessionStorage.getItem("wfsim-release-reload") || 0) < 3_600_000) return;
    sessionStorage.setItem("wfsim-release-reload", String(Date.now()));
  } catch (_) { return; }
  reloadForRelease();
}

/// A RIVEN GAIN, run here for someone waiting in a chat (worker/appraise.js
/// §"Volunteer work"): the frozen request through a search of its own
/// (`quickFleet`, one worker, the reader's optimizer untouched), paced so the
/// reader goes first, and its winner sent back under the lease — as the build
/// the page that froze it would have sent — with the search's work. Turning
/// computing off stops it, and the lease lapses to another computer.
const RIVEN_RENEW_MS = 2 * 60_000;
async function rivenGainOnce(w, id) {
  heldLease = { code: w.code, lease: w.lease, verifier: id };
  computeStart({ kind: "riven_gain", weapon: w.weapon, ruler: w.ruler });
  const began = performance.now();
  const job = quickFleet(w.request, communityLanes(), () => yieldToReader());
  // STILL AT IT, said every `RIVEN_RENEW_MS` so the lease runs on while the
  // search does; told the task went elsewhere, it stops (worker/appraise.js `renew`).
  let lost = false, said = performance.now();
  while (!job.result) {
    if (performance.now() - said > RIVEN_RENEW_MS) {
      said = performance.now();
      const r = await postBoardWork(`/api/appraise/${encodeURIComponent(w.code)}/renew`, { lease: w.lease, verifier: id });
      if (r && r.held === false) { lost = true; heldLease = null; }
    }
    // …AND IT STOPS THE MOMENT THE READER COMPUTES: its workers are its own, so
    // waiting between rounds would leave them on the reader's cores for a round.
    if (lost || !boardVerifyOn() || computeHeld() || readerBusy()) {
      job.cancelled = true;
      job.workers.forEach((x) => x.terminate());
      giveBack();
      computeEnd(null);
      return true;
    }
    const s = job.status || {};
    computeProgress(s.sims_done || 0, 0, s);
    await new Promise((r) => setTimeout(r, 1000));
  }
  const r = job.result;
  const best = r && r.ok !== false && (r.results || []).find((x) => x && (x.mods || []).length);
  if (!best) { giveBack(); computeEnd(null); return true; }
  const sent = await fetch(`/api/appraise/${encodeURIComponent(w.code)}/result`, {
    method: "POST",
    signal: boardSignal(),
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ build: boardPayloadFromResult(best, w.context), lease: w.lease, verifier: id, engine: ENGINE_ID,
      score: Number(best.kill_progress) || 0, work: r.work || 0 }),
  }).then((x) => x.ok).catch(() => false);
  if (sent) heldLease = null; else giveBack();
  // …AND WHAT IT FOUND, for this browser's own list: the build, its number, what the search took.
  const m = metricOf(), v = Number(metricValue(m, best));
  computeEnd(sent ? { ms: Math.round(performance.now() - began), work: r.work || 0, ...(Number.isFinite(v) ? { score: v, metric: m.id } : {}),
    record: boardPayloadFromResult(best, w.context), search: { builds: (job.status || {}).enumerated || 0, fights: r.fights || 0 } } : null);
  return true;
}

/// ONE ORDER, fought here and answered — `true` when there was one. The answer
/// never says whether it agreed; a lease it does not answer is given back.
async function workOnce() {
  // NO LEASE WHILE THE READER COMPUTES: one taken now would sit idle under it.
  if (!boardVerifyOn() || onPhone() || computeHeld() || readerBusy()) return false;
  const id = verifierId();
  if (!id) return false;
  await claimDevice(id);
  const c = computeConsent();
  const ask = await postBoardWork("/api/board/work",
    { verifier: id, engine: ENGINE_ID, protocol: 6, consent: { v: c.v, at: c.at }, lanes: communityLanes() });
  if (ask && ask.stale && !boardStale) { boardStale = true; renderBoardConsent(); }
  if (ask && ask.release && RELEASE_ID !== "dev" && ask.release !== RELEASE_ID) releaseNewer = true;
  if (ask && (!!ask.banned !== boardBanned || (ask.until || 0) !== boardBannedUntil)) {
    boardBanned = !!ask.banned; boardBannedUntil = ask.until || 0; computeRedraw(); computeChrome();
  }
  maybeReloadForRelease();
  const w = ask && ask.work;
  if (!w) return false;
  if (w.kind === "riven_gain") return rivenGainOnce(w, id);
  heldLease = { lease: w.lease, verifier: id };
  const until = Date.now() + LEASE_SAFE_MS;
  const order = await api("/api/board/order", { record: w.record, ruler: w.ruler, mode: w.mode }, null, { community: true });
  if (!order || !order.ok) { giveBack(); return true; }
  computeStart({ kind: "board", weapon: w.record.weapon, ruler: w.ruler, mode: w.mode, identity: w.identity, record: w.record });
  const s = await measureRow(order.request, w.ruler, () => boardVerifyOn() && !computeHeld() && Date.now() < until, computeProgress);
  if (!s) { giveBack(); computeEnd(null); return true; }
  const sent = await postBoardWork("/api/board/verify",
    { lease: w.lease, verifier: id, engine: ENGINE_ID, score: s.score, metric: s.metric, work: s.work, compute_ms: s.compute_ms });
  if (sent) heldLease = null; else giveBack();
  computeEnd(sent ? { ms: s.compute_ms, work: s.work, score: s.score, metric: s.metric } : null);
  if (!sent) return true;
  try { localStorage.setItem(VERIFIED_KEY, String(boardVerifiedCount() + 1)); } catch (_) { /* private mode */ }
  renderBoardConsent();
  loadDevicePoints();
  return true;
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
  const computeLoop = async () => {
    for (;;) {
      let worked = false;
      try { worked = await workOnce(); } catch (_) { giveBack(); /* the next ask tries again */ }
      if (!worked) await new Promise((r) => setTimeout(r, ASK_EVERY_MS));
    }
  };
  if (navigator.locks && navigator.locks.request) navigator.locks.request("wfsim-community-compute", computeLoop);
  else computeLoop();
}

/// THE SWITCH, stated beside the board's own in the consent box.
function boardVerifyHtml() {
  const on = boardVerifyOn();
  const text = on
    ? tr("Your browser helps compute what WFSim gives everyone for free, in the background ({n} so far).")
      .replace("{n}", String(boardVerifiedCount()))
    : tr("Your browser does not compute the board's scores.");
  const stale = on && boardStale
    ? ` <span class="board-state">${escHtml(tr("A new version is out; this page refreshes itself once it is left idle."))}</span>
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
  const d = on && devicePoints;
  const earned = d ? ` <span class="board-state">${escHtml(tr("This browser: {n} points.")
    .replace("{n}", d.points.toLocaleString(accountLocale())))}</span>` : "";
  const join = on && !accountState.account
    ? ` <a href="/login?return=${encodeURIComponent("/contributors")}">${escHtml(tr("Sign in to count it under your name"))}</a> ·`
    : "";
  const mine = accountState.account && devicesState;
  const ask = on && mine && !mine.decided && mine.points > 0
    ? ` <span class="board-state">${escHtml(tr("You are on the contributors' ranking without your name. Show it?"))}</span>
      <button class="ghost-btn small" id="board-name-yes">${escHtml(tr("Show my name"))}</button>
      <button class="ghost-btn small" id="board-name-no">${escHtml(tr("Keep it anonymous"))}</button> ·`
    : "";
  return `${earned}${join}${ask} <a href="/compute">${escHtml(tr("Compute"))}</a> · <a href="/contributors">${escHtml(tr("Contributors"))}</a>`;
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
