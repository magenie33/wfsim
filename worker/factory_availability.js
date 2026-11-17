// SPDX-License-Identifier: AGPL-3.0-or-later
// Availability has its own clock; complete statistics never borrow it.
export const AVAILABILITY_KEY='factory_availability';

export function availabilityWriter(client) {
  return async(statement,allowed)=>{
    const stopped=new Error('availability owner stopped');
    try{return await client.transaction(async tx=>{
      if(!await allowed())throw stopped;
      await tx.query("SELECT set_config('statement_timeout','3s',true),set_config('lock_timeout','250ms',true)",[]);
      if(!await allowed())throw stopped;
      const result=await tx.query(statement.sql,statement.args);
      if(!await allowed())throw stopped;
      return result.affected>0;
    });}catch(error){if(error===stopped)return false;throw error;}
  };
}

export async function cacheAvailability(db,sample,{write,allowed=()=>true}={}) {
  if(!write||!await allowed())return false;
  const {at,open,asking,pack_max,epoch}=sample;
  if(typeof at!=='string'||!Number.isFinite(Date.parse(at))||new Date(at).toISOString()!==at
    ||![open,asking,pack_max].every(value=>Number.isSafeInteger(value)&&value>=0)
    ||pack_max<1||!(epoch===null||typeof epoch==='string'))throw Error('invalid availability sample');
  const body=JSON.stringify({v:1,open,asking,pack_max,epoch});
  const statement=db.prepare(`INSERT INTO state_factory_findings(kind,body,at)
    SELECT $1::text,$2::text,$3::text WHERE $4::text IS NOT DISTINCT FROM compute_order_cut_epoch()
    ON CONFLICT(kind)DO UPDATE SET body=excluded.body,at=excluded.at
    WHERE state_factory_findings.at<=excluded.at`).bind(AVAILABILITY_KEY,body,at,epoch);
  return write(statement,allowed);
}

export async function publishAvailability(db,sample,{write,allowed=()=>true,publish}) {
  const results=await Promise.allSettled([
    cacheAvailability(db,sample,{write,allowed}),
    Promise.resolve().then(async()=>await allowed()?publish():null),
  ]);
  const failed=results.filter(result=>result.status==='rejected');
  if(failed.length)throw new AggregateError(failed.map(result=>result.reason),failed.map(result=>result.reason.message).join('; '));
  return results[1].value;
}
