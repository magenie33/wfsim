// Preset info reads saved state; browsing never applies it to a live editor.
const presetInfoButton = (cfg, p) => cfg.info
  ? `<button type="button" class="pop pinfo" data-preset-info="${escHtml(presetId(p))}" title="${escHtml(tr("Info"))}" aria-label="${escHtml(tr("Info") + ': ' + p.name)}">ⓘ</button>` : "";
const presetInfoSection = (head, labels) => `<div class="sb-h">${escHtml(tr(head))}</div><div class="sb-chips">${labels.filter(Boolean)
  .map(label => `<span class="sb-chip"><span>${escHtml(label)}</span></span>`).join("") || `<span class="sb-empty">${escHtml(tr("none selected"))}</span>`}</div>`;
function renderPresetInfo(host, cfg, p) {
  host.innerHTML = cfg.info(p);
  if (cfg.prepareInfo) cfg.prepareInfo().then(() => {
    if (host.isConnected) host.innerHTML = cfg.info(p);
  }).catch(() => {});
}
const presetContext = (cfg, p) => cfg.context ? cfg.context(p) : tr("Global");
function wirePresetInfo(host, cfg, entries) {
  host.querySelectorAll("[data-preset-info]").forEach(button => button.onclick = event => {
    event.preventDefault(); event.stopPropagation();
    const p = entries.find(p => presetId(p) === button.dataset.presetInfo);
    if (!p) return;
    closePopovers();
    let pop = $("preset-info-popover");
    if (!pop) {
      pop = document.createElement("div"); pop.id = "preset-info-popover";
      pop.className = "popover preset-info-popover"; pop.setAttribute("role", "dialog");
      document.body.append(pop);
    }
    pop.setAttribute("aria-label", tr("Info") + ": " + p.name);
    pop.innerHTML = `<div class="preset-info-head"><b>${escHtml(p.name)}</b><span>${escHtml(presetContext(cfg, p))}</span><button class="ghost-btn small" data-info-close aria-label="${escHtml(tr("close"))}">×</button></div>`
      + `<div class="preset-info-body"></div>`;
    renderPresetInfo(pop.querySelector(".preset-info-body"), cfg, p);
    place(pop, button);
    pop.querySelector("[data-info-close]").onclick = () => { pop.hidden = true; button.focus({preventScroll:true}); };
    pop.querySelector("[data-info-close]").focus({preventScroll:true});
  });
}

function presetSearchNames() {
  const names = new Map();
  const add = values => (Array.isArray(values) ? values : []).forEach(x => { if (x?.id) names.set(x.id, x.name || x.id); });
  Object.values(META.mod_pools || {}).forEach(add);
  (META.weapons || []).forEach(w => (w.evolutions || []).forEach(t => add(t.options)));
  [META.weapons, META.enemies, META.warframes, META.companions, META.abilities, META.buffs, META.arcanes].forEach(add);
  if (WFCAT) Object.values(WFCAT).filter(Array.isArray).forEach(add);
  COLLECTIONS.filter(c => c.kind === "custom").forEach(c => add(loadPresetWhole(c.domain)));
  return names;
}
function presetSearchText(cfg, p, names) {
  const strings = [];
  const walk = value => {
    if (typeof value === "string") strings.push(value, names.get(value.split("@")[0]) || "", names.get(value.replace(/^riven:/, "")) || "");
    else if (typeof value === "number" || typeof value === "boolean") strings.push(String(value));
    else if (value && typeof value === "object") Object.values(value).forEach(walk);
  };
  walk(p.state);
  return [p.name, presetContext(cfg, p), ...strings].join(" ").replace(/_/g, " ").toLocaleLowerCase();
}

