// SPDX-License-Identifier: AGPL-3.0-or-later
// An explicitly configured data disk must remain mounted before any board state is created or changed.
import {existsSync,lstatSync,realpathSync,statSync,statfsSync,accessSync,constants} from 'node:fs';
import {resolve,isAbsolute,dirname,join} from 'node:path';
import {pathToFileURL} from 'node:url';

export function boardStorage(work,{env=process.env,stat=statSync,filesystem=statfsSync}={}) {
 const fields=['WFSIM_BOARD_WORK_ROOT','WFSIM_BOARD_DATA_MOUNT','WFSIM_BOARD_DATA_UUID'];
 if(env.WFSIM_BOARD_STORAGE_REQUIRED!=null&&!['0','1'].includes(env.WFSIM_BOARD_STORAGE_REQUIRED))throw Error('invalid board storage requirement');
 const required=env.WFSIM_BOARD_STORAGE_REQUIRED==='1'||fields.some(key=>env[key]!=null);
 if(!required)return resolve(work);
 if(fields.some(key=>!env[key])||!isAbsolute(env.WFSIM_BOARD_WORK_ROOT)||!isAbsolute(env.WFSIM_BOARD_DATA_MOUNT)
  ||!/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i.test(env.WFSIM_BOARD_DATA_UUID))throw Error('board storage requires an absolute work root, data mount and filesystem UUID');
 const mount=resolve(env.WFSIM_BOARD_DATA_MOUNT),selected=resolve(env.WFSIM_BOARD_WORK_ROOT);
 if(mount==='/'||!existsSync(mount)||lstatSync(mount).isSymbolicLink()||!lstatSync(mount).isDirectory()
  ||realpathSync(mount)!==mount)throw Error('board data mount is missing or linked; no fallback is allowed');
 if(selected!==mount&&!selected.startsWith(mount+'/'))throw Error('board work root is outside its data mount');
 const device=stat(mount,{bigint:true}).dev;
 if(device===stat('/',{bigint:true}).dev||device===stat(dirname(mount),{bigint:true}).dev)throw Error('board data disk is not mounted separately from the system filesystem');
 if(![0xef53,0x58465342].includes(Number(filesystem(mount).type)))throw Error('board SQLite storage requires a local ext4 or xfs data disk');
 const expected=stat(join('/dev/disk/by-uuid',env.WFSIM_BOARD_DATA_UUID),{bigint:true});
 if(!expected.isBlockDevice()||expected.rdev!==device)throw Error('board data mount does not match its filesystem UUID');
 let existing=selected;while(!existsSync(existing))existing=dirname(existing);
 if(realpathSync(existing)!==existing||stat(existing,{bigint:true}).dev!==device)throw Error('board work root resolves outside its selected data disk');
 accessSync(existing,constants.R_OK|constants.W_OK|constants.X_OK);
 return selected;
}
if(process.argv[1]&&pathToFileURL(resolve(process.argv[1])).href===import.meta.url) {
 if(!process.argv[2])throw Error('usage: board_storage.mjs <default-work-root>');
 console.log(boardStorage(process.argv[2]));
}
