/// THE FIGHT — and there is exactly ONE, for every module that simulates.
///
/// `parse_fight` is the server's half of this rule; this is the page's. Five
/// spellings of "the fight" is five chances to measure something the simulator
/// never runs, because a fight keeps GAINING fields and each new one reaches
/// whichever spellings somebody remembered.
///
/// THE LIVE `sim` IS THE FIGHT, not the preset behind it: a scenario preset is
/// a SAVED COPY that `applyScenario` seeds `sim` from and the auto-save writes
/// back, so reading it over the top can only hand back something staler.
///
/// THE BUFF MAP TRAVELS WHOLE, because buff settings are the FIGHT's and it is
/// the BUILD that decides which have a source (the server's `BuffCfg` is a
/// lookup, so an entry nothing grants is never read). Filtering to `buffList`
/// makes the quick calc a different fight the moment a candidate grants a buff
/// the current build lacks — which is every candidate worth ranking.
///
/// `over` is what a CALLER owns rather than the fight — `replay`, a seed, and
/// the quick calc's RUN COUNT, a reader's precision rather than an edit. It
/// lands LAST, which is the whole of that decoupling.
function theFight(over) {
  return { ...sim, runs: simRuns(), custom_enemies: customEnemiesFor(sim.enemy),
    buffs: sim.buffs || {},
    // THE ROSTER, RESOLVED — links in the scenario, whole builds on the wire.
    // Resolved HERE rather than stored resolved, so a seat's preset edited in
    // its own weapon page is the build this fight runs on the next Run.
    also_acting: alsoActingPayload(),
    ...(over || {}) };
}

function renderEnemies() {
  if (!META || !$("enemy-block")) return;
  const ps = loadPresetList(ENEMIES);
  const open = ps.find((p) => p.name === activeEnemyName());
  $("enemy-sub").textContent = ps.length
    ? `${ps.length} ${tr("saved")}`
    : tr("none yet — a target you build here appears in every scenario's target list");
  renderEnemyTools();
  if (!open) {
    // LIST MODE — nothing is being edited, so nothing pretends to be.
    enemyDoc = null;
    if ($("enemy-form")) $("enemy-form").innerHTML = "";
    renderEnemyAll();
    return;
  }
  if (!enemyDoc) enemyDoc = JSON.parse(JSON.stringify({ ...blankEnemy(), ...(open.state || {}) }));
  renderEnemyForm();
  renderEnemyAll();
}

function saveEnemyDoc() {
  const ps = loadPresetList(ENEMIES);
  const i = ps.findIndex((p) => p.name === activeEnemyName());
  if (i < 0) return;
  ps[i] = { ...ps[i], savedAt: Date.now(), state: snapshotEnemy() };
  storePresetList(ENEMIES, ps);
  // The SCENARIO may be pointing at this target right now, and its card shows
  // the numbers being edited. Redraw it rather than leaving a stale target on
  // a tab the editor cannot see.
  renderSimTargetIfAny();
}

function renderSimTargetIfAny() {
  if (typeof renderEnemy === "function") renderEnemy();
  if (typeof renderOptEnemy === "function") renderOptEnemy();
}

function renderEnemyAll() {
  const box = $("enemy-all");
  if (!box) return;
  const ps = loadPresetList(ENEMIES);
  box.innerHTML = ps.length
    ? ps.map((p) => {
        const s = { ...blankEnemy(), ...(p.state || {}) };
        const parts = s.body_parts.map((b) => `${escHtml(b.name)} ×${b.multiplier}`).join(", ");
        return `<div class="en-row" data-en="${escHtml(p.name)}">
          <span class="nm">${escHtml(p.name)}</span>
          <span class="sm">${escHtml(s.faction || "unknown")} · ${s.stats.health} HP · ${s.stats.armor} ${escHtml(tr("Armor"))} · ${s.stats.shield} ${escHtml(tr("Shield"))}</span>
          <span class="sm">${parts}</span>
          ${enemyId(p.name) === sim.enemy ? `<span class="sm">✓ ${escHtml(tr("in the current fight"))}</span>` : ""}
        </div>`;
      }).join("")
    : `<div class="placeholder">${escHtml(tr("no targets yet"))}</div>`;
  box.querySelectorAll(".en-row").forEach((el) =>
    el.addEventListener("click", () => {
      activeEnemy = el.dataset.en;
      localStorage.setItem(presetActiveKey(ENEMIES), activeEnemy);
      enemyDoc = null;
      renderEnemies();
    })
  );
}

