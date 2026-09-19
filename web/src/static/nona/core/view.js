// WHAT A MODEL IS SENT, as a pure function of the record, her rules, her memory
// and the tool table. The record is never changed by building this: the panel
// shows the whole conversation, the model is sent what the marks leave of it.
// The protocol encoders translate this neutral view; nothing below them knows
// which provider is on the other end.

import { SENT_ROLES } from "./record.js";
import { toolId } from "./tools.js";
import { CAPS, clip, estimate } from "./size.js";

/// ONE MESSAGE AS SENT — the only place that says so, which is what lets the
/// budget measure exactly what the view sends. The reader's words, the page and
/// a result are each cut to their cap here; the record keeps them whole.
export const pageText = (m) => (m.page && !m.pageMasked ? `\n\n<page>${clip(m.page, CAPS.page)}</page>` : "");
export const userText = (m) => clip(m.text || "", CAPS.text) + pageText(m);
export const toolText = (m) => (m.masked
  ? `[set aside: the result of ${toolId(m.name)} (${Math.round(((m.result || "").length / 1024) * 10) / 10} KB) — call it again if you need it]`
  : clip(m.result || "", CAPS.result));
export const summaryText = (s) => `<summary of the conversation so far>\n${s.text}\n</summary>\n\n`;

/// What one message costs to send, in estimated tokens.
export function sizeOf(m) {
  if (m.role === "user") return estimate(userText(m));
  if (m.role === "assistant") return estimate((m.text || "") + JSON.stringify(m.calls || []));
  if (m.role === "tool") return estimate(toolText(m));
  return 0;
}

/// The messages sent, from the summary on: the ones after it, the first of which
/// carries the summary ahead of its own text.
export function sent(record) {
  const from = record.summary ? record.summary.upto : 0;
  return record.messages.slice(from).filter((m) => SENT_ROLES.includes(m.role))
    .map((m, i) => (i === 0 && record.summary ? { ...m, summary: record.summary } : m));
}

/// THE VIEW: the system texts (rules, then memory when there is any — both in
/// the stable prefix), the turns, and the tools.
export function view(record, { rules, memory, tools }) {
  return {
    system: [rules, memory].filter(Boolean),
    tools,
    turns: sent(record).map((m) => (m.role === "user" ? { role: "user", text: (m.summary ? summaryText(m.summary) : "") + userText(m) }
      : m.role === "assistant" ? { role: "assistant", text: m.text || "", calls: m.calls || [] }
      : { role: "tool", id: m.id, name: m.name, text: toolText(m) })),
  };
}

/// The whole view as one string, for estimating what it costs.
export const viewText = (v) => v.system.join("") + JSON.stringify(v.tools)
  + v.turns.map((t) => t.text + (t.calls ? JSON.stringify(t.calls) : "")).join("");
