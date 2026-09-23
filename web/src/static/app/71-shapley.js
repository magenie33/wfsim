// ---- PART VALUE ANALYSIS: what each chosen part multiplies the result by ----
//
// docs/SHAPLEY.md. Exact Shapley values over ln(metric): EVERY subset of the
// chosen parts is simulated through `/api/simulate` on one seed, the rest of
// the build stays on as the background, and `/api/shapley` does the arithmetic.
// A report of the SIMULATOR — it runs `theFight()` and owns only its run count.

const SHAPLEY_KEY = "wfsim-shapley";
// TWO AT LEAST, because a band needs two runs; the run count is the only lever
// on the cost, since the subsets are enumerated exactly.
const SHAPLEY_RUNS_MIN = 2;
const SHAPLEY_RUNS_MAX = 2000;
let shapleyPrefs = { runs: 10 };
try { const s = JSON.parse(localStorage.getItem(SHAPLEY_KEY)); if (s) shapleyPrefs = { ...shapleyPrefs, ...s }; } catch (_) {}
const saveShapleyPrefs = () => { try { localStorage.setItem(SHAPLEY_KEY, JSON.stringify(shapleyPrefs)); } catch (_) {} };
const shapleyRuns = () => Math.max(SHAPLEY_RUNS_MIN,
  Math.min(SHAPLEY_RUNS_MAX, Math.round(Number(shapleyPrefs.runs)) || 10));
const shapleyMax = () => (META && META.shapley_max_participants) || 0;

/// The keys the reader unticked or ticked away from the default, which is
/// EVERY MOD. A key names a POSITION (`mod:3`, `arcane:0`, `evo:2`), so a
/// choice survives swapping the card in that slot.
let shapleyChoice = {};
let shapleyGen = 0;
let shapleyJob = { running: false, done: 0, total: 0, note: "", result: null };

/// EVERY PART OF THE OPEN BUILD THAT CAN BE SWITCHED OFF. Mode, assembly and
/// valence are not here: none of them has an "off", only another choice.
function shapleyParts() {
  const out = [];
  slots.forEach((s, i) => {
    const m = s.mod && modById(s.mod);
    if (m) out.push({ key: "mod:" + i, kind: "mod", label: m.name });
  });
  arcanes.forEach((id, i) => {
    const a = id !== "none" && arcaneById(id);
    if (a) out.push({ key: "arcane:" + i, kind: "arcane", label: a.name });
  });
  weaponEvos().forEach((t) => {
    const id = evoSel[t.tier];
    if (id) out.push({ key: "evo:" + t.tier, kind: "evo", label: `${t.tier} · ${evoName(id)}` });
  });
  return out;
}
const shapleyPicked = (p) => (p.key in shapleyChoice ? shapleyChoice[p.key] : p.kind === "mod");
const shapleyChosen = () => shapleyParts().filter(shapleyPicked);

/// THE BUILD WITH ONLY THE SUBSET `mask` OF `parts` ON, as fields to override
/// on `buildPayload()`. A removed card leaves the others in their order, so the
/// elements recombine exactly as the builder would combine them.
function shapleyPayload(parts, mask) {
  const off = new Set(parts.filter((_, i) => !(mask >> i & 1)).map((p) => p.key));
  return {
    mods: slots.map((s, i) => (s.mod && !off.has("mod:" + i) ? slotModId(s) : null)).filter(Boolean),
    arcane: arcanes.map((id, i) => (off.has("arcane:" + i) ? "none" : id)),
    evolutions: weaponEvos().filter((t) => evoSel[t.tier] && !off.has("evo:" + t.tier))
      .map((t) => evoSel[t.tier]),
  };
}

const shapleyFight = () => theFight({ runs: shapleyRuns(), seed: GAIN_SEED, run_series: true });
/// WHAT AN ANALYSIS IS OF: the build, the fight and the chosen parts. A result
/// whose key is not the current one is still shown, and said to be stale.
const shapleyKey = () => JSON.stringify([buildPayload(), shapleyFight(), shapleyChosen().map((p) => p.key)]);

function shapleyStop(note) {
  shapleyJob.running = false;
  shapleyJob.note = note || "";
  renderShapley();
}

