// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFile, readdir, stat, statfs } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout } from 'node:timers/promises';
import { iso } from '../worker/instant.js';
import { queueZonesSql } from '../worker/queue_counts.js';

const POLL_MS = 10_000;
const DEMAND_MAX_AGE = 120_000;
const REFILL_OUTSIDE_CHECK_MS = 60_000;
const MINUTE_MS = 60_000;
const positive = n => Number.isSafeInteger(n) && n > 0;
const zone = 'coalesce(s.zone,factory_default_zone(s.producer))';
const stockShapeMismatch = `(s.lanes IS NOT NULL OR
  (s.required_capability IS NOT NULL AND s.required_capability<>'fixed-board-input-v1') OR
  s.verb NOT IN ('simulate','optimize'))`;
export const REFILL_STOCK_EXCEPTION = `(s.required_engine IS NOT NULL OR ${stockShapeMismatch})`;

// Exact totals can prove the low mark without walking historical tasks.
// The all-client ready count plus holds bounds the compatible physical rows.
export const REFILL_STOCK_SQL = `WITH balance AS MATERIALIZED (${queueZonesSql}),
  zones AS MATERIALIZED (
    SELECT (v->>0)::smallint zone,(v->>1)::bigint n
    FROM balance CROSS JOIN LATERAL jsonb_array_elements(zones) v
  ), size AS MATERIALIZED (
    SELECT coalesce(sum(z.n),0)::bigint AS open FROM balance LEFT JOIN zones z ON true
    GROUP BY balance.order_epoch HAVING coalesce(min(z.n),1)>0
  ), candidates AS MATERIALIZED (
    SELECT candidate.* FROM zones z CROSS JOIN LATERAL (
      SELECT s.id,s.task_id,s.waits,s.lease_until,s.required_engine,s.required_capability,s.lanes,s.verb
      FROM state_compute_scheduling s WHERE s.state='open' AND ${zone}=z.zone
      ORDER BY ${zone} DESC,coalesce(s.zone_position,s.task_id) DESC,s.id DESC
      LIMIT least(z.n+16,$4::bigint+16)
    ) candidate
  ), effective AS MATERIALIZED (
    SELECT c.* FROM candidates c
    WHERE EXISTS(SELECT 1 FROM state_compute_effective_open q WHERE q.id=c.id AND q.task_id=c.task_id OFFSET 0)
      AND NOT EXISTS(SELECT 1 FROM record_compute_order_cancellations r
        WHERE r.cut_id=(SELECT id FROM compute_order_cut()) AND r.question=c.id AND r.task_id=c.task_id)
  )
  SELECT CASE WHEN size.open<$3 THEN true WHEN size.open>=$4 THEN false
    WHEN EXISTS(SELECT 1 FROM factory_queue_signal($1::text) AS signal WHERE signal.exact
      AND signal.ready+(SELECT count(*) FROM state_compute_order_stock_holds)<$3) THEN true
    WHEN (SELECT count(*) FROM (
      SELECT 1 FROM effective c WHERE c.waits=0
        AND (c.lease_until IS NULL OR c.lease_until<$1)
        AND (c.required_engine IS NULL OR c.required_engine=$2)
        AND (c.required_capability IS NULL OR c.required_capability='fixed-board-input-v1')
        AND c.lanes IS NULL AND c.verb IN ('simulate','optimize') LIMIT $3
    ) ready)=$3 THEN false
    WHEN (SELECT count(*) FROM effective)=size.open THEN true END AS below FROM size`;

export async function belowReadyOrderStock(reader, engine, limit, maxOpen, now = Date.now()) {
  if (!positive(limit) || !positive(maxOpen) || limit >= maxOpen) throw Error('invalid ready stock watermarks');
  return reader.begin(async tx => {
    await tx.unsafe("SET TRANSACTION READ ONLY; SET LOCAL statement_timeout='2s'; SET LOCAL jit=off");
    const [sample] = await tx.unsafe(REFILL_STOCK_SQL, [iso(now), engine, limit, maxOpen]);
    return sample?.below ?? null;
  });
}

