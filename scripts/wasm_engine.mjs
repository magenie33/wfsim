// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SHIPPED ENGINE IN NODE: `site/pkg`'s metered wasm, called exactly as
// the page calls it, so a machine that is not a browser — an official witness,
// a check, the work fit — answers a board row with the page's bits, work
// included (docs/WASM.md §"Gas").
//
//   import { loadEngine, measureRow } from "./wasm_engine.mjs";
//   const e = loadEngine();                       // site/pkg, by site/release.json
//   const e2 = loadEngine({ pkg: "<dir>" });      // wfsim_wasm.js + wfsim_wasm_bg.wasm
//   e.api("/api/board/order", { record, ruler, mode })   // any wasm door, JSON in and out
//   await measureRow([e], { record, ruler, mode })       // → { score, metric, work, counts, ... }

import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/(\w:)/, "$1")), "..");

/// ONE ENGINE INSTANCE: its own memory, as one of the page's lanes is.
export function loadEngine({ pkg = "" } = {}) {
  let js, wasm, release = {};
  if (pkg) {
    js = path.join(pkg, "wfsim_wasm.js");
    wasm = path.join(pkg, "wfsim_wasm_bg.wasm");
  } else {
    release = JSON.parse(fs.readFileSync(path.join(ROOT, "site/release.json"), "utf8"));
    js = path.join(ROOT, `site/pkg/wfsim_wasm.${release.wasm}.js`);
    wasm = path.join(ROOT, `site/pkg/wfsim_wasm_bg.${release.wasm}.wasm`);
  }
  const ctx = vm.createContext({ console, TextEncoder, TextDecoder, WebAssembly });
  vm.runInContext(fs.readFileSync(js, "utf8") + "\nthis.wasm_bindgen = wasm_bindgen;", ctx);
  // Built inside the context: the shim tells its argument's shape by the
  // context's own `Object.prototype`.
  ctx.bytes = fs.readFileSync(wasm);
  const exports = vm.runInContext("wasm_bindgen.initSync({ module: bytes })", ctx);
  const gas = exports.__wfsim_gas;
  return {
    release,
    api: (p, body) => JSON.parse(ctx.wasm_bindgen.api(p, JSON.stringify(body))),
    /// The meter's running total, for a check; 0n on a module nobody metered.
    gas: () => (gas ? gas.value : 0n),
  };
}

/// A BOARD ROW, MEASURED AS THE PAGE MEASURES IT (69-board-work.js
/// `measureRow`): the order, the runs in pieces of `piece` handed round the
/// `lanes` in turn, folded IN RUN ORDER on the first, scored there. Any lanes
/// and any piece size give the same bits; that is what check_gas holds.
export async function measureRow(lanes, { record, ruler, mode = "base" }, { piece = 1 } = {}) {
  const [first] = lanes;
  const order = first.api("/api/board/order", { record, ruler, mode });
  if (!order.ok) return order;
  const request = order.request;
  const runs = Number(request.runs) || 0;
  let acc = null;
  for (let from = 0, k = 0; from < runs; from += piece, k++) {
    const count = Math.min(piece, runs - from);
    const got = lanes[k % lanes.length].api("/api/board/runs", { request, from, count });
    if (!got.ok) return got;
    const step = first.api("/api/board/fold", { request, acc, pieces: got.shards });
    if (!step.ok) return step;
    acc = step.acc;
  }
  return { ...first.api("/api/board/score", { ruler, request, acc }), runs };
}
