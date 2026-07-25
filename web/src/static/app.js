// WFSim build configurator — PURE CONFIG. Modules: Mods / Arcane / Evolution /
// Element; each weapon enables only the ones it has. Data from /api/meta;
// official polarity icons from the wiki, art from WFCD.

const $ = (id) => document.getElementById(id);
const CDN = "https://cdn.warframestat.us/img/";
// Omni (universal) polarity uses the wiki's "Any" symbol.
const POL = (p) => `https://wiki.warframe.com/w/Special:FilePath/${p === "Omni" ? "Any" : p}_Pol.svg`;
// Polarities available on GUN slots. Zenurik/Unairu/Penjaga are Warframe-augment
// / melee-stance / companion-ability polarities — not gun slots. "Omni" is the
// Omni Forma universal polarity (matches any mod EXCEPT Umbra mods).
const GUN_POLS = ["Madurai", "Naramon", "Vazarin", "Umbra", "Omni"];
const CAP = 60;
const imgTag = (src, cls) => src ? `<img class="${cls||''}" src="${src}" onerror="this.style.visibility='hidden'"/>` : `<span class="${cls||''}"></span>`;

let META = null;
let slots = [];      // 8 × { mod:id|null, pol:string|null, manual:bool } — POSITIONAL
let innate = [];     // 8 × innate polarity name|null
let arcane = "none";
let evo2 = "fevered";

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
  arcane = d.arcane; evo2 = d.evo2;
  applyWeapon(d.weapon, d.mods);

  $("weapon").addEventListener("change", () => applyWeapon($("weapon").value, null));
  $("auto-forma").addEventListener("click", () => { autoForma(); renderMods(); });
  $("clear-mods").addEventListener("click", () => { slots.forEach((s, i) => { s.mod = null; s.pol = innate[i]; }); renderMods(); });
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".popover") && !e.target.closest(".slot")) closePopovers();
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") closePopovers(); });
}

function fillSelect(id, items) {
  const el = $(id); el.innerHTML = "";
  for (const it of items) { const o = document.createElement("option"); o.value = it.id; o.textContent = it.name; el.appendChild(o); }
}
let currentPool = [];
const weaponInfo = (id) => META.weapons.find((w) => w.id === id) || META.weapons[0];
const modById = (id) => currentPool.find((m) => m.id === id);
const show = (id, on) => { const el = $(id); if (on) el.removeAttribute("hidden"); else el.setAttribute("hidden", ""); };
const placedElsewhere = (id, exceptIdx) => slots.some((s, i) => i !== exceptIdx && s.mod === id);

