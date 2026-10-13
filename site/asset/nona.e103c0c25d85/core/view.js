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
/// The skill documents attached to a reader's message (the module they are on).
export const preloadText = (m) => (m.preload && !m.skillsMasked ? `\n\n${m.preload}` : "");
export const userText = (m) => clip(m.text || "", CAPS.text) + pageText(m) + preloadText(m);
/// A SKILL'S RESULT IS NOT CUT: its size is bounded where it is made, and it
/// is K's to account for. Unloaded, it says so and how to have it back.
export const toolText = (m) => (m.skills
  ? (m.masked ? `[skill ${m.skills.join(", ")} unloaded — load it again if you need it]` : m.result || "")
  : m.masked
    ? `[set aside: the result of ${m.action || toolId(m.name)} (${Math.round(((m.result || "").length / 1024) * 10) / 10} KB) — call it again if you need it]`
    : clip(m.result || "", CAPS.result));
/// THE SUMMARY, and after it the skill documents it carries — the ones in view
/// when it was written, as they were loaded, so she need not load them again.
export const summarySkillsText = (s) => (s.skills || []).filter((d) => !d.unloaded).map((d) => `${d.text}\n\n`).join("");
export const summaryText = (s) => `<summary of the conversation so far>\n${s.text}\n</summary>\n\n${summarySkillsText(s)}`;

/// WHAT ONE MESSAGE COSTS TO SEND, in estimated tokens, split between K (the
/// skill documents it carries) and the zone it sits in (H or P).
export function skillSizeOf(m) {
  if (m.role === "user") return m.preload && !m.skillsMasked ? estimate(preloadText(m)) : 0;
  if (m.role === "tool" && m.skills && !m.masked) return estimate(toolText(m));
  return 0;
}
export const checkText = (m) => `<check>${m.text}</check>`;
export function sizeOf(m) {
  if (m.role === "user") return estimate(userText(m)) - skillSizeOf(m);
  if (m.role === "check") return estimate(checkText(m));
  if (m.role === "assistant") return estimate((m.text || "") + JSON.stringify(m.calls || []));
  if (m.role === "tool") return estimate(toolText(m)) - skillSizeOf(m);
  return 0;
}

/// The messages sent, from the summary on: the ones after it, the first of which
/// carries the summary ahead of its own text. A reply with neither words nor
/// calls says nothing, and some providers refuse one, so it is not sent.
const blank = (m) => m.role === "assistant" && !String(m.text || "").trim() && !(m.calls || []).length;
export function sent(record) {
  const from = record.summary ? record.summary.upto : 0;
  return record.messages.slice(from).filter((m) => SENT_ROLES.includes(m.role) && !blank(m))
    .map((m, i) => (i === 0 && record.summary ? { ...m, summary: record.summary } : m));
}

/// THE VIEW: the system texts (rules, then memory when there is any — both in
/// the stable prefix), the turns, and the tools.
export function view(record, { rules, memory, tools }) {
  return {
    system: [rules, memory].filter(Boolean),
    tools,
    turns: sent(record).map((m) => (m.role === "user" ? { role: "user", text: (m.summary ? summaryText(m.summary) : "") + userText(m) }
      : m.role === "check" ? { role: "user", text: (m.summary ? summaryText(m.summary) : "") + checkText(m) }
      : m.role === "assistant" ? { role: "assistant", text: m.text || "", calls: m.calls || [] }
      : { role: "tool", id: m.id, name: m.name, text: toolText(m) })),
  };
}

/// The whole view as one string, for estimating what it costs.
export const viewText = (v) => v.system.join("") + JSON.stringify(v.tools)
  + v.turns.map((t) => t.text + (t.calls ? JSON.stringify(t.calls) : "")).join("");
