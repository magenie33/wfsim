// HER TOOLS: the door's table at request time, then her own few in a fixed
// order. Nothing here lists what the page can do — that is `wfsim.tools()`.

import { SLOTS } from "./memory.js";

/// A door id as a tool name. Providers allow [a-zA-Z0-9_-] only, and a door id
/// is lowercase letters and dots, so the swap is its own inverse.
export const toolName = (id) => id.replace(/\./g, "_");
export const toolId = (name) => name.replace(/_/g, ".");

export const OBSERVE = {
  name: "shell_page_observe",
  description: "See the page as it is now: which weapon and module are open, the build, the fight, the last result and what can be done from here.",
  input_schema: { type: "object", properties: {}, required: [] },
};

/// The conversation's own record, searchable once it has been summarised or set
/// aside: what the model is sent is trimmed, what is kept is not.
export const HISTORY = {
  name: "shell_history_search",
  description: "Search this conversation's full record — earlier messages and tool results, including ones summarised or set aside — for a word or an id.",
  input_schema: { type: "object", properties: { query: { type: "string", description: "text to find" } }, required: ["query"] },
};

export const MEMORY_SET = {
  name: "memory_set",
  description: "Remember a lasting preference the reader stated, in a slot (written over in place) or as a short note. Quote the reader's own words in `quote`: a memory their words back takes effect at once; any other is only proposed to them. Slots: "
    + Object.entries(SLOTS).map(([k, v]) => `${k} — ${v}`).join("; ") + ".",
  input_schema: { type: "object", properties: {
    slot: { type: "string", enum: Object.keys(SLOTS), description: "which slot; omit for a note" },
    value: { type: "string", description: "what to remember, in the reader's language" },
    quote: { type: "string", description: "the reader's own words this comes from" },
  }, required: ["value"] },
};

export const MEMORY_FORGET = {
  name: "memory_forget",
  description: "Forget one memory by the id shown in <memory>, when the reader asks or it no longer holds.",
  input_schema: { type: "object", properties: { id: { type: "string" } }, required: ["id"] },
};

/// Her own, in the order they are appended. A new one goes at the END, so the
/// tools already sent keep their bytes and the prompt cache keeps matching.
export const OWN = [OBSERVE, HISTORY, MEMORY_SET, MEMORY_FORGET];

/// The whole list she is sent: the door's tools (renamed for providers), then
/// her own. `doorTools` is `wfsim.tools()`.
export const allTools = (doorTools) => doorTools.map((t) => ({ ...t, name: toolName(t.name) })).concat(OWN);

export const isOwn = (name) => OWN.some((t) => t.name === name);
