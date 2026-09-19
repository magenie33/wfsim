// THE LAST STEP, once setting results aside is not enough to bring the history
// under its cap: the oldest turns are written up by the model, and what it wrote
// replaces them in what is sent. Where to cut is `plan`'s decision. A measured
// number survives only with where it came from — the summary is told to drop
// one that has lost its source — so "every number she states was sent to her"
// holds across it.

import { CAPS, clip } from "./size.js";
import { toolText } from "./view.js";
import { toolId } from "./tools.js";

export const SUMMARY_RULES = [
  "You write the working summary of a conversation between a Warframe player and Nona, the assistant of the WFSim calculator, so the conversation can continue without its early part.",
  "Write in the conversation's language, as short sections: the reader's goal; preferences they confirmed; the builds, scenarios, rivens and targets in play by their ids; every number measured, each with the tool and the build and fight it was measured on; decisions made; what is still to do.",
  "Keep a number only with its source. Drop any number whose source you cannot state. Never invent one.",
  "An earlier summary is folded into the new one, not repeated beside it. Stay within 2,000 characters: keep what the conversation still needs and drop what it has finished with.",
].join("\n");

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

/// The summary in place — cut to its cap whatever the model wrote, since a cap
/// the model is only asked to respect is not a cap.
export const applySummary = (record, cut, text) => ({ ...record, summary: { upto: cut, text: clip(text, CAPS.summary - 50) } });
