// THE AGENT LOOP, and the one writer of the conversation it runs: it appends,
// persists after every append, and says what happened as events. The panel
// draws those events; it never writes the record. docs/NONA.md §"One turn".

import * as record from "../core/record.js";
import { view, viewText } from "../core/view.js";
import { CAPS, clip, estimate } from "../core/size.js";
import * as budget from "../core/budget.js";
import * as summary from "../core/summary.js";
import * as memoryOps from "../core/memory.js";
import { rules } from "../core/prompt.js";
import { fixedTools, toolId, OBSERVE, HISTORY, MEMORY_SET, MEMORY_FORGET, ACT } from "../core/tools.js";
import { catalogue, skillDoc } from "../core/skills.js";
import { calc } from "../core/calc.js";
import { numbersIn } from "../core/measure.js";
import * as transport from "./transport.js";
import * as store from "./store.js";
import { branch, madeBy } from "./policy.js";

/// How many model turns one question may take. A question that has not settled
/// by then is handed back rather than billed on.
const MAX_STEPS = 24;

/// ONE LINE PER CALL, derived from the call itself — the trail cannot describe
/// a move that was not made.
export const callLine = (c) => {
  const act = c.name === "act" && c.args && typeof c.args.id === "string";
  const shown = act ? c.args.args || {} : c.args || {};
  const args = Object.entries(shown).map(([k, v]) => `${k}=${typeof v === "object" ? JSON.stringify(v) : v}`).join(" ");
  return `${act ? c.args.id : toolId(c.name)}${args ? " " + args : ""}`;
};

