// SPDX-License-Identifier: AGPL-3.0-or-later
// New arrivals seed sibling searches; derived inbox records never seed another round.
import { facts, DEADLINE_MS, factoryAuthed } from "./factory.js";
import { json, servedRelease } from "./verify.js";
import { iso } from "./instant.js";
import { submitRecord } from "./index.js";
import { headlessBoardPayloadFromResult } from "./board_payload.js";

const ID = /^[a-f0-9]{32}$/, SLUG = /^[a-z0-9_]{1,128}$/;
export const FAMILY_PRODUCERS = { family_build: {
  still_wanted: async (env, demand) => !!await env.LIBRARY.prepare("SELECT id FROM entity_family_search_requests WHERE id = ? AND state = 'waiting'")
    .bind(demand.ref).first(),
  on_fact: async (env, demand, fact) => {
    const row = await env.LIBRARY.prepare("SELECT * FROM entity_family_search_requests WHERE id = ? AND state = 'waiting'")
      .bind(demand.ref).first();
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
  const { results } = await env.LIBRARY.prepare(`SELECT t.*,EXISTS(SELECT 1 FROM relation_task_consumers d WHERE d.producer='family_build' AND d.ref=t.id) AS already_asked FROM entity_family_search_requests t
    WHERE t.state = 'waiting' AND (t.asked_engine IS NULL OR NOT EXISTS
      (SELECT 1 FROM relation_task_consumers d WHERE d.producer = 'family_build' AND d.ref = t.id)) ORDER BY t.created_at, t.id LIMIT 100`)
    .all();
  if (!results.length) return;
  const rows = results;
  const missing = rows.filter(row => !row.already_asked);
  if (missing.length) await facts().demandAll(env, missing.map(row => ({ producer: "family_build", ref: row.id, verb: "optimize_report", request: JSON.parse(row.request),
    due_in_ms: DEADLINE_MS.family_build, payload: { weapon: row.weapon, ruler: row.ruler }, waits: false, initiator: "system" })), engine);
  await env.LIBRARY.prepare(`UPDATE entity_family_search_requests SET asked_engine = ?1::text WHERE id IN
    (SELECT id FROM entity_family_search_requests WHERE state='waiting'
      AND id IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb)) ORDER BY id FOR UPDATE)`)
    .bind(engine, JSON.stringify(rows.map(row => row.id))).run();
}

export async function familyRoute(request, env) {
  if (request.method !== "POST" || !factoryAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  return json({ok:true,accepted:0,disabled:true});
}
