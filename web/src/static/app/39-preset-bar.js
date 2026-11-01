// The shared quick switcher for every preset domain. Edits autosave; creating
// and duplicating allocate a numeric label, while info reads without applying.
let deleteArmed = null; // "<bar>:<preset>" whose delete has said who links it

// SELECT and COPY, lifted out of the bar so the BENCHMARK bar performs the
// same two actions rather than its own versions of them.
// They are the only two a read-only entry has, and "the copy is an ordinary
// editable preset" has to stay one behaviour — a second implementation is how
// one bar's copy comes to capture something the other's does not.
/// WHAT THE ACTIVE POINTER STORES, and it is not the label. An official entry's
/// NAME is a rank inside one ruler — "#1 · Incarnon cycle" — so the aimed board
/// and the no-aim board each have one, and `find(x => x.name === n)` returned
/// whichever came first. That is the whole of the bug where the no-aim board's
/// leader opened the AIMED board's leader instead.
///
/// `builtin` is already unique per ruler, mode and rank; a preset of your own
/// has its `id`. So this is the identity, and `presetLabel` is what a reader
/// sees — a label two entries may share.
const presetId = (p) => (p || {}).builtin || (p || {}).id || "";
const presetLabel = (p) => (p || {}).name || "";
/// THE CLOUD ON A CHIP, while the account syncs: filled, the entry is on every
/// browser signed in to it; hollow, on this one only. A click switches it.
/// Signed out where accounts exist, it is the hollow cloud as a link to sign
/// in: the feature shown where it would be used, never a prompt.
const CLOUD_SVG = `<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path d="M7 18h10a4 4 0 0 0 .6-7.96A6 6 0 0 0 6.1 9.2 4.5 4.5 0 0 0 7 18z"/></svg>`;
function cloudMark(domain, p) {
  if (typeof accountState === "undefined" || !p.id) return "";
  if (!accountState.account) return accountState.providers.length
    ? `<a class="pop pcloud" href="/login?return=${encodeURIComponent(location.pathname + location.search)}" title="${escHtml(tr(
      "on this browser only - sign in to sync it to every browser you use"))}">${CLOUD_SVG}</a>`
    : "";
  if (syncStatus.state === "not_included" || syncStatus.state === "other") return "";
  const on = isCloudSynced(p);
  return `<button class="pop pcloud ${on ? "on" : ""}" data-cloud="${escHtml(p.id)}" aria-pressed="${on}" title="${escHtml(tr(on
    ? "synced to your account - click to keep it on this browser only"
    : "on this browser only - click to sync it to your account"))}">${CLOUD_SVG}</button>`;
}

/// EVERY ENTRY A BAR CAN OPEN: the reader's own, and what its collection
/// publishes (an official ruler, a board build), which are read-only.
const barEntries = (cfg) => cfg.load().concat(cfg.published ? cfg.published() : []);

// ---- published entries opened into a bar ----------------------------------------------
//
// A BAR WHOSE COLLECTION PUBLISHES ENTRIES keeps the ones opened into it — docs/
// UI.md §"Presets and customs". `cfg.pins` says where, and how an entry is
// known: by what it IS (`ref`, compared by `same`), never by a rank a rescore
// renumbers or a position. The open one is always among them, however it was
// opened. Nothing is pruned on a miss: a source not loaded yet misses them all.
const pinRefs = (cfg) => {
  try { return JSON.parse(localStorage.getItem(cfg.pins.key()) || "[]"); } catch (_) { return []; }
};
const storePinRefs = (cfg, refs) => {
  try { localStorage.setItem(cfg.pins.key(), JSON.stringify(refs)); } catch (_) { /* a private window keeps none */ }
};
function rememberPublished(cfg, p) {
  const ref = cfg.pins.ref(p);
  const refs = pinRefs(cfg);
  if (!refs.some((r) => cfg.pins.same(r, ref))) storePinRefs(cfg, refs.concat([ref]));
}
/// The published entries in the bar, in the order they were opened.
function openedPublished(cfg) {
  const all = cfg.published();
  const act = all.find((p) => presetId(p) === cfg.active());
  if (act) rememberPublished(cfg, act);
  const out = [];
  for (const ref of pinRefs(cfg)) {
    const p = all.find((x) => cfg.pins.same(cfg.pins.ref(x), ref));
    if (p && !out.includes(p)) out.push(p);
  }
  return out;
}
/// × ON A PUBLISHED ENTRY: out of the bar, and off the page if it was the one
/// open — onto your first entry, else the collection's default (`cfg.fallback`,
/// or the blank), which is exactly what deleting your last one leaves.
function unpinPublished(cfg, id) {
  const p = cfg.published().find((x) => presetId(x) === id);
  if (p) storePinRefs(cfg, pinRefs(cfg).filter((r) => !cfg.pins.same(r, cfg.pins.ref(p))));
  if (id === cfg.active()) {
    const next = cfg.load()[0] || (cfg.fallback ? cfg.fallback() : null);
    cfg.setActive(next ? presetId(next) : "");
    whileApplying(() => cfg.apply(next ? next.state : cfg.blank()));
    if (!next && cfg.pristine) cfg.pristine();
  }
  cfg.rerender();
}

