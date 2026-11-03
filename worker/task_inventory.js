// SPDX-License-Identifier: AGPL-3.0-or-later
// Complete workload snapshots, independent of task admission and dispatch.
export const INVENTORY_KEY = "task_inventory", INVENTORY_MAX_MS = 3 * 60 * 60000;

export function validInventory(value, now = Date.now()) {
  return value?.v === 1 && typeof value.at === "string" && typeof value.started_at === "string"
    && Number.isFinite(Date.parse(value.at)) && Number.isFinite(Date.parse(value.started_at))
    && Date.parse(value.started_at) <= Date.parse(value.at) && Date.parse(value.at) <= now + 60000
    && now - Date.parse(value.started_at) <= INVENTORY_MAX_MS && Date.parse(value.expires_at) > now
    && Number.isSafeInteger(value.total) && value.total >= 0
    && Number.isSafeInteger(value.registered) && value.registered >= 0 && value.registered <= value.total
    && Array.isArray(value.orders) && value.orders.every(o => typeof o.kind === "string" && Number.isSafeInteger(o.total) && o.total >= 0)
    && value.orders.reduce((n, o) => n + o.total, 0) === value.total
    && value.verbs && Object.values(value.verbs).every(n => Number.isSafeInteger(n) && n >= 0)
    && Object.values(value.verbs).reduce((n, x) => n + x, 0) === value.total;
}

export async function readInventory(db, now = Date.now()) {
  const row = await db.prepare("SELECT body FROM factory_findings WHERE kind = ?").bind(INVENTORY_KEY).first();
  if (!row) return null;
  try { const body = JSON.parse(row.body); return validInventory(body, now) ? body : null; }
  catch (_) { return null; }
}

/// A producer publishes a complete scan; all other registered task types are
/// counted in the same database statement. Admission does not change this total.
export function inventoryWrite(plan, now = Date.now()) {
  if (!plan || !Number.isSafeInteger(plan.rows) || plan.rows < 0 || typeof plan.verb !== "string" || typeof plan.kind !== "string"
    || !Number.isSafeInteger(plan.queued) || plan.queued < 0 || plan.queued > plan.rows
    || !Array.isArray(plan.covers) || !plan.covers.length || !plan.covers.every(p => typeof p === "string")
    || !Number.isFinite(Date.parse(plan.started_at)) || Date.parse(plan.started_at) > now || now - Date.parse(plan.started_at) > INVENTORY_MAX_MS)
    throw new Error("incomplete task inventory");
  const at = new Date(now).toISOString();
  return { sql: `WITH live AS MATERIALIZED (
      SELECT d.question,(array_agg(d.producer ORDER BY d.due_at,d.producer,d.ref))[1] producer FROM demands d
      WHERE d.state='live' AND d.producer NOT IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb))
      GROUP BY d.question), pending AS (
      SELECT l.producer kind,q.verb FROM live l JOIN questions q ON q.id=l.question
      WHERE q.state IN ('open','disputed','nondeterministic') AND NOT EXISTS (
        SELECT 1 FROM demands d WHERE d.question=q.id AND d.state='live'
          AND d.producer IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb)))
      UNION ALL SELECT 'verification',q.verb FROM questions q WHERE q.state='spot'
      UNION ALL SELECT 'verification',q.verb FROM questions q WHERE q.state IN ('disputed','nondeterministic')
        AND NOT EXISTS (SELECT 1 FROM demands d WHERE d.question=q.id AND d.state='live')),
    other AS MATERIALIZED (SELECT kind,verb,count(*) n FROM pending
      GROUP BY 1,2), all_orders AS (
      SELECT ?3::text kind, ?4::text verb, ?5::bigint n UNION ALL SELECT kind,verb,n FROM other),
    orders AS (SELECT kind,sum(n)::bigint n FROM all_orders GROUP BY kind),
    verbs AS (SELECT verb,sum(n)::bigint n FROM all_orders GROUP BY verb)
    INSERT INTO factory_findings(kind,body,at)
    SELECT ?1::text, jsonb_build_object('v',1,'at',?6::text,'started_at',?7::text,'source',?8::text,'expires_at',?10::text,
      'total',(SELECT sum(n) FROM all_orders),'registered',?9::bigint+(SELECT coalesce(sum(n),0) FROM other),
      'orders',(SELECT jsonb_agg(jsonb_build_object('kind',kind,'total',n) ORDER BY kind) FROM orders),
      'verbs',(SELECT jsonb_object_agg(verb,n) FROM verbs))::text,?6::text
    ON CONFLICT(kind) DO UPDATE SET body=excluded.body,at=excluded.at
      WHERE factory_findings.body::jsonb->>'started_at' <= excluded.body::jsonb->>'started_at'`,
    params: [INVENTORY_KEY, JSON.stringify(plan.covers), plan.kind, plan.verb, plan.rows, at, plan.started_at, plan.source || "", plan.queued,
      new Date(Date.parse(plan.started_at) + INVENTORY_MAX_MS).toISOString()] };
}
