// ---- THE OPERATOR ------------------------------------------------------
//
// `/operator`: the active Focus school, and which of its conditional nodes to
// count as running. A Warframe build LINKS to one by its `id`, so a Focus choice
// is made once and every frame reads it.
const OPS = "operators";
let op = null;
let opActive = "";
let opSaveTimer = null;
/// THE OPERATOR BUILDS, EACH WITH AN `id` — the link a Warframe build stores. A
/// name is not one: a rename would cut every link to it. A missing id is written
/// straight to storage, past undo, since an undo that dropped it would re-mint it.
function presetListWithIds(d, scope) {
  const ps = loadPresetList(d, scope);
  if (ps.every((p) => p.id)) return ps;
  const out = opWithIds(ps);
  try { localStorage.setItem(presetListKey(d, scope), JSON.stringify(out)); } catch (_) { /* unsaved: minted again next read */ }
  return out;
}
const opList = () => presetListWithIds(OPS);
// `randomUUID` exists only in a secure context; the fallback is as unique here.
const opNewId = () => (crypto.randomUUID ? crypto.randomUUID()
  : Date.now().toString(36) + Math.random().toString(36).slice(2));
const opWithIds = (ps) => ps.map((p) => (p.id ? p : { ...p, id: opNewId() }));
const focusSchool = (id) => (WFCAT && id && WFCAT.focus.find((s) => s.id === id)) || null;
const opAMod = (id) => (WFCAT && id && WFCAT.artifact_mods.find((m) => m.id === id)) || null;
const opAArcane = (id) => (WFCAT && id && WFCAT.artifact_arcanes.find((a) => a.id === id)) || null;
const opBlank = () => opNormalize(null);
/// The artifact stays with the build when the school changes: any artifact
/// seats any school's card, and the page shows the one the school owns.
///
/// **A BUILD WITH NO SCHOOL IS THE FLOOR, NOT NOTHING.** Focus is one-way, so
/// an account far enough in to have an Operator has picked one and cannot
/// un-pick it — the engine reads an unlinked build as `operator_floor` and this
/// is the same answer on the page, off the same served value.
function opNormalize(st) {
  const s = st || {};
  const school = focusSchool(s.school) || focusSchool(WFCAT.operator_floor);
  const art = s.artifact || {};
  const mods = Array.from({ length: WFCAT.artifact_slots }, (_, i) => (art.mods || [])[i])
    .map((id, i, all) => (opAMod(id) && all.indexOf(id) === i ? id : null));
  return {
    school: school ? school.id : null,
    assumed: school ? (s.assumed || []).filter((id) => school.nodes.some((n) => n.id === id && !n.always)) : [],
    artifact: { mods, arcane: opAArcane(art.arcane) ? art.arcane : null },
  };
}
function opBarCfg() {
  return {
    domain: OPS,
    label: tr("Operator builds"),
    noun: "operator",
    load: opList,
    usedBy: (p) => linkersOfOperatorPreset(p.id),
    store: (ps) => storePresetList(OPS, opWithIds(ps)),
    active: () => opActive,
    setActive: (n) => {
      opActive = n;
      localStorage.setItem(presetActiveKey(OPS), n);
      // FRAMED BY A WARFRAME PAGE, the open build is that frame's link.
      if (EMBED && window.parent !== window) {
        const p = opList().find((x) => x.name === n);
        if (p) window.parent.postMessage({ wfsim: "operator-build", id: p.id }, location.origin);
      }
    },
    snapshot: () => JSON.parse(JSON.stringify(op)),
    apply: (st) => opApply(st),
    blank: opBlank,
    isBlank: (st) => sameState(opNormalize(st), opBlank()),
    rerender: renderOpPresetBar,
  };
}
const renderOpPresetBar = () => renderPresetBarIn($("preset-bar-operators"), opBarCfg());
function opApply(st) {
  op = opNormalize(st);
  clearTimeout(opSaveTimer);
  renderOperator();
}
function opMarkDirty() {
  if (presetApplying) return;
  clearTimeout(opSaveTimer);
  opSaveTimer = setTimeout(() => {
    if (presetApplying || !op) return;
    const cfg = opBarCfg();
    const ps = cfg.load();
    const at = ps.findIndex((p) => p.name === opActive);
    if (at < 0) {
      if (sameState(op, opBlank())) return;
      const name = newPresetName(ps);
      ps.push({ name, savedAt: Date.now(), state: cfg.snapshot() });
      cfg.store(ps);
      cfg.setActive(name);
      renderOpPresetBar();
      return;
    }
    if (sameState(ps[at].state, op)) return;
    if (deleteIfBlank(cfg, ps[at].state)) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: cfg.snapshot() };
    cfg.store(ps);
  }, 400);
}

