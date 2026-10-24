// SPDX-License-Identifier: AGPL-3.0-or-later
// A SIMULATION SPREAD OVER THIS MACHINE'S CORES, to the bit what one call
// answers. The runs are independent given their index, so each thread fights a
// slice of them ONE SHARD A RUN (`/api/board/runs`), and the shards fold into
// one accumulator in run order (`/api/board/fold`) — the board scorer's own
// order, since a float sum merged in any other grouping moves the last bits.
// Threads live for one simulation: a small server keeps no idle engines.
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { availableParallelism } from "node:os";
import { loadWasm } from "./engine.mjs";

if (!isMainThread && workerData && workerData.fleet) {
  const wasm = loadWasm();
  parentPort.on("message", ({ body, from, count }) => {
    parentPort.postMessage(wasm.api("/api/board/runs", JSON.stringify({ request: JSON.parse(body), from, count })));
  });
}

/// THE PAGE'S CEILING TOO: past eight lanes a fight's merge and the copies of the
/// engine cost more than the cores give back.
const MAX_LANES = 8;

/// `/api/simulate`'s answer for `body`, on this process's engine `wasm` and up to
/// `lanes` threads. Too few runs to split, or one core, is one call.
export async function simulateFleet(wasm, body, lanes = Math.min(availableParallelism(), MAX_LANES)) {
  const text = JSON.stringify(body ?? {});
  const runs = Math.max(1, Number(body.runs) || 1);
  if (lanes < 2 || runs < lanes * 2) return JSON.parse(wasm.api("/api/simulate", text));
  // EVERY RUN COVERED EXACTLY ONCE, the remainder to the first lanes: a gap or an
  // overlap is not a slow answer, it is a wrong one.
  const each = Math.floor(runs / lanes), extra = runs % lanes;
  const slices = Array.from({ length: lanes }, (_, k) => ({ from: k * each + Math.min(k, extra), count: each + (k < extra ? 1 : 0) }));
  const shards = await Promise.all(slices.map((s) => new Promise((ok, no) => {
    const t = new Worker(new URL(import.meta.url), { workerData: { fleet: true } });
    t.once("message", (out) => { t.terminate(); ok(JSON.parse(out)); });
    t.once("error", (e) => { t.terminate(); no(e); });
    t.postMessage({ body: text, ...s });
  })));
  // THE ENGINE'S REFUSAL IS AN ANSWER, passed on whole as one call's would be.
  const refused = shards.find((s) => !s || s.error || s.ok === false);
  if (refused) return refused;
  const fold = JSON.parse(wasm.api("/api/board/fold", JSON.stringify({ request: body, pieces: shards.flatMap((s) => s.shards) })));
  if (!fold || fold.ok === false) return fold;
  return JSON.parse(wasm.simulate_merged(text, JSON.stringify([fold.acc])));
}
