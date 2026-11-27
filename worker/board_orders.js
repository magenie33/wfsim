// SPDX-License-Identifier: AGPL-3.0-or-later
// Queue controller and consumer callbacks for explicit board orders: admit,
// reorder within a zone, move across zones 00..99, cancel and inspect.
import { iso } from "./instant.js";
import { facts, DEADLINE_MS, factoryAuthed } from "./factory.js";
import { servedRelease } from "./verify.js";
import { scoreWitnessWrites } from "./computation_records.js";
import { BOARD_KINDS } from "./board_kinds.js";
import { BOARD_ORDER_CATALOG } from "./board_order_catalog.js";
import { catalogSummary, planBoardOrder } from "./board_order_planner.js";

export const ORDER_POSITION_GAP = 10_000_000;
export const DEFAULT_ADMIT_SLICE = 1_000;
export const DEFAULT_MUTATE_SLICE = 2_000;
const ORDER_ID = /^[a-z0-9][a-z0-9_-]{1,95}$/;

const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" },
});

export const boardOrderRef = (orderId, ordinal) => `${orderId}|${ordinal}`;

export const BOARD_ORDER_PRODUCER = {
  wanted_in_sql: true,
  still_wanted: async (env, d) => !!(await env.LIBRARY.prepare("SELECT board_order_wanted(?1::text, ?2::text) AS x")
    .bind(d.ref, d.question).first()).x,
  wanted_many: async (env, ds) => {
    const { results } = await env.LIBRARY.prepare(
      `SELECT x.ref FROM jsonb_to_recordset((?1::text)::jsonb) AS x(ref text, question text)
       WHERE board_order_wanted(x.ref, x.question)`
    ).bind(JSON.stringify(ds.map((d) => ({ ref: d.ref, question: d.question })))).all();
    return new Set(results.map((r) => r.ref));
  },
  on_fact_writes: (env, d, fact) => {
    if (!fact.result) return [];
    const { order_id, ordinal, identity, ruler, mode } = d.payload;
    const at = fact.at || iso(Date.now()), db = env.LIBRARY;
    const measurement = fact.official && fact.result.measurement;
    const native = measurement && measurement.measured_by === fact.engine
      && Number.isFinite(measurement.cost_seconds) && measurement.cost_seconds >= 0
      && Number.isFinite(Date.parse(measurement.started_at)) && Number.isFinite(Date.parse(measurement.finished_at))
      && measurement.started_at <= measurement.finished_at;
    const measuredBy = native ? measurement.measured_by : `verified:${fact.engine}`;
    const startedAt = native ? measurement.started_at : at;
    const finishedAt = native ? measurement.finished_at : at;
    const costSeconds = native ? measurement.cost_seconds : 0;
    return [
      db.prepare(
        `UPDATE entity_measurement_reviews SET state = 'succeeded', result_engine = ?1::text, completed_at = ?2::text
         WHERE identity = ?3::text AND ruler = ?4::text AND mode = ?5::text AND state = 'waiting'`
      ).bind(fact.engine, finishedAt, identity, ruler, mode),
      db.prepare(
        `INSERT INTO state_board_measurement_selection (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at, publication_zone)
         VALUES (?1::text, ?2::text, ?3::text, ?4::text, ?5::double precision, ?6::text, ?7::double precision, ?8::text, ?9::text, ?10::smallint)
         ON CONFLICT (identity, ruler, mode) DO UPDATE SET
           measured_by = excluded.measured_by, score = excluded.score, metric = excluded.metric,
           cost_seconds = excluded.cost_seconds, started_at = excluded.started_at, finished_at = excluded.finished_at,
           review_id = NULL, publication_zone = excluded.publication_zone
         WHERE state_board_measurement_selection.finished_at <= excluded.finished_at`
      ).bind(identity, ruler, mode, measuredBy, fact.result.score, fact.result.metric, costSeconds, startedAt, finishedAt, fact.zone ?? 0),
      ...scoreWitnessWrites(db, identity, ruler, mode, at, fact),
      db.prepare("DELETE FROM state_board_pending_work WHERE build_id = ?1::text AND ruler = ?2::text AND mode = ?3::text")
        .bind(identity, ruler, mode),
      db.prepare(
        `UPDATE relation_board_order_items SET state = 'settled', settled_at = ?3::text
         WHERE order_id = ?1::text AND ordinal = ?2::bigint AND state IN ('pending', 'queued')`
      ).bind(order_id, ordinal, finishedAt),
      db.prepare(
        `UPDATE entity_board_orders SET state = 'completed', updated_at = ?2::text
         WHERE id = ?1::text AND state = 'active'
           AND admitted_items >= total_items
           AND NOT EXISTS (SELECT 1 FROM relation_board_order_items WHERE order_id = ?1::text AND state IN ('pending', 'queued'))`
      ).bind(order_id, finishedAt),
    ];
  },
  on_retract: async (env, d) => {
    const { order_id, ordinal } = d.payload, at = iso(Date.now());
    await env.LIBRARY.batch([
      env.LIBRARY.prepare(
        `UPDATE relation_board_order_items SET state = 'queued', settled_at = NULL
         WHERE order_id = ?1::text AND ordinal = ?2::bigint AND state = 'settled'
           AND EXISTS (SELECT 1 FROM entity_board_orders WHERE id = ?1::text AND state IN ('active', 'completed'))`
      ).bind(order_id, ordinal),
      env.LIBRARY.prepare(
        `UPDATE entity_board_orders SET state = 'active', updated_at = ?2::text
         WHERE id = ?1::text AND state = 'completed'
           AND EXISTS (SELECT 1 FROM relation_board_order_items WHERE order_id = ?1::text AND state = 'queued')`
      ).bind(order_id, at),
    ]);
  },
};

