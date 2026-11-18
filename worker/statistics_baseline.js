// SPDX-License-Identifier: AGPL-3.0-or-later
// Candidate pages follow the existing order cut indexes; projection reads
// still apply the canonical effective-work views to each candidate.
const zone='coalesce(s.zone,factory_default_zone(s.producer))';
const position='coalesce(s.zone_position,s.task_id)';
const order=`${zone},${position},s.id`;
const tuple=`(${order})`;
const cut='/* statistics question range */ WITH cut AS MATERIALIZED(SELECT * FROM compute_order_cut())';
const ordinary="('new_build','survey_build','refresh','sweep')";
const ranges=['kept','after_cut','protected','before_open','after_open'];

export function questionRangeCursor(cursor) {
  const state=Array.isArray(cursor)?{lower:cursor[0]??'',range:ranges[0],after:null}:cursor;
  if(!state||typeof state.lower!=='string'||!ranges.includes(state.range)
    ||state.after!==null&&!Array.isArray(state.after))throw Error('invalid statistics question cursor');
  return state;
}

export function questionRangePage({lower,range,after},rows) {
  const args=[lower,rows,...(after??[])];
  let sql;
  switch(range){
  case 'kept':
    sql=`${cut}
      SELECT s.id,s.id>$1::text candidate,jsonb_build_array(${zone}::text,${position}::text,s.id) anchor
      FROM state_compute_scheduling s WHERE state='open'
        AND ${tuple}<=(SELECT keep_zone,keep_position,keep_id FROM cut)
        ${after?'AND '+tuple+'>($3::bigint,$4::bigint,$5::text)':''}
      ORDER BY ${order} LIMIT $2::bigint`;
    break;
  case 'after_cut':
    sql=`${cut}, page AS MATERIALIZED (
      SELECT t.id,t.computation FROM entity_compute_tasks t
       WHERE t.id>(SELECT task_cutoff FROM cut) ${after?'AND t.id>$3::bigint':''}
       ORDER BY t.id LIMIT $2::bigint
    )
    SELECT s.id,s.id>$1::text candidate,jsonb_build_array(p.id::text) anchor
    FROM page p LEFT JOIN LATERAL (
      SELECT s.id FROM state_compute_scheduling s
       WHERE s.id=p.computation AND s.task_id=p.id AND state='open'
        AND ${tuple}>(SELECT keep_zone,keep_position,keep_id FROM cut) OFFSET 0
    ) s ON true ORDER BY p.id`;
    break;
  case 'protected':
    sql=`${cut}, page AS MATERIALIZED (
      SELECT question FROM relation_compute_order_cut_protection
       WHERE cut_id=(SELECT id FROM cut) ${after?'AND question>$3::text':''}
       ORDER BY question LIMIT $2::bigint
    )
    SELECT s.id,s.id>$1::text candidate,jsonb_build_array(p.question) anchor
    FROM page p LEFT JOIN LATERAL (
      SELECT s.id FROM state_compute_scheduling s
       WHERE s.id=p.question AND state='open' AND s.task_id<=(SELECT task_cutoff FROM cut)
        AND ${tuple}>(SELECT keep_zone,keep_position,keep_id FROM cut) OFFSET 0
    ) s ON true ORDER BY p.question`;
    break;
  default:
    sql=`/* statistics question range */ WITH page AS MATERIALIZED (
      SELECT s.id,s.state,${zone} z,${position} p FROM state_compute_scheduling s WHERE s.state
       ${range==='before_open'?'<':'>'}'open'
       ${after?'AND (s.state,'+order+')>($3::text,$4::bigint,$5::bigint,$6::text)':''}
       ORDER BY s.state,${order} LIMIT $2::bigint
    )
    SELECT p.id,p.id>$1::text AND EXISTS(SELECT 1 FROM relation_task_consumers d
      WHERE d.question=p.id AND d.state='live' AND d.producer IN${ordinary} LIMIT 1 OFFSET 0) candidate,
      jsonb_build_array(p.state,p.z::text,p.p::text,p.id) anchor FROM page p ORDER BY p.state,p.z,p.p,p.id`;
  }
  return {sql,args};
}

export function advanceQuestionRange(cursor,last,finished) {
  if(!finished)return {...cursor,after:last.anchor};
  const range=ranges[ranges.indexOf(cursor.range)+1];
  return range?{...cursor,range,after:null}:null;
}
