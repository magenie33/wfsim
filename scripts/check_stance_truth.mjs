// SPDX-License-Identifier: AGPL-3.0-or-later
// A successful CLI exit alone does not establish a settled stance reference.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import path from 'node:path';

const root=fileURLToPath(new URL('../',import.meta.url));
const fixture=path.join(root,'tests/fixtures/optimizer/stance-truth.json');
const input=readFileSync(fixture),request=JSON.parse(input);
assert.equal(request.weapon,'magistar');assert.equal(request.strategy,'quick');
assert(!Object.hasOwn(request,'stance'),'fixture must search the stance slot');
assert.deepEqual(request.starts,[{slots:[]}]);
assert.deepEqual(request.modes,{base:'search',forward:'search',block:'search',block_forward:'search'});
assert.equal(request.runs,50);assert.equal(request.candidate_runs,10);

export function verdict(output,status){
 assert.equal(status,0,'wfsim-truth failed');
 assert(!output.includes('!!'),'truth emitted an unsettled/unmatched warning');
 const scope=output.match(/^\[scope\] (\d+) builds x arcanes = (\d+) jobs \((\d+) distinct to the fight\), exhaustive$/m);
 const reference=output.match(/^\[reference\] (\d+) runs each = (\d+) sims \| answer set (\d+) builds \| settled across seeds: (true|false) \| top-(\d+) overlap ([\d.]+)$/m);
 const search=output.match(/^\[search\] rank (\d+) \| regret ([\d.]+)% \| within noise: (true|false) \| top-(\d+) recall ([\d.]+)% \|/m);
 assert(scope&&reference&&search,'missing complete truth report');
 assert.equal(Number(scope[2]),24,'stance/mode exhaustive scope changed');
 assert.equal(Number(reference[1]),200,'reference precision changed');
 assert.equal(reference[4],'true','reference seeds did not settle');
 assert.equal(Number(reference[5]),5,'reference top-five missing');
 assert.equal(Number(search[1]),1,'search missed the reference winner');
 // Rank one implies exactly zero regret; it is not inferred from rounded text.
 assert.equal(Number(search[2]),0,'search gave up objective value');
 assert.equal(search[3],'true','winner lies outside the reference answer set');
 assert.equal(Number(search[4]),5,'search top-five missing');
 assert.equal(Number(search[5]),100,'search missed the reference top five');
 return{jobs:Number(scope[2]),truth_runs:Number(reference[1]),settled:true,rank:1,regret_percent:0,top5_recall_percent:100};
}

const good='[scope] 24 builds x arcanes = 24 jobs (24 distinct to the fight), exhaustive\n'
 +'[reference] 200 runs each = 4800 sims | answer set 1 builds | settled across seeds: true | top-5 overlap 1.00\n'
 +'[search] rank 1 | regret 0.000% | within noise: true | top-5 recall 100% | 100 sims (2.1% of the reference)\n';
function selfTest(){
 verdict(good,0);
 for(const bad of [good.replace('seeds: true','seeds: false'),good.replace('rank 1','rank 2'),
  good.replace('recall 100','recall 80'),good.replace('= 24 jobs','= 8 jobs'),good.replace('200 runs','20 runs'),
  good+'!! 1 finalists were not in the exhaustive list',good.replace('[reference]','[missing]'),''])
  assert.throws(()=>verdict(bad,0));
 assert.throws(()=>verdict(good,1));
 console.log('PASS stance truth checker: good output accepted; nine false-success controls rejected');
}
selfTest();
if(!process.argv.includes('--self-test')){
 const folder=path.join(root,'target/stance-truth');mkdirSync(folder,{recursive:true});
 const args=['run','-p','wfsim-webapi','--bin','wfsim-truth','--',`request=${fixture}`,'truth_runs=200','max_jobs=1000'];
 const result=spawnSync('cargo',args,{cwd:root,encoding:'utf8',maxBuffer:4*1024*1024});
 const stdout=result.stdout??'',stderr=result.stderr??'';
 writeFileSync(path.join(folder,'stdout.log'),stdout);writeFileSync(path.join(folder,'stderr.log'),stderr);
 process.stdout.write(stdout);process.stderr.write(stderr);
 if(result.error)throw result.error;
 const report={source:process.env.GITHUB_SHA??null,fixture_sha256:createHash('sha256').update(input).digest('hex'),
  command:['cargo',...args],...verdict(stdout,result.status)};
 writeFileSync(path.join(folder,'verdict.json'),JSON.stringify(report,null,2)+'\n');
 console.log('PASS stance truth quality gate',JSON.stringify(report));
}
