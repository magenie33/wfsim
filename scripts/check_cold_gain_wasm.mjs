// SPDX-License-Identifier: AGPL-3.0-or-later
// node scripts/check_cold_gain_wasm.mjs NEW_NO_MODULES [OLD_NO_MODULES [DATA_PACK]]
// With no old package, use the checked-in release. Every lane receives the
// same data pack before its first request, including a fresh candidate lane.
// Set WFSIM_COLD_GAIN_PREP=1 to time only preparation/wire/report overhead.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const [nextDir, oldDir, dataPack] = process.argv.slice(2);
assert(nextDir, "expected a no-modules wasm-bindgen package directory");
const site = new URL("../site/", import.meta.url);
const release = JSON.parse(fs.readFileSync(new URL("release.json", site), "utf8"));
const data = fs.readFileSync(dataPack || new URL(`pkg/${release.packs.find(x => x.startsWith("pack.data."))}`, site));
const workerSource = fs.readFileSync(new URL("../web/src/static/worker.js", import.meta.url), "utf8");

function packageFiles(dir) {
  const file = (name, ext) => dir && dir !== "site" ? path.join(dir, `${name}.${ext}`)
    : new URL(`pkg/${name}.${release.wasm}.${ext}`, site);
  return {
    glue: fs.readFileSync(file("wfsim_wasm", "js"), "utf8"),
    module: new WebAssembly.Module(fs.readFileSync(file("wfsim_wasm_bg", "wasm"))),
  };
}

function lane({ glue, module }, packs = []) {
  let id = 0;
  const messages = [];
  const context = vm.createContext({ console, performance, TextEncoder, TextDecoder, WebAssembly,
    setInterval, clearInterval, setTimeout, onmessage: null,
    postMessage: msg => messages.push(msg),
    importScripts: () => vm.runInContext(glue, context) });
  vm.runInContext(workerSource, context);
  const send = data => context.onmessage({ data });
  const ready = send({ kind: "engine", module, packs });
  return async message => {
    await ready;
    const requestId = ++id;
    await send(structuredClone({ ...message, id: requestId }));
    const responses = messages.splice(0).filter(m => m.id === requestId);
    assert.equal(responses.length > 0, true, `no response for ${message.kind}`);
    const result = responses.pop();
    return { payload: JSON.stringify(result.payload), progress: responses.map(m => [m.done, m.total]) };
  };
}

const old = lane(packageFiles(oldDir), [data]);
const nextFiles = packageFiles(nextDir);
const next = lane(nextFiles, [data]);
const fresh = lane(nextFiles, [data]);

async function equal(message, actualLane = next) {
  const expected = await old(message);
  assert.deepEqual(await actualLane({ ...message, prepared: true }), expected, message.kind);
  return JSON.parse(expected.payload);
}

const base = { weapon: "braton", duration: 4, enemy: "thrax_centurion", level: 100,
  runs: 4, seed: 0x5eed, run_series: true, headshot_pct: 50,
  arcane: ["primary_merciless"], buffs: { "arcane:primary_merciless": { stacks: 0, locked: false } },
  also_acting: [{ weapon: "lato", mods: ["hornet_strike"] }] };
const a = { ...base, mods: ["cryo_rounds", "hellfire", "stormbringer", "hellfire"] };
const b = { ...a, mods: ["stormbringer", "hellfire", "cryo_rounds", "hellfire"] };
const c = { ...b, buffs: { "arcane:primary_merciless": { stacks: 12, locked: true } } };
const d = { ...c, replay: true, replay_follow: ["not_a_body"] };
const piece = (body, from = 0, count = 1) => ({ kind: "shard", body, from, count });
const merge = (body, shards) => ({ kind: "merge", body, shards });

const reports = [];
for (const body of [a, b, c, d]) assert.equal((await equal(piece(body))).runs, 1);
for (const body of [a, b, c, d]) {
  const first = await equal(piece(body));
  assert.equal(first.runs, 1, JSON.stringify(first));
  // Interleave different complete requests before asking for the second piece.
  assert.equal((await equal(piece(body === a ? c : a))).runs, 1);
  const middle = await equal(piece(body, 1, 1));
  const tail = await equal(piece(body, 2, 2));
  assert.equal(tail.runs, 2);
  const shards = [first, middle, tail];
  const report = await equal(merge(body, shards));
  assert.equal(report.ok, true, JSON.stringify(report));
  assert.equal(report.runs, 4);
  assert.equal(report.dps_runs.length, 4);
  assert.equal(report.score_runs.length, 4);
  assert.equal(report.combatants.length, 2);
  reports.push(report);
  await equal(merge(body, shards), fresh);
}
assert.notDeepEqual(reports[0].panel.damage, reports[1].panel.damage, "order control must change damage types");
assert.notDeepEqual(reports[1].dps_runs, reports[2].dps_runs, "hidden buff control must change a run");
assert(reports[3].replay, "full replay must be retained");

const refusal = { ...a, mods: ["serration", "serration"] };
assert.match((await equal(piece(refusal))).error, /incompatible/);
const first = await equal(piece(a));
assert.match((await equal(merge(a, [{}]))).error, /^shard:/);
assert.match((await equal(merge(refusal, [{}]))).error, /^shard:/);
assert.match((await equal(piece({ ...a, mods: ["not_a_mod"] }))).error, /not_a_mod/);
await equal(merge(a, [first]));
console.log("Cold gain worker parity: four complete reports, indexed shards/progress, fresh lane and refusals");

if (process.env.WFSIM_COLD_GAIN_PREP === "1") {
  const slots = ["serration", "split_chamber", "point_strike", "vital_sense",
    "hellfire", "cryo_rounds", "stormbringer"];
  const list = JSON.parse((await old({ kind: "api", path: "/api/candidates", body: {
    weapon: base.weapon, slots, axis: { kind: "mods", idx: slots.length },
  } })).payload);
  const bodies = list.candidates.map(({ payload }) => ({ ...base, ...payload }));
  assert(bodies.length > 1);
  assert.equal(new Set(bodies.map(body => JSON.stringify(body))).size, bodies.length);
  const measure = async (run, prepared) => {
    const t0 = performance.now();
    const outputs = [];
    for (const body of bodies) {
      for (const from of [0, 1]) {
        const shard = await run({ ...piece(body, from, 0), prepared });
        assert.equal(JSON.parse(shard.payload).runs, 0);
      }
      // Empty pieces are not mergeable on the JSON boundary (nonfinite
      // extrema), so use an empty parts list. No indexed or median fight runs.
      const report = await run({ ...merge(body, []), prepared });
      const parsed = JSON.parse(report.payload);
      assert.equal(parsed.ok, true, report.payload);
      assert.equal(parsed.runs, 0);
      outputs.push(report.payload);
    }
    return { ms: performance.now() - t0, outputs };
  };
  const reference = await measure(old, false);
  const candidate = await measure(next, true);
  assert.deepEqual(candidate.outputs, reference.outputs);
  console.log(`Zero engagements, ${bodies.length} distinct candidate bodies, two preparations + merge vs one preparation + merge: ${reference.ms.toFixed(1)} -> ${candidate.ms.toFixed(1)} ms (one pass, Node worker protocol)`);
}
