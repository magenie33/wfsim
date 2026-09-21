// WHAT NONA KEEPS, and where: docs/NONA.md §"What is stored". Every value read
// goes through `migrate`, so the rest of her code only ever sees the current
// shape; a value newer than this code is left alone rather than written over.

import { V, migrate } from "../core/record.js";
import * as memoryOps from "../core/memory.js";

const SETTINGS = "wfsim-nona";
/// THE KEY LIVES FOR THIS TAB unless the reader asks otherwise: a key in
/// localStorage is one cross-site script away from anyone (OWASP), so keeping
/// it past the tab is a choice the reader makes, not the default.
const SESSION_KEY = "wfsim-nona-key";
const MEMORY = "wfsim-nona-memory";
const CALIB = "wfsim-nona-calib";
/// How many conversations are kept besides the pinned ones.
const KEEP_CONVERSATIONS = 100;

const read = (key) => { try { return JSON.parse(localStorage.getItem(key) || "null"); } catch (_) { return null; } };
const write = (key, v) => { try { localStorage.setItem(key, JSON.stringify(v)); } catch (_) { /* a private window keeps nothing */ } };
const newer = (value) => !!value && typeof value === "object" && (value.v ?? value.schema ?? 0) > V;

/// The saved settings, with the key from wherever the reader chose to keep it.
export function settings() {
  const s = migrate("settings", read(SETTINGS) || {}) || {};
  let key = s.remember ? s.key : "";
  if (!s.remember) { try { key = sessionStorage.getItem(SESSION_KEY) || ""; } catch (_) { /* no session storage */ } }
  return { ...s, key: key || "" };
}

export function saveSettings(v) {
  if (newer(read(SETTINGS))) return;
  const { key, ...rest } = { ...v, v: V };
  try {
    localStorage.setItem(SETTINGS, JSON.stringify(v.remember ? { ...rest, key } : rest));
    if (v.remember) sessionStorage.removeItem(SESSION_KEY); else sessionStorage.setItem(SESSION_KEY, key || "");
  } catch (_) { /* a private window keeps nothing */ }
}

// ---- memory: every change is a core operation, then one write ----------------

export const memory = {
  get: () => migrate("memory", read(MEMORY)) || memoryOps.emptyMemory(),
  put(m) { if (!newer(read(MEMORY))) write(MEMORY, m); },
  /// Apply `fn` to the memory and keep what it returns.
  change(fn) { const next = fn(memory.get()); memory.put(next); return next; },
  keep: (id) => memory.change((m) => memoryOps.confirm(m, id, null, Date.now())),
  edit: (id, value) => memory.change((m) => memoryOps.confirm(m, id, value, Date.now())),
  undo: (id) => memory.change((m) => memoryOps.undo(m, id, Date.now())),
  forget(id) { let out; memory.change((m) => { const r = memoryOps.forget(m, id); out = r.result; return r.mem; }); return out; },
  pause: (on) => memory.change((m) => ({ ...m, paused: !!on })),
  clear: () => memory.change((m) => ({ ...memoryOps.emptyMemory(), paused: m.paused })),
  restore: () => memory.change(memoryOps.restore),
};

// ---- the token estimate's correction, per model -------------------------------

export const ratio = (model) => (read(CALIB) || {})[model] || 1;
export function saveRatio(model, r) {
  const c = read(CALIB) || {};
  c[model] = r;
  write(CALIB, c);
}

// ---- conversations ------------------------------------------------------------
//
// KEPT WHOLE, in IndexedDB — localStorage cannot hold a long one with its tool
// results. Without IndexedDB they live for the page only.

const db = (() => {
  let dbp = null;
  const mem = new Map();
  const open = () => {
    if (!dbp) {
      dbp = new Promise((ok) => {
        try {
          const req = indexedDB.open("wfsim-nona", 1);
          req.onupgradeneeded = () => req.result.createObjectStore("conversations", { keyPath: "id" });
          req.onsuccess = () => ok(req.result);
          req.onerror = () => ok(null);
        } catch (_) { ok(null); }
      });
    }
    return dbp;
  };
  const tx = async (mode, fn) => {
    const d = await open();
    if (!d) return fn(null);
    return new Promise((ok) => {
      const t = d.transaction("conversations", mode);
      const r = fn(t.objectStore("conversations"));
      t.oncomplete = () => ok(r && "result" in r ? r.result : undefined);
      t.onerror = () => ok(undefined);
    });
  };
  return {
    all: async () => {
      const r = await tx("readonly", (s) => (s ? s.getAll() : [...mem.values()]));
      return Array.isArray(r) ? r : [];
    },
    put: (c) => tx("readwrite", (s) => (s ? s.put(JSON.parse(JSON.stringify(c))) : mem.set(c.id, c))),
    del: (id) => tx("readwrite", (s) => (s ? s.delete(id) : mem.delete(id))),
  };
})();

/// Every kept conversation this code can read, pinned first, then newest; the
/// oldest unpinned past the limit are deleted on the way.
export async function conversations() {
  const all = (await db.all()).map((c) => migrate("conversation", c)).filter(Boolean);
  all.sort((a, b) => (b.pinned - a.pinned) || (b.updated_at - a.updated_at));
  const extra = all.filter((c) => !c.pinned).slice(KEEP_CONVERSATIONS);
  for (const c of extra) await db.del(c.id);
  return all.filter((c) => !extra.includes(c));
}

export const putConversation = (c) => db.put(c);
export const deleteConversation = (id) => db.del(id);
