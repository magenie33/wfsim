// A TIME IS STORED ONE WAY — docs/NAMING.md §9. An instant is UTC ISO 8601 to
// the millisecond (`new Date().toISOString()`), a column `<event>_at`; a
// calendar day is `YYYY-MM-DD`, a column `day`. Two stores that each picked
// their own compare wrong in silence: "…:31Z" sorts after "…:31.000Z", and an
// integer sorts before every string.
//
// Plain node over the schemas and the code that writes them, so it sits in CI.
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { resolve, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(resolve(ROOT, p), "utf8");
const failures = [];

// THE ANONYMOUS STORE KEEPS THE DAY AND NOTHING FINER (docs/BOARD.md, the
// upload consent): these hold a day under the name `at`.
const DAY_EXEMPT = new Set(["builds.at", "inbox.at", "shares.at"]);
const instant = (name) => /(^at|_at|_until)$/.test(name);

function schema(path) {
  const s = read(path);
  for (const t of s.matchAll(/CREATE TABLE IF NOT EXISTS (\w+)\s*\(([\s\S]*?)\n\);/g)) {
    for (const line of t[2].split("\n")) {
      const c = line.replace(/--.*/, "").trim().match(/^(\w+)\s+(TEXT|INTEGER|REAL)\b/);
      if (!c) continue;
      const [, col, type] = c, key = `${t[1]}.${col}`;
      if (/_at_ms$/.test(col)) failures.push(`${path}: ${key} is an instant in milliseconds; store it as ${col.replace(/_ms$/, "")} TEXT`);
      else if (instant(col) && type !== "TEXT") failures.push(`${path}: ${key} is ${type}; an instant is TEXT`);
    }
  }
}

const sources = (dir, ext) => readdirSync(resolve(ROOT, dir)).filter((f) => ext.test(f) && !f.startsWith("check_")).map((f) => join(dir, f).replace(/\\/g, "/"));

function writers(path) {
  const s = read(path);
  // A DAY CUT FROM `now()` is an instant someone meant to store and truncated.
  for (const m of s.matchAll(/\bnow\(\)\.slice\(0, 10\)/g)) failures.push(`${path}:${s.slice(0, m.index).split("\n").length}: now().slice(0, 10) — store now(), or name the column day`);
  // AN INSTANT TO THE SECOND sorts after the same second to the millisecond.
  for (const m of s.matchAll(/toISOString\(\)\.slice\(0, 19\)|%Y-%m-%dT%H:%M:%SZ/g)) failures.push(`${path}:${s.slice(0, m.index).split("\n").length}: an instant written to the second; write toISOString() whole`);
  // A DAY UNDER AN INSTANT'S NAME, outside the anonymous store.
  for (const m of s.matchAll(/INTO (\w+) \(([^)]*)\)/g)) {
    for (const col of m[2].split(",").map((x) => x.trim())) {
      if (instant(col) && DAY_EXEMPT.has(`${m[1]}.${col}`) === false && /toISOString\(\)\.slice\(0, 10\)/.test(s.slice(m.index, m.index + 600).split(".run()")[0])) {
        failures.push(`${path}: ${m[1]}.${col} is written a day; an instant is the whole toISOString()`);
      }
    }
  }
}

schema("worker/schema.sql");
schema("worker/accounts.sql");
for (const f of [...sources("worker", /\.js$/), ...sources("scripts", /\.(mjs|sh)$/)]) writers(f);
// THE PRIVATE WORKER is checked wherever it is checked out; CI has no copy.
if (existsSync(resolve(ROOT, "private/cloud/schema"))) {
  for (const f of sources("private/cloud/schema", /\.sql$/)) schema(f);
  for (const f of sources("private/cloud/src", /\.js$/)) writers(f);
}

if (failures.length) {
  console.error(failures.map((f) => `FAIL ${f}`).join("\n"));
  process.exit(1);
}
console.log("ok — every stored instant is ISO 8601 to the millisecond");
