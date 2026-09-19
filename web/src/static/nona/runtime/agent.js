// THE AGENT LOOP, and the one writer of the conversation it runs: it appends,
// persists after every append, and says what happened as events. The panel
// draws those events; it never writes the record. docs/NONA.md §"One turn".

import * as record from "../core/record.js";
import { view } from "../core/view.js";
import * as budget from "../core/budget.js";
import * as summary from "../core/summary.js";
import * as memoryOps from "../core/memory.js";
import { rules } from "../core/prompt.js";
import { allTools, toolId, OBSERVE, HISTORY, MEMORY_SET, MEMORY_FORGET } from "../core/tools.js";
import * as transport from "./transport.js";
import * as store from "./store.js";
import { branch, madeBy } from "./policy.js";

/// How many model turns one question may take. A question that has not settled
/// by then is handed back rather than billed on.
const MAX_STEPS = 24;
/// A tool result longer than this is cut, and says so: the model pays for every
/// byte, and a list it needs more of can be narrowed with the tool's own limit.
const RESULT_CAP = 16000;

/// ONE LINE PER CALL, derived from the call itself — the trail cannot describe
/// a move that was not made.
export const callLine = (c) => {
  const args = Object.entries(c.args || {}).map(([k, v]) => `${k}=${typeof v === "object" ? JSON.stringify(v) : v}`).join(" ");
  return `${toolId(c.name)}${args ? " " + args : ""}`;
};

const cap = (v) => {
  const s = JSON.stringify(v);
  return s.length <= RESULT_CAP ? s : s.slice(0, RESULT_CAP) + ` …[cut at ${RESULT_CAP} of ${s.length} characters]`;
};
const uid = (p) => `${p}${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;

/// EVENTS: `open` (a conversation replaced the one shown), `list`, `append`
/// ({message, index} — the record grew), `stream` ({text} — the reply so far),
/// `calling` ({line}), `say` ({kind, text} — shown, not kept; `text` is a ui
/// string unless `raw`), `busy`, `settings` (none are set), `done`.
export function createAgent(door) {
  const listeners = new Set();
  const emit = (ev) => listeners.forEach((f) => f(ev));
  const s = { conv: null, list: [], busy: false, abort: null, owned: new Set() };

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
    emit({ type: "open" });
  }

  /// What every request of this turn shares: her rules, the reader's memory and
  /// the tool table — the stable prefix.
  function parts(cfg) {
    return {
      rules: rules({ lang: door.observe().lang, concise: !!cfg.concise }),
      memory: memoryOps.block(store.memory.get(), Date.now(), !!s.conv.incognito),
      tools: allTools(door.tools()),
    };
  }

  /// Set aside what the budget says to, written into the record so the next
  /// request is sent the same prefix. Returns the estimate of what is sent.
  function fit(cfg, p, force) {
    const d = budget.decide(s.conv, { window: cfg.context, fixed: p.rules + p.memory + JSON.stringify(p.tools),
      ratio: store.ratio(cfg.model), force });
    if (d.tools.length || d.pages.length) {
      s.conv = budget.applyMarks(s.conv, d);
      say("note", "earlier tool results were set aside to save space");
    }
    return d.est;
  }

  async function summarise(cfg, p, signal) {
    const cut = summary.wantsSummary(s.conv, { window: cfg.context, fixed: p.rules + JSON.stringify(p.tools), ratio: store.ratio(cfg.model) });
    if (cut == null) return false;
    const text = await transport.complete(cfg, summary.SUMMARY_RULES, summary.summaryInput(s.conv, cut, callLine), signal);
    s.conv = summary.applySummary(s.conv, cut, text);
    say("note", "the early part of this chat was summarised to save space");
    await save();
    return true;
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
        hits.push({ turn: i, role: m.role, ...(m.name ? { tool: toolId(m.name) } : {}),
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

  async function runTool(call) {
    const args = call.args || {};
    if (call.name === OBSERVE.name) return door.observe();
    if (call.name === HISTORY.name) return historySearch(args.query);
    if (call.name === MEMORY_SET.name) return memorySet(args);
    if (call.name === MEMORY_FORGET.name) return store.memory.forget(args.id);
    const id = toolId(call.name);
    const b = await branch(door, id, s.owned);
    if (b) {
      s.owned.add(b.owned);
      if (b.pair) s.conv.pairs = [...(s.conv.pairs || []), b.pair];
    }
    const r = await door.do(id, args);
    const made = madeBy(id, args, r);
    if (made) s.owned.add(made);
    return b ? { ...r, branched_to_copy: b.copy } : r;
  }

  /// THE CHANGE CARD'S CONTENT: her copy as it is now, read through the door,
  /// kept in the record so the card reads the same when reopened.
  async function card(asked) {
    const c = s.conv;
    const pair = (c.pairs || [])[(c.pairs || []).length - 1];
    if (!pair || !c.messages.slice(asked).some((m) => m.role === "tool" && m.ok && /^builder\./.test(toolId(m.name)))) return;
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
    append({ role: "user", text, page: JSON.stringify(page), at: Date.now() });
    const asked = s.conv.messages.length;
    s.busy = true; s.abort = new AbortController(); emit({ type: "busy" });
    const signal = s.abort.signal;
    let retried = false;
    const seen = [];
    try {
      for (let step = 0; step < MAX_STEPS; step++) {
        let p = parts(cfg);
        let est = fit(cfg, p, false);
        if (await summarise(cfg, p, signal)) { p = parts(cfg); est = fit(cfg, p, false); }
        let out;
        try {
          out = await transport.send(cfg, view(s.conv, p), { signal, onText: (t) => emit({ type: "stream", text: t }) });
        } catch (e) {
          emit({ type: "stream", text: null });
          // TOO LONG FOR THE ADDRESS: set aside everything but the newest, once.
          if (!retried && transport.tooLong(e)) { retried = true; fit(cfg, p, true); step--; continue; }
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
          let r;
          try { r = await runTool(call); } catch (e) { r = { ok: false, reason: "action_failed", because: String((e && e.message) || e) }; }
          append({ role: "tool", id: call.id, name: call.name, ok: !(r && r.ok === false), line: callLine(call), result: cap(r) });
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
