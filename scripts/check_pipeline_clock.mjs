// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {pipelineClock} from './pipeline_clock.mjs';

export async function pipelineClockCases() {
  let release,entered,ran=0,parallel=0,peak=0;
  const held=new Promise(resolve=>{release=resolve;});
  const started=new Promise(resolve=>{entered=resolve;});
  const errors=[],clock=pipelineClock({onError:(name,error)=>errors.push({name,message:error.message})});
  const waitFor=async predicate=>{
    const until=Date.now()+1000;
    while(!predicate()){assert(Date.now()<until,'pipeline clock did not progress');await new Promise(resolve=>setTimeout(resolve,1));}
  };
  try {
    clock.add('delivery',10000,async()=>{parallel++;peak=Math.max(peak,parallel);ran++;if(ran===1){entered();await held;}parallel--;});
    await started;
    for(let i=0;i<50;i++)clock.wake('delivery');
    let verified=false;clock.add('verification',10000,async()=>{verified=true;});
    await waitFor(()=>verified);
    assert.equal(ran,1);assert.equal(peak,1);
    release();await waitFor(()=>ran===2);
    assert.equal(peak,1,'notifications overlapped the same consumer');
    assert.equal(clock.wake('missing'),false);
    let attempts=0;
    clock.add('retry',5,async()=>{if(++attempts===1)throw Error('temporary');});
    await waitFor(()=>attempts>=2);assert.equal(errors[0].message,'temporary');
    clock.stop();await clock.drain();const finished=ran;
    assert.equal(clock.wake('delivery'),false);
    await new Promise(resolve=>setTimeout(resolve,10));assert.equal(ran,finished);
  }finally{release();clock.stop();await clock.drain();}
  console.log('ok: wakeups coalesce, slow consumers stay independent, errors retry and shutdown drains');
}
