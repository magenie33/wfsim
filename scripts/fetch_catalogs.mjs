// Cache the wiki's PER-WEAPON CATALOGS locally, as wikitext.
//
// Some mechanics are a formula plus a published table with one ROW PER WEAPON,
// and the row says what the weapon's own stats never would (docs/CATALOGS.md).
// Every intake has to read them, and reading them meant a network round trip —
// so a weapon gets added without its row being checked, which is how the Boar's
// CO row became the Boar Prime's.
//
// THROUGH A BROWSER, because the wiki answers `curl` and `urllib` with a 403
// and its API with a "Please wait" interstitial: it is behind a bot challenge
// that a real browser passes and a script does not. This repo already owns a
// headless Chrome for exactly this class of problem (`cdp.mjs`), so the fetch
// uses it rather than inventing a way around the challenge.
//
//   node scripts/fetch_catalogs.mjs            fetch what is missing
//   node scripts/fetch_catalogs.mjs --force    re-fetch, and say what MOVED
//
// Cached as WIKITEXT (`?action=raw`), not rendered HTML: the table is a wiki
// table, `grep` finds a weapon's row in it, and a diff between two fetches is a
// diff of the CATALOG rather than of the site's markup.
//
// The cache lives under `vendor/`, which is gitignored — these are somebody
// else's pages and this script is the tracked half. A row that DECIDES
// something is transcribed into the weapon's own yaml and into
// docs/CATALOGS.md; the cache is where you go to find it.
import { openApp, sleep } from "./cdp.mjs";
import fs from "node:fs";
import path from "node:path";

const CACHE = path.join(process.cwd(), "vendor", "wiki");
const FORCE = process.argv.includes("--force");

// The catalogs, by the name docs/CATALOGS.md gives them.
//
// …plus the MECHANIC pages that carry a per-weapon table of their own without
// being catalogs in that document's sense. `Sniper Rifle` is one: the rule is
// prose, and the Minimum Combo and the zoom buffs are a table with one row per
// sniper (MECHANICS §7 §"THE SNIPER RIFLE"). Same reason to cache it — every
// sniper intake has to read that row, and a network round trip is how a row
// goes unread.
const PAGES = {
  condition_overload: "Condition_Overload_(Mechanic)",
  primary_compression: "Primary_Compression",
  sniper_rifle: "Sniper_Rifle",
};

fs.mkdirSync(CACHE, { recursive: true });
const todo = Object.entries(PAGES).filter(([name]) =>
  FORCE || !fs.existsSync(path.join(CACHE, `${name}.wiki`)));
for (const [name] of Object.entries(PAGES)) {
  if (!todo.some(([n]) => n === name)) {
    const f = path.join(CACHE, `${name}.wiki`);
    console.log(`  cached  ${name.padEnd(22)} ${fs.statSync(f).size} bytes`);
  }
}
if (!todo.length) {
  console.log(`\n${CACHE}`);
  process.exit(0);
}

// `base` points Chrome at the wiki instead of at `site/`, so no local server is
// started; `boot: 0` because there is no app to come up, only a page to load.
const app = await openApp({ base: "https://wiki.warframe.com", boot: 0 });
let moved = 0;
for (const [name, page] of todo) {
  await app.load(`/w/${page}?action=raw`, 0);
  // The challenge, when it appears, is a page that resolves and then replaces
  // itself. Poll for the wikitext rather than sleeping a guessed interval.
  let text = "";
  for (let i = 0; i < 40; i++) {
    await sleep(500);
    text = await app.evaluate("document.body ? document.body.innerText : ''");
    if (text && text.includes("{|")) break;
  }
  // A TABLE HAS TO BE IN IT. A challenge page, a redirect stub and an error
  // body are all strings, and caching one would put a silent hole where a
  // catalog is.
  if (!text || !text.includes("{|") || text.length < 2000) {
    console.log(`  FAILED  ${name.padEnd(22)} no table in ${text.length} chars — not cached`);
    app.failures++;
    continue;
  }
  const file = path.join(CACHE, `${name}.wiki`);
  const old = fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
  fs.writeFileSync(file, text.replace(/\r\n/g, "\n"), "utf8");
  if (old && old !== text.replace(/\r\n/g, "\n")) {
    moved++;
    console.log(`  MOVED   ${name.padEnd(22)} the catalog changed — re-read every row this repo transcribes`);
  } else {
    console.log(`  fetched ${name.padEnd(22)} ${text.length} chars`);
  }
}
console.log(`\n${CACHE}`);
await app.finish(moved ? "a catalog moved — check what it says now" : "catalogs cached");
