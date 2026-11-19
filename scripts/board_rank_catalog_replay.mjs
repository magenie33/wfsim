// SPDX-License-Identifier: AGPL-3.0-or-later
import {createHash} from 'node:crypto';
import {BoardRankQualificationError} from './board_rank_index.mjs';

export function catalogSelection(part,selected){
 if(!selected||selected.enabled||!selected.canonical_sources||selected.revision!==null||selected.scene!==part.catalog_proof.scene)
  throw new BoardRankQualificationError('catalog public selection changed');
}

export async function catalogPageDigest(tx,generation){
 const pages=createHash('sha256'),ordered=createHash('sha256'),rows=Number(generation.output_rows);
 for(let after=0;after<rows;after+=128){
  const last=Math.min(after+128,rows),page=createHash('sha256');
  const values=await tx.unsafe(`SELECT ordinal,body FROM state_board_rank_snapshot_rows
   WHERE token=$1::uuid AND ordinal>$2 AND ordinal<=$3 ORDER BY ordinal`,[generation.token,after,last]);
  if(values.length!==last-after)throw new BoardRankQualificationError('incomplete catalog page');
  for(const [i,row] of values.entries()){
   if(Number(row.ordinal)!==after+i+1)throw new BoardRankQualificationError('incomplete catalog page');
   const line=row.body+'\n';ordered.update(line);page.update(line);
  }
  pages.update((after+1)+' '+last+' '+page.digest('hex')+'\n');
 }
 if(ordered.digest('hex')!==generation.output_sha256)throw new BoardRankQualificationError('staged PostgreSQL pages differ from Native output');
 return pages.digest('hex');
}

export async function replayCatalogCut(tx,part,generation,checked){
 const keys=[part.weapon,part.ruler];
 if(String(part.revision)!==String(generation.index_revision))
  throw new BoardRankQualificationError('frozen catalog index changed');
 if(checked!==part.catalog_proof.pages_sha256||Number(generation.output_rows)!==part.catalog_proof.rows)
  throw new BoardRankQualificationError('staged PostgreSQL pages differ from the qualified catalog');
 await tx.unsafe(`UPDATE state_board_rank_partitions SET catalog_phase='replay',complete=false,reconciled=false,reconcile_cursor='',
  catalog_proof=catalog_proof||jsonb_build_object('checked',$3::text,'generation',$4::text) WHERE weapon=$1 AND ruler=$2`,[...keys,checked,generation.token]);
 return {status:'catalog-replay',weapon:part.weapon,ruler:part.ruler};
}
