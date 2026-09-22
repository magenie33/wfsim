// SPDX-License-Identifier: AGPL-3.0-or-later
// A DOCUMENT CARRIES ITS OWN PAGE AND NOBODY ELSE'S.
//
// Four routes have a `<main>` in the shell. A document that shipped all four
// is one of four hundred that differ by a stat line, which is one document to
// anything that reads them — so the build leaves each one the body it is
// about and puts the rest in a file the app fetches. Both halves fail
// SILENTLY: keeping all four is merely large, and a page whose body was taken
// and never fetched is merely blank. Neither throws.
//
//   node scripts/check_own_body.mjs
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { openApp } from "./cdp.mjs";

const SITE = resolve(dirname(fileURLToPath(import.meta.url)), "..", "site");
const PAGES = { "/download": "download-page", "/support": "support-page",
  "/thanks": "thanks-page", "/benchmark": "bench-page" };

/// What sits between `<main id="x" …>` and its own `</main>`, depth-counted —
/// `<main>` nests in this document and the first close is not its own.
const bodyOf = (html, id) => {
  const m = html.match(new RegExp(`<main id="${id}"[^>]*>`));
  if (!m) return null;
  let depth = 0;
  const from = m.index + m[0].length;
  for (const t of html.slice(m.index).matchAll(/<\/?main\b/g)) {
    depth += t[0] === "<main" ? 1 : -1;
    if (depth === 0) return html.slice(from, m.index + t.index).trim();
  }
  return null;
};

const page = (p) => readFileSync(resolve(SITE, p), "utf8");
let failures = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || detail === undefined ? "" : `  — ${detail}`}`);
  if (!ok) failures += 1;
};

// A WEAPON PAGE IS THE ORDINARY CASE: it is none of the four, so it keeps none.
const weapon = page("weapons/Torid/index.html");
for (const [route, id] of Object.entries(PAGES)) {
  check(`a weapon page ships no ${route} body`, bodyOf(weapon, id) === "",
    `${(bodyOf(weapon, id) || "").length} chars of it`);
}

// …AND EACH OF THE FOUR KEEPS ITS OWN, because that document IS that page and
// a reader served it with an empty main would see nothing at all.
for (const [route, id] of Object.entries(PAGES)) {
  if (route === "/benchmark") continue;   // no prerendered page; the shell serves it
  const own = page(`${route.slice(1)}/index.html`);
  check(`${route} ships its own body`, (bodyOf(own, id) || "").length > 100,
    `${(bodyOf(own, id) || "").length} chars`);
  for (const [other, oid] of Object.entries(PAGES)) {
    if (oid === id) continue;
    check(`${route} ships no ${other} body`, bodyOf(own, oid) === "");
  }
}

// THE OTHER HALF: the app fetches what the document did not carry. Asserted in
// a browser because nothing else can — the file is named by the build and the
// injection happens on a route.
const app = await openApp({ boot: 14000 });
await app.load("/weapons/Torid", 14000);
const got = await app.evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  history.pushState({}, '', '/support'); route(); await sleep(2500);
  const s = document.getElementById('support-page');
  return { kids: s ? s.childElementCount : -1, text: (s ? s.textContent : '').trim().length };
})()`);
check("routing to /support brings its body with it", got.kids > 0 && got.text > 200,
  JSON.stringify(got));
app.failures += failures;
await app.finish("a document carries its own page");
