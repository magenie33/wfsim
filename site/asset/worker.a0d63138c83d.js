// SPDX-License-Identifier: AGPL-3.0-or-later
// wfsim wasm worker (docs/WASM.md phase 4). Owns one wasm engine instance.
// Protocol (from app.js's api() shim):
//   { id, kind: "api", path, body }  → { id, payload }          (quick endpoints)
//   { id?, kind: "rest", ms }          → { id, kind: "rested", payload }   (a pause, no wasm)
//   { kind: "optimize", body, checkpoint? }
//                                    → { kind: "progress",   payload }*
//                                      { kind: "checkpoint", payload }*
//                                      { kind: "board",      payload }*
//                                      { kind: "result",     payload }
//   `checkpoint` (a JSON string from a previous session) RESUMES that run.
// The optimize call blocks this worker until done — that is the design: the
// page runs it in a DEDICATED worker and cancels by terminating it.
// A LOAD THAT FAILS SAYS WHERE before it throws: `{ kind: "load_failed", cause }`,
// one of `LOAD_CAUSES` in 06-transport.js. The throw stays, because the
// optimizer's fleets settle on `onerror` alone and know no such message.
const loadFailed = (cause) => postMessage({ kind: "load_failed", cause });
try {
  importScripts("/pkg/wfsim_wasm.522aea1d5364.js");
} catch (err) {
  loadFailed("glue");
  throw err;
}

// THE ENGINE ARRIVES FROM THE PAGE, compiled, in the first `engine` message:
// the page downloads it once for all its workers (06-transport.js
// `spawnEngine`). A module the browser would not post comes as its `url`,
// fetched here; a download the page could not make comes as its `cause`.
// A FAILED LOAD IS SAID, then thrown from a task, which reaches the page's
// `onerror` and settles the lane as dead.
let handed;
const ready = new Promise((res) => { handed = res; }).then(({ module, url, cause }) => {
  if (cause) throw Object.assign(new Error(`engine ${cause}`), { loadCause: cause });
  return wasm_bindgen({ module_or_path: module || url }).catch((err) => {
    throw Object.assign(err, { loadCause: err instanceof WebAssembly.CompileError ? "compile"
      : err instanceof RangeError ? "memory" : url ? "network" : "unclassified" });
  });
});
ready.catch((err) => {
  loadFailed(err.loadCause);
  setTimeout(() => { throw err; });
});
// …AND ONE STILL WAITING FOR IT SAYS SO, so a slow line is not taken for a dead
// lane: the page's watchdog resets on any word. The page's download ends on
// its own clock (`ENGINE_STALL_MS`), so this ends with it.
const arriving = setInterval(() => postMessage({ kind: "alive" }), 5000);
ready.then(() => clearInterval(arriving), () => clearInterval(arriving));
// …AND ONCE LOADED IT SAYS SO FROM INSIDE EVERY ENGINE CALL (wasm
// `set_heartbeat`): the call blocks this thread, so a long one was silence the
// page could not tell from a worker the browser had reclaimed. One word in 2 s.
let beatAt = 0;
ready.then(() => wasm_bindgen.set_heartbeat(() => {
  const now = performance.now();
  if (now - beatAt >= 2000) { beatAt = now; postMessage({ kind: "alive" }); }
}), () => {});

onmessage = async (e) => {
  if (e.data.kind === "engine") { handed(e.data); return; }
  await ready;
  const msg = e.data;
  if (msg.kind === "shard") {
    // ONE SLICE OF A SIMULATION. The runs are independent given their index, so
    // a fleet of these covers the range between them — see `simulate_shard`.
    const out = wasm_bindgen.simulate_shard(
      JSON.stringify(msg.body ?? {}), msg.from, msg.count,
      (done, total) => postMessage({ id: msg.id, kind: "progress", done, total }),
    );
    postMessage({ id: msg.id, payload: JSON.parse(out) });
  } else if (msg.kind === "merge") {
    const out = wasm_bindgen.simulate_merged(
      JSON.stringify(msg.body ?? {}), JSON.stringify(msg.shards ?? []));
    postMessage({ id: msg.id, payload: JSON.parse(out) });
  } else if (msg.kind === "api") {
    // A SIMULATE SAYS HOW FAR IT HAS GOT — ALWAYS, whether or not anyone asked
    // to see it. It is the one endpoint whose cost is unbounded (a 361-body
    // fight at the rulers' 1000 runs is a minute), and the wasm call BLOCKS
    // this thread, so from the page a worker deep in a fight and a worker that
    // has stopped existing look exactly alike. The beat is what tells them
    // apart: `makeLane` gives up on a lane that goes quiet, and a lane the page
    // cannot give up on is a list that never produces a number again. The page
    // forwards the numbers only to a caller that wanted them.
    const body = JSON.stringify(msg.body ?? {});
    const out = msg.path === "/api/simulate"
      ? wasm_bindgen.simulate_progress(body, (done, total) =>
          postMessage({ id: msg.id, kind: "progress", done, total }))
      : wasm_bindgen.api(msg.path, body);
    postMessage({ id: msg.id, payload: JSON.parse(out) });
  } else if (msg.kind === "rest") {
    // A REST BETWEEN PIECES, so a lane runs the share of the time its owner set
    // (69-board-work.js `communityRest`). Kept here, not on the page: a page in
    // the background has its timers slowed, a worker does not.
    setTimeout(() => postMessage({ id: msg.id, kind: "rested", payload: { ok: true } }), Math.max(0, Math.min(30000, msg.ms || 0)));
  } else if (msg.kind === "optimize") {
    const onProgress = (p) => postMessage({ kind: "progress", payload: JSON.parse(p) });
    // Emitted after every completed round; the page persists it so a reload
    // costs ONE round instead of the whole search.
    const onCheckpoint = (c) => postMessage({ kind: "checkpoint", payload: JSON.parse(c) });
    // Best-so-far during the screen — the long phase with no rounds in it.
    // Cancel terminates this worker, so whatever has not been posted out by
    // then is gone; this is what makes a cancel show numbers.
    const onBoard = (b) => postMessage({ kind: "board", payload: JSON.parse(b) });
    const body = JSON.stringify(msg.body ?? {});
    const cp = msg.checkpoint;
    const out = cp
      ? wasm_bindgen.optimize_resume(body, typeof cp === "string" ? cp : JSON.stringify(cp), onProgress, onCheckpoint, onBoard)
      : wasm_bindgen.optimize(body, onProgress, onCheckpoint, onBoard);
    postMessage({ kind: "result", payload: JSON.parse(out) });
  }
};