function validateZone(zone) {
  const z = Number(zone);
  if (!Number.isInteger(z) || z < 0 || z > 99) throw new Error("bad_zone");
  return z;
}

function validateReason(why) {
  const text = typeof why === "string" ? why.trim() : "";
  if (!text || text.length > 300) throw new Error("bad_why");
  return text;
}

function defaultOrderId(spec, zone, now) {
  const day = iso(now).slice(0, 10).replace(/-/g, "");
  const label = spec.tags[0] || spec.classes[0] || (spec.weapons.length === 1 ? spec.weapons[0] : spec.policy);
  const clean = String(label || "order").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 24) || "order";
  const rand = crypto.randomUUID().slice(0, 6);
  return `order-${day}-z${String(zone).padStart(2, "0")}-${clean}-${rand}`;
}

async function advanceQueueSequencePast(db, ceiling) {
  if (!Number.isSafeInteger(ceiling) || ceiling <= 0) return;
  await db.prepare(
    `SELECT setval('compute_queue_position',
       GREATEST((SELECT last_value FROM compute_queue_position), ?1::bigint), true)`
  ).bind(ceiling).run();
}

async function allocateZoneOrderBase(db, zone, totalItems, placement = "tail") {
  const count = Math.max(1, totalItems);
  const bounds = await db.prepare(
    `SELECT
       LEAST(
         (SELECT min(coalesce(s.zone_position, s.task_id)) FROM state_compute_scheduling s
          WHERE s.state = 'open' AND coalesce(s.zone, factory_default_zone(s.producer)) = ?1::bigint),
         (SELECT min(o.order_base + 1) FROM entity_board_orders o
          WHERE o.zone = ?1::bigint AND o.state IN ('admitting', 'active'))
       ) AS min_pos,
       GREATEST(
         (SELECT max(coalesce(s.zone_position, s.task_id)) FROM state_compute_scheduling s
          WHERE s.state = 'open' AND coalesce(s.zone, factory_default_zone(s.producer)) = ?1::bigint),
         (SELECT max(o.order_base + GREATEST(o.total_items, 1)) FROM entity_board_orders o
          WHERE o.zone = ?1::bigint AND o.state IN ('admitting', 'active')),
         nextval('compute_queue_position')
       ) AS max_pos`
  ).bind(zone).first();

  if (placement === "head") {
    const anchor = bounds?.min_pos != null ? Number(bounds.min_pos) : Number(bounds?.max_pos || 1);
    return anchor - count - ORDER_POSITION_GAP;
  }
  const base = Number(bounds?.max_pos || 0) + ORDER_POSITION_GAP;
  await advanceQueueSequencePast(db, base + count + ORDER_POSITION_GAP);
  return base;
}

async function resolveEngine(env, explicitEngine) {
  if (typeof explicitEngine === "string" && explicitEngine.trim()) return explicitEngine.trim();
  const served = await servedRelease(env);
  if (served?.engine) return served.engine;
  return "local";
}

export async function previewBoardOrder(env, body = {}, options = {}) {
  const zone = body.zone === undefined ? 0 : validateZone(body.zone);
  const engine = await resolveEngine(env, body.engine || options.engine);
  const planned = await planBoardOrder(env.LIBRARY, body.specification || body, {
    engine,
    catalog: options.catalog || BOARD_ORDER_CATALOG,
    previewLimit: Number.isInteger(body.preview_limit) ? body.preview_limit : 25,
  });
  return {
    ok: true,
    zone,
    engine,
    specification: planned.specification,
    summary: planned.summary,
  };
}

