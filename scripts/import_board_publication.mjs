// SPDX-License-Identifier: AGPL-3.0-or-later
// Import published wire rows once. Concurrent Native publications always keep their selection.
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

const hash=value=>createHash('sha256').update(value).digest('hex');
const validName=value=>typeof value==='string'&&/^[a-z0-9_]{1,160}$/.test(value);

function splitRulers(rows) {
  if(!Array.isArray(rows))throw Error('published rows must be an array');
  const groups=new Map();
  for(const row of rows){
    if(!validName(row?.benchmark)||!Number.isFinite(row.score))throw Error('invalid published ranking');
    if(!groups.has(row.benchmark))groups.set(row.benchmark,[]);
    groups.get(row.benchmark).push(row);
  }
  return groups;
}

function readPinned(file,digest) {
  const bytes=readFileSync(file);
  if(hash(bytes)!==digest)throw Error('published file changed: '+file);
  return JSON.parse(bytes);
}

function inventoryScenes(inventory) {
  const digest=value=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
  if(inventory.v!==1||!digest(inventory.stamp)||!digest(inventory.digest)||!Array.isArray(inventory.files)||!inventory.files.length)
    throw Error('invalid published inventory');
  const scenes=new Map();
  for(const item of inventory.files){
    const {scene,name,file}=item;
    if((scene!==''&&!validName(scene))||!validName(name)||!digest(item.digest)||file!==(scene?scene+'/':'')+name+'.json')
      throw Error('invalid published file');
    if(!scenes.has(scene))scenes.set(scene,new Map());
    if(scenes.get(scene).has(name))throw Error('duplicate published file: '+file);
    scenes.get(scene).set(name,item);
  }
  for(const files of scenes.values())if(!files.has('index'))throw Error('missing published index');
  return scenes;
}

export function* firstPublications(root,inventoryPath,expected) {
  const inventory=readPinned(inventoryPath,expected);
  const scenes=inventoryScenes(inventory);
  const meta=readPinned(join(root,'board.meta.json'),inventory.stamp);
  if(!Number.isSafeInteger(meta.scored_at)||meta.scored_at<0)throw Error('invalid publication time');
  const at=new Date(meta.scored_at*1000).toISOString(),seen=new Set();
  for(const [scene,files] of scenes){
    const index=files.get('index');
    const read=f=>readPinned(join(root,'board',f.file),f.digest);
    const leaders=read(index);
    for(const w of Object.keys(leaders))if(!files.has(w)&&leaders[w].length)throw Error('leaders without published file: '+w);
    for(const f of files.values()){
      if(f===index)continue;
      const groups=splitRulers(read(f)),heads=splitRulers(leaders[f.name]||[]);
      for(const ruler of heads.keys())if(!groups.has(ruler))throw Error('leaders without published rows: '+f.name+'/'+ruler);
      for(const [ruler,rows] of groups){
        const key=f.name+'/'+ruler;
        if(seen.has(key))throw Error('published scope in multiple scenes: '+key);
        seen.add(key);
        const top=heads.get(ruler)||[];
        const revision=hash(JSON.stringify(['published-board-v1',expected,scene,f.name,ruler,f.digest,index.digest]));
        yield {weapon:f.name,ruler,scene,revision,archive:expected,at,rows,leaders:top};
      }
    }
  }
}

async function importFirstPublication(sql,part) {
  const {weapon,ruler,scene,revision,archive,at,rows,leaders}=part;
  return sql.begin(async tx=>{
    const claimed=await tx.unsafe(`INSERT INTO state_board_public_heads(weapon,ruler,scene,revision)
      VALUES($1,$2,$3,$4) ON CONFLICT(weapon,ruler) DO NOTHING RETURNING revision`,[weapon,ruler,scene,revision]);
    if(!claimed.length)return false;
    await tx.unsafe(`INSERT INTO record_board_read_snapshots
      (weapon,ruler,revision,source,engine,inputs_sha256,rows,leaders,captured_at,published_at)
      VALUES($1,$2,$3,$4,'',$5,$6,$7::text::jsonb,$8,$8)`,[weapon,ruler,revision,'archive:'+archive,archive,rows.length,JSON.stringify(leaders),at]);
    for(let start=0;start<rows.length;start+=128){
      await tx.unsafe(`INSERT INTO state_board_read_rows(weapon,ruler,revision,ordinal,body)
        SELECT $1,$2,$3,$4::bigint+n,body FROM json_array_elements_text($5::text::json) WITH ORDINALITY AS page(body,n)`,
        [weapon,ruler,revision,start,JSON.stringify(rows.slice(start,start+128))]);
    }
    return true;
  });
}

export async function importPublicBoard(sql,root,inventoryPath,expected) {
  const parts=firstPublications(root,inventoryPath,expected);
  const first=parts.next();
  if(first.done)throw Error('empty published board');
  await sql.begin(async tx=>{
    await tx.unsafe('INSERT INTO state_board_public_import(id,archive) VALUES(1,$1) ON CONFLICT DO NOTHING',[expected]);
    const [pin]=await tx.unsafe('SELECT archive FROM state_board_public_import WHERE id=1');
    if(pin.archive!==expected)throw Error('another published inventory is already selected');
  });
  let inserted=0,existing=0,rows=0;
  for(let item=first;!item.done;item=parts.next()){
    const part=item.value;
    if(await importFirstPublication(sql,part)){inserted++;rows+=part.rows.length;}
    else existing++;
  }
  return {archive:expected,inserted,existing,rows};
}

if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
  const [root,inventoryPath,expected]=process.argv.slice(2);
  if(!root||!inventoryPath||!/^[a-f0-9]{64}$/.test(expected)||!process.env.PG_URL)throw Error('expected SITE_ROOT INVENTORY_JSON INVENTORY_SHA and PG_URL');
  const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
  const sql=postgres(process.env.PG_URL,{max:1,fetch_types:false,connection:{lock_timeout:2000,statement_timeout:30000}});
  try{console.log(JSON.stringify(await importPublicBoard(sql,root,inventoryPath,expected)));}finally{await sql.end();}
}
