// ---- checkpoint / resume -------------------------------------------------
// A reload KILLS the worker, and there is no browser mechanism that avoids it
// (measured 2026-07-30: a SharedWorker is terminated too, the moment its last
// client disconnects, busy or idle). So instead of pretending a run can
// survive, make losing it cheap: the worker emits the surviving field after
// every completed round, and a fresh page rebuilds from the last one.
//
// Stored as IDENTITIES only — (mod pool indices, evo set, exilus, arcane) — so
// it fits localStorage and cannot drift from what the engine would rebuild.
// The REQUEST is stored with it and is what a resume replays: a checkpoint
// only ever means anything under the scope that produced it, so it is never
// re-derived from whatever the form happens to say later.
const OPT_CKPT = "wfsim-optimize-checkpoint";
function saveCheckpoint(body, cp, board) {
  const write = (payload) => localStorage.setItem(OPT_CKPT, JSON.stringify(payload));
  try {
    write({ body, cp, board, at: Date.now() });
  } catch (_) {
    // A screen cut is the whole surviving field — tens of thousands of pairs.
    // If it does not fit alongside the leaderboard, the RESUME POINT is worth
    // more than the display copy, so drop the board and keep the cut.
    try { write({ body, cp, at: Date.now() }); } catch (_) { /* nothing fits: no checkpoint */ }
  }
}
const clearCheckpoint = () => { try { localStorage.removeItem(OPT_CKPT); } catch (_) {} };
function loadCheckpoint() {
  try {
    const s = JSON.parse(localStorage.getItem(OPT_CKPT));
    if (!s || !s.body || !s.cp) return null;
    // A day-old checkpoint is almost certainly not what the visitor meant to
    // resume, and the data behind it may have been rebuilt since.
    if (Date.now() - (s.at || 0) > 24 * 3600 * 1000) { clearCheckpoint(); return null; }
    return s;
  } catch (_) { return null; }
}

// HOW MANY WORKERS the browser search runs on. This is the only lever the
// browser has: it is single-threaded at ~150 simulated engagements per second
// against ~5,100 on a 26-thread desktop, so coverage is scarcest exactly where
// there is least compute. A WALK gives the N workers DISJOINT STRIDES of the
// shuffled index range (`shard`, `shard + shards`, …), a partition — nothing
// is evaluated twice and nothing is missed (`shards_partition_the_shuffled_
// order_exactly`). A DESCENT gives each worker its share of the starts, so a
// worker with no start of its own returns an empty, complete envelope.
//
// THE COUNT IS THE TOPBAR'S COMPUTE SHARE, and it is the whole answer. A `CPU
// threads` box in a search preset — on the reasoning that a heavy scope might
// want more cores than a light one — is two controls for one fact, which is
// the arena's own rule, and it puts the override on the one thing most able
// to
// cook a phone, which is the last place a global heat setting should be
// ignorable. `poolSize()` is the setting; this is now only a name for it.
const woptWorkerCount = () => poolSize();

// Merge the fleet's leaderboards into one. Each worker ran its own funnel over
// its own elites, so every row here is measured at the SAME run count under the
// SAME scenario and the scores are directly comparable — the merge is a sort.
//
// Deduplicate first: two descents can reach the same build from different
// starts. Counting it twice would push a real alternative off the board.
function woptMerge(parts) {
  const bad = parts.find((p) => p && p.ok === false);
  if (bad) return bad;
  // A shard that owned no ground returns an empty but complete envelope, so
  // any part is a valid head — prefer one that actually ranked something.
  const rows = [];
  const seen = new Map();
  for (const p of parts) {
    for (const r of p.results || []) {
      const key = JSON.stringify([[...(r.mods || [])].sort(), r.arcane, r.evolutions, r.exilus, r.mode, r.valence]);
      // Two shards' starts that settled on one build are ONE answer from both.
      const had = seen.get(key);
      if (had) { had.from_starts = [...(had.from_starts || []), ...(r.from_starts || [])]; continue; }
      const row = { ...r };
      seen.set(key, row);
      rows.push(row);
    }
  }
  rows.sort((a, b) => (b.kill_progress ?? b.kills ?? 0) - (a.kill_progress ?? a.kills ?? 0));
  const head = parts.find((p) => (p.results || []).length) || parts[0] || {};
  const finalists = head.finalists || rows.length;
  const space = head.space || 0;
  const sampled = parts.reduce((n, p) => n + (p.sampled || 0), 0);
  return {
    ...head,
    // EVERY shard must have finished its stride for the union to be the space.
    exhaustive: parts.length > 0 && parts.every((p) => p.exhaustive),
    // A descent's starts are split across the fleet, so they add up, and one
    // shard the clock stopped makes the whole run a cut one.
    strategy: (parts.find((p) => p.strategy) || head).strategy,
    starts: parts.reduce((n, p) => n + (p.starts || 0), 0),
    cut: parts.some((p) => p.cut),
    // Coverage of the FLEET, not of one worker: the strides are disjoint, so
    // the positions add up.
    coverage: space > 0 ? Math.min(1, sampled / space) : 0,
    sampled,
    searched: parts.reduce((n, p) => n + (p.searched || 0), 0),
    candidates: parts.reduce((n, p) => n + (p.candidates || 0), 0),
    jobs: parts.reduce((n, p) => n + (p.jobs || 0), 0),
    cancelled: parts.some((p) => p.cancelled),
    failed_starts: parts.flatMap((p) => p.failed_starts || []),
    results: rows.slice(0, finalists).map((r, i) => ({ ...r, rank: i + 1 })),
  };
}

