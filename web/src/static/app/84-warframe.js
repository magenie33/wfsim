// ---- THE WARFRAME BUILDER ----------------------------------------------
//
// `/warframes/<Wiki_Name>`: a frame's mods, arcanes, shards and Helminth, and
// what they resolve to. A builder and nothing else. The numbers are the
// engine's (`/api/warframe/panel`); what this draws of its own is the slot
// arithmetic the weapon builder also draws. docs/WARFRAMES.md.
const WF_BUILDS = "warframes";
const WF_EXILUS = 8;
const WF_AURA = 9;
// "30 [Base] × 2 [Orokin Reactor]" (W`Orokin_Reactor`).
const WF_BASE_CAPACITY = 60;
const WF_POLS = ["Madurai", "Naramon", "Vazarin", "Zenurik", "Unairu", "Umbra", "Omni"];
const WF_SCALE_TAG = { strength: "STR", duration: "DUR", range: "RNG", casting_speed: "CAST" };
const WF_CAPS = [["invulnerable", "Invulnerable"], ["status_cleanse", "Status cleanse"],
  ["status_immunity", "Status immunity"], ["damage_cap", "Damage cap"]];
/// An item's tags as chips, for the card that carries them.
const wfTagChips = (tags) => (tags || []).map((t) => {
  const c = WF_CAPS.find(([id]) => id === t.tag);
  return ` <span class="wf-cap ${t.tag}" title="${escHtml(t.when)}">${escHtml(tr(c ? c[1] : t.tag))}</span>`;
}).join("");
const SHARD_HUE = { crimson: "#d64545", azure: "#3d8bfd", amber: "#e8a33d",
  violet: "#9b59d0", emerald: "#2fb36d", topaz: "#e07b2a" };
let WFCAT = null;
let wf = null;
let wfActive = "";
let wfWired = false;

const wfFrames = () => (META && META.warframes) || [];
const wfFrame = (id) => (WFCAT && WFCAT.frames.find((f) => f.id === id)) || null;
const wfMod = (id) => (WFCAT && id && WFCAT.mods.find((m) => m.id === id)) || null;
const wfArcane = (id) => (WFCAT && id && WFCAT.arcanes.find((a) => a.id === id)) || null;
const wfAbility = (id) => (WFCAT && id && WFCAT.abilities.find((a) => a.id === id)) || null;
const warframePath = (f) => "/warframes/" + String(f.name_en || f.name).replace(/ /g, "_");
const frameName = (id) => (id === "helminth" ? "Helminth"
  : ((META.frames || []).find((f) => f.id === id) || {}).name || id);

async function loadWarframeCatalog() {
  if (WFCAT) return WFCAT;
  const c = await api("/api/warframe/catalog", {});
  const over = (x, table) => { x.name_en = x.name; x.name = LN(table, x.id, x.name); };
  c.mods.forEach((m) => over(m, I18N && (I18N.auras || {})[m.id] ? "auras" : "warframe_mods"));
  c.arcanes.forEach((a) => over(a, "warframe_arcanes"));
  c.abilities.forEach((a) => over(a, "warframe_abilities"));
  c.artifact_mods.forEach((m) => over(m, "artifact_mods"));
  c.artifact_arcanes.forEach((a) => over(a, "artifact_arcanes"));
  WFCAT = c;
  return c;
}

// ---- the state ----
function wfBlank(id) {
  const f = wfFrame(id);
  const slots = Array.from({ length: 10 }, () => ({ mod: null, pol: null, rank: null }));
  (f.polarities || []).forEach((p, i) => { slots[i].pol = p; });
  slots[WF_EXILUS].pol = f.exilus_polarity || null;
  slots[WF_AURA].pol = f.aura_polarity || null;
  return { frame: id, slots, arcanes: [{ id: null, rank: null }, { id: null, rank: null }],
    shards: [null, null, null, null, null], helminth: { slot: 0, ability: null }, operator: null };
}

/// A stored build, repaired against today's catalogue: an id that is gone
/// becomes an empty slot rather than a card nothing can draw.
function wfNormalize(st, id) {
  const b = wfBlank(id);
  const s = st || {};
  return {
    frame: id,
    slots: b.slots.map((x, i) => {
      const y = (s.slots || [])[i];
      return y ? { mod: wfMod(y.mod) ? y.mod : null, pol: y.pol ?? null, rank: y.rank ?? null } : x;
    }),
    arcanes: [0, 1].map((i) => {
      const y = (s.arcanes || [])[i];
      return y && wfArcane(y.id) ? { id: y.id, rank: y.rank ?? null } : { id: null, rank: null };
    }),
    shards: [0, 1, 2, 3, 4].map((i) => {
      const y = (s.shards || [])[i];
      const d = y && SHARDS().find((x) => x.id === y.shard);
      return d && d.options.some((o) => o.id === y.effect)
        ? { shard: y.shard, effect: y.effect, tauforged: !!y.tauforged } : null;
    }),
    helminth: { slot: Number((s.helminth || {}).slot) || 0,
      ability: wfAbility((s.helminth || {}).ability) ? s.helminth.ability : null },
    // THE LINKED OPERATOR BUILD'S `id`, resolved when the build is sent. A NAME
    // here is a link saved before ids, and becomes that build's id.
    operator: typeof s.operator === "string" && s.operator
      ? ((opList().find((p) => p.id === s.operator || p.name === s.operator) || {}).id || null) : null,
  };
}

