// WFSim build configurator — PURE CONFIG. Modules: Mods / Arcane / Evolution /
// Element; each weapon enables only the ones it has. Data from /api/meta;
// official polarity icons from the wiki, art from WFCD.

const $ = (id) => document.getElementById(id);
const CDN = "https://cdn.warframestat.us/img/";
const POL = (p) => `https://wiki.warframe.com/w/Special:FilePath/${p}_Pol.svg`;
const POLS = ["Madurai", "Naramon", "Vazarin", "Zenurik", "Unairu", "Penjaga", "Umbra"];
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
  $("clear-mods").addEventListener("click", () => { slots.forEach((s, i) => { s.mod = null; s.manual = false; s.pol = innate[i]; }); renderMods(); });
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

  slots = Array.from({ length: 8 }, (_, i) => ({ mod: null, pol: innate[i], manual: false }));
  (presetMods || []).filter((m) => modById(m)).slice(0, 8).forEach((m, i) => { slots[i].mod = m; });

  renderMods(); renderArcanes(); renderEvo();
}

// ---- forma / capacity plan (mirrors engine::mods::plan_forma) ----
function slotDrain(base, modPol, slotPol) {
  if (slotPol && slotPol === modPol) return Math.ceil(base / 2);   // matched: −50% round up
  if (slotPol) return Math.round(base * 1.25);                     // mismatched: +25%
  return base;                                                     // no polarity
}

// Auto-assign polarities to non-manual slots to minimize Forma-to-fit; returns
// {drain, forma, over}. Manual slots keep their chosen polarity.
function computePlan() {
  const filled = [];
  slots.forEach((s, i) => { const m = modById(s.mod); if (m) filled.push({ i, m }); });

  let drain = 0, forma = 0;
  const manual = filled.filter((x) => slots[x.i].manual);
  const auto = filled.filter((x) => !slots[x.i].manual);
  for (const { i, m } of manual) {
    drain += slotDrain(m.drain, m.polarity, slots[i].pol);
    if (slots[i].pol && slots[i].pol !== innate[i]) forma++;
  }
  // plan_forma over the auto slots
  const pool = innate.filter(Boolean).slice();
  const order = auto.slice().sort((a, b) => b.m.drain - a.m.drain);
  const matched = new Set();
  for (const { i, m } of order) { const k = pool.indexOf(m.polarity); if (k >= 0) { pool.splice(k, 1); matched.add(i); } }
  const autoDrain = () => auto.reduce((s, { i, m }) => s + (matched.has(i) ? Math.ceil(m.drain / 2) : m.drain), 0);
  const budget = CAP - drain;
  while (autoDrain() > budget) {
    const next = order.find(({ i }) => !matched.has(i));
    if (!next) break;
    matched.add(next.i); forma++;
  }
  drain += autoDrain();
  for (const { i, m } of auto) slots[i].pol = matched.has(i) ? m.polarity : (innate[i] || null);
  return { drain, forma, over: drain > CAP };
}

// ---- render mods ----
function renderMods() {
  const plan = computePlan();
  const capEl = $("capacity");
  capEl.textContent = `${plan.drain} / ${CAP}`;
  capEl.classList.toggle("over", plan.over);
  $("forma").textContent = `${plan.forma} Forma`;

  const box = $("mod-slots");
  box.innerHTML = "";
  slots.forEach((s, i) => {
    const el = document.createElement("div");
    const m = s.mod ? modById(s.mod) : null;
    if (m) {
      el.className = "slot filled";
      const eff = slotDrain(m.drain, m.polarity, s.pol);
      el.innerHTML =
        `<button class="pol-btn" title="change polarity">${imgTag(s.pol ? POL(s.pol) : null, "pol")}${s.pol ? "" : '<span class="nopol">◇</span>'}</button>` +
        imgTag(m.image ? CDN + m.image : null, "mod") +
        `<div class="info"><div class="mn">${m.name}</div><div class="dr">${eff} drain${eff !== m.drain ? ` (base ${m.drain})` : ""}</div></div>` +
        `<button class="dots" title="options">⋯</button>`;
      el.querySelector(".pol-btn").addEventListener("click", (e) => { e.stopPropagation(); openPolMenu(i); });
      el.querySelector(".dots").addEventListener("click", (e) => { e.stopPropagation(); openSlotMenu(i, e.currentTarget); });
    } else {
      el.className = "slot empty";
      el.innerHTML = `<span class="pol ghost">${s.pol ? imgTag(POL(s.pol), "pol") : "◇"}</span><span class="plus">+ add mod</span>`;
      el.addEventListener("click", () => openPicker(i, el));
    }
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
    slots[slotIdx].mod = o.dataset.id; slots[slotIdx].manual = false; slots[slotIdx].pol = innate[slotIdx];
    closePopovers(); renderMods();
  }));
}

function openSlotMenu(slotIdx, anchor) {
  closePopovers();
  const menu = $("slot-menu");
  menu.innerHTML = `
    <div class="mi" data-a="pol">Change polarity ▸</div>
    <div class="mi" data-a="swap">Swap mod</div>
    <div class="mi danger" data-a="remove">Remove</div>`;
  place(menu, anchor);
  menu.querySelector('[data-a="swap"]').addEventListener("click", () => { const el = $("mod-slots").children[slotIdx]; openPicker(slotIdx, el); });
  menu.querySelector('[data-a="remove"]').addEventListener("click", () => { slots[slotIdx].mod = null; slots[slotIdx].manual = false; slots[slotIdx].pol = innate[slotIdx]; closePopovers(); renderMods(); });
  menu.querySelector('[data-a="pol"]').addEventListener("click", () => openPolMenu(slotIdx));
}

function openPolMenu(slotIdx) {
  closePopovers();
  const menu = $("slot-menu");
  const cur = slots[slotIdx].pol;
  menu.innerHTML = POLS.map((p) => `<div class="mi ${p === cur ? "sel" : ""}" data-p="${p}">${imgTag(POL(p), "pol")} ${p}</div>`).join("") +
    `<div class="mi ${!cur ? "sel" : ""}" data-p="">◇ none</div>`;
  const el = $("mod-slots").children[slotIdx];
  place(menu, el);
  menu.querySelectorAll(".mi").forEach((o) => o.addEventListener("click", () => {
    const p = o.dataset.p || null;
    slots[slotIdx].pol = p; slots[slotIdx].manual = true;
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
