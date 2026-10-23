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
check("robots.txt points at the catalog", /^Agentmap: https:\/\/wfsim\.app\/\.well-known\/ai-catalog\.json$/m.test(read("robots.txt")));
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

console.log(failed ? `\n${failed} failed` : "\nwhat an agent reads to find WFSim is what the site serves");
process.exit(failed ? 1 : 0);