function applyWeapon(id, presetMods) {
  const w = weaponInfo(id);
  currentPool = META.mod_pools[w.mod_class] || [];
  innate = (w.innate_polarities || []).slice(0, 8);
  while (innate.length < 8) innate.push(null);

  $("w-img").src = w.image ? CDN + w.image : "";
  $("w-name").textContent = w.name;
  $("w-tags").innerHTML = [w.mod_class + " mods", w.sentinel ? "sentinel" : null, w.uses_evo2 ? "Incarnon" : null]
    .filter(Boolean).map((t) => `<span class="tag">${t}</span>`).join("");

  show("arcane-block", w.arcane_slots >= 1);
  show("evo-block", w.uses_evo2);
  show("element-block", !!w.element_config);
  $("arcane-sub").textContent = w.sentinel ? "sentinels cannot equip arcanes" : `${w.arcane_slots} slot`;
  if (w.arcane_slots < 1) arcane = "none";

  slots = Array.from({ length: 8 }, (_, i) => ({ mod: null, pol: innate[i] }));
  (presetMods || []).filter((m) => modById(m)).slice(0, 8).forEach((m, i) => { slots[i].mod = m; });
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

// Capacity = Σ effective drain over slots holding a mod.
function capacityUsed() {
  return slots.reduce((sum, s) => { const m = modById(s.mod); return m ? sum + slotDrain(m.drain, m.polarity, s.pol) : sum; }, 0);
}

// Forma = multiset difference of the final slot polarities vs the innate POOL.
// Innate polarities can be freely repositioned (Forma repositions), so only
// polarities BEYOND what the pool provides cost a Forma. (Polarity is fully
// decoupled from mods: empty slots and innate slots can be re-polarized.)
function formaCount() {
  const need = {}, pool = {};
  slots.forEach((s) => { if (s.pol) need[s.pol] = (need[s.pol] || 0) + 1; });
  innate.forEach((p) => { if (p) pool[p] = (pool[p] || 0) + 1; });
  let forma = 0;
  for (const p in need) forma += Math.max(0, need[p] - (pool[p] || 0));
  return forma;
}

// Auto-assign polarities for MINIMUM Forma-to-fit (mirrors engine plan_forma):
// spend the innate pool on the biggest matching mods, then Forma the biggest
// unmatched until it fits; unmatched slots left blank. Overwrites polarities.
function autoForma() {
  const filled = [];
  slots.forEach((s, i) => { const m = modById(s.mod); if (m) filled.push({ i, m }); });
  slots.forEach((s) => { s.pol = null; });
  const pool = innate.filter(Boolean).slice();
  const order = filled.slice().sort((a, b) => b.m.drain - a.m.drain);
  const matched = new Set();
  for (const { i, m } of order) { const k = pool.indexOf(m.polarity); if (k >= 0) { pool.splice(k, 1); matched.add(i); } }
  const drainOf = () => filled.reduce((s, { i, m }) => s + (matched.has(i) ? Math.ceil(m.drain / 2) : m.drain), 0);
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
  $("forma").textContent = `${formaCount()} Forma`;

  const box = $("mod-slots");
  box.innerHTML = "";
  slots.forEach((s, i) => {
    const el = document.createElement("div");
    const m = s.mod ? modById(s.mod) : null;
    if (m) {
      el.className = "slot filled";
      const eff = slotDrain(m.drain, m.polarity, s.pol);
      el.innerHTML = polBtn(s.pol, i) + imgTag(m.image ? CDN + m.image : null, "mod") +
        `<div class="info"><div class="mn">${m.name}</div><div class="dr">${eff} drain${eff !== m.drain ? ` (base ${m.drain})` : ""}</div></div>` +
        `<button class="dots" title="options">⋯</button>`;
      el.querySelector(".dots").addEventListener("click", (e) => { e.stopPropagation(); openSlotMenu(i, e.currentTarget); });
    } else {
      el.className = "slot empty";
      el.innerHTML = polBtn(s.pol, i) + `<span class="plus">+ add mod</span>`;
      el.querySelector(".plus").addEventListener("click", (e) => { e.stopPropagation(); openPicker(i, el); });
    }
    // polarity is decoupled: clickable on every slot (mod or empty, incl. innate)
    el.querySelector(".pol-btn").addEventListener("click", (e) => { e.stopPropagation(); openPolMenu(i); });
    box.appendChild(el);
  });
  $("exilus").innerHTML = `<div class="slot empty exl"><span class="plus">utility only</span></div>`;
}

// ---- popovers ----
function closePopovers() { $("mod-popover").hidden = true; $("slot-menu").hidden = true; }
function place(pop, anchor) {
  const r = anchor.getBoundingClientRect();
  pop.hidden = false;
  pop.style.top = (window.scrollY + r.bottom + 4) + "px";
  pop.style.left = (window.scrollX + r.left) + "px";
}

function openPicker(slotIdx, anchor) {
  closePopovers();
  const pop = $("mod-popover");
  place(pop, anchor);
  const search = $("mod-search");
  search.value = "";
  const draw = () => renderMenu(slotIdx, search.value);
  search.oninput = draw;
  draw();
  search.focus();
}

function familyConflict(mod, exceptIdx) {
  if (!mod.family) return false;
  return slots.some((s, i) => { if (i === exceptIdx || !s.mod) return false; const o = modById(s.mod); return o && o.family === mod.family; });
}

function renderMenu(slotIdx, query) {
  const menu = $("mod-menu");
  const q = query.trim().toLowerCase();
  const hits = currentPool
    .filter((m) => !placedElsewhere(m.id, slotIdx))
    .filter((m) => !q || m.name.toLowerCase().includes(q) || m.effects.join(" ").toLowerCase().includes(q))
    .sort((a, b) => a.name.localeCompare(b.name))
    .slice(0, 12);
  menu.innerHTML = hits.length ? hits.map((m) => {
    const conflict = familyConflict(m, slotIdx);
    return `<div class="opt ${conflict ? "dis" : ""}" data-id="${m.id}" title="${conflict ? "incompatible (" + m.family + ")" : m.effects.join(" · ")}">
      ${imgTag(POL(m.polarity), "pol")}${imgTag(m.image ? CDN + m.image : null, "mod")}
      <div class="info"><div class="mn">${m.name}</div><div class="me">${m.effects.join(", ")}</div></div><span class="dr">${m.drain}</span></div>`;
  }).join("") : `<div class="opt dis">no matches</div>`;
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", () => {
    slots[slotIdx].mod = o.dataset.id; // polarity is decoupled — keep the slot's polarity
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
  menu.querySelector('[data-a="swap"]').addEventListener("click", () => { const el = $("mod-slots").children[slotIdx]; openPicker(slotIdx, el); });
  menu.querySelector('[data-a="remove"]').addEventListener("click", () => { slots[slotIdx].mod = null; closePopovers(); renderMods(); }); // in-place; keep polarity
}

function openPolMenu(slotIdx) {
  closePopovers();
  const menu = $("slot-menu");
  const cur = slots[slotIdx].pol;
  menu.innerHTML = GUN_POLS.map((p) => `<div class="mi ${p === cur ? "sel" : ""}" data-p="${p}">${imgTag(POL(p), "pol")} ${p === "Omni" ? "Omni (any)" : p}</div>`).join("") +
    `<div class="mi ${!cur ? "sel" : ""}" data-p="">◇ none</div>`;
  const el = $("mod-slots").children[slotIdx];
  place(menu, el);
  menu.querySelectorAll(".mi").forEach((o) => o.addEventListener("click", () => {
    slots[slotIdx].pol = o.dataset.p || null;
    closePopovers(); renderMods();
  }));
}

// ---- Arcane ----
function renderArcanes() {
  const box = $("arcane-slots");
  box.innerHTML = META.arcanes.map((a) => `
    <div class="arcane-opt ${a.id === arcane ? "sel" : ""}" data-id="${a.id}">
      ${a.image ? imgTag(CDN + a.image, "aimg") : `<span class="aimg none">∅</span>`}<span>${a.name}</span></div>`).join("");
  box.querySelectorAll(".arcane-opt").forEach((o) => o.addEventListener("click", () => { arcane = o.dataset.id; renderArcanes(); }));
}

// ---- Evolution ----
function renderEvo() {
  const rows = [
    { rank: "EVO I", pick: "Incarnon Form", locked: true },
    { rank: "EVO II", options: META.evo2, sel: evo2 },
    { rank: "EVO III–V", pick: "fixed (folded into the weapon)", locked: true },
  ];
  $("evo-rows").innerHTML = rows.map((r) => {
    if (r.locked) return `<div class="evo"><span class="rank">${r.rank}</span><div class="locked">${r.pick}</div></div>`;
    const chips = r.options.map((o) => `<span class="echip2 ${o.id === r.sel ? "sel" : ""}" data-id="${o.id}">${o.name}</span>`).join("");
    return `<div class="evo"><span class="rank">${r.rank}</span><div class="picks">${chips}</div></div>`;
  }).join("");
  $("evo-rows").querySelectorAll(".echip2").forEach((c) => c.addEventListener("click", () => { evo2 = c.dataset.id; renderEvo(); }));
}

init().catch((e) => { document.querySelector(".config-page").insertAdjacentHTML("afterbegin", `<div class="error">failed to load: ${e}</div>`); });
