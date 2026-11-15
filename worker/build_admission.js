// SPDX-License-Identifier: AGPL-3.0-or-later
// Canonical intake has already committed; each leased handoff advances only its own bounded work.
import {iso} from './instant.js';

export async function admitBuildWork(env,now=Date.now(),{max=16,budget_ms=1000}={}) {
  if(!Number.isSafeInteger(max)||max<1||max>128||!Number.isSafeInteger(budget_ms)||budget_ms<1||budget_ms>5000)
    throw Error('invalid build admission budget');
  const db=env.LIBRARY,start=performance.now();let processed=0;
  for(const kind of ['intake','expansion']) {
    if(performance.now()-start>=budget_ms)break;
    const claimed=await db.prepare('SELECT build_admission_claim(?::text,?::text) claim').bind(kind,iso(now)).first();
    const job=claimed?.claim;if(!job)continue;
    try {
      const row=await db.prepare('SELECT build_admission_run(?::text,?::text,?::text,?::bigint,?::text) result')
        .bind(job.kind,job.key,job.lease,max,iso(now)).first();
      processed+=Number(row.result.processed);
    }catch(error) {
      await db.prepare('SELECT build_admission_retry(?::text,?::text,?::text,?::text,?::text)')
        .bind(job.kind,job.key,job.lease,String(error.message||error).slice(0,1000),iso(now)).run().catch(()=>{});
      throw error;
    }
  }
  return processed;
}