let presetLibraryWindow = null;
function openPresetLibrary(cfg, anchor) {
  flushPresetSaves(); closePopovers();
  if (presetLibraryWindow) presetLibraryWindow.close();
  const overlay = document.createElement("div"); overlay.className = "preset-library-window";
  overlay.innerHTML = `<section class="preset-library-dialog" role="dialog" aria-modal="true" aria-label="${escHtml(tr("Manage presets"))}"><header class="preset-library-head"><div><b>${escHtml(tr("My presets"))} · ${escHtml(cfg.label)}</b><p>${escHtml(tr("Saved automatically. Search names and contents; info shows what each preset contains."))}</p></div><button class="ghost-btn" data-library-close>${escHtml(tr("close"))} · Esc</button></header><div class="preset-library-tools"></div><div class="preset-library-results"></div></section>`;
  const scope = cfg.libraryScope ? cfg.libraryScope() : entryScope(cfg.domain);
  const state = { query: "", sort: "recent", scope: "current", source: "mine", visible: 30 };
  const close = () => {
    overlay.remove(); document.body.classList.remove("preset-library-open"); presetLibraryWindow = null;
    closePopovers(); if (anchor.isConnected) anchor.querySelector("[data-preset-manage]")?.focus({preventScroll:true});
  };
  presetLibraryWindow = { close, render: () => render() };
  document.body.append(overlay); document.body.classList.add("preset-library-open");
  overlay.querySelector("[data-library-close]").onclick = close;
  overlay.onclick = event => { if (event.target === overlay) close(); };
  overlay.addEventListener("keydown", event => {
    if (imeComposing(event)) return;
    if (event.key === "Escape" && !document.querySelector('.popover:not([hidden])')) { event.preventDefault(); event.stopPropagation(); close(); }
    if (event.key !== "Tab" || document.querySelector('.popover:not([hidden])')) return;
    const nodes = [...overlay.querySelectorAll('button,input,a[href],summary,[tabindex="0"]')].filter(x => !x.disabled && x.offsetParent !== null);
    if (event.shiftKey && document.activeElement === nodes[0]) { event.preventDefault(); nodes.at(-1)?.focus(); }
    else if (!event.shiftKey && document.activeElement === nodes.at(-1)) { event.preventDefault(); nodes[0]?.focus(); }
  }, true);
  const tools = overlay.querySelector(".preset-library-tools"), results = overlay.querySelector(".preset-library-results");
  let currentRows = [], armed = "";
  const expanded = new Set();
  const current = p => scope === null || (p.scope || scope) === scope;
  const store = (p, entries) => storePresetList(cfg.domain, entries, p.scope || scope);
  const refresh = () => { cfg.rerender(); render(); };
  const open = p => {
    if (current(p)) { close(); pickPreset(cfg, presetId(p)); return; }
    localStorage.setItem(presetActiveKey(cfg.domain, p.scope), presetId(p));
    const url = cfg.libraryPath(p);
    close(); location.assign(url);
  };
  function renderRows() {
    const own = state.scope === "all" ? loadPresetWhole(cfg.domain) : cfg.load();
    const published = cfg.pins ? openedPublished(cfg) : [];
    const source = state.source === "published" ? published : state.source === "all" ? own.concat(published) : own;
    const terms = state.query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
    const names = presetSearchNames();
    currentRows = source.filter(p => {
      if (!terms.length) return true;
      const text = presetSearchText(cfg, p, names);
      return terms.every(term => text.includes(term));
    });
    currentRows.sort((a, b) => (state.sort === "name" ? a.name.localeCompare(b.name, LANG, {numeric:true})
      : (Number(b.savedAt) || 0) - (Number(a.savedAt) || 0)) || presetId(a).localeCompare(presetId(b)));
    if (state.sort === "oldest") currentRows.reverse();
    results.innerHTML = `<div class="preset-library-count">${currentRows.length} / ${source.length}</div>`
      + currentRows.slice(0, state.visible).map(p => `<article class="preset-library-item${presetId(p) === cfg.active() ? ' active' : ''}" data-entry="${escHtml(presetId(p))}"><div class="preset-library-row"><div><b>${escHtml(p.name)}</b><span class="preset-library-context">${escHtml(presetContext(cfg, p))}${p.builtin ? ' · ' + escHtml(tr("read-only")) : ''}</span><p class="preset-library-summary" title="${escHtml(presetSummary(cfg,p,names))}">${escHtml(presetSummary(cfg,p,names))}</p><time>${p.savedAt ? escHtml(new Date(p.savedAt).toLocaleString(LANG)) : ''}</time></div><div class="preset-library-actions">${p.builtin ? '' : cloudMark(cfg.domain, p)}<button class="ghost-btn small" data-library-open>${escHtml(tr("Open"))}</button>${presetActionsHtml((current(p) ? `<button class="ghost-btn small" data-library-copy>⧉ ${escHtml(tr("duplicate"))}</button>` : "") + (p.builtin ? "" : `<button class="ghost-btn small" data-library-rename>✎ ${escHtml(tr("rename"))}</button><button class="ghost-btn small" data-library-delete>× ${escHtml(tr("delete"))}</button>`))}</div></div><details class="preset-library-info"${expanded.has(presetId(p)) ? " open" : ""}><summary>${escHtml(tr("Info"))}</summary><div class="preset-info-body"></div></details><div class="preset-library-feedback"></div></article>`).join("")
      + (!currentRows.length ? `<p class="sb-empty">${escHtml(tr("No matching presets"))}</p>` : "")
      + (currentRows.length > state.visible ? `<button class="ghost-btn" data-library-more>${escHtml(tr("Show more"))}</button>` : "");
    results.querySelectorAll("[data-entry]").forEach(row => {
      const p = currentRows.find(p => presetId(p) === row.dataset.entry);
      row.querySelector("[data-library-open]").onclick = () => open(p);
      const details = row.querySelector(".preset-library-info");
      const fillInfo = () => { if (details.open && !details.dataset.loaded) { renderPresetInfo(details.querySelector(".preset-info-body"), cfg, p); details.dataset.loaded = "1"; } };
      details.ontoggle = () => { if (details.open) expanded.add(presetId(p)); else expanded.delete(presetId(p)); fillInfo(); };
      fillInfo();
      const copy = row.querySelector("[data-library-copy]");
      if (copy) copy.onclick = () => { pickPreset(cfg, presetId(p)); copyActivePreset(cfg); refresh(); };
      const rename = row.querySelector("[data-library-rename]");
      if (rename) rename.onclick = () => {
        const feedback = row.querySelector(".preset-library-feedback");
        feedback.innerHTML = `<form class="preset-library-rename"><input aria-label="${escHtml(tr("name"))}" maxlength="80" value="${escHtml(p.name)}"><button class="ghost-btn small">${escHtml(tr("Apply"))}</button><button type="button" class="ghost-btn small" data-cancel>${escHtml(tr("cancel"))}</button></form>`;
        feedback.querySelector("[data-cancel]").onclick = () => { feedback.innerHTML = ""; };
        const input = feedback.querySelector("input"); input.focus(); input.select();
        feedback.querySelector("form").onsubmit = event => {
          event.preventDefault(); const name = input.value.trim(); if (!name) return;
          const list = loadPresetList(cfg.domain, p.scope || scope).map(x => x.id === p.id ? {...x, name} : x);
          store(p, list); refresh();
        };
      };
      const del = row.querySelector("[data-library-delete]");
      if (del) del.onclick = () => {
        if (armed !== p.id) {
          armed = p.id;
          const users = cfg.usedBy ? cfg.usedBy(p) : [];
          row.querySelector(".preset-library-feedback").textContent = users.length ? tr("linked by") + ': ' + users.join(' · ') + ' · ' + tr("{n} linked — they return to the default; click again").replace('{n}', users.length) : tr("Click delete again to remove this preset.");
          return;
        }
        armed = "";
        if (current(p)) { deleteOwnedPreset(cfg, p.id); expanded.add(cfg.active()); render(); }
        else { store(p, loadPresetList(cfg.domain, p.scope || scope).filter(x => x.id !== p.id)); refresh(); }
      };
      const cloud = row.querySelector("[data-cloud]");
      if (cloud) cloud.onclick = () => { setCloudSync(presetListKey(cfg.domain), p.id, !cloud.classList.contains("on")); refresh(); };
    });
    wirePresetActions(results);
    const more = results.querySelector("[data-library-more]");
    if (more) more.onclick = () => { state.visible += 30; renderRows(); };
  }
  results.onscroll = () => {
    if (currentRows.length > state.visible && results.scrollTop + results.clientHeight >= results.scrollHeight - 180) {
      const at = results.scrollTop; state.visible += 30; renderRows(); results.scrollTop = at;
    }
  };
  function render() {
    tools.innerHTML = `<div class="addbar preset-library-search"><input type="search" aria-label="${escHtml(tr("Search presets"))}" placeholder="${escHtml(tr("Search names, equipment and settings…"))}" value="${escHtml(state.query)}"></div>`
      + ddButton("preset-library-sort", { value:state.sort, items:[{value:"recent",label:tr("Recently modified")},{value:"name",label:tr("Name")},{value:"oldest",label:tr("Oldest first")}], onPick:value => {state.sort=value; render();} })
      + (scope !== null && cfg.libraryPath ? ddButton("preset-library-scope", {value:state.scope, items:[{value:"current",label:tr("Current item")},{value:"all",label:tr("All items")}],onPick:value=>{state.scope=value;state.visible=30;render();}}) : "")
      + (cfg.pins ? ddButton("preset-library-source",{value:state.source,items:[{value:"mine",label:tr("Mine")},{value:"published",label:cfg.roGroup},{value:"all",label:tr("All")}],onPick:value=>{state.source=value;state.visible=30;render();}}) : "")
      + `<button class="ghost-btn" data-library-new>+ ${escHtml(tr("new"))}</button>`;
    const search = tools.querySelector("input");
    onTyped(search, () => {state.query=search.value;state.visible=30;renderRows();});
    tools.querySelector("[data-library-new]").onclick = () => {newPreset(cfg);refresh();};
    renderRows();
  }
  render(); tools.querySelector("input").focus({preventScroll:true});
}

