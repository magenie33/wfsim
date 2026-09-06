// A BUILT `site/` SAYS WHICH RELEASE IT IS, AND CANNOT BE MIXED WITH ANOTHER.
//
// Both properties fail silently, which is why they are a check. A page with no
// release identity still opens and still computes — the only symptom is two
// readers comparing numbers and disagreeing, with nothing either of them can
// quote. And a wasm module served from a fixed path still loads: from whatever
// copy the browser cached, which may be the previous engine under this build's
// `app.js`, producing different answers from the same seed with no error
// anywhere.
//
// No browser: every one of these is a fact about the files, and a check that
// costs a second runs on every change rather than the ones somebody remembered.
// docs/DISTRIBUTION.md.
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SITE = resolve(ROOT, "site");
const NL = String.fromCharCode(10);
let bad = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok || !detail ? "" : `  — ${detail}`}`);
  if (!ok) bad += 1;
};
const read = (rel) => readFileSync(resolve(SITE, rel), "utf8");

if (!existsSync(resolve(SITE, "release.json"))) {
  console.log("site/release.json is missing — run scripts/build_site_app.py first");
  process.exit(1);
}

// ── the release identity ──────────────────────────────────────────────────
const rel = JSON.parse(read("release.json"));
check(/^[0-9a-f]{12}$/.test(rel.release || ""), "site/release.json names a release",
  `release is ${JSON.stringify(rel.release)}`);

// STAMPED INTO THE SCRIPT, not only served beside it. `release.json` is what a
// mirror is asked; `RELEASE_ID` is what the running page can print, and a build
// that shipped one without the other answers only half of a bug report.
const app = read("app.js");
check(app.includes(`const RELEASE_ID = "${rel.release}";`),
  "app.js carries the same release",
  `app.js does not hold RELEASE_ID = "${rel.release}"`);
check(!app.includes('const RELEASE_ID = "dev";'),
  "…and not the placeholder", "the substitution did not run");

// ── the engine module is content-addressed ────────────────────────────────
const pkg = readdirSync(resolve(SITE, "pkg"));
const wasm = pkg.filter((f) => f.endsWith(".wasm"));
const glue = pkg.filter((f) => f.endsWith(".js"));
check(wasm.length === 1 && glue.length === 1,
  "site/pkg holds exactly one module and one glue script",
  `pkg/ holds ${pkg.join(" ") || "nothing"} — a stale copy is payload nothing references`);

const digest = (wasm[0] || "").match(/\.([0-9a-f]{12})\.wasm$/);
check(!!digest, "the module is named by its digest",
  `${wasm[0]} carries no digest — a browser can pair a cached module with this app.js`);
if (digest) {
  const actual = createHash("sha256")
    .update(readFileSync(resolve(SITE, "pkg", wasm[0]))).digest("hex").slice(0, 12);
  check(actual === digest[1], "…and the digest is the module's own",
    `named ${digest[1]}, hashes to ${actual}`);
  check(glue[0] === `wfsim_wasm.${digest[1]}.js`, "…and the glue carries the same one",
    `glue is ${glue[0]}`);
}

// THE WORKER ASKS FOR THE NAMES THAT EXIST. A substitution that half ran leaves
// a 404 the page reports as "could not start", which reads as a broken build
// rather than a broken rename.
const worker = read("worker.js");
for (const f of [wasm[0], glue[0]].filter(Boolean)) {
  check(worker.includes(`pkg/${f}`), `worker.js asks for pkg/${f}`, "the module would 404");
}
check(!/pkg\/wfsim_wasm(_bg\.wasm|\.js)/.test(worker),
  "…and for no unhashed name", "an unhashed path is a cache that can hold the wrong engine");

// A HASHED NAME IS ONLY HALF OF IT: without the header the edge still revalidates
// on every load, and the immutability the name promises is never taken.
check(/^\/pkg\/\*$/m.test(read("_headers"))
  && /\/pkg\/\*[\s\S]{0,120}?immutable/.test(read("_headers")),
  "the edge is told pkg/ is immutable", "no immutable rule for /pkg/*");

// ── the board does not travel in the release ──────────────────────────────
// 4.3 MB rescored three times an hour, against a release that moves on a code
// change: inside the payload it banks a new immortal blob every twenty minutes
// and hands every client a download for a file it replaces before reading it.
// Nothing breaks if it goes back in, which is why this is asserted.
const list = readFileSync(resolve(ROOT, "desktop/payload.lst"), "utf8")
  .split(NL).map((l) => l.trim()).filter((l) => l && !l.startsWith("#"));
check(!list.some((l) => l.startsWith("board.")), "the board is not in the payload",
  `desktop/payload.lst carries ${list.filter((l) => l.startsWith("board.")).join(" ")}`);

// A HASHED NAME NEEDS ITS UNHASHED NEIGHBOURS REVALIDATED. `worker.js` keeps its
// name across releases, and a stale copy asks for the previous release's module
// — which is no longer served, so the page does not start at all.
const headers = read("_headers");
for (const f of ["/app.js", "/worker.js", "/style.css"]) {
  const rule = new RegExp(`^\\${f}$[\\s\\S]{0,120}?must-revalidate`, "m");
  check(rule.test(headers), `the edge revalidates ${f}`,
    "a cached copy can outlive the hashed module it asks for");
}

// ── the board says which board it is ──────────────────────────────────────
const meta = JSON.parse(read("board.meta.json"));
const board = readFileSync(resolve(SITE, "board.json"));
check(meta.digest === createHash("sha256").update(board).digest("hex"),
  "board.meta.json stamps the board beside it",
  "the stamp names a board this build does not hold — run scripts/board_meta.py");
check(meta.rows > 0 && meta.bytes === board.length,
  "…and counts what is in it", `${meta.rows} rows, ${meta.bytes} of ${board.length} bytes`);

console.log(NL + (bad ? `${bad} failed` : "the build says which release it is"));
process.exit(bad ? 1 : 0);