async function runShapley() {
  const gen = ++shapleyGen;
  const live = () => gen === shapleyGen;
  const parts = shapleyChosen();
  const k = parts.length;
  if (!k) return shapleyStop(tr("choose at least one part"));
  if (k > shapleyMax()) {
    return shapleyStop(tr("at most {n} parts are analysed exactly").replace("{n}", shapleyMax()));
  }
  const fight = shapleyFight();
  const base = buildPayload();
  const key = shapleyKey();
  const metric = metricOf(fight.metric);
  const field = metric.per_minute ? "score_runs" : "dps_runs";
  const total = 1 << k;
  const values = new Array(total);
  const scenario = (scenarioList().find((x) => presetId(x) === activeScenario) || {}).name || "—";
  shapleyJob = { running: true, done: 0, total, note: "", result: shapleyJob.result, beganAt: Date.now() };
  renderShapley();
  let cursor = 0, refused = null, duration = 0;
  // ONE SUBSET PER CALL, every lane pulling the next as it frees up — the same
  // shape as the quick calc's scan, and it yields to a person's own Run.
  const ask = async (lane, m) => {
    const r = await laneAsk(lane, "/api/simulate", { ...base, ...fight, ...shapleyPayload(parts, m) }, live);
    if (!live() || r === null) return r !== null;
    if (!r.ok || !Array.isArray(r[field])) {
      const on = parts.filter((_, i) => m >> i & 1).map((p) => p.label).join(", ") || tr("none of them");
      refused = `${tr("the build with only these parts did not run")}: [${on}] — ${r.error || "?"}`;
      return true;
    }
    values[m] = r[field];
    duration = r.duration || duration;
    shapleyJob.done++;
    renderShapleyProgress();
    return true;
  };
  await Promise.all((await gainLanes()).map(async (lane) => {
    for (;;) {
      if (!live() || refused) return;
      await yieldToForeground(live);
      if (!live() || refused) return;
      const m = cursor++;
      if (m >= total) return;
      if (!(await ask(lane, m))) return;
    }
  }));
  // WHAT A LOST LANE NEVER ASKED, asked once more on a fresh one.
  for (let m = 0; m < total && live() && !refused; m++) {
    if (values[m] === undefined && !(await ask(freeLane(), m))) break;
  }
  if (!live()) return;
  if (refused) return shapleyStop(refused);
  const holes = values.filter((x) => x === undefined).length;
  if (holes) return shapleyStop(tr("{n} of {total} could not be measured").replace("{n}", holes).replace("{total}", total));
  const res = await api("/api/shapley", { participants: parts.map((p) => p.key), values });
  if (!live()) return;
  if (!res || !res.ok) return shapleyStop((res && res.error) || tr("the analysis failed"));
  // THE ENDS IN THE METRIC'S OWN UNIT: `score` is kill progress over the whole
  // engagement, which a per-minute metric turns into a rate.
  const unit = (x) => (metric.per_minute ? kpm(x, duration) : x);
  shapleyJob.result = { res, parts, key, scenario, metric: metricLabel(metric), runs: fight.runs,
    full: unit(res.full), empty: unit(res.empty) };
  shapleyStop("");
}

// ---- drawing ------------------------------------------------------------

const shpX = (log) => "×" + sig2(Math.exp(log));
const shpPct = (log) => gainPct(Math.exp(log) - 1);
const shpBand = (st) => (st && st.se ? ` ±${sig2(st.se * 100)}%` : "");
/// A figure the runs cannot tell from nothing is drawn muted, never hidden.
const shpClear = (st) => !st.se || Math.abs(st.log) > 2 * st.se;

function renderShapleyProgress() {
  const bar = $("shapley-progress");
  if (!bar) return;
  const j = shapleyJob;
  const pct = j.total ? Math.round((j.done / j.total) * 100) : 0;
  bar.innerHTML = j.running
    ? `<div class="scan-strip" role="status"><span class="scan-bar"><i style="width:${pct}%"></i></span>`
      + `<span class="scan-txt">${escHtml(tr("subsets"))} ${j.done}/${j.total}</span></div>`
    : "";
}

