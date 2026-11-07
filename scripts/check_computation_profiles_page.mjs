import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { openApp, serveSite } from "./cdp.mjs";
import { computationFixture } from "./computation_fixture.mjs";

const staticServer = process.env.WFSIM_BASE ? null : await serveSite(new URL("../site/", import.meta.url).pathname);
const fixture = await computationFixture(process.env.WFSIM_BASE || "http://127.0.0.1:" + staticServer.address().port);
const app = await openApp({ base: fixture.base, lang: "zh", boot: 1500 });
let passed = 0;
const check = (name, value) => { assert.ok(value, name); passed++; console.log("  ok  " + name); };
async function until(expression) {
  for (let n = 0; n < 80; n++) { if (await app.evaluate(expression)) return; await app.sleep(200); }
  throw new Error("Timed out: " + expression + "\n" + JSON.stringify(await app.evaluate("({url:location.href,text:document.querySelector('#auth-page')?.innerText.slice(0,1500),library:recommendationLibrary&&{error:recommendationLibrary.error,loading:recommendationLibrary.loading},names:[...computationRows.values()].slice(0,3).map(r=>({identity:r.identity,html:r.html,at:r.at})),nodes:document.querySelectorAll('[data-computation]').length,selected:recommendationRequested()})")));
}
try {
  await app.load("/people/me", 1500);
  await until("computationProfile?.value && !computationProfile.loading");
  check("Own profile lists a real currently published build", await app.evaluate("document.querySelectorAll('.profile-build').length === 1"));
  check("Lifetime signatures and points are distinct", await app.evaluate("document.querySelector('.profile-heading').innerText.includes('128') && document.querySelector('.profile-heading').innerText.includes('128,640')"));
  check("Names link to stable contributor profiles", await app.evaluate("document.querySelectorAll('.computation-credits a[href^=\"/people/\"]').length === 2"));
  check("The build link includes the engine's canonical identity", await app.evaluate("document.querySelector('.profile-build h3 a').search.includes('computed_build=" + fixture.identity + "')"));
  await app.evaluate("document.querySelector('.profile-tabs a[href*=optimization]').click()");
  await until("computationProfile?.value?.kind === 'optimization' && !computationProfile.loading");
  check("Optimization tab shows the fourteen day rule", await app.evaluate("document.querySelector('.contributor-profile').innerText.includes('14') && document.querySelectorAll('.profile-build').length === 1"));
  await app.evaluate("document.querySelector('.profile-build h3 a').click()");
  await until("!!document.querySelector('.computation-record')");
  check("Calculation provenance is directly addressable", await app.evaluate("document.querySelector('.computation-record').innerText.includes('123456789abc') && document.querySelector('.computation-record').innerText.includes('bbb')"));
  await fixture.env.ACCOUNTS.run("UPDATE accounts SET display_name='aaa_new' WHERE id=?", fixture.account);
  await app.load("/people/" + fixture.profileIds.aaa, 600);
  await until("computationProfile?.value && !computationProfile.loading");
  check("Renaming updates both the profile and existing attribution", await app.evaluate("document.querySelector('.profile-heading').innerText.includes('aaa_new') && document.querySelector('.computation-credits').innerText.includes('aaa_new')"));
  fixture.hideBoard(true);
  await app.evaluate("document.querySelector('[data-computation-retry]').click()");
  await until("computationProfile?.value && !computationProfile.loading");
  check("A row absent from publication disappears even while its score remains stored", await app.evaluate("!document.querySelector('.profile-build') && computationProfile.value.profile.signatures === 128"));
  fixture.hideBoard(false);
  await fixture.env.ACCOUNTS.run("UPDATE contribution_choice SET named=0 WHERE account=?", fixture.account);
  await app.load("/people/me", 600);
  await until("computationProfile?.value && !computationProfile.loading");
  check("Anonymous owners retain their current builds and lifetime credit", await app.evaluate("computationProfile.value.profile.named === false && computationProfile.value.profile.signatures === 128 && document.querySelectorAll('.profile-build').length === 1"));
  check("Anonymous calculations expose no link to their owner's profile", await app.evaluate("!document.querySelector('.computation-credits a[href=\"/people/" + fixture.profileIds.aaa + "\"]')"));
  await fixture.env.ACCOUNTS.run("UPDATE contribution_choice SET named=1 WHERE account=?", fixture.account);
  await app.load("/weapons/Laetum?view=library&bench=" + fixture.row.benchmark + "&computed_build=" + fixture.identity, 2000);
  await until("!!document.querySelector('[data-computation] a[href=\"/people/" + fixture.profileIds.aaa + "\"]')");
  check("The actual build library shows the current calculator names", await app.evaluate("[...document.querySelectorAll('[data-computation]')].some(node => node.innerText.includes('aaa_new') && node.innerText.includes('bbb'))"));
  await fixture.env.ACCOUNTS.run("UPDATE accounts SET display_name='aaa_again' WHERE id=?", fixture.account);
  await app.evaluate("window.dispatchEvent(new Event('focus'))");
  await until("[...document.querySelectorAll('[data-computation]')].some(node => node.innerText.includes('aaa_again'))");
  check("Returning to a page resolves the contributor's new name", await app.evaluate("![...document.querySelectorAll('[data-computation]')].some(node => node.innerText.includes('aaa_new'))"));
  if (process.argv.includes("--negative-control")) fixture.hideBoard(true);
  await app.load("/people/me", 600);
  await until("computationProfile?.value && !computationProfile.loading");
  check("Restored publication restores the current signature", await app.evaluate("document.querySelectorAll('.profile-build').length === 1"));
  await app.send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
  await app.sleep(300);
  check("The profile fits a phone viewport", await app.evaluate("document.documentElement.scrollWidth <= innerWidth + 1"));
  if (process.env.WFSIM_REVIEW_DIR) {
    await mkdir(process.env.WFSIM_REVIEW_DIR, { recursive: true });
    const shot = await app.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true });
    await writeFile(process.env.WFSIM_REVIEW_DIR + "/profile-mobile.png", Buffer.from(shot.result.data, "base64"));
  }
  console.log(`\n${passed} computation profile page checks passed`);
} finally { await app.close(); await fixture.close(); staticServer?.closeAllConnections(); staticServer?.close(); }
