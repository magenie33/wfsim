// NONA'S CORE, TESTED WITHOUT A BROWSER.
//
// `web/src/static/nona/core/` is pure — no window, no fetch, no storage, no
// clock — so everything that decides what a model is sent, what is kept and
// what a stored shape becomes is checked here in milliseconds. docs/NONA.md
// §"The invariants" names which of them this is the enforcement for.
import * as record from "../web/src/static/nona/core/record.js";
import { view, sent, viewText } from "../web/src/static/nona/core/view.js";
import * as budget from "../web/src/static/nona/core/budget.js";
import * as summary from "../web/src/static/nona/core/summary.js";
import { markNumbers, numbersIn } from "../web/src/static/nona/core/measure.js";
import * as memory from "../web/src/static/nona/core/memory.js";
import { rules } from "../web/src/static/nona/core/prompt.js";
import { allTools, toolName, toolId, OWN } from "../web/src/static/nona/core/tools.js";
import { sseSplit } from "../web/src/static/nona/core/protocols/sse.js";
import * as openai from "../web/src/static/nona/core/protocols/openai.js";
import * as anthropic from "../web/src/static/nona/core/protocols/anthropic.js";

let failed = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failed++;
  console.log(`${ok ? "  ok" : "FAIL"}  ${name}${ok ? "" : `  — ${detail}`}`);
};
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
/// A FROZEN record: a core function that writes into its input throws here.
const freeze = (o) => { if (o && typeof o === "object") { Object.values(o).forEach(freeze); Object.freeze(o); } return o; };

const DOOR = [{ name: "builder.mod.set", description: "seat a mod", input_schema: { type: "object", properties: {} } }];
const TOOLS = allTools(DOOR);
const RULES = rules({ lang: "zh", concise: false });

function convo(turns, resultSize = 6000) {
  const r = record.newConversation({ id: "c1", now: 1, weapon: "torid" });
  for (let i = 0; i < turns; i++) {
    r.messages.push({ role: "user", text: `question ${i}`, page: `{"p":${i}}`, at: i });
    r.messages.push({ role: "assistant", text: "", calls: [{ id: `t${i}`, name: "builder_stats_read", args: {} }] });
    r.messages.push({ role: "tool", id: `t${i}`, name: "builder_stats_read", ok: true, line: "x", result: "y".repeat(resultSize) });
    r.messages.push({ role: "assistant", text: `answer ${i}`, calls: [] });
  }
  return r;
}

// ---- 7: stored shapes, versioned and frozen --------------------------------------

check("a conversation written before versions migrates to v1 with its missing fields",
  same(Object.keys(record.migrate("conversation", { id: "a", messages: [] })).sort(),
    ["id", "made", "messages", "pairs", "pinned", "summary", "usage", "v"]));
check("a memory written as schema 1 migrates to v1", (() => {
  const m = record.migrate("memory", { schema: 1, items: [{ id: "x" }], paused: true });
  return m.v === 1 && !("schema" in m) && m.items.length === 1 && m.paused === true;
})());
check("a shape newer than the code is left alone, not guessed at",
  record.migrate("conversation", { v: 99 }) === null && record.migrate("memory", { v: 99 }) === null);
// THE NAMES AS SHIPPED. A name dropped from FROZEN fails here; adding one is
// a new version, never an edit of this list.
const SHIPPED = {
  conversation: ["v", "id", "title", "pinned", "created_at", "updated_at", "weapon", "made", "pairs", "summary", "usage", "messages"],
  memory: ["v", "paused", "items"],
  memoryItem: ["id", "kind", "key", "value", "status", "source", "created_at", "updated_at", "history"],
  settings: ["v", "base", "proto", "model", "context", "price", "remember", "concise", "key"],
};
check("no shipped name has left FROZEN",
  Object.entries(SHIPPED).every(([k, names]) => names.every((n) => record.FROZEN[k].includes(n))));
check("a new conversation has exactly the frozen fields",
  same(Object.keys(record.newConversation({ id: "a", now: 0 })).sort(), [...record.FROZEN.conversation].sort()));

// ---- 5: the view is a pure function of the record --------------------------------