/// OPEN A CUSTOM TARGET for editing by name, or null for the list.
function openEnemy(name) {
  activeEnemy = name;
  if (name) localStorage.setItem(presetActiveKey(ENEMIES), name);
  else localStorage.removeItem(presetActiveKey(ENEMIES));
  enemyDoc = null;
  renderEnemies();
}
/// "+ new target": a blank one, saved and opened. Returns its name.
function newEnemy() {
  const ps = loadPresetList(ENEMIES);
  const name = freeName(ps, (n) => autoPresetName("target", n));
  ps.push({ name, savedAt: Date.now(), state: blankEnemy() });
  storePresetList(ENEMIES, ps);
  openEnemy(name);
  return name;
}
/// ⧉ on the open target: a copy, saved and opened. Returns its name.
function copyEnemy() {
  const ps = loadPresetList(ENEMIES);
  const cur = ps.find((x) => x.name === activeEnemyName());
  if (!cur) return null;
  const name = freeName(ps, (n) => `${cur.name} (${n})`);
  ps.push({ name, savedAt: Date.now(), state: JSON.parse(JSON.stringify(cur.state)) });
  storePresetList(ENEMIES, ps);
  openEnemy(name);
  return name;
}

/// A TARGET'S FIELDS, WRITTEN — the one place their rules live. Every number is
/// 0 or more; a part is named, weighted, and says whether it is a head and
/// whether it multiplies crits; immunities keep the damage types' own order.
/// `damage_modifiers: "faction"` switches the target to a column of its own
/// SEEDED FROM ITS FACTION, so the starting point is what it already was;
/// `null` hands the column back to the faction.
function writeEnemyDoc(d, patch) {
  const n = (v) => Math.max(0, Number(v) || 0);
  if (patch.faction != null) d.faction = patch.faction;
  if (patch.scaling_faction != null) d.scaling_faction = patch.scaling_faction;
  if (patch.can_be_eximus != null) d.can_be_eximus = !!patch.can_be_eximus;
  for (const [k, v] of Object.entries(patch.stats || {})) d.stats[k] = n(v);
  if ("damage_modifiers" in patch) {
    const col = Object.fromEntries((((META.factions || []).find((f) => f.id === d.faction) || {}).modifiers || [])
      .map((m) => [m.type, m.mult]));
    const seed = () => Object.fromEntries(DAMAGE_TYPES.map((k) => [k, col[k] === undefined ? 1 : col[k]]));
    const dm = patch.damage_modifiers;
    d.damage_modifiers = dm === null ? null : dm === "faction" ? seed()
      : { ...(d.damage_modifiers || seed()), ...Object.fromEntries(Object.entries(dm).map(([k, v]) => [k, n(v)])) };
  }
  if (patch.status_immunities) {
    const on = new Set(patch.status_immunities);
    d.status_immunities = DAMAGE_TYPES.filter((x) => on.has(x));
  }
  if (patch.body_parts) {
    d.body_parts = patch.body_parts.map((b) => ({
      name: String(b.name || "").trim() || "part", multiplier: n(b.multiplier ?? 1),
      is_head: !!b.is_head, crit_bonus: !!b.crit_bonus,
    }));
  }
}

