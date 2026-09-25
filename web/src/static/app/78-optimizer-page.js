// ---- The optimizer tab: the run bar, the starts, the results, the fight ----
//
// There is no scope to mark: the candidates are the quick calc's (docs/
// OPTIMIZER.md, "The quick descent"), and a start's pins are the one way to
// keep something. The fight is the simulator's, drawn as a card.

/// THE FINAL ROUND'S RUN COUNT — the simulator's Runs control, said again.
/// Saved by no preset (`OPT_RUNS_KEY`): how hard to measure right now is the
/// reader's, and neither the search nor the fight has an opinion about it.
function renderOptRuns() {
  const box = $("opt-runs-block");
  if (!box) return;
  box.innerHTML =
    `<label title="${escHtml(tr("how many simulations each finalist gets in the last round. Yours, not the search's and not the fight's: it is saved by no preset and pinned by no ruler. A smaller number searches faster and is worth re-measuring in the simulator"))}">${
      escHtml(tr("Final-round runs"))} <input type="number" id="opt-runs" min="1" max="20000" step="10" value="${finalRuns()}"></label>` +
    `<span class="sim-hint">${escHtml(tr("yours, in neither preset — the simulator's Runs is the same setting for the replay"))}</span>`;
  const el = $("opt-runs");
  el.addEventListener("change", () => {
    setFinalRuns(el.value);
    el.value = String(finalRuns());
    updateOptEstimate();
  });
}

function renderOpt() {
  show("opt-block", !!META);
  if (!META) return;
  renderOptRuns();
  // A weapon's first visit: the four default starts, then the active preset.
  if (!optSeeded) {
    opt.starts = defaultStarts();
    optSeeded = true;
    bootstrapOptPresets();
  }
  renderOptPresetBars();
  renderOptStarts();
  renderOptFight();
  updateOptEstimate();
}

/// THE FIGHT, READ BACK: which scenario, where it is edited, and the card. Not
/// a preset bar — that bar renames, duplicates and deletes, and the scenario
/// collection is the simulator's to edit. Redrawn wherever the fight changes.
function renderOptFight() {
  if (!META) return;
  const box = $("opt-fight-brief");
  if (box) box.innerHTML = fightCardHtml();
  const ref = $("opt-scenario-ref");
  if (ref) {
    const w = weaponInfo($("weapon").value) || {};
    ref.innerHTML =
      `<span class="plabel">${escHtml(tr("Scenario"))}</span>` +
      `<span class="pchip sel" title="${escHtml(tr("the scenario the simulator is set to"))}">${escHtml(presetLabel(scenarioNamed(activeScenario)) || tr("current"))}</span>` +
      `<a class="pchip" href="${weaponPath(w.id)}/simulator">${escHtml(tr("edit in the Simulator"))} →</a>`;
  }
}
