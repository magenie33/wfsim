// SPDX-License-Identifier: AGPL-3.0-or-later
// node scripts/check_riven_branch_shapes.mjs [baseline git ref] [cargo target dir]
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const ref = process.argv[2] || "46d8e72e1961e7ae94d9bb0bedf51563d6116a7d";
const target = resolve(root, process.argv[3] || "target/riven-branch-check");
const runOpts = { cwd: root, maxBuffer: 16 << 20, timeout: 180_000 };
const cargo = execFileSync("cargo", ["build", "--locked", "--release", "-p", "wfsim-engine", "--bin", "riven_shapes",
  "--target-dir", target, "--message-format=json"], { ...runOpts, encoding: "utf8" });
const artifacts = cargo.trim().split(/\r?\n/).map(JSON.parse).filter((r) => r.reason === "compiler-artifact");
const current = artifacts.find((r) => r.target.name === "riven_shapes").executable;
const engine = artifacts.find((r) => r.target.name === "wfsim_engine").filenames.find((p) => p.endsWith(".rlib"));
const baseline = join(target, process.platform === "win32" ? "riven-shapes-baseline.exe" : "riven-shapes-baseline");
const input = execFileSync("git", ["show", `${ref}:engine/src/bin/riven_shapes.rs`], runOpts);
execFileSync("rustc", ["--edition=2021", "--crate-name", "riven_shapes_baseline", "-C", "opt-level=3",
  "-C", "codegen-units=1", "--extern", `wfsim_engine=${engine}`, "-L", `dependency=${dirname(engine)}`,
  "-o", baseline, "-"], { ...runOpts, input });

let cases = 0;
function parity(args, expectedStatus, warning = null) {
  const a = spawnSync(baseline, args, runOpts), b = spawnSync(current, args, runOpts);
  for (const r of [a, b]) {
    if (r.error) throw r.error;
    assert.equal(r.signal, null);
    assert.equal(r.status, expectedStatus, `${args.join(" ")}: ${r.stderr}`);
  }
  assert.ok(a.stdout.equals(b.stdout), `stdout differs: ${args.join(" ")}`);
  assert.ok(a.stderr.equals(b.stderr), `stderr differs: ${args.join(" ")}`);
  if (warning) assert.match(b.stderr.toString(), warning);
  else assert.equal(b.stderr.length, 0);
  cases++;
  const lines = b.stdout.length ? b.stdout.toString().trimEnd().split(/\r?\n/).map(JSON.parse) : [];
  return { lines, sha256: createHash("sha256").update(b.stdout).digest("hex"), bytes: b.stdout.length };
}

const defaultRolls = [0.9, 0.95, 1, 1.05, 1.1];
const bySize = (cards) => [2, 3].map((k) => cards.filter((c) => c.bonuses.length === k).length);
const weapons = ["torid", "ocucor"];
const defaults = parity(weapons, 0);
const onlyPositive = parity([...weapons, "--malus=none"], 0);
assert.deepEqual(defaults.lines.map((row) => row.weapon), weapons);
for (const [i, row] of defaults.lines.entries()) {
  const n = row.stats.length;
  const oldLeaves = [n * (n - 1) / 2, n * (n - 1) * (n - 2) / 6];
  const noMalus = row.shapes.filter((c) => c.malus === null);
  const newLeaves = bySize(noMalus);
  assert.deepEqual(noMalus, onlyPositive.lines[i].shapes);
  assert.ok(newLeaves.every((x, k) => x > 0 && x < oldLeaves[k]));
  assert.deepEqual(row.malus.filter((m) => m.startsWith("status_duration@")), defaultRolls.map((r) => `status_duration@${r}`));
  assert.deepEqual([...new Set(row.shapes.filter((c) => c.malus?.id === "status_duration").map((c) => c.malus.roll))], defaultRolls);
  const unique = new Set(row.shapes.map((c) => JSON.stringify(c.bonuses)));
  assert.equal(unique.size, noMalus.length);
  console.log(JSON.stringify({ weapon: row.weapon, pool: n, oldLeaves, newLeaves,
    avoidedLeaves: oldLeaves.map((x, k) => x - newLeaves[k]), shapesBySize: bySize(row.shapes),
    positiveFormats: [row.shapes.length, unique.size], cards: row.shapes.length }));
}
console.log(JSON.stringify({ defaultStdoutBytes: defaults.bytes, defaultStdoutSha256: defaults.sha256 }));

const small = parity(["torid", "--bonus=status_duration,damage,blast,critical_chance,damage,ammo_efficiency,blast",
  "--malus=status_duration@1.05,none,status_duration@0.95,status_duration@1.0,status_duration@1e0,status_duration@1.1,status_duration@0.900,harmless,none,toxin"],
0, /toxin: not a malus this riven rolls, left out/).lines[0];
assert.deepEqual(small.stats, ["ammo_efficiency", "blast", "critical_chance", "damage", "status_duration"]);
assert.deepEqual(small.malus, ["status_duration@1.05", "none", "status_duration@0.95", "status_duration@1",
  "status_duration@1.1", "status_duration@0.9", "zoom@0.9"]);
assert.ok(small.shapes.every((c) => c.bonuses.filter((s) => ["ammo_efficiency", "blast"].includes(s.id)).length <= 1
  && !c.bonuses.some((s) => s.id === c.malus?.id)));
assert.deepEqual(bySize(small.shapes.filter((c) => !c.malus)), [9, 7]);

for (const args of [
  ["torid", "--bonus=blast,ammo_efficiency", "--malus=none"],
  ["torid", "--bonus=status_duration,damage", "--malus=status_duration@0.95,status_duration@1.05"],
  ["torid", "--bonus=,,", "--malus=none"],
]) assert.deepEqual(parity(args, 0).lines[0].shapes, []);
assert.deepEqual(parity([], 0).lines, []);
assert.deepEqual(parity(["torid", "--bonus=damage,no_such_stat,no_such_stat"], 1,
  /torid: no_such_stat is not a bonus this riven rolls/).lines, []);
assert.deepEqual(parity(["torid", "--bonus=damage,toxin", "--malus=toxin"], 1,
  /toxin: not a malus this riven rolls, left out\r?\ntorid: no malus choice left/).lines, []);
for (const roll of ["bad", "1.1001", "NaN"]) {
  assert.deepEqual(parity(["torid", "--bonus=damage,toxin", `--malus=toxin,status_duration@${roll},none`], 1,
    /toxin: not a malus this riven rolls, left out\r?\ntorid: status_duration@/).lines, []);
}
assert.deepEqual(parity(["magistar", "torid", "no_such_weapon", "ocucor", "--bonus=damage,multishot", "--malus=none"], 1,
  /magistar: melee rivens are not surveyed\r?\nno_such_weapon: no riven class/).lines.map((row) => row.weapon), weapons);
console.log(`${cases} CLI cases: stdout, stderr and exit exactly match ${ref}`);
