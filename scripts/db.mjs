// SPDX-License-Identifier: AGPL-3.0-or-later
// THE DATABASES AS THE SCRIPTS REACH THEM — docs/BOARD.md §"Postgres". One
// statement in D1's words (`?` or `?N` bound, never interpolated), its rows out:
// D1's query API, or Postgres when the database's URL is set — `PG_URL` for the
// library, `PG_ACCOUNTS_URL` for the accounts. The switch is that one value, so
// a rollback is unsetting it. `pglite:<dir>` is an on-disk PGlite, for checks.
//
//   import { query, answer } from "./db.mjs";
//   const rows = await query("SELECT id FROM builds WHERE at > ?", [since]);
import { placeholders } from "../worker/placeholders.js";

const DATABASES = {
  library: { url: "PG_URL", d1: "CF_D1_DATABASE" },
  accounts: { url: "PG_ACCOUNTS_URL", d1: "CF_D1_ACCOUNTS" },
};
/// A STATEMENT HELD PAST THIS IS ENDED, as a D1 call is by its clock: a hung
/// one holds a workflow until GitHub's six-hour cap (scripts/fetch_queue.sh).
const STATEMENT_MS = 180_000;

export const onPostgres = (database = "library") => !!process.env[DATABASES[database].url];
export const configured = (database = "library") => onPostgres(database)
  || !!(process.env.CF_ACCOUNT && process.env.CF_TOKEN && process.env[DATABASES[database].d1]);

const clients = new Map();
async function client(database) {
  if (clients.has(database)) return clients.get(database);
  const url = process.env[DATABASES[database].url];
  let c;
  if (url.startsWith("pglite:")) {
    const { PGlite } = await import("./pg/node_modules/@electric-sql/pglite/dist/index.js");
    const pg = new PGlite(url.slice("pglite:".length), { parsers: { 20: (x) => Number(x), 1700: (x) => Number(x) } });
    c = { run: async (text, params) => { const r = await pg.query(text, params); return { rows: r.rows, changes: r.affectedRows ?? 0 }; },
      end: () => pg.close() };
  } else {
    const { default: postgres } = await import("./pg/node_modules/postgres/src/index.js");
    // A COUNT OR A SUM IS A NUMBER, as D1 answers it (worker/factory_db.js).
    const sql = postgres(url, { max: 4, onnotice: () => {}, connect_timeout: 15, idle_timeout: 5, fetch_types: false,
      connection: { statement_timeout: STATEMENT_MS, application_name: "wfsim-scripts" },
      types: { bigint: { to: 20, from: [20, 1700], serialize: (x) => String(x), parse: (x) => Number(x) } } });
    c = { run: async (text, params) => { const r = await sql.unsafe(text, params); return { rows: [...r], changes: r.count ?? 0 }; },
      end: () => sql.end({ timeout: 5 }) };
  }
  clients.set(database, c);
  return c;
}

/// D1'S ANSWER TO ONE STATEMENT — `{ success, result: [{ results, meta: { changes } }] }`
/// — from whichever database holds it; a refusal is `{ success: false, errors }`.
export async function answer(sql, params = [], database = "library") {
  if (onPostgres(database)) {
    try {
      const r = await (await client(database)).run(placeholders(sql), params);
      return { success: true, result: [{ results: r.rows, success: true, meta: { changes: r.changes } }] };
    } catch (e) {
      return { success: false, errors: [{ code: e.code || "", message: e.message }] };
    }
  }
  const { CF_ACCOUNT, CF_TOKEN } = process.env, id = process.env[DATABASES[database].d1];
  const r = await fetch(`https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT}/d1/database/${id}/query`, {
    method: "POST", headers: { authorization: `Bearer ${CF_TOKEN}`, "content-type": "application/json" },
    body: JSON.stringify({ sql, params }), signal: AbortSignal.timeout(STATEMENT_MS) }).catch((e) => ({ ok: false, status: 0, e }));
  const j = r.json ? await r.json().catch(() => null) : null;
  return j || { success: false, errors: [{ code: r.status, message: String(r.e || `HTTP ${r.status}`) }] };
}

/// ITS ROWS, or a thrown refusal.
export async function query(sql, params = [], database = "library") {
  const j = await answer(sql, params, database);
  if (!j.success) throw new Error(`${onPostgres(database) ? "postgres" : "d1"}: ${JSON.stringify(j.errors)}`);
  return j.result[0].results;
}

/// Every client closed, so the process may end.
export const close = () => Promise.all([...clients.values()].map((c) => c.end())).then(() => clients.clear());
