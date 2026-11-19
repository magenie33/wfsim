// WHEN AND HOW A CONVERSATION IS COMPACTED — docs/NONA.md §"The context
// budget". Nothing here estimates: `pressed` reads what the provider said the
// last request cost, and `plan` says what to set aside and where to summarise.
// Nothing removes a message: setting aside is a mark, summarising moves where
// the request starts, and the record stays whole.

import { HIGH } from "./size.js";

/// Where this turn starts: the reader's newest message.
function turnStart(record) {
  for (let i = record.messages.length - 1; i >= 0; i--) if (record.messages[i].role === "user") return i;
  return record.messages.length;
}

/// WHAT THE LAST REQUEST COST, as the provider billed it: its input and the
/// reply, which the next request carries too. Null before any reply.
export function used(record) {
  for (let i = record.messages.length - 1; i >= 0; i--) {
    const u = record.messages[i].usage;
    if (record.messages[i].role === "assistant" && u) return u.input + u.output;
  }
  return null;
}

/// NEAR THE WINDOW: the last request filled past HIGH of it. A model whose
/// window its address does not state is compacted only when it says too long.
export const pressed = (record, context) => !!context && (used(record) || 0) >= context * HIGH;

/// WHAT TO DO WHEN PRESSED: this turn's results but the newest two are set
/// aside, and every turn before this one is summarised. Both empty means there
/// is nothing left to compact, and the provider's answer stands.
export function plan(record) {
  const msgs = record.messages;
  const from = record.summary ? record.summary.upto : 0;
  const start = Math.max(from, turnStart(record));
  const results = [];
  for (let i = start; i < msgs.length; i++) if (msgs[i].role === "tool" && !msgs[i].masked && !msgs[i].skills) results.push(i);
  return { marks: { tools: results.slice(0, -2) }, summarize: start > from ? start : null };
}

/// The skills whose documents the next request carries: those the summary
/// carries, then every one attached or loaded since.
export function loadedSkills(record) {
  const from = record.summary ? record.summary.upto : 0;
  const ids = new Set(((record.summary && record.summary.skills) || []).filter((d) => !d.unloaded).map((d) => d.id));
  record.messages.forEach((m, i) => {
    if (i < from) return;
    if ((m.role === "user" && m.preload && !m.skillsMasked) || (m.role === "tool" && m.skills && !m.masked)) (m.skills || []).forEach((x) => ids.add(x));
  });
  return ids;
}

/// The record with those marks written in — a new record; the old is untouched.
/// Written INTO the record so the next request is sent the same prefix.
export function applyMarks(record, { tools = [] }) {
  if (!tools.length) return record;
  const t = new Set(tools);
  return { ...record, messages: record.messages.map((m, i) => (t.has(i) ? { ...m, masked: true } : m)) };
}