const r1 = freeze(convo(3, 100));
const v1 = view(r1, { rules: RULES, memory: "", tools: TOOLS });
check("building the view leaves the record untouched and gives the same view twice",
  same(v1, view(r1, { rules: RULES, memory: "", tools: TOOLS })));
check("each reader turn carries its page, and tool results their text",
  v1.turns[0].text.includes('<page>{"p":0}</page>') && v1.turns[2].role === "tool" && v1.turns[2].text.length === 100);
check("notes, cards and memory chips are drawn, never sent", (() => {
  const r = convo(1, 10);
  r.messages.push({ role: "note", text: "n" }, { role: "card", pair: {} }, { role: "memory", id: "m" });
  return sent(r).every((m) => record.SENT_ROLES.includes(m.role));
})());

// ---- 6: the prefix is stable -------------------------------------------------------

const r2 = convo(4, 100);
const r2a = { ...r2, messages: r2.messages.slice(0, 8) };
for (const [name, p] of [["openai", openai], ["anthropic", anthropic]]) {
  const a = p.encode(view(r2a, { rules: RULES, memory: "<memory>x</memory>", tools: TOOLS }), { model: "m" });
  const b = p.encode(view(r2, { rules: RULES, memory: "<memory>x</memory>", tools: TOOLS }), { model: "m" });
  const strip = (ms) => JSON.stringify(ms).replace(/,"cache_control":\{"type":"ephemeral"\}/g, "");
  const prefix = name === "openai" ? a.messages : a.messages.slice(0, -1);
  check(`${name}: a later request repeats the earlier one's rules, tools and turns byte for byte`,
    same(a.system, b.system) && same(a.tools, b.tools)
    && strip(b.messages).startsWith(strip(prefix).slice(0, -1)), "prefix moved");
}
check("the rules are the same text every time for one reader", rules({ lang: "zh", concise: false }) === RULES
  && rules({ lang: "zh", concise: true }) !== RULES);

// ---- the budget: oldest set aside first, newest kept ------------------------------

const big = freeze(convo(10));
const fixed = RULES + JSON.stringify(TOOLS);
const soft = budget.decide(big, { context: 20000, fixed, ratio: 1 });
const toolIdx = big.messages.map((m, i) => (m.role === "tool" ? i : -1)).filter((i) => i >= 0);
check("past half the window the oldest tool results are set aside and the newest six kept",
  same(soft.tools, toolIdx.slice(0, 4)), JSON.stringify(soft.tools));
check("...and forced, only the newest two stay", budget.decide(big, { context: 20000, fixed, ratio: 1, force: true }).tools.length === 8);
check("under half the window, nothing is set aside",
  budget.decide(convo(2, 50), { context: 200000, fixed, ratio: 1 }).tools.length === 0);
const marked = budget.applyMarks(big, soft);
check("marks are written into a new record, the old one untouched",
  marked.messages[toolIdx[0]].masked === true && !big.messages[toolIdx[0]].masked);
check("a set-aside result says how to get it back",
  /set aside.*call it again/.test(view(marked, { rules: RULES, memory: "", tools: TOOLS }).turns[2].text));
check("the estimate reads CJK at about twice Latin",
  budget.estimate("你".repeat(100)) > budget.estimate("a".repeat(100)) * 1.8);
check("calibration moves toward what the provider billed",
  budget.calibrate(1, 1000, 2000) > 1 && budget.calibrate(1, 1000, 500) < 1);
check("a reply's cost counts cached input at a tenth",
  Math.abs(budget.cost([1, 2], { input: 1000, cached: 1000, output: 0 }) - 0.0001) < 1e-12);

// ---- the summary: all but the newest four turns -----------------------------------

const long = freeze(convo(7, 4000));
const users = long.messages.map((m, i) => (m.role === "user" ? i : -1)).filter((i) => i >= 0);
const cut = summary.wantsSummary(long, { context: 12000, fixed: "", ratio: 1 });
check("past the budget the cut is the fourth-newest reader turn", cut === users[users.length - 4], String(cut));
const input = summary.summaryInput(long, cut, (c) => c.name);
check("the summariser is given the turns before the cut and none after",
  input.includes("[reader] question 0") && !input.includes("question 3") && input.startsWith("<record>"));
