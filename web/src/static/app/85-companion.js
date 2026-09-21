// ---- THE COMPANION HOST -------------------------------------------------
//
// `/companions/<Name>`: what carries a robotic weapon — a Sentinel or a MOA —
// and therefore the holder a Sentinel weapon's build links, exactly as any other
// weapon links a Warframe. Its stat block is the wielder floor
// (`META.sentinel_floor`) and its pool is `data/companion_mods/`.
//
// EVERY CARD HERE PAYS NOTHING, and the page says so once rather than on each:
// no robotic weapon pool reads a wielder's stat and no precept is run, so what
// this builder answers is capacity and Forma. docs/WARFRAMES.md §Companions.
const COMP_BUILDS = "companions";
// TEN GENERAL SLOTS AND NO SPECIAL ONE — W`Mod`. Served rather than assumed
// (`companion_slots`), with this as the value before the catalogue lands.
const COMP_SLOTS = 10;
let comp = null;
let compActive = "";

const compHosts = () => (META && META.companions) || [];
const compHost = (id) => compHosts().find((c) => c.id === id) || null;
const companionPath = (c) => "/companions/" + String(c.name).replace(/ /g, "_");
const compList = (id) => presetListWithIds(COMP_BUILDS, id);
const compSlotCount = () => (WFCAT && WFCAT.companion_slots) || COMP_SLOTS;
const compMod = (id) => (WFCAT && id && WFCAT.companion_mods.find((m) => m.id === id)) || null;
/// WHAT THIS HOST SEATS: the universal cards, plus any naming it. Mirrors
/// `data::companions::seatable_by`.
const compPool = (host) => ((WFCAT && WFCAT.companion_mods) || [])
  .filter((m) => m.compat === "companion" || m.compat === "robotic" || m.compat === host);
/// The innate polarities every companion has, served beside the slot count.
const compInnate = () => (WFCAT && WFCAT.companion_polarities) || [];

function compBlank(id) {
  const slots = Array.from({ length: compSlotCount() }, () => ({ mod: null, pol: null, rank: null }));
  compInnate().forEach((p, i) => { if (slots[i]) slots[i].pol = p; });
  return { companion: id, slots };
}

/// A stored build, repaired against today's catalogue: a card that is gone
/// becomes an empty slot rather than one nothing can draw.
function compNormalize(st, id) {
  const b = compBlank(id);
  const s = st || {};
  return {
    companion: id,
    slots: b.slots.map((x, i) => {
      const y = (s.slots || [])[i];
      return y ? { mod: compMod(y.mod) ? y.mod : null, pol: y.pol ?? null, rank: y.rank ?? null } : x;
    }),
  };
}

async function showCompanion(id) {
  await loadWarframeCatalog();
  if (!comp || comp.companion !== id) {
    const list = compList(id);
    const last = localStorage.getItem(presetActiveKey(COMP_BUILDS, id));
    const p = presetToOpen(list, new URLSearchParams(location.search).get("build"), last);
    compActive = p ? p.name : "";
    comp = compNormalize(p ? p.state : null, id);
  }
  renderCompanion();
}

function renderCompanion() {
  const c = compHost(comp.companion);
  const f = META.sentinel_floor || {};
  $("comp-name").textContent = c.name;
  $("comp-tags").innerHTML = [["Health", f.health], ["Shield", f.shield], ["Armor", f.armor]]
    .map(([k, v]) => `<span class="tag">${escHtml(tr(k))} ${v}</span>`).join("");
  renderCompPresetBar();
  renderCompMods();
}

// ---- capacity and Forma ----
// A COMPANION IS SUPERCHARGED BY AN OROKIN REACTOR, and takes Forma, on the same
// terms as a Warframe (W`Mod`) — so the reader's own rules decide both.
const compDrainAt = (m, rank) =>
  m.drain - m.max_rank + (rank == null ? m.max_rank : Math.max(0, Math.min(m.max_rank, rank)));
const compCapacity = () => WF_BASE_CAPACITY / (formaRules().catalyst ? 1 : 2);
function compUsed() {
  let n = 0;
  for (const s of comp.slots) {
    const m = compMod(s.mod);
    if (m) n += slotDrain(compDrainAt(m, s.rank), m.polarity, s.pol);
  }
  return n;
}
/// Forma owed: the innate colours are one pool across the slots, so moving one
/// is free and each colour added or removed beyond the pool is one Forma.
function compFormaCount() {
  const need = {}, pool = {};
  let umbra = 0, omni = 0;
  comp.slots.forEach((s) => {
    if (!s.pol) return;
    if (s.pol === "Omni") omni++;
    else if (s.pol === "Umbra") umbra++;
    else need[s.pol] = (need[s.pol] || 0) + 1;
  });
  compInnate().forEach((p) => { pool[p] = (pool[p] || 0) + 1; });
  let added = 0, removed = 0;
  for (const p of new Set([...Object.keys(need), ...Object.keys(pool)])) {
    const d = (need[p] || 0) - (pool[p] || 0);
    if (d > 0) added += d; else removed -= d;
  }
  return { regular: Math.max(added, removed), umbra, omni };
}

