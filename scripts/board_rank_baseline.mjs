// SPDX-License-Identifier: AGPL-3.0-or-later
// One immutable canonical cut supplies resumable coverage; live indexed changes take precedence.
import {rankMeasurementKey} from './board_rank_keys.mjs';
import {cachedRankInputs} from './board_rank_inputs.mjs';
import {createHash,randomUUID} from 'node:crypto';
import {createReadStream,lstatSync,realpathSync,statSync} from 'node:fs';
import {mkdir,open,chmod} from 'node:fs/promises';
import {join,resolve,dirname,basename} from 'node:path';
import {boardStorage} from './board_storage.mjs';
import {rankFilePage,syncRankDirectory} from './board_rank_files.mjs';
import {createRankPartition,prepareRankSources,applyRankSources,BoardRankQualificationError} from './board_rank_index.mjs';


export async function captureRankBaseline(sql,options) {
 const work=boardStorage(options.work);
 const directory=join(work,'rank-cut-'+randomUUID());await mkdir(directory,{mode:0o750});
 return captureRankBaselineAt(sql,{...options,work},directory);
}

export async function captureRankBaselineAt(sql,{weapon,ruler,source,native,work,signal,mailboxFloor},directory) {
 const path=join(directory,'inputs.ndjson'),file=await open(path,'wx'),checksum=createHash('sha256');
 let bytes=0,total=0,facts=0;
 const append=async row=>{
  const body=JSON.stringify(row)+'\n';
  if(Buffer.byteLength(body)>8*1024*1024)throw new BoardRankQualificationError('one baseline source exceeds its row budget');
  bytes+=Buffer.byteLength(body);if(bytes>512*1024*1024)throw new BoardRankQualificationError('baseline exceeds its disk budget');
  checksum.update(body);await file.writeFile(body);
 };
 try{
  const metadata=await sql.begin('isolation level repeatable read read only',async tx=>{
   const [{actual,captured_at,snapshot}]=await tx.unsafe(`SELECT (SELECT source FROM state_board_input_source WHERE id=1) actual,
    to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') captured_at,pg_current_snapshot()::text snapshot`);
   if(actual!==source)throw new BoardRankQualificationError('board source changed');
   const meta={kind:'meta',v:1,weapon,ruler,source,native,captured_at,snapshot,...(mailboxFloor==null?{}:{mailbox_floor:String(mailboxFloor)})};await append(meta);
   await tx.unsafe(`DECLARE board_rank_baseline NO SCROLL CURSOR FOR SELECT b.id,b.at,
    CASE WHEN f.rows IS NOT NULL THEN b.record END record,f.rows facts,h.rows headers
    FROM entity_weapon_builds b CROSS JOIN LATERAL(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.mode) rows
     FROM board_facts s WHERE s.identity=b.id AND s.ruler=$2)f CROSS JOIN LATERAL(
      SELECT coalesce(jsonb_agg(jsonb_build_object('scope',m.scope,'key',m.key,'version',m.version::text)),'[]') rows
      FROM(SELECT 'build'::text scope,b.id key WHERE f.rows IS NOT NULL UNION
       SELECT 'measurement',jsonb_build_array(b.id,$2::text,x->>'mode')::text FROM jsonb_array_elements(f.rows)x UNION
       SELECT 'review',x->>'review_id' FROM jsonb_array_elements(f.rows)x WHERE x->>'review_id' IS NOT NULL
      )k CROSS JOIN LATERAL(SELECT scope,key,version FROM state_board_input_mailbox
       WHERE scope=k.scope AND key=k.key OFFSET 0)m)h WHERE b.weapon=$1`,[weapon,ruler]);
   const started=Date.now();
   for(;;){
    signal?.throwIfAborted();if(Date.now()-started>120000)throw new BoardRankQualificationError('baseline capture exceeded its time budget');
    const page=await tx.unsafe('FETCH FORWARD 256 FROM board_rank_baseline');
    for(const row of page){
     if(!row.facts)continue;
     total++;facts+=row.facts.length;await append({kind:'source',source:{id:row.id,at:row.at,record:row.record},facts:row.facts,headers:row.headers});
    }
    if(page.length<256)break;
   }
   await tx.unsafe('CLOSE board_rank_baseline');await append({kind:'complete',total,facts,snapshot});
   return meta;
  });
  await file.sync();await file.close();await chmod(path,0o440);syncRankDirectory(directory);syncRankDirectory(work);
  return {...metadata,path,bytes,total,facts,sha256:checksum.digest('hex')};
 }catch(error){
  await file.close();
  if(error.code==='57014')throw new BoardRankQualificationError('baseline capture exceeded its SQL statement budget: '+error.message,{cause:error});
  throw error;
 }
}

