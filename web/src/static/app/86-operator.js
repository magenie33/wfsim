// ---- THE OPERATOR ------------------------------------------------------
//
// `/operator`: the active Focus school, and which of its conditional nodes to
// count as running. A Warframe build LINKS to one by its `id`, so a Focus choice
// is made once and every frame reads it.
const OPS = "operators";
let op = null;
let opActive = "";
/// A LIST WHOSE `id`s A LINK MAY STORE — a Warframe build links an Operator
/// build by it. Every list is minted at load (`mintStoredPresetIds`); this
/// covers one written since by an older tab, straight to storage and past
/// undo, since an undo that dropped the id would mint another.
function presetListWithIds(d, scope) {
  const ps = loadPresetList(d, scope);
  if (!mintPresetIds(ps)) return ps;
  try { localStorage.setItem(presetListKey(d), JSON.stringify(loadPresetWhole(d))); } catch (_) { /* unsaved: minted again next read */ }
  return ps;
}
const opList = () => presetListWithIds(OPS);
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
    info: p => operatorPresetInfo(p.state),
    summary: (p,names) => [focusSchool(p.state.school)?.name, presetEquipmentSummary(p.state,names)].filter(Boolean).join(" · "),
    label: tr("Operator builds"),
    noun: "operator",
    load: opList,
    usedBy: (p) => linkersOfOperatorPreset(p.id),
    store: (ps) => storePresetList(OPS, ps),
    active: () => opActive,
    setActive: (n) => {
      opActive = n;
      localStorage.setItem(presetActiveKey(OPS), n);
      // FRAMED BY A WARFRAME PAGE, the open build is that frame's link.
      if (EMBED && window.parent !== window) {
        const p = opList().find((x) => presetId(x) === n);
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
  dropSave("operators");
  renderOperator();
}
function opMarkDirty() {
  if (presetApplying) return;
  deferSave("operators", () => {
    if (presetApplying || !op) return;
    const cfg = opBarCfg();
    const ps = cfg.load();
    const at = ps.findIndex((p) => presetId(p) === opActive);
    if (at < 0) {
      if (sameState(op, opBlank())) return;
      const e = presetEntry(newPresetName(ps, OPS), cfg.snapshot());
      ps.push(e);
      cfg.store(ps);
      cfg.setActive(e.id);
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
  // A SCHOOL CHOSEN is the page used; a card seated is too.
  if (op.school || r.mods.length) track("builder.operator");
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
    opActive = p ? presetId(p) : "";
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
  const items = [...ps.map((p) => ({ value: p.id, label: p.name, info: () => operatorPresetInfo(p.state),
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
  const passive = (I18N && (I18N.warframe_passives || {})[f.id]) || f.passive;
  $("wf-passive").innerHTML = passive ? `<div>${escHtml(passive)}${wfTagChips(f.passive_tags)}</div>` : "";
  renderWfPresetBar();
  renderWfMods();
  renderWfArcanes();
  renderWfShards();
  renderWfOperator();
  renderWfHelminth();
  wfLockDefault();
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
  const requested = new URLSearchParams(location.search).get("build");
  if (!wf || wf.frame !== id || (requested && requested !== wfActive)) {
    const list = presetListWithIds(WF_BUILDS, id);
    const last = localStorage.getItem(presetActiveKey(WF_BUILDS, id));
    // `?build=` NAMES THE BUILD TO OPEN — a weapon's wielder link.
    const p = presetToOpen(list, requested || (last === DEFAULT_PRESET_ID ? DEFAULT_PRESET_ID : null), last);
    wfActive = p ? presetId(p) : DEFAULT_PRESET_ID;
    wf = wfNormalize(p ? p.state : null, id);
  }
  renderWarframe();
  scheduleEquipmentPublish("warframe", wfPayload());
}


function normalizeWarframeLibraryAddress() {
  const hit = location.pathname.match(/^\/warframes\/([^/]+?)(?:\/(builder|build-library))?\/?$/);
  if (!hit) return;
  const q = new URLSearchParams(location.search);
  const editor = hit[2] === 'builder' || q.has('embed') || q.has('build');
  const next = '/warframes/' + hit[1] + (editor ? '/builder' : '') + location.search + location.hash;
  if (location.pathname + location.search + location.hash !== next) history.replaceState(history.state, '', next);
}
function warframeLibraryCard(state, frame = state?.frame || wf.frame) {
  const st = wfNormalize(state, frame);
  const mod = s => {
    const m = s && wfMod(s.mod);
    return m ? {label:m.name, img:IMG(m.image), rank:s.rank ?? m.max_rank} : null;
  };
  const chips = (title, values) => `<div class="sb-h">${escHtml(title)}</div><div class="sb-chips">${values.length ? values.map(label => `<span class="sb-chip"><span>${escHtml(label)}</span></span>`).join('') : `<span class="sb-empty">${escHtml(tr('none selected'))}</span>`}</div>`;
  const shards = st.shards.filter(Boolean).map(s => {
    const d = SHARDS().find(x => x.id === s.shard), o = d?.options.find(x => x.id === s.effect);
    return d && o ? wfShardLine(d,o,s.tauforged) : '';
  }).filter(Boolean);
  return buildCardHtml({aura:[mod(st.slots[WF_AURA])].filter(Boolean), mods:st.slots.slice(0,8).map(mod).filter(Boolean),
    exilus:[mod(st.slots[WF_EXILUS])].filter(Boolean), arcanes:st.arcanes.map(a=>{
      const arc=wfArcane(a.id);return arc ? {label:arc.name,img:IMG(arc.image),rank:a.rank ?? arc.max_rank} : null;
    }).filter(Boolean)}) + chips(tr('Archon Shards'),shards)
    + chips(tr('Helminth'),st.helminth.ability ? [(wfAbility(st.helminth.ability)?.name || st.helminth.ability)] : [])
    + chips(tr('Operator'),[opBuildOf(st.operator)?.name || tr('Default')]);
}
function renderWarframeLibrary() {
  const page=$('warframe-page'), f=wfFrames().find(f=>f.id===wf?.frame);
  if (!f || page.hidden) return;
  const base=warframePath(f), editor=/\/builder\/?$/.test(location.pathname);
  page.classList.toggle('wf-library',!editor);
  $('wf-module-tabs').innerHTML=[['',tr('Build Library')],['/builder',tr('Builder')]]
    .map(([suffix,label])=>`<a class="mtab${editor === (suffix === '/builder') ? ' sel' : ''}" href="${base+suffix}">${escHtml(label)}</a>`).join('');
  $('wf-module-tabs').hidden=!!EMBED;
  const box=$('wf-library');box.hidden=editor;
  if (editor) return;
  renderEquipmentLibrary("warframe", f.id, box);
}

const equipmentLibraries = new Map(), equipmentUploads = new Map();
const equipmentScope = (kind, id) => kind + ":" + id;
const companionLibraryPayload = () => ({companion:comp.companion,
  mods:comp.slots.filter(s => s.mod).map(s => ({id:s.mod, rank:s.rank}))});
const equipmentBase = (kind, id) => kind === "warframe" ? warframePath(wfFrames().find(f => f.id === id)) : companionPath(compHost(id));
const equipmentStatus = (kind, text) => {
  const el = $(kind === "warframe" ? "wf-library-status" : "comp-library-status");
  if (el) el.textContent = text;
};

function scheduleEquipmentPublish(kind, build) {
  const snapshot = JSON.parse(JSON.stringify(build));
  delete snapshot.operator;
  const id = snapshot.frame || snapshot.companion, key = equipmentScope(kind, id);
  clearTimeout(equipmentUploads.get(key)?.timer);
  const job = {kind, build:snapshot};
  equipmentUploads.set(key, job);
  const status = text => {
    if ((kind === "warframe" ? wf?.frame : comp?.companion) === id) equipmentStatus(kind, text);
  };
  const run = async () => {
    if (equipmentUploads.get(key) !== job) return;
    try {
      const checked = await api("/api/equipment/build", {kind, build:snapshot});
      if (equipmentUploads.get(key) !== job) return;
      if (!checked?.ok) throw new Error("admission unavailable");
      if (!checked.accepted) { status(""); return; }
      const storage = "wfsim-equipment-uploaded-" + key;
      let sent;
      try { sent = JSON.parse(localStorage.getItem(storage) || "[]"); } catch { sent = []; }
      if (!Array.isArray(sent)) sent = [];
      if (sent.includes(checked.id)) { status(tr("Submitted to the public library")); return; }
      status(tr("Adding to the public library…"));
      const response = await fetch("/api/equipment/submit", {method:"POST", headers:{"content-type":"application/json"},
        body:JSON.stringify(checked.record)});
      const result = await response.json();
      if (!response.ok || !result.ok || !result.queued) throw new Error("upload failed");
      try { localStorage.setItem(storage, JSON.stringify(sent.concat(checked.id).slice(-128))); } catch (_) { /* storage is optional */ }
      equipmentLibraries.delete(key);
      if (equipmentUploads.get(key) === job) status(tr("Submitted to the public library"));
    } catch (_) {
      if (equipmentUploads.get(key) !== job) return;
      status(tr("Could not upload; retrying"));
      job.timer = setTimeout(run, 15000);
    }
  };
  job.timer = setTimeout(run, 1500);
}

function equipmentState(record) {
  const b = JSON.parse(JSON.stringify(record.build));
  if (record.kind === "warframe") {
    const state = wfBlank(b.frame);
    state.slots = b.mods.concat([b.exilus, b.aura]).map(p => ({mod:p.id, rank:p.rank ?? null, pol:wfMod(p.id)?.polarity || null}));
    state.arcanes = b.arcanes.map(p => ({id:p.id, rank:p.rank ?? null}));
    state.shards = b.shards;
    state.helminth = b.helminth || {slot:0, ability:null};
    return state;
  }
  return {companion:b.companion, slots:b.mods.map(p => ({mod:p.id, rank:p.rank ?? null, pol:compMod(p.id)?.polarity || null}))};
}

async function readEquipmentLibrary(kind, id, more = false) {
  const key = equipmentScope(kind, id);
  let entry = equipmentLibraries.get(key);
  if (entry?.pending || (more && !entry?.next)) return;
  if (!entry || !more) entry = {rows:[], next:null};
  entry.pending = true; entry.error = false; equipmentLibraries.set(key, entry);
  try {
    const response = await fetch("/api/equipment/library", {method:"POST", headers:{"content-type":"application/json"},
      body:JSON.stringify({kind, equipment:id, after:more ? entry.next : undefined})});
    const result = await response.json();
    if (!response.ok || !result.ok) throw new Error("library unavailable");
    entry.rows.push(...result.builds); entry.next = result.next;
  } catch (_) {
    entry.error = true;
    setTimeout(() => {
      if (equipmentLibraries.get(key) === entry) readEquipmentLibrary(kind, id);
    }, 15000);
  }
  entry.pending = false;
  if (kind === "warframe" && wf?.frame === id) renderWarframeLibrary();
  if (kind === "companion" && comp?.companion === id) renderCompanionLibrary();
}

function openEquipmentBuild(kind, id, state, preset) {
  flushPresetSaves();
  const domain = kind === "warframe" ? WF_BUILDS : COMP_BUILDS;
  let p = preset;
  if (!p) {
    const own = presetListWithIds(domain, id);
    p = presetEntry(newPresetName(own, domain), state);
    storePresetList(domain, own.concat(p), id);
  }
  if (kind === "warframe") { wfActive = p.id; wfBarCfg().setActive(p.id); wfApply(p.state); }
  else { compActive = p.id; compBarCfg().setActive(p.id); comp = compNormalize(p.state, id); }
  nav(equipmentBase(kind, id) + "/builder?build=" + encodeURIComponent(p.id));
}

function renderEquipmentLibrary(kind, id, box) {
  const previousSearch = box.querySelector("[data-equipment-search]");
  if (previousSearch?.dataset.composing === "true") {
    previousSearch.addEventListener("compositionend", () => queueMicrotask(() => renderEquipmentLibrary(kind, id, box)), {once:true});
    return;
  }
  const query = previousSearch?.value || "", focused = previousSearch === document.activeElement;
  const key = equipmentScope(kind, id), base = equipmentBase(kind, id);
  const own = presetListWithIds(kind === "warframe" ? WF_BUILDS : COMP_BUILDS, id);
  const entry = equipmentLibraries.get(key);
  const card = state => kind === "warframe" ? warframeLibraryCard(state, id) : companionPresetInfo(state);
  box.innerHTML = `<div class="library-toolbar"><div class="library-toolbar-row"><b>${escHtml(tr("Build Library"))}</b>`
    + `<a class="ghost-btn small" href="${base}/builder">${escHtml(tr("Builder"))} →</a></div>`
    + `<input class="slotf-search" type="search" placeholder="${escHtml(tr("Search builds"))}" data-equipment-search></div>`
    + `<h2 class="sb-h">${escHtml(tr("Saved builds on this browser"))} · ${own.length}</h2>`
    + own.map(p => `<article class="wf-library-entry"><div class="wf-library-build-head"><b>${escHtml(p.name)}</b>`
      + `<a class="ghost-btn small" data-wf-library-open="${escHtml(p.id)}" href="${base}/builder?build=${encodeURIComponent(p.id)}">${escHtml(tr("Open"))}</a></div>`
      + `<div class="fd-card">${card(p.state)}</div></article>`).join("")
    + (!own.length ? `<div class="sim-empty">${escHtml(tr("No saved builds yet"))}</div>` : "")
    + `<h2 class="sb-h">${escHtml(tr("Public builds"))}</h2>`
    + (entry?.rows || []).map(row => `<article class="wf-library-entry"><div class="wf-library-build-head"><b>${escHtml(row.id.slice(0,8))}</b>`
      + `<button type="button" class="ghost-btn small" data-equipment-copy="${row.id}">${escHtml(tr("Copy to Builder"))}</button></div>`
      + `<div class="fd-card">${card(equipmentState(row))}</div></article>`).join("")
    + (entry?.pending || !entry ? `<div class="sim-empty">${escHtml(tr("Loading…"))}</div>` : entry.error
      ? `<div class="sim-empty">${escHtml(tr("The public library is unavailable"))}</div>` : !entry.rows.length
        ? `<div class="sim-empty">${escHtml(tr("No public builds yet"))}</div>` : "")
    + (entry?.next ? `<button type="button" class="ghost-btn" data-equipment-more>${escHtml(tr("Load more"))}</button>` : "");
  box.querySelectorAll("[data-wf-library-open]").forEach(a => a.onclick = event => {
    event.preventDefault(); openEquipmentBuild(kind, id, null, own.find(p => p.id === a.dataset.wfLibraryOpen));
  });
  box.querySelectorAll("[data-equipment-copy]").forEach(button => button.onclick = () => {
    const row = entry.rows.find(row => row.id === button.dataset.equipmentCopy);
    openEquipmentBuild(kind, id, equipmentState(row));
  });
  const more = box.querySelector("[data-equipment-more]");
  if (more) more.onclick = () => readEquipmentLibrary(kind, id, true);
  const input = box.querySelector("[data-equipment-search]");
  input.value = query;
  input.addEventListener("compositionstart", () => { input.dataset.composing = "true"; });
  input.addEventListener("compositionend", () => { delete input.dataset.composing; });
  const filter = () => box.querySelectorAll(".wf-library-entry").forEach(row => {
    row.hidden = !squash(row.textContent.toLowerCase()).includes(squash(input.value.toLowerCase()));
  });
  onTyped(input, filter); filter();
  if (focused) input.focus();
  if (!entry) readEquipmentLibrary(kind, id);
}

function renderCompanionLibrary() {
  if (!comp || $("companion-page").hidden) return;
  const base = companionPath(compHost(comp.companion));
  const editor = /\/builder\/?$/.test(location.pathname) || !!EMBED || new URLSearchParams(location.search).has("build");
  $("comp-module-tabs").innerHTML = [["", "Build Library"], ["/builder", "Builder"]]
    .map(([suffix, label]) => `<a class="mtab${editor === (suffix === "/builder") ? " sel" : ""}" href="${base + suffix}">${escHtml(tr(label))}</a>`).join("");
  $("comp-module-tabs").hidden = !!EMBED;
  ["preset-bar-companions", "comp-mods-block", "comp-catalogue-block"].forEach(id => $(id).hidden = !editor);
  $("comp-library").hidden = editor;
  if (!editor) renderEquipmentLibrary("companion", comp.companion, $("comp-library"));
}
