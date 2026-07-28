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
// Sim scenario + per-buff config. Seeded from META.defaults in init().
// `buffs` maps buff id -> { stacks, locked } (section 2); the buff SET comes
// from /api/panel and syncs as the build changes.
let sim = { enemy: "thrax_centurion", level: 9999, steel_path: true, headshot_pct: 100,
  duration: 120, runs: 300, form: "incarnon_cycle", buffs: {} };
// The current build's configurable buffs (from the last /api/panel response).
let buffList = [];
// Optimizer scope, 8 + 1 slots in TWO blocks: `mods` (id -> "search"|"fixed")
// scopes the MAIN 8 slots — exilus-flagged mods may sit here too, all 9
// slots accept them (game rule); `exilus` (same states, exilus-eligible mods
// only) scopes the +1 exilus slot — "search" = a slot option next to "leave
// empty", "fixed" = pin it (max one). Plus the arcane set and per-tier
// evolution option sets. Enemy + buffs are shared with the Sim panel
// (`sim`). Seeded from the current build on weapon change.
let opt = { mods: {}, exilus: {}, arcanes: {}, evos: {}, size: 8, buffs: {} };
let optSeeded = false;
// Buffs across the WHOLE optimizer scope (from /api/opt-buffs), + a debounce.
let optBuffList = [];
let optBuffTimer = null;
// Sort/polarity prefs for the optimizer mod list (independent of the picker's).
let optPrefs = { sort: "name", dir: "asc", pol: null };
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
  sim = { enemy: d.enemy, level: d.level, steel_path: d.steel_path,
    headshot_pct: d.headshot_pct, duration: d.duration, runs: d.runs,
    form: d.form, buffs: {} };
  applyWeapon(d.weapon, d.mods);

  $("weapon").addEventListener("change", () => applyWeapon($("weapon").value, null));
  $("run-sim").addEventListener("click", runSim);
  $("run-opt").addEventListener("click", runOptimize);
  $("opt-mod-filter").addEventListener("input", renderOptModList);
  $("opt-arc-filter").addEventListener("input", renderOptArcanes);
  $("opt-size").addEventListener("input", () => {
    opt.size = Math.max(1, Math.min(8, Number($("opt-size").value) || 8));
    updateOptEstimate();
  });
  initPresets();
  reattachOptimize(); // resume progress display if a server-side job survives a reload
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
    sim: { ...sim },
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
  if (st.sim) sim = { ...sim, ...st.sim };
  renderMods(); renderArcanes(); renderEvo(); renderSim(); refreshPanel();
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
  buffList = []; // rebuilt from the next /api/panel response for this build
  opt = { mods: {}, exilus: {}, arcanes: {}, evos: {}, size: 8, buffs: {} }; optSeeded = false; optBuffList = []; // reset scope
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

  renderMods(); renderArcanes(); renderEvo(); renderSim(); renderOpt();
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
  // Innate polarities are never destroyed by the auto plan (blanking one
  // costs a Forma): leftovers go back onto mod-less slots — preferring
  // their original position — else sit on an unmatched modded slot
  // (+25% mismatch drain beats paying a Forma to remove them). An empty
  // build therefore reports 0 Forma.
  for (const p of pool) {
    let k = slots.findIndex((s, i) => !s.mod && !s.pol && innate[i] === p);
    if (k < 0) k = slots.findIndex((s) => !s.mod && !s.pol);
    if (k < 0) k = slots.findIndex((s) => !s.pol);
    if (k < 0) break;
    slots[k].pol = p;
  }
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

// The current build as a request payload — shared by /api/panel and
// /api/simulate so the stats panel and the sim always agree on the loadout.
// Mods keep slot order (elements are position-sensitive).
function buildPayload() {
  return {
    weapon: $("weapon").value,
    evolutions: Object.values(evoSel).filter(Boolean),
    arcane,
    arcane_rank: arcaneRank,
    mods: slots.filter((s) => s.mod).map((s) => s.mod),
  };
}

