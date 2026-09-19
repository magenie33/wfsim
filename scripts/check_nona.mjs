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
    // A REQUEST WITH NO TOOLS is the summary being asked for.
    if (!body.tools) {
      res.writeHead(200, { ...cors, "Content-Type": "application/json" });
      const t = "SUMMARY: the reader wants a Torid build; DPS 1234 measured by simulator.run.start on build 1.";
      res.end(JSON.stringify(anthropic ? { content: [{ type: "text", text: t }] } : { choices: [{ message: { content: t } }] }));
      return;
    }
    seen.push({ url: req.url, body });
    const done = anthropic
      ? body.messages.flatMap((m) => (Array.isArray(m.content) ? m.content : [])).filter((b) => b.type === "tool_result").length
      : body.messages.filter((m) => m.role === "tool").length;
    const step = SCRIPT[Math.min(done, SCRIPT.length - 1)];
    const id = `call_${done}`;
    // STREAMED WHEN ASKED, in pieces the way a provider sends them: text in two
    // halves, a tool call's arguments split mid-JSON, usage at the end.
    if (body.stream) {
      res.writeHead(200, { ...cors, "Content-Type": "text/event-stream" });
      const send = (o) => res.write(`data: ${JSON.stringify(o)}\n\n`);
      const args = JSON.stringify(step.args || {});
      const half = Math.floor(args.length / 2);
      if (anthropic) {
        send({ type: "message_start", message: { usage: { input_tokens: 200, cache_read_input_tokens: 800, output_tokens: 1 } } });
        if (step.text) {
          send({ type: "content_block_start", index: 0, content_block: { type: "text", text: "" } });
          send({ type: "content_block_delta", index: 0, delta: { type: "text_delta", text: step.text.slice(0, 10) } });
          send({ type: "content_block_delta", index: 0, delta: { type: "text_delta", text: step.text.slice(10) } });
        } else {
          send({ type: "content_block_start", index: 0, content_block: { type: "tool_use", id, name: step.name, input: {} } });
          send({ type: "content_block_delta", index: 0, delta: { type: "input_json_delta", partial_json: args.slice(0, half) } });
          send({ type: "content_block_delta", index: 0, delta: { type: "input_json_delta", partial_json: args.slice(half) } });
        }
        send({ type: "message_delta", usage: { output_tokens: 20 } });
        send({ type: "message_stop" });
      } else {
        if (step.text) {
          send({ choices: [{ delta: { content: step.text.slice(0, 10) } }] });
          send({ choices: [{ delta: { content: step.text.slice(10) } }] });
        } else {
          send({ choices: [{ delta: { tool_calls: [{ index: 0, id, type: "function", function: { name: step.name, arguments: args.slice(0, half) } }] } }] });
          send({ choices: [{ delta: { tool_calls: [{ index: 0, function: { arguments: args.slice(half) } }] } }] });
        }
        send({ choices: [], usage: { prompt_tokens: 1000, completion_tokens: 20, prompt_tokens_details: { cached_tokens: 800 } } });
        res.write("data: [DONE]\n\n");
      }
      res.end();
      return;
    }
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
  localStorage.setItem("wfsim-nona", JSON.stringify({ proto: ${JSON.stringify(provider === "anthropic" ? "anthropic" : "openai")}, base: ${JSON.stringify(base)}, key: "test", remember: true, model: "mock", price: [1, 2] }));
  document.getElementById("nona-fab").click();
  document.getElementById("nona-new").click();
  const suggested = [...document.querySelectorAll("#nona-suggest [data-q]")].map(e => e.dataset.q);
  document.getElementById("nona-input").value = "seat serration";
  document.getElementById("nona-send").click();
  for (let i = 0; i < 80 && (nona.busy || !document.querySelector("#nona-log .assistant")); i++) await wait(250);
  const log = [...document.querySelectorAll("#nona-log .nona-msg")].map(e => e.className + " | " + e.textContent);
  const foot = document.getElementById("nona-foot-note").textContent;
  const card = !!document.querySelector("#nona-log .card [data-card=apply]");
  const cardText = (document.querySelector("#nona-log .card") || {}).textContent || "";
  const onCopy = window.wfsim.observe().build;
  await window.wfsim.do("shell.preset.open", { bar: "build", preset: mine });
  const mineAfter = window.wfsim.observe().build;
  // THE READER TAKES THE CHANGE: the card writes the copy over their build.
  const apply = document.querySelector("#nona-log .card [data-card=apply]");
  if (apply) apply.click();
  await wait(300);
  const taken = window.wfsim.observe().build;
  document.getElementById("nona-close").click();
  return {
    log, mine, copy: onCopy.preset, foot, suggested, card, cardText,
    taken: taken.preset === mine && taken.slots.some(s => s.mod === "serration"),
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
  check(`${provider}: she is sent every door tool, plus the observation and her history search`,
    names.length === r.tools + 2 && names.includes("builder_board_read") && names.includes("shell_page_observe")
    && names.includes("shell_history_search"), `${names.length} vs ${r.tools + 2}`);
  check(`${provider}: the loop runs to an answer`, sent.length === 3 && r.log.some((l) => /assistant \| Serration is seated/.test(l)),
    JSON.stringify(r.log));
  check(`${provider}: each call is a line in the trail, and landed`,
    r.log.filter((l) => /nona-msg tool ok/.test(l)).length === 2, JSON.stringify(r.log));
  check(`${provider}: she worked on a copy, and says so`,
    r.copy && r.copy !== r.mine && r.copyHasIt === true && r.log.some((l) => /note \|.*copy/.test(l)),
    JSON.stringify([r.mine, r.copy, r.copyHasIt]));
  check(`${provider}: the reader's own build is untouched`, r.mineClean === true);
  check(`${provider}: an empty conversation offers questions read off the page`, r.suggested.length >= 1, JSON.stringify(r.suggested));
  check(`${provider}: a change card shows what she changed, and applying it gives the reader the copy`,
    r.card === true && /Serration|膛线/.test(r.cardText) && r.taken === true, JSON.stringify([r.cardText, r.taken]));
  check(`${provider}: every request streams, and each reader message carries the page`,
    sent.every((x) => x.body.stream === true) && JSON.stringify(sent[0].body.messages).includes("<page>"));
  check(`${provider}: each reply shows what it cost, and the chat its total and the AI label`,
    r.log.some((l) => /nona-msg usage \| .*tokens.*80%.*≈\$/.test(l)) && /tokens/.test(r.foot) && r.foot.length > 20,
    JSON.stringify([r.log.filter((l) => /usage/.test(l)), r.foot]));
  if (provider === "anthropic") {
    const b = sent[0].body;
    check("anthropic: the rules, the tools and the newest block carry cache breakpoints",
      b.system[0].cache_control && b.tools[b.tools.length - 1].cache_control
      && JSON.stringify(b.messages[b.messages.length - 1]).includes("cache_control"));
  }
}

