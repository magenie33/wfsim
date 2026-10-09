// ---- The optimizer preset — ONE document per weapon
// (`wfsim-presets-<weapon>-optimizer`).
//
// What it holds is the SEARCH: the starts and the runs per candidate. No scope — the candidates are the quick calc's, every one the
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

// ---- the built-in searches -------------------------------------------------
//
// `data/search/presets.yaml`, served as `META.search_presets`: read-only like the
// official rulers in the scenario bar — never stored, never edited; ⧉ copies one
// into a search of your own. Owning none, the search is the first one listed.

/// A built-in search's starts on weapon `wId`: one per element row, holding the
/// row's 60/60 card the weapon equips — and, for a riven preset, `rivenMod`
/// beside it, pinned. A row the weapon takes none of adds nothing new.
function builtinSearchStarts(wId, rivenMod) {
  const fits = new Set(buildPool().map((m) => m.id));
  const rows = ((META.search_presets || {}).element_starts || [])
    .map((row) => [rivenMod, row.find((c) => fits.has(c))].filter(Boolean)).filter((mods) => mods.length);
  const unique = [...new Set(rows.map((r) => JSON.stringify(r)))].map((r) => JSON.parse(r));
  const starts = unique.map((mods) => ({ build: stateFromBuild({ mods }, wId), fixed: rivenMod ? ["mods:0"] : [] }));
  return starts.length ? starts : [blankStart()];
}
/// A built-in search as a whole search state, on the open weapon.
const builtinSearchState = (p, rivenMod) => ({ starts: builtinSearchStarts($("weapon").value, rivenMod),
  limits: normalizeLimits(null), candidate_runs: p.candidate_runs, finalists: p.finalists });
/// The search bar's built-in entries: the listed ones, and the riven appraisal's
/// own while an appraisal is open on this page (81-appraisal.js).
const builtinSearches = () => ((META.search_presets || {}).presets || [])
  .filter((p) => p.listed || (p.riven && appraisalActive() && appraisal.rivenMod))
  .map((p) => ({ name: tr(p.name), builtin: "search:" + p.id, savedAt: 0,
    state: builtinSearchState(p, p.riven ? appraisal.rivenMod : null) }));
const builtinSearchActive = () => String(activeOptPreset || "").startsWith("search:");

/// A BUILT-IN SEARCH, ON SCREEN: the starts, the limits and the run terms go
/// inert and a note says why and offers the copy. Only the visible half — a
/// built-in is never stored, so the auto-save has nothing to write it into.
/// A box marked `data-view-only` narrows what is SHOWN and edits nothing, so it
/// stays live: locked, a reader could not even look for what a built-in leaves out.
function lockBuiltinSearch() {
  const on = builtinSearchActive();
  ["opt-starts", "opt-limits", "opt-finalists", "opt-cand-runs"].map((id) => $(id)).filter(Boolean).forEach((b) => {
    b.classList.toggle("locked", on);
    const els = (b.matches("input,select,button,textarea") ? [b] : [...b.querySelectorAll("input,select,button,textarea")])
      .filter((el) => !el.hasAttribute("data-view-only"));
    els.forEach((el) => {
      if (on) {
        if (!el.disabled) { el.disabled = true; el.dataset.builtinLock = "1"; }
      } else if (el.dataset.builtinLock) {
        el.disabled = false;
        delete el.dataset.builtinLock;
      }
    });
  });
  const note = $("opt-builtin");
  if (!note) return;
  note.hidden = !on;
  if (on) {
    note.innerHTML = `<b>${escHtml(tr("Built-in search"))}</b> — ${escHtml(tr("the same on every weapon. It cannot be edited."))}`
      + ` <button class="ghost-btn small" id="opt-builtin-copy">⧉ ${escHtml(tr("edit a copy of this search"))}</button>`;
    $("opt-builtin-copy").onclick = () => copyActivePreset(optBarCfg());
  }
}

