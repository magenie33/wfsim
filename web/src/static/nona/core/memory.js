// A SMALL, STANDING PROFILE of the reader. What it holds is decided by slots,
// each written over in place, so a changed mind replaces the old answer instead
// of contradicting it (the old value is kept for undo). The whole profile rides
// every request, and an old item is marked as old: research on agent memory
// found elaborate retrieval earns little, and asking beats guessing.
//
// Pure: every function takes the memory and returns a new one.

import { V } from "./record.js";

/// THE SLOTS, and what each is for — the model reads the descriptions.
export const SLOTS = {
  answers: "how the reader likes answers: language, length, tone",
  riven_policy: "whether they use rivens",
  content: "what they play: Steel Path, which factions, which missions",
  budget: "what they will spend: Forma, Primed or Galvanized mods, Umbral",
  owned: "key items they own: Primed mods, arcanes, Incarnon adapters, Helminth",
  disliked: "mods or playstyles they do not want",
  playstyle: "how they play: aim, range, fire discipline",
};
/// What each slot is called on the page; the id itself is never shown.
export const SLOT_LABELS = { answers: "Answers", riven_policy: "Rivens", content: "Content", budget: "Budget",
  owned: "Owned", disliked: "Dislikes", playstyle: "Playstyle" };
/// Past this, a memory is shown to her as old, to be confirmed before use.
export const STALE_DAYS = 90;
export const NOTES_MAX = 30;
const DAY = 864e5;

export const emptyMemory = () => ({ v: V, paused: false, items: [] });

const squash = (s) => String(s).replace(/\s+/g, "");

/// WRITE ONE. It takes effect at once only when the reader's own words back it:
/// `quote` must appear in their latest message. Anything else — her inference,
/// or words that came out of a tool result — waits as proposed until the reader
/// takes it, which keeps a name in someone else's build from writing itself
/// into this reader's profile. `ctx`: { lastUserText, conversation, now, id }.
export function set(mem, { slot, value, quote }, ctx) {
  if (mem.paused) return { mem, result: { ok: false, reason: "memory_off" } };
  if (!value || typeof value !== "string") return { mem, result: { ok: false, reason: "missing_argument", argument: "value" } };
  if (slot && !SLOTS[slot]) return { mem, result: { ok: false, reason: "bad_argument", argument: "slot", alternatives: Object.keys(SLOTS) } };
  const byUser = !!quote && squash(ctx.lastUserText || "").includes(squash(quote));
  const status = byUser ? "active" : "proposed";
  const source = { conversation: ctx.conversation, quote: quote || "", by: byUser ? "user" : "inferred" };
  const items = mem.items.map((x) => ({ ...x }));
  let item = slot ? items.find((x) => x.key === slot) : null;
  if (item) {
    item.history = [...(item.history || []), { value: item.value, until: ctx.now }].slice(-5);
    Object.assign(item, { value, status, updated_at: ctx.now, source });
  } else {
    item = { id: ctx.id, kind: slot ? "profile" : "note", ...(slot ? { key: slot } : {}), value, status,
      source, created_at: ctx.now, updated_at: ctx.now, history: [] };
    items.push(item);
  }
  const notes = items.filter((x) => x.kind === "note");
  const keep = new Set(notes.slice(-NOTES_MAX));
  const next = { ...mem, items: items.filter((x) => x.kind !== "note" || keep.has(x)) };
  return { mem: next, result: { ok: true, id: item.id, status, text: status === "active" ? "remembered" : "proposed; the reader will confirm it" } };
}

export function forget(mem, id) {
  const items = mem.items.filter((x) => x.id !== id);
  return { mem: { ...mem, items }, result: items.length < mem.items.length ? { ok: true } : { ok: false, reason: "unknown_memory" } };
}

/// Undo the last write to an item: its previous value back, or the item gone.
export function undo(mem, id, now) {
  const it = mem.items.find((x) => x.id === id);
  if (!it) return mem;
  const history = [...(it.history || [])];
  const prev = history.pop();
  if (!prev) return { ...mem, items: mem.items.filter((x) => x !== it) };
  return { ...mem, items: mem.items.map((x) => (x === it ? { ...x, value: prev.value, status: "active", updated_at: now, history } : x)) };
}

/// The reader takes a proposed memory, or writes a new value over one.
export function confirm(mem, id, value, now) {
  return { ...mem, items: mem.items.map((x) => (x.id !== id ? x : value == null ? { ...x, status: "active" }
    : { ...x, value, status: "active", updated_at: now, history: [...(x.history || []), { value: x.value, until: now }].slice(-5) })) };
}

/// THE PROFILE AS SHE READS IT, one line per active memory; empty when there is
/// none or memory is off — and then nothing is sent at all.
export function block(mem, now, off) {
  if (off || mem.paused) return "";
  const live = mem.items.filter((x) => x.status === "active");
  if (!live.length) return "";
  return "<memory of this reader>\n" + live.map((x) => {
    const age = Math.floor((now - x.updated_at) / DAY);
    return `- [${x.id}] ${x.key ? x.key + ": " : "note: "}${x.value}${age > STALE_DAYS ? ` (saved ${age} days ago — confirm before relying on it)` : ""}`;
  }).join("\n") + "\n</memory>";
}