export async function admitBoardOrderSlice(env, { order_id, limit = DEFAULT_ADMIT_SLICE } = {}, now = Date.now()) {
  if (!ORDER_ID.test(String(order_id || ""))) throw new Error("bad_order_id");
  const db = env.LIBRARY, at = iso(now);
  const sliceSize = Math.max(1, Math.min(2_500, Number(limit) || DEFAULT_ADMIT_SLICE));
  const order = await db.prepare("SELECT * FROM entity_board_orders WHERE id = ?1::text").bind(order_id).first();
  if (!order) throw new Error("no_order");
  if (order.state === "cancelled" || order.state === "completed") {
    return { ok: true, order_id, state: order.state, admitted: 0, admitted_items: Number(order.admitted_items), total_items: Number(order.total_items), left: 0, done: true };
  }

  const { results: pending } = await db.prepare(
    `SELECT i.ordinal, i.computation, i.identity, i.ruler, i.mode, i.force_recalculation, b.record
     FROM relation_board_order_items i
     JOIN entity_weapon_builds b ON b.id = i.identity
     WHERE i.order_id = ?1::text AND i.state = 'pending'
     ORDER BY i.ordinal ASC
     LIMIT ?2::bigint`
  ).bind(order_id, sliceSize).all();

  if (!pending.length) {
    const remaining = await db.prepare(
      "SELECT count(*) AS n FROM relation_board_order_items WHERE order_id = ?1::text AND state IN ('pending', 'queued')"
    ).bind(order_id).first();
    const nextState = Number(remaining?.n || 0) === 0 ? "completed" : "active";
    if (order.state !== nextState) {
      await db.prepare("UPDATE entity_board_orders SET state = ?2::text, updated_at = ?3::text WHERE id = ?1::text AND state <> 'cancelled'")
        .bind(order_id, nextState, at).run();
    }
    return { ok: true, order_id, state: nextState, admitted: 0, admitted_items: Number(order.admitted_items), total_items: Number(order.total_items), left: 0, done: true };
  }

  const zone = Number(order.zone), orderBase = Number(order.order_base);
  for (let i = 0; i < pending.length; i += 250) {
    const chunk = pending.slice(i, i + 250);
    const asks = chunk.map((item) => ({
      producer: "board_order",
      ref: boardOrderRef(order_id, item.ordinal),
      trigger_id: boardOrderRef(order_id, item.ordinal),
      verb: "simulate",
      request: { record: JSON.parse(item.record), ruler: item.ruler, mode: item.mode },
      due_in_ms: DEADLINE_MS.board_order,
      zone,
      position: orderBase + Number(item.ordinal),
      force: !!item.force_recalculation,
      reason: "requested_recalculation",
      initiator: "system",
      payload: {
        order_id,
        ordinal: Number(item.ordinal),
        identity: item.identity,
        ruler: item.ruler,
        mode: item.mode,
      },
    }));
    const ordinalsJson = JSON.stringify(chunk.map((item) => Number(item.ordinal)));
    await facts().demandAll(env, asks, order.engine, now, {
      after: [
        db.prepare(
          `UPDATE relation_board_order_items SET state = 'queued'
           WHERE order_id = ?1::text AND state = 'pending'
             AND ordinal IN (SELECT value::bigint FROM jsonb_array_elements_text((?2::text)::jsonb))`
        ).bind(order_id, ordinalsJson),
        db.prepare(
          `UPDATE entity_board_orders SET
             admitted_items = (SELECT count(*) FROM relation_board_order_items WHERE order_id = ?1::text AND state IN ('queued', 'settled')),
             state = CASE
               WHEN state = 'cancelled' THEN 'cancelled'
               WHEN NOT EXISTS (SELECT 1 FROM relation_board_order_items WHERE order_id = ?1::text AND state = 'pending') THEN 'active'
               ELSE 'admitting'
             END,
             updated_at = ?2::text
           WHERE id = ?1::text`
        ).bind(order_id, at),
      ],
    });
  }

  await db.prepare(
    `UPDATE entity_board_orders SET state = 'completed', updated_at = ?2::text
     WHERE id = ?1::text AND state = 'active'
       AND admitted_items >= total_items
       AND NOT EXISTS (SELECT 1 FROM relation_board_order_items WHERE order_id = ?1::text AND state IN ('pending', 'queued'))`
  ).bind(order_id, at).run();

  const updated = await db.prepare(
    `SELECT o.state, o.admitted_items, o.total_items,
       (SELECT count(*) FROM relation_board_order_items i WHERE i.order_id = o.id AND i.state = 'pending') AS left_pending
     FROM entity_board_orders o WHERE o.id = ?1::text`
  ).bind(order_id).first();

  const left = Number(updated?.left_pending || 0);
  return {
    ok: true,
    order_id,
    state: updated.state,
    admitted: pending.length,
    admitted_items: Number(updated.admitted_items),
    total_items: Number(updated.total_items),
    left,
    done: left === 0,
  };
}

export async function createBoardOrder(env, body = {}, now = Date.now(), options = {}) {
  const db = env.LIBRARY, at = iso(now);
  const zone = validateZone(body.zone ?? 0);
  const why = validateReason(body.why);
  const placement = body.placement === "head" ? "head" : "tail";
  const engine = await resolveEngine(env, body.engine || options.engine);

  const planned = await planBoardOrder(db, body.specification || body, {
    engine,
    catalog: options.catalog || BOARD_ORDER_CATALOG,
    previewLimit: 10,
  });
  if (!planned.items.length) {
    return { ok: false, reason: "empty_order", summary: planned.summary };
  }

  const orderId = body.order_id ? String(body.order_id).trim() : defaultOrderId(planned.specification, zone, now);
  if (!ORDER_ID.test(orderId)) throw new Error("bad_order_id");
  const existing = await db.prepare("SELECT id FROM entity_board_orders WHERE id = ?1::text").bind(orderId).first();
  if (existing) return { ok: false, reason: "duplicate_order_id" };

  const orderBase = await allocateZoneOrderBase(db, zone, planned.items.length, placement);
  await db.prepare(
    `INSERT INTO entity_board_orders
       (id, why, policy, zone, order_base, engine, specification, total_items, admitted_items, state, created_at, updated_at)
     VALUES (?1::text, ?2::text, ?3::text, ?4::bigint, ?5::bigint, ?6::text, ?7::text, ?8::bigint, 0, 'admitting', ?9::text, ?9::text)`
  ).bind(
    orderId,
    why,
    planned.specification.policy,
    zone,
    orderBase,
    engine,
    JSON.stringify(planned.specification),
    planned.items.length,
    at,
  ).run();

  const INSERT_SLICE = 1_000;
  for (let i = 0; i < planned.items.length; i += INSERT_SLICE) {
    const chunk = planned.items.slice(i, i + INSERT_SLICE).map(({ ordinal, computation, identity, ruler, mode, force_recalculation }) => ({
      ordinal,
      computation,
      identity,
      ruler,
      mode,
      force_recalculation,
    }));
    await db.prepare(
      `INSERT INTO relation_board_order_items (order_id, ordinal, computation, identity, ruler, mode, force_recalculation, state)
       SELECT ?1::text, x.ordinal, x.computation, x.identity, x.ruler, x.mode, x.force_recalculation, 'pending'
       FROM jsonb_to_recordset((?2::text)::jsonb) AS x(
         ordinal bigint, computation text, identity text, ruler text, mode text, force_recalculation boolean
       )`
    ).bind(orderId, JSON.stringify(chunk)).run();
  }

  const initialAdmit = await admitBoardOrderSlice(env, {
    order_id: orderId,
    limit: body.admit_limit ?? DEFAULT_ADMIT_SLICE,
  }, now);

  return {
    ok: true,
    order_id: orderId,
    zone,
    order_base: orderBase,
    engine,
    summary: planned.summary,
    ...initialAdmit,
  };
}