/// The four abilities as the loadout carries them, the infused one in its slot.
const wfLoadout = () => wfFrame(wf.frame).abilities.map((id, i) =>
  wfAbility(wf.helminth.slot === i + 1 && wf.helminth.ability ? wf.helminth.ability : id));

const wfPayload = () => wfPayloadOf(wf);

/// A Warframe build's state as the Warframe module reads it — this page's, or the
/// one a weapon's wielder links to, which is a stored state read raw: the server
/// refuses whatever it cannot seat.
function wfPayloadOf(s) {
  const pick = (x) => (x && x.mod ? { id: x.mod, rank: x.rank } : null);
  const slots = s.slots || [];
  const h = s.helminth || {};
  return {
    frame: s.frame,
    mods: slots.slice(0, 8).map(pick).filter(Boolean),
    exilus: pick(slots[WF_EXILUS]),
    aura: pick(slots[WF_AURA]),
    arcanes: (s.arcanes || []).filter((a) => a && a.id),
    shards: (s.shards || []).filter(Boolean),
    helminth: h.slot && h.ability ? { slot: h.slot, ability: h.ability } : null,
    operator: operatorPickFor(s.operator),
  };
}

// ---- capacity and Forma ----
const wfDrainAt = (m, rank) =>
  m.drain - m.max_rank + (rank == null ? m.max_rank : Math.max(0, Math.min(m.max_rank, rank)));

/// What the aura hands back: twice its drain on its own polarity, 80% rounded
/// down on another (W`Aura`). Mirrors `data::warframes::aura_capacity`.
function wfAuraGrant() {
  const s = wf.slots[WF_AURA];
  const m = wfMod(s.mod);
  if (!m) return 0;
  const g = wfDrainAt(m, s.rank);
  if (!s.pol) return g;
  return s.pol === m.polarity || s.pol === "Omni" ? g * 2 : Math.floor(g * 0.8);
}
const wfCapacity = () => WF_BASE_CAPACITY / (formaRules().catalyst ? 1 : 2) + wfAuraGrant();
function wfUsed() {
  let n = 0;
  for (let i = 0; i <= WF_EXILUS; i++) {
    const s = wf.slots[i];
    const m = wfMod(s.mod);
    if (m) n += slotDrain(wfDrainAt(m, s.rank), m.polarity, s.pol);
  }
  return n;
}
const wfInnate = () => {
  const f = wfFrame(wf.frame);
  return [...(f.polarities || []), f.exilus_polarity, f.aura_polarity].filter(Boolean);
};

/// Forma owed: the innate colours are one pool across all ten slots, so moving
/// one is free and each colour added or removed beyond the pool is one Forma.
function wfFormaCount() {
  const need = {}, pool = {};
  let umbra = 0, omni = 0;
  wf.slots.forEach((s) => {
    if (!s.pol) return;
    if (s.pol === "Omni") omni++;
    else if (s.pol === "Umbra") umbra++;
    else need[s.pol] = (need[s.pol] || 0) + 1;
  });
  wfInnate().forEach((p) => { pool[p] = (pool[p] || 0) + 1; });
  let added = 0, removed = 0;
  for (const p of new Set([...Object.keys(need), ...Object.keys(pool)])) {
    const d = (need[p] || 0) - (pool[p] || 0);
    if (d > 0) added += d; else removed -= d;
  }
  return { regular: Math.max(added, removed), umbra, omni };
}

/// A Warframe build as the planner reads it: the aura is the slot that grants.
function wfFormaLoadout(st) {
  const card = (s) => {
    const m = s && s.mod ? wfMod(s.mod) : null;
    return m ? { drain: wfDrainAt(m, s.rank), polarity: m.polarity } : null;
  };
  return { main: st.slots.slice(0, 8).map(card), exilus: card(st.slots[WF_EXILUS]), grant: card(st.slots[WF_AURA]) };
}
const wfItem = () => `warframe-${wf.frame}`;
const wfActiveBuild = () => wfBarCfg().load().find((p) => p.name === wfActive) || null;
function wfFormaPartners() {
  const want = new Set(formaGroup(wfItem()));
  const open = wfActiveBuild();
  return wfBarCfg().load().filter((p) => p.id !== (open || {}).id && want.has(p.id));
}

