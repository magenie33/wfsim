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

