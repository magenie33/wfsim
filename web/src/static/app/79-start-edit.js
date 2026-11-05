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

/// The starts as `/api/optimize` reads them.
const startsPayload = () => opt.starts.map(startPayload);
const startPinned = (s, k) => (s.fixed || []).includes(k) || (k.startsWith("mods:") && (s.fixed || []).includes(k.replace("mods:", "slots:")));

/// A start as `/api/optimize` reads it (`headlessStartPayload`).
const startPayload = (s) => headlessStartPayload(META, s);

/// A RIVEN NO CARD OF THIS READER'S NAMES — the board's bare slot id, or a card
/// since deleted. In `stance` or the `mods` scope one refuses the whole search
/// ("unknown mod id: riven"), and a RESUMED body is whatever an older page
/// stored, so every body is passed through `nameableOptimizeBody` on its way
/// out. The stored start keeps the card: one not yet loaded reads the same.
const unnamedRiven = (id) => !!id && (splitRank(id)[0] === BOARD_RIVEN_SLOT || (isRivenId(id) && !modById(splitRank(id)[0])));
const unnamedRivenSlots = (b) => (b.slots || []).map((x, i) => (x && unnamedRiven(x.mod) ? i : -1)).filter((i) => i >= 0);
function nameableOptimizeBody(body) {
  const out = { ...body };
  if (unnamedRiven(out.stance)) out.stance = "";
  if (out.mods && typeof out.mods === "object" && !Array.isArray(out.mods)) {
    out.mods = Object.fromEntries(Object.entries(out.mods).filter(([id]) => !unnamedRiven(id)));
  }
  out.starts = (out.starts || []).map((s) => {
    const gone = new Set(((s && s.slots) || []).map((id, i) => (unnamedRiven(id) ? i : -1)).filter((i) => i >= 0));
    if (!gone.size) return s;
    return { ...s, slots: s.slots.map((id, i) => (gone.has(i) ? null : id)),
      fixed: (s.fixed || []).filter((f) => !(f.kind === "mods" && gone.has(f.idx))) };
  });
  return out;
}