// ---- the slots ----
function renderCompMods() {
  const used = compUsed(), cap = compCapacity();
  $("comp-capacity").textContent = `${used} / ${cap}`;
  $("comp-capacity").classList.toggle("over", used > cap);
  const f = compFormaCount();
  $("comp-forma").textContent = [`${f.regular} Forma`, f.umbra ? `${f.umbra} Umbra` : null,
    f.omni ? `${f.omni} Omni` : null].filter(Boolean).join(" · ");
  const box = $("comp-mod-slots");
  box.innerHTML = "";
  comp.slots.forEach((_, i) => box.appendChild(compSlotEl(i)));
}

function compSlotEl(i) {
  const s = comp.slots[i];
  const m = compMod(s.mod);
  const el = document.createElement("div");
  if (m) {
    el.className = "slot filled" + (m.rarity ? " rar-" + m.rarity : "");
    const r = s.rank == null ? m.max_rank : s.rank;
    const base = compDrainAt(m, r);
    const eff = slotDrain(base, m.polarity, s.pol);
    const matched = s.pol === m.polarity || (s.pol === "Omni" && m.polarity !== "Umbra");
    const fit = !s.pol ? "" : matched ? " matched" : " mismatched";
    el.innerHTML = polBtn(s.pol, i) + `<div class="info"><div class="mn">${wl(m.name, m.url || wikiUrl(m.name))}${
      m.precept ? ` <span class="exchip">${escHtml(tr("Precept"))}</span>` : ""}</div>`
      + `<div class="me">${m.effects.map((x) => `<div>${escHtml(x)}</div>`).join("")}</div>`
      + `<div class="drow"><div class="dr${fit}"><span class="mpol">${polGlyph(m.polarity)}</span>${eff} drain${
        eff !== base ? ` (base ${base})` : ""}</div>${wfRank(r, m.max_rank)}</div></div>`
      + `<button class="dots" title="options">⋯</button>`;
    el.querySelector(".dots").addEventListener("click", (e) => {
      e.stopPropagation();
      openSlotMenu(e.currentTarget, null, {
        label: tr("Mod"), removable: true,
        onSwap: () => openCompPicker(i, el),
        onPick: () => { s.mod = null; s.rank = null; compChanged(); },
      });
    });
    el.querySelectorAll(".rk").forEach((b) => b.addEventListener("click", (e) => {
      e.stopPropagation();
      s.rank = Math.max(0, Math.min(m.max_rank, r + Number(b.dataset.d)));
      compChanged();
    }));
  } else {
    el.className = "slot empty";
    el.innerHTML = polBtn(s.pol, i) + `<div class="info"><div class="mn">${escHtml(tr("empty slot"))}</div></div>`;
    el.addEventListener("click", () => openCompPicker(i, el));
  }
  el.querySelectorAll(".pol-btn").forEach((b) => b.addEventListener("click", (e) => {
    e.stopPropagation();
    openCompPolMenu(i, e.currentTarget);
  }));
  return el;
}

function openCompPolMenu(i, anchor) {
  closePopovers();
  const menu = $("slot-menu");
  const cur = comp.slots[i].pol;
  menu.innerHTML = WF_POLS.map((p) => `<div class="mi ${p === cur ? "sel" : ""}" data-p="${p}">${polGlyph(p)} ${p === "Omni" ? "Omni (any)" : p}</div>`).join("")
    + `<div class="mi ${!cur ? "sel" : ""}" data-p="">◇ none</div>`;
  place(menu, anchor);
  menu.querySelectorAll(".mi").forEach((o) => o.addEventListener("click", () => {
    comp.slots[i].pol = o.dataset.p || null;
    closePopovers();
    compChanged();
  }));
}

function openCompPicker(idx, anchor) {
  closePopovers();
  place($("wf-popover"), anchor);
  const s = $("wf-search");
  s.value = "";
  s.oninput = () => renderCompMenu(idx, s.value);
  renderCompMenu(idx, "");
  s.focus();
}

