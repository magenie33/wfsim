import {readFileSync} from "node:fs";
import {resolve, join} from "node:path";
import {headlessLibrarySummary} from "../mcp/headless.js";

const [site, catalog] = process.argv.slice(2);
if (!site || !catalog) throw new Error("usage: library_summaries.mjs <site> <meta.json>");
const meta = JSON.parse(readFileSync(resolve(catalog), "utf8"));
const benchmark = meta.benchmarks.find((b) => b.primary) || meta.benchmarks[0];
const say = {meta, t: (s) => s, tf: (s) => s, dt: (s) => s, zone: "UTC"};
const out = {};
for (const w of meta.weapons) {
  let rows = [];
  try { rows = JSON.parse(readFileSync(join(resolve(site), "board", w.id + ".json"), "utf8")); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
  out[w.id] = headlessLibrarySummary(say, w, rows, benchmark.id);
}
process.stdout.write(JSON.stringify(out));
