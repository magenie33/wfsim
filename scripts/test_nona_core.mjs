// NONA'S CORE, TESTED WITHOUT A BROWSER.
//
// `web/src/static/nona/core/` is pure — no window, no fetch, no storage, no
// clock — so everything that decides what a model is sent, what is kept and
// what a stored shape becomes is checked here in milliseconds. docs/NONA.md
// §"The invariants" names which of them this is the enforcement for.
import * as record from "../web/src/static/nona/core/record.js";
import { view, sent, viewText, userText, toolText } from "../web/src/static/nona/core/view.js";
import * as budget from "../web/src/static/nona/core/budget.js";
import * as summary from "../web/src/static/nona/core/summary.js";
import { CAPS, clip } from "../web/src/static/nona/core/size.js";
import { markNumbers, numbersIn } from "../web/src/static/nona/core/measure.js";
import * as memory from "../web/src/static/nona/core/memory.js";
import { rules } from "../web/src/static/nona/core/prompt.js";
import { fixedTools } from "../web/src/static/nona/core/tools.js";
import { catalogue, skillDoc, docsOf } from "../web/src/static/nona/core/skills.js";
import { calc } from "../web/src/static/nona/core/calc.js";
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

const SKILLS = [{ id: "builder", what: "the build on screen", actions: ["builder.mod.set", "builder.stats.read"] },
  { id: "simulator", what: "the fight", actions: ["simulator.run.start"] }];
const TOOLS = fixedTools(SKILLS, catalogue(SKILLS));
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
    ["id", "made", "memory", "messages", "pairs", "pinned", "summary", "usage", "v"]));
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

