// SPDX-License-Identifier: AGPL-3.0-or-later
// Frozen calls for simulation_cost / engine_compare, using the shipped ruler's body order.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';

const [output, suite = 'fight'] = process.argv.slice(2);
if (!output || !['fight', 'search', 'lead', 'finals'].includes(suite)) throw Error('output.json [fight|search|lead|finals]');
const read = name => JSON.parse(fs.readFileSync(path.join('site/pkg', name)));
const meta = read(read('generation.json').names.find(n => /^meta\..*\.json$/.test(n)));
const scene = meta.benchmarks.find(b => b.id === 'standard_multi_target__aimed_half').scenario;
assert.equal(scene.formation.length, 24);
assert.equal(scene.headshot_pct, 50);
const rows = [];
for (const weapon of ['synoid_gammacor', 'torid']) {
  const pistol = weapon === 'synoid_gammacor';
  const evolutions = (pistol ? ['evo1_incarnon_form','sages_resolve','evolved_autoloader','critical_parallel']
    : ['evo1_incarnon_form','final_fusillade','extended_volley','commodores_fortune']).map(s => `${weapon}_${s}`);
  const common = pistol ? ['hornet_strike','barrel_diffusion','lethal_torrent','target_cracker']
    : ['serration','split_chamber','point_strike','vital_sense'];
  // Combine Cold and Toxin first so Torid's innate Toxin does not consume the Electricity.
  const status = pistol ? ['frostbite','pistol_pestilence','jolt'] : ['rime_rounds','malignant_force','high_voltage'];
  const heat = pistol ? 'scorch' : 'thermite_rounds';
  const build = {...scene, weapon, mode:'cycle', evolutions};
  if (suite === 'fight') {
    for (const [name, mods] of [['electric', [...common,...status]], ['viral', [...common,...status.slice(0,2)]]]) {
      for (const headshot_pct of [0,50,100]) {
        rows.push({id:`${weapon}-${name}-${headshot_pct}`,path:'/api/board/runs',body:{
          request:{...build,mods,headshot_pct},from:0,count:3}});
      }
    }
    rows.push({id:`${weapon}-single-50`,path:'/api/board/runs',body:{request:{...build,mods:[...common,...status],formation:[]},from:0,count:3}});
  } else {
    const req = {...build, strategy:'quick',search_strategy:'four_elements',candidate_runs:10,
      starts:[{slots:[],mode:'cycle',evolutions}], threads:1};
    if (suite === 'lead') req.quick_fleet = {lead:true,fresh:true,scores:[]};
    else Object.assign(req,{duration:suite === 'finals' ? scene.duration : 12,
      runs:suite === 'finals' ? scene.runs : 12,finalists:suite === 'finals' ? 10 : 2,build_size:3,
      mods:Object.fromEntries([...status,heat,...common.slice(0,2)].map(k=>[k,'search'])),arcanes:{none:'fixed'}});
    rows.push({id:`${weapon}-${suite}`,path:'/api/compute/optimize',body:req});
  }
}
fs.writeFileSync(output,JSON.stringify(rows));
