// SPDX-License-Identifier: AGPL-3.0-or-later
// THE LONG IMAGE, DRAWN HERE — docs/AGENT.md §"The QQ bot". The card page's own
// drawing (`headlessLongCard`, 83-headless-card.js) run on the bot's engine and
// written as one document holding everything it shows: the site's style, every
// picture as data, the QR code. Chrome only rasterizes it (render.mjs
// `renderDocument`): it loads nothing, so a busy page or a slow border cannot
// keep a card from coming back.
import { readFileSync, readdirSync } from "node:fs";
import { resolve, dirname, join, extname } from "node:path";
import { fileURLToPath } from "node:url";
import { SITE } from "./engine.mjs";
import { simulateFleet } from "./fleet.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
/// THE SITE'S STYLE AND PICTURES, the copy shipped beside the bot (scripts/ship_bot.py).
const ASSET = join(ROOT, "site", "asset"), IMG = join(ROOT, "site", "img");
/// THE CHAT'S CLOCK: a card's times are printed where it is read.
const ZONE = { zh: "Asia/Shanghai", en: "UTC" };
const TYPE = { ".webp": "image/webp", ".png": "image/png", ".jpg": "image/jpeg", ".svg": "image/svg+xml" };

/// The one stylesheet the site ships; two would be a deploy that kept an old one.
let style = null;
export function siteStyle(dir = ASSET) {
  if (style && style.dir === dir) return style.css;
  const found = readdirSync(dir).filter((f) => /^style\.[0-9a-f]+\.css$/.test(f));
  if (found.length !== 1) throw new Error(`${dir} holds ${found.length} stylesheets, not one`);
  style = { dir, css: readFileSync(join(dir, found[0]), "utf8") };
  return style.css;
}

/// A PICTURE AS DATA, or its address as it was: one the copy lacks fails to load
/// the way it fails on the page, and the page hides it.
function inlined(src, img) {
  const m = /^\/img\/([^/?#]+)$/.exec(src);
  if (!m) return src;
  try {
    const name = decodeURIComponent(m[1]);
    return `data:${TYPE[extname(name).toLowerCase()] || "application/octet-stream"};base64,${readFileSync(join(img, name)).toString("base64")}`;
  } catch (_) {
    return src;
  }
}

const speakers = new Map();
const speaker = (engine, lang) => {
  if (!speakers.has(lang)) speakers.set(lang, engine.headless.headlessSpeaker(engine.meta, engine.i18n, lang, ZONE[lang] || "UTC"));
  return speakers.get(lang);
};

/// An appraisal as its door answers it, with the one build `result` names.
const appraisalAt = (engine) => (code, result) => fetch(`${engine.site}/api/appraise/${encodeURIComponent(code)}${
  result ? `?result=${result}` : ""}`, { signal: AbortSignal.timeout(30_000) }).then((r) => (r.ok ? r.json() : null));

/// THE CARD AT `url` (a `/weapons/<Name>/card?…` address): `{ html, verdict }`,
/// the whole document and, for an appraisal's answer, its replay's verdict
/// (`headlessAppraisalReplay`, the fight run on this machine's cores) — or null
/// for a card that is not a weapon's, which only the page draws.
/// `opts.css` and `opts.img` name the style and pictures (a check's own build),
/// `opts.appraisal(code, result)` the appraisal's door and `opts.now` the clock
/// a verdict is dated by.
export async function cardDocument(engine, url, { lang = "zh", css = siteStyle(), img = IMG, appraisal = appraisalAt(engine), now } = {}) {
  const h = engine.headless;
  const u = new URL(url);
  const path = /^(\/weapons\/[^/]+)\/card\/?$/.exec(u.pathname);
  if (!path) return null;
  const ask = h.headlessCardAsk(u.search);
  const wanted = decodeURIComponent(path[1]);
  const plain = (engine.meta.weapons || []).find((w) => decodeURIComponent(h.headlessWeaponPath(engine.meta.weapons, w.id)) === wanted);
  if (!plain) throw new Error(`no weapon at ${wanted}`);
  const say = speaker(engine, lang);
  const w = say.meta.weapons.find((x) => x.id === plain.id);
  const job = ask.kind === "appraise" ? await appraisal(ask.code, ask.result) : null;
  if (ask.kind === "appraise" && !job) throw new Error(`no appraisal ${ask.code}`);
  const rows = ask.kind === "appraise" && !(job && job.result) ? [] : (await engine.host.board(w.id)) || [];
  // THE NUMBER A CHAT IS TOLD is this replay's, never the searcher's.
  const verdict = job && job.result ? await h.headlessAppraisalReplay(say, w, job, rows, {
    check: async (b) => engine.api("/api/board/check", b), simulate: (body) => simulateFleet(engine.wasm, body), now }) : null;
  // A RIVEN'S STATS FROM THE ENGINE, whole: the page fills them in afterwards.
  const riven = (wp, spec) => h.headlessRivenAnswer(say, engine.api("/api/riven", { weapon: wp.id, ...spec }));
  const card = h.headlessLongCard(say, w, ask, { rows, job, verdict, origin: SITE, riven });
  const qr = engine.api("/api/qr", { text: card.link });
  // EVERY PICTURE EAGER, as the page sets them before its screenshot.
  const inside = card.draw(qr && qr.svg).replace(/ loading="lazy"/g, ' loading="eager"')
    .replace(/ src="([^"]*)"/g, (_, src) => ` src="${inlined(src, img)}"`);
  return { html: `<!doctype html><html lang="${lang}" data-theme="light"><head><meta charset="utf-8"><style>${css}</style></head>`
    + `<body class="on-card"><main id="card-page" class="card-page${card.wayIn ? " lc-way-in" : ""}">${inside}</main></body></html>`,
    verdict: verdict ? h.headlessVerdictRecord(verdict) : null };
}
