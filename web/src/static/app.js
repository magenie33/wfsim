// WFSim build configurator — PURE CONFIG. Modules: Mods / Arcane / Evolution /
// Element; each weapon enables only the ones it has. Data from /api/meta;
// official polarity icons from the wiki, art from WFCD.

const $ = (id) => document.getElementById(id);
// Art is served through our OWN origin (/img proxy: local disk cache, WFCD
// fallback) instead of hotlinking the CDN — fast, offline-capable, one source.
const CDN = "/img/";
// Polarity icons are vendored locally (/pol) — no more slow wiki 302 redirects.
// Omni (universal) uses the "Any" symbol (a PNG); the rest are SVGs.
const POL = (p) => `/pol/${p === "Omni" ? "Any" : p}_Pol.${p === "Omni" ? "png" : "svg"}`;
// Polarities available on GUN slots. Zenurik/Unairu/Penjaga are Warframe-augment
// / melee-stance / companion-ability polarities — not gun slots. "Omni" is the
// Omni Forma universal polarity (matches any mod EXCEPT Umbra mods).
const GUN_POLS = ["Madurai", "Naramon", "Vazarin", "Umbra", "Omni"];
const CAP = 60;
const imgTag = (src, cls) => src ? `<img class="${cls||''}" src="${src}" onerror="this.style.visibility='hidden'"/>` : `<span class="${cls||''}"></span>`;

let META = null;
// 9 × { mod:id|null, pol:string|null, rank:int|null } — POSITIONAL.
// Indices 0–7 are the regular slots; index 8 is the EXILUS slot (utility mods
// only; drain counts toward capacity like any slot; absent on sentinels).
const EXILUS = 8;
let slots = [];
let innate = [];     // 9 × innate polarity name|null (exilus never innate)
let arcane = "none";
let arcaneRank = null;   // null → max rank (mirrors mod slot ranks)
// Per-tier evolution selection {tier: id|null}; null = EMPTY (nothing
// installed at that tier). Tier 1 is the Incarnon Form unlock: empty there
// means no transformation, so the panel falls back to the base form.
// Overwritten by META.defaults on init.
let evoSel = { 1: null, 2: null, 3: null, 4: null };
let pickerSlot = 0;
// Mod-picker sort/filter prefs — persisted across slots, presets and weapons.
let pickerPrefs = { sort: "name", dir: "asc", pol: null };
try { const s = JSON.parse(localStorage.getItem("wfsim-picker")); if (s) pickerPrefs = { ...pickerPrefs, ...s }; } catch (_) {}
const savePickerPrefs = () => localStorage.setItem("wfsim-picker", JSON.stringify(pickerPrefs));

// theme
(function () {
  const saved = localStorage.getItem("wfsim-theme");
  if (saved) document.documentElement.setAttribute("data-theme", saved);
  $("theme-toggle").addEventListener("click", () => {
    const cur = document.documentElement.getAttribute("data-theme");
    const dark = cur === "dark" || (!cur && matchMedia("(prefers-color-scheme: dark)").matches);
    const next = dark ? "light" : "dark";
    document.documentElement.setAttribute("data-theme", next);
    localStorage.setItem("wfsim-theme", next);
  });
})();

async function init() {
  META = await (await fetch("/api/meta")).json();
  fillSelect("weapon", META.weapons);
  const d = META.defaults;
  $("weapon").value = d.weapon;
  arcane = d.arcane;
  evoSel = { 1: null, 2: null, 3: null, 4: null, ...(d.evolutions || {}) };
  applyWeapon(d.weapon, d.mods);

  $("weapon").addEventListener("change", () => applyWeapon($("weapon").value, null));
  initPresets();
  $("auto-forma").addEventListener("click", () => { autoForma(); renderMods(); });
  $("clear-mods").addEventListener("click", () => { slots.forEach((s, i) => { s.mod = null; s.pol = innate[i]; }); renderMods(); });
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".popover") && !e.target.closest(".slot")) closePopovers();
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") closePopovers(); });
}

// ---- Presets: up to 10 saved builds (localStorage) --------------------
// A preset captures the WHOLE configuration: weapon, mod slots (mod id +
// polarity + rank), arcane + rank, and the per-tier evolution selection.
const PRESET_KEY = "wfsim-presets";
const PRESET_MAX = 10;
const loadPresets = () => {
  try { const p = JSON.parse(localStorage.getItem(PRESET_KEY)); return Array.isArray(p) ? p : []; }
  catch (_) { return []; }
};
const storePresets = (ps) => localStorage.setItem(PRESET_KEY, JSON.stringify(ps));

function snapshotState() {
  return {
    weapon: $("weapon").value,
    evoSel: { ...evoSel },
    arcane,
    arcaneRank,
    slots: slots.map((s) => ({ mod: s.mod, pol: s.pol, rank: s.rank })),
  };
}

