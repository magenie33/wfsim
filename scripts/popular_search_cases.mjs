// SPDX-License-Identifier: AGPL-3.0-or-later
// Freeze the homepage population and actual board inputs before benchmarking locally.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';

const [popFile, wasm, out, limit = '40'] = process.argv.slice(2);
if (!out) throw Error('popularity.json wasm-node-dir output-dir [limit]');
const engine = createRequire(import.meta.url)(path.resolve(wasm, 'wfsim_wasm.js'));
const read = f => JSON.parse(fs.readFileSync(f, 'utf8'));
const save = (name, rows) => fs.writeFileSync(path.join(out, name + '.json'), JSON.stringify(rows));
const api = (route, body) => JSON.parse(engine.api(route, JSON.stringify(body)));
const pop = read(popFile), meta = read('site/pkg/' + read('site/pkg/generation.json').names.find(n => n.startsWith('meta.')));
assert(pop.ok && pop.window_days === 30 && Number.isInteger(+limit) && +limit > 0);
const ranked = meta.weapons.filter(w => pop.scores[w.id] > 0).sort((a,b) => pop.scores[b.id]-pop.scores[a.id] || a.name.localeCompare(b.name)).slice(0,+limit);
const geom = ['single','multi'];
const name = (g, head) => `standard_${g}_target${head === 50 ? '__aimed_half' : head === 0 ? '__aimed_body' : ''}`;
const cases = [], searches = [], starts = [], samples = [];
for (const [index,w] of ranked.entries()) {
  // Use the same accepted record for both geometries and all aim settings. Prefer a fixed non-riven witness.
  const files = ['aimed_half/','', 'sp200__aimed_half/'].map(d => `site/board/${d}${w.id}.json`).filter(fs.existsSync);
  const lists = files.map(read);
  let chosen;
  outer: for (const withRiven of [false,true]) for (const [fi,list] of lists.entries()) for (const row of list) {
    if (!!row.riven !== withRiven) continue;
    const {benchmark,measured_at,measured_by,score,shown,mode,riven,...fields} = row;
    const record = {weapon:w.id,...fields};
    if (riven) Object.assign(record,{riven_pos:riven.bonuses,riven_neg:riven.malus,riven_rolls:riven.rolls});
    const requests = geom.map(g => api('/api/board/order',{record,mode,ruler:name(g,50)}));
    if (requests.some(v => !v.ok)) continue;
    assert.equal(requests[0].identity, requests[1].identity);
    chosen = {requests, record, mode, file:files[fi], publishedScore:score};
    break outer;
  }
  assert(chosen, `no admitted board witness for ${w.id}`);
  const {requests,record,mode,file,publishedScore} = chosen;
  samples.push({rank:index+1,weapon:w.id,name:w.name,slot:w.slot,popularity:pop.scores[w.id],record,mode,file,
    publishedScore,identity:requests[0].identity});
  for (const [i,g] of geom.entries()) {
    const req = requests[i].request;
    assert.equal((req.formation || []).length, i ? 24 : 0);
    for (const head of [0,50,100]) cases.push({id:`${w.id}:${g}:${head}`,path:'/api/board/runs',
      body:{request:{...req,headshot_pct:head},from:0,count:3}});
    const elems = meta.search_presets.element_starts.map(row => row.find(id=>w.mods.includes(id))).filter(Boolean);
    const others = record.mods.slice(0,8).filter(id=>id !== 'riven' && !elems.includes(id) && w.mods.includes(id));
    const cards = [...new Set([...elems,...others.slice(0,3)])];
    const search = {...req, strategy:'quick', search_strategy:'four_elements', candidate_runs:10,
      starts:[{slots:[],mode,evolutions:req.evolutions,assembly:req.assembly}],finalists:4,threads:1};
    delete search.mods;
    starts.push({id:`${w.id}:${g}:lead`,path:'/api/compute/optimize',body:{...search,candidate_runs:10,quick_fleet:{lead:true,fresh:true,scores:[]}}});
    searches.push({id:`${w.id}:${g}:search`,path:'/api/compute/optimize',body:{...search,
      duration:12,runs:12,mods:Object.fromEntries(cards.map(k=>[k,'search'])),build_size:3,arcanes:{none:'fixed'}}});
  }
}
fs.mkdirSync(out,{recursive:true});
save('sample',{from:pop.from,to:pop.to,samples});save('fights',cases);save('searches',searches);save('starts',starts);
const browser = new Set(['burston_prime:single','dual_toxocyst:single','phantasma_prime:multi','okina_prime:multi']);
const whole = new Set(['akarius_prime:single','phantasma_prime:multi']);
const baseId = id => id.split(':').slice(0,2).join(':');
save('browser-cases',searches.filter(c=>browser.has(baseId(c.id))).map(c=>({...c,body:{...c.body,duration:180,runs:100,finalists:5}})));
save('unrestricted',starts.filter(c=>whole.has(baseId(c.id))).map(c=>{
  const {quick_fleet,...req}=c.body;
  return {...c,id:baseId(c.id)+':whole',body:{...req,duration:6,runs:12,candidate_runs:2,finalists:3}};
}));
console.log(`${samples.length} weapons, ${cases.length} fight cells, ${searches.length} searches`);
