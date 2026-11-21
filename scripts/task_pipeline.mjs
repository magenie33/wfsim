// SPDX-License-Identifier: AGPL-3.0-or-later
// Inspect or control one stage without clearing its durable input.
import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
import {facts} from '../worker/factory.js';
import {TASK_PIPELINE_STAGES,TASK_PIPELINE_CONTROLS,readPipelineControl,setPipelineControl} from '../worker/task_pipeline.js';
import {openEnv} from './factoryd.mjs';
import {statisticsStatus} from '../worker/statistics_provider.js';

export async function pipelineCommand(env,args,now=Date.now()) {
  const [command,name,...extra]=args;
  if(extra.length||!['status','pause','resume','run'].includes(command)
    || command==='status'&&name&&!['statistics','family'].includes(name)
    || ['pause','resume'].includes(command)&&!Object.hasOwn(TASK_PIPELINE_CONTROLS,name)
    || command==='run'&&!Object.hasOwn(TASK_PIPELINE_STAGES,name))
    throw Error('usage: task_pipeline.mjs status [statistics|family] | pause|resume <control> | run <stage>');
  if(command==='status'&&name==='statistics'){
    return {...TASK_PIPELINE_CONTROLS[name],...await statisticsStatus(env,now)};
  }
  if(command==='status'&&name==='family')return {...TASK_PIPELINE_CONTROLS[name],...await readPipelineControl(env,name)};
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
