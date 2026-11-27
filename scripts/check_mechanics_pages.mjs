// Verify /mechanics and /mechanics/zh static HTML, markdown twins, SEO/LLM
// metadata, and auto-linked weapon targets without requiring a full WASM build.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = resolve(ROOT, "target/mechanics-check");
const SITE = "https://wfsim.app";
const py = process.platform === "win32" ? "python" : "python3";
function pickRunner() {
  try {
    execFileSync(py, ["-c", "import yaml, PIL"], { stdio: "ignore" });
    return { cmd: py, prefix: [] };
  } catch {
    return { cmd: "uv", prefix: ["run", "--with", "pyyaml", "--with", "pillow", "python"] };
  }
}
const runner = pickRunner();

let failed = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok" : "FAIL"}  ${name}${ok ? "" : `  — ${String(detail).slice(0, 260)}`}`);
  if (!ok) failed++;
};

rmSync(OUT, { recursive: true, force: true });
execFileSync(runner.cmd, [...runner.prefix, resolve(ROOT, "scripts/mechanics_site.py"), "--out", OUT], {
  cwd: ROOT,
  stdio: "inherit",
});

// Ask build_site_app for the authoritative set of roster weapon paths.
const validPaths = new Set(JSON.parse(execFileSync(
  runner.cmd,
  [...runner.prefix, "-c",
    "import json, sys; sys.path.insert(0, 'scripts'); import build_site_app as b; " +
    "print(json.dumps(sorted({b.wiki_path(s) for s in b.roster()})))"],
  { cwd: ROOT, encoding: "utf8" },
)));

const slugs = ["beam-weapons", "magnetic-electricity"];
const routes = [
  "/mechanics",
  "/mechanics/zh",
  ...slugs.map((s) => `/mechanics/${s}`),
  ...slugs.map((s) => `/mechanics/zh/${s}`),
];

for (const route of routes) {
  const htmlPath = resolve(OUT, route.slice(1), "index.html");
  const mdPath = resolve(OUT, `${route.slice(1)}.md`);
  check(`${route}: HTML and .md twin exist`, existsSync(htmlPath) && existsSync(mdPath));
  if (!existsSync(htmlPath) || !existsSync(mdPath)) continue;

  const html = readFileSync(htmlPath, "utf8");
  const md = readFileSync(mdPath, "utf8");
  check(`${route}: canonical & alternate markdown link`,
    html.includes(`<link rel="canonical" href="${SITE}${route}">`) &&
    html.includes(`<link rel="alternate" type="text/markdown" href="${route}.md">`));

  const ogm = html.match(/<meta property="og:image" content="https:\/\/wfsim\.app\/(og\/mechanics[^"]*\.png)">/);
  check(`${route}: dedicated 1200x630 OG share card generated`,
    !!ogm && existsSync(resolve(OUT, ogm[1])) && html.includes('<meta name="twitter:card" content="summary_large_image">'),
    ogm ? ogm[1] : "no og:image card");

  const isZh = route.startsWith("/mechanics/zh");
  const enAlt = isZh ? route.replace("/mechanics/zh", "/mechanics") : route;
  const zhAlt = isZh ? route : route.replace("/mechanics", "/mechanics/zh");
  check(`${route}: bidirectional hreflang en & zh-Hans`,
    html.includes(`hreflang="en" href="${SITE}${enAlt}"`) &&
    html.includes(`hreflang="zh-Hans" href="${SITE}${zhAlt}"`));
  check(`${route}: unified site topbar and mobile menu shell`,
    html.includes('id="menu-toggle"') &&
    html.includes('id="topmenu"') &&
    html.includes('class="topnav"') &&
    html.includes('class="tnav sel"') &&
    html.includes(isZh ? ">军械库</a>" : ">Arsenal</a>") &&
    html.includes('id="community-toggle"') &&
    html.includes('id="theme-toggle"'));

  // Validate all JSON-LD blocks parse cleanly.
  const lds = [...html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)]
    .map((m) => JSON.parse(m[1]));
  const types = new Set(lds.map((x) => x["@type"]));
  if (route === "/mechanics" || route === "/mechanics/zh") {
    check(`${route}: CollectionPage JSON-LD`, types.has("CollectionPage"), [...types]);
  } else {
    check(`${route}: TechArticle + FAQPage JSON-LD`,
      types.has("TechArticle") && types.has("FAQPage"), [...types]);
  }

  // Every auto-linked weapon in HTML & markdown twin must exist in roster.
  const htmlLinks = [...html.matchAll(/<a class="wlink" href="([^"]+)">/g)].map((m) => m[1]);
  const badHtml = htmlLinks.filter((h) => !validPaths.has(h));
  const mdLinks = [...md.matchAll(/\]\(https:\/\/wfsim\.app(\/weapons\/[^)\s]+)\)/g)].map((m) => m[1]);
  const badMd = mdLinks.filter((h) => !validPaths.has(h));
  check(`${route}: all auto-linked weapons exist in roster (${htmlLinks.length} HTML / ${mdLinks.length} MD)`,
    badHtml.length === 0 && badMd.length === 0, `badHtml=${badHtml.slice(0, 5)} badMd=${badMd.slice(0, 5)}`);

  // Every weapon link renders its same-origin inline icon from /img/*.webp.
  if (htmlLinks.length > 0) {
    const icons = [...html.matchAll(/<img class="wicon" src="(\/img\/[^"]+\.webp)"/g)].map((m) => m[1]);
    const missingImg = icons.filter((src) => !existsSync(resolve(OUT, src.slice(1))));
    check(`${route}: weapon icons present & same-origin (${icons.length} icons)`,
      icons.length >= htmlLinks.length && missingImg.length === 0,
      `icons=${icons.length} links=${htmlLinks.length} missing=${missingImg.slice(0, 5)}`);
  }
}

