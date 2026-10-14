// SPDX-License-Identifier: AGPL-3.0-or-later
// WHAT EACH FACTORY CALL COST — docs/ANALYTICS.md §"Factory calls". One point a
// call to `FACTORY_CALLS`: its path and protocol, the database calls, statements
// and time it took, and its wall time, so worker calls per device-hour and
// round trips per call are measured, never guessed. A device appears only as a
// salted hash of its id, which is its secret.

/// The databases' cost so far, summed over a request's handles (worker/db.js).
export const costOf = (env) => ["LIBRARY", "ACCOUNTS"].reduce((t, k) => {
  const c = env[k] && env[k].cost;
  return c ? { calls: t.calls + c.calls, statements: t.statements + c.statements, db_ms: t.db_ms + c.db_ms } : t;
}, { calls: 0, statements: 0, db_ms: 0 });

async function deviceHash(env, id) {
  if (!id) return "";
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`${env.AUTH_SECRET || "wfsim"}|device|${id}`));
  return [...new Uint8Array(bytes).slice(0, 8)].map((x) => x.toString(16).padStart(2, "0")).join("");
}

/// ANSWER A FACTORY CALL and note what it cost. `handle` answers it; the body is
/// read again here only for who asked and with which protocol. Never throws: a
/// point that cannot be written costs the caller nothing.
export async function noteFactoryCall(request, env, path, handle) {
  const t0 = Date.now(), before = costOf(env);
  const peek = env.FACTORY_CALLS ? request.clone() : null;
  const response = await handle();
  if (!peek) return response;
  try {
    let b = {};
    try { b = JSON.parse(await peek.text()) || {}; } catch { /* not json */ }
    const after = costOf(env);
    env.FACTORY_CALLS.writeDataPoint({
      indexes: [await deviceHash(env, typeof b.verifier === "string" ? b.verifier : "")],
      blobs: [path, Number.isInteger(b.protocol) ? String(b.protocol) : "", String(response.status)],
      doubles: [1, after.calls - before.calls, after.statements - before.statements, after.db_ms - before.db_ms, Date.now() - t0],
    });
  } catch { /* unmeasured */ }
  return response;
}
