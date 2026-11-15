// SPDX-License-Identifier: AGPL-3.0-or-later
// An explicit allocation covers every generated file and enforces the host reserve before bounded writes.
import {readFileSync,lstatSync,readdirSync,statfsSync} from 'node:fs';
import {resolve,join} from 'node:path';

function allocated(path) {
 const stat=lstatSync(path);if(stat.isSymbolicLink())throw Error('capacity output cannot contain links');
 if(!stat.isDirectory())return stat.size;
 return readdirSync(path).reduce((n,name)=>n+allocated(join(path,name)),0);
}
export function offlineCapacityGuard(path,work) {
 if(!path)return {check(){},report(){return null;}};
 const allocation=JSON.parse(readFileSync(path,'utf8')),root=resolve(allocation.directory);work=resolve(work);
 if(work!==root&&!work.startsWith(root+'/'))throw Error('replay output is outside its explicit capacity allocation');
 if(!Number.isSafeInteger(allocation.max_new_bytes)||allocation.max_new_bytes<1
  ||!Number.isSafeInteger(allocation.minimum_host_free_bytes)||allocation.minimum_host_free_bytes<8*1024**3)throw Error('invalid offline capacity allocation');
 const device=String(lstatSync(root).dev);let last=0,used=0,peak=0,free=0;
 const guard={check(force=false) {
  if(String(lstatSync(root).dev)!==device)throw Error('offline capacity filesystem changed');
  const fs=statfsSync(root);free=fs.bavail*fs.bsize;
  if(free<allocation.minimum_host_free_bytes)throw Error('offline capacity reached the host free-space reserve');
  if(force||Date.now()-last>=1000) {
   last=Date.now();used=allocated(root);peak=Math.max(peak,used);
   if(used>allocation.max_new_bytes)throw Error('offline capacity allocation exceeded; checkpoints retained');
  }
 },report:()=>({root,maximum_growth_bytes:allocation.max_new_bytes,minimum_host_free_bytes:allocation.minimum_host_free_bytes,
  measured_growth_bytes:used,peak_growth_bytes:peak,host_free_bytes:free})};guard.check(true);return guard;
}
