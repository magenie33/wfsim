// wfsim wasm worker (docs/WASM.md phase 4). Owns one wasm engine instance.
// Protocol (from app.js's api() shim):
//   { id, kind: "api", path, body }  → { id, payload }          (quick endpoints)
//   { kind: "optimize", body, checkpoint? }
//                                    → { kind: "progress",   payload }*
//                                      { kind: "checkpoint", payload }*
//                                      { kind: "board",      payload }*
//                                      { kind: "result",     payload }
//   `checkpoint` (a JSON string from a previous session) RESUMES that run.
// The optimize call blocks this worker until done — that is the design: the
// page runs it in a DEDICATED worker and cancels by terminating it.
importScripts("pkg/wfsim_wasm.js");

const ready = wasm_bindgen({ module_or_path: "pkg/wfsim_wasm_bg.wasm" });

onmessage = async (e) => {
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
