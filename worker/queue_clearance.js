// SPDX-License-Identifier: AGPL-3.0-or-later
import {clearanceOf} from './clearance.js';
import {iso, PACE_HOURS} from './instant.js';
import {queueZonesSql,queueOrders} from './queue_counts.js';

const HOUR = 3600000;
export const QUEUE_PRICE_INTERVAL = 10 * 60000;

// The sample unit is one finished execution round, including all its measured
// volunteer attempts. A result selected from an earlier round cannot price it.
async function sampleCost(db, verb, since) {
  const leastCommon = verb === 'simulate' ? '' : "AND verb<>'simulate'";
  return db.prepare(`WITH sample AS MATERIALIZED (
    SELECT id,task_id,result_id,fact_engine FROM state_compute_scheduling
    WHERE verb=?1::text ${leastCommon} AND state IN('fact','spot') AND fact_at>=?2::text
    ORDER BY fact_at DESC LIMIT 256)
    SELECT count(*)::int facts,coalesce(sum(cost.ms),0)::float8 ms FROM sample q
    JOIN entity_compute_results r ON r.id=q.result_id AND r.task_id=q.task_id
    CROSS JOIN LATERAL (SELECT sum(a.compute_ms) ms FROM record_candidate_answers a
      WHERE a.task_id=q.task_id AND a.question=q.id AND a.engine=q.fact_engine AND a.official=0
      HAVING count(*)>0 AND count(*)=count(*) FILTER(WHERE a.compute_ms>0)) cost`).bind(verb,since).first();
}

export function queueHours(row,total,prices,now) {
  if (!row.verbs || !prices || row.order_epoch!==prices.epoch || !(now>=prices.at && now<prices.expires)) return null;
  const counts=Object.values(row.verbs);
  if (!counts.every(n=>Number.isSafeInteger(n) && n>=0) || counts.reduce((a,b)=>a+b,0)!==total) return null;
  return clearanceOf({total,verbs:row.verbs},prices).hours;
}

export async function measureQueueCosts(db, prior, now=Date.now()) {
  const row=await db.prepare(queueZonesSql).first();
  if (!row?.verbs) return null;
  const verbs=Object.keys(row.verbs);
  if (prior && prior.epoch===row.order_epoch && now>=prior.at && now<prior.at+QUEUE_PRICE_INTERVAL
      && verbs.every(verb=>Object.hasOwn(prior.msOf,verb))) return prior;
  const hour=iso(now).slice(0,13), since=iso(now-PACE_HOURS*HOUR).slice(0,13);
  const capacity=await db.prepare(`SELECT coalesce(sum(ms),0)::float8 ms FROM record_hourly_compute_totals
    WHERE hour>=?1::text AND hour<?2::text`).bind(since,hour).first();
  const msOf={},factsOf={};
  for (const verb of verbs) {
    const cost=await sampleCost(db,verb,iso(now-7*24*HOUR));
    msOf[verb]=Number(cost.ms);
    factsOf[verb]=Number(cost.facts);
  }
  return {epoch:row.order_epoch,at:now,expires:now+2*QUEUE_PRICE_INTERVAL,msOf,factsOf,
    perHour:Number(capacity.ms)/PACE_HOURS,sampleHours:PACE_HOURS};
}

// Price actual completions at yesterday's prices. Cancellations never enter
// this counter, even when they make the visible queue shrink.
export function compareQueueDay(prior, row) {
  if (!prior) return null;
  const elapsed=(Date.parse(row.counts_at)-Date.parse(prior.at))/HOUR;
  if (!(elapsed>0)) return null;
  let actual=0;
  for (const [verb,n] of Object.entries(row.completed)) {
    const count=n-(prior.completed[verb]??0);
    if (!Number.isSafeInteger(count) || count<0 || (count && !prior.unit_ms[verb])) return null;
    if (count) actual+=count*prior.unit_ms[verb];
  }
  return {elapsed_hours:elapsed,expected_ms:Math.min(elapsed,prior.hours)*prior.ms_per_hour,completed_ms:actual};
}

export async function recordQueueDay(db, prices, now=Date.now()) {
  if (!prices) return;
  const row=await db.prepare(queueZonesSql).first();
  if (!row) return;
  const {total}=queueOrders(row.zones), hours=queueHours(row,total,prices,now);
  if (hours===null) return;
  const day=row.counts_at.slice(0,10),epoch=row.order_epoch??'';
  const last=await db.prepare(`SELECT day::text AS day,forecast FROM state_queue_clearance_days
    WHERE epoch=?1::text AND day<=?2::date ORDER BY day DESC LIMIT 1`).bind(epoch,day).first();
  if (last?.day===day) return;
  const unit_ms=Object.fromEntries(Object.keys(prices.msOf).filter(v=>prices.factsOf[v]>0)
    .map(v=>[v,prices.msOf[v]/prices.factsOf[v]]));
  const forecast={at:row.counts_at,verbs:row.verbs,completed:row.completed,unit_ms,ms_per_hour:prices.perHour,
    hours,comparison:compareQueueDay(last?.forecast,row)};
  await db.batch([
    db.prepare(`INSERT INTO state_queue_clearance_days(day,epoch,observed_at,forecast)
      VALUES(?1::date,?2::text,?3::timestamptz,?4::text::jsonb) ON CONFLICT DO NOTHING`)
      .bind(day,epoch,row.counts_at,JSON.stringify(forecast)),
    db.prepare('DELETE FROM state_queue_clearance_days WHERE day<?1::date-35').bind(day),
  ]);
}