const summed = summary.applySummary(long, cut, "SUMMARY");
const afterSum = sent(summed);
check("what is sent starts from the summary and keeps the four turns",
  afterSum[0].text.startsWith("<summary of the conversation so far>\nSUMMARY") && afterSum.filter((m) => m.role === "user").length === 4);
check("an already-summarised record is not summarised again at the same cut",
  summary.wantsSummary(summed, { context: 12000, fixed: "", ratio: 1 }) === null);

// ---- 2: a number no tool returned is marked ---------------------------------------

const wrap = (s) => `[${s}]`;
check("only the unmeasured number is marked",
  markNumbers("1,234.5 DPS at 35% crit, 999.9 per hit, 240 per tick, 12.0k total, slot 3", [1234.5, 0.35, 12003], wrap)
  === "1,234.5 DPS at 35% crit, [999.9] per hit, [240] per tick, 12.0k total, slot 3");
check("the numbers a reply is checked against are the tool results before it",
  same(numbersIn([{ role: "tool", result: "a 12.5 b" }, { role: "tool", result: "7" }], 1), [12.5]));

// ---- the protocols ----------------------------------------------------------------

const small = { system: ["R"], tools: [{ name: "t", description: "d", input_schema: { type: "object" } }],
  turns: [{ role: "user", text: "hi" }, { role: "assistant", text: "", calls: [{ id: "c", name: "t", args: { a: 1 } }] },
    { role: "tool", id: "c", name: "t", text: "ok" }, { role: "tool", id: "d", name: "t", text: "ok2" }] };
check("openai: the body is the golden one", same(openai.encode(small, { model: "m" }), {
  model: "m",
  messages: [{ role: "system", content: "R" }, { role: "user", content: "hi" },
    { role: "assistant", content: null, tool_calls: [{ id: "c", type: "function", function: { name: "t", arguments: '{"a":1}' } }] },
    { role: "tool", tool_call_id: "c", content: "ok" }, { role: "tool", tool_call_id: "d", content: "ok2" }],
  stream: true, stream_options: { include_usage: true },
  tools: [{ type: "function", function: { name: "t", description: "d", parameters: { type: "object" } } }],
}));
const ab = anthropic.encode(small, { model: "m" });
check("anthropic: tool results of one turn share a message, and three blocks carry cache breakpoints",
  ab.messages.length === 3 && ab.messages[2].content.length === 2
  && ab.system[0].cache_control && ab.tools[0].cache_control && ab.messages[2].content[1].cache_control
  && !ab.messages[2].content[0].cache_control);
check("anthropic: an address typed with or without /v1 reaches the same endpoint",
  anthropic.chatUrl("https://api.anthropic.com/v1/") === anthropic.chatUrl("https://api.anthropic.com"));

// A STREAM CUT MID-LINE, the way a network delivers one.
function feed(decoder, text, cuts) {
  let rest = "";
  let at = 0;
  for (const c of [...cuts, text.length]) {
    const { payloads, rest: r } = sseSplit(rest + text.slice(at, c));
    rest = r; at = c;
    payloads.forEach((p) => decoder.push(p));
  }
  return decoder.done();
}
const oaStream = [
  { choices: [{ delta: { content: "Hel" } }] },
  { choices: [{ delta: { content: "lo" } }] },
  { choices: [{ delta: { tool_calls: [{ index: 0, id: "c1", function: { name: "builder_", arguments: '{"slot"' } }] } }] },
  { choices: [{ delta: { tool_calls: [{ index: 0, function: { name: "mod_set", arguments: ':0}' } }] } }] },
  { choices: [], usage: { prompt_tokens: 100, completion_tokens: 5, prompt_tokens_details: { cached_tokens: 80 } } },
].map((e) => `data: ${JSON.stringify(e)}\n\n`).join("") + "data: [DONE]\n\n";
const oa = feed(openai.streamDecoder(), oaStream, [7, 50, 120]);
check("openai: a stream cut anywhere decodes to its text, its tool call and its usage",
  oa.text === "Hello" && oa.calls[0].name === "builder_mod_set" && oa.calls[0].args.slot === 0
  && same(oa.usage, { input: 100, output: 5, cached: 80 }), JSON.stringify(oa));
