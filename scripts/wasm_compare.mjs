// SPDX-License-Identifier: AGPL-3.0-or-later
// Preserve response number tokens while timing the same API in a standalone WASM instance.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadEngine } from "./wasm_engine.mjs";

const [input, output, count = "3"] = process.argv.slice(2);
const engine = loadEngine({ pkg: path.dirname(fileURLToPath(import.meta.url)) });
const rows = [];
for (const call of JSON.parse(fs.readFileSync(input, "utf8"))) {
  const result = engine.apiText(call.path, call.body_text);
  if (JSON.parse(result).ok !== true) throw new Error(`${call.id}: ${result}`);
  const milliseconds = [];
  for (let repeat = 0; repeat < Number(count); repeat++) {
    const started = performance.now();
    const next = engine.apiText(call.path, call.body_text);
    milliseconds.push(performance.now() - started);
    if (next !== result) throw new Error(`${call.id}: nondeterministic response`);
  }
  rows.push(`{"id":${JSON.stringify(call.id)},"milliseconds":${JSON.stringify(milliseconds)},` +
    `"memory_bytes":${engine.memoryBytes()},"result":${result}}`);
}
fs.writeFileSync(output, `[${rows.join(",")}]`);
