// THE BOARD'S PIPELINE ON POSTGRES (scripts/db.sh, scripts/db.mjs): the real
// scripts, not their stubs, run one pass against an on-disk PGlite laid with
// worker/pg — an arrival read from the inbox, shipped as a build, asked for,
// read back from the queue, measured, its fact shipped and the row spent, the
// facts and the library read, a backup restored, the orphans and dead rulers
// purged — and each table must hold what the pass says it wrote.
//   npm ci --prefix scripts/pg && node scripts/check_pg_scripts.mjs
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, writeFileSync, rmSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const dir = mkdtempSync(join(tmpdir(), "wfsim-pg-scripts-"));
process.env.PG_URL = `pglite:${join(dir, "db")}`;
const { query, close, answer, useClient, READ_TRIES } = await import("./db.mjs");
let failed = 0;
const check = (name, ok, detail = "") => { console.log(`  ${ok ? "ok  " : "FAIL"} ${name}${ok ? "" : ` — ${detail}`}`); if (!ok) failed++; };
const ROOT = new URL("..", import.meta.url);
const file = (name, text) => { writeFileSync(join(dir, name), text); return join(dir, name); };
const read = (name) => readFileSync(join(dir, name), "utf8").split("\n").filter(Boolean);
/// Where every script's `mktemp` lands, so what one leaves behind is seen.
const scratch = join(dir, "tmp");
mkdirSync(scratch);
/// One script, its database closed here first: PGlite is one process's at a time.
async function run(script, ...args) {
  await close();
  const r = spawnSync("bash", [new URL(`./${script}`, import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1"), ...args],
    { cwd: dir, encoding: "utf8", env: { ...process.env, TMPDIR: scratch, FACTORY_ORIGIN: "http://127.0.0.1:9", FACTORY_TOKEN: "" } });
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
// …ITS ANSWERS IN A FILE, the pipe saying only whether: bash reads a pipe a
// byte at a time, and a page of facts through it cost minutes a read.
{
  await close();
  const out = join(dir, "answer.json");
  const r = spawnSync(process.execPath, [new URL("./pgq.mjs", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1"), "--lines", out],
    { input: `${JSON.stringify({ sql: "SELECT repeat('x', 100000) AS s" })}\n`, encoding: "utf8", env: process.env });
  const kept = (() => { try { return JSON.parse(readFileSync(out, "utf8")); } catch (_) { return null; } })();
  check("...its answer in a file, the pipe saying only ok", r.stdout === "ok\n" && kept?.result?.[0]?.results?.[0]?.s?.length === 100000,
    `stdout ${JSON.stringify(r.stdout.slice(0, 80))} ${r.stderr.slice(0, 200)}`);
}

// A READ THE TUNNEL CUT IS READ AGAIN; A WRITE IS NOT, since it may have landed.
{
  await close();
  let calls = 0;
  const cut = () => { calls++; if (calls < READ_TRIES) throw Object.assign(new Error("write CONNECTION_CLOSED"), { code: "CONNECTION_CLOSED" }); return { rows: [{ n: 1 }], changes: 0 }; };
  useClient("library", { run: async () => cut(), end: async () => {} });
  const read = await answer("SELECT 1 AS n");
  const readCalls = calls;
  calls = 0;
  const write = await answer("DELETE FROM queue");
  check("a read cut by the tunnel is read again, a write is not",
    read.success && readCalls === READ_TRIES && !write.success && calls === 1, `read ${JSON.stringify(read)} in ${readCalls}, write ${calls}`);
  await close();
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
// THE LIVE BOARD'S FACTS FILE IS STREAMED AND APPENDED TO (scripts/live_publish.mjs),
// never read whole: it is every row of `scores`, past what one string can hold.
{
  const live = join(dir, "live");
  mkdirSync(live);
  const facts = join(live, "facts-known.ndjson");
  const old = `${JSON.stringify({ identity: "gone", ruler: "standard_single_target", mode: "base", measured_by: "m", score: 1,
    cost_seconds: 0, started_at: "2020-01-01T00:00:00.000Z", finished_at: "2020-01-01T00:00:00.000Z" })}\n`;
  writeFileSync(facts, old);
  writeFileSync(join(live, "facts-cursor.txt"), "2020-01-01T00:00:00.000Z\n");
  const text = (p) => { try { return readFileSync(p, "utf8"); } catch (_) { return ""; } };
  const publish = async (...args) => {
    await close();
    return spawnSync(process.execPath, [new URL("./live_publish.mjs", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1"), ...args],
      { encoding: "utf8", env: process.env });
  };
  const rows = (await query("SELECT identity, ruler, mode FROM scores")).length;
  const first = await publish("delta", live);
  const grown = readFileSync(facts, "utf8");
  check("a delta appends every row that moved and keeps what the file held",
    first.status === 0 && rows > 0 && grown.startsWith(old) && grown.split("\n").filter(Boolean).length === 1 + rows,
    `exit ${first.status}, ${grown.split("\n").filter(Boolean).length} line(s) for ${rows} row(s) ${first.stderr.slice(0, 300)}`);
  const again = await publish("delta", live);
  check("...and the next, with nothing new, moves nothing", again.status === 3 && readFileSync(facts, "utf8") === grown,
    `exit ${again.status} ${again.stderr.slice(0, 300)}`);
  // A FILE READ WHOLE MOVES THE CURSOR to its newest row: a delta after it asks
  // for hours, never every row since a cursor that had stopped.
  writeFileSync(join(live, "facts-cursor.txt"), "2020-01-01T00:00:00.000Z\n");
  const newest = (await query("SELECT max(finished_at) AS at FROM scores"))[0].at;
  const moved = await publish("cursor", live);
  check("a store read whole moves the cursor to its newest row",
    moved.status === 0 && readFileSync(join(live, "facts-cursor.txt"), "utf8").trim() === newest,
    `${readFileSync(join(live, "facts-cursor.txt"), "utf8").trim()} for ${newest} ${moved.stderr.slice(0, 300)}`);
  const [known] = await query("SELECT identity, ruler, mode FROM scores LIMIT 1");
  const waiting = { identity: "b1", ruler: "no_fact_yet", mode: "base", score: 2 };
  writeFileSync(join(live, "unverified.ndjson"), `${JSON.stringify({ ...known, score: 3 })}\n${JSON.stringify(waiting)}\n`);
  const opened = await publish("open", live, join(live, "unverified.ndjson"), join(live, "open.ndjson"));
  check("a client result a fact answers is dropped, and one none does is kept",
    opened.status === 0 && text(join(live, "open.ndjson")) === `${JSON.stringify(waiting)}\n`,
    text(join(live, "open.ndjson")).slice(0, 300));
  writeFileSync(join(live, "library-ids.ndjson"), `${JSON.stringify({ k: "b1", v: { weapon: "torid" } })}\n`);
  writeFileSync(join(live, "library-live.json"), JSON.stringify([{ id: "b1", weapon: "torid" }, { id: "b9", weapon: "braton" }]));
  writeFileSync(join(live, "ids.txt"), "b1\n");
  const cut = await publish("subset", live, join(live, "ids.txt"), join(live, "sub.json"), facts, join(live, "open.ndjson"));
  const cutFacts = text(join(live, "sub.json.facts")).split("\n").filter(Boolean).map((l) => JSON.parse(l));
  check("a subset holds its weapon's builds and their facts from every file it is given",
    cut.status === 0 && JSON.parse(text(join(live, "sub.json")) || "[]").length === 1
      && cutFacts.length > 1 && cutFacts.every((f) => f.identity === "b1") && cutFacts.some((f) => f.ruler === "no_fact_yet"),
    `exit ${cut.status} ${cut.stderr.slice(0, 300)}`);
}
// NOTHING A SCRIPT MADE OUTLIVES IT (scripts/db.sh `DB_TMP`): the live board
// runs them all day on a server whose /tmp is memory.
const left = readdirSync(scratch);
check("every script's temp files are gone when it exits", left.length === 0, left.join(" "));

await close();
rmSync(dir, { recursive: true, force: true });
console.log(failed ? `\n${failed} failed` : "\nthe pipeline's scripts read and write Postgres");
process.exitCode = failed ? 1 : 0;
