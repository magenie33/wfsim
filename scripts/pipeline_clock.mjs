// SPDX-License-Identifier: AGPL-3.0-or-later
// Notifications wake durable consumers; timers recover missed notifications.
export function pipelineClock({onError=()=>{}}={}) {
  const jobs=new Map(),inflight=new Set();let stopped=false;
  const arm=(job,ms)=>{clearTimeout(job.timer);job.timer=setTimeout(()=>run(job),ms);};
  const run=job=>{
    if(stopped)return;
    if(job.busy){job.owed=true;return;}
    job.busy=true;job.owed=false;
    const pending=Promise.resolve().then(job.work).catch(error=>onError(job.name,error)).finally(()=>{
      inflight.delete(pending);job.busy=false;
      if(!stopped)arm(job,job.owed?0:job.interval);
    });
    inflight.add(pending);
  };
  return {
    add(name,interval,work) {
      if(stopped || jobs.has(name) || !Number.isFinite(interval) || interval<1 || typeof work!=='function')throw Error('invalid pipeline clock job');
      const job={name,interval,work,busy:false,owed:false,timer:null};jobs.set(name,job);arm(job,0);
    },
    wake(name) {
      const job=jobs.get(name);if(stopped || !job)return false;
      if(job.busy)job.owed=true;else arm(job,0);
      return true;
    },
    stop(){stopped=true;for(const job of jobs.values())clearTimeout(job.timer);},
    drain:()=>Promise.allSettled([...inflight]),
  };
}