/// One node, for the Operator page and for the summary a Warframe page shows.
const opNodeHtml = (s, n, on, toggle) => `<div class="op-node${on ? " on" : ""}">
  <div class="mn">${wl(n.name, s.url)}${wfTagChips(n.tags)}</div>
  <div class="me">${escHtml(n.text)}</div>
  <div class="op-when">${n.always ? escHtml(tr("always on"))
    : toggle ? `<label><input type="checkbox" data-node="${n.id}"${on ? " checked" : ""}> ${escHtml(tr("count it as running"))} — ${escHtml(n.when)}</label>`
    : escHtml(n.when)}</div></div>`;

/// **THE TEN WAYBOUNDS, SHOWN AND NOT OFFERED.** Two a school, and they apply
/// whichever school is active: "these can be 'unbound' from the Focus school
/// they are part of, therefore showing up … in any selected school afterwards"
/// (W`Focus`). Unlocking one cannot be undone, so an account that has played
/// far enough to pick a school has them — which is why they are drawn at max
/// rank with no control beside them. None of them reaches the Warframe, so
/// nothing here pays a weapon and none of it is part of a build.
function wayboundHtml() {
  const rows = (WFCAT.focus || []).flatMap((s) => (s.waybound || []).map((w) =>
    `<div class="op-node wb"><div class="mn">${wl(w.name, s.url)}<span class="exchip">${escHtml(s.name)}</span></div>
     <div class="me">${escHtml(w.text)}</div></div>`));
  if (!rows.length) return "";
  return `<div class="sb-h">${escHtml(tr("Waybound"))}</div>
    <div class="exhint">${escHtml(tr("unbound from their school and permanent, so they are always on, at max rank, whichever school is active — none of them reaches the Warframe"))}</div>
    ${rows.join("")}`;
}

function renderOperator() {
  renderOpPresetBar();
  $("op-schools").innerHTML = WFCAT.focus.map((s) =>
    `<button class="op-school${op.school === s.id ? " sel" : ""}" data-school="${s.id}">${escHtml(s.name)}</button>`).join("");
  $("op-schools").querySelectorAll("[data-school]").forEach((b) => b.addEventListener("click", () => {
    op.school = op.school === b.dataset.school ? null : b.dataset.school;
    op.assumed = [];
    renderOperator();
    opMarkDirty();
  }));
  const s = focusSchool(op.school);
  $("op-nodes").innerHTML = (s
    ? s.nodes.map((n) => opNodeHtml(s, n, n.always || op.assumed.includes(n.id), true)).join("")
    : `<div class="exhint">${escHtml(tr("pick the active Focus school"))}</div>`) + wayboundHtml();
  $("op-nodes").querySelectorAll("[data-node]").forEach((c) => c.addEventListener("change", () => {
    op.assumed = c.checked ? [...op.assumed, c.dataset.node] : op.assumed.filter((x) => x !== c.dataset.node);
    renderOperator();
    opMarkDirty();
  }));
  renderOpArtifact();
  refreshOpArtifact();
}

// ---- the Tektolyst Artifact ----
const opSchoolChip = (id) => `<span class="exchip">${escHtml((focusSchool(id) || {}).name || id)}</span>`;

