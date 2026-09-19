// THE PANEL: the button, the log, the conversation picker, the suggestions and
// the running cost. It draws the agent's events while a turn runs and the record
// when a conversation is opened — through the same `draw`, so a conversation
// reopened tomorrow reads exactly as it did.

import { numbersIn } from "../core/measure.js";
import { toolId } from "../core/tools.js";
import * as store from "../runtime/store.js";
import { tr, esc, dd, $, k, usd, markup, usageLine, line } from "./kit.js";
import { drawCard } from "./cards.js";
import { drawChip } from "./chips.js";
import { fillSettings } from "./settings.js";

export function mountPanel(door, agent) {
  if ($("nona-fab")) return;
  const name = tr("Nona");
  const fab = document.createElement("button");
  fab.id = "nona-fab";
  fab.className = "nona-fab";
  fab.textContent = name;
  fab.title = name;
  const panel = document.createElement("aside");
  panel.id = "nona";
  panel.className = "nona";
  panel.hidden = true;
  panel.innerHTML = `
    <div class="nona-head">
      <span class="nona-name">${esc(name)}</span>
      <button class="ghost-btn small" id="nona-new">${esc(tr("New chat"))}</button>
      <button class="ghost-btn small" id="nona-gear">${esc(tr("Settings"))}</button>
      <button class="ghost-btn small" id="nona-close" aria-label="${esc(tr("Close"))}">✕</button>
    </div>
    <div class="nona-convs" id="nona-convs"></div>
    <div id="nona-chat" class="nona-chat">
      <div id="nona-log" class="nona-log"></div>
      <div id="nona-suggest" class="nona-suggest" hidden></div>
      <div class="nona-foot">
        <textarea id="nona-input" rows="2" placeholder="${esc(tr("Ask Nona about this build…"))}"></textarea>
        <button class="run-btn" id="nona-send">${esc(tr("Send"))}</button>
      </div>
      <div class="nona-foot-note" id="nona-foot-note"></div>
    </div>
    <div id="nona-settings" class="nona-settings" hidden></div>`;
  document.body.append(fab, panel);

  let live = null; // the reply bubble while it streams
  let calling = null; // the trail line of the call being run

  /// ONE RECORD MESSAGE, DRAWN. `index` is where it sits in the record: a reply
  /// is checked against the numbers the tools returned before it.
  function draw(m, index) {
    if (m.role === "user") line("user", m.text);
    else if (m.role === "assistant") {
      if (m.text) line("assistant", "").innerHTML = markup(m.text, numbersIn(agent.conv, index));
      if (m.usage) line("usage", usageLine(m.usage));
    } else if (m.role === "tool") {
      const el = calling || line("tool", m.line || toolId(m.name));
      calling = null;
      el.classList.add(m.ok === false ? "no" : "ok");
    } else if (m.role === "note") line("note", m.text);
    else if (m.role === "card") drawCard(door, m.pair);
    else if (m.role === "memory") drawChip(m.id);
  }

  function render() {
    const log = $("nona-log");
    log.innerHTML = "";
    live = null; calling = null;
    agent.conv.messages.forEach(draw);
    paintHead();
    paintFoot();
    paintSuggest();
  }

  function paintBusy() {
    const send = $("nona-send");
    send.textContent = agent.busy ? tr("Stop") : tr("Send");
    send.classList.toggle("busy", agent.busy);
    $("nona-input").disabled = agent.busy;
  }

  /// The conversation picker and its two moves — the page's own searchable
  /// dropdown, pinned ones first.
  function paintHead() {
    const host = $("nona-convs");
    const c = agent.conv;
    if (!c) return;
    const saved = agent.list.some((x) => x.id === c.id);
    const day = (t) => new Date(t).toLocaleDateString();
    host.innerHTML = dd("nona-conv", {
      value: saved ? c.id : "", search: true, placeholder: tr("New chat"),
      items: agent.list.map((x) => ({ value: x.id, label: x.title || tr("untitled"),
        group: x.pinned ? tr("Pinned") : tr("Recent"), hint: day(x.updated_at) })),
      onPick: (v) => agent.open(v),
    }) + (saved
      ? `<button class="ghost-btn small" id="nona-pin" title="${esc(tr(c.pinned ? "Unpin" : "Pin"))}">${c.pinned ? "★" : "☆"}</button>`
        + `<button class="ghost-btn small" id="nona-del" title="${esc(tr("Delete"))}">🗑</button>`
      : "");
    const pin = $("nona-pin"), del = $("nona-del");
    if (pin) pin.onclick = () => agent.pin();
    // DELETING TAKES TWO CLICKS on the same button — no native dialog here.
    if (del) {
      del.onclick = () => {
        if (!del.dataset.armed) {
          del.dataset.armed = "1"; del.textContent = tr("Delete?");
          setTimeout(() => { if (del.isConnected) { delete del.dataset.armed; del.textContent = "🗑"; } }, 3000);
          return;
        }
        agent.remove(c.id);
      };
    }
  }

  /// THREE QUESTIONS THE PAGE SUGGESTS, read off its own state rather than asked
  /// of a model: specific to what is open, and free. Shown on an empty
  /// conversation only — once one is going, the reader knows what to ask.
  async function suggestions() {
    const o = door.observe();
    if (!o.route || !o.route.weapon) return [];
    const board = await door.do("builder.board.read", { limit: 1 });
    const ranked = !!(board && board.ok && board.rows.length);
    const out = [];
    if (ranked) out.push(tr("What is this weapon's best build without a riven?"));
    if (o.build && o.build.slots.some((x) => x.mod)) out.push(tr("How could my build do better?"));
    else out.push(tr("Put together a build for this weapon"));
    if (o.result && o.result.fresh && ranked) out.push(tr("Why does my build score below the board's leader?"));
    return out.slice(0, 3);
  }

  function paintSuggest() {
    const host = $("nona-suggest");
    const c = agent.conv;
    const empty = !!c && !c.messages.length && !agent.busy;
    host.hidden = !empty;
    if (!empty) { host.innerHTML = ""; return; }
    // PICK UP WHERE WE LEFT OFF: the latest conversation, one tap away. It costs
    // nothing — its title is what was asked — and it is the plainest sign that
    // she remembers.
    const last = !c.incognito ? agent.list.find((x) => x.id !== c.id) : null;
    const draw1 = (list) => {
      host.innerHTML = (last ? `<span class="pchip resume" data-resume="${esc(last.id)}">↩ ${esc(tr("Continue"))}: ${esc(last.title || tr("untitled"))}</span>` : "")
        + list.map((q) => `<span class="pchip" data-q="${esc(q)}">${esc(q)}</span>`).join("")
        + `<span class="pchip incog${c.incognito ? " sel" : ""}" data-incog="1" title="${esc(tr("this chat neither reads nor writes memory, and is not kept"))}">${esc(tr("Incognito"))}</span>`;
      host.querySelectorAll("[data-q]").forEach((el) => { el.onclick = () => agent.ask(el.dataset.q); });
      host.querySelectorAll("[data-resume]").forEach((el) => { el.onclick = () => agent.open(el.dataset.resume); });
      host.querySelectorAll("[data-incog]").forEach((el) => { el.onclick = () => { agent.setIncognito(!c.incognito); paintSuggest(); }; });
    };
    draw1([]);
    suggestions().then((list) => { if (agent.conv === c && !c.messages.length) draw1(list); });
  }

  /// The running total, and the label the law and the providers both ask for.
  function paintFoot() {
    const f = $("nona-foot-note");
    const u = agent.conv && agent.conv.usage;
    if (!u) return;
    f.textContent = [tr("AI-generated content; it may be wrong."),
      u.input ? `${tr("this chat")} ${k(u.input + u.output)} tokens${u.cost ? ` · ≈${usd(u.cost)}` : ""}` : ""]
      .filter(Boolean).join(" · ");
  }

  function show(which) {
    $("nona-chat").hidden = which !== "chat";
    $("nona-settings").hidden = which !== "settings";
    if (which === "settings") fillSettings(() => { show("chat"); $("nona-input").focus(); });
  }

  agent.on((ev) => {
    if (ev.type === "open") render();
    else if (ev.type === "list") paintHead();
    else if (ev.type === "append") {
      draw(ev.message, ev.index);
      if (ev.message.role === "user") paintSuggest();
    } else if (ev.type === "stream") {
      if (ev.text == null) { if (live) live.remove(); live = null; } else {
        if (!live) live = line("assistant", "");
        live.textContent = ev.text;
        $("nona-log").scrollTop = $("nona-log").scrollHeight;
      }
    } else if (ev.type === "calling") calling = line("tool", ev.line);
    else if (ev.type === "say") line(ev.kind, ev.raw ? ev.text : tr(ev.text));
    else if (ev.type === "busy") paintBusy();
    else if (ev.type === "settings") show("settings");
    else if (ev.type === "done") { paintFoot(); paintSuggest(); }
  });

  fab.addEventListener("click", () => {
    panel.hidden = !panel.hidden;
    fab.hidden = !panel.hidden;
    document.body.classList.toggle("nona-open", !panel.hidden);
    if (panel.hidden) return;
    // THE CONVERSATION EXISTS AT ONCE; the list of older ones follows. Opening
    // one only after the list has loaded let a question typed meanwhile land in
    // a conversation that was then replaced.
    if (!agent.conv) { agent.open(null); agent.refreshList(); }
    const s = store.settings();
    show(s.key ? "chat" : "settings");
    if (s.key) $("nona-input").focus();
  });
  $("nona-close").addEventListener("click", () => { panel.hidden = true; fab.hidden = false; document.body.classList.remove("nona-open"); });
  $("nona-gear").addEventListener("click", () => show($("nona-settings").hidden ? "settings" : "chat"));
  $("nona-new").addEventListener("click", () => { agent.open(null); show("chat"); });
  const submit = () => {
    if (agent.busy) { agent.stop(); return; }
    const text = $("nona-input").value.trim();
    if (!text) return;
    $("nona-input").value = "";
    agent.ask(text);
  };
  $("nona-send").addEventListener("click", submit);
  $("nona-input").addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); submit(); }
  });
}