const pickPreset = (cfg, key) => {
  flushPresetSaves();
  const p = presetFind(barEntries(cfg), key);
  if (!p || presetId(p) === cfg.active()) return;
  if (p.builtin && cfg.pins) rememberPublished(cfg, p);
  // A BOARD ROW is the one built-in that carries its ruler; a scenario does not.
  if (p.builtin && p.benchmark) track("board.open", $("weapon").value);
  cfg.setActive(presetId(p));
  notePresetUse(cfg.domain, presetId(p));
  whileApplying(() => cfg.apply(p.state)); // a load is not an edit
  cfg.rerender();
};

/// Activate before applying, so renderers observe the new identity. Capture
/// the realised blank after apply, exactly as the editor shows it.
const newPreset = (cfg) => {
  flushPresetSaves();
  const ps = cfg.load();
  const e = presetEntry(newPresetName(ps, cfg.domain), null);
  cfg.setActive(e.id);
  notePresetUse(cfg.domain, e.id);
  whileApplying(() => cfg.apply(cfg.blank()));
  // THE BLANK, SEEN: what "edited back to the blank" is compared against.
  if (cfg.pristine) cfg.pristine();
  e.state = cfg.snapshot();
  ps.push(e);
  cfg.store(ps);
  cfg.rerender();
  return e.id;
};

/// A PRESET EDITED BACK TO THE BLANK IS DELETED: by CONTENT it is the default
/// again, and there is nothing of the reader's left in it. Only an EDIT does it —
/// the stored state was not blank and the live one is — so a blank one made by
/// "+ new" stays until it has been worked on. `cfg.isBlank(state)` says whether
/// a state is the blank; a collection without it keeps what it has. True when it
/// deleted, so the caller writes nothing.
function deleteIfBlank(cfg, stored) {
  if (!cfg.isBlank || !cfg.isBlank(cfg.snapshot()) || cfg.isBlank(stored)) return false;
  const ps = cfg.load();
  const at = ps.findIndex((p) => presetId(p) === cfg.active());
  if (at < 0) return false;
  ps.splice(at, 1);
  cfg.store(ps);
  cfg.setActive("");
  cfg.rerender();
  return true;
}

// The copy captures the LIVE editor state and becomes the active document; the
// original keeps what auto-save last wrote into it. For a read-only entry the
// live state IS that entry, because selecting it is what put it there.
const copyActivePreset = (cfg) => {
  flushPresetSaves();
  const ps = cfg.load();
  const state = cfg.copyState ? cfg.copyState(cfg.snapshot()) : cfg.snapshot();
  const e = presetEntry(newPresetName(ps, cfg.domain), state);
  ps.push(e);
  cfg.store(ps);
  cfg.setActive(e.id);
  notePresetUse(cfg.domain, e.id);
  if (cfg.copyState) whileApplying(() => cfg.apply(e.state));
  cfg.rerender();
  return e.id;
};

/// A READ-ONLY ENTRY ON SCREEN, one behaviour for every collection: the region
/// shows not-allowed, and a click anywhere in it makes your own copy, which is
/// then what you edit. A disabled control swallows its click and says nothing,
/// which is how a locked panel reads as broken. Its controls are disabled too,
/// so the keyboard cannot edit what the mouse cannot; one marked
/// `data-view-only` only narrows what is shown and stays live.
function lockReadOnly(el, on, cfg) {
  if (!el) return;
  el.classList.toggle("locked-hard", on);
  el.querySelectorAll("input,select,button,textarea").forEach((c) => {
    if (c.hasAttribute("data-view-only")) return;
    // ONLY UNLOCK WHAT THIS LOCKED: a control disabled for its own reason stays so.
    if (on) {
      if (!c.disabled) { c.disabled = true; c.dataset.roLock = "1"; }
    } else if (c.dataset.roLock) {
      c.disabled = false;
      delete c.dataset.roLock;
    }
  });
  if (el._roFork) {
    el.removeEventListener("click", el._roFork, true);
    el.removeAttribute("title");
    el._roFork = null;
  }
  if (!on) return;
  el.title = tr("read-only — click to edit your own copy");
  // CAPTURE, and nothing after it: the region's own handlers would otherwise
  // act on the read-only entry in the same click.
  el._roFork = (e) => {
    if (e.target.closest && e.target.closest("[data-view-only]")) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    copyActivePreset(typeof cfg === "function" ? cfg() : cfg);
  };
  el.addEventListener("click", el._roFork, true);
}