export async function syncBoardOrderPositions(env, { order_id, limit = DEFAULT_MUTATE_SLICE } = {}, now = Date.now()) {
  if (!ORDER_ID.test(String(order_id || ""))) throw new Error("bad_order_id");
  const db = env.LIBRARY, at = iso(now);
  const sliceSize = Math.max(1, Math.min(5_000, Number(limit) || DEFAULT_MUTATE_SLICE));
  const order = await db.prepare("SELECT * FROM entity_board_orders WHERE id = ?1::text").bind(order_id).first();
  if (!order) throw new Error("no_order");

  const { results: movedRows } = await db.prepare(
    `WITH targets AS (
       SELECT d.producer, d.ref, d.question, ?2::bigint AS target_zone, (?3::bigint + i.ordinal) AS target_pos
       FROM relation_board_order_items i
       JOIN relation_task_consumers d ON d.producer = 'board_order' AND d.ref = (?1::text || '|' || i.ordinal::text)
       WHERE i.order_id = ?1::text AND i.state = 'queued' AND d.state = 'live'
         AND (d.zone IS DISTINCT FROM ?2::bigint OR d.zone_position IS DISTINCT FROM (?3::bigint + i.ordinal))
       ORDER BY i.ordinal ASC
       LIMIT ?4::bigint
       FOR UPDATE OF d SKIP LOCKED
     )
     UPDATE relation_task_consumers d
     SET zone = t.target_zone, zone_position = t.target_pos
     FROM targets t
     WHERE d.producer = t.producer AND d.ref = t.ref
     RETURNING d.question`
  ).bind(order_id, Number(order.zone), Number(order.order_base), sliceSize).all();

  const questions = [...new Set(movedRows.map((r) => r.question))].sort();
  if (questions.length) {
    await db.prepare(
      `SELECT factory_restate(g.id) FROM (
         SELECT id FROM state_compute_scheduling
         WHERE id IN (SELECT jsonb_array_elements_text((?1::text)::jsonb))
         ORDER BY id FOR UPDATE SKIP LOCKED
       ) g`
    ).bind(JSON.stringify(questions)).all();
  }

  const remaining = await db.prepare(
    `SELECT count(*) AS n
     FROM relation_board_order_items i
     JOIN relation_task_consumers d ON d.producer = 'board_order' AND d.ref = (?1::text || '|' || i.ordinal::text)
     WHERE i.order_id = ?1::text AND i.state = 'queued' AND d.state = 'live'
       AND (d.zone IS DISTINCT FROM ?2::bigint OR d.zone_position IS DISTINCT FROM (?3::bigint + i.ordinal))`
  ).bind(order_id, Number(order.zone), Number(order.order_base)).first();

  await db.prepare("UPDATE entity_board_orders SET updated_at = ?2::text WHERE id = ?1::text").bind(order_id, at).run();
  const left = Number(remaining?.n || 0);
  return {
    ok: true,
    order_id,
    zone: Number(order.zone),
    order_base: Number(order.order_base),
    moved: movedRows.length,
    left,
    done: left === 0,
  };
}

export async function rezoneBoardOrder(env, body = {}, now = Date.now()) {
  const orderId = String(body.order_id || "").trim();
  if (!ORDER_ID.test(orderId)) throw new Error("bad_order_id");
  const targetZone = validateZone(body.zone);
  const placement = body.placement === "head" ? "head" : "tail";
  const db = env.LIBRARY, at = iso(now);

  const order = await db.prepare("SELECT * FROM entity_board_orders WHERE id = ?1::text").bind(orderId).first();
  if (!order) return { ok: false, reason: "no_order" };
  if (order.state === "cancelled" || order.state === "completed") return { ok: false, reason: "order_closed" };

  const newBase = await allocateZoneOrderBase(db, targetZone, Number(order.total_items), placement);
  await db.prepare(
    "UPDATE entity_board_orders SET zone = ?2::bigint, order_base = ?3::bigint, updated_at = ?4::text WHERE id = ?1::text"
  ).bind(orderId, targetZone, newBase, at).run();

  return syncBoardOrderPositions(env, { order_id: orderId, limit: body.limit ?? DEFAULT_MUTATE_SLICE }, now);
}

