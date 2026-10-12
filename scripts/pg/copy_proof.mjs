// The copy into the server's scratch databases (never `wfsim` or
// `wfsim_accounts`), through the ssh tunnel on localhost:15432: worker/pg's DDL
// laid fresh, then scripts/pg_copy.mjs. Secrets read, never printed.
//   node scripts/pg/copy_proof.mjs <pg.env> <cloudflare-d1.env> <mode> <group|table…>
// PG_DB names the scratch database (wfsim_scratch_all), PG_ACCOUNTS_DB the
// accounts' (wfsim_scratch_all_accounts); CF_D1_ACCOUNTS the accounts' D1.
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import postgres from "./node_modules/postgres/src/index.js";
const read = (f) => Object.fromEntries(readFileSync(f, "utf8").split(/\r?\n/).filter((l) => l.includes("="))
  .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim()]));
const pg = read(process.argv[2]), cf = read(process.argv[3]);
const url = (db) => `postgres://${pg.PG_USER}:${encodeURIComponent(pg.PG_PASSWORD)}@localhost:15432/${db}`;
const library = process.env.PG_DB || "wfsim_scratch_all", accounts = process.env.PG_ACCOUNTS_DB || "wfsim_scratch_all_accounts";
if (/^wfsim(_accounts)?$/.test(library) || /^wfsim(_accounts)?$/.test(accounts)) { console.error("a scratch database, never the live one"); process.exit(2); }
const ddl = (f) => readFileSync(`worker/pg/${f}`, "utf8");
const accountsRun = process.argv.slice(5).includes("accounts");
const sql = postgres(url(accountsRun ? accounts : library), { onnotice: () => {} });
// THE DDL LAID FRESH ONLY FOR A COPY: a comparison reads what the last one left.
if (["copy", "fresh"].includes(process.argv[4])) {
  await sql.unsafe("DROP SCHEMA public CASCADE; CREATE SCHEMA public");
  await sql.unsafe(accountsRun ? ddl("accounts.sql") : `${ddl("schema.sql")}\n${ddl("library.sql")}`);
}
await sql.end();
const t0 = Date.now();
const r = spawnSync(process.execPath, ["scripts/pg_copy.mjs", ...process.argv.slice(4)], { encoding: "utf8",
  env: { ...process.env, PG_URL: url(library), PG_ACCOUNTS_URL: url(accounts), CF_ACCOUNT: cf.CF_ACCOUNT, CF_TOKEN: cf.CF_TOKEN,
    CF_D1_DATABASE: cf.CF_D1_DATABASE, CF_D1_ACCOUNTS: process.env.CF_D1_ACCOUNTS || "" } });
const hide = (s) => (s || "").replaceAll(pg.PG_PASSWORD, "<hidden>").replaceAll(cf.CF_TOKEN, "<hidden>");
process.stdout.write(hide(r.stdout));
process.stderr.write(hide(r.stderr));
console.log(`took ${((Date.now() - t0) / 60000).toFixed(1)} min`);
process.exitCode = r.status;
