// PROSE IN THIS REPOSITORY STATES THE CURRENT RULE, AND NOTHING ELSE.
//
// Git holds what a rule used to be, who asked for it and how it was found. A
// doc or a comment that retells that is duplicated, goes stale, and buries the
// one thing a reader needs — which for an agent loading `AGENTS.md` into every
// session is a direct cost per turn.
//
// Two claims, enforced differently, because only one of them can be finished in
// a single pass:
//
//   ATTRIBUTION is a HARD ZERO. `(owner, 2026-08-29)`, `(user, …)`, a bare
//   dated parenthetical: a rule in a public repository has no author and no
//   date. `docs/MEASUREMENTS.md` is exempt — there the provenance IS the data.
//
//   NARRATIVE is a RATCHET. "used to", "was considered", "it turned out" mark a
//   history being retold, but they also appear in legitimate sentences about
//   the GAME ("Heat used to refresh"), so a hard zero would be wrong. The count
//   may fall and never rise, which is `naming::FROZEN`'s rule in another
//   domain: it makes the direction one-way without demanding a flag day.
//
// No browser and no build: plain node over `git ls-files`, so it costs
// milliseconds and runs beside `check_page_bodies` at the front of CI.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const EXEMPT = new Set(["docs/MEASUREMENTS.md", "scripts/check_comment_style.mjs"]);
const SKIP = ["site/", "private/", "vendor/", "target/"];
const EXTS = [".rs", ".js", ".mjs", ".html", ".css", ".md", ".py", ".yaml", ".yml", ".jsonc", ".toml"];

const DATE = String.raw`20\d\d-\d\d-\d\d`;
const ATTRIBUTION = [
  new RegExp(String.raw`\((owner|user)[^()]{0,90}${DATE}[^()]{0,40}\)`, "g"),
  // The lookbehind is what keeps this off CODE: `Some(owner)` is a match
  // binding, and prose never runs a word straight into the parenthesis.
  new RegExp(String.raw`(?<![A-Za-z0-9_])\((owner|user)\)`, "g"),
  new RegExp(String.raw`\((?:decision |rule |player report, |reaffirmed |amended |measured |from |added |as of )?${DATE}[^()]{0,90}\)`, "g"),
  new RegExp(String.raw`,[ \t]*(owner|user),[ \t]*${DATE}`, "g"),
];

// The phrases that mark a history rather than a rule. Kept deliberately narrow:
// each one is a construction that can only be about the PAST of this code.
const NARRATIVE = new RegExp(
  String.raw`\b(used to|was considered|first proposal|it turned out|cost an evening|for a long time|the day it happened|by mistake at first|briefly hardcoded)\b`,
  "gi",
);

// THE CEILING MAY ONLY FALL. Lower it whenever a pass removes some; never raise
// it to make a red run green — that is the one edit this file exists to refuse.
const NARRATIVE_CEILING = 304;

const files = execFileSync("git", ["ls-files"], { cwd: ROOT, encoding: "utf8" })
  .split("\n")
  .filter((f) => f && !EXEMPT.has(f) && !SKIP.some((d) => f.startsWith(d))
    && EXTS.some((e) => f.endsWith(e)));

let failures = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || detail === undefined ? "" : `  — ${detail}`}`);
  if (!ok) failures += 1;
};

const attributions = [];
let narrative = 0;
for (const rel of files) {
  let src;
  try { src = readFileSync(resolve(ROOT, rel), "utf8"); } catch { continue; }
  for (const rx of ATTRIBUTION) {
    for (const m of src.matchAll(rx)) {
      const line = src.slice(0, m.index).split("\n").length;
      attributions.push(`${rel}:${line}: ${m[0].replace(/\s+/g, " ").slice(0, 60)}`);
    }
  }
  narrative += [...src.matchAll(NARRATIVE)].length;
}

check("comment style no decision is attributed to a person or a date",
  attributions.length === 0,
  attributions.slice(0, 8).join("\n        ") + (attributions.length > 8 ? `\n        …and ${attributions.length - 8} more` : ""));

check(`comment style the narrative ratchet holds (${narrative} ≤ ${NARRATIVE_CEILING})`,
  narrative <= NARRATIVE_CEILING,
  `${narrative} phrases retelling a history, ceiling ${NARRATIVE_CEILING}`);

// A ratchet nobody lowers is a ratchet that stops meaning anything, so a run
// that is comfortably under says so rather than passing in silence.
if (narrative < NARRATIVE_CEILING) {
  console.log(`  note  ceiling can drop to ${narrative} in scripts/check_comment_style.mjs`);
}

console.log(failures ? `\n${failures} failed` : "\nprose states the current rule");
process.exit(failures ? 1 : 0);
