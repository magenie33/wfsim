// ---- The quick calc's controls: ranked slots, its settings, the mod picker

/// **ONE CONTROL FOR EVERY QUANTIFIABLE AXIS** — a CARD saying what is
/// installed, with a ⋯ in its corner that opens the RANKED list.
///
/// The arcane slot has been this shape all along and is the model: a card while
/// something is in it, a `+ add` plate while nothing is, and a menu offering
/// Swap and Remove. The parts, the valence element and the evolution tiers are
/// the same question — pick one of N, each worth a measurable amount on THIS
/// build — so they are the same control.
///
/// **WHETHER THE AXIS CAN BE EMPTY IS ONE MENU ITEM, NOT A SECOND SHAPE.**
/// `Remove` appears where nothing-installed is a real state (an arcane, an
/// evolution tier) and does not where it is not: a Kitgun has a grip, and every
/// copy of a Lich weapon carries an element. Offering it there would be a verb
/// that can never be used, which is the "always drawn and disabled" noise this
/// page already refuses — and hiding it costs no consistency, because the
/// consistency a reader actually reads is the CARD and the ranked list behind
/// it, not the length of a menu they have to open to see.
///
/// It registers with `ddReg` and opens through `ddOpen`, so the list, its
/// search and its ordering are the dropdown's own — only the TRIGGER differs.
function rankedSlot(id, cfg) {
  ddReg.set(id, {
    value: cfg.value,
    items: rankedItems(cfg),
    search: cfg.items.length > 8,
    // WHICH AXIS THIS LIST RANKS — what `openRanked` measures, what the strip
    // inside the list reports on, and what tells `ddRender` to rank its rows.
    axis: cfg.axis,
    axisLabel: cfg.label,
    onPick: cfg.onPick,
  });
  const card = cfg.card;
  if (!card) {
    // NOTHING INSTALLED. A locked tier says why instead of inviting a click it
    // would refuse.
    return `<div class="slot empty axis${cfg.locked ? " axlocked" : ""}" data-slot="${id}"${
      cfg.lockedWhy ? ` title="${escHtml(cfg.lockedWhy)}"` : ""}>
      <span class="axl">${escHtml(cfg.label)}</span>
      <span class="plus">${cfg.locked ? escHtml(cfg.lockedWhy || "") : "+ " + escHtml(cfg.addLabel || cfg.label)}</span>
    </div>`;
  }
  const icon = card.icon ? `<img class="eicon" src="${IMG(card.icon)}" alt="">` : "";
  const lines = (card.lines || []).map((x) => `<div>${escHtml(x)}</div>`).join("");
  return `<div class="slot filled axis${card.broken ? " broken" : ""}" data-slot="${id}" data-id="${
    escHtml(String(cfg.value))}"${card.title ? ` title="${escHtml(card.title)}"` : ""}>
    <span class="axl">${escHtml(cfg.label)}</span>
    ${icon}<div class="info"><div class="mn">${card.href ? wl(card.name, card.href) : escHtml(card.name)}${
      card.chips || ""}</div><div class="ed">${lines}</div>${card.notes || ""}</div>
    ${cfg.fixed ? "" : `<button class="dots" title="${escHtml(tr("options"))}">⋯</button>`}
  </div>`;
}

/// The rows a `rankedSlot` offers, ordered by gain.
function rankedItems(cfg) {
  // NEITHER RANKED NOR CHIPPED HERE. Both are read from the SCAN, which has
  // not started when a list is registered — `ddRender` does them, every time
  // it draws.
  return cfg.items.map((it) => ({
    value: it.value,
    label: it.label,
    hint: it.hint,
    disabled: it.disabled,
    key: it.key,
    badge: it.extra || "",
  }));
}

/// OPEN A RANKED LIST BY NAME — what a slot's ⋯ "Swap" and an empty plate
/// reach for, neither of which has a trigger element to delegate off.
/// Measuring is `ddOpen`'s, so every list that declares an axis is scanned
/// whichever control opened it.
function openRanked(id, anchor) {
  if (ddReg.get(id)) ddOpen(id, anchor);
}

/// The ⋯ menu a `rankedSlot` opens — Swap, and Remove where the axis has one.
function openSlotMenu(anchor, id, cfg) {
  closePopovers();
  const menu = $("slot-menu");
  const swap = `<div class="mi" data-a="swap">${escHtml(tr("Swap") + " " + cfg.label)}</div>`;
  const rm = cfg.removable
    ? `<div class="mi danger" data-a="remove">${escHtml(tr("Remove") + " " + cfg.label)}</div>` : "";
  menu.innerHTML = swap + rm;
  place(menu, anchor);
  menu.querySelector('[data-a="swap"]')
    .addEventListener("click", () => (cfg.onSwap ? cfg.onSwap() : openRanked(id, anchor)));
  const r = menu.querySelector('[data-a="remove"]');
  if (r) r.addEventListener("click", () => { closePopovers(); cfg.onPick(""); });
}

