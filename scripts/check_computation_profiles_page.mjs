import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { openApp, serveSite } from "./cdp.mjs";
import { computationFixture } from "./computation_fixture.mjs";

const staticServer = process.env.WFSIM_BASE ? null : await serveSite(new URL("../site/", import.meta.url).pathname);
const fixture = await computationFixture(process.env.WFSIM_BASE || "http://127.0.0.1:" + staticServer.address().port);
const app = await openApp({ base: fixture.base, lang: "zh", boot: 1500 });
const held = new Map(), blocked = new Set(["summary", "records", "full"]), requested = new Map(), interceptionErrors = [];
await app.send("Fetch.enable", { patterns: [{ urlPattern: "*/api/account/contribution-profile*" }, { urlPattern: "*/api/contributors/profile/*" }] });
app.on("Fetch.requestPaused", event => {
  const view = new URL(event.request.url).searchParams.get("view") || "full";
  requested.set(view, (requested.get(view) || 0) + 1);
  if (blocked.has(view)) { if (!held.has(view)) held.set(view, []); held.get(view).push(event.requestId); }
  else app.send("Fetch.continueRequest", { requestId: event.requestId }).catch(error => interceptionErrors.push(String(error)));
});
async function release(view) { blocked.delete(view); for (const requestId of held.get(view) || []) await app.send("Fetch.continueRequest", { requestId }); held.delete(view); }
let passed = 0;
const check = (name, value, detail = "") => { assert.ok(value, name + (detail ? "\n" + detail : "")); passed++; console.log("  ok  " + name); };
async function until(expression) {
  for (let n = 0; n < 80; n++) { if (await app.evaluate(expression)) return; await app.sleep(200); }
  throw new Error("Timed out: " + expression + "\n" + JSON.stringify(await app.evaluate("({url:location.href,text:document.querySelector('#auth-page')?.innerText.slice(0,1500),library:recommendationLibrary&&{error:recommendationLibrary.error,loading:recommendationLibrary.loading},names:[...computationRows.values()].slice(0,3).map(r=>({identity:r.identity,html:r.html,at:r.at})),nodes:document.querySelectorAll('[data-computation]').length,selected:recommendationRequested()})")));
}
try {
  await app.load("/people/me?weapon=laetum#profile", 600);
  await until("!!document.querySelector('.profile-heading h2')");
  check("Legacy profile addresses preserve query and fragment on the canonical user route", await app.evaluate("location.pathname === '/user/me' && location.search === '?weapon=laetum' && location.hash === '#profile'"), await app.evaluate("location.href"));
  check("Identity appears while contribution totals are withheld", await app.evaluate("document.querySelector('.profile-heading h2').innerText.includes('aaa') && computationProfile.statsLoading && computationProfile.value.profile.points == null"));
  check("Opening a user profile does not fetch record lists", !requested.get("records") && !requested.get("full"));
  await release("summary");
  await until("computationProfile?.value && !computationProfile.statsLoading");
  await app.evaluate("document.querySelector('.profile-tabs a[href*=board]').click()");
  await until("computationProfile?.recordsLoading");
  check("The user card stays visible while records are withheld", await app.evaluate("document.querySelector('.profile-heading h2').innerText.includes('aaa') && document.querySelectorAll('.profile-builds .profile-build').length === 0"));
  await release("records"); await release("full");
  await until("computationProfile?.value && !computationProfile.loading && !computationProfile.statsLoading && !computationProfile.recordsLoading");
  check("Own profile lists a real currently published build", await app.evaluate("document.querySelectorAll('.profile-build').length === 1"));
  check("Lifetime signatures and points are distinct", await app.evaluate("document.querySelector('.profile-heading').innerText.includes('128') && document.querySelector('.profile-heading').innerText.includes('128,640')"));
  check("Names link to stable contributor profiles", await app.evaluate("document.querySelectorAll('.computation-credits a[href^=\"/user/\"]').length === 2"));
  check("User names share an un-underlined link style", await app.evaluate("[...document.querySelectorAll('.computation-credits a')].every(a => a.classList.contains('user-link') && getComputedStyle(a).textDecorationLine === 'none')"));
  const point = await app.evaluate("(() => { const a = document.querySelector('.computation-credits a'); a.scrollIntoView({block:'center'}); const r=a.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()");
  await app.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
  await until("!!document.querySelector('.computation-credits a')");
  check("Hovered user names keep the same un-underlined treatment", await app.evaluate("getComputedStyle(document.querySelector('.computation-credits a')).textDecorationLine === 'none'"));
  await app.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Tab", code: "Tab", windowsVirtualKeyCode: 9 });
  await app.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Tab", code: "Tab", windowsVirtualKeyCode: 9 });
  check("Keyboard user links retain a visible focus ring", await app.evaluate("(() => { const a=document.querySelector('.computation-credits a'); a.focus(); return a.matches(':focus-visible') && getComputedStyle(a).outlineStyle === 'solid'; })()"));
  check("The build link includes the engine's canonical identity", await app.evaluate("document.querySelector('.profile-build h3 a').search.includes('computed_build=" + fixture.identity + "')"));
  await app.evaluate("document.querySelector('.profile-tabs a[href*=optimization]').click()");
  await until("computationProfile?.value?.kind === 'optimization' && !computationProfile.loading");
  check("Optimization tab shows the fourteen day rule", await app.evaluate("document.querySelector('.contributor-profile').innerText.includes('14') && document.querySelectorAll('.profile-build').length === 1"));
  await app.evaluate("document.querySelector('.profile-build h3 a').click()");
  await until("!!document.querySelector('.computation-record')");
  check("Calculation provenance is directly addressable", await app.evaluate("document.querySelector('.computation-record').innerText.includes('123456789abc') && document.querySelector('.computation-record').innerText.includes('bbb')"));
  await fixture.env.ACCOUNTS.run("UPDATE accounts SET display_name='aaa_new' WHERE id=?", fixture.account);
  await app.load("/user/" + fixture.profileIds.aaa + "?kind=board", 600);
  await until("computationProfile?.value && !computationProfile.loading && !computationProfile.statsLoading && !computationProfile.recordsLoading");
  check("Renaming updates both the profile and existing attribution", await app.evaluate("document.querySelector('.profile-heading').innerText.includes('aaa_new') && document.querySelector('.computation-credits').innerText.includes('aaa_new')"));
  fixture.hideBoard(true);
  await app.evaluate("document.querySelector('[data-computation-retry]').click()");
  await until("computationProfile?.value && !computationProfile.loading && !computationProfile.statsLoading && !computationProfile.recordsLoading");
  check("A row absent from publication disappears even while its score remains stored", await app.evaluate("!document.querySelector('.profile-build') && computationProfile.value.profile.signatures === 128"));
  fixture.hideBoard(false);
  await fixture.env.ACCOUNTS.run("UPDATE contribution_choice SET named=0 WHERE account=?", fixture.account);
  await app.load("/user/me?kind=board", 600);
  await until("computationProfile?.value && !computationProfile.loading && !computationProfile.statsLoading && !computationProfile.recordsLoading");
  check("Anonymous owners retain their current builds and lifetime credit", await app.evaluate("computationProfile.value.profile.named === false && computationProfile.value.profile.signatures === 128 && document.querySelectorAll('.profile-build').length === 1"));
  check("Anonymous calculations expose no link to their owner's profile", await app.evaluate("!document.querySelector('.computation-credits a[href=\"/user/" + fixture.profileIds.aaa + "\"]')"));
  await fixture.env.ACCOUNTS.run("UPDATE contribution_choice SET named=1 WHERE account=?", fixture.account);
  await app.load("/weapons/Laetum?view=library&bench=" + fixture.row.benchmark + "&computed_build=" + fixture.identity, 2000);
  await until("!!document.querySelector('[data-computation] a[href=\"/user/" + fixture.profileIds.aaa + "\"]')");
  check("The actual build library shows the current calculator names", await app.evaluate("[...document.querySelectorAll('[data-computation]')].some(node => node.innerText.includes('aaa_new') && node.innerText.includes('bbb'))"));
  await fixture.env.ACCOUNTS.run("UPDATE accounts SET display_name='aaa_again' WHERE id=?", fixture.account);
  await app.evaluate("window.dispatchEvent(new Event('focus'))");
  await until("[...document.querySelectorAll('[data-computation]')].some(node => node.innerText.includes('aaa_again'))");
  check("Returning to a page resolves the contributor's new name", await app.evaluate("![...document.querySelectorAll('[data-computation]')].some(node => node.innerText.includes('aaa_new'))"));
  if (process.argv.includes("--negative-control")) fixture.hideBoard(true);
  await app.load("/user/me?kind=board", 600);
  await until("computationProfile?.value && !computationProfile.loading && !computationProfile.statsLoading && !computationProfile.recordsLoading");
  check("Restored publication restores the current signature", await app.evaluate("document.querySelectorAll('.profile-build').length === 1"));
  await app.send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
  await app.sleep(300);
  check("The profile fits a phone viewport", await app.evaluate("document.documentElement.scrollWidth <= innerWidth + 1"));
  if (process.env.WFSIM_REVIEW_DIR) {
    await mkdir(process.env.WFSIM_REVIEW_DIR, { recursive: true });
    const shot = await app.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true });
    await writeFile(process.env.WFSIM_REVIEW_DIR + "/profile-mobile.png", Buffer.from(shot.result.data, "base64"));
  }
  blocked.add("summary");
  const recordCalls = requested.get("records") || 0;
  await app.evaluate("(() => { const row=document.createElement('div'); row.id='profile-ranking-entry-check'; row.className='rank-row'; row.innerHTML=contributorRowInner({key:'profile-fixture',name:'bbb',profile:" + JSON.stringify(fixture.profileIds.bbb) + ",contributor_rank:computationProfile.value.profile.contributor_rank},1,1); document.querySelector('#auth-page').append(row); })()");
  check("The ranking renderer uses the shared canonical user link", await app.evaluate("(() => { const a=document.querySelector('#profile-ranking-entry-check a'); return a.getAttribute('href') === '/user/" + fixture.profileIds.bbb + "' && a.classList.contains('user-link') && getComputedStyle(a).textDecorationLine === 'none'; })()"));
  await app.evaluate("document.querySelector('#profile-ranking-entry-check a').click()");
  await until("document.querySelector('.profile-heading h2')?.innerText.includes('bbb')");
  check("Opening another user from a ranking row shows identity without loading their record lists", (requested.get("records") || 0) === recordCalls
    && await app.evaluate("location.pathname === '/user/" + fixture.profileIds.bbb + "' && computationProfile.statsLoading && !computationProfile.value.profile.you"));
  await release("summary");
  await until("!computationProfile.statsLoading");
  check("Profile request interception completed cleanly", interceptionErrors.length === 0);
  console.log(`\n${passed} computation profile page checks passed`);
} finally {
  for (const view of [...blocked]) await release(view);
  await app.send("Fetch.disable"); await app.close(); await fixture.close(); staticServer?.closeAllConnections(); staticServer?.close();
}