export async function reorderBoardOrder(env, body = {}, now = Date.now()) {
  const orderId = String(body.order_id || "").trim();
  if (!ORDER_ID.test(orderId)) throw new Error("bad_order_id");
  const db = env.LIBRARY, at = iso(now);
  const order = await db.prepare("SELECT * FROM entity_board_orders WHERE id = ?1::text").bind(orderId).first();
  if (!order) return { ok: false, reason: "no_order" };
  if (order.state === "cancelled" || order.state === "completed") return { ok: false, reason: "order_closed" };

  const zone = Number(order.zone);
  const count = Math.max(1, Number(order.total_items));
  const { results: siblings } = await db.prepare(
    `SELECT id, order_base, total_items FROM entity_board_orders
     WHERE zone = ?1::bigint AND state IN ('admitting', 'active') AND id <> ?2::text
     ORDER BY order_base ASC, id ASC`
  ).bind(zone, orderId).all();

  if (!siblings.length && body.placement !== "head" && body.placement !== "tail") {
    return syncBoardOrderPositions(env, { order_id: orderId, limit: body.limit ?? DEFAULT_MUTATE_SLICE }, now);
  }

  let insertIndex = siblings.length;
  if (body.placement === "head") {
    insertIndex = 0;
  } else if (body.placement === "tail") {
    insertIndex = siblings.length;
  } else if (body.before_order_id) {
    const idx = siblings.findIndex((s) => s.id === body.before_order_id);
    if (idx < 0) return { ok: false, reason: "no_target_order" };
    insertIndex = idx;
  } else if (body.after_order_id) {
    const idx = siblings.findIndex((s) => s.id === body.after_order_id);
    if (idx < 0) return { ok: false, reason: "no_target_order" };
    insertIndex = idx + 1;
  } else {
    return { ok: false, reason: "bad_reorder_target" };
  }

  const prev = insertIndex > 0 ? siblings[insertIndex - 1] : null;
  const next = insertIndex < siblings.length ? siblings[insertIndex] : null;

  let newBase = null;
  const rebalancedIds = [];

  if (!prev && !next) {
    newBase = await allocateZoneOrderBase(db, zone, count, body.placement === "head" ? "head" : "tail");
  } else if (!prev && next) {
    const nextStart = Number(next.order_base) + 1;
    const minNonOrder = await db.prepare(
      `SELECT min(coalesce(s.zone_position, s.task_id)) AS m FROM state_compute_scheduling s
       WHERE s.state = 'open' AND coalesce(s.zone, factory_default_zone(s.producer)) = ?1::bigint
         AND s.producer <> 'board_order'`
    ).bind(zone).first();
    const ceiling = body.placement === "head" && minNonOrder?.m != null
      ? Math.min(nextStart, Number(minNonOrder.m))
      : nextStart;
    newBase = ceiling - count - ORDER_POSITION_GAP;
  } else if (prev && !next) {
    const prevEnd = Number(prev.order_base) + Math.max(1, Number(prev.total_items));
    const maxNonOrder = await db.prepare(
      `SELECT max(coalesce(s.zone_position, s.task_id)) AS m FROM state_compute_scheduling s
       WHERE s.state = 'open' AND coalesce(s.zone, factory_default_zone(s.producer)) = ?1::bigint`
    ).bind(zone).first();
    const floor = body.placement === "tail" && maxNonOrder?.m != null
      ? Math.max(prevEnd, Number(maxNonOrder.m))
      : prevEnd;
    newBase = floor + ORDER_POSITION_GAP;
    await advanceQueueSequencePast(db, newBase + count + ORDER_POSITION_GAP);
  } else {
    const prevEnd = Number(prev.order_base) + Math.max(1, Number(prev.total_items));
    const nextStart = Number(next.order_base) + 1;
    const gap = nextStart - prevEnd - 1;
    if (gap >= count + 2) {
      newBase = prevEnd + Math.floor((gap - count) / 2);
    } else {
      const ordered = [...siblings.slice(0, insertIndex), { id: orderId, total_items: count }, ...siblings.slice(insertIndex)];
      let cursor = Number(ordered[0].id === orderId ? prev.order_base : ordered[0].order_base);
      for (const entry of ordered) {
        const len = Math.max(1, Number(entry.total_items));
        await db.prepare("UPDATE entity_board_orders SET order_base = ?2::bigint, updated_at = ?3::text WHERE id = ?1::text")
          .bind(entry.id, cursor, at).run();
        if (entry.id === orderId) newBase = cursor;
        else rebalancedIds.push(entry.id);
        cursor += len + ORDER_POSITION_GAP;
      }
      await advanceQueueSequencePast(db, cursor);
    }
  }

  if (!rebalancedIds.length) {
    await db.prepare("UPDATE entity_board_orders SET order_base = ?2::bigint, updated_at = ?3::text WHERE id = ?1::text")
      .bind(orderId, newBase, at).run();
  } else {
    for (const siblingId of rebalancedIds) {
      await syncBoardOrderPositions(env, { order_id: siblingId, limit: body.limit ?? DEFAULT_MUTATE_SLICE }, now);
    }
  }

  return syncBoardOrderPositions(env, { order_id: orderId, limit: body.limit ?? DEFAULT_MUTATE_SLICE }, now);
}

