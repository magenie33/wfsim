// PROSE IN THIS REPOSITORY STATES THE CURRENT RULE, AND NOTHING ELSE.
//
// Git holds what a rule used to be, who asked for it and how it was found. A
// doc or a comment retelling that is duplicated, goes stale, and buries the one
// thing a reader needs — a direct cost per turn for an agent loading
// `AGENTS.md` into every session. Two claims, enforced differently, because
// only one can be finished in a single pass:
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

const NL = String.fromCharCode(10);
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
// LICENSE is the AGPL text verbatim and is nobody's to edit.
const EXEMPT = new Set(["docs/MEASUREMENTS.md", "scripts/check_comment_style.mjs", "LICENSE"]);
const SKIP = ["site/", "private/", "vendor/", "target/"];
const EXTS = [".rs", ".js", ".mjs", ".html", ".css", ".md", ".py", ".yaml", ".yml", ".jsonc", ".toml"];

const DATE = String.raw`20\d\d-\d\d-\d\d`;
const ATTRIBUTION = [
  // A parenthetical that OPENS with the attribution is provenance whatever
  // follows it, so this is not bounded by a length or a date.
  // `\s` and not a literal space: the attribution is just as much one when
  // the name ends a comment line and the date opens the next.
  new RegExp(String.raw`[ 	]*\((owner|user),\s`, "g"),
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
//
// The six left are all ordinary English rather than a history being retold:
// this rule's own statement in AGENTS.md, "in the backlog for a long time" in
// CONTRIBUTING.md, "may not be used to brand" in README.md, "only ever used to
// pick BETWEEN corners" in the scorer, and DE's own Energy Channel card text
// twice. A phrase list cannot tell those from the rest, which is why this is a
// ratchet and not a hard zero.
const NARRATIVE_CEILING = 6;

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

// ---------------------------------------------------------------------------
// A NOTE THAT REPEATS IS AN ID, NOT A PARAGRAPH.
//
// `data/notes.yaml` holds each shared sourcing note once and a file that would
// repeat it carries `# see notes: <id>`. Nothing in the engine reads either —
// `build.rs` embeds every yaml with its comments stripped — so this is what
// keeps a reference from naming an entry that is gone and an entry from
// outliving its last use.
const notesSrc = readFileSync(resolve(ROOT, "data/notes.yaml"), "utf8");
const defined = new Set(
  [...notesSrc.matchAll(/^ {2}([a-z0-9_]+): \|/gm)].map((m) => m[1]));
const used = new Map();
for (const rel of files) {
  if (!rel.startsWith("data/")) continue;
  let src;
  try { src = readFileSync(resolve(ROOT, rel), "utf8"); } catch { continue; }
  for (const m of src.matchAll(/# see notes: ([a-z0-9_]+)/g)) {
    used.set(m[1], (used.get(m[1]) || 0) + 1);
  }
}
const dangling = [...used.keys()].filter((id) => !defined.has(id));
const orphans = [...defined].filter((id) => !used.has(id));

check(`comment style every \`see notes:\` resolves (${used.size} ids, ${[...used.values()].reduce((a, b) => a + b, 0)} uses)`,
  dangling.length === 0, dangling.slice(0, 6).join(", "));
check("comment style ...and no note outlives its last use",
  orphans.length === 0, orphans.slice(0, 6).join(", "));

// ---------------------------------------------------------------------------
// A COMMENT IS A CONSTRAINT, NOT AN ESSAY.
//
// Two ratchets, and neither counts blocks over twelve lines — that number GOES
// UP when one essay is split into two well-sized comments, which is the
// improvement, so it is a metric a fix can fail.
//
//   ESSAYS — blocks over twenty lines. Past twenty a block has stopped stating
//   a rule and started explaining a subject, which is what `docs/` is for, and
//   an explanation in two places is two explanations that drift. Measured: the
//   band above twenty is a fifth of the blocks over twelve and five times the
//   lines, while 321 blocks are THIRTEEN TO FIFTEEN and account for 596 lines
//   between them. This is the number being driven to zero.
//
//   TOTAL COMMENT LINES — monotone, and it cannot be gamed by splitting.
//
// Both may only FALL. Lower them whenever a pass removes some; never raise one
// to make a red run green, which is the one edit this file exists to refuse.
const ESSAY_LIMIT = 20;
const ESSAY_CEILING = 54;
const LINE_CEILING = 67503;
const LINE_COMMENT = /^\s*(\/\/\/|\/\/!|\/\/|#)/;
let essays = 0;
let commentLines = 0;
const worst = [];
for (const rel of files) {
  if (rel.endsWith(".md") || rel.endsWith(".html") || rel.endsWith(".css")) continue;
  let src;
  try { src = readFileSync(resolve(ROOT, rel), "utf8"); } catch { continue; }
  let run = 0, start = 0;
  const lines = src.split(NL);
  for (let i = 0; i <= lines.length; i += 1) {
    if (i < lines.length && LINE_COMMENT.test(lines[i]) && lines[i].trim() !== "#") {
      if (run === 0) start = i + 1;
      run += 1;
      commentLines += 1;
      continue;
    }
    if (run > ESSAY_LIMIT) { essays += 1; worst.push(`${rel}:${start} (${run})`); }
    run = 0;
  }
}
worst.sort((a, b) => Number(b.match(/\((\d+)\)$/)[1]) - Number(a.match(/\((\d+)\)$/)[1]));
check(`comment style essays over ${ESSAY_LIMIT} lines (${essays} ≤ ${ESSAY_CEILING})`,
  essays <= ESSAY_CEILING,
  `${essays} blocks, longest: ${worst.slice(0, 4).join(", ")}`);
check(`comment style total comment lines (${commentLines.toLocaleString()} ≤ ${LINE_CEILING.toLocaleString()})`,
  commentLines <= LINE_CEILING, `${commentLines} lines`);

// A ratchet nobody lowers is a ratchet that stops meaning anything, so a run
// that is comfortably under says so rather than passing in silence.
if (narrative < NARRATIVE_CEILING) {
  console.log(`  note  ceiling can drop to ${narrative} in scripts/check_comment_style.mjs`);
}

console.log(failures ? `\n${failures} failed` : "\nprose states the current rule");
process.exit(failures ? 1 : 0);