/// The Warframe page's plan — `autoForma`'s, for a frame.
async function wfAutoForma() {
  const live = wf;
  const sig = () => JSON.stringify(live.slots.map((s) => [s.mod, s.rank]));
  const before = sig();
  const partners = wfFormaPartners();
  const names = [wfActive || tr("this build"), ...partners.map(presetLabel)];
  const r = await api("/api/forma/plan", {
    warframe: live.frame, rules: formaRules(),
    loadouts: [wfFormaLoadout(live), ...partners.map((p) => wfFormaLoadout(wfNormalize(p.state, live.frame)))],
  });
  if (wf !== live || sig() !== before) return null;
  formaNotes.warframe = { r, names, at: `${live.frame}\u0000${wfActive}` };
  if (!r || !r.ok || !r.fits) return formaRefused(r, live.slots, "wf-forma-plan");
  placeFormaPlan(live.slots, r, 0);
  if (partners.length) {
    const cfg = wfBarCfg();
    const ps = cfg.load();
    partners.forEach((p, k) => {
      const q = ps.find((x) => x.id === p.id);
      if (!q) return;
      const st = wfNormalize(q.state, live.frame);
      placeFormaPlan(st.slots, r, k + 1);
      q.state = st;
    });
    cfg.store(ps);
  }
  return r;
}

function renderWfFormaPlan() {
  const open = wfActiveBuild();
  renderFormaPlan($("wf-forma-plan"), {
    note: "warframe",
    at: `${wf.frame}\u0000${wfActive}`,
    item: wfItem(),
    activeLabel: wfActive,
    partners: wfBarCfg().load().filter((p) => p.id !== (open || {}).id)
      .map((p) => ({ id: p.id, name: presetLabel(p) })),
    catalystLabel: "Orokin Reactor",
    grantLabel: "Aura slot first",
    plan: async () => { await wfAutoForma(); wfChanged(); },
    changed: () => wfChanged(),
  });
}

// ---- drawing ----
/// DE'S OWN CARD in the display language when there is one, else ours.
const wfLines = (o, r) => {
  const zh = I18N && ((I18N.warframe_mod_descriptions || {})[o.id]
    || (I18N.warframe_arcane_descriptions || {})[o.id]);
  if (zh && zh.length) return String(zh[Math.max(0, Math.min(zh.length - 1, r))]).split("\n").filter(Boolean);
  const all = o.desc_ranks || [o.description || ""];
  return String(all[Math.max(0, Math.min(all.length - 1, r))] || "").split("\n").filter(Boolean).map(tf);
};
const wfNum = (v) => (Number.isFinite(v) ? String(Math.round(v * 100) / 100) : "∞");
const wfPct = (v) => `${Math.round(v * 1000) / 10}%`;
const wfSigned = (v) => { const r = Math.round(v * 10) / 10; return (r >= 0 ? "+" : "") + r; };
const wfUnit = (v, unit) => (unit === "pct" ? wfPct(v) : unit === "m" ? `${wfNum(v)} m`
  : unit === "seconds" ? `${wfNum(v)} s` : unit === "multiplier" ? `${wfNum(v)}x` : wfNum(v));
const wfRank = (r, max) => (max > 0
  ? `<span class="rank ${r < max ? "lowered" : ""}"><button class="rk" data-d="-1">−</button><b>R${r}${r < max ? "/" + max : ""}</b><button class="rk" data-d="1">+</button></span>`
  : "");

function wfSlotEl(i) {
  const s = wf.slots[i];
  const m = wfMod(s.mod);
  const el = document.createElement("div");
  if (m) {
    el.className = "slot filled" + (m.rarity ? " rar-" + m.rarity : "");
    const r = s.rank == null ? m.max_rank : s.rank;
    const base = wfDrainAt(m, r);
    const eff = i === WF_AURA ? wfAuraGrant() : slotDrain(base, m.polarity, s.pol);
    const matched = s.pol === m.polarity || (s.pol === "Omni" && m.polarity !== "Umbra");
    const fit = !s.pol ? "" : matched ? " matched" : " mismatched";
    const cost = i === WF_AURA ? `+${eff} ${escHtml(tr("capacity"))}`
      : `${eff} drain${eff !== base ? ` (base ${base})` : ""}`;
    el.innerHTML = polBtn(s.pol, i) + imgTag(IMG(m.image), "mod")
      + `<div class="info"><div class="mn">${wl(m.name, wikiUrl(m.name_en || m.name))}${wfTagChips(m.tags)}</div>`
      + `<div class="me">${wfLines(m, r).map((x) => `<div>${escHtml(x)}</div>`).join("")}</div>`
      + `<div class="drow"><div class="dr${fit}"><span class="mpol">${polGlyph(m.polarity)}</span>${cost}</div>${wfRank(r, m.max_rank)}</div></div>`
      + `<button class="dots" title="options">⋯</button>`;
    el.querySelector(".dots").addEventListener("click", (e) => {
      e.stopPropagation();
      openSlotMenu(e.currentTarget, null, {
        label: tr(i === WF_AURA ? "Aura" : "Mod"), removable: true,
        onSwap: () => openWfPicker("mod", i, el),
        onPick: () => { s.mod = null; s.rank = null; wfChanged(); },
      });
    });
    el.querySelectorAll(".rk").forEach((b) => b.addEventListener("click", (e) => {
      e.stopPropagation();
      s.rank = Math.max(0, Math.min(m.max_rank, r + Number(b.dataset.d)));
      wfChanged();
    }));
  } else {
    el.className = "slot empty";
    const plus = i === WF_AURA ? "+ add aura" : i === WF_EXILUS ? "+ add exilus mod" : "+ add mod";
    el.innerHTML = polBtn(s.pol, i) + `<span class="plus">${escHtml(tr(plus))}</span>`;
    el.addEventListener("click", (e) => { e.stopPropagation(); openWfPicker("mod", i, el); });
  }
  if (i < WF_EXILUS) {
    const no = document.createElement("span");
    no.className = "slotno";
    no.textContent = String(i + 1);
    el.appendChild(no);
  }
  el.querySelector(".pol-btn").addEventListener("click", (e) => { e.stopPropagation(); openWfPolMenu(i, el); });
  return el;
}