export async function cancelBoardOrder(env, body = {}, now = Date.now()) {
  const orderId = String(body.order_id || body.batch || "").trim();
  if (!ORDER_ID.test(orderId)) throw new Error("bad_order_id");
  const sliceSize = Math.max(1, Math.min(5_000, Number(body.limit) || DEFAULT_MUTATE_SLICE));
  const db = env.LIBRARY, at = iso(now);

  const order = await db.prepare("SELECT * FROM entity_board_orders WHERE id = ?1::text").bind(orderId).first();
  if (!order) return { ok: false, reason: "no_order" };

  if (order.state !== "cancelled") {
    await db.prepare("UPDATE entity_board_orders SET state = 'cancelled', updated_at = ?2::text WHERE id = ?1::text")
      .bind(orderId, at).run();
  }

  const { results: items } = await db.prepare(
    `WITH picked AS (
       SELECT order_id, ordinal FROM relation_board_order_items
       WHERE order_id = ?1::text AND state IN ('pending', 'queued')
       ORDER BY ordinal ASC
       LIMIT ?2::bigint
       FOR UPDATE SKIP LOCKED
     )
     UPDATE relation_board_order_items i
     SET state = 'cancelled'
     FROM picked p
     WHERE i.order_id = p.order_id AND i.ordinal = p.ordinal
     RETURNING i.ordinal, i.computation`
  ).bind(orderId, sliceSize).all();

  const refs = items.map((x) => boardOrderRef(orderId, x.ordinal));
  let withdrawn = 0;
  if (refs.length) {
    const res = await db.prepare(
      `UPDATE relation_task_consumers SET state = 'withdrawn', delivery_at = NULL
       WHERE producer = 'board_order' AND state = 'live'
         AND ref IN (SELECT jsonb_array_elements_text((?1::text)::jsonb))`
    ).bind(JSON.stringify(refs)).run();
    withdrawn = res.meta?.changes ?? 0;
  }

  const questions = [...new Set(items.map((x) => x.computation))].sort();
  if (questions.length) {
    await db.prepare(
      `SELECT factory_restate(g.id) FROM (
         SELECT id FROM state_compute_scheduling
         WHERE id IN (SELECT jsonb_array_elements_text((?1::text)::jsonb))
         ORDER BY id FOR UPDATE SKIP LOCKED
       ) g`
    ).bind(JSON.stringify(questions)).all();
  }

  const remaining = await db.prepare(
    `SELECT
       (SELECT count(*) FROM relation_board_order_items WHERE order_id = ?1::text AND state IN ('pending', 'queued')) AS items_left,
       (SELECT count(*) FROM relation_task_consumers
        WHERE producer = 'board_order' AND state = 'live' AND ref >= (?1::text || '|') AND ref < (?1::text || '}')) AS consumers_left`
  ).bind(orderId).first();

  if (Number(remaining?.items_left || 0) === 0 && Number(remaining?.consumers_left || 0) > 0) {
    const { results: stray } = await db.prepare(
      `WITH picked AS (
         SELECT producer, ref FROM relation_task_consumers
         WHERE producer = 'board_order' AND state = 'live'
           AND ref >= (?1::text || '|') AND ref < (?1::text || '}') AND split_part(ref, '|', 1) = ?1::text
         ORDER BY ref LIMIT ?2::bigint FOR UPDATE SKIP LOCKED
       )
       UPDATE relation_task_consumers d SET state = 'withdrawn', delivery_at = NULL
       FROM picked p WHERE d.producer = p.producer AND d.ref = p.ref
       RETURNING d.question`
    ).bind(orderId, sliceSize).all();
    const strayIds = [...new Set(stray.map((r) => r.question))].sort();
    if (strayIds.length) {
      await db.prepare("SELECT factory_restate(value) FROM jsonb_array_elements_text((?1::text)::jsonb) ORDER BY 1")
        .bind(JSON.stringify(strayIds)).all();
    }
  }

  const leftCheck = await db.prepare(
    `SELECT
       (SELECT count(*) FROM relation_board_order_items WHERE order_id = ?1::text AND state IN ('pending', 'queued')) +
       (SELECT count(*) FROM relation_task_consumers
        WHERE producer = 'board_order' AND state = 'live' AND ref >= (?1::text || '|') AND ref < (?1::text || '}') AND split_part(ref, '|', 1) = ?1::text) AS left`
  ).bind(orderId).first();
  const left = Number(leftCheck?.left || 0);
  return { ok: true, order_id: orderId, cancelled: items.length, withdrawn, left, done: left === 0 };
}

export async function updateBoardOrderReason(env, body = {}, now = Date.now()) {
  const id = String(body.order_id || body.batch || "").trim();
  if (!id) throw new Error("bad_order_id");
  const why = validateReason(body.why);
  const db = env.LIBRARY, at = iso(now);

  const updatedOrder = await db.prepare(
    "UPDATE entity_board_orders SET why = ?2::text, updated_at = ?3::text WHERE id = ?1::text"
  ).bind(id, why, at).run();
  if (updatedOrder.meta?.changes) return { ok: true, kind: "board_order" };

  const updatedBatch = await db.prepare(
    "UPDATE entity_compute_batches SET why = ?2::text WHERE id = ?1::text"
  ).bind(id, why).run();
  if (updatedBatch.meta?.changes) return { ok: true, kind: "legacy_batch" };
  return { ok: false, reason: "no_order" };
}