function restoreState(st) {
  if (!st || !weaponInfo(st.weapon)) return;
  $("weapon").value = st.weapon;
  applyWeapon(st.weapon, null); // resets pool/innate/visibility
  (st.slots || []).forEach((s, i) => {
    if (i >= slots.length) return;
    slots[i].mod = s.mod && modById(s.mod) ? s.mod : null; // drop ids gone from the pool
    slots[i].pol = s.pol ?? null;
    slots[i].rank = s.rank ?? null;
  });
  evoSel = { 1: null, 2: null, 3: null, 4: null, ...(st.evoSel || {}) };
  arcane = st.arcane && arcaneById(st.arcane) ? st.arcane : "none";
  arcaneRank = st.arcaneRank ?? null;
  renderMods(); renderArcanes(); renderEvo(); refreshPanel();
}

const escHtml = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

// The active (last loaded / saved) preset name, or null.
let activePreset = null;

function initPresets() {
  renderPresetBar();
}

function renderPresetBar() {
  const bar = $("preset-bar");
  const ps = loadPresets();
  const chip = (p) => {
    const active = p.name === activePreset;
    // The active chip carries its own update / rename / delete controls.
    const ops = active
      ? `<button class="pop upd" title="overwrite this preset with the current build">↻</button>` +
        `<button class="pop ren" title="rename">✎</button>` +
        `<button class="pop del" title="delete">✕</button>`
      : "";
    return `<span class="pchip ${active ? "sel" : ""}" data-name="${escHtml(p.name)}" title="load ${escHtml(p.name)}">${escHtml(p.name)}${ops}</span>`;
  };
  bar.innerHTML =
    `<span class="plabel">Presets <b>${ps.length}/${PRESET_MAX}</b></span>` +
    ps.map(chip).join("") +
    (ps.length < PRESET_MAX ? `<span class="pchip add" title="save the current build as a new preset">+ save</span>` : "");

  bar.querySelectorAll(".pchip:not(.add)").forEach((c) => c.addEventListener("click", () => {
    const p = loadPresets().find((x) => x.name === c.dataset.name);
    if (p) { activePreset = p.name; restoreState(p.state); renderPresetBar(); }
  }));
  const addBtn = bar.querySelector(".pchip.add");
  if (addBtn) addBtn.addEventListener("click", () => {
    const ps2 = loadPresets();
    const name = (prompt("Preset name:", `preset ${ps2.length + 1}`) || "").trim();
    if (!name) return;
    const at = ps2.findIndex((p) => p.name === name);
    const entry = { name, savedAt: Date.now(), state: snapshotState() };
    if (at >= 0) ps2[at] = entry; else ps2.push(entry); // same name overwrites
    storePresets(ps2);
    activePreset = name;
    renderPresetBar();
  });
  const on = (sel, fn) => { const b = bar.querySelector(sel); if (b) b.addEventListener("click", (e) => { e.stopPropagation(); fn(); }); };
  on(".pop.upd", () => {
    const ps2 = loadPresets();
    const at = ps2.findIndex((p) => p.name === activePreset);
    if (at < 0) return;
    ps2[at] = { name: activePreset, savedAt: Date.now(), state: snapshotState() };
    storePresets(ps2);
    renderPresetBar();
  });
  on(".pop.ren", () => {
    const name = (prompt("New name:", activePreset) || "").trim();
    if (!name || name === activePreset) return;
    const ps2 = loadPresets();
    if (ps2.some((p) => p.name === name)) { alert(`A preset named "${name}" already exists.`); return; }
    const at = ps2.findIndex((p) => p.name === activePreset);
    if (at < 0) return;
    ps2[at].name = name;
    storePresets(ps2);
    activePreset = name;
    renderPresetBar();
  });
  on(".pop.del", () => {
    if (!confirm(`Delete preset "${activePreset}"?`)) return;
    storePresets(loadPresets().filter((p) => p.name !== activePreset));
    activePreset = null;
    renderPresetBar();
  });
}

function fillSelect(id, items) {
  const el = $(id); el.innerHTML = "";
  for (const it of items) { const o = document.createElement("option"); o.value = it.id; o.textContent = it.name; el.appendChild(o); }
}
let currentPool = [];
const weaponInfo = (id) => META.weapons.find((w) => w.id === id) || META.weapons[0];
const modById = (id) => currentPool.find((m) => m.id === id);
const show = (id, on) => { const el = $(id); if (on) el.removeAttribute("hidden"); else el.setAttribute("hidden", ""); };
// Where (other than exceptIdx) this mod is currently slotted, or -1.
const placedAt = (id, exceptIdx) => slots.findIndex((s, i) => i !== exceptIdx && s.mod === id);

