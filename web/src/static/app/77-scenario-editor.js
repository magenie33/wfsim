// View preferences project one global scenario without changing its values.
let scenarioView = localStorage.getItem("wfsim-scenario-view") === "all" ? "all" : "context";
let scenarioQuery = "", scenarioSearchTimer = 0;
let scenarioCatalog = null, scenarioCatalogRequest = null, scenarioCatalogError = "";
const scenarioSearchFolds = new Map();
const scenarioAll = () => scenarioView === "all" || !!scenarioQuery.trim();
const scenarioAxisLabel = id => tr(({aiming:"Aiming",headshot_pct:"Headshot %",scope_zoom:"Zoom",gauge_charging_headshots:"Headshots while charging",infinite_ammo:"Infinite ammo",deployment:"Environment"})[id] || prettify(id));
const scenarioEffective = (w,id) => {
  const forced = settledAxis(w,id);
  if (!forced) return "";
  const rule = forced.overridable ? classRuleOf(w.weapon_class,id) : undefined;
  const value = rule === undefined ? forced.value : rule;
  return `<small class="scenario-effective" title="${escHtml(tr(forced.why))}">${escHtml(tr("Current effective value"))}: ${escHtml(typeof value === "boolean" ? tr(value ? "On" : "Off") : String(value))}</small>`;
};
function setScenarioView(view) {
  scenarioView = view; localStorage.setItem("wfsim-scenario-view",view); renderSim();
}
async function fetchScenarioCatalog() {
  if (scenarioCatalog) return scenarioCatalog;
  if (scenarioCatalogRequest) return scenarioCatalogRequest;
  scenarioCatalogRequest = api("/api/opt-buffs",{global:true}).then(answer => {
    if (!answer?.ok || !Array.isArray(answer.buffs)) throw new Error(answer?.error || tr("Could not load global buffs"));
    scenarioCatalog = answer.buffs; scenarioCatalogError = ""; return scenarioCatalog;
  }).catch(error => { scenarioCatalogError = String(error.message || error); throw error; })
    .finally(() => { scenarioCatalogRequest = null; });
  return scenarioCatalogRequest;
}
function scenarioBuffProjection() {
  const weapon = presetWeapon(), active = new Map(buffList.map(buff => [buff.id,buff]));
  const catalog = scenarioCatalog || buffList.map(buff => ({...buff,weapons:[weapon],variants:[]}));
  const all = catalog.map(entry => {
    const variant = entry.variants?.find(row => row.weapons.includes(weapon));
    const metadata = active.get(entry.id) || variant?.buff || entry;
    const applicable = entry.weapons?.includes(weapon) ?? active.has(entry.id);
    const variants = (entry.variants || []).map(row => row.buff);
    return {...metadata,applicable,
      cap_varies:!applicable && new Set(variants.map(row => row.uncapped ? "infinite" : row.max_stacks)).size > 1,
      default_varies:!applicable && new Set(variants.map(row => JSON.stringify([row.default_stacks,row.default_locked]))).size > 1,
      permanent:applicable ? metadata.permanent : variants.length ? variants.every(row => row.permanent) : metadata.permanent,
      source_names:[...new Set([entry.name,...variants.map(row => row.name)])],
      source_weapons:entry.weapons || [],
    };
  });
  const ids = new Set(all.map(buff => buff.id));
  for (const id of Object.keys(sim.buffs || {})) if (!ids.has(id)) {
    all.push({id,name:prettify(id),max_stacks:null,default_stacks:0,default_locked:false,applicable:false,source_names:[],source_weapons:[]});
  }
  all.sort((a,b) => Number(active.has(b.id))-Number(active.has(a.id)) || Number(b.applicable)-Number(a.applicable) || buffCardName(a.name).localeCompare(buffCardName(b.name)));
  return scenarioAll() ? all : all.filter(buff => buff.applicable);
}
function renderSimBuffs() {
  const host = $("sim-buffs"); if (!host) return;
  const button = $("sim-buffs-all");
  if (button) {
    button.textContent = tr(scenarioView === "all" ? "Current context" : "All settings");
    button.onclick = () => setScenarioView(scenarioView === "all" ? "context" : "all");
  }
  if (!scenarioCatalog && !scenarioCatalogError && document.body.classList.contains("on-simulator")) fetchScenarioCatalog().then(() => {
    renderSimBuffs(); renderScenarioEditorView();
  },() => { renderSimBuffs(); renderScenarioEditorView(); });
  const list = scenarioBuffProjection();
  renderBuffCards(host,list,sim.buffs,new Set(buffList.map(buff => buff.id)));
  if (!scenarioCatalog) {
    host.insertAdjacentHTML("afterbegin",`<div class="scenario-catalog-status" data-view-only>${escHtml(scenarioCatalogError || tr("Loading global buffs…"))}${scenarioCatalogError ? ` <button type="button" class="ghost-btn small" data-view-only data-buff-retry>${escHtml(tr("Retry"))}</button>` : ""}</div>`);
    const retry = host.querySelector("[data-buff-retry]");
    if (retry) retry.onclick = () => { scenarioCatalogError = ""; renderSimBuffs(); };
  }
  const visible = new Set(list.map(buff => buff.id));
  const hidden = Object.keys(sim.buffs || {}).filter(id => !visible.has(id)).length;
  if (hidden) {
    host.insertAdjacentHTML("beforeend",`<button type="button" class="ghost-btn small scenario-more" data-view-only>${escHtml(trF("Other saved buff settings ({n})",{n:hidden}))} →</button>`);
    host.querySelector(".scenario-more").onclick = () => setScenarioView("all");
  }
  lockOfficialScenario();
  renderScenarioEditorView();
}
function renderScenarioEditorView() {
  const bar = $("scenario-viewbar"), editor = $("calculation-scenario-editor");
  if (!bar || !editor || !META) return;
  if (!bar.children.length) {
    bar.innerHTML = `<div class="scenario-view-options" role="group" aria-label="${escHtml(tr("Scenario view"))}"><button type="button" class="bchip" data-view-only data-scenario-view="context">${escHtml(tr("Current context"))}</button><button type="button" class="bchip" data-view-only data-scenario-view="all">${escHtml(tr("All settings"))}</button></div>`
      + `<label class="scenario-search"><span>${escHtml(tr("Search all settings"))}</span><input type="search" data-view-only placeholder="${escHtml(tr("Search all settings"))}" aria-label="${escHtml(tr("Search all settings"))}"></label><span class="scenario-search-count"></span>`;
    bar.querySelectorAll("[data-scenario-view]").forEach(button => { button.onclick = () => setScenarioView(button.dataset.scenarioView); });
    bar.querySelector("input").oninput = event => {
      scenarioQuery = event.target.value; clearTimeout(scenarioSearchTimer);
      scenarioSearchTimer = setTimeout(renderSim,120);
    };
  }
  bar.querySelectorAll("[data-scenario-view]").forEach(button => {
    const selected = scenarioView === button.dataset.scenarioView;
    button.classList.toggle("sel",selected); button.setAttribute("aria-pressed",selected);
  });
  const input = bar.querySelector("input"); if (input.value !== scenarioQuery) input.value = scenarioQuery;
  const context = $("calculation-scenario-context");
  if (context) context.textContent = tr("Viewing for") + " " + weaponInfo(presetWeapon()).name;
  $("sim-sub").textContent = tr("One scenario across all weapons");
  const weapon = weaponInfo(presetWeapon());
  for (const control of editor.querySelectorAll("[data-k]")) {
    const label = control.closest("label");
    if (label && !label.querySelector(".scenario-effective")) label.insertAdjacentHTML("beforeend",scenarioEffective(weapon,control.dataset.k));
  }
  const query = scenarioQuery.trim().toLocaleLowerCase();
  const items = [...editor.querySelectorAll(".field-grid > label,.buff-card,.wfb,.sq-row,.hr-col,.arc-side,.scenario-trigger-rules .bev,.scenario-trigger-rules .bevg-h,#sim-roster .rs")];
  let matches = 0;
  for (const item of items) {
    item.classList.add("scenario-search-item");
    const section = item.closest(".scenario-section")?.querySelector(":scope > .fold-h");
    const keys = [...item.querySelectorAll("[data-k],[data-bev],[data-bevg]")].map(node => node.dataset.k || node.dataset.bev || node.dataset.bevg).join(" ");
    const name = [item.textContent,item.dataset.search,section?.textContent,section?.dataset.i18nSrc,keys].filter(Boolean).join(" ").toLocaleLowerCase();
    item.hidden = (!scenarioAll() && item.dataset.contextApplicable === "false") || (!!query && !name.includes(query));
    if (!item.hidden) matches++;
  }
  editor.querySelectorAll(".scenario-rule-card").forEach(card => {
    card.hidden = ![...card.querySelectorAll(".hr-col")].some(column => !column.hidden);
  });
  if (query) {
    for (const fold of editor.querySelectorAll(".fold.sect")) {
      if (!scenarioSearchFolds.has(fold)) scenarioSearchFolds.set(fold,fold.classList.contains("shut"));
      fold.classList.remove("shut");
      const searchable = fold.querySelectorAll(".scenario-search-item");
      fold.hidden = searchable.length > 0 && ![...searchable].some(item => !item.hidden);
    }
  } else {
    for (const [fold,shut] of scenarioSearchFolds) { fold.classList.toggle("shut",shut); fold.hidden = false; }
    scenarioSearchFolds.clear();
    editor.querySelectorAll(".fold.sect").forEach(fold => { fold.hidden = false; });
  }
  bar.querySelector(".scenario-search-count").textContent = query ? trF("{n} matching settings",{n:matches}) : "";
}