/// WIRE A BLOCK'S SLOTS, once its markup is in the DOM. One call per block
/// rather than per slot: the cards are re-rendered wholesale on every change,
/// so a listener bound to a card dies with the card that carried it — the
/// arena's own lesson.
function bindRankedSlots(box, cfgs) {
  box.querySelectorAll(".slot.axis").forEach((el) => {
    const id = el.dataset.slot;
    const cfg = cfgs[id];
    // A FIXED slot has nothing to pick: no ⋯, and a click opens no list of one.
    if (!cfg || cfg.locked || cfg.fixed) return;
    const dots = el.querySelector(".dots");
    if (dots) {
      // THE MENU HANGS OFF THE ⋯, NOT OFF THE CARD. `place` puts a popover's
      // top-left under its anchor's bottom-left, so anchoring to the whole card
      // dropped the menu at the card's BOTTOM-LEFT while a mod slot's — the one
      // that passes its own button — came out under the ⋯ at the top right.
      // Same control, same gesture, two different places on the page.
      dots.addEventListener("click", (e) => { e.stopPropagation(); openSlotMenu(e.currentTarget, id, cfg); });
    } else {
      // An EMPTY slot opens the list on a click anywhere, which is the mod and
      // arcane slots' own rule: there is nothing in it to select, so the whole
      // plate is the button.
      el.addEventListener("click", () => openRanked(id, el));
    }
  });
}

/// The picker's ONE ordering rule, over whatever keys an axis has.
/// Descending on every key, the chosen one first, the rest in a fixed order —
/// and an unscanned option sorts last whichever way the arrow points, because
/// an absent answer is not a small one.
function gainSort(a, b, keys) {
  const ga = gainOf(a.id), gb = gainOf(b.id);
  if (!ga !== !gb) return ga ? -1 : 1;
  // A GAIN IS RANKED BY ITS MEAN, and the band is shown beside it.
  //
  // The mean is the unbiased estimate of what an option is worth; the spread is
  // a property of the MEASUREMENT, not of the option — run it long enough and
  // the spread goes to zero while the mean stays put. Ranking on a lower bound
  // (`pct - se`) was tried and reverted for exactly that reason: it
  // systematically demotes whatever is merely hard to measure, and a status mod
  // is hard to measure by nature, so the list would have been telling players
  // something about the simulator rather than about their build.
  //
  // THIS DOES NOT MAKE THE ORDER STABLE, and it is not meant to. Two options
  // whose bands overlap are genuinely unranked, so which sits higher can move
  // between scans — that is the measurement talking, and the chip says so with
  // its ±. The answer to an order that moves is more runs, not a different
  // ranking rule; hiding it behind a pessimistic sort would have made a coin
  // flip look like a verdict.
  const cmp = { gain: () => (ga && gb ? gb.pct - ga.pct : 0),
                drain: () => (b.drain || 0) - (a.drain || 0),
                name: () => String(b.name).localeCompare(String(a.name)) };
  for (const k of keys) {
    const c = cmp[k]();
    if (Math.abs(c) > 1e-9) return pickerPrefs.dir === "desc" ? c : -c;
  }
  return 0;
}

/// The gain for `id`, or null when this axis position has not been scanned.
const gainOf = (id) => (gainAbout() === gainKey() ? gainScan.by[id] || null : null);

function openCalculationScenario() {
  nav(weaponPath(presetWeapon()) + "/simulator");
}

