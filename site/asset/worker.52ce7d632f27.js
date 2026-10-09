// SPDX-License-Identifier: AGPL-3.0-or-later
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
importScripts("/pkg/wfsim_wasm.bb24fc266b8a.js");

// A MODULE THAT WILL NOT DOWNLOAD IS SAID OUT LOUD, after one retry. A rejected
// `ready` only rejects each message's await, which the page never sees, so the
// lane sat silent until `LANE_WATCHDOG.loading` ran out; a throw from a task
// reaches the page's `onerror` now, which settles the lane as dead.
// THE DOWNLOAD IS COUNTED, so a slow line shows the reader how far it has got
// rather than a blank page and then a banner (index.html's boot guard).
// `WASM_BYTES` is the module's size, written in by build_site_app.py; 0 on the
// dev server, where the page shows the bytes alone.
const WASM_BYTES = 10112351;
const counted = async () => {
  const r = await fetch("/pkg/wfsim_wasm_bg.bb24fc266b8a.wasm");
  if (!r.ok || !r.body) return r;
  const reader = r.body.getReader();
  let got = 0, said = 0;
  const say = () => { said = Date.now(); postMessage({ kind: "loading", got, total: WASM_BYTES }); };
  return new Response(new ReadableStream({
    async pull(c) {
      const { done, value } = await reader.read();
      if (done) { say(); c.close(); return; }
      got += value.byteLength;
      if (Date.now() - said > 250) say();
      c.enqueue(value);
    },
    cancel: (why) => reader.cancel(why),
  }), { headers: { "content-type": "application/wasm" } });
};
const load = () => wasm_bindgen({ module_or_path: counted() });
const ready = load().catch(load);
ready.catch((err) => setTimeout(() => { throw err; }));
// …AND ONE THAT IS STILL DOWNLOADING SAYS SO, so a slow line is not taken for a
// dead lane: the page's watchdog resets on any word. It says so for five
// minutes at most, so a download that has truly stalled still ends.
const arriving = setInterval(() => postMessage({ kind: "alive" }), 5000);
setTimeout(() => clearInterval(arriving), 300000);
ready.then(() => clearInterval(arriving), () => clearInterval(arriving));

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
