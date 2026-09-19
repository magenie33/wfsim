// THE SECOND STEP, once setting results aside is not enough: everything before
// the newest four reader turns is written up by the model, and what it wrote
// replaces them in what is sent. A measured number survives only with where it
// came from — the summary is told to drop one that has lost its source — so
// "every number from a tool" holds across it.

import { BUDGET, room, recordCost } from "./budget.js";
import { toolText } from "./view.js";
import { toolId } from "./tools.js";

export const KEEP_TURNS = 4;
export const SUMMARY_RULES = [
  "You write the working summary of a conversation between a Warframe player and Nona, the assistant of the WFSim calculator, so the conversation can continue without its early part.",
  "Write in the conversation's language, as short sections: the reader's goal; preferences they confirmed; the builds, scenarios, rivens and targets in play by their ids; every number measured, each with the tool and the build and fight it was measured on; decisions made; what is still to do.",
  "Keep a number only with its source. Drop any number whose source you cannot state. Never invent one.",
].join("\n");

/// Where to cut — the index of the fourth-newest reader turn — or null when the
/// record still fits, is too short, or is already summarised that far.
export function wantsSummary(record, { context, fixed, ratio }) {
  const users = record.messages.map((m, i) => (m.role === "user" ? i : -1)).filter((i) => i >= 0);
  if (users.length <= KEEP_TURNS) return null;
  const cut = users[users.length - KEEP_TURNS];
  if (record.summary && record.summary.upto >= cut) return null;
  return recordCost(record, fixed, ratio) > room(context) * BUDGET.force_at ? cut : null;
}

/// What the summariser is given: the earlier summary, then the record up to the
/// cut, one line per message.
export function summaryInput(record, cut, callLine) {
  const from = record.summary ? record.summary.upto : 0;
  const lines = record.messages.slice(from, cut).filter((m) => m.role !== "card" && m.role !== "memory").map((m) =>
    (m.role === "tool" ? `[tool ${toolId(m.name)}] ${toolText(m)}`
      : m.role === "assistant" ? `[Nona] ${m.text || ""}${(m.calls || []).map((x) => ` <calls ${callLine(x)}>`).join("")}`
      : m.role === "user" ? `[reader] ${m.text}` : `[note] ${m.text}`)).join("\n");
  return `${record.summary ? `<earlier summary>\n${record.summary.text}\n</earlier summary>\n\n` : ""}<record>\n${lines}\n</record>`;
}

export const applySummary = (record, cut, text) => ({ ...record, summary: { upto: cut, text } });
