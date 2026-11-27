// SPDX-License-Identifier: AGPL-3.0-or-later
// End-to-end verification of the declarative board order planner, partitioned
// queue zone/position merging, manual order reordering, cross-zone movement,
// shared-task cancellation rollback and two-witness in-place rescore overwrite.
import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";
import { facts, refusalsFor } from "../worker/factory.js";
import { PROTOCOL } from "../worker/verify.js";
import { syncRoute, forgetWitnesses } from "../worker/sync.js";
import { questionId } from "../worker/tasks.js";
import { BOARD_ORDER_CATALOG } from "../worker/board_order_catalog.js";
import { planBoardOrder } from "../worker/board_order_planner.js";
import {
  previewBoardOrder,
  createBoardOrder,
  reorderBoardOrder,
  rezoneBoardOrder,
  cancelBoardOrder,
  updateBoardOrderReason,
  inspectQueueAndOrders,
} from "../worker/board_orders.js";

function fullMods(prefix = "serration") {
  return [
    prefix,
    "split_chamber",
    "vital_sense",
    "point_strike",
    "infected_clip",
    "stormbringer",
    "hellfire",
    "cryo_rounds",
  ];
}

export async function boardOrderCases(db) {
  refusalsFor(false);
  const f = facts();
  const baseNow = Date.parse("2026-11-27T06:00:00.000Z");
  const iso = (ms) => new Date(ms).toISOString();
  const env = { LIBRARY: db, FACTORY_TOKEN: "secret-token" };
  const witness = (name) => ({ device: `dev-${name}`, owner: `own-${name}`, net: `net-${name}`, lanes: 16 });

  // 1. Negative control & fix for same-batch duplicate question zone merging.
  const sharedReq = { record: { weapon: "torid", mods: fullMods() }, ruler: "standard_single_target", mode: "incarnon" };
  if (process.argv.includes("--break-zone-merge")) {
    const { readFileSync } = await import("node:fs");
    const schema = readFileSync(new URL("../worker/pg/schema.sql", import.meta.url), "utf8");
    const admitDef = schema.match(/CREATE OR REPLACE FUNCTION compute_admit_tasks\([^]*?\$\$;/)[0];
    await db.exec(admitDef.replace(
      /'zone',\(SELECT min\(coalesce\(\(y->>'zone'\)::bigint,factory_default_zone\(y->>'producer'\)\)\)\s*FROM jsonb_array_elements\(d\) y WHERE y->>'question'=source\.value->>'id'\),/,
      "'zone',5::bigint,",
    ).replace(
      /PERFORM factory_restate\(id\)[^;]*;/,
      "",
    ));
  }
  const [dupA, dupB] = await f.demandAll(env, [
    { producer: "sweep", ref: "b0000000000000000000000000000001|standard_single_target|incarnon", verb: "simulate", request: sharedReq, due_in_ms: 60_000, zone: 5, payload: { identity: "b0000000000000000000000000000001", ruler: "standard_single_target", mode: "incarnon", batch: "b1" } },
    { producer: "user_calculation", ref: "uc-1", verb: "simulate", request: sharedReq, due_in_ms: 60_000, zone: 0, payload: {} },
  ], "eng-new", baseNow);
  assert.equal(dupA, dupB);
  const mergedRow = await db.get("SELECT zone FROM state_compute_work WHERE id = ?", dupA);
  assert.equal(mergedRow.zone, 0, "same-batch duplicate question must merge to minimum zone 00");
  await f.withdraw(env, "sweep", "b0000000000000000000000000000001|standard_single_target|incarnon");
  await f.withdraw(env, "user_calculation", "uc-1");

  // 2. Seed library builds, popularity and baseline scores for beam & non-beam weapons.
  const bToridTop = "10000000000000000000000000000001";
  const bToridLow = "10000000000000000000000000000002";
  const bPhantasma = "20000000000000000000000000000001";
  const bGlaxion = "30000000000000000000000000000001";
  const bBraton = "40000000000000000000000000000001";
  const bIncomplete = "50000000000000000000000000000001";

  const builds = [
    [bToridTop, { weapon: "torid", mods: fullMods("serration"), evolutions: ["evo1", "evo2", "evo3", "evo4"], arcanes: ["primary_merciless"] }],
    [bToridLow, { weapon: "torid", mods: fullMods("heavy_caliber"), evolutions: ["evo1", "evo2", "evo3", "evo4"], arcanes: ["primary_deadhead"] }],
    [bPhantasma, { weapon: "phantasma_prime", mods: fullMods("primed_point_blank"), arcanes: ["primary_merciless"] }],
    [bGlaxion, { weapon: "tenet_glaxion", mods: fullMods("serration"), valence: "magnetic", arcanes: ["primary_merciless"] }],
    [bBraton, { weapon: "braton_prime", mods: fullMods("serration"), evolutions: ["evo1", "evo2", "evo3", "evo4"], arcanes: ["primary_merciless"] }],
    [bIncomplete, { weapon: "phantasma_prime", mods: ["primed_point_blank"], arcanes: [] }],
  ];
  for (const [id, rec] of builds) {
    await db.run("INSERT INTO entity_weapon_builds (id, at, record) VALUES (?, '2026-11-20', ?)", id, JSON.stringify(rec));
  }

  // Popularity: torid (500) > tenet_glaxion (200) > phantasma_prime (80), braton_prime (900, non-beam).
  for (const [subject, n] of [["all", 2000], ["torid", 500], ["tenet_glaxion", 200], ["phantasma_prime", 80], ["braton_prime", 900]]) {
    await db.run(
      "INSERT INTO record_daily_usage_summaries (day, event, subject, market, visitors, points, sampled) VALUES ('2026-11-26', ?, ?, 'all', ?, ?, ?)",
      subject === "all" ? "visitors" : "tested",
      subject,
      n,
      n,
      n,
    );
  }

  // Baseline scores: bToridTop (120 KPM, old engine), bToridLow (60 KPM, old engine),
  // bGlaxion (95 KPM, old engine), bPhantasma (already measured on eng-new -> skipped by default rescore!),
  // bBraton (150 KPM, old engine, non-beam -> excluded by tags=['beam']).
  const oldAt = iso(baseNow - 86_400_000);
  const seededScores = [
    [bToridLow, "standard_single_target", "incarnon", "verified:eng-old", 60.0],
    [bToridTop, "standard_single_target", "incarnon", "verified:eng-old", 120.0],
    [bToridTop, "standard_multi_target", "incarnon", "verified:eng-old", 340.0],
    [bGlaxion, "standard_single_target", "base", "verified:eng-old", 95.0],
    [bPhantasma, "standard_single_target", "base", "verified:eng-new", 110.0],
    [bBraton, "standard_single_target", "incarnon", "verified:eng-old", 150.0],
  ];
  for (const [id, ruler, mode, by, score] of seededScores) {
    await db.run(
      `INSERT INTO state_board_measurement_selection
         (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at)
       VALUES (?, ?, ?, ?, ?, 'kpm', 1.2, ?, ?)`,
      id, ruler, mode, by, score, oldAt, oldAt,
    );
  }

  // 3. Planner verification: beam filter, engine-aware rescore, popularity -> score -> ruler ordering, SHA-256 identity parity.
  const preview = await previewBoardOrder(env, {
    zone: 0,
    engine: "eng-new",
    specification: { tags: ["beam"], policy: "rescore", sort: "popularity" },
  });
  assert.equal(preview.ok, true);
  assert(preview.summary.weapons_matched >= 50, `expected >=50 beam weapons, got ${preview.summary.weapons_matched}`);
  assert.equal(preview.summary.total_items, 4, "excludes non-beam Braton and already-verified Phantasma on eng-new");
  assert.deepEqual(
    preview.summary.preview.map((p) => `${p.weapon}:${p.identity.slice(-2)}:${p.ruler}`),
    [
      "torid:01:standard_single_target",
      "torid:02:standard_single_target",
      "torid:01:standard_multi_target",
      "tenet_glaxion:01:standard_single_target",
    ] === null ? [] : [
      // Within torid (pop 500): score 340 (multi) > score 120 (single) > score 60 (single), then tenet_glaxion (pop 200).
      "torid:01:standard_multi_target",
      "torid:01:standard_single_target",
      "torid:02:standard_single_target",
      "tenet_glaxion:01:standard_single_target",
    ],
  );

  // Verify computation hash equals canonical questionId("simulate", ...).
  const fullPlan = await planBoardOrder(db, { tags: ["beam"], policy: "rescore_all", pinned_weapons: ["phantasma_prime"] }, { engine: "eng-new" });
  assert.equal(fullPlan.items.length, 5, "rescore_all includes Phantasma on eng-new");
  assert.equal(fullPlan.items[0].weapon, "phantasma_prime", "pinned_weapons places Phantasma ahead of Torid");
  const expectedQid = await questionId("simulate", {
    record: builds.find(([id]) => id === bPhantasma)[1],
    ruler: "standard_single_target",
    mode: "base",
  });
  assert.equal(fullPlan.items[0].computation, expectedQid, "planner fast SHA-256 matches canonical questionId");

  // Verify `missing` policy finds unmeasured admissible cells and rejects incomplete builds.
  const missingPlan = await planBoardOrder(db, { weapons: ["phantasma_prime"], rulers: ["standard_single_target", "standard_multi_target"], modes: ["base"], policy: "missing" }, { engine: "eng-new" });
  assert.deepEqual(
    missingPlan.items.map((x) => `${x.identity}:${x.ruler}:${x.mode}`),
    [`${bPhantasma}:standard_multi_target:base`],
    "incomplete build excluded; already-measured single_target skipped; missing multi_target selected",
  );

  // 4. Admit order A in Zone 03, then order B + order C in Zone 00; verify FIFO, manual reorder, rezone, and cancellation rollback.
  const orderA = await createBoardOrder(env, {
    order_id: "order-zone03-beams",
    zone: 3,
    engine: "eng-new",
    why: "background beam rescore in zone 03",
    specification: { tags: ["beam"], policy: "rescore", sort: "popularity" },
  }, baseNow);
  assert.equal(orderA.ok && orderA.done, true);
  assert.equal(orderA.total_items, 4);

  // Order B (Glaxion only) and Order C (Torid only) in Zone 00.
  const orderB = await createBoardOrder(env, {
    order_id: "order-zone00-glaxion",
    zone: 0,
    engine: "eng-new",
    why: "glaxion check",
    specification: { weapons: ["tenet_glaxion"], policy: "rescore" },
  }, baseNow + 1000);
  const orderC = await createBoardOrder(env, {
    order_id: "order-zone00-torid",
    zone: 0,
    engine: "eng-new",
    why: "torid urgent check",
    specification: { weapons: ["torid"], policy: "rescore", sort: "popularity" },
  }, baseNow + 2000);
  assert.equal(orderB.ok && orderC.ok, true);

  // Initially in Zone 00: Order B was created before Order C, so Glaxion leads Zone 00.
  const headBeforeReorder = await f.nextPack(env, witness("peek1"), "eng-new", { n: 4, lease_ms: 60_000 }, baseNow + 3000);
  assert.deepEqual(
    headBeforeReorder.questions.map((q) => q.request.record.weapon),
    ["tenet_glaxion", "torid", "torid", "torid"],
  );
  await f.release(env, "dev-peek1", headBeforeReorder.lease);

  // Manually reorder Order C BEFORE Order B inside Zone 00!
  const reordered = await reorderBoardOrder(env, {
    order_id: "order-zone00-torid",
    before_order_id: "order-zone00-glaxion",
  }, baseNow + 4000);
  assert.equal(reordered.ok && reordered.done, true);

  // Verify Order C now dispatches ahead of Order B AND keeps its internal 340 -> 120 -> 60 score order!
  const headAfterReorder = await f.nextPack(env, witness("peek2"), "eng-new", { n: 4, lease_ms: 60_000 }, baseNow + 5000);
  assert.deepEqual(
    headAfterReorder.questions.map((q) => `${q.request.record.weapon}:${q.demands.find((d) => d.payload.order_id === "order-zone00-torid")?.payload.ordinal ?? "glax"}`),
    ["torid:1", "torid:2", "torid:3", "tenet_glaxion:glax"],
    "reorder moves Order C ahead of Order B while preserving Order C's 1..3 popularity/score order",
  );
  await f.release(env, "dev-peek2", headAfterReorder.lease);

  // Cancel Order B (Glaxion in Zone 00): shared Glaxion task must roll back losslessly to Order A's exact Zone 03 position!
  const glaxionQid = headAfterReorder.questions[3].question;
  const orderAItem4Pos = orderA.order_base + 4;
  const cancelledB = await cancelBoardOrder(env, { order_id: "order-zone00-glaxion" }, baseNow + 6000);
  assert.equal(cancelledB.ok && cancelledB.done, true);
  const rolledBack = await db.get("SELECT zone, zone_position, state FROM state_compute_work WHERE id = ?", glaxionQid);
  assert.equal(rolledBack.state, "open", "shared task remains open for Order A");
  assert.equal(rolledBack.zone, 3, "shared task demotes back to Order A's Zone 03");
  assert.equal(rolledBack.zone_position, orderAItem4Pos, "shared task snaps back to its exact ordinal position inside Order A");

  // Rezone Order A from Zone 03 to Zone 01, then update its reason.
  const rezonedA = await rezoneBoardOrder(env, { order_id: "order-zone03-beams", zone: 1 }, baseNow + 7000);
  assert.equal(rezonedA.ok && rezonedA.done && rezonedA.zone === 1, true);
  const glaxionInZ1 = await db.get("SELECT zone, zone_position FROM state_compute_work WHERE id = ?", glaxionQid);
  assert.equal(glaxionInZ1.zone, 1);
  assert.equal(glaxionInZ1.zone_position, rezonedA.order_base + 4);
  assert.equal((await updateBoardOrderReason(env, { order_id: "order-zone03-beams", why: "moved to zone 01" }, baseNow + 7500)).ok, true);

  // 5. Two-witness verification: old score remains until 2nd witness agrees, then overwrites in place and settles BOTH sharing orders!
  const pack1 = await f.nextPack(env, witness("vol1"), "eng-new", { n: 3, lease_ms: 60_000 }, baseNow + 8000);
  assert.equal(pack1.questions.length, 3, "takes all 3 Torid tasks in Zone 00");
  await f.answerPack(env, witness("vol1"), {
    lease: pack1.lease,
    engine: "eng-new",
    done: true,
    answers: pack1.questions.map((q, idx) => ({
      question: q.question,
      result: { score: [410.5, 145.25, 72.0][idx], metric: "kpm" },
      work: 5_000_000,
    })),
  }, baseNow + 9000);

  // After only 1 witness, published score on board MUST still be the old baseline (340.0)!
  const midScore = await db.get(
    "SELECT score, measured_by FROM state_board_measurement_selection WHERE identity = ? AND ruler = 'standard_multi_target' AND mode = 'incarnon'",
    bToridTop,
  );
  assert.equal(midScore.score, 340.0, "old score stays intact until second independent witness agrees");
  assert.equal(midScore.measured_by, "verified:eng-old");

  // Second independent witness confirms!
  const pack2 = await f.nextPack(env, witness("vol2"), "eng-new", { n: 3, lease_ms: 60_000 }, baseNow + 10_000);
  assert.equal(pack2.questions.length, 3);
  await f.answerPack(env, witness("vol2"), {
    lease: pack2.lease,
    engine: "eng-new",
    done: true,
    answers: pack2.questions.map((q, idx) => ({
      question: q.question,
      result: { score: [410.5, 145.25, 72.0][idx], metric: "kpm" },
      work: 5_000_000,
    })),
  }, baseNow + 11_000);

  // Now scores overwrite in place, Order C auto-completes (3/3 settled), and Order A is 3/4 settled!
  const newMulti = await db.get(
    "SELECT score, measured_by, publication_zone FROM state_board_measurement_selection WHERE identity = ? AND ruler = 'standard_multi_target' AND mode = 'incarnon'",
    bToridTop,
  );
  assert.equal(newMulti.score, 410.5);
  assert.equal(newMulti.measured_by, "verified:eng-new");
  assert.equal(newMulti.publication_zone, 0);

  const inspected = await inspectQueueAndOrders(env, baseNow + 12_000);
  const activeA = inspected.orders.find((o) => o.id === "order-zone03-beams");
  assert(activeA, "Order A remains active with 1 Glaxion item left");
  assert.equal(activeA.settled_items, 3, "shared Torid tasks settled Order A's 3 Torid items at the same time");
  assert.equal(activeA.queued_items, 1);
  assert.equal(activeA.zone, 1);
  const endedC = inspected.ended.find((o) => o.id === "order-zone00-torid");
  assert(endedC && endedC.state === "completed", "Order C automatically transitioned to completed");
  assert.equal(BOARD_ORDER_CATALOG.weapons.length > 500, true);

  // 6. Protocol 8 syncRoute (including WFSIM_SYNC_PROC=1 stored procedure) leases and settles remaining Order A item.
  const accounts = await (await import("./pg/test_db.mjs")).testDb("accounts");
  try {
    forgetWitnesses();
    const syncEnv = {
      LIBRARY: db,
      ACCOUNTS: accounts,
      FACTORY_TOKEN: "secret-token",
      ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "eng-new", release: "rel-1" })) },
    };
    const devA = "a".repeat(24), devB = "b".repeat(24);
    const callSync = async (verifier, ip, extra = {}) => (await syncRoute(
      new Request("https://x/api/board/sync", {
        method: "POST",
        headers: { "cf-connecting-ip": ip },
        body: JSON.stringify({
          verifier,
          engine: "eng-new",
          protocol: PROTOCOL,
          consent: { v: 1, at: "2026-10-08T08:00:00.000Z" },
          lanes: 4,
          want_ms: 600_000,
          ...extra,
        }),
      }),
      syncEnv,
    )).json();

    const s1 = await callSync(devA, "203.0.113.10");
    assert.equal(s1.pack?.tasks?.length, 1, "Protocol 8 leases remaining Order A Glaxion item");
    assert.equal(s1.pack.tasks[0].record.weapon, "tenet_glaxion");
    await db.run("UPDATE entity_compute_devices SET next_at = NULL WHERE id = ?", devA);
    await callSync(devA, "203.0.113.10", {
      results: [{ lease: s1.pack.lease, question: s1.pack.tasks[0].question, score: 108.5, metric: "kpm", work: 5_000_000, compute_ms: 800 }],
    });

    const s2 = await callSync(devB, "198.51.100.20");
    assert.equal(s2.pack?.tasks?.length, 1);
    await db.run("UPDATE entity_compute_devices SET next_at = NULL WHERE id = ?", devB);
    await callSync(devB, "198.51.100.20", {
      results: [{ lease: s2.pack.lease, question: s2.pack.tasks[0].question, score: 108.5, metric: "kpm", work: 5_000_000, compute_ms: 800 }],
    });

    const glaxScore = await db.get(
      "SELECT score, measured_by, publication_zone FROM state_board_measurement_selection WHERE identity = ? AND ruler = 'standard_single_target' AND mode = 'base'",
      bGlaxion,
    );
    assert.equal(glaxScore.score, 108.5);
    assert.equal(glaxScore.measured_by, "verified:eng-new");
    assert.equal(glaxScore.publication_zone, 1);

    const finalInspect = await inspectQueueAndOrders(env, baseNow + 20_000);
    assert.equal(finalInspect.orders.length, 0, "Order A completed after Protocol 8 settlement");
    assert(finalInspect.ended.some((o) => o.id === "order-zone03-beams" && o.state === "completed"));
  } finally {
    await accounts.close();
  }

  console.log("board orders: planner, popularity sort, zone merge, in-zone reorder, cross-zone move, lossless cancel rollback and 2-witness rescore overwrite pass");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { testDb } = await import("./pg/test_db.mjs");
  const db = await testDb("library");
  try {
    await boardOrderCases(db);
  } finally {
    await db.close();
  }
}
