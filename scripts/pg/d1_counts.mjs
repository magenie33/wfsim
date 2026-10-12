// Row counts of the factory's tables in D1, read through its query API. Read
// only; the token is read, never printed.
import { readFileSync } from "node:fs";
const cf = Object.fromEntries(readFileSync(process.argv[2], "utf8").split(/\r?\n/).filter((l) => l.includes("="))
  .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim()]));
for (const t of ["questions", "demands", "answers", "verifiers", "verifier_hours", "watchdog_told", "points_pool"]) {
  const r = await fetch(`https://api.cloudflare.com/client/v4/accounts/${cf.CF_ACCOUNT}/d1/database/${cf.CF_D1_DATABASE}/query`, {
    method: "POST", headers: { authorization: `Bearer ${cf.CF_TOKEN}`, "content-type": "application/json" }, body: JSON.stringify({ sql: `SELECT count(*) AS n FROM ${t}` }) });
  const j = await r.json();
  console.log(t.padEnd(15), j.success ? j.result[0].results[0].n : `failed ${r.status} ${JSON.stringify(j.errors)}`);
}