function renderEnemyTools() {
  const box = $("enemy-tools");
  if (!box) return;
  const ps = loadPresetList(ENEMIES);
  const cur = ps.find((x) => x.name === activeEnemyName());
  if (!cur) {
    box.innerHTML =
      `<button class="cu-btn cu-new">+ ${escHtml(tr("new target"))}</button>` +
      `<span class="cu-ops">${undoButtons(ENEMIES)}</span>`;
  } else {
    box.innerHTML =
      `<button class="cu-btn cu-back">← ${escHtml(tr("all targets"))}</button>` +
      `<span class="cu-open"><b>${escHtml(cur.name)}</b></span>` +
      `<span class="cu-ops">` +
      `<button class="cu-btn cu-dup" title="${escHtml(tr("duplicate"))}">⧉</button>` +
      `<button class="cu-btn cu-ren" title="${escHtml(tr("rename"))}">✎</button>` +
      `<button class="cu-btn cu-del" title="${escHtml(tr("delete"))}">✕</button>` +
      undoButtons(ENEMIES) +
      `</span>`;
  }
  wireUndoButtons(box, ENEMIES);
  const q = (s) => box.querySelector(s);
  const openIt = openEnemy;
  const click = (sel, fn) => { const b = q(sel); if (b) b.onclick = (e) => { e.stopPropagation(); fn(); }; };

  click(".cu-new", newEnemy);
  click(".cu-back", () => openIt(null));
  click(".cu-dup", copyEnemy);
  click(".cu-del", () => {
    const ps2 = loadPresetList(ENEMIES).filter((x) => x.name !== cur.name);
    storePresetList(ENEMIES, ps2);
    // DELETING A CUSTOM BREAKS REFERENCES, and this is the one it can break:
    // the fight may be pointing at it. It falls back to the roster's first unit
    // rather than to an id nothing answers to.
    if (sim.enemy === enemyId(cur.name)) {
      sim.enemy = ((META.enemies || [])[0] || {}).id || "thrax_centurion";
      markPresetDirty();
      renderSimTargetIfAny();
    }
    openIt(null);
  });
  click(".cu-ren", () => {
    // NO NATIVE DIALOGS (AGENTS.md): an inline input, as everywhere else.
    const span = q(".cu-open");
    span.innerHTML = `<input class="cu-rename" type="text" value="${escHtml(cur.name)}">`;
    const inp = span.querySelector("input");
    inp.focus(); inp.select();
    let done = false;
    const commit = () => {
      if (done) return;
      done = true;
      const name = inp.value.trim();
      const ps2 = loadPresetList(ENEMIES);
      if (!name || name === cur.name || ps2.some((x) => x.name === name)) return renderEnemyTools();
      const i = ps2.findIndex((x) => x.name === cur.name);
      ps2[i] = { ...ps2[i], name };
      storePresetList(ENEMIES, ps2);
      // The id IS the name, so a rename moves whatever names it — otherwise the
      // fight would point at a target that no longer exists.
      if (sim.enemy === enemyId(cur.name)) { sim.enemy = enemyId(name); markPresetDirty(); }
      openIt(name);
      renderSimTargetIfAny();
    };
    inp.onkeydown = (e) => { if (e.key === "Enter") commit(); if (e.key === "Escape") { done = true; renderEnemyTools(); } };
    inp.onblur = commit;
  });
}

