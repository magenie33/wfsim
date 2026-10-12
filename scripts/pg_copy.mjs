// D1'S TABLES, COPIED TO POSTGRES — docs/BOARD.md §"Postgres". D1 is read
// through its query API a page at a time by primary key — `wrangler d1 export`
// would make the database unavailable while it ran — and each page is upserted,
// so a second run brings a first one up to date; `fresh` empties the tables
// first, which is what a cutover runs. Then each table is compared: its rows,
// keys and sums, or with `verify` every row, value by value.
//
//   CF_ACCOUNT=… CF_TOKEN=… CF_D1_DATABASE=… PG_URL=postgres://…/wfsim \
//     node scripts/pg_copy.mjs [copy|fresh|compare|verify] [factory|library|table…]
//   CF_ACCOUNT=… CF_TOKEN=… CF_D1_ACCOUNTS=… PG_ACCOUNTS_URL=postgres://…/wfsim_accounts \
//     node scripts/pg_copy.mjs [copy|fresh|compare|verify] accounts
//
// Needs `npm install --prefix scripts/pg` (postgres.js) and worker/pg's DDL
// applied: schema.sql and library.sql to `wfsim`, accounts.sql to
// `wfsim_accounts`. Run on the server, PG_URL on localhost.
import postgres from "./pg/node_modules/postgres/src/index.js";

/// EVERY TABLE AND ITS KEY, by the database it is in. A table with no key is
/// emptied and copied whole, in D1's rowid order.
const GROUPS = {
  factory: { d1: "CF_D1_DATABASE", pg: "PG_URL", tables: {
    verifiers: ["id"], verifier_hours: ["verifier", "hour"], questions: ["id"], demands: ["producer", "ref"],
    answers: ["id"], watchdog_told: ["cause"], points_pool: null } },
  library: { d1: "CF_D1_DATABASE", pg: "PG_URL", tables: {
    inbox: ["id"], builds: ["id"], scores: ["identity", "ruler", "mode"], batches: ["id"],
    queue: ["batch", "build_id", "ruler", "mode"], orders: ["identity", "ruler", "mode"], shares: ["id"],
    usage_days: ["day", "event", "subject", "market"], bot_inbox: ["id"], appraisals: ["code"],
    appraisal_results: ["id"], fissures: ["id"], donors: ["id"], rates: ["currency"], donations: ["id"] } },
  // `accounts` FIRST: every other table here names one, and is copied after it.
  accounts: { d1: "CF_D1_ACCOUNTS", pg: "PG_ACCOUNTS_URL", tables: {
    accounts: ["id"], identities: ["provider", "subject"], sessions: ["token_hash"], email_codes: ["email"],
    login_failures: ["email"], username_holds: ["username"], agent_keys: ["id"], devices: ["verifier"],
    device_spans: ["verifier", "from_hour"], contribution_choice: ["account"], account_titles: ["account", "title"],
    billing_attempts: null, billing_card_refunds: ["card"], billing_customers: ["account"],
    billing_endings: ["account", "channel", "ref"], billing_events: ["id"], billing_grants: ["id"], billing_orders: ["id"],
    billing_periods: ["invoice", "line"], billing_subscriptions: ["id"], billing_usage: ["id"],
    discord_grants: ["discord_id"], patron_wall: ["account"], share_signatures: ["id"], sync_devices: ["account", "id"],
    sync_entries: ["account", "id"] } },
};
/// WHAT A COMPARISON ADDS UP, beside the count.
const SUMS = { verifiers: "work", verifier_hours: "work", answers: "work", points_pool: "work", scores: "cost_seconds",
  appraisal_results: "id", usage_days: "visitors", device_spans: "length(verifier)" };
const PAGE = 2000, CHUNK = 500;