/// THERE IS ALWAYS A START, the way the builder always holds a build: a new
/// search holds one BLANK start — the search fills it in its own order — and
/// removing the last start leaves a blank one.
const blankStart = () => ({ build: stateFromBuild({ mods: [] }, $("weapon").value), fixed: [] });

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
  if (!opt.starts.length) opt.starts.push(blankStart());
  const mine = loadPresetList(BUILDS).filter((p) => p.state && p.state.weapon === w.id);
  box.innerHTML =
    `<div class="opt-guide">${escHtml(tr("A start is a build the search begins from. Pin the choices it must keep. Each start keeps its own answer; final requirements apply to every result."))}</div>`
    + opt.starts.map((s, i) => `<div class="opt-start" data-i="${i}">
        <div class="opt-start-h"><b>${escHtml(tr("Start"))} ${i + 1}</b><span style="flex-grow:1"></span>
        <button type="button" class="ghost-btn small" data-edit="${i}">${escHtml(tr("edit in the builder"))}</button>
        <button type="button" class="ghost-btn small" data-del="${i}" aria-label="${escHtml(tr("remove this start"))}">✕</button></div>
        ${cardOfState(s.build, w, new Set(s.fixed))}${pinSummary(s)}${startConflictHtml(s)}
        <button type="button" class="ghost-btn small" data-final="${i}">${escHtml(tr("use these pins as final requirements"))}</button></div>`).join("")
    + `<div class="opt-start-add">`
    + `<button type="button" class="ghost-btn small" id="opt-start-add">${escHtml(tr("+ add the current build as a start"))}</button>`
    + `<button type="button" class="ghost-btn small" id="opt-start-blank">${escHtml(tr("+ a blank start"))}</button>`
    + (mine.length ? `<select id="opt-start-mine" class="ghost-btn small"><option value="">${escHtml(tr("+ add one of my builds"))}</option>`
      + mine.map((p, i) => `<option value="${i}">${escHtml(presetLabel(p))}</option>`).join("") + `</select>` : "")
    + `</div>`;
  box.querySelectorAll("[data-edit]").forEach((b) => b.addEventListener("click", () => editStart(Number(b.dataset.edit))));
  box.querySelectorAll("[data-final]").forEach((b) => b.addEventListener("click", () => {
    opt.final = JSON.parse(JSON.stringify(opt.starts[Number(b.dataset.final)]));
    renderOptFinal(); updateOptEstimate();
  }));
  box.querySelectorAll("[data-del]").forEach((b) => b.addEventListener("click", () => {
    opt.starts.splice(Number(b.dataset.del), 1);
    if (!opt.starts.length) opt.starts.push(blankStart());
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

/// What the limits do to a start, under its card: a pinned conflict in red,
/// what will be replaced in grey.
function startConflictHtml(s) {
  const unnamed = unnamedRivenSlots(s.build).length
    ? `<div class="opt-start-swap">${escHtml(tr("a riven you do not have is left out of this start — add it in Rivens to search with it"))}</div>` : "";
  if (!opt.limits) return unnamed;
  const c = startConflicts(s);
  return unnamed + (c.blocked.length ? `<div class="opt-start-clash">${escHtml(tr("pinned, and ruled out by the limits — this start cannot run:"))} ${c.blocked.map(escHtml).join(" · ")}</div>` : "")
    + (c.replaced.length ? `<div class="opt-start-swap">${escHtml(tr("ruled out by the limits, so the search replaces:"))} ${c.replaced.map(escHtml).join(" · ")}</div>` : "");
}

/// Open start `i` in the builder.
function editStart(i) {
  const s = i === -1 ? opt.final || blankStart() : opt.starts[i];
  if (!s) return;
  flushPresetSaves();
  const w = $("weapon").value;
  startEdit = { idx: i, weapon: w, saved: snapshotState(), savedPreset: activePreset, fixed: new Set(s.fixed || []) };
  history.pushState({}, "", weaponPath(w) + "/builder");
  route();
  whileApplying(() => restoreState(s.build, w));
  renderStartEditBanner();
  decorateStartPins();
}

/// Close the start: `save` writes the builder's build and pins into it. Either
/// way the builder goes back to the player's own build. `navigate` false when
/// the page is already on its way somewhere else (`leaveStartEdit`).
function finishStartEdit(save, navigate = true) {
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
  if (navigate) {
    history.pushState({}, "", `${weaponPath(w)}/optimizer`);
    route();
  }
  // WRITTEN AFTER THE RESTORE: putting the player's build back re-applies the
  // weapon, which reads the search back from its preset — a start written
  // before that was read over.
  if (build) {
    if (se.idx === -1) opt.final = { build, fixed: [...se.fixed] };
    else opt.starts[se.idx] = { build, fixed: [...se.fixed] };
  }
  renderOptFinal();
  renderOptStarts(); updateOptEstimate();
}

/// LEAVING THE BUILDER ENDS THE EDIT, as Done does. A start left open by a tab,
/// the back button or another weapon kept the builder in start mode — pins on,
/// the player's autosave suspended — wherever they went next.
function leaveStartEdit() {
  if (!startEdit) return;
  const here = decodeURIComponent(location.pathname).replace(/\/$/, "");
  if (here === decodeURIComponent(weaponPath(startEdit.weapon) + "/builder")) return;
  finishStartEdit(true, false);
}

function renderStartEditBanner() {
  const box = $("start-edit-banner");
  if (!box) return;
  const bar = $("preset-bar-builder-builds");
  if (bar) bar.hidden = !!startEdit;
  if (!startEdit) { box.hidden = true; box.innerHTML = ""; return; }
  box.hidden = false;
  box.innerHTML = `<div class="bh"><span class="n">★</span><h2>${startEdit.idx === -1 ? escHtml(tr("Final requirements")) : `${escHtml(tr("Editing start"))} ${startEdit.idx + 1}`}</h2>`
    + `<span class="sub">${escHtml(tr("for the optimizer — your own build comes back when you close this"))}</span></div>`
    + `<div class="bb start-edit-row"><span class="sim-hint">${escHtml(tr("Pin a MOD to require it, then choose Contains or Fixed slot."))}</span>`
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
  document.querySelectorAll(".start-placement").forEach((p) => { if (!on) p.remove(); });
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
        const strict = key.replace("mods:", "slots:");
        if (startEdit.fixed.has(key) || startEdit.fixed.has(strict)) { startEdit.fixed.delete(key); startEdit.fixed.delete(strict); }
        else startEdit.fixed.add(key.startsWith("mods:") && Number(key.split(":")[1]) < 8 && !slots[Number(key.split(":")[1])]?.mod ? strict : key);
        decorateStartPins();
      });
      host.appendChild(pin);
    }
    const strict = key.startsWith("mods:") && startEdit.fixed.has(key.replace("mods:", "slots:"));
    const fixed = startEdit.fixed.has(key) || strict;
    pin.classList.toggle("on", fixed);
    pin.title = tr(fixed ? "fixed — click to free it" : "fix this position in the start");
    pin.setAttribute("aria-label", pin.title);
    if (host.classList.contains("slot")) host.classList.toggle("fixed-pin", fixed);
    if (key.startsWith("mods:") && Number(key.split(":")[1]) < 8) {
      const idx = Number(key.split(":")[1]);
      let select = host.querySelector(":scope > .start-placement");
      if (!select) {
        select = document.createElement("select"); select.className = "start-placement";
        select.setAttribute("aria-label", tr("MOD pin type"));
        select.innerHTML = `<option value="contains">${escHtml(tr("Contains"))}</option><option value="slot">${escHtml(tr("Fixed slot"))} ${idx + 1}</option>`;
        select.addEventListener("click", e => e.stopPropagation());
        select.addEventListener("change", () => {
          startEdit.fixed.delete(key); startEdit.fixed.delete("slots:" + idx);
          startEdit.fixed.add(select.value === "slot" ? "slots:" + idx : key);
          decorateStartPins();
        });
        host.appendChild(select);
      }
      select.hidden = !fixed; select.value = strict ? "slot" : "contains";
    }
  };
  const kids = (id) => [...(($(id) || {}).children || [])];
  kids("mod-slots").forEach((el, i) => { if (i < 8) put(el, "mods:" + i); });
  kids("exilus").slice(0, 1).forEach((el) => put(el, "mods:8"));
  kids("arcane-slots").forEach((el, i) => put(el, "arcane:" + i));
  const head = (id, key) => { const b = $(id); if (b && !b.hidden) put(b.querySelector(".bh"), key); };
  kids("assembly-row").forEach((el, i) => put(el, "assembly:" + i));
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

