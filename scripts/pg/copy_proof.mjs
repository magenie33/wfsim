// The copy into the server's scratch database (never `wfsim`), through the ssh
// tunnel on localhost:15432: schema fresh, then scripts/pg_copy.mjs. Secrets read, never printed.
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import postgres from "./node_modules/postgres/src/index.js";
const read = (f) => Object.fromEntries(readFileSync(f, "utf8").split(/\r?\n/).filter((l) => l.includes("="))
  .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim()]));
const pg = read(process.argv[2]), cf = read(process.argv[3]);
const url = `postgres://${pg.PG_USER}:${encodeURIComponent(pg.PG_PASSWORD)}@localhost:15432/${process.env.PG_DB || "wfsim_scratch"}`;
const sql = postgres(url, { onnotice: () => {} });
await sql.unsafe("DROP TABLE IF EXISTS questions, demands, answers, verifiers, verifier_hours, watchdog_told, points_pool");
await sql.unsafe(readFileSync("worker/pg/schema.sql", "utf8"));
await sql.end();
const t0 = Date.now();
const r = spawnSync(process.execPath, ["scripts/pg_copy.mjs", ...process.argv.slice(4)], { encoding: "utf8",
  env: { ...process.env, PG_URL: url, CF_ACCOUNT: cf.CF_ACCOUNT, CF_TOKEN: cf.CF_TOKEN, CF_D1_DATABASE: cf.CF_D1_DATABASE } });
const hide = (s) => (s || "").replaceAll(pg.PG_PASSWORD, "<hidden>").replaceAll(cf.CF_TOKEN, "<hidden>");
process.stdout.write(hide(r.stdout));
process.stderr.write(hide(r.stderr));
console.log(`took ${((Date.now() - t0) / 60000).toFixed(1)} min`);
process.exitCode = r.status;
