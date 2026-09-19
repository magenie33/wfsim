// ---- The ONE preset-bar component -------------------------------------
// Every preset bar on the page (the build bar and the optimizer's three
// scope bars) is the same template on the same document model: label +
// count, the chips (the active one carries duplicate / rename / delete;
// the last remaining preset cannot be deleted — there is always one),
// "+ new". Edits AUTO-SAVE into the active preset, so there is no save
// button and no dirty marker. "+ new" creates an EMPTY preset instantly
// under an auto-name ("preset N") and switches to it — no naming step; rename after via ✎. Branching an existing preset
// is the ⧉ duplicate on the active chip.
// Counts are UNLIMITED, so past PRESET_FILTER_AT chips the bar grows a
// name filter; the active chip always shows (it is the document being
// edited).
const PRESET_FILTER_AT = 10;
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
/// has none and is its own name. So this is the id, and `presetLabel` is what a
/// reader sees — the two were the same string until the board grew a second
/// ruler.
const presetId = (p) => (p || {}).builtin || (p || {}).name || "";
const presetLabel = (p) => (p || {}).name || "";

const pickPreset = (cfg, key) => {
  flushPresetSaves();
  const ps = cfg.load();
  // By ID first: a name may now be shared by two rulers' rows.
  const p = ps.find((x) => presetId(x) === key) || ps.find((x) => x.name === key);
  if (!p || presetId(p) === cfg.active()) return;
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
  const name = freeName(ps, (n) => autoPresetName(cfg.noun || "preset", n));
  cfg.setActive(name);
  whileApplying(() => cfg.apply(cfg.blank()));
  ps.push({ name, savedAt: Date.now(), state: cfg.snapshot() });
  cfg.store(ps);
  cfg.rerender();
  return name;
};

// The copy captures the LIVE editor state and becomes the active document; the
// original keeps what auto-save last wrote into it. For a read-only entry the
// live state IS that entry, because selecting it is what put it there.
const copyActivePreset = (cfg) => {
  flushPresetSaves();
  const ps = cfg.load();
  const base = cfg.active();
  const name = freeName(ps, (n) => base + " copy" + (n > 1 ? " " + n : ""));
  ps.push({ name, savedAt: Date.now(), state: cfg.snapshot() });
  cfg.store(ps);
  cfg.setActive(name);
  cfg.rerender();
  return name;
};

// THE BENCHMARK BAR — the official SCENARIOS, one per ruler, in a bar of their
// own above the player's. Same chip styling, deliberately: it is the same kind
// of thing to pick. A different component, also deliberately: none of it is
// yours, so there is no new, rename, delete, filter or undo — only select and ⧉.
//
// ONE CONTROL: picking the ruler IS picking the scenario. The board's BUILDS
// are not here — they are the build finder's (`renderBuildFinder`), because a
// build is found by what it contains rather than by walking four menus.
function renderBenchmarkBarIn(bar, cfg) {
  if (!bar) return;
  const ps = cfg.load().filter((p) => p.builtin);
  const active = cfg.active();
  const noun = cfg.noun || "preset";
  const sel = ps.find((p) => presetId(p) === active) || null;
  bar.hidden = false;
  bar.innerHTML =
    `<span class="plabel bench" title="${escHtml(cfg.benchHint || "")}">${escHtml(cfg.benchLabel)} <b>${ps.length}</b></span>` +
    ddButton(`dd-bench-${cfg.domain}`, {
      value: sel ? presetId(sel) : (ps[0] ? presetId(ps[0]) : ""),
      search: ps.length > 1,
      // ONLY AN EMPTY LIST IS DISABLED: picking here is what loads a scenario,
      // so a single entry still has to be clickable.
      disabled: ps.length === 0,
      title: cfg.benchHint || "",
      items: ps.map((p) => ({ value: presetId(p), label: p.group || p.name })),
      onPick: (v) => pickPreset(cfg, v),
    }) +
    (sel
      ? `<button class="pop dup" title="${escHtml(
          tr("copy it into a {thing} of your own — the official one cannot be edited")
            .replace("{thing}", tr(noun)))}">⧉</button>`
      : "");
  const dup = bar.querySelector(".pop.dup");
  if (dup) dup.addEventListener("click", (e) => { e.stopPropagation(); copyActivePreset(cfg); });
}

function renderPresetBarIn(bar, cfg) {
  // WHAT ONE OF THESE IS CALLED. "Preset" is the CATEGORY — a saved state of
  // a module, as opposed to a custom — and no collection is named after its
  // category. A build is a build, a scenario a scenario, a
  // search a search; the noun names new ones and every tooltip that has to
  // refer to one.
  const noun = cfg.noun || "preset";
  // YOURS, and then — where the collection has any — the READ-ONLY entries you
  // opened (a board build). `ps` stays yours alone, so the filter threshold and
  // the delete rule count the presets you own; the label counts every build in
  // the bar, because every build is in it.
  const ps = cfg.load().filter((p) => !p.builtin);
  const ro = cfg.opened ? cfg.opened() : [];
  const active = cfg.active();
  const ftext = presetFilters[bar.id] || "";
  const f = ftext.trim().toLowerCase();
  const shown = f ? ps.filter((p) => p.name === active || p.name.toLowerCase().includes(f)) : ps;
  const hint = cfg.hint ? ` (${cfg.hint})` : "";
  const chip = (p) => {
    const sel = p.name === active;
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
    return `<span class="pchip ${sel ? "sel" : ""}" data-name="${escHtml(p.name)}" title="switch to ${escHtml(p.name)}${escHtml(hint)}">${escHtml(p.name)}${ops}</span>`;
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
    shown.map(chip).join("") +
    (ro.length
      ? `<span class="psep" aria-hidden="true"></span><span class="pgroup">${escHtml(tr("From the board · read-only"))}</span>` + ro.map(roChip).join("")
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
  if (filt) filt.addEventListener("input", () => {
    presetFilters[bar.id] = filt.value;
    cfg.rerender();
    const nf = bar.querySelector(".pfilter");
    if (nf) { nf.focus(); nf.setSelectionRange(nf.value.length, nf.value.length); }
  });
  bar.querySelectorAll(".pchip:not(.add)").forEach((c) =>
    c.addEventListener("click", () => pickPreset(cfg, c.dataset.name)));
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
    nameInput(chipEl, cfg.active(), (name) => {
      const ps2 = cfg.load();
      // Empty, unchanged, or colliding names just cancel the rename.
      if (name && name !== cfg.active() && !ps2.some((p) => p.name === name)) {
        const at = ps2.findIndex((p) => p.name === cfg.active());
        if (at >= 0) {
          ps2[at].name = name;
          cfg.store(ps2);
          cfg.setActive(name);
        }
      }
      cfg.rerender();
    });
  });

  on(".pop.del", () => {
    // YOURS ONLY, which is what the bar drawing these chips already counts
    // (`renderPresetBarIn` filters `!p.builtin`). Counting the joint list here
    // meant that on a weapon WITH board rows, deleting your last build fell
    // through to a BENCHMARK build — so the page answered "you deleted your
    // build" by loading somebody else's, and the bar's count and this handler's
    // disagreed about what a collection contains.
    const ps2 = cfg.load().filter((p) => !p.builtin && p.name !== cfg.active());
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
    cfg.setActive(ps2.length ? ps2[0].name : "");
    whileApplying(() => cfg.apply(
      ps2.length ? ps2[0].state : (cfg.optional || !cfg.blank ? null : cfg.blank())));
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