/// Builder preferences read the global fight; scenario editing stays in its module.
function renderQuickCalc() {
  const box = $("quick-calc");
  if (!box || !META) return;
  box.hidden = !routeWeaponId() || modSuffix() !== "";
  const on = gainPrefs.on !== false;
  const list = everyRank();
  box.innerHTML =
    `<div class="calculation-heading"><h2>${escHtml(tr("Quick calc"))}</h2><span>${escHtml(tr("Global preferences"))}</span></div>` +
    `<div class="calculation-scene"><div><span class="calculation-label">${escHtml(tr("Current scenario"))}</span>` +
    `<b id="gp-scen">${escHtml(gainScenario().name)}</b></div><div class="calculation-actions">` +
    `<button type="button" class="ghost-btn small" data-calculation-edit>${escHtml(tr("Edit scenario"))} →</button></div></div>` +
    `<div class="calculation-quick"><div class="calculation-switch"><label class="pc-h" title="${escHtml(tr("rank a slot's options by what they would change — off, nothing is simulated"))}">` +
    `<input type="checkbox" id="gp-on"${on ? " checked" : ""}> ⚡ ${escHtml(tr("Quick calc"))}</label>` +
    `<span class="calculation-help">${escHtml(tr("Open a slot to compare its options"))}</span></div>` +
    `<label class="pc-runs" title="${escHtml(tr("how many simulations each option is averaged over — 10 is the floor, and more costs proportionally more time"))}">` +
    `<span>${escHtml(tr("Per option"))}</span>` +
    `<input type="number" id="gp-runs" min="${GAIN_RUNS_MIN}" max="${GAIN_RUNS_MAX}" step="10" value="${gainRuns()}">` +
    `<span>${escHtml(tr("runs"))}</span></label></div>` +
    `<button type="button" class="calculation-rank-toggle" id="gp-ranks" aria-expanded="${everyRankOpen}"><span>${everyRankOpen ? "▾" : "▸"} ${escHtml(tr("Global rank comparison"))}</span>` +
    `<span class="calculation-label">${escHtml(tr("Mods"))} ${list.mods.length} · ${escHtml(tr("Arcanes"))} ${list.arcanes.length}</span></button>` +
    (everyRankOpen ? `<div class="calculation-ranks"><p>${escHtml(tr("All weapons share this list. Selected cards are compared at each rank; other candidates use max rank."))}</p>${everyRankPanel()}</div>` : "");
  if ($("calculation-scenario-context")) $("calculation-scenario-context").textContent = trF("View for {name}", { name: weaponInfo(presetWeapon()).name });
  box.querySelector("[data-calculation-edit]").onclick = openCalculationScenario;
  // Every click stays inside: a redraw detaches these nodes, and the document
  // outside-click handler closes on a target whose `.popover` ancestor is gone.
  box.onclick = (e) => e.stopPropagation();
  $("gp-on").onchange = (e) => {
    e.stopPropagation();
    gainPrefs = { ...gainPrefs, on: $("gp-on").checked };
    // A stale ranking must not outlive the switch, and "提升" must not stay
    // selected with nothing behind it.
    if (!gainPrefs.on) {
      // OFF MEANS STOP, not "stop asking for more". A scan already in flight
      // kept its workers and its right to write, so switching off and back on —
      // the first thing anyone does to something that looks stuck — resumed
      // into the state it was stuck in. Now it is the rebuild button.
      restartCalc();
      if (pickerPrefs.sort === "gain") { pickerPrefs.sort = "drain"; savePickerPrefs(); }
    }
    saveGainPrefs();
    renderQuickCalc();
    if (!$("mod-popover").hidden) { renderTools(); renderMenu(pickerSlot, $("mod-search").value); }
    renderEvo(); renderMode();
  };
  // A COUNT CHANGE IS A NEW QUESTION, so it re-runs rather than waiting for
  // the next picker to open — the same contract the scenario dropdown has.
  // `change` and not `input`, because a half-typed "1" on the way to "100" is
  // not a request to re-scan at one run.
  const gr = $("gp-runs");
  if (gr) gr.onchange = (e) => {
    e.stopPropagation();
    gainPrefs = { ...gainPrefs, runs: Number(gr.value) };
    saveGainPrefs();
    gr.value = gainRuns(); // show what was actually taken
    refreshGains();
  };
  // (Picking a scenario is handled by the dropdown's own `onPick`, which does
  // the same thing it always did: save, then answer the new question NOW
  // rather than at the next time a picker happens to open.)
  const rb = $("gp-ranks");
  if (rb) rb.onclick = () => { everyRankOpen = !everyRankOpen; renderQuickCalc(); };
  box.querySelectorAll(".rk-x").forEach((b) => { b.onclick = () => {
    const cur = everyRank();
    setEveryRank({ ...cur, [b.dataset.k]: cur[b.dataset.k].filter((x) => x !== b.dataset.id) });
    if (!$("rank-popover").hidden) renderEveryRankMenu($("rank-search").value);
  }; });
  const add = $("gp-ranks-add");
  if (add) add.onclick = () => openEveryRankPicker(add);
  const reset = $("gp-ranks-reset");
  if (reset) reset.onclick = () => setEveryRank(null);
}