function applyWeapon(id, presetMods) {
  const w = weaponInfo(id);
  currentPool = META.mod_pools[w.mod_class] || [];
  innate = (w.innate_polarities || []).slice(0, 8);
  while (innate.length < 9) innate.push(null);

  $("w-img").src = w.image ? CDN + w.image : "";
  // The weapon name links to its wiki page too (display suffixes like
  // " (sentinel)" are ours, not part of the page name).
  $("w-name").innerHTML = wl(w.name, wikiUrl(w.name.replace(" (sentinel)", "")));
  // Subtype first (e.g. "Dual Pistols") — the precise weapon type that drives
  // which mod pool actually applies; then the eligibility group + form tags.
  $("w-tags").innerHTML = [w.subtype, w.mod_class + " mods", w.sentinel ? "sentinel" : null, w.uses_evo2 ? "Incarnon" : null]
    .filter(Boolean).map((t) => `<span class="tag">${t}</span>`).join("");

  show("arcane-block", w.arcane_slots >= 1);
  show("evo-block", w.uses_evo2);
  show("element-block", !!w.element_config);
  $("arcane-sub").textContent = w.sentinel ? "sentinels cannot equip arcanes" : `${w.arcane_slots} slot`;
  if (w.arcane_slots < 1) arcane = "none";

  slots = Array.from({ length: 9 }, (_, i) => ({ mod: null, pol: innate[i], rank: null }));
  (presetMods || []).filter((m) => modById(m)).slice(0, 8).forEach((m, i) => { slots[i].mod = m; slots[i].rank = modById(m).max_rank; });
  autoForma(); // sensible default: minimum-Forma polarities for the preset

  renderMods(); renderArcanes(); renderEvo();
}

// ---- forma / capacity plan (mirrors engine::mods::plan_forma) ----
function slotDrain(base, modPol, slotPol) {
  if (!slotPol) return base;                                        // no polarity
  if (slotPol === "Omni") return modPol === "Umbra" ? base : Math.ceil(base / 2); // universal, not Umbra
  if (slotPol === modPol) return Math.ceil(base / 2);              // matched: −50% round up
  return Math.round(base * 1.25);                                  // mismatched: +25%
}

// Drain at a given rank: rises 1 per rank from rank 0 (= max-rank drain − max_rank).
function modDrain(m, rank) {
  const r = rank == null ? m.max_rank : Math.max(0, Math.min(m.max_rank, rank));
  return m.drain - m.max_rank + r;
}

// Capacity = Σ effective drain over slots holding a mod (at its rank).
function capacityUsed() {
  return slots.reduce((sum, s) => { const m = modById(s.mod); return m ? sum + slotDrain(modDrain(m, s.rank), m.polarity, s.pol) : sum; }, 0);
}

// Forma cost, broken down by TYPE (regular / Omni / Umbra cost different items).
// Innate polarities form a free-repositionable pool of REGULAR polarities, so
// regular Forma = max(added-beyond-pool, removed-from-pool): same-polarity
// repositioning nets 0, but BLANKING an innate polarity (removal) costs a Forma,
// and a colour swap costs one (add+remove of one slot). Omni/Umbra are never
// innate here — each such slot is one Omni/Umbra Forma.
function formaCount() {
  const need = {}, pool = {};
  let umbra = 0, omni = 0;
  slots.forEach((s) => {
    if (!s.pol) return;
    if (s.pol === "Omni") omni++;
    else if (s.pol === "Umbra") umbra++;
    else need[s.pol] = (need[s.pol] || 0) + 1;
  });
  innate.forEach((p) => { if (p && p !== "Omni" && p !== "Umbra") pool[p] = (pool[p] || 0) + 1; });
  let added = 0, removed = 0;
  for (const p of new Set([...Object.keys(need), ...Object.keys(pool)])) {
    const d = (need[p] || 0) - (pool[p] || 0);
    if (d > 0) added += d; else removed += -d;
  }
  return { regular: Math.max(added, removed), umbra, omni };
}

// Auto-assign polarities for MINIMUM Forma-to-fit (mirrors engine plan_forma):
// spend the innate pool on the biggest matching mods, then Forma the biggest
// unmatched until it fits; unmatched slots left blank. Overwrites polarities.
function autoForma() {
  const filled = [];
  slots.forEach((s, i) => { const m = modById(s.mod); if (m) filled.push({ i, m }); });
  slots.forEach((s) => { s.pol = null; });
  const pool = innate.filter(Boolean).slice();
  const bd = ({ i, m }) => modDrain(m, slots[i].rank);
  const order = filled.slice().sort((a, b) => bd(b) - bd(a));
  const matched = new Set();
  for (const { i, m } of order) { const k = pool.indexOf(m.polarity); if (k >= 0) { pool.splice(k, 1); matched.add(i); } }
  const drainOf = () => filled.reduce((s, x) => s + (matched.has(x.i) ? Math.ceil(bd(x) / 2) : bd(x)), 0);
  while (drainOf() > CAP) { const next = order.find(({ i }) => !matched.has(i)); if (!next) break; matched.add(next.i); }
  for (const { i, m } of filled) slots[i].pol = matched.has(i) ? m.polarity : null;
}

