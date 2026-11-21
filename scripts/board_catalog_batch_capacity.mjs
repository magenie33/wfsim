// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {statfsSync} from 'node:fs';

const gib=1024**3;
const free=path=>{const stat=statfsSync(path,{bigint:true});return Number(stat.bavail*stat.bsize);};

export async function sampleCapacity(journal,sql,{work,budget,contract},available=free){
 assert(budget.preliminary_capacity_pass===true&&budget.source_cut?.snapshot===contract.snapshot&&
  budget.scope_count===contract.scopes,'capacity forecast differs from the qualified cut');
 const [row]=await sql.unsafe('SELECT coalesce(sum(size),0)::text bytes FROM pg_ls_waldir()');
 const data=available(work),root=available('/'),wal=Number(row.bytes);
 const total=budget.budget_total,walBudget=budget.root_required;
 assert([data,root,wal,total,walBudget,budget.wal_observed].every(n=>Number.isSafeInteger(n)&&n>=0),'invalid capacity reading');
 const initial=journal.prepare('SELECT * FROM high_water WHERE id=1').get();
 journal.prepare(`INSERT INTO high_water VALUES(1,?,?,?,?,?,1) ON CONFLICT(id) DO UPDATE SET
  min_data=min(min_data,excluded.min_data),min_root=min(min_root,excluded.min_root),max_wal=max(max_wal,excluded.max_wal),samples=samples+1`)
 .run(data,root,data,root,wal);
 const used=Math.max(0,(initial?.first_data??data)-data);
 assert(data>=32*gib&&root>=gib,'live filesystem reserve exhausted');
 assert(used<=total&&wal<=walBudget,'live allocation exceeds the qualified forecast');
 // A volume-wide free-space change cannot discharge the catalog's reserved forecast.
 assert(data-32*gib>=total&&root-gib>=Math.max(0,walBudget-wal),'insufficient remaining Data or WAL headroom');
 return {data,root,wal,used};
}
