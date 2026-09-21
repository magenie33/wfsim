// ---- The optimizer preset — ONE document per weapon.
//
// It was three (mods / arcanes / evolutions), split so the parts could be
// reused across weapons. They no longer need to be: preset storage is
// weapon-scoped (`wfsim-presets-<weapon>-optimizer`), and carrying a search to
// another weapon is the explicit IMPORT, which filters per axis. Three lists
// bought nothing and cost three bootstraps, three actives and three bars over
// one search.
//
// What it holds is the SEARCH: the scope (mods + exilus + max size, arcanes,
// per-tier evolutions) and the funnel's final-round contract. NOT the
// scenario — the optimizer does not own one; it runs the simulator's, which
// has its own preset domain.
//
// `threads` stays out too: it is a property of the MACHINE, not of a search,
// and a preset carrying it would re-tune the CPU on every load.
const OPT_DOMAIN = "optimizer";
// Same DOCUMENT MODEL as every other bar: there is always >=1 preset, one is always active and being
// edited, edits auto-save in place, and the active survives a reload.
let activeOptPreset = null;

const loadOptPresets = () => loadPresetList(OPT_DOMAIN);
const storeOptPresets = (ps) => storePresetList(OPT_DOMAIN, ps);

// Called from renderOpt's seed block (page load AND weapon switch). The
// first-ever run creates "search 1" from the build-seeded scope; afterwards
// the active preset IS the scope.
function bootstrapOptPresets() {
  // NOTHING IS AUTO-CREATED here either — see `initPresets`. A search that has
  // never been run is not a search you own, and the scope controls are already
  // a complete live state without one (`OPT_RUN_DEFAULTS` plus an empty scope).
  const ps = loadOptPresets();
  const want = activeOptPreset || localStorage.getItem(presetActiveKey(OPT_DOMAIN));
  activeOptPreset = ps.some((p) => p.name === want) ? want : (ps[0] ? ps[0].name : "");
  localStorage.setItem(presetActiveKey(OPT_DOMAIN), activeOptPreset);
  const cur = ps.find((p) => p.name === activeOptPreset);
  if (cur) applyOptState(cur.state);
}

// One-time merges, oldest first: the single legacy bar was split into three
// groups, and the three are now one again. Both run over whatever is on the
// machine, so a browser that skipped a release still lands on the current
// shape. Names are the join key — a preset named "crit" in each group was one
// search described three times, which is exactly what it becomes.
(function migrateOptPresets() {
  const parse = (k) => { try { return JSON.parse(localStorage.getItem(k)); } catch (_) { return null; } };
  // Step 1: one bar -> three groups, under the current weapon.
  const legacy = parse("wfsim-opt-presets");
  if (Array.isArray(legacy)) {
    legacy.forEach((p) => {
      const st = p.state || {};
      [["mods", { mods: st.mods || {}, exilus: (st.exilus && typeof st.exilus === "object") ? st.exilus : {}, size: st.size || 8 }],
       ["arcanes", { arcanes: st.arcanes || {} }],
       ["evolutions", { evos: st.evos || {} }]].forEach(([g, state]) => {
        const key = "wfsim-presets-" + presetWeapon() + "-optimizer-" + g;
        const ps = parse(key) || [];
        if (!ps.some((x) => x.name === p.name)) {
          ps.push({ name: p.name, savedAt: p.savedAt || Date.now(), state });
          localStorage.setItem(key, JSON.stringify(ps));
        }
      });
    });
    localStorage.removeItem("wfsim-opt-presets");
  }
  // Step 2: three groups -> one, for EVERY weapon that has them.
  const groups = ["mods", "arcanes", "evolutions"];
  const weapons = new Set();
  for (let i = 0; i < localStorage.length; i++) {
    const m = /^wfsim-presets-(.+)-optimizer-(mods|arcanes|evolutions)$/.exec(localStorage.key(i));
    if (m) weapons.add(m[1]);
  }
  weapons.forEach((w) => {
    const merged = parse(`wfsim-presets-${w}-optimizer`) || [];
    const byName = new Map(merged.map((p) => [p.name, p]));
    groups.forEach((g) => {
      (parse(`wfsim-presets-${w}-optimizer-${g}`) || []).forEach((p) => {
        const into = byName.get(p.name)
          || { name: p.name, savedAt: p.savedAt || Date.now(), state: {} };
        into.state = { ...into.state, ...(p.state || {}) };
        byName.set(p.name, into);
      });
    });
    if (byName.size) {
      localStorage.setItem(`wfsim-presets-${w}-optimizer`, JSON.stringify([...byName.values()]));
      // The three old actives disagree by construction (three bars, three
      // choices); the mod scope is the one that decided what the search was.
      const act = localStorage.getItem(`wfsim-preset-active-${w}-optimizer-mods`);
      if (act && byName.has(act)) localStorage.setItem(`wfsim-preset-active-${w}-optimizer`, act);
    }
    groups.forEach((g) => {
      localStorage.removeItem(`wfsim-presets-${w}-optimizer-${g}`);
      localStorage.removeItem(`wfsim-preset-active-${w}-optimizer-${g}`);
    });
  });
})();

function snapshotOpt() {
  return {
    mods: { ...opt.mods }, exilus: { ...opt.exilus }, size: opt.size, min: opt.min,
    arcanes: { ...opt.arcanes }, modes: { ...opt.modes },
    evos: JSON.parse(JSON.stringify(opt.evos)),
    finalists: optRun.finalists,
  };
}

// An empty search: nothing marked, a fresh size, the contract left alone (it
// is how hard to search, not what to search).
// `min` IS IN HERE, and it was not: `sameState(snapshotOpt(), blankOpt())` is
// the "has this search been touched" guard, and a field one side carries and
// the other omits makes it answer "touched" for every input — the same shape as
// the key-order bug `canon` was written for.
const blankOpt = () => ({ mods: {}, exilus: {}, size: 8, min: 0, arcanes: {}, evos: {},
  // The build's own mode, the way an empty scope seeds every other axis from
  // what you are holding.
  modes: { [mode]: "fixed" },
  finalists: optRun.finalists });