function renderPresetBarIn(bar, cfg) {
  const noun = cfg.noun || "preset";
  const ps = cfg.load(), ro = cfg.pins ? openedPublished(cfg) : [];
  const entries = ps.concat(ro), active = cfg.active();
  const current = presetFind(entries, active);
  const items = () => {
    const names = presetSearchNames(), recent = presetRecentUses(cfg.domain), selected = cfg.active();
    const choices = cfg.load().concat(cfg.pins ? openedPublished(cfg) : []).sort((a,b) =>
      Number(presetId(b) === selected) - Number(presetId(a) === selected)
      || (recent[presetId(b)] || b.savedAt || 0) - (recent[presetId(a)] || a.savedAt || 0));
    return choices.map(p => {
      const by = cfg.usedBy ? cfg.usedBy(p) : [];
      const links = by.length ? `<span class="pby" title="${escHtml(tr('linked by') + ': ' + by.join(' · '))}">↩${by.length}</span>` : "";
      return {value:presetId(p), label:p.name, hint:presetSummary(cfg, p, names),
        badge:(p.builtin ? LOCK_SVG : "") + links, searchText:presetSearchText(cfg, p, names), info:() => cfg.info(p)};
    });
  };
  const switcher = ddButton(`dd-preset-${cfg.domain}`, {
    value: current ? presetId(current) : "", placeholder:tr("Default"), search:true, disabled:!entries.length, limit:8,
    title:tr("Search presets"), get items() { return items(); },
    onPick:id => pickPreset(cfg, id),
    footer:{label:tr("Manage presets") + ` · ${ps.length}`, onClick:() => openPresetLibrary(cfg, bar)},
  });
  const ownOps = current && !current.builtin
    ? `<button type="button" class="pop dup">⧉ ${escHtml(tr("duplicate"))}</button><button type="button" class="pop ren">✎ ${escHtml(tr("rename"))}</button><button type="button" class="pop del">× ${escHtml(tr("delete"))}</button>` : "";
  const roOps = current?.builtin
    ? `<button type="button" class="pop dup">⧉ ${escHtml(tr("duplicate"))}</button><button type="button" class="pop unpin" data-unpin="${escHtml(presetId(current))}">× ${escHtml(tr("take it out of the bar — the board keeps it"))}</button>` : "";
  const published = cfg.openable ? `<span class="popen">${cfg.boardPicker
    ? `<button type="button" class="ghost-btn small" data-board-open>+ ${escHtml(cfg.openLabel)}</button>`
    : ddButton(`dd-open-${cfg.domain}`, {value:"", search:true, placeholder:`+ ${cfg.openLabel}`,
      items:cfg.openable().map(p => ({value:presetId(p), label:p.group || p.name})), onPick:id => pickPreset(cfg, id)})}</span>` : "";
  const usedBy = current && cfg.usedBy ? cfg.usedBy(current) : [];
  bar.innerHTML = `<span class="plabel">${cfg.label}</span><span class="preset-current ${current ? `pchip sel${current.builtin ? ' ro' : ''}` : 'pnote'}"${current ? ` data-name="${escHtml(presetId(current))}"` : ''}>`
    + (current ? current.builtin ? LOCK_SVG : cloudMark(cfg.domain, current) : "")
    + switcher + (current ? presetInfoButton(cfg, current) : "")
    + (usedBy.length ? `<span class="pby" title="${escHtml(tr("linked by") + ': ' + usedBy.join(' · '))}">↩${usedBy.length}</span>` : "")
    + ((ownOps || roOps || published || cfg.extra) ? presetActionsHtml(ownOps + roOps + published + (cfg.extra || "")) : "") + `</span>`
    + `<button type="button" class="pchip add" title="${escHtml(tr("new empty {thing}").replace('{thing}',tr(noun)))}">＋</button>`
    + `<button type="button" class="ghost-btn small" data-preset-manage>${escHtml(tr("Manage presets"))} <span class="preset-count">${ps.length}</span></button>`
    + undoButtons(cfg.domain) + `<div class="pshare" hidden></div>`;
  wireUndoButtons(bar, cfg.domain);
  if (cfg.onExtra) cfg.onExtra(bar);
  wirePresetInfo(bar, cfg, entries); wirePresetActions(bar);
  bar.querySelector("[data-preset-manage]").onclick = () => openPresetLibrary(cfg, bar);
  const boardOpen = bar.querySelector("[data-board-open]");
  if (boardOpen) boardOpen.onclick = event => { event.stopPropagation(); cfg.boardPicker(boardOpen); };
  bar.querySelectorAll(".pcloud[data-cloud]").forEach(button => button.onclick = event => {
    event.stopPropagation();
    if (setCloudSync(presetListKey(cfg.domain), button.dataset.cloud, !button.classList.contains("on"))) cfg.rerender();
  });
  // No prompt()/alert()/confirm() anywhere — the browser can block those
  // dialogs, which made saving silently fail. Naming
  // happens in an INLINE input: Enter commits, Esc cancels.
  const nameInput = (placeholderEl, initial, onCommit) => {
    placeholderEl.outerHTML = `<input class="pname" type="text" value="${escHtml(initial)}" placeholder="name, then Enter…" maxlength="80">`;
    const inp = bar.querySelector(".pname");
    inp.focus();
    if (initial) inp.select();
    let done = false;
    const commit = () => {
      if (done) return;
      done = true;
      onCommit((inp.value || "").trim());
    };
    inp.addEventListener("keydown", (ev) => {
      if (imeComposing(ev)) return;
      if (ev.key === "Enter") commit();
      if (ev.key === "Escape") { done = true; cfg.rerender(); }
    });
    inp.addEventListener("blur", commit);
  };
  const addBtn = bar.querySelector(".pchip.add");
  addBtn.addEventListener("click", (e) => { e.stopPropagation(); newPreset(cfg); });
  const on = (sel, fn) => { const b = bar.querySelector(sel); if (b) b.addEventListener("click", (e) => { e.stopPropagation(); fn(); }); };
  on(".pop.dup", () => copyActivePreset(cfg));
  bar.querySelectorAll(".pop.unpin").forEach((b) => b.addEventListener("click", (e) => {
    e.stopPropagation();
    cfg.unpin(b.dataset.unpin);
  }));
  on(".pop.ren", () => {
    const chipEl = bar.querySelector(".pchip.sel");
    if (!chipEl) return;
    const cur = presetFind(cfg.load(), cfg.active());
    nameInput(chipEl, presetLabel(cur), (name) => {
      const ps2 = cfg.load();
      // A LABEL EDIT AND NOTHING ELSE: everything points at the id, so two
      // entries may share a name. Empty or unchanged just cancels.
      const at = ps2.findIndex((p) => presetId(p) === cfg.active());
      if (name && at >= 0 && name !== ps2[at].name) {
        ps2[at].name = name;
        cfg.store(ps2);
      }
      cfg.rerender();
    });
  });

  on(".pop.del", () => {
    // A PRESET OTHERS LINK IS DELETED ON THE SECOND CLICK, and the first says
    // who is affected: their links land on the default, so their numbers move.
    // Inline, because a native dialog is blocked.
    const open = cfg.usedBy && cfg.load().find((p) => presetId(p) === cfg.active());
    const users = open ? cfg.usedBy(open) : [];
    if (users.length && deleteArmed !== `${bar.id}:${cfg.active()}`) {
      deleteArmed = `${bar.id}:${cfg.active()}`;
      bar.querySelector(".pop.del").textContent = `✕ ${tr("{n} linked — they return to the default; click again").replace("{n}", users.length)}`;
      return;
    }
    deleteArmed = null;
    deleteOwnedPreset(cfg, cfg.active());
  });
}

