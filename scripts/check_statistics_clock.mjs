// SPDX-License-Identifier: AGPL-3.0-or-later
// Sealed public statistics expire without database or browser dependencies.
import assert from 'node:assert/strict';
import {createContext,runInContext} from 'node:vm';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {clientSource} from './ci_client_inputs.mjs';
import {contributionRoute} from '../worker/contribution.js';
import {currentStatistics,statisticsExpiresAt,STATISTICS_FRESH_MS} from '../worker/statistics_clock.js';

export async function statisticsClockCases(){
 const at='2026-11-17T00:00:00.000Z',sampled=Date.parse(at);
 const fresh={ok:true,v:1,at,counts_at:at,expires_at:statisticsExpiresAt(at),order_epoch:null,
  orders:[{kind:'new_build',total:23}],per_hour:{volunteers:100,official:0},clear_hours:1};
 assert(currentStatistics(fresh,sampled+85000),'normal seal/upload/cache/refresh chain expired');
 assert(currentStatistics(fresh,sampled+STATISTICS_FRESH_MS-1));
 assert(!currentStatistics(fresh,sampled+STATISTICS_FRESH_MS),'expiry boundary is inclusive');
 assert(!currentStatistics(fresh,sampled-1),'future sample accepted');
 for(const field of ['at','counts_at','expires_at','order_epoch']){
  assert(!currentStatistics({...fresh,[field]:undefined},sampled));
  assert(!currentStatistics({...fresh,[field]:'invalid'},sampled));
 }
 assert(!currentStatistics({...fresh,at:new Date(sampled+STATISTICS_FRESH_MS).toISOString()},sampled+STATISTICS_FRESH_MS));
 assert(!currentStatistics({...fresh,expires_at:new Date(sampled+2*STATISTICS_FRESH_MS).toISOString()},sampled));
 assert.equal(statisticsExpiresAt('invalid'),null);
 const source=process.env.WFSIM_STATISTICS_CLOCK_UI_SOURCE?readFileSync(process.env.WFSIM_STATISTICS_CLOCK_UI_SOURCE,'utf8'):clientSource('scripts/check_statistics_clock.mjs');
 const begin=source.indexOf('let computeDemand = null;'),end=source.indexOf("/// THE NAV'S COUNT:",begin);
 assert(begin>=0&&end>begin);
 let time=sampled+85000,redraws=0,next=0,cdn=fresh,fallback=null,apiCalls=0;
 const timers=new Map(),context=createContext({serverNow:()=>time,computingCount:37,computingEpoch:undefined,
  tr:x=>x,aT:x=>x,escHtml:x=>String(x),accountLocale:()=> 'en-US',META:null,IMG:x=>x,BOARD_ORIGIN:'https://fixture.invalid',
  fetchJson:async()=>cdn,accountCall:async()=>{apiCalls++;return fallback;},computeRedraw:()=>redraws++,
  setTimeout:(fn,ms)=>{const id=++next;timers.set(id,{fn,at:time+ms});return id;},clearTimeout:id=>timers.delete(id)});
 runInContext(source.slice(begin,end),context);
 const evaluate=text=>runInContext(text,context),draw=value=>{context.value=value;return evaluate('computeDemand=value;computeDemandHtml()');};
 const legacy={...fresh,counts_at:'2026-11-15T00:00:00.000Z',orders:[{kind:'sweep',total:24328063}],per_hour:{volunteers:22660,official:824},clear_hours:1197};
 delete legacy.expires_at;
 const oldHtml=draw(legacy);assert(!oldHtml.includes('24,328,063')&&!oldHtml.includes('824')&&!oldHtml.includes('Cleared in'),'old demand shown beside live devices');
 assert(oldHtml.includes('37')&&oldHtml.includes('—'),'missing statistics hid live devices or invented zero');
 assert(draw(fresh).includes('23'));
 context.computingEpoch=at;assert(!draw(fresh).includes('new builds 23'),'previous order epoch shown');
 assert(draw({...fresh,order_epoch:at}).includes('new builds 23'));context.computingEpoch=undefined;
 cdn=legacy;fallback=fresh;await evaluate('loadComputeDemand()');assert.equal(apiCalls,1,'stale CDN skipped API fallback');
 assert.equal(evaluate('computeDemand.counts_at'),at);
 cdn=fallback=null;time+=1000;await evaluate('loadComputeDemand()');
 assert.equal(evaluate('computeDemand.expires_at'),fresh.expires_at,'failed refresh extended lifetime');
 const newerAt=new Date(sampled+1000).toISOString();cdn={...fresh,at:newerAt,counts_at:newerAt,expires_at:statisticsExpiresAt(newerAt)};
 await evaluate('loadComputeDemand()');cdn=fresh;await evaluate('loadComputeDemand()');
 assert.equal(evaluate('computeDemand.counts_at'),newerAt,'older response replaced newer statistics');
 assert.equal(timers.size,1,'expiry timers accumulated');
 time=Date.parse(evaluate('computeDemand.expires_at'));
 const [id,timer]=timers.entries().next().value;timers.delete(id);timer.fn();
 assert.equal(evaluate('computeDemand'),null);assert.equal(redraws,1,'expiry did not redraw an idle page');
 time=sampled;assert(draw({...fresh,orders:[],per_hour:{volunteers:0,official:0}}).includes('>0</b>'),'fresh zero was hidden');
 const env={PUBLIC:{get:async()=>({body:JSON.stringify(legacy)})},LIBRARY:{prepare(){throw Error('demand read historical database');}}};
 const route=()=>contributionRoute(new Request('https://wfsim.app/api/board/demand'),env,'/api/board/demand');
 let response=await route();assert.equal(response.status,503);assert.equal((await response.json()).error,'stale_demand');
 const now=Date.now(),liveAt=new Date(now-STATISTICS_FRESH_MS+5000).toISOString();
 const body=JSON.stringify({...fresh,at:liveAt,counts_at:liveAt,expires_at:statisticsExpiresAt(liveAt)});
 env.PUBLIC.get=async()=>({body});response=await route();assert.equal(response.status,200);assert.equal(await response.text(),body,'fallback rewrote sealed bytes');
 assert(Number(response.headers.get('cache-control').match(/max-age=(\d+)/)[1])<=5,'edge cache outlived expiry');
 console.log('PASS statistics clock: normal 85s, deadline, old payload, epoch, fallback, failed refresh, monotonic sample, idle expiry, real zero and API byte/cache controls');
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url)await statisticsClockCases();
