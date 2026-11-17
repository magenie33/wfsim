import { seedComputeWork } from "./pg/fixtures.mjs";
// THE SETTLER IN SHADOW (worker/settler.js, scripts/factoryd.mjs): its fair table
// is the worker's `tableOf` to the last digit through Postgres, its snapshot
// counts what the worker's own answers count, its judgement of a settled
// question agrees with the request path's, and every comparison it makes
// catches a difference put there on purpose.
//   node scripts/check_settler.mjs
import { tableOf } from "../worker/fair.js";
import { factory, canon, nearAnswer } from "../worker/tasks.js";
import { contributionRoute } from "../worker/contribution.js";
import { signalTick, snapshotTick, fairTick, readFair, compareFair, settleShadow, judgeFact, SETTLE_SHADOW_LAG_MS } from "../worker/settler.js";
import { testDb } from "./pg/test_db.mjs";
import { SETTLER_SILENT_MS } from "../worker/factory.js";
import { mock } from "node:test";
import { DIAL_DEFAULTS, dialsOf, forgetDials } from "../worker/dials.js";
import { statisticsReadCases } from "./check_order_cut_reads.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};
const iso = (ms) => new Date(ms).toISOString();
const now = Date.now();

const LIBRARY = await testDb("library"), ACCOUNTS = await testDb("accounts"), L = LIBRARY;
const env = { LIBRARY, ACCOUNTS };

console.log("the fair table");
{
  await ACCOUNTS.run("INSERT INTO entity_accounts (id, created_at, username) VALUES ('acc-big', ?, 'big')", iso(now - 3 * 86_400_000));
  const BIG = "b".repeat(24), BIG2 = "e".repeat(24), WALKER = "c".repeat(24);
  for (const v of [BIG, BIG2]) await ACCOUNTS.run("INSERT INTO relation_account_compute_devices (verifier, account, claimed_at) VALUES (?, 'acc-big', 'x')", v);
  for (const v of [BIG, BIG2, WALKER]) await L.run("INSERT INTO entity_compute_devices (id, seen, asked_at, asked_engine, asked_lanes, work) VALUES (?, 'x', ?, 'e1', 4, 1)", v, iso(now - 30_000));
  await L.run(`INSERT INTO record_candidate_answers (question, engine, device, owner, canon, result, work, compute_ms, at, credited) VALUES
      ('a1', 'e1', ?, 'acc-big', 'k', '{}', 3000, 1000, ?, 1), ('a2', 'e1', ?, ?, 'k', '{}', 1000, 1000, ?, 0)`,
    BIG, iso(now - 60_000), WALKER, WALKER, iso(now - 90_000));
  const table = await fairTick(env, "e1", now);
  const worker = await tableOf(env, "e1", now);
  const stored = await readFair(L, "e1");
  check("the daemon's table is the worker's tableOf, account by account", !compareFair(table, worker, 0).differ, JSON.stringify(compareFair(table, worker, 0)));
  check("…and reads back from state_compute_fair_shares to the last digit", !compareFair(worker, stored.table, 0).differ && Object.keys(worker).length === 2,
    JSON.stringify({ worker, stored: stored.table }));
  await L.run("UPDATE state_compute_fair_shares SET target = target + 0.1 WHERE account = 'acc-big'");
  const moved = compareFair(worker, (await readFair(L, "e1")).table, 0.05);
  check("a target moved past the tolerance is a difference, on the account it moved", moved.differ && moved.worst.account === "acc-big", JSON.stringify(moved));
  await L.run("DELETE FROM state_compute_fair_shares WHERE account = 'walk-ins'");
  check("an account missing from one table is a difference", compareFair(worker, (await readFair(L, "e1")).table, 1).missing.includes("walk-ins"));
}

