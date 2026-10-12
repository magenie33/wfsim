// THE SHADOW DATABASE (scripts/pg/shadow.mjs): `node:sqlite`'s DatabaseSync,
// every statement repeated in order on PGlite and the worker's compared.
import * as real from "node:sqlite";
import { readFileSync } from "node:fs";
import { PGlite } from "./node_modules/@electric-sql/pglite/dist/index.js";
import { placeholders } from "../../worker/factory_db.js";

export * from "node:sqlite";

const PG_DIR = new URL("../../worker/pg/", import.meta.url);
const pgSchema = (kind) => (kind === "accounts" ? ["accounts.sql"] : ["schema.sql", "library.sql"])
  .map((f) => readFileSync(new URL(f, PG_DIR), "utf8")).join("\n");
const tablesOf = (kind) => new RegExp(`CREATE TABLE (IF NOT EXISTS )?(${[...pgSchema(kind)
  .matchAll(/CREATE TABLE IF NOT EXISTS ([a-z_]+)/g)].map((m) => m[1]).join("|")})\\b`);
const TABLES = { accounts: tablesOf("accounts"), library: tablesOf("library") };

const found = [];
let judged = 0, repeated = 0;
/// ONE POSTGRES FOR EVERY DATABASE A CHECK OPENS, each in its own schema: a
/// PGlite each ran a check that opens hundreds of databases out of memory.
const pg = new PGlite({ parsers: { 20: (x) => Number(x), 1700: (x) => Number(x) } });
let chain = pg.waitReady, current = null, opened = 0;

/// WHO ISSUED A STATEMENT: the worker's are judged, a check's own are only repeated.
/// `WFSIM_SHADOW_WORKER` names another worker's source folder to judge, by a
/// regular expression over the caller's path — a worker beside this one.
const OTHER_WORKER = process.env.WFSIM_SHADOW_WORKER ? new RegExp(process.env.WFSIM_SHADOW_WORKER) : null;
const byWorker = () => {
  const stack = new Error().stack.split("\n").slice(3).join("\n");
  return /[\\/]worker[\\/](?!pg[\\/])[a-z_]+\.js/.test(stack) || !!(OTHER_WORKER && OTHER_WORKER.test(stack));
};

/// A CHECK'S OWN SQL, in SQLite's words, made Postgres's where it is plain to.
function lenient(sql) {
  let s = sql.trim().replace(/;\s*$/, "");
  if (/^INSERT\s+OR\s+IGNORE\s+INTO/i.test(s)) {
    s = s.replace(/^INSERT\s+OR\s+IGNORE\s+INTO/i, "INSERT INTO");
    s = /\bRETURNING\b/i.test(s) ? s.replace(/\bRETURNING\b/i, "ON CONFLICT DO NOTHING RETURNING") : `${s} ON CONFLICT DO NOTHING`;
  }
  return s.replace(/^INSERT\s+OR\s+REPLACE\s+INTO/i, "INSERT INTO");
}

/// WHAT ONLY SQLITE HAS — a pragma, its catalogue — is a check's look at SQLite, not repeated.
const sqliteOnly = (sql) => /^\s*PRAGMA\b|\bsqlite_master\b/i.test(sql);

const canonValue = (v) => (typeof v === "bigint" ? Number(v) : v instanceof Uint8Array ? Buffer.from(v).toString("hex") : v);
// A LEASE IS DRAWN AT RANDOM in each database, and only its being there compares.
const drawn = (k, v) => (/^lease(_prev)?$/.test(k) && typeof v === "string" && /^[0-9a-f]{32}$/.test(v) ? "<drawn>" : v);
// AN UNNAMED COLUMN is SQLite's text of it and Postgres's `?column?`.
const named = (k) => (/^\d+$/.test(k) ? "?column?" : k);
const canonRow = (r) => (r == null ? null : JSON.stringify(Object.keys(r).map((k) => [named(k), drawn(k, canonValue(r[k]))]).sort()));
function same(sql, a, b) {
  const rows = (x) => (Array.isArray(x) ? x : x == null ? [] : [x]).map(canonRow);
  const ra = rows(a), rb = rows(b);
  // RETURNING's rows come in no stated order, whatever its subqueries order by.
  if (!/^\s*(SELECT|WITH)\b/i.test(sql) || !/\border\s+by\b/i.test(sql)) { ra.sort(); rb.sort(); }
  return JSON.stringify(ra) === JSON.stringify(rb);
}

