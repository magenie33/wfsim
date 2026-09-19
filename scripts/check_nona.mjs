// NONA (九九) DRIVES THE PAGE THROUGH THE DOOR, ON A COPY, WITH TOOLS SHE WAS
// NEVER TOLD ABOUT BY NAME.
//
// A real provider cannot run in CI, so a local one stands in: it speaks both
// protocols the panel speaks (OpenAI chat completions, Anthropic messages),
// answers each request from a script, and records what it was sent. That makes
// three things checkable that a real model would only make likely —
//
//   * her tools are the door's table at the moment of the request, so a query
//     added to the door is in what she is sent with no edit to her section;
//   * a tool call she makes lands on the page the reader is watching;
//   * her first change branches the reader's build — the reader's own build is
//     untouched afterwards, whatever the model decided.
import { createServer } from "node:http";
import { openApp } from "./cdp.mjs";

const seen = [];
// THE SCRIPT: seat a mod, read the panel, then answer. Keyed by how many tool
// results the conversation already holds, which is what a model would see.
const SCRIPT = [
  { name: "builder_mod_set", args: { slot: 0, mod: "serration" } },
  { name: "builder_stats_read", args: {} },
  { text: "Serration is seated on the copy." },
];

const mock = createServer((req, res) => {
  const cors = {
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Headers": "*",
    "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  };
  if (req.method === "OPTIONS") { res.writeHead(204, cors); res.end(); return; }
  let raw = "";
  req.on("data", (c) => { raw += c; });
  req.on("end", () => {
    // THE MODEL LIST, in the shape each protocol answers it: OpenRouter's rows
    // carry what a model supports, and one here says it cannot call tools.
    if (req.method === "GET" && req.url.startsWith("/v1/models")) {
      const anth = !!req.headers["x-api-key"];
      const data = anth
        ? [{ id: "mock-claude", display_name: "Mock Claude" }]
        : [{ id: "vendor/mock-tools", name: "Mock Tools", context_length: 128000,
            pricing: { prompt: "0.000001", completion: "0.000002" }, supported_parameters: ["tools"] },
           { id: "vendor/mock-notools", name: "Mock No Tools", supported_parameters: ["temperature"] }];
      res.writeHead(200, { ...cors, "Content-Type": "application/json" });
      res.end(JSON.stringify({ data }));
      return;
    }
    const body = JSON.parse(raw || "{}");
    const anthropic = req.url.endsWith("/v1/messages");
    seen.push({ url: req.url, body });
    const done = anthropic
      ? body.messages.flatMap((m) => (Array.isArray(m.content) ? m.content : [])).filter((b) => b.type === "tool_result").length
      : body.messages.filter((m) => m.role === "tool").length;
    const step = SCRIPT[Math.min(done, SCRIPT.length - 1)];
    const id = `call_${done}`;
    const out = anthropic
      ? { content: step.text ? [{ type: "text", text: step.text }] : [{ type: "tool_use", id, name: step.name, input: step.args }] }
      : { choices: [{ message: step.text ? { role: "assistant", content: step.text }
        : { role: "assistant", content: null, tool_calls: [{ id, type: "function", function: { name: step.name, arguments: JSON.stringify(step.args) } }] } }] };
    res.writeHead(200, { ...cors, "Content-Type": "application/json" });
    res.end(JSON.stringify(out));
  });
});
await new Promise((r) => mock.listen(0, "127.0.0.1", r));
const MOCK = `http://127.0.0.1:${mock.address().port}`;

const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const { evaluate, check, finish } = app;
await app.load("/weapons/Torid");

const run = (provider, base) => evaluate(`(async () => {
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  // THE READER'S OWN BUILD, saved, before she is asked anything.
  await window.wfsim.do("shell.preset.new", { bar: "build" });
  await window.wfsim.do("builder.mods.clear", {});
  await wait(600);
  const mine = window.wfsim.observe().build.preset;
  localStorage.setItem("wfsim-nona", JSON.stringify({ proto: ${JSON.stringify(provider === "anthropic" ? "anthropic" : "openai")}, base: ${JSON.stringify(base)}, key: "test", model: "mock" }));
  document.getElementById("nona-fab").click();
  document.getElementById("nona-new").click();
  document.getElementById("nona-input").value = "seat serration";
  document.getElementById("nona-send").click();
  for (let i = 0; i < 80 && (nona.busy || !document.querySelector("#nona-log .assistant")); i++) await wait(250);
  const log = [...document.querySelectorAll("#nona-log .nona-msg")].map(e => e.className + " | " + e.textContent);
  const onCopy = window.wfsim.observe().build;
  await window.wfsim.do("shell.preset.open", { bar: "build", preset: mine });
  const mineAfter = window.wfsim.observe().build;
  document.getElementById("nona-close").click();
  return {
    log, mine, copy: onCopy.preset,
    copyHasIt: onCopy.slots.some(s => s.mod === "serration"),
    mineClean: mineAfter.slots.every(s => !s.mod),
    tools: window.wfsim.tools().length,
  };
})()`, { awaitPromise: true });

for (const [provider, base, path] of [["openrouter", `${MOCK}/v1`, "/v1/chat/completions"], ["anthropic", MOCK, "/v1/messages"]]) {
  seen.length = 0;
  const r = await run(provider, base);
  const sent = seen.filter((x) => x.url === path);
  const tools = (sent[0] && sent[0].body.tools) || [];
  const names = tools.map((t) => (t.function ? t.function.name : t.name));
  check(`${provider}: she is sent every door tool, plus the observation`,
    names.length === r.tools + 1 && names.includes("builder_board_read") && names.includes("shell_page_observe"),
    `${names.length} vs ${r.tools + 1}`);
  check(`${provider}: the loop runs to an answer`, sent.length === 3 && r.log.some((l) => /assistant \| Serration is seated/.test(l)),
    JSON.stringify(r.log));
  check(`${provider}: each call is a line in the trail, and landed`,
    r.log.filter((l) => /nona-msg tool ok/.test(l)).length === 2, JSON.stringify(r.log));
  check(`${provider}: she worked on a copy, and says so`,
    r.copy && r.copy !== r.mine && r.copyHasIt === true && r.log.some((l) => /note \|.*copy/.test(l)),
    JSON.stringify([r.mine, r.copy, r.copyHasIt]));
  check(`${provider}: the reader's own build is untouched`, r.mineClean === true);
}

// ---- the settings: an address, a key, what it serves, a model picked ---------

const set = await evaluate(`(async () => {
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  const out = {};
  localStorage.removeItem("wfsim-nona");
  document.getElementById("nona-fab").click();
  await wait(200);
  out.settingsFirst = !document.getElementById("nona-settings").hidden;
  // A QUICK FILL IS ONLY AN ADDRESS: the address box then holds it.
  document.querySelector("[data-nona-base]").click();
  out.filled = document.getElementById("nona-base").value.startsWith("https://");
  const base = document.getElementById("nona-base");
  base.value = ${JSON.stringify(MOCK + "/v1")}; base.dispatchEvent(new Event("change"));
  const key = document.getElementById("nona-apikey");
  key.value = "test"; key.dispatchEvent(new Event("change"));
  for (let i = 0; i < 40 && nonaDraft.state !== "ok" && nonaDraft.state !== "error"; i++) await wait(100);
  out.state = nonaDraft.state; out.proto = nonaDraft.proto; out.status = document.querySelector(".nona-status").textContent;
  out.saveOff = document.getElementById("nona-save").disabled;
  // THE PAGE'S OWN SEARCHABLE DROPDOWN: open it, search, pick.
  document.getElementById("nona-model-dd").click();
  await wait(150);
  const search = document.querySelector("#dd-popover input");
  if (search) { search.value = "tools"; search.dispatchEvent(new Event("input")); }
  await wait(150);
  const rows = [...document.querySelectorAll("#dd-menu .opt[data-v]")];
  out.noToolsGreyed = rows.some(r => r.dataset.v === "vendor/mock-notools" && r.classList.contains("dis"));
  out.popoverOnTop = Number(getComputedStyle(document.getElementById("dd-popover")).zIndex) > Number(getComputedStyle(document.getElementById("nona")).zIndex);
  const pick = rows.find(r => r.dataset.v === "vendor/mock-tools");
  if (pick) pick.click();
  await wait(100);
  out.model = nonaDraft.model;
  document.getElementById("nona-save").click();
  out.saved = JSON.parse(localStorage.getItem("wfsim-nona") || "{}");
  document.getElementById("nona-close").click();
  return out;
})()`, { awaitPromise: true });

check("with no key saved, the panel opens on its settings", set.settingsFirst === true);
check("a quick fill puts an address in the address box", set.filled === true);
check("the address and key are checked, and the protocol is detected",
  set.state === "ok" && set.proto === "openai" && /2/.test(set.status), JSON.stringify([set.state, set.proto, set.status]));
check("nothing is saved before a model is chosen", set.saveOff === true);
check("the model list is the page's searchable dropdown, over the panel, with a tool-less model greyed",
  set.noToolsGreyed === true && set.popoverOnTop === true, JSON.stringify([set.noToolsGreyed, set.popoverOnTop]));
check("the model picked is saved with the address, key and protocol",
  set.model === "vendor/mock-tools" && set.saved.model === "vendor/mock-tools" && set.saved.proto === "openai" && set.saved.key === "test",
  JSON.stringify(set.saved));

mock.close();
await finish("Nona works the page through the door, on a copy");
