import assert from "node:assert/strict";
import { openApp, serveSite } from "./cdp.mjs";
import { recommendationFixture } from "./recommendations_fixture.mjs";
import { writeFile, mkdir } from "node:fs/promises";
import { usagePoint, USAGE_SCHEMA } from "../worker/index.js";

const staticServer = process.env.WFSIM_BASE ? null : await serveSite(new URL("../site/", import.meta.url).pathname);
const base = process.env.WFSIM_BASE || "http://127.0.0.1:" + staticServer.address().port;
const fixture = await recommendationFixture(base);
const app = await openApp({ base: fixture.base, boot: 6500, lang: "en" });
await app.send("Network.enable");
await app.send("Network.setCacheDisabled", { cacheDisabled: true });
await app.send("Page.addScriptToEvaluateOnNewDocument", { source: `
  addEventListener('wfsim:ready', () => {
    const originalApi = api;
    api = async (path, body, ...args) => {
      const result = await originalApi(path, body, ...args);
      if (path === '/api/build/keys' && body.kind === 'build' && result.keys?.length > 1 && !window.recommendationRejectedBuild) {
        window.recommendationRejectedBuild = body.builds[0];
        result.keys[0] = null;
      }
      return result;
    };
    const originalTrack = track;
    track = (event, subject, n) => {
      if (event.startsWith('recommendation.')) {
        const points = JSON.parse(sessionStorage.getItem('recommendation-test-points') || '[]');
        points.push({ event, subject, n });
        sessionStorage.setItem('recommendation-test-points', JSON.stringify(points));
      }
      return originalTrack(event, subject, n);
    };
    refreshLibraryRecommendations();
  });
` });
const check = (name, value) => { assert.ok(value, name); console.log("  ok  " + name); };
async function until(expression) {
  for (let n = 0; n < 120; n++) { if (await app.evaluate(expression)) return; await app.sleep(250); }
  console.log(await app.evaluate("({ url: location.pathname + location.search, day: recommendationAccount.day, pending: [...recommendationAccount.pending], library: recommendationLibrary && { loading: recommendationLibrary.loading, error: recommendationLibrary.error, period: recommendationLibrary.period, heat: recommendationLibrary.heat }, text: document.querySelector('#build-finder')?.innerText.slice(-1000) })"));
  throw new Error("Timed out: " + expression);
}
try {
  await app.load("/weapons/Laetum", 6000);
  await app.evaluate("gainPrefs.on = false; restartCalc()");
  await until("!!document.querySelector('[data-recommendation-id]:not([disabled])')");
  check("An obsolete row leaves legal builds recommendable", await app.evaluate("!!recommendationRejectedBuild && !recommendationLibrary.error && builtinBuilds(finder.b).some(p => recommendationLibrary.ids.get(p.board) === null)"));
  check("An obsolete row has no recommendation control", await app.evaluate("[...document.querySelectorAll('[data-recommend-build]')].every(node => recommendationLibrary.ids.get(recommendationLibrary.presets.get(node.dataset.recommendBuild)?.board) !== null)"));
  const target = await app.evaluate(`(() => {
    const button = [...document.querySelectorAll('[data-recommendation-id]:not([disabled])')].at(-1);
    const p = builtinBuilds(finder.b).find(p => presetId(p) === button.dataset.recommend);
    return { id: button.dataset.recommendationId, preset: presetId(p), record: recommendationSource(p), ruler: p.benchmark, mode: p.mode };
  })()`);
  await fixture.seed(target.id, target.record, target.ruler, target.mode);
  check("The engine gives the displayed build a stable library identity", /^[a-f0-9]{32}$/.test(target.id));
  check("Recommendation identities match the scorer", await app.evaluate("(async () => (await api('/api/board/order', { record: " + JSON.stringify(target.record) + ", ruler: " + JSON.stringify(target.ruler) + ", mode: " + JSON.stringify(target.mode) + " })).identity === " + JSON.stringify(target.id) + ")()"));
  await app.evaluate("window.recommendationSearchNode = document.querySelector('#fd-input'); recommendationSearchNode.focus(); recommendationSearchNode.value = 'galv'; recommendationAccount.known.clear(); recommendationLibrary.heat = null; refreshLibraryRecommendations()");
  await until("!recommendationLibrary.loading");
  check("Background recommendation refresh preserves search input and focus", await app.evaluate("document.activeElement === recommendationSearchNode && recommendationSearchNode.value === 'galv'"));
  await app.evaluate("recommendationSearchNode.value = ''; recommendationSearchNode.blur()");
  const selector = '[data-recommendation-id="' + target.id + '"]';
  await app.evaluate("document.querySelector(" + JSON.stringify(selector) + ").scrollIntoView({ block: 'center' })");
  await until("JSON.parse(sessionStorage.getItem('recommendation-test-points') || '[]').some(p => p.event === 'recommendation.ready' && p.subject === 'library')");
  await app.evaluate("document.querySelector(" + JSON.stringify(selector) + ").click()");
  await until("document.querySelector(" + JSON.stringify(selector) + ")?.getAttribute('aria-pressed') === 'true'");
  check("A recommendation is durably pending", (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes")).n === 1);
  check("Pending recommendations leave settled popularity at zero", await app.evaluate("document.querySelector(" + JSON.stringify(selector) + ").closest('.recommendation-control').querySelector('b').textContent === '0.0'"));
  await fetch(fixture.base + '/api/recommendations', { method: 'POST', headers: { 'content-type': 'application/json', origin: fixture.base, cookie: 'wfsim_session=' + fixture.token },
    body: JSON.stringify({ identity: target.id, action: 'withdraw', day: new Date().toISOString().slice(0, 10) }) });
  await app.evaluate("window.dispatchEvent(new Event('focus'))");
  await until("document.querySelector(" + JSON.stringify(selector) + ")?.getAttribute('aria-pressed') === 'false'");
  check("Returning to the tab observes another client's withdrawal", (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes")).n === 0);
  await app.evaluate("document.querySelector(" + JSON.stringify(selector) + ").click()");
  await until("document.querySelector(" + JSON.stringify(selector) + ")?.getAttribute('aria-pressed') === 'true'");
  await app.evaluate("document.querySelector(" + JSON.stringify(selector) + ").closest('.recommendation-control').querySelector('[data-recommend-withdraw]').click()");
  await until("document.querySelector(" + JSON.stringify(selector) + ")?.getAttribute('aria-pressed') === 'false'");
  check("Withdrawing a pending recommendation removes it", (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes")).n === 0);
  await app.evaluate("document.querySelector(" + JSON.stringify(selector) + ").click()");
  await until("document.querySelector(" + JSON.stringify(selector) + ")?.getAttribute('aria-pressed') === 'true'");
  await app.evaluate("openLibraryBuild(recommendationLibrary.presets.get(" + JSON.stringify(target.preset) + "))");
  const presetVote = '#preset-bar-builder-builds ' + selector;
  await until("document.querySelector(" + JSON.stringify(presetVote) + ")?.getAttribute('aria-pressed') === 'true'");
  await app.evaluate("document.querySelector(" + JSON.stringify(presetVote) + ").scrollIntoView({ block: 'center' })");
  await until("JSON.parse(sessionStorage.getItem('recommendation-test-points') || '[]').some(p => p.event === 'recommendation.ready' && p.subject === 'preset')");
  check("A board preset shares the Library's pending recommendation", (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes")).n === 1);
  await app.evaluate("document.querySelector('#preset-bar-builder-builds [data-preset-info]').click()");
  const infoVote = '#preset-info-popover ' + selector;
  await until("!!document.querySelector(" + JSON.stringify(infoVote) + ")");
  await app.evaluate("document.querySelector(" + JSON.stringify(infoVote) + ").scrollIntoView({ block: 'center' })");
  await until("JSON.parse(sessionStorage.getItem('recommendation-test-points') || '[]').some(p => p.event === 'recommendation.ready' && p.subject === 'preset_info')");
  await app.evaluate("document.querySelector('#preset-info-popover [data-recommend-withdraw]').click()");
  await until("document.querySelector(" + JSON.stringify(presetVote) + ")?.getAttribute('aria-pressed') === 'false'");
  check("Withdrawing in preset information updates the preset bar", (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes")).n === 0);
  fixture.env.RECOMMENDATION_LIMIT = { limit: async () => ({ success: false }) };
  await app.evaluate("document.querySelector(" + JSON.stringify(infoVote) + ").click()");
  await until("!!document.querySelector('#preset-info-popover [data-recommend-retry]')");
  check("A failed recommendation remains retryable without adding a vote", (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes")).n === 0);
  delete fixture.env.RECOMMENDATION_LIMIT;
  await app.evaluate("document.querySelector('#preset-info-popover [data-recommend-retry]').click(); document.querySelector('#preset-info-popover [data-info-close]').click(); document.querySelector(" + JSON.stringify(presetVote) + ").click()");
  await until("document.querySelector(" + JSON.stringify(presetVote) + ")?.getAttribute('aria-pressed') === 'true'");
  check("Recommending from the preset reuses the same daily vote", (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes")).n === 1);
  await app.evaluate("copyActivePreset(buildBarCfg())");
  check("An owned copy has no recommendation control", await app.evaluate("!document.querySelector('#preset-bar-builder-builds [data-recommend]')"));
  await app.evaluate("nav(weaponPath(presetWeapon()) + '?view=library')");
  await until("document.querySelector(" + JSON.stringify(selector) + ")?.getAttribute('aria-pressed') === 'true'");
  await app.load("/account/recommendations", 4000);
  await until("!!recommendationHistory?.builds");
  check("Mine opens directly with saved recommendations", await app.evaluate("!document.querySelector('#boot-note') && !!document.querySelector('.recommendation-points')"));
  check("Mine shows permanent recommendation points beside contribution points", await app.evaluate("document.querySelector('.recommendation-points').textContent.includes('Recommendation points') && document.querySelector('.recommendation-points').textContent.includes('Contribution points')"));
  check("Mine offers navigation, with no visible recommendation buttons", await app.evaluate("![...document.querySelectorAll('[data-recommend]')].some(el => el.getClientRects().length) && !!document.querySelector('[data-recommend-filter=pending]')"));
  await app.evaluate("document.querySelector('[data-recommend-filter=pending]').click()");
  await until("!!document.querySelector('.recommendation-history-build')");
  await app.evaluate("document.querySelector('.recommendation-history-build footer a').click()");
  await until("document.querySelectorAll('[data-recommendation-id]').length > 0 && document.querySelectorAll('[data-recommendation-id]').length <= 6");
  check("History locates the exact recommended build", await app.evaluate("[...document.querySelectorAll('[data-recommendation-id]')].every(el => el.dataset.recommendationId === " + JSON.stringify(target.id) + ")"));
  const today = new Date().toISOString().slice(0, 10), tomorrow = new Date(Date.parse(today) + 86400000).toISOString().slice(0, 10);
  await app.evaluate("nav('/account/recommendations')");
  await until("recommendationHistory?.pending === 1 && !recommendationHistoryLoading");
  await fixture.day(tomorrow);
  await app.evaluate("recommendationPeriod({ day: " + JSON.stringify(today) + ", next_settlement: new Date(Date.now() + 1).toISOString() })");
  await until("recommendationHistory?.points === 1 && !recommendationHistoryLoading");
  check("The midnight timer returns history to builds available today", await app.evaluate("recommendationHistoryFilter === 'available' && recommendationHistory.builds.length === 1"));
  await app.evaluate("document.querySelector('.recommendation-history-build footer a').click()");
  await until("document.querySelector(" + JSON.stringify(selector) + ")?.getAttribute('aria-pressed') === 'false' && document.querySelector(" + JSON.stringify(selector) + ").closest('.recommendation-control').querySelector('b').textContent === '1.0'");
  check("UTC settlement enables the next day's recommendation", (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes")).n === 1);
  await app.evaluate("document.querySelector('[data-recommend-clear]').click(); finder.query.orderBy = [{ field: 'heat', direction: 'desc' }]; libraryProjectQuery(); renderBuildFinder()");
  await until("!recommendationLibrary.loading && document.querySelector('[data-recommendation-id]')?.dataset.recommendationId === " + JSON.stringify(target.id));
  check("Popularity sorting raises the recommended lower-scoring build", await app.evaluate("document.querySelector('[data-recommendation-id]').dataset.recommendationId === " + JSON.stringify(target.id)));
  await app.evaluate("nav('/account/recommendations')");
  await until("recommendationHistory?.points === 1");
  check("A settled recommendation adds a permanent point", await app.evaluate("recommendationAccount.points === 1"));
  await app.send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
  check("Mine fits a phone", await app.evaluate("document.documentElement.scrollWidth <= innerWidth"));
  await app.evaluate("document.querySelector('.recommendation-history-build footer a').click()");
  await until("!!document.querySelector('[data-recommendation-id]:not([disabled])')");
  check("The Library fits a phone", await app.evaluate("document.documentElement.scrollWidth <= innerWidth && document.querySelector('.fd-main').scrollWidth <= document.querySelector('.fd-main').clientWidth + 1"));
  check("Popularity sorting is offered", await app.evaluate("LIBRARY_SORTS.includes('heat') && LIBRARY_ORDER_FIELDS.some(([id]) => id === 'heat')"));
  await mkdir(new URL("../target/recommendations-review/", import.meta.url), { recursive: true });
  const shot = await app.send("Page.captureScreenshot", { format: "png" });
  await writeFile(new URL("../target/recommendations-review/library-mobile.png", import.meta.url), Buffer.from(shot.result.data, "base64"));
  await app.evaluate("openLibraryBuild(recommendationLibrary.presets.get(" + JSON.stringify(target.preset) + "))");
  await until("!!document.querySelector(" + JSON.stringify(presetVote) + ")");
  await app.evaluate("document.querySelector(" + JSON.stringify(presetVote) + ").scrollIntoView({ block: 'center' }); document.querySelector('#preset-bar-builder-builds [data-preset-info]').click(); document.querySelector(" + JSON.stringify(infoVote) + ").scrollIntoView({ block: 'center' })");
  check("The preset bar and recommendation information fit a phone", await app.evaluate("document.documentElement.scrollWidth <= innerWidth && document.querySelector('#preset-info-popover').getBoundingClientRect().right <= innerWidth + 1"));
  const presetShot = await app.send("Page.captureScreenshot", { format: "png" });
  await writeFile(new URL("../target/recommendations-review/preset-mobile.png", import.meta.url), Buffer.from(presetShot.result.data, "base64"));
  await app.evaluate("closePopovers(); accountState.account = null; renderAccountEntry(); recommendationOwner(); refreshLibraryRecommendations()");
  const before = (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes")).n;
  await app.evaluate("document.querySelector(" + JSON.stringify(presetVote) + ").click()");
  await until("location.pathname === '/login'");
  check("Signed-out recommendations lead to registration or sign-in", (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes")).n === before);
  const back = await app.evaluate("new URLSearchParams(location.search).get('return')");
  await app.load(back, 4000);
  await until("!sessionStorage.getItem(RECOMMENDATION_INTENT) && recommendationAccount.pending.has(" + JSON.stringify(target.id) + ")");
  check("Sign-in returns from a preset to the exact Library build and resumes its recommendation", await app.evaluate("libraryActive && recommendationRequested() === " + JSON.stringify(target.id)) && (await fixture.env.ACCOUNTS.get("SELECT count(*) AS n FROM record_recommendation_votes WHERE day = recommendation_day()")).n === 1);
  const points = await app.evaluate("JSON.parse(sessionStorage.getItem('recommendation-test-points'))");
  for (const surface of ['library', 'preset', 'preset_info']) {
    check(surface + " records visible, ready and clicked controls", ['view', 'ready', 'click'].every(event => points.some(p => p.event === 'recommendation.' + event && (p.subject === surface || p.subject.startsWith(surface + '_')))));
  }
  check("Sign-in departure and return retain their preset entry point", ['preset_start', 'preset_return'].every(subject => points.some(p => p.event === 'recommendation.signin' && p.subject === subject)));
  check("Recommendation failures name only a fixed category", points.some(p => p.event === 'recommendation.error' && p.subject === 'preset_info_submit_rate_limited'));
  check("Every recommendation point is accepted without account or build identifiers", points.every(p => usagePoint({ v: USAGE_SCHEMA, cid: 'a'.repeat(32), e: p.event, subject: p.subject, n: p.n, route: 'weapon', lang: 'en', release: 'test', shell: 'web' }) && !p.subject.includes(target.id) && !p.subject.includes(fixture.account)));
  console.log("\nRecommendation page checks passed");
} finally {
  await app.close(); await fixture.close();
  if (staticServer) await new Promise(resolve => staticServer.close(resolve));
}
