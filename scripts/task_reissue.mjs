// SPDX-License-Identifier: AGPL-3.0-or-later
// The operator owns writer fencing; pages retain the original queue ordering.
import {readFileSync} from 'node:fs';

export function taskReissueSchemaSql() {
  const source=readFileSync(new URL('../worker/pg/schema.sql',import.meta.url),'utf8');
  const sql=source.match(/CREATE TABLE IF NOT EXISTS record_compute_task_reissues[^]*?REVOKE ALL ON FUNCTION compute_reissue_task\(text,bigint,text,text,text\) FROM PUBLIC;/)?.[0];
  const guard=source.match(/CREATE OR REPLACE FUNCTION compute_answer_task\(\)[^]*?END \$\$;/)?.[0];
  if(!sql || !guard)throw Error('incomplete task reissue schema');
  return sql+'\n'+guard+'\n';
}

export async function taskReissuePage(db,{cutoff,state='open',after=null,limit=100}={}) {
  if(!Number.isSafeInteger(cutoff) || cutoff<1 || !Number.isSafeInteger(limit) || limit<1 || limit>250
      || !['open','disputed','nondeterministic'].includes(state)
      || after && (!Number.isInteger(after.zone) || !Number.isSafeInteger(after.position) || typeof after.id!=='string'))
    throw Error('invalid task reissue page');
  const cursor=after || {zone:-1,position:0,id:''};
  const rows=(await db.prepare(`SELECT q.id,q.task_id,q.state,q.result_id,
      coalesce(q.zone,factory_default_zone(q.producer)) AS zone,coalesce(q.zone_position,q.task_id) AS position,
      compute_task_has_source(q.id) AS traced
    FROM state_compute_scheduling q WHERE q.state=?1::text AND q.task_id<=?2::bigint
      AND (coalesce(q.zone,factory_default_zone(q.producer)),coalesce(q.zone_position,q.task_id),q.id)>(?3::bigint,?4::bigint,?5::text)
    ORDER BY coalesce(q.zone,factory_default_zone(q.producer)),coalesce(q.zone_position,q.task_id),q.id LIMIT ?6::bigint`)
    .bind(state,cutoff,cursor.zone,cursor.position,cursor.id,limit+1).all()).results;
  const entries=rows.slice(0,limit),last=entries.at(-1);
  return {cutoff,state,entries,after:last?{zone:Number(last.zone),position:Number(last.position),id:last.id}:after,
    has_more:rows.length>limit};
}

export async function applyTaskReissue(db,entry,{operation,engine,at}={}) {
  if(!entry || typeof entry.id!=='string' || !Number.isSafeInteger(entry.task_id) || entry.task_id<1
      || !entry.traced || entry.result_id!==null || !operation || !engine || !Number.isFinite(Date.parse(at)))
    throw Error('a task reissue requires an unfinished sourced round and fixed operation');
  return db.prepare('SELECT compute_reissue_task(?1::text,?2::bigint,?3::text,?4::text,?5::text) AS task_id')
    .bind(entry.id,entry.task_id,operation,engine,at).first();
}