const SCHEDULED_STOCK_SQL = `WITH signal AS MATERIALIZED (
    SELECT exact,ready FROM factory_queue_signal($1::text)
  ), can_count AS MATERIALIZED (
    SELECT EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('compute_scheduling_stock_exception')
      AND indrelid='state_compute_scheduling'::regclass AND indisready AND indisvalid) AS ready
  ), possible AS MATERIALIZED (
    SELECT s.id,s.task_id,s.lease_until,${zone} AS zone,s.required_engine,
      s.required_capability,s.lanes,s.verb
    FROM state_compute_scheduling s
    WHERE s.state='open' AND s.waits=0 AND ${REFILL_STOCK_EXCEPTION}
      AND EXISTS(SELECT 1 FROM signal WHERE exact AND ready>=$3)
      AND (SELECT ready FROM can_count)
    ORDER BY s.task_id,s.id LIMIT 4097
  ), bound AS MATERIALIZED (
    SELECT count(*) AS n FROM possible
  ), incompatible AS (
    SELECT count(*) AS n FROM possible s,bound b
    WHERE b.n<4097 AND (s.lease_until IS NULL OR s.lease_until<$1)
      AND ((s.required_engine IS NOT NULL AND s.required_engine<>$2) OR ${stockShapeMismatch})
      AND compute_order_stock_dispatchable(s.task_id,s.zone)
      AND EXISTS(SELECT 1 FROM state_compute_effective_open q WHERE (q.id,q.task_id)=(s.id,s.task_id) OFFSET 0)
  )
  SELECT CASE WHEN NOT signal.exact THEN NULL WHEN signal.ready<$3 THEN true
    WHEN can_count.ready AND bound.n<4097 AND incompatible.n<=signal.ready THEN (signal.ready-incompatible.n)<$3 END AS below
  FROM signal,can_count,bound,incompatible`;

// Subtract a bounded set of exceptions from the same snapshot's ready count.
// A missing index or too many exceptions leaves the result undecided.
export async function belowScheduledReadyStock(reader, engine, limit, now = Date.now()) {
  return reader.begin(async tx => {
    await tx.unsafe("SET TRANSACTION READ ONLY; SET LOCAL statement_timeout='2s'; SET LOCAL jit=off");
    const [sample] = await tx.unsafe(SCHEDULED_STOCK_SQL, [iso(now), engine, limit]);
    return sample?.below ?? null;
  });
}

function scheduleOf(value, normalLow) {
  if (value === undefined) return null;
  const { start_minute_cst, period_hours, window_minutes, emergency_below, emergency_refill_to } = value ?? {};
  if (!Number.isSafeInteger(start_minute_cst) || start_minute_cst < 0 || start_minute_cst >= 1440
      || !positive(period_hours) || 24 % period_hours !== 0
      || !positive(window_minutes) || window_minutes >= period_hours * 60
      || !positive(emergency_below) || !positive(emergency_refill_to)
      || emergency_below >= emergency_refill_to || emergency_refill_to > normalLow)
    throw Error('invalid refill schedule or emergency watermarks');
  const period = period_hours * 60;
  return { emergency_below, emergency_refill_to, open: at => {
    const minute = Math.floor(at / MINUTE_MS) + 8 * 60;
    return ((minute - start_minute_cst) % period + period) % period < window_minutes;
  } };
}

export function orderRefill({ low, maxHours, stock, scheduledStock = stock, capacity, release, demand, schedule,
  delay = signal => setTimeout(POLL_MS, undefined, { signal }), now = Date.now, onHold = () => {} }, cycle = {}) {
  const window = scheduleOf(schedule, low);
  let lastHold;
  const hold = async (reason, signal) => {
    if (lastHold !== reason) onHold(reason);
    lastHold = reason;
    await delay(signal);
  };
  return {
    pollMs: POLL_MS,
    mayStart: window ? () => {
      const at = now();
      return window.open(at) || (cycle.emergency && at >= cycle.emergencyAt && at - cycle.emergencyAt < REFILL_OUTSIDE_CHECK_MS);
    } : undefined,
    observe({ result }) { if (result === 'queue_full') cycle.filling = false; },
    async wait({ cut, engine, signal, reserved }) {
      if (cycle.cut !== cut || cycle.engine !== engine)
        Object.assign(cycle, { cut, engine, filling: false, emergency: false, outsideCheck: undefined });
      for (;;) {
        signal?.throwIfAborted();
        if (!await capacity()) { await hold('host_capacity', signal); continue; }
        const current = await release(signal);
        if (current?.engine !== engine) { await hold('served_engine', signal); continue; }
        const snapshot = await demand(signal);
        const age = now() - Date.parse(snapshot?.counts_at);
        if (snapshot?.ok !== true || (snapshot.order_epoch ?? '') !== cut || !(age >= 0 && age <= DEMAND_MAX_AGE)
            || !Number.isFinite(snapshot.clear_hours) || snapshot.clear_hours < 0 || snapshot.clear_hours >= maxHours) {
          await hold('clearance_budget', signal);
          continue;
        }
        if (window && !reserved && !window.open(now())) {
          const limit = cycle.emergency ? window.emergency_refill_to : window.emergency_below;
          const sampled = now(), prior = cycle.outsideCheck;
          const fresh = prior && prior.limit === limit && sampled >= prior.at && sampled - prior.at < REFILL_OUTSIDE_CHECK_MS;
          const below = fresh ? prior.below : await scheduledStock(engine, limit);
          cycle.filling = false;
          if (below !== true) {
            if (below === false) cycle.emergency = false;
            if (!fresh) cycle.outsideCheck = { at: sampled, limit, below };
            await hold(below === false ? 'refill_window' : 'ready_stock_unavailable', signal);
            continue;
          }
          cycle.outsideCheck = undefined;
          cycle.emergency = true;
          cycle.emergencyAt = sampled;
          lastHold = undefined;
          return;
        }
        if (window && !reserved) { cycle.emergency = false; cycle.outsideCheck = undefined; }
        if (!cycle.filling && !reserved) {
          const below = await stock(engine, low);
          if (below === false) { await hold('ready_stock', signal); continue; }
          if (below !== true) { await hold('ready_stock_unavailable', signal); continue; }
          cycle.filling = true;
        }
        lastHold = undefined;
        return;
      }
    }
  };
}

