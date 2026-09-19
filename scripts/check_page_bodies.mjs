// EVERY CHECK SCRIPT PARSES — the THIRTY-NINTH check, and the cheapest here.
//
// A browser check hands the page a block of JavaScript as a TEMPLATE LITERAL,
// and an unescaped backtick anywhere inside it closes the literal — just as
// well from a COMMENT as from code, which is where it always comes from, since
// these files are more comment than code and the natural way to name an
// identifier in prose is to put it in backticks. It has happened seven times,
// each costing a full browser run to find, because the failure is a SyntaxError
// from inside `node:internal` naming the line the literal STARTS on.
//
// SO THIS IS `node --check` OVER EVERY SCRIPT HERE and nothing cleverer. A
// scanner hunting the backtick itself was wrong both ways: an escaped one in
// prose is legal, and the first unescaped one IS the terminator. The parser was
// always the authority; what was missing was running it unasked.
//
// It costs milliseconds and needs no browser, so it runs first in CI.
//
//   node scripts/check_page_bodies.mjs
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { spawnSync } from "node:child_process";
import { appSource } from "./app_source.mjs";

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

// THE PAGE'S OWN SCRIPT PARSES TOO, checked JOINED: a part of `app.js` is a
// fragment and does not parse on its own, so a broken seam shows only here.
const joined = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "wfsim-app-")), "app.js");
fs.writeFileSync(joined, appSource());
const whole = spawnSync(process.execPath, ["--check", joined], { encoding: "utf8" });
if (whole.status !== 0) {
  bad += 1;
  console.log(`  FAIL  app.js (joined from web/src/static/app/) does not parse\n        ${
    (whole.stderr || "").split("\n").filter((l) => l.trim() && !l.includes("node:internal")).slice(0, 4).join("\n        ")}`);
}
fs.rmSync(path.dirname(joined), { recursive: true, force: true });

// THE NEGATIVE CONTROL, and not a formality: a run that found no files would
// pass silently for ever, and this file's whole value is that nobody has to
// remember it exists.
if (!files.length) {
  console.log("  FAIL  no check scripts found at all");
  bad += 1;
}

console.log(bad ? `\n${bad} failed` : `\n${files.length} scripts parse, and app.js`);
process.exit(bad ? 1 : 0);