/// Each seated card as `/api/operator/panel` pays it out, by id. Until it answers
/// a card shows its catalogue text.
let opPanel = {};
let opPanelSeq = 0;
async function refreshOpArtifact() {
  const seq = ++opPanelSeq;
  const r = await api("/api/operator/panel", { artifact: { mods: op.artifact.mods.filter(Boolean), arcane: op.artifact.arcane } });
  if (seq !== opPanelSeq || !r || !r.ok) return;
  opPanel = Object.fromEntries(r.mods.map((m) => [m.id, m]));
  renderOpArtifact();
}

function opBonusNote(m) {
  const p = opPanel[m.id];
  if (!m.bonus || !p) return "";
  const text = m.bonus.per === "unique_school"
    ? tr("other schools seated: {n}")
    : tr("{school} mods seated: {n}").replace("{school}", (focusSchool(m.bonus.per) || {}).name || m.bonus.per);
  return `<div class="op-when">${escHtml(text.replace("{n}", p.count))}</div>`;
}

function opCardEl(kind, i) {
  const m = kind === "mod" ? opAMod(op.artifact.mods[i]) : opAArcane(op.artifact.arcane);
  const el = document.createElement("div");
  if (!m) {
    el.className = "slot empty" + (kind === "mod" ? "" : " arc");
    el.innerHTML = `<span class="plus">${escHtml(kind === "mod" ? tr("+ add mod") : "+ " + tr("add arcane"))}</span>`;
    el.addEventListener("click", (e) => { e.stopPropagation(); openOpPicker(kind, i, el); });
    return el;
  }
  el.className = `slot filled${kind === "mod" ? "" : " arc"} rar-${m.rarity}`;
  el.innerHTML = imgTag(IMG(m.image), "mod")
    + `<div class="info"><div class="mn">${wl(m.name, wikiUrl(m.name_en || m.name))}${kind === "mod" ? " " + opSchoolChip(m.school) : ""}</div>`
    + `${effLines((kind === "mod" && opPanel[m.id] ? opPanel[m.id].lines : m.effects).map(escHtml))}`
    + `${kind === "mod" ? opBonusNote(m) : ""}</div>`
    + `<button class="dots" title="options">⋯</button>`;
  el.querySelector(".dots").addEventListener("click", (e) => {
    e.stopPropagation();
    openSlotMenu(e.currentTarget, null, {
      label: tr(kind === "mod" ? "Mod" : "Arcane"), removable: true,
      onSwap: () => openOpPicker(kind, i, el),
      onPick: () => {
        if (kind === "mod") op.artifact.mods[i] = null; else op.artifact.arcane = null;
        opArtifactChanged();
      },
    });
  });
  return el;
}

function renderOpArtifact() {
  const s = focusSchool(op.school);
  $("op-artifact-block").hidden = !s;
  if (!s || !s.artifact) return;
  const a = s.artifact;
  $("op-artifact").innerHTML = `<div class="op-artifact-head">${a.image ? imgTag(IMG(a.image), "mod") : ""}`
    + `<div class="mn">${wl(a.name, wikiUrl("Tektolyst Artifact"))}</div>`
    + `<span class="op-when">${escHtml(tr("5 mod slots · 1 arcane slot · every card at max rank"))}</span></div>`
    + `<div class="slots" id="op-artifact-mods"></div><div class="slots" id="op-artifact-arcane"></div>`;
  op.artifact.mods.forEach((_, i) => $("op-artifact-mods").appendChild(opCardEl("mod", i)));
  $("op-artifact-arcane").appendChild(opCardEl("arcane", 0));
}

function opArtifactChanged() {
  renderOpArtifact();
  refreshOpArtifact();
  opMarkDirty();
}

function openOpPicker(kind, idx, anchor) {
  closePopovers();
  place($("wf-popover"), anchor);
  const s = $("wf-search");
  s.value = "";
  s.oninput = () => renderOpMenu(kind, idx, s.value);
  renderOpMenu(kind, idx, "");
  s.focus();
}