// ---- render mods ----
function polBtn(pol, i) {
  return `<button class="pol-btn" data-i="${i}" title="change polarity">${pol ? imgTag(POL(pol), "pol") : '<span class="nopol">◇</span>'}</button>`;
}
function renderMods() {
  const used = capacityUsed();
  const capEl = $("capacity");
  capEl.textContent = `${used} / ${CAP}`;
  capEl.classList.toggle("over", used > CAP);
  const f = formaCount();
  $("forma").textContent = [`${f.regular} Forma`, f.umbra ? `${f.umbra} Umbra` : null, f.omni ? `${f.omni} Omni` : null]
    .filter(Boolean).join(" · ");

  const box = $("mod-slots");
  box.innerHTML = "";
  for (let i = 0; i < 8; i++) box.appendChild(buildSlot(i));

  // Exilus: a REAL slot (utility mods only, drain counts) — absent on sentinels.
  const ex = $("exilus");
  ex.innerHTML = "";
  if (weaponInfo($("weapon").value).sentinel) {
    ex.innerHTML = `<div class="slot empty exl"><span class="plus">sentinel weapons have no exilus slot</span></div>`;
  } else {
    ex.appendChild(buildSlot(EXILUS));
  }
  refreshPanel();
}

// ---- Stats panel: merged buckets, each explained by source ----
let panelTimer = null;
function refreshPanel() {
  clearTimeout(panelTimer);
  panelTimer = setTimeout(async () => {
    const body = {
      weapon: $("weapon").value,
      evolutions: Object.values(evoSel).filter(Boolean),
      // No Incarnon Form unlock (tier 1 empty) = the weapon cannot
      // transform: the stats panel shows the BASE form.
      form: evoSel[1] ? "incarnon" : "base",
      arcane,
      arcane_rank: arcaneRank,
      mods: slots.filter((s) => s.mod).map((s) => s.mod), // slot order (elements are position-sensitive)
    };
    try {
      const r = await (await fetch("/api/panel", {
        method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
      })).json();
      renderPanel(r);
    } catch (e) {
      $("stats-rows").innerHTML = `<div class="error">panel failed: ${e}</div>`;
    }
  }, 120);
}

function renderPanel(r) {
  if (!r || r.ok === false) {
    $("stats-rows").innerHTML = `<div class="error">${r ? r.error : "no data"}</div>`;
    return;
  }
  $("stats-sub").textContent = `${r.form} · max-rank values · ${r.policy}`;
  const srcLine = (s) =>
    `<div class="ssrc">${s.value} — ${s.mod}${s.note ? ` <span class="snote">(${s.note})</span>` : ""}</div>`;
  const rowHtml = (row) => `
    <div class="srow">
      <div class="shead"><span class="sk">${row.label}</span>
        <span class="sv">${row.base !== "—" ? `<span class="sbase">${row.base}</span> → ` : ""}<b>${row.final}</b></span></div>
      ${row.note ? `<div class="srownote">⚙ ${row.note}</div>` : ""}
      ${(row.sources || []).map(srcLine).join("")}
    </div>`;
  // Indirect stats (recoil, accuracy, ammo…) render like any bucket — they
  // are outside theoretical DPS but real in practice, so the panel states them.
  const rows = [...(r.stats || []), ...(r.elements || []), ...(r.indirect || [])];
  $("stats-rows").innerHTML = rows.length ? rows.map(rowHtml).join("") : `<div class="placeholder">no mods — base stats only</div>`;

  $("stats-damage").innerHTML = (r.damage && r.damage.length)
    ? `<div class="sdmg-title">Damage (combined) — ${r.damage_total} total</div>` +
      r.damage.map((d) => `<div class="sdmg"><span class="sk">${d.type}</span><span class="sv"><b>${d.amount}</b> <span class="snote">${d.share}</span></span></div>`).join("")
    : "";

  $("stats-conditionals").innerHTML = (r.conditionals && r.conditionals.length)
    ? `<div class="sdmg-title">Conditional / not merged</div>` +
      r.conditionals.map((c) => `<div class="scond ${c.active ? "" : "off"}"><b>${c.mod}</b>: ${c.desc} <span class="snote">${c.why}</span></div>`).join("")
    : "";
}

// Wiki page for an item, from its display name (the data files' source
// urls follow the same Name_With_Underscores convention).
const wikiUrl = (name) => "https://wiki.warframe.com/w/" + encodeURIComponent(name.replace(/ /g, "_"));
// EVERY item name (mod / arcane / evolution) opens its wiki page in a new
// tab; the click never triggers the card's own handlers.
const wl = (text, url) => `<a class="wl" href="${url || wikiUrl(text)}" target="_blank" rel="noopener" onclick="event.stopPropagation()">${text}</a>`;