function searchPresetInfo(p) {
  const st = p.state || {}, w = weaponInfo(p.scope || presetWeapon()), limits = st.limits || {};
  return withPoolOf(w, () => {
    const name = id => modById(id)?.name || arcaneById(id)?.name || prettify(id);
    const excludes = Object.entries(limits.exclude || {}).flatMap(([axis, ids]) => (ids || []).map(id => `${tr(axis)}: ${name(id)}`));
    return presetInfoSection("Search", [tr("starts") + ': ' + (st.starts || []).length,
      tr("finalists") + ': ' + (st.finalists ?? ''), tr("runs per candidate") + ': ' + (st.candidate_runs ?? '')])
      + presetInfoSection("Limits", [tr("fill at most") + ' ' + (limits.mods ?? 8) + ' ' + tr("mods"),
        tr("Exilus") + ': ' + tr(limits.exilus === false ? "off" : "on"),
        ...(limits.arcane_seats || []).map((on, i) => tr("Arcane") + ' ' + (i + 1) + ': ' + tr(on ? "on" : "off"))])
      + presetInfoSection("Excluded", excludes)
      + (st.starts || []).map((start, i) => `<details class="preset-start-info"${i === 0 ? ' open' : ''}><summary>${escHtml(tr("start"))} ${i + 1}</summary>${start.build
        ? cardOfState({...start.build, mode:start.build.mode || defaultMode(w.id, null)}, w, new Set(start.fixed || [])) : ''}</details>`).join("");
  });
}
function operatorPresetInfo(state) {
  const st = opNormalize(state), school = focusSchool(st.school);
  const card = (id, fn) => { const m = fn(id); return m ? {label:m.name, img:IMG(m.image)} : null; };
  return presetInfoSection("Focus", [school?.name])
    + presetInfoSection("assumed up", (st.assumed || []).map(id => school?.nodes.find(n => n.id === id)?.name || id))
    + buildCardHtml({mods:st.artifact.mods.map(id => card(id, opAMod)).filter(Boolean), arcanes:[card(st.artifact.arcane, opAArcane)].filter(Boolean)});
}
function weaponPresetInfo(p, weapon = p.scope || p.state.weapon || presetWeapon()) {
  const w = weaponInfo(weapon);
  const held = defaultWielder(w.id, p.state.wielder), linked = wielderBuild(held);
  const owner = compHost(held.frame) || wfFrames().find(f => f.id === held.frame);
  return withPoolOf(w, () => cardOfState({...p.state, mode:p.state.mode || defaultMode(w.id, null)}, w))
    + presetInfoSection("Wielder", [owner?.name || held.frame, linked?.name || tr("Default")])
    + (p.board ? presetInfoSection("Benchmark", [benchmarkName(p.benchmark)]) + measuredRecordHtml(p.board) : "");
}
function companionPresetInfo(state) {
  return buildCardHtml({mods:(state.slots || []).filter(s => s?.mod).map(s => {
    const m = compMod(s.mod); return {label:m?.name || s.mod, img:m ? IMG(m.image) : null, rank:s.rank ?? m?.max_rank};
  })});
}

