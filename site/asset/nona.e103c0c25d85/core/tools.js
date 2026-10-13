// HER TOOLS: seven, fixed for a release. The door's actions reach her through
// skills (`skills.js`) and `act`; nothing here lists what the page can do.

import { SLOTS, VALUE_MAX } from "./memory.js";

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
    value: { type: "string", maxLength: VALUE_MAX, description: "what to remember, in the reader's language, briefly" },
    quote: { type: "string", description: "the reader's own words this comes from" },
  }, required: ["value"] },
};

export const MEMORY_FORGET = {
  name: "memory_forget",
  description: "Forget one memory by the id shown in <memory>, when the reader asks or it no longer holds.",
  input_schema: { type: "object", properties: { id: { type: "string" } }, required: ["id"] },
};

/// The door's table, a part at a time: the catalogue rides in the description,
/// so it is in the tools and in the cached prefix with them.
export const skillLoad = (skills, catalogueText) => ({
  name: "skill_load",
  description: "Load skills' documents — their actions, what each does and its arguments — before calling them with act. A loaded skill stays in view until a line says it was unloaded. Skills:\n" + catalogueText,
  input_schema: { type: "object", properties: {
    skills: { type: "array", items: { type: "string", enum: skills.map((s) => s.id) }, description: "which skills to load" },
  }, required: ["skills"] },
});

export const ACT = {
  name: "act",
  description: "Call one action of the page by its id, with its arguments, as a loaded skill describes them. It returns what changed. Actions that do not depend on each other can be called together in one reply.",
  input_schema: { type: "object", properties: {
    id: { type: "string", description: "the action's id, e.g. builder.mod.set" },
    args: { type: "object", description: "its arguments" },
  }, required: ["id"] },
};

export const CALC = {
  name: "calc",
  description: "Arithmetic on numbers you were sent: + - * / ( ) and %. Every number in it must be one a tool, the page, the reader or the summary gave you; whole numbers under 100 are free. Use it for any difference, ratio or percentage you state.",
  input_schema: { type: "object", properties: { expression: { type: "string", description: "e.g. (51.98 - 29.32) / 29.32 * 100" } }, required: ["expression"] },
};

/// THE SEVEN SHE IS SENT, in this order, the same on every request of a
/// release. `skills` is `wfsim.skills`; `catalogueText` is its catalogue.
export const fixedTools = (skills, catalogueText) =>
  [OBSERVE, HISTORY, MEMORY_SET, MEMORY_FORGET, skillLoad(skills, catalogueText), ACT, CALC];
