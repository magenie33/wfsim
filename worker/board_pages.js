// SPDX-License-Identifier: AGPL-3.0-or-later
// Public reads expose only complete native partitions and immutable page versions.
const name=value=>typeof value==='string'&&/^[a-z0-9_]{1,160}$/.test(value);
const reply=(body,status=200)=>Response.json(body,{status,headers:{'cache-control':status===200?'public, max-age=10':'no-store','access-control-allow-origin':'*'}});

export async function boardPagesRoute(request,env,ctx) {
 if(request.method!=='GET')return reply({ok:false,error:'GET only'},405);
 const url=new URL(request.url),params=url.searchParams,weapon=params.get('weapon'),ruler=params.get('ruler');
 const after=params.get('after')??'',revision=params.get('revision');
 if((weapon!==null&&!name(weapon))||(ruler!==null&&!name(ruler))
   ||(revision!==null&&!/^[a-f0-9]{64}$/.test(revision)))return reply({ok:false,error:'invalid board partition'},400);
 const cache=globalThis.caches?.default,hit=cache&&await cache.match(request);
 if(hit)return hit;
 let data;
 try {
  if(url.pathname==='/api/board/pages/catalog') {
   const scene=params.get('scene')??'';
   if((scene!==''&&!name(scene))||!/^[a-z0-9_/]*$/.test(after)||after.length>322)return reply({ok:false,error:'invalid board cursor'},400);
   const result=await env.LIBRARY.prepare('SELECT board_read_catalog(?::text,?::text,?::text,128::bigint) page').bind(weapon,scene,after).first();
   data=result.page;const all=data.partitions;
   data={...data,partitions:all.slice(0,128),next:all.length>128?all[127].weapon+'/'+all[127].ruler:null};
  }else{
   if(!weapon||!ruler||!/^(0|[1-9][0-9]{0,14})$/.test(after||'0'))return reply({ok:false,error:'invalid board cursor'},400);
   const result=await env.LIBRARY.prepare('SELECT board_read_page(?::text,?::text,?::text,?::bigint,128::bigint) page')
    .bind(weapon,ruler,revision,after||'0').first();
   data=result.page;
  }
 }catch(_){return reply({ok:false,error:'board query unavailable'},503);}
 const response=reply(data,data.ok?200:data.error==='revision_unavailable'?409:404);
 if(data.ok&&cache&&ctx?.waitUntil)ctx.waitUntil(cache.put(request,response.clone()));
 return response;
}