// ---- Stats panel: merged buckets, each explained by source ----
let panelTimer = null;
function refreshPanel() {
  clearTimeout(panelTimer);
  panelTimer = setTimeout(async () => {
    const body = buildPayload();
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
  $("stats-sub").textContent = `max-rank values · ${r.policy}`;
  const srcLine = (s) =>
    `<div class="ssrc">${s.value} — ${s.mod}${s.note ? ` <span class="snote">(${s.note})</span>` : ""}</div>`;
  const rowHtml = (row) => `
    <div class="srow">
      <div class="shead"><span class="sk">${row.label}</span>
        <span class="sv">${row.base !== "—" && row.base !== row.final ? `<span class="sbase">${row.base}</span> → ` : ""}<b>${row.final}</b></span></div>
      ${row.note ? `<div class="srownote">⚙ ${row.note}</div>` : ""}
      ${(row.sources || []).map(srcLine).join("")}
    </div>`;
  const dmgHtml = (f) => (f.damage && f.damage.length)
    ? `<div class="sdmg-title">Damage (combined) — ${f.damage_total} total</div>` +
      f.damage.map((d) => `<div class="sdmg"><span class="sk">${d.type}</span><span class="sv"><b>${d.amount}</b> <span class="snote">${d.share}</span></span></div>`).join("")
    : "";
  // EVERY available form renders as its own section (base + Incarnon side
  // by side — no switching), headed by the form name + trigger mechanics.
  // Indirect stats (recoil, accuracy, ammo…) render like any bucket — they
  // are outside theoretical DPS but real in practice, so the panel states them.
  const section = (f) => `
    <div class="fsec">
      <div class="fhead">${f.label}<span class="fmeta">${f.meta}</span></div>
      ${[...(f.stats || []), ...(f.elements || []), ...(f.indirect || [])].map(rowHtml).join("")}
      ${dmgHtml(f)}
    </div>`;
  $("stats-rows").innerHTML = (r.forms || []).map(section).join("");
  $("stats-damage").innerHTML = "";

  $("stats-conditionals").innerHTML = (r.conditionals && r.conditionals.length)
    ? `<div class="sdmg-title">Conditional / not merged</div>` +
      r.conditionals.map((c) => `<div class="scond ${c.active ? "" : "off"}"><b>${c.mod}</b>: ${c.desc} <span class="snote">${c.why}</span></div>`).join("")
    : "";
  // The build's configurable buffs (weapon-scoped) drive the Sim section 2.
  buffList = r.buffs || [];
  renderSimBuffs();
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
      <span class="einfo"><b class="en">None</b><span class="ed"><div>nothing installed at this tier</div></span></span></span>`;
    // None comes FIRST (the default state is a bare weapon).
    rows.push(`<div class="evo"><span class="rank">${roman[t.tier] || "EVO " + t.tier}</span><div class="picks">${empty}${t.options.map(card).join("")}</div></div>`);
  }
  $("evo-rows").innerHTML = rows.join("");
  $("evo-rows").querySelectorAll(".evopick").forEach((c) => c.addEventListener("click", () => {
    evoSel[Number(c.dataset.tier)] = c.dataset.id || null;
    renderEvo(); refreshPanel();
  }));
}

// ---- Sim: scenario/buff settings + run against an enemy -----------------
// The build (mods/arcane/evolutions) comes from buildPayload(); this block
// only owns the scenario + engine-modeled buff levers (`sim`). Run POSTs to
// /api/simulate and renders a summary card + an illustrative arena replay.
function renderSim() {
  if (!META) return;
  const w = weaponInfo($("weapon").value);
  if (!w.uses_evo2) sim.form = "base"; // non-transforming weapons: single form
  const enemies = META.enemies || [];
  const en = enemies.find((e) => e.id === sim.enemy) || enemies[0];
  if (en) sim.enemy = en.id;
  const eopts = enemies.map((e) =>
    `<option value="${e.id}" ${e.id === sim.enemy ? "selected" : ""}>${e.name} · Lv ${e.base_level}</option>`).join("");
  // Section 1 — the enemy / scenario.
  $("sim-enemy").innerHTML = `
    <label>Enemy <select data-k="enemy">${eopts}</select></label>
    <label>Level <input type="number" data-k="level" min="1" max="9999" value="${sim.level}"></label>
    <label class="check"><input type="checkbox" data-k="steel_path" ${sim.steel_path ? "checked" : ""}> Steel Path</label>`;
  // Section 3 — how to test.
  const formField = w.uses_evo2 ? `
    <label>Form
      <select data-k="form">
        <option value="incarnon_cycle" ${sim.form === "incarnon_cycle" ? "selected" : ""}>Incarnon cycle</option>
        <option value="incarnon" ${sim.form === "incarnon" ? "selected" : ""}>Incarnon only</option>
        <option value="base" ${sim.form === "base" ? "selected" : ""}>Base only</option>
      </select></label>` : "";
  $("sim-run").innerHTML = `
    <label>Headshot % <input type="number" data-k="headshot_pct" min="0" max="100" value="${sim.headshot_pct}"></label>
    <label>Duration (s) <input type="number" data-k="duration" min="1" max="3600" value="${sim.duration}"></label>
    <label>Runs <input type="number" data-k="runs" min="1" max="20000" value="${sim.runs}"></label>
    ${formField}`;
  [$("sim-enemy"), $("sim-run")].forEach((box) =>
    box.querySelectorAll("[data-k]").forEach((el) => {
      el.addEventListener("change", () => {
        const k = el.dataset.k;
        if (el.type === "checkbox") sim[k] = el.checked;
        else if (el.type === "number") sim[k] = Number(el.value);
        else sim[k] = el.value;
        if (k === "enemy") $("arena-ename").textContent = (enemies.find((e) => e.id === sim.enemy) || {}).name || "Enemy";
      });
    }));
  $("arena-ename").textContent = en ? en.name : "Enemy";
  $("sim-sub").textContent = "current build vs the enemy";
  renderSimBuffs();
}

// Section 2 — one card per configurable buff of the current build (from the
// last /api/panel `buffs`). Each: initial stacks (stepper / on-off) + lock.
// Missing configs default to the buff's `default_*`; ids no longer present are
// dropped from the payload (kept in `sim.buffs` for preset round-trips).
function syncBuffConfig(list, cfg) {
  list.forEach((b) => {
    if (!cfg[b.id]) cfg[b.id] = { stacks: b.default_stacks, locked: b.default_locked };
    else cfg[b.id].stacks = Math.min(cfg[b.id].stacks, b.max_stacks);
  });
}

// Shared buff-card renderer (Sim panel + Optimizer scope). `list` = the buff
// metadata; `cfg` = the mutated config map; `feveredShown` gates the static
// Fevered Frenzy info row.
function renderBuffCards(box, list, cfg, feveredShown) {
  if (!box) return;
  syncBuffConfig(list, cfg);
  if (!list.length) {
    box.innerHTML = `<div class="sim-empty">no configurable buffs here.</div>`;
    return;
  }
  const card = (b) => {
    const c = cfg[b.id];
    const ctl = b.kind === "toggle"
      ? `<label class="bchk"><input type="checkbox" data-b="${b.id}" data-f="stacks" ${c.stacks > 0 ? "checked" : ""}> active</label>`
      : `<span class="bstep"><input type="number" data-b="${b.id}" data-f="stacks" min="0" max="${b.max_stacks}" value="${c.stacks}"><span class="bmax">/ ${b.max_stacks}</span></span>`;
    return `<div class="buff-card">
      <span class="bn">${b.name}</span>
      <span class="bctl">${ctl}</span>
      <label class="block-lock" title="lock = permanent 100% uptime"><input type="checkbox" data-b="${b.id}" data-f="locked" ${c.locked ? "checked" : ""}> lock</label>
    </div>`;
  };
  const fevered = feveredShown
    ? `<div class="buff-card info"><span class="bn">Fevered Frenzy</span><span class="bctl">multishot</span><span class="binfo">always on</span></div>`
    : "";
  box.innerHTML = list.map(card).join("") + fevered;
  box.querySelectorAll("[data-b]").forEach((el) => {
    el.addEventListener("change", () => {
      const id = el.dataset.b, f = el.dataset.f, c = cfg[id];
      if (f === "locked") c.locked = el.checked;
      else if (el.type === "checkbox") c.stacks = el.checked ? 1 : 0;
      else c.stacks = Math.max(0, Number(el.value));
    });
  });
}

function renderSimBuffs() {
  renderBuffCards($("sim-buffs"), buffList, sim.buffs, Object.values(evoSel).includes("dt_fevered_frenzy"));
}

async function runSim() {
  const btn = $("run-sim");
  btn.disabled = true; btn.textContent = "Simulating…";
  $("sim-results").innerHTML = `<div class="placeholder">running ${sim.runs} simulations…</div>`;
  try {
    // Send only the buffs the current build actually has (ids in buffList).
    const buffs = {};
    buffList.forEach((b) => { const c = sim.buffs[b.id]; if (c) buffs[b.id] = { stacks: c.stacks, locked: c.locked }; });
    const body = { ...buildPayload(), ...sim, buffs };
    const r = await (await fetch("/api/simulate", {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
    })).json();
    if (!r || r.ok === false) {
      $("sim-results").innerHTML = `<div class="error">sim failed: ${r ? r.error : "no data"}</div>`;
      return;
    }
    renderResults(r);
    animateArena(r);
  } catch (e) {
    $("sim-results").innerHTML = `<div class="error">sim failed: ${e}</div>`;
  } finally {
    btn.disabled = false; btn.textContent = "Run Simulation";
  }
}

function renderResults(r) {
  const t = r.target || {};
  const pc = (x) => ((x || 0) * 100).toFixed(1) + "%";
  const n0 = (x) => Math.round(x || 0).toLocaleString();
  const n1 = (x) => (x || 0).toFixed(1);
  const killed = (r.kills || 0) >= 1;
  const ehp = (t.overguard || 0) + (t.shield || 0) + (t.health || 0);
  const ttk = r.effective_dps > 0 ? ehp / r.effective_dps : Infinity;
  const heroNum = killed ? n1(r.kills) : pc(r.score);
  const heroSub = killed
    ? `kills in ${n0(r.duration)}s · ~${isFinite(ttk) ? ttk.toFixed(2) : "∞"}s to first kill`
    : `of one ${t.name || "enemy"}'s EHP in ${n0(r.duration)}s (not killed)`;
  const f = r.forma || {};
  const fbadge = f.legal
    ? `<span class="forma-badge legal"><b>${f.used} Forma</b>${f.total_drain}/${f.cap} drain</span>`
    : `<span class="forma-badge illegal"><b>illegal</b>${f.error || "over capacity"}</span>`;
  const kpi = (l, v) => `<div class="kpi"><div class="kv">${v}</div><div class="kl">${l}</div></div>`;
  const kpis = [
    kpi("DPS", n0(r.dps)), kpi("Effective DPS", n0(r.effective_dps)),
    kpi("Crit rate", pc(r.crit_rate)), kpi("Orange+ crit", pc(r.big_crit_rate)),
    kpi("Headshot rate", pc(r.headshot_rate)), kpi("Procs / run", n1(r.procs)),
    kpi("DoT dmg", n0(r.dot)), kpi("Reloads", n1(r.reloads)),
  ].join("");
  const dmg = (r.panel && r.panel.damage) || [];
  const total = dmg.reduce((a, d) => a + d.value, 0) || 1;
  const seg = dmg.map((d, i) => `<div class="dmg-seg" style="width:${(d.value / total * 100).toFixed(1)}%;background:var(--s${(i % 8) + 1})" title="${d.type}: ${n0(d.value)}"></div>`).join("");
  const legend = dmg.map((d, i) => `<span class="li"><span class="dmgic" style="background:var(--s${(i % 8) + 1})"></span>${d.type} <span class="lv">${n0(d.value)}</span></span>`).join("");
  const row = (k, v) => `<div class="row"><span class="k">${k}</span><span class="v">${v}</span></div>`;
  const detail = [
    row("Target", `${t.name || "?"} · Lv ${t.level}${t.steel_path ? " (SP)" : ""}`),
    row("OG / Shield / Health", `${n0(t.overguard)} / ${n0(t.shield)} / ${n0(t.health)}`),
    row("Armor", n0(t.armor)),
    row("Shots / Pellets", `${n1(r.shots)} / ${n1(r.pellets)}`),
    row("Kills min–max (±σ)", `${r.kills_min}–${r.kills_max} (±${n1(r.kills_std)})`),
    row("Transforms", n1(r.transforms)),
    row("Runs", n0(r.runs)),
  ].join("");
  $("sim-results").innerHTML = `
    <div class="results">
      <div class="hero"><div><div class="hero-label">Result</div><div class="hero-num">${heroNum}</div><div class="hero-sub">${heroSub}</div></div>${fbadge}</div>
      <div class="kpi-row">${kpis}</div>
      <h3>Damage per shot (combined)</h3>
      <div class="dmg-bar">${seg}</div>
      <div class="legend">${legend}</div>
      <h3>Detail</h3>
      <div class="stat-table">${detail}</div>
    </div>`;
}

// Illustrative arena replay: drain the enemy's layered bars over a fixed
// visual window (NOT the simulated kill time — the sim is Monte-Carlo over
// many runs, so the headline number is authoritative; this only shows the
// enemy getting beaten).
function animateArena(r) {
  const t = r.target || {};
  const layers = [
    { k: "og", v: t.overguard || 0 }, { k: "sh", v: t.shield || 0 }, { k: "hp", v: t.health || 0 },
  ].filter((l) => l.v > 0);
  $("arena-bars").innerHTML = layers.map((l) =>
    `<div class="bar ${l.k}"><div class="fill" style="width:100%"></div></div>`).join("");
  const killed = (r.kills || 0) >= 1;
  const frac = killed ? 1 : Math.max(0, Math.min(1, r.score || 0));
  const remain = ((1 - frac) * 100).toFixed(1) + "%";
  const dmg = (r.panel && r.panel.damage) || [];
  const chips = dmg.map((d, i) => `<span class="chip" style="--c:var(--s${(i % 8) + 1})">${d.type}</span>`);
  if ((r.procs || 0) > 0) chips.push(`<span class="chip proc">⚡ ${Math.round(r.procs)} procs</span>`);
  $("arena-status").innerHTML = chips.join("");
  const enemy = $("arena-enemy"), tracer = $("arena-tracer");
  enemy.classList.remove("defeated");
  tracer.classList.remove("firing");
  void tracer.offsetWidth; // force reflow so the animation restarts each run
  tracer.classList.add("firing");
  requestAnimationFrame(() => {
    $("arena-bars").querySelectorAll(".fill").forEach((f) => { f.style.width = remain; });
    if (killed) setTimeout(() => enemy.classList.add("defeated"), 1500);
  });
}

// ---- Optimizer: scope (mods/arcanes/evolutions) → top-10 builds ---------
// Mod scope is a 3-state cycle (off / pool / required); the client estimates
// the candidate count (no cap — the server funnel culls large spaces).
function nChooseK(n, k) {
  if (k < 0 || k > n) return 0;
  k = Math.min(k, n - k);
  let r = 1;
  for (let i = 0; i < k; i++) r = (r * (n - i)) / (i + 1);
  return r;
}

// ---- scope mutex helpers -----------------------------------------------
// Family exclusivity is a GAME rule across all 9 slots: req'ing a mod kills
// its family siblings everywhere (e.g. req Primed Pistol Gambit → plain
// Pistol Gambit can be neither pool nor req). The UI greys them out; setting
// a req actively clears conflicting marks so the scope never lies.
function famReqBy(m) {
  if (!m || !m.family) return null;
  const hit = (map) => Object.keys(map).find((id) => map[id] === "fixed" && id !== m.id && (modById(id) || {}).family === m.family);
  return hit(opt.mods) || hit(opt.exilus) || null;
}
function clearFamMarks(id) {
  const m = modById(id);
  if (!m || !m.family) return;
  [opt.mods, opt.exilus].forEach((map) => Object.keys(map).forEach((o) => {
    if (o !== id && (modById(o) || {}).family === m.family) delete map[o];
  }));
}
const reqCountMain = () => Object.values(opt.mods).filter((s) => s === "fixed").length;
const exilusPinned = () => Object.keys(opt.exilus).find((id) => opt.exilus[id] === "fixed") || null;
const arcanePinned = () => Object.keys(opt.arcanes).find((id) => opt.arcanes[id] === "fixed") || null;
const evoPinned = (tier) => { const m = opt.evos[tier] || {}; return Object.keys(m).find((id) => m[id] === "fixed") || null; };

function renderOpt() {
  if (!META || weaponInfo($("weapon").value).id !== "dual_toxocyst") {
    show("opt-block", weaponInfo($("weapon").value).id === "dual_toxocyst");
    if (weaponInfo($("weapon").value).id !== "dual_toxocyst") return;
  }
  // Seed scope from the current build once: equipped mods = fixed.
  if (!optSeeded) {
    opt.mods = {}; opt.exilus = {};
    // Everything equipped seeds as REQ (pinned) — the scope mirrors the
    // build until the user opens dimensions up.
    slots.slice(0, 8).forEach((s) => { if (s.mod) opt.mods[s.mod] = "fixed"; });
    if (slots[EXILUS].mod) opt.exilus[slots[EXILUS].mod] = "fixed";
    opt.arcanes = arcane && arcane !== "none" ? { [arcane]: "fixed" } : {};
    opt.evos = {};
    Object.entries(evoSel).forEach(([t, id]) => { if (id) opt.evos[t] = { [id]: "fixed" }; });
    optSeeded = true;
  }
  renderOptMods();
  renderOptPresetBar();
  renderOptExilus();
  renderOptArcanes();
  renderOptEvos();
  updateOptEstimate();
  fetchOptBuffs();
}

// The buffs across the WHOLE scope (union of every fixed/search mod + every
// searched arcane) — enumerated server-side; the optimizer configures these,
// NOT the current build's buffs. Debounced as the scope changes.
function fetchOptBuffs() {
  clearTimeout(optBuffTimer);
  optBuffTimer = setTimeout(async () => {
    try {
      const r = await (await fetch("/api/opt-buffs", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ weapon: $("weapon").value, mods: opt.mods, arcanes: Object.keys(opt.arcanes) }),
      })).json();
      optBuffList = (r && r.buffs) || [];
    } catch (_) { optBuffList = []; }
    renderOptBuffs();
  }, 250);
}