const anStream = [
  { type: "message_start", message: { usage: { input_tokens: 20, cache_read_input_tokens: 80 } } },
  { type: "content_block_start", index: 0, content_block: { type: "text", text: "" } },
  { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: "Hi" } },
  { type: "content_block_start", index: 1, content_block: { type: "tool_use", id: "u1", name: "t", input: {} } },
  { type: "content_block_delta", index: 1, delta: { type: "input_json_delta", partial_json: '{"a"' } },
  { type: "content_block_delta", index: 1, delta: { type: "input_json_delta", partial_json: ":2}" } },
  { type: "message_delta", usage: { output_tokens: 9 } },
].map((e) => `event: x\ndata: ${JSON.stringify(e)}\n\n`).join("");
const an = feed(anthropic.streamDecoder(), anStream, [30, 200]);
check("anthropic: a stream decodes to its text, its tool call and its usage",
  an.text === "Hi" && an.calls[0].args.a === 2 && same(an.usage, { input: 100, output: 9, cached: 80 }), JSON.stringify(an));
check("an error inside a stream is thrown, not swallowed", (() => {
  try { openai.streamDecoder().push({ error: { message: "quota" } }); return false; } catch (e) { return /quota/.test(e.message); }
})());

// ---- memory ------------------------------------------------------------------------

const ctx = (id, lastUserText) => ({ id, lastUserText, conversation: "c1", now: 1000 });
let mem = freeze(memory.emptyMemory());
const said = memory.set(mem, { slot: "riven_policy", value: "不用紫卡", quote: "不用 紫卡" }, ctx("m1", "我平时不用紫卡"));
const guessed = memory.set(said.mem, { slot: "content", value: "钢铁之路", quote: "钢铁之路" }, ctx("m2", "我平时不用紫卡"));
check("a memory the reader's words back takes effect; one they did not say is only proposed",
  said.result.status === "active" && guessed.result.status === "proposed");
check("...and only the confirmed one reaches her",
  /riven_policy: 不用紫卡/.test(memory.block(guessed.mem, 1000)) && !/钢铁之路/.test(memory.block(guessed.mem, 1000)));
const over = memory.set(guessed.mem, { slot: "riven_policy", value: "偶尔用", quote: "不用紫卡" }, ctx("m3", "不用紫卡"));
check("a slot is written over in place and undo brings the old value back",
  over.mem.items.filter((x) => x.key === "riven_policy").length === 1
  && memory.undo(over.mem, "m1", 2000).items.find((x) => x.key === "riven_policy").value === "不用紫卡");
check("paused, memory is neither read nor written",
  memory.block({ ...said.mem, paused: true }, 1000) === ""
  && memory.set({ ...said.mem, paused: true }, { value: "x" }, ctx("m9", "x")).result.ok === false);
check("an old memory is marked for confirmation",
  /confirm before relying/.test(memory.block(said.mem, 1000 + 100 * 864e5)));
check("forgetting removes it", memory.forget(said.mem, "m1").mem.items.length === 0);
let notes = memory.emptyMemory();
for (let i = 0; i < memory.NOTES_MAX + 5; i++) notes = memory.set(notes, { value: `n${i}`, quote: "x" }, ctx(`n${i}`, "x")).mem;
check("notes are capped, keeping the newest", notes.items.length === memory.NOTES_MAX && notes.items[0].value === "n5");

// ---- 8: her tools --------------------------------------------------------------------

check("her tools are the door's, renamed, then her own in fixed order",
  TOOLS[0].name === "builder_mod_set" && same(TOOLS.slice(1).map((t) => t.name), OWN.map((t) => t.name)));
check("a door id and its tool name map back and forth", toolId(toolName("simulator.arena.add")) === "simulator.arena.add");
check("the whole view estimates as text", viewText(v1).length > 0);

console.log(failed ? `\n${failed} failed` : "\nnona's core holds");
process.exit(failed ? 1 : 0);
