import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

const part = (name) => readFileSync(new URL(`../web/src/static/app/${name}.js`, import.meta.url), "utf8");

export async function checkHkFrontend() {
  const usage = part("07-usage");
  const base = usage.match(/const LIVE_ORIGIN = [^;]+;/)[0]
    + usage.match(/const liveSiteBase = [^;]+;/)[0] + "\nliveSiteBase()";
  for (const [hostname, expected] of [["cn.wfsim.app", ""], ["wfsim.app", ""],
    ["wfsim.localhost", "https://wfsim.app"], ["127.0.0.1", "https://wfsim.app"]]) {
    assert.equal(runInNewContext(base, { location: { hostname, origin: `https://${hostname}` } }), expected);
    const board = part("40-official-builds").match(/const BOARD_ORIGIN = [^;]+;/)[0];
    assert.equal(runInNewContext(board + "\nBOARD_ORIGIN", { location: { hostname } }),
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
  for (const hostname of ["cn.wfsim.app", "wfsim.app", "wfsim.localhost"]) {
    const requests = [];
    await runInNewContext(signal + "\nreadSignal()", {
      location: { hostname }, serverSkewMs: 0, SIGNAL_URL: "https://cdn.wfsim.app/work.json",
      SIGNAL_STALE_MS: 90_000, RELEASE_ID: "dev", syncDials: {},
      boardSignal: () => null, serverNow: () => Date.now(),
      fetch: async (url) => { requests.push(url); return { ok: true, json: async () => ({ v: 1, at: new Date().toISOString() }) }; },
    });
    assert.deepEqual(requests, [hostname === "cn.wfsim.app" ? "/work.json" : "https://cdn.wfsim.app/work.json"]);
  }
  console.log("HK frontend: usage, board files, work signal and page cache stay on the entrance; desktop and US routing preserved");
}