// Called from renderOpt's seed block (page load AND weapon switch): the active
// preset — your own, else the first built-in — replaces the blank start.
function bootstrapOptPresets() {
  // NOTHING IS AUTO-CREATED here either — see `initPresets`. A search that has
  // never been run is not a search you own, and the scope controls are already
  // a complete live state without one (`OPT_RUN_DEFAULTS` plus one blank start).
  const ps = loadOptPresets();
  const want = activeOptPreset || localStorage.getItem(presetActiveKey(OPT_DOMAIN));
  const cur = presetFind(ps.concat(builtinSearches()), want) || ps[0] || builtinSearches()[0] || null;
  activeOptPreset = cur ? presetId(cur) : "";
  localStorage.setItem(presetActiveKey(OPT_DOMAIN), activeOptPreset);
  if (cur) applyOptState(cur.state);
}

function snapshotOpt() {
  return {
    starts: JSON.parse(JSON.stringify(opt.starts)),
    limits: JSON.parse(JSON.stringify(opt.limits || normalizeLimits(null))),
    candidate_runs: optRun.candidate_runs,
    finalists: optRun.finalists,
  };
}

// A new search: one blank start, the run settings left alone.
const blankOpt = () => ({ starts: [blankStart()], limits: normalizeLimits(null),
  candidate_runs: optRun.candidate_runs, finalists: optRun.finalists });

/// How many builds a search answers with: 1–100, the server's own clamp.
const finalistsOf = (n) => {
  const v = Math.round(Number(n));
  return Number.isFinite(v) && v >= 1 ? Math.min(100, v) : OPT_RUN_DEFAULTS.finalists;
};

// State-only apply (validation + cross-weapon dropping); no re-render.
function applyOptState(st) {
  // A preset from before the quick descent also carries a scope (`mods`,
  // `arcanes`, …); nothing reads it now, and the next save drops it.
  const w = $("weapon").value;
  optRun.candidate_runs = st.candidate_runs === 1 ? 1 : 10;
  optRun.finalists = finalistsOf(st.finalists);
  const cr = $("opt-cand-runs");
  if (cr) cr.value = String(optRun.candidate_runs);
  const fin = $("opt-finalists");
  if (fin) fin.value = String(optRun.finalists);
  // Limits name THIS weapon's options; another weapon's do not apply here. A
  // search saved with only an excluded-mods list carries it into the limits.
  opt.limits = normalizeLimits(st.limits || (st.exclude ? { exclude: { mods: st.exclude } } : null));
  // Starts: builds, each a build of THIS weapon — another weapon's start is
  // not a start here, the way another weapon's build is not a build here.
  opt.starts = (st.starts || [])
    .map((s) => normalizeStart(s, w))
    .filter((s) => s.build && (!s.build.weapon || s.build.weapon === w));
  if (!opt.starts.length) opt.starts = [blankStart()];
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
  const cfg = {
    domain: OPT_DOMAIN,
    label: tr("Searches"),
    noun: "search",
    hint: "starts, limits, results and runs per candidate",
    load: loadOptPresets,
    store: storeOptPresets,
    active: () => activeOptPreset,
    setActive: (n) => { activeOptPreset = n; localStorage.setItem(presetActiveKey(OPT_DOMAIN), n); },
    snapshot: snapshotOpt,
    apply: (st) => applyOptPreset(st || {}),
    blank: blankOpt,
    isBlank: (st) => sameState(st, blankOpt()),
    // THE BUILT-IN SEARCHES, read-only, as the scenario bar's rulers are.
    published: builtinSearches,
    pins: { key: () => "wfsim-opened-searches", ref: (p) => ({ id: p.builtin }), same: (a, b) => a.id === b.id },
    roGroup: tr("Built-in searches · read-only"),
    openable: builtinSearches,
    openLabel: tr("built-in search"),
    openHint: tr("searches every reader has, the same on every weapon"),
    readonly: (p) => !!p.builtin,
    roTitle: () => tr("a built-in search — copy it to edit"),
    unpin: (id) => unpinPublished(cfg, id),
    // Owning nothing, the search is the first built-in: there is no blank to show.
    fallback: () => builtinSearches()[0] || null,
    pinned: true,
    // …and a switch to or from one relocks the controls.
    rerender: () => { renderOptPresetBars(); lockBuiltinSearch(); },
  };
  return cfg;
}

/// The search's run settings, from the run bar or the agent door.
function setOptSizes({ candidate_runs, finalists }) {
  if (candidate_runs != null) optRun.candidate_runs = candidate_runs === 1 ? 1 : 10;
  if (finalists != null) optRun.finalists = finalistsOf(finalists);
  const cr = $("opt-cand-runs");
  if (cr) cr.value = String(optRun.candidate_runs);
  const fin = $("opt-finalists");
  if (fin) fin.value = String(optRun.finalists);
  updateOptEstimate();
}

