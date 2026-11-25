// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {statfsSync,statSync} from 'node:fs';
import {join} from 'node:path';

const gib=1024**3;
const free=path=>{const stat=statfsSync(path,{bigint:true});return Number(stat.bavail*stat.bsize);};

export function rankWalVolume(work,dataDirectory,stat=statSync){
 assert(typeof dataDirectory==='string'&&dataDirectory.startsWith('/'),'absolute WAL data directory required');
 const root=stat('/',{bigint:true}).dev,data=stat(work,{bigint:true}).dev,wal=stat(join(dataDirectory,'pg_wal'),{bigint:true}).dev;
 assert(root!==data,'ranking Data volume must differ from root');
 if(wal===data)return 'data';
 if(wal===root)return 'root';
 throw Error('WAL volume is outside the qualified Data and root filesystems');
}

export async function sampleCapacity(journal,sql,{work,budget,contract},available=free){
 assert(budget.preliminary_capacity_pass===true&&budget.source_cut?.snapshot===contract.snapshot&&
  budget.scope_count===contract.scopes,'capacity forecast differs from the qualified cut');
 const [row]=await sql.unsafe("SELECT coalesce(sum(size),0)::text bytes,current_setting('data_directory') data_directory FROM pg_ls_waldir()");
 const data=available(work),root=available('/'),wal=Number(row.bytes);
 const total=budget.budget_total,walBudget=budget.root_required,nonWal=budget.non_wal_budget_bytes??total;
 const walVolume=process.env.WFSIM_BOARD_STORAGE_REQUIRED==='1'?rankWalVolume(work,row.data_directory):budget.wal_on_root===false?'data':'root';
 assert([data,root,wal,total,walBudget,budget.wal_observed,nonWal].every(n=>Number.isSafeInteger(n)&&n>=0)&&nonWal<=total,'invalid capacity reading');
 const initial=journal.prepare('SELECT * FROM high_water WHERE id=1').get();
 journal.prepare(`INSERT INTO high_water VALUES(1,?,?,?,?,?,1) ON CONFLICT(id) DO UPDATE SET
  min_data=min(min_data,excluded.min_data),min_root=min(min_root,excluded.min_root),max_wal=max(max_wal,excluded.max_wal),samples=samples+1`)
 .run(data,root,data,root,wal);
 const used=Math.max(0,(initial?.first_data??data)-data);
 assert(data>=32*gib&&root>=gib,'live filesystem reserve exhausted');
 assert(used<=total&&wal<=walBudget,'live allocation exceeds the qualified forecast');
 // A volume-wide free-space change cannot discharge the catalog's reserved forecast.
 const remainingWal=Math.max(0,walBudget-wal);
 const dataRequired=walVolume==='data'?Math.max(total,nonWal+remainingWal):total;
 const rootRequired=walVolume==='root'?remainingWal:0;
 assert(data-32*gib>=dataRequired&&root-gib>=rootRequired,'insufficient remaining Data or WAL headroom');
 return {data,root,wal,used,walVolume,dataRequired,rootRequired};
}
