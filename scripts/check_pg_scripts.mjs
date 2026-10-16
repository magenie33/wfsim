// THE BOARD'S PIPELINE ON POSTGRES (scripts/db.sh, scripts/db.mjs): the real
// scripts, not their stubs, run one pass against an on-disk PGlite laid with
// worker/pg — an arrival read from the inbox, shipped as a build, asked for,
// read back from the queue, measured, its fact shipped and the row spent, the
// facts and the library read, a backup restored, the orphans and dead rulers
// purged — and each table must hold what the pass says it wrote.
//   npm ci --prefix scripts/pg && node scripts/check_pg_scripts.mjs
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const dir = mkdtempSync(join(tmpdir(), "wfsim-pg-scripts-"));
process.env.PG_URL = `pglite:${join(dir, "db")}`;
const { query, close } = await import("./db.mjs");
let failed = 0;
const check = (name, ok, detail = "") => { console.log(`  ${ok ? "ok  " : "FAIL"} ${name}${ok ? "" : ` — ${detail}`}`); if (!ok) failed++; };
const ROOT = new URL("..", import.meta.url);
const file = (name, text) => { writeFileSync(join(dir, name), text); return join(dir, name); };
const read = (name) => readFileSync(join(dir, name), "utf8").split("\n").filter(Boolean);
/// One script, its database closed here first: PGlite is one process's at a time.
async function run(script, ...args) {
  await close();
  const r = spawnSync("bash", [new URL(`./${script}`, import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1"), ...args],
    { cwd: dir, encoding: "utf8", env: { ...process.env, FACTORY_ORIGIN: "http://127.0.0.1:9", FACTORY_TOKEN: "" } });
  if (r.status !== 0) console.log(`${r.stdout}${r.stderr}`.split("\n").map((l) => `      ${l}`).join("\n"));
  return r.status;
}

const schema = ["schema.sql", "library.sql"].map((f) => readFileSync(new URL(`worker/pg/${f}`, ROOT), "utf8")).join("\n");
{
  const { PGlite } = await import("./pg/node_modules/@electric-sql/pglite/dist/index.js");
  const pg = new PGlite(join(dir, "db"));
  await pg.exec(schema);
  await pg.close();
}
const record = { weapon: "torid", mods: ["serration"] };
await query("INSERT INTO inbox (id, at, record) VALUES (?, ?, ?), (?, ?, ?)",
  ["i1", "2026-10-01", JSON.stringify(record), "i2", "2026-10-01", JSON.stringify({ weapon: "braton", mods: [] })]);

check("the inbox is read", await run("fetch_inbox.sh", "inbox.ndjson") === 0 && read("inbox.ndjson").length === 2);
file("builds.ndjson", `${JSON.stringify({ id: "b1", at: "2026-10-01", record })}\n`);
file("done.txt", "i1\ni2\n");
check("a build is shipped and its arrivals spent", await run("ship_builds.sh", "builds.ndjson", "done.txt") === 0
  && (await query("SELECT weapon FROM builds WHERE id = 'b1'"))[0]?.weapon === "torid"
  && (await query("SELECT count(*) AS n FROM inbox"))[0].n === 0);
check("...and shipped again it is one row", await run("ship_builds.sh", "builds.ndjson", "done.txt") === 0
  && (await query("SELECT count(*) AS n FROM builds"))[0].n === 1);

const rows = ["standard_single_target", "heavy_gunner"].map((ruler) => JSON.stringify({ build_id: "b1", ruler, mode: "base" })).join("\n");
file("missing.ndjson", `${rows}\n`);
await run("ship_queue.sh", "arrivals-2026-10-01", "test", "missing.ndjson");
await run("ship_queue.sh", "arrivals-2026-10-01", "test", "missing.ndjson");
check("a batch is asked for once a row, its total counting every ask",
  (await query("SELECT count(*) AS n FROM queue"))[0].n === 2 && (await query("SELECT total FROM batches"))[0]?.total === 4);
check("the queue is read in order", await run("fetch_queue.sh", "queue.ndjson") === 0
  && read("queue.ndjson").map((l) => JSON.parse(l).ruler).join() === "heavy_gunner,standard_single_target");
// …OVER ONE CONNECTION (scripts/db.sh `db_send`): a read is two statements at
// least, the batches and a page, and a process a statement cost seconds a page.
{
  const bin = join(dir, "bin"), log = join(dir, "node.log");
  rmSync(log, { force: true });
  mkdirSync(bin, { recursive: true });
  const posix = (p) => p.replace(/\\/g, "/");
  file("bin/node", `#!/usr/bin/env bash\necho "$*" >> '${posix(log)}'\nexec '${posix(process.execPath)}' "$@"\n`);
  chmodSync(join(bin, "node"), 0o755);
  const path = process.env.PATH;
  process.env.PATH = `${bin}${process.platform === "win32" ? ";" : ":"}${path}`;
  const status = await run("fetch_queue.sh", "queue.ndjson");
  process.env.PATH = path;
  const spawned = readFileSync(log, "utf8").split("\n").filter((l) => l.includes("pgq.mjs"));
  check("...over one database process, not one a statement", status === 0 && spawned.length === 1, spawned.join(" | "));
}

const fact = { identity: "b1", ruler: "heavy_gunner", mode: "base", measured_by: "sha", score: 12.5, metric: "kpm",
  cost_seconds: 3, started_at: "2026-10-01T00:00:00.000Z", finished_at: "2026-10-01T00:00:03.000Z" };
file("facts.ndjson", `${JSON.stringify(fact)}\n`);
check("a fact is shipped and its row spent", await run("ship_facts.sh", "facts.ndjson") === 0
  && (await query("SELECT score FROM scores"))[0]?.score === 12.5 && (await query("SELECT count(*) AS n FROM queue"))[0].n === 1);
file("facts-again.ndjson", `${JSON.stringify({ ...fact, score: 13 })}\n`);
check("...and measured again it is overwritten", await run("ship_facts.sh", "facts-again.ndjson") === 0
  && (await query("SELECT score FROM scores"))[0]?.score === 13);
check("the facts are read", await run("fetch_facts.sh", "facts-read.ndjson") === 0 && JSON.parse(read("facts-read.ndjson")[0]).score === 13);
check("the library is read", await run("fetch_library.sh", "library.json") === 0 && readFileSync(join(dir, "library.json"), "utf8").includes("torid"));
file("snapshot.ndjson", `${JSON.stringify({ k: "b2", v: { at: "2026-10-02", weapon: "braton", mods: [] } })}\n`);
check("a backup is restored into the library", await run("restore_library.sh", "snapshot.ndjson", "--write") === 0
  && (await query("SELECT weapon FROM builds WHERE id = 'b2'"))[0]?.weapon === "braton");

file("orphans.ndjson", `${JSON.stringify({ build_id: "b1", ruler: "standard_single_target", mode: "base" })}\n`);
check("an orphan is purged", await run("purge_orphans.sh", "orphans.ndjson") === 0 && (await query("SELECT count(*) AS n FROM queue"))[0].n === 0);
await query("INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('arrivals-2026-10-01', 'b1', 'no_such_ruler', 'base')");
// EXECUTED, not sourced (`purge_queue.sh` runs it), so Linux needs the bit.
process.env.WFSIM_BOARD = file("rulers.sh","#!/bin/sh\necho standard_single_target\necho heavy_gunner\n");
chmodSync(process.env.WFSIM_BOARD, 0o755);
check("a ruler the repo no longer has is purged", await run("purge_queue.sh") === 0
  && (await query("SELECT count(*) AS n FROM queue"))[0].n === 0);

await close();
rmSync(dir, { recursive: true, force: true });
console.log(failed ? `\n${failed} failed` : "\nthe pipeline's scripts read and write Postgres");
process.exitCode = failed ? 1 : 0;
