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
// FOUND BY LOOKING, because the script's name carries a digest that changes
// every release — naming it here would go stale on the first build.
const appName = readdirSync(resolve(SITE, "asset")).find((f) => /^app\..*\.js$/.test(f));
check(!!appName, "site/asset holds the page's script", "nothing matches app.<digest>.js");
const app = read(`asset/${appName}`);
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
const workerName = readdirSync(resolve(SITE, "asset")).find((f) => /^worker\..*\.js$/.test(f));
check(!!workerName, "site/asset holds the compute worker", "nothing matches worker.<digest>.js");
const worker = read(`asset/${workerName}`);
// …AND THE SCRIPT ASKS FOR THAT WORKER. Both names carry a digest, so the pair
// can only be wrong in one direction: a substitution that did not run leaves
// `/worker.js`, which no longer exists.
check(app.includes(`new Worker("/asset/${workerName}")`),
  "the page's script starts the hashed worker",
  "app.js still names /worker.js, which this build does not serve");
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
const carried = list.filter((l) => l.startsWith("board.") || l.startsWith("board/"));
check(carried.length === 0, "the board is not in the payload",
  `desktop/payload.lst carries ${carried.join(" ")}`);

// EVERY ASSET THE PAGE NAMES IS IMMUTABLE, which is the whole of the caching
// model: the HTML is the one mutable file a release has, it names digests of
// its own assets, and a repeat visit therefore costs one conditional request
// and nothing else. A script or a stylesheet at a name that survives a release
// breaks that in the direction nobody looks — the page still works, and every
// reader pays a blocking round trip for it before anything draws.
//
// READ OUT OF THE SHIPPED HTML rather than from a list written here, so an
// asset added tomorrow is covered by a file nobody edited.
const headers = read("_headers");
const immutable = [];
{
  let at = null;
  for (const line of headers.split(/\r?\n/)) {
    if (/^\//.test(line)) at = line.trim();
    else if (at && /immutable/.test(line)) immutable.push(at);
  }
}
const covers = (path) => immutable.some((rule) =>
  new RegExp("^" + rule.replace(/[.]/g, "[.]").replace(/[*]/g, ".*") + "$").test(path));

const page = read("index.html");
const pageAssets = [...page.matchAll(/(?:src|href)="(\/[^"]+\.(?:js|css))"/g)].map((m) => m[1]);
check(pageAssets.length >= 2, "the page names a script and a stylesheet",
  JSON.stringify(pageAssets));
for (const f of pageAssets) {
  check(covers(f), `the edge serves ${f} immutable`,
    "a name that survives a release costs every repeat visit a blocking round trip");
  check(existsSync(resolve(SITE, f.replace(/^\//, ""))), `...and ${f} is in site/`);
}

// ── the board says which board it is ──────────────────────────────────────
// A DIGEST PER FILE AND ONE OVER THE MANIFEST, recomputed here rather than
// trusted: the board is published a weapon at a time, so the stamp is what a
// client uses to decide which files to fetch, and a stamp that does not describe
// the directory beside it sends a client to fetch a board that is not there.
const meta = JSON.parse(read("board.meta.json"));
const dir = resolve(SITE, "board");
const files = Object.fromEntries(readdirSync(dir)
  .filter((f) => f.endsWith(".json"))
  .map((f) => [f.replace(/[.]json$/, ""),
    createHash("sha256").update(readFileSync(resolve(dir, f))).digest("hex")]));
const lines = Object.keys(files).sort().map((k) => `${k} ${files[k]}\n`).join("");
check(meta.digest === createHash("sha256").update(lines).digest("hex"),
  "board.meta.json stamps the board beside it",
  "the stamp names a board this build does not hold — run scripts/board_meta.py");
const named = Object.keys(meta.files || {}).sort().join(",");
check(named === Object.keys(files).sort().join(","),
  "…and names every file of it",
  `${Object.keys(meta.files || {}).length} named, ${Object.keys(files).length} present`);
check(meta.rows > 0 && meta.weapons > 0 && meta.generation,
  "…and counts what is in it and says which generation it is",
  `${meta.rows} rows, ${meta.weapons} weapons, generation ${meta.generation || "(none)"}`);

// …AND THE PAGE READS THAT STAMP RATHER THAN THE ONE IN THE BINARY. The same
// fields are `data/board_state.yaml`, which is compiled into the wasm, so a
// page reading the compiled copy dates the board by its own BUILD — and "N
// more submitted since this board was scored" then counts every submission
// since that build. The scoring job rewrites this file hourly and rebuilds
// nothing, so the fetched stamp is the only copy that moves with the board.
check(/const benchState = [\s\S]{0,300}?BOARD_META[\s\S]{0,120}?\.boards/.test(app),
  "…and the page prefers it over the compiled copy",
  "nothing reads BOARD_META.boards — the age would date the build");
// THE BINDING IS READ OUT OF THE SOURCE, so renaming it is not a failure here
// and reading a field off anything else still is.
const bind = (app.match(/const (\w+) = benchState\(/) || [])[1];
check(!!bind, "…through one binding", "nothing calls benchState()");
for (const f of ["scored_at_epoch_seconds", "submissions"]) {
  const off = [...app.matchAll(new RegExp(`([A-Za-z_$][\\w$]*)\\.${f}\\b`, "g"))].map((m) => m[1]);
  check(off.length > 0 && off.every((o) => o === bind), `…and reads ${f} from it`,
    `read off ${[...new Set(off)].join(", ") || "nothing"}`);
}

console.log(NL + (bad ? `${bad} failed` : "the build says which release it is"));
process.exit(bad ? 1 : 0);
