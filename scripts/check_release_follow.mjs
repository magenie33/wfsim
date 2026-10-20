/// A NEW RELEASE IS TAKEN AT ONCE, AND TAKES NOTHING WITH IT (69-board-work.js
/// `readerWouldLose`). A page told of a newer release does not reload while the
/// reader was here lately, a field has focus, a write is in flight, a panel
/// holds it or a row is mid-measure — and does, at once, when none is so. What
/// the reload stashes is never an edit the reader has not taken, and a search's
/// ranking on screen is drawn again after it.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 13000, lang: "en", base: process.env.WFSIM_BASE });
const { evaluate, check, sleep } = app;

await app.load("/weapons/Torid/optimizer");

// A MARK ONLY THIS DOCUMENT HAS: gone means the page reloaded.
const stays = async (name, setup) => {
  await evaluate(`(() => { window.__mark = 1; releaseNewer = true; ${setup} maybeReloadForRelease(); })()`);
  await sleep(2000);
  check(name, await evaluate(`window.__mark === 1`) === true);
};
const quiet = `lastTouched = Date.now() - RELOAD_QUIET_MS - 60_000;`;

await stays("a reader here lately is not reloaded", `lastTouched = Date.now();`);
await stays("a page whose row is mid-measure is not reloaded",
  `${quiet} computeNow = { kind: "board", done: 1, total: 10, started: Date.now() };`);
await evaluate(`computeNow = null`);
await stays("a field in focus is not reloaded",
  `${quiet} const i = document.createElement("input"); i.id = "__field"; document.body.append(i); i.focus();`);
await evaluate(`document.getElementById("__field").remove()`);
await stays("a page a panel holds is not reloaded", `${quiet} window.__release = window.wfsim.hold();`);
await evaluate(`window.__release()`);

const writes = await evaluate(`(async () => {
  const p = fetch("/api/__nothing", { method: "POST", body: "{}" });
  const during = writesInFlight;
  await p.catch(() => null);
  return [during, writesInFlight];
})()`);
check("a write in flight is counted until it ends", JSON.stringify(writes) === "[1,0]", JSON.stringify(writes));

const stash = await evaluate(`(() => {
  const k = "wfsim-lang-stash", out = {};
  startEdit = { saved: { weapon: "torid", __saved: true } };
  stashForReload();
  out.start = JSON.parse(sessionStorage.getItem(k) || "null");
  startEdit = null;
  sessionStorage.removeItem(k);
  appraisal = { code: "TEST" };
  stashForReload();
  out.appraisal = sessionStorage.getItem(k);
  appraisal = null;
  sessionStorage.removeItem(k);
  return out;
})()`);
check("an open start stashes the build it was opened over", !!(stash.start && stash.start.__saved), JSON.stringify(stash.start));
check("an appraisal stashes no build", stash.appraisal === null, String(stash.appraisal));

// A RANKING ON SCREEN, drawn the way a finished search draws one.
await evaluate(`renderOptResults({ duration: 30, final_runs: 100, target: { name: "Test", level: 1 }, failed_starts: [],
  results: [{ rank: 1, kill_progress: 5, kill_progress_se: 0.1, forma: { used: 0 }, from_starts: [] }] })`);
await evaluate(`(() => { window.__mark = 1; releaseNewer = true; ${quiet} maybeReloadForRelease(); })()`);
await sleep(13000);
check("a reader away, with nothing held, is reloaded at once", await evaluate(`window.__mark === undefined`) === true);
const kept = await evaluate(`({ rank: optLast && optLast.results[0].rank, drawn: (document.getElementById("opt-results").textContent || "").includes("#1") })`);
check("the search's ranking is drawn again after the reload", kept.rank === 1 && kept.drawn, JSON.stringify(kept));

await app.finish("a new release is taken at once, and the reload loses the reader nothing");
