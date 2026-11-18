// SPDX-License-Identifier: AGPL-3.0-or-later
// One repeatable-read snapshot counts the disjoint dispatch ranges. Pages only
// return their zone totals and continuation, even for a full bootstrap.
const zone = 'coalesce(s.zone,factory_default_zone(s.producer))';
const position = 'coalesce(s.zone_position,s.task_id)';
const order = [zone, position, 's.id'].join(',');
const tail = '(s.z,s.pos,s.id)>($1::bigint,$2::bigint,$3::text)';

export async function sampleQueueZones(query, {pageRows = 8192} = {}) {
  if (!Number.isSafeInteger(pageRows) || pageRows < 1 || pageRows > 32768) throw Error('invalid queue page size');
  const [header] = await query(`SELECT compute_order_cut_epoch() epoch,
    (SELECT jsonb_build_object('id',c.id,'keep_zone',c.keep_zone::text,'keep_position',c.keep_position::text,
      'keep_id',c.keep_id,'task_cutoff',c.task_cutoff::text) FROM compute_order_cut() c) cut,
    to_char(transaction_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') at`);
  const {cut} = header, totals = new Map();
  let pages = 0;
  const scan = async (sql, initial, sign = 1) => {
    let cursor = null;
    for (;;) {
      const args = [...initial, ...(cursor ?? []), pageRows];
      const [page] = await query(sql(cursor, args.length), args);
      pages++;
      for (const [z, count] of page.zones) {
        if (!Number.isInteger(z) || z < 0 || z > 99 || !Number.isSafeInteger(count)) throw Error('invalid queue zone');
        totals.set(z, (totals.get(z) ?? 0) + sign * count);
      }
      if (!page.next || page.n < pageRows) break;
      cursor = page.next;
    }
  };
  const summarise = (page, grouped, lastOrder, anchor, include = '') => `WITH page AS MATERIALIZED (${page}),
    grouped AS (SELECT z,count(*) n FROM (${grouped}) live ${include} GROUP BY z)
    SELECT (SELECT count(*)::int FROM page) n,
      coalesce((SELECT jsonb_agg(jsonb_build_array(z,n)) FROM grouped),'[]') zones,
      (SELECT jsonb_build_array(${anchor}) FROM page ORDER BY ${lastOrder} LIMIT 1) next`;
  const boundary = cut ? [String(cut.keep_zone), String(cut.keep_position), cut.keep_id] : [];
  await scan((cursor, limit) => {
    const i = boundary.length + 1;
    return summarise(`SELECT ${zone} z,${position} pos,s.id FROM state_compute_scheduling s
      WHERE s.state='open' ${cut ? 'AND (' + order + ')<=($1::bigint,$2::bigint,$3::text)' : ''}
      ${cursor ? 'AND (' + order + ')>($' + i + '::bigint,$' + (i+1) + '::bigint,$' + (i+2) + '::text)' : ''}
      ORDER BY ${order} LIMIT $${limit}::bigint`, 'SELECT z FROM page','z DESC,pos DESC,id DESC','z::text,pos::text,id');
  }, boundary);
  if (cut) {
    const args = [...boundary, String(cut.task_cutoff), cut.id];
    const lookup = `(SELECT s.id,s.task_id,s.state,${zone} z,${position} pos FROM state_compute_scheduling s WHERE s.id=p.question OFFSET 0) s WHERE s.state='open'`;
    await scan((cursor, limit) => summarise(`SELECT t.id,t.computation question FROM entity_compute_tasks t
      WHERE t.id>$4::bigint AND $5::text IS NOT NULL ${cursor ? 'AND t.id>$6::bigint' : ''}
      ORDER BY t.id LIMIT $${limit}::bigint`, `SELECT s.z FROM page p CROSS JOIN LATERAL
      ${lookup} AND s.task_id=p.id AND ${tail}`, 'id DESC','id::text'),args);
    await scan((cursor, limit) => summarise(`SELECT question FROM relation_compute_order_cut_protection
      WHERE cut_id=$5::text ${cursor ? 'AND question>$6::text' : ''} ORDER BY question LIMIT $${limit}::bigint`,
      `SELECT s.z FROM page p CROSS JOIN LATERAL ${lookup} AND s.task_id<=$4::bigint AND ${tail}`,
      'question DESC','question'),args);
    await scan((cursor, limit) => summarise(`SELECT question,task_id FROM record_compute_order_cancellations
      WHERE cut_id=$5::text ${cursor ? 'AND (question,task_id)>($6::text,$7::bigint)' : ''}
      ORDER BY question,task_id LIMIT $${limit}::bigint`, `SELECT s.z FROM page p CROSS JOIN LATERAL
      ${lookup} AND s.task_id=p.task_id AND ((s.z,s.pos,s.id)<=($1::bigint,$2::bigint,$3::text) OR s.task_id>$4::bigint
        OR EXISTS(SELECT 1 FROM relation_compute_order_cut_protection e WHERE e.cut_id=$5 AND e.question=s.id))`,
      'question DESC,task_id DESC','question,task_id::text'),args,-1);
  }
  const zones = [...totals].filter(([,n]) => n !== 0).sort(([a],[b]) => a-b);
  if (zones.some(([,n]) => n < 0 || !Number.isSafeInteger(n))) throw Error('invalid queue count');
  return {at:header.at, order_epoch:header.epoch, zones, pages};
}

