// `app.js` IS ITS PARTS, JOINED. `web/src/static/app/NN-name.js` in filename
// order is the whole of the page's script, and the filename prefix is the only
// statement of that order — `web/build.rs`, `app_source()` in
// `scripts/build_site_app.py` and `scripts/app_source.mjs` all sort and join.
// No browser: it reads the directory, so it costs milliseconds.
//
//   node scripts/check_app_parts.mjs
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { APP_PARTS_DIR, appParts } from "./app_source.mjs";

// THE CEILING MAY ONLY FALL: it is the largest part today. Lower it when a part
// is split; never raise it — a part past it is a section to cut in two.
const PART_LINE_CEILING = 1292;
const NAME = /^\d\d-[a-z0-9]+(?:-[a-z0-9]+)*\.js$/;

let failures = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || detail === undefined ? "" : `  — ${detail}`}`);
  if (!ok) failures += 1;
};

const entries = readdirSync(APP_PARTS_DIR);
const parts = appParts();
check(`app parts every file in web/src/static/app/ is a part (${parts.length})`,
  parts.length > 0 && entries.length === parts.length && parts.every((n) => NAME.test(n)),
  [...entries.filter((n) => !parts.includes(n)), ...parts.filter((n) => !NAME.test(n))].join(", ") || "no parts");

const prefixes = parts.map((n) => n.slice(0, 2));
const twice = prefixes.filter((p, i) => prefixes.indexOf(p) !== i);
check("app parts ...each at its own position, so the order has one reading",
  twice.length === 0, twice.join(", "));

const long = parts
  .map((n) => [n, readFileSync(resolve(APP_PARTS_DIR, n), "utf8").split("\n").length - 1])
  .filter(([, lines]) => lines > PART_LINE_CEILING);
check(`app parts no part runs past ${PART_LINE_CEILING} lines`,
  long.length === 0, long.map(([n, l]) => `${n} (${l})`).join(", "));

const whole = resolve(APP_PARTS_DIR, "..", "app.js");
check("app parts there is no web/src/static/app.js beside them — the parts are the one source",
  !existsSync(whole), "delete it; the served app.js is joined from the parts");

console.log(failures ? `\n${failures} failed` : "\napp.js is its parts");
process.exit(failures ? 1 : 0);
