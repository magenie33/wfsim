// SPDX-License-Identifier: AGPL-3.0-or-later
function placed(list, score, group = () => "") {
  const last = new Map();
  return list.map((x) => {
    const g = group(x), s = score(x), was = last.get(g);
    const rank = !was ? 1 : was.s === s ? was.rank : was.rank + 1;
    last.set(g, { s, rank });
    return { ...x, rank };
  });
}

function rankBoard(rows, benchIds, modes) {
  const order = (id) => {
    const i = benchIds.indexOf(id);
    return i < 0 ? 99 : i;
  };
  const modeOrder = (m) => {
    const i = (modes || []).indexOf(m || "base");
    return i < 0 ? 99 : i;
  };
  const riven = (r) => !!(r && r.riven);
  const modeKey = (r) => `${r.benchmark}#${r.mode || "base"}`;
  const kindKey = (r) => `${modeKey(r)}#${riven(r) ? "r" : "p"}`;
  const best = {};
  for (const r of rows) {
    best[modeKey(r)] = Math.max(best[modeKey(r)] ?? -1, r.score || 0);
    best[kindKey(r)] = Math.max(best[kindKey(r)] ?? -1, r.score || 0);
  }
  const sorted = rows.slice().sort((a, b) =>
    order(a.benchmark) - order(b.benchmark)
    || (best[modeKey(b)] || 0) - (best[modeKey(a)] || 0)
    || modeOrder(a.mode) - modeOrder(b.mode)
    || (best[kindKey(b)] || 0) - (best[kindKey(a)] || 0)
    || (riven(a) ? 1 : 0) - (riven(b) ? 1 : 0)
    || (b.score || 0) - (a.score || 0));
  const seen = {};
  return placed(sorted.map((row) => {
    const k = kindKey(row);
    seen[k] = (seen[k] || 0) + 1;
    return { row, mode: row.mode || "base", riven: riven(row), key: `${k}#${seen[k]}` };
  }), (x) => x.row.score || 0, (x) => kindKey(x.row));
}

const BOARD_HEADS_PLACES = 20;

function boardHeads(rows, k = BOARD_HEADS_PLACES) {
  const keep = new Set(rankBoard(rows, [], []).filter((x) => x.rank <= k).map((x) => x.row));
  const newest = new Map();
  for (const r of rows) {
    const had = newest.get(r.benchmark);
    if (r.measured_at && (!had || r.measured_at > had.measured_at)) newest.set(r.benchmark, r);
  }
  for (const r of newest.values()) keep.add(r);
  return rows.filter((r) => keep.has(r));
}

export { boardHeads };
