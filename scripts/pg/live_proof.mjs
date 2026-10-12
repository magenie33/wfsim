// The differential check against the server's scratch database, through an
// ssh tunnel on localhost:15432; the password is read, never printed.
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
const env = Object.fromEntries(readFileSync(process.argv[2], "utf8").split(/\r?\n/).filter((l) => l.includes("="))
  .map((l) => [l.slice(0, l.indexOf("=")), l.slice(l.indexOf("=") + 1)]));
const url = `postgres://${env.PG_USER}:${encodeURIComponent(env.PG_PASSWORD)}@localhost:15432/${process.argv[3] || "wfsim_scratch"}`;
const r = spawnSync(process.execPath, ["scripts/check_pg_factory.mjs"], { encoding: "utf8", env: { ...process.env, PG_URL: url } });
process.stdout.write((r.stdout || "").replaceAll(env.PG_PASSWORD, "<hidden>"));
process.stderr.write((r.stderr || "").replaceAll(env.PG_PASSWORD, "<hidden>"));
process.exitCode = r.status;
