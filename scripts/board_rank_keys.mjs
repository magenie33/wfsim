// SPDX-License-Identifier: AGPL-3.0-or-later
// Routing and dependency registration serialize before taking destination row locks.
export const rankMeasurementKey=(identity,ruler,mode)=>'['+[identity,ruler,mode].map(JSON.stringify).join(', ')+']';

export async function lockRankInputs(tx,source,{identities=[],reviews=[]}) {
 const keys=[...new Set([...identities.map(id=>'build/'+id),...reviews.map(id=>'review/'+id)])].sort();
 if(!keys.length)return;
 await tx.unsafe(`SELECT pg_advisory_xact_lock(hashtextextended($1::text||key,0))
  FROM (SELECT key FROM jsonb_array_elements_text($2::text::jsonb) WITH ORDINALITY a(key,n) ORDER BY n OFFSET 0) ordered`,
  [source+'/board-rank/',JSON.stringify(keys)]);
}