console.log("the snapshot");
{
  await (async (fixtureDb,args)=>{await seedComputeWork(fixtureDb,{id:"o1",verb:"simulate",request:"{}",state:"open",due_at:args[0],opened_at:args[1],waits:0});await seedComputeWork(fixtureDb,{id:"o2",verb:"simulate",request:"{}",state:"open",due_at:args[2],opened_at:args[3],waits:0});await seedComputeWork(fixtureDb,{id:"u1",verb:"simulate",request:"{}",state:"open",due_at:args[4],opened_at:args[5],waits:1});})(L,[iso(now + 1),iso(now),iso(now + 1),iso(now),iso(now + 1),iso(now)]);
  await L.run("UPDATE state_compute_work SET lease = 'l', lease_until = ?, leased_to = 'x' WHERE id = 'o2'", iso(now + 60_000));
  await L.run("UPDATE entity_compute_devices SET last_at = ? WHERE id = ?", iso(now - 60_000), "b".repeat(24));
  const s = await snapshotTick(env, { engine: "e1", release: "r1" }, now);
  const served = await (await contributionRoute(new Request("https://x/api/board/computing"), env, "/api/board/computing")).json();
  check("computing is the count /api/board/computing serves", s.computing === served.computing && s.computing === 1, JSON.stringify({ s, served }));
  // ASKING IS WHO ASKED LATELY OR HOLDS A LIVE LEASE (worker/tasks.js `askingIds`):
  // three asked, and 'x' holds o2 without having asked.
  check("open counts what may be taken now, urgent what someone waits on, asking who asked lately or holds a lease",
    s.open === 1 && s.urgent === 1 && s.asking === 4, JSON.stringify(s));
  const row = await L.get("SELECT * FROM state_factory_snapshot WHERE id = 1");
  const signal=await signalTick(env,{engine:'e1',release:'r1'},now);
  check('a separate fast sample has the same live counts and writes no complete statistics',
    ['at','engine','release','open','urgent','asking','computing'].every(key=>signal[key]===s[key])&&
    !('queue_rows' in signal)&&!('settle_queue_rows' in signal)&&
    JSON.stringify(await L.get('SELECT * FROM state_factory_snapshot WHERE id=1'))===JSON.stringify(row));

  check("statistics without an owner retain an expired valid heartbeat and cannot suppress fallback settlement",
    row && row.daemon_commit === "" && Number.isFinite(Date.parse(row.daemon_at))
      && Date.parse(row.daemon_at) <= now - SETTLER_SILENT_MS, JSON.stringify(row));
  const beat=iso(now+1000);
  let releaseCount;
  const delayed={LIBRARY:{...L,prepare(sql){
    const stmt=L.prepare(sql);
    if(!sql.startsWith('WITH a AS'))return stmt;
    return {...stmt,bind(...args){const bound=stmt.bind(...args);return {...bound,first:async()=>{
      await new Promise(resolve=>{releaseCount=resolve;});return bound.first();
    }};}};
  }}};
  const late=snapshotTick(delayed,{engine:'e1',release:'r1'},now-1000);
  while(!releaseCount)await new Promise(resolve=>setTimeout(resolve,1));
  await L.run("UPDATE state_factory_snapshot SET daemon_at=?,daemon_commit='actual-owner' WHERE id=1",beat);
  releaseCount();await late;
  const retained=await L.get("SELECT daemon_at,daemon_commit FROM state_factory_snapshot WHERE id=1");
  check("a snapshot completing after a newer owner pulse cannot roll its heartbeat back",
    retained.daemon_at===beat&&retained.daemon_commit==='actual-owner',JSON.stringify(retained));
  const before=await L.get("SELECT * FROM state_factory_snapshot WHERE id=1");
  const countFailure=db=>({...db,prepare(sql){
    const stmt=db.prepare(sql);if(!sql.startsWith('WITH a AS'))return stmt;
    return {...stmt,bind(){return {first:async()=>{throw Error('count failed');}};}};
  }});
  const failed={LIBRARY:countFailure(L)};
  let rejected=false;try{await snapshotTick(failed,{},now+2000);}catch(error){rejected=error.message==='count failed';}
  check("failed counts remain a failure and change neither statistics nor heartbeat",rejected&&
    JSON.stringify(await L.get("SELECT * FROM state_factory_snapshot WHERE id=1"))===JSON.stringify(before));
  const empty=await testDb('library');
  let absentFailed=false;try{await snapshotTick({LIBRARY:countFailure(empty)}, {},now);}catch{absentFailed=true;}
  check("a first failed count invents neither a snapshot nor a heartbeat",absentFailed&&!(await empty.get("SELECT 1 FROM state_factory_snapshot")));
  const firstSignal=await signalTick({LIBRARY:empty},{},now);
  check('a first fast sample creates no full snapshot or heartbeat',firstSignal.open===0&&!(await empty.get('SELECT 1 FROM state_factory_snapshot')));
  await empty.close();
}

