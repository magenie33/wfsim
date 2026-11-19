// SPDX-License-Identifier: AGPL-3.0-or-later
import {DatabaseSync} from 'node:sqlite';
import {pathToFileURL} from 'node:url';

export async function startCatalogScan(sql,{source,floor,epoch}){
 if(!epoch||!/^(0|[1-9][0-9]*)$/.test(String(floor)))throw Error('invalid drained mailbox floor');
 return sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1)',[source]);
  await tx.unsafe('SELECT pg_advisory_xact_lock(hashtextextended($1,0))',[source+'/catalog-scan']);
  const [prior]=await tx.unsafe('SELECT * FROM state_board_rank_catalog_scan WHERE id=1');
  if(prior?.closed&&prior.source===source&&prior.epoch===epoch&&String(prior.floor)===String(floor))return prior;
  if(prior&&!prior.closed){
   if(prior.source!==source||prior.epoch!==epoch||String(prior.floor)!==String(floor))throw Error('catalog mailbox boundary changed');
   const [trigger]=await tx.unsafe("SELECT 1 FROM pg_trigger WHERE tgrelid='state_board_input_mailbox'::regclass AND tgname='board_rank_catalog_input' AND tgenabled IN('O','A')");
   if(!trigger)throw Error('catalog change capture is not active');
   return prior;
  }
  await tx.unsafe(`CREATE TRIGGER board_rank_catalog_input AFTER INSERT OR UPDATE OF version ON state_board_input_mailbox
   FOR EACH ROW WHEN(NEW.version>0 AND NEW.scope IN('build','measurement')) EXECUTE FUNCTION board_rank_catalog_input()`);
  const [upper]=await tx.unsafe('SELECT scope,key FROM state_board_input_mailbox ORDER BY scope DESC,key DESC LIMIT 1');
  const [row]=await tx.unsafe(`INSERT INTO state_board_rank_catalog_scan(id,source,floor,upper_scope,upper_key,epoch) VALUES(1,$1,$2,$3,$4,$5)
   ON CONFLICT(id) DO UPDATE SET source=excluded.source,epoch=excluded.epoch,floor=excluded.floor,upper_scope=excluded.upper_scope,upper_key=excluded.upper_key,
   last_scope='',last_key='',complete=false,closed=false RETURNING *`,
   [source,String(floor),upper?.scope||'',upper?.key||'',epoch]);
  return row;
 });
}

export async function scanCatalogPage(sql,source,limit=4096){
 if(!Number.isSafeInteger(limit)||limit<1||limit>4096)throw Error('invalid mailbox page size');
 return sql.begin(async tx=>{
  const [scan]=await tx.unsafe('SELECT * FROM state_board_rank_catalog_scan WHERE id=1 AND source=$1 FOR UPDATE',[source]);
  if(!scan)throw Error('catalog capture has not started');
  if(scan.complete)return scan;
  const [page]=await tx.unsafe(`WITH page AS MATERIALIZED (SELECT scope,key,version FROM state_board_input_mailbox
    WHERE (scope,key)>($1,$2) AND (scope,key)<=($3,$4) ORDER BY scope,key LIMIT $5),
   changes AS MATERIALIZED(SELECT CASE WHEN scope='build' THEN key ELSE key::jsonb->>0 END identity,
    CASE WHEN scope='build' THEN '' ELSE key::jsonb->>1 END ruler FROM page WHERE version>$6 AND scope IN('build','measurement')),
   marked AS(INSERT INTO state_board_rank_catalog_inputs SELECT b.weapon,c.ruler,b.id FROM changes c
    JOIN LATERAL (SELECT id,weapon FROM entity_weapon_builds WHERE id=c.identity OFFSET 0) b ON true
    ORDER BY b.weapon,c.ruler,b.id ON CONFLICT DO NOTHING)
   SELECT count(*) n,(SELECT jsonb_build_array(scope,key) FROM page ORDER BY scope DESC,key DESC LIMIT 1) tail FROM page`,
   [scan.last_scope,scan.last_key,scan.upper_scope,scan.upper_key,limit,scan.floor]);
  const [scope,key]=page.tail||[scan.last_scope,scan.last_key];
  const [row]=await tx.unsafe('UPDATE state_board_rank_catalog_scan SET last_scope=$1,last_key=$2,complete=$3 WHERE id=1 RETURNING *',
   [scope,key,Number(page.n)<limit||(scope===scan.upper_scope&&key===scan.upper_key)]);
  return row;
 });
}

export async function requireCatalogScan(tx,m,epoch){
 const [scan]=await tx.unsafe(`SELECT floor FROM state_board_rank_catalog_scan WHERE id=1 AND complete AND NOT closed AND source=$1 AND epoch=$2
  AND EXISTS(SELECT FROM pg_trigger WHERE tgrelid='state_board_input_mailbox'::regclass
   AND tgname='board_rank_catalog_input' AND tgenabled IN('O','A')) FOR SHARE`,[m.source,epoch]);
 if(!scan||!/^(0|[1-9][0-9]*)$/.test(String(m.mailbox_floor))||BigInt(m.mailbox_floor)<BigInt(scan.floor))
  throw Error('catalog cut has no complete change capture');
}

