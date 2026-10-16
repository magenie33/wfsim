// THE PUBLIC STATS ON THE PAGE (docs/BOARD.md §"The public stats"), on the built
// site, the CDN and the worker stubbed before the app boots. Asserted: the home
// tally reads `stats/tally.json` and asks no worker; a file stale by the server's
// clock, unreachable, or of another version is passed over for `/api/board/tally`.
// Signed out, the ranking is sliced from `stats/contributors.json`; a file of
// another day, or none, is passed over for `/api/contributors`; signed in, the
// worker is asked and the file never read.
//   node scripts/check_stats_files.mjs
//   WFSIM_BREAK=<stale|nofile|day|signedin> …  breaks one, to see it fail
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 13000 });
const { evaluate, check } = app;
const BREAK = process.env.WFSIM_BREAK || "";

// THE CDN AND THE WORKER: each answers what the page set, every ask recorded.
await app.send("Page.addScriptToEvaluateOnNewDocument", { source: `(() => {
  const realFetch = window.fetch;
  window.__calls = []; window.__stats = {};
  const reply = (j) => new Response(JSON.stringify(j), { status: 200, headers: { "content-type": "application/json", date: new Date().toUTCString() } });
  window.fetch = async (url, init) => {
    const u = typeof url === "string" ? url : url.url;
    const file = u.match(/^https:\\/\\/cdn\\.wfsim\\.app\\/stats\\/(\\w+)\\.json$/);
    if (file) {
      window.__calls.push("file:" + file[1]);
      const s = window.__stats[file[1]];
      if (s === "fail" || s === undefined) throw new TypeError("unreachable");
      if (window.__slow) await new Promise((r) => setTimeout(r, window.__slow));
      return reply(s);
    }
    if (u.endsWith("/api/board/tally")) {
      window.__calls.push("api:tally");
      return reply({ ok: true, v: 1, at: new Date().toISOString(), totals: { volunteers: 222, official: 2 }, per_hour: { volunteers: 0, official: 0 }, computing: 0 });
    }
    if (u.includes("/api/contributors")) {
      window.__calls.push("api:contributors");
      return reply({ ok: true, period: "all", computing: 0, total: 1, contributors: [{ key: "from-worker", name: null, points: 1, recent: 1, week: 1 }], pool: null });
    }
    return realFetch(url, init);
  };
})();` });
await app.send("Page.reload");
await app.sleep(4000);
await evaluate(`(async () => { const end = Date.now() + 30000; while (!window.__wfsimReady && Date.now() < end) await new Promise((r) => setTimeout(r, 100)); return true; })()`);

