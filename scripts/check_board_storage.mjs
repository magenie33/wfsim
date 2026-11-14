// SPDX-License-Identifier: AGPL-3.0-or-later
// Missing, substituted or remote storage cannot create a replacement board store on the system disk.
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,existsSync,realpathSync,rmSync,symlinkSync,chmodSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {spawnSync} from 'node:child_process';
import {boardStorage} from './board_storage.mjs';
import {BoardPipeline} from './board_pipeline.mjs';
import {runPrepareBoardCycle} from './split_board_pipeline.mjs';

const root=realpathSync(mkdtempSync(join(tmpdir(),'board-storage-'))),mount=join(root,'data'),work=join(mount,'board');mkdirSync(mount);
const uuid='12345678-1234-1234-1234-123456789abc',env={WFSIM_BOARD_STORAGE_REQUIRED:'1',WFSIM_BOARD_WORK_ROOT:work,WFSIM_BOARD_DATA_MOUNT:mount,WFSIM_BOARD_DATA_UUID:uuid};
let mounted=true,device=2n,expected=2n,type=0xef53;
const probe={env,stat:path=>path.startsWith('/dev/disk/by-uuid/')?{rdev:expected,isBlockDevice:()=>true}
 :{dev:mounted&&(path===mount||path.startsWith(mount+'/'))?device:1n},filesystem:()=>({type})};
try {
 assert.equal(boardStorage(join(root,'legacy'),{env:{}}),join(root,'legacy'));
 assert.equal(boardStorage(join(root,'legacy'),probe),work);assert.equal(existsSync(work),false);
 if(process.getuid?.()!==0) {
  chmodSync(mount,0o500);assert.throws(()=>boardStorage(work,probe),/EACCES/);chmodSync(mount,0o700);
 }
 mounted=false;assert.throws(()=>boardStorage(work,probe),/not mounted separately/);mounted=true;
 expected=3n;assert.throws(()=>boardStorage(work,probe),/filesystem UUID/);expected=2n;
 for(const remote of [0x6969,0x65735546,0x1021994]){type=remote;assert.throws(()=>boardStorage(work,probe),/local ext4 or xfs/);}type=0x58465342;
 assert.equal(boardStorage(work,probe),work);device=1n;assert.throws(()=>boardStorage(work,probe),/system filesystem/);device=2n;
 assert.throws(()=>boardStorage(work,{...probe,env:{...env,WFSIM_BOARD_WORK_ROOT:join(root,'outside')}}),/outside its data mount/);
 assert.throws(()=>boardStorage(work,{env:{WFSIM_BOARD_WORK_ROOT:work}}),/requires an absolute/);
 const linked=join(root,'linked');symlinkSync(mount,linked);assert.throws(()=>boardStorage(work,{...probe,env:{...env,WFSIM_BOARD_DATA_MOUNT:linked}}),/linked/);
 symlinkSync(root,work);assert.throws(()=>boardStorage(work,probe),/resolves outside/);rmSync(work);
 const bad={...process.env,WFSIM_BOARD_STORAGE_REQUIRED:'1',WFSIM_BOARD_WORK_ROOT:work,WFSIM_BOARD_DATA_MOUNT:join(root,'absent'),WFSIM_BOARD_DATA_UUID:uuid};
 const saved=Object.fromEntries(Object.keys(env).map(key=>[key,process.env[key]]));Object.assign(process.env,bad);
 try {
  assert.throws(()=>new BoardPipeline(work),/mount is missing/);
  await assert.rejects(runPrepareBoardCycle({work,sql:{unsafe:()=>{throw Error('database must not be contacted');}}}),/mount is missing/);
 } finally {for(const [key,value]of Object.entries(saved))if(value===undefined)delete process.env[key];else process.env[key]=value;}
 for(const [program,args]of [['bash',['scripts/live_board.sh',work,join(root,'bin')]],
  [process.execPath,['scripts/inventory_rebuild.mjs','status','--work',work]]]) {
  const result=spawnSync(program,args,{env:bad,encoding:'utf8',timeout:10000});assert.notEqual(result.status,0);assert.match(result.stderr,/mount is missing/);
 }
 assert.equal(existsSync(work),false);assert.equal(existsSync(join(root,'absent')),false);
 console.log('ok: configured local ext4/xfs UUID and work path bind; missing/wrong/system/remote/linked disks fail; actual shell, producer, publisher and rebuild entry points create no fallback state or database request');
}finally{rmSync(root,{recursive:true,force:true});}
