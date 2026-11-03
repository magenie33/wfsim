// SPDX-License-Identifier: AGPL-3.0-or-later
// Complete workload snapshots, independent of task admission and dispatch.
export const INVENTORY_KEY = "task_inventory", INVENTORY_MAX_MS = 3 * 60 * 60000;

export function validInventory(value, now = Date.now()) {
  return value?.v === 1 && typeof value.at === "string" && typeof value.started_at === "string"
    && typeof value.snapshot === "string" && !!value.snapshot && /^[a-f0-9]{12}$/.test(value.engine)
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
  const row = await db.prepare("SELECT body,(SELECT engine FROM factory_snapshot WHERE id=1) engine FROM factory_findings WHERE kind = ?").bind(INVENTORY_KEY).first();
  if (!row) return null;
  try { const body = JSON.parse(row.body); return validInventory(body, now) && (!row.engine || row.engine === body.engine) ? body : null; }
  catch (_) { return null; }
}

/// Publication stores one classified database snapshot without adding later rows.
export function inventoryWrite(plan, now = Date.now()) {
  const at = new Date(now).toISOString();
  const body = { v: 1, at, started_at: plan?.started_at, source: plan?.source || "", snapshot: plan?.snapshot, engine: plan?.engine,
    expires_at: new Date(Number.isFinite(Date.parse(plan?.started_at)) ? Date.parse(plan.started_at) + INVENTORY_MAX_MS : 0).toISOString(),
    total: plan?.rows, registered: plan?.queued, orders: plan?.orders, verbs: plan?.verbs };
  if (plan?.v !== 2 || !body.snapshot || typeof body.engine !== "string" || !validInventory(body, now))
    throw new Error("incomplete task inventory");
  return { sql: `INSERT INTO factory_findings(kind,body,at) VALUES (?1::text,?2::text,?3::text)
    ON CONFLICT(kind) DO UPDATE SET body=excluded.body,at=excluded.at
    WHERE factory_findings.body::jsonb->>'started_at' <= excluded.body::jsonb->>'started_at'`,
    params: [INVENTORY_KEY, JSON.stringify(body), at] };
}
