// SPDX-License-Identifier: AGPL-3.0-or-later
// THE WORKER'S DATABASES — docs/BOARD.md §"The fact factory". The library, the
// fact factory's tables and the accounts live in D1 or in Postgres through
// Hyperdrive, as `withFactory` reads the switches. A query on the factory's
// tables asks `factoryDb(env)`; every query states its Postgres text where the
// dialects differ (`pick`), and the same D1-shaped object answers both ways.
import postgres from "./vendor/postgres/cf/src/index.js";
import { placeholders } from "./placeholders.js";

/// THE DATABASE THE FACTORY'S TABLES ARE IN for this request.
export const factoryDb = (env) => env.FACTORY || env.LIBRARY;
export const isPg = (db) => !!db && db.dialect === "pg";
/// ONE QUERY OR ONE PIECE OF ONE IN EACH DIALECT: SQLite's text, or Postgres's
/// where it differs — text, never a list of statements, which the shadow check
/// (scripts/pg/shadow.mjs) cannot see into. Under it SQLite's text carries the
/// Postgres text, so one run checks both.
export const pick = (db, lite, pg) => (isPg(db) ? pg
  : globalThis.__WFSIM_PG_SHADOW__ ? globalThis.__WFSIM_PG_SHADOW__(lite, pg) : lite);
/// WHETHER TWO STATEMENTS GO IN ONE TRANSACTION: only on one database.
export const sameDb = (env) => factoryDb(env) === env.LIBRARY;

export { placeholders };

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
    // A DEADLOCK OR A SERIALIZATION FAILURE IS TRIED AGAIN, twice: Postgres
    // rolls the whole transaction back, so running it again is running it once.
    batch: async (list) => {
      for (let i = 0; ; i++) {
        try {
          // EVERY STATEMENT SENT AT ONCE, pipelined on the one connection: sent one
          // round trip at a time, a settlement held its row locks for seconds and
          // every other credit to those devices queued behind it.
          return await client.transaction(async (tx) => (await Promise.all(list.map((s) => exec(tx, s))))
            .map((r) => ({ results: r.rows, meta: { changes: r.affected } })));
        } catch (e) {
          if (i >= 2 || !["40P01", "40001"].includes(e && e.code)) throw e;
          await new Promise((r) => setTimeout(r, 25 * (i + 1) + Math.random() * 50));
        }
      }
    },
  };
}

/// POSTGRES.JS OVER HYPERDRIVE, one client a request (Hyperdrive pools the
/// connections). A count or a sum is a number, as D1 answers it.
export const POSTGRES_OPTIONS = {
  max: 5, fetch_types: false, prepare: true, idle_timeout: 5,
  types: { bigint: { to: 20, from: [20, 1700], serialize: (x) => String(x), parse: (x) => Number(x) } },
};
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

/// THIS REQUEST'S ENV, its databases on Postgres where its switches say, and
/// what closes them once it is answered. `DB_LIBRARY = "pg"` serves the
/// library — and the factory with it: one database, one transaction — from
/// `HYPERDRIVE`; `DB_ACCOUNTS = "pg"` serves the accounts from their own
/// database (`HYPERDRIVE_ACCOUNTS`). `FACTORY_DB = "pg"` alone moves the
/// factory's tables and leaves the library in D1. Unset, every one is D1's.
export function withFactory(env) {
  const clients = [], next = { ...env };
  const pg = (binding) => {
    const client = postgresClient(postgres(binding.connectionString, POSTGRES_OPTIONS));
    clients.push(client);
    return pgDb(client);
  };
  if (env.HYPERDRIVE && (env.DB_LIBRARY === "pg" || env.FACTORY_DB === "pg")) {
    next.FACTORY = pg(env.HYPERDRIVE);
    if (env.DB_LIBRARY === "pg") next.LIBRARY = next.FACTORY;
  }
  if (env.HYPERDRIVE_ACCOUNTS && env.DB_ACCOUNTS === "pg") next.ACCOUNTS = pg(env.HYPERDRIVE_ACCOUNTS);
  return { env: clients.length ? next : env, close: () => Promise.all(clients.map((c) => c.end())) };
}
