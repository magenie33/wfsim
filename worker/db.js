// SPDX-License-Identifier: AGPL-3.0-or-later
// THE WORKER'S DATABASES — docs/BOARD.md §"Postgres". The library with the fact
// factory's tables, and the accounts in a database of their own, both Postgres
// through Hyperdrive (`withDatabases`).
import postgres from "./vendor/postgres/cf/src/index.js";
import { placeholders } from "./placeholders.js";

export { placeholders };

/// ONE STATEMENT API OVER A POSTGRES CLIENT — `{ query(text, params) → { rows,
/// affected }, transaction(fn) }` — so every query reads one way: `.first()`,
/// `.all()` → `{ results }`, `.run()` → `{ meta: { changes } }`, and `batch`
/// one transaction.
export function pgDb(client) {
  const exec = (c, s) => c.query(placeholders(s.sql), s.args);
  // WHAT THIS HANDLE COST ITS REQUEST (docs/ANALYTICS.md §"Factory calls"): a
  // call is one wait on the database — a statement, or a batch pipelined as one.
  const cost = { calls: 0, statements: 0, db_ms: 0 };
  const timed = async (n, f) => {
    const t0 = Date.now();
    try { return await f(); } finally { cost.calls += 1; cost.statements += n; cost.db_ms += Date.now() - t0; }
  };
  const one = (sql, args) => timed(1, () => exec(client, { sql, args }));
  const stmt = (sql, args = []) => ({
    sql, args,
    bind: (...a) => stmt(sql, a),
    first: async () => (await one(sql, args)).rows[0] ?? null,
    all: async () => ({ results: (await one(sql, args)).rows }),
    run: async () => ({ meta: { changes: (await one(sql, args)).affected } }),
  });
  return {
    cost,
    prepare: (sql) => stmt(sql),
    // A DEADLOCK OR A SERIALIZATION FAILURE IS TRIED AGAIN, twice: Postgres
    // rolls the whole transaction back, so running it again is running it once.
    batch: async (list) => {
      for (let i = 0; ; i++) {
        try {
          // EVERY STATEMENT SENT AT ONCE, pipelined on the one connection: sent one
          // round trip at a time, a settlement held its row locks for seconds and
          // every other credit to those devices queued behind it.
          return await timed(list.length, async () => (await client.transaction(async (tx) => Promise.all(list.map((s) => exec(tx, s)))))
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
/// connections). A count or a sum is a number.
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

/// THIS REQUEST'S ENV with its databases, and what closes them once it is
/// answered: `LIBRARY` from `HYPERDRIVE`, `ACCOUNTS` from `HYPERDRIVE_ACCOUNTS`.
export function withDatabases(env) {
  const clients = [], next = { ...env };
  const pg = (binding) => {
    const client = postgresClient(postgres(binding.connectionString, POSTGRES_OPTIONS));
    clients.push(client);
    return pgDb(client);
  };
  if (env.HYPERDRIVE) next.LIBRARY = pg(env.HYPERDRIVE);
  if (env.HYPERDRIVE_ACCOUNTS) next.ACCOUNTS = pg(env.HYPERDRIVE_ACCOUNTS);
  return { env: next, close: () => Promise.all(clients.map((c) => c.end())) };
}
