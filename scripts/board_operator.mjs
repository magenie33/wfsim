// SPDX-License-Identifier: AGPL-3.0-or-later
// A board-stage request has one deadline across both pipe backpressure and its reply.
import {openInventoryOperator} from './inventory_operator.mjs';

export async function openBoardOperator(binary,options={}) {
 const timeoutMs=options.timeoutMs??60000;
 if(!Number.isSafeInteger(timeoutMs)||timeoutMs<1)throw Error('invalid board operator deadline');
 const operator=await openInventoryOperator(binary,options),call=operator.call;
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