// Description lines at a rank: the verbatim in-game text with the
// rank-varying numbers filled server-side (mods and arcanes alike). Null
// when the pool has no yaml description (hardcoded rifle pool) — callers
// fall back to the model's effect lines.
const descAt = (o, r) => o.desc_ranks
  ? o.desc_ranks[Math.max(0, Math.min(o.desc_ranks.length - 1, r))].split("\n")
  : null;

// One slot card (regular or exilus) with its polarity / rank / menu wiring.
function buildSlot(i) {
  const s = slots[i];
  const el = document.createElement("div");
  const m = s.mod ? modById(s.mod) : null;
  if (m) {
    el.className = "slot filled" + (m.rarity ? " rar-" + m.rarity : "");
    const r = s.rank == null ? m.max_rank : s.rank;
    const base = modDrain(m, r);
    const eff = slotDrain(base, m.polarity, s.pol);
    const lowered = r < m.max_rank;
    const rank = m.max_rank > 0
      ? `<span class="rank ${lowered ? "lowered" : ""}"><button class="rk" data-d="-1">−</button><b>R${r}${lowered ? "/" + m.max_rank : ""}</b><button class="rk" data-d="1">+</button></span>`
      : "";
    // The configured mod shows its CURRENT description (values at the
    // slot's rank), exactly like the in-game card.
    const desc = descAt(m, r);
    el.innerHTML = polBtn(s.pol, i) + imgTag(m.image ? CDN + m.image : null, "mod") +
      `<div class="info"><div class="mn">${wl(m.name)}</div>${desc ? `<div class="me">${desc.map((x) => `<div>${x}</div>`).join("")}</div>` : ""}<div class="dr">${eff} drain${eff !== base ? ` (base ${base})` : ""}</div>${rank}</div>` +
      `<button class="dots" title="options">⋯</button>`;
    el.querySelector(".dots").addEventListener("click", (e) => { e.stopPropagation(); openSlotMenu(i, e.currentTarget); });
    el.querySelectorAll(".rk").forEach((b) => b.addEventListener("click", (e) => {
      e.stopPropagation();
      const nr = Math.max(0, Math.min(m.max_rank, r + Number(b.dataset.d)));
      slots[i].rank = nr; renderMods();
    }));
  } else {
    el.className = "slot empty";
    el.innerHTML = polBtn(s.pol, i) + `<span class="plus">${i === EXILUS ? "+ add exilus mod" : "+ add mod"}</span>`;
    // the WHOLE empty slot opens the picker (the pol-btn stops propagation)
    el.addEventListener("click", (e) => { e.stopPropagation(); openPicker(i, el); });
  }
  // polarity is decoupled: clickable on every slot (mod or empty, incl. innate)
  el.querySelector(".pol-btn").addEventListener("click", (e) => { e.stopPropagation(); openPolMenu(i); });
  return el;
}

// The DOM node for slot i (popover anchoring).
const slotEl = (i) => i === EXILUS ? $("exilus").firstElementChild : $("mod-slots").children[i];

// ---- popovers ----
function closePopovers() { $("mod-popover").hidden = true; $("slot-menu").hidden = true; $("arcane-popover").hidden = true; }
function place(pop, anchor) {
  const r = anchor.getBoundingClientRect();
  pop.hidden = false;
  pop.style.top = (window.scrollY + r.bottom + 4) + "px";
  pop.style.left = (window.scrollX + r.left) + "px";
}

function openPicker(slotIdx, anchor) {
  closePopovers();
  pickerSlot = slotIdx;
  const pop = $("mod-popover");
  place(pop, anchor);
  const search = $("mod-search");
  search.value = "";
  search.oninput = () => renderMenu(slotIdx, search.value);
  renderTools();
  renderMenu(slotIdx, "");
  search.focus();
}

function renderTools() {
  const t = $("picker-tools");
  const pols = ["Madurai", "Naramon", "Vazarin", "Umbra"].filter((p) => currentPool.some((m) => m.polarity === p));
  t.innerHTML =
    `<label>Sort <select id="pk-sort"><option value="name">Name</option><option value="drain">Drain</option></select></label>` +
    `<button id="pk-dir" class="ghost-btn small" title="direction">${pickerPrefs.dir === "asc" ? "▲" : "▼"}</button>` +
    `<span class="pk-pols"><span class="pk-pol ${!pickerPrefs.pol ? "sel" : ""}" data-p="">all</span>` +
    pols.map((p) => `<span class="pk-pol ${pickerPrefs.pol === p ? "sel" : ""}" data-p="${p}" title="${p}">${imgTag(POL(p), "pol")}</span>`).join("") +
    `</span>`;
  // redraw() re-renders these tools via innerHTML, which DETACHES the clicked
  // node; without stopPropagation the click would bubble to the document
  // outside-click handler, whose closest(".popover") now fails on the detached
  // target → the picker would wrongly close. Keep every tool click inside.
  const redraw = () => { savePickerPrefs(); renderTools(); renderMenu(pickerSlot, $("mod-search").value); };
  $("pk-sort").value = pickerPrefs.sort;
  $("pk-sort").onclick = (e) => e.stopPropagation();
  $("pk-sort").onchange = (e) => { e.stopPropagation(); pickerPrefs.sort = $("pk-sort").value; redraw(); };
  $("pk-dir").onclick = (e) => { e.stopPropagation(); pickerPrefs.dir = pickerPrefs.dir === "asc" ? "desc" : "asc"; redraw(); };
  t.querySelectorAll(".pk-pol").forEach((o) => o.onclick = (e) => { e.stopPropagation(); pickerPrefs.pol = o.dataset.p || null; redraw(); });
}