async function available(path) {
  const space = await statfs(path, { bigint: true });
  return space.bavail * space.bsize;
}

async function pressure(resource) {
  const text = await readFile('/proc/pressure/' + resource, 'utf8');
  return Number(/^full .*?avg10=([\d.]+)/m.exec(text)?.[1]);
}

async function walBytes(path) {
  const names = (await readdir(path)).filter(name => /^[0-9A-F]{24}(?:\.partial)?$/.test(name));
  const allocation = await Promise.all(names.map(name => stat(join(path, name), { bigint: true })));
  if (!allocation.length) return null;
  return allocation.reduce((sum, file) => sum + file.blocks * 512n, 0n);
}

function publicSnapshot(url) {
  let value, expires = 0;
  return async signal => {
    if (Date.now() < expires) return value;
    const timeout = AbortSignal.timeout(3000);
    try {
      const response = await fetch(url, { signal: signal ? AbortSignal.any([signal, timeout]) : timeout });
      value = response.ok ? await response.json() : null;
    } catch {
      signal?.throwIfAborted();
      value = null;
    }
    expires = Date.now() + POLL_MS;
    return value;
  };
}

export async function openOrderRefill(reader, config, maxOpen, source, cycle) {
  const cfg = JSON.parse(await readFile(config, 'utf8'));
  const inputSha = source?.inputSha ?? cfg.input_sha256;
  if (cfg.v !== 1 || !positive(cfg.ready_below) || !positive(maxOpen) || cfg.ready_below >= maxOpen
      || !/^[0-9a-f]{64}$/.test(inputSha || '') || typeof cfg.order_epoch !== 'string'
      || (source && source.cut !== cfg.order_epoch)
      || ![cfg.root_reserve_bytes, cfg.data_reserve_bytes, cfg.wal_max_bytes, cfg.wal_reserve_bytes].every(positive)
      || cfg.wal_reserve_bytes >= cfg.wal_max_bytes
      || ![cfg.max_io_pressure, cfg.max_memory_pressure, cfg.max_clear_hours].every(n => Number.isFinite(n) && n > 0)
      || ![cfg.data_directory, cfg.wal_directory].every(path => typeof path === 'string' && path.startsWith('/')))
    throw Error('refill requires explicit, bounded queue and host budgets');
  if (process.platform !== 'linux') throw Error('refill host budgets require Linux');
  const capacity = async () => {
    try {
      const [root, data, walDevice, rootFree, dataFree, io, memory] = await Promise.all([
        stat('/'), stat(cfg.data_directory), stat(cfg.wal_directory), available('/'), available(cfg.data_directory),
        pressure('io'), pressure('memory')
      ]);
      if (root.dev === data.dev || walDevice.dev !== data.dev
          || rootFree <= BigInt(cfg.root_reserve_bytes) || dataFree <= BigInt(cfg.data_reserve_bytes)
          || !(io < cfg.max_io_pressure && memory < cfg.max_memory_pressure)) return false;
      const wal = await walBytes(cfg.wal_directory);
      return wal !== null && wal + BigInt(cfg.wal_reserve_bytes) <= BigInt(cfg.wal_max_bytes);
    } catch {
      return false;
    }
  };
  const refill = orderRefill({ low: cfg.ready_below, maxHours: cfg.max_clear_hours, capacity, schedule: cfg.refill_schedule,
    stock: async (engine, limit) => {
      try { return await belowReadyOrderStock(reader, engine, limit, maxOpen); }
      catch (error) { if (error.code === '57014') return null; throw error; }
    },
    scheduledStock: async (engine, limit) => {
      try { return await belowScheduledReadyStock(reader, engine, limit); }
      catch (error) { if (error.code === '57014') return null; throw error; }
    },
    release: publicSnapshot('https://wfsim.app/release.json'),
    demand: publicSnapshot('https://cdn.wfsim.app/demand.json'),
    onHold: reason => console.error(JSON.stringify({ result: 'refill_wait', reason })) }, cycle);
  return { ...refill, inputSha, cut: cfg.order_epoch, source };
}
