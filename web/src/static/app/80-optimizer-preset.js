// ---- The optimizer preset — ONE document per weapon
// (`wfsim-presets-<weapon>-optimizer`).
//
// What it holds is the SEARCH: the starts, the swap width and the runs per
// candidate. No scope — the candidates are the quick calc's, every one the
// weapon takes. NOT the scenario — the optimizer runs the simulator's, which
// has its own preset domain.
//
// `threads` stays out too: it is a property of the MACHINE, not of a search,
// and a preset carrying it would re-tune the CPU on every load.
const OPT_DOMAIN = "optimizer";
// Same DOCUMENT MODEL as every other bar: there is always >=1 preset, one is always active and being
// edited, edits auto-save in place, and the active survives a reload.
let activeOptPreset = null;

const loadOptPresets = () => loadPresetList(OPT_DOMAIN);
const storeOptPresets = (ps) => storePresetList(OPT_DOMAIN, ps);

// Called from renderOpt's seed block (page load AND weapon switch): the active
// preset, when there is one, replaces the default starts.
function bootstrapOptPresets() {
  // NOTHING IS AUTO-CREATED here either — see `initPresets`. A search that has
  // never been run is not a search you own, and the scope controls are already
  // a complete live state without one (`OPT_RUN_DEFAULTS` plus the default starts).
  const ps = loadOptPresets();
  const want = activeOptPreset || localStorage.getItem(presetActiveKey(OPT_DOMAIN));
  activeOptPreset = ps.some((p) => p.name === want) ? want : (ps[0] ? ps[0].name : "");
  localStorage.setItem(presetActiveKey(OPT_DOMAIN), activeOptPreset);
  const cur = ps.find((p) => p.name === activeOptPreset);
  if (cur) applyOptState(cur.state);
}

function snapshotOpt() {
  return {
    starts: JSON.parse(JSON.stringify(opt.starts)),
    swap_width: optRun.swap_width, candidate_runs: optRun.candidate_runs,
  };
}

// A new search: the four default starts, the run settings left alone.
const blankOpt = () => ({ starts: defaultStarts(),
  swap_width: optRun.swap_width, candidate_runs: optRun.candidate_runs });

// State-only apply (validation + cross-weapon dropping); no re-render.
function applyOptState(st) {
  // A preset from before the quick descent also carries a scope (`mods`,
  // `arcanes`, …); nothing reads it now, and the next save drops it.
  const w = $("weapon").value;
  optRun.swap_width = Math.max(1, Math.min(4, st.swap_width || 1));
  optRun.candidate_runs = st.candidate_runs === 1 ? 1 : 10;
  const sw = $("opt-swap-width");
  if (sw) sw.value = optRun.swap_width;
  const cr = $("opt-cand-runs");
  if (cr) cr.value = String(optRun.candidate_runs);
  // Starts: builds, each a build of THIS weapon — another weapon's start is
  // not a start here, the way another weapon's build is not a build here.
  opt.starts = (st.starts || [])
    .map((s) => normalizeStart(s, w))
    .filter((s) => s.build && (!s.build.weapon || s.build.weapon === w));
}

function applyOptPreset(st) {
  applyOptState(st);
  optSeeded = true;
  renderOpt(); updateOptEstimate();
}

function renderOptPresetBars() {
  const bar = $("preset-bar-" + OPT_DOMAIN);
  if (!bar) return;
  renderPresetBarIn(bar, optBarCfg());
}

/// The search bar's document model — what the bar and the agent door both
/// pick, start and copy.
function optBarCfg() {
  return {
    domain: OPT_DOMAIN,
    label: tr("Searches"),
    noun: "search",
    hint: "starts, swap width and runs per candidate",
    load: loadOptPresets,
    store: storeOptPresets,
    active: () => activeOptPreset,
    setActive: (n) => { activeOptPreset = n; localStorage.setItem(presetActiveKey(OPT_DOMAIN), n); },
    snapshot: snapshotOpt,
    apply: (st) => applyOptPreset(st || {}),
    blank: blankOpt,
    isBlank: (st) => sameState(st, blankOpt()),
    rerender: renderOptPresetBars,
  };
}