// Loading timers prove contact; engine callbacks and returns prove calculation feedback.
function optimizerWorkerSignals() {
  const workers = [];
  let engineAt = null, responseAt = null;
  return {
    start(i, activity, warm = false) {
      workers[i] = { loaded: workers[i]?.loaded || warm, activity };
    },
    heard(i, message) {
      responseAt = Date.now();
      const worker = workers[i];
      if (!worker) return;
      const engine = (message.kind === "alive" && message.source === "engine")
        || ["progress", "result", "board", "builds", "checkpoint"].includes(message.kind);
      if (engine) engineAt = responseAt;
      if (engine || message.kind === "rested") worker.loaded = true;
    },
    finish(i) { if (workers[i]) workers[i].activity = null; },
    read(activity) {
      const pending = workers.filter((w) => w?.activity);
      const active = pending.filter((w) => w.loaded && w.activity !== "resting");
      const loading = pending.filter((w) => !w.loaded);
      return { activity: !active.length && loading.length ? "loading"
        : pending.length && pending.every((w) => w.activity === "resting") ? "resting" : activity,
        workers_active: active.length, workers_loading: loading.length,
        engine_last_at: engineAt, response_last_at: responseAt };
    },
  };
}

/// THE QUICK DESCENT ACROSS THE FLEET (docs/WASM.md). A worker is one thread
/// and cannot wait on another, so worker 0 LEADS: it runs the descent on the
/// scores it holds and answers with the builds its starts wait for; every
/// worker scores a slice of them, and the scores go back to the leader, until
/// it answers with the result. Every worker is busy on every batch, whatever
/// the number of starts.
function woptQuickFleet(body, n, onBuilds) {
  const job = quickFleet(body, n, undefined, { onBuilds });
  wopt = job;
  return { ok: true, job_id: job.id };
}

/// WORKERS A BACKGROUND SEARCH HANDS BACK, for the next one to start on: a
/// new worker does nothing until its 10 MB engine has loaded, and a
/// volunteer's searches come one after another. Kept a minute idle, then let go.
const FLEET_KEEP_MS = 60_000;
const fleetKept = [];
let fleetReap = null;
function fleetKeep(workers) {
  workers.forEach((w) => { w.onmessage = null; w.onerror = null; fleetKept.push(w); });
  clearTimeout(fleetReap);
  fleetReap = setTimeout(() => fleetKept.splice(0).forEach((w) => w.terminate()), FLEET_KEEP_MS);
}

