// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {constants,lstatSync,realpathSync,statSync} from 'node:fs';
import {link,mkdir,open,unlink} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {verifyRankBaseline} from './board_rank_baseline.mjs';
import {rankFilePage,syncRankDirectory} from './board_rank_files.mjs';
import {guardRankSpace} from './board_rank_storage.mjs';

function cutName(digest){
 const s=digest.slice(0,32);
 return 'rank-cut-'+[s.slice(0,8),s.slice(8,12),s.slice(12,16),s.slice(16,20),s.slice(20)].join('-');
}

export async function readerCut(work,{action,sha256,bytes,name},source=process.stdin){
 const root=guardRankSpace(work),identity={root,device:statSync(root,{bigint:true}).dev.toString(),uid:process.getuid(),gid:process.getgid()};
 assert(root===resolve(work)&&realpathSync(root)===root,'reader root changed');
 if(action==='probe')return identity;
 assert(['stage','verify'].includes(action)&&/^[0-9a-f]{64}$/.test(sha256)&&Number.isSafeInteger(bytes)&&bytes>0&&bytes<=512*1024**2,
  'invalid reader cut request');
 if(action==='stage')name=cutName(sha256);
 assert(/^rank-cut-[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(name),'invalid cut directory');
 const dir=join(root,name),path=join(dir,'inputs.ndjson');
 if(action==='stage'){
  try{await mkdir(dir,{mode:0o750});}catch(e){if(e.code!=='EEXIST')throw e;}
 }
 const place=lstatSync(dir);
 assert(place.isDirectory()&&realpathSync(dir)===dir&&place.dev===statSync(root).dev&&place.uid===identity.uid,'unexpected reader cut directory');
 if(action==='stage'&&!lstatSync(path,{throwIfNoEntry:false})){
  const tmp=join(dir,'inputs.partial');
  const partial=lstatSync(tmp,{throwIfNoEntry:false});
  if(partial){assert(partial.isFile()&&partial.uid===identity.uid&&partial.nlink===1,'unexpected partial cut');await unlink(tmp);}
  const fd=await open(tmp,constants.O_EXCL|constants.O_CREAT|constants.O_WRONLY|constants.O_NOFOLLOW,0o600);
  try{
   const info=await fd.stat();assert(info.isFile()&&info.nlink===1&&info.uid===identity.uid,'unexpected partial cut');
   const hash=createHash('sha256');let size=0;
   for await(const block of source){
    size+=block.length;assert(size<=bytes,'cut exceeds its sealed size');hash.update(block);await fd.writeFile(block);
   }
   assert(size===bytes&&hash.digest('hex')===sha256,'transferred cut differs from the sealed input');
   await fd.chmod(0o440);await fd.sync();
  }catch(e){await fd.close();await unlink(tmp);throw e;}
  await fd.close();
  // No existing immutable file can be overwritten, including on a retried transfer.
  await link(tmp,path);await unlink(tmp);syncRankDirectory(dir);syncRankDirectory(root);
 }
 const final=lstatSync(path);assert(final.isFile()&&final.uid===identity.uid&&!(final.mode&0o222),'cut must be immutable and readable by its owner');
 await verifyRankBaseline({path,bytes,cut:sha256},root);
 if(action==='stage'){
  const tmp=join(dir,'inputs.partial'),p=lstatSync(tmp,{throwIfNoEntry:false});
  if(p&&p.dev===final.dev&&p.ino===final.ino){await unlink(tmp);syncRankDirectory(dir);}
 }
 return {...identity,path,bytes,sha256,meta:rankFilePage(path,0,1).rows[0]};
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url){
 const [root,request]=process.argv.slice(2);console.log(JSON.stringify(await readerCut(root,JSON.parse(request))));
}
