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
    "Access-Control-Allow-Methods": "POST, OPTIONS",
  };
  if (req.method === "OPTIONS") { res.writeHead(204, cors); res.end(); return; }
  let raw = "";
  req.on("data", (c) => { raw += c; });
  req.on("end", () => {
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
  localStorage.setItem("wfsim-nona", JSON.stringify({ provider: ${JSON.stringify(provider)}, base: ${JSON.stringify(base)}, key: "test", model: "mock" }));
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

mock.close();
await finish("Nona works the page through the door, on a copy");
