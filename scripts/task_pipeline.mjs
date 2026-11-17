// SPDX-License-Identifier: AGPL-3.0-or-later
// Inspect or control one stage without clearing its durable input.
import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
import {facts} from '../worker/factory.js';
import {TASK_PIPELINE_STAGES,TASK_PIPELINE_CONTROLS,readPipelineControl,setPipelineControl} from '../worker/task_pipeline.js';
import {openEnv} from './factoryd.mjs';

export async function pipelineCommand(env,args,now=Date.now()) {
  const [command,name,...extra]=args;
  if(extra.length||!['status','pause','resume','run'].includes(command)
    || command==='status'&&name&&name!=='statistics'
    || ['pause','resume'].includes(command)&&!Object.hasOwn(TASK_PIPELINE_CONTROLS,name)
    || command==='run'&&!Object.hasOwn(TASK_PIPELINE_STAGES,name))
    throw Error('usage: task_pipeline.mjs status [statistics] | pause|resume admission|verification|delivery|contribution|statistics | run admission|verification|delivery|contribution');
  if(command==='status'&&name==='statistics'){
    const control=await readPipelineControl(env,name);
    const snapshot=await env.LIBRARY.prepare('SELECT at,engine FROM state_factory_snapshot WHERE id=1').first();
    return {name,...TASK_PIPELINE_CONTROLS[name],paused:control.paused,control,snapshot:snapshot||null,pending:null};
  }
  if(command==='status')return facts().pipelineStatus(env,now);
  if(command==='run')return facts().runStage(env,name,now,{runtime:'native'});
  await setPipelineControl(env,name,{paused:command==='pause',by:'operator'},now);
  return {stage:name,paused:command==='pause'};
}

if(process.argv[1] && pathToFileURL(resolve(process.argv[1])).href===import.meta.url) {
  const opened=await openEnv();
  try {console.log(JSON.stringify(await pipelineCommand(opened.env,process.argv.slice(2))));}
  finally {await opened.close();}
}