export async function inspectQueueAndOrders(env, now = Date.now(), options = {}) {
  const db = env.LIBRARY, at = iso(now);
  const kindsJson = JSON.stringify(BOARD_KINDS);
  const served = await servedRelease(env).catch(() => ({ engine: null }));

  const [
    zoneRows,
    activeOrders,
    orderItemCounts,
    orderOpenCounts,
    endedOrders,
    legacyDemands,
    legacyBatches,
    legacyPending,
    rulersMeasured,
  ] = (await db.batch([
    db.prepare(
      `SELECT
         coalesce(zone, factory_default_zone(producer)) AS zone,
         count(*) AS open,
         sum(CASE WHEN lease_until IS NOT NULL AND lease_until > ?1::text THEN 1 ELSE 0 END) AS leased,
         sum(CASE WHEN waits > 0 THEN 1 ELSE 0 END) AS waited,
         sum(CASE WHEN producer IN ('user_calculation', 'chat', 'survey') THEN 1 ELSE 0 END) AS interactive_open,
         sum(CASE WHEN producer = 'board_order' THEN 1 ELSE 0 END) AS order_open,
         sum(CASE WHEN producer = 'build_workflow' THEN 1 ELSE 0 END) AS workflow_open
       FROM state_compute_effective_open
       GROUP BY 1 ORDER BY 1 ASC`
    ).bind(at),
    db.prepare(
      `SELECT id, why, policy, zone, order_base, engine, specification, total_items, admitted_items, state, created_at, updated_at
       FROM entity_board_orders
       WHERE state IN ('admitting', 'active')
       ORDER BY zone ASC, order_base ASC, id ASC`
    ),
    db.prepare(
      `SELECT i.order_id, i.state, count(*) AS n
       FROM relation_board_order_items i
       JOIN entity_board_orders o ON o.id = i.order_id
       WHERE o.state IN ('admitting', 'active')
       GROUP BY 1, 2`
    ),
    db.prepare(
      `SELECT
         i.order_id,
         count(DISTINCT q.id) AS open_questions,
         sum(CASE WHEN d.zone IS DISTINCT FROM o.zone OR d.zone_position IS DISTINCT FROM (o.order_base + i.ordinal) THEN 1 ELSE 0 END) AS unsynced_items
       FROM entity_board_orders o
       JOIN relation_board_order_items i ON i.order_id = o.id AND i.state = 'queued'
       LEFT JOIN relation_task_consumers d ON d.producer = 'board_order' AND d.ref = (o.id || '|' || i.ordinal::text) AND d.state = 'live'
       LEFT JOIN state_compute_effective_open q ON q.id = i.computation
       WHERE o.state IN ('admitting', 'active')
       GROUP BY 1`
    ),
    db.prepare(
      `SELECT id, why, policy, zone, engine, total_items, state, created_at, updated_at
       FROM entity_board_orders
       WHERE state IN ('completed', 'cancelled')
       ORDER BY updated_at DESC, id DESC
       LIMIT 50`
    ),
    db.prepare(
      `SELECT
         coalesce(q.zone, factory_default_zone(q.producer)) AS zone,
         d.producer,
         CASE
           WHEN d.producer IN (SELECT jsonb_array_elements_text((?1::text)::jsonb)) THEN d.payload::jsonb ->> 'batch'
           WHEN d.producer = 'survey' THEN 'survey:' || (d.payload::jsonb ->> 'weapon') || ':' || (d.payload::jsonb ->> 'ruler')
           ELSE d.producer
         END AS key,
         count(DISTINCT q.id) AS open_questions
       FROM relation_task_consumers d
       JOIN state_compute_effective_open q ON q.id = d.question
       WHERE d.state = 'live' AND d.producer <> 'board_order'
       GROUP BY 1, 2, 3
       ORDER BY 1 ASC, 4 DESC`
    ).bind(kindsJson),
    db.prepare("SELECT id, at, why, total FROM entity_compute_batches ORDER BY at DESC, id DESC LIMIT 200"),
    db.prepare("SELECT batch, count(*) AS n FROM state_board_effective_pending_work GROUP BY 1"),
    db.prepare("SELECT ruler, count(*) AS n FROM state_board_measurement_selection GROUP BY 1 ORDER BY 1"),
  ])).map((r) => r.results);

  const countsByOrder = new Map();
  for (const row of orderItemCounts) {
    const entry = countsByOrder.get(row.order_id) || { pending: 0, queued: 0, settled: 0, cancelled: 0 };
    entry[row.state] = Number(row.n);
    countsByOrder.set(row.order_id, entry);
  }
  const openByOrder = new Map(orderOpenCounts.map((r) => [r.order_id, {
    open_questions: Number(r.open_questions || 0),
    unsynced_items: Number(r.unsynced_items || 0),
  }]));

  const formattedOrders = activeOrders.map((o) => {
    const c = countsByOrder.get(o.id) || { pending: 0, queued: 0, settled: 0, cancelled: 0 };
    const op = openByOrder.get(o.id) || { open_questions: 0, unsynced_items: 0 };
    return {
      id: o.id,
      why: o.why,
      policy: o.policy,
      zone: Number(o.zone),
      order_base: Number(o.order_base),
      engine: o.engine,
      specification: JSON.parse(o.specification),
      total_items: Number(o.total_items),
      admitted_items: Number(o.admitted_items),
      pending_items: c.pending,
      queued_items: c.queued,
      settled_items: c.settled,
      open_questions: op.open_questions,
      unsynced_items: op.unsynced_items,
      state: o.state,
      created_at: o.created_at,
      updated_at: o.updated_at,
    };
  });

  const batchMap = new Map(legacyBatches.map((b) => [b.id, b]));
  const owedMap = new Map(legacyPending.map((p) => [p.batch, Number(p.n)]));
  const legacyOrders = legacyDemands.map((d) => {
    const b = batchMap.get(d.key);
    return {
      zone: Number(d.zone),
      producer: d.producer,
      key: d.key,
      open_questions: Number(d.open_questions),
      why: b?.why || null,
      total: b ? Number(b.total) : null,
      left: b ? (owedMap.get(d.key) ?? 0) : null,
      cancellable: !!b && d.producer === "sweep",
    };
  });

  const zoneMap = new Map();
  const ensureZone = (z) => {
    const num = Number(z);
    if (!zoneMap.has(num)) {
      zoneMap.set(num, {
        zone: num,
        open: 0,
        leased: 0,
        waited: 0,
        interactive_open: 0,
        order_open: 0,
        workflow_open: 0,
        orders: [],
        legacy: [],
      });
    }
    return zoneMap.get(num);
  };

  for (const zr of zoneRows) {
    const entry = ensureZone(zr.zone);
    entry.open = Number(zr.open || 0);
    entry.leased = Number(zr.leased || 0);
    entry.waited = Number(zr.waited || 0);
    entry.interactive_open = Number(zr.interactive_open || 0);
    entry.order_open = Number(zr.order_open || 0);
    entry.workflow_open = Number(zr.workflow_open || 0);
  }
  for (const o of formattedOrders) ensureZone(o.zone).orders.push(o);
  for (const l of legacyOrders) ensureZone(l.zone).legacy.push(l);

  const zones = [...zoneMap.values()].sort((a, b) => a.zone - b.zone);
  const totalOpen = zoneRows.reduce((sum, r) => sum + Number(r.open || 0), 0);
  const totalWaited = zoneRows.reduce((sum, r) => sum + Number(r.waited || 0), 0);
  const totalLeased = zoneRows.reduce((sum, r) => sum + Number(r.leased || 0), 0);

  const liveLegacyKeys = new Set(legacyDemands.map((d) => d.key));
  const endedLegacy = legacyBatches
    .filter((b) => !liveLegacyKeys.has(b.id) && !(owedMap.get(b.id) > 0))
    .slice(0, 30)
    .map((b) => ({
      id: b.id,
      kind: "legacy_batch",
      why: b.why,
      policy: "sweep",
      zone: 3,
      engine: null,
      total_items: Number(b.total),
      state: "completed",
      created_at: b.at,
      updated_at: b.at,
    }));

  const ended = [
    ...endedOrders.map((o) => ({
      id: o.id,
      kind: "board_order",
      why: o.why,
      policy: o.policy,
      zone: Number(o.zone),
      engine: o.engine,
      total_items: Number(o.total_items),
      state: o.state,
      created_at: o.created_at,
      updated_at: o.updated_at,
    })),
    ...endedLegacy,
  ].sort((a, b) => (a.updated_at < b.updated_at ? 1 : a.updated_at > b.updated_at ? -1 : 0)).slice(0, 50);

  return {
    at,
    engine: served?.engine || null,
    totals: {
      open_questions: totalOpen,
      leased_questions: totalLeased,
      waited_questions: totalWaited,
      active_zones: zones.filter((z) => z.open > 0 || z.orders.length > 0).length,
      active_orders: formattedOrders.length,
    },
    zones,
    orders: formattedOrders,
    rulers_measured: rulersMeasured.map((r) => ({ ruler: r.ruler, count: Number(r.n) })),
    catalog: catalogSummary(options.catalog || BOARD_ORDER_CATALOG),
    ended,
  };
}