function deleteOwnedPreset(cfg, id) {
  flushPresetSaves();
  const ps = cfg.load().filter(p => presetId(p) !== id);
  cfg.store(ps);
  if (id === cfg.active()) {
    const next = ps[0] || (cfg.fallback ? cfg.fallback() : null);
    cfg.setActive(next ? presetId(next) : "");
    whileApplying(() => cfg.apply(next ? next.state : cfg.blank()));
    if (!ps.length && cfg.pristine) cfg.pristine();
  }
  cfg.rerender();
  dropSave("builds"); dropSave("search");
}

// An EMPTY build for "+ new": the CURRENT weapon (the page is a weapon
// page — a new preset should not navigate away), bare slots, no arcane, no
// evolutions. NO scenario: a build does not carry a fight, so making one
// cannot reset the fight you are in.
function blankBuildState() {
  return buildState($("weapon").value, {
    evoSel: {},
    arcane: ["none"],
    arcaneRank: [null],
    slots: [],
    // THE ARSENAL'S OWN, said out loud. A new build is played the way the
    // weapon comes, and a Lich weapon comes carrying an element — which is
    // what `defaultMode`/`defaultValence` answer when handed nothing. Named
    // rather than omitted, because an omission is what a producer that forgot
    // an axis also looks like (`BUILD_AXES`).
    // `null`, not `undefined` — see `stateFromBuild`: the two mean the same
    // thing to `restoreState` and only one of them survives being saved.
    mode: null,
    valence: null,
    // …and a modular weapon comes assembled — the derived default, which is
    // also what the server uses for a request that names none, so a brand new
    // build and a blank request describe the same weapon.
    assembly: null,
    // …and held by whoever holds this weapon when nobody is named: the Prototype,
    // or a locked weapon's own frame (`defaultWielder`).
    wielder: null,
  });
}

// Preset info reads saved state; browsing never applies it to a live editor.
const presetInfoButton = (cfg, p) => cfg.info
  ? `<button type="button" class="pop pinfo" data-preset-info="${escHtml(presetId(p))}" title="${escHtml(tr("Info"))}" aria-label="${escHtml(tr("Info") + ': ' + p.name)}">ⓘ</button>` : "";
const presetInfoSection = (head, labels) => `<div class="sb-h">${escHtml(tr(head))}</div><div class="sb-chips">${labels.filter(Boolean)
  .map(label => `<span class="sb-chip"><span>${escHtml(label)}</span></span>`).join("") || `<span class="sb-empty">${escHtml(tr("none selected"))}</span>`}</div>`;
