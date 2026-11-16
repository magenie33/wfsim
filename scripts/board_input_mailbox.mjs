// Projection writes and the source acknowledgement share one short database transaction.
import {createHash,randomUUID} from 'node:crypto';

function captured(input) {
 const payload=input.payload??'null';
 if(typeof payload!=='string'||Buffer.byteLength(payload)>8*1024*1024
   ||createHash('sha256').update(payload).digest('hex')!==input.payload_sha256)throw Error('captured board input changed');
 const body=JSON.parse(payload);
 if(typeof input.deleted!=='boolean'||(input.deleted?body!==null:!body||typeof body!=='object'||Array.isArray(body)))
  throw Error('invalid captured board input');
 return {...input,body};
}

export class BoardInputMailbox {
 constructor(sql,{source}) {
  if(typeof source!=='string'||!source||Buffer.byteLength(source)>128)throw Error('invalid board input source');
  this.sql=sql;this.source=source;
 }
 async claim({items=64,leaseSeconds=60,token=randomUUID()}={}) {
  const [{claim}]=await this.sql.unsafe('SELECT board_input_claim($1::bigint,$2::text,$3::bigint,$4::text) claim',[items,token,leaseSeconds,this.source]);
  if(claim.source!==this.source)throw Error('board input source changed');
  return claim;
 }
 async read(item,token,{maxBytes=8*1024*1024}={}) {
  const [{input}]=await this.sql.unsafe('SELECT board_input_read($1::text,$2::text,$3::text,$4::bigint,$5::bigint) input',
   [item.scope,item.key,token,item.version,maxBytes]);
  if(input.status==='input'&&input.source!==this.source)throw Error('board input source changed');
  return input.status==='input'?captured(input):input;
 }
 async apply(input,token,writeProjection) {
  if(input.source!==this.source)throw Error('board input source changed');
  const packet=captured(input);
  if(typeof writeProjection!=='function')throw Error('board projection writer required');
  return this.sql.begin(async tx=>{
   const [{guard}]=await tx.unsafe('SELECT board_input_lock($1::text,$2::text,$3::text,$4::bigint) guard',
    [input.scope,input.key,token,input.version]);
   if(guard.source!==this.source)throw Error('board input source changed');
   if(guard.status==='superseded') {
    const [{ack}]=await tx.unsafe('SELECT board_input_ack($1::text,$2::text,$3::text,$4::bigint) ack',[input.scope,input.key,token,input.version]);
    if(!['acknowledged','already_applied'].includes(ack))throw Error('board input claim expired during acknowledgement');
    return {status:'superseded',version:guard.version};
   }
   if(guard.status!=='owned')return {status:guard.status,version:guard.version};
   if(guard.payload_sha256!==packet.payload_sha256)throw Error('captured board input changed');
   const output=await writeProjection(tx,packet);
   const [{ack}]=await tx.unsafe('SELECT board_input_ack($1::text,$2::text,$3::text,$4::bigint) ack',[input.scope,input.key,token,input.version]);
   if(ack!=='acknowledged')throw Error('board input claim expired during projection');
   return {status:'applied',version:input.version,output};
  });
 }
 async retry(item,token,{error,delaySeconds=1}) {
  const [{retried}]=await this.sql.unsafe('SELECT board_input_retry($1::text,$2::text,$3::text,$4::bigint,$5::text,$6::bigint) retried',
   [item.scope,item.key,token,item.version,String(error),delaySeconds]);
  return retried;
 }
}
