// SPDX-License-Identifier: AGPL-3.0-or-later
// THE DATABASES AS THE SCRIPTS REACH THEM — docs/BOARD.md §"Postgres". One
// statement (`?` or `?N` bound, never interpolated), its rows out, from the
// Postgres the database's URL names: `PG_URL` for the library,
// `PG_ACCOUNTS_URL` for the accounts. `pglite:<dir>` is an on-disk PGlite, for
// checks.
//
//   import { query, answer } from "./db.mjs";
//   const rows = await query("SELECT id FROM builds WHERE at > ?", [since]);
import { placeholders } from "../worker/placeholders.js";

const URLS = { library: "PG_URL", accounts: "PG_ACCOUNTS_URL" };
/// A STATEMENT HELD PAST THIS IS ENDED: a hung one holds a workflow until
/// GitHub's six-hour cap (scripts/fetch_queue.sh).
const STATEMENT_MS = 180_000;

const clients = new Map();
async function client(database) {
  if (clients.has(database)) return clients.get(database);
  const name = URLS[database];
  if (!name) throw new Error(`no such database: ${database}`);
  const url = process.env[name];
  if (!url) throw new Error(`${name} is not set — the ${database}'s Postgres (docs/BOARD.md §"Postgres")`);
  let c;
  if (url.startsWith("pglite:")) {
    const { PGlite } = await import("./pg/node_modules/@electric-sql/pglite/dist/index.js");
    const pg = new PGlite(url.slice("pglite:".length), { parsers: { 20: (x) => Number(x), 1700: (x) => Number(x) } });
    c = { run: async (text, params) => { const r = await pg.query(text, params); return { rows: r.rows, changes: r.affectedRows ?? 0 }; },
      end: () => pg.close() };
  } else {
    const { default: postgres } = await import("./pg/node_modules/postgres/src/index.js");
    // A COUNT OR A SUM IS A NUMBER, as the worker reads it (worker/db.js).
    const sql = postgres(url, { max: 4, onnotice: () => {}, connect_timeout: 15, idle_timeout: 5, fetch_types: false,
      connection: { statement_timeout: STATEMENT_MS, application_name: "wfsim-scripts" },
      types: { bigint: { to: 20, from: [20, 1700], serialize: (x) => String(x), parse: (x) => Number(x) } } });
    c = { run: async (text, params) => { const r = await sql.unsafe(text, params); return { rows: [...r], changes: r.count ?? 0 }; },
      end: () => sql.end({ timeout: 5 }) };
  }
  clients.set(database, c);
  return c;
}

/// ONE STATEMENT'S ANSWER — `{ success, result: [{ results, meta: { changes } }] }`,
/// the shape the shell scripts parse (scripts/pgq.mjs); a refusal, or a
/// database whose URL is not set, is `{ success: false, errors }`.
export async function answer(sql, params = [], database = "library") {
  try {
    const r = await (await client(database)).run(placeholders(sql), params);
    return { success: true, result: [{ results: r.rows, success: true, meta: { changes: r.changes } }] };
  } catch (e) {
    return { success: false, errors: [{ code: e.code || "", message: e.message }] };
  }
}

/// ITS ROWS, or a thrown refusal.
export async function query(sql, params = [], database = "library") {
  const j = await answer(sql, params, database);
  if (!j.success) throw new Error(`postgres: ${JSON.stringify(j.errors)}`);
  return j.result[0].results;
}

/// Every client closed, so the process may end.
export const close = () => Promise.all([...clients.values()].map((c) => c.end())).then(() => clients.clear());