/// THE EVERY-RANK LIST, EDITED IN PLACE — by card, never by effect. `null` is
/// the published default (`every_rank` in /api/meta). A list is a new question,
/// so the open scan re-asks.
function setEveryRank(next) {
  gainPrefs = { ...gainPrefs, everyRank: next || undefined };
  if (!next) delete gainPrefs.everyRank;
  saveGainPrefs();
  renderQuickCalc();
  if (!$("mod-popover").hidden) renderMenu(pickerSlot, $("mod-search").value);
  if (!$("arcane-popover").hidden) renderArcaneMenu($("arcane-search").value);
  refreshGains();
}

/// ONE CARD OF THE LIST, as the picker draws it — the mod or arcane row the
/// slot pickers use, so this list reads as the same cards and not as labels.
const everyRankRow = (k, card, opts = {}) => (k === "mods"
  ? modRow(card, { ...opts, attrs: `data-id="${card.id}" ${opts.attrs || ""}` })
  : arcaneRow(card, opts));
const everyRankMods = () => new Map(Object.values(META.mod_pools || {}).flat().map((m) => [m.id, m]));
const everyRankCard = (k, id) => (k === "mods"
  ? everyRankMods().get(id)
  : arcaneById(id));

/// Global candidates and saved selections do not narrow when a weapon changes.
function everyRankOffers() {
  return [
    ...[...everyRankMods().values()].filter((m) => !m.riven && !m.stance && m.max_rank > 0).map((m) => ["mods", m]),
    ...(META.arcanes || []).filter((a) => a.id !== "none" && (a.max_rank || 0) > 0).map((a) => ["arcanes", a]),
  ];
}

/// The editor's body: the list as picker rows, each removable, the button
/// that opens the picker, and the way back to the default.
function everyRankPanel() {
  const list = everyRank();
  const cards = ["mods", "arcanes"].flatMap((k) => list[k].map((id) => [k, everyRankCard(k, id)]))
    .filter(([, c]) => c);
  const row = ([k, c]) => everyRankRow(k, c, {
    trailing: `<button class="rk-x" data-k="${k}" data-id="${escHtml(c.id)}" title="${escHtml(tr("remove"))}">×</button>`,
  });
  const groups = [["mods", "Mods"], ["arcanes", "Arcanes"]].map(([k, label]) => {
    const selected = cards.filter(([kind]) => kind === k);
    return `<div class="calculation-rank-group"><h3>${escHtml(tr(label))} <span>${selected.length}</span></h3>`
      + (selected.length ? `<div class="combo-menu pc-rank-list">${selected.map(row).join("")}</div>`
        : `<p>${escHtml(tr("No cards selected"))}</p>`) + `</div>`;
  }).join("");
  return `<div class="pc-ranks">` +
    groups +
    `<button class="ghost-btn small" id="gp-ranks-add">+ ${escHtml(tr("add a card"))}</button>` +
    (gainPrefs.everyRank ? `<button class="ghost-btn small" id="gp-ranks-reset">${escHtml(tr("default list"))}</button>` : "") +
    `</div>`;
}

/// THE LIST'S PICKER — its own popover, like the riven stat picker, because a
/// slot picker's clicks EQUIP. A click here toggles membership and the picker
/// stays open, so several cards are one visit.
function openEveryRankPicker(anchor) {
  closePopovers();
  const pop = $("rank-popover");
  place(pop, anchor);
  const search = $("rank-search");
  search.value = "";
  search.oninput = () => renderEveryRankMenu(search.value);
  renderEveryRankMenu("");
  search.focus();
}

function renderEveryRankMenu(query) {
  const menu = $("rank-menu");
  const q = query.trim().toLowerCase();
  const list = everyRank();
  const on = ([k, c]) => list[k].includes(c.id);
  const hits = everyRankOffers()
    .filter(([k, c]) => !q || (k === "mods" ? searchHit(c, q) : searchBlob(c).includes(q)))
    // Mods before arcanes, the listed cards first in each, then by name.
    .sort((a, b) => (a[0] === b[0] ? 0 : a[0] === "mods" ? -1 : 1)
      || (on(b) - on(a))
      || String(a[1].name).localeCompare(String(b[1].name)));
  menu.innerHTML = hits.length
    ? sectionedRows(hits, ([k]) => (k === "mods" ? "Mods" : "Arcanes"), ([k, c]) => everyRankRow(k, c, {
      cls: on([k, c]) ? "cur" : "",
      attrs: `data-k="${k}"`,
      chips: on([k, c]) ? ` <span class="slotchip cur">${escHtml(tr("every rank"))}</span>` : "",
    }))
    : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", (e) => {
    // The redraw below detaches this row; the document handler would then
    // read the click as outside every popover and close this one.
    e.stopPropagation();
    if (e.target.closest("a")) return;
    const { k, id } = o.dataset;
    const cur = everyRank();
    setEveryRank({ ...cur, [k]: cur[k].includes(id) ? cur[k].filter((x) => x !== id) : [...cur[k], id] });
    renderEveryRankMenu($("rank-search").value);
  }));
}