function renderOptBuffs() {
  const fev = Object.values(opt.evos).some((m) => m && m.dt_fevered_frenzy);
  renderBuffCards($("opt-buffs"), optBuffList, opt.buffs, fev);
}

// The mod scope: the SAME rich list as the mod picker (image, polarity icon,
// sort / polarity filter, effect lines) — the only difference is the rightmost
// control is pool/req instead of the drain. Plus a summary of selections and
// a "mods per build" size. `optPrefs` mirrors the picker's sort/filter.
function renderOptMods() {
  $("opt-size").value = opt.size;
  renderOptTools();
  renderOptModSel();
  renderOptModList();
}

// Exilus-slot scope (the +1 slot) — exilus-eligible mods with the same
// pool/req segs as the main list: pool = a slot option (empty always
// allowed), req = pin the slot (max one). The same mods may ALSO be marked
// in the main scope above — all 9 slots accept exilus mods; the search
// never equips one twice.
function renderOptExilus() {
  const pinned = exilusPinned();
  const hasPool = Object.values(opt.exilus).some((s) => s === "search");
  // "None" is a first-class option: pool it to keep "leave empty" among the
  // searched options, req it to pin the slot empty. Marked pools OCCUPY the
  // slot — empty is never an implicit extra next to pooled mods.
  const noneRow = (() => {
    const st = opt.exilus["none"] || "off";
    const poolDead = pinned && pinned !== "none" && st !== "search";
    const reqDead = hasPool && st !== "fixed";
    return `<div class="opt ${st === "off" ? "" : st}">
      <span class="mod none">∅</span>
      <div class="info"><div class="mn">None</div><div class="me"><div>leave the exilus slot empty (no drain)</div></div></div>
      <div class="oseg">
        <span class="seg ${st === "search" ? "on" : ""} ${poolDead ? "dis" : ""}" data-m="none" data-s="search">pool</span>
        <span class="seg ${st === "fixed" ? "on" : ""} ${reqDead ? "dis" : ""}" data-m="none" data-s="fixed" ${reqDead ? 'title="clear the pool marks first — req pins the slot"' : ""}>req</span>
      </div>
    </div>`;
  })();
  const row = (m) => {
    const st = opt.exilus[m.id] || "off";
    const fam = famReqBy(m);
    // Family conflict kills the row. One slot: a pin kills other pools; any
    // pool mark kills req (pooling = the slot is open for search — pinning
    // would silently discard that). ON segs stay clickable to toggle off.
    const poolDead = !!fam || (pinned && pinned !== m.id && st !== "search");
    const reqDead = !!fam || (hasPool && st !== "fixed");
    const why = fam ? `excluded: ${(modById(fam) || { name: fam }).name} is required (same family)` : "";
    const eff = (descAt(m, m.max_rank) || m.effects || []).map((x) => `<div>${x}</div>`).join("");
    return `<div class="opt ${st === "off" ? "" : st} ${fam ? "dis-soft" : ""} ${m.rarity ? "rar-" + m.rarity : ""}" title="${why}">
      ${imgTag(POL(m.polarity), "pol")}${imgTag(m.image ? CDN + m.image : null, "mod")}
      <div class="info"><div class="mn">${wl(m.name)}</div><div class="me">${eff}</div></div>
      <div class="oseg">
        <span class="seg ${st === "search" ? "on" : ""} ${poolDead ? "dis" : ""}" data-m="${m.id}" data-s="search">pool</span>
        <span class="seg ${st === "fixed" ? "on" : ""} ${reqDead ? "dis" : ""}" data-m="${m.id}" data-s="fixed" ${reqDead && !fam ? 'title="clear the pool marks first — req pins the slot"' : ""}>req</span>
      </div>
    </div>`;
  };
  $("opt-exilus").innerHTML = noneRow + (currentPool.filter((m) => m.exilus).map(row).join("")
    || `<div class="opt dis">no exilus mods in this pool</div>`);
  $("opt-exilus").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const id = el.dataset.m, want = el.dataset.s, cur = opt.exilus[id] || "off";
      if (cur === want) { delete opt.exilus[id]; }
      else {
        if (want === "fixed") { // one slot — req is a radio
          Object.keys(opt.exilus).forEach((o) => { if (opt.exilus[o] === "fixed") delete opt.exilus[o]; });
        }
        opt.exilus[id] = want;
        if (want === "fixed") clearFamMarks(id);
      }
      renderOptMods(); renderOptExilus(); updateOptEstimate();
    }));
}

