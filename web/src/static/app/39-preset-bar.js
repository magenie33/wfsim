// ---- The ONE preset-bar component -------------------------------------
// Every preset bar on the page (the build bar and the optimizer's three
// scope bars) is the same template on the same document model: label +
// count, the chips (the active one carries duplicate / rename / delete),
// "+ new". OWNING NONE, THE BAR IS EMPTY and the editor shows the DEFAULT, the
// blank: the first effective edit writes "preset 1", and a preset edited back to
// the blank is deleted, so what is left is the default again. Edits AUTO-SAVE into the active preset, so there is no save
// button and no dirty marker. "+ new" creates an EMPTY preset instantly
// under an auto-name ("preset N") and switches to it — no naming step; rename after via ✎. Branching an existing preset
// is the ⧉ duplicate on the active chip.
// Counts are UNLIMITED, so past PRESET_FILTER_AT chips the bar grows a
// name filter; the active chip always shows (it is the document being
// edited).
const PRESET_FILTER_AT = 10;
let deleteArmed = null; // "<bar>:<preset>" whose delete has said who links it
const presetFilters = {}; // per-bar filter text — survives re-renders, not persisted

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
  whileApplying(() => cfg.apply(p.state)); // a load is not an edit
  cfg.rerender();
};

/// "+ new": a blank document of this bar's kind, made active, named with the
/// bar's own noun ("riven N" on the riven bar). Activated FIRST so everything
/// that renders during apply() (the sim's per-preset stored result) already
/// sees the new one; the stored state is the live snapshot after the blank is
/// applied, so it matches exactly what the editor shows.
const newPreset = (cfg) => {
  flushPresetSaves();
  const ps = cfg.load();
  const e = presetEntry(newPresetName(ps), null);
  cfg.setActive(e.id);
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
  const base = presetLabel(presetFind(barEntries(cfg), cfg.active()));
  const state = cfg.copyState ? cfg.copyState(cfg.snapshot()) : cfg.snapshot();
  const e = presetEntry(freeName(ps, (n) => base + " copy" + (n > 1 ? " " + n : "")), state);
  ps.push(e);
  cfg.store(ps);
  cfg.setActive(e.id);
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
  // WHAT ONE OF THESE IS CALLED. "Preset" is the CATEGORY — a saved state of
  // a module, as opposed to a custom — and no collection is named after its
  // category. A build is a build, a scenario a scenario, a
  // search a search; the noun names new ones and every tooltip that has to
  // refer to one.
  const noun = cfg.noun || "preset";
  // YOURS, and then — where the collection publishes any — the READ-ONLY
  // entries opened into it (a board build, an official ruler). `ps` is yours
  // alone, so the filter threshold and the delete rule count what you own; the
  // label counts every entry in the bar, because every one is in it.
  const ps = cfg.load();
  const ro = cfg.pins ? openedPublished(cfg) : [];
  const active = cfg.active();
  const ftext = presetFilters[bar.id] || "";
  const f = ftext.trim().toLowerCase();
  const shown = f ? ps.filter((p) => presetId(p) === active || p.name.toLowerCase().includes(f)) : ps;
  const hint = cfg.hint ? ` (${cfg.hint})` : "";
  // OWNING NONE SAYS WHERE THE EDITOR STANDS: on the default, the blank. A
  // scenario owning none is on a pinned official ruler (`pinned`) and a
  // custom's editor stands down (`optional`); neither has a blank to name.
  const onDefault = cfg.blank && !cfg.optional && !cfg.pinned && ps.length === 0 && !ro.length;
  const defaultNote = onDefault
    ? `<span class="pnote" title="${escHtml(tr("the default is read-only"))}">${escHtml(tr("Default"))} · ${escHtml(tr("your first change is saved as"))} ${escHtml(autoPresetName(PRESET_NAME, 1))}</span>`
    : "";
  const chip = (p) => {
    const sel = presetId(p) === active;
    const ops = !sel
      ? ""
      : `<button class="pop dup" title="${escHtml(tr("duplicate"))}">⧉</button>` +
        `<button class="pop ren" title="rename">✎</button>` +
        // DELETABLE TO ZERO. "There is always one" was true
        // while one was auto-created; now that nothing is, the last one is as
        // deletable as the first — and a collection you cannot empty is one the
        // config page can never show you an honest count of. `cfg.optional` was
        // already the customs' flag for exactly this and is simply no longer
        // the thing that distinguishes them.
        `<button class="pop del" title="delete">✕</button>`;
    // WHO LINKS IT, where the collection is one others link to: the linked side
    // only says so, and choosing here moves no link.
    const by = cfg.usedBy ? cfg.usedBy(p) : [];
    const used = by.length
      ? `<span class="pby" title="${escHtml(tr("linked by") + ": " + by.join(" · "))}">↩${by.length}</span>` : "";
    return `<span class="pchip ${sel ? "sel" : ""}" data-name="${escHtml(presetId(p))}" title="switch to ${escHtml(p.name)}${escHtml(hint)}">${cloudMark(cfg.domain, p)}${escHtml(p.name)}${used}${ops}</span>`;
  };
  // A READ-ONLY ENTRY: select, ⧉ on the one you are on, and × to take it out of
  // the bar — which removes nothing from where it came from.
  const roChip = (p) => {
    const sel = presetId(p) === active;
    return `<span class="pchip ro ${sel ? "sel" : ""}" data-name="${escHtml(presetId(p))}" title="${escHtml(cfg.roTitle ? cfg.roTitle(p) : "")}">${LOCK_SVG}${escHtml(presetLabel(p))}` +
      (sel ? `<button class="pop dup" title="${escHtml(tr("copy it into a {thing} of your own — the official one cannot be edited").replace("{thing}", tr(noun)))}">⧉</button>` : "") +
      `<button class="pop unpin" data-unpin="${escHtml(presetId(p))}" title="${escHtml(tr("take it out of the bar — the board keeps it"))}">×</button></span>`;
  };
  bar.innerHTML =
    // Every bar says the shortcut: auto-save means a slip is written before
    // you can regret it, so the way back has to be visible on the thing that
    // slipped.
    `<span class="plabel" title="${escHtml(tr("Ctrl+Z undoes the last change"))}">${cfg.label} <b>${ps.length + ro.length}</b></span>` +
    (ps.length > PRESET_FILTER_AT ? `<input class="pfilter" type="text" placeholder="${escHtml(tr("filter…"))}" value="${escHtml(ftext)}">` : "") +
    (ro.length ? `<span class="pgroup">${escHtml(tr("Mine"))}</span>` : "") +
    defaultNote + shown.map(chip).join("") +
    (ro.length || cfg.openable
      ? `<span class="psep" aria-hidden="true"></span><span class="pgroup">${escHtml(cfg.roGroup)}</span>` + ro.map(roChip).join("")
      : "") +
    // …AND THE WAY IN, where the bar is it: a published entry picked from a list
    // lands in the group above like any other. A board build's way in is the
    // build finder, which searches rather than lists.
    (cfg.openable
      ? `<span class="popen">${cfg.boardPicker ? `<button type="button" class="ghost-btn small" data-board-open>${escHtml(`+ ${cfg.openLabel}`)}</button>` : ddButton(`dd-open-${cfg.domain}`, {
          value: "", search: true, title: cfg.openHint || "",
          placeholder: `+ ${cfg.openLabel}`,
          items: cfg.openable().map((p) => ({ value: presetId(p), label: p.group || p.name })),
          onPick: (v) => pickPreset(cfg, v),
        })}</span>`
      : "") +
    // One template, not two words joined by a space: Chinese does not put one
    // between them, so concatenating produced "新建空白 配装".
    `<span class="pchip add" title="${escHtml(
      tr("new empty {thing}").replace("{thing}", tr(noun)) + (cfg.hint ? " · " + cfg.hint : "")
    )}">+ new</span>` +
    (cfg.extra || "") +
    undoButtons(cfg.domain) +
    `<div class="pshare" hidden></div>`;
  wireUndoButtons(bar, cfg.domain);
  if (cfg.onExtra) cfg.onExtra(bar);

  // Typing re-renders the bar (chips re-filter), so hand focus back.
  const filt = bar.querySelector(".pfilter");
  if (filt) onTyped(filt, () => {
    presetFilters[bar.id] = filt.value;
    cfg.rerender();
    const nf = bar.querySelector(".pfilter");
    if (nf) { nf.focus(); nf.setSelectionRange(nf.value.length, nf.value.length); }
  });
  bar.querySelectorAll(".pchip:not(.add)").forEach((c) =>
    c.addEventListener("click", (e) => { if (!e.target.closest("a.pcloud")) pickPreset(cfg, c.dataset.name); }));
  const boardOpen = bar.querySelector("[data-board-open]");
  if (boardOpen) boardOpen.onclick = (event) => { event.stopPropagation(); cfg.boardPicker(boardOpen); };
  bar.querySelectorAll(".pcloud[data-cloud]").forEach((b) => b.addEventListener("click", (e) => {
    e.stopPropagation();
    if (setCloudSync(presetListKey(cfg.domain), b.dataset.cloud, !b.classList.contains("on"))) cfg.rerender();
  }));
  // No prompt()/alert()/confirm() anywhere — the browser can block those
  // dialogs, which made saving silently fail. Naming
  // happens in an INLINE input: Enter commits, Esc cancels.
  const nameInput = (placeholderEl, initial, onCommit) => {
    placeholderEl.outerHTML = `<input class="pname" type="text" value="${escHtml(initial)}" placeholder="name, then Enter…" maxlength="24">`;
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
  // Unique auto-names: "+ new" takes the smallest free "preset N";
  // duplicate takes "<name> copy", then "<name> copy 2", …
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
    // YOURS ONLY: `load()` is the reader's own, so deleting your last build
    // leaves the default rather than falling through to a published one.
    const ps2 = cfg.load().filter((p) => presetId(p) !== cfg.active());
    // EVERY COLLECTION MAY GO TO ZERO, not only the OPTIONAL ones. A module
    // always has a state and "no build" is not a thing the builder can show —
    // both true, and neither needs a stored row: nothing is
    // auto-created any more (`initPresets`), so the state the builder shows
    // when you own nothing is `cfg.blank()`, and a preset comes back the moment
    // you edit it.
    //
    // `cfg.blank` is what the three modules already declare for "+ new", so the
    // state after deleting the last one is the state a new one would start
    // from — one answer, not two. A CUSTOM keeps `null`, which is how its
    // editor knows to stand down.
    cfg.store(ps2);
    // …AND A COLLECTION THAT PUBLISHES ITS DEFAULT goes back to it (`fallback`:
    // the first official ruler, the first built-in search), as unpinning does.
    const next = ps2[0] || (cfg.fallback ? cfg.fallback() : null);
    cfg.setActive(next ? presetId(next) : "");
    whileApplying(() => cfg.apply(
      next ? next.state : (cfg.optional || !cfg.blank ? null : cfg.blank())));
    // A DELETE IS NOT AN EDIT, and the state it leaves behind is the pristine
    // one — recorded here so the re-render this very handler causes cannot
    // create the row that was just removed.
    if (!ps2.length && cfg.pristine) cfg.pristine();
    cfg.rerender();
    dropSave("builds");
    dropSave("search");
  });
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