function familyConflict(mod, exceptIdx) {
  if (!mod.family) return false;
  return slots.some((s, i) => { if (i === exceptIdx || !s.mod) return false; const o = modById(s.mod); return o && o.family === mod.family; });
}

function renderMenu(slotIdx, query) {
  const menu = $("mod-menu");
  const q = query.trim().toLowerCase();
  // Equipped mods stay LISTED: the current slot's mod is marked, mods in other
  // slots show their slot number — picking one of those EXCHANGES the two slots.
  const group = (m) => slots[slotIdx].mod === m.id ? 0 : placedAt(m.id, slotIdx) >= 0 ? 1 : 2;
  const hits = currentPool
    .filter((m) => slotIdx !== EXILUS || m.exilus) // exilus slot: utility mods only
    .filter((m) => !pickerPrefs.pol || m.polarity === pickerPrefs.pol)
    .filter((m) => !q || m.name.toLowerCase().includes(q) || m.effects.join(" ").toLowerCase().includes(q)
      || (m.desc_ranks || []).join(" ").toLowerCase().includes(q))
    .sort((a, b) => {
      const g = group(a) - group(b); // current first, then equipped, then the rest
      if (g) return g;
      const c = pickerPrefs.sort === "drain" ? a.drain - b.drain : a.name.localeCompare(b.name);
      return pickerPrefs.dir === "desc" ? -c : c;
    });
  // No cap: every pool mod must be reachable. The popover menu scrolls
  // (`.combo-menu` overflow-y), so the whole sorted/filtered list is browsable.
  menu.innerHTML = hits.length ? hits.map((m) => {
    const isCur = slots[slotIdx].mod === m.id;
    const at = placedAt(m.id, slotIdx);
    // Exchanging with the exilus slot would move OUR mod there — only legal
    // if it is exilus-eligible (or the slot is empty).
    const ownMod = slots[slotIdx].mod ? modById(slots[slotIdx].mod) : null;
    const exIllegal = at === EXILUS && ownMod && !ownMod.exilus;
    const conflict = at < 0 && !isCur && familyConflict(m, slotIdx);
    // Every placed mod shows a "slot N" chip (the current one shows ITS OWN
    // slot). No "current" word — same color family; background does the
    // distinguishing, the current slot rendered a touch stronger.
    const slotName = (idx) => idx === EXILUS ? "exilus" : "slot " + (idx + 1);
    const badge = isCur ? `<span class="slotchip cur">${slotName(slotIdx)}</span>`
      : at >= 0 ? `<span class="slotchip">${slotName(at)}</span>` : "";
    const title = conflict ? `incompatible (${m.family})`
      : exIllegal ? `cannot swap: ${ownMod.name} is not an exilus mod`
      : at >= 0 ? `swap with ${at === EXILUS ? "the exilus slot" : "slot " + (at + 1)}`
      : m.effects.join(" · ");
    return `<div class="opt ${conflict || exIllegal ? "dis" : ""} ${isCur ? "cur" : at >= 0 ? "placed" : ""} ${m.rarity ? "rar-" + m.rarity : ""}" data-id="${m.id}" title="${title}">
      ${imgTag(POL(m.polarity), "pol")}${imgTag(m.image ? CDN + m.image : null, "mod")}
      <div class="info"><div class="mn">${wl(m.name)}${m.exilus ? ' <span class="exchip">EXILUS</span>' : ""} ${badge}</div><div class="me">${(descAt(m, m.max_rank) || m.effects).map((x) => `<div>${x}</div>`).join("")}</div></div><span class="dr">${m.drain}</span></div>`;
  }).join("") : `<div class="opt dis">no matches</div>`;
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", () => {
    const id = o.dataset.id;
    if (slots[slotIdx].mod === id) { closePopovers(); return; } // already here
    const at = placedAt(id, slotIdx);
    if (at >= 0) {
      // EXCHANGE the two slots' mods (+ranks); polarities stay with their slots
      const a = slots[slotIdx], b = slots[at];
      [a.mod, b.mod] = [b.mod, a.mod];
      [a.rank, b.rank] = [b.rank, a.rank];
    } else {
      slots[slotIdx].mod = id; // polarity is decoupled — keep the slot's polarity
      slots[slotIdx].rank = modById(id).max_rank; // added mods default to max rank
    }
    closePopovers(); renderMods();
  }));
}