function shapleyResultHtml() {
  const R = shapleyJob.result;
  if (!R) return "";
  const { res, parts } = R;
  const byKey = Object.fromEntries(parts.map((p) => [p.key, p]));
  const rows = res.participants.map((p, i) => ({ ...p, i, label: (byKey[p.id] || {}).label || p.id }))
    .sort((a, b) => b.shapley.log - a.shapley.log);
  const stale = R.key !== shapleyKey()
    ? `<div class="shp-stale">${escHtml(tr("this analysis is of another build or fight — analyse again to update it"))}</div>` : "";
  const product = rows.reduce((s, r) => s + r.shapley.log, 0);
  const head = `<div class="shp-meta">${escHtml(R.scenario)} · `
    + `${escHtml(tr("{s} subsets × {n} runs").replace("{s}", res.subsets).replace("{n}", R.runs))}</div>`
    + `<div class="shp-scope">${escHtml(tr("With the rest of the build kept on, what each chosen part multiplies the result by."))}</div>`
    + `<div class="shp-total">${escHtml(tr("all chosen parts together"))}: <b>${shpX(res.total.log)}</b>${shpBand(res.total)}`
    + ` <span class="sb-empty">(${sig2(R.full)} / ${sig2(R.empty)} ${escHtml(R.metric)}; ${escHtml(tr("the multipliers below multiply to"))} ${shpX(product)})</span></div>`;
  const cell = (st, fmt, cls) => `<td class="${cls}${shpClear(st) ? "" : " shp-dim"}" title="${escHtml(fmt(st.log) + shpBand(st))}">${fmt(st.log)}<small>${shpBand(st)}</small></td>`;
  const table = `<div class="shp-scroll"><table class="shp-table"><thead><tr>`
    + `<th>${escHtml(tr("Part"))}</th>`
    + `<th title="${escHtml(tr("its Shapley value: its average marginal effect over every order the parts could be added in"))}">${escHtml(tr("Equivalent multiplier"))}</th>`
    + `<th title="${escHtml(tr("the full build against the full build without this part"))}">${escHtml(tr("Take it out"))}</th>`
    + `<th title="${escHtml(tr("this part alone against none of the chosen parts"))}">${escHtml(tr("Alone"))}</th>`
    + `</tr></thead><tbody>${rows.map((r) => `<tr><th>${escHtml(r.label)}</th>`
      + cell(r.shapley, shpX, "shp-phi")
      + cell({ ...r.leave_one_out, log: -r.leave_one_out.log }, shpPct, "")
      + cell(r.alone, shpX, "") + `</tr>`).join("")}</tbody></table></div>`;
  if (rows.length < 2) return stale + head + table;
  const m = res.interactions;
  // THE INTERACTION MATRIX in the same order as the table: above 1 the pair is
  // worth more together (synergy), below 1 one dilutes the other.
  const big = Math.max(1e-9, ...rows.flatMap((a) => rows.map((b) => (a.i === b.i ? 0 : Math.abs(m[a.i][b.i].log)))));
  const icell = (a, b) => {
    if (a.i === b.i) return `<td class="shp-diag"></td>`;
    const st = m[a.i][b.i];
    const tone = st.log >= 0 ? "syn" : "dil";
    const alpha = Math.round((Math.abs(st.log) / big) * 60);
    return `<td class="shp-i ${tone}${shpClear(st) ? "" : " shp-dim"}" style="--a:${alpha}%" title="${escHtml(
      `${a.label} × ${b.label}: ${shpPct(st.log)}${shpBand(st)}`)}">${shpPct(st.log)}</td>`;
  };
  const matrix = `<h4 class="shp-h">${escHtml(tr("Pairs"))} <span class="sim-hint">${escHtml(
    tr("above zero the two are worth more together than apart; below zero one dilutes the other"))}</span></h4>`
    + `<div class="shp-scroll"><table class="shp-matrix"><thead><tr><th></th>${rows.map((r, n) =>
      `<th title="${escHtml(r.label)}">${n + 1}</th>`).join("")}</tr></thead><tbody>${rows.map((a, n) =>
      `<tr><th>${n + 1} · ${escHtml(a.label)}</th>${rows.map((b) => icell(a, b)).join("")}</tr>`).join("")}</tbody></table></div>`;
  return stale + head + table + matrix;
}