const out = JSON.parse(await evaluate(`(async () => {
  const BREAK = ${JSON.stringify(BREAK)};
  const ago = (ms) => new Date(serverNow() - ms).toISOString();
  const today = new Date(serverNow()).toISOString().slice(0, 10);
  if (BREAK === "stale") { const real = statsFile; statsFile = (name) => real(name, Infinity); }
  if (BREAK === "nofile") statsFile = async () => null;
  const tally = (at, v = 1) => ({ ok: true, v, at, totals: { volunteers: 111, official: 1 }, per_hour: { volunteers: 60, official: 0 }, computing: 3 });
  const home = async (file) => {
    window.__stats.tally = file; window.__calls.length = 0; tallyShown = null;
    $("home-page").hidden = false;
    await loadHomeTally();
    return { got: tallyTo && tallyTo.volunteers, calls: [...window.__calls] };
  };
  const o = {};
  o.fresh = await home(tally(ago(60_000)));
  o.stale = await home(tally(ago(20 * 60_000)));
  o.unreachable = await home("fail");
  o.version = await home(tally(ago(0), 2));
  const row = (key, points, week) => ({ key, name: null, points, recent: points, week });
  const ranking = (day) => ({ ok: true, v: 1, at: ago(60_000), day, computing: 4, pool: null,
    rankings: { all: [row("k1", 9, 0), row("k2", 5, 5), row("k3", 2, 1)], recent: [row("k2", 5, 5), row("k3", 2, 1)], week: [row("k2", 5, 5), row("k3", 2, 1)] } });
  const ranked = async (file, period = "week", limit = 1) => {
    window.__stats.contributors = file; window.__calls.length = 0;
    if (BREAK === "day" && file !== "fail") file.day = "2000-01-01";
    contributorsPeriod = period; contributorsLimit = limit; contributorsState = []; contributorsLoading = false;
    await contributorsLoad();
    return { keys: contributorsState.map((c) => c.key), total: contributorsTotal, calls: [...window.__calls] };
  };
  const was = accountState.account;
  accountState.account = null;
  o.signedOut = await ranked(ranking(today));
  o.otherDay = await ranked(ranking("2000-01-01"));
  o.noFile = await ranked("fail");
  accountState.account = BREAK === "signedin" ? null : { id: "x", username: "u" };
  o.signedIn = await ranked(ranking(today));
  accountState.account = null;
  // A TAB CHOSEN WHILE A LOAD IS IN FLIGHT is loaded once that one lands.
  window.__stats.contributors = ranking(today); window.__slow = 400;
  contributorsPeriod = "week"; contributorsLimit = 10; contributorsState = []; contributorsLoading = false;
  const first = contributorsLoad();
  await new Promise((r) => setTimeout(r, 50));
  if (BREAK !== "tab") { contributorsPeriod = "all"; contributorsState = undefined; contributorsLoad(); }
  await first; await new Promise((r) => setTimeout(r, 800)); window.__slow = 0;
  o.tab = { keys: (contributorsState || []).map((c) => c.key), loading: contributorsLoading };
  // A REFRESH THAT ONLY MOVES ROWS still redraws the period line and the totals.
  nav("/contributors");
  await new Promise((r) => setTimeout(r, 300));
  contributorsPeriod = "week"; contributorsState = null; renderAuthPage("contributors");
  await new Promise((r) => setTimeout(r, 300));
  const shown = () => ($("rank-window")?.textContent || "") + "|" + ($("rank-totals")?.textContent || "");
  const before = shown();
  const later = ranking(today);
  later.windows = { week: { from: "2031-03-03T00:00:00.000Z", until: "2031-03-10T00:00:00.000Z" } };
  later.pool = { points: 777, deleted_account: 1, refused: 2 };
  window.__stats.contributors = later;
  if (BREAK === "patch") contributorsPatch = () => true;
  await contributorsLoad();
  o.patch = { list: !!document.querySelector(".rank-list"), before, after: shown() };
  accountState.account = was;
  return JSON.stringify(o);
})()`).catch((e) => JSON.stringify({ error: String(e) })));
const said = JSON.stringify(out);
const only = (r, ...calls) => r && JSON.stringify(r.calls) === JSON.stringify(calls);

check("the home tally reads the CDN's file and asks no worker", out.fresh?.got === 111 && only(out.fresh, "file:tally"), said);
check("...a file stale by the server's clock is passed over for the worker", out.stale?.got === 222 && only(out.stale, "file:tally", "api:tally"), said);
check("...and so is one that cannot be reached, or of another version",
  out.unreachable?.got === 222 && only(out.unreachable, "file:tally", "api:tally") && out.version?.got === 222 && only(out.version, "file:tally", "api:tally"), said);
check("signed out, the ranking is the file's period, sliced to the page, its count the file's",
  JSON.stringify(out.signedOut?.keys) === JSON.stringify(["k2"]) && out.signedOut.total === 2 && only(out.signedOut, "file:contributors"), said);
check("...a file of another day, or none, is passed over for the worker",
  JSON.stringify(out.otherDay?.keys) === JSON.stringify(["from-worker"]) && only(out.otherDay, "file:contributors", "api:contributors")
  && JSON.stringify(out.noFile?.keys) === JSON.stringify(["from-worker"]) && only(out.noFile, "file:contributors", "api:contributors"), said);
check("signed in, the worker is asked, for the reader's own row, and the file is never read",
  JSON.stringify(out.signedIn?.keys) === JSON.stringify(["from-worker"]) && only(out.signedIn, "api:contributors"), said);

check("a tab chosen while a load is in flight is loaded when that one lands, not left empty",
  JSON.stringify(out.tab?.keys) === JSON.stringify(["k1", "k2", "k3"]) && out.tab.loading === false, said);
check("a refresh that only moves rows still redraws the period line and the totals",
  out.patch?.list && /777/.test(out.patch.after) && out.patch.after !== out.patch.before && out.patch.after.split("|")[0].trim() !== "", said);

await app.finish("the home tally and the ranking are files first, the worker when a file is stale, missing or not the reader's");
