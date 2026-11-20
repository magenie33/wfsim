// SPDX-License-Identifier: AGPL-3.0-or-later
// A board-stage request has one deadline across both pipe backpressure and its reply.
import {openInventoryOperator} from './inventory_operator.mjs';

export async function openBoardOperator(binary,options={}) {
 const timeoutMs=options.timeoutMs??60000;
 if(!Number.isSafeInteger(timeoutMs)||timeoutMs<1)throw Error('invalid board operator deadline');
 const operator=await openInventoryOperator(binary,{...options,timeoutMs}),call=operator.call;
 operator.call=async(...args)=>{
  let timer;
  try {
   return await Promise.race([call(...args),new Promise((_,reject)=>{
    timer=setTimeout(()=>reject(Object.assign(Error('board operator request deadline'),{code:'INVENTORY_OPERATOR_DEADLINE'})),timeoutMs);
   })]);
  }catch(error) {
   try {await operator.close();}catch(cause){error.cause??=cause;}
   throw error;
  }finally{clearTimeout(timer);}
 };
 return operator;
}

export async function openRankInventoryOperator(binary,{policyBinary,policySha}={}) {
 const native=await openBoardOperator(binary);
 let policy;
 try {
  if(!policyBinary&&!policySha&&native.contract.operators.includes('standard_riven'))return native;
  if(!policyBinary||!/^[a-f0-9]{64}$/.test(policySha||''))throw Error('pin the ranking riven policy binary');
  policy=await openBoardOperator(policyBinary);
  if(policy.adapter!==policySha||!policy.contract.operators.includes('standard_riven'))
   throw Error('ranking riven policy native contract changed');
  return {...native,policyAdapter:policy.adapter,
   get healthy(){return native.healthy&&policy.healthy;},
   call:(op,items)=>(op==='standard_riven'?policy:native).call(op,items),
   close:async()=>{await Promise.all([native.close(),policy.close()]);}};
 }catch(error){
  await Promise.all([native.close(),policy?.close()]);throw error;
 }
}
