// SPDX-License-Identifier: AGPL-3.0-or-later
// Serve versioned catalog and page responses from immutable test rows.
import { createHash } from "node:crypto";

export function boardFixture(load, { immutable = false } = {}) {
  const snapshots = new Map(), catalogs = new Map();

  async function catalog(scene, weapon) {
    const index = await load(scene, weapon);
    const entries = [];
    for (const id of Object.keys(index).sort()) {
      const groups = new Map();
      for (const row of index[id]) {
        if (!groups.has(row.benchmark)) groups.set(row.benchmark, []);
        groups.get(row.benchmark).push(row);
      }
      for (const ruler of [...groups.keys()].sort()) {
        const values = groups.get(ruler);
        const revision = createHash("sha256").update(JSON.stringify([id, ruler, values])).digest("hex");
        const top = new Map();
        for (const row of values) {
          const key = JSON.stringify([row.mode, Boolean(row.riven)]);
          if (!top.has(key) || top.get(key).score < row.score) top.set(key, row);
        }
        const entry = { weapon: id, ruler, scene, revision, rows: values.length, leaders: [...top.values()] };
        if (!snapshots.has(revision)) snapshots.set(revision, { entry, values });
        entries.push(entry);
      }
    }
    return entries;
  }

  return async (url) => {
    const params = url.searchParams, weapon = params.get("weapon"), after = params.get("after") || "";
    if (url.pathname === "/api/board/pages/catalog") {
      const scene = params.get("scene") || "", key = JSON.stringify([scene, weapon]);
      const ruler = params.get("ruler"), size = ruler ? 512 : 128;
      if (immutable && !catalogs.has(key)) catalogs.set(key, catalog(scene, weapon));
      const entries = await (immutable ? catalogs.get(key) : catalog(scene, weapon));
      const visible = entries.filter(p => (!ruler || p.ruler === ruler) && p.weapon + "/" + p.ruler > after).slice(0, size + 1);
      const next = visible.length > size ? visible[size - 1].weapon + "/" + visible[size - 1].ruler : null;
      return { ok: true, partitions: visible.slice(0, size), next };
    }
    if (url.pathname !== "/api/board/pages") return null;
    const ruler = params.get("ruler"), revision = params.get("revision");
    const held = snapshots.get(revision);
    if (!held || held.entry.weapon !== weapon || held.entry.ruler !== ruler) {
      return { ok: false, error: "revision_unavailable" };
    }
    const offset = Number(after), rows = held.values.slice(offset, offset + 128);
    const end = offset + rows.length;
    return { ok: true, weapon, ruler, revision, after: offset, count: held.values.length, rows, next: end < held.values.length ? end : null };
  };
}