// The offset and the writer balances are read from the same MVCC snapshot.
// A later read adds every committed transition exactly once, including changes
// committed while the initial indexed pages were being read.
export async function seedQueueZones(query, options) {
  const [isolation] = await query("SELECT current_setting('transaction_isolation') level");
  if (!['repeatable read', 'serializable'].includes(isolation.level)) throw Error('queue seed requires a stable snapshot');
  const sample = await sampleQueueZones(query, options);
  const rows = await query(`WITH totals AS (
    SELECT (v->>0)::smallint zone,(v->>1)::bigint n FROM jsonb_array_elements($2::text::jsonb) v
    UNION ALL SELECT zone,-sum(n)::bigint FROM state_queue_zone_changes WHERE epoch=$1 GROUP BY zone
  ), offsets AS (SELECT zone,sum(n)::bigint n FROM totals GROUP BY zone HAVING sum(n)<>0)
  INSERT INTO state_queue_zone_seed(epoch,zones,sampled_at)
    SELECT $1,coalesce(jsonb_agg(jsonb_build_array(zone,n) ORDER BY zone),'[]'),$3::timestamptz FROM offsets
    ON CONFLICT(epoch) DO NOTHING RETURNING epoch`, [sample.order_epoch ?? '',JSON.stringify(sample.zones),sample.at]);
  return {...sample, installed:rows.length===1};
}

export const queueZonesSql = `WITH epoch AS MATERIALIZED (SELECT compute_order_cut_epoch() at),
  seed AS MATERIALIZED (SELECT s.zones FROM state_queue_zone_seed s WHERE epoch=coalesce((SELECT at FROM epoch),'')),
  pieces AS (
    SELECT (v->>0)::smallint zone,(v->>1)::bigint n FROM seed CROSS JOIN LATERAL jsonb_array_elements(zones) v
    UNION ALL SELECT zone,n FROM state_queue_zone_changes WHERE epoch=coalesce((SELECT at FROM epoch),'')
  ), totals AS (SELECT zone,sum(n)::bigint n FROM pieces GROUP BY zone HAVING sum(n)<>0)
  SELECT (SELECT at FROM epoch) order_epoch,
    to_char(statement_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') counts_at,
    coalesce((SELECT jsonb_agg(jsonb_build_array(zone,n) ORDER BY zone) FROM totals),'[]') zones FROM seed`;


export function queueOrders(zones, held = []) {
  const active = new Map(held.map(row=>[Number(row.zone),Number(row.n)]));
  let prior=-1, total=0;
  if (!Array.isArray(zones)) throw Error('missing queue zones');
  const orders=zones.map(([zone,n])=>{
    if (!Number.isInteger(zone)||zone<=prior||zone>99||!Number.isSafeInteger(n)||n<=0) throw Error('invalid queue zones');
    prior=zone; total+=n;
    return {kind:String(zone).padStart(2,'0'),zone,total:n,dispatched:Math.min(n,active.get(zone)??0)};
  });
  if (!Number.isSafeInteger(total)) throw Error('queue total exceeds the public format');
  return {orders,total};
}