check("a reply with neither words nor calls is not sent; the page's check is, as the reader's side of the turn", (() => {
  const r = { ...convo(0), messages: [{ role: "user", text: "q", page: "{}" }, { role: "assistant", text: " ", calls: [] },
    { role: "check", text: "Your last reply was empty." }] };
  const v = view(freeze(r), { rules: RULES, memory: "", tools: TOOLS });
  return v.turns.length === 2 && v.turns[1].role === "user" && v.turns[1].text === "<check>Your last reply was empty.</check>";
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
check("her rules and character fit zone S, in every language and mode",
  [["zh", false], ["zh", true], ["en", false], ["en", true]].every(([lang, concise]) => rules({ lang, concise }).length <= CAPS.S),
  String(RULES.length));
// HER VOICE (docs/NONA.md §"Her voice"): the words calm, the kaomoji her inside,
// never one inside a number, and never an emoji.
check("her voice keeps asides out of numbers and verdicts, owns its mistakes plainly, and speaks its feelings in kaomoji, not emoji",
  /no kaomoji and no aside inside them/.test(RULES) && /corrected plainly, never cutely/.test(RULES)
    && /\(＞﹏＜\)/.test(RULES) && /never an emoji/.test(RULES) && /no fixed title/.test(RULES));
check("concise mode drops the character, not the rules",
  !/contrast/.test(rules({ lang: "zh", concise: true })) && /contrast/.test(RULES) && /never agree to please/.test(rules({ lang: "zh", concise: true })));

// ---- 9: compacted on what the provider billed, however long the conversation ------

const FIXED = { rules: RULES, memory: "<memory of this reader>\n- [m1] riven_policy: none\n</memory>", tools: TOOLS, skills: "" };
/// A reply the provider billed at `n` tokens in all.
const billed = (n) => ({ role: "assistant", text: "a", calls: [], usage: { input: n, output: 0, cached: 0 } });

const brief = freeze(convo(2, 300));
check("before any reply, or with no window stated, nothing is pressed",
  budget.pressed(brief, 64000) === false && budget.pressed(freeze({ ...brief, messages: [...brief.messages, billed(60000)] }), null) === false);
check("pressed once the last reply was billed past the share of the window, and not before",
  budget.pressed(freeze({ ...brief, messages: [...brief.messages, billed(45000)] }), 64000) === true
  && budget.pressed(freeze({ ...brief, messages: [...brief.messages, billed(44000)] }), 64000) === false);

// THIS TURN: one question, eight large results.
const turn = { ...convo(0), messages: [{ role: "user", text: "q", page: "{}" }] };
for (let i = 0; i < 8; i++) {
  turn.messages.push({ role: "assistant", text: "", calls: [{ id: `p${i}`, name: "builder_stats_read", args: {} }] });
  turn.messages.push({ role: "tool", id: `p${i}`, name: "builder_stats_read", ok: true, line: "x", result: "z".repeat(5000) });
}
freeze(turn);
const pTurn = budget.plan(turn);
const pIdx = turn.messages.map((m, i) => (m.role === "tool" ? i : -1)).filter((i) => i >= 0);
check("this turn's results are set aside but the newest two, and a lone turn has nothing to summarise",
  same(pTurn.marks.tools, pIdx.slice(0, 6)) && pTurn.summarize == null, JSON.stringify(pTurn));
check("marks are written into a new record, the old one untouched",
  budget.applyMarks(turn, pTurn.marks).messages[pIdx[0]].masked === true && !turn.messages[pIdx[0]].masked);
check("a set-aside result says how to get it back",
  /set aside.*call it again/.test(view(budget.applyMarks(turn, pTurn.marks), FIXED).turns[2].text));
const again = budget.plan(budget.applyMarks(turn, pTurn.marks));
check("compacting twice does nothing the second time", !again.marks.tools.length && again.summarize == null);

const talky = { ...convo(0), messages: [] };
for (let i = 0; i < 40; i++) talky.messages.push({ role: "user", text: "长".repeat(600), page: "{}" }, { role: "assistant", text: "答".repeat(600), calls: [] });
talky.messages.push({ role: "user", text: "now", page: "{}" });
freeze(talky);
const pTalk = budget.plan(talky);
check("every turn before this one is summarised, at this turn's start", pTalk.summarize === talky.messages.length - 1, String(pTalk.summarize));
const input = summary.summaryInput(talky, 4, (c) => c.name);
check("the summariser is given the turns before the cut and none after",
  (input.match(/\[reader\]/g) || []).length === 2 && input.startsWith("<record>"));
const summed = summary.applySummary(talky, pTalk.summarize, "S".repeat(20000));
check("a summary is cut to its cap whatever the model wrote", summed.summary.text.length <= CAPS.summary);
check("what is sent starts from the summary", view(summed, FIXED).turns[0].text.startsWith("<summary of the conversation so far>"));
const skilled = { ...convo(0), messages: ["builder", "simulator", "optimizer", "rivens"].flatMap((id, i) => [
  { role: "user", text: "q", page: "{}" }, { role: "assistant", text: "", calls: [{ id: `k${i}`, name: "skill_load", args: {} }] },
  { role: "tool", id: `k${i}`, name: "skill_load", ok: true, line: "x", skills: [id], result: `<skill ${id}>\n${"s".repeat(5000)}\n</skill>` }]) };
skilled.messages.push({ role: "user", text: "now", page: "{}" });
const kept = summary.applySummary(freeze(skilled), skilled.messages.length - 1, "S").summary.skills;
check("a summary carries the skills in view up to K, the most recently loaded kept",
  kept.reduce((n, d) => n + d.text.length, 0) <= CAPS.K && kept[kept.length - 1].id === "rivens" && !kept.some((d) => d.id === "builder"),
  JSON.stringify(kept.map((d) => d.id)));

// FOR EVER: a seeded conversation of hundreds of turns, sizes random, sent to a
// stand-in provider of each tokenizer weight and window. It bills what it was
// sent, and refuses a request over its window. After one compaction a request
// always fits, and no message is ever removed and no mark undone.
let seed = 42;
const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
const words = (n) => Array.from({ length: n }, () => (rnd() < 0.5 ? "伤害" : "dmg ")).join("");
const MODS = ["builder", "simulator", "optimizer", "rivens"];
const skillText = (id) => clip(`<skill ${id}>\n${words(Math.floor(rnd() * 3000))}`, CAPS.skill - 10) + "\n</skill>";
let bad = [], summaries = 0, tooLong = 0;
for (const [weight, context] of [[0.3, 64000], [0.6, 64000], [1, 64000], [1, 200000], [0.6, null]]) {
  const tokens = (rec) => Math.ceil(viewText(view(rec, FIXED)).length * weight);
  const window = context || 128000;
  let rec = { ...convo(0), messages: [] };
  /// One request: compacted first when pressed, and once more when refused.
  const send = () => {
    for (let tries = 0; tries < 2; tries++) {
      if (tries || budget.pressed(rec, context)) {
        const before = rec, pl = budget.plan(rec);
        rec = budget.applyMarks(rec, pl.marks);
        if (pl.summarize != null) { rec = summary.applySummary(rec, pl.summarize, words(Math.floor(rnd() * 1500))); summaries++; }
        if (rec.messages.length !== before.messages.length || before.messages.some((x, i) => x.masked && !rec.messages[i].masked)) bad.push("record changed shape");
      }
      const n = tokens(rec);
      if (n <= window) return n;
      tooLong++;
    }
    bad.push(`weight ${weight} window ${window}: over after compacting`);
    return 0;
  };
  for (let t = 0; t < 250 && !bad.length; t++) {
    const mod = MODS[Math.floor(rnd() * MODS.length)];
    rec = { ...rec, messages: [...rec.messages, { role: "user", text: words(Math.floor(rnd() * 3000)), page: words(Math.floor(rnd() * 1500)),
      ...(rnd() < 0.3 ? { preload: skillText(mod), skills: [mod] } : {}) }] };
    const steps = Math.floor(rnd() * 7);
    for (let st = 0; st <= steps; st++) {
      const n = send();
      if (st === steps) { rec = { ...rec, messages: [...rec.messages, { ...billed(n), text: words(Math.floor(rnd() * 800)) }] }; break; }
      const id = `c${t}-${st}`;
      const load = rnd() < 0.15 ? MODS[Math.floor(rnd() * MODS.length)] : null;
      rec = { ...rec, messages: [...rec.messages,
        { ...billed(n), text: words(Math.floor(rnd() * 200)), calls: [{ id, name: load ? "skill_load" : "act", args: {} }] },
        load ? { role: "tool", id, name: "skill_load", ok: true, line: "x", skills: [load], result: skillText(load) }
          : { role: "tool", id, name: "act", action: `${MODS[Math.floor(rnd() * MODS.length)]}.x.read`, ok: true, line: "x",
            result: clip(words(Math.floor(rnd() * 20000)), CAPS.result) }] };
    }
  }
}
check("over 1,000 turns at five tokenizer weights and windows, a request always fits after one compaction",
  !bad.length, bad.slice(0, 3).join(" | "));
check("...and summaries did happen, without the window being stated too", summaries > 10, JSON.stringify({ summaries, tooLong }));

// ---- small pieces ---------------------------------------------------------------------

check("a result, the reader's words and the page are each sent cut to their caps, whatever the record holds",
  toolText({ role: "tool", name: "x", result: "r".repeat(90000) }).length <= CAPS.result
  && userText({ role: "user", text: "t".repeat(90000), page: "p".repeat(90000) }).length <= CAPS.text + CAPS.page + 20);
check("a clipped text fits its cap and says how much is missing",
  clip("x".repeat(50000), 500).length <= 500 && /cut: \d+ more characters/.test(clip("x".repeat(50000), 500)));

// ---- 2: a number no tool returned is marked ---------------------------------------

const wrap = (s) => `[${s}]`;
check("only the unmeasured number is marked",
  markNumbers("1,234.5 DPS at 35% crit, 999.9 per hit, 240 per tick, 12.0k total, slot 3", [1234.5, 0.35, 12003], wrap)
  === "1,234.5 DPS at 35% crit, [999.9] per hit, [240] per tick, 12.0k total, slot 3");
check("the numbers a reply is checked against are the ones sent before it",
  same(numbersIn({ messages: [{ role: "tool", result: "a 12.5 b" }, { role: "tool", result: "7" }] }, 1), [12.5]));
const seenBy = (record, text) => markNumbers(text, numbersIn(record), wrap);
check("the page and the reader's own words count as sent; a result set aside does not",
  seenBy({ messages: [{ role: "user", text: "my riven: +120.5% crit", page: '{"level":9999}' },
    { role: "tool", result: "3456.7", masked: true }] }, "level 9999, 120.5% crit, 3456.7 damage")
  === "level 9999, 120.5% crit, [3456.7] damage");
check("the summary counts from where it stands",
  seenBy({ summary: { upto: 1, text: "DPS 1234.5 on build 1" }, messages: [{ role: "user", text: "a" }, { role: "user", text: "b" }] },
    "still 1234.5") === "still 1234.5");
check("a sign is not a difference: -34.7 was sent, 34.7% is measured",
  seenBy({ messages: [{ role: "tool", result: '"-34.7% Zoom"' }] }, "zoom -34.7%") === "zoom -34.7%");

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

// A PROFILE OVER ITS CAP, tidied by a model and checked by the page.
const fat = { ...memory.emptyMemory(), items: [
  { id: "p1", kind: "profile", key: "riven_policy", value: "不用紫卡", status: "active", source: {}, created_at: 1, updated_at: 5, history: [] },
  { id: "q1", kind: "note", value: "想试试", status: "proposed", source: {}, created_at: 1, updated_at: 1, history: [] },
  ...Array.from({ length: 20 }, (_, i) => ({ id: `n${i}`, kind: "note", value: `主玩钢铁之路，偏好单体高伤害配装，第${i}条记录`.repeat(3).slice(0, 110),
    status: "active", source: {}, created_at: 10 + i, updated_at: 20 + i, history: [] })) ] };
freeze(fat);
const ids = (a, b) => Array.from({ length: b - a }, (_, i) => `n${a + i}`).join(",");
const good = `- [p1] riven_policy: 不用紫卡\n- [${ids(0, 10)}] note: 主玩钢铁之路\n- [${ids(10, 20)}] note: 偏好单体高伤害配装`;
check("a profile past its cap is offered for tidying; one within it is not",
  memory.needsMerge(fat, 100) === true && memory.needsMerge(said.mem, 1000) === false);
check("the model is given every active memory, and no proposed one",
  (memory.mergeInput(fat).match(/^- \[/gm) || []).length === 21 && !memory.mergeInput(fat).includes("q1"));
const okMerge = memory.parseMerge(fat, "Sure! Here it is:\n" + good, 100);
check("a tidy that names its sources, keeps every slot and is smaller is taken", okMerge.ok === true && okMerge.lines.length === 3);
const tidied = memory.merge(fat, okMerge.lines, 100, (i) => `t${i}`);
check("...notes are replaced by the merged ones, which say where they came from; slots and proposals stay",
  tidied.items.filter((x) => x.kind === "note" && x.status === "active").length === 2
  && same(tidied.items.find((x) => x.id === "t0").source.from, ids(0, 10).split(","))
  && tidied.items.some((x) => x.id === "p1") && tidied.items.some((x) => x.id === "q1"));
check("...and it fits the memory zone", memory.block(tidied, 100).length <= CAPS.M && memory.needsMerge(tidied, 100) === false);
check("...and the version before comes back in one step", same(memory.restore(tidied).items, fat.items));
for (const [why, text] of [
  ["a source that does not exist", good.replace("[p1]", "[p9]")],
  ["a slot dropped", good.split("\n").slice(1).join("\n")],
  ["a slot invented", good + "\n- [n0] budget: 很多钱"],
  ["a value past its length", good.replace("主玩钢铁之路", "长".repeat(200))],
  ["nothing smaller", memory.mergeInput(fat)],
  ["no lines at all", "I could not do that."],
]) check(`a tidy is refused for ${why}, and the profile stays as it was`, memory.parseMerge(fat, text, 100).ok === false, why);

// ---- 8: her tools, and the door's reached through skills ------------------------------

check("her tools are seven, in a fixed order, whatever the door holds",
  same(TOOLS.map((t) => t.name), ["shell_page_observe", "shell_history_search", "memory_set", "memory_forget", "skill_load", "act", "calc"]));
check("the catalogue rides in skill_load and names every skill and action",
  /builder — the build/.test(TOOLS[4].description) && /mod\.set/.test(TOOLS[4].description)
  && same(TOOLS[4].input_schema.properties.skills.items.enum, SKILLS.map((x) => x.id)));
const DOOR_TOOLS = [
  { name: "builder.mod.set", description: "Seat a mod.", input_schema: { type: "object",
    properties: { slot: { type: ["string", "integer"], description: "0-7" }, mod: { type: ["string", "null"], description: "mod id" } }, required: ["slot", "mod"] } },
  { name: "builder.weapon.set", description: "Open a weapon.", input_schema: { type: "object",
    properties: { weapon: { type: "string", enum: Array.from({ length: 300 }, (_, i) => `w${i}`) }, mode: { type: "string", enum: ["a", "b"] } }, required: ["weapon"] } },
];
const doc = skillDoc({ id: "builder", what: "the build", actions: ["builder.mod.set", "builder.weapon.set"] }, DOOR_TOOLS);
check("a skill's document is each action's signature, what it does, and its arguments — a required one starred",
  /builder\.mod\.set — Seat a mod\./.test(doc) && /slot\*: string\|integer — 0-7/.test(doc) && doc.startsWith("<skill builder>"));
check("...a long list of values is counted, not copied; a short one is listed",
  /weapon\*: one of 300 values/.test(doc) && /mode: one of "a"\|"b"/.test(doc) && !/w299/.test(doc));
check("a document splits back into its skills", same(docsOf(doc + "\n\n" + doc.replace(/builder/g, "rivens")).map((d) => d.id), ["builder", "rivens"]));

// K: the skills in view, and what a summary carries of them.
const big = (id) => `<skill ${id}>\n${"x ".repeat(3000)}\n</skill>`;
const kRec = { ...convo(0), messages: [
  { role: "user", text: "q1", page: "{}", preload: big("builder"), skills: ["builder"] },
  { role: "assistant", text: "", calls: [{ id: "k1", name: "skill_load", args: { skills: ["simulator"] } }] },
  { role: "tool", id: "k1", name: "skill_load", ok: true, line: "x", skills: ["simulator"], result: big("simulator") },
  { role: "assistant", text: "", calls: [{ id: "k2", name: "act", args: { id: "builder.stats.read" } }] },
  { role: "tool", id: "k2", name: "act", action: "builder.stats.read", ok: true, line: "x", result: "{}" },
  { role: "user", text: "q2", page: "{}" },
  { role: "assistant", text: "", calls: [{ id: "k3", name: "skill_load", args: { skills: ["optimizer"] } }] },
  { role: "tool", id: "k3", name: "skill_load", ok: true, line: "x", skills: ["optimizer"], result: big("optimizer") },
] };
freeze(kRec);
check("the skills in view are every one attached or loaded since the summary",
  same([...budget.loadedSkills(kRec)], ["builder", "simulator", "optimizer"]));
const carried = summary.applySummary(kRec, 5, "S");
check("a summary carries the skills in view before its cut, as they were loaded",
  same(carried.summary.skills.map((d) => d.id), ["builder", "simulator"]) && carried.summary.skills[1].text === big("simulator")
  && view(carried, FIXED).turns[0].text.includes("<skill simulator>"));

// calc: arithmetic on what she was sent, and nothing else.
const sentNums = [51.98, 29.32, 1234.5, 0.35];
check("calc works out a difference and a percentage of measured numbers",
  calc("51.98 - 29.32", sentNums).value === 22.66 && calc("(51.98 / 29.32 - 1) * 100", sentNums).value === 77.2851);
check("...with precedence, brackets, unary minus, × and ÷", calc("-1234.5 × 2 + 29.32 ÷ 2", sentNums).value === -2454.34);
check("...and a percentage as a fraction", calc("1234.5 * 35%", sentNums).value === 432.075);
const refused = calc("530 - 29.32", sentNums);
check("a number she was never sent is refused, and named", refused.ok === false && refused.reason === "unmeasured" && same(refused.numbers, ["530"]));
check("a broken expression says why", calc("(51.98 - ", sentNums).reason === "bad_expression" && calc("51.98 / 0", sentNums).reason === "not_a_number"
  && calc("alert(1)", sentNums).reason === "bad_expression");

console.log(failed ? `\n${failed} failed` : "\nnona's core holds");
process.exit(failed ? 1 : 0);