function renderEnemyForm() {
  const box = $("enemy-form");
  if (!box) return;
  const d = enemyDoc;
  const cols = Object.fromEntries((META.factions || []).map((f) => [f.id, f.modifiers]));
  const factionCol = Object.fromEntries((cols[d.faction] || []).map((m) => [m.type, m.mult]));
  const own = d.damage_modifiers;
  const num = (k, label, val, step) =>
    `<label>${escHtml(label)}<input type="number" data-en-k="${k}" value="${val}" step="${step || 1}" min="0"></label>`;
  const opts = (list, cur) => list.map((x) =>
    `<option value="${escHtml(x)}"${x === cur ? " selected" : ""}>${escHtml(x)}</option>`).join("");
  box.innerHTML =
    `<div class="en-grid">
      <label>${escHtml(tr("Faction"))}
        <select data-en-k="faction">${opts(["unknown"].concat((META.factions || []).map((f) => f.id)), d.faction || "unknown")}</select></label>
      <label>${escHtml(tr("Level scaling"))}
        <select data-en-k="scaling_faction">${opts(SCALING_FACTIONS, d.scaling_faction)}</select></label>
      ${num("stats.base_level", tr("Base level"), d.stats.base_level)}
      ${num("stats.health", tr("Health"), d.stats.health)}
      ${num("stats.shield", tr("Shield"), d.stats.shield)}
      ${num("stats.armor", tr("Armor"), d.stats.armor)}
      ${num("stats.overguard", tr("Overguard"), d.stats.overguard)}
      <label>${escHtml(tr("Eximus possible"))}
        <input type="checkbox" data-en-k="can_be_eximus"${d.can_be_eximus ? " checked" : ""}></label>
    </div>` +
    // THE VULNERABILITY COLUMN, and the two states it has. A FACTION ALREADY
    // ANSWERS THIS — that is what a faction is to incoming damage — so writing
    // your own is a deliberate second state rather than a set of blanks to
    // fill. IMMUNITY IS 0 HERE, not a checkbox of its own: the game has no
    // third state between a multiplier and nothing getting through.
    `<div class="en-sect"><b>${escHtml(tr("Damage taken"))}</b>
      <label><input type="checkbox" id="en-own-col"${own ? " checked" : ""}> ${
        escHtml(tr("write my own column (0 = takes none of that type)"))}</label>
      ${own ? "" : `<span>${escHtml(tr("from the faction"))}: ${
        (cols[d.faction] || []).map((m) => `${escHtml(DT(m.type))} ×${m.mult}`).join(", ")
        || escHtml(tr("takes every type as written"))}</span>`}
    </div>` +
    (own
      ? `<div class="en-mods">${DAMAGE_TYPES.map((k) =>
          `<label>${escHtml(DT(k))}<input type="number" data-en-dm="${k}" step="0.1" min="0"
             value="${own[k] === undefined ? (factionCol[k] === undefined ? 1 : factionCol[k]) : own[k]}"></label>`).join("")}</div>`
      : "") +
    // STATUS IMMUNITY, and it is a SECTION OF ITS OWN rather than a 0 in the
    // column above, because it is a different mechanic and the difference is
    // not a detail. Taking no DAMAGE of a type does not
    // stop that type from being drawn for procs; a status immunity removes it
    // from the draw and the remaining types RENORMALIZE — the wiki's own worked
    // example moves the other four from 18/5/9/23% to 33/8/17/42% when
    // Corrosive leaves. So an enemy can be immune to one and not the other, and
    // the page has to let a player say which.
    `<div class="en-sect"><b>${escHtml(tr("Status immunity"))}</b>
      <span>${escHtml(tr("these procs cannot land — the other types take over their share of the roll"))}</span></div>
     <div class="en-mods">${DAMAGE_TYPES.map((k) =>
       `<label><input type="checkbox" data-en-si="${k}"${
         (d.status_immunities || []).includes(k) ? " checked" : ""}> ${escHtml(DT(k))}</label>`).join("")}</div>` +
    // BODY PARTS — the weak points, and what each one is worth. A head is not a
    // NAME: `is_head` is what a headshot-conditional mod asks about, and
    // `crit_bonus` is the separate question of whether the part also multiplies
    // critical damage.
    `<div class="en-sect"><b>${escHtml(tr("Body parts"))}</b>
      <button class="cu-btn" id="en-add-part">+ ${escHtml(tr("part"))}</button></div>
     <div class="en-parts">${d.body_parts.map((b, i) => `
      <div class="en-part" data-i="${i}">
        <input type="text" data-en-p="name" value="${escHtml(b.name)}">
        <input type="number" data-en-p="multiplier" value="${b.multiplier}" step="0.1" min="0">
        <label><input type="checkbox" data-en-p="is_head"${b.is_head ? " checked" : ""}> ${escHtml(tr("head"))}</label>
        <label><input type="checkbox" data-en-p="crit_bonus"${b.crit_bonus ? " checked" : ""}> ${escHtml(tr("crit bonus"))}</label>
        ${d.body_parts.length > 1 ? `<button class="cu-btn en-del-part">✕</button>` : ""}
      </div>`).join("")}</div>`;

  const commit = () => { saveEnemyDoc(); renderEnemyForm(); renderEnemyAll(); };
  box.querySelectorAll("[data-en-k]").forEach((el) => {
    el.onchange = () => {
      const [a, b] = el.dataset.enK.split(".");
      const v = el.type === "checkbox" ? el.checked : el.value;
      writeEnemyDoc(d, b ? { [a]: { [b]: v } } : { [a]: v });
      commit();
    };
  });
  const oc = $("en-own-col");
  if (oc) oc.onchange = () => { writeEnemyDoc(d, { damage_modifiers: oc.checked ? "faction" : null }); commit(); };
  box.querySelectorAll("[data-en-si]").forEach((el) => {
    el.onchange = () => {
      const cur = new Set(d.status_immunities || []);
      if (el.checked) cur.add(el.dataset.enSi); else cur.delete(el.dataset.enSi);
      writeEnemyDoc(d, { status_immunities: [...cur] });
      commit();
    };
  });
  box.querySelectorAll("[data-en-dm]").forEach((el) => {
    el.onchange = () => { writeEnemyDoc(d, { damage_modifiers: { [el.dataset.enDm]: el.value } }); commit(); };
  });
  const parts = (f) => { const next = d.body_parts.map((b) => ({ ...b })); f(next); writeEnemyDoc(d, { body_parts: next }); commit(); };
  box.querySelectorAll(".en-part").forEach((row) => {
    const i = Number(row.dataset.i);
    row.querySelectorAll("[data-en-p]").forEach((el) => {
      el.onchange = () => parts((next) => { next[i][el.dataset.enP] = el.type === "checkbox" ? el.checked : el.value; });
    });
    const del = row.querySelector(".en-del-part");
    if (del) del.onclick = () => parts((next) => { next.splice(i, 1); });
  });
  const add = $("en-add-part");
  if (add) add.onclick = () => parts((next) => { next.push({ name: "part", multiplier: 1, is_head: false, crit_bonus: false }); });
}