/// A card seated in another slot MOVES here and the two swap, as on a Warframe:
/// one card is seated once.
function renderOpMenu(kind, idx, query) {
  const q = query.trim().toLowerCase();
  const menu = $("wf-menu");
  const cur = kind === "mod" ? op.artifact.mods[idx] : op.artifact.arcane;
  const hits = (kind === "mod" ? WFCAT.artifact_mods : WFCAT.artifact_arcanes).filter((x) => searchHit(x, q))
    .sort((a, b) => (b.id === cur) - (a.id === cur) || a.name.localeCompare(b.name));
  menu.innerHTML = hits.length ? hits.map((x) => {
    const placed = kind === "mod" && x.id !== cur && op.artifact.mods.includes(x.id);
    return `<div class="opt ${x.id === cur ? "cur" : placed ? "placed" : ""} rar-${x.rarity}" data-id="${x.id}">`
      + `${imgTag(IMG(x.image), "mod")}<div class="info"><div class="mn">${escHtml(x.name)}${kind === "mod" ? " " + opSchoolChip(x.school) : ""}</div>`
      + `${effLines(x.effects.map(escHtml))}</div></div>`;
  }).join("") : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
  menu.querySelectorAll(".opt[data-id]").forEach((o) => o.addEventListener("click", () => {
    const id = o.dataset.id;
    if (kind === "mod") {
      const from = op.artifact.mods.indexOf(id);
      if (from >= 0) op.artifact.mods[from] = op.artifact.mods[idx];
      op.artifact.mods[idx] = id;
    } else {
      op.artifact.arcane = id;
    }
    closePopovers();
    opArtifactChanged();
  }));
}

async function showOperator() {
  await loadWarframeCatalog();
  if (!op) {
    const list = opList();
    const last = localStorage.getItem(presetActiveKey(OPS));
    const want = new URLSearchParams(location.search).get("build");
    const p = presetToOpen(list, want, last);
    opActive = p ? p.name : "";
    op = opNormalize(p ? p.state : null);
  }
  renderOperator();
}

/// The Operator build a Warframe build links, as the engine reads it.
const wfOperatorPick = () => (wf ? operatorPickFor(wf.operator) : null);

/// …by the Operator build's id. Null before the catalogue has loaded, which a
/// weapon page may not have fetched: the Operator reaches a wielder once it has.
function operatorPickFor(id) {
  if (!WFCAT) return null;
  const p = opBuildOf(id);
  const st = p && opNormalize(p.state);
  return st && st.school ? { ...st, artifact: { mods: st.artifact.mods.filter(Boolean), arcane: st.artifact.arcane } } : null;
}

/// WHAT A WARFRAME BUILD'S OPERATOR LINK NAMES, resolved the way a weapon's
/// wielder link is: a preset that exists by its id; the DEFAULT — the read-only
/// blank, no Operator — when it names the default or one that was deleted; and,
/// unset, the first preset, or the default while there is none.
const opIdOf = (id) => {
  const list = opList();
  if (id) return list.some((x) => x.id === id) ? id : DEFAULT_PRESET_ID;
  return list[0] ? list[0].id : DEFAULT_PRESET_ID;
};
/// The stored Operator build a link means; `null` for the blank.
const opBuildOf = (id) => opList().find((x) => x.id === opIdOf(id)) || null;