/// The search's run settings, from the run bar or the agent door.
function setOptSizes({ swap_width, candidate_runs }) {
  if (swap_width != null) optRun.swap_width = Math.max(1, Math.min(4, swap_width));
  if (candidate_runs != null) optRun.candidate_runs = candidate_runs === 1 ? 1 : 10;
  const sw = $("opt-swap-width");
  if (sw) sw.value = optRun.swap_width;
  const cr = $("opt-cand-runs");
  if (cr) cr.value = String(optRun.candidate_runs);
  updateOptEstimate();
}

/// WHAT A RUN WILL DO. How many fights a descent takes is not known until it
/// settles, so this states the inputs that decide it rather than a number.
function updateOptEstimate() {
  const n = Math.max(1, opt.starts.length);
  const en = allEnemies().find((e) => e.id === sim.enemy) || {};
  $("opt-estimate").innerHTML = escHtml(tr("{n} starts · {r} runs a candidate · final round {f} runs")
    .replace("{n}", n).replace("{r}", optRun.candidate_runs).replace("{f}", finalRuns().toLocaleString()))
    + ` · ${escHtml(tr("vs"))} <b>${escHtml(en.name || sim.enemy)}</b> Lv ${sim.level}${sim.steel_path ? " (SP)" : ""} · ${sim.duration} s`;
  // Never re-enable while a background job is still running.
  $("run-opt").disabled = optJobId != null;
  // Every search mutation funnels through here — AUTO-SAVE into the active
  // preset (debounced), same contract as the build bar.
  optSaveTimer = deferSave("search", () => {
    if (presetApplying) return;
    const ps = loadOptPresets();
    // A SEARCH IS BORN ON THE FIRST EDIT, like a build (`markPresetDirty`).
    // This funnel also runs on every render, so the evidence of an edit is the
    // STATE: a search that is still `blankOpt()` is a search nobody has made.
    if (!activeOptPreset) {
      if (sameState(snapshotOpt(), blankOpt())) return;
      const name = newPresetName(ps);
      ps.push({ name, savedAt: Date.now(), state: snapshotOpt() });
      storeOptPresets(ps);
      activeOptPreset = name;
      localStorage.setItem(presetActiveKey(OPT_DOMAIN), name);
      renderOptPresetBars();
      return;
    }
    const at = ps.findIndex((p) => p.name === activeOptPreset);
    if (at < 0) return;
    if (deleteIfBlank(optBarCfg(), ps[at].state)) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: snapshotOpt() };
    storeOptPresets(ps);
    renderOptPresetBars();
  }, 400);
}
let optSaveTimer = null;

// The optimize run is a BACKGROUND JOB on the server: POST /api/optimize
// returns a job_id immediately; we poll /api/optimize/status for live funnel
// progress (overall % is exact — the schedule fixes every round's sim count
// up front) and can /api/optimize/cancel. On page reload, init() reattaches
// to a still-running job via a no-id status call.
let optJobId = null;
let optPollTimer = null;
let optCancelling = false; // survives the poll's 500 ms re-renders
let optLastStatus = null; // the running job's last poll, which the door reads

const postJson = (url, body) => api(url, body);

async function runOptimize() {
  clearCheckpoint(); // a fresh run supersedes any interrupted one
  optLastStatus = null;
  $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
  $("opt-results").innerHTML = `<div class="placeholder">starting…</div>`;
  try {
    // NO SCOPE: the server searches every candidate the quick calc offers.
    const starts = startsPayload();
    const body = {
      weapon: $("weapon").value,
      rivens: rivenPayload(),
      // The quick calc's every-rank list: those cards are candidates at each rank.
      every_rank: everyRank(),
      mode,
      valence_element: valence.element,
      valence_bonus: valence.bonus,
      ...(assembly ? { assembly: { ...assembly } } : {}),
      // THE STANCE, PINNED TO THE BUILDER'S — a stance decides what a swing
      // IS, and no position of the descent ranges over it.
      stance: slotModId(slots[STANCE]) || "",
      // THE FIGHT, WHOLE AND DERIVED — `theFight()`, the call the simulator
      // makes, so a candidate is scored under the fight the replay runs.
      ...theFight(),
      // One answer per start at most, each re-measured at the final runs.
      final_runs: finalRuns(), finalists: starts.length,
      strategy: "quick",
      starts, swap_width: optRun.swap_width, candidate_runs: optRun.candidate_runs,
    };
    const r = await postJson("/api/optimize", body);
    if (!r || r.ok === false) {
      optFinish(`<div class="error">optimize failed: ${r ? r.error : "no data"}</div>`);
      return;
    }
    optJobId = r.job_id;
    pollOptimize();
  } catch (e) {
    optFinish(`<div class="error">optimize failed: ${e}</div>`);
  }
}

