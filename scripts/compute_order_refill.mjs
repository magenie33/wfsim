// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFile, readdir, stat, statfs } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout } from 'node:timers/promises';
import { iso } from '../worker/instant.js';
import { queueZonesSql } from '../worker/queue_counts.js';

const POLL_MS = 10_000;
const DEMAND_MAX_AGE = 120_000;
const positive = n => Number.isSafeInteger(n) && n > 0;
const zone = 'coalesce(s.zone,factory_default_zone(s.producer))';

// The exact total can prove the low mark without walking historical tasks.
// Otherwise the zone index gives a bounded set of keys to check individually.
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

export function orderRefill({ low, maxHours, stock, capacity, release, demand,
  delay = signal => setTimeout(POLL_MS, undefined, { signal }), now = Date.now, onHold = () => {} }, cycle = {}) {
  let lastHold;
  const hold = async (reason, signal) => {
    if (lastHold !== reason) onHold(reason);
    lastHold = reason;
    await delay(signal);
  };
  return {
    pollMs: POLL_MS,
    observe({ result }) { if (result === 'queue_full') cycle.filling = false; },
    async wait({ cut, engine, signal, reserved }) {
      if (cycle.cut !== cut || cycle.engine !== engine) Object.assign(cycle, { cut, engine, filling: false });
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
  const refill = orderRefill({ low: cfg.ready_below, maxHours: cfg.max_clear_hours, capacity,
    stock: async (engine, limit) => {
      try { return await belowReadyOrderStock(reader, engine, limit, maxOpen); }
      catch (error) { if (error.code === '57014') return null; throw error; }
    },
    release: publicSnapshot('https://wfsim.app/release.json'),
    demand: publicSnapshot('https://cdn.wfsim.app/demand.json'),
    onHold: reason => console.error(JSON.stringify({ result: 'refill_wait', reason })) }, cycle);
  return { ...refill, inputSha, cut: cfg.order_epoch, source };
}
