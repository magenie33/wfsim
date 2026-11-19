// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {Socket} from 'node:net';
import {createInterface} from 'node:readline';
import {basename,dirname,join,resolve} from 'node:path';

export async function connectFileOwner(fd,work,device){
 const socket=new Socket({fd,readable:true,writable:true});
 const reader=createInterface({input:socket,crlfDelay:Infinity}),lines=reader[Symbol.asyncIterator]();
 async function request(message){
  socket.write(JSON.stringify(message)+'\n');
  const {value,done}=await lines.next();assert(!done&&value.length<65536,'reader file transfer ended');
  const reply=JSON.parse(value);if(reply.error)throw Error(reply.error);return reply;
 }
 const close=()=>{reader.close();socket.destroy();};
 try{
  const identity=await request({action:'probe'});
  assert(identity.root===resolve(work)&&identity.device===String(device)&&Number.isSafeInteger(identity.uid)&&identity.uid!==process.getuid(),
   'file owner or guarded Data device changed');
  async function checked(message){
   const result=await request(message);
   assert(result.root===identity.root&&result.device===identity.device&&result.uid===identity.uid&&result.gid===identity.gid&&
    result.sha256===message.sha256&&result.bytes===message.bytes&&dirname(dirname(result.path))===identity.root,'reader cut proof changed');
   return result;
  }
  return {close,get alive(){return !socket.destroyed&&!socket.readableEnded;},
   stage:m=>checked({action:'stage',sha256:m.sha256,bytes:m.bytes}),verify:async part=>{
   const name=basename(dirname(part.path));
   assert(part.path===join(identity.root,name,'inputs.ndjson'),'predecessor is outside the reader work area');
   const r=await checked({action:'verify',name,sha256:part.cut,bytes:Number(part.bytes)});
   assert(r.path===part.path,'predecessor cut path changed');return r.path;
  }};
 }catch(e){close();throw e;}
}