function optFinish(html) {
  if (optPollTimer) { clearTimeout(optPollTimer); optPollTimer = null; }
  optJobId = null;
  optCancelling = false;
  if (html !== undefined) $("opt-results").innerHTML = html;
  $("run-opt").textContent = "Run Optimizer";
  updateOptEstimate(); // re-enables the button when the scope is valid
}

async function pollOptimize() {
  let st;
  try {
    st = await postJson("/api/optimize/status", optJobId != null ? { id: optJobId } : {});
  } catch (e) {
    optFinish(`<div class="error">optimize status failed: ${e}</div>`);
    return;
  }
  if (!st || st.ok === false) {
    optFinish(`<div class="error">optimize failed: ${st ? st.error : "no data"}</div>`);
    return;
  }
  optJobId = st.job_id;
  optLastStatus = st;
  if (st.phase === "error") {
    optFinish(`<div class="error">optimize failed: ${(st.result && st.result.error) || "unknown error"}</div>`);
    return;
  }
  if (st.phase === "done" || st.phase === "cancelled") {
    optFinish();
    if (st.result && st.result.results && st.result.results.length) {
      renderOptResults(st.result);
      // …AND EVERY FINALIST GOES TO THE BOARD. After the results are drawn, so
      // a slow door never delays the answer the reader asked for.
      offerOptBoardSubmit(st.result);
      // A cancel is not necessarily the end of the search — the run stopped,
      // but its resume point is still on disk. Offer it under the results.
      if (st.phase === "cancelled") appendResumeOffer();
      offerSupportOnce($("opt-results"));
    } else {
      $("opt-results").innerHTML = `<div class="placeholder">cancelled before anything had been ranked — no results</div>`;
    }
    return;
  }
  renderOptProgress(st);
  optPollTimer = setTimeout(pollOptimize, 500);
}

function renderOptProgress(st) {
  const pct = st.sims_planned ? Math.min(100, (100 * st.sims_done) / st.sims_planned) : 0;
  const head = st.phase === "enumerating"
    ? `enumerating candidates…${st.enumerated ? ` ${st.enumerated.toLocaleString()} so far` : ""}${st.sims_done ? ` · ${st.sims_done.toLocaleString()} screened` : ""}`
    : `round ${st.round}/${st.rounds} — ${(st.round_jobs || 0).toLocaleString()} jobs × ${st.round_runs} runs`;
  const notes = (st.notes || []).map((n) =>
    `<div class="opt-note">round ${n.round}: ${n.jobs.toLocaleString()} × ${n.runs} (${n.by_kills ? "kills" : "dmg"}) → keep ${n.kept.toLocaleString()} · best ${n.by_kills ? sig2(kpm(n.best, sim.duration)) + " KPM" : n.best.toExponential(2) + " dmg"} · ${(n.ms / 1000).toFixed(1)}s</div>`
  ).join("");
  const sub = st.phase === "enumerating"
    ? ""
    : `<div class="opt-prog-sub">${pct.toFixed(1)}% · ${st.sims_done.toLocaleString()} / ${st.sims_planned.toLocaleString()} sims${st.jobs ? ` · ${st.jobs.toLocaleString()} candidate builds` : ""}</div>`;
  $("opt-results").innerHTML = `<div class="opt-progress">
    <div class="opt-prog-head"><span>${head}</span><span class="opt-elapsed">${st.elapsed_s.toFixed(0)}s</span></div>
    <div class="opt-bar"><i style="width:${pct}%"></i></div>
    ${sub}${notes}
    <button class="ghost-btn small" id="opt-cancel" ${optCancelling ? "disabled" : ""}>${optCancelling ? "Cancelling…" : "Cancel"}</button>
  </div>`;
  // The 500 ms poll re-renders this whole block — `optCancelling` keeps the
  // button's cancelling state alive across re-renders, or it snaps back to a
  // live-looking "Cancel" and cancellation looks ignored.
  $("opt-cancel").addEventListener("click", cancelOptimize);
}