/// THE POOL THIS HOST SEATS. A card already in another slot is offered as a
/// SWAP; nothing else is refused, because a companion card has no family, no
/// set and no ability to augment.
function renderCompMenu(idx, query) {
  const q = (query || "").trim().toLowerCase();
  const menu = $("wf-menu");
  const own = compMod(comp.slots[idx].mod);
  const at = (id) => comp.slots.findIndex((s, i) => i !== idx && s.mod === id);
  const hits = compPool(comp.companion).filter((m) => searchHit(m, q))
    .sort((a, b) => (b.id === (own && own.id)) - (a.id === (own && own.id)) || a.name.localeCompare(b.name));
  menu.innerHTML = hits.length ? hits.map((m) => {
    const placed = at(m.id);
    return `<div class="opt ${m.id === (own && own.id) ? "cur" : ""} rar-${m.rarity}" data-id="${m.id}"${
      placed >= 0 ? ` title="${escHtml(tr("seated in another slot — picking it swaps the two"))}"` : ""}>
      <div class="info"><div class="mn">${escHtml(m.name)}${placed >= 0 ? ` <span class="exchip">${escHtml(tr("seated"))}</span>` : ""}${
        m.precept ? ` <span class="exchip">${escHtml(tr("Precept"))}</span>` : ""}</div>
      ${effLines(m.effects.map(escHtml))}
      <div class="dr"><span class="mpol">${polGlyph(m.polarity)}</span>${m.drain} drain</div></div></div>`;
  }).join("") : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
  menu.querySelectorAll(".opt[data-id]").forEach((o) => o.addEventListener("click", () => {
    const id = o.dataset.id;
    const placed = at(id);
    // A SWAP KEEPS BOTH CARDS: the one being displaced takes this slot rather
    // than being dropped, which is what every other picker on the page does.
    if (placed >= 0) {
      const mine = { ...comp.slots[idx] };
      comp.slots[idx] = { ...comp.slots[idx], mod: id, rank: comp.slots[placed].rank };
      comp.slots[placed] = { ...comp.slots[placed], mod: mine.mod, rank: mine.rank };
    } else {
      comp.slots[idx] = { ...comp.slots[idx], mod: id, rank: null };
    }
    closePopovers();
    compChanged();
  }));
}

// ---- builds ----
/// FRAMED BY A WEAPON PAGE, which build is open is the weapon's link: told to the
/// parent whenever a real one is open.
function compAnnounceBuild() {
  if (!EMBED || window.parent === window || !comp) return;
  const p = compList(comp.companion).find((x) => x.name === compActive);
  if (p) window.parent.postMessage({ wfsim: "wielder-build", frame: comp.companion, id: p.id }, location.origin);
}

function compBarCfg() {
  return {
    domain: COMP_BUILDS,
    label: tr("Companion builds"),
    noun: "companion",
    load: () => compList(comp.companion),
    usedBy: (p) => linkersOfCompanionPreset(comp.companion, p.id),
    store: (ps) => storePresetList(COMP_BUILDS, opWithIds(ps), comp.companion),
    active: () => compActive,
    setActive: (n) => {
      compActive = n;
      if (EMBED) compAnnounceBuild();
      else localStorage.setItem(presetActiveKey(COMP_BUILDS, comp.companion), n);
    },
    snapshot: () => JSON.parse(JSON.stringify(comp)),
    apply: (st) => compApply(st),
    blank: () => compBlank(comp.companion),
    isBlank: (st) => sameState(compNormalize(st, comp.companion), compBlank(comp.companion)),
    rerender: renderCompPresetBar,
  };
}
const renderCompPresetBar = () => renderPresetBarIn($("preset-bar-companions"), compBarCfg());
function compApply(st) {
  comp = compNormalize(st, comp.companion);
  clearTimeout(compSaveTimer);
  renderCompanion();
}

/// A BUILD IS BORN ON THE FIRST EDIT, as a Warframe build is (`wfMarkDirty`), and
/// one edited back to the blank is deleted.
let compSaveTimer = null;
function compMarkDirty() {
  if (presetApplying) return;
  clearTimeout(compSaveTimer);
  compSaveTimer = setTimeout(() => {
    if (presetApplying || !comp) return;
    const cfg = compBarCfg();
    const ps = cfg.load();
    const at = ps.findIndex((p) => p.name === compActive);
    if (at < 0) {
      if (sameState(comp, compBlank(comp.companion))) return;
      const name = newPresetName(ps);
      ps.push({ name, savedAt: Date.now(), state: cfg.snapshot() });
      cfg.store(ps);
      cfg.setActive(name);
      renderCompPresetBar();
      return;
    }
    if (sameState(ps[at].state, comp)) return;
    if (deleteIfBlank(cfg, ps[at].state)) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: cfg.snapshot() };
    cfg.store(ps);
  }, 400);
}

function compChanged() {
  renderCompanion();
  compMarkDirty();
}
