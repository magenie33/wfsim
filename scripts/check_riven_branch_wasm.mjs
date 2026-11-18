// SPDX-License-Identifier: AGPL-3.0-or-later
// node scripts/check_riven_branch_wasm.mjs old-node-wasm-dir new-node-wasm-dir
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { runInNewContext } from "node:vm";

const paths = process.argv.slice(2);
assert.equal(paths.length, 2, "provide old and new node WASM directories");
const require = createRequire(import.meta.url);
const [before, after] = paths.map((p) => require(resolve(p, "wfsim_wasm.js")));
const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const metaName = read("site/index.html").match(/meta\.[a-f0-9]+\.json/)[0];
const meta = JSON.parse(read(`site/pkg/${metaName}`));
const digest = createHash("sha256");
const counts = {};
function same(path, req) {
  const wire = JSON.stringify(req);
  const a = before.api(path, wire), b = after.api(path, wire);
  assert.equal(b, a, `${path} ${wire}`);
  digest.update(b);
  counts[path] = (counts[path] || 0) + 1;
  const out = JSON.parse(b);
  assert.equal(out.ok, true, JSON.stringify(out));
  return out;
}

const rolls = [0.9, 0.95, 1, 1.05, 1.1];
let statCount = 0, roleCount = 0;
for (const [kind, pool] of Object.entries(meta.riven_stats)) {
  const roster = meta.weapons.filter((w) => !w.no_riven && w.riven_class === kind);
  const enabled = (w, stat) => !w.riven_excludes.includes(stat.id);
  let total = 0, roles = 0;
  for (const stat of pool) {
    const w = roster.find((x) => enabled(x, stat) && !x.riven_unconfirmed.includes(stat.id));
    assert.ok(w, `${kind}/${stat.id} needs a roster witness`);
    total++;
    const other = pool.filter((s) => s !== stat && !s.spliced && enabled(w, s));
    const anchors = other.filter((s) => s.bonus);
    const negative = other.find((s) => s.malus);
    for (const positive of [true, false]) {
      if (!(positive ? stat.bonus : stat.malus)) continue;
      roles++;
      for (const n of [2, 3]) for (const hasMalus of positive ? [false, true] : [true]) {
        for (const [i, roll] of rolls.entries()) {
          const at = { id: stat.id, roll };
          const fill = positive && hasMalus ? anchors.filter((s) => s !== negative) : anchors;
          const bonuses = fill.slice(0, n - Number(positive)).map((s) => ({ id: s.id, roll: 1 }));
          if (positive) bonuses.unshift(at);
          const malus = positive ? hasMalus ? { id: negative.id, roll: 1 } : null : at;
          const req = { weapon: w.id, bonuses, malus, rank: i * 2 };
          const res = same("/api/riven", req);
          assert.deepEqual(res.illegal, [], JSON.stringify(req));
          const line = res.stats.find((s) => s.slot === (positive ? "0" : "malus"));
          assert.equal(line.id, stat.id);
          assert.equal(line.roll, roll);
          const typed = { id: stat.id, value: line.shown };
          const converted = same("/api/riven", positive ? { ...req, bonuses: [typed, ...bonuses.slice(1)] } : { ...req, malus: typed });
          assert.deepEqual(converted.illegal, []);
        }
      }
    }
  }
  statCount += total;
  roleCount += roles;
  console.log(`${kind}: ${total} stats, ${roles} legal stat/role pairs`);
}

const appraisal = runInNewContext(
  read("web/src/static/app/87-headless.js").split("const HEADLESS_QUERIES =")[0] + "\nheadlessRivenGainRequest");
for (const weapon of ["torid", "laetum", "furis", "magistar"]) {
  const rows = JSON.parse(read(`site/board/${weapon}.json`));
  const row = rows.find((r) => r.riven);
  const { bonuses, malus, rolls: actual } = row.riven;
  const card = { bonuses: bonuses.map((id, i) => ({ id, roll: actual[i] })),
    malus: malus ? { id: malus, roll: actual[bonuses.length] } : null, rank: 8 };
  const { request, context } = appraisal(meta, { weapon, ruler: row.benchmark, card, runs: 3 });
  const frozen = same("/api/computation/input", { kind: "optimization", request, context });
  const spec = frozen.request.rivens[0].spec;
  assert.deepEqual([...spec.bonuses.map((b) => b.roll), ...(spec.malus ? [spec.malus.roll] : [])], actual);
}

console.log(`${statCount} class/stat pairs; ${roleCount} class/stat/role pairs; ${JSON.stringify(counts)} byte-identical; SHA-256 ${digest.digest("hex")}`);