console.log("settling, in shadow");
{
  const VERBS = { simulate: { cap: 2, lease_ms: 30 * 60_000, pack: true, canon: (r, work) => canon({ score: r.score, work }), near: nearAnswer(["work"]) } };
  const f = factory({ verbs: VERBS, producers: { p: { on_fact: async () => {} } }, random: () => 0.999 });
  const t0 = now - 10 * 60_000;
  const ask = async (id) => {
    await (async (fixtureDb,args)=>{await seedComputeWork(fixtureDb,{id:args[0],verb:"simulate",request:"{}",state:"open",due_at:args[1],opened_at:args[2]});})(L,[id,iso(t0 + 3_600_000),iso(t0)]);
    await L.run("INSERT INTO relation_task_consumers (producer, ref, question, state, asked_at, due_at) VALUES ('p', ?, ?, 'live', ?, ?)", id, id, iso(t0), iso(t0 + 3_600_000));
  };
  const give = async (id, device, owner, score) => {
    await L.run("INSERT INTO entity_compute_devices (id, seen) VALUES (?, 'x') ON CONFLICT DO NOTHING", device);
    await L.run("UPDATE state_compute_work SET lease = ?, lease_until = ?, leased_to = ? WHERE id = ?", `L-${device}`, iso(t0 + 600_000), device, id);
    await f.answer(env, { device, owner }, { lease: `L-${device}`, engine: "e1", result: { score }, work: 7, compute_ms: 10 }, t0 + 1_000);
  };
  for (const id of ["s1", "s2", "s3"]) await ask(id);
  for (const id of ["s1", "s2", "s3"]) { await give(id, `${id}a`, "o1", 5); await give(id, `${id}b`, "o2", 5); }
  const facts = await L.all("SELECT id, state, canon FROM state_compute_work WHERE id IN ('s1', 's2', 's3') ORDER BY id");
  check("the request path made three facts", facts.every((q) => q.state === "fact"), JSON.stringify(facts));
  const from = { at: iso(t0 - 1), id: "" };
  let r = await settleShadow(env, { threshold: 2, from }, t0 + 1_000 + SETTLE_SHADOW_LAG_MS + 1);
  check("the shadow judges each settled question as the request path did", r.checked === 3 && r.ok === 3 && !Object.keys(r.differ).length, JSON.stringify(r));
  check("…and goes on from its cursor, judging none twice",
    (await settleShadow(env, { threshold: 2, from: r.cursor }, t0 + 1_000 + SETTLE_SHADOW_LAG_MS + 1)).checked === 0);
  check("a question settled less than the lag ago waits", (await settleShadow(env, { threshold: 2, from }, t0 + 1_000)).checked === 0);
  const injected=await L.get("SELECT task_id,fact_engine,fact_at FROM state_compute_work WHERE id='s1'");
  const evidence=await L.all("SELECT id FROM state_compute_candidate_answers WHERE question='s1'");
  const corrected=await L.get("SELECT compute_prepare_correction('s1',?1::bigint,'x',?2::text,?3::text,?4::text) AS task_id",
    injected.task_id,injected.fact_engine,injected.fact_at,JSON.stringify(evidence.map(answer=>answer.id)));
  await L.run("SELECT compute_accept_result('s1',?1::bigint,'open','fact','x',?2::text,?3::text,0,'[]')",
    corrected.task_id,injected.fact_engine,injected.fact_at);
  await L.run("UPDATE record_candidate_answers SET credited = 0 WHERE question = 's2' AND device = 's2b'");
  r = await settleShadow(env, { threshold: 2, from }, t0 + 1_000 + SETTLE_SHADOW_LAG_MS + 1);
  check("a fact whose canon no group gave, and a witness left unpaid, are each a difference",
    r.differ.canon === 1 && r.differ.uncredited === 1 && r.ok === 1, JSON.stringify(r));
  const finding = JSON.parse((await L.get("SELECT body FROM state_factory_findings WHERE kind = 'settle_shadow'")).body);
  check("the finding keeps the running count and the questions that differed", finding.checked === 6 && finding.samples.length === 2, JSON.stringify(finding));
  check("an official answer is the official path's to judge, an older engine's a peer's",
    judgeFact({ fact_engine: "e1", fact_at: "z", canon: "k" }, [{ engine: "e1", official: 1, canon: "k", at: "a" }], 2).skip === "official"
    && judgeFact({ fact_engine: "e1", fact_at: "z", canon: "k" }, [{ engine: "e1", owner: "a", canon: "k", at: "a" }, { engine: "e0", owner: "b", canon: "j", at: "a" }], 2).skip === "peer");
  check("an older engine's answer of the same bits joins the group", judgeFact({ fact_engine: "e1", fact_at: "z", canon: "k" },
    [{ engine: "e1", owner: "a", canon: "k", at: "a", credited: 1 }, { engine: "e0", owner: "b", canon: "k", at: "a", credited: 1 }], 2).ok === true);
  check("two answers of one owner are one witness", judgeFact({ fact_engine: "e1", fact_at: "z", canon: "k" },
    [{ engine: "e1", owner: "a", canon: "k", at: "a", credited: 1 }, { engine: "e1", owner: "a", canon: "k", at: "a", credited: 1 }], 2).differ === "no_group");
}

