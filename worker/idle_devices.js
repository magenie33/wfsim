// SPDX-License-Identifier: AGPL-3.0-or-later
// COMPUTERS ON BUT IDLE, for the watchdog (worker/factory.js `watchdog`): an id
// that asked in the last half hour yet holds no row and answered nothing in it.
// Read from what the server already keeps — when each id asked, on what engine,
// how often, what it holds and what it answered — so finding them costs one
// query over the ids asking, and no client reports anything new.
import { iso } from "./instant.js";
import { dialsOf } from "./dials.js";

/// ON = ASKED IN THIS WINDOW, and idle = nothing held or answered in it.
export const IDLE_WINDOW_MS = 30 * 60_000;
/// A LOOP: more asks in an hour than twice what the page makes when it is handed
/// nothing, one every `sync_floor_ms` — the page cannot, so another client does.
export const loopAsksPerHour = (d) => Math.round((2 * 3_600_000) / d.sync_floor_ms);

/// THE IDLE, each `{ device, engine, protocol, lanes, asks, last_answer, label, why }`,
/// `why` one of `old_engine` (asks with an engine other than `served`: handed at
/// most join rows, usually nothing), `asking_loop` and `not_computing`. An id on
/// the served engine is counted only once it asked twice, so a page that just
/// opened is not.
export async function idleDevices(env, served, now = Date.now()) {
  const db = env.LIBRARY, d = await dialsOf(env, now);
  const since = iso(now - IDLE_WINDOW_MS), at = iso(now), hour = iso(now - 3_600_000).slice(0, 13);
  const { results } = await db.prepare(`WITH online AS (SELECT id, asked_engine, asked_lanes FROM verifiers WHERE asked_at > ?1::text AND banned = 0),
      asks AS (SELECT verifier, sum(asks) AS asks, max(asks) AS most FROM verifier_asks
                WHERE hour >= ?3::text AND verifier IN (SELECT id FROM online) GROUP BY 1)
    SELECT o.id AS device, o.asked_engine, o.asked_lanes AS lanes, coalesce(k.asks, 0) AS asks, coalesce(k.most, 0) AS most,
           (SELECT max(a.at) FROM answers a WHERE a.device = o.id) AS last_answer
      FROM online o LEFT JOIN asks k ON k.verifier = o.id
     WHERE NOT EXISTS (SELECT 1 FROM questions q WHERE q.leased_to = o.id AND q.lease_until > ?2::text)
       AND NOT EXISTS (SELECT 1 FROM answers a WHERE a.device = o.id AND a.at > ?1::text)
       AND NOT EXISTS (SELECT 1 FROM appraisals p WHERE p.leased_to = o.id AND p.lease_until > ?2::text)
       AND NOT EXISTS (SELECT 1 FROM appraisal_results r WHERE r.verifier = o.id AND r.at > ?1::text)
     ORDER BY o.id`).bind(since, at, hour).all();
  const labels = new Map();
  if (env.ACCOUNTS && results.length) {
    const { results: named } = await env.ACCOUNTS.prepare(`SELECT verifier, label FROM devices
        WHERE verifier IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb))`).bind(JSON.stringify(results.map((r) => r.device))).all();
    for (const n of named) labels.set(n.verifier, n.label || null);
  }
  const loop = loopAsksPerHour(d), out = [];
  for (const r of results) {
    const [engine, p] = String(r.asked_engine || "").split("/");
    const why = engine !== served ? "old_engine" : r.most > loop ? "asking_loop" : "not_computing";
    if (why === "not_computing" && r.asks < 2) continue;
    out.push({ device: r.device, engine, protocol: p ? Number(p.slice(1)) : 8, lanes: r.lanes, asks: Number(r.asks),
      last_answer: r.last_answer || null, label: labels.get(r.device) || null, why });
  }
  return out;
}

/// THE FINDINGS, one a kind that has any: `idle:<why>` with its devices and how
/// many ids asked in the window, the share that idle is read against.
export async function idleFindings(env, served, now = Date.now()) {
  const idle = await idleDevices(env, served, now);
  const online = (await env.LIBRARY.prepare("SELECT count(*) AS n FROM verifiers WHERE asked_at > ? AND banned = 0")
    .bind(iso(now - IDLE_WINDOW_MS)).first()).n;
  const d = await dialsOf(env, now);
  const out = [];
  for (const why of ["old_engine", "asking_loop", "not_computing"]) {
    const devices = idle.filter((x) => x.why === why);
    if (devices.length) out.push({ kind: `idle:${why}`, devices, online: Number(online), served, loop_per_hour: loopAsksPerHour(d) });
  }
  return out;
}