/// A RESULT IS CUT TO ITS CAP WHEN IT IS RECORDED, and says so: the model pays
/// for every byte, and a list it needs more of can be narrowed with the tool's
/// own limit. It also bounds what one round can add to the record.
const cap = (v) => clip(JSON.stringify(v), CAPS.result);
const uid = (p) => `${p}${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;

/// EVENTS: `open` (a conversation replaced the one shown), `list`, `append`
/// ({message, index} — the record grew), `stream` ({text} — the reply so far),
/// `calling` ({line}), `say` ({kind, text} — shown, not kept; `text` is a ui
/// string unless `raw`), `busy`, `settings` (none are set), `done`.
export function createAgent(door) {
  const listeners = new Set();
  const emit = (ev) => listeners.forEach((f) => f(ev));
  const s = { conv: null, list: [], busy: false, abort: null, owned: new Set(), shrink: 1 };

  function append(m) {
    s.conv.messages.push(m);
    emit({ type: "append", message: m, index: s.conv.messages.length - 1 });
  }
  const say = (kind, text, raw = false) => emit({ type: "say", kind, text, raw });

  async function refreshList() {
    s.list = await store.conversations();
    emit({ type: "list" });
  }

  async function save() {
    const c = s.conv;
    if (!c || !c.messages.length || c.incognito) return;
    c.updated_at = Date.now();
    c.made = [...s.owned];
    await store.putConversation(c);
    await refreshList();
  }

  function open(id) {
    if (s.abort) s.abort.abort();
    const c = id && s.list.find((x) => x.id === id);
    s.conv = c ? JSON.parse(JSON.stringify(c))
      : record.newConversation({ id: uid("c"), now: Date.now(), weapon: (door.observe().route || {}).weapon });
    s.owned = new Set(s.conv.made || []);
    s.shrink = 1;
    emit({ type: "open" });
  }

  /// A PROFILE OVER ITS CAP, TIDIED by the model and taken only if the page can
  /// check the result (`memoryOps.parseMerge`); otherwise it stays as it was.
  /// It runs where M is taken anyway, so it never moves a prefix on its own.
  async function tidyMemory(cfg, signal) {
    if (s.conv.incognito) return;
    const mem = store.memory.get();
    if (!memoryOps.needsMerge(mem, Date.now())) return;
    let text;
    try { text = await transport.complete(cfg, memoryOps.MERGE_RULES, memoryOps.mergeInput(mem), signal); } catch (_) { return; }
    const checked = memoryOps.parseMerge(mem, text, Date.now());
    if (!checked.ok) return;
    const before = mem.items.filter((x) => x.status === "active").length;
    const next = store.memory.change((m) => memoryOps.merge(m, checked.lines, Date.now(), (i) => uid(`m${i}`)));
    append({ role: "memory", merged: [before, next.items.filter((x) => x.status === "active").length] });
  }

  /// M, FROZEN FOR A STRETCH: taken when the conversation starts and again at
  /// each summary, which breaks the cache anyway. A memory written in between
  /// is in the conversation as her tool result, so the prefix never moves for it.
  function freezeMemory() {
    s.conv.memory = { text: memoryOps.block(store.memory.get(), Date.now(), !!s.conv.incognito), at: Date.now() };
  }

  /// What every request shares: her rules, the reader's memory and the tool
  /// table — S, M and T, the stable prefix.
  function parts(cfg) {
    return {
      rules: rules({ lang: door.observe().lang, concise: !!cfg.concise }),
      memory: (s.conv.memory && s.conv.memory.text) || "",
      tools: fixedTools(door.skills, catalogue(door.skills)),
    };
  }

  /// EVERY ZONE UNDER ITS CAP before a request: set aside what `plan` says to,
  /// summarise when it says to, and ask again until it asks for nothing more.
  /// `shrink` is how much smaller than the estimate the address has shown the
  /// window to be. Returns the parts to send, or why not to send.
  async function maintain(cfg, signal) {
    let p = parts(cfg);
    for (let round = 0; round < 3; round++) {
      const pl = budget.plan(s.conv, p, { context: cfg.context, budget: cfg.budget, ratio: store.ratio(cfg.model) * s.shrink });
      if (pl.refuse) return { refuse: true };
      const mk = pl.marks;
      if (mk.tools.length || mk.pages.length || mk.skills.length || mk.summary.length) {
        s.conv = budget.applyMarks(s.conv, mk);
        say("note", "earlier tool results were set aside to save space");
      }
      if (pl.summarize == null) return { p, stop: pl.stop };
      const text = await transport.complete(cfg, summary.SUMMARY_RULES, summary.summaryInput(s.conv, pl.summarize, callLine), signal);
      s.conv = summary.applySummary(s.conv, pl.summarize, text);
      await tidyMemory(cfg, signal);
      freezeMemory();
      p = parts(cfg);
      say("note", "the early part of this chat was summarised to save space");
      await save();
    }
    return { p, stop: false };
  }

  /// Up to eight places the record mentions `q`, each with a little around it.
  function historySearch(q) {
    const needle = String(q || "").trim().toLowerCase();
    if (!needle) return { ok: false, reason: "missing_argument", argument: "query" };
    const hits = [];
    s.conv.messages.forEach((m, i) => {
      const text = m.role === "tool" ? m.result || "" : m.text || "";
      const at = text.toLowerCase().indexOf(needle);
      if (at >= 0 && hits.length < 8) {
        hits.push({ turn: i, role: m.role, ...(m.name ? { tool: m.action || toolId(m.name) } : {}),
          text: text.slice(Math.max(0, at - 150), at + 150 + needle.length) });
      }
    });
    return { found: hits.length, hits };
  }

  function memorySet(args) {
    if (s.conv.incognito) return { ok: false, reason: "memory_off" };
    const users = s.conv.messages.filter((m) => m.role === "user");
    let result;
    store.memory.change((m) => {
      const r = memoryOps.set(m, args, { lastUserText: users.length ? users[users.length - 1].text : "",
        conversation: s.conv.id, now: Date.now(), id: uid("m") });
      result = r.result;
      return r.mem;
    });
    return result;
  }

  /// Skills whose documents are in view now — never loaded twice.
  const loaded = () => new Set(budget.skillsInView(s.conv).flatMap((e) => e.ids));

  /// The documents of `ids`, as generated from the door now.
  const docsFor = (ids) => {
    const all = door.skills, tools = door.tools();
    return ids.map((id) => skillDoc(all.find((x) => x.id === id), tools)).join("\n\n");
  };

  /// SKILL_LOAD: the documents, skipping what is in view, refusing what would
  /// not fit the skill zone at once. What it returns is the documents
  /// themselves, recorded as they are so they are resent byte for byte.
  function skillLoad(args) {
    const want = [].concat(args.skills || []).filter(Boolean);
    const known = door.skills.map((x) => x.id);
    const bad = want.filter((x) => !known.includes(x));
    if (!want.length || bad.length) return { r: { ok: false, reason: "bad_argument", argument: "skills", alternatives: known } };
    const have = loaded();
    const ids = want.filter((x) => !have.has(x));
    if (!ids.length) return { r: { ok: true, text: `already loaded: ${want.join(", ")}` } };
    const text = docsFor(ids);
    if (estimate(text) > CAPS.K) return { r: { ok: false, reason: "too_large", because: "load fewer skills at once" } };
    return { r: { ok: true }, result: text, skills: ids };
  }

  /// ACT: one door action, through the same path as ever — branch before a
  /// write, the door's own checks, and what the action made is hers.
  async function act(id, args) {
    const b = await branch(door, id, s.owned);
    if (b) {
      s.owned.add(b.owned);
      if (b.pair) s.conv.pairs = [...(s.conv.pairs || []), b.pair];
    }
    const r = await door.do(id, args || {});
    const made = madeBy(id, args || {}, r);
    if (made) s.owned.add(made);
    return b ? { ...r, branched_to_copy: b.copy } : r;
  }

  /// One call, run: what it returned, and what the tool message records beside it.
  async function runTool(call) {
    const args = call.args || {};
    if (call.name === OBSERVE.name) return { r: door.observe() };
    if (call.name === HISTORY.name) return { r: historySearch(args.query) };
    if (call.name === MEMORY_SET.name) return { r: memorySet(args) };
    if (call.name === MEMORY_FORGET.name) return { r: store.memory.forget(args.id) };
    if (call.name === "skill_load") return skillLoad(args);
    if (call.name === "calc") return { r: calc(args.expression, numbersIn(s.conv)) };
    if (call.name === ACT.name) {
      if (typeof args.id !== "string" || !args.id) return { r: { ok: false, reason: "missing_argument", argument: "id" } };
      return { r: await act(args.id, args.args), action: args.id };
    }
    return { r: { ok: false, reason: "unknown_tool", because: "call the page's actions with act" } };
  }

  /// THE CHANGE CARD'S CONTENT: her copy as it is now, read through the door,
  /// kept in the record so the card reads the same when reopened.
  async function card(asked) {
    const c = s.conv;
    const pair = (c.pairs || [])[(c.pairs || []).length - 1];
    if (!pair || !c.messages.slice(asked).some((m) => m.role === "tool" && m.ok && /^builder\./.test(m.action || ""))) return;
    const r = await door.do("shell.preset.read", { bar: "build", preset: pair.copy });
    if (r && r.ok) append({ role: "card", pair: { ...pair, state: { mods: r.mods, arcanes: r.arcanes, mode: r.mode } } });
  }

  async function ask(text) {
    const cfg = store.settings();
    if (!cfg.key || !cfg.base || !cfg.model) { say("error", "Set a provider, key and model first."); emit({ type: "settings" }); return; }
    const c = s.conv;
    if (!c.messages.length) {
      const w = door.observe().weapon;
      c.title = record.titleOf(text, w && w.id === c.weapon ? w.name : null);
    }
    const { can, ...page } = door.observe();
    // THE SKILL OF THE MODULE THE READER IS ON rides with their message, unless
    // it is in view already: most questions then need no load first.
    const here = (page.route || {}).module;
    const preload = here && door.skills.some((x) => x.id === here) && !loaded().has(here) ? { preload: docsFor([here]), skills: [here] } : {};
    append({ role: "user", text, page: JSON.stringify(page), at: Date.now(), ...preload });
    const asked = s.conv.messages.length;
    s.busy = true; s.abort = new AbortController(); emit({ type: "busy" });
    const signal = s.abort.signal;
    let retried = false, nudged = false;
    const seen = [];
    try {
      // M IS TAKEN when the conversation starts — tidied first if it has grown
      // past its cap — and then left alone until a summary.
      if (!s.conv.memory) { await tidyMemory(cfg, signal); freezeMemory(); }
      for (let step = 0; step < MAX_STEPS; step++) {
        const m = await maintain(cfg, signal);
        if (m.refuse) { say("error", "This model's context window is too small for Nona."); return; }
        // THIS TURN ALONE HAS OUTGROWN ITS ZONE: it ends here, and the next
        // message starts a turn that can summarise this one.
        if (m.stop) { say("note", "this question has taken many steps — ask me to go on and I will continue from here"); return; }
        const v = view(s.conv, m.p);
        const est = estimate(viewText(v)) * store.ratio(cfg.model);
        let out;
        try {
          out = await transport.send(cfg, v, { signal, onText: (t) => emit({ type: "stream", text: t }) });
        } catch (e) {
          emit({ type: "stream", text: null });
          // TOO LONG FOR THE ADDRESS: its window is smaller than it said or than
          // the estimate thinks; plan against a third less, once.
          if (!retried && transport.tooLong(e)) { retried = true; s.shrink *= 1.5; step--; continue; }
          throw e;
        }
        emit({ type: "stream", text: null });
        if (out.usage) {
          store.saveRatio(cfg.model, budget.calibrate(store.ratio(cfg.model), est, out.usage.input));
          const cost = budget.cost(cfg.price, out.usage);
          const u = s.conv.usage;
          s.conv.usage = { input: u.input + out.usage.input, output: u.output + out.usage.output,
            cached: u.cached + out.usage.cached, cost: u.cost + (cost || 0) };
          out.usage.cost = cost;
        }
        append({ role: "assistant", text: out.text, calls: out.calls, usage: out.usage || null });
        await save();
        // A TURN ENDED IN SILENCE is sent back once, from the page: some models
        // stop with neither words nor calls, and the reader would see nothing.
        if (!String(out.text || "").trim() && !out.calls.length) {
          if (nudged) { say("note", "she gave no answer"); return; }
          nudged = true;
          append({ role: "check", text: "Your last reply was empty. Answer the reader now, or call a tool if something is still to be done." });
          await save();
          continue;
        }
        if (!out.calls.length) return;
        for (const call of out.calls) {
          if (signal.aborted) return;
          // THE SAME STEP THREE TIMES is a loop, not progress: stop and say so
          // rather than spend the reader's tokens going round.
          const sig = call.name + JSON.stringify(call.args || {});
          seen.push(sig);
          if (seen.length >= 3 && seen.slice(-3).every((x) => x === sig)) {
            say("note", "she kept repeating the same step, so she stopped");
            append({ role: "tool", id: call.id, name: call.name, ok: false, line: callLine(call),
              result: JSON.stringify({ ok: false, reason: "repeated", because: "the same call three times in a row" }) });
            await save();
            return;
          }
          emit({ type: "calling", line: callLine(call) });
          let ran;
          try { ran = await runTool(call); } catch (e) { ran = { r: { ok: false, reason: "action_failed", because: String((e && e.message) || e) } }; }
          const r = ran.r;
          append({ role: "tool", id: call.id, name: call.name, ok: !(r && r.ok === false), line: callLine(call),
            ...(ran.action ? { action: ran.action } : {}),
            ...(ran.skills ? { skills: ran.skills, result: ran.result } : { result: cap(r) }) });
          if (call.name === MEMORY_SET.name && r && r.ok) append({ role: "memory", id: r.id });
          if (r && r.branched_to_copy) append({ role: "note", text: `${door.ui.tr("working on a copy")}: ${r.branched_to_copy}` });
          await save();
        }
      }
      say("note", "step limit reached");
    } catch (e) {
      if (signal.aborted) say("note", "stopped");
      else if (transport.unreachable(e)) say("error", "The provider could not be reached from the browser — the network failed, or it does not accept requests from a web page (CORS).");
      else say("error", String((e && e.message) || e), true);
    } finally {
      // A conversation opened meanwhile aborted this one, and is not its to write.
      if (s.conv.id === c.id) { await card(asked); await save(); }
      s.busy = false; s.abort = null; emit({ type: "busy" });
      emit({ type: "done" });
    }
  }

  return {
    on: (f) => listeners.add(f),
    get conv() { return s.conv; },
    get list() { return s.list; },
    get busy() { return s.busy; },
    open,
    ask,
    refreshList,
    stop: () => { if (s.abort) s.abort.abort(); },
    setIncognito: (on) => { s.conv.incognito = !!on; },
    pin: async () => { s.conv.pinned = !s.conv.pinned; await save(); },
    remove: async (id) => { await store.deleteConversation(id); await refreshList(); open(null); },
  };
}