function pinSummary(s) {
  const b = s.build || {}, w = weaponInfo(b.weapon || $("weapon").value) || {};
  const labels = (s.fixed || []).map(k => {
    const [kind, n] = k.split(":"), i = Number(n);
    if (kind === "mods" || kind === "slots") {
      const slot = (b.slots || [])[i];
      const name = slot?.mod ? requiredLabel(rankedId(slot.mod, slot.rank)) : tr("empty");
      return (kind === "slots" || !slot?.mod || i === 8 ? `${tr(i === 8 ? "Exilus" : "Fixed slot")} ${i < 8 ? i + 1 : ""}: ` : `${tr("Contains")}: `) + name;
    }
    if (kind === "assembly") {
      const spec = w.assembly || {}, part = i ? spec.component || "loader" : "grip";
      return ((spec[part + "s"] || []).find(p => p.id === b.assembly?.[part]) || {}).name || b.assembly?.[part] || "";
    }
    if (kind === "arcane") return arcName((b.arcane || [])[i] || "none");
    if (kind === "mode") return modeLabel(w, b.mode || defaultMode(w.id, null));
    if (kind === "valence") return DT(b.valence?.element || "");
    return tr("Evolutions") + ": " + Object.values(b.evoSel || {}).filter(Boolean).map(evoName).join(" · ");
  });
  return `<div class="opt-pin-summary">${labels.filter(Boolean).map(label => `<span class="pchip sel">${escHtml(label)}</span>`).join("")}</div>`;
}

