// SPDX-License-Identifier: AGPL-3.0-or-later
// The sparse capture retains identities that arrived while their public partition was being selected.
import {putRankDirtyBatch} from './board_rank_work.mjs';

export async function reconcileSelectedInputs({sql,journal,contract},now){
 const saved=journal.prepare("SELECT value FROM priority_cursor WHERE feed='selected-inputs'").get();
 const checkpoint=saved?JSON.parse(saved.value):{};
 const {after=[],next=0}=checkpoint;
 if(now<next)return;
 const keys=await sql.unsafe(`SELECT weapon,ruler,identity FROM state_board_rank_catalog_inputs
  WHERE (weapon,ruler,identity)>($1,$2,$3) ORDER BY weapon,ruler,identity LIMIT 64`,after.length?after:['','','']);
 const repairs=keys.length?await sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1::text)',[contract.source]);
  const gaps=await tx.unsafe(`WITH keys AS MATERIALIZED (SELECT * FROM jsonb_to_recordset($1::text::jsonb) k(weapon text,ruler text,identity text)),
   targets AS MATERIALIZED (SELECT DISTINCT p.weapon,p.ruler,k.identity FROM keys k
    JOIN state_board_rank_partitions p ON p.weapon=k.weapon AND (p.ruler=k.ruler OR k.ruler='')
    JOIN state_board_read_partitions n ON (n.weapon,n.ruler)=(p.weapon,p.ruler)
    WHERE p.source=$2 AND p.native=$3 AND p.complete AND n.enabled AND n.canonical_sources AND n.revision IS NOT NULL)
   SELECT t.* FROM targets t WHERE
    (NOT EXISTS(SELECT 1 FROM state_board_rank_sources s WHERE (s.weapon,s.ruler,s.identity)=(t.weapon,t.ruler,t.identity))
     AND EXISTS(SELECT 1 FROM entity_weapon_builds b WHERE b.id=t.identity AND b.weapon=t.weapon)
     AND EXISTS(SELECT 1 FROM board_facts f WHERE f.identity=t.identity AND f.ruler=t.ruler AND f.retired_at IS NULL))
    OR EXISTS(SELECT 1 FROM LATERAL (SELECT scope,key,version FROM state_board_rank_dependencies
      WHERE weapon=t.weapon AND ruler=t.ruler AND identity=t.identity OFFSET 0) d
     JOIN LATERAL (SELECT version FROM state_board_input_mailbox WHERE scope=d.scope AND key=d.key OFFSET 0) m ON true
     WHERE d.version IS DISTINCT FROM m.version)`,[JSON.stringify(keys),contract.source,contract.native]);
  return putRankDirtyBatch(tx,gaps,false,true);
 }):[];
 const last=keys.at(-1),more=keys.length===64;
 journal.prepare("INSERT OR REPLACE INTO priority_cursor VALUES('selected-inputs',?)")
  .run(JSON.stringify({after:more?[last.weapon,last.ruler,last.identity]:[],next:more?0:now+30000,
   lastRepaired:repairs[0]||checkpoint.lastRepaired}));
}