function openSlotMenu(slotIdx, anchor) {
  closePopovers();
  // MOD ops only (polarity lives on the left icon — no duplication).
  const menu = $("slot-menu");
  menu.innerHTML = `
    <div class="mi" data-a="swap">Swap mod</div>
    <div class="mi danger" data-a="remove">Remove mod</div>`;
  place(menu, anchor);
  menu.querySelector('[data-a="swap"]').addEventListener("click", () => { openPicker(slotIdx, slotEl(slotIdx)); });
  menu.querySelector('[data-a="remove"]').addEventListener("click", () => { slots[slotIdx].mod = null; closePopovers(); renderMods(); }); // in-place; keep polarity
}

function openPolMenu(slotIdx) {
  closePopovers();
  const menu = $("slot-menu");
  const cur = slots[slotIdx].pol;
  menu.innerHTML = GUN_POLS.map((p) => `<div class="mi ${p === cur ? "sel" : ""}" data-p="${p}">${imgTag(POL(p), "pol")} ${p === "Omni" ? "Omni (any)" : p}</div>`).join("") +
    `<div class="mi ${!cur ? "sel" : ""}" data-p="">◇ none</div>`;
  place(menu, slotEl(slotIdx));
  menu.querySelectorAll(".mi").forEach((o) => o.addEventListener("click", () => {
    slots[slotIdx].pol = o.dataset.p || null;
    closePopovers(); renderMods();
  }));
}

// ---- Arcane ----
// Full parity with mods: ONE slot card (rank stepper, ⋯ menu) → click opens a
// searchable picker that matches name OR effect, with effect lines, rarity
// frames, and the equipped arcane highlighted in the accent background family.
const arcaneById = (id) => META.arcanes.find((x) => x.id === id);
function setArcane(id) { arcane = id; arcaneRank = null; } // new arcane → max rank
// Effect lines for a specific rank (clamped). Arcane strengths scale per rank
// (wiki), so the slot shows the SELECTED rank; the picker shows max rank.
const effectsAt = (a, r) => {
  const rk = a && a.ranks || [];
  if (!rk.length) return [];
  return rk[Math.max(0, Math.min(rk.length - 1, r))] || [];
};
const effLines = (arr) => arr.length ? `<div class="me">${arr.map((x) => `<div>${x}</div>`).join("")}</div>` : "";

function renderArcanes() {
  const box = $("arcane-slots");
  box.innerHTML = "";
  const a = arcaneById(arcane);
  const none = !a || a.id === "none";
  const el = document.createElement("div");
  if (none) {
    el.className = "slot empty arc";
    el.innerHTML = `<span class="plus">+ add arcane</span>`;
  } else {
    const maxr = a.max_rank || 0;
    const r = arcaneRank == null ? maxr : Math.max(0, Math.min(maxr, arcaneRank));
    const lowered = r < maxr;
    const rank = maxr > 0
      ? `<span class="rank ${lowered ? "lowered" : ""}"><button class="rk" data-d="-1">−</button><b>R${r}${lowered ? "/" + maxr : ""}</b><button class="rk" data-d="1">+</button></span>`
      : "";
    el.className = "slot filled arc" + (a.rarity ? " rar-" + a.rarity : "");
    // The slot shows the verbatim DESCRIPTION at the selected rank (like
    // the mod cards); model effect lines remain the search text.
    el.innerHTML = imgTag(a.image ? CDN + a.image : null, "mod") +
      `<div class="info"><div class="mn">${wl(a.name)}</div>${effLines(descAt(a, r) || effectsAt(a, r))}${rank}</div>` +
      `<button class="dots" title="options">⋯</button>`;
    el.querySelector(".dots").addEventListener("click", (e) => { e.stopPropagation(); openArcaneMenu(el); });
    el.querySelectorAll(".rk").forEach((b) => b.addEventListener("click", (e) => {
      e.stopPropagation();
      arcaneRank = Math.max(0, Math.min(maxr, r + Number(b.dataset.d)));
      renderArcanes();
    }));
  }
  // Mod-slot parity: only the EMPTY slot opens the picker on click; a
  // filled card swaps via its ⋯ menu — so its text stays selectable.
  if (none) {
    el.addEventListener("click", (e) => { e.stopPropagation(); openArcanePicker(el); });
  }
  box.appendChild(el);
}

function openArcanePicker(anchor) {
  closePopovers();
  const pop = $("arcane-popover");
  place(pop, anchor);
  const search = $("arcane-search");
  search.value = "";
  search.oninput = () => renderArcaneMenu(search.value);
  renderArcaneMenu("");
  search.focus();
}