/// The source remains gainScan; the task list is its only progress surface.
function renderCalcStatus() { scheduleUserTasks("progress"); }

function quickCalculationDetails() {
  const made = pool.filter(Boolean), alive = made.filter((lane) => !lane.dead).length;
  return `<div id="calc-status" class="task-quick-status">
    <div class="cs-row"><span>${escHtml(tr("fight"))}</span><b>${escHtml(gainScenario().name)}</b></div>
    <div class="cs-row"><span>${escHtml(tr("measuring"))}</span><b>${escHtml(gainScan.note || "—")}</b></div>
    <div class="cs-row"><span>${escHtml(tr("step"))}</span><b>${escHtml(gainScan.phase || tr("ranking the options"))}</b></div>
    <div class="cs-row"><span>${escHtml(tr("ranked"))}</span><b>${gainScan.done} / ${gainScan.total}</b></div>
    <div class="cs-row"><span>${escHtml(tr("workers"))}</span><b>${alive} / ${made.length || poolSize()}</b></div>
    ${gainScan.failed ? `<div class="cs-why">${escHtml(gainScan.note || tr("unknown"))}</div>` : ""}</div>`;
}

/// STOP EVERYTHING THE QUICK CALC IS DOING, and leave nothing that can write.
///
/// THE GENERATION IS BUMPED FIRST, before a worker is touched. A scan in flight
/// is `live()` until something bumps it, so a restart that only replaced the
/// state left the old scan free to finish into the NEW one and stamp its key —
/// filing a ranking of a fight nobody is looking at as complete, which is worse
/// than the stall it was asked to end.
///
/// BOTH WAYS OUT ARE THIS ONE FUNCTION: the rebuild button, and switching the
/// quick calc off. A reader flicks that switch when it looks stuck, so it has
/// to mean what the button means — anything less makes the obvious gesture the
/// one that does not work.
function restartCalc() {
  gainGen += 1;
  gainPending = null;
  resetPool();
  gainScan = { key: null, want: null, running: false, base: 0, floor: 0, by: {},
    done: 0, total: 0, note: "", metric: "", failed: false, lanesLost: 0 };
}

/// DROP EVERY WORKER, so the next question builds fresh ones.
///
/// A dead lane is replaced on demand now, so this is not the recovery path — it
/// is the reader's own, for the case nobody predicted. `abandon` first, because
/// a worker holding a live request has waiters that must be settled rather than
/// left hanging on a lane nobody will read again.
function resetPool() {
  for (const l of pool) {
    if (!l) continue;
    try { l.abandon(); } catch (_) { /* already gone */ }
    try { l.worker.terminate(); } catch (_) { /* already gone */ }
  }
  pool = [];
  gainPool = null;
}

/// Re-run whatever quick-calc surface is on screen. Called after ANY scenario
/// edit: the scan is measured under the scenario, so a change to it makes the
/// numbers on screen answers to a question nobody is asking any more.
///
/// It is not a repaint — `ensureGains` compares the key first, so a change the
/// chosen scenario does not care about costs nothing here. Evolution rows scan
/// without being opened, so they always refresh; the pickers only when open.
function refreshGains() {
  // The BOX first and unconditionally — it names the fight being measured, so
  // it has to follow a switch even when the scan itself is off.
  renderQuickCalc();
  if (gainPrefs.on === false) return;
  // **AN OPEN PICKER RE-ASKS; A SHUT ONE HAS NOTHING TO RE-ASK**. Every ranked axis now measures when its list is OPENED, so an
  // open list is the only place an answer exists that a fight change can make
  // stale — and all three kinds of list are treated the same way here, which
  // is the point.
  //
  // IT WAS A REPAINT FOR TWO OF THEM. The mod and arcane pickers were
  // re-rendered without being re-measured, and the freshness check passed
  // anyway because this function ended in `renderEvo()` — which asked for an
  // EVOLUTION scan, which made `gainScan.key` catch up with the fight while the
  // open mod picker's own chips still answered the old one. A check passing for
  // the wrong reason is what hid it.
  if ($("mod-popover") && !$("mod-popover").hidden) {
    renderTools();
    ensureGains({ kind: "mods", idx: pickerSlot },
      () => { if (!$("mod-popover").hidden) renderMenu(pickerSlot, $("mod-search").value); });
  }
  if ($("arcane-popover") && !$("arcane-popover").hidden) {
    ensureGains({ kind: "arcane", idx: arcaneSlotIdx },
      () => { if (!$("arcane-popover").hidden) renderArcaneMenu($("arcane-search").value); });
  }
  const dd = $("dd-popover");
  if (dd && !dd.hidden && dd._anchor) {
    const id = dd._anchor.dataset.slot || dd._anchor.dataset.dd;
    if (id && (ddReg.get(id) || {}).axis) openRanked(id, dd._anchor);
  }
  renderMode();
}