/// WHAT A RUN WILL DO. How many fights a descent takes is not known until it
/// settles, so this states the inputs that decide it rather than a number.
function updateOptEstimate() {
  const n = Math.max(1, opt.starts.length);
  // The run bar shows what will run: a weapon change resets `optRun` and
  // redraws nothing else.
  if ($("opt-finalists")) $("opt-finalists").value = String(optRun.finalists);
  if ($("opt-cand-runs")) $("opt-cand-runs").value = String(optRun.candidate_runs);
  const en = allEnemies().find((e) => e.id === sim.enemy) || {};
  $("opt-estimate").innerHTML = escHtml(tr("{n} starts · best {k} · {r} runs a candidate · final round {f} runs")
    .replace("{n}", n).replace("{k}", optRun.finalists).replace("{r}", optRun.candidate_runs).replace("{f}", simRuns().toLocaleString()))
    + ` · ${escHtml(tr("vs"))} <b>${escHtml(en.name || sim.enemy)}</b> Lv ${sim.level}${sim.steel_path ? " (SP)" : ""} · ${sim.duration} s`;
  // A START THAT PINS WHAT A LIMIT RULES OUT has no answer to give: the run
  // waits until one side changes, and says so here.
  const blocked = opt.limits && startsBlocked();
  if (blocked) {
    $("opt-estimate").innerHTML += ` · <span class="warn">${escHtml(tr("a start pins what a limit rules out — change the start or the limit"))}</span>`;
  }
  // Never re-enable while a background job is still running.
  $("run-opt").disabled = optJobId != null || !!blocked;
  lockBuiltinSearch();
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
      const e = presetEntry(newPresetName(ps), snapshotOpt());
      ps.push(e);
      storeOptPresets(ps);
      activeOptPreset = e.id;
      localStorage.setItem(presetActiveKey(OPT_DOMAIN), e.id);
      renderOptPresetBars();
      return;
    }
    const at = ps.findIndex((p) => presetId(p) === activeOptPreset);
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
/// WHEN THE POLL LAST HEARD FROM THE JOB. `optJobId` is cleared by `optFinish`
/// alone, so an exit that skips it latches it; the community's work asks
/// `optIsLive` instead, which expires (69-board-work.js `ownTabBusy`).
let optHeardAt = 0;
const OPT_STALE_MS = 15000;
const optIsLive = () => optJobId !== null && Date.now() - optHeardAt < OPT_STALE_MS;
let optPollTimer = null;
let optCancelling = false; // survives the poll's 500 ms re-renders
let optLastStatus = null; // the running job's last poll, which the door reads

const postJson = (url, body) => api(url, body);

/// THE SEARCH THE PAGE WOULD RUN, as the request it sends — read off the builder
/// and the optimizer as they stand. A riven gain freezes it once, on a page that
/// holds nothing of anyone's (81-appraisal.js), so every computer searches the
/// same question.
function optimizeBody() {
  // NO SCOPE: the server searches every candidate the quick calc offers.
  const starts = startsPayload();
  return {
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
    // The best N of every whole build the starts' sweeps scored, each
    // re-measured at the final runs.
    finalists: optRun.finalists,
    strategy: "quick",
    starts, candidate_runs: optRun.candidate_runs,
    // What the player ruled out; the rest of the builder's lists is the scope.
    limits: opt.limits,
  };
}