/// …ONE SUCH SEARCH, as a job object of its own: the reader's optimizer holds
/// one (`wopt`), and a riven gain run for someone else holds another, so
/// neither waits on the other. `pace` is awaited before every round — the
/// background's way to let the reader go first. The result carries `work`:
/// what every scorer's fights and the final round cost (`Shard::work`).
///
/// `n` IS A NUMBER OR A FUNCTION asked before every round, so a background
/// search takes the cores the reader leaves it now, not the ones it had when it
/// began. A round's builds are dealt in slices a free worker pulls, so one slow
/// slice does not hold every other core idle; the scores go back in the order
/// of the builds, which is all the leader reads, so the answer is the same
/// however many workers scored it. `keep` hands the workers back (`fleetKeep`);
/// `rest(ms)` is how long a worker rests after a call that took `ms`, so a
/// background search runs the share of the time its owner set.
function quickFleet(body, n, pace = async () => {}, { keep = false, rest = () => 0, onBuilds = async () => {} } = {}) {
  const lanesNow = () => Math.max(1, Math.floor(typeof n === "function" ? n() : n) || 1);
  const job = { id: woptNextId++, workers: [], status: null, result: null, board: null,
    cancelled: false, shards: lanesNow(), t0: Date.now() };
  // …AND WHAT IT TOOK: every call's own time summed over the workers, the rests
  // between calls not in it (`cpu_ms`), and how many rounds the leader ran.
  const done = { builds: 0, fights: 0, work: 0, cpu_ms: 0, steps: 0 };
  const live = [];
  const signals = optimizerWorkerSignals();
  const warm = [];
  const pendingCalls = new Set();
  let activity = "planning", finalStatus = null;
  // Where each start stands, as the leader last reported it.
  let starts = [];
  const emptyRound = () => ({ builds: 0, fights: 0, total: 0, slices_done: 0, slices_total: 0 });
  let round = emptyRound();
  const show = () => {
    const cur = live.filter(Boolean);
    job.status = { phase: "searching", round: 0, rounds: 0, round_jobs: 0, round_runs: 0, sims_planned: 0, notes: [], ...finalStatus,
      workers: job.workers.length, starts,
      enumerated: done.builds + round.builds + cur.reduce((a, s) => a + (s.enumerated || 0), 0),
      sims_done: done.fights + round.fights + cur.reduce((a, s) => a + (s.sims_done || 0), 0) + (finalStatus?.sims_done || 0),
      batches_completed: done.steps, batch_builds_done: round.builds, batch_builds_total: round.total,
      batch_slices_done: round.slices_done, batch_slices_total: round.slices_total,
      ...signals.read(activity) };
  };
  const stop = (finished) => {
    pendingCalls.forEach(finish => finish(null));
    const ws = job.workers;
    job.workers = [];
    if (finished && keep) fleetKeep(ws); else ws.forEach((w) => w && w.terminate());
  };
  const fail = (why, engine = false) => {
    if (job.result) return;
    job.result = { ok: false, error: why, worker_dead: !engine };
    stop(false);
    show();
  };
  // One call to one worker; its progress, if any, to `onProgress`.
  const call = (i, b, onProgress, resting = false) => new Promise((resolve) => {
    const w = job.workers[i];
    if (!w) { resolve(null); return; }
    const wd = watchSilence(() => fail("worker stopped answering"));
    let finished = false, deliveries = null;
    const finish = payload => { if (finished) return; finished = true; pendingCalls.delete(finish); wd.clear(); signals.finish(i); resolve(payload); };
    pendingCalls.add(finish);
    w.onmessage = (e) => {
      try {
        wd.heard();
        signals.heard(i, e.data);
        if (e.data.kind === "progress" && onProgress) onProgress(e.data.payload);
        if (e.data.kind === "builds") {
          const builds = Array.isArray(e.data.payload) ? e.data.payload : e.data.payload?.builds;
          if (builds?.length) {
            deliveries = (deliveries || Promise.resolve()).then(() => onBuilds(builds));
            deliveries.catch(error => fail(String(error?.message || error)));
          }
        }
        if (e.data.kind === (resting ? "rested" : "result")) {
          const payload = e.data.payload;
          if (Array.isArray(payload?.builds)) {
            const builds = payload.builds; delete payload.builds;
            if (builds.length) deliveries = (deliveries || Promise.resolve()).then(() => onBuilds(builds));
          }
          if (payload?.ok === false) {
            if (deliveries) deliveries.catch(error => fail(String(error?.message || error)));
            fail(payload.error || "The worker could not finish.", true);
          } else if (deliveries) deliveries.then(() => finish(payload), error => fail(String(error?.message || error)));
          else finish(payload);
        }
        show();
      } catch (error) { fail(String(error?.message || error)); }
    };
    w.onerror = (e) => { e?.preventDefault?.(); fail(String(e?.message || "worker error")); };
    signals.start(i, resting ? "resting" : activity, resting || warm[i]);
    show();
    try {
      wd.start();
      w.postMessage(resting ? { kind: "rest", ms: b } : { kind: "optimize", body: b });
    } catch (error) { fail(String(error?.message || error)); }
  });
  const pause = (i, ms) => {
    const r = rest(ms);
    if (!(r > 0) || !job.workers[i]) return Promise.resolve();
    return call(i, r, null, true);
  };
  const timed = async (i, b, onProgress) => {
    const t = performance.now();
    const o = await call(i, b, onProgress);
    const ms = performance.now() - t;
    done.cpu_ms += ms;
    if (o) await pause(i, ms);
    return job.cancelled || job.result ? null : o;
  };
  const grow = (k) => {
    while (job.workers.length < k) {
      const held = fleetKept.pop();
      warm.push(!!held);
      job.workers.push(held || spawnEngine());
    }
  };
  (async () => {
    grow(job.shards);
    show();
    let scores = [];
    for (let step = 0; ; step++) {
      activity = "resting";
      show();
      await pace();
      if (job.cancelled || job.result) return;
      activity = "planning";
      const r = await timed(0, { ...body, quick_fleet: { lead: true, fresh: step === 0, scores } },
        (p) => {
          if (p.phase === "running" && p.jobs != null) { finalStatus = p; activity = "final_simulation"; }
        });
      if (!r || job.cancelled) return;
      if (!r.pending) {
        job.result = { ...r, work: done.work + (r.final_work || 0), fights: done.fights, builds: done.builds, steps: step, cpu_ms: Math.round(done.cpu_ms) };
        stop(true);
        return;
      }
      starts = r.progress || starts;
      const k = Math.min(lanesNow(), r.pending.length);
      grow(k);
      // TWO SLICES A WORKER: enough that a fast one takes a slow one's share,
      // few enough that each call's fixed cost stays small beside its fights.
      const size = Math.max(1, Math.ceil(r.pending.length / (k * 2)));
      const slices = [];
      for (let at = 0; at < r.pending.length; at += size) slices.push(r.pending.slice(at, at + size));
      const outs = new Array(slices.length).fill(null);
      let next = 0, broken = false;
      activity = "scoring";
      round = { ...emptyRound(), total: r.pending.length, slices_total: slices.length };
      show();
      await Promise.all(Array.from({ length: k }, async (_, i) => {
        while (!broken && !job.cancelled && next < slices.length) {
          const at = next++;
          const o = await timed(i, { ...body, quick_fleet: { score: slices[at] } }, (st) => { live[i] = st; show(); });
          live[i] = null;
          if (!o) { broken = true; return; }
          outs[at] = o;
          round.builds += (o.scores || []).length;
          round.fights += o.fights || 0;
          round.slices_done += 1;
          show();
        }
      }));
      if (broken || outs.some((o) => !o) || job.cancelled) return;
      const bad = outs.find((o) => o.ok === false);
      if (bad) { fail(bad.error || "a worker could not score its share", true); return; }
      scores = outs.flatMap((o) => o.scores || []);
      done.builds += scores.length;
      done.fights += outs.reduce((a, o) => a + (o.fights || 0), 0);
      done.work += outs.reduce((a, o) => a + (o.work || 0), 0);
      done.steps += 1;
      round = emptyRound();
      live.fill(null);
      show();
    }
  })().catch(error => fail(String(error?.message || error)));
  return job;
}

