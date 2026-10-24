// THE BOT'S CARD IS THE PAGE'S CARD, pixel for pixel — docs/AGENT.md §"The QQ
// bot". The bot draws a weapon's long image itself (`bot/card.mjs`, the card
// page's own `headlessLongCard`) and Chrome only rasterizes the document; the
// page draws it on `/weapons/<Name>/card`. Each fixture is taken both ways in
// each language and the two PNGs must be the same bytes: a wrapper that drifts
// from index.html, a style or picture the document lost, a number the bot reads
// from other rows, each moves a pixel.
//
//   node scripts/check_card_document.mjs              serves site/ itself
//   WFSIM_BASE=http://127.0.0.1:<port> node scripts/check_card_document.mjs
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { CHROME, serveSite } from "./cdp.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
process.env.CHROME = CHROME;
const { renderCardWith, renderDocument } = await import("../bot/render.mjs");
const { loadEngine } = await import("../bot/engine.mjs");
const { cardDocument } = await import("../bot/card.mjs");

/// Every shape a chat asks for: a plain gun, riven builds, the analyst with and
/// without a filter, a melee, a weapon with modes, an Incarnon, a kitgun.
const FIXTURES = [
  "Torid/card?kind=pz",
  "Torid/card?kind=pz&rv=1",
  "Torid/card?kind=zk",
  "Torid/card?kind=zk&has=critical_chance&malus=any",
  "Laetum/card?kind=zk&n=5",
  "Nikana_Prime/card?kind=pz",
  "Haalvu/card?kind=pz",
  "Catchmoon/card?kind=pz",
];
/// A PAGE PICTURE THAT FAILED TO ARRIVE is hidden, not an error, so a page shot
/// that differs is taken again before it counts: the document's is fixed.
const PAGE_TRIES = 3;

const srv = process.env.WFSIM_BASE ? null : await serveSite(join(ROOT, "site"));
const BASE = process.env.WFSIM_BASE || `http://127.0.0.1:${srv.address().port}`;
const engine = await loadEngine({ site: BASE });
// THE STYLE THE PAGE UNDER TEST LINKS, so a dev server's unbuilt one is compared too.
const shell = await (await fetch(`${BASE}/`)).text();
const href = (/<link rel="stylesheet" href="([^"]+)"/.exec(shell) || [])[1];
if (!href) throw new Error(`no stylesheet linked from ${BASE}/`);
const css = await (await fetch(new URL(href, BASE))).text();
const img = join(ROOT, "site", "img");

const out = mkdtempSync(join(tmpdir(), "wfsim-card-document-"));
let failures = 0, compared = 0;
for (const f of FIXTURES) for (const lang of ["zh", "en"]) {
  const url = `${BASE}/weapons/${f}`;
  const doc = await renderDocument(await cardDocument(engine, url, { lang, css, img }));
  let page = null, same = false;
  for (let i = 0; i < PAGE_TRIES && !same; i++) {
    page = (await renderCardWith(url, { lang })).png;
    same = Buffer.compare(page, doc) === 0;
  }
  compared++;
  if (same) continue;
  failures++;
  const tag = `${f.replace(/[^a-z0-9]+/gi, "_")}_${lang}`;
  writeFileSync(join(out, `${tag}.page.png`), page);
  writeFileSync(join(out, `${tag}.bot.png`), doc);
  console.log(`  FAIL  ${f} (${lang}): the bot's picture is not the page's — ${out}/${tag}.{page,bot}.png`);
}
srv && srv.close();
console.log(failures ? `${failures} of ${compared} cards differ` : `ok  ${compared} cards, the bot's picture is the page's`);
process.exit(failures ? 1 : 0);