console.log("the daemon");
{
  const { run } = await import("./factoryd.mjs");
  process.env.DAEMON_COMMIT = "c0ffee";
  const prior=await L.get("SELECT daemon_at,daemon_commit FROM state_factory_snapshot WHERE id=1");
  const code = await run({ once: true, opened: { env, close: async () => {} }, servedNow: async () => ({ engine: "e1", release: "r1" }), target: null });
  const row = await L.get("SELECT daemon_at,daemon_commit, engine FROM state_factory_snapshot WHERE id = 1");
  check("one pass without ownership runs clean and cannot replace the real owner's heartbeat", code === 0
    && row.daemon_at===prior.daemon_at&&row.daemon_commit===prior.daemon_commit&&row.engine === "e1",
    JSON.stringify({ code, row }));
  check("without an R2 token the signal is skipped and said so, the other ticks unharmed",
    code === 0 && !!(await L.get("SELECT 1 AS x FROM state_factory_findings WHERE kind = 'signal_unconfigured'")));
}

console.log("the work signal");
{
  const { run, SIGNAL_KEY, SIGNAL_CACHE } = await import("./factoryd.mjs");
  const puts = [];
  const target = { endpoint: "https://acct.r2.cloudflarestorage.com", bucket: "wfsim-public", accessKey: "k", secretKey: "s" };
  const go = () => run({ once: true, opened: { env, close: async () => {} }, servedNow: async () => ({ engine: "e1", release: "r1" }),
    target, put: async (t, o) => { puts.push({ t, o }); } });
  const signals = () => puts.filter((p) => p.o.key === SIGNAL_KEY);
  check("one pass writes the signal once", (await go()) === 0 && signals().length === 1, JSON.stringify(puts.map((p) => p.o.key)));
  const { o } = signals()[0], body = JSON.parse(o.body);
  const { DEMAND_KEY, DEMAND_CACHE } = await import("../worker/contribution.js");
  const demands = puts.filter((p) => p.o.key === DEMAND_KEY);
  check("…and the compute page's picture once, as demand.json, kept half a minute at the edge",
    demands.length === 1 && DEMAND_KEY === "demand.json" && demands[0].o.cacheControl === DEMAND_CACHE
      && JSON.parse(demands[0].o.body).v === 1, JSON.stringify(demands.map((p) => p.o)));
  const { TALLY_KEY, TALLY_CACHE } = await import("../worker/contribution.js");
  const tallies = puts.filter((p) => p.o.key === TALLY_KEY), told = tallies[0] && JSON.parse(tallies[0].o.body);
  check("…and the home page's count once, as stats/tally.json, kept half a minute at the edge, nobody named in it",
    tallies.length === 1 && TALLY_KEY === "stats/tally.json" && tallies[0].o.cacheControl === TALLY_CACHE && TALLY_CACHE === "public, max-age=30"
      && JSON.stringify(Object.keys(told)) === JSON.stringify(["ok", "v", "at", "totals", "per_hour", "computing"]),
    JSON.stringify(tallies.map((p) => p.o)));
  check("…as work.json, kept five seconds at the edge, as JSON",
    o.key === SIGNAL_KEY && SIGNAL_KEY === "work.json" && o.cacheControl === "public, max-age=5" && SIGNAL_CACHE === o.cacheControl && o.contentType === "application/json", JSON.stringify(o));
  const { PROTOCOL, PROTOCOLS_SERVED } = await import("../worker/verify.js");
  const snap = await L.get("SELECT * FROM state_factory_snapshot WHERE id = 1");
  check("its independently sampled counts, served engine, protocols and dials are current",
    body.v === 1 && Number.isFinite(Date.parse(body.at)) && Date.parse(body.at) <= Date.parse(snap.at) && body.open === 1 && body.urgent === 1 && body.computing === Number(snap.computing)
    && body.engine === "e1" && body.release === "r1" && body.protocol === PROTOCOL && body.protocols.length === PROTOCOLS_SERVED.size
    && body.poll_ms === DIAL_DEFAULTS.poll_ms && body.jitter_ms === DIAL_DEFAULTS.jitter_ms
    && body.urgent_jitter_ms === DIAL_DEFAULTS.urgent_jitter_ms && body.mode === "signal"
    && Object.keys(body).join() === "v,at,open,urgent,engine,release,protocol,protocols,computing,poll_ms,jitter_ms,urgent_jitter_ms,mode", JSON.stringify(body));
  const { setDial, forgetDials } = await import("../worker/dials.js");
  await setDial(env, "signal_mode", 1, "check");
  await setDial(env, "poll_ms", 30_000, "check");
  puts.length = 0; await go(); forgetDials();
  const turned = JSON.parse(signals()[0].o.body);
  check("the owner's dials reach the signal: ask mode turns every computer back to asking", turned.mode === "ask" && turned.poll_ms === 30_000, JSON.stringify(turned));
  await setDial(env, "signal_mode", 0, "check"); await setDial(env, "poll_ms", 10_000, "check"); forgetDials();
  const { workWaiting } = await import("../worker/settler.js");
  const w = await workWaiting(L);
  check("the wave probe sees open work and waited-on work", w.open === true && w.urgent === true, JSON.stringify(w));
  await L.run("UPDATE state_compute_work SET state = 'withdrawn' WHERE id IN ('o1', 'u1')");
  const none = await workWaiting(L);
  check("…and none when nothing may be taken", none.open === false && none.urgent === false, JSON.stringify(none));
}

