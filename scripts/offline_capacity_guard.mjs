// SPDX-License-Identifier: AGPL-3.0-or-later
// An explicit allocation covers every generated file and enforces the host reserve before bounded writes.
import {readFileSync,lstatSync,statfsSync} from 'node:fs';
import {resolve} from 'node:path';
import {assertCapacityDirectory,capacityDirectoryBytes} from './capacity_directory_bytes.mjs';
export function offlineCapacityGuard(path,work) {
 if(!path)return {check(){},report(){return null;}};
 const allocation=JSON.parse(readFileSync(path,'utf8')),root=resolve(allocation.directory);work=resolve(work);
 if(work!==root&&!work.startsWith(root+'/'))throw Error('replay output is outside its explicit capacity allocation');
 if(!Number.isSafeInteger(allocation.max_new_bytes)||allocation.max_new_bytes<1
  ||!Number.isSafeInteger(allocation.minimum_host_free_bytes)||allocation.minimum_host_free_bytes<8*1024**3)throw Error('invalid offline capacity allocation');
 const identity=lstatSync(root);let last=0,used=0,peak=0,free=0;
 const guard={check(force=false) {
  assertCapacityDirectory(root,identity);
  const fs=statfsSync(root);free=fs.bavail*fs.bsize;
  if(free<allocation.minimum_host_free_bytes)throw Error('offline capacity reached the host free-space reserve');
  if(force||Date.now()-last>=1000) {
   used=capacityDirectoryBytes(root,identity);peak=Math.max(peak,used);
   if(used>allocation.max_new_bytes)throw Error('offline capacity allocation exceeded; checkpoints retained');
   last=Date.now();
  }
 },report:()=>({root,maximum_growth_bytes:allocation.max_new_bytes,minimum_host_free_bytes:allocation.minimum_host_free_bytes,
  measured_growth_bytes:used,peak_growth_bytes:peak,host_free_bytes:free})};guard.check(true);return guard;
}
