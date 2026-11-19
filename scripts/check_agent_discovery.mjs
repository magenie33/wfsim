// WHAT AN AGENT READS TO FIND WFSIM, as `build_site_app.py` wrote it into
// `site/` (docs/AGENT.md §"Machine-readable"). What it holds: the AI catalog
// (ARD) names only documents the site serves, each entry by exactly one of
// `url` or `data`, under `urn:air:` identifiers and a media type; robots.txt
// and the page's head both point at it; and the A2A card the build wrote is the
// one `mcp/a2a.js` serves; the method is served in every language it has. No
// network, no browser; run after a site build.
import { readFileSync, existsSync, readdirSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { agentCard } from "../mcp/a2a.js";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SITE = "https://wfsim.app";
let failed = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok" : "FAIL"}  ${name}${ok ? "" : `  — ${String(detail).slice(0, 300)}`}`);
  if (!ok) failed++;
};
const read = (p) => readFileSync(resolve(ROOT, "site", p), "utf8");
const served = (url) => url.startsWith(SITE + "/") && existsSync(resolve(ROOT, "site", url.slice(SITE.length + 1)));

const has = existsSync(resolve(ROOT, "site/.well-known/ai-catalog.json"));
check("the site serves an AI catalog", has);
if (!has) { console.log(`\n${failed} failed`); process.exit(1); }
const cat = JSON.parse(read(".well-known/ai-catalog.json"));
check("the catalog names its spec version and its host", !!cat.specVersion && cat.host && cat.host.displayName
  && /^did:web:/.test(cat.host.identifier), JSON.stringify(cat.host));
check("...and has entries", Array.isArray(cat.entries) && cat.entries.length > 0);
for (const e of cat.entries || []) {
  check(`${e.identifier}: one of url or data, a type, a name, 2–5 queries`,
    /^urn:air:wfsim\.app:[a-z0-9-]+:[a-z0-9-]+$/.test(e.identifier) && !!e.displayName && /^[a-z]+\/[a-z0-9.+-]+$/.test(e.type)
      && ("url" in e) !== ("data" in e) && e.representativeQueries.length >= 2 && e.representativeQueries.length <= 5,
    JSON.stringify(e));
  check(`${e.identifier}: names a document the site serves`, !e.url || served(e.url), e.url);
}
const robots = read("robots.txt");
check("robots.txt points at the catalog", /^Agentmap: https:\/\/wfsim\.app\/\.well-known\/ai-catalog\.json$/m.test(robots));
const groups = robots.split(/\n\s*\n/).filter((group) => /^User-agent:/m.test(group));
const accessRules = (name) => groups
  .filter((group) => group.split("\n").includes("User-agent: " + name))
  .flatMap((group) => group.match(/^(?:Allow|Disallow): .*$/gm) || []).join("\n");
check("every crawler group permits search and retrieval and reserves training", groups.length > 0 &&
  groups.every((group) => /^Content-Signal: search=yes, ai-input=yes, ai-train=no$/m.test(group)));
check("search and user-directed fetches can read public pages",
  ["*", "OAI-SearchBot", "ChatGPT-User", "Claude-SearchBot", "Claude-User", "Amzn-SearchBot", "Amzn-User", "PerplexityBot"]
    .every((name) => accessRules(name) === "Allow: /"));
check("model training and corpus collection tokens opt out",
  ["GPTBot", "ClaudeBot", "Applebot-Extended", "Google-Extended", "Amazonbot", "meta-externalagent", "Bytespider", "CCBot"]
    .every((name) => accessRules(name) === "Disallow: /"));
check("creator and crawler permissions are published and discoverable",
  served(SITE + "/reuse/index.html") &&
  read("reuse/index.html").includes(readFileSync(resolve(ROOT, "docs/creator-use.html"), "utf8").trim()) &&
  read("terms/index.html").includes('href="/reuse"') &&
  [robots, read("llms.txt"), read("weapons/Braton.md")].every((body) => body.includes(SITE + "/reuse")));
check("the page's head points at it", /<link rel="ai-catalog" href="\/\.well-known\/ai-catalog\.json"/.test(read("index.html")));

const card = JSON.parse(read(".well-known/agent-card.json"));
const { version, ...rest } = card;
const { version: _v, ...live } = agentCard("x");
check("the A2A card the build wrote is the one the server serves, versioned by an engine",
  JSON.stringify(rest) === JSON.stringify(live) && /^\w+$/.test(version), version);

// THE METHOD (docs/AGENT.md §"The method"): each language's page and twin is
// served, every page names every other, and the index and the skill point at it.
const methodDir = resolve(ROOT, "site/method");
const tongues = ["/method", ...(existsSync(methodDir) ? readdirSync(methodDir, { withFileTypes: true }) : [])
  .filter((d) => d.isDirectory()).map((d) => `/method/${d.name}`)];
const page = (t) => (existsSync(resolve(ROOT, `site${t}/index.html`)) ? read(`${t.slice(1)}/index.html`) : "");
check("the method is served in English and each language it is translated into",
  tongues.length > 1 && tongues.every((t) => page(t) && existsSync(resolve(ROOT, `site${t}.md`))), tongues);
check("...and each of its pages names every language's", tongues.every((t) =>
  tongues.every((u) => page(t).includes(`hreflang="${u === "/method" ? "en" : u.split("/")[2]}" href="${SITE}${u}"`))));
check("llms.txt and the skill point at the method", read("llms.txt").includes(`${SITE}/method.md`)
  && read(".well-known/agent-skills/wfsim-weapon-builds/SKILL.md").includes(`${SITE}/method#ruler`));

const boardCatalog = `${SITE}/api/board/pages/catalog`;
const weaponCatalog = `${boardCatalog}?weapon=braton`;
const weaponMd = read("weapons/Braton.md");
const weaponHtml = read("weapons/Braton/index.html");
const discoveryPaths = ["index.md", "llms.txt", ".well-known/api-catalog", ".well-known/agent-skills/wfsim-weapon-builds/SKILL.md"];
check("board discovery points to the current revision catalog",
  discoveryPaths.every((path) => {
    const content = read(path);
    return content.includes(boardCatalog) && !content.includes(SITE + "/board/index.json");
  }) && weaponMd.includes(weaponCatalog) && weaponHtml.includes(`href="${weaponCatalog}"`));
if (weaponHtml.includes("data-library-lead") || weaponMd.includes("/board/braton.json")) {
  const publishedAt = new Date(JSON.parse(read("board.meta.json")).scored_at * 1000).toISOString().slice(0, 10);
  check("static build summaries date their scores and label the archived download",
    weaponHtml.includes(`aria-label="Archived WFSim board, ${publishedAt}"`) &&
    weaponMd.includes("Archived board JSON:"));
}

console.log(failed ? `\n${failed} failed` : "\nwhat an agent reads to find WFSim is what the site serves");
process.exit(failed ? 1 : 0);
