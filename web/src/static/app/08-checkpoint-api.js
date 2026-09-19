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
// there is least compute. N workers walk DISJOINT STRIDES of the shuffled
// index range (`shard`, `shard + shards`, …), which is a partition — nothing
// is evaluated twice and nothing is missed (`shards_partition_the_shuffled_
// order_exactly`). Each also climbs on its own, so N workers are also N
// independent hill-climbs, which is the diversity one best-first climb lacks.
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
// Deduplicate first: strides are disjoint but the CLIMB is not, so two workers
// can reach the same build from different samples. Counting it twice would
// push a real alternative off the board.
function woptMerge(parts) {
  const bad = parts.find((p) => p && p.ok === false);
  if (bad) return bad;
  // A shard that owned no ground returns an empty but complete envelope, so
  // any part is a valid head — prefer one that actually ranked something.
  const rows = [];
  const seen = new Set();
  for (const p of parts) {
    for (const r of p.results || []) {
      const key = JSON.stringify([r.mods, r.arcane, r.evolutions, r.exilus]);
      if (seen.has(key)) continue;
      seen.add(key);
      rows.push(r);
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
    // Coverage of the FLEET, not of one worker: the strides are disjoint, so
    // the positions add up.
    coverage: space > 0 ? Math.min(1, sampled / space) : 0,
    sampled,
    searched: parts.reduce((n, p) => n + (p.searched || 0), 0),
    candidates: parts.reduce((n, p) => n + (p.candidates || 0), 0),
    jobs: parts.reduce((n, p) => n + (p.jobs || 0), 0),
    cancelled: parts.some((p) => p.cancelled),
    results: rows.slice(0, finalists).map((r, i) => ({ ...r, rank: i + 1 })),
  };
}

function woptStart(body, checkpoint) {
  if (wopt && wopt.workers && wopt.workers.length) {
    return { ok: false, error: "an optimization is already running — cancel it or wait", job_id: wopt.id };
  }
  const { __resume, ...req } = body ?? {}; // the resume marker is transport, not scope
  body = req;
  // A CHECKPOINT is one worker's field, so it can only resume a run that had
  // one worker. Rather than resume a fraction of a fleet, a resume runs
  // unsharded — slower, but it is continuing a search that already exists.
  const shards = checkpoint ? 1 : woptWorkerCount();
  const job = {
    id: woptNextId++, workers: [], status: null, statuses: new Array(shards).fill(null),
    parts: new Array(shards).fill(null), result: null, board: null, boards: new Array(shards).fill(null),
    cancelled: false, shards, t0: Date.now(),
  };
  // One STATUS out of many: the fleet's progress is the sum of its workers,
  // and the phase is the least advanced of them — a run is still searching
  // while any worker still is.
  const rollup = () => {
    const live = job.statuses.filter(Boolean);
    if (!live.length) return null;
    const sum = (k) => live.reduce((n, s) => n + (Number(s[k]) || 0), 0);
    const searching = live.some((s) => s.phase === "searching") || live.length < shards;
    return {
      ...live[0],
      phase: searching ? "searching" : "running",
      sims_done: sum("sims_done"), sims_planned: sum("sims_planned"),
      enumerated: sum("enumerated"),
      round_jobs: sum("round_jobs"),
      workers: shards, workers_done: job.parts.filter(Boolean).length,
    };
  };
  for (let i = 0; i < shards; i++) {
    const w = new Worker("/worker.js");
    job.workers.push(w);
    // THE SEARCH'S WORKERS ARE WATCHED TOO. They are not lanes — they are made
    // here and run one long call — but the failure is the same: a reclaimed
    // worker neither answers nor errors, and this fleet had only `onerror`. A
    // search that can never finish and can never fail leaves the reader with a
    // progress bar that has stopped and no way to tell it from a slow one.
    const wd = watchSilence(() => gone("worker stopped answering"));
    w.onmessage = (e) => {
      wd.heard();
      if (e.data.kind === "progress") { job.statuses[i] = e.data.payload; job.status = rollup(); }
      if (e.data.kind === "board") { job.boards[i] = e.data.payload; job.board = woptMerge(job.boards.filter(Boolean)); }
      if (e.data.kind === "checkpoint") {
        const { board, ...cp } = e.data.payload;
        if (board) { job.boards[i] = board; job.board = woptMerge(job.boards.filter(Boolean)); }
        // Only an UNSHARDED run can be resumed from a checkpoint — see above.
        if (shards === 1) saveCheckpoint(body, cp, board || job.board);
      }
      if (e.data.kind === "result") {
        wd.done();
        job.parts[i] = e.data.payload;
        w.terminate();
        job.workers[i] = null;
        if (job.parts.every(Boolean)) {
          job.result = woptMerge(job.parts);
          clearCheckpoint();
          job.workers = [];
        }
      }
    };
    // ONE WAY A SHARD ENDS BADLY, however it went bad: it has an answer that
    // says so, its worker is let go, and the run completes on what it has.
    const gone = (why) => {
      wd.clear();
      if (job.parts[i]) return;
      job.parts[i] = { ok: false, error: why };
      if (job.workers[i]) { job.workers[i].terminate(); job.workers[i] = null; }
      if (job.parts.every(Boolean)) { job.result = woptMerge(job.parts); job.workers = []; }
    };
    w.onerror = (e) => gone(String((e && e.message) || "worker error"));
    w.postMessage({
      kind: "optimize",
      body: { ...body, shard: i, shards },
      checkpoint: checkpoint || null,
    });
    wd.start();
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

async function api(path, body, onProgress) {
  if (!WASM) {
    // GET endpoints (the rest are POST-with-body):
    if (path === "/api/meta" || path === "/api/i18n") return (await fetch(path)).json();
    return (await fetch(path, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body ?? {}),
    })).json();
  }
  if (path === "/api/optimize") return woptStart(body, body && body.__resume);
  if (path === "/api/optimize/status") return woptStatus();
  if (path === "/api/optimize/cancel") return woptCancel();
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
    r = await freeLane().call(path, body, onProgress);
    if (!r || !r.worker_dead) return r;
    const i = pool.findIndex((l) => l && l.dead);
    if (i >= 0) pool[i] = null;
  }
  return r;
}

