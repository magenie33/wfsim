// SPDX-License-Identifier: AGPL-3.0-or-later
// Candidate rows are checked against the published board by the calculator's shared identity API.
import { json, no, sessionAccount, sameSite, limited } from "./accounts.js";
import { iso } from "./instant.js";
import { contributionOf } from "./contribution.js";
import { contributorAccounts, witnessNames } from "./contributor_identity.js";

const ID = /^[a-f0-9]{32}$/, SLUG = /^[a-z0-9_]{1,128}$/;
const reply = (body, status = 200) => json(body, status, { "cache-control": "no-store" });
const bounded = value => typeof value === "string" && value.length <= 200;
const minute = at => typeof at === "string" ? at.slice(0, 16) : "";
async function ownershipSpans(env, account) {
  const { results } = await env.ACCOUNTS.prepare("SELECT verifier,from_hour,until_hour FROM device_spans WHERE account=?").bind(account).all();
  return JSON.stringify(results);
}
const OWNED_WITNESS_SQL = `EXISTS (SELECT 1 FROM jsonb_to_recordset((?1::text)::jsonb) o(verifier text,from_hour text,until_hour text)
  WHERE o.verifier=w.device AND left(w.credited_at,13)>=o.from_hour AND (o.until_hour IS NULL OR left(w.credited_at,13)<o.until_hour))`;