const [mode = "copy", ...only] = process.argv.slice(2);
// NAMED, NEVER DEFAULTED: the factory's tables have lived in Postgres since
// they moved, and copying D1's over them would put back what they replaced.
if (!only.length) { console.error("name what to copy: library, accounts, factory or tables"); process.exit(2); }
const wanted = only;
const groups = Object.entries(GROUPS).filter(([g, x]) => wanted.includes(g) || Object.keys(x.tables).some((t) => wanted.includes(t)));
if (groups.some(([g]) => g === "accounts") && groups.length > 1) { console.error("the accounts are copied alone: another database"); process.exit(2); }
const { d1: D1_ENV, pg: PG_ENV } = groups[0][1];
const names = groups.flatMap(([g, x]) => Object.keys(x.tables).filter((t) => wanted.includes(g) || wanted.includes(t)));
const keyOf = (t) => groups.map(([, x]) => x.tables).find((ts) => t in ts)[t];
for (const v of ["CF_ACCOUNT", "CF_TOKEN", D1_ENV, PG_ENV]) if (!process.env[v]) { console.error(`needs ${v}`); process.exit(2); }

async function d1(sql, params = []) {
  for (let tries = 0; ; tries++) {
    const r = await fetch(`https://api.cloudflare.com/client/v4/accounts/${process.env.CF_ACCOUNT}/d1/database/${process.env[D1_ENV]}/query`, {
      method: "POST", headers: { authorization: `Bearer ${process.env.CF_TOKEN}`, "content-type": "application/json" },
      body: JSON.stringify({ sql, params }), signal: AbortSignal.timeout(180_000) }).catch((e) => ({ ok: false, e }));
    const j = r.ok ? await r.json().catch(() => null) : null;
    if (j && j.success) return j.result[0].results;
    if (tries >= 4) throw new Error(`d1: ${JSON.stringify(j && j.errors) || r.status || r.e}`);
    await new Promise((res) => setTimeout(res, 2000 * 2 ** tries));
  }
}

// THREE CONNECTIONS, beside the worker's: the server allows a hundred and the
// live worker has been seen holding 85, so a wider copy starves the site.
const sql = postgres(process.env[PG_ENV], { max: Number(process.env.PG_COPY_CONNECTIONS) || 3, onnotice: () => {},
  types: { bigint: { to: 20, from: [20, 1700], serialize: (x) => String(x), parse: (x) => Number(x) } } });

/// A TABLE'S COLUMNS AS WRITTEN: a generated one (`builds.weapon`) is Postgres's to fill.
async function columns(table) {
  return sql.unsafe(`SELECT column_name AS name, data_type AS type, is_identity = 'YES' AS identity FROM information_schema.columns
    WHERE table_name = $1 AND table_schema = current_schema() AND is_generated = 'NEVER' ORDER BY ordinal_position`, [table]);
}

/// D1'S ROWS IN KEY ORDER, a page after `after` (the last row of the one before).
const page = (table, key, cols, after, offset) => d1(`SELECT ${cols.map((c) => `"${c.name}"`).join(", ")} FROM ${table}
  ${key && after ? `WHERE (${key.join(", ")}) > (${key.map(() => "?").join(", ")})` : ""}
  ORDER BY ${key ? key.join(", ") : "rowid"} LIMIT ${PAGE}${key ? "" : ` OFFSET ${offset}`}`, key && after ? key.map((k) => after[k]) : []);

/// ONE TABLE, page by page in key order, each page upserted in chunks.
async function copy(table) {
  const key = keyOf(table), cols = await columns(table);
  const def = cols.map((c) => `"${c.name}" ${c.type}`).join(", ");
  const list = cols.map((c) => `"${c.name}"`).join(", ");
  const rest = cols.filter((c) => !(key || []).includes(c.name));
  const upsert = `INSERT INTO ${table} (${list}) OVERRIDING SYSTEM VALUE SELECT ${list} FROM jsonb_to_recordset($1::text::jsonb) AS x(${def})${
    !key ? "" : rest.length ? ` ON CONFLICT (${key.join(", ")}) DO UPDATE SET ${rest.map((c) => `"${c.name}" = excluded."${c.name}"`).join(", ")}`
      : ` ON CONFLICT (${key.join(", ")}) DO NOTHING`}`;
  if (!key) await sql.unsafe(`TRUNCATE ${table}`);
  // The next page is read while this one is written, and its chunks are written
  // at once: they are disjoint rows, so no order between them matters.
  let total = 0, next = page(table, key, cols, null, 0);
  for (;;) {
    const rows = await next;
    if (rows.length === PAGE) next = page(table, key, cols, rows[rows.length - 1], total + PAGE);
    const chunks = [];
    for (let i = 0; i < rows.length; i += CHUNK) chunks.push(sql.unsafe(upsert, [JSON.stringify(rows.slice(i, i + CHUNK))]));
    await Promise.all(chunks);
    total += rows.length;
    if (rows.length < PAGE) break;
    if (total % 20_000 === 0) console.log(`  ${table}: ${total}`);
  }
  // AN IDENTITY GOES ON FROM THE LAST ID COPIED, or the next insert collides.
  for (const c of cols.filter((x) => x.identity)) {
    await sql.unsafe(`SELECT setval(pg_get_serial_sequence('${table}', '${c.name}'), GREATEST((SELECT max("${c.name}") FROM ${table}), 1))`);
  }
  console.log(`copied ${table}: ${total} row(s)`);
}