// State-only apply (validation + cross-weapon id dropping); no re-render.
//
// Every axis drops what THIS weapon cannot hold, which is what makes a preset
// carried over from another weapon land as "the part that still applies"
// rather than as a search the run cannot execute.
function applyOptState(st) {
  const norm = (s) => (s === true ? "search" : s); // boolean-era marks
  // Mods: ids missing from this weapon's pool drop out.
  opt.mods = {}; opt.exilus = {};
  Object.entries(st.mods || {}).forEach(([id, s]) => { if (modById(id)) opt.mods[id] = norm(s); });
  // `none` IS THE SLOT'S RANGE and survives, on every axis. It is the axis's
  // "searched empty" mark, so deleting it here would silently widen a scope
  // back to 1–1 on every preset load.
  Object.entries(st.exilus || {}).forEach(([id, s]) => {
    if (id === "none" || (modById(id) || {}).exilus) opt.exilus[id] = norm(s);
  });
  if (st.size) opt.size = st.size;
  // Presets written before the range existed carry no min, and `?? 0` is what
  // they meant: with marks in them the DERIVED floor is at least 1 anyway, so
  // the two readings differ only on a scope with nothing marked — which such a
  // preset, by definition, is not. `??` rather than `||`, since 0 is now a
  // value a reader can deliberately store.
  opt.min = Math.min(st.min ?? 0, opt.size);
  // Arcanes: another SLOT's arcanes are not equippable here, so they drop
  // rather than becoming search dimensions the run cannot use.
  opt.arcanes = {};
  const w = $("weapon").value;
  Object.entries(st.arcanes || {}).forEach(([id, s]) => {
    // ANY of the weapon's pools, not just the first — see arcaneFitsWeapon.
    // A `none:<pool>` mark is the SEAT'S RANGE rather than an arcane, and it
    // is kept when this weapon actually has that seat — so a scope imported
    // from a weapon with a Primary seat does not leave a range behind on one
    // that has none.
    if (id.startsWith("none:")) {
      if ((weaponInfo(w) || {}).arcane_pools?.includes(id.slice(5))) opt.arcanes[id] = norm(s);
    } else if (arcaneFitsWeapon(w, id)) {
      opt.arcanes[ARCANE_RENAMED[id] || id] = norm(s);
    }
  });
  // Modes: a scope carried over from another weapon names modes this one does
  // not have, and a search over none of them is not a scope — so what does not
  // apply drops and the build's own mode stands in.
  opt.modes = {};
  const mopts = modeOpts(weaponInfo(w) || {});
  Object.entries(st.modes || {}).forEach(([id, s]) => {
    if (mopts.some(([o]) => o === id)) opt.modes[id] = norm(s);
  });
  if (mopts.length && !Object.keys(opt.modes).length) opt.modes = { [mode]: "fixed" };
  // Evolutions: keep only ids the CURRENT weapon's tiers actually offer (ids
  // are globally unique, so a family sharing evolutions imports cleanly and a
  // different weapon's ids just drop).
  const tiers = weaponEvos();
  opt.evos = {};
  Object.entries(st.evos || {}).forEach(([t, m]) => {
    const tier = tiers.find((x) => String(x.tier) === String(t));
    if (!tier) return;
    const valid = {};
    Object.entries(m || {}).forEach(([id, s]) => {
      // `none` is the tier's RANGE, not one of its perks — see `slotRange`.
      if (id === "none" || tier.options.some((o) => o.id === id)) valid[id] = norm(s);
    });
    if (Object.keys(valid).length) opt.evos[t] = valid;
  });
  // FINALISTS IS THE ONLY THING LEFT HERE. A preset written
  // before today also carries `runs` and `threads`; both are read by nothing
  // now — the run count is a preference of the reader's (`OPT_RUNS_KEY`) and
  // the thread count is the topbar's compute share — so they are not migrated
  // into either, they are simply dropped the next time this scope is saved.
  if (st.finalists) optRun.finalists = st.finalists;
  const f = $("opt-finalists");
  if (f) f.value = optRun.finalists;
}

function applyOptPreset(st) {
  applyOptState(st);
  optSeeded = true;
  renderOpt(); updateOptEstimate();
}

function renderOptPresetBars() {
  const bar = $("preset-bar-" + OPT_DOMAIN);
  if (!bar) return;
  renderPresetBarIn(bar, optBarCfg());
}

/// The search bar's document model — what the bar and the agent door both
/// pick, start and copy.
function optBarCfg() {
  return {
    domain: OPT_DOMAIN,
    label: tr("Searches"),
    noun: "search",
    hint: "scope + final round; import filters per axis",
    load: loadOptPresets,
    store: storeOptPresets,
    active: () => activeOptPreset,
    setActive: (n) => { activeOptPreset = n; localStorage.setItem(presetActiveKey(OPT_DOMAIN), n); },
    snapshot: snapshotOpt,
    apply: (st) => applyOptPreset(st || {}),
    blank: blankOpt,
    rerender: renderOptPresetBars,
  };
}

function renderOptTools() {
  const t = $("opt-picker-tools");
  const pols = ["Madurai", "Naramon", "Vazarin", "Umbra"].filter((p) => currentPool.some((m) => m.polarity === p));
  t.innerHTML =
    `<label>${escHtml(tr("Sort"))} ` + ddButton("opk-sort", {
      value: optPrefs.sort,
      items: [{ value: "name", label: tr("Name") }, { value: "drain", label: tr("Drain") }],
      onPick: (v) => { optPrefs.sort = v; renderOptTools(); renderOptModList(); },
    }) + `</label>` +
    `<button id="opk-dir" class="ghost-btn small" title="direction">${optPrefs.dir === "asc" ? "▲" : "▼"}</button>` +
    `<span class="pk-pols"><span class="pk-pol ${!optPrefs.pol ? "sel" : ""}" data-p="">all</span>` +
    pols.map((p) => `<span class="pk-pol ${optPrefs.pol === p ? "sel" : ""}" data-p="${p}" title="${p}">${imgTag(POL(p), "pol")}</span>`).join("") +
    `</span>` +
    // QUICK CALC, on a button rather than on every edit. The builder's scan
    // follows an opened slot because opening one IS the question; here every
    // click on a pool/req control would restart ~250 engagements, and the
    // scope is edited many clicks in a row.
    `<span class="pk-gain"><button id="opk-gain" class="ghost-btn small"${optGain.running ? " disabled" : ""}>${
      optGain.running ? `${optGain.done}/${optGain.total}` : escHtml(tr("quick calc"))}</button>` +
    (optWinnerMods()
      ? ddButton("opk-gain-ref", {
        value: optGain.mode,
        title: tr("the build every number is measured on"),
        items: [
          { value: "require", label: tr("vs required"), hint: tr("the mods you have pinned") },
          { value: "winner", label: tr("vs winner"), hint: tr("the build the search returned") },
        ],
        onPick: (v) => { optGain.mode = v; renderOptTools(); renderOptPairings(); renderOptModList(); renderOptArcanes(); renderOptEvos(); },
      })
      : "") +
    `</span>`;
  $("opk-dir").onclick = () => { optPrefs.dir = optPrefs.dir === "asc" ? "desc" : "asc"; renderOptTools(); renderOptModList(); };
  t.querySelectorAll(".pk-pol").forEach((o) => o.onclick = () => { optPrefs.pol = o.dataset.p || null; renderOptTools(); renderOptModList(); });
  // All three axes are on screen at once and all three now carry numbers, so
  // a tick repaints all three — a chip that appeared on one list and not the
  // others would read as "this axis was not scanned".
  const paint = () => {
    renderOptTools(); renderOptPairings(); renderOptModList();
    renderOptArcanes(); renderOptEvos();
  };
  $("opk-gain").onclick = () => scanOptGains(() => paint());
}