/// STOPPING A SEARCH, from its button or the agent door. The poll reports the
/// outcome; a stopped search keeps what it had ranked.
async function cancelOptimize() {
  optCancelling = true;
  const b = $("opt-cancel");
  if (b) { b.disabled = true; b.textContent = "Cancelling…"; }
  try { await postJson("/api/optimize/cancel", { id: optJobId }); } catch (e) { /* poll reports */ }
}

// Reattach to a job that is still running server-side (e.g. after a page
// reload): a no-id status call returns the latest job.
async function reattachOptimize() {
  try {
    const st = await postJson("/api/optimize/status", {});
    if (st && st.ok !== false && (st.phase === "enumerating" || st.phase === "running")) {
      optJobId = st.job_id;
      $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
      renderOptProgress(st);
      optPollTimer = setTimeout(pollOptimize, 500);
      return;
    }
  } catch (e) { /* no server-side job — nothing to reattach */ }
  offerResume(); // nothing is running: a reload may have killed a wasm run
}

// The run itself is gone, but the field it had narrowed to is not. Offer to
// continue from the last completed round instead of paying for it again.
// Never auto-start: resuming costs minutes of the visitor's CPU, so it takes a
// click — and the offer only appears for the weapon the checkpoint belongs to.
function offerResume() {
  const el = resumeControl();
  if (!el) return;
  const box = $("opt-results");
  box.innerHTML = "";
  box.append(el);
}

// The same control, under a cancelled run's leaderboard.
function appendResumeOffer() {
  const el = resumeControl();
  if (el) $("opt-results").append(el);
}

function resumeControl() {
  const saved = loadCheckpoint();
  const box = $("opt-results");
  if (!saved || !box || optJobId != null) return null;
  if (saved.body.weapon !== $("weapon").value) return null;
  const cp = saved.cp;
  const el = document.createElement("div");
  el.className = "opt-resume";
  const sel = $("weapon");
  const shown = (sel.selectedOptions[0] || {}).textContent || sel.value;
  const where = cp.kind === "screen"
    ? `while screening — ${cp.start_seq.toLocaleString()} candidates walked, `
      + `${(cp.keepers.length / 2).toLocaleString()} jobs still standing`
    : `after round ${cp.round} — ${cp.alive.length.toLocaleString()} builds still standing`;
  el.innerHTML = `<div>An optimization for <b>${escHtml(shown)}</b> stopped ${where}.</div>`;
  const go = document.createElement("button");
  go.className = "ghost-btn"; go.textContent = "resume it";
  go.onclick = () => resumeOptimize(saved);
  const no = document.createElement("button");
  no.className = "ghost-btn small"; no.textContent = "discard";
  no.onclick = () => { clearCheckpoint(); el.remove(); };
  el.append(go, no);
  return el;
}

async function resumeOptimize(saved) {
  $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
  $("opt-results").innerHTML = `<div class="placeholder">${saved.cp.kind === "screen"
    ? `re-walking to the saved point (${saved.cp.start_seq.toLocaleString()} candidates)…`
    : `resuming from round ${saved.cp.round}…`}</div>`;
  try {
    // The STORED body, not the current form: the checkpoint describes a field
    // narrowed under that exact scope, and re-deriving the body from the UI
    // would let an edited setting resume into a run it never belonged to.
    const r = await postJson("/api/optimize", { ...saved.body, __resume: saved.cp });
    if (!r || r.ok === false) {
      clearCheckpoint();
      optFinish(`<div class="error">resume failed: ${r ? r.error : "no data"}</div>`);
      return;
    }
    optJobId = r.job_id;
    pollOptimize();
  } catch (e) {
    optFinish(`<div class="error">resume failed: ${e}</div>`);
  }
}

