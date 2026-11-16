// SPDX-License-Identifier: AGPL-3.0-or-later
// Preserve Native JSON literals and byte cursors without loading an entire file.
import {openSync,closeSync,readSync,statSync,fsyncSync} from 'node:fs';
export function rankFilePage(path,offset,limit=64,{maxBytes=4*1024*1024,maxRowBytes=8*1024*1024}={}) {
 if(!Number.isSafeInteger(offset)||offset<0||!Number.isSafeInteger(limit)||limit<1||limit>256)throw Error('invalid rank file cursor');
 const size=statSync(path).size;if(offset>size)throw Error('rank file cursor exceeds its input');
 const fd=openSync(path,'r'),buffer=Buffer.allocUnsafe(65536),rows=[],literals=[];
 let position=offset,next=offset,readAt=offset,cursor=0,available=0,total=0;
 const line=()=>{
  const parts=[];let bytes=0;
  for(;;){
   if(cursor===available){available=readSync(fd,buffer,0,buffer.length,readAt);cursor=0;readAt+=available;if(!available)return bytes?Buffer.concat(parts,bytes):null;}
   let end=buffer.indexOf(10,cursor);if(end<0||end>=available)end=available;
   const part=buffer.subarray(cursor,end);bytes+=part.length;if(bytes>maxRowBytes)throw Error('rank input row exceeds its byte budget');
   parts.push(Buffer.from(part));position+=end-cursor;cursor=end;
   if(end<available){cursor++;position++;return Buffer.concat(parts,bytes);}
  }
 };
 try{
  for(;;){
   const bytes=line();if(bytes===null)break;
   if(rows.length&&total+bytes.length>maxBytes)break;
   next=position;const text=bytes.toString('utf8');if(text.trim()){rows.push(JSON.parse(text));literals.push(text);total+=bytes.length;}
   if(rows.length===limit)break;
  }
 }finally{closeSync(fd);}
 return {rows,literals,offset:next,done:next===size};
}
export function syncRankDirectory(path) {
 const fd=openSync(path,'r');try{fsyncSync(fd);}finally{closeSync(fd);}
}