function woptStart(body, checkpoint, onCheckpoint, onBuilds = builds => keepOptimizationBuilds(builds, { kind: "public" })) {
  if (wopt && wopt.workers && wopt.workers.length) {
    return { ok: false, error: "an optimization is already running — cancel it or wait", job_id: wopt.id };
  }
  const { __resume, ...req } = body ?? {}; // the resume marker is transport, not scope
  body = req;
  if (body.strategy === "quick" && !checkpoint) return woptQuickFleet(body, Math.max(1, woptWorkerCount()), onBuilds);
  // A CHECKPOINT is one worker's field, so it can only resume a run that had
  // one worker. Rather than resume a fraction of a fleet, a resume runs
  // unsharded — slower, but it is continuing a search that already exists.
  const shards = checkpoint ? 1 : woptWorkerCount();
  const job = {
    id: woptNextId++, workers: [], status: null, statuses: new Array(shards).fill(null),
    parts: new Array(shards).fill(null), result: null, board: null, boards: new Array(shards).fill(null),
    cancelled: false, shards, t0: Date.now(),
  };
  const signals = optimizerWorkerSignals();
  // One STATUS out of many: the fleet's progress is the sum of its workers,
  // and the phase is the least advanced of them — a run is still searching
  // while any worker still is.
  const rollup = () => {
    const live = job.statuses.filter(Boolean);
    const sum = (k) => live.reduce((n, s) => n + (Number(s[k]) || 0), 0);
    const searching = live.some((s) => s.phase === "searching") || live.length < shards;
    return {
      round: 0, rounds: 0, round_jobs: 0, round_runs: 0, notes: [],
      ...live[0],
      phase: searching ? "searching" : "running",
      sims_done: sum("sims_done"), sims_planned: sum("sims_planned"),
      enumerated: sum("enumerated"),
      round_jobs: sum("round_jobs"),
      workers: shards, workers_done: job.parts.filter(Boolean).length,
      ...signals.read("scoring"),
    };
  };
  const watchdogs = [];
  const failJob = (i, why) => {
    if (job.parts[i] || job.result || job.cancelled) return;
    job.parts[i] = { ok: false, error: why };
    job.result = job.parts[i];
    watchdogs.forEach(wd => wd.clear());
    job.workers.forEach((worker, index) => { signals.finish(index); worker?.terminate(); });
    job.workers = [];
    job.status = rollup();
  };
  for (let i = 0; i < shards && !job.result; i++) {
    const gone = why => failJob(i, why);
    let w;
    try { w = spawnEngine(); } catch (error) { gone(String(error?.message || error)); break; }
    job.workers.push(w);
    // THE SEARCH'S WORKERS ARE WATCHED TOO. They are not lanes — they are made
    // here and run one long call — but the failure is the same: a reclaimed
    // worker neither answers nor errors, and this fleet had only `onerror`. A
    // search that can never finish and can never fail leaves the reader with a
    // progress bar that has stopped and no way to tell it from a slow one.
    const wd = watchSilence(() => gone("worker stopped answering"));
    watchdogs.push(wd);
    let deliveries = null;
    w.onmessage = (e) => {
      wd.heard();
      signals.heard(i, e.data);
      if (e.data.kind === "progress") job.statuses[i] = e.data.payload;
      if (e.data.kind === "board") { job.boards[i] = e.data.payload; job.board = woptMerge(job.boards.filter(Boolean)); }
      if (e.data.kind === "builds") {
        const builds = Array.isArray(e.data.payload) ? e.data.payload : e.data.payload?.builds;
        if (builds?.length) {
          deliveries = (deliveries || Promise.resolve()).then(() => onBuilds(builds));
          deliveries.catch(error => gone(String(error?.message || error)));
        }
      }
      if (e.data.kind === "checkpoint") {
        const { board, ...cp } = e.data.payload;
        if (board) { job.boards[i] = board; job.board = woptMerge(job.boards.filter(Boolean)); }
        // Only an UNSHARDED run can be resumed from a checkpoint — see above.
        if (shards === 1) {
          if (onCheckpoint) onCheckpoint(cp); else saveCheckpoint(body, cp, board || job.board);
        }
      }
      if (e.data.kind === "result") {
        wd.done();
        signals.finish(i);
        w.terminate();
        job.workers[i] = null;
        const payload = e.data.payload;
        if (Array.isArray(payload?.builds)) {
          const builds = payload.builds; delete payload.builds;
          if (builds.length) deliveries = (deliveries || Promise.resolve()).then(() => onBuilds(builds));
        }
        const complete = () => {
          if (job.cancelled || job.result) return;
          job.parts[i] = payload;
          if (job.parts.every(Boolean)) {
            job.result = woptMerge(job.parts);
            if (!onCheckpoint) clearCheckpoint();
            job.workers = [];
          }
        };
        if (payload?.ok === false) {
          if (deliveries) deliveries.catch(error => gone(String(error?.message || error)));
          gone(payload.error || "The worker could not finish.");
        } else if (deliveries) deliveries.then(complete).catch(error => gone(String(error?.message || error)));
        else complete();
      }
      job.status = rollup();
    };
    w.onerror = (e) => { e?.preventDefault?.(); gone(String(e?.message || "worker error")); };
    signals.start(i, "scoring");
    job.status = rollup();
    try {
      wd.start();
      w.postMessage({ kind: "optimize", body: { ...body, shard: i, shards }, checkpoint: checkpoint || null });
    } catch (error) { gone(String(error?.message || error)); }
  }
  wopt = job;
  return { ok: true, job_id: job.id };
}
function woptStatus() {
  if (!wopt) return { ok: false, error: "no such optimize job" };
  const st = wopt.status || { round: 0, rounds: 0, round_jobs: 0, round_runs: 0, sims_done: 0, sims_planned: 0, notes: [] };
  const out = { ...st, ok: true, job_id: wopt.id, elapsed_s: (Date.now() - wopt.t0) / 1000,
    // The worker's heartbeat carries its own phase (enumerating/running) —
    // keep it; the fallbacks cover the moments before the first message.
    phase: (wopt.status && wopt.status.phase) || (wopt.status ? "running" : "enumerating") };
  if (wopt.cancelled) {
    out.phase = "cancelled";
    // Cancel KILLED the worker, so there is no returned result and never will
    // be — hand back the last best-so-far it pushed out instead. It is already
    // result-shaped and flagged `cancelled`, so the normal renderer labels it
    // lower-precision without knowing where it came from.
    if (!wopt.result && wopt.board) out.result = wopt.board;
  }
  if (wopt.result) {
    out.result = wopt.result;
    out.phase = wopt.result.ok === false ? "error" : (wopt.result.cancelled ? "cancelled" : "done");
  }
  return out;
}
function woptCancel() {
  if (!wopt) return { ok: false, error: "no such optimize job" };
  // Cancel kills the WHOLE fleet. Each worker's last pushed board is already
  // here and merged, which is what a cancel has to show.
  if (wopt.workers && wopt.workers.length) {
    wopt.workers.forEach((w) => w && w.terminate());
    wopt.workers = [];
    wopt.cancelled = true;
  }
  return { ok: true, job_id: wopt.id };
}