function renderPresetInfo(host, cfg, p) {
  host.innerHTML = cfg.info(p);
  if (cfg.prepareInfo) cfg.prepareInfo().then(() => {
    if (host.isConnected) host.innerHTML = cfg.info(p);
  }).catch(() => {});
}
const presetContext = (cfg, p) => cfg.context ? cfg.context(p) : tr("Global");
function wirePresetInfo(host, cfg, entries) {
  host.querySelectorAll("[data-preset-info]").forEach(button => button.onclick = event => {
    event.preventDefault(); event.stopPropagation();
    const p = entries.find(p => presetId(p) === button.dataset.presetInfo);
    if (!p) return;
    closePopovers();
    let pop = $("preset-info-popover");
    if (!pop) {
      pop = document.createElement("div"); pop.id = "preset-info-popover";
      pop.className = "popover preset-info-popover"; pop.setAttribute("role", "dialog");
      document.body.append(pop);
    }
    pop.setAttribute("aria-label", tr("Info") + ": " + p.name);
    pop.innerHTML = `<div class="preset-info-head"><b>${escHtml(p.name)}</b><span>${escHtml(presetContext(cfg, p))}</span><button class="ghost-btn small" data-info-close aria-label="${escHtml(tr("close"))}">×</button></div>`
      + `<div class="preset-info-body"></div>`;
    renderPresetInfo(pop.querySelector(".preset-info-body"), cfg, p);
    place(pop, button);
    pop.querySelector("[data-info-close]").onclick = () => { pop.hidden = true; button.focus({preventScroll:true}); };
    pop.querySelector("[data-info-close]").focus({preventScroll:true});
  });
}

function presetSearchNames() {
  const names = new Map();
  const add = values => (Array.isArray(values) ? values : []).forEach(x => { if (x?.id) names.set(x.id, x.name || x.id); });
  Object.values(META.mod_pools || {}).forEach(add);
  (META.weapons || []).forEach(w => (w.evolutions || []).forEach(t => add(t.options)));
  [META.weapons, META.enemies, META.warframes, META.companions, META.abilities, META.buffs, META.arcanes].forEach(add);
  if (WFCAT) Object.values(WFCAT).filter(Array.isArray).forEach(add);
  COLLECTIONS.filter(c => c.kind === "custom").forEach(c => add(loadPresetWhole(c.domain)));
  return names;
}
function presetSearchText(cfg, p, names) {
  const strings = [];
  const walk = value => {
    if (typeof value === "string") strings.push(value, names.get(value.split("@")[0]) || "", names.get(value.replace(/^riven:/, "")) || "");
    else if (typeof value === "number" || typeof value === "boolean") strings.push(String(value));
    else if (value && typeof value === "object") Object.values(value).forEach(walk);
  };
  walk(p.state);
  return [p.name, presetContext(cfg, p), ...strings].join(" ").replace(/_/g, " ").toLocaleLowerCase();
}

