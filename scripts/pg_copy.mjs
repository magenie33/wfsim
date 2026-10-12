// THE FACT FACTORY'S TABLES, COPIED FROM D1 TO POSTGRES (worker/factory_db.js,
// docs/BOARD.md §"The fact factory"). D1 is read through its query API a page
// at a time by primary key — `wrangler d1 export` would make the database
// unavailable while it ran — and each page is upserted, so a second run brings
// a first one up to date. Then each table's rows, keys and work are compared.
//
//   CF_ACCOUNT=… CF_TOKEN=… CF_D1_DATABASE=… PG_URL=postgres://… \
//     node scripts/pg_copy.mjs [copy|compare] [table…]
//
// Needs `npm install --prefix scripts/pg` (postgres.js) and worker/pg/schema.sql
// applied. PG_URL reaches the server through a tunnel (`ssh -L 15432:localhost:5432`).
import postgres from "./pg/node_modules/postgres/src/index.js";

const TABLES = {
  verifiers: ["id"],
  verifier_hours: ["verifier", "hour"],
  questions: ["id"],
  demands: ["producer", "ref"],
  answers: ["id"],
  watchdog_told: ["cause"],
  points_pool: null,
};
/// WHAT A COMPARISON ADDS UP, beside the count.
const SUMS = { verifiers: "work", verifier_hours: "work", answers: "work", points_pool: "work" };
const PAGE = 2000, CHUNK = 500;

const [mode = "copy", ...only] = process.argv.slice(2);
const names = only.length ? only : Object.keys(TABLES);
for (const v of ["CF_ACCOUNT", "CF_TOKEN", "CF_D1_DATABASE", "PG_URL"]) if (!process.env[v]) { console.error(`needs ${v}`); process.exit(2); }

async function d1(sql, params = []) {
  for (let tries = 0; ; tries++) {
    const r = await fetch(`https://api.cloudflare.com/client/v4/accounts/${process.env.CF_ACCOUNT}/d1/database/${process.env.CF_D1_DATABASE}/query`, {
      method: "POST", headers: { authorization: `Bearer ${process.env.CF_TOKEN}`, "content-type": "application/json" },
      body: JSON.stringify({ sql, params }), signal: AbortSignal.timeout(180_000) }).catch((e) => ({ ok: false, e }));
    const j = r.ok ? await r.json().catch(() => null) : null;
    if (j && j.success) return j.result[0].results;
    if (tries >= 4) throw new Error(`d1: ${JSON.stringify(j && j.errors) || r.status || r.e}`);
    await new Promise((res) => setTimeout(res, 2000 * 2 ** tries));
  }
}

const sql = postgres(process.env.PG_URL, { max: 12, onnotice: () => {},
  types: { bigint: { to: 20, from: [20, 1700], serialize: (x) => String(x), parse: (x) => Number(x) } } });

async function columns(table) {
  return sql.unsafe(`SELECT column_name AS name, data_type AS type FROM information_schema.columns
    WHERE table_name = $1 AND table_schema = current_schema() ORDER BY ordinal_position`, [table]);
}

/// ONE TABLE, page by page in key order, each page upserted in chunks.
async function copy(table) {
  const key = TABLES[table], cols = await columns(table);
  const def = cols.map((c) => `"${c.name}" ${c.type}`).join(", ");
  const list = cols.map((c) => `"${c.name}"`).join(", ");
  const upsert = key
    ? `INSERT INTO ${table} (${list}) OVERRIDING SYSTEM VALUE SELECT ${list} FROM jsonb_to_recordset($1::text::jsonb) AS x(${def})
         ON CONFLICT (${key.join(", ")}) DO UPDATE SET ${cols.filter((c) => !key.includes(c.name)).map((c) => `"${c.name}" = excluded."${c.name}"`).join(", ")}`
    : `INSERT INTO ${table} (${list}) SELECT ${list} FROM jsonb_to_recordset($1::text::jsonb) AS x(${def})`;
  if (!key) await sql.unsafe(`TRUNCATE ${table}`);
  const page = (after, offset) => d1(`SELECT * FROM ${table} ${!key || !after ? "" : key.length === 1 ? `WHERE ${key[0]} > ?1`
    : `WHERE ${key[0]} > ?1 OR (${key[0]} = ?1 AND ${key[1]} > ?2)`} ${key ? `ORDER BY ${key.join(", ")}` : ""} LIMIT ${PAGE}${key ? "" : ` OFFSET ${offset}`}`,
  !key || !after ? [] : key.map((k) => after[k]));
  // The next page is read while this one is written, and its chunks are written
  // at once: they are disjoint rows, so no order between them matters.
  let total = 0, next = page(null, 0);
  for (;;) {
    const rows = await next;
    if (rows.length === PAGE) next = page(rows[rows.length - 1], total + PAGE);
    const chunks = [];
    for (let i = 0; i < rows.length; i += CHUNK) chunks.push(sql.unsafe(upsert, [JSON.stringify(rows.slice(i, i + CHUNK))]));
    await Promise.all(chunks);
    total += rows.length;
    if (rows.length < PAGE) break;
    if (total % 20_000 === 0) console.log(`  ${table}: ${total}`);
  }
  if (table === "answers") await sql.unsafe("SELECT setval(pg_get_serial_sequence('answers', 'id'), GREATEST((SELECT max(id) FROM answers), 1))");
  console.log(`copied ${table}: ${total} row(s)`);
}

/// ROWS, KEYS AND WORK ALIKE in both.
async function compare(table) {
  const key = TABLES[table], sum = SUMS[table];
  // The sums pass 2^53, so each side states its own as text.
  const q = (text) => `SELECT count(*) AS n${sum ? `, ${text(`coalesce(sum(${sum}), 0)`)} AS s` : ""}${key ? `, min(${key[0]}) AS lo, max(${key[0]}) AS hi` : ""} FROM ${table}`;
  const [a] = await d1(q((x) => `CAST(${x} AS TEXT)`)), [b] = await sql.unsafe(q((x) => `${x}::text`));
  const same = ["n", "s", "lo", "hi"].every((k) => a[k] === undefined || String(a[k]) === String(b[k]));
  console.log(`${same ? "  ok " : "FAIL"}  ${table}: d1 ${JSON.stringify(a)} pg ${JSON.stringify(b)}`);
  return same;
}

let bad = 0;
try {
  if (mode === "copy") await Promise.all(names.map(copy));
  for (const t of names) if (!(await compare(t))) bad++;
} finally {
  await sql.end({ timeout: 5 });
}
console.log(bad ? `\n${bad} table(s) differ` : "\nthe factory's tables are alike in D1 and Postgres");
process.exitCode = bad ? 1 : 0;