function renderOptFinal() {
  const box = $("opt-final");
  if (!box) return;
  const final = opt.final;
  box.innerHTML = `<p class="sim-hint">${escHtml(tr("Every final build must satisfy these conditions before the top results are selected."))}</p>`
    + (final?.fixed.length ? pinSummary(final) : `<p class="sim-hint">${escHtml(tr("No final requirements"))}</p>`)
    + `<div class="opt-start-add"><button type="button" class="ghost-btn small" id="opt-final-edit">${escHtml(tr("edit in the builder"))}</button>`
    + (final?.fixed.length ? `<button type="button" class="ghost-btn small" id="opt-final-clear">${escHtml(tr("Clear"))}</button>` : "") + `</div>`;
  $("opt-final-edit").onclick = () => editStart(-1);
  if ($("opt-final-clear")) $("opt-final-clear").onclick = () => { opt.final = null; renderOptFinal(); updateOptEstimate(); };
  lockBuiltinSearch();
}

let optCompatibility = null, optCompatibilityTimer = null, optCompatibilityPending = "";
const optCompatibilityKey = () => JSON.stringify(nameableOptimizeBody(optimizeBody()));
const optCompatibilityBlocked = () => optCompatibility?.key === optCompatibilityKey() && !optCompatibility.result?.compatible;
function renderOptCompatibility() {
  const box = $("opt-compatibility");
  if (!box || !META) return;
  const current = optCompatibility?.key === optCompatibilityKey() ? optCompatibility.result : null;
  box.innerHTML = current ? (current.starts || []).map(s => `<div class="${s.compatible ? "ok" : "opt-start-clash"}">${escHtml(tr("Start"))} ${s.start + 1} · ${
    escHtml(s.compatible ? tr("compatible with final requirements") : tf(s.error || tr("The search conditions are incompatible.")))}</div>`).join("")
    : `<span class="sim-hint">${escHtml(tr("Checking compatibility…"))}</span>`;
  if (current?.error && !(current.starts || []).some(s => !s.compatible)) box.innerHTML += `<div class="opt-start-clash">${escHtml(tf(current.error))}</div>`;
  if ($("run-opt") && current && !current.compatible) $("run-opt").disabled = true;
}
function scheduleOptCompatibility() {
  if (!META || !opt.limits || startEdit) return;
  const key = optCompatibilityKey();
  if (optCompatibility?.key === key || optCompatibilityPending === key) { renderOptCompatibility(); return; }
  clearTimeout(optCompatibilityTimer);
  optCompatibilityPending = key;
  renderOptCompatibility();
  optCompatibilityTimer = setTimeout(async () => {
    let result;
    try {
      result = await api("/api/optimize/check", JSON.parse(key));
      if (!result || result.ok === false) result = {compatible:false,starts:[{start:0,compatible:false,error:result?.error || tr("The search conditions are incompatible.")}]};
    }
    catch (e) { result = { compatible: false, starts: [{start: 0, compatible: false, error: String(e.message || e)}] }; }
    if (optCompatibilityPending !== key) return;
    optCompatibilityPending = "";
    optCompatibility = { key, result };
    if (optCompatibilityKey() !== key) return;
    renderOptCompatibility();
    $("run-opt").disabled = !!optimizationTaskForPage() || !!startsBlocked();
  }, 250);
}
