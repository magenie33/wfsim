// SPDX-License-Identifier: AGPL-3.0-or-later
// Provenance follows the accepted witnesses; aggregate credit is settled independently.
import { iso } from "./instant.js";

export const COMPUTATION_KEEP_MS = 14 * 86_400_000;
export function computationWrites(db, q, witnesses, engine, at) {
  const guard = "id=?1::text AND canon=?2::text AND fact_engine=?3::text AND fact_at=?4::text AND state IN ('fact','spot')";
  const writes = [];
  if (!["optimize", "optimize_report", "simulate_report"].includes(q.verb)) return writes;
  const completedAt = witnesses.map(w => w.completed_at).sort().at(-1) || at;
  const request = typeof q.request === "string" ? JSON.parse(q.request) : q.request;
  const weaponId = request?.weapon || request?.request?.weapon || request?.context?.weapon;
  const weapon = /^[a-z0-9_]{1,100}$/.test(weaponId || "") ? weaponId : null;
  writes.push(db.prepare(`INSERT INTO state_public_computation_summaries (question,kind,engine,weapon,completed_at,expires_at)
    SELECT ?1::text,?5::text,?3::text,?6::text,?8::text,?7::text FROM state_compute_work WHERE ${guard}
    ON CONFLICT (question) DO UPDATE SET engine=excluded.engine,weapon=excluded.weapon,
      completed_at=excluded.completed_at,expires_at=excluded.expires_at`)
    .bind(q.id, q.canon, engine, at, q.verb === "simulate_report" ? "simulation" : "optimization", weapon, iso(Date.parse(completedAt) + COMPUTATION_KEEP_MS), completedAt),
  db.prepare(`DELETE FROM relation_computation_witnesses WHERE question=?1::text AND EXISTS (SELECT 1 FROM state_compute_work WHERE ${guard})`).bind(q.id, q.canon, engine, at),
  db.prepare(`INSERT INTO relation_computation_witnesses (question,owner,device,official,completed_at,credited_at,position)
    SELECT ?1::text,x.owner,x.device,x.official,x.completed_at,x.credited_at,x.position
      FROM jsonb_to_recordset((?5::text)::jsonb) x(owner text,device text,official bigint,completed_at text,credited_at text,position bigint)
      WHERE EXISTS (SELECT 1 FROM state_compute_work WHERE ${guard})
    ON CONFLICT DO NOTHING`).bind(q.id, q.canon, engine, at, JSON.stringify(witnesses)));
  return writes;
}
export function scoreWitnessWrites(db, identity, ruler, mode, at, fact) {
  const witnesses = Array.isArray(fact.witnesses) ? fact.witnesses : [];
  return [db.prepare(`DELETE FROM relation_measurement_witnesses w WHERE identity=?1::text AND ruler=?2::text AND mode=?3::text
    AND EXISTS (SELECT 1 FROM state_board_measurement_selection s WHERE s.identity=w.identity AND s.ruler=w.ruler AND s.mode=w.mode AND s.finished_at=?4::text)`)
    .bind(identity, ruler, mode, at),
  db.prepare(`INSERT INTO relation_measurement_witnesses (identity,ruler,mode,measured_at,engine,owner,device,official,completed_at,credited_at,position)
    SELECT s.identity,s.ruler,s.mode,s.finished_at,?5::text,x.owner,x.device,x.official,x.completed_at,x.credited_at,x.position
    FROM state_board_measurement_selection s CROSS JOIN jsonb_to_recordset((?6::text)::jsonb) x(owner text,device text,official bigint,completed_at text,credited_at text,position bigint)
    WHERE s.identity=?1::text AND s.ruler=?2::text AND s.mode=?3::text AND s.finished_at=?4::text
    ON CONFLICT (identity,ruler,mode,device) DO NOTHING`)
    .bind(identity, ruler, mode, at, fact.engine, JSON.stringify(witnesses))];
}
export async function purgeComputationRecords(env, now = Date.now()) {
  const db = env.LIBRARY;
  if (!db) return;
  const { results } = await db.prepare(`SELECT question,completed_at FROM state_public_computation_summaries WHERE expires_at<=? ORDER BY expires_at LIMIT 1000`)
    .bind(iso(now)).all();
  if (!results.length) return;
  const expired = JSON.stringify(results);
  await db.batch([
    db.prepare(`SELECT id FROM state_compute_work WHERE id IN (SELECT x.question FROM jsonb_to_recordset((?::text)::jsonb) x(question text)) ORDER BY id FOR UPDATE`).bind(expired),
    db.prepare(`DELETE FROM state_public_computation_summaries r USING jsonb_to_recordset((?::text)::jsonb) x(question text,completed_at text)
      WHERE r.question=x.question AND r.completed_at=x.completed_at`).bind(expired),
    db.prepare(`DELETE FROM record_compute_result_proofs p USING entity_compute_results r,
      jsonb_to_recordset((?::text)::jsonb) x(question text)
      WHERE p.result_id=r.id AND r.computation=x.question
        AND NOT EXISTS(SELECT 1 FROM state_public_computation_summaries s WHERE s.question=x.question)`).bind(expired),
  ]);
}