export async function beginRankBaseline(tx,manifest) {
 await tx.unsafe('SELECT board_rank_source_lock($1::text)',[manifest.source]);
 const part=await createRankPartition(tx,{weapon:manifest.weapon,ruler:manifest.ruler,source:manifest.source,native:manifest.native,cut:manifest.sha256});
 if(part.path&&part.path!==manifest.path)throw new BoardRankQualificationError('baseline staging path changed');
 const [row]=await tx.unsafe('UPDATE state_board_rank_partitions SET path=$3,bytes=$4,total=$5 WHERE weapon=$1 AND ruler=$2 RETURNING *',
  [manifest.weapon,manifest.ruler,manifest.path,manifest.bytes,manifest.total]);
 return row;
}

export async function verifyRankBaseline(part,work,verified=new Map()) {
 const path=resolve(part.path),directory=dirname(path),root=resolve(boardStorage(work));
 if(dirname(directory)!==root||!/^rank-cut-[a-f0-9-]{36}$/.test(basename(directory))||basename(path)!=='inputs.ndjson'
  ||realpathSync(path)!==path||lstatSync(path).isSymbolicLink()||statSync(directory).st_dev!==statSync(root).st_dev)
  throw new BoardRankQualificationError('baseline work root changed');
 const state=statSync(path),fingerprint=[state.dev,state.ino,state.size,state.mtimeMs,part.cut].join(':');
 if(Number(part.bytes)!==state.size)throw new BoardRankQualificationError('baseline staging size changed');
 if(verified.get(path)!==fingerprint){
  const checksum=createHash('sha256');for await(const block of createReadStream(path))checksum.update(block);
  if(checksum.digest('hex')!==part.cut)throw new BoardRankQualificationError('baseline staging digest changed');
  verified.set(path,fingerprint);
 }
 return path;
}

export async function prepareRankBaselinePage(sql,native,part,{work,verified=new Map()}) {
 const path=await verifyRankBaseline(part,work,verified),page=rankFilePage(path,Number(part.cursor),64);
 const sources=page.rows.filter(row=>row.kind==='source');
 for(const row of page.rows){
  if(row.kind==='meta'&&(row.source!==part.source||row.native!==part.native||row.weapon!==part.weapon||row.ruler!==part.ruler))
   throw new BoardRankQualificationError('baseline source context changed');
  if(!['source','meta','complete'].includes(row.kind))throw new BoardRankQualificationError('unknown baseline input');
 }
 if(page.done&&(page.rows.at(-1)?.kind!=='complete'||Number(page.rows.at(-1).total)!==Number(part.total)))
  throw new BoardRankQualificationError('baseline terminal receipt is missing');
 const all=sources.map(row=>({identity:row.source.id,record:JSON.parse(row.source.record),facts:row.facts}));
 const cached=await cachedRankInputs(sql,part,all);
 const live=new Set(cached.filter(r=>r.live).map(r=>r.identity));
 const pending=sources.filter(row=>!live.has(row.source.id)),inputs=all.filter(row=>!live.has(row.identity));
 const prepared=await prepareRankSources(native,{weapon:part.weapon,ruler:part.ruler,inputs,cached});
 for(const source of prepared.sources){
  const input=inputs.find(row=>row.identity===source.identity),modes=source.descriptor.publication_modes||[];
  source.dependencies=[{scope:'build',key:source.identity,version:null},
   ...modes.map(mode=>({scope:'measurement',key:rankMeasurementKey(source.identity,part.ruler,mode.id),version:null})),
   ...[...new Set(input.facts.map(row=>row.review_id).filter(Boolean))].map(key=>({scope:'review',key,version:null}))];
  const headers=pending.find(row=>row.source.id===source.identity).headers||[];
  for(const dependency of source.dependencies)dependency.version=headers.find(row=>row.scope===dependency.scope&&row.key===dependency.key)?.version??null;
 }
 return {prepared,offset:page.offset,loaded:Number(part.loaded)+sources.length,done:page.done};
}

export async function applyRankBaselinePage(sql,part,page) {
 if(page.loaded>Number(part.total))throw new BoardRankQualificationError('baseline source coverage exceeded its receipt');
 return sql.begin(async tx=>{
  const updated=await applyRankSources(tx,part,page.prepared,{cursor:page.offset,loaded:page.loaded});
  if(page.done){
   if(page.loaded!==Number(part.total)||page.offset!==Number(part.bytes))throw new BoardRankQualificationError('baseline source coverage is incomplete');
  }
  await tx.unsafe('UPDATE state_board_rank_partitions SET seeded=$3,available_at=board_input_timestamp(clock_timestamp()) WHERE weapon=$1 AND ruler=$2',[part.weapon,part.ruler,page.done]);
  return {...updated,seeded:page.done};
 });
}
