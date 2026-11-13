// SPDX-License-Identifier: AGPL-3.0-or-later
// Legacy business obligations enter the existing task ledger through retryable registration.
import { iso } from "./instant.js";
import { REFRESH_WITHIN_MS } from "./deadlines.js";
import { DIAL_DEFAULTS } from "./dials.js";

export const BOARD_ADMISSION_ROWS = 250, BOARD_RECOVERY_ROWS = 2000;
const CLAIM_MS = 120000, RETRY_MS = 60000;

export async function advanceBoardAdmission(env, { engine, rowAsk, register, now = Date.now(), recover = true, recoveryOpenMax = DIAL_DEFAULTS.board_recovery_open_max } = {}) {
  const db = env.LIBRARY, at = iso(now);
  const before = recover ? (await db.prepare("SELECT board_admission_status(?1::text) AS status").bind(at).first()).status : null;
  const recovery = recover && !before.pending ? (await db.prepare("SELECT board_admission_seed(?1::bigint,?2::text,?3::bigint) AS progress")
    .bind(BOARD_RECOVERY_ROWS, at, recoveryOpenMax).first()).progress : null;
  const { results: rows } = await db.prepare("SELECT * FROM board_admission_claim(?1::bigint,?2::text,?3::text)")
    .bind(BOARD_ADMISSION_ROWS, at, iso(now + CLAIM_MS)).all();
  const admitted = [], deferred = [];
  for (const row of rows) {
    if (!row.batch || !row.record) {
      deferred.push({ row, until: iso(now + RETRY_MS), error: "missing_source" });
      continue;
    }
    const measured = row.measured_at && Date.parse(row.measured_at);
    if (row.batch.startsWith("refresh-") && Number.isFinite(measured) && measured > now - REFRESH_WITHIN_MS) {
      deferred.push({ row, until: iso(measured + REFRESH_WITHIN_MS), error: "refresh_not_due" });
      continue;
    }
    admitted.push(row);
  }
  if (deferred.length) {
    await db.prepare("SELECT board_admission_retry(?1::text,?2::text,?3::text)")
      .bind(JSON.stringify(deferred.map(({ row, until, error }) => ({ ...row, retry_at: until, error }))), iso(now + RETRY_MS), "retry").run();
  }
  if (admitted.length) {
    const receipt = JSON.stringify(admitted.map(({ build_id, ruler, mode, revision, claim, batch, initiator }) =>
      ({ build_id, ruler, mode, revision, claim, batch, initiator })));
    try {
      const asks = admitted.map(row => rowAsk(row.batch, { identity: row.build_id, ruler: row.ruler, mode: row.mode,
        initiator: row.initiator }, row.record, row.built_at, row.measured_at, now));
      await register(env, asks, engine, now, {
        after: [db.prepare("SELECT board_admission_finish(?1::text)").bind(receipt)],
      });
    } catch (error) {
      await db.prepare("SELECT board_admission_retry(?1::text,?2::text,?3::text)")
        .bind(receipt, iso(now + RETRY_MS), "retry").run();
      throw error;
    }
  }
  const { status } = await db.prepare("SELECT board_admission_status(?1::text) AS status").bind(at).first();
  return { ok: true, mode: "admission", asked: admitted.length, deferred: deferred.length,
    scanned: recovery?.scanned ?? null, recovery: status.recovery, pending: status.pending,
    has_deferred: status.deferred, window_full: recovery?.window_full === true,
    complete: status.recovery.complete && !status.pending && !status.deferred };
}