async function runOptimize() {
  clearCheckpoint(); // a fresh run supersedes any interrupted one
  optLastStatus = null;
  $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
  $("opt-results").innerHTML = `<div class="placeholder">starting…</div>`;
  try {
    const body = optimizeBody();
    // STARTED, beside `optimizer.run` for finished: the gap is what was cancelled or failed.
    track("optimizer.start", $("weapon").value);
    const r = await postJson("/api/optimize", body);
    if (!r || r.ok === false) {
      optFinish(`<div class="error">optimize failed: ${r ? r.error : "no data"}</div>`);
      return;
    }
    optJobId = r.job_id; optHeardAt = Date.now();
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
  optJobId = st.job_id; optHeardAt = Date.now();
  optLastStatus = st;
  if (st.phase === "error") {
    optFinish(`<div class="error">optimize failed: ${(st.result && st.result.error) || "unknown error"}</div>`);
    return;
  }
  if (st.phase === "done" || st.phase === "cancelled") {
    optFinish();
    if (st.result && st.result.results && st.result.results.length) {
      renderOptResults(st.result);
      // FINISHED, not started: a start can be cancelled, and a cancel is not a run.
      if (st.phase === "done") track("optimizer.run", $("weapon").value, st.elapsed_s);
      // …AND EVERY FINALIST GOES TO THE BOARD. After the results are drawn, so
      // a slow door never delays the answer the reader asked for.
      offerOptBoardSubmit(st.result);
      // …AND AN APPRAISAL'S WINNER back to the chat that asked (81-appraisal.js).
      if (st.phase === "done") appraisalAnswered(st.result);
      // A cancel is not necessarily the end of the search — the run stopped,
      // but its resume point is still on disk. Offer it under the results.
      if (st.phase === "cancelled") appendResumeOffer();
    } else {
      $("opt-results").innerHTML = `<div class="placeholder">cancelled before anything had been ranked — no results</div>`;
    }
    return;
  }
  renderOptProgress(st);
  optPollTimer = setTimeout(pollOptimize, 500);
}

function renderOptProgress(st) {
  // A DESCENT'S BAR COUNTS SETTLED STARTS: how many rounds one takes is found
  // by taking them, so the only total that exists is the number of starts.
  const lanes = st.starts || [];
  const pct = !st.rounds && lanes.length
    ? (100 * lanes.filter((s) => s.settled).length) / lanes.length
    : st.sims_planned ? Math.min(100, (100 * st.sims_done) / st.sims_planned) : 0;
  // THE DESCENT HAS NO PLAN TO BE A PERCENTAGE OF — how many changes a start
  // takes is found by taking them — so it reports what it has done.
  const descending = !st.rounds;
  const head = descending
    ? escHtml(tr("improving the starts — {b} builds scored, {s} fights")
      .replace("{b}", (st.enumerated || 0).toLocaleString()).replace("{s}", (st.sims_done || 0).toLocaleString()))
    : st.phase === "enumerating"
    ? `enumerating candidates…${st.enumerated ? ` ${st.enumerated.toLocaleString()} so far` : ""}${st.sims_done ? ` · ${st.sims_done.toLocaleString()} screened` : ""}`
    : `round ${st.round}/${st.rounds} — ${(st.round_jobs || 0).toLocaleString()} jobs × ${st.round_runs} runs`;
  const notes = (st.notes || []).map((n) =>
    `<div class="opt-note">round ${n.round}: ${n.jobs.toLocaleString()} × ${n.runs} (${n.by_kills ? "kills" : "dmg"}) → keep ${n.kept.toLocaleString()} · best ${n.by_kills ? sig2(kpm(n.best, sim.duration)) + " KPM" : n.best.toExponential(2) + " dmg"} · ${(n.ms / 1000).toFixed(1)}s</div>`
  ).join("");
  const lane = (s, i) => `<div class="opt-note">${escHtml(tr("start"))} ${i + 1}: ${escHtml(s.settled
    ? tr("settled")
    : s.round === 0
      ? tr("filling {k} of {n}").replace("{k}", s.at + 1).replace("{n}", s.of)
      : tr("round {r}, position {k} of {n}").replace("{r}", s.round).replace("{k}", s.at + 1).replace("{n}", s.of))}</div>`;
  const sub = descending
    ? `<div class="opt-prog-sub">${escHtml(tr("{d} of {n} starts settled").replace("{d}", lanes.filter((s) => s.settled).length).replace("{n}", lanes.length || "…"))} · ${escHtml(tr("every worker scores each batch; in the browser a search takes minutes"))}</div>${lanes.map(lane).join("")}`
    : st.phase === "enumerating"
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
      optJobId = st.job_id; optHeardAt = Date.now();
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
    // A RESUME IS A START: its finish is an `optimizer.run` like any other.
    track("optimizer.start", $("weapon").value);
    const r = await postJson("/api/optimize", { ...saved.body, __resume: saved.cp });
    if (!r || r.ok === false) {
      clearCheckpoint();
      optFinish(`<div class="error">resume failed: ${r ? r.error : "no data"}</div>`);
      return;
    }
    optJobId = r.job_id; optHeardAt = Date.now();
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

/// ③ THE RESULTS: the best N builds the search scored. A row a start settled on
/// names its starts, what each scored before and how many changes it took; any
/// other row names the answer it is nearest to and what differs from it. A
/// start that reached no legal build is said so rather than left out.
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
  const said = (axis, id) => {
    if (id == null) return tr("empty");
    if (axis === "arcane") return arcName(String(id).split("@")[0]);
    if (axis === "evolution") return evoName(id);
    if (axis === "mode") return modeLabel(w, id);
    if (axis === "valence") return DT(id);
    const [card, rank] = String(id).split("@");
    return ((modById(card) || {}).name || prettify(card)) + (rank != null ? ` R${rank}` : "");
  };
  const near = (res) => {
    const n = res.near;
    if (!n) return "";
    const whose = n.starts.map((s) => `${tr("start")} ${s + 1}`).join(", ");
    const what = (n.changes || []).map((c) => `${escHtml(said(c.axis, c.from))} → ${escHtml(said(c.axis, c.to))}`).join(" · ");
    return `<span class="opt-lane">${escHtml(tr("{s}'s answer with").replace("{s}", whose))} ${what}</span>`;
  };
  const html = rows.map((res) => `<div class="opt-row">
      <div class="opt-head">
        <span class="opt-rank">#${res.rank}</span>
        <span class="opt-kills" id="opt-kpm-${res.rank}">${
          sig2(kpm(res.kill_progress ?? res.kills, d))}<small> KPM</small>${res.simulator_error ? `<span class="opt-repro failed" title="${
          escHtml(tr("the simulator refused this build — see the build's own card"))}">!</span>` : ""}</span>
        <span class="opt-dps">± ${sig2(kpm(res.kill_progress_se || 0, d))}</span>
        ${tied(res) ? `<span class="opt-tie">${escHtml(tr("tied"))}</span>` : ""}
        <span class="forma-badge legal">${res.forma.used} Forma</span>
        <span style="flex-grow:1"></span>
        <button class="ghost-btn small opt-add" title="${escHtml(tr("save as a new build"))}" data-r='${JSON.stringify(res).replace(/'/g, "&#39;")}'>+ add</button>
        <button class="ghost-btn small opt-restart" data-rank="${res.rank}">${escHtml(tr("use as a new start"))}</button>
        ${SHARE_ENABLED ? `<button class="ghost-btn small opt-share" data-rank="${res.rank}">${escHtml(tr("share"))}</button>` : ""}
      </div>
      <div class="opt-lanes">${res.near ? near(res) : `${escHtml(tr("from"))} ${lanes(res)}`}</div>
      <div class="opt-card" data-rank="${res.rank}"></div>
    </div>`).join("");
  const failed = (r.failed_starts || []).map((f) => `<div class="opt-row opt-failed"><b>${escHtml(tr("start"))} ${f.start + 1}</b> — ${
    escHtml(tr("no legal build from this start"))} <span class="sim-hint">${escHtml(f.why)}</span></div>`).join("");
  const meta = `<span class="${r.cut ? "warn" : "ok"}">${escHtml(tr(r.cut
    ? "the time budget ran out before every start settled — strong builds, short of what those starts reach"
    : "each start improved until no change helped — the best those starts reach, not a proven best; add a start to look further"))}</span>`
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
  // A FINALIST IS SHARED WITHOUT BEING OPENED — opening a build resets the
  // search — so the panel opens under its row, for that row's build.
  $("opt-results").querySelectorAll(".opt-share").forEach((el) => el.addEventListener("click", async () => {
    openBuildShare("optimizer", el.closest(".opt-row"), { state: await resultToState(byRank.get(Number(el.dataset.rank))) });
  }));
  // THE SIMULATOR'S CARD, once each row's build is planned.
  $("opt-results").querySelectorAll(".opt-card").forEach(async (el) => {
    try { el.innerHTML = cardOfState(await resultToState(byRank.get(Number(el.dataset.rank))), w); } catch (_) { /* the row still stands */ }
  });
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
  const e = presetEntry("opt " + n, state);
  ps.push(e);
  storePresetList(BUILDS, ps);
  renderPresetBar(); // the builder's bar shows the new chip when you switch back
  if (btn) { btn.textContent = "✓ " + e.name; btn.disabled = true; }
  return e.id;
}
