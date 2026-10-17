// SPDX-License-Identifier: AGPL-3.0-or-later
// THE ENGINE'S READ-ONLY ANSWERS, worked out once a build: the catalogs take no
// body and depend on nothing but the module and its packs,
// so the build asks the module it publishes and writes each answer beside it.
// A page reads these and draws before its engine has downloaded
// (docs/WASM.md §"Answers the build already knows").
//
//   node scripts/engine_views.mjs <out-dir> <glue.js> <module.wasm> <pack.json>...
//   → prints {"/api/meta": "<file>", ...}, one file a path of `VIEWS`

import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { loadEngine } from "./wasm_engine.mjs";

/// Each path the page reads from a file, and the stem the file is named by.
export const VIEWS = {
  "/api/meta": "meta", "/api/i18n": "i18n", "/api/warframe/catalog": "warframes", "/api/catalogue": "catalogue",
};

const [out, js, wasm, ...packs] = process.argv.slice(2);
if (!out || !js || !wasm) {
  console.error("usage: engine_views.mjs <out-dir> <glue.js> <module.wasm> <pack.json>...");
  process.exit(2);
}
const engine = loadEngine({ files: { js, wasm, packs } });
const written = {};
for (const [p, stem] of Object.entries(VIEWS)) {
  const answer = engine.api(p, {});
  if (!answer || answer.error) {
    console.error(`${p}: ${answer && answer.error}`);
    process.exit(1);
  }
  const body = JSON.stringify(answer);
  const name = `${stem}.${createHash("sha256").update(body).digest("hex").slice(0, 12)}.json`;
  fs.writeFileSync(path.join(out, name), body);
  written[p] = name;
}
console.log(JSON.stringify(written));
