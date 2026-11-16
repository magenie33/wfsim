// SPDX-License-Identifier: AGPL-3.0-or-later
// Coalescing keeps live work; an explicit new click can replace only a stranded legacy request.
import {boardReviewsRoute} from './board_reviews.js';
import {questionId} from './task_contract.js';

export async function boardRequestedReview(request,env,ctx) {
 const copy=request.clone(),response=await boardReviewsRoute(request,env,'/api/board/review',ctx);
 if(!response.ok)return response;
 const accepted=await response.clone().json();
 if(!accepted.id||accepted.accepted||accepted.skipped)return response;
 try{
  const body=await copy.json();
  const prior=await env.LIBRARY.prepare("SELECT record,to_regprocedure('board_requested_review_fresh(text,text,text,text)') IS NOT NULL requested_intake FROM entity_measurement_reviews WHERE id=?").bind(accepted.id).first();
  if(!prior?.requested_intake)return response;
  const id=crypto.randomUUID(),question=await questionId('simulate',{record:JSON.parse(prior.record),ruler:body.ruler,mode:body.mode,review:id});
  const {result}=await env.LIBRARY.prepare('SELECT board_requested_review_fresh(?::text,?::text,?::text,?::text) result')
   .bind(accepted.id,id,question,JSON.stringify(body)).first();
  return result?Response.json(result):response;
 }catch{return Response.json({ok:false,error:'The review could not be registered. Try again.'},{status:503});}
}
