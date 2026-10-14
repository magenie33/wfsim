// THE CLIENTS' CONTRACT STATES WHAT THE CODE DOES. docs/CLIENTS.md is what a
// third-party client is built from; a value it states that the code no longer
// holds is a client refused for following the document. Every key of its
// `Values` block is a constant read here from the code, and a key nobody reads
// here fails, so the block cannot grow past what is held to it.
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { DIALS, CLIENT_DIALS } from "../worker/dials.js";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(resolve(ROOT, p), "utf8");
let bad = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok || !detail ? "" : `  — ${detail}`}`);
  if (!ok) bad += 1;
};

// Each key: the constant and every file that defines it (all must agree). A
// dial (worker/dials.js) is stated as its default and read from there.
const DIAL = Symbol("dial");
const KEYS = {
  protocol: ["PROTOCOL", ["worker/verify.js"]],
  protocols_served: ["PROTOCOLS_SERVED", ["worker/verify.js"]],
  recognised_client_id: ["RECOGNISED_ID", ["worker/verify.js"]],
  verifier_id_shape: ["VERIFIER_ID", ["worker/appraise.js", "worker/contribution.js"]],
  engine_id_shape: ["ENGINE_ID", ["worker/appraise.js", "worker/factory.js"]],
  gas_price_per_million: ["GAS_PRICE_PER_MILLION", ["engine/src/fight/gas.rs"]],
  run_price: ["RUN_PRICE", ["engine/src/fight/gas.rs"]],
  work_per_point: ["WORK_PER_POINT", ["worker/contribution.js"]],
  score_near_relative: ["SCORE_NEAR", ["worker/tasks.js"]],
  lease_ms: [DIAL, ["lease_ms"]],
  pack_ms: [DIAL, ["pack_ms"]],
  pack_first: [DIAL, ["pack_first"]],
  pack_max: [DIAL, ["pack_max"]],
  pack_lease_ms: [DIAL, ["pack_lease_ms"]],
  packs_held: [DIAL, ["packs_held"]],
  renew_ms: [DIAL, ["renew_ms"]],
  row_ceiling_ms: [DIAL, ["row_ceiling_ms"]],
  clients_per_fact: ["CLIENTS_PER_FACT", ["worker/verify.js"]],
  facts_per_refusal_forgiven: ["FACTS_PER_REFUSAL_FORGIVEN", ["worker/tasks.js"]],
  engine_grace_days: ["ENGINE_GRACE_DAYS", ["scripts/build_site_app.py"]],
};

// THE FILE A NAME IS IMPORTED FROM, so a constant set from another reads through.
function importedFrom(name, file) {
  const m = read(file).match(new RegExp(`^import\\s*\\{[^}]*\\b${name}\\b[^}]*\\}\\s*from\\s*"\\./([^"]+)"`, "m"));
  return m ? `${file.slice(0, file.lastIndexOf("/") + 1)}${m[1]}` : null;
}

// A constant's value as the doc writes it: a regex by its source, a set as
// its members joined by ", ", a number as a number.
function valueOf(name, file) {
  if (!file) return undefined;
  const m = read(file).match(file.endsWith(".py") ? new RegExp(`^${name}\\s*=\\s*(.+?)\\s*(?:#.*)?$`, "m")
    : new RegExp(`^\\s*(?:export\\s+)?(?:pub\\s+)?const\\s+${name}\\b[^=]*=\\s*(.+?);\\s*(?://.*)?$`, "m"));
  if (!m) return undefined;
  const rhs = m[1].trim();
  const re = rhs.match(/^\/(.+)\/[a-z]*$/);
  if (re) return re[1];
  const set = rhs.match(/^new Set\(\[(.*)\]\)$/);
  const expr = (set ? set[1] : rhs).replace(/(\d)_(?=\d)/g, "$1")
    .replace(/\b[A-Z][A-Z0-9_]+\b/g, (n) => String(valueOf(n, file) ?? valueOf(n, importedFrom(n, file))));
  if (!/^[\d\s.e+\-*,()]+$/.test(expr)) return undefined;
  const v = Function(`"use strict"; return [${expr}];`)();
  return set ? v.join(", ") : v[0];
}

const doc = read("docs/CLIENTS.md");
const block = doc.match(/## Values[\s\S]*?```text\n([\s\S]*?)```/);
check(!!block, "docs/CLIENTS.md has its Values block");
const stated = new Map((block ? block[1] : "").split("\n").filter((l) => l.trim())
  .map((l) => { const i = l.indexOf(":"); return [l.slice(0, i).trim(), l.slice(i + 1).trim()]; }));
for (const [key, said] of stated) {
  const spec = KEYS[key];
  if (!spec) { check(false, `${key} is a value this check reads`, "add it to KEYS or take it out of the block"); continue; }
  const [name, files] = spec;
  if (name === DIAL) {
    const dial = DIALS[files[0]];
    check(!!dial && Number(said) === dial.default, `${key}: ${said} is the default of the dial ${files[0]}`, `the dial says ${dial && dial.default}`);
    continue;
  }
  for (const f of files) {
    const v = valueOf(name, f);
    const same = typeof v === "number" ? Number(said) === v : said === v;
    check(same, `${key}: ${said} is ${name} in ${f}`, `the code says ${v}`);
  }
}
for (const key of Object.keys(KEYS)) check(stated.has(key), `${key} is stated`);
// EVERY DIAL A CLIENT IS TOLD is stated, and the page says a client reads it from the reply.
for (const k of CLIENT_DIALS) check(stated.has(k) && KEYS[k] && KEYS[k][0] === DIAL, `the client dial ${k} is stated as a dial`);
check(/never hard-code/i.test(doc) && /`dials`/.test(doc), "the page says a client reads the dials from the reply and hard-codes none");

console.log(bad ? `\n${bad} failed` : "\nthe clients' contract states what the code does");
process.exit(bad ? 1 : 0);
