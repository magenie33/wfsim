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
  if (msg.kind === "api") {
    // A SIMULATE SAYS HOW FAR IT HAS GOT, and nothing else does. It is the one
    // endpoint whose cost is unbounded — a 361-body fight at the rulers' 1000
    // runs is a minute — and a button that says "Simulating…" for a minute
    // reads as a hang (owner, 2026-08-18). Asked for only when the caller
    // wants it, so every other call keeps the plain path.
    const body = JSON.stringify(msg.body ?? {});
    const out = msg.progress && msg.path === "/api/simulate"
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