function renderWfMods() {
  const used = wfUsed(), cap = wfCapacity();
  $("wf-capacity").textContent = `${used} / ${cap}`;
  $("wf-capacity").classList.toggle("over", used > cap);
  const f = wfFormaCount();
  $("wf-forma").textContent = [`${f.regular} Forma`, f.umbra ? `${f.umbra} Umbra` : null, f.omni ? `${f.omni} Omni` : null]
    .filter(Boolean).join(" · ");
  const box = $("wf-mod-slots");
  box.innerHTML = "";
  for (let i = 0; i < 8; i++) box.appendChild(wfSlotEl(i));
  $("wf-exilus").innerHTML = "";
  $("wf-exilus").appendChild(wfSlotEl(WF_EXILUS));
  $("wf-aura").innerHTML = "";
  $("wf-aura").appendChild(wfSlotEl(WF_AURA));
  renderWfFormaPlan();
}

function openWfPolMenu(i, anchor) {
  closePopovers();
  const menu = $("slot-menu");
  const cur = wf.slots[i].pol;
  menu.innerHTML = WF_POLS.map((p) => `<div class="mi ${p === cur ? "sel" : ""}" data-p="${p}">${polGlyph(p)} ${p === "Omni" ? "Omni (any)" : p}</div>`).join("")
    + `<div class="mi ${!cur ? "sel" : ""}" data-p="">◇ none</div>`;
  place(menu, anchor);
  menu.querySelectorAll(".mi").forEach((o) => o.addEventListener("click", () => {
    wf.slots[i].pol = o.dataset.p || null;
    closePopovers();
    wfChanged();
  }));
}

function openWfPicker(kind, idx, anchor) {
  closePopovers();
  const pop = $("wf-popover");
  place(pop, anchor);
  const s = $("wf-search");
  s.value = "";
  s.oninput = () => renderWfMenu(kind, idx, s.value);
  renderWfMenu(kind, idx, "");
  s.focus();
}