// Search matches NAME or any EFFECT line (like the mod picker). "None" always
// stays listed as the clear-out option.
function renderArcaneMenu(query) {
  const menu = $("arcane-menu");
  const q = query.trim().toLowerCase();
  // Search matches NAME, ANY rank's effect text, or the description.
  const allEff = (a) => (a.ranks || []).reduce((acc, r) => acc.concat(r), [])
    .concat(a.desc_ranks || []).join(" ").toLowerCase();
  const hits = META.arcanes.filter((a) => a.id === "none" || !q ||
    a.name.toLowerCase().includes(q) || allEff(a).includes(q));
  menu.innerHTML = hits.length ? hits.map((a) => {
    const isCur = a.id === arcane;
    const none = a.id === "none";
    return `<div class="opt ${isCur ? "cur" : ""} ${a.rarity ? "rar-" + a.rarity : ""}" data-id="${a.id}">
      ${none ? '<span class="mod none">∅</span>' : imgTag(a.image ? CDN + a.image : null, "mod")}
      <div class="info"><div class="mn">${none ? a.name : wl(a.name)}${isCur ? ' <span class="slotchip cur">equipped</span>' : ""}</div>${effLines(descAt(a, a.max_rank) || effectsAt(a, a.max_rank))}</div></div>`;
  }).join("") : `<div class="opt dis">no matches</div>`;
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", () => { setArcane(o.dataset.id); closePopovers(); renderArcanes(); }));
}

// ⋯ on a filled arcane slot: mirror the mod slot menu (remove).
function openArcaneMenu(anchor) {
  closePopovers();
  const menu = $("slot-menu");
  menu.innerHTML = `<div class="mi" data-a="swap">Swap arcane</div><div class="mi danger" data-a="remove">Remove arcane</div>`;
  place(menu, anchor);
  menu.querySelector('[data-a="swap"]').addEventListener("click", () => openArcanePicker(anchor));
  menu.querySelector('[data-a="remove"]').addEventListener("click", () => { setArcane("none"); closePopovers(); renderArcanes(); });
}

// ---- Evolution ----
// Every tier (EVO I–IV) renders its options as CARDS — icon, name, and the
// verbatim effect text, like the mod/arcane cards — PLUS an explicit None
// card (nothing installed). Wiki-flagged broken evolutions carry a red
// BROKEN badge, and selecting one shows a red note: the engine really
// computes them as NO EFFECT. Deselecting tier 1 (the Incarnon Form
// unlock) drops the weapon to its base form.
function renderEvo() {
  const roman = { 1: "EVO I", 2: "EVO II", 3: "EVO III", 4: "EVO IV" };
  const tiers = META.evolutions || [];
  const rows = [];
  for (const t of tiers) {
    const sel = evoSel[t.tier] || null;
    const card = (o) => {
      const icon = o.icon ? `<img class="eicon" src="/img/${encodeURIComponent(o.icon)}" alt="">` : "";
      const cls = ["evopick", o.id === sel ? "sel" : "", o.broken ? "broken" : ""].join(" ");
      const lines = (o.desc && o.desc.length ? o.desc : o.effects || []).map((x) => `<div>${x}</div>`).join("");
      const title = (o.effects || []).join("\n"); // model statement as tooltip
      // The broken warning lives INSIDE the selected card, so it never
      // straddles the row divider into the next tier.
      const warn = o.broken && o.id === sel
        ? `<span class="ed warn">⚠ does not work in-game (wiki) — the simulation computes it as NO EFFECT</span>`
        : "";
      // Evolutions have no standalone wiki pages — link to the weapon's
      // Incarnon Genesis page.
      const genesis = wikiUrl(weaponInfo($("weapon").value).name.replace(" (sentinel)", "") + " Incarnon Genesis");
      return `<span class="${cls}" data-tier="${t.tier}" data-id="${o.id}" title="${title}">
        ${icon}<span class="einfo"><b class="en">${wl(o.name, genesis)}${o.broken ? ' <i class="bx">BROKEN</i>' : ""}</b><span class="ed">${lines}</span>${warn}</span></span>`;
    };
    const empty = `<span class="evopick empty ${sel === null ? "sel" : ""}" data-tier="${t.tier}" data-id="">
      <span class="einfo"><b class="en">None</b><span class="ed"><div>${t.tier === 1 ? "no Incarnon Form — the weapon stays in its base form" : "nothing installed at this tier"}</div></span></span></span>`;
    rows.push(`<div class="evo"><span class="rank">${roman[t.tier] || "EVO " + t.tier}</span><div class="picks">${t.options.map(card).join("")}${empty}</div></div>`);
  }
  $("evo-rows").innerHTML = rows.join("");
  $("evo-rows").querySelectorAll(".evopick").forEach((c) => c.addEventListener("click", () => {
    evoSel[Number(c.dataset.tier)] = c.dataset.id || null;
    renderEvo(); refreshPanel();
  }));
}

init().catch((e) => { document.querySelector(".config-page").insertAdjacentHTML("afterbegin", `<div class="error">failed to load: ${e}</div>`); });
