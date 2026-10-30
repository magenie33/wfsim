// Permanent attributes are calculated lazily and cached by engine and build.
function libraryStaticBuild(w, p) {
  const defs = {}, axes = headlessBoardRowAxes(pageSay(), w, p.board, defs);
  const request = headlessSeat(META, { weapon: w.id, ...axes });
  request.evolutions_as_given = true;
  request.rivens = Object.entries(defs).map(([id, spec]) => ({ id: id.slice("riven:".length), spec: boardRivenState(spec) }));
  return request;
}
function libraryStatScope(w, all) {
  return [...new Map(all.filter((p) => p.benchmark === finder.b
    && (finder.mo === FINDER_EVERY_MODE || p.mode === finder.mo))
    .map((p) => [libraryStatKey(w, p), p])).values()];
}
function libraryStatStatus(w, scope) {
  const box = $("library-stat-status");
  if (!box) return;
  box.hidden = !libraryStatBounds().length;
  if (box.hidden) return;
  const pending = scope.filter((p) => !libraryStatCache.has(libraryStatKey(w, p))).length;
  const missing = scope.filter((p) => libraryStatCache.get(libraryStatKey(w, p))?.ok === false).length;
  const failed = libraryStatRun?.weapon === w.id && libraryStatRun.error;
  box.textContent = failed ? tr("Static attributes could not be calculated.")
    : pending ? trF("Calculating static attributes… {done}/{total}", { done: scope.length - pending, total: scope.length })
      : missing ? trF("{n} builds have unavailable static attributes.", { n: missing }) : "";
  if (failed || missing) {
    const retry = document.createElement("button");
    retry.type = "button"; retry.className = "ghost-btn small"; retry.dataset.libraryStatsRetry = ""; retry.textContent = tr("Retry");
    box.append(" ", retry);
  }
}
function libraryEnsureAttributes(w, all, retry = false) {
  if (!libraryStatBounds().length || !libraryActive) return;
  const scope = libraryStatScope(w, all), key = JSON.stringify([w.id, finder.b, finder.mo]);
  if (retry) {
    for (const p of scope) if (libraryStatCache.get(libraryStatKey(w, p))?.ok === false) libraryStatCache.delete(libraryStatKey(w, p));
    libraryStatRun = null;
  }
  libraryStatStatus(w, scope);
  if (libraryStatRun?.key === key && (libraryStatRun.running || libraryStatRun.error)) return;
  const pending = scope.filter((p) => !libraryStatCache.has(libraryStatKey(w, p)));
  if (!pending.length) return;
  const run = libraryStatRun = { key, weapon: w.id, running: true, error: false };
  (async () => {
    try {
      for (let i = 0; i < pending.length; i += 64) {
        if (libraryStatRun !== run || !libraryActive || !libraryStatBounds().length || finder.weapon !== w.id) return;
        const rows = pending.slice(i, i + 64);
        const r = await api("/api/build/static", { builds: rows.map((p) => libraryStaticBuild(w, p)) });
        if (!r?.ok || r.stats?.length !== rows.length) throw new Error(r?.error || "invalid static attributes response");
        rows.forEach((p, k) => libraryStatCache.set(libraryStatKey(w, p), r.stats[k]));
        if (libraryStatRun === run && libraryActive && finder.weapon === w.id) libraryStatStatus(w, scope);
        await new Promise((resolve) => setTimeout(resolve, 0));
      }
    } catch (_) { run.error = true; }
    finally {
      run.running = false;
      if (libraryStatRun === run && libraryActive && finder.weapon === w.id) {
        const focused = document.activeElement;
        const input = focused?.dataset.queryValue != null
          ? { path: focused.dataset.queryValue, value: focused.value } : null;
        librarySaveView(); renderBuildFinder();
        if (input) {
          const field = document.querySelector(`[data-query-value="${input.path}"]`);
          if (field) { field.value = input.value; field.focus({ preventScroll: true }); }
        }
      }
    }
  })();
}