export async function finishCatalogScan(sql,source,native,scopes,epoch){
 return sql.begin(async tx=>{
  const [scan]=await tx.unsafe('SELECT * FROM state_board_rank_catalog_scan WHERE id=1 AND source=$1 FOR UPDATE',[source]);
  if(!scan?.complete||scan.epoch!==epoch)throw Error('catalog mailbox scan is incomplete');
  if(scan.closed)return {closed:true};
  let checked=0,batch=[];
  async function check(){
   const [missing]=await tx.unsafe(`SELECT k.* FROM jsonb_to_recordset($1::text::jsonb) k(weapon text,ruler text)
    WHERE NOT EXISTS(SELECT 1 FROM state_board_read_partitions s JOIN state_board_rank_partitions p USING(weapon,ruler)
     JOIN record_board_read_snapshots h ON (h.weapon,h.ruler,h.revision)=(s.weapon,s.ruler,s.revision)
     WHERE (s.weapon,s.ruler)=(k.weapon,k.ruler) AND s.enabled AND s.canonical_sources
     AND p.complete AND p.source=$2 AND h.source=$2 AND p.native=$3
     AND (p.catalog_phase IS NULL OR p.catalog_phase='active')) LIMIT 1`,[JSON.stringify(batch),source,native]);
   if(missing)throw Error('catalog selection is incomplete for '+missing.weapon+'/'+missing.ruler);
   checked+=batch.length;batch=[];
  }
  for(const scope of scopes){batch.push(scope);if(batch.length===128)await check();}
  if(batch.length)await check();
  if(!checked||(await tx.unsafe("SELECT 1 FROM state_board_rank_partitions WHERE catalog_phase IN('frozen','replay') LIMIT 1")).length)
   throw Error('catalog registration is incomplete');
  await tx.unsafe('DROP TRIGGER board_rank_catalog_input ON state_board_input_mailbox');
  await tx.unsafe('TRUNCATE state_board_rank_catalog_inputs');
  await tx.unsafe('UPDATE state_board_rank_catalog_scan SET closed=true WHERE id=1');
  return {closed:true,checked};
 });
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url){
 if(process.argv.length!==3&&!(process.argv.length===6&&process.argv[3]==='--finish'))throw Error('expected CAPTURE_DB [--finish ARCHIVE_DB LEDGER_DB]');
 if(!process.env.PG_URL||!process.argv[2])throw Error('PG_URL and sealed CAPTURE_DB required');
 const capture=new DatabaseSync(process.argv[2],{readOnly:true});
 const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
 const sql=postgres(process.env.PG_URL,{max:1,fetch_types:false,connection:{lock_timeout:1000,statement_timeout:10000}});
 try{
  capture.exec('BEGIN');
  const state=k=>JSON.parse(capture.prepare('SELECT v FROM progress WHERE k=?').get(k)?.v||'null');
  const spec=state('spec'),floor=state('floor')??state('start')?.version;
  const cut=state('replay');
  const epoch=JSON.stringify([spec?.source,spec?.cluster,spec?.native,spec?.archive,cut?.snapshot,cut?.captured_at]);
  if(!cut?.snapshot||!spec?.cluster)throw Error('coherent source capture required');
  const [cluster]=await sql.unsafe('SELECT system_identifier::text id FROM pg_control_system()');
  if(cluster.id!==String(spec.cluster))throw Error('catalog database cluster changed');
  if(process.argv[3]==='--finish'){
   const {coverage}=await import('./rank_catalog_coverage.mjs');
   const archive=new DatabaseSync(process.argv[4],{readOnly:true}),ledger=new DatabaseSync(process.argv[5]);
   try{
    if(!coverage(capture,archive,ledger).eligible)throw Error('complete eligible catalog evidence required');
    const receipts=ledger.prepare('SELECT body FROM receipts JOIN evidenced USING(weapon,ruler) ORDER BY weapon,ruler');
    function* scopes(){
     for(const {body} of receipts.iterate()){
      const r=JSON.parse(body);if(r.reason!=='ruler absent from pinned Native')yield {weapon:r.weapon,ruler:r.ruler};
     }
    }
    console.log(JSON.stringify(await finishCatalogScan(sql,spec.source,spec.native,scopes(),epoch)));
   }finally{archive.close();ledger.close();}
  }else{
   let scan=await startCatalogScan(sql,{source:spec.source,floor,epoch});
   while(!scan.complete)scan=await scanCatalogPage(sql,spec.source);
   console.log(JSON.stringify(scan));
  }
 }finally{capture.close();await sql.end();}
}
