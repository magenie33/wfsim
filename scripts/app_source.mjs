// `app.js` IS JOINED FROM `web/src/static/app/*.js`, in filename order with
// nothing between the parts — the same join as `web/build.rs` and
// `app_source()` in `scripts/build_site_app.py`. A check that reads the page's
// source reads it through here: a part on its own is a fragment of one script.
import { readFileSync, readdirSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const APP_PARTS_DIR = resolve(ROOT, "web/src/static/app");

/// The part files, in the order they are joined.
export const appParts = () => readdirSync(APP_PARTS_DIR).filter((n) => n.endsWith(".js")).sort();

/// The whole of `app.js`, as the page receives it.
export const appSource = () =>
  appParts().map((n) => readFileSync(resolve(APP_PARTS_DIR, n), "utf8")).join("");