function presetRecentUses(domain) {
  try { return Object.assign(Object.create(null), JSON.parse(localStorage.getItem(`wfsim-preset-recent-${domain}`) || '{}')); }
  catch (_) { return Object.create(null); }
}
function notePresetUse(domain, id) {
  const recent = presetRecentUses(domain); recent[id] = Date.now();
  try { localStorage.setItem(`wfsim-preset-recent-${domain}`, JSON.stringify(recent)); } catch (_) { /* optional view preference */ }
}
function presetEquipmentSummary(state, names) {
  const ids = [...(state.slots || []).map(s => s?.mod), ...(state.arcane || []), ...(state.arcanes || []).map(a => a?.id),
    ...(state.artifact?.mods || []), state.artifact?.arcane, state.helminth?.ability].filter(id => id && id !== 'none');
  const labels = ids.map(id => names.get(String(id).replace(/^riven:/, '').split('@')[0]) || prettify(id));
  if ((state.shards || []).some(Boolean)) labels.push(`${tr('Archon Shards')} ${(state.shards || []).filter(Boolean).length}`);
  return labels.slice(0, 3).join(' · ') + (labels.length > 3 ? ` +${labels.length - 3}` : '') || tr('none selected');
}
const presetSummary = (cfg, p, names) => cfg.summary ? cfg.summary(p, names) : presetEquipmentSummary(p.state || {}, names);
function weaponPresetSummary(p, names) {
  const w = weaponInfo(p.scope || p.state.weapon || presetWeapon());
  return [modeLabel(w, p.state.mode || defaultMode(w.id, null)), presetEquipmentSummary(p.state, names)].filter(Boolean).join(' · ');
}
function scenarioPresetSummary(p) {
  const s = headlessScenarioOnto(META, p.state, CONTACT_M, METRIC_FALLBACK.id);
  const enemy = allEnemies().find(e => e.id === s.enemy);
  return `${enemy?.name || s.enemy} Lv ${s.level} · ${empowermentLabel(s.steel_path, s.empowered, true)} · ${s.headshot_pct}% ${tr('headshots')} · ${tr(s.aiming ? 'Aiming' : 'hip-fire')}`;
}
const presetActionsHtml = body => body ? `<details class="preset-actions"><summary class="ghost-btn small" aria-label="${escHtml(tr('More actions'))}" title="${escHtml(tr('More actions'))}">⋯</summary><div class="context-menu preset-action-menu">${body}</div></details>` : "";
function wirePresetActions(host) {
  host.querySelectorAll('.preset-actions').forEach(details => {
    details.ontoggle = () => {
      if (!details.open) return;
      document.querySelectorAll('.preset-actions[open]').forEach(other => { if (other !== details) other.open = false; });
      const box = details.querySelector('summary').getBoundingClientRect(), menu = details.querySelector('.preset-action-menu');
      menu.style.left = Math.max(8, Math.min(box.right - menu.offsetWidth, innerWidth - menu.offsetWidth - 8)) + 'px';
      menu.style.top = Math.max(8, Math.min(box.bottom + 4, innerHeight - menu.offsetHeight - 8)) + 'px';
    };
    details.querySelectorAll('button').forEach(button => {
      button.classList.add('mi');
      if (button.matches('.del,[data-library-delete]')) button.classList.add('danger');
      button.addEventListener('click', () => {
        if (!button.matches('.del,[data-library-delete]')) queueMicrotask(() => { details.open = false; });
      });
    });
  });
}
document.addEventListener('click', event => document.querySelectorAll('.preset-actions[open]').forEach(details => {
  if (!details.contains(event.target)) details.open = false;
}));
document.addEventListener('keydown', event => {
  if (event.key !== 'Escape' || imeComposing(event)) return;
  document.querySelectorAll('.preset-actions[open]').forEach(details => { details.open = false; details.querySelector('summary').focus({preventScroll:true}); });
});
document.addEventListener('scroll', event => {
  if (event.target instanceof Element && event.target.closest('.preset-action-menu')) return;
  document.querySelectorAll('.preset-actions[open]').forEach(details => { details.open = false; });
}, true);
