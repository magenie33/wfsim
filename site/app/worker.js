// wfsim wasm worker (docs/WASM.md phase 4). Owns one wasm engine instance.
// Protocol (from app.js's api() shim):
//   { id, kind: "api", path, body }  → { id, payload }          (quick endpoints)
//   { kind: "optimize", body }       → { kind: "progress", payload }*  then
//                                      { kind: "result", payload }
// The optimize call blocks this worker until done — that is the design: the
// page runs it in a DEDICATED worker and cancels by terminating it.
importScripts("pkg/wfsim_wasm.js");

const ready = wasm_bindgen({ module_or_path: "pkg/wfsim_wasm_bg.wasm" });

onmessage = async (e) => {
  await ready;
  const msg = e.data;
  if (msg.kind === "api") {
    const out = wasm_bindgen.api(msg.path, JSON.stringify(msg.body ?? {}));
    postMessage({ id: msg.id, payload: JSON.parse(out) });
  } else if (msg.kind === "optimize") {
    const out = wasm_bindgen.optimize(JSON.stringify(msg.body ?? {}), (p) => {
      postMessage({ kind: "progress", payload: JSON.parse(p) });
    });
    postMessage({ kind: "result", payload: JSON.parse(out) });
  }
};
