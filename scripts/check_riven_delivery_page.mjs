import { openApp } from "./cdp.mjs";
const app = await openApp({ base: process.env.WFSIM_BASE, lang: "en", boot: 1000 });
try {
  await app.load("/weapons/Torid/optimizer", 3000);
  const result = await app.evaluate(`(async () => {
    const queue = await initUserTasks(), fetchBefore = window.fetch, changeBefore = queue.change;
    const controller = new AbortController(), seen = [], cursors = [], progress = [];
    const task = {id:crypto.randomUUID(),attempt:crypto.randomUUID(),kind:"appraisal",engine:ENGINE_ID,
      request:{weapon:"torid",runs:1,strategy:"quick",starts:[]},origin:{board_upload:false},destination:"community"};
    const source = n => String(n).repeat(32);
    communityTaskStates.clear(); communityCandidateCursors.clear(); communityTaskPollAt = 0;
    const timer = setTimeout(()=>controller.abort(),5000);
    queue.change = async () => {};
    window.fetch = async (url, options) => {
      const path = String(url);
      if (!path.startsWith("/api/user-tasks/")) return fetchBefore(url,options);
      const body = JSON.parse(options.body);
      const answer = value => Response.json({ok:true,...value});
      if (path.endsWith("/accept")) return answer({id:body.id});
      if (!path.endsWith("/status")) throw new Error("Unexpected route: "+path);
      const cursor = body.after[task.attempt] || 0; cursors.push(cursor);
      const candidate = cursor ? 2 : 1;
      return answer({tasks:[{id:task.attempt,state:"waiting",candidate,
        ...(body.include_results ? {result:{candidate},delivery:source(candidate)} : {})}]});
    };
    try {
      const value = await USER_TASK_EXECUTORS.get("community").run(task,{community:{finish:async (task,execution,report)=>{
        seen.push({candidate:report.candidate,delivery:report.delivery});
        if (report.candidate === 1) throw Object.assign(new Error("Replay rejected"),{candidateRejected:true});
        return {delivery:report.delivery,accepted:true};
      }}},{signal:controller.signal,report:state=>progress.push(state.phase)});
      return {value,seen,cursors,progress};
    } finally {clearTimeout(timer);window.fetch=fetchBefore;queue.change=changeBefore;}
  })()`);
  app.check("a first candidate reaches replay before the server reports consensus",
    result.seen[0]?.candidate === 1 && result.seen[0].delivery === "1".repeat(32));
  app.check("a rejected replay advances the cursor and replays the next calculator",
    result.seen.length === 2 && result.seen[1].candidate === 2 && result.cursors.join(",") === "0,0,1,1");
  app.check("delivery preserves the accepted candidate's own source",
    result.value.accepted && result.value.delivery === "2".repeat(32) && result.progress.includes("Waiting for another calculation"));
} catch (error) { await app.close(); throw error; }
await app.finish("Riven candidates deliver through replay independently of consensus.");
