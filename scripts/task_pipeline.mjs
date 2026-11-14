// SPDX-License-Identifier: AGPL-3.0-or-later
// Inspect or control one stage without clearing its durable input.
import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
import {facts} from '../worker/factory.js';
import {TASK_PIPELINE_STAGES} from '../worker/task_pipeline.js';
import {openEnv} from './factoryd.mjs';

export async function pipelineCommand(env,args,now=Date.now()) {
  const [command,name,...extra]=args;
  if(extra.length || !['status','pause','resume','run'].includes(command)
    || command!=='status' && !Object.hasOwn(TASK_PIPELINE_STAGES,name) || command==='status' && name)
    throw Error('usage: task_pipeline.mjs status | pause|resume|run verification|delivery|contribution');
  const f=facts();
  if(command==='status')return f.pipelineStatus(env,now);
  if(command==='run')return f.runStage(env,name,now,{runtime:'native'});
  await f.controlStage(env,name,{paused:command==='pause',by:'operator'},now);
  return {stage:name,paused:command==='pause'};
}

if(process.argv[1] && pathToFileURL(resolve(process.argv[1])).href===import.meta.url) {
  const opened=await openEnv();
  try {console.log(JSON.stringify(await pipelineCommand(opened.env,process.argv.slice(2))));}
  finally {await opened.close();}
}