// Arcane scope — the SAME rich rows as the arcane picker (image, name, effect
// lines), searchable, with an include toggle on the right. "None" included.
function renderOptArcanes() {
  const q = ($("opt-arc-filter") && $("opt-arc-filter").value || "").trim().toLowerCase();
  const allEff = (a) => (a.ranks || []).reduce((acc, r) => acc.concat(r), []).concat(a.desc_ranks || []).join(" ").toLowerCase();
  const hits = (META.arcanes || []).filter((a) => a.id === "none" || !q || a.name.toLowerCase().includes(q) || allEff(a).includes(q));
  const pinned = arcanePinned();
  const hasPool = Object.values(opt.arcanes).some((s) => s === "search");
  $("opt-arcanes").innerHTML = hits.map((a) => {
    const st = opt.arcanes[a.id] || "off", none = a.id === "none";
    // One arcane slot: pool and req are exclusive GROUP states — a pin
    // kills every other pool; any pool mark kills req (pooling means the
    // slot is open for search; pinning would silently discard that, so it
    // is blocked until the pools are cleared). An ON seg always stays
    // clickable — it must be able to toggle itself off.
    const poolDead = pinned && pinned !== a.id && st !== "search";
    const reqDead = hasPool && st !== "fixed";
    const eff = none ? "" : effLines(descAt(a, a.max_rank) || effectsAt(a, a.max_rank));
    return `<div class="opt ${a.rarity ? "rar-" + a.rarity : ""} ${st === "off" ? "" : st}">
      ${none ? '<span class="mod none">∅</span>' : imgTag(a.image ? CDN + a.image : null, "mod")}
      <div class="info"><div class="mn">${none ? a.name : wl(a.name)}</div>${eff}</div>
      <div class="oseg">
        <span class="seg ${st === "search" ? "on" : ""} ${poolDead ? "dis" : ""}" data-a="${a.id}" data-s="search">pool</span>
        <span class="seg ${st === "fixed" ? "on" : ""} ${reqDead ? "dis" : ""}" data-a="${a.id}" data-s="fixed" ${reqDead ? 'title="clear the pool marks first — req pins the slot"' : ""}>req</span>
      </div>
    </div>`;
  }).join("");
  $("opt-arcanes").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const id = el.dataset.a, want = el.dataset.s, cur = opt.arcanes[id] || "off";
      if (cur === want) { delete opt.arcanes[id]; }
      else {
        if (want === "fixed") { // one slot — req is a radio
          Object.keys(opt.arcanes).forEach((o) => { if (opt.arcanes[o] === "fixed") delete opt.arcanes[o]; });
        }
        opt.arcanes[id] = want;
      }
      renderOptArcanes(); updateOptEstimate(); fetchOptBuffs();
    }));
}

