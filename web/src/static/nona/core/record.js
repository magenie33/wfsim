// THE SHAPES NONA KEEPS, and how every version of them becomes the current one.
//
// These live in readers' browsers, so they are a wire: a field that ships keeps
// its name (FROZEN), and a change of shape is a new version with a migration
// here — never an edit in place. docs/NONA.md §"What is stored".

export const V = 1;

/// Every field name a shipped version has written, by kind. It only grows:
/// `test_nona_core` fails if a name here is missing from the current shape.
export const FROZEN = {
  conversation: ["v", "id", "title", "pinned", "created_at", "updated_at", "weapon", "made", "pairs", "summary", "usage", "messages"],
  message: ["role", "text", "page", "at", "pageMasked", "calls", "usage", "id", "name", "ok", "line", "result", "masked", "pair", "state"],
  memory: ["v", "paused", "items"],
  memoryItem: ["id", "kind", "key", "value", "status", "source", "created_at", "updated_at", "history"],
  settings: ["v", "base", "proto", "model", "context", "price", "remember", "concise", "key"],
};

/// The roles a model is sent. The others are drawn in the panel only.
export const SENT_ROLES = ["user", "assistant", "tool"];

export function newConversation({ id, now, weapon }) {
  return { v: V, id, title: "", pinned: false, created_at: now, updated_at: now, weapon: weapon || null,
    made: [], pairs: [], summary: null, usage: { input: 0, output: 0, cached: 0, cost: 0 }, messages: [] };
}

/// A title from the first thing asked, and the weapon it was asked on: free,
/// and specific enough to find again.
export function titleOf(text, weaponName) {
  const first = String(text).split("\n")[0].trim();
  return `${weaponName ? weaponName + " · " : ""}${first.length > 28 ? first.slice(0, 28) + "…" : first}`;
}

/// ANY VERSION THIS CODE HAS WRITTEN, brought to the current one. A version
/// newer than the code comes back `null`: it is left alone, never guessed at.
export function migrate(kind, value) {
  if (!value || typeof value !== "object") return null;
  const v = kind === "memory" ? (value.v ?? value.schema ?? 0) : (value.v ?? 0);
  if (v > V) return null;
  if (kind === "conversation") {
    return { pairs: [], summary: null, made: [], usage: { input: 0, output: 0, cached: 0, cost: 0 }, pinned: false,
      ...value, v: V };
  }
  if (kind === "memory") {
    const { schema, ...rest } = value;
    return { paused: false, items: [], ...rest, v: V };
  }
  if (kind === "settings") return { ...value, v: V };
  return null;
}
