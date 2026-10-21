// SPDX-License-Identifier: AGPL-3.0-or-later
// PROSE IS NEVER COMPUTED FROM (engine/src/prose.rs). `engine_id` leaves every
// key of data/prose.yaml out, which is right only while the engine cannot read
// one where a number is made:
// 1. a field of that name is a `Prose`, never a `String`;
// 2. a loader walking raw `Value`s names one only through `Prose::read`;
// 3. `Prose::shown` is not called in the metered sources, the build layer or
//    the optimizer.
//   node scripts/check_prose.mjs
import fs from "node:fs";
import path from "node:path";
import { METERED } from "./metered.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/(\w:)/, "$1")), "..");
let failures = 0;
const check = (what, bad) => {
  console.log(`  ${bad.length ? "FAIL" : "ok  "}  ${what}${bad.length ? `\n        ${bad.slice(0, 12).join("\n        ")}` : ""}`);
  if (bad.length) failures++;
};

const keys = (fs.readFileSync(path.join(ROOT, "data/prose.yaml"), "utf8").match(/^keys:\s*\[([^\]]*)\]/m) || [, ""])[1]
  .split(",").map((k) => k.trim()).filter(Boolean);
const rust = (p) => {
  const abs = path.join(ROOT, p);
  if (!fs.existsSync(abs)) return [];
  if (fs.statSync(abs).isFile()) return [p];
  return fs.readdirSync(abs, { withFileTypes: true }).flatMap((d) =>
    d.isDirectory() ? (d.name === "tests" ? [] : rust(`${p}/${d.name}`)) : d.name.endsWith(".rs") ? [`${p}/${d.name}`] : []);
};
const lines = (files) => files.flatMap((f) => fs.readFileSync(path.join(ROOT, f), "utf8").split(/\r?\n/)
  .map((l, i) => ({ at: `${f}:${i + 1}`, l, code: l.replace(/\/\/.*$/, "") })));

console.log(`prose (${keys.join(", ")}) is never computed from`);
check("data/prose.yaml names its keys", keys.length ? [] : ["no `keys: [...]` in data/prose.yaml"]);

const engine = lines(["engine/src", "wasm/src", "native/src", "cli/src", "optimizer/src"].flatMap(rust));
const all = engine.concat(lines(rust("webapi/src")));
const field = new RegExp(`\\b(${keys.join("|")})\\s*:\\s*(Option<|Vec<)?String\\b`);
check("a prose key is held as a Prose", all.filter((x) => field.test(x.code) && !/\b(let|fn)\b/.test(x.code)).map((x) => `${x.at}: ${x.l.trim()}`));

const literal = new RegExp(`"(${keys.join("|")})"`);
check("…and read off a raw map only through Prose::read",
  engine.filter((x) => literal.test(x.code) && !/Prose::read\(/.test(x.code)).map((x) => `${x.at}: ${x.l.trim()}`));

const computing = lines([...METERED, "engine/src/build", "optimizer/src"].flatMap(rust));
check("…and never shown where a number is computed",
  computing.filter((x) => /\.shown\(\)/.test(x.code)).map((x) => `${x.at}: ${x.l.trim()}`));

console.log(failures ? `\n${failures} failed` : "\nprose is read only as prose");
process.exit(failures ? 1 : 0);
