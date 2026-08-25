// EVERY CHECK SCRIPT PARSES — the THIRTY-NINTH check, and the cheapest here.
//
// A browser check hands the page a block of JavaScript as a template literal:
// `await evaluate("(async () => { … })()")` with backticks. An UNESCAPED
// backtick anywhere inside that block closes the literal, and it closes it just
// as well from a COMMENT as from code — which is where it always comes from,
// because these files are more comment than code and the natural way to name an
// identifier in prose is to put it in backticks.
//
// IT HAS HAPPENED SEVEN TIMES: check_assembly, check_riven_board,
// check_equip_rules, check_gain_freshness, check_parity, check_scan_progress,
// check_board_link. Every time the same way, and every time it cost a full
// browser run to find out — the failure is a SyntaxError from the module loader
// with a stack inside `node:internal`, naming the line the literal STARTS on,
// which is nowhere near the backtick.
//
// SO THIS IS `node --check` OVER EVERY SCRIPT HERE, and nothing cleverer. A
// scanner that tried to find the backtick itself was written first and was
// wrong in both directions: an escaped backtick in a comment is legal and it
// flagged eleven, while the first unescaped one IS the terminator, so there is
// nothing for it to find that the parser does not already know. The parser is
// the authority; what was missing was running it without being asked.
//
// It costs milliseconds and needs no browser, which is why it runs in CI beside
// the parity and board-submit checks rather than living in somebody's memory.
//
//   node scripts/check_page_bodies.mjs
//
// Exits non-zero listing every file that does not parse.
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const DIR = "scripts";
const me = path.basename(new URL(import.meta.url).pathname);

const files = fs.readdirSync(DIR).sort()
  .filter((n) => n.endsWith(".mjs") && n !== me);

let bad = 0;
for (const name of files) {
  const r = spawnSync(process.execPath, ["--check", path.join(DIR, name)],
    { encoding: "utf8" });
  if (r.status === 0) continue;
  bad += 1;
  // THE PARSER'S OWN WORDS, trimmed to the part that names the fault. Its stack
  // is all `node:internal` frames and says nothing about this file.
  const why = (r.stderr || "").split("\n")
    .filter((l) => l.trim() && !l.includes("node:internal") && !l.startsWith("    at "))
    .slice(0, 4).join("\n        ");
  console.log(`  FAIL  ${name} does not parse\n        ${why}`);
}

// THE NEGATIVE CONTROL, and not a formality: a run that found no files would
// pass silently for ever, and this file's whole value is that nobody has to
// remember it exists.
if (!files.length) {
  console.log("  FAIL  no check scripts found at all");
  bad += 1;
}

console.log(bad ? `\n${bad} failed` : `\n${files.length} scripts parse`);
process.exit(bad ? 1 : 0);
