// SPDX-License-Identifier: AGPL-3.0-or-later
/// PLACEHOLDERS ARE BOUND, NEVER REWRITTEN AS SQL: `?` in order and `?N` by
/// number become Postgres's `$N`, outside quoted text. The query itself is
/// written for Postgres by its caller; this numbers its parameters. Imports
/// nothing, so the scripts share it (scripts/db.mjs) wherever they run.
const numbered = new Map();
export function placeholders(sql) {
  let out = numbered.get(sql);
  if (out) return out;
  let n = 0, i = 0;
  out = "";
  while (i < sql.length) {
    const c = sql[i];
    if (c === "'" || c === '"') {
      const end = sql.indexOf(c, i + 1);
      const stop = end < 0 ? sql.length : end + 1;
      out += sql.slice(i, stop);
      i = stop;
    } else if (c === "?") {
      const m = /^\d+/.exec(sql.slice(i + 1));
      out += `$${m ? m[0] : ++n}`;
      i += 1 + (m ? m[0].length : 0);
    } else {
      out += c;
      i += 1;
    }
  }
  numbered.set(sql, out);
  return out;
}
