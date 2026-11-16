// SPDX-License-Identifier: AGPL-3.0-or-later
// Request status reports committed intake and the existing workflow's selected measurement.
import {sameSite} from './accounts.js';

export async function boardRequestsRoute(request,env) {
 if(request.method!=='POST'||!sameSite(request))return Response.json({ok:false,error:'invalid request'},{status:400});
 const raw=await request.text();if(raw.length>4096)return Response.json({ok:false,error:'payload too large'},{status:413});
 let body;try{body=JSON.parse(raw);}catch{return Response.json({ok:false,error:'not json'},{status:400});}
 if(!Array.isArray(body?.ids)||!body.ids.length||body.ids.length>50||!body.ids.every(id=>typeof id==='string'&&/^[a-f0-9-]{36}$/.test(id)))
  return Response.json({ok:false,error:'invalid request ids'},{status:400});
 try{
  const {results}=await env.LIBRARY.prepare(`SELECT r.id,r.ruler,r.mode,s.phase,s.error,s.workflow,w.state workflow_state,
    i.build_id identity,i.input->'record' record,m.score current_score,m.measured_by current_by,m.finished_at completed_at
    FROM jsonb_array_elements_text((?::text)::jsonb) wanted(id)
    JOIN entity_board_requests r ON r.id=wanted.id JOIN state_board_requests s ON s.id=r.id
    LEFT JOIN entity_build_intake_intents i ON i.id=s.intent LEFT JOIN state_build_workflows w ON w.id=s.workflow
    LEFT JOIN state_board_measurement_selection m ON m.identity=i.build_id AND m.ruler=r.ruler AND m.mode=r.mode`)
   .bind(JSON.stringify(body.ids)).all();
  return Response.json({ok:true,requests:results});
 }catch{return Response.json({ok:false,error:'board requests are unavailable'},{status:503});}
}
