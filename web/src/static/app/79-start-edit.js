// ---- The optimizer's starts, and editing one in the builder -----------------
//
// A START IS A BUILD (docs/OPTIMIZER.md, "The quick descent"), so it is drawn with the
// simulator's own card (`cardOfState`) and edited in the BUILDER itself — never
// in a second slot UI. While one is open (`startEdit`, declared in 09-state):
//   - the builder's autosave is suspended, so the player's own build is never
//     written with the start's contents (`markPresetDirty`);
//   - a banner names the start, with Done and Discard, and the build bar is put
//     away, since switching builds mid-edit would mean nothing;
//   - every position carries a FIXED pin: that start's answer always keeps it.
// Done writes the builder's build and pins into the start; both put the
// builder back exactly as it was.

const START_PIN_SVG = '<svg viewBox="0 0 12 12" width="12" height="12" fill="none" stroke="currentColor" '
  + 'stroke-width="1.4" stroke-linecap="round" aria-hidden="true"><path d="M4.5 1.5h3M5 1.5v3.5L3 7h6L7 5V1.5M6 7v4"></path></svg>';

/// A start in the stored shape — `{ build, fixed }` — whatever it was saved as.
/// A start from before builds were starts named cards and locks; it becomes the
/// build those cards make, pins where the locks were.
function normalizeStart(s, w) {
  if (s && s.build) return { build: s.build, fixed: (s.fixed || []).slice() };
  const mods = (s && s.mods) || [];
  const build = stateFromBuild({ mods, arcane: (s && s.arcane) || [] }, w);
  const fixed = mods.map((id, i) => ((s.locked || []).includes(id) ? "mods:" + i : null)).filter(Boolean);
  if (s && s.lock_arcane) fixed.push("arcane:0");
  return { build, fixed };
}

/// The starts as `/api/optimize` reads them. No starts is ONE BLANK build — the
/// builder cleared and filled in the search's own order — not a hidden default.
const startsPayload = () => (opt.starts.length ? opt.starts.map(startPayload)
  : [{ slots: Array(10).fill(null), evolutions: [], arcane: [], arcane_rank: [], mode: null, valence_element: null, fixed: [] }]);

/// A start as `/api/optimize` reads it: the build's slots, its axes, its pins.
function startPayload(s) {
  const b = s.build;
  return {
    slots: (b.slots || []).map((x) => (x && x.mod ? rankedId(x.mod, x.rank) : null)),
    evolutions: Object.values(b.evoSel || {}).filter(Boolean),
    arcane: (b.arcane || []).slice(),
    arcane_rank: (b.arcaneRank || []).slice(),
    mode: b.mode || null,
    valence_element: (b.valence || {}).element || null,
    fixed: (s.fixed || []).map((k) => { const [kind, idx] = k.split(":"); return { kind, idx: Number(idx) }; }),
  };
}

/// THE FOUR DEFAULT STARTS: one build per primary element, holding the
/// strongest card of it this weapon can equip and nothing else — what a player
/// gets by making those four builds by hand, and edited and removed the same way.
function defaultStarts() {
  const w = $("weapon").value;
  const pool = buildPool();
  return ["cold", "heat", "electricity", "toxin"].map((e) => {
    let best = null;
    for (const m of pool) {
      if (m.element && m.element[0] === e && (!best || m.element[1] > best.element[1])) best = m;
    }
    return best ? { build: stateFromBuild({ mods: [best.id] }, w), fixed: [] } : null;
  }).filter(Boolean);
}

function addStart(build) {
  opt.starts.push({ build, fixed: [] });
  renderOptStarts(); updateOptEstimate();
}

/// THE STARTS LIST: each start is the simulator's card, with Edit and Remove.
function renderOptStarts() {
  const box = $("opt-starts");
  if (!box) return;
  const w = weaponInfo($("weapon").value) || {};
  opt.starts = (opt.starts || []).map((s) => normalizeStart(s, w.id));
  const mine = loadPresetList(BUILDS).filter((p) => p.state && p.state.weapon === w.id);
  box.innerHTML =
    `<h4 class="sim-h">① ${escHtml(tr("Starts"))} <span class="sim-hint">${escHtml(tr(opt.starts.length
      ? "each is a build the search begins from; edit one in the builder, where a pinned position is FIXED in its answer"
      : "none — the search begins from a blank build"))}</span></h4>`
    + opt.starts.map((s, i) => `<div class="opt-start" data-i="${i}">
        <div class="opt-start-h"><b>${escHtml(tr("Start"))} ${i + 1}</b><span style="flex-grow:1"></span>
        <button type="button" class="ghost-btn small" data-edit="${i}">${escHtml(tr("edit in the builder"))}</button>
        <button type="button" class="ghost-btn small" data-del="${i}" aria-label="${escHtml(tr("remove this start"))}">✕</button></div>
        ${cardOfState(s.build, w, new Set(s.fixed))}</div>`).join("")
    + `<div class="opt-start-add">`
    + `<button type="button" class="ghost-btn small" id="opt-start-add">${escHtml(tr("+ add the current build as a start"))}</button>`
    + `<button type="button" class="ghost-btn small" id="opt-start-blank">${escHtml(tr("+ a blank start"))}</button>`
    + (mine.length ? `<select id="opt-start-mine" class="ghost-btn small"><option value="">${escHtml(tr("+ add one of my builds"))}</option>`
      + mine.map((p, i) => `<option value="${i}">${escHtml(presetLabel(p))}</option>`).join("") + `</select>` : "")
    + `<button type="button" class="ghost-btn small" id="opt-start-defaults">${escHtml(tr("restore the 4 default starts"))}</button>`
    + `</div>`;
  $("opt-start-defaults").addEventListener("click", () => {
    defaultStarts().forEach((s) => opt.starts.push(s));
    renderOptStarts(); updateOptEstimate();
  });
  box.querySelectorAll("[data-edit]").forEach((b) => b.addEventListener("click", () => editStart(Number(b.dataset.edit))));
  box.querySelectorAll("[data-del]").forEach((b) => b.addEventListener("click", () => {
    opt.starts.splice(Number(b.dataset.del), 1);
    renderOptStarts(); updateOptEstimate();
  }));
  $("opt-start-add").addEventListener("click", () => addStart(snapshotState()));
  $("opt-start-blank").addEventListener("click", () => addStart(stateFromBuild({ mods: [] }, w.id)));
  const pick = $("opt-start-mine");
  if (pick) pick.addEventListener("change", () => {
    const p = mine[Number(pick.value)];
    if (p) addStart(JSON.parse(JSON.stringify(p.state)));
  });
}

