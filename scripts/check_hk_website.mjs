import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

const part = (name) => readFileSync(new URL(`../web/src/static/app/${name}.js`, import.meta.url), "utf8");

export async function checkHkFrontend() {
  const origin = (part("00-boot") + part("40-official-builds")).match(/const BOARD_ORIGIN = [^;]+;/)[0];
  const signalUrl = part("69-board-work").match(/const SIGNAL_URL = [^;]+;/)[0];
  const usage = part("07-usage");
  const base = usage.match(/const LIVE_ORIGIN = [^;]+;/)[0]
    + usage.match(/const liveSiteBase = [^;]+;/)[0] + "\nliveSiteBase()";
  for (const [hostname, expected] of [["cn.wfsim.app", ""], ["wfsim.app", ""],
    ["wfsim.localhost", "https://wfsim.app"], ["127.0.0.1", "https://wfsim.app"]]) {
    assert.equal(runInNewContext(base, { location: { hostname, origin: `https://${hostname}` } }), expected);
    assert.equal(runInNewContext(origin + "\nBOARD_ORIGIN", { location: { hostname } }),
      hostname === "cn.wfsim.app" ? "" : "https://cdn.wfsim.app");
    let load, registered = 0;
    runInNewContext(part("04-page-cache"), {
      location: { hostname }, window: {},
      addEventListener: (_event, callback) => { load = callback; },
      navigator: { serviceWorker: { register: () => { registered++; return Promise.resolve(); } } },
    });
    load();
    assert.equal(registered, ["wfsim.app", "cn.wfsim.app"].includes(hostname) ? 1 : 0);
  }
  const signal = part("69-board-work").match(/async function readSignal\(\) \{[\s\S]*?\n\}/)[0];
  const nav = part("73-compute").match(/async function navComputing\(\) \{[\s\S]*?\n\}/)[0];
  const stats = part("16-support").match(/async function statsFile\(name, staleMs\) \{[\s\S]*?\n\}/)[0];
  const statsOrigin = part("16-support").match(/const STATS_ORIGIN = [^;]+;/)?.[0] || "";
  for (const hostname of ["cn.wfsim.app", "wfsim.app", "wfsim.localhost"]) {
    const requests = [], el = {};
    const context = {
      location: { hostname, pathname: "/weapons/Torid/simulator" }, serverSkewMs: 0,
      SIGNAL_STALE_MS: 90_000, RELEASE_ID: "dev", syncDials: {},
      boardSignal: () => null, serverNow: () => Date.now(),
      timeoutSignal: () => null, document: { hidden: false, getElementById: () => el }, WASM: true,
      computingAskedAt: 0, computingCount: null, computingEpoch: undefined, computeDemand: null,
      COMPUTING_EVERY_MS: 60_000, COMPUTING_AWAY_EVERY_MS: 300_000,
      accountLocale: () => "en", escHtml: s => s, tr: s => s,
      computeLinkPaint() {}, renderHomeTally() {}, renderSupportCounts() {}, computeRedraw() {},
      fetch: async url => { requests.push(url); return { ok: true, json: async () => ({ v: 1, at: new Date().toISOString(), computing: 7 }) }; },
    };
    const definitions = origin + "\n" + signalUrl + "\n" + statsOrigin + "\n";
    const base = hostname === "cn.wfsim.app" ? "" : "https://cdn.wfsim.app";
    await runInNewContext(definitions + signal + "\nreadSignal()", context);
    assert.deepEqual(requests.splice(0), [base + "/work.json"], "idle signal: " + hostname);
    await runInNewContext(definitions + nav + "\nnavComputing()", { ...context });
    assert.deepEqual(requests.splice(0), [base + "/work.json"], "navigation signal: " + hostname);
    await runInNewContext(definitions + stats + '\nstatsFile("tally", 600000)', { ...context });
    assert.deepEqual(requests.splice(0), [base + "/stats/tally.json"], "public statistics: " + hostname);
  }
  console.log("HK frontend: usage, board files, work signal and page cache stay on the entrance; desktop and US routing preserved");
}
