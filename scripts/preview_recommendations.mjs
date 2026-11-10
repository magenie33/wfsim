import { recommendationFixture } from "./recommendations_fixture.mjs";
import { openApp } from "./cdp.mjs";

const fixture = await recommendationFixture(process.env.WFSIM_BASE || "http://127.0.0.1:8908", Number(process.env.WFSIM_PORT || 8907), true);
const app = await openApp({ base: fixture.base, lang: "zh", boot: 4000 });
try {
  for (const name of ["Furis", "Laetum", "Torid"]) {
    await app.load("/weapons/" + name, 4000);
    await app.evaluate("gainPrefs.on = false; restartCalc()");
    for (let n = 0; n < 100; n++) {
      if (await app.evaluate("!!recommendationLibrary?.heat && !recommendationLibrary.loading && [...document.querySelectorAll('[data-recommendation-id]')].some(el => el.dataset.recommendationId)")) break;
      await app.sleep(250);
    }
    const builds = await app.evaluate("builtinBuilds(finder.b).filter(p => recommendationId(p)).map(p => ({ identity: recommendationId(p), record: recommendationSource(p), ruler: p.benchmark, mode: p.mode }))");
    for (const build of builds) await fixture.seed(build.identity, build.record, build.ruler, build.mode);
    const first = builds[0];
    if (first) {
      const day = new Date().toISOString().slice(0, 10);
      const result = await fetch(fixture.base + "/api/recommendations", { method: "POST", headers: { "content-type": "application/json", origin: fixture.base, cookie: "wfsim_session=" + fixture.token },
        body: JSON.stringify({ identity: first.identity, day, ruler: first.ruler, mode: first.mode, action: "recommend" }) });
      if (!result.ok) throw new Error(await result.text());
      if (name !== "Torid") await fixture.env.ACCOUNTS.run("DELETE FROM record_recommendation_votes WHERE account = ? AND identity = ?", fixture.account, first.identity);
      for (const age of name === "Furis" ? [1, 2] : name === "Laetum" ? [12] : []) {
        const voteDay = new Date(Date.parse(day) - age * 86400000).toISOString().slice(0, 10);
        await fixture.env.ACCOUNTS.run("INSERT INTO record_recommendation_votes (account, identity, day) VALUES (?, ?, ?::date)", fixture.account, first.identity, voteDay);
      }
    }
    console.log(name + ": " + builds.length + " reviewable builds");
  }
} finally { await app.close(); }
console.log("Review: " + fixture.base + "/__fixture/login");
console.log("History: " + fixture.base + "/account/recommendations");
const stop = async () => { await fixture.close(); process.exit(0); };
process.on("SIGINT", stop); process.on("SIGTERM", stop);
