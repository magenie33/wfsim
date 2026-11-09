// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { loadEngine } from './wasm_engine.mjs';

const fixture=JSON.parse(readFileSync('tests/client_search.json','utf8'));
const engine=loadEngine();
const adapterSource=readFileSync('mcp/headless.js','utf8').match(/function headlessBoardPayloadFromResult[\s\S]*?\n\}/)[0];
const payload=runInNewContext(adapterSource+'\nheadlessBoardPayloadFromResult');
let scores=[],work=0,fights=0,builds=0,steps=0;
for(;;steps++){
  assert(steps<100,'bounded quick search must finish');
  const lead=engine.optimize({...fixture.request,quick_fleet:{lead:true,fresh:steps===0,scores}});
  assert.notEqual(lead.ok,false,lead.error);
  if(!lead.pending){
    const best=lead.results.find(r=>r.mods?.length);
    const expected={build:payload(best,fixture.context),score:best.kill_progress||0,work:work+(lead.final_work||0),counts:{builds,fights,steps}};
    mkdirSync('target/official-cli',{recursive:true});
    writeFileSync('target/official-cli/search-expected.json',JSON.stringify(expected));
    writeFileSync('target/official-cli/search-request.json',JSON.stringify(fixture.request));
    console.log(`WASM quick reference: ${steps} rounds, ${builds} builds, ${fights} fights, ${expected.work} work.`);
    break;
  }
  assert(lead.pending.length>0,'a pending round contains work');
  scores=[];
  for(const candidate of lead.pending){
    const scored=engine.optimize({...fixture.request,quick_fleet:{score:[candidate]}});
    assert.notEqual(scored.ok,false,scored.error);
    scores.push(...scored.scores);work+=scored.work||0;fights+=scored.fights||0;
  }
  builds+=scores.length;
}
