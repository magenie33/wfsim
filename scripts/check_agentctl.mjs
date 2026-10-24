// Local coordination invariants, with isolated stores and competing processes.
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { spawn, spawnSync, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const script = process.env.AGENTCTL_TEST_SCRIPT ?? fileURLToPath(new URL("./agentctl.mjs", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "wfsim-agentctl-"));
const details = { summary: "Ready", branch: "task-one", commit: "uncommitted",
  validation: "targeted checks passed", next: "review", post_live: "none", tool_note: "可接手" };
let directory, a, b;
const args = (...values) => [script, "--state-dir", directory, ...values.map(String)];
function raw(...values) {
  return spawnSync(process.execPath, args(...values), { encoding: "utf8" });
}
function call(...values) {
  const r = raw(...values);
  assert.equal(r.status, 0, r.stderr);
  return JSON.parse(r.stdout).result;
}
function fails(pattern, ...values) {
  const r = raw(...values);
  assert.notEqual(r.status, 0, r.stdout);
  assert.match(JSON.parse(r.stderr.split("\n").find(l => l.startsWith("{"))).error, pattern);
}
function compete(...values) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, args(...values), { stdio: ["ignore", "pipe", "pipe"] });
    let stderr = "";
    child.stdout.resume();
    child.stderr.on("data", data => { stderr += data; });
    child.on("error", reject);
    child.on("close", code => resolve({ code, stderr }));
  });
}
function sql(statement, ...values) {
  const db = new DatabaseSync(join(directory, "state.sqlite3"));
  try { db.prepare(statement).run(...values); } finally { db.close(); }
}
function age(actor) {
  sql("UPDATE sessions SET heartbeat=? WHERE id=?", new Date(Date.now() - 3600000).toISOString(), actor);
}
function create(id) {
  return call("task", "create", id, "--session", a, "--title", "Shared work", "--scope", "scripts; live: none");
}
function claim(id = "one", actor = a) { return call("task", "claim", id, "--session", actor); }
let checks = 0;
async function test(name, body) {
  directory = join(root, String(++checks));
  a = call("session", "start", "--tool", "vendor-a").session;
  b = call("session", "start", "--tool", "vendor-b").session;
  create("one");
  await body();
  console.log(`ok ${name}`);
}
try {
  await test("one winner for simultaneous task claims", async () => {
    const results = await Promise.all([a, b].map(actor => compete("task", "claim", "one", "--session", actor)));
    assert.deepEqual(results.map(r => r.code).sort(), [0, 1]);
    assert.equal(call("task", "show", "one").revision, 1);
  });
  await test("concurrent first use creates one coherent schema", async () => {
    directory = join(root, "fresh");
    const results = await Promise.all([1, 2, 3, 4].map(n => compete("session", "start", "--tool", `vendor-${n}`)));
    assert.deepEqual(results.map(r => r.code), [0, 0, 0, 0]);
    assert.equal(call("session", "list").length, 4);
  });
  await test("handoff across tools preserves details and ends old owner", () => {
    claim();
    call("task", "handoff", "one", "--session", a, "--revision", 1, "--details", JSON.stringify(details));
    call("session", "end", "--session", a);
    const task = claim("one", b);
    assert.equal(task.revision, 3);
    assert.deepEqual(task.details, details);
    fails(/ended/, "task", "update", "one", "--session", a, "--revision", 1, "--details", "{}");
    call("task", "complete", "one", "--session", b, "--revision", 3, "--details", JSON.stringify(details));
    call("session", "end", "--session", b);
  });
  await test("owner, revisions and handoff shape are enforced", () => {
    claim();
    fails(/owned/, "task", "update", "one", "--session", b, "--revision", 1, "--details", "{}");
    call("task", "update", "one", "--session", a, "--revision", 1, "--details", "{}");
    fails(/stale task revision/, "task", "update", "one", "--session", a, "--revision", 1, "--details", "{}");
    fails(/needs nonblank/, "task", "handoff", "one", "--session", a, "--revision", 2, "--details", "{}");
    assert.equal(call("task", "show", "one").revision, 2);
  });
  await test("UTF-8 file handoff and its event survive file removal", () => {
    claim();
    const file = join(root, "handoff details.json");
    fails(/ENOENT/, "task", "update", "one", "--session", a, "--revision", 1, "--details-file", file);
    writeFileSync(file, JSON.stringify(details));
    call("task", "handoff", "one", "--session", a, "--revision", 1, "--details-file", file);
    rmSync(file);
    const event = call("state", "events").find(e => e.action === "task.handoff");
    assert.deepEqual(event.details.task.details, details);
  });
  await test("stale presence is unknown, not automatic takeover", () => {
    claim(); age(a);
    assert.equal(call("session", "list").find(s => s.id === a).presence, "unknown");
    fails(/recovery requires/, "task", "claim", "one", "--session", b);
    call("task", "claim", "one", "--session", b, "--confirm-stopped", "--note", "Old execution checked stopped");
    fails(/owned/, "task", "update", "one", "--session", a, "--revision", 1, "--details", "{}");
    fails(/recent heartbeat/, "task", "claim", "one", "--session", a, "--confirm-stopped", "--note", "try");
  });
  await test("resources never expire and require explicit recovery", () => {
    claim(); create("two"); claim("two", b);
    call("resource", "acquire", "prod:bot", "--session", a, "--task", "one");
    age(a);
    fails(/already held/, "resource", "acquire", "prod:bot", "--session", b, "--task", "two");
    fails(/another session/, "resource", "release", "prod:bot", "--session", b);
    fails(/confirm-stopped/, "resource", "recover", "prod:bot", "--session", b, "--note", "not checked");
    fails(/release task resources/, "task", "claim", "one", "--session", b, "--confirm-stopped", "--note", "stopped");
    call("resource", "recover", "prod:bot", "--session", b, "--confirm-stopped", "--note", "Checked stopped");
    call("resource", "acquire", "prod:bot", "--session", b, "--task", "two");
    fails(/release task resources/, "task", "handoff", "two", "--session", b, "--revision", 1, "--details", JSON.stringify(details));
    fails(/owned tasks/, "session", "end", "--session", b);
    call("resource", "release", "prod:bot", "--session", b);
  });
  await test("one winner for simultaneous resource acquisitions", async () => {
    claim(); create("two"); claim("two", b);
    const results = await Promise.all([[a, "one"], [b, "two"]].map(([actor, task]) =>
      compete("resource", "acquire", "release:main", "--session", actor, "--task", task)));
    assert.deepEqual(results.map(r => r.code).sort(), [0, 1]);
  });
  await test("messages persist and only recipient acknowledges", () => {
    const { id } = call("message", "send", "--session", a, "--to", b, "--body", "Review one");
    fails(/recipient/, "message", "ack", id, "--session", a);
    assert.equal(call("message", "inbox", "--session", b)[0].acknowledged, null);
    call("message", "ack", id, "--session", b);
    assert.ok(call("message", "inbox", "--session", b)[0].acknowledged);
    call("session", "end", "--session", b);
    fails(/recipient ended/, "message", "send", "--session", a, "--to", b, "--body", "later");
  });
  await test("invalid JSON, arguments and schema fail without task mutation", () => {
    claim();
    for (const value of ["[]", "null", "{", '{"number":NaN}'])
      fails(/JSON|Unexpected|Expected|property/i, "task", "update", "one", "--session", a, "--revision", 1, "--details", value);
    for (const extra of [["--revision", "0"], ["--unknown", "x"], ["--revision", "1", "--revision", "2"]])
      fails(/revision|unknown|duplicate/, "task", "update", "one", "--session", a, "--details", "{}", ...extra);
    assert.equal(call("task", "show", "one").revision, 1);
    sql("PRAGMA user_version=99");
    fails(/unsupported state schema/, "task", "list");
  });
  await test("snapshot preserves legacy claims", () => {
    const legacy = join(root, ".handoff.md");
    writeFileSync(legacy, "Existing claims");
    assert.match(call("state", "snapshot").legacy_claims, /not imported/);
    assert.equal(readFileSync(legacy, "utf8"), "Existing claims");
  });
  await test("Git worktrees share default store, including existing schema-1 records", () => {
    const repo = join(root, "repo"), worktree = join(root, "worktree");
    mkdirSync(repo);
    const git = (...values) => execFileSync("git", ["-C", repo, ...values], { stdio: "pipe" });
    git("init", "--quiet");
    git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--allow-empty", "--no-verify", "-m", "initial");
    git("worktree", "add", "--quiet", "-b", "other", worktree);
    try {
      for (const checkout of [repo, worktree]) {
        mkdirSync(join(checkout, "scripts"));
        writeFileSync(join(checkout, "scripts", "agentctl.mjs"), readFileSync(script));
      }
      const common = git("rev-parse", "--path-format=absolute", "--git-common-dir").toString().trim();
      mkdirSync(join(common, "wfsim-agents"));
      // Existing Python schema-1 data must be readable without resetting it.
      const db = new DatabaseSync(join(common, "wfsim-agents", "state.sqlite3"));
      db.exec("PRAGMA user_version=1; CREATE TABLE tasks (id TEXT PRIMARY KEY,title TEXT NOT NULL,scope TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'pending',owner TEXT,revision INTEGER NOT NULL DEFAULT 0,details TEXT NOT NULL DEFAULT '{}'); INSERT INTO tasks(id,title,scope) VALUES ('existing','Preserved','none')");
      db.close();
      const run = (checkout, ...values) => JSON.parse(execFileSync(process.execPath,
        [join(checkout, "scripts", "agentctl.mjs"), ...values], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] })).result;
      assert.equal(run(repo, "task", "list")[0].id, "existing");
      assert.equal(run(worktree, "task", "list")[0].id, "existing");
      assert.equal(run(repo, "state", "snapshot").directory, run(worktree, "state", "snapshot").directory);
    } finally {
      execFileSync(process.execPath, [fileURLToPath(new URL("./remove_worktree.mjs", import.meta.url)), worktree, "--force"],
        { cwd: repo, stdio: "pipe" });
    }
  });
  console.log(`${checks} coordination checks passed`);
} finally {
  rmSync(root, { recursive: true, force: true });
}
