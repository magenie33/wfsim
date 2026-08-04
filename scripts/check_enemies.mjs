// The ELEVENTH check: the fight has a TARGET, and the target is a unit —
// a picture that actually loads, a wiki page that actually exists, and a
// statement of what the sim does not model about it.
//
// Three failure modes it exists for:
//   1. Art that ships in `data/` but not in `site/img/` — the enemy portrait
//      is declared in the enemy's own YAML (`image:`), NOT in assets.yaml,
//      so it rides a different path to the build than every mod card.
//   2. A wiki link built from the DISPLAY name. In Chinese the target reads
//      堕落重型机枪手, and a wiki URL built from that lands on nothing. The
//      link must come from the English name in EVERY language, which is why
//      the whole check runs twice.
//   3. A silent modelling gap. An Acolyte carries damage attenuation whose
//      constants DE has never published, so the number this app gives against
//      one is too HIGH — and a caveat nobody can see is a wrong number.
//   4. A vulnerability column that never arrives. The Thrax's Void x1.5 rides
//      a FactionDamageOverride, which spent this whole project being parsed
//      and dropped; the column is now what decides which elements a build
//      wants, so the card has to say it and the api has to send it.
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, resolve } from "node:path";

const ROOT = resolve("site");
const MIME = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css",
  ".json": "application/json", ".wasm": "application/wasm", ".svg": "image/svg+xml",
  ".png": "image/png", ".jpg": "image/jpeg", ".ico": "image/x-icon" };
const srv = createServer(async (q, s) => {
  const p = decodeURIComponent(q.url.split("?")[0]);
  try {
    const b = await readFile(join(ROOT, p));
    s.writeHead(200, { "content-type": MIME[extname(p)] || "application/octet-stream", "cache-control": "no-store" });
    s.end(b);
  } catch {
    s.writeHead(200, { "content-type": "text/html" });
    s.end(await readFile(join(ROOT, "index.html")));
  }
});
await new Promise((r) => srv.listen(0, "127.0.0.1", r));
const BASE = `http://127.0.0.1:${srv.address().port}`;
const PORT = 9491;
const proc = spawn("C:/Program Files/Google/Chrome/Application/chrome.exe",
  [`--remote-debugging-port=${PORT}`, "--headless=new", "--disable-gpu", "--no-first-run",
    `--user-data-dir=${process.env.TEMP}/wfsim-enemies-check`, "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function cdp(path) {
  for (let i = 0; i < 60; i++) {
    try { const r = await fetch(`http://127.0.0.1:${PORT}${path}`); if (r.ok) return r.json(); } catch {}
    await sleep(250);
  }
  throw new Error("no CDP");
}
const page = (await cdp("/json/list")).find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
};
await new Promise((r) => (ws.onopen = r));
const send = (method, params = {}) => new Promise((res) => {
  const i = ++id; pending.set(i, res);
  ws.send(JSON.stringify({ id: i, method, params }));
});
const evaluate = async (expr) => {
  const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(String(r.result.exceptionDetails.exception?.description || "").slice(0, 500));
  return r.result?.result?.value;
};
await send("Page.enable"); await send("Runtime.enable");

