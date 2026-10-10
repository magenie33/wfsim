// SPDX-License-Identifier: AGPL-3.0-or-later
// A TITLE SAYS WHAT THE PAGE IS, IN BOTH LANGUAGES ITS READERS SEARCH IN.
//
// The served title is the one a search result and a pasted link show, and the
// app keeps it on the page it was served for. Every half of that fails
// SILENTLY: an English-only title still renders, a stray placeholder <h1> is
// invisible, and a title the app overwrites on boot is only seen by a crawler.
// docs/UI.md §Page titles.
//
//   node scripts/check_page_titles.mjs
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { openApp } from "./cdp.mjs";

const SITE = resolve(dirname(fileURLToPath(import.meta.url)), "..", "site");
const page = (p) => readFileSync(resolve(SITE, p), "utf8");
const titleOf = (html) => ((html.match(/<title>([^<]*)<\/title>/) || [])[1] || "")
  .replace(/&amp;/g, "&");
const HAN = /[一-鿿]/;
// The pages a search finds or a reader pastes, and the <h1> count each owes:
// /compute and /contributors draw theirs in the app.
const SERVED = { "index.html": 1, "weapons/index.html": 1, "weapons/Torid/index.html": 1,
  "warframes/Valkyr/index.html": 1, "companions/Prototype_Companion/index.html": 1,
  "support/index.html": 1, "download/index.html": 1, "operator/index.html": 1,
  "utility/fissures/index.html": 1, "utility/arbitrations/index.html": 1,
  "benchmark/index.html": 1, "compute/index.html": 0, "contributors/index.html": 0 };

const app = await openApp({ boot: 14000 });
for (const [file, h1s] of Object.entries(SERVED)) {
  const html = page(file);
  const t = titleOf(html);
  app.check(`${file} is titled in Chinese too`, HAN.test(t), t);
  app.check(`${file} names the brand`, t.includes("WFSim"), t);
  const og = (html.match(/<meta property="og:title" content="([^"]*)"/) || [])[1] || "";
  app.check(`${file} previews under the same title`, og.replace(/&amp;/g, "&") === t, og);
  const n = (html.match(/<h1\b/g) || []).length;
  app.check(`${file} has ${h1s} <h1>`, n === h1s, `${n}`);
}
app.check("a weapon's title carries its Chinese name", titleOf(page("weapons/Torid/index.html")).includes("托里德"));

// THE APP KEEPS THE SERVED TITLE, then takes the shape on every page after.
await app.load("/weapons/Torid", 14000);
const got = await app.evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const first = document.title;
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(1500);
  const sim = document.title;
  history.pushState({}, '', '/compute'); route(); await sleep(1500);
  const compute = document.title;
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(1500);
  return { first, sim, compute, back: document.title };
})()`);
const served = titleOf(page("weapons/Torid/index.html"));
app.check("the served page keeps its served title", got.first === served, got.first);
app.check("a page reached in the app takes the shape", /^(Torid|托里德) · .+ — .+ \| WFSim$/.test(got.sim), got.sim);
app.check("an app page is titled, not blank", /^\S.* \| WFSim$/.test(got.compute), got.compute);
app.check("coming back is a page reached in the app", got.back !== served && got.back.endsWith(" | WFSim"), got.back);
await app.finish("every page is titled in both languages");
