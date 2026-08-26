// The desktop shell's injected scripts are JS inside Rust raw strings, and
// nothing in either compiler looks at them.
//
// WHY THIS EXISTS. `cargo build` is happy with any bytes between `r#"` and
// `"#`, and the webview reports a syntax error by silently not running the
// script. For an injected probe that means: no assertions, no output, and a
// check that fails on its own watchdog two hundred and forty seconds later
// saying "the page never reported" — which reads as a hung network, a slow
// machine, or a broken update server. It cost an hour of chasing an update
// channel that was working perfectly, twice, before the parser was asked.
//
// It is `check_page_bodies.mjs`'s own lesson in the other crate: the parser was
// always the authority, and what was missing was running it unasked.
//
// Milliseconds, no browser.
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SOURCE = "desktop/src/main.rs";
const OPEN = /const (\w+): &str = r#"/g;

const src = readFileSync(SOURCE, "utf8");
const dir = mkdtempSync(join(tmpdir(), "wfsim-probes-"));
let checked = 0;
const broken = [];

for (const m of src.matchAll(OPEN)) {
  const name = m[1];
  const from = m.index + m[0].length;
  const to = src.indexOf('"#;', from);
  if (to < 0) {
    broken.push(`${name}: raw string is never closed`);
    continue;
  }
  const body = src.slice(from, to);
  const file = join(dir, `${name}.mjs`);
  writeFileSync(file, body);
  try {
    execFileSync(process.execPath, ["--check", file], { stdio: "pipe" });
    checked++;
  } catch (e) {
    const why = (e.stderr?.toString() || e.message).split("\n").slice(0, 4).join("\n");
    broken.push(`${name}:\n${why}`);
  }
}

rmSync(dir, { recursive: true, force: true });

if (!checked && !broken.length) {
  console.error(`no injected scripts found in ${SOURCE} — has the pattern changed?`);
  process.exit(1);
}

if (broken.length) {
  console.error(`FAIL  ${broken.length} injected script(s) do not parse:\n`);
  for (const b of broken) console.error(b + "\n");
  process.exit(1);
}

console.log(`PASS  ${checked} injected scripts in ${SOURCE} parse`);