// Evolution scope — per tier, the option rows with their verbatim description
// and a search toggle (broken evolutions flagged).
function renderOptEvos() {
  const roman = ["", "I", "II", "III", "IV"];
  $("opt-evos").innerHTML = (META.evolutions || []).map((t) => {
    const sel = opt.evos[t.tier] || {};
    const pinned = evoPinned(t.tier);
    const hasPool = Object.values(sel).some((s) => s === "search");
    const rows = t.options.map((o) => {
      const st = sel[o.id] || "off";
      // One choice per tier: a pin kills other pools; any pool mark kills
      // req (pooling = the tier is open for search). ON segs stay clickable.
      const poolDead = pinned && pinned !== o.id && st !== "search";
      const reqDead = hasPool && st !== "fixed";
      const desc = (o.desc || o.effects || []).map((x) => `<div>${x}</div>`).join("");
      return `<div class="opt ${st === "off" ? "" : st} ${o.broken ? "dis-soft" : ""}">
        <div class="info"><div class="mn">${o.name}${o.broken ? ' <span class="exchip brk">BROKEN</span>' : ""}</div><div class="me">${desc}</div></div>
        <div class="oseg">
          <span class="seg ${st === "search" ? "on" : ""} ${poolDead ? "dis" : ""}" data-t="${t.tier}" data-e="${o.id}" data-s="search">pool</span>
          <span class="seg ${st === "fixed" ? "on" : ""} ${reqDead ? "dis" : ""}" data-t="${t.tier}" data-e="${o.id}" data-s="fixed" ${reqDead ? 'title="clear the pool marks first — req pins the tier"' : ""}>req</span>
        </div>
      </div>`;
    }).join("");
    return `<div class="opt-tier-block"><div class="opt-tier-h">EVO ${roman[t.tier]}</div><div class="combo-menu opt-evolist">${rows}</div></div>`;
  }).join("");
  $("opt-evos").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const t = el.dataset.t, id = el.dataset.e, want = el.dataset.s;
      opt.evos[t] = opt.evos[t] || {};
      const cur = opt.evos[t][id] || "off";
      if (cur === want) { delete opt.evos[t][id]; }
      else {
        if (want === "fixed") { // one choice per tier — req is a radio
          Object.keys(opt.evos[t]).forEach((o) => { if (opt.evos[t][o] === "fixed") delete opt.evos[t][o]; });
        }
        opt.evos[t][id] = want;
      }
      renderOptEvos(); updateOptEstimate(); fetchOptBuffs();
    }));
}

// ---- Optimizer config presets (localStorage; independent of build presets)
const OPT_PRESET_KEY = "wfsim-opt-presets";
const OPT_PRESET_MAX = 10;
let activeOptPreset = null;
const loadOptPresets = () => { try { const p = JSON.parse(localStorage.getItem(OPT_PRESET_KEY)); return Array.isArray(p) ? p : []; } catch (_) { return []; } };
const storeOptPresets = (ps) => localStorage.setItem(OPT_PRESET_KEY, JSON.stringify(ps));
const snapshotOpt = () => ({ mods: { ...opt.mods }, exilus: { ...opt.exilus }, arcanes: { ...opt.arcanes }, evos: JSON.parse(JSON.stringify(opt.evos)), size: opt.size, buffs: JSON.parse(JSON.stringify(opt.buffs)) });
function restoreOpt(st) {
  opt = { mods: { ...(st.mods || {}) }, exilus: typeof st.exilus === "object" && st.exilus ? { ...st.exilus } : {}, arcanes: { ...(st.arcanes || {}) }, evos: JSON.parse(JSON.stringify(st.evos || {})), size: st.size || 8, buffs: JSON.parse(JSON.stringify(st.buffs || {})) };
  // Migrations from the short-lived earlier scope formats — ONLY when the
  // preset predates the exilus map (a new-format preset may legitimately
  // keep exilus mods in the main scope: all 9 slots accept them).
  if (typeof st.exilus !== "object" || !st.exilus) {
    if (typeof st.exilus === "string" && st.exilus) opt.exilus[st.exilus] = "fixed"; // single-select era
    Object.keys(opt.mods).forEach((id) => { // merged-list era
      const m = modById(id);
      if (m && m.exilus && !(id in opt.exilus)) { opt.exilus[id] = opt.mods[id]; delete opt.mods[id]; }
    });
  }
  // Boolean-era arcane/evo selections ({id: true}) become pool marks.
  Object.keys(opt.arcanes).forEach((id) => { if (opt.arcanes[id] === true) opt.arcanes[id] = "search"; });
  Object.values(opt.evos).forEach((m) => Object.keys(m).forEach((id) => { if (m[id] === true) m[id] = "search"; }));
  optSeeded = true;
  renderOpt(); fetchOptBuffs(); updateOptEstimate();
}
function renderOptPresetBar() {
  const bar = $("opt-preset-bar");
  const ps = loadOptPresets();
  const chip = (p) => {
    const active = p.name === activeOptPreset;
    const ops = active
      ? `<button class="pop upd" title="overwrite with the current scope">↻</button><button class="pop ren" title="rename">✎</button><button class="pop del" title="delete">✕</button>`
      : "";
    return `<span class="pchip ${active ? "sel" : ""}" data-name="${escHtml(p.name)}" title="load ${escHtml(p.name)}">${escHtml(p.name)}${ops}</span>`;
  };
  bar.innerHTML =
    `<span class="plabel">Scope presets <b>${ps.length}/${OPT_PRESET_MAX}</b></span>` +
    ps.map(chip).join("") +
    (ps.length < OPT_PRESET_MAX ? `<span class="pchip add" title="save the current optimizer scope">+ save</span>` : "");
  bar.querySelectorAll(".pchip:not(.add)").forEach((c) => c.addEventListener("click", () => {
    const p = loadOptPresets().find((x) => x.name === c.dataset.name);
    if (p) { activeOptPreset = p.name; restoreOpt(p.state); }
  }));
  const addBtn = bar.querySelector(".pchip.add");
  if (addBtn) addBtn.addEventListener("click", () => {
    const ps2 = loadOptPresets();
    const name = (prompt("Scope preset name:", `scope ${ps2.length + 1}`) || "").trim();
    if (!name) return;
    const at = ps2.findIndex((p) => p.name === name);
    const entry = { name, savedAt: Date.now(), state: snapshotOpt() };
    if (at >= 0) ps2[at] = entry; else ps2.push(entry);
    storeOptPresets(ps2); activeOptPreset = name; renderOptPresetBar();
  });
  const on = (sel, fn) => { const b = bar.querySelector(sel); if (b) b.addEventListener("click", (e) => { e.stopPropagation(); fn(); }); };
  on(".pop.upd", () => {
    const ps2 = loadOptPresets(); const at = ps2.findIndex((p) => p.name === activeOptPreset);
    if (at < 0) return; ps2[at] = { name: activeOptPreset, savedAt: Date.now(), state: snapshotOpt() }; storeOptPresets(ps2); renderOptPresetBar();
  });
  on(".pop.ren", () => {
    const name = (prompt("New name:", activeOptPreset) || "").trim();
    if (!name || name === activeOptPreset) return;
    const ps2 = loadOptPresets(); const at = ps2.findIndex((p) => p.name === activeOptPreset);
    if (at < 0) return; ps2[at].name = name; storeOptPresets(ps2); activeOptPreset = name; renderOptPresetBar();
  });
  on(".pop.del", () => {
    const ps2 = loadOptPresets().filter((p) => p.name !== activeOptPreset); storeOptPresets(ps2); activeOptPreset = null; renderOptPresetBar();
  });
}

