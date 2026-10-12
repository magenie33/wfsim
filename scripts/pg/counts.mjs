// Row counts of the factory's tables in the server's scratch database, through
// the ssh tunnel on localhost:15432. Secrets read, never printed.
import { readFileSync } from "node:fs";
import postgres from "./node_modules/postgres/src/index.js";
const pg = Object.fromEntries(readFileSync(process.argv[2], "utf8").split(/\r?\n/).filter((l) => l.includes("="))
  .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim()]));
const sql = postgres(`postgres://${pg.PG_USER}:${encodeURIComponent(pg.PG_PASSWORD)}@localhost:15432/wfsim_scratch`, { onnotice: () => {} });
for (const t of ["questions", "demands", "answers", "verifiers", "verifier_hours", "watchdog_told", "points_pool"])
  console.log(t.padEnd(15), (await sql.unsafe(`SELECT count(*)::int AS n FROM ${t}`))[0].n);
await sql.end();