/// Compute this axis position's ranking, unless it is already on screen.
/// `gainKey` covers the axis, the build, the scenario and the settings, so
/// re-opening the same picker costs nothing and any edit invalidates it.
/// `user` — a person opened this list, rather than a repaint asking again for
/// one already on screen. It is the difference between preempting and queueing;
/// see the comment on the queue below.
function ensureGains(axis, repaint, user) {
  // Nothing to measure against yet. The evolution rows scan without being
  // opened, so on a cold load they can fire before `initPresets` has seeded
  // the scenario library — and a scan with no named scenario is one nobody
  // can reproduce or compare against (it labelled itself "—").
  if (gainPrefs.on === false) return;
  if (!scenarioList().length) return;
  gainAxis = axis;                       // so the key describes what we want
  // Is what we have MEASURED the fight on screen now? The key is stamped on
  // completion, so this means answered and nothing else — a scan that died
  // half way leaves it null and the next request re-asks, which is the whole
  // point of moving it there.
  if (gainScan.key === gainKey()) return;
  // …AND IS ONE ALREADY IN FLIGHT FOR IT? A separate question from the line
  // above, and it has to be: one check covering both cannot tell "answering"
  // from "answered", and a scan that never finishes is then filed for ever as
  // one that did.
  if (scanIsLive() && gainScan.want === gainKey()
      && JSON.stringify(gainScan.axis) === JSON.stringify(axis)) return;
  // A running scan is STALE and gives way — but only to its own axis. The mod
  // picker and the evolution rows both ask on every refresh (`refreshGains`
  // ends in `renderEvo`), so "the newest request wins" makes the two cancel
  // each other on every repaint and neither ever finishes. Which axis is asking
  // is not a staleness signal; the fight moving is.
  // A PERSON ASKING PREEMPTS; A REPAINT WAITS ITS TURN.
  //
  // The two are not the same request and were treated as one. Opening a slot is
  // a decision — the reader has stopped caring about the list they were looking
  // at — and queueing it behind a scan that is 87 melee engagements deep means
  // minutes before the list they just opened says anything, which reads as the
  // calculator being stuck. A REPAINT is not a decision: the mod picker and the
  // evolution rows both re-ask on every refresh, and letting those preempt each
  // other is what made the two cancel each other for ever and neither finish.
  //
  // So the queue stays, for the case it was built for, and a user request goes
  // straight past it. `scanGains` bumps the generation, so whatever was running
  // stands down at its next await — bounded to one outstanding simulation per
  // lane rather than the whole queue.
  if (user && scanIsLive()) gainPending = null;
  if (!user && scanIsLive()
      && JSON.stringify(gainScan.axis) !== JSON.stringify(axis)) {
    // …BUT IT IS NOT FORGOTTEN. Dropping it silently is what made a player have
    // to click between two evolutions until the numbers appeared (report,
    // 2026-08-13): the evolution rows ask on EVERY refresh while a picker asks
    // only while it is open, so with a picker open the evolution request was
    // dropped and nothing ever re-asked — the running scan's completion
    // repaints the caller that started it, which is the picker, not the rows.
    gainPending = { axis, repaint };
    return;
  }
  // A REJECTION HERE STOPPED THE CALCULATOR FOR GOOD. This is launched without
  // `await`, so anything that throws inside would otherwise leave `running`
  // true and the counter frozen. The scan is handed the LIST'S REPAINT and
  // draws nothing itself — `paintCalc` owns every surface, including the queue
  // taking its turn, so a new exit path cannot forget one of them.
  scanGains(axis, repaint).catch((e) => {
    gainStop(`${tr("the calculator stopped")}: ${(e && e.message) || e}`);
  });
}

function familyConflict(mod, exceptIdx) {
  if (!mod.family) return false;
  return slots.some((s, i) => { if (i === exceptIdx || !s.mod) return false; const o = modById(s.mod); return o && o.family === mod.family; });
}