function renderOptTools() {
  const t = $("opt-picker-tools");
  const pols = ["Madurai", "Naramon", "Vazarin", "Umbra"].filter((p) => currentPool.some((m) => m.polarity === p));
  t.innerHTML =
    `<label>Sort <select id="opk-sort"><option value="name">Name</option><option value="drain">Drain</option></select></label>` +
    `<button id="opk-dir" class="ghost-btn small" title="direction">${optPrefs.dir === "asc" ? "▲" : "▼"}</button>` +
    `<span class="pk-pols"><span class="pk-pol ${!optPrefs.pol ? "sel" : ""}" data-p="">all</span>` +
    pols.map((p) => `<span class="pk-pol ${optPrefs.pol === p ? "sel" : ""}" data-p="${p}" title="${p}">${imgTag(POL(p), "pol")}</span>`).join("") +
    `</span>`;
  $("opk-sort").value = optPrefs.sort;
  $("opk-sort").onchange = () => { optPrefs.sort = $("opk-sort").value; renderOptModList(); };
  $("opk-dir").onclick = () => { optPrefs.dir = optPrefs.dir === "asc" ? "desc" : "asc"; renderOptTools(); renderOptModList(); };
  t.querySelectorAll(".pk-pol").forEach((o) => o.onclick = () => { optPrefs.pol = o.dataset.p || null; renderOptTools(); renderOptModList(); });
}

function renderOptModSel() {
  const chip = (id, cls) => {
    const m = modById(id);
    return `<span class="oselchip ${cls}" data-m="${id}" title="click to remove">${m ? m.name : id} ✕</span>`;
  };
  const req = Object.keys(opt.mods).filter((id) => opt.mods[id] === "fixed").map((id) => chip(id, "fixed"));
  const pool = Object.keys(opt.mods).filter((id) => opt.mods[id] === "search").map((id) => chip(id, "search"));
  const box = $("opt-mods-sel");
  box.innerHTML =
    (req.length ? `<div class="oselrow"><span class="osellbl">required (${req.length}/${opt.size})</span>${req.join("")}</div>` : "") +
    (pool.length ? `<div class="oselrow"><span class="osellbl">pool (${pool.length})</span>${pool.join("")}</div>` : "") +
    (!req.length && !pool.length ? `<div class="sim-empty">nothing selected yet — mark mods below as pool or required.</div>` : "");
  box.querySelectorAll("[data-m]").forEach((el) =>
    el.addEventListener("click", () => { delete opt.mods[el.dataset.m]; renderOptMods(); updateOptEstimate(); }));
}

function renderOptModList() {
  const q = ($("opt-mod-filter").value || "").trim().toLowerCase();
  // Exilus mods are IN this list too — all 9 slots accept them (game rule),
  // so marking one here makes it compete for a MAIN slot; the exilus SLOT
  // has its own block below.
  const hits = currentPool
    .filter((m) => !optPrefs.pol || m.polarity === optPrefs.pol)
    .filter((m) => !q || m.name.toLowerCase().includes(q)
      || (m.effects || []).join(" ").toLowerCase().includes(q)
      || (m.desc_ranks || []).join(" ").toLowerCase().includes(q))
    .sort((a, b) => {
      const c = optPrefs.sort === "drain" ? a.drain - b.drain : a.name.localeCompare(b.name);
      return optPrefs.dir === "desc" ? -c : c;
    });
  // The picker's `.opt` row markup verbatim; only the trailing `.dr` is
  // replaced by the pool/req control (`.oseg`). Mutex-aware: a family
  // sibling of a req'd mod is dead (game exclusivity); once required fills
  // every slot, unmarked mods can no longer join; and pooled mods RESERVE
  // one open slot — req may only grow to size−1 while any pool mark exists
  // (pinning the last slot would silently kill the search).
  const fixedN = reqCountMain();
  const poolN = Object.values(opt.mods).filter((s) => s === "search").length;
  const full = fixedN >= opt.size;
  const row = (m) => {
    const st = opt.mods[m.id] || "off";
    const fam = famReqBy(m);
    const dead = !!fam || (full && st === "off");
    // Would req'ing this row leave pooled mods with zero open slots?
    const poolAfter = poolN - (st === "search" ? 1 : 0);
    const reqBlocked = st !== "fixed" && (fixedN + 1 > opt.size - (poolAfter > 0 ? 1 : 0));
    const why = fam ? `excluded: ${(modById(fam) || { name: fam }).name} is required (same family)`
      : dead ? `all ${opt.size} slots are required already` : "";
    const eff = (descAt(m, m.max_rank) || m.effects || []).map((x) => `<div>${x}</div>`).join("");
    return `<div class="opt ${st === "off" ? "" : st} ${dead ? "dis-soft" : ""} ${m.rarity ? "rar-" + m.rarity : ""}" title="${why || (m.effects || []).join(" · ")}">
      ${imgTag(POL(m.polarity), "pol")}${imgTag(m.image ? CDN + m.image : null, "mod")}
      <div class="info"><div class="mn">${wl(m.name)}${m.exilus ? ' <span class="exchip">EXILUS</span>' : ""}</div><div class="me">${eff}</div></div>
      <div class="oseg">
        <span class="seg ${st === "search" ? "on" : ""} ${dead ? "dis" : ""}" data-m="${m.id}" data-s="search">pool</span>
        <span class="seg ${st === "fixed" ? "on" : ""} ${dead || reqBlocked ? "dis" : ""}" data-m="${m.id}" data-s="fixed" ${!dead && reqBlocked ? 'title="pooled mods reserve ≥1 open slot — raise max mods or clear pools"' : ""}>req</span>
      </div>
    </div>`;
  };
  $("opt-mods").innerHTML = hits.length ? hits.map(row).join("") : `<div class="opt dis">no matches</div>`;
  $("opt-mods").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const id = el.dataset.m, want = el.dataset.s, cur = opt.mods[id] || "off";
      if (cur === want) delete opt.mods[id]; else opt.mods[id] = want; // toggle off if same
      if (opt.mods[id] === "fixed") clearFamMarks(id);
      renderOptMods(); renderOptExilus(); updateOptEstimate(); fetchOptBuffs();
    }));
}