let presetLibraryWindow = null;
function openPresetLibrary(cfg, anchor) {
  flushPresetSaves(); closePopovers();
  if (presetLibraryWindow) presetLibraryWindow.close();
  const overlay = document.createElement("div"); overlay.className = "preset-library-window";
  overlay.innerHTML = `<section class="preset-library-dialog" role="dialog" aria-modal="true" aria-label="${escHtml(tr("Manage presets"))}"><header class="preset-library-head"><div><b>${escHtml(tr("My presets"))} · ${escHtml(cfg.label)}</b><p>${escHtml(tr("Saved automatically. Search names and contents; info shows what each preset contains."))}</p></div><button class="ghost-btn" data-library-close>${escHtml(tr("close"))} · Esc</button></header><div class="preset-library-tools"></div><div class="preset-library-results"></div></section>`;
  const scope = cfg.libraryScope ? cfg.libraryScope() : entryScope(cfg.domain);
  const state = { query: "", sort: "recent", scope: "current", source: "mine", visible: 30 };
  const close = () => {
    overlay.remove(); document.body.classList.remove("preset-library-open"); presetLibraryWindow = null;
    closePopovers(); if (anchor.isConnected) anchor.querySelector("[data-preset-manage]")?.focus({preventScroll:true});
  };
  presetLibraryWindow = { close, render: () => render() };
  document.body.append(overlay); document.body.classList.add("preset-library-open");
  overlay.querySelector("[data-library-close]").onclick = close;
  overlay.onclick = event => { if (event.target === overlay) close(); };
  overlay.addEventListener("keydown", event => {
    if (imeComposing(event)) return;
    if (event.key === "Escape" && !document.querySelector('.popover:not([hidden])')) { event.preventDefault(); event.stopPropagation(); close(); }
    if (event.key !== "Tab" || document.querySelector('.popover:not([hidden])')) return;
    const nodes = [...overlay.querySelectorAll('button,input,a[href],summary,[tabindex="0"]')].filter(x => !x.disabled && x.offsetParent !== null);
    if (event.shiftKey && document.activeElement === nodes[0]) { event.preventDefault(); nodes.at(-1)?.focus(); }
    else if (!event.shiftKey && document.activeElement === nodes.at(-1)) { event.preventDefault(); nodes[0]?.focus(); }
  }, true);
  const tools = overlay.querySelector(".preset-library-tools"), results = overlay.querySelector(".preset-library-results");
  let currentRows = [], armed = "";
  const expanded = new Set();
  const current = p => scope === null || (p.scope || scope) === scope;
  const store = (p, entries) => storePresetList(cfg.domain, entries, p.scope || scope);
  const refresh = () => { cfg.rerender(); render(); };
  const open = p => {
    if (current(p)) { close(); pickPreset(cfg, presetId(p)); return; }
    localStorage.setItem(presetActiveKey(cfg.domain, p.scope), presetId(p));
    const url = cfg.libraryPath(p);
    close(); location.assign(url);
  };
  function renderRows() {
    const own = state.scope === "all" ? loadPresetWhole(cfg.domain) : cfg.load();
    const published = cfg.pins ? openedPublished(cfg) : [];
    const source = state.source === "published" ? published : state.source === "all" ? own.concat(published) : own;
    const terms = state.query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
    const names = presetSearchNames();
    currentRows = source.filter(p => {
      if (!terms.length) return true;
      const text = presetSearchText(cfg, p, names);
      return terms.every(term => text.includes(term));
    });
    currentRows.sort((a, b) => (state.sort === "name" ? a.name.localeCompare(b.name, LANG, {numeric:true})
      : (Number(b.savedAt) || 0) - (Number(a.savedAt) || 0)) || presetId(a).localeCompare(presetId(b)));
    if (state.sort === "oldest") currentRows.reverse();
    results.innerHTML = `<div class="preset-library-count">${currentRows.length} / ${source.length}</div>`
      + currentRows.slice(0, state.visible).map(p => `<article class="preset-library-item${presetId(p) === cfg.active() ? ' active' : ''}" data-entry="${escHtml(presetId(p))}"><div class="preset-library-row"><div><b>${escHtml(p.name)}</b><span class="preset-library-context">${escHtml(presetContext(cfg, p))}${p.builtin ? ' · ' + escHtml(tr("read-only")) : ''}</span><p class="preset-library-summary" title="${escHtml(presetSummary(cfg,p,names))}">${escHtml(presetSummary(cfg,p,names))}</p><time>${p.savedAt ? escHtml(new Date(p.savedAt).toLocaleString(LANG)) : ''}</time></div><div class="preset-library-actions">${p.builtin ? '' : cloudMark(cfg.domain, p)}<button class="ghost-btn small" data-library-open>${escHtml(tr("Open"))}</button>${presetActionsHtml((current(p) ? `<button class="ghost-btn small" data-library-copy>⧉ ${escHtml(tr("duplicate"))}</button>` : "") + (p.builtin ? "" : `<button class="ghost-btn small" data-library-rename>✎ ${escHtml(tr("rename"))}</button><button class="ghost-btn small" data-library-delete>× ${escHtml(tr("delete"))}</button>`))}</div></div><details class="preset-library-info"${expanded.has(presetId(p)) ? " open" : ""}><summary>${escHtml(tr("Info"))}</summary><div class="preset-info-body"></div></details><div class="preset-library-feedback"></div></article>`).join("")
      + (!currentRows.length ? `<p class="sb-empty">${escHtml(tr("No matching presets"))}</p>` : "")
      + (currentRows.length > state.visible ? `<button class="ghost-btn" data-library-more>${escHtml(tr("Show more"))}</button>` : "");
    results.querySelectorAll("[data-entry]").forEach(row => {
      const p = currentRows.find(p => presetId(p) === row.dataset.entry);
      row.querySelector("[data-library-open]").onclick = () => open(p);
      const details = row.querySelector(".preset-library-info");
      const fillInfo = () => { if (details.open && !details.dataset.loaded) { renderPresetInfo(details.querySelector(".preset-info-body"), cfg, p); details.dataset.loaded = "1"; } };
      details.ontoggle = () => { if (details.open) expanded.add(presetId(p)); else expanded.delete(presetId(p)); fillInfo(); };
      fillInfo();
      const copy = row.querySelector("[data-library-copy]");
      if (copy) copy.onclick = () => { pickPreset(cfg, presetId(p)); copyActivePreset(cfg); refresh(); };
      const rename = row.querySelector("[data-library-rename]");
      if (rename) rename.onclick = () => {
        const feedback = row.querySelector(".preset-library-feedback");
        feedback.innerHTML = `<form class="preset-library-rename"><input aria-label="${escHtml(tr("name"))}" maxlength="80" value="${escHtml(p.name)}"><button class="ghost-btn small">${escHtml(tr("Apply"))}</button><button type="button" class="ghost-btn small" data-cancel>${escHtml(tr("cancel"))}</button></form>`;
        feedback.querySelector("[data-cancel]").onclick = () => { feedback.innerHTML = ""; };
        const input = feedback.querySelector("input"); input.focus(); input.select();
        feedback.querySelector("form").onsubmit = event => {
          event.preventDefault(); const name = input.value.trim(); if (!name) return;
          const list = loadPresetList(cfg.domain, p.scope || scope).map(x => x.id === p.id ? {...x, name} : x);
          store(p, list); refresh();
        };
      };
      const del = row.querySelector("[data-library-delete]");
      if (del) del.onclick = () => {
        if (armed !== p.id) {
          armed = p.id;
          const users = cfg.usedBy ? cfg.usedBy(p) : [];
          row.querySelector(".preset-library-feedback").textContent = users.length ? tr("linked by") + ': ' + users.join(' · ') + ' · ' + tr("{n} linked — they return to the default; click again").replace('{n}', users.length) : tr("Click delete again to remove this preset.");
          return;
        }
        armed = "";
        if (current(p)) { deleteOwnedPreset(cfg, p.id); expanded.add(cfg.active()); render(); }
        else { store(p, loadPresetList(cfg.domain, p.scope || scope).filter(x => x.id !== p.id)); refresh(); }
      };
      const cloud = row.querySelector("[data-cloud]");
      if (cloud) cloud.onclick = () => { setCloudSync(presetListKey(cfg.domain), p.id, !cloud.classList.contains("on")); refresh(); };
    });
    wirePresetActions(results);
    const more = results.querySelector("[data-library-more]");
    if (more) more.onclick = () => { state.visible += 30; renderRows(); };
  }
  results.onscroll = () => {
    if (currentRows.length > state.visible && results.scrollTop + results.clientHeight >= results.scrollHeight - 180) {
      const at = results.scrollTop; state.visible += 30; renderRows(); results.scrollTop = at;
    }
  };
  function render() {
    tools.innerHTML = `<div class="addbar preset-library-search"><input type="search" aria-label="${escHtml(tr("Search presets"))}" placeholder="${escHtml(tr("Search names, equipment and settings…"))}" value="${escHtml(state.query)}"></div>`
      + ddButton("preset-library-sort", { value:state.sort, items:[{value:"recent",label:tr("Recently modified")},{value:"name",label:tr("Name")},{value:"oldest",label:tr("Oldest first")}], onPick:value => {state.sort=value; render();} })
      + (scope !== null && cfg.libraryPath ? ddButton("preset-library-scope", {value:state.scope, items:[{value:"current",label:tr("Current item")},{value:"all",label:tr("All items")}],onPick:value=>{state.scope=value;state.visible=30;render();}}) : "")
      + (cfg.pins ? ddButton("preset-library-source",{value:state.source,items:[{value:"mine",label:tr("Mine")},{value:"published",label:cfg.roGroup},{value:"all",label:tr("All")}],onPick:value=>{state.source=value;state.visible=30;render();}}) : "")
      + `<button class="ghost-btn" data-library-new>+ ${escHtml(tr("new"))}</button>`;
    const search = tools.querySelector("input");
    onTyped(search, () => {state.query=search.value;state.visible=30;renderRows();});
    tools.querySelector("[data-library-new]").onclick = () => {newPreset(cfg);refresh();};
    renderRows();
  }
  render(); tools.querySelector("input").focus({preventScroll:true});
}

