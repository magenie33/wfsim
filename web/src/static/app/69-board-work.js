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
/// order, then scored, with what the pieces took in ms summed over the lanes
/// (`compute_ms`) — the rests between them (`communityRest`) are not in it.
/// `null` when the engine refused or `live()` went false between pieces.
///
/// EVERY LANE FIGHTS, ONE MERGES. A run depends on its index alone, so lanes
/// pull pieces off one cursor (`/api/board/runs`, a shard per run) and the
/// shards are folded as `pieces` strictly in run order — the scorer's merges in
/// the scorer's order, so the bits are the scorer's however the lanes raced.
/// A lane past `communityLanes()` — the share was lowered — takes no new piece.
async function measureRow(request, ruler, live, onPiece = () => {}) {
  const runs = Number(request.runs) || 0;
  const ls = await boardLanes(communityLanes());
  let cursor = 0, est = null, failed = false, spent = 0, folded = 0;
  const ready = new Map();
  let acc = null;
  let merging = Promise.resolve();
  // ONE CALL ON A LANE, asked again on a fresh worker when a Stop or a lost
  // worker took it: the range is the same, so the answer is too.
  const ask = async (k, path, body) => {
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
    merging = merging.then(async () => {
      if (failed) return;
      const step = await ask(0, "/api/board/fold", { request, acc, pieces });
      if (step) { acc = step.acc; onPiece(Math.min(runs, folded), runs); } else failed = true;
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
const releaseLease = (h, beacon) => {
  const path = h.code ? `/api/appraise/${encodeURIComponent(h.code)}/renew` : "/api/board/release";
  const body = { lease: h.lease, verifier: h.verifier, ...(h.code ? { release: true } : {}) };
  if (beacon && navigator.sendBeacon) navigator.sendBeacon(path, JSON.stringify(body));
  else postBoardWork(path, body);
};
function giveBack(beacon = false) {
  const h = heldLease;
  heldLease = null;
  if (h) releaseLease(h, beacon);
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

/// THE NEXT TASKS, ASKED BEFORE THIS ONE ENDS (worker/verify.js, asking ahead):
/// once a task is `AHEAD_AT` done, so the next starts the moment it ends and no
/// core waits on a round trip and the server's choosing. As many as the server
/// says (`ahead`, its `TASKS_AHEAD`) and no more, asked once per task.
const AHEAD_AT = 0.8;
let tasksAhead = 0;
let aheadAsks = [], aheadAskedFor = null;
function askAhead(id, task) {
  if (aheadAskedFor === task || !tasksAhead || !boardVerifyOn() || computeHeld()) return;
  aheadAskedFor = task;
  const c = computeConsent();
  const at = Date.now();
  for (let i = aheadAsks.length; i < tasksAhead; i++) {
    const entry = { held: null };
    entry.ask = postBoardWork("/api/board/work",
      { verifier: id, engine: ENGINE_ID, protocol: 6, consent: { v: c.v, at: c.at }, lanes: communityLanes(), ahead: true })
      .then((r) => {
        const w = r && r.work;
        entry.held = w ? { code: w.kind === "riven_gain" ? w.code : null, lease: w.lease, verifier: id } : null;
        return r && { ...r, asked_at: at };
      });
    aheadAsks.push(entry);
  }
}
/// …GIVEN BACK on every way out that does not start them.
function giveBackAhead(beacon = false) {
  const all = aheadAsks;
  aheadAsks = [];
  all.forEach((e) => { if (e.held) releaseLease(e.held, beacon); });
}
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
/// `how` is what the next page's `app.boot` says it arrived by (`usageArrival`):
/// `idle` when the page reloaded itself, `asked` when the reader clicked.
function reloadForRelease(how = "asked") {
  try {
    sessionStorage.setItem("wfsim-lang-stash", JSON.stringify(snapshotState()));
    sessionStorage.setItem(USAGE_ARRIVAL, `release_${how}`);
  } catch (_) { /* nothing to keep */ }
  location.reload();
}
function maybeReloadForRelease() {
  if ((!boardStale && !releaseNewer) || window.__WFSIM_DESKTOP__ || readerBusy() || Date.now() - lastTouched < IDLE_RELOAD_MS) return;
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
/// in bursts; a newer one is said on the page and reloaded into once idle.
/// THE ANSWER NAMES THE FAILURE in `engine.fail`: `worker_load_stale` when a newer
/// release left the page behind, `worker_load_offline` when the site itself did
/// not answer, `worker_load` when it did and the download failed all the same.
let releaseAskedAt = 0, releaseVerdict = "worker_load";
async function releaseAfterLaneFailed(ms) {
  if (!WASM || RELEASE_ID === "dev" || window.__WFSIM_DESKTOP__ || Date.now() - releaseAskedAt < 60_000) {
    track("engine.fail", releaseVerdict, ms);
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
  releaseVerdict = newer ? "worker_load_stale" : "worker_load";
  track("engine.fail", releaseVerdict, ms);
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
  el.innerHTML = `${escHtml(tr("A new version is out; this page refreshes itself once it is left idle."))}<a href="#">${escHtml(tr("refresh now"))}</a>`;
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
    computeProgress(s.sims_done || 0, 0, s);
    // A SEARCH'S LENGTH IS NOT KNOWN AHEAD, so the last one's stands for it.
    if (rivenTookMs && performance.now() - began > AHEAD_AT * rivenTookMs) askAhead(id, w.lease);
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
      score: Number(best.kill_progress) || 0, work: r.work || 0 }),
  }).then((x) => x.ok).catch(() => false);
  if (sent) heldLease = null; else giveBack();
  // …AND WHAT IT FOUND, for this browser's own list: the build, its number, what the search took.
  // A search row carries `kill_progress`, never `score`, and the duration is the answer's.
  const v = kpm(best.kill_progress ?? best.kills, r.duration);
  computeEnd(sent ? { ms: Math.round(performance.now() - began), work: r.work || 0, ...(Number.isFinite(v) ? { score: v, metric: "kpm" } : {}),
    record: boardPayloadFromResult(best, w.context), search: { builds: (job.status || {}).enumerated || 0, fights: r.fights || 0 } } : null);
  return true;
}

/// ONE ORDER, fought here and answered — `true` when there was one. The answer
/// never says whether it agreed; a lease it does not answer is given back.
async function workOnce() {
  computeFrozen = false;
  if (!boardVerifyOn() || computeHeld()) { giveBackAhead(); return false; }
  const id = verifierId();
  if (!id) return false;
  await claimDevice(id);
  const c = computeConsent();
  // THE FIRST TASK ASKED AHEAD that holds one, when there is one; else asked now.
  let pre = null;
  while (aheadAsks.length && !(pre && pre.work)) pre = await aheadAsks.shift().ask;
  const ask = pre && pre.work ? pre : await postBoardWork("/api/board/work",
    { verifier: id, engine: ENGINE_ID, protocol: 6, consent: { v: c.v, at: c.at }, lanes: communityLanes() });
  if (ask && Number.isInteger(ask.ahead) && ask.ahead >= 0) tasksAhead = ask.ahead;
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
  const until = (ask.asked_at || Date.now()) + LEASE_SAFE_MS;
  const order = await api("/api/board/order", { record: w.record, ruler: w.ruler, mode: w.mode }, null, { community: true });
  if (!order || !order.ok) { giveBack(); return true; }
  computeStart({ kind: "board", weapon: w.record.weapon, ruler: w.ruler, mode: w.mode, identity: w.identity, record: w.record });
  const began = Date.now();
  const s = await measureRow(order.request, w.ruler, () => boardVerifyOn() && !computeHeld() && !computeFrozen && Date.now() < until, (done, total) => {
    computeProgress(done, total);
    if (total && done >= AHEAD_AT * total) askAhead(id, w.lease);
  });
  if (!s) { giveBack(); computeEnd(null); return true; }
  const sent = await postBoardWork("/api/board/verify",
    { lease: w.lease, verifier: id, engine: ENGINE_ID, score: s.score, metric: s.metric, work: s.work, compute_ms: s.compute_ms });
  if (sent) heldLease = null; else giveBack();
  computeEnd(sent ? { ms: Date.now() - began, cpu_ms: s.compute_ms, work: s.work, score: s.score, metric: s.metric } : null);
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
      if (!worked) await new Promise((r) => { computeWake = r; setTimeout(r, ASK_EVERY_MS); });
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
  const d = on && devicePoints && !devicePoints.claimed && devicePoints;
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