function renderWfMenu(kind, idx, query) {
  const q = query.trim().toLowerCase();
  const menu = $("wf-menu");
  const none = `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
  if (kind === "arcane") {
    const cur = wf.arcanes[idx].id, other = wf.arcanes[1 - idx].id;
    const hits = WFCAT.arcanes.filter((a) => searchHit(a, q))
      .sort((a, b) => (b.id === cur) - (a.id === cur) || a.name.localeCompare(b.name));
    menu.innerHTML = hits.length ? hits.map((a) => `<div class="opt ${a.id === cur ? "cur" : ""} ${a.id === other ? "dis" : ""} rar-${a.rarity}" data-id="${a.id}">
      ${imgTag(IMG(a.image), "mod")}<div class="info"><div class="mn">${wl(a.name, wikiUrl(a.name_en || a.name))}</div>${effLines(wfLines(a, a.max_rank).map(escHtml))}</div></div>`).join("") : none;
    menu.querySelectorAll(".opt[data-id]:not(.dis)").forEach((o) => o.addEventListener("click", () => {
      wf.arcanes[idx] = { id: o.dataset.id, rank: null };
      closePopovers();
      wfChanged();
    }));
    return;
  }
  const carries = wfLoadout().map((a) => a && a.id);
  const fits = (m, i) => (i === WF_AURA ? m.aura : i === WF_EXILUS ? m.exilus : !m.aura);
  const at = (id) => wf.slots.findIndex((s, i) => i !== idx && s.mod === id);
  const own = wfMod(wf.slots[idx].mod);
  const hits = WFCAT.mods.filter((m) => fits(m, idx) && searchHit(m, q))
    .sort((a, b) => (b.id === (own && own.id)) - (a.id === (own && own.id)) || a.name.localeCompare(b.name));
  menu.innerHTML = hits.length ? hits.map((m) => {
    const placed = at(m.id);
    const family = placed < 0 && m.family && wf.slots.some((s, i) => i !== idx && (wfMod(s.mod) || {}).family === m.family);
    const orphan = m.augments && !carries.includes(m.augments);
    const noSwap = placed >= 0 && own && !fits(own, placed);
    const title = family ? tr("incompatible with a card already seated")
      : noSwap ? tr("cannot swap: the card in this slot does not fit that one")
      : orphan ? tr("augments an ability this loadout does not carry — it would pay nothing") : "";
    const chip = placed >= 0 ? ` <span class="slotchip">${escHtml(placed === WF_AURA ? tr("aura") : placed === WF_EXILUS ? tr("exilus") : tr("slot") + " " + (placed + 1))}</span>` : "";
    return modRow(m, {
      cls: `${family || noSwap ? "dis" : ""} ${own && own.id === m.id ? "cur" : placed >= 0 ? "placed" : ""}`,
      attrs: `data-id="${m.id}"`, title: escHtml(title), exilusChip: idx !== WF_EXILUS,
      chips: chip + (orphan ? ` <span class="exchip unmod">${escHtml(tr("inert here"))}</span>` : ""),
      trailing: `<span class="dr">${m.drain}</span>`,
    });
  }).join("") : none;
  menu.querySelectorAll(".opt[data-id]:not(.dis)").forEach((o) => o.addEventListener("click", () => {
    const id = o.dataset.id;
    const here = wf.slots[idx];
    const from = at(id);
    if (here.mod === id) { closePopovers(); return; }
    if (from >= 0) {
      const there = wf.slots[from];
      [here.mod, there.mod] = [there.mod, here.mod];
      [here.rank, there.rank] = [there.rank, here.rank];
    } else {
      here.mod = id;
      here.rank = null;
    }
    closePopovers();
    wfChanged();
  }));
}

function wfArcaneEl(i) {
  const p = wf.arcanes[i];
  const a = wfArcane(p.id);
  const el = document.createElement("div");
  if (!a) {
    el.className = "slot empty arc";
    el.innerHTML = `<span class="plus">+ ${escHtml(tr("add arcane"))}</span>`;
    el.addEventListener("click", (e) => { e.stopPropagation(); openWfPicker("arcane", i, el); });
    return el;
  }
  const r = p.rank == null ? a.max_rank : p.rank;
  el.className = "slot filled arc" + (a.rarity ? " rar-" + a.rarity : "");
  el.innerHTML = imgTag(IMG(a.image), "mod")
    + `<div class="info"><div class="mn">${wl(a.name, wikiUrl(a.name_en || a.name))}${wfTagChips(a.tags)}</div>${effLines(wfLines(a, r).map(escHtml))}${wfRank(r, a.max_rank)}</div>`
    + `<button class="dots" title="options">⋯</button>`;
  el.querySelector(".dots").addEventListener("click", (e) => {
    e.stopPropagation();
    openSlotMenu(e.currentTarget, null, {
      label: tr("Arcane"), removable: true,
      onSwap: () => openWfPicker("arcane", i, el),
      onPick: () => { wf.arcanes[i] = { id: null, rank: null }; wfChanged(); },
    });
  });
  el.querySelectorAll(".rk").forEach((b) => b.addEventListener("click", (e) => {
    e.stopPropagation();
    p.rank = Math.max(0, Math.min(a.max_rank, r + Number(b.dataset.d)));
    wfChanged();
  }));
  return el;
}

function renderWfArcanes() {
  const box = $("wf-arcane-slots");
  box.innerHTML = "";
  [0, 1].forEach((i) => box.appendChild(wfArcaneEl(i)));
}

const wfShardLine = (d, o, tau) => {
  const v = tau ? o.tauforged : o.value;
  const n = o.unit === "pct" ? `+${Math.round(v * 1000) / 10}%` : `+${v}`;
  return `${n} ${(I18N && (I18N.shards || {})[`${d.id}/${o.id}`]) || o.text}`;
};

function renderWfShards() {
  const box = $("wf-shards");
  box.innerHTML = wf.shards.map((p, i) => {
    const d = p ? SHARDS().find((x) => x.id === p.shard) : null;
    const colour = ddButton(`dd-wf-shard-${i}`, {
      value: p ? p.shard : "",
      items: [{ value: "", label: tr("empty socket") },
        ...SHARDS().map((s) => ({ value: s.id, label: LN("shards", s.id, s.name) }))],
      onPick: (v) => {
        const s = SHARDS().find((x) => x.id === v);
        wf.shards[i] = s ? { shard: v, effect: s.options[0].id, tauforged: !!(p && p.tauforged) } : null;
        wfChanged();
      },
    });
    const effect = d ? ddButton(`dd-wf-shard-effect-${i}`, {
      value: p.effect,
      items: d.options.map((o) => ({ value: o.id, label: wfShardLine(d, o, p.tauforged) })),
      onPick: (v) => { wf.shards[i].effect = v; wfChanged(); },
    }) : "";
    const tau = d ? `<label class="wf-tau"><input type="checkbox" data-wf-tau="${i}"${p.tauforged ? " checked" : ""}> ${escHtml(tr("Tauforged"))}</label>` : "";
    return `<div class="wf-shard"><span class="wf-swatch" style="background:${d ? SHARD_HUE[d.colour] || "var(--line)" : "transparent"}"></span>${colour}${effect}${tau}</div>`;
  }).join("");
  box.querySelectorAll("[data-wf-tau]").forEach((c) => c.addEventListener("change", () => {
    wf.shards[Number(c.dataset.wfTau)].tauforged = c.checked;
    wfChanged();
  }));
}

function renderWfHelminth() {
  const f = wfFrame(wf.frame);
  // NEVER HER OWN: a frame offered its own ability would carry two copies of it.
  const pool = WFCAT.abilities.filter((a) => a.subsumable && !f.abilities.includes(a.id));
  const items = pool.map((a) => ({ value: a.id, label: a.name, group: frameName(a.frame),
    hint: `${a.energy_cost} ${tr("energy")}` }))
    .sort((a, b) => a.group.localeCompare(b.group) || a.label.localeCompare(b.label));
  $("wf-helminth").innerHTML = `<span class="wf-hl">${escHtml(tr("Helminth"))}</span>`
    + ddButton("dd-wf-helminth-slot", {
      value: String(wf.helminth.slot || 0),
      items: [{ value: "0", label: tr("no infusion") },
        ...f.abilities.map((id, i) => ({ value: String(i + 1), label: `${i + 1} · ${(wfAbility(id) || {}).name || id}` }))],
      onPick: (v) => {
        wf.helminth.slot = Number(v);
        if (!wf.helminth.slot) wf.helminth.ability = null;
        wfChanged();
      },
    })
    + ddButton("dd-wf-helminth-ability", {
      value: wf.helminth.ability || "", placeholder: tr("pick an ability"), items, search: true,
      disabled: !wf.helminth.slot,
      onPick: (v) => { wf.helminth.ability = v; wfChanged(); },
    });
}

const wfArrow = (a, b) => (a != null && Math.abs(a - b) > 1e-9
  ? `<span class="sbase">${wfNum(a)}</span> → <b>${wfNum(b)}</b>` : `<b>${wfNum(b)}</b>`);

function renderWfAbilities(r) {
  $("wf-abilities").innerHTML = (r.abilities || []).map((x) => {
    const a = wfAbility(x.id) || { name: x.id, description: "" };
    const cost = x.cost_type ? "" : `<span class="wf-cost" title="${escHtml(tr("energy cost"))}">⚡ ${wfArrow(x.base_energy_cost, x.energy_cost)}</span>`;
    const drain = x.drain_per_second != null
      ? `<span class="wf-cost" title="${escHtml(tr("energy drained per second"))}">⚡/s ${wfArrow(x.base_drain_per_second, x.drain_per_second)}</span>` : "";
    const rows = (x.lines || []).map((l) => `<div class="row"><span class="k">${escHtml(tr(l.label))}${
      WF_SCALE_TAG[l.scales_with] ? ` <span class="wf-scale">${WF_SCALE_TAG[l.scales_with]}</span>` : ""}</span><span class="v">${
      Math.abs(l.value - l.base) > 1e-9 ? `<span class="sbase">${wfUnit(l.base, l.unit)}</span> → ` : ""}${wfUnit(l.value, l.unit)}</span></div>`).join("")
      + (x.derived || []).map((d) => `<div class="row wf-der"><span class="k">${escHtml(tr(d.label))}</span><span class="v">${Math.round(d.value)}</span></div>`).join("");
    return `<div class="wf-ab${x.helminth ? " infused" : ""}">
      <div class="wf-ab-h"><span class="wf-key">${x.slot}</span>${imgTag(IMG(a.icon), "wf-ab-icon")}
        <div class="info"><div class="mn">${wl(a.name, wikiUrl(a.name_en || a.name))}${x.helminth ? ` <span class="exchip">${escHtml(tr("Helminth"))}</span>` : ""}${wfTagChips(a.tags)}</div>
        <div class="wf-costs">${cost}${drain}</div></div></div>
      <div class="wf-ab-desc">${escHtml((I18N && (I18N.warframe_ability_descriptions || {})[x.id]) || a.description || "")}</div>
      ${(x.infused_notes || []).map((n) => `<div class="srownote">⚙ ${escHtml(n)}</div>`).join("")}
      ${rows ? `<div class="stat-table wf-ab-stats">${rows}</div>`
        : `<div class="wf-ab-none">${escHtml(tr("this ability's numbers are not transcribed yet"))}</div>`}
    </div>`;
  }).join("");
}

function wfSourceName(from) {
  if (String(from).startsWith("focus:")) {
    const [, sid, nid] = String(from).split(":");
    const s = focusSchool(sid);
    const n = s && s.nodes.find((x) => x.id === nid);
    return `${s ? s.name : sid} · ${n ? n.name : nid}`;
  }
  const [a, b] = String(from).split("/");
  if (b) {
    const d = SHARDS().find((x) => x.id === a);
    const o = d && d.options.find((x) => x.id === b);
    return `${LN("shards", a, d ? d.name : a)} · ${(I18N && (I18N.shards || {})[from]) || (o ? o.text : b)}`;
  }
  if (from === "shield_gate") return tr("Shield gate");
  const f = wfFrame(from);
  if (f) return `${f.name} · ${tr("Passive")}`;
  const x = wfMod(from) || wfArcane(from) || wfAbility(from);
  return x ? x.name : from;
}

const WF_ADMIT = {
  unmodelled: ["⊘", "not computed by this panel yet"],
  out_of_scope: ["◇", "cannot change a number in a Warframe's own panel"],
  inert: ["⚠", "seated, and paying nothing in this build"],
};

function renderWfStats(r) {
  const fmt = (l, v) => (l.ratio ? wfPct(v) : l.id === "sprint_speed" ? wfNum(v) : String(Math.round(v)));
  $("wf-stats").innerHTML = (r.refused || []).map((x) => `<div class="error">${escHtml(x)}</div>`).join("")
    + (r.stats || []).map((l) => `<div class="srow"><div class="shead"><span class="sk">${escHtml(tr(l.label))}</span><span class="sv">${
      Math.abs(l.value - l.base) > 1e-9 ? `<span class="sbase">${fmt(l, l.base)}</span> → ` : ""}<b>${fmt(l, l.value)}</b></span></div>${
      (l.sources || []).map((c) => `<div class="ssrc">${c.times ? "×" + wfNum(c.value) : c.flat ? wfSigned(c.value) : wfSigned(c.value * 100) + "%"} — ${escHtml(wfSourceName(c.from))}</div>`).join("")}</div>`).join("");
  const by = new Map();
  (r.admissions || []).forEach((x) => { if (!by.has(x.from)) by.set(x.from, []); by.get(x.from).push(x); });
  $("wf-admissions").innerHTML = by.size
    ? `<div class="sdmg-title">${escHtml(tr("What this panel does not compute"))}</div>` + [...by].map(([from, list]) =>
      `<div class="scond"><b>${escHtml(wfSourceName(from))}</b>: ${list.map((x) => `<span class="wf-adm ${x.kind}" title="${
        escHtml(tr(WF_ADMIT[x.kind][1]))}">${WF_ADMIT[x.kind][0]} ${escHtml(tr(x.text))}</span>`).join(" ")}</div>`).join("")
    : "";
}

let wfPanelTimer = null;
let wfPanelGen = 0;
function refreshWfPanel() {
  clearTimeout(wfPanelTimer);
  wfPanelTimer = setTimeout(async () => {
    const gen = ++wfPanelGen;
    let r = null;
    try { r = await api("/api/warframe/panel", wfPayload()); } catch (e) { r = { ok: false, error: String(e) }; }
    if (gen !== wfPanelGen) return;
    if (!r || r.ok === false) {
      $("wf-stats").innerHTML = `<div class="error">${escHtml((r && r.error) || "no data")}</div>`;
      return;
    }
    renderWfStats(r);
    renderWfAbilities(r);
    renderWfCaps(r);
  }, 120);
}

/// THE THREE TAGS, each with every source the build carries for it. A tag
/// nobody grants is drawn dimmed rather than left out: "no invulnerability" is
/// part of what a build says.
function renderWfCaps(r) {
  const by = (id) => (r.tags || []).filter((t) => t.tag === id);
  $("wf-caps").innerHTML = WF_CAPS.map(([id, label]) => {
    const src = by(id);
    return `<div class="wf-capbox ${id}${src.length ? "" : " off"}"><div class="wf-caph">${escHtml(tr(label))}${
      src.length ? "" : ` <span class="wf-capsrc">${escHtml(tr("none in this build"))}</span>`}</div>${
      src.map((t) => `<div class="wf-capsrc">${escHtml(wfSourceName(t.from))} — ${escHtml(t.when)}</div>`).join("")}</div>`;
  }).join("");
  renderWfGate(r.shield_gate);
}

/// THE SHIELD GATE: how long it lasts on this build, and what each cast refills.
/// Under Catalyzing Shields any refill gives the fixed gate (MEASUREMENTS M92).
function renderWfGate(g) {
  const box = $("wf-gate");
  if (!g || g.max_shields <= 0) {
    box.innerHTML = `<div class="wf-caph">${escHtml(tr("Shield gate"))}</div><div class="wf-capsrc">${escHtml(tr("no shields, so no shield gate"))}</div>`;
    return;
  }
  const fixed = g.fixed_by ? ` — ${escHtml(tr("fixed by"))} ${escHtml(wfSourceName(g.fixed_by))}` : ` — ${escHtml(tr("scales with the shields held when they break"))}`;
  const srcs = (g.sources || []).map((s) => `${escHtml(s.from === "augur" ? tr("Augur set") : wfSourceName(s.from))} ${Math.round(s.value * 100)}%`).join(" + ");
  const rows = (g.casts || []).map((c) => {
    const a = wfAbility(c.ability);
    // FIXED BY A CARD, the refill's size does not matter; otherwise it sets the length.
    const verdict = g.fixed_by || c.full
      ? `<span class="ok">${escHtml(tr(c.full ? "full refill" : "partial refill"))} · ${wfNum(c.seconds)} s</span>`
      : `<span class="warn">${escHtml(tr("partial refill"))} · ${wfNum(c.seconds)} s</span>`;
    return `<tr><td>${c.slot} · ${escHtml(a ? a.name : c.ability)}</td><td>${wfNum(c.energy)}</td><td>${wfNum(c.shields)} / ${wfNum(g.max_shields)}</td><td>${verdict}</td></tr>`;
  }).join("");
  box.innerHTML = `<div class="wf-caph">${escHtml(tr("Shield gate"))}</div>`
    + `<div class="wf-capsrc">${escHtml(tr("After a full break"))}: <b>${wfNum(g.full_seconds)} s</b>${fixed}</div>`
    + (g.energy_to_shield > 0
      ? `<div class="wf-capsrc">${escHtml(tr("Casting converts energy to shields"))}: ${srcs}</div>`
        + `<table><tr><th>${escHtml(tr("Ability"))}</th><th>${escHtml(tr("energy"))}</th><th>${escHtml(tr("shields"))}</th><th>${escHtml(tr("Shield gate"))}</th></tr>${rows}</table>`
      : `<div class="wf-capsrc">${escHtml(tr("nothing re-opens it on cast — the Augur set or Brief Respite would"))}</div>`);
}

// ---- builds ----
/// FRAMED BY A WEAPON PAGE, WHICH BUILD IS OPEN IS THE WEAPON'S LINK: told to the
/// parent whenever it changes, born or picked, so the wielder follows the page.
function wfAnnounceBuild() {
  if (!EMBED || window.parent === window) return;
  const p = presetListWithIds(WF_BUILDS, wf.frame).find((x) => x.name === wfActive);
  window.parent.postMessage({ wfsim: "wielder-build", frame: wf.frame, id: p ? p.id : PRESET_SEED_ID }, location.origin);
}

function wfBarCfg() {
  return {
    domain: WF_BUILDS,
    label: tr("Builds"),
    noun: BUILD_NOUN,
    // AN `id` ON EVERY BUILD, because a weapon's wielder links to one by it.
    load: () => presetListWithIds(WF_BUILDS, wf.frame),
    store: (ps) => storePresetList(WF_BUILDS, opWithIds(ps), wf.frame),
    active: () => wfActive,
    setActive: (n) => {
      wfActive = n;
      // FRAMED, THE OPEN BUILD IS A WEAPON'S LINK, not this frame's own last
      // choice: the pointer is the standalone page's and the wielder picker's
      // default, and another weapon's pane must not move it.
      if (EMBED) wfAnnounceBuild();
      else localStorage.setItem(presetActiveKey(WF_BUILDS, wf.frame), n);
    },
    snapshot: () => JSON.parse(JSON.stringify(wf)),
    apply: (st) => wfApply(st),
    blank: () => wfBlank(wf.frame),
    rerender: renderWfPresetBar,
  };
}
const renderWfPresetBar = () => renderPresetBarIn($("preset-bar-warframes"), wfBarCfg());
function wfApply(st) {
  wf = wfNormalize(st, wf.frame);
  clearTimeout(wfSaveTimer);
  renderWarframe();
}

/// A BUILD IS BORN ON THE FIRST EDIT, as a weapon build is (`markPresetDirty`).
let wfSaveTimer = null;
function wfMarkDirty() {
  if (presetApplying) return;
  clearTimeout(wfSaveTimer);
  wfSaveTimer = setTimeout(() => {
    if (presetApplying || !wf) return;
    const cfg = wfBarCfg();
    const ps = cfg.load();
    const at = ps.findIndex((p) => p.name === wfActive);
    if (at < 0) {
      if (sameState(wf, wfBlank(wf.frame))) return;
      const name = newPresetName(ps);
      ps.push({ name, savedAt: Date.now(), state: cfg.snapshot() });
      cfg.store(ps);
      cfg.setActive(name);
      renderWfPresetBar();
      return;
    }
    if (sameState(ps[at].state, wf)) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: cfg.snapshot() };
    cfg.store(ps);
  }, 400);
}

function wfChanged() {
  renderWfMods();
  renderWfArcanes();
  renderWfShards();
  renderWfOperator();
  renderWfHelminth();
  refreshWfPanel();
  wfMarkDirty();
}

