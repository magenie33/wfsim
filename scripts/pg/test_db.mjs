// THE DATABASE A NODE CHECK RUNS THE WORKER ON: an in-memory Postgres (PGlite)
// laid with worker/pg/*.sql, behind the same statement API the worker gets
// from Hyperdrive (`pgDb`, worker/db.js). Every database a check opens is a
// schema of ONE PGlite — a PGlite each ran a check that opens hundreds out of
// memory — and every step runs in turn, in its own schema.
//   npm install --prefix scripts/pg
//   const db = await testDb("library"), env = { LIBRARY: db };
import { readFileSync } from "node:fs";
import { PGlite } from "./node_modules/@electric-sql/pglite/dist/index.js";
import { pgDb, placeholders } from "../../worker/db.js";

const PG_DIR = new URL("../../worker/pg/", import.meta.url);
const DDL = { library: ["schema.sql", "library.sql"], accounts: ["accounts.sql"] };
const ddl = (kind) => DDL[kind].map((f) => readFileSync(new URL(f, PG_DIR), "utf8")).join("\n");

// POSTGRES'S OWN TIMERS NEVER HOLD A CHECK OPEN: PGlite arms them as plain
// timers (ten seconds, or a day under a check's moved clock), so a check that
// had finished waited for them before it could exit.
const setTimer = globalThis.setTimeout;
globalThis.setTimeout = function (fn, ms, ...args) {
  const t = setTimer(fn, ms, ...args);
  if (ms >= 1000 && /pglite/.test(new Error().stack)) t.unref?.();
  return t;
};

// A COUNT OR A SUM IS A NUMBER, as POSTGRES_OPTIONS reads it on Hyperdrive.
const pg = new PGlite({ parsers: { 20: (x) => Number(x), 1700: (x) => Number(x) } });
let chain = pg.waitReady, current = null, opened = 0;

/// ONE STEP AFTER EVERY ONE BEFORE IT, in `schema`.
function step(schema, fn) {
  const run = async () => {
    if (schema && current !== schema) { await pg.exec(`SET search_path TO ${schema}`); current = schema; }
    return fn();
  };
  const out = chain.then(run, run);
  chain = out.catch(() => {});
  return out;
}

const rowsOf = (r) => ({ rows: r.rows, affected: r.affectedRows ?? 0 });

/// A NEW, EMPTY DATABASE laid with the tables of each of `kinds` ("library",
/// "accounts"). Beside the worker's API it answers a check's own SQL directly:
/// `exec` (any statements), `get`/`all` (rows) and `run` (rows changed).
export async function testDb(...kinds) {
  const schema = `s${++opened}`;
  await step(null, () => pg.exec(`CREATE SCHEMA ${schema}`));
  await step(schema, () => pg.exec(kinds.map(ddl).join("\n")));
  const client = {
    query: (text, params) => step(schema, async () => rowsOf(await pg.query(text, params))),
    transaction: (fn) => step(schema, () => pg.transaction((tx) => fn({ query: async (text, params) => rowsOf(await tx.query(text, params)) }))),
  };
  const db = pgDb(client);
  const q = (sql, args) => client.query(placeholders(sql), args);
  return Object.assign(db, {
    exec: (sql) => step(schema, () => pg.exec(sql)),
    get: async (sql, ...args) => (await q(sql, args)).rows[0] ?? null,
    all: async (sql, ...args) => (await q(sql, args)).rows,
    run: async (sql, ...args) => (await q(sql, args)).affected,
    close: () => step(null, () => pg.exec(`DROP SCHEMA ${schema} CASCADE`)),
  });
}
