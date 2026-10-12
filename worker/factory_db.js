// SPDX-License-Identifier: AGPL-3.0-or-later
// THE FACT FACTORY'S DATABASE — docs/BOARD.md §"The fact factory". Its tables
// (questions, demands, answers, verifiers, verifier_hours, watchdog_told,
// points_pool) live in D1 (`LIBRARY`) or, with `FACTORY_DB = "pg"`, in Postgres
// through Hyperdrive (`HYPERDRIVE`); the library and the board stay in D1
// either way. Every query on those tables asks `factoryDb(env)`, never
// `env.LIBRARY`, and states its Postgres text where the dialects differ (`pick`).
import postgres from "./vendor/postgres/cf/src/index.js";

/// THE DATABASE THE FACTORY'S TABLES ARE IN for this request.
export const factoryDb = (env) => env.FACTORY || env.LIBRARY;
export const isPg = (db) => !!db && db.dialect === "pg";
/// ONE QUERY IN EACH DIALECT: SQLite's text, or Postgres's where it differs.
export const pick = (db, lite, pg) => (isPg(db) ? pg : lite);
/// WHETHER TWO STATEMENTS GO IN ONE TRANSACTION: only on one database.
export const sameDb = (env) => factoryDb(env) === env.LIBRARY;

/// PLACEHOLDERS ARE BOUND, NEVER REWRITTEN AS SQL: `?` in order and `?N` by
/// number become Postgres's `$N`, outside quoted text. The query itself is
/// written for Postgres by its caller; this numbers its parameters.
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

/// D1'S SHAPE OVER A POSTGRES CLIENT — `{ query(text, params) → { rows,
/// affected }, transaction(fn) }` — so the factory's code reads one way: `.first()`,
/// `.all()` → `{ results }`, `.run()` → `{ meta: { changes } }`, and `batch`
/// one transaction as D1's is.
export function pgDb(client) {
  const exec = (c, s) => c.query(placeholders(s.sql), s.args);
  const stmt = (sql, args = []) => ({
    sql, args,
    bind: (...a) => stmt(sql, a),
    first: async () => (await exec(client, { sql, args })).rows[0] ?? null,
    all: async () => ({ results: (await exec(client, { sql, args })).rows }),
    run: async () => ({ meta: { changes: (await exec(client, { sql, args })).affected } }),
  });
  return {
    dialect: "pg",
    prepare: (sql) => stmt(sql),
    batch: (list) => client.transaction(async (tx) => {
      const out = [];
      for (const s of list) {
        const r = await exec(tx, s);
        out.push({ results: r.rows, meta: { changes: r.affected } });
      }
      return out;
    }),
  };
}

/// POSTGRES.JS OVER HYPERDRIVE, one client a request (Hyperdrive pools the
/// connections). A count or a sum is a number, as D1 answers it.
export const POSTGRES_OPTIONS = {
  max: 5, fetch_types: false, prepare: true, idle_timeout: 5,
  types: { bigint: { to: 20, from: [20, 1700], serialize: (x) => String(x), parse: (x) => Number(x) } },
};
const hyperdriveClient = (env) => postgresClient(postgres(env.HYPERDRIVE.connectionString, POSTGRES_OPTIONS));
/// `{ query, transaction, end }` over a postgres.js instance.
export function postgresClient(sql) {
  const wrap = (q) => ({
    query: async (text, params) => {
      const rows = await q.unsafe(text, params);
      return { rows: [...rows], affected: rows.count ?? 0 };
    },
  });
  return { ...wrap(sql), transaction: (fn) => sql.begin((tx) => fn(wrap(tx))), end: () => sql.end({ timeout: 5 }) };
}

/// THIS REQUEST'S ENV, with the factory's database when it is Postgres, and
/// what closes it once the request is answered.
export function withFactory(env) {
  if (env.FACTORY_DB !== "pg" || !env.HYPERDRIVE) return { env, close: async () => {} };
  const client = hyperdriveClient(env);
  return { env: { ...env, FACTORY: pgDb(client) }, close: () => client.end() };
}