// Rows grouped under STICKY headings, each section in its own box.
//
// A sticky element is confined to its containing block, so headings that are
// all siblings of the rows share one: the first one sticks at the top and
// never leaves, and the second slides underneath it — two headings on one
// line. A box per section is the whole fix: each heading
// sticks while its own rows are on screen and is pushed out by the next.
function sectionedRows(items, sectionOf, rowHtml) {
  const parts = [];
  let cur = null;
  items.forEach((m, i) => {
    const s = sectionOf(m);
    if (s !== cur) {
      if (cur !== null) parts.push("</div>");
      cur = s;
      parts.push(`<div class="menu-sect"><div class="menu-head">${escHtml(tr(s))}</div>`);
    }
    parts.push(rowHtml(m, i));
  });
  if (cur !== null) parts.push("</div>");
  return parts.join("");
}

/// ONE MOD ROW — the card every mod list draws: polarity, image, the
/// wiki-linked name, the EXILUS chip, the effect lines, and a TRAILING control
/// that is the list's own (the picker's drain, the every-rank list's ×).
/// `exilusChip` is off in the EXILUS SLOT's own list, where every row is
/// exilus-eligible and the chip is the heading repeated once per card.
const modRow = (m, { cls = "", title = "", attrs = "", chips = "", note = "",
  trailing = "", exilusChip = true, rank = m.max_rank } = {}) =>
  `<div class="opt ${cls} ${m.rarity ? "rar-" + m.rarity : ""}" ${attrs} title="${title}">
      ${imgTag(POL(m.polarity), "pol")}${imgTag(IMG(m.image), "mod")}
      <div class="info"><div class="mn">${
    wl(m.name, modWikiUrl(m))}${modMarketLink(m)}${
    exilusChip && m.exilus ? ' <span class="exchip">EXILUS</span>' : ""}${chips}</div><div class="me">${
    cardLines(m, rank).map((x) => `<div>${x}</div>`).join("")}</div>${note}</div>${trailing}</div>`;

