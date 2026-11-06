import { authorizeHarvest } from "../worker/board_harvest.js";

export function appraisalDeliveryFixture({ engine, code, weapon, request }) {
  const env = {
    AUTH_SECRET: "appraisal-delivery-check-only",
    ASSETS: { fetch: async () => Response.json({ engine }) },
    LIBRARY: { prepare(sql) {
      if (sql.startsWith("SELECT code, channel, asker, weapon, engine")) return { bind: supplied => ({
        first: async () => supplied === code ? { code, channel: "qq", asker: "check-asker", weapon, engine,
          request: JSON.stringify({ request, context: { weapon } }), at: new Date().toISOString() } : null
      }) };
      if (sql.startsWith("SELECT q.id, q.verb")) return { bind: (lease, verifier) => ({
        first: async () => lease === "0".repeat(32) && verifier === "a".repeat(24)
          ? { id: "c".repeat(32), verb: "optimize_report", request: JSON.stringify(request),
            producer: "chat", initiator: "bot:check-asker" } : null
      }) };
      throw new Error("Unexpected appraisal fixture query: " + sql);
    } }
  };
  const delivered = [];
  return { delivered, async submit(body) {
    const auth = await authorizeHarvest(env, body.harvest);
    if (auth.error) return { status: auth.status, body: { ok: false, error: auth.error, retryable: auth.retryable } };
    if (auth.response) return { status: auth.response.status, body: await auth.response.json() };
    if (!Array.isArray(body.builds)) return { status: 400, body: { ok: false, error: "Expected builds" } };
    if (!body.builds.length) return { status: 200, body: { ok: true, ...auth.public } };
    const results = body.builds.map(build => {
      if (build.weapon !== auth.claims.weapon) return { ok: false, retryable: false, error: "Wrong weapon" };
      delivered.push(build);
      return { ok: true };
    });
    return { status: 200, body: { ok: true, results } };
  } };
}
