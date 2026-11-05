// SPDX-License-Identifier: AGPL-3.0-or-later
// A search lease grants build-only delivery after its calculation has finished.
import { hmac, initiatorCode } from "./accounts.js";
import { USER_TASK_KEEP_MS } from "./user_tasks.js";
import { servedRelease, ENGINE_ID, LEASE_ID, QUESTION_ID, unrecognised } from "./verify.js";
import { iso } from "./instant.js";
import { appraisalKept, SURVEY_CHANNEL } from "./appraise.js";
import { questionId } from "./task_contract.js";

const keyOf = env => `${env.AUTH_SECRET}:board-harvest`;
const failure = (error, status) => ({ error, status, retryable: status >= 500 || status === 429 });
const details = claims => ({ question: claims.question, weapon: claims.weapon, engine: claims.engine, expires_at: claims.expires_at });

async function issue(env, fields, now) {
  const claims = { scope: "board-builds", ...fields, expires_at: iso(now + USER_TASK_KEEP_MS) };
  const carried = btoa(JSON.stringify(claims)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  return { claims, public: { ...details(claims), receipt: `${carried}.${await hmac(keyOf(env), carried)}` } };
}

export async function authorizeHarvest(env, input, now = Date.now(), httpRequest = null) {
  if (!input || typeof input !== "object" || Array.isArray(input)) return failure("bad harvest authorization", 400);
  if (!env.AUTH_SECRET) return failure("harvest authorization is unavailable", 503);
  if (Object.hasOwn(input, "receipt")) {
    if (typeof input.receipt !== "string" || input.receipt.length > 4096) return failure("bad harvest receipt", 401);
    const parts = input.receipt.split(".");
    if (parts.length !== 2 || !/^[A-Za-z0-9_-]+$/.test(parts[0]) || !/^[A-Za-z0-9_-]{43}$/.test(parts[1])
        || parts[1] !== await hmac(keyOf(env), parts[0])) return failure("bad harvest receipt", 401);
    let claims;
    try { claims = JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(parts[0].replace(/-/g, "+").replace(/_/g, "/")), c => c.charCodeAt(0)))); }
    catch (_) { return failure("bad harvest receipt", 401); }
    if (claims?.scope !== "board-builds" || !QUESTION_ID.test(claims.question || "") || !ENGINE_ID.test(claims.engine || "")
        || !/^[a-z0-9_]{1,64}$/.test(claims.weapon || "") || !Number.isFinite(Date.parse(claims.expires_at))
        || !(claims.initiator === null || typeof claims.initiator === "string")
        || !(claims.family_source === null || typeof claims.family_source === "string")) return failure("bad harvest receipt", 401);
    if (Date.parse(claims.expires_at) <= now) return failure("harvest receipt expired", 410);
    if ((input.question !== undefined && input.question !== claims.question)
        || (input.engine !== undefined && input.engine !== claims.engine)) return failure("harvest receipt does not match this search", 403);
    return { claims, public: details(claims) };
  }
  if (!ENGINE_ID.test(input.engine || "")) return failure("bad harvest engine", 400);
  if ((await servedRelease(env)).engine !== input.engine) return failure("the harvest engine is not served", 409);
  if (input.code !== undefined) {
    if (typeof input.code !== "string" || !/^[A-Za-z0-9]{3,12}$/.test(input.code)) return failure("bad appraisal code", 400);
    if (env.OCR_LIMIT) {
      const { success } = await env.OCR_LIMIT.limit({ key: "appraise" + (httpRequest?.headers.get("cf-connecting-ip") || "unknown") });
      if (!success) return failure("slow down", 429);
    }
    const row = await env.LIBRARY.prepare("SELECT code, channel, asker, weapon, engine, request, at FROM appraisals WHERE code = ?")
      .bind(input.code.toUpperCase()).first();
    if (!appraisalKept(row, now)) return failure("no such appraisal", 404);
    if (!row.request || row.engine !== input.engine) return failure("the appraisal is not frozen on this engine", 409);
    let frozen;
    try { frozen = JSON.parse(row.request); } catch (_) { return failure("bad harvest search", 403); }
    if (!frozen.request || typeof frozen.request !== "object" || Array.isArray(frozen.request) || frozen.request.weapon !== row.weapon
        || !/^[a-z0-9_]{1,64}$/.test(row.weapon || "")) return failure("bad harvest search", 403);
    const question = await questionId("optimize", frozen);
    if (input.question !== undefined && input.question !== question) return failure("the appraisal names another search", 403);
    return issue(env, { question, weapon: row.weapon, engine: row.engine,
      initiator: row.channel === SURVEY_CHANNEL ? "system" : await initiatorCode(env, "bot", `${row.channel}:${row.asker}`), family_source: null }, now);
  }
  const refused = unrecognised(input.verifier);
  if (refused) return { response: refused };
  if (!LEASE_ID.test(input.lease || "")
      || (input.question !== undefined && !QUESTION_ID.test(input.question))) return failure("bad harvest lease", 400);
  const row = await env.LIBRARY.prepare(`SELECT q.id, q.verb, q.request, d.producer, d.ref, d.initiator
      FROM questions q JOIN demands d ON d.question = q.id AND d.state = 'live'
      WHERE q.lease = ?1::text AND q.leased_to = ?2::text AND q.lease_until >= ?3::text
        AND q.verb IN ('optimize', 'optimize_report') AND (?4::text = '' OR q.id = ?4::text)
        AND d.producer IN ('chat', 'survey', 'user_calculation', 'family_build')
      ORDER BY d.due_at, d.asked_at, d.producer, d.ref LIMIT 1`)
    .bind(input.lease, input.verifier, iso(now), input.question || "").first();
  if (!row || !["optimize", "optimize_report"].includes(row.verb)) return failure("the harvest search is no longer held", 403);
  let request;
  try { request = JSON.parse(row.request); } catch (_) { return failure("bad harvest search", 403); }
  if (row.verb === "optimize") request = request.request;
  if (!request || typeof request !== "object" || Array.isArray(request)
      || !/^[a-z0-9_]{1,64}$/.test(request.weapon || "")) return failure("bad harvest search", 403);
  if (row.producer === "user_calculation" && request.__board_upload !== true) return { disabled: true, public: { disabled: true } };
  return issue(env, { question: row.id, weapon: request.weapon, engine: input.engine,
    initiator: row.producer === "survey" || row.producer === "family_build" ? "system" : row.initiator || null,
    family_source: row.producer === "family_build" ? row.ref : null }, now);
}