const prettify = (id) => id.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
const arcName = (id) => (id === "none" ? "no arcane" : ((META.arcanes || []).find((a) => a.id === id) || {}).name || prettify(id));
const evoName = (id) => {
  for (const t of weaponEvos()) { const o = t.options.find((o) => o.id === id); if (o) return o.name; }
  return prettify(id);
};

/// The ranking on screen, which the agent door reads. Cleared with the results
/// themselves when the weapon changes.
let optLast = null;

/// ③ THE RESULTS: one row per answer, which is one start's — or several
/// starts' that settled on the same build. Each row names its starts, what each
/// scored before and how many changes it took, and a start that reached no
/// legal build is said so rather than left out.
function renderOptResults(r) {
  optLast = r;
  const w = weaponInfo($("weapon").value) || {};
  const d = r.duration;
  const rows = r.results || [];
  const lead = rows[0];
  // TIED WITH THE LEADER when the two differ by less than twice their
  // combined standard error.
  const tied = (res) => lead && res !== lead && Math.abs((lead.kill_progress ?? 0) - (res.kill_progress ?? 0))
    < 2 * Math.hypot(lead.kill_progress_se || 0, res.kill_progress_se || 0);
  const lanes = (res) => (res.from_starts || []).map((l) => `<span class="opt-lane">${escHtml(tr("start"))} ${l.start + 1}: ${
    l.from == null ? "—" : sig2(kpm(l.from, d))} → ${sig2(kpm(res.kill_progress ?? res.kills, d))} KPM · ${
    escHtml(tr("{n} changes").replace("{n}", l.moves))}</span>`).join("");
  const html = rows.map((res) => `<div class="opt-row">
      <div class="opt-head">
        <span class="opt-rank">#${res.rank}</span>
        <span class="opt-kills" id="opt-kpm-${res.rank}" data-search="${kpm(res.kill_progress ?? res.kills, d)}">${
          sig2(kpm(res.kill_progress ?? res.kills, d))}<small> KPM</small><span class="opt-repro pending" title="${
          escHtml(tr("re-measuring this build in the simulator"))}">·</span></span>
        <span class="opt-dps">± ${sig2(kpm(res.kill_progress_se || 0, d))}</span>
        ${tied(res) ? `<span class="opt-tie">${escHtml(tr("tied"))}</span>` : ""}
        <span class="forma-badge legal">${res.forma.used} Forma</span>
        <span style="flex-grow:1"></span>
        <button class="ghost-btn small opt-add" title="${escHtml(tr("save as a new build"))}" data-r='${JSON.stringify(res).replace(/'/g, "&#39;")}'>+ add</button>
        <button class="ghost-btn small opt-restart" data-rank="${res.rank}">${escHtml(tr("use as a new start"))}</button>
      </div>
      <div class="opt-lanes">${escHtml(tr("from"))} ${lanes(res)}</div>
      <div class="opt-card" data-rank="${res.rank}"></div>
    </div>`).join("");
  const failed = (r.failed_starts || []).map((f) => `<div class="opt-row opt-failed"><b>${escHtml(tr("start"))} ${f.start + 1}</b> — ${
    escHtml(tr("no legal build from this start"))} <span class="sim-hint">${escHtml(f.why)}</span></div>`).join("");
  const meta = `<span class="${r.cut ? "warn" : "ok"}">${escHtml(tr(r.cut
    ? "the time budget ran out before every start settled — strong builds, short of what those starts reach"
    : "each start improved until no change helped — the best those starts reach, not a proven best; add a start or raise the swap width to look further"))}</span>`
    + `${r.cancelled ? ` · <span class="warn">${escHtml(tr("cancelled — best so far"))}</span>` : ""}`
    + ` · vs ${escHtml(r.target.name)} Lv ${r.target.level}${r.target.steel_path ? " (SP)" : ""} · ${d ?? "?"} s · ${(r.final_runs || 0).toLocaleString()} ${escHtml(tr("runs each"))}`;
  $("opt-results").innerHTML = `<h4 class="sim-h">③ ${escHtml(tr("Results"))}</h4><div class="opt-board" id="opt-board"></div><div class="opt-meta">${meta}</div>${html}${failed}`;
  const byRank = new Map(rows.map((res) => [res.rank, res]));
  $("opt-results").querySelectorAll(".opt-add").forEach((el) =>
    el.addEventListener("click", () => addResult(JSON.parse(el.dataset.r), el)));
  $("opt-results").querySelectorAll(".opt-restart").forEach((el) => el.addEventListener("click", async () => {
    addStart(await resultToState(byRank.get(Number(el.dataset.rank))));
    el.textContent = "✓ " + tr("start") + " " + opt.starts.length; el.disabled = true;
  }));
  // THE SIMULATOR'S CARD, once each row's build is planned.
  $("opt-results").querySelectorAll(".opt-card").forEach(async (el) => {
    try { el.innerHTML = cardOfState(await resultToState(byRank.get(Number(el.dataset.rank))), w); } catch (_) { /* the row still stands */ }
  });
  verifyOptRows(r);
}