function updateOptEstimate() {
  // 8 + 1 slots, slots may stay EMPTY: builds are every subset of the main
  // scope from `required` up to `size` mods (an empty scope = the bare
  // weapon, still a legal search). `opt.exilus` scopes the +1 slot (req
  // pins it, pool adds options next to "empty"); arcanes and evolution
  // tiers are pool/req the same way.
  const fixed = Object.values(opt.mods).filter((s) => s === "fixed").length;
  const search = Object.values(opt.mods).filter((s) => s === "search").length;
  const exFixed = exilusPinned();
  const exSearch = Object.values(opt.exilus).filter((s) => s === "search").length;
  // Pooled exilus marks ARE the option set ("None" is itself a markable
  // option); nothing marked = the slot stays empty (one option).
  const exOptions = exFixed ? 1 : Math.max(1, exSearch);
  // Required in BOTH blocks = impossible (a mod equips once).
  const dupReq = exFixed && opt.mods[exFixed] === "fixed" ? exFixed : null;
  const arcCount = arcanePinned() ? 1
    : Math.max(1, Object.values(opt.arcanes).filter((s) => s === "search").length);
  let evoProduct = 1;
  (META.evolutions || []).forEach((t) => {
    const m = opt.evos[t.tier] || {};
    evoProduct *= evoPinned(t.tier) ? 1
      : Math.max(1, Object.values(m).filter((s) => s === "search").length);
  });
  const size = opt.size;
  // The pool group occupies ≥1 slot: with pools marked, every build carries
  // at least one pooled mod (k starts above the required count).
  const minK = fixed + (search > 0 ? 1 : 0);
  let subsets = 0;
  for (let k = minK; k <= size; k++) subsets += nChooseK(search, k - fixed);
  subsets *= evoProduct * exOptions;
  const jobs = subsets * arcCount;
  // Pooled mods reserve ≥1 open slot (reachable only via shrinking max
  // mods after marking — req clicks are blocked before this point).
  const poolStarved = search > 0 && fixed >= size;
  const valid = fixed <= size && subsets > 0 && !dupReq && !poolStarved;
  const est = Math.round(subsets).toLocaleString();
  // No cap (user: use local resources). Show the estimate + a heads-up when big;
  // only block genuinely invalid scopes.
  const big = jobs > 500000;
  const exNote = exFixed ? " (exilus pinned)" : exOptions > 1 ? ` (× ${exOptions} exilus options)` : "";
  $("opt-estimate").innerHTML = valid
    ? `~<b>${est}</b> candidates${exNote} × ${arcCount} arcanes ≈ ${Math.round(jobs).toLocaleString()} jobs${big ? ` <span class="warn">— large; this may take a while</span>` : ""}`
    : `<span class="warn">${dupReq ? `${(modById(dupReq) || { name: dupReq }).name} is required in both blocks — a mod equips once` : poolStarved ? `pooled mods reserve ≥1 open slot — raise max mods or clear pools` : `more required (${fixed}) than slots (${size})`}</span>`;
  // Never re-enable while a background job is still running.
  $("run-opt").disabled = !valid || optJobId != null;
}

// The optimize run is a BACKGROUND JOB on the server: POST /api/optimize
// returns a job_id immediately; we poll /api/optimize/status for live funnel
// progress (overall % is exact — the schedule fixes every round's sim count
// up front) and can /api/optimize/cancel. On page reload, init() reattaches
// to a still-running job via a no-id status call.
let optJobId = null;
let optPollTimer = null;

const postJson = async (url, body) =>
  (await fetch(url, {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
  })).json();

async function runOptimize() {
  $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
  $("opt-results").innerHTML = `<div class="placeholder">starting…</div>`;
  try {
    // pool/req collapse to effective option lists: a req pins its slot/tier
    // (single option), pools are the searched set.
    const evolutions = {};
    Object.entries(opt.evos).forEach(([t, m]) => {
      const f = Object.keys(m).find((id) => m[id] === "fixed");
      const ids = f ? [f] : Object.keys(m).filter((id) => m[id] === "search");
      if (ids.length) evolutions[t] = ids;
    });
    const arcFixed = arcanePinned();
    const arcs = arcFixed ? [arcFixed] : Object.keys(opt.arcanes).filter((id) => opt.arcanes[id] === "search");
    // Buffs configured over the whole scope (opt.buffs), pruned to the current scope's ids.
    const buffs = {};
    optBuffList.forEach((b) => { const c = opt.buffs[b.id]; if (c) buffs[b.id] = { stacks: c.stacks, locked: c.locked }; });
    const body = {
      weapon: $("weapon").value,
      mods: opt.mods,
      build_size: opt.size,
      arcanes: arcs.length ? arcs : ["none"],
      evolutions,
      exilus: opt.exilus,
      enemy: sim.enemy, level: sim.level, steel_path: sim.steel_path,
      headshot_pct: sim.headshot_pct, duration: sim.duration,
      buffs,
    };
    const r = await postJson("/api/optimize", body);
    if (!r || r.ok === false) {
      optFinish(`<div class="error">optimize failed: ${r ? r.error : "no data"}</div>`);
      return;
    }
    optJobId = r.job_id;
    pollOptimize();
  } catch (e) {
    optFinish(`<div class="error">optimize failed: ${e}</div>`);
  }
}

function optFinish(html) {
  if (optPollTimer) { clearTimeout(optPollTimer); optPollTimer = null; }
  optJobId = null;
  if (html !== undefined) $("opt-results").innerHTML = html;
  $("run-opt").textContent = "Run Optimizer";
  updateOptEstimate(); // re-enables the button when the scope is valid
}