export async function boardOrdersRoute(request, env, path) {
  if (!path.startsWith("/api/board/orders/")) return null;
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  if (!factoryAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  if (!env.LIBRARY) return json({ ok: false, error: "unavailable" }, 503);
  let body = {};
  try { body = await request.json(); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  const now = Date.now();
  try {
    if (path === "/api/board/orders/list") return json({ ok: true, ...(await inspectQueueAndOrders(env, now)) });
    if (path === "/api/board/orders/preview") return json(await previewBoardOrder(env, body));
    if (path === "/api/board/orders/create") return json(await createBoardOrder(env, body, now));
    if (path === "/api/board/orders/admit") return json(await admitBoardOrderSlice(env, body, now));
    if (path === "/api/board/orders/rezone") return json(await rezoneBoardOrder(env, body, now));
    if (path === "/api/board/orders/reorder") return json(await reorderBoardOrder(env, body, now));
    if (path === "/api/board/orders/sync") return json(await syncBoardOrderPositions(env, body, now));
    if (path === "/api/board/orders/cancel") return json(await cancelBoardOrder(env, body, now));
    if (path === "/api/board/orders/why") return json(await updateBoardOrderReason(env, body, now));
    return json({ ok: false, error: "not found" }, 404);
  } catch (error) {
    return json({ ok: false, reason: error?.message || String(error) }, 400);
  }
}