/// THE NUMBER ON A ROW IS THE SIMULATOR'S.
///
/// The hard rule made operational on the PAGE. "The simulator is the truth"
/// covers the ENGINE, where `parse_fight` sees to it; a page with its own
/// translation of a ranked row into a build ranks by one thing while the
/// builder fires another, measured at 26 KPM against 15.
///
/// So each row is re-run through `/api/simulate` — with the request the SERVER
/// wrote for that candidate — and the KPM on screen is what came back. The
/// search's own figure keeps one job, ORDERING the list, since re-measuring
/// cannot reorder a ranking without making it meaningless.
///
/// The two are compared, and both sides report their own standard error, so
/// "they disagree" is arithmetic rather than a tolerance somebody picked: 4
/// sigma of the two combined. Any axis lost anywhere on the chain moves the
/// number and trips it, which is why this checks the ANSWER instead of counting
/// fields — it cannot go stale when an axis is added.
///
/// Top-down and sequential: the leader is what a reader looks at first, and
/// twenty engagements at the final round's precision is real time.
let optVerifyToken = 0;
async function verifyOptRows(r) {
  const token = ++optVerifyToken;
  const rows = (r.results || []).slice();
  for (const res of rows) {
    if (token !== optVerifyToken) return;          // a newer ranking owns the panel
    const el = $(`opt-kpm-${res.rank}`);
    if (!el) continue;
    const mark = el.querySelector(".opt-repro");
    if (!res.replay) {
      if (mark) { mark.className = "opt-repro stale"; mark.textContent = ""; mark.title = tr("this ranking predates the simulator re-run"); }
      continue;
    }
    // …ALSO THROUGH THE FLEET. Every ranked row is re-simulated, so on a crowd
    // ruler this is the finalist count TIMES a full simulation — the one place
    // on the page where the fleet is worth the most.
    let s = null;
    try { s = await simulateFleet(res.replay); } catch (_) { s = null; }
    if (token !== optVerifyToken) return;
    if (!s || s.ok === false || s.score == null) {
      if (mark) { mark.className = "opt-repro failed"; mark.textContent = "!"; mark.title = tr("the simulator refused this build — see the build's own card"); }
      continue;
    }
    const shown = kpm(s.score, r.duration);
    const search = Number(el.dataset.search) || 0;
    // FOUR SIGMA OF THE TWO COMBINED. Both are means of independent runs, so
    // their difference has the two standard errors added in quadrature — there
    // is no systematic gap to allow for, and any tolerance written as a flat
    // percentage would be too tight at 40 runs and too loose at 1000.
    const se = Math.hypot(kpm(s.score_se || 0, r.duration),
                          kpm(res.kill_progress_se || 0, r.duration));
    const off = Math.abs(shown - search) > Math.max(4 * se, 0.01 * Math.abs(search));
    el.firstChild.nodeValue = sig2(shown);
    if (mark) {
      mark.className = "opt-repro " + (off ? "off" : "ok");
      mark.textContent = off ? "≠" : "✓";
      mark.title = off
        ? tr("the simulator does not reproduce the search's own score for this build — the build shown may not be the one that was scored")
          + ` (${sig2(search)} → ${sig2(shown)} KPM)`
        : tr("re-run in the simulator: this is the simulator's own number for this build");
    }
    if (off) el.closest(".opt-row").classList.add("opt-unreproduced");
  }
}