/// Open start `i` in the builder.
function editStart(i) {
  const s = opt.starts[i];
  if (!s) return;
  flushPresetSaves();
  const w = $("weapon").value;
  startEdit = { idx: i, saved: snapshotState(), savedPreset: activePreset, fixed: new Set(s.fixed || []) };
  history.pushState({}, "", weaponPath(w));
  route();
  whileApplying(() => restoreState(s.build, w));
  renderStartEditBanner();
  decorateStartPins();
}

/// Close the start: `save` writes the builder's build and pins into it. Either
/// way the builder goes back to the player's own build.
function finishStartEdit(save) {
  if (!startEdit) return;
  const se = startEdit;
  const w = $("weapon").value;
  const build = save ? snapshotState() : null;
  startEdit = null;
  whileApplying(() => restoreState(se.saved, w));
  activePreset = se.savedPreset;
  renderPresetBar();
  renderStartEditBanner();
  decorateStartPins();
  history.pushState({}, "", `${weaponPath(w)}/optimizer`);
  route();
  // WRITTEN AFTER THE RESTORE: putting the player's build back re-applies the
  // weapon, which reads the search back from its preset — a start written
  // before that was read over.
  if (build) opt.starts[se.idx] = { build, fixed: [...se.fixed] };
  renderOptStarts(); updateOptEstimate();
}

function renderStartEditBanner() {
  const box = $("start-edit-banner");
  if (!box) return;
  const bar = $("preset-bar-builder-builds");
  if (bar) bar.hidden = !!startEdit;
  if (!startEdit) { box.hidden = true; box.innerHTML = ""; return; }
  box.hidden = false;
  box.innerHTML = `<div class="bh"><span class="n">★</span><h2>${escHtml(tr("Editing start"))} ${startEdit.idx + 1}</h2>`
    + `<span class="sub">${escHtml(tr("for the optimizer — your own build comes back when you close this"))}</span></div>`
    + `<div class="bb start-edit-row"><span class="sim-hint">${escHtml(tr("the pin beside a position FIXES it: this start's answer always keeps it, and the search never changes it"))}</span>`
    + `<span style="flex-grow:1"></span><button type="button" class="ghost-btn small" id="start-edit-discard">${escHtml(tr("discard"))}</button>`
    + `<button type="button" class="ghost-btn small start-edit-done" id="start-edit-done">${escHtml(tr("done — back to the optimizer"))}</button></div>`;
  $("start-edit-discard").addEventListener("click", () => finishStartEdit(false));
  $("start-edit-done").addEventListener("click", () => finishStartEdit(true));
}

/// THE PINS, put on after the builder draws — one decoration pass rather than
/// a pin written into five renderers. The keys are the search's positions.
function decorateStartPins() {
  const on = !!startEdit;
  document.querySelectorAll(".start-pin").forEach((p) => { if (!on) p.remove(); });
  document.querySelectorAll(".slot.fixed-pin").forEach((el) => { if (!on) el.classList.remove("fixed-pin"); });
  if (!on) return;
  const put = (host, key) => {
    if (!host) return;
    let pin = host.querySelector(`:scope > .start-pin[data-k="${key}"]`);
    if (!pin) {
      pin = document.createElement("button");
      pin.type = "button";
      pin.className = "start-pin";
      pin.dataset.k = key;
      pin.innerHTML = START_PIN_SVG;
      pin.addEventListener("click", (e) => {
        e.stopPropagation();
        if (startEdit.fixed.has(key)) startEdit.fixed.delete(key); else startEdit.fixed.add(key);
        decorateStartPins();
      });
      host.appendChild(pin);
    }
    const fixed = startEdit.fixed.has(key);
    pin.classList.toggle("on", fixed);
    pin.title = tr(fixed ? "fixed — click to free it" : "fix this position in the start");
    pin.setAttribute("aria-label", pin.title);
    if (host.classList.contains("slot")) host.classList.toggle("fixed-pin", fixed);
  };
  const kids = (id) => [...(($(id) || {}).children || [])];
  kids("mod-slots").forEach((el, i) => { if (i < 8) put(el, "mods:" + i); });
  kids("exilus").slice(0, 1).forEach((el) => put(el, "mods:8"));
  kids("arcane-slots").forEach((el, i) => put(el, "arcane:" + i));
  const head = (id, key) => { const b = $(id); if (b && !b.hidden) put(b.querySelector(".bh"), key); };
  head("evo-block", "evo:0");
  head("mode-block", "mode:0");
  head("element-block", "valence:0");
}

// A REDRAW TAKES THE PINS WITH IT, so they go back on after one — on the next
// frame, once, however many nodes the redraw touched.
let startPinFrame = 0;
new MutationObserver(() => {
  if (!startEdit || startPinFrame) return;
  startPinFrame = requestAnimationFrame(() => { startPinFrame = 0; decorateStartPins(); });
}).observe(document.documentElement, { childList: true, subtree: true });
