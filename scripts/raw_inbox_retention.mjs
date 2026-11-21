// SPDX-License-Identifier: AGPL-3.0-or-later
import {existsSync, readFileSync, renameSync, statfsSync, writeFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {setTimeout as wait} from 'node:timers/promises';
import {parseArgs} from 'node:util';

export async function rawInboxRetentionPage(sql, {after='', limit=128, apply=false}={}) {
  return sql.begin(apply ? 'isolation level read committed' : 'read only', async tx => {
    await tx.unsafe("SET LOCAL statement_timeout='2s'; SET LOCAL lock_timeout='300ms'; SET LOCAL plan_cache_mode='force_custom_plan'");
    const [{result}]=await tx.unsafe('SELECT build_raw_inbox_retire_page($1::text,$2::integer,$3::boolean) result',[after,limit,apply]);
    return result;
  });
}

const initial=apply=>({v:1,apply,after:'',scanned:0,removed:0,groups:{}});

function checkpoint(path,apply) {
  if(!path || !existsSync(path))return initial(apply);
  const state=JSON.parse(readFileSync(path,'utf8'));
  if(state.v!==1 || state.apply!==apply || typeof state.after!=='string')throw Error('invalid raw inbox checkpoint');
  return state;
}

function save(path,state) {
  if(!path)return;
  const pending=path+'.pending-'+process.pid;
  writeFileSync(pending,JSON.stringify(state)+'\n',{mode:0o600});
  renameSync(pending,path);
}

function addPage(state,page) {
  state.after=page.next??'';
  state.scanned+=page.scanned;
  state.removed+=page.removed;
  for(const [name,{rows,bytes}] of Object.entries(page.groups)) {
    const group=state.groups[name]??={rows:0,bytes:0};
    group.rows+=rows;
    group.bytes+=bytes;
  }
}

function capacityGate(env) {
  const root=Number(env.WFSIM_RAW_INBOX_ROOT_RESERVE_BYTES);
  if(!Number.isSafeInteger(root) || root<=0)throw Error('set a reviewed raw inbox root reserve in bytes');
  const psi=kind=>{
    const value=readFileSync('/proc/pressure/'+kind,'utf8').match(/^full avg10=(\d+(?:\.\d+)?)/m);
    if(!value)throw Error('invalid host pressure reading');
    return Number(value[1]);
  };
  return ()=>{
    const {bavail,bsize}=statfsSync('/');
    if(bavail*bsize<=root)return 'root_reserve';
    if(psi('io')>=10 || psi('memory')>=2)return 'host_pressure';
    return null;
  };
}

async function main() {
  const {values:{state:path,watch=false,apply=false,limit='128'}}=parseArgs({
    options:{state:{type:'string'},watch:{type:'boolean'},apply:{type:'boolean'},limit:{type:'string'}}
  });
  const rows=Number(limit);
  if(!Number.isInteger(rows) || rows<1 || rows>(apply?128:1000))throw Error('invalid raw inbox batch size');
  if((watch||apply)&&!path)throw Error('watch and apply require a checkpoint path');
  if(!process.env.PG_URL)throw Error('set PG_URL for the library owner');
  const gate=watch||apply?capacityGate(process.env):()=>null;
  const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
  const sql=postgres(process.env.PG_URL,{max:1,connect_timeout:2,
    connection:{application_name:'wfsim-raw-inbox-retention'}});
  let state=checkpoint(path,apply),stopped=false,lastStop;
  process.once('SIGTERM',()=>{stopped=true;});
  process.once('SIGINT',()=>{stopped=true;});
  try {
    while(!stopped) {
      const reason=gate();
      if(reason) {
        if(reason!==lastStop)console.log(JSON.stringify({paused:reason}));
        lastStop=reason;
        if(!watch)break;
        await wait(30_000);
        continue;
      }
      lastStop=undefined;
      let page;
      try {
        page=await rawInboxRetentionPage(sql,{after:state.after,limit:rows,apply});
      } catch(error) {
        if(!watch || !['55P03','57014','40001'].includes(error.code))throw error;
        await wait(5_000);
        continue;
      }
      addPage(state,page);
      if(!page.next) {
        console.log(JSON.stringify({sweep:state}));
        state=initial(apply);
      } else if(!watch)console.log(JSON.stringify(page));
      save(path,state);
      if(!watch)break;
      await wait(page.next?1000:60_000);
    }
  } finally {
    await sql.end();
  }
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url)await main();
