// SPDX-License-Identifier: AGPL-3.0-or-later
import {createHash} from 'node:crypto';
import {verifyRankBaseline} from './board_rank_baseline.mjs';

const indexed=['state_board_rank_sources','state_board_rank_input_sets','state_board_rank_legacy_dependencies',
 'state_board_rank_rows','state_board_rank_winners'];
const where=' WHERE weapon=$1 AND ruler=$2';
export async function requestedReservation(tx,keys,scene,selected,part,witness,archivedRevision=null){
 if(!selected||selected.enabled||selected.canonical_sources||selected.revision!==null||selected.scene!==scene)
  throw Error('catalog reservation changed');
 if(!witness||witness.weapon!==keys[0]||witness.ruler!==keys[1])throw Error('pinned requested reservation required');
 const [request]=await tx.unsafe('SELECT * FROM state_board_requested_partitions'+where+' FOR UPDATE',keys);
 const [mail]=await tx.unsafe(`SELECT version,applied_version,removed,CASE WHEN lease_until>board_input_timestamp(clock_timestamp()) THEN lease_token END lease_token FROM state_board_input_mailbox
  WHERE scope='partition' AND key=$1 FOR UPDATE`,[JSON.stringify(keys)]);
 if(!request?.enqueued||request.activated_at!==null||String(request.position)!==String(witness.request?.position)||
    !mail||mail.removed||mail.lease_token!==null||String(mail.version)!==String(witness.partition_mail?.version)||
    String(mail.applied_version)!==String(witness.partition_mail?.applied))
  throw Error('player request or partition input changed');
 if((BigInt(mail.version)>BigInt(mail.applied_version))===!!part)throw Error('unexpected player request acknowledgement');
 const [blocked]=await tx.unsafe(`SELECT EXISTS(SELECT FROM record_board_read_snapshots${where} AND revision IS DISTINCT FROM $3)
  OR EXISTS(SELECT FROM record_board_rank_snapshot_refs${where}) OR EXISTS(SELECT FROM state_board_rank_publication_ready${where})
  OR EXISTS(SELECT FROM state_board_rank_generations${where}) OR EXISTS(SELECT FROM state_board_rank_baseline_obligations${where}) present`,[...keys,archivedRevision]);
 if(blocked.present||!!part!==!!witness.rank)throw Error('private catalog preparation changed');
 if(part&&(part.catalog_phase!==null||part.available_at!==null||part.seeded||part.complete||part.reconciled||
    !['cut','path','cursor','bytes','total','loaded','revision','input_sha256'].every(k=>String(part[k])===String(witness.rank[k]))))
  throw Error('occupied catalog predecessor changed');
 return {request_position:String(request.position),partition_version:String(mail.version)};
}

export async function preserveCatalogPredecessor(tx,part,witness,work,verify=verifyRankBaseline){
 const keys=[part.weapon,part.ruler],cut=part.cut;
 if(!/^[a-f0-9]{64}$/.test(cut)||witness.cut_file?.sha256!==cut)throw Error('invalid predecessor cut');
 await verify(part,work);
 const dirt=await tx.unsafe('SELECT version,applied_version,lease FROM state_board_rank_dirty'+where+' ORDER BY identity FOR UPDATE',keys);
 if(dirt.length!==witness.dirty?.all||dirt.some(d=>d.lease!==null||String(d.version)!==String(d.applied_version)))
  throw Error('occupied catalog dirty work changed');
 const schema='board_cut_'+cut.slice(0,40);
 await tx.unsafe(`SELECT set_config('default_tablespace',coalesce((SELECT t.spcname FROM pg_class c JOIN pg_tablespace t ON t.oid=c.reltablespace
  WHERE c.oid='state_board_rank_partitions'::regclass),current_setting('default_tablespace')),true)`);
 await tx.unsafe('CREATE SCHEMA '+schema);
 const counts={};
 for(const name of ['state_board_rank_partitions',...indexed,'state_board_rank_descriptions','state_board_rank_dirty']){
  let select='SELECT * FROM '+name+where,params=keys;
  if(name==='state_board_rank_input_sets')select=`SELECT i.* FROM state_board_rank_sources s JOIN LATERAL(
   SELECT * FROM state_board_rank_input_sets WHERE (identity,ruler,weapon)=(s.identity,s.ruler,s.weapon) OFFSET 0)i ON true
   WHERE s.weapon=$1 AND s.ruler=$2`;
  if(name==='state_board_rank_descriptions'){
   select='SELECT d.* FROM (SELECT DISTINCT description FROM '+schema+'.state_board_rank_input_sets) a '+
    'JOIN LATERAL(SELECT * FROM state_board_rank_descriptions WHERE id=a.description OFFSET 0)d ON true';params=[];
  }
  await tx.unsafe('CREATE TABLE '+schema+'.'+name+' (LIKE '+name+' INCLUDING CONSTRAINTS INCLUDING INDEXES INCLUDING STORAGE)');
  const copied=await tx.unsafe('INSERT INTO '+schema+'.'+name+' '+select,params);
  const [saved]=await tx.unsafe('SELECT count(*) n FROM '+schema+'.'+name);
  counts[name]=Number(saved.n);if(copied.count!==counts[name])throw Error('incomplete private index archive');
 }
 if(counts.state_board_rank_sources!==witness.source_count||counts.state_board_rank_dirty!==dirt.length||
    counts.state_board_rank_partitions!==1)throw Error('occupied catalog coverage changed');
 // The typed archive, cut file, player request and dirty mailbox survive the index replacement.
 for(const name of ['state_board_rank_winners','state_board_rank_rows','state_board_rank_input_sets','state_board_rank_legacy_dependencies','state_board_rank_sources']){
  const cleared=name==='state_board_rank_input_sets'?await tx.unsafe('DELETE FROM '+name+' i USING '+schema+'.'+name+
   ' a WHERE (i.identity,i.ruler,i.weapon)=(a.identity,a.ruler,a.weapon)'):await tx.unsafe('DELETE FROM '+name+where,keys);
  if(cleared.count!==counts[name])throw Error('occupied catalog indices changed');
 }
 return {schema,path:part.path,cut,cursor:String(part.cursor),loaded:String(part.loaded),revision:String(part.revision),counts};
}

export async function replaceCatalogPredecessor(tx,m){
 const input=createHash('sha256').update(JSON.stringify({source:m.source,native:m.native,cut:m.sha256})).digest('hex');
 await tx.unsafe(`UPDATE state_board_rank_partitions SET cut=$3,path=$4,bytes=$5,total=$6,loaded=0,cursor=0,reconcile_cursor='',
  error=NULL,input_sha256=$7,revision=nextval('board_rank_generation'),available_at=board_input_timestamp(clock_timestamp())
  WHERE weapon=$1 AND ruler=$2`,[m.weapon,m.ruler,m.sha256,m.path,m.bytes,m.total,input]);
}