// An optimizer result as a builder-builds preset STATE (snapshotState
// shape) — built without touching the build being edited.
async function resultToState(res) {
  // THE ROW'S OWN REQUEST, not a build re-derived from a description of one.
  //
  // `replay` is a complete simulate request written by the server out of the
  // very candidate it scored, so applying a result reads exactly ONE field and
  // there is nothing here to keep in step with the search. Every axis the row
  // was measured under is in it, including the ones nobody has invented yet.
  //
  // A row from a run predating `replay` still has to open, so the named fields
  // remain the fallback — and they are a translation, with everything that
  // implies: `mode` and `valence` fall back to the PAGE, which is the mode and
  // the element that run was launched in.
  const payload = res.replay || {
    mods: (res.mods || []).concat(
      res.exilus && res.exilus !== "none" ? [res.exilus] : [],
    ),
    evolutions: res.evolutions || [],
    arcane: res.arcane,
    arcane_rank: res.arcane_rank,
    mode: res.mode || mode,
    valence_element: res.valence || valence.element,
    valence_bonus: valence.bonus,
  };
  const st = stateFromBuild(payload, $("weapon").value, res.exilus);
  // …and the POLARITIES, which are the page's alone: a payload states the
  // build, and the cheapest layout that fits it is a plan the builder makes.
  st.slots = await planSlotsAlone(st.slots.map((s, i) => ({ mod: s.mod, pol: innate[i], rank: s.rank })));
  // NO `sim`. Copying the optimizer's own buff config into the scenario so
  // that "add then Run Sim" matches its score makes a result rewrite the fight
  // you are working in; a result is a BUILD. The two configs can still
  // disagree — the search's is scope-wide, the scenario's is this build's —
  // and that disagreement is now visible instead of resolved by silently
  // editing a preset the user owns.
  return st;
}

// "+ add" (not load): the result becomes a NEW preset
// appended after the existing builds; the build being edited is never
// clobbered. Auto-named "opt N" (rename in the preset bar if it earns a
// real name).
async function addResult(res, btn) {
  const state = await resultToState(res);
  const ps = loadPresetList(BUILDS);
  let n = 1;
  while (ps.some((p) => p.name === "opt " + n)) n++;
  const name = "opt " + n;
  ps.push({ name, savedAt: Date.now(), state });
  storePresetList(BUILDS, ps);
  renderPresetBar(); // the builder's bar shows the new chip when you switch back
  if (btn) { btn.textContent = "✓ " + name; btn.disabled = true; }
  return name;
}

/// RECLAIM WHAT THE OLD RULE LEFT BEHIND, once, on the way in.
///
/// `stripReplays` stops the growth; it does not undo it. Every replay written
/// before 2026-08-18 is still on the reader's disk, in a key for a weapon they
/// may never open again — and a browser that is already at its quota fails the
/// NEXT write, not the one that filled it, so without this the fix would only
/// arrive for whoever cleared their own storage first.
///
/// It rewrites each `wfsim-presets-*` key in place and only when the stripped
/// copy is actually smaller, so a reader with nothing to reclaim pays one parse
/// per key and writes nothing.
function reclaimStoredReplays() {
  let freed = 0;
  for (const k of Object.keys(localStorage)) {
    if (!k.startsWith("wfsim-presets-")) continue;
    const raw = localStorage.getItem(k) || "";
    // The cheap test first: a list with no replay in it is not worth parsing.
    if (!raw.includes('"replay"')) continue;
    let list = null;
    try { list = JSON.parse(raw); } catch (_) { continue; }
    if (!Array.isArray(list)) continue;
    const next = JSON.stringify(stripReplays(list));
    if (next.length >= raw.length) continue;
    try {
      localStorage.setItem(k, next);
      freed += raw.length - next.length;
    } catch (_) {
      // Full enough that even the SMALLER copy will not go in. Removing the key
      // is still better than leaving it: the collection re-creates itself.
      try { localStorage.removeItem(k); freed += raw.length; } catch (_) { /* nothing left to try */ }
    }
  }
  return freed;
}
try { reclaimStoredReplays(); } catch (_) { /* storage may be unavailable entirely */ }