// A chip's ✕ removes; the chip itself REVEALS the mod in the list below.
// Making the whole chip a delete button meant reaching for a selected mod to
// look at it threw it away instead — and the ✕ was sitting
// right there looking like the control that did it.
function revealOptMod(id) {
  const m = modById(id);
  if (!m) return;
  // The list is filtered; a chip must be able to reach a row the current
  // filter hides, so clear whatever would keep it off screen.
  if (optPrefs.pol && m.polarity !== optPrefs.pol) { optPrefs.pol = null; renderOptTools(); }
  const q = ($("opt-mod-filter").value || "").trim().toLowerCase();
  if (q && !searchBlob(m).includes(q)) $("opt-mod-filter").value = "";
  renderOptModList();
  const row = $("opt-mods").querySelector(`.opt .seg[data-m="${CSS.escape(id)}"]`);
  if (!row) return;
  const box = row.closest(".opt");
  box.scrollIntoView({ block: "center", behavior: "smooth" });
  box.classList.add("revealed");
  setTimeout(() => box.classList.remove("revealed"), 1600);
}

function renderOptModSel() {
  const chip = (id, cls) => {
    const m = modById(id);
    return `<span class="oselchip ${cls}" data-m="${id}" title="${escHtml(tr("click to find it in the list below"))}">`
      + `${m ? m.name : id}<button class="oselx" data-x="${id}" title="${escHtml(tr("remove"))}">✕</button></span>`;
  };
  const req = Object.keys(opt.mods).filter((id) => opt.mods[id] === "fixed").map((id) => chip(id, "fixed"));
  const pool = Object.keys(opt.mods).filter((id) => opt.mods[id] === "search").map((id) => chip(id, "search"));
  const box = $("opt-mods-sel");
  // TRANSLATED, and it was not: these three lines and the size row under them
  // are meant to read down as one sentence, and two of them were English on a
  // Chinese page.
  box.innerHTML =
    (req.length ? `<div class="oselrow"><span class="osellbl">${escHtml(tr("required"))} (${req.length}/${opt.size})</span>${req.join("")}</div>` : "") +
    (pool.length ? `<div class="oselrow"><span class="osellbl">${escHtml(tr("pool"))} (${pool.length})</span>${pool.join("")}</div>` : "") +
    (!req.length && !pool.length ? `<div class="sim-empty">${escHtml(tr("nothing marked — the search is the bare weapon. Mark mods below as pool or required."))}</div>` : "");
  box.querySelectorAll("[data-x]").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      delete opt.mods[el.dataset.x];
      renderOptMods(); renderOptExilus(); updateOptEstimate();
    }));
  box.querySelectorAll(".oselchip[data-m]").forEach((el) =>
    el.addEventListener("click", () => revealOptMod(el.dataset.m)));
}

/// THE PAIRING LADDER — the quick calc's first statement, above the mods.
///
/// Absent until there is a choice to make: one pairing is not a ladder, and a
/// scope with no elemental mod has nothing to say. What it reports is the
/// reference build measured every way its elements can pair, best first.
function renderOptPairings() {
  const box = $("opt-pairings");
  if (!box) return;
  const fresh = optGain.key === optGainKey();
  const rows = fresh ? optGain.orders : [];
  if (rows.length < 2) { box.innerHTML = ""; return; }
  const head = `${tr("element pairings")} · ${rows.length} · ${escHtml(optGain.metric)} · ${escHtml(optGain.note)}`;
  box.innerHTML = `<div class="pairbox"><div class="pairhead">${head}</div>${rows.map((o, i) => `
    <div class="pairrow${i === 0 ? " best" : ""}">
      <span class="pl">${pairingLabel(o.combined, o.leftover)}</span>
      <span class="pv">${o.value == null ? "—" : sig2(o.value)}</span>
      <span class="pd">${i === 0 ? tr("best") : gainPct(o.pct)}</span>
    </div>`).join("")}</div>`;
}

