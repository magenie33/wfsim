// SPDX-License-Identifier: AGPL-3.0-or-later
// New arrivals seed sibling searches; derived inbox records never seed another round.
import { facts, DEADLINE_MS, factoryAuthed } from "./factory.js";
import { questionId } from "./tasks.js";
import { json, servedRelease } from "./verify.js";
import { iso } from "./instant.js";
import { submitRecord } from "./index.js";
import { headlessBoardPayloadFromResult } from "./board_payload.js";

const ID = /^[a-f0-9]{32}$/, SLUG = /^[a-z0-9_]{1,128}$/;
export const FAMILY_PRODUCERS = { family_build: {
  still_wanted: async (env, demand) => !!await env.LIBRARY.prepare("SELECT id FROM family_transfers WHERE id = ? AND state = 'waiting'")
    .bind(demand.ref).first(),
  on_fact: async (env, demand, fact) => {
    const row = await env.LIBRARY.prepare("SELECT * FROM family_transfers WHERE id = ? AND state = 'waiting' AND engine = ?")
      .bind(demand.ref, fact.engine).first();
    if (!row) return;
    const best = fact.result?.report?.results?.[0];
    if (!best) {
      await env.LIBRARY.prepare("UPDATE family_transfers SET state = 'failed', completed_at = ? WHERE id = ? AND state = 'waiting'")
        .bind(iso(Date.now()), row.id).run();
      return;
    }
    if (best.replay?.weapon !== row.weapon) throw new Error("family result names another weapon");
    const build = headlessBoardPayloadFromResult(best, JSON.parse(row.context));
    if (!await submitRecord(env, build, "system", { family_source: row.id, id: `family:${row.id}` }))
      throw new Error("family result is not a board record");
    await env.LIBRARY.prepare("UPDATE family_transfers SET state = 'succeeded', completed_at = ? WHERE id = ? AND state = 'waiting'")
      .bind(iso(Date.now()), row.id).run();
  },
} };

/// Resume only admitted transfer requests, including an ask interrupted after its
/// durable write. The source library is never scanned to create historical work.
export async function askFamilyTransfers(env) {
  if (!env.LIBRARY || env.FACTORY_PAUSED === "1") return;
  const { engine } = await servedRelease(env);
  if (!engine) return;
  const { results } = await env.LIBRARY.prepare(`SELECT t.* FROM family_transfers t
    WHERE t.state = 'waiting' AND (t.asked_engine IS DISTINCT FROM ?1::text OR NOT EXISTS
      (SELECT 1 FROM demands d WHERE d.producer = 'family_build' AND d.ref = t.id)) ORDER BY t.created_at, t.id LIMIT 100`)
    .bind(engine).all();
  for (const row of results) {
    await env.LIBRARY.prepare("UPDATE family_transfers SET engine = ? WHERE id = ? AND state = 'waiting'").bind(engine, row.id).run();
    await facts().demand(env, { producer: "family_build", ref: row.id, verb: "optimize_report", request: JSON.parse(row.request),
      due_in_ms: DEADLINE_MS.family_build, payload: { weapon: row.weapon, ruler: row.ruler }, waits: false, initiator: "system" }, engine);
    await env.LIBRARY.prepare("UPDATE family_transfers SET asked_engine = ? WHERE id = ? AND engine = ?")
      .bind(engine, row.id, engine).run();
  }
}

export async function familyRoute(request, env) {
  if (request.method !== "POST" || !factoryAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  if (!env.LIBRARY) return json({ ok: false, error: "The library is unavailable." }, 503);
  const raw = await request.text();
  if (raw.length > 1_048_576) return json({ ok: false, error: "payload too large" }, 413);
  let body;
  try { body = JSON.parse(raw); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!Array.isArray(body?.jobs) || body.jobs.length > 100 || body.jobs.some(job => !job || !ID.test(job.source || "")
      || !SLUG.test(job.weapon || "") || !SLUG.test(job.ruler || "") || job.request?.weapon !== job.weapon
      || job.request.strategy !== "quick" || !Array.isArray(job.request.starts) || job.request.finalists !== 1
      || job.context?.weapon !== job.weapon || job.context.benchmark !== job.ruler))
    return json({ ok: false, error: "invalid family requests" }, 400);
  const { engine } = await servedRelease(env);
  if (!engine) return json({ ok: false, error: "No engine is served." }, 503);
  for (const job of body.jobs) {
    const id = await questionId("family_build", { source: job.source, weapon: job.weapon, ruler: job.ruler, engine });
    const stored = await env.LIBRARY.prepare(`INSERT INTO family_transfers (id, source, weapon, ruler, request, context, engine, state, created_at)
      SELECT ?1::text, ?2::text, ?3::text, ?4::text, ?5::text, ?6::text, ?7::text, 'waiting', ?8::text
      WHERE EXISTS (SELECT 1 FROM builds WHERE id = ?2::text) ON CONFLICT DO NOTHING RETURNING id`)
      .bind(id, job.source, job.weapon, job.ruler, JSON.stringify(job.request), JSON.stringify(job.context), engine, iso(Date.now())).first();
    if (!stored && !await env.LIBRARY.prepare("SELECT id FROM family_transfers WHERE id = ?").bind(id).first())
      return json({ ok: false, error: "The source build has not landed." }, 409);
  }
  await askFamilyTransfers(env);
  return json({ ok: true });
}