// ---- conversations outlive the page, and reopen as they were -----------------

await app.load("/weapons/Torid");
const kept = await evaluate(`(async () => {
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  document.getElementById("nona-fab").click();
  for (let i = 0; i < 40 && !nona.list.length; i++) await wait(100);
  const list = nona.list.map(c => ({ id: c.id, title: c.title, n: c.messages.length }));
  if (!list.length) return { list, log: [] };
  await nonaOpen(list[0].id);
  const log = [...document.querySelectorAll("#nona-log .nona-msg")].map(e => e.className);
  document.getElementById("nona-close").click();
  return { list, log };
})()`, { awaitPromise: true });
check("both conversations were kept, titled from what was asked", kept.list.length >= 2 && kept.list.every((c) => /seat serration/.test(c.title)),
  JSON.stringify(kept.list));
check("a kept conversation reopens with its trail", kept.log.filter((c) => /tool ok/.test(c)).length === 2 && kept.log.some((c) => /assistant/.test(c)),
  JSON.stringify(kept.log));

// ---- the second step: early turns summarised, the record kept whole ----------

const sum = await evaluate(`(async () => {
  const saved = nona.conv;
  nona.conv = nonaNewConversation();
  for (let i = 0; i < 7; i++) {
    nona.conv.messages.push({ role: "user", text: "question " + i + (i === 1 ? " about the Plinx" : ""), page: "{}" });
    nona.conv.messages.push({ role: "assistant", text: "", calls: [{ id: "s" + i, name: "builder_stats_read", args: {} }] });
    nona.conv.messages.push({ role: "tool", id: "s" + i, name: "builder_stats_read", ok: true, line: "x", result: "y".repeat(4000) });
    nona.conv.messages.push({ role: "assistant", text: "answer " + i });
  }
  const cfg = { context: 12000, model: "sum-test", proto: "openai", base: ${JSON.stringify(MOCK + "/v1")}, key: "t" };
  nonaFitBudget(cfg, false);
  const cut = nonaWantsSummary(cfg);
  const users = nona.conv.messages.map((m, i) => (m.role === "user" ? i : -1)).filter(i => i >= 0);
  if (cut != null) await nonaSummarize(cfg, cut, new AbortController().signal);
  const sent = nonaSent(nona.conv);
  const found = nonaHistorySearch("Plinx");
  const out = { cut, expect: users[users.length - 4], summary: (nona.conv.summary || {}).text || "",
    first: (sent[0] || {}).text || "", sentUsers: sent.filter(m => m.role === "user").length, found: found.found };
  nona.conv = saved;
  return out;
})()`, { awaitPromise: true });
check("past the budget, all but the newest four turns are summarised", sum.cut === sum.expect && /SUMMARY/.test(sum.summary),
  JSON.stringify(sum));