function renderOptModList() {
  const q = ($("opt-mod-filter").value || "").trim().toLowerCase();
  // Exilus mods are IN this list too — all 9 slots accept them (game rule),
  // so marking one here makes it compete for a MAIN slot; the exilus SLOT
  // has its own block below.
  const hits = poolWithRivens()
    // A STANCE IS NOT A MAIN-SLOT MOD, and this list offered every melee
    // weapon's as one until 2026-08-29. The builder's picker has run the same
    // filter in both directions since the stance slot landed — a stance is
    // legal there and NOWHERE else — and this copy of the list never grew it,
    // so marking one here asked the search for a build nobody can hold.
    // Searching the stance SLOT is a real axis and is not this: it wants the
    // treatment the exilus slot has, in the optimizer as well as here.
    .filter((m) => !m.stance)
    .filter((m) => !optPrefs.pol || m.polarity === optPrefs.pol)
    .filter((m) => !q || searchBlob(m).includes(q))
    .sort((a, b) => {
      // Rivens first, as their own block, as in the builder's picker.
      const r = (b.riven ? 1 : 0) - (a.riven ? 1 : 0);
      if (r) return r;
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
    return modRow(m, {
      cls: `${st === "off" ? "" : st} ${dead ? "dis-soft" : ""}`,
      title: why || (m.effects || []).join(" · "),
      chips: optGainChipFor(m.id),
      note: optPairingNoteFor(m.id),
      // …AND THE OPTIMIZER BINDS A SET.
      trailing: oseg(`data-m="${m.id}"`, st, {
        poolDead: dead,
        reqDead: dead || reqBlocked,
        reqTitle: !dead && reqBlocked
          ? tr("pooled mods reserve ≥1 open slot — raise max mods or clear pools") : "",
      }),
    });
  };
  // THE OPTIMIZER'S OWN SCAN, same component and a different state. It has no
  // slots and therefore no axis — there is one list and one question — so the
  // strip is asked without one.
  $("opt-mods").innerHTML = scanStrip(optGain) + (hits.length
    ? sectionedRows(hits, (m) => (m.riven ? "Riven" : "Mods"), row)
    : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`);
  $("opt-mods").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => { e.stopPropagation(); markOptMod(el.dataset.m, el.dataset.s); }));
}

/// A MOD'S MARK IN THE SEARCH: "fixed" (required), "search" (pooled), or the
/// same mark again to clear it. Requiring one clears its family everywhere.
function markOptMod(id, want) {
  const cur = opt.mods[id] || "off";
  if (cur === want) delete opt.mods[id]; else opt.mods[id] = want;
  if (opt.mods[id] === "fixed") clearFamMarks(id);
  renderOptMods(); renderOptExilus(); updateOptEstimate();
}

/// HOW MANY MODS A SEARCHED BUILD HOLDS (min to size, 0 to 8) and how many
/// builds reach the last round. The two bounds push each other, never cross.
function setOptSizes({ size, min, finalists }) {
  if (size != null) {
    opt.size = Math.max(0, Math.min(8, size));
    if (opt.min > opt.size) opt.min = opt.size;
  }
  if (min != null) {
    opt.min = Math.max(0, Math.min(8, min));
    if (opt.min > opt.size) opt.size = opt.min;
  }
  if (finalists != null) optRun.finalists = Math.max(1, Math.min(100, finalists));
  $("opt-size").value = opt.size; $("opt-min").value = opt.min; $("opt-finalists").value = optRun.finalists;
  updateOptEstimate();
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
  const exSearch = Object.keys(opt.exilus)
    .filter((id) => id !== "none" && opt.exilus[id] === "search").length;
  // Pooled exilus marks ARE the option set, and the slot's RANGE says whether
  // the empty choice sits beside them: nothing marked, or 0–0, is one option.
  const exOptions = exFixed ? 1
    : opt.exilus.none === "fixed" || !exSearch ? 1
      : exSearch + (opt.exilus.none === "search" ? 1 : 0);
  // Required in BOTH blocks = impossible (a mod equips once).
  const dupReq = exFixed && opt.mods[exFixed] === "fixed" ? exFixed : null;
  // Slots MULTIPLY: a weapon that seats two arcanes is searched over pairs,
  // because the best Primary and the best Secondary are not independent
  // questions.
  const arcCount = arcanePools().reduce((n, p) => n * arcaneOptionsIn(p), 1);
  // MODE AND VALENCE ARE FACTORS TOO, and they were missing (found 2026-08-29,
  // while completing the axis model). The server's variant table is
  // `modes × evo_sets × valences` — pooling a second mode genuinely doubles
  // the search — and this line counted only the evolution sets, so the
  // candidate count UNDER-reported by exactly those two. It is the arcane
  // over-count's mirror image, and both came from a factor written by hand
  // instead of read off the axis.
  const markedCount = (marks) => {
    const ks = Object.keys(marks);
    if (ks.some((k) => marks[k] === "fixed")) return 1;
    return Math.max(1, ks.filter((k) => marks[k] === "search").length);
  };
  const modeCount = markedCount(opt.modes);
  const valCount = valenceSpec($("weapon").value) ? markedCount(opt.valence) : 1;
  // ONE FACTOR PER TIER, from the tier's own RANGE — a pin is one option, an
  // empty tier is one option, and 0–1 is the marked ones plus that empty.
  let evoProduct = 1;
  (weaponEvos()).forEach((t) => {
    const m = opt.evos[t.tier] || {};
    if (evoPinned(t.tier)) { return; }                       // settled: one option
    const pooled = evoRealMarks(t.tier).filter((id) => m[id] === "search").length;
    if (m.none === "fixed" || !pooled) return;               // the empty tier, alone
    evoProduct *= pooled + (m.none === "search" ? 1 : 0);
  });
  const size = opt.size;
  // The pool group occupies ≥1 slot: with pools marked, every build carries
  // at least one pooled mod (k starts above the required count).
  // ...and `opt.min` raises that floor: "exactly 8 mods" is min 8, max 8.
  // A CEILING OF 0 OUTRANKS THE DERIVED FLOOR — the page's half of the rule
  // `min_slots` applies on the server. The marks say "use these" and 0–0 says
  // "not this time, but keep them"; without this the two contradict and the
  // count comes out empty for a request the server answers happily.
  const minK = size === 0 ? 0 : Math.max(fixed + (search > 0 ? 1 : 0), opt.min);
  // …AND THE READER IS TOLD WHEN THE MARKS RAISE THEIR OWN FLOOR. The floor is the larger of what you TYPED and what the marks
  // IMPLY — every required mod is in every candidate, and pooling is the
  // statement that at least one pooled mod is too — so the box could say 0 over
  // a search that never looks below 3, which is a control that lies about what
  // it does. It is stated only when the two DIFFER: a line repeating the two
  // numbers beside it distinguishes nothing.
  const eff = $("opt-size-eff");
  if (eff) {
    eff.textContent = minK > opt.min
      ? tr("actually {min}–{max}: {why}")
        .replace("{min}", minK).replace("{max}", size)
        .replace("{why}", fixed > 0 && search > 0
          ? tr("{n} required, plus at least one pooled").replace("{n}", fixed)
          : fixed > 0
            ? tr("{n} required").replace("{n}", fixed)
            : tr("at least one pooled"))
      : "";
  }
  let subsets = 0;
  for (let k = minK; k <= size; k++) subsets += nChooseK(search, k - fixed);
  subsets *= evoProduct * exOptions;
  const jobs = subsets * arcCount * modeCount * valCount;
  // Pooled mods reserve ≥1 open slot (reachable only via shrinking max
  // mods after marking — req clicks are blocked before this point).
  // …and the same exemption for the refusal that states that floor. At a
  // ceiling of 0 there is no slot for a pooled mod to reserve, and none is
  // wanted.
  const poolStarved = size > 0 && search > 0 && fixed >= size;
  const valid = fixed <= size && subsets > 0 && !dupReq && !poolStarved;
  // No cap (user: use local resources). Show the estimate + a heads-up when big;
  // only block genuinely invalid scopes.
  const big = jobs > 500000;
  // Scenario + funnel preview: what every candidate is actually tested
  // against — the Sim panel's enemy settings, and the successive-halving
  // schedule (survivors × runs per round; a JS mirror of schedule()).
  let scenario = "";
  if (valid) {
    const en = allEnemies().find((e) => e.id === sim.enemy) || {};
    // Mirror of schedule_to()'s auto-planned cadence: k = ceil(log8(N/F))
    // rounds, even log-space culls landing exactly on the finalists, runs
    // from a halving cost budget ((ρ/2)^i, capped at final/4), then the
    // guaranteed final. Racing/amnesty adapt this plan at runtime.
    const F = optRun.finalists, FR = finalRuns();
    const N = Math.round(jobs);
    const rounds = [];
    if (N > F) {
      const k = Math.max(1, Math.ceil(Math.log(N / F) / Math.log(8)));
      const rho = Math.pow(N / F, 1 / k);
      const growth = Math.max(1, rho / 2);
      const cap = Math.max(1, Math.floor(FR / 4));
      let field = N, runsF = 1;
      for (let i = 0; i < k; i++) {
        const keep = i + 1 === k ? F : Math.max(F, Math.round(field / rho));
        rounds.push([Math.min(cap, Math.max(1, Math.round(runsF))), keep]);
        field = keep; runsF *= growth;
      }
    }
    rounds.push([FR, F]);
    const parts = [];
    let field = Math.round(jobs);
    rounds.forEach(([r, k]) => { parts.push(`${field.toLocaleString()}×${r}`); field = Math.min(field, k); });
    // The scenario's Measure is the SIMULATOR's; the funnel still ranks by
    // kills whatever it says, so say so rather than letting the shared state
    // imply a DPS search that does not exist yet.
    // THE SEARCH RANKS BY KILLS whatever the scenario measures by, so any
    // metric that is not the kill rate is a mismatch worth saying — named, so
    // the sentence stays true when a third one lands.
    const met = metricOf(sim.metric);
    const measured = met.per_minute
      ? ""
      : ` · <span class="warn">${escHtml(tr("the search ranks by kills — the scenario's {m} measure applies to the simulator")
          .replace("{m}", metricLabel(met)))}</span>`;
    scenario = `<div class="opt-scn">each build vs <b>${en.name || sim.enemy}</b> Lv ${sim.level}${sim.steel_path ? " (SP)" : ""}${sim.distance > 0 ? ` · ${sim.distance} m` : ""} · ${sim.headshot_pct}% headshots${sim.aiming ? "" : " · hip-fire"} · ${sim.duration} s engagements · planned funnel (builds×runs): ${parts.join(" → ")} → ${F} finalists at ${FR.toLocaleString()} runs (racing cuts deeper, tie-amnesty keeps up to 2×)${measured}</div>`;
  }
  // ONE total, no decomposition — "×N arcanes" leaked a search-internal
  // dimension into the summary line.
  $("opt-estimate").innerHTML = (valid
    ? `~<b>${Math.round(jobs).toLocaleString()}</b> candidate builds${big ? ` <span class="warn">— large; this may take a while</span>` : ""}`
    : `<span class="warn">${dupReq ? `${(modById(dupReq) || { name: dupReq }).name} is required in both blocks — a mod equips once` : poolStarved ? `pooled mods reserve ≥1 open slot — raise max mods or clear pools` : `more required (${fixed}) than slots (${size})`}</span>`) + scenario;
  // Never re-enable while a background job is still running.
  $("run-opt").disabled = !valid || optJobId != null;
  // Every scope mutation funnels through here — AUTO-SAVE into the active
  // preset (debounced), same contract as the build bar.
  optSaveTimer = deferSave("search", () => {
    if (presetApplying) return;
    const ps = loadOptPresets();
    // A SEARCH IS BORN ON THE FIRST EDIT, like a build (`markPresetDirty`).
    //
    // …BUT THIS FUNNEL IS NOT ONLY AN EDIT. Unlike `markPresetDirty`, which
    // edit handlers call by hand, `updateOptEstimate` also runs on every render
    // — including the first paint of a tab nobody has touched — so "it ran" is
    // no evidence of an edit. The evidence is the STATE: a search that is still
    // `blankOpt()` is a search nobody has made. Compared as a whole rather than
    // field by field, so an axis added tomorrow counts on the day it is added.
    if (!activeOptPreset) {
      if (sameState(snapshotOpt(), blankOpt())) return;
      const name = newPresetName(ps);
      ps.push({ name, savedAt: Date.now(), state: snapshotOpt() });
      storeOptPresets(ps);
      activeOptPreset = name;
      localStorage.setItem(presetActiveKey(OPT_DOMAIN), name);
      renderOptPresetBars();
      return;
    }
    const at = ps.findIndex((p) => p.name === activeOptPreset);
    if (at < 0) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: snapshotOpt() };
    storeOptPresets(ps);
    renderOptPresetBars();
  }, 400);
}
let optSaveTimer = null;

// The optimize run is a BACKGROUND JOB on the server: POST /api/optimize
// returns a job_id immediately; we poll /api/optimize/status for live funnel
// progress (overall % is exact — the schedule fixes every round's sim count
// up front) and can /api/optimize/cancel. On page reload, init() reattaches
// to a still-running job via a no-id status call.
let optJobId = null;
let optPollTimer = null;
let optCancelling = false; // survives the poll's 500 ms re-renders
let optLastStatus = null; // the running job's last poll, which the door reads

const postJson = (url, body) => api(url, body);

async function runOptimize() {
  clearCheckpoint(); // a fresh run supersedes any interrupted one
  optLastStatus = null;
  $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
  $("opt-results").innerHTML = `<div class="placeholder">starting…</div>`;
  try {
    // pool/req collapse to effective option lists: a req pins its slot/tier
    // (single option), pools are the searched set.
    // A tier collapses to its effective option list, and `none` is one of the
    // options — it is this tier's range saying "searched empty". A REAL pin is looked for first, for the reason `slotRange`
    // asks in that order: a settled tier outranks a stale range mark.
    const evolutions = {};
    Object.entries(opt.evos).forEach(([t, m]) => {
      const keys = Object.keys(m);
      const f = keys.find((id) => id !== "none" && m[id] === "fixed")
        || keys.find((id) => m[id] === "fixed");
      const ids = f ? [f] : keys.filter((id) => m[id] === "search");
      if (ids.length) evolutions[t] = ids;
    });
    // The MARKS, like `mods` and `exilus` — a pin means "this slot is
    // settled", which a flat list of ids cannot say. The server splits them
    // by pool and takes the product.
    const arcs = {};
    Object.keys(opt.arcanes).forEach((id) => {
      if (opt.arcanes[id] && opt.arcanes[id] !== "off") arcs[id] = opt.arcanes[id];
    });
    // A POOLED CARD ON THE EVERY-RANK LIST is pooled at each of its ranks.
    const withRanks = (marks, lower) => {
      const out = { ...marks };
      Object.entries(marks).forEach(([id, st]) => {
        if (st === "search") lower(id).forEach((x) => { out[x.id] = "search"; });
      });
      return out;
    };
    const modLower = (id) => { const m = modById(id); return m ? lowerRanks(m) : []; };
    const arcLower = (id) => { const a = arcaneById(id); return a ? lowerArcaneRanks(a) : []; };
    const body = {
      weapon: $("weapon").value,
      mods: withRanks(opt.mods, modLower),
      rivens: rivenPayload(),
      build_size: opt.size,
      build_min: opt.min,
      arcanes: withRanks(arcs, arcLower),
      evolutions,
      // HOW IT IS PLAYED, as a search dimension — the marks, like the arcanes'.
      // `mode` travels too and is what a scope with no axis falls back to, so
      // every caller written before this keeps meaning what it meant.
      modes: opt.modes,
      mode,
      // THE VALENCE, pinned to the builder's. It is not a search axis yet —
      // every candidate is built with the same bonus, which is the weapon the
      // replay will fire. The day it becomes an axis it joins `modes` above.
      // THE VALENCE AXIS, as marks — the same shape `modes` takes. `_element`
      // travels too and is what a scope with no axis falls back to, so every
      // caller written before this keeps meaning what it meant.
      valence: opt.valence,
      valence_element: valence.element,
      valence_bonus: valence.bonus,
      // THE PARTS, pinned to the builder's. Not a search axis yet: every
      // candidate is assembled the same way, which is the weapon the replay
      // will fire. The day it becomes one it joins `modes` above — 100 grip and
      // loader pairs per chamber is a real scope and a real cost, and it is the
      // owner's call whether a search should spend it.
      ...(assembly ? { assembly: { ...assembly } } : {}),
      exilus: withRanks(opt.exilus, modLower),
      // THE STANCE, PINNED TO THE BUILDER'S — not a search axis, the way the
      // valence and the parts above are not. A stance decides what a swing IS,
      // so a melee search without one ranks builds nobody holds; the mod list
      // filters stances out (a stance is legal in the stance slot and nowhere
      // else), so this is the only thing that puts it back.
      stance: slotModId(slots[STANCE]) || "",
      // THE FIGHT, WHOLE AND DERIVED — never a hand-written list of its
      // fields. This was twelve of them copied out one by one, under a comment
      // claiming "the TENNO travels whole", which was true only by inspection:
      // every scenario field added since had to be remembered here, and the
      // one nobody remembered would score builds under a fight the replay
      // never runs. That is the divergence AGENTS.md's hard rule is about, and
      // a spread cannot forget (`eximus` was the field that found it).
      //
      // …and it is `theFight()`, the same call the simulator makes, rather
      // than the SNAPSHOT shape of it this sent until 2026-08-17. It also
      // stopped building the buff map itself: whole rather than pruned is what
      // `theFight` does for everyone now, for the reason this spot argued
      // first — a candidate carries mods you are not holding, and a setting for
      // one of them is what the scenario's wide buff view exists to record.
      //
      // Safe to send everything: `parse_fight` reads the fight's fields and
      // `parse_optimize` reads only its own five, so a scenario field the
      // optimizer has no opinion about simply arrives and is used.
      ...theFight(),
      // No `threads`: the server reads an absent one as 0 = auto, which is the
      // only answer this page has now that the compute share is the topbar's.
      final_runs: finalRuns(), finalists: optRun.finalists,
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
  optCancelling = false;
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
  optLastStatus = st;
  if (st.phase === "error") {
    optFinish(`<div class="error">optimize failed: ${(st.result && st.result.error) || "unknown error"}</div>`);
    return;
  }
  if (st.phase === "done" || st.phase === "cancelled") {
    optFinish();
    if (st.result && st.result.results && st.result.results.length) {
      renderOptResults(st.result);
      // …AND EVERY FINALIST GOES TO THE BOARD. After the results are drawn, so
      // a slow door never delays the answer the reader asked for.
      offerOptBoardSubmit(st.result);
      // A cancel is not necessarily the end of the search — the run stopped,
      // but its resume point is still on disk. Offer it under the results.
      if (st.phase === "cancelled") appendResumeOffer();
      offerSupportOnce($("opt-results"));
    } else {
      $("opt-results").innerHTML = `<div class="placeholder">cancelled before anything had been ranked — no results</div>`;
    }
    return;
  }
  renderOptProgress(st);
  optPollTimer = setTimeout(pollOptimize, 500);
}

function renderOptProgress(st) {
  const pct = st.sims_planned ? Math.min(100, (100 * st.sims_done) / st.sims_planned) : 0;
  const head = st.phase === "enumerating"
    ? `enumerating candidates…${st.enumerated ? ` ${st.enumerated.toLocaleString()} so far` : ""}${st.sims_done ? ` · ${st.sims_done.toLocaleString()} screened` : ""}`
    : `round ${st.round}/${st.rounds} — ${(st.round_jobs || 0).toLocaleString()} jobs × ${st.round_runs} runs`;
  const notes = (st.notes || []).map((n) =>
    `<div class="opt-note">round ${n.round}: ${n.jobs.toLocaleString()} × ${n.runs} (${n.by_kills ? "kills" : "dmg"}) → keep ${n.kept.toLocaleString()} · best ${n.by_kills ? sig2(kpm(n.best, sim.duration)) + " KPM" : n.best.toExponential(2) + " dmg"} · ${(n.ms / 1000).toFixed(1)}s</div>`
  ).join("");
  const sub = st.phase === "enumerating"
    ? ""
    : `<div class="opt-prog-sub">${pct.toFixed(1)}% · ${st.sims_done.toLocaleString()} / ${st.sims_planned.toLocaleString()} sims${st.jobs ? ` · ${st.jobs.toLocaleString()} candidate builds` : ""}</div>`;
  $("opt-results").innerHTML = `<div class="opt-progress">
    <div class="opt-prog-head"><span>${head}</span><span class="opt-elapsed">${st.elapsed_s.toFixed(0)}s</span></div>
    <div class="opt-bar"><i style="width:${pct}%"></i></div>
    ${sub}${notes}
    <button class="ghost-btn small" id="opt-cancel" ${optCancelling ? "disabled" : ""}>${optCancelling ? "Cancelling…" : "Cancel"}</button>
  </div>`;
  // The 500 ms poll re-renders this whole block — `optCancelling` keeps the
  // button's cancelling state alive across re-renders, or it snaps back to a
  // live-looking "Cancel" and cancellation looks ignored.
  $("opt-cancel").addEventListener("click", cancelOptimize);
}

/// STOPPING A SEARCH, from its button or the agent door. The poll reports the
/// outcome; a stopped search keeps what it had ranked.
async function cancelOptimize() {
  optCancelling = true;
  const b = $("opt-cancel");
  if (b) { b.disabled = true; b.textContent = "Cancelling…"; }
  try { await postJson("/api/optimize/cancel", { id: optJobId }); } catch (e) { /* poll reports */ }
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
      return;
    }
  } catch (e) { /* no server-side job — nothing to reattach */ }
  offerResume(); // nothing is running: a reload may have killed a wasm run
}

// The run itself is gone, but the field it had narrowed to is not. Offer to
// continue from the last completed round instead of paying for it again.
// Never auto-start: resuming costs minutes of the visitor's CPU, so it takes a
// click — and the offer only appears for the weapon the checkpoint belongs to.
function offerResume() {
  const el = resumeControl();
  if (!el) return;
  const box = $("opt-results");
  box.innerHTML = "";
  box.append(el);
}

// The same control, under a cancelled run's leaderboard.
function appendResumeOffer() {
  const el = resumeControl();
  if (el) $("opt-results").append(el);
}

function resumeControl() {
  const saved = loadCheckpoint();
  const box = $("opt-results");
  if (!saved || !box || optJobId != null) return null;
  if (saved.body.weapon !== $("weapon").value) return null;
  const cp = saved.cp;
  const el = document.createElement("div");
  el.className = "opt-resume";
  const sel = $("weapon");
  const shown = (sel.selectedOptions[0] || {}).textContent || sel.value;
  const where = cp.kind === "screen"
    ? `while screening — ${cp.start_seq.toLocaleString()} candidates walked, `
      + `${(cp.keepers.length / 2).toLocaleString()} jobs still standing`
    : `after round ${cp.round} — ${cp.alive.length.toLocaleString()} builds still standing`;
  el.innerHTML = `<div>An optimization for <b>${escHtml(shown)}</b> stopped ${where}.</div>`;
  const go = document.createElement("button");
  go.className = "ghost-btn"; go.textContent = "resume it";
  go.onclick = () => resumeOptimize(saved);
  const no = document.createElement("button");
  no.className = "ghost-btn small"; no.textContent = "discard";
  no.onclick = () => { clearCheckpoint(); el.remove(); };
  el.append(go, no);
  return el;
}

async function resumeOptimize(saved) {
  $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
  $("opt-results").innerHTML = `<div class="placeholder">${saved.cp.kind === "screen"
    ? `re-walking to the saved point (${saved.cp.start_seq.toLocaleString()} candidates)…`
    : `resuming from round ${saved.cp.round}…`}</div>`;
  try {
    // The STORED body, not the current form: the checkpoint describes a field
    // narrowed under that exact scope, and re-deriving the body from the UI
    // would let an edited setting resume into a run it never belonged to.
    const r = await postJson("/api/optimize", { ...saved.body, __resume: saved.cp });
    if (!r || r.ok === false) {
      clearCheckpoint();
      optFinish(`<div class="error">resume failed: ${r ? r.error : "no data"}</div>`);
      return;
    }
    optJobId = r.job_id;
    pollOptimize();
  } catch (e) {
    optFinish(`<div class="error">resume failed: ${e}</div>`);
  }
}

const prettify = (id) => id.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
const arcName = (id) => (id === "none" ? "no arcane" : ((META.arcanes || []).find((a) => a.id === id) || {}).name || prettify(id));
const evoName = (id) => {
  for (const t of weaponEvos()) { const o = t.options.find((o) => o.id === id); if (o) return o.name; }
  return prettify(id);
};

/// The ranking on screen. Kept so the quick calc can offer the WINNER as its
/// reference build — a mod measured on two required cards meets no diminishing
/// returns, and the winner is the same question asked on a build that is full.
/// Cleared with the results themselves when the weapon changes.
let optLast = null;

function renderOptResults(r) {
  optLast = r;
  const rows = (r.results || []).map((res) => {
    // HOW THIS ROW WAS PLAYED, drawn only when the search RANGED over modes —
    // otherwise every row would repeat the one answer the scope already
    // states. A ranking that mixes them has to say which is which: two rows
    // with the same mods and different modes are two different builds.
    const modes = new Set((r.results || []).map((x) => x.mode).filter(Boolean));
    // …AND THE SAME RULE FOR THE ELEMENT. A search that pinned one says it in
    // its scope; one that ranged over several has to name the winner's, or a
    // ranking of adversary builds is a list of rows nobody can reproduce.
    const valences = new Set((r.results || []).map((x) => x.valence).filter(Boolean));
    const detail = buildContentsHtml({
      mods: res.mods,
      exilus: res.exilus,
      arcanes: asArcaneList(res.arcane, (res.arcane || []).length),
      arcaneRanks: asArcaneList(res.arcane_rank, (res.arcane || []).length),
      evolutions: res.evolutions,
      modeLabel: modes.size > 1 && res.mode
        ? modeLabel(weaponInfo($("weapon").value) || {}, res.mode)
        : "",
      valence: valences.size > 1 ? res.valence : "",
      assembly: res.assembly,
      riven: (res.mods || []).includes("riven"),
    });
    return `<div class="opt-row">
      <div class="opt-head">
        <span class="opt-rank">#${res.rank}</span>
        <span class="opt-kills" id="opt-kpm-${res.rank}" data-search="${
          kpm(res.kill_progress ?? res.kills, r.duration)}">${
          sig2(kpm(res.kill_progress ?? res.kills, r.duration))}<small> KPM</small><span class="opt-repro pending" title="${
          escHtml(tr("re-measuring this build in the simulator"))}">·</span></span>
        <span class="opt-dps">${Math.round(res.dps || res.effective_dps || 0).toLocaleString()} DPS</span>
        <span class="opt-total">${sig2(res.kill_progress ?? res.kills)} kill score / ${Math.round(r.duration || 0)}s</span>
        <span class="forma-badge legal">${res.forma.used} Forma</span>
        <button class="ghost-btn small opt-add" title="${escHtml(tr("save as a new build"))}" data-r='${JSON.stringify(res).replace(/'/g, "&#39;")}'>+ add</button>
      </div>
      ${detail}
    </div>`;
  }).join("");
  // WHAT THE SEARCH COVERED, whenever it did not cover everything. This is not
  // CANCELLED — that means you stopped it and this is the best it had. This
  // means the scope is bigger than one search's budget, so the ranking below
  // was chosen from a SAMPLE. The sample is uniform over the whole space (the
  // search walks a shuffled index range), so the number is a real confidence
  // statement rather than an apology: at 3% of the space the winner is a good
  // build, not necessarily THE build.
  //
  // `exhaustive` is the other half and it is the one worth saying out loud:
  // when the search reaches the end of its space, the answer is not a
  // best-so-far, it is the optimum of everything you pooled.
  const cov = r.exhaustive
    ? `<span class="ok">${escHtml(tr("every build in this scope was searched"))}</span> · `
    : (r.coverage != null && r.coverage < 1
      ? `<span class="warn">${escHtml(tr("searched {pct}% of this scope ({n} of {total} builds) — a uniform sample, so this is a strong build rather than a proven best; pool fewer mods to search all of it"))
          .replace("{pct}", (r.coverage * 100).toFixed(r.coverage < 0.01 ? 3 : 1))
          .replace("{n}", (r.searched || 0).toLocaleString())
          .replace("{total}", Math.round(r.space || 0).toLocaleString())}</span> · `
      : "");
  $("opt-results").innerHTML = `<div class="opt-board" id="opt-board"></div><div class="opt-meta">${cov}${r.cancelled ? `<span class="warn">cancelled — best-so-far ranking (lower precision than a full run)</span> · ` : ""}${(r.jobs || 0).toLocaleString()} candidate builds · vs ${r.target.name} Lv ${r.target.level}${r.target.steel_path ? " (SP)" : ""} · ${r.headshot_pct ?? "?"}% headshots · ${r.duration ?? "?"} s engagements · ${r.finalists || 20} finalists × ${(r.final_runs || 1024).toLocaleString()} runs</div>${rows}`;
  $("opt-results").querySelectorAll(".opt-add").forEach((el) =>
    el.addEventListener("click", () => addResult(JSON.parse(el.dataset.r), el)));
  verifyOptRows(r);
}

/// THE NUMBER ON A ROW IS THE SIMULATOR'S.
///
/// The hard rule made operational on the PAGE. "The simulator is the truth"
/// covers the ENGINE, where `parse_fight` sees to it; a page with its own
/// translation of a ranked row into a build ranks by one thing while the
/// builder fires another, measured at 26 KPM against 15.
///
/// So each row is re-run through `/api/simulate` — with the request the SERVER
/// wrote for that candidate — and the KPM on screen is what came back. The
/// search's own figure keeps one job, ORDERING the list, since re-measuring
/// cannot reorder a ranking without making it meaningless.
///
/// The two are compared, and both sides report their own standard error, so
/// "they disagree" is arithmetic rather than a tolerance somebody picked: 4
/// sigma of the two combined. Any axis lost anywhere on the chain moves the
/// number and trips it, which is why this checks the ANSWER instead of counting
/// fields — it cannot go stale when an axis is added.
///
/// Top-down and sequential: the leader is what a reader looks at first, and
/// twenty engagements at the final round's precision is real time.
let optVerifyToken = 0;
async function verifyOptRows(r) {
  const token = ++optVerifyToken;
  const rows = (r.results || []).slice();
  for (const res of rows) {
    if (token !== optVerifyToken) return;          // a newer ranking owns the panel
    const el = $(`opt-kpm-${res.rank}`);
    if (!el) continue;
    const mark = el.querySelector(".opt-repro");
    if (!res.replay) {
      if (mark) { mark.className = "opt-repro stale"; mark.textContent = ""; mark.title = tr("this ranking predates the simulator re-run"); }
      continue;
    }
    // …ALSO THROUGH THE FLEET. Every ranked row is re-simulated, so on a crowd
    // ruler this is the finalist count TIMES a full simulation — the one place
    // on the page where the fleet is worth the most.
    let s = null;
    try { s = await simulateFleet(res.replay); } catch (_) { s = null; }
    if (token !== optVerifyToken) return;
    if (!s || s.ok === false || s.score == null) {
      if (mark) { mark.className = "opt-repro failed"; mark.textContent = "!"; mark.title = tr("the simulator refused this build — see the build's own card"); }
      continue;
    }
    const shown = kpm(s.score, r.duration);
    const search = Number(el.dataset.search) || 0;
    // FOUR SIGMA OF THE TWO COMBINED. Both are means of independent runs, so
    // their difference has the two standard errors added in quadrature — there
    // is no systematic gap to allow for, and any tolerance written as a flat
    // percentage would be too tight at 40 runs and too loose at 1000.
    const se = Math.hypot(kpm(s.score_se || 0, r.duration),
                          kpm(res.kill_progress_se || 0, r.duration));
    const off = Math.abs(shown - search) > Math.max(4 * se, 0.01 * Math.abs(search));
    el.firstChild.nodeValue = sig2(shown);
    if (mark) {
      mark.className = "opt-repro " + (off ? "off" : "ok");
      mark.textContent = off ? "≠" : "✓";
      mark.title = off
        ? tr("the simulator does not reproduce the search's own score for this build — the build shown may not be the one that was scored")
          + ` (${sig2(search)} → ${sig2(shown)} KPM)`
        : tr("re-run in the simulator: this is the simulator's own number for this build");
    }
    if (off) el.closest(".opt-row").classList.add("opt-unreproduced");
  }
}

// An optimizer result as a builder-builds preset STATE (snapshotState
// shape) — built without touching the build being edited.
async function resultToState(res) {
  // THE ROW'S OWN REQUEST, not a build re-derived from a description of one.
  //
  // `replay` is a complete simulate request written by the server out of the
  // very candidate it scored, so applying a result reads exactly ONE field and
  // there is nothing here to keep in step with the search. Every axis the row
  // was measured under is in it, including the ones nobody has invented yet.
  //
  // A row from a run predating `replay` still has to open, so the named fields
  // remain the fallback — and they are a translation, with everything that
  // implies: `mode` and `valence` fall back to the PAGE, which is the mode and
  // the element that run was launched in.
  const payload = res.replay || {
    mods: (res.mods || []).concat(
      res.exilus && res.exilus !== "none" ? [res.exilus] : [],
    ),
    evolutions: res.evolutions || [],
    arcane: res.arcane,
    arcane_rank: res.arcane_rank,
    mode: res.mode || mode,
    valence_element: res.valence || valence.element,
    valence_bonus: valence.bonus,
  };
  const st = stateFromBuild(payload, $("weapon").value, res.exilus);
  // …and the POLARITIES, which are the page's alone: a payload states the
  // build, and the cheapest layout that fits it is a plan the builder makes.
  st.slots = await planSlotsAlone(st.slots.map((s, i) => ({ mod: s.mod, pol: innate[i], rank: s.rank })));
  // NO `sim`. Copying the optimizer's own buff config into the scenario so
  // that "add then Run Sim" matches its score makes a result rewrite the fight
  // you are working in; a result is a BUILD. The two configs can still
  // disagree — the search's is scope-wide, the scenario's is this build's —
  // and that disagreement is now visible instead of resolved by silently
  // editing a preset the user owns.
  return st;
}

// "+ add" (not load): the result becomes a NEW preset
// appended after the existing builds; the build being edited is never
// clobbered. Auto-named "opt N" (rename in the preset bar if it earns a
// real name).
async function addResult(res, btn) {
  const state = await resultToState(res);
  const ps = loadPresetList(BUILDS);
  let n = 1;
  while (ps.some((p) => p.name === "opt " + n)) n++;
  const name = "opt " + n;
  ps.push({ name, savedAt: Date.now(), state });
  storePresetList(BUILDS, ps);
  renderPresetBar(); // the builder's bar shows the new chip when you switch back
  if (btn) { btn.textContent = "✓ " + name; btn.disabled = true; }
  return name;
}

/// RECLAIM WHAT THE OLD RULE LEFT BEHIND, once, on the way in.
///
/// `stripReplays` stops the growth; it does not undo it. Every replay written
/// before 2026-08-18 is still on the reader's disk, in a key for a weapon they
/// may never open again — and a browser that is already at its quota fails the
/// NEXT write, not the one that filled it, so without this the fix would only
/// arrive for whoever cleared their own storage first.
///
/// It rewrites each `wfsim-presets-*` key in place and only when the stripped
/// copy is actually smaller, so a reader with nothing to reclaim pays one parse
/// per key and writes nothing.
function reclaimStoredReplays() {
  let freed = 0;
  for (const k of Object.keys(localStorage)) {
    if (!k.startsWith("wfsim-presets-")) continue;
    const raw = localStorage.getItem(k) || "";
    // The cheap test first: a list with no replay in it is not worth parsing.
    if (!raw.includes('"replay"')) continue;
    let list = null;
    try { list = JSON.parse(raw); } catch (_) { continue; }
    if (!Array.isArray(list)) continue;
    const next = JSON.stringify(stripReplays(list));
    if (next.length >= raw.length) continue;
    try {
      localStorage.setItem(k, next);
      freed += raw.length - next.length;
    } catch (_) {
      // Full enough that even the SMALLER copy will not go in. Removing the key
      // is still better than leaving it: the collection re-creates itself.
      try { localStorage.removeItem(k); freed += raw.length; } catch (_) { /* nothing left to try */ }
    }
  }
  return freed;
}
try { reclaimStoredReplays(); } catch (_) { /* storage may be unavailable entirely */ }