function searchPresetInfo(p) {
  const st = p.state || {}, w = weaponInfo(p.scope || presetWeapon()), limits = st.limits || {};
  return withPoolOf(w, () => {
    const name = id => modById(id)?.name || arcaneById(id)?.name || prettify(id);
    const excludes = Object.entries(limits.exclude || {}).flatMap(([axis, ids]) => (ids || []).map(id => `${tr(axis)}: ${name(id)}`));
    return presetInfoSection("Search", [tr("starts") + ': ' + (st.starts || []).length,
      tr("finalists") + ': ' + (st.finalists ?? ''), tr("runs per candidate") + ': ' + (st.candidate_runs ?? '')])
      + presetInfoSection("Limits", [tr("fill at most") + ' ' + (limits.mods ?? 8) + ' ' + tr("mods"),
        tr("Exilus") + ': ' + tr(limits.exilus === false ? "off" : "on"),
        ...(limits.arcane_seats || []).map((on, i) => tr("Arcane") + ' ' + (i + 1) + ': ' + tr(on ? "on" : "off"))])
      + presetInfoSection("Excluded", excludes)
      + (st.starts || []).map((start, i) => `<details class="preset-start-info"${i === 0 ? ' open' : ''}><summary>${escHtml(tr("start"))} ${i + 1}</summary>${start.build
        ? cardOfState({...start.build, mode:start.build.mode || defaultMode(w.id, null)}, w, new Set(start.fixed || [])) : ''}</details>`).join("");
  });
}
function operatorPresetInfo(state) {
  const st = opNormalize(state), school = focusSchool(st.school);
  const card = (id, fn) => { const m = fn(id); return m ? {label:m.name, img:IMG(m.image)} : null; };
  return presetInfoSection("Focus", [school?.name])
    + presetInfoSection("assumed up", (st.assumed || []).map(id => school?.nodes.find(n => n.id === id)?.name || id))
    + buildCardHtml({mods:st.artifact.mods.map(id => card(id, opAMod)).filter(Boolean), arcanes:[card(st.artifact.arcane, opAArcane)].filter(Boolean)});
}
function weaponPresetInfo(p, weapon = p.scope || p.state.weapon || presetWeapon()) {
  const w = weaponInfo(weapon);
  const held = defaultWielder(w.id, p.state.wielder), linked = wielderBuild(held);
  const owner = compHost(held.frame) || wfFrames().find(f => f.id === held.frame);
  return withPoolOf(w, () => cardOfState({...p.state, mode:p.state.mode || defaultMode(w.id, null)}, w))
    + presetInfoSection("Wielder", [owner?.name || held.frame, linked?.name || tr("Default")])
    + (p.board ? presetInfoSection("Benchmark", [benchmarkName(p.benchmark)]) + measuredRecordHtml(p.board) : "");
}
function companionPresetInfo(state) {
  return buildCardHtml({mods:(state.slots || []).filter(s => s?.mod).map(s => {
    const m = compMod(s.mod); return {label:m?.name || s.mod, img:m ? IMG(m.image) : null, rank:s.rank ?? m?.max_rank};
  })});
}

