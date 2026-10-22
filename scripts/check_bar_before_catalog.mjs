// SPDX-License-Identifier: AGPL-3.0-or-later
// A BUILD BAR REDRAWN BEFORE ITS PAGE HAS A BUILD draws nothing, and throws
// nothing: the Warframe and companion pages show before the catalogue arrives,
// and the account's answer redraws every bar on screen meanwhile. A bar that
// read the empty build threw inside that answer, which then never drew the
// sign-in entry, loaded the notices or started a sync.
//
//   WFSIM_BASE=http://127.0.0.1:<port> node scripts/check_bar_before_catalog.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  const loose = [];
  addEventListener('unhandledrejection', (e) => loose.push(String(e.reason && e.reason.message || e.reason)));
  const real = api;
  const one = async (path, bar) => {
    WFCAT = null; wf = null; comp = null;
    let open;
    const gate = new Promise((res) => { open = res; });
    api = (p, b, ...rest) => (p === '/api/warframe/catalog' ? gate.then(() => real(p, b, ...rest)) : real(p, b, ...rest));
    history.pushState({}, '', path); route(); await sleep(800);
    const shown = !!document.getElementById(bar).offsetParent;
    // THE ACCOUNT ANSWERS WHILE THE CATALOGUE IS HELD, saying something new.
    accountState = { ...accountState, providers: ['held'] };
    let threw = '';
    try { await loadAccount(); } catch (e) { threw = String(e && e.message || e); }
    open(); await sleep(2500);
    api = real;
    return { path, shown, threw, drawn: document.getElementById(bar).children.length > 0 };
  };
  const frame = await one(warframePath(META.warframes[0]), 'preset-bar-warframes');
  const host = await one(companionPath(compHosts()[0]), 'preset-bar-companions');
  return { frame, host, loose };
})()`);
console.log(JSON.stringify(r, null, 1));
for (const [what, x] of [["the Warframe page", r.frame], ["a companion page", r.host]]) {
  check(`${what}'s bar is on screen while its catalogue is held`, x.shown, JSON.stringify(x));
  check(`...the account answering then throws nothing`, !x.threw, x.threw);
  check(`...and the bar is drawn once the catalogue lands`, x.drawn, JSON.stringify(x));
}
check("no promise was left rejected", !r.loose.length, JSON.stringify(r.loose));
await app.finish("a build bar redrawn before its page has a build");