// Confirm key weapons & slot-specific kitguns are linked on both EN and ZH articles.
const beamEn = readFileSync(resolve(OUT, "mechanics/beam-weapons/index.html"), "utf8");
const beamZh = readFileSync(resolve(OUT, "mechanics/zh/beam-weapons/index.html"), "utf8");
const magEn = readFileSync(resolve(OUT, "mechanics/magnetic-electricity/index.html"), "utf8");
const magZh = readFileSync(resolve(OUT, "mechanics/zh/magnetic-electricity/index.html"), "utf8");

for (const [label, doc, expected] of [
  ["EN beam weapons", beamEn, ["/weapons/Torid", "/weapons/Phantasma_Prime", "/weapons/Kuva_Nukor", "/weapons/gaze_primary", "/weapons/gaze_secondary", "/weapons/vermisplicer_primary", "/weapons/vermisplicer_secondary"]],
  ["ZH beam weapons", beamZh, ["/weapons/Torid", "/weapons/Phantasma_Prime", "/weapons/Kuva_Nukor", "/weapons/gaze_primary", "/weapons/gaze_secondary", "/weapons/vermisplicer_primary", "/weapons/vermisplicer_secondary"]],
  ["EN magnetic/electricity", magEn, ["/weapons/Laetum", "/weapons/Kuva_Nukor", "/weapons/Lanka", "/weapons/Glaive_Prime", "/weapons/Torid", "/weapons/Vectis_Prime"]],
  ["ZH magnetic/electricity", magZh, ["/weapons/Laetum", "/weapons/Kuva_Nukor", "/weapons/Lanka", "/weapons/Glaive_Prime", "/weapons/Torid", "/weapons/Vectis_Prime"]],
]) {
  const missing = expected.filter((href) => !doc.includes(`href="${href}"`));
  check(`${label}: key weapons linked`, missing.length === 0, missing.join(", "));
}

check("magnetic-electricity embeds interactive calculator",
  magEn.includes('id="magnetic-calculator"') && magZh.includes('id="magnetic-calculator"'));
check("beam-weapons embeds collapsible full beam index",
  beamEn.includes('id="full-beam-index"') && beamZh.includes('id="full-beam-index"'));
check("magnetic-electricity embeds collapsible full W-tier index (all 5 tiers + melee)",
  [magEn, magZh].every((doc) =>
    doc.includes('id="full-weakness-index"') &&
    doc.includes("W = 0.0") && doc.includes("W = 0.2") && doc.includes("W = 0.4") &&
    doc.includes("W = 0.5") && doc.includes("W = 1.0") && doc.includes("data-melee-sub")));

// Verify all catalog weapon links inside <details> point to valid roster weapons.
for (const [label, doc] of [["EN beam", beamEn], ["ZH beam", beamZh], ["EN mag", magEn], ["ZH mag", magZh]]) {
  const chips = [...doc.matchAll(/class="mech-chip"[^>]*href="([^"]+)"/g)].map((m) => m[1]);
  const bad = chips.filter((h) => !validPaths.has(h));
  check(`${label}: catalog weapon links valid (${chips.length} chips)`, bad.length === 0, bad.slice(0, 5));
}

console.log(failed ? `\n${failed} failed` : "\nmechanics static pages, markdown twins, SEO metadata, and weapon links PASS");
process.exit(failed ? 1 : 0);