function renderShapley() {
  const box = $("shapley");
  if (!box || !META) return;
  const parts = shapleyParts();
  const chosen = parts.filter(shapleyPicked);
  const k = chosen.length, runs = shapleyRuns(), max = shapleyMax();
  const group = (kind, title) => {
    const list = parts.filter((p) => p.kind === kind);
    return list.length ? `<div class="shp-group"><span class="sb-h">${escHtml(tr(title))}</span>${list.map((p) =>
      `<label class="shp-part"><input type="checkbox" data-shp="${escHtml(p.key)}"${shapleyPicked(p) ? " checked" : ""}>${escHtml(p.label)}</label>`).join("")}</div>` : "";
  };
  const cost = k > max
    ? `<span class="shp-over">${escHtml(tr("at most {n} parts are analysed exactly").replace("{n}", max))}</span>`
    : escHtml(tr("{k} parts → {s} subsets × {n} runs = {f} fights")
      .replace("{k}", k).replace("{s}", 1 << k).replace("{n}", runs).replace("{f}", (1 << k) * runs));
  const j = shapleyJob;
  box.innerHTML = `<div class="shp-parts">${group("mod", "Mods")}${group("arcane", "Arcanes")}${group("evo", "Evolutions")}`
    + (parts.length ? "" : `<span class="sb-empty">${escHtml(tr("nothing on this build can be switched off"))}</span>`) + `</div>`
    + `<div class="shp-controls"><label>${escHtml(tr("Runs per subset"))} <input id="shapley-runs" type="number" min="${SHAPLEY_RUNS_MIN}" max="${SHAPLEY_RUNS_MAX}" value="${runs}"></label>`
    + `<span class="shp-cost">${cost}</span>`
    + (j.running
      ? `<button id="shapley-stop" class="ghost-btn small">${escHtml(tr("Stop"))}</button>`
      : `<button id="shapley-run" class="ghost-btn"${!k || k > max ? " disabled" : ""}>${escHtml(tr("Analyse"))}</button>`)
    + `</div><div id="shapley-progress"></div>`
    + (j.note ? `<div class="shp-note">${escHtml(j.note)}</div>` : "")
    + `<div class="shp-result">${shapleyResultHtml()}</div>`;
  renderShapleyProgress();
  box.querySelectorAll("input[data-shp]").forEach((el) => {
    el.onchange = () => { shapleyChoice[el.dataset.shp] = el.checked; renderShapley(); };
  });
  const ri = $("shapley-runs");
  if (ri) ri.onchange = () => { shapleyPrefs.runs = Number(ri.value); saveShapleyPrefs(); renderShapley(); };
  const go = $("shapley-run");
  if (go) go.onclick = () => runShapley();
  const stop = $("shapley-stop");
  if (stop) stop.onclick = () => { shapleyGen++; shapleyStop(tr("stopped")); };
}

// ---- the agent door's two actions, spread into `AGENT_ACTIONS` -----------
const SHAPLEY_ACTIONS = [
  {
    id: "simulator.shapley.parts",
    query: true,
    what: "List the parts of the open build a Shapley analysis can switch off, with the key each is chosen by and whether it is chosen now.",
    anchor: "#shapley-block",
    needs_weapon: true,
    args: {},
    run() {
      return { max: shapleyMax(), runs: shapleyRuns(),
        parts: shapleyParts().map((p) => ({ key: p.key, kind: p.kind, name: p.label, chosen: shapleyPicked(p) })) };
    },
  },
  {
    id: "simulator.shapley.run",
    writes: "prefs",
    what: "Run the Shapley analysis on the current fight: every subset of the chosen parts is simulated, and each part's equivalent multiplier, its take-it-out and alone effects, and every pair's interaction come back in log space. 2^k fights, so it can take minutes.",
    anchor: "#shapley-block",
    needs_weapon: true,
    args: {
      parts: { kind: "string", what: "comma-separated keys from simulator.shapley.parts; omitted keeps the current choice" },
      runs: { kind: "number", min: 2, max: 2000, what: "runs per subset; omitted keeps the reader's" },
    },
    async run({ parts, runs }) {
      if (parts != null) {
        const want = String(parts).split(",").map((x) => x.trim()).filter(Boolean);
        const all = shapleyParts();
        const unknown = want.filter((k) => !all.some((p) => p.key === k));
        if (unknown.length) return agentNo("unknown_part", { argument: "parts", got: unknown, alternatives: all.map((p) => p.key) });
        all.forEach((p) => { shapleyChoice[p.key] = want.includes(p.key); });
      }
      if (runs != null) { shapleyPrefs.runs = runs; saveShapleyPrefs(); }
      await runShapley();
      const R = shapleyJob.result;
      if (!R || R.key !== shapleyKey()) return agentNo("nothing_measured", { why: shapleyJob.note || null });
      const name = Object.fromEntries(R.parts.map((p) => [p.key, p.label]));
      return { scenario: R.scenario, metric: R.metric, runs: R.runs, total_log: R.res.total.log,
        parts: R.res.participants.map((p) => ({ key: p.id, name: name[p.id], shapley: p.shapley, leave_one_out: p.leave_one_out, alone: p.alone })),
        interactions: R.res.interactions };
    },
  },
];