function renderMenu(slotIdx, query) {
  rivenPickerSlot = slotIdx;
  const menu = $("mod-menu");
  const q = query.trim().toLowerCase();
  // Equipped mods stay LISTED: the current slot's mod is marked, mods in other
  // slots show their slot number — picking one of those EXCHANGES the two slots.
  // ONE group ahead of the rule: this slot's own mod, because it is the
  // baseline every number below is measured against. Everything else obeys
  // the sort — including mods sitting in OTHER slots, which are not pinned
  // into a band of their own. Such a band contradicts whichever order is
  // chosen (eight rows of unsorted drain at the top of a drain sort), and it
  // carries nothing: every placed mod already has a "slot N" chip
  // that says where it is.
  const here = slotModId(slots[slotIdx]);
  const group = (m) => (here === m.id ? 0 : 1);
  const hits = buildPool()
    .filter((m) => modFitsSlot(m, slotIdx))
    .filter((m) => !pickerPrefs.pol || m.polarity === pickerPrefs.pol)
    .filter((m) => searchHit(m, q))
    // A CARD ON THE EVERY-RANK LIST is a row per rank, each with its own gain.
    .flatMap((m) => [m, ...lowerRanks(m)])
    .sort((a, b) => {
      const g = group(a) - group(b); // current first, then equipped, then the rest
      if (g) return g;
      // RIVENS FIRST, as their own block: they are the build's own items and
      // sorting them in by name would scatter them through the pool.
      const r = (b.riven ? 1 : 0) - (a.riven ? 1 : 0);
      if (r) return r;
      // ONE RULE, not three special cases.
      //
      //   · every key sorts DESCENDING — effect, then drain, then name;
      //   · the key you PICK is hoisted to the front, and the rest keep that
      //     fixed order behind it. Pick drain and you get drain, effect, name.
      //
      // Drain descending is deliberate: two mods the target ignores score the
      // same, and the expensive one is doing more (it just is not doing it
      // HERE), so it is the one worth looking at first.
      //
      // The arrow flips the whole comparison, keys and all — one rule means
      // one direction. What it does not flip is UNSCANNED-LAST: an absent
      // answer is not a small one, so it stays at the bottom either way.
      return gainSort(a, b, [pickerPrefs.sort,
        ...["gain", "drain", "name"].filter((k) => k !== pickerPrefs.sort)]);
    });
  // No cap: every pool mod must be reachable. The popover menu scrolls
  // (`.combo-menu` overflow-y), so the whole sorted/filtered list is browsable.
  // Two sections, each labelled. With rivens leading, an unlabelled pool
  // below them would read as a continuation of the riven list.
  const row = (m) => {
    const isCur = here === m.id;
    const at = placedAt(m.card || m.id, slotIdx);
    // Exchanging with the exilus slot would move OUR mod there — only legal
    // if it is exilus-eligible (or the slot is empty).
    const ownMod = slots[slotIdx].mod ? modById(slots[slotIdx].mod) : null;
    const exIllegal = at === EXILUS && ownMod && !ownMod.exilus;
    const conflict = at < 0 && !isCur && familyConflict(m, slotIdx);
    // Every placed mod shows a "slot N" chip (the current one shows ITS OWN
    // slot). No "current" word — same color family; background does the
    // distinguishing, the current slot rendered a touch stronger.
    // LOCALIZED, and it was not until 2026-08-10: this chip is the ONLY place
    // the app names a slot, so a Chinese page read "slot 5" while everything
    // around it was translated — and the number badge on the slot itself now
    // has to agree with it word for word.
    const slotName = (idx) =>
      idx === EXILUS ? tr("exilus")
      : idx === STANCE ? tr("stance")
      : tr("slot") + " " + (idx + 1);
    const badge = isCur ? `<span class="slotchip cur">${slotName(slotIdx)}</span>`
      : at >= 0 ? `<span class="slotchip">${slotName(at)}</span>` : "";
    // The gain is THIS SLOT's — the same mod is worth something different in
    // another slot, because elements combine by mod order.
    const gainChip = gainChipFor(m.id, slotName(slotIdx));
    const title = conflict ? `incompatible (${m.family})`
      : exIllegal ? `cannot swap: ${ownMod.name} is not an exilus mod`
      : at >= 0 ? `swap with ${at === EXILUS ? "the exilus slot" : "slot " + (at + 1)}`
      : m.effects.join(" · ");
    return modRow(m, {
      cls: `${conflict || exIllegal ? "dis" : ""} ${isCur ? "cur" : at >= 0 ? "placed" : ""}`,
      attrs: `data-id="${m.id}"`,
      title,
      rank: m.card ? m.rank : m.max_rank,
      chips: ` ${m.card ? `<span class="rkchip">R${m.rank}</span>` : ""}${badge}${gainChip}`,
      // THE BUILDER BINDS A VALUE, and this is it: what seating this card
      // costs the slot you are standing in.
      trailing: `<span class="dr">${m.drain}</span>`,
    });
  };
  menu.innerHTML = scanStrip(gainScan, { kind: "mods", idx: slotIdx }, hits.map((m) => m.id))
    + (hits.length
      ? sectionedRows(hits, (m) => (m.riven ? "Riven" : "Mods"), row)
      : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`);
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", () => {
    if (here === o.dataset.id) { closePopovers(); return; } // already here
    const [id, rank] = splitRank(o.dataset.id);
    equipMod(slotIdx, id, rank);
    closePopovers(); renderMods();
  }));
}

// THE MOD SLOT'S MENU, on the shared one. It was a THIRD copy of
// the same two items — and the collision that found it is worth recording: this
// function was called `openSlotMenu` too, and being declared later in the file
// it silently SHADOWED the shared one, so an evolution card's ⋯ opened "Swap
// mod". Its items were hardcoded English as well, which is the same gap the
// arcane menu carried.
//
// MOD ops only: polarity lives on the left icon, and duplicating it here is
// what this menu has always refused.
function openModSlotMenu(slotIdx, anchor) {
  openSlotMenu(anchor, null, {
    label: tr("Mod"),
    removable: true,
    onSwap: () => openPicker(slotIdx, slotEl(slotIdx)),
    // In place: the slot keeps its polarity, which is a property of the SLOT
    // and not of what was in it.
    onPick: () => { equipMod(slotIdx, null); renderMods(); },
  });
}

function openPolMenu(slotIdx) {
  closePopovers();
  const menu = $("slot-menu");
  const cur = slots[slotIdx].pol;
  menu.innerHTML = GUN_POLS.map((p) => `<div class="mi ${p === cur ? "sel" : ""}" data-p="${p}">${imgTag(POL(p), "pol")} ${p === "Omni" ? "Omni (any)" : p}</div>`).join("") +
    `<div class="mi ${!cur ? "sel" : ""}" data-p="">◇ none</div>`;
  place(menu, slotEl(slotIdx));
  menu.querySelectorAll(".mi").forEach((o) => o.addEventListener("click", () => {
    setSlotPolarity(slotIdx, o.dataset.p || null);
    closePopovers(); renderMods();
  }));
}

/// A slot's polarity, or none. The mod stays: polarity is the slot's.
function setSlotPolarity(i, pol) {
  slots[i].pol = pol || null;
}

