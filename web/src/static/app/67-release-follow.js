// ---- A NEW RELEASE, followed at once ------------------------------------
//
// A page on a replaced engine is handed next to nothing, so it reloads into the
// new release the moment that loses the reader nothing (`readerWouldLose`), and
// what the reload stashes is restored by the next page. The desktop client
// applies its update on the same terms (82-desktop.js).

/// A NEWER ENGINE IS OUT (`stale`): this page is given no more work.
let boardStale = false;
/// …OR A NEWER RELEASE: an older engine still accepted is handed only the rows
/// a served answer leads, which is nearly nothing. Either way the page reloads
/// into it AT ONCE, the moment a reload loses the reader nothing
/// (`readerWouldLose`). Not in the desktop shell, whose updater swaps the files.
let releaseNewer = false;
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

/// WORK IN PROGRESS A RELOAD WOULD END, held for as long as it lasts: a write
/// in flight is counted where every write goes out (`fetch`), so a new form is
/// covered without anyone listing it; a span of several writes, or a panel's
/// own work (the door's `hold`), holds the page itself.
let pageHolds = 0, writesInFlight = 0;
function holdPage() {
  pageHolds += 1;
  let done = false;
  return () => { if (!done) { done = true; pageHolds = Math.max(0, pageHolds - 1); } };
}
{
  const send = window.fetch.bind(window);
  window.fetch = (input, init) => {
    const method = String((init && init.method) || (input && input.method) || "GET").toUpperCase();
    if (method === "GET" || method === "HEAD") return send(input, init);
    writesInFlight += 1;
    return send(input, init).finally(() => { writesInFlight = Math.max(0, writesInFlight - 1); });
  };
}
/// A READER WHO TOUCHED THE PAGE THIS RECENTLY may hold what nothing here can
/// see — a half-typed field, a step only memory keeps — so the reload waits;
/// a machine left computing is untouched and reloads at once.
const RELOAD_QUIET_MS = 2 * 60_000;
const WRITABLE = "textarea, select, input:not([type=checkbox]):not([type=radio]):not([type=range]):not([type=button]):not([type=submit]):not([type=reset]):not([type=file])";
/// WHAT A RELOAD WOULD LOSE, derived from the page's state: anything running or
/// being written, a field in focus, an edit only memory holds (a search's start
/// open in the builder, a chat's appraisal, an e-mailed code awaited), or a
/// reader here lately — unless `lately` is false, for a page left behind whose
/// lanes cannot load. The pending saves are not on it: a reload writes them.
function readerWouldLose(lately = true) {
  if (readerBusy() || pageHolds > 0 || writesInFlight > 0) return true;
  if (startEdit || appraisal || authFlow.step === 2) return true;
  const el = document.activeElement;
  if (el && (el.isContentEditable || (el.matches(WRITABLE) && !el.readOnly && !el.disabled))) return true;
  return lately && Date.now() - lastTouched < RELOAD_QUIET_MS;
}

/// THE PAGE AS THE NEXT ONE RESTORES IT (`10-weapon-search.js`), for a language
/// switch or a release: the build being edited — but never an edit the reader
/// has not taken, since the restore saves what it restores into the active
/// build. An open start restores the build it was opened over; an appraisal,
/// whose build is a chat's, restores nothing. And where the page was scrolled.
function stashForReload() {
  try {
    const st = appraisal ? null : startEdit ? startEdit.saved : snapshotState();
    if (st) sessionStorage.setItem("wfsim-lang-stash", JSON.stringify(st));
    sessionStorage.setItem(RELOAD_SCROLL_KEY, String(Math.round(scrollY)));
  } catch (_) { /* nothing to keep */ }
}
const RELOAD_SCROLL_KEY = "wfsim-reload-scroll";
/// …SCROLLED BACK once the page is tall enough to hold it, which a page drawing
/// as its data arrives is not at boot; given up after a few seconds, or as soon
/// as the reader scrolls for themselves.
function restoreReloadScroll() {
  let y = 0;
  try { y = Number(sessionStorage.getItem(RELOAD_SCROLL_KEY)) || 0; sessionStorage.removeItem(RELOAD_SCROLL_KEY); } catch (_) { return; }
  if (y <= 0) return;
  const from = lastTouched, until = Date.now() + 5000;
  const tryIt = () => {
    if (lastTouched !== from) return;
    if (document.documentElement.scrollHeight >= y + innerHeight || Date.now() > until) { scrollTo(0, y); return; }
    setTimeout(tryIt, 100);
  };
  tryIt();
}

/// `how` is what the next page's `app.boot` says it arrived by (`usageArrival`):
/// `idle` when the page reloaded itself, `asked` when the reader clicked. The
/// saves waiting on a debounce are written and the results computed here handed
/// in first: the next page keeps only its own engine's results.
async function reloadForRelease(how = "asked") {
  flushPresetSaves();
  stashForReload();
  try { sessionStorage.setItem(USAGE_ARRIVAL, `release_${how}`); } catch (_) { /* nothing to keep */ }
  await handInResults();
  forgetKeptPages().finally(() => location.reload());
}
/// …AT MOST `HAND_IN_MS`: a sync that does not answer must not hold the update,
/// and what it did not take still goes by beacon as the page leaves (`giveBack8`).
const HAND_IN_MS = 8000;
async function handInResults() {
  const id = verifierId();
  if (!id || boardProtocol !== 8 || !resultsMem.some((r) => r.verifier === id && r.engine === ENGINE_ID)) return;
  await Promise.race([boardSync(id, { want: 0 }).catch(() => null), new Promise((ok) => setTimeout(ok, HAND_IN_MS))]);
}
let reloadingForRelease = false;
function maybeReloadForRelease() {
  if (!boardStale && !releaseNewer) return;
  // THE DESKTOP CLIENT'S UPDATER SWAPS THE FILES (82-desktop.js), told to look now.
  if (window.__WFSIM_DESKTOP__) { desktopFollowRelease(); return; }
  if (reloadingForRelease || computeNow || readerWouldLose(!releaseLeftBehind)) return;
  // ONE TRY AN HOUR: a CDN still serving the old files must not make a loop.
  try {
    if (Date.now() - Number(sessionStorage.getItem("wfsim-release-reload") || 0) < 3_600_000) return;
    sessionStorage.setItem("wfsim-release-reload", String(Date.now()));
  } catch (_) { return; }
  reloadingForRelease = true;
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