function renderRivenTools() {
  const box = $("riven-tools");
  if (!box) return;
  const ps = loadPresetList(RIVENS);
  const open = activeRivenId();
  const cur = ps.find((x) => x.id === open);
  if (!cur) {
    box.innerHTML =
      `<button class="cu-btn cu-new">+ ${escHtml(tr("new riven"))}</button>` +
      `<span class="cu-ops">${undoButtons(RIVENS)}</span>`;
  } else {
    const official = (rivenNames[cur.id] || {}).name || "";
    box.innerHTML =
      `<button class="cu-btn cu-back">← ${escHtml(tr("all rivens"))}</button>` +
      `<span class="cu-open"><b>${escHtml(cur.name)}</b>${
        official ? `<span class="rv-official">${escHtml(official)}</span>` : ""}</span>` +
      `<span class="cu-ops">` +
      `<button class="cu-btn cu-dup" title="${escHtml(tr("duplicate"))}">⧉</button>` +
      `<button class="cu-btn cu-ren" title="${escHtml(tr("rename"))}">✎</button>` +
      `<button class="cu-btn cu-del" title="${escHtml(tr("delete"))}">✕</button>` +
      undoButtons(RIVENS) +
      `</span>`;
  }
  wireUndoButtons(box, RIVENS);
  const q = (s) => box.querySelector(s);
  const openIt = openRiven;
  const click = (sel, fn) => { const b = q(sel); if (b) b.onclick = (e) => { e.stopPropagation(); fn(); }; };

  click(".cu-new", newRiven);
  click(".cu-back", () => openIt(""));
  click(".cu-dup", copyRiven);
  click(".cu-del", () => {
    storePresetList(RIVENS, loadPresetList(RIVENS).filter((x) => x.id !== open));
    // …AND EVERY SAVED BUILD IN THE FAMILY LETS IT GO. `pruneDanglingRivens`
    // below clears the LIVE build; these are the ones nobody has open, which
    // would otherwise come back holding an id nothing resolves.
    repointRivenInBuilds(rivenKinWeapons(), open, null);
    // Back to the LIST, not to another riven: deleting the thing you had open
    // is not a request to open a different one.
    openIt("");
    pruneDanglingRivens();
  });
  // Renaming happens in an INLINE input — no prompt(), which the owner's
  // browser blocks. Enter commits, Esc cancels.
  click(".cu-ren", () => {
    const host = q(".cu-open");
    host.innerHTML = `<input class="cu-name" type="text" maxlength="24" value="${
      escHtml((loadPresetList(RIVENS).find((x) => x.id === open) || {}).name || "")}">`;
    const inp = q(".cu-name");
    inp.focus(); inp.select();
    let done = false;
    const commit = (ok) => {
      if (done) return;
      done = true;
      const want = (inp.value || "").trim();
      const ps2 = loadPresetList(RIVENS);
      const at = ps2.findIndex((x) => x.id === open);
      if (!ok || !want || at < 0 || want === ps2[at].name) return renderRivenTools();
      // A LABEL EDIT AND NOTHING ELSE. A build points at the card's id, so
      // nothing has to be chased and two cards may carry one name — which they
      // may genuinely deserve to, and which is not the app's business.
      ps2[at] = { ...ps2[at], name: want };
      storePresetList(RIVENS, ps2);
      openIt(open);
      renderMods(); refreshPanel();
    };
    inp.onkeydown = (ev) => {
      if (ev.key === "Enter") commit(true);
      if (ev.key === "Escape") commit(false);
    };
    inp.onblur = () => commit(true);
  });
}