check("...what is sent starts from the summary and keeps the four turns", /<summary/.test(sum.first) && sum.sentUsers === 4,
  JSON.stringify([sum.first.slice(0, 60), sum.sentUsers]));
check("...and the record stays whole: an early turn is still searchable", sum.found >= 1, JSON.stringify(sum.found));

// ---- a number she did not measure is marked -----------------------------------

const marks = await evaluate(`(() => {
  const html = nonaMarkNumbers(escHtml("1,234.5 DPS at 35% crit, 999.9 per hit, 12.0k total, slot 3"), [1234.5, 0.35, 12003]);
  return [...html.matchAll(/nona-unmeasured[^>]*>([^<]+)</g)].map(m => m[1]);
})()`);
check("a number no tool returned is marked; measured ones, percentages of fractions, k-scaled and small counts are not",
  JSON.stringify(marks) === JSON.stringify(["999.9"]), JSON.stringify(marks));

// ---- the budget: old tool results are set aside, newest kept -----------------

const budget = await evaluate(`(() => {
  const saved = nona.conv;
  nona.conv = nonaNewConversation();
  for (let i = 0; i < 10; i++) {
    nona.conv.messages.push({ role: "assistant", text: "", calls: [{ id: "t" + i, name: "builder_stats_read", args: {} }] });
    nona.conv.messages.push({ role: "tool", id: "t" + i, name: "builder_stats_read", ok: true, line: "x", result: "x".repeat(6000) });
  }
  const cfg = { context: 20000, model: "budget-test" };
  nonaFitBudget(cfg, false);
  const soft = nona.conv.messages.filter(m => m.role === "tool").map(m => !!m.masked);
  nonaFitBudget(cfg, true);
  const hard = nona.conv.messages.filter(m => m.role === "tool").map(m => !!m.masked);
  const sent = nonaToolText(nona.conv.messages[1]);
  nona.conv = saved;
  return { soft, hard, sent };
})()`);
check("past half the window the oldest tool results are set aside and the newest six kept",
  budget.soft.slice(0, 4).every(Boolean) && budget.soft.slice(4).every((x) => !x), JSON.stringify(budget.soft));
check("...forced, only the newest two stay", budget.hard.filter((x) => !x).length === 2, JSON.stringify(budget.hard));
check("...and a set-aside result says how to get it back", /set aside/.test(budget.sent) && /call it again/.test(budget.sent), budget.sent);

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
  out.sessionKey = sessionStorage.getItem("wfsim-nona-key");
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
check("the model picked is saved with the address, protocol, context and price",
  set.model === "vendor/mock-tools" && set.saved.model === "vendor/mock-tools" && set.saved.proto === "openai"
  && set.saved.context === 128000 && Array.isArray(set.saved.price), JSON.stringify(set.saved));
check("the key is kept for this tab only, unless the reader asks to remember it",
  !("key" in set.saved) && set.sessionKey === "test", JSON.stringify([set.saved, set.sessionKey]));

mock.close();
await finish("Nona works the page through the door, on a copy");
