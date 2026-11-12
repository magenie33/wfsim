// THE DATABASE A NODE CHECK RUNS THE WORKER ON: an in-memory Postgres (PGlite)
// laid with worker/pg/*.sql, behind the same statement API the worker gets
// from Hyperdrive (`pgDb`, worker/db.js). Every database a check opens is a
// schema of ONE PGlite — a PGlite each ran a check that opens hundreds out of
// memory — and every step runs in turn, in its own schema.
//   npm install --prefix scripts/pg
//   const db = await testDb("library"), env = { LIBRARY: db };
import { readFileSync, writeFileSync } from "node:fs";
import { PGlite } from "./node_modules/@electric-sql/pglite/dist/index.js";
import { pgDb, placeholders } from "../../worker/db.js";

const PG_DIR = new URL("../../worker/pg/", import.meta.url);
const DDL = { library: ["schema.sql", "library.sql", "sync.sql", "inventory.sql", "build_workflows.sql"], accounts: ["accounts.sql"] };
const ddl = (kind) => DDL[kind].map((f) => readFileSync(new URL(f, PG_DIR), "utf8")).join("\n");
const profile = process.env.WFSIM_TEST_DB_TIMING === "1" || !!process.env.WFSIM_TEST_DB_TIMING_FILE;
const phases = Object.fromEntries(["startup", "schema", "ddl", "search_path", "query", "exec", "transaction"].map(key => [key, { calls: 0, ms: 0 }]));
const timed = profile ? async (phase, fn) => {
  const start = performance.now();
  try { return await fn(); }
  finally { phases[phase].calls++; phases[phase].ms += performance.now() - start; }
} : (_, fn) => fn();
export const testDbTiming = () => profile ? structuredClone(phases) : null;
if (process.env.WFSIM_TEST_DB_TIMING_FILE) process.on("exit", () => {
  writeFileSync(process.env.WFSIM_TEST_DB_TIMING_FILE, JSON.stringify({ phases, schemas: opened,
    transaction_timing_includes_queries: true }, null, 2) + "\n");
});

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
const createPg = () => new PGlite({ parsers: { 20: (x) => Number(x), 1700: (x) => Number(x) } });
let pg = createPg(), chain = timed("startup", () => pg.waitReady), current = null, opened = 0, epochSchemas = 0;
const liveSchemas = new Set();

/// ONE STEP AFTER EVERY ONE BEFORE IT, in `schema`.
function step(schema, fn) {
  const run = async () => {
    if (schema && current !== schema) { await timed("search_path", () => pg.exec(`SET search_path TO ${schema}`)); current = schema; }
    return fn();
  };
  const out = chain.then(run, run);
  chain = out.catch(() => {});
  return out;
}

const rowsOf = (r) => ({ rows: r.rows, affected: r.affectedRows ?? 0 });
// A PARAMETER IS NEVER CAST STRAIGHT TO JSONB: Hyperdrive's driver sends the
// bound string as a JSON string, so every field reads NULL in production while
// PGlite reads the object. Bind text and cast it: `(?::text)::jsonb`.
const textOnly = (text) => {
  if (/\$\d+::jsonb/.test(text)) throw new Error(`test_db: a parameter cast straight to jsonb, write (?::text)::jsonb: ${text.slice(0, 120)}`);
  return text;
};

/// A NEW, EMPTY DATABASE laid with the tables of each of `kinds` ("library",
/// "accounts"). Beside the worker's API it answers a check's own SQL directly:
/// `exec` (any statements), `get`/`all` (rows) and `run` (rows changed).
export async function testDb(...kinds) {
  const schema = `s${++opened}`;
  await step(null, () => timed("schema", () => pg.exec(`CREATE SCHEMA ${schema}`)));
  liveSchemas.add(schema);epochSchemas++;
  await step(schema, () => timed("ddl", () => pg.exec(kinds.map(ddl).join("\n"))));
  const client = {
    query: (text, params) => step(schema, async () => rowsOf(await timed("query", () => pg.query(textOnly(text), params)))),
    transaction: (fn) => step(schema, () => timed("transaction", () => pg.transaction((tx) => fn({
      query: async (text, params) => rowsOf(await timed("query", () => tx.query(textOnly(text), params))),
    })))),
  };
  const db = pgDb(client);
  const q = (sql, args) => client.query(placeholders(sql), args);
  // WFSIM_SETTLE_ASYNC=1 RUNS A CHECK WITH ITS ANSWERS QUEUED (dial
  // `settle_async`), each settled by the settler's pass at once
  // (worker/tasks.js `recordMany`), so every check holds on both paths.
  if (process.env.WFSIM_SETTLE_ASYNC === "1" && kinds.includes("library")) {
    await q("INSERT INTO state_factory_parameters (name, value, set_by, set_at) VALUES ('settle_async', 1, 'check', '2026-01-01T00:00:00.000Z') ON CONFLICT DO NOTHING", []);
    db.settleAtOnce = true;
  }
  // WFSIM_SYNC_PROC=1 RUNS A CHECK WITH EACH PROTOCOL 8 SYNC ONE CALL (dial
  // `sync_proc`, worker/sync.js), so it holds on both paths.
  if (process.env.WFSIM_SYNC_PROC === "1" && kinds.includes("library")) {
    await q("INSERT INTO state_factory_parameters (name, value, set_by, set_at) VALUES ('sync_proc', 1, 'check', '2026-01-01T00:00:00.000Z') ON CONFLICT DO NOTHING", []);
  }
  return Object.assign(db, {
    exec: (sql) => step(schema, () => timed("exec", () => pg.exec(sql))),
    get: async (sql, ...args) => (await q(sql, args)).rows[0] ?? null,
    all: async (sql, ...args) => (await q(sql, args)).rows,
    run: async (sql, ...args) => (await q(sql, args)).affected,
    close: () => step(null, async () => {
      await timed("schema", () => pg.exec(`DROP SCHEMA ${schema} CASCADE`));liveSchemas.delete(schema);
      // PostgreSQL catalog caches outlive dropped schemas inside one WASM VM.
      // Rotate only when every database has closed, keeping large books bounded.
      if(!liveSchemas.size && epochSchemas>=64) {
        await pg.close();pg=createPg();await timed("startup",()=>pg.waitReady);current=null;epochSchemas=0;
      }
    }),
  });
}