/// ROWS, KEYS AND SUMS ALIKE in both.
async function compare(table) {
  const key = keyOf(table), sum = SUMS[table];
  // The sums pass 2^53, so each side states its own as text.
  const q = (text) => `SELECT count(*) AS n${sum ? `, ${text(`coalesce(sum(${sum}), 0)`)} AS s` : ""}${key ? `, min(${key[0]}) AS lo, max(${key[0]}) AS hi` : ""} FROM ${table}`;
  const [a] = await d1(q((x) => `CAST(${x} AS TEXT)`)), [b] = await sql.unsafe(q((x) => `${x}::text`));
  const same = ["n", "s", "lo", "hi"].every((k) => a[k] === undefined || String(a[k]) === String(b[k]));
  // COUNTS AND SUMS SAID, NEVER A KEY: an account's key may be an address.
  const said = (x) => JSON.stringify({ n: x.n, ...(sum ? { s: x.s } : {}) });
  console.log(`${same ? "  ok " : "FAIL"}  ${table}: d1 ${said(a)} pg ${said(b)}${same ? "" : " (or its first or last key)"}`);
  return same;
}

/// EVERY ROW, VALUE BY VALUE, both read in key order a page at a time. A value
/// is compared as its text: D1 answers a REAL that is whole as an integer.
async function verify(table) {
  const key = keyOf(table), cols = await columns(table);
  if (!key) return compare(table);
  const text = (r) => JSON.stringify(cols.map((c) => (r[c.name] == null ? null : String(r[c.name]))));
  const list = cols.map((c) => `"${c.name}"`).join(", ");
  let after = null, n = 0, bad = 0;
  for (;;) {
    const a = await page(table, key, cols, after, 0);
    const b = await sql.unsafe(`SELECT ${list} FROM ${table} ${after ? `WHERE (${key.join(", ")}) > (${key.map((_, i) => `$${i + 1}`).join(", ")})` : ""}
      ORDER BY ${key.join(", ")} LIMIT ${PAGE}`, after ? key.map((k) => after[k]) : []);
    for (let i = 0; i < Math.max(a.length, b.length); i++) {
      if (!a[i] || !b[i] || text(a[i]) !== text(b[i])) {
        // WHICH COLUMNS, never their values: a row may be somebody's.
        if (bad++ < 3) console.log(`      ${table}: ${!a[i] || !b[i] ? "a row in one only" : `differs in ${cols.filter((c) => String(a[i][c.name]) !== String(b[i][c.name])).map((c) => c.name).join(", ")}`}`);
      }
    }
    n += a.length;
    if (a.length < PAGE) break;
    after = a[a.length - 1];
  }
  console.log(`${bad ? "FAIL" : "  ok "}  ${table}: ${n} row(s)${bad ? `, ${bad} differ` : " alike"}`);
  return !bad;
}

let bad = 0;
try {
  if (mode === "fresh") await sql.unsafe(`TRUNCATE ${names.join(", ")}`);
  if (mode === "copy" || mode === "fresh") {
    // THE ACCOUNTS BEFORE WHAT NAMES THEM; the rest at once.
    const first = names.filter((t) => t === "accounts");
    for (const t of first) await copy(t);
    await Promise.all(names.filter((t) => !first.includes(t)).map(copy));
  }
  for (const t of names) if (!(await (mode === "verify" ? verify(t) : compare(t)))) bad++;
} finally {
  await sql.end({ timeout: 5 });
}
console.log(bad ? `\n${bad} table(s) differ` : `\nalike in D1 and Postgres: ${names.join(", ")}`);
process.exitCode = bad ? 1 : 0;
