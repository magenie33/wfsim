// SPDX-License-Identifier: AGPL-3.0-or-later
// Time immutable standalone WASM snapshots in an owned headless Chrome process.
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { createHash, randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
import { launchChrome, stopChrome } from "./cdp_boot.mjs";
import { CHROME } from "./cdp.mjs";

const [baseline, candidate, callsFile, output, repeatText = "3", roundsText = "2"] = process.argv.slice(2);
if (!output) throw new Error("baseline-dir candidate-dir calls-with-body_text.json new-output-dir [repeats] [rounds]");
const calls = JSON.parse(fs.readFileSync(callsFile, "utf8"));
if (calls.some(c => typeof c.body_text !== "string")) throw new Error("preserved body_text required");
fs.mkdirSync(output, { recursive: false });
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const manifests = {};
for (const [side, folder] of Object.entries({ A: baseline, B: candidate })) {
  const manifest = JSON.parse(fs.readFileSync(path.join(folder, "manifest.json"), "utf8"));
  for (const [name, expected] of Object.entries(manifest.files)) {
    if (path.basename(name) !== name || hash(fs.readFileSync(path.join(folder, name))) !== expected) throw new Error("changed snapshot dependency");
  }
  manifests[side] = manifest;
}
const report = { complete: false, calls_sha256: hash(fs.readFileSync(callsFile)), manifests, order: [], observations: [],
  tool_sha256: hash(fs.readFileSync(fileURLToPath(import.meta.url))), node: process.version };
const save = () => fs.writeFileSync(path.join(output, "report.json"), JSON.stringify(report, null, 2));
save();
const started = await launchChrome(CHROME, ["--remote-debugging-port=0", "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check"], path.join(os.tmpdir(), `wfsim-native-compare-${randomUUID()}`));
let ws;
try {
  ws = new WebSocket(started.page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
  let counter = 0;
  const pending = new Map();
  ws.onmessage = e => {
    const row = JSON.parse(e.data);
    if (row.id && pending.has(row.id)) {
      const { resolve, reject, timer } = pending.get(row.id);
      pending.delete(row.id); clearTimeout(timer);
      if (row.error || row.result?.exceptionDetails) reject(new Error(JSON.stringify(row.error || row.result.exceptionDetails)));
      else resolve(row.result);
    }
  };
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++counter;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} timeout`)); }, 120000);
    pending.set(id, { resolve, reject, timer });
    ws.send(JSON.stringify({ id, method, params }));
  });
  const evaluate = async expression => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result.value;
  report.browser = await send("Browser.getVersion");
  let index = 0;
  for (const side of "ABBA".repeat(Number(roundsText))) {
    const folder = side === "A" ? baseline : candidate;
    const glue = fs.readFileSync(path.join(folder, "wfsim_wasm.js"), "utf8");
    const module = fs.readFileSync(path.join(folder, "wfsim_wasm_bg.wasm"));
    const init = await evaluate(`(async () => { ${glue}\nconst bytes=Uint8Array.from(atob(${JSON.stringify(module.toString("base64"))}), c=>c.charCodeAt(0));
      const start=performance.now(); const wasm=wasm_bindgen.initSync({module:await WebAssembly.compile(bytes)});
      globalThis.compareEngine={api:(p,b)=>wasm_bindgen.api(p,b),memory:()=>wasm.memory.buffer.byteLength};
      return {milliseconds:performance.now()-start,memory_bytes:wasm.memory.buffer.byteLength}; })()`);
    const rows = [];
    for (const call of calls) {
      const result = await evaluate(`(() => {
        const path=${JSON.stringify(call.path)}, body=${JSON.stringify(call.body_text)};
        const result=compareEngine.api(path,body); const times=[];
        for(let i=0;i<${Number(repeatText)};i++){const at=performance.now();const next=compareEngine.api(path,body);times.push(performance.now()-at);if(next!==result)throw new Error("nondeterministic API");}
        return {result,milliseconds:times,memory_bytes:compareEngine.memory()}; })()`);
      if (JSON.parse(result.result).ok !== true) throw new Error(`${call.id}: endpoint failed`);
      rows.push(`{"id":${JSON.stringify(call.id)},"milliseconds":${JSON.stringify(result.milliseconds)},"memory_bytes":${result.memory_bytes},"result":${result.result}}`);
    }
    const name = `${String(index++).padStart(3, "0")}-${side}.json`;
    const raw = `[${rows.join(",")}]`;
    fs.writeFileSync(path.join(output, name), raw);
    report.order.push(side);
    report.observations.push({ side, result_file: name, result_sha256: hash(raw), init });
    save();
    console.log(`${name}: ${calls.length} cases`);
  }
  report.complete = true; save();
} finally {
  ws?.close();
  await stopChrome(started.proc, started.profile);
}