async function pollOptimize() {
  let st;
  try {
    st = await postJson("/api/optimize/status", optJobId != null ? { id: optJobId } : {});
  } catch (e) {
    optFinish(`<div class="error">optimize status failed: ${e}</div>`);
    return;
  }
  if (!st || st.ok === false) {
    optFinish(`<div class="error">optimize failed: ${st ? st.error : "no data"}</div>`);
    return;
  }
  optJobId = st.job_id;
  if (st.phase === "error") {
    optFinish(`<div class="error">optimize failed: ${(st.result && st.result.error) || "unknown error"}</div>`);
    return;
  }
  if (st.phase === "done" || st.phase === "cancelled") {
    optFinish();
    if (st.result && st.result.results && st.result.results.length) {
      renderOptResults(st.result);
    } else {
      $("opt-results").innerHTML = `<div class="placeholder">cancelled before the first round finished — no results</div>`;
    }
    return;
  }
  renderOptProgress(st);
  optPollTimer = setTimeout(pollOptimize, 500);
}

function renderOptProgress(st) {
  const pct = st.sims_planned ? Math.min(100, (100 * st.sims_done) / st.sims_planned) : 0;
  const head = st.phase === "enumerating"
    ? "enumerating candidates…"
    : `round ${st.round}/${st.rounds} — ${(st.round_jobs || 0).toLocaleString()} jobs × ${st.round_runs} runs`;
  const notes = (st.notes || []).map((n) =>
    `<div class="opt-note">round ${n.round}: ${n.jobs.toLocaleString()} × ${n.runs} (${n.by_kills ? "kills" : "eff dmg"}) → keep ${n.kept.toLocaleString()} · best ${n.by_kills ? n.best.toFixed(2) + " kill score" : n.best.toExponential(2) + " eff"} · ${(n.ms / 1000).toFixed(1)}s</div>`
  ).join("");
  const sub = st.phase === "enumerating"
    ? ""
    : `<div class="opt-prog-sub">${pct.toFixed(1)}% · ${st.sims_done.toLocaleString()} / ${st.sims_planned.toLocaleString()} sims${st.candidates ? ` · ${st.candidates.toLocaleString()} candidates × arcanes = ${st.jobs.toLocaleString()} jobs` : ""}</div>`;
  $("opt-results").innerHTML = `<div class="opt-progress">
    <div class="opt-prog-head"><span>${head}</span><span class="opt-elapsed">${st.elapsed_s.toFixed(0)}s</span></div>
    <div class="opt-bar"><i style="width:${pct}%"></i></div>
    ${sub}${notes}
    <button class="ghost-btn small" id="opt-cancel">Cancel</button>
  </div>`;
  $("opt-cancel").addEventListener("click", async () => {
    $("opt-cancel").disabled = true; $("opt-cancel").textContent = "Cancelling…";
    try { await postJson("/api/optimize/cancel", { id: optJobId }); } catch (e) { /* poll reports */ }
  });
}

// Reattach to a job that is still running server-side (e.g. after a page
// reload): a no-id status call returns the latest job.
async function reattachOptimize() {
  try {
    const st = await postJson("/api/optimize/status", {});
    if (st && st.ok !== false && (st.phase === "enumerating" || st.phase === "running")) {
      optJobId = st.job_id;
      $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
      renderOptProgress(st);
      optPollTimer = setTimeout(pollOptimize, 500);
    }
  } catch (e) { /* no server-side job — nothing to reattach */ }
}

const prettify = (id) => id.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
const arcName = (id) => (id === "none" ? "no arcane" : ((META.arcanes || []).find((a) => a.id === id) || {}).name || prettify(id));
const evoName = (id) => {
  for (const t of META.evolutions || []) { const o = t.options.find((o) => o.id === id); if (o) return o.name; }
  return prettify(id);
};

function renderOptResults(r) {
  const modName = (id) => (modById(id) || { name: null }).name || prettify(id);
  const rows = (r.results || []).map((res) => {
    const ex = res.exilus && res.exilus !== "none" ? `, ${modName(res.exilus)} (exilus)` : "";
    const mods = res.mods.map(modName).join(", ") + ex;
    const arc = res.arcane === "none" ? "no arcane" : `${arcName(res.arcane)} r${res.arcane_rank}`;
    const evos = (res.evolutions || []).map(evoName).join(" · ") || "—";
    return `<div class="opt-row">
      <div class="opt-head">
        <span class="opt-rank">#${res.rank}</span>
        <span class="opt-kills">${res.kills.toFixed(2)}<small> kills</small></span>
        <span class="opt-dps">${(res.effective_dps || 0).toExponential(2)} eff DPS</span>
        <span class="forma-badge legal">${res.forma.used} Forma</span>
        <button class="ghost-btn small opt-load" data-r='${JSON.stringify(res).replace(/'/g, "&#39;")}'>load</button>
      </div>
      <div class="opt-detail"><b>${arc}</b> · ${evos}</div>
      <div class="opt-mods">${mods}</div>
    </div>`;
  }).join("");
  $("opt-results").innerHTML = `<div class="opt-meta">${r.cancelled ? `<span class="warn">cancelled — ranking from the last completed round</span> · ` : ""}${r.candidates} candidates · ${r.jobs} jobs · vs ${r.target.name} Lv ${r.target.level}${r.target.steel_path ? " (SP)" : ""}</div>${rows}`;
  $("opt-results").querySelectorAll(".opt-load").forEach((el) =>
    el.addEventListener("click", () => loadResult(JSON.parse(el.dataset.r))));
}

// Load an optimizer result into the current build for hand-tuning.
function loadResult(res) {
  slots.forEach((s, i) => { s.mod = null; s.pol = innate[i]; s.rank = null; });
  res.mods.slice(0, 8).forEach((mid, i) => {
    if (modById(mid)) { slots[i].mod = mid; slots[i].rank = modById(mid).max_rank; }
  });
  // The scope's exilus choice rides along on every result row.
  if (res.exilus && res.exilus !== "none" && modById(res.exilus)) {
    slots[EXILUS].mod = res.exilus; slots[EXILUS].rank = modById(res.exilus).max_rank;
  }
  arcane = res.arcane === "none" ? "none" : res.arcane;
  arcaneRank = res.arcane === "none" ? null : (res.arcane_rank ?? null);
  evoSel = { 1: null, 2: null, 3: null, 4: null };
  (res.evolutions || []).forEach((id) => {
    const t = (META.evolutions || []).find((tt) => tt.options.some((o) => o.id === id));
    if (t) evoSel[t.tier] = id;
  });
  autoForma();
  renderMods(); renderArcanes(); renderEvo(); renderSim(); renderOpt();
  $("mod-block").scrollIntoView({ behavior: "smooth" });
}

init().catch((e) => { document.querySelector(".config-page").insertAdjacentHTML("afterbegin", `<div class="error">failed to load: ${e}</div>`); });