async function namesForRows(env, rows, table, key) {
  if (!rows.length) return [];
  const wanted = JSON.stringify(rows.map(row => key === "question" ? { question: row.question } : { identity: row.identity, ruler: row.ruler, mode: row.mode, measured_at: row.finished_at }));
  const match = key === "question" ? "x.question=w.question" : "x.identity=w.identity AND x.ruler=w.ruler AND x.mode=w.mode AND x.measured_at=w.measured_at";
  const shape = key === "question" ? "question text" : "identity text,ruler text,mode text,measured_at text";
  const { results } = await env.LIBRARY.prepare(`SELECT w.* FROM ${table} w JOIN jsonb_to_recordset((?::text)::jsonb) x(${shape}) ON ${match}
    ORDER BY w.position`).bind(wanted).all();
  const named = await witnessNames(env, results);
  return rows.map(row => ({ ...row, witnesses: results.flatMap((w, i) => (key === "question" ? w.question === row.question
    : w.identity === row.identity && w.ruler === row.ruler && w.mode === row.mode) ? [named[i]] : []) }));
}
export async function computationNames(env, rows) {
  if (!env.LIBRARY || !rows.length) return [];
  const { results } = await env.LIBRARY.prepare(`SELECT s.identity,s.ruler,s.mode,s.score,s.measured_by,s.finished_at
    FROM scores s JOIN jsonb_to_recordset((?::text)::jsonb) x(identity text,ruler text,mode text)
      ON x.identity=s.identity AND x.ruler=s.ruler AND x.mode=s.mode`).bind(JSON.stringify(rows)).all();
  const current = results.filter(s => rows.some(row => row.identity === s.identity && row.ruler === s.ruler && row.mode === s.mode
    && minute(row.measured_at) === minute(s.finished_at) && row.measured_by === s.measured_by.replace(/^verified:/, "")
    && Math.abs(row.score - s.score) <= 1e-12 * Math.max(1, Math.abs(s.score))));
  const named = await namesForRows(env, current, "score_witnesses", "score");
  return named.map(row => ({ identity: row.identity, ruler: row.ruler, mode: row.mode, measured_at: row.finished_at,
    witnesses: row.witnesses.length ? row.witnesses : row.measured_by.startsWith("verified:") ? [] : [{ official: true, name: null }] }));
}
export async function contributorProfilesRoute(request, env, path) {
  const profile = path.match(/^\/api\/contributors\/profile\/([a-f0-9]{32})$/);
  const record = path.match(/^\/api\/computations\/([a-f0-9]{32})$/);
  if (!profile && !record && !["/api/account/contribution-profile", "/api/computations/names"].includes(path)) return null;
  if (!env.ACCOUNTS || (!env.LIBRARY && (record || path === "/api/computations/names"))) return no("unavailable", 503);
  if (await limited(env, request, "computation_records")) return no("rate_limited", 429);
  if (path === "/api/computations/names") {
    if (request.method !== "POST" || !sameSite(request)) return no("bad_request", 403);
    const raw = await request.text();
    if (raw.length > 131072) return no("too_large", 413);
    let body;
    try { body = JSON.parse(raw); } catch { return no("not_json"); }
    if (!Array.isArray(body?.rows) || body.rows.length > 200 || body.rows.some(row => !row || !ID.test(row.identity || "")
      || !SLUG.test(row.ruler || "") || !SLUG.test(row.mode || "") || !bounded(row.measured_at) || !bounded(row.measured_by) || !Number.isFinite(row.score))) return no("bad_rows");
    return reply({ ok: true, rows: await computationNames(env, body.rows) });
  }
  if (request.method !== "GET") return no("method", 405);
  if (record) {
    const row = await env.LIBRARY.prepare("SELECT * FROM computation_records WHERE question=? AND expires_at>?").bind(record[1], iso(Date.now())).first();
    if (!row) return no("unavailable", 410);
    return reply({ ok: true, record: (await namesForRows(env, [row], "computation_witnesses", "question"))[0] });
  }
  const me = await sessionAccount(env, request);
  if (!profile && !me) return no("not_signed_in", 401);
  const people = await contributorAccounts(env, [profile ? profile[1] : me], !!profile);
  const person = profile ? people.values().next().value : people.get(me), account = person?.id;
  if (!person || !person.named && account !== me) return no("not_found", 404);
  const query = new URL(request.url).searchParams, kind = query.get("kind") || "board", weapon = query.get("weapon") || "";
  const view = query.get("view") || "full", size = Number(query.get("limit") || 50);
  let before = null;
  try { before = query.has("before") ? JSON.parse(query.get("before")) : null; } catch { return no("bad_cursor"); }
  if (!["full", "identity", "summary", "records"].includes(view) || !Number.isInteger(size) || size < 1 || size > 50
    || !["board", "optimization"].includes(kind) || weapon && !SLUG.test(weapon) || before && (!Array.isArray(before)
    || before.length !== (kind === "board" ? 4 : 2) || !before.every(bounded))) return no("bad_query");
  const identity = { id: person.profile_id, name: person.name, username: person.username, named: !!person.named, you: account === me };
  if (view === "identity") return reply({ ok: true, profile: identity });
  if (!env.LIBRARY) return no("unavailable", 503);
  const contribution = view === "records" ? {} : await contributionOf(env, account);
  if (view === "summary") return reply({ ok: true, profile: { ...identity, ...contribution }, keep_days: 14 });
  const owners = await ownershipSpans(env, account);
  let rows, next;
  if (kind === "board") {
    const cursor = before || ["9999", "~", "~", "~"];
    ({ results: rows } = await env.LIBRARY.prepare(`SELECT DISTINCT s.identity,s.ruler,s.mode,s.score,s.metric,s.finished_at,s.measured_by,b.weapon,b.record
      FROM score_witnesses w JOIN scores s ON s.identity=w.identity AND s.ruler=w.ruler AND s.mode=w.mode AND s.finished_at=w.measured_at
      JOIN builds b ON b.id=s.identity WHERE ${OWNED_WITNESS_SQL}
      AND (?2::text='' OR b.weapon=?2::text) AND (s.finished_at,s.identity,s.ruler,s.mode)<(?3::text,?4::text,?5::text,?6::text)
      AND NOT EXISTS (SELECT 1 FROM board_facts f WHERE f.identity=s.identity AND f.ruler=s.ruler AND f.mode=s.mode AND f.retired_at IS NOT NULL)
      ORDER BY s.finished_at DESC,s.identity DESC,s.ruler DESC,s.mode DESC LIMIT ?7::bigint`).bind(owners, weapon, ...cursor, size + 1).all());
    next = rows.length > size ? rows[size - 1] : null;
    rows = (await namesForRows(env, rows.slice(0, size), "score_witnesses", "score")).map(row => ({ ...row, record: JSON.parse(row.record) }));
    next = next ? [next.finished_at, next.identity, next.ruler, next.mode] : null;
  } else {
    const cursor = before || ["9999", "~"];
    ({ results: rows } = await env.LIBRARY.prepare(`SELECT DISTINCT r.* FROM computation_records r JOIN computation_witnesses w ON w.question=r.question
      WHERE r.kind='optimization' AND ${OWNED_WITNESS_SQL} AND r.expires_at>?2::text
      AND (?3::text='' OR r.weapon=?3::text) AND (r.completed_at,r.question)<(?4::text,?5::text)
      ORDER BY r.completed_at DESC,r.question DESC LIMIT ?6::bigint`).bind(owners, iso(Date.now()), weapon, ...cursor, size + 1).all());
    next = rows.length > size ? [rows[size - 1].completed_at, rows[size - 1].question] : null;
    rows = await namesForRows(env, rows.slice(0, size), "computation_witnesses", "question");
  }
  return reply({ ok: true, profile: { ...identity, ...contribution }, kind, keep_days: 14, candidates: rows, next });
}
