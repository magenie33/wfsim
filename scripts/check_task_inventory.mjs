// Complete demand is independent of admission pacing and partial scans.
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { completeInventory, publishInventory } from "./compute_inventory.mjs";
import { inventoryWrite, readInventory, validInventory, INVENTORY_MAX_MS } from "../worker/task_inventory.js";
import { testDb } from "./pg/test_db.mjs";
import worker from "../worker/index.js";

const dir = mkdtempSync(join(tmpdir(), "compute-inventory-test-")), now = Date.now(), started = new Date(now - 60000).toISOString();
const row = (id, ruler = "r") => ({ build_id: id, ruler, mode: "base" });
const file = (name, rows) => { const p = join(dir, name); writeFileSync(p, rows.map(r => JSON.stringify(r)).join("\n") + "\n"); return p; };
try {
  const queue = file("queue", [row("a"), row("a"), row("b"), row("retired", "old")]);
  const missing = file("missing", [row("b"), row("d"), row("e"), row("d")]);
  const orphan = file("orphans", [row("b")]);
  const plan = await completeInventory(queue, missing, orphan, started, ["r"]);
  assert.equal(plan.rows, 3); assert.equal(plan.queued, 1);
  const admitted = await completeInventory(file("queue2", [row("a"), row("d")]), file("missing2", [row("e")]), file("empty", []), started, ["r"]);
  assert.equal(admitted.rows, plan.rows, "admission changed complete demand");
  assert.equal(admitted.queued, 2);
  const completed = await completeInventory(file("queue3", [row("d")]), file("missing3", [row("e")]), file("empty", []), started, ["r"]);
  assert.equal(completed.rows, 2, "confirmed work did not leave the next inventory");
  const many = Array.from({ length: 10001 }, (_, i) => row(String(i)));
  const chunked = await completeInventory(file("many", many), file("duplicates", many), file("remove-many", [many[0]]), started, ["r"]);
  assert.equal(chunked.rows, 10000); assert.equal(chunked.queued, 10000);
  await assert.rejects(completeInventory(queue, file("invalid", [null]), orphan, started, ["r"]), /invalid task row/);
  await assert.rejects(completeInventory(queue, missing, orphan, started, []), /incomplete inventory roster/);
  console.log("ok: the complete union includes unadmitted work, deduplicates batches, removes refused/retired work and falls after completion");

  const db = await testDb("library");
  await db.run(`INSERT INTO questions(id,verb,request,state,due_at,opened_at,producer) VALUES
    ('covered','simulate','{}','open','x','x','sweep'),
    ('new-type','future_verb','{}','open','x','x','future_owner'),
    ('review','simulate','{}','disputed','x','x','board_review'),
    ('spot','simulate','{}','spot','x','x','sweep'),
    ('shared','simulate','{}','open','x','x','future_owner'),
    ('done','future_verb','{}','fact','x','x','future_owner')`);
  await db.run("INSERT INTO demands(producer,ref,question,state,asked_at,due_at,payload) VALUES ('sweep','shared','shared','live','x','x','{}')");
  const save = async (p, at = now) => { const w = inventoryWrite(p, at); await db.run(w.sql, ...w.params); };
  await save(plan);
  const goal = await readInventory(db, now);
  assert.equal(goal.total, 6); assert.equal(goal.registered, 4);
  assert.equal(goal.orders.find(o => o.kind === "future_owner").total, 1);
  assert.equal(goal.verbs.future_verb, 1); assert.equal(goal.verbs.simulate, 5);
  assert(validInventory(goal, now));
  await save({ ...plan, rows: 999, started_at: new Date(now - 120000).toISOString() }, now + 1);
  assert.equal((await readInventory(db, now + 1)).total, 6, "an older scan overwrote newer inputs");
  assert.equal(await readInventory(db, now + INVENTORY_MAX_MS + 2), null);
  assert(!validInventory({ ...goal, total: 999 }, now));
  assert.throws(() => inventoryWrite({ ...plan, queued: 999 }, now), /incomplete/);
  assert.throws(() => inventoryWrite({ ...plan, started_at: new Date(now - INVENTORY_MAX_MS - 1).toISOString() }, now), /incomplete/);
  console.log("ok: new task kinds count automatically, settled work is excluded, older/partial/stale inventories cannot masquerade as current demand");
  const env = { LIBRARY: db, FACTORY_TOKEN: "inventory-test" };
  const send = (url, init) => worker.fetch(new Request(url, init), env);
  const publish = (p, token = "inventory-test") => publishInventory(p, { origin: "https://example.test", token, send });
  await assert.rejects(publish(plan, ""), /needs FACTORY_TOKEN/);
  await assert.rejects(publish(plan, "wrong"), /refused \(401\)/);
  assert.equal((await send("https://example.test/api/board/inventory")).status, 405);
  await assert.rejects(publish({ ...plan, rows: -1 }), /refused \(400\)/);
  assert.equal((await readInventory(db, now)).total, 6);
  await publish({ ...plan, covers: ["future_owner"], started_at: new Date(now - 1000).toISOString() });
  assert.equal((await readInventory(db)).total, 6, "a board scan replaced another producer's coverage");
  assert.equal((await readInventory(db)).verbs.future_verb, 1);
  console.log("ok: the real Worker authenticates publication, rejects partial scans and owns producer coverage without pipeline task-table grants");
  await db.close();
} finally { rmSync(dir, { recursive: true, force: true }); }