let failures = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || detail === undefined ? "" : `  — ${detail}`}`);
  if (!ok) failures++;
};

// One pass per language. Everything asserted here has to hold in both: the
// art is language-independent, and the wiki link must be.
for (const lang of ["en", "zh"]) {
  await send("Page.navigate", { url: BASE });
  await sleep(lang === "en" ? 12000 : 4000);
  await evaluate(`localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)})`);
  await send("Page.navigate", { url: BASE });
  await sleep(12000);

  const r = await evaluate(`(async () => {
    const sleep = (ms) => new Promise(r => setTimeout(r, ms));
    history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(2800);
    // THE FIGHT IS READ-ONLY ON ARRIVAL (2026-08-05): the official benchmark is
    // now the default scenario, and its controls are locked — so the enemy
    // picker's own button is disabled and cannot be opened. Copying it is the
    // real user flow for changing the target, and it is what this check needs
    // before it can ask anything about the picker.
    if (typeof officialScenarioActive === 'function' && officialScenarioActive()) {
      copyActiveScenario(); await sleep(1200);
    }


    // Every portrait in the roster, fetched the way the page asks for it.
    const roster = (META.enemies || []).map(e => ({ id: e.id, name: e.name, name_en: e.name_en,
      image: e.image, unmodeled: e.unmodeled || [], mods: e.type_modifiers || [] }));
    const art = [];
    for (const e of roster) {
      if (!e.image) { art.push([e.id, 'no image declared']); continue; }
      const ok = await new Promise(res => {
        const i = new Image();
        i.onload = () => res(i.naturalWidth > 0);
        i.onerror = () => res(false);
        i.src = '/img/' + encodeURIComponent(e.image);
      });
      if (!ok) art.push([e.id, e.image]);
    }

    // The card, for a target that HAS a caveat and one that does not.
    const cardFor = async (id) => {
      sim.enemy = id; renderSim(); await sleep(700);
      const host = document.getElementById('sim-target');
      const img = host.querySelector('.en-img');
      const link = host.querySelector('.en-wiki');
      return {
        name: (host.querySelector('.en-name') || {}).textContent || '',
        imgSrc: img ? img.getAttribute('src') : null,
        imgShown: !!img && img.naturalWidth > 0,
        href: link ? link.getAttribute('href') : null,
        gap: (host.querySelector('.en-gap') || {}).textContent || '',
        vuln: [...host.querySelectorAll('.en-vuln span')]
          .map(e => e.className + ':' + e.textContent.trim()),
        arenaImg: document.getElementById('arena-eimg').hidden
          ? null : document.getElementById('arena-eimg').getAttribute('src'),
        arenaDot: document.getElementById('arena-edot').hidden,
      };
    };
    const acolyte = await cardFor('angst');
    const gunner = await cardFor('corrupted_heavy_gunner');

    // The picker lists them all, with their pictures.
    document.getElementById('sim-target-pick').click(); await sleep(500);
    const rows = [...document.querySelectorAll('#enemy-menu .opt')];
    const menu = { rows: rows.length, thumbs: rows.filter(o => o.querySelector('.en-thumb')).length };
    closePopovers();

    return { roster, art, acolyte, gunner, menu, lang: LANG };
  })()`);

  const tag = `[${lang}]`;
  check(`${tag} the app is in ${lang}`, r.lang === lang, r.lang);
  check(`${tag} every target declares a portrait and it LOADS`, r.art.length === 0,
    JSON.stringify(r.art));
  check(`${tag} the roster is the whole data/enemies/ library`, r.roster.length >= 8,
    `${r.roster.length} targets`);

  for (const [who, card] of [["acolyte", r.acolyte], ["gunner", r.gunner]]) {
    check(`${tag} ${who}: the card shows the portrait`, card.imgShown, card.imgSrc);
    check(`${tag} ${who}: the arena shows it too, instead of the dot`,
      !!card.arenaImg && card.arenaDot, `${card.arenaImg} / dot hidden ${card.arenaDot}`);
    // The whole reason this runs in zh: the label is localized, the URL is not.
    const want = who === "acolyte" ? "Angst" : "Corrupted_Heavy_Gunner";
    check(`${tag} ${who}: the wiki link is the ENGLISH page`,
      card.href === `https://wiki.warframe.com/w/${want}`, card.href);
  }
  check(`${tag} the acolyte states what is not modeled`, /⚠/.test(r.acolyte.gap), r.acolyte.gap);
  // The column, end to end: data file -> override resolution -> api -> card.
  const thrax = r.roster.find((e) => e.id === "thrax_centurion");
  check(`${tag} the Thrax's OVERRIDE column arrives (Void ×1.5)`,
    thrax.mods.length === 1 && thrax.mods[0].type === "void" && thrax.mods[0].mult === 1.5,
    JSON.stringify(thrax.mods));
  check(`${tag} the gunner shows what to bring BEFORE what to avoid`,
    r.gunner.vuln.length === 3 && r.gunner.vuln[2].startsWith("dn")
      && r.gunner.vuln.slice(0, 2).every((v) => v.startsWith("up")),
    JSON.stringify(r.gunner.vuln));
  // Neutral is NOTHING on screen, not an empty label.
  check(`${tag} a neutral unit claims no vulnerability`, r.acolyte.vuln.length === 0,
    JSON.stringify(r.acolyte.vuln));
  check(`${tag} the gunner claims no caveat it does not have`, r.gunner.gap === "", r.gunner.gap);
  check(`${tag} the picker lists every target, each with its picture`,
    r.menu.rows === r.roster.length && r.menu.thumbs === r.menu.rows, JSON.stringify(r.menu));
  // Localized names must actually arrive — otherwise "the link is English" is
  // passing for the boring reason that everything is.
  if (lang === "zh") {
    check(`${tag} the target is NAMED in Chinese`, /[\u4e00-\u9fff]/.test(r.acolyte.name),
      r.acolyte.name);
  }
}

ws.close(); proc.kill(); srv.close();
console.log(failures ? `\n${failures} FAILED` : "\nall good");
process.exit(failures ? 1 : 0);