function renderWfOperator() {
  const ps = opList();
  const cur = opIdOf(wf.operator);
  // THE SAME TWO CONTROLS AS A WEAPON'S WIELDER — the type, then which preset of
  // it — though there is one Operator: one shape for every link. The default is
  // always the last entry, and the only one while none is owned.
  const items = [...ps.map((p) => ({ value: p.id, label: p.name,
    hint: (focusSchool((p.state || {}).school) || {}).name || tr("no school picked") })),
    { value: DEFAULT_PRESET_ID, label: `${tr("Default")} · ${tr("read-only")}`, hint: tr("no Operator") }];
  const box = $("wf-operator");
  // THE OPERATOR IS EDITED HERE, INSIDE THE FRAME THAT HOLDS IT: the Operator
  // page, framed as itself. The row only chooses WHICH build this frame links.
  // A build born or picked in the frame is announced (`opAnnounceBuild`) and
  // linked without reloading it; an unchanged `src` is left alone for the same
  // reason a weapon's wielder pane is.
  let row = box.querySelector(".wf-op-row");
  if (!row) {
    box.innerHTML = `<div class="wf-op-row"></div>
      <iframe class="wld-frame" loading="lazy" title="${escHtml(tr("Operator"))}"></iframe>`;
    row = box.querySelector(".wf-op-row");
  }
  row.innerHTML = `<label>${escHtml(tr("Operator"))} ${ddButton("dd-wf-operator-type", {
    value: "operator", items: [{ value: "operator", label: tr("Operator"), image: META.operator_image }],
    onPick: () => {},
  })}</label> <label>${escHtml(tr("Preset"))} ${ddButton("dd-wf-operator", {
    value: cur, items, onPick: (v) => { wf.operator = v; wfChanged(); },
  })}</label> <a class="ghost-btn small" href="/operator?build=${encodeURIComponent(cur)}">${escHtml(tr("open the full page"))}</a>`;
  const frame = box.querySelector("iframe");
  const src = `/operator?embed=1&build=${encodeURIComponent(cur)}`;
  if (frame.dataset.src !== src) { frame.dataset.src = src; frame.src = src; }
}

// AN OPERATOR BUILD OPENED OR BORN IN THE FRAMED PAGE IS THIS FRAME'S LINK.
addEventListener("message", (e) => {
  const d = e.data;
  if (e.origin !== location.origin || !d || d.wfsim !== "operator-build" || !wf) return;
  if (!d.id || opIdOf(wf.operator) === d.id) return;
  const frame = document.querySelector("#wf-operator iframe");
  if (frame) frame.dataset.src = `/operator?embed=1&build=${encodeURIComponent(d.id)}`;
  wf.operator = d.id;
  wfChanged();
});
// …AND WHAT IT WRITES IS READ BACK: the numbers above resolve the linked build.
let wfOperatorSync = null;
addEventListener("storage", (e) => {
  if (!wf || !e.key || e.key !== presetListKey(OPS)) return;
  clearTimeout(wfOperatorSync);
  wfOperatorSync = setTimeout(() => { renderWfOperator(); refreshWfPanel(); }, 200);
});

function renderWarframe() {
  const f = wfFrame(wf.frame);
  const img = $("wf-img");
  img.hidden = !f.image;
  if (f.image) img.src = IMG(f.image);
  $("wf-name").textContent = f.name;
  $("wf-tags").innerHTML = [["Health", f.health], ["Shield", f.shield], ["Armor", f.armor], ["Energy", f.energy], ["Sprint", f.sprint]]
    .map(([k, v]) => `<span class="tag">${escHtml(tr(k))} ${wfNum(v)}</span>`).join("");
  $("wf-passive").innerHTML = f.passive ? `<div>${escHtml(f.passive)}${wfTagChips(f.passive_tags)}</div>` : "";
  renderWfPresetBar();
  renderWfMods();
  renderWfArcanes();
  renderWfShards();
  renderWfOperator();
  renderWfHelminth();
  refreshWfPanel();
}

/// The route's door: the catalogue once, then this frame's open build.
async function showWarframe(id) {
  await loadWarframeCatalog();
  if (!wfWired) {
    wfWired = true;
    $("wf-auto-forma").addEventListener("click", () => wfAutoForma().then(() => wfChanged()));
    $("wf-clear").addEventListener("click", () => {
      wf.slots = wfBlank(wf.frame).slots;
      wfChanged();
    });
  }
  if (!wf || wf.frame !== id) {
    const list = presetListWithIds(WF_BUILDS, id);
    const last = localStorage.getItem(presetActiveKey(WF_BUILDS, id));
    // `?build=` NAMES THE BUILD TO OPEN — a weapon's wielder link.
    const p = presetToOpen(list, new URLSearchParams(location.search).get("build"), last);
    wfActive = p ? p.name : "";
    wf = wfNormalize(p ? p.state : null, id);
  }
  renderWarframe();
}

