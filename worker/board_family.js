// SPDX-License-Identifier: AGPL-3.0-or-later
// New arrivals seed sibling searches; derived inbox records never seed another round.
import { facts, DEADLINE_MS, factoryAuthed } from "./factory.js";
import { questionId } from "./tasks.js";
import { json, servedRelease, ENGINE_ID } from "./verify.js";
import { iso } from "./instant.js";
import { submitRecord } from "./index.js";
import { headlessBoardPayloadFromResult } from "./board_payload.js";

const ID = /^[a-f0-9]{32}$/, SLUG = /^[a-z0-9_]{1,128}$/;
export const FAMILY_PRODUCERS = { family_build: {
  still_wanted: async (env, demand) => !!await env.LIBRARY.prepare("SELECT id FROM entity_family_search_requests WHERE id = ? AND state = 'waiting'")
    .bind(demand.ref).first(),
  on_fact: async (env, demand, fact) => {
    const row = await env.LIBRARY.prepare("SELECT * FROM entity_family_search_requests WHERE id = ? AND state = 'waiting' AND engine = ?")
      .bind(demand.ref, fact.engine).first();
    if (!row) return;
    const best = fact.result?.report?.results?.[0];
    if (!best) {
      await env.LIBRARY.prepare("UPDATE entity_family_search_requests SET state = 'failed', completed_at = ? WHERE id = ? AND state = 'waiting'")
        .bind(iso(Date.now()), row.id).run();
      return;
    }
    if (best.replay?.weapon !== row.weapon) throw new Error("family result names another weapon");
    const build = headlessBoardPayloadFromResult(best, JSON.parse(row.context));
    if (!await submitRecord(env, build, "system", { family_source: row.id, id: `family:${row.id}` }))
      throw new Error("family result is not a board record");
    await env.LIBRARY.prepare("UPDATE entity_family_search_requests SET state = 'succeeded', completed_at = ? WHERE id = ? AND state = 'waiting'")
      .bind(iso(Date.now()), row.id).run();
  },
} };

/// Resume only admitted transfer requests, including an ask interrupted after its
/// durable write. The source library is never scanned to create historical work.
export async function askFamilyTransfers(env) {
  if (!env.LIBRARY || env.FACTORY_PAUSED === "1") return;
  const { engine } = await servedRelease(env);
  if (!engine) return;
  const { results } = await env.LIBRARY.prepare(`SELECT t.* FROM entity_family_search_requests t
    WHERE t.state = 'waiting' AND (t.asked_engine IS DISTINCT FROM ?1::text OR NOT EXISTS
      (SELECT 1 FROM relation_task_consumers d WHERE d.producer = 'family_build' AND d.ref = t.id)) ORDER BY t.created_at, t.id LIMIT 100`)
    .bind(engine).all();
  if (!results.length) return;
  const keys = JSON.stringify(results.map(row => row.id).sort());
  const { results: waiting } = await env.LIBRARY.prepare(`UPDATE entity_family_search_requests SET engine = ?1::text
    WHERE id IN (SELECT id FROM entity_family_search_requests WHERE state = 'waiting'
      AND id IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb)) ORDER BY id FOR UPDATE) RETURNING id`)
    .bind(engine, keys).all();
  const rows = results.filter(row => waiting.some(item => item.id === row.id));
  if (!rows.length) return;
  await facts().demandAll(env, rows.map(row => ({ producer: "family_build", ref: row.id, verb: "optimize_report", request: JSON.parse(row.request),
    due_in_ms: DEADLINE_MS.family_build, payload: { weapon: row.weapon, ruler: row.ruler }, waits: false, initiator: "system" })), engine);
  await env.LIBRARY.prepare(`UPDATE entity_family_search_requests SET asked_engine = ?1::text WHERE id IN
    (SELECT id FROM entity_family_search_requests WHERE engine = ?1::text
      AND id IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb)) ORDER BY id FOR UPDATE)`)
    .bind(engine, JSON.stringify(rows.map(row => row.id))).run();
}

export async function familyRoute(request, env) {
  if (request.method !== "POST" || !factoryAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  if (!env.LIBRARY) return json({ ok: false, error: "The library is unavailable." }, 503);
  const raw = await request.text();
  if (raw.length > 1_048_576) return json({ ok: false, error: "payload too large" }, 413);
  let body;
  try { body = JSON.parse(raw); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if ((body?.engine !== undefined && (typeof body.engine !== "string" || !ENGINE_ID.test(body.engine)))
      || !Array.isArray(body?.jobs) || body.jobs.length > 100 || body.jobs.some(job => !job || !ID.test(job.source || "")
      || !SLUG.test(job.weapon || "") || !SLUG.test(job.ruler || "") || job.request?.weapon !== job.weapon
      || job.request.strategy !== "quick" || !Array.isArray(job.request.starts) || job.request.finalists !== 1
      || job.context?.weapon !== job.weapon || job.context.benchmark !== job.ruler))
    return json({ ok: false, error: "invalid family requests" }, 400);
  const { engine } = await servedRelease(env);
  if (!engine) return json({ ok: false, error: "No engine is served." }, 503);
  const wanted = body.engine || engine, rows = new Map();
  for (const job of body.jobs) {
    const id = await questionId("family_build", { source: job.source, weapon: job.weapon, ruler: job.ruler, engine: wanted });
    if (!rows.has(id)) rows.set(id, { id, source: job.source, weapon: job.weapon, ruler: job.ruler,
      request: JSON.stringify(job.request), context: JSON.stringify(job.context) });
  }
  if (!rows.size) return json({ ok: true, engine: wanted, accepted: 0 });
  const batch = JSON.stringify([...rows.values()].sort((a, b) => a.id.localeCompare(b.id)));
  if (wanted === engine) await env.LIBRARY.prepare(`INSERT INTO entity_family_search_requests
    (id, source, weapon, ruler, request, context, engine, state, created_at)
    SELECT x.id, x.source, x.weapon, x.ruler, x.request, x.context, ?2::text, 'waiting', ?3::text
    FROM jsonb_to_recordset((?1::text)::jsonb) AS x(id text, source text, weapon text, ruler text, request text, context text)
    WHERE EXISTS (SELECT 1 FROM entity_weapon_builds b WHERE b.id = x.source) ORDER BY x.id ON CONFLICT DO NOTHING`)
    .bind(batch, wanted, iso(Date.now())).run();
  const { results: stored } = await env.LIBRARY.prepare(`SELECT t.id FROM entity_family_search_requests t
    JOIN jsonb_to_recordset((?::text)::jsonb) AS x(id text) ON t.id = x.id`).bind(batch).all();
  if (stored.length !== rows.size) return json({ ok: false, error: wanted === engine
    ? "The source build has not landed." : "The served engine changed before admission." }, 409);
  return json({ ok: true, engine: wanted, accepted: rows.size });
}