class ShadowDatabase extends real.DatabaseSync {
  constructor(...a) {
    super(...a);
    this.kinds = new Set();
    this.pg = pg;
    this.schema = `s${++opened}`;
    const schema = this.schema;
    chain = chain.then(() => pg.exec(`CREATE SCHEMA ${schema}`));
  }
  /// One Postgres step after every one before it, in this database's schema.
  later(fn) {
    const step = async () => {
      if (current !== this.schema) { await pg.exec(`SET search_path TO ${this.schema}`); current = this.schema; }
      return fn();
    };
    chain = chain.then(step, step);
  }
  close() {
    super.close();
    const schema = this.schema;
    this.later(() => pg.exec(`DROP SCHEMA ${schema} CASCADE`));
  }
  exec(sql) {
    const text = String(sql);
    super.exec(text);
    // A DATABASE IS THE ACCOUNTS', THE LIBRARY'S OR BOTH by the worker schema
    // laid in it, and its Postgres twin is laid with worker/pg's.
    // Any table of either: a worker beside this one lays its own tables first.
    const kinds = [["accounts", TABLES.accounts], ["library", TABLES.library]]
      .filter(([k, re]) => re.test(text) && !this.kinds.has(k)).map(([k]) => k);
    if (kinds.length) {
      for (const k of kinds) { this.kinds.add(k); this.later(() => this.pg.exec(pgSchema(k))); }
      return;
    }
    if (sqliteOnly(text)) return;
    const worker = byWorker();
    this.later(async () => {
      try { await this.pg.exec(worker ? globalThis.__wfsimPgText(text) : lenient(text)); } catch (e) {
        // A TABLE A CHECK MAKES BESIDE THE SCHEMA may be one worker/pg's already has.
        if (["42P07", "42701", "42710"].includes(e.code)) return;
        found.push({ what: worker ? "exec failed on Postgres" : "the check's own exec failed on Postgres", sql: text.replace(/\s+/g, " ").slice(0, 400), err: e.message });
      }
    });
  }
  prepare(sql) {
    const lite = String(sql), pgText = globalThis.__wfsimPgText(sql);
    const st = super.prepare(lite), db = this;
    if (sqliteOnly(lite)) return st;
    const run = (how) => (...args) => {
      const worker = byWorker();
      let out, threw = null;
      try { out = st[how](...args); } catch (e) { threw = e; }
      const pgSql = worker ? pgText : lenient(pgText);
      db.later(async () => {
        repeated++;
        let r = null, err = null;
        try { r = await db.pg.query(placeholders(pgSql), args.map(canonValue)); } catch (e) { err = e; }
        // A CHECK'S OWN STATEMENT that Postgres refuses leaves the two apart, and
        // every difference after it is that one's: it is said, so the check is mended.
        if (!worker) {
          if (!threw && err && err.code !== "25P02") found.push({ what: "the check's own statement failed on Postgres", sql: pgSql.replace(/\s+/g, " ").slice(0, 400), err: err.message });
          return;
        }
        judged++;
        const where = { sql: pgSql.replace(/\s+/g, " ").slice(0, 400), args: JSON.stringify(args).slice(0, 300) };
        if (threw && !err) found.push({ what: "SQLite failed, Postgres did not", ...where, err: threw.message });
        else if (!threw && err && err.code !== "25P02") found.push({ what: "Postgres failed", ...where, err: err.message });
        else if (!threw && !err) {
          if (how === "run") {
            if (Number(out.changes) !== (r.affectedRows ?? 0)) found.push({ what: `changed ${out.changes} rows on SQLite, ${r.affectedRows} on Postgres`, ...where });
          } else if (!same(lite, how === "get" ? out : out, how === "get" ? r.rows[0] : r.rows)) {
            found.push({ what: "rows differ", ...where, sqlite: JSON.stringify(out).slice(0, 300), pg: JSON.stringify(how === "get" ? r.rows[0] : r.rows).slice(0, 300) });
          }
        }
      });
      if (threw) throw threw;
      return out;
    };
    return new Proxy(st, {
      get(t, k) {
        if (k === "run" || k === "get" || k === "all") return run(k);
        const v = t[k];
        return typeof v === "function" ? v.bind(t) : v;
      },
    });
  }
}
export { ShadowDatabase as DatabaseSync };

globalThis.__wfsimShadow = async () => {
  await chain.catch(() => {});
  // EACH DIFFERENCE ONCE, by what it is and where.
  const seen = new Map();
  for (const f of found) {
    const k = `${f.what}|${f.sql}`;
    if (!seen.has(k)) seen.set(k, { ...f, n: 0 });
    seen.get(k).n++;
  }
  console.log(`\nshadow: ${judged} worker statement(s) repeated on Postgres (${repeated} in all), ${seen.size} difference(s)`);
  for (const f of [...seen.values()].slice(0, 40)) {
    console.log(`  PG   ${f.what}${f.n > 1 ? ` (x${f.n})` : ""}\n       ${f.sql}\n       args ${f.args || ""}${f.err ? `\n       ${f.err}` : ""}${f.sqlite ? `\n       sqlite ${f.sqlite}\n       pg     ${f.pg}` : ""}`);
  }
  return seen.size ? 1 : 0;
};