function presetRecentUses(domain) {
  try { return Object.assign(Object.create(null), JSON.parse(localStorage.getItem(`wfsim-preset-recent-${domain}`) || '{}')); }
  catch (_) { return Object.create(null); }
}
function notePresetUse(domain, id) {
  const recent = presetRecentUses(domain); recent[id] = Date.now();
  try { localStorage.setItem(`wfsim-preset-recent-${domain}`, JSON.stringify(recent)); } catch (_) { /* optional view preference */ }
}
function presetEquipmentSummary(state, names) {
  const ids = [...(state.slots || []).map(s => s?.mod), ...(state.arcane || []), ...(state.arcanes || []).map(a => a?.id),
    ...(state.artifact?.mods || []), state.artifact?.arcane, state.helminth?.ability].filter(id => id && id !== 'none');
  const labels = ids.map(id => names.get(String(id).replace(/^riven:/, '').split('@')[0]) || prettify(id));
  if ((state.shards || []).some(Boolean)) labels.push(`${tr('Archon Shards')} ${(state.shards || []).filter(Boolean).length}`);
  return labels.slice(0, 3).join(' · ') + (labels.length > 3 ? ` +${labels.length - 3}` : '') || tr('none selected');
}
const presetSummary = (cfg, p, names) => cfg.summary ? cfg.summary(p, names) : presetEquipmentSummary(p.state || {}, names);
function weaponPresetSummary(p, names) {
  const w = weaponInfo(p.scope || p.state.weapon || presetWeapon());
  return [modeLabel(w, p.state.mode || defaultMode(w.id, null)), presetEquipmentSummary(p.state, names)].filter(Boolean).join(' · ');
}
function scenarioPresetSummary(p) {
  const s = headlessScenarioOnto(META, p.state, CONTACT_M, METRIC_FALLBACK.id);
  const enemy = allEnemies().find(e => e.id === s.enemy);
  return `${enemy?.name || s.enemy} Lv ${s.level} · ${empowermentLabel(s.steel_path, s.empowered, true)} · ${s.headshot_pct}% ${tr('headshots')} · ${tr(s.aiming ? 'Aiming' : 'hip-fire')}`;
}
const presetActionsHtml = body => body ? `<details class="preset-actions"><summary class="ghost-btn small" aria-label="${escHtml(tr('More actions'))}" title="${escHtml(tr('More actions'))}">⋯</summary><div class="context-menu preset-action-menu">${body}</div></details>` : "";
function wirePresetActions(host) {
  host.querySelectorAll('.preset-actions').forEach(details => {
    details.ontoggle = () => {
      if (!details.open) return;
      document.querySelectorAll('.preset-actions[open]').forEach(other => { if (other !== details) other.open = false; });
      const box = details.querySelector('summary').getBoundingClientRect(), menu = details.querySelector('.preset-action-menu');
      menu.style.left = Math.max(8, Math.min(box.right - menu.offsetWidth, innerWidth - menu.offsetWidth - 8)) + 'px';
      menu.style.top = Math.max(8, Math.min(box.bottom + 4, innerHeight - menu.offsetHeight - 8)) + 'px';
    };
    details.querySelectorAll('button').forEach(button => {
      button.classList.add('mi');
      if (button.matches('.del,[data-library-delete]')) button.classList.add('danger');
      button.addEventListener('click', () => {
        if (!button.matches('.del,[data-library-delete]')) queueMicrotask(() => { details.open = false; });
      });
    });
  });
}
document.addEventListener('click', event => document.querySelectorAll('.preset-actions[open]').forEach(details => {
  if (!details.contains(event.target)) details.open = false;
}));
document.addEventListener('keydown', event => {
  if (event.key !== 'Escape' || imeComposing(event)) return;
  document.querySelectorAll('.preset-actions[open]').forEach(details => { details.open = false; details.querySelector('summary').focus({preventScroll:true}); });
});
document.addEventListener('scroll', event => {
  if (event.target instanceof Element && event.target.closest('.preset-action-menu')) return;
  document.querySelectorAll('.preset-actions[open]').forEach(details => { details.open = false; });
}, true);