console.log("independent signal sampling");
for(const stopMode of ['lost','stop']){
  const {run,SIGNAL_KEY}=await import('./factoryd.mjs');
  const {DEMAND_KEY}=await import('../worker/contribution.js');
  const db=await testDb('library'),defer=()=>{let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};};
  const started=defer(),releaseSlow=defer(),firstPut=defer(),sampleHeld=defer(),releaseSample=defer(),listening=defer();
  let blocked=false,samples=0,fastDials=0,pulses=0,lost,closed=false;
  const puts=[];
  const wrap=(base,before)=>({...base,prepare(sql){
    const wrapStatement=stmt=>({...stmt,bind(...args){return wrapStatement(stmt.bind(...args));},
      first:async()=>{await before(sql);return stmt.first();},all:async()=>{await before(sql);return stmt.all();},
      run:async()=>{await before(sql);return stmt.run();}});
    return wrapStatement(base.prepare(sql));
  }});
  const slow=wrap(db,async sql=>{
    if(sql.startsWith('WITH a AS')){blocked=true;started.resolve();await releaseSlow.promise;throw Error('full count failed');}
    if(blocked)await releaseSlow.promise;
  });
  const fast=wrap(db,async sql=>{
    if(sql==='SELECT name, value FROM state_factory_parameters')fastDials++;
    if(sql.startsWith('WITH a AS')){
      samples++;await started.promise;
      if(samples===2){sampleHeld.resolve();await releaseSample.promise;}
      if(sql.includes('stats_pending')||sql.includes('state_pending_verifications'))throw Error('signal read includes full counts');
    }
  });
  await db.run(`INSERT INTO state_factory_snapshot(id,at,open,urgent,asking,computing,queue_rows,settle_queue_rows,daemon_at,daemon_commit)
    VALUES(1,'old-statistics',7,2,3,4,5,6,'1970-01-01T00:00:00.000Z','')`);
  const before=await db.get('SELECT * FROM state_factory_snapshot');
  const timeout=setTimeout,clear=clearTimeout;
  const wait=async(promise,label)=>{let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=timeout(()=>reject(Error(label)),5000);})]);}finally{clear(timer);}};
  forgetDials();mock.timers.enable({apis:['setTimeout']});
  let running;
  try{
    running=run({opened:{env:{LIBRARY:slow},signalEnv:{LIBRARY:fast},backgroundEnv:{LIBRARY:slow},
      stageEnvs:Object.fromEntries(['admission','verification','delivery','contribution'].map(name=>[name,{LIBRARY:slow}])),
      claim:async fn=>{lost=fn;return {pulse:async()=>{pulses++;return true;},release:async()=>{}};},
      listen:async()=>listening.resolve(),close:async()=>{closed=true;}},
      servedNow:async()=>({engine:null,release:'signal-release'}),target:{bucket:'isolated-fixture'},
      put:async(_,object)=>{puts.push(object);if(object.key===SIGNAL_KEY)firstPut.resolve();},
      readEdge:async()=>({at:iso(Date.now()),cache:'HIT'})});
    await wait(listening.promise,'daemon clocks did not start');mock.timers.tick(0);await wait(firstPut.promise,'blocked full counts prevented the independent work signal');
    check('blocked full counts do not block a real fresh work signal or owner pulse',
      blocked&&pulses>0&&puts.filter(p=>p.key===SIGNAL_KEY).length===1&&
      Number.isFinite(Date.parse(JSON.parse(puts[0].body).at)));
    check('a fast sample never restamps old goal counts or the complete snapshot',
      JSON.stringify(await db.get('SELECT * FROM state_factory_snapshot'))===JSON.stringify(before)&&
      !puts.some(p=>p.key===DEMAND_KEY));
    await seedComputeWork(db,{id:'signal-wave',verb:'simulate',state:'open',request:'{}',opened_at:iso(now),due_at:iso(now)});
    forgetDials();const staleDials=dialsOf({LIBRARY:slow});
    mock.timers.tick(2000);await wait(sampleHeld.promise,'blocked main-pool dials prevented the wave sample');
    check('a wave reads expired dials on the reserved fast pool while the main pool is blocked',fastDials>=2);
    mock.timers.tick(10000);await new Promise(resolve=>setImmediate(resolve));
    check('timer and wave share one in-flight sample and one writer',samples===2&&puts.filter(p=>p.key===SIGNAL_KEY).length===1);
    const heldPulses=pulses;if(stopMode==='lost')lost();else process.emit('SIGTERM');releaseSample.resolve();releaseSlow.resolve();await staleDials;
    check(stopMode+' drains the sample without publishing it or pulsing again',(await running)===(stopMode==='lost'?1:0)&&closed&&
      puts.filter(p=>p.key===SIGNAL_KEY).length===1&&pulses===heldPulses);
    check('failed full counts remain visibly stale after the fast signal succeeds',
      (await db.get('SELECT at FROM state_factory_snapshot')).at==='old-statistics');
  }finally{
    releaseSample.resolve();releaseSlow.resolve();if(running&&!closed)process.emit('SIGTERM');
    if(running)await running;mock.timers.reset();forgetDials();await db.close();
  }
}

console.log("signing for R2");
{
  // AWS's published example (S3 API reference, "Signature Calculations for the
  // Authorization Header", GET Object): the signature must come out to the hex they print.
  const { signV4 } = await import("./sigv4.mjs");
  const s = signV4({ method: "GET", url: "https://examplebucket.s3.amazonaws.com/test.txt", headers: { range: "bytes=0-9" },
    payloadHash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    accessKey: "AKIAIOSFODNN7EXAMPLE", secretKey: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    region: "us-east-1", service: "s3", amzDate: "20130524T000000Z" });
  check("the signature is AWS's own example to the last digit",
    s.signature === "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41", s.signature);
  check("…and the header names what was signed",
    s.authorization === "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41",
    s.authorization);
}

const statisticsDb=await testDb('library');
try{await statisticsReadCases(statisticsDb);}finally{await statisticsDb.close();}
console.log(failures ? `\n${failures} failure(s)` : "\nthe settler in shadow agrees with the request path, and says where it does not");
process.exit(failures ? 1 : 0);