async function api(path, body, onProgress, opts) {
  if (!WASM) {
    // GET endpoints (the rest are POST-with-body):
    if (path === "/api/meta" || path === "/api/i18n") return (await fetch(path)).json();
    return (await fetch(path, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body ?? {}), signal: opts && opts.signal,
    })).json();
  }
  // A FILE THE BUILD WROTE answers before any lane would; the engine is asked
  // only where the file did not come.
  if (ENGINE_VIEWS[path]) {
    try {
      const r = await fetch(ENGINE_VIEWS[path]);
      if (r.ok) return await r.json();
    } catch (_) { /* the engine answers it */ }
  }
  if (path === "/api/optimize") return woptStart(body, body && body.__resume, opts?.checkpoint, opts?.builds);
  if (path === "/api/optimize/status" || path === "/api/optimize/cancel") {
    if (body?.id != null && wopt?.id !== body.id) return { ok: false, error: "no such optimize job" };
    return path.endsWith("/cancel") ? woptCancel() : woptStatus();
  }
  // A LANE THAT DIED MID-CALL IS DROPPED AND THE CALL RE-ASKED ON A FRESH ONE.
  //
  // ONCE WAS NOT ENOUGH, and the assumption behind once was wrong: a refusal
  // was read as "the deployment is broken, and it will be broken again", which
  // is true of a module that will not download and false of a WORKER the
  // browser reclaimed. Reclaims are not surgical — the renderer takes several
  // at a time — so two dead lanes in a row is the ordinary shape of it, and a
  // single retry handed the caller a failure with twelve healthy lanes idle.
  // For the quick calc that caller is the BASELINE, and a failed baseline shows
  // the reader an empty list.
  //
  // STILL BOUNDED, and low: each attempt costs the watchdog's window, and a
  // pool that is genuinely gone must end in an answer rather than a loop.
  let r = null;
  for (let attempt = 0; attempt < 3; attempt++) {
    r = await freeLane().call(path, body, onProgress, !!(opts && opts.community));
    if (!r || !r.worker_dead) return r;
    const i = pool.findIndex((l) => l && l.dead);
    if (i >= 0) pool[i] = null;
  }
  return r;
}

