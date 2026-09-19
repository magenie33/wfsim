// ---- capacity and the bill for the layout on screen ----
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
//
// TOLD WHICH SLOTS, because two questions are asked of it: the panel counts
// every slot the weapon has, and the BOARD counts the eight it judges — a
// benchmark build drops the exilus one, so measuring a submission against all
// nine would ask about a slot the board never reads.
const drainOver = (ss) => ss.reduce(
  (sum, s) => { const m = modById(s.mod); return m ? sum + slotDrain(modDrain(m, s.rank), m.polarity, s.pol) : sum; }, 0);
function capacityUsed() {
  return drainOver(slots);
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
  slots.forEach((s, i) => {
    // THE STANCE SLOT IS NOT IN THE POOL. Its polarity cannot be moved to a mod
    // slot — it is a slot of its own — so it is billed on its own line below
    // rather than competing for the nine the weapon was born with.
    if (i === STANCE || !s.pol) return;
    if (s.pol === "Omni") omni++;
    else if (s.pol === "Umbra") umbra++;
    else need[s.pol] = (need[s.pol] || 0) + 1;
  });
  // THE STANCE SLOT'S COLOUR IS NOT IN THE POOL. A polarity is repositionable
  // between the nine slots the weapon is born with; the stance slot is its own,
  // and letting a mod slot claim its Vazarin for free would pay a Forma nobody
  // spent.
  innate.slice(0, 9).forEach((p) => { if (p && p !== "Omni" && p !== "Umbra") pool[p] = (pool[p] || 0) + 1; });
  let added = 0, removed = 0;
  for (const p of new Set([...Object.keys(need), ...Object.keys(pool)])) {
    const d = (need[p] || 0) - (pool[p] || 0);
    if (d > 0) added += d; else removed += -d;
  }
  // ...and an ADVERSARY weapon is billed its five whatever the slots say. The
  // rank-40 ceiling costs five polarizations and the 80 capacity this build
  // was planned against assumes them, so a build using three has still spent
  // five (engine: `cost.regular += spend - cost.total()`).
  // …AND REPOLARIZING THE STANCE SLOT COSTS ONE. "As with Aura slots, Stance
  // slots can be repolarized using Forma" (wiki), and what it buys is the
  // doubled grant rather than a smaller drain.
  const stancePol = (slots[STANCE] && slots[STANCE].pol) || null;
  // BLANKING IT COSTS ONE TOO. The stance slot is not in the pool, so a colour
  // the weapon was born with cannot be swapped away — removing it is a Forma
  // like any other change to that slot.
  const stanceForma = stancePol !== (stancePolOf($("weapon").value) || null) ? 1 : 0;
  const regular = Math.max(added, removed) + stanceForma;
  // …UNLESS THE PLAYER'S RULES SAY NOT TO spend the mastery Forma.
  const owed = formaRules().reach_max_rank ? formaMin($("weapon").value) : 0;
  const floor = Math.max(0, owed - regular - umbra - omni);
  return { regular: regular + floor, umbra, omni };
}

// ---- the Forma plan: the player's rules, and the builds planned together ----
//
// THE PLAN IS THE ENGINE'S (`/api/forma/plan`, docs/INVESTMENT.md §The
// planner); the page only says what to plan and puts the answer in the slots.
// The RULES are the player's and GLOBAL — one set for every weapon and frame.
// Which builds share an item's polarities is the ITEM's, and the build being
// edited is always one of them, first, and never moved.
const FORMA_RULES_KEY = "wfsim-forma-rules";
const FORMA_RULES_DEFAULT = Object.freeze({
  catalyst: true, reach_max_rank: true, grant_slot_first: true, fixed_order: false,
  omni_forma: "never", umbra_forma: "when_needed", forma_limit: null,
});
/// The choices the planner offers for the two special Forma.
const FORMA_SPECIAL = { omni_forma: ["never", "allowed", "preferred"], umbra_forma: ["never", "when_needed", "allowed"] };
function formaRules() {
  let r = {};
  try { r = JSON.parse(localStorage.getItem(FORMA_RULES_KEY) || "{}") || {}; } catch (_) { r = {}; }
  const out = { ...FORMA_RULES_DEFAULT };
  for (const k of Object.keys(out)) if (k in r) out[k] = r[k];
  return out;
}
function storeFormaRules(r) {
  try { localStorage.setItem(FORMA_RULES_KEY, JSON.stringify(r)); } catch (_) { /* a private window keeps the defaults */ }
}
/// The Forma rules, patched. A limit is a whole number of Forma, or null for none.
function setFormaRules(patch) {
  const next = { ...formaRules(), ...patch };
  if ("forma_limit" in patch && patch.forma_limit !== null) next.forma_limit = Math.max(0, Math.floor(Number(patch.forma_limit)) || 0);
  storeFormaRules(next);
}
/// The OTHER builds planned with the open one, by preset id, per item.
const formaGroupKey = (item) => `wfsim-forma-group-${item}`;
function formaGroup(item) {
  try {
    const g = JSON.parse(localStorage.getItem(formaGroupKey(item)) || "[]");
    return Array.isArray(g) ? g : [];
  } catch (_) { return []; }
}
function storeFormaGroup(item, ids) {
  try { localStorage.setItem(formaGroupKey(item), JSON.stringify(ids)); } catch (_) { /* kept for this page only */ }
}
/// One build in or out of the set planned together with the open one.
function setFormaPartner(item, id, on) {
  const ids = new Set(formaGroup(item));
  if (on) ids.add(id); else ids.delete(id);
  storeFormaGroup(item, [...ids]);
}
/// What the last plan for each page said, drawn by `renderFormaPlan`.
const formaNotes = { builder: null, warframe: null };

/// Put loadout `k` of a plan onto a ten-slot array: the layout's colours, and
/// the loadout's mods where the plan placed them. Slot 8 is the exilus and
/// slot 9 the slot that grants capacity, on both pages.
function placeFormaPlan(ss, plan, k) {
  const at = plan.loadouts[k].slots;
  const main = ss.slice(0, 8).map((s) => ({ mod: s.mod, rank: s.rank }));
  for (let i = 0; i < 8; i++) {
    const src = at[i];
    ss[i].mod = src == null ? null : main[src].mod;
    ss[i].rank = src == null ? null : main[src].rank;
    ss[i].pol = plan.layout.main[i] || null;
  }
  ss[8].pol = plan.layout.exilus || null;
  ss[9].pol = plan.layout.grant || null;
}

/// A weapon build as the planner reads it. A stance hands back five on a bare
/// slot (`rules::capacity::STANCE_CAPACITY_GRANT`).
/// An ELEMENT-BEARING card is `ordered`: its place among the others is part of
/// the build, so the plan moves it only in order.
function weaponLoadout(ss, strict = false) {
  let unread = false;
  const card = (s) => {
    if (!s || !s.mod) return null;
    // A BOARD RIVEN that has not been taken yet: its shape is all there is.
    const rv = isRivenId(s.mod) && !modById(s.mod) ? boardRivenDefs[s.mod] : null;
    if (rv) {
      const st = boardRivenState(rv);
      return { drain: 2 + 2 * st.rank, polarity: polCap(st.polarity),
        ordered: (rv.bonuses || []).some((id) => (rivenStat(id) || {}).elemental) };
    }
    const m = modById(s.mod);
    if (!m) { unread = true; return null; }
    return { drain: modDrain(m, s.rank), polarity: m.polarity, ordered: !!m.elemental };
  };
  const st = ss[STANCE] && ss[STANCE].mod ? modById(ss[STANCE].mod) : null;
  const out = {
    main: ss.slice(0, 8).map(card),
    exilus: weaponAxes().hasExilus ? card(ss[EXILUS]) : null,
    grant: st ? { drain: 5, polarity: st.polarity } : null,
  };
  return strict && unread ? null : out;
}
/// A stored build's slots, repaired against this weapon's pool.
const storedSlots = (st) => Array.from({ length: 10 }, (_, i) => {
  const s = ((st && st.slots) || [])[i] || {};
  return { mod: s.mod && modById(s.mod) ? s.mod : null, pol: s.pol ?? null, rank: s.rank ?? null };
});
/// The saved builds planned with the open one. A board build is read-only and
/// planned alone, so it has none.
function weaponFormaPartners() {
  if (officialBuildActive()) return [];
  const want = new Set(formaGroup(presetWeapon()));
  return loadPresetList(BUILDS).filter((p) => presetId(p) !== activePreset && want.has(presetId(p)));
}

/// Plan the open build — with its partners unless `alone` — and write the
/// layout into every one of them. Resolves to the answer, or null when the
/// build changed while the engine was thinking and the answer is not for it.
/// `onto`: a layout already chosen (the optimizer's point) — the builds are
/// placed on it for no further Forma, and its own bill is what is shown.
async function autoForma({ alone = false, onto = null } = {}) {
  const w = $("weapon").value;
  const live = slots;
  const sig = () => JSON.stringify(live.map((s) => [s.mod, s.rank]));
  const before = sig();
  const partners = alone ? [] : weaponFormaPartners();
  const names = [presetLabel(buildNamed(activePreset)) || tr("this build"), ...partners.map(presetLabel)];
  const rules = formaRules();
  const r = await api("/api/forma/plan", {
    weapon: w,
    rules: onto ? { ...rules, forma_limit: 0 } : rules,
    start: onto ? { layout: onto.layout, forma_spent: onto.regular + onto.umbra + onto.omni, pinned: true } : null,
    loadouts: [weaponLoadout(live), ...partners.map((p) => weaponLoadout(storedSlots(p.state)))],
  });
  if (slots !== live || $("weapon").value !== w || sig() !== before) return null;
  if (r && r.ok && onto) {
    if (r.fits) Object.assign(r, { regular: onto.regular, umbra: onto.umbra, omni: onto.omni });
    else { r.reason = { kind: "layout_misfit" }; r.closest = null; }
  }
  formaNotes.builder = { r, names, at: `${w}\u0000${activePreset}` };
  if (!r || !r.ok || !r.fits) return formaRefused(r, live, "forma-plan");
  placeFormaPlan(live, r, 0);
  if (partners.length) {
    const ps = loadPresetList(BUILDS);
    partners.forEach((p, k) => {
      const q = ps.find((x) => presetId(x) === presetId(p));
      if (!q) return;
      const ss = storedSlots(q.state);
      placeFormaPlan(ss, r, k + 1);
      q.state = { ...q.state, slots: ss };
    });
    storePresetList(BUILDS, ps);
  }
  return r;
}

/// A REFUSAL IS SHOWN, not swallowed: the open build takes its nearest miss, so
/// the capacity line says how far over it is, and the box opens on the reason
/// for this render only — the reader's own fold choice is not rewritten.
function formaRefused(r, ss, boxId) {
  if (r && r.closest) placeFormaPlan(ss, r.closest, 0);
  const fold = $(boxId) && $(boxId).closest(".fold");
  if (fold) fold.classList.remove("shut");
  return r;
}

/// The same plan for a result that is not on screen: its own layout, alone.
async function planSlotsAlone(ss) {
  const r = await api("/api/forma/plan", { weapon: $("weapon").value, rules: formaRules(), loadouts: [weaponLoadout(ss)] });
  if (r && r.ok && r.fits) placeFormaPlan(ss, r, 0);
  return ss;
}

/// THE LIVE BUILD'S CAPACITY UNDER THE PLAYER'S RULES. `capOf` is the weapon at
/// its max rank with a Catalyst; without the mastery Forma the rank is what the
/// build spent — "max rank increases by 2 per Forma added" (docs/INVESTMENT.md).
function builderCap() {
  const r = formaRules();
  const w = weaponInfo($("weapon").value) || {};
  const max = w.max_rank || 30;
  const f = formaCount();
  const rank = r.reach_max_rank ? max : Math.min(max, 30 + 2 * (f.regular + f.umbra + f.omni));
  return rank * (r.catalyst ? 2 : 1) + stanceGrant();
}

/// A refusal in the page's own words, naming the builds it is about.
function formaRefusal(reason, names) {
  const who = (ids) => ids.map((i) => `「${names[i] || i + 1}」`).join(" ");
  switch (reason && reason.kind) {
    case "over_limit":
      return tr("needs {n} Forma, over your limit of {m}")
        .replace("{n}", reason.need).replace("{m}", reason.limit);
    case "does_not_fit":
      return tr("{who} does not fit even fully polarized").replace("{who}", who(reason.loadouts))
        + (reason.umbra_off ? " " + tr("— an Umbra mod pays full drain while Umbra Forma is off") : "");
    case "cannot_share":
      return tr("each of these builds fits on its own, but no one layout serves them all");
    case "layout_misfit":
      return tr("the ticked builds do not all fit that layout");
    default:
      return tr("no plan");
  }
}

/// THE FORMA BOX, one renderer for both pages. `ctx` names the page (`note`),
/// the item (`item`, the group's key), the open build's label, the builds it
/// may be planned with (`[{id, name}]`, or null when it is planned alone), and
/// what planning and a rule change do.
function renderFormaPlan(box, ctx) {
  if (!box) return;
  const r = formaRules();
  const group = new Set(formaGroup(ctx.item));
  const opt = (k, vals) => vals.map(([v, l]) =>
    `<option value="${v}"${r[k] === v ? " selected" : ""}>${escHtml(tr(l))}</option>`).join("");
  const tick = (k, label, hint) => `<label class="fp-tick" title="${escHtml(tr(hint))}">`
    + `<input type="checkbox" data-r="${k}"${r[k] ? " checked" : ""}> ${escHtml(tr(label))}</label>`;
  const partners = ctx.partners;
  const note = formaNotes[ctx.note] && formaNotes[ctx.note].at === ctx.at ? formaNotes[ctx.note] : null;
  let result = "";
  if (note && note.r && note.r.ok && note.r.fits) {
    const p = note.r;
    const bill = [`${p.regular} Forma`, p.umbra ? `${p.umbra} Umbra` : null, p.omni ? `${p.omni} Omni` : null]
      .filter(Boolean).join(" · ");
    const rows = p.loadouts.map((l, i) => `<tr><td>${escHtml(note.names[i] || "")}</td>`
      + `<td>${l.drain} / ${p.capacity + l.grant}</td><td>${l.spare}</td><td>${l.moved || ""}</td></tr>`).join("");
    result = `<div class="fp-bill"><b>${escHtml(bill)}</b> · ${escHtml(tr("rank"))} ${p.rank}</div>`
      + `<table class="fp-table"><tr><th>${escHtml(tr("Builds"))}</th><th>${escHtml(tr("capacity"))}</th>`
      + `<th>${escHtml(tr("spare"))}</th><th>${escHtml(tr("mods moved"))}</th></tr>${rows}</table>`;
  } else if (note && note.r && note.r.ok) {
    result = `<div class="warn">${escHtml(formaRefusal(note.r.reason, note.names))}</div>`;
  } else if (note) {
    result = `<div class="warn">${escHtml((note.r && note.r.error) || tr("no plan"))}</div>`;
  }
  box.innerHTML = `<div class="exlabel">${escHtml(tr("Rules"))} · ${escHtml(tr("for every weapon and every Warframe"))}</div>`
    + `<div class="fp-rules"><div class="fp-ticks">`
    + tick("catalyst", ctx.catalystLabel, "doubles capacity")
    + tick("reach_max_rank", "Reach max rank", "spend at least the Forma the item's max rank takes — five on a rank-40 weapon — even where the build needs fewer")
    + (ctx.grantLabel ? tick("grant_slot_first", ctx.grantLabel, "once any Forma is spent, polarize the slot that grants capacity first: it costs at most one Forma over the minimum") : "")
    + tick("fixed_order", "Mods stay in place", "no mod is moved: each slot's polarity serves whatever every build keeps there, which can take more Forma or Omni Forma. Off, mods are moved onto the layout and element mods keep their order")
    + `</div>`
    + `<label class="fp-pick">Omni Forma <select data-r="omni_forma">${opt("omni_forma",
      [["never", "never"], ["allowed", "where it saves a Forma"], ["preferred", "for every polarization"]])}</select></label>`
    + `<label class="fp-pick">Umbra Forma <select data-r="umbra_forma">${opt("umbra_forma",
      [["never", "never"], ["when_needed", "only when nothing else fits"], ["allowed", "like any Forma"]])}</select></label>`
    + `<label class="fp-pick">${escHtml(tr("Forma limit"))} <input type="number" min="0" step="1" data-r="forma_limit"`
    + ` placeholder="${escHtml(tr("none"))}" value="${r.forma_limit ?? ""}"></label>`
    + `</div><div class="exlabel">${escHtml(tr("Plan together"))}</div><div class="fp-group">`
    + `<label class="fp-tick"><input type="checkbox" checked disabled> ${escHtml(ctx.activeLabel || tr("this build"))}`
    + `${ctx.activeLabel ? ` <span class="fp-dim">(${escHtml(tr("open"))})</span>` : ""}</label>`
    + (partners
      ? partners.map((p) => `<label class="fp-tick"><input type="checkbox" data-g="${escHtml(p.id)}"`
        + `${group.has(p.id) ? " checked" : ""}> ${escHtml(p.name)}</label>`).join("")
      : `<span class="fp-dim">${escHtml(tr("a board build is planned on its own"))}</span>`)
    + `</div><div class="exhint">${escHtml(tr("Builds of one item share its polarities. The plan finds one layout every ticked build fits, and moves the other builds' mods onto it."))}</div>`
    + `<div class="fp-foot"><button class="ghost-btn small fp-run">${escHtml(tr("plan Forma"))}</button>`
    + `<div class="fp-result">${result}</div></div>`
    + (ctx.reach ? `<div class="fp-reach">${renderFormaReach()}</div>` : "");
  if (ctx.reach) wireFormaReach(box);
  box.querySelectorAll("[data-r]").forEach((el) => {
    el.addEventListener("change", () => {
      const k = el.dataset.r;
      setFormaRules({ [k]: el.type === "checkbox" ? el.checked
        : k === "forma_limit" ? (el.value === "" ? null : Number(el.value)) : el.value });
      ctx.changed();
    });
  });
  box.querySelectorAll("[data-g]").forEach((el) => {
    el.addEventListener("change", () => setFormaPartner(ctx.item, el.dataset.g, el.checked));
  });
  box.querySelector(".fp-run").addEventListener("click", ctx.plan);
}

function renderBuilderFormaPlan() {
  const w = presetWeapon();
  renderFormaPlan($("forma-plan"), {
    note: "builder",
    at: `${w}\u0000${activePreset}`,
    item: w,
    activeLabel: presetLabel(buildNamed(activePreset)),
    partners: officialBuildActive() ? null : loadPresetList(BUILDS)
      .filter((p) => presetId(p) !== activePreset).map((p) => ({ id: presetId(p), name: presetLabel(p) })),
    catalystLabel: "Orokin Catalyst",
    grantLabel: weaponAxes().hasStance ? "Stance slot first" : null,
    plan: async () => { await autoForma(); renderMods(); },
    changed: () => renderMods(),
    reach: true,
  });
}

// ---- THE PLAN'S OPTIMIZER ----------------------------------------------
//
// What one layout reaches across whole board rulers: for each ruler, the best
// of its builds that fits, as a share of that ruler's leader, and what each
// Forma buys (`/api/forma/optimize`, docs/INVESTMENT.md §The optimizer of the
// plan). It
// only reads; saving a build and placing the builds onto a point are buttons.
const formaReachKey = (w) => `wfsim-forma-reach-${w}`;
function formaReachScope(w) {
  let x = {};
  try { x = JSON.parse(localStorage.getItem(formaReachKey(w)) || "{}") || {}; } catch (_) { x = {}; }
  return {
    // null is every ruler the weapon has rows under.
    benchmarks: Array.isArray(x.benchmarks) ? x.benchmarks : null,
    riven: !!x.riven,
    threshold: Number.isFinite(x.threshold) ? x.threshold : 0.8,
    hard: x.hard !== false,
  };
}
function storeFormaReachScope(w, x) {
  try { localStorage.setItem(formaReachKey(w), JSON.stringify(x)); } catch (_) { /* kept for this page only */ }
}
let formaReach = null;

const boardRulersOf = (w) => {
  const have = new Set((BOARD[w] || []).map((r) => r.benchmark));
  return benchList().map((b) => b.id).filter((id) => have.has(id));
};

/// Each ruler's rows as the planner reads them, with their share of the leader
/// among the rows in scope. A row with a card this page cannot read is left out
/// rather than read light.
function formaReachGroups(w, scope) {
  const rows = (BOARD[w.id] || []).filter((r) => scope.riven || !rowHasRiven(r));
  const want = scope.benchmarks || boardRulersOf(w.id);
  return want.map((b) => {
    const mine = rows.filter((r) => r.benchmark === b);
    const lead = Math.max(0, ...mine.map((r) => r.score || 0));
    const builds = [];
    for (const row of mine) {
      const st = boardRowState(w, row);
      const loadout = weaponLoadout(st.slots, true);
      if (loadout) builds.push({ row, st, loadout, ratio: lead > 0 ? (row.score || 0) / lead : 0 });
    }
    return { benchmark: b, builds };
  }).filter((g) => g.builds.length);
}

async function runFormaReach() {
  const w = weaponInfo($("weapon").value);
  const scope = formaReachScope(w.id);
  const groups = formaReachGroups(w, scope);
  if (!groups.length) {
    formaReach = { at: w.id, error: tr("no board builds in this scope") };
    return;
  }
  const hard = scope.hard && !officialBuildActive()
    ? [weaponLoadout(slots), ...weaponFormaPartners().map((p) => weaponLoadout(storedSlots(p.state)))]
    : [];
  formaReach = { at: w.id, busy: true };
  renderBuilderFormaPlan();
  const r = await api("/api/forma/optimize", {
    weapon: w.id, rules: formaRules(), hard, floor: 0,
    groups: groups.map((g) => ({ builds: g.builds.map((b) => ({ loadout: b.loadout, ratio: b.ratio })) })),
  });
  if ($("weapon").value !== w.id) return;
  const curve = (r && r.curve) || [];
  const hit = curve.findIndex((p) => p.worst >= scope.threshold - 1e-9);
  formaReach = { at: w.id, r, groups, scope, sel: hit >= 0 ? hit : curve.length - 1 };
}

/// A ruler's name cut to its first two terms — what it is and what it fights.
/// The whole name rides in the tooltip.
const benchmarkShort = (id) => benchmarkName(id).split(" · ").slice(0, 2).join(" · ");
const reachPct = (x) => `${Math.floor(x * 1000) / 10}%`;
const reachBill = (p) => [`${p.regular} Forma`, p.umbra ? `${p.umbra} Umbra` : null, p.omni ? `${p.omni} Omni` : null]
  .filter(Boolean).join(" · ");
const reachLayout = (p) => p.layout.main.map((x) => (x ? polGlyph(x) : '<span class="nopol">◇</span>')).join("")
  + (p.layout.exilus ? ` <span class="fp-dim">E</span>${polGlyph(p.layout.exilus)}` : "")
  + (p.layout.grant ? ` <span class="fp-dim">S</span>${polGlyph(p.layout.grant)}` : "");

function renderFormaReach() {
  const w = presetWeapon();
  const scope = formaReachScope(w);
  const rulers = boardRulersOf(w);
  const on = new Set(scope.benchmarks || rulers);
  const tickR = rulers.map((b) => `<label class="fp-tick"><input type="checkbox" data-rb="${escHtml(b)}"`
    + `${on.has(b) ? " checked" : ""}> <span title="${escHtml(benchmarkName(b))}">${escHtml(benchmarkShort(b))}</span></label>`).join("");
  const head = `<div class="exlabel">${escHtml(tr("Plan ahead"))} · ${escHtml(tr("one layout across the board"))}</div>`;
  if (!rulers.length) return head + `<div class="fp-dim">${escHtml(tr("no board builds in this scope"))}</div>`;
  let out = "";
  const x = formaReach && formaReach.at === w ? formaReach : null;
  if (x && x.busy) out = `<div class="fp-dim">${escHtml(tr("working…"))}</div>`;
  else if (x && x.error) out = `<div class="warn">${escHtml(x.error)}</div>`;
  else if (x && x.r && !x.r.ok) out = `<div class="warn">${escHtml(x.r.error || tr("no plan"))}</div>`;
  else if (x && x.r && !x.r.fits) out = `<div class="warn">${escHtml(formaRefusal(x.r.reason, []))}</div>`;
  else if (x && x.r) {
    const curve = x.r.curve || [];
    const rows = curve.map((p, i) => `<tr class="fp-pt${i === x.sel ? " on" : ""}" data-pt="${i}">`
      + `<td>${escHtml(reachBill(p.plan))}</td><td>${reachPct(p.worst)}`
      + `${p.worst >= x.scope.threshold - 1e-9 ? ` <span class="fp-ok">${escHtml(tr("meets the line"))}</span>` : ""}</td>`
      + `<td class="fp-lay">${reachLayout(p.plan)}</td></tr>`).join("");
    const pt = curve[x.sel];
    const picks = pt ? x.groups.map((g, gi) => {
      const k = pt.picks[gi];
      const b = k && g.builds[k.build];
      const what = b
        ? `${escHtml(modeLabel(weaponInfo(w), b.row.mode || "base") || "")}${rowHasRiven(b.row) ? ` <span class="bl-riven">${escHtml(tr("riven"))}</span>` : ""}`
          + ` · <b>${reachPct(k.ratio)}</b> · ${b.row.shown ?? (b.row.score || 0).toFixed(2)}`
          + ` <button class="ghost-btn small" data-save="${gi}">${escHtml(tr("save as a build"))}</button>`
        : `<span class="warn">${escHtml(tr("no build here fits this layout"))}</span>`;
      return `<tr><td title="${escHtml(benchmarkName(g.benchmark))}">${escHtml(benchmarkShort(g.benchmark))}</td><td>${what}</td></tr>`;
    }).join("") : "";
    out = `<table class="fp-table fp-curve"><tr><th>Forma</th><th>${escHtml(tr("weakest ruler"))}</th>`
      + `<th>${escHtml(tr("polarities"))}</th></tr>${rows}</table>`
      + (x.r.exhaustive ? "" : `<div class="warn">${escHtml(tr("the search ran out of budget, so this is the best found rather than the best there is"))}</div>`)
      + (pt ? `<table class="fp-table">${picks}</table>`
        + `<button class="ghost-btn small" data-apply="1">${escHtml(tr("place the ticked builds on these polarities"))}</button>` : "");
  }
  return head
    + `<div class="fp-group">${tickR}</div>`
    + `<div class="fp-rules">`
    + `<label class="fp-tick"><input type="checkbox" data-rs="riven"${scope.riven ? " checked" : ""}> ${escHtml(tr("count riven builds"))}</label>`
    + `<label class="fp-tick"><input type="checkbox" data-rs="hard"${scope.hard ? " checked" : ""}> ${escHtml(tr("the ticked builds must fit too"))}</label>`
    + `<label class="fp-pick">${escHtml(tr("the line"))} <input type="number" min="1" max="100" step="1" data-rs="threshold" value="${Math.round(scope.threshold * 100)}"></label>`
    + `</div><div class="exhint">${escHtml(tr("A ruler is covered by the best of its builds that fits: the line is how close to that ruler's leader it has to come."))}</div>`
    + `<div class="fp-foot"><button class="ghost-btn small fp-reach-run">${escHtml(tr("work it out"))}</button>`
    + `<div class="fp-result">${out}</div></div>`;
}

/// The reach's scope, patched: which rulers, whether riven builds count,
/// whether the builds planned together must fit too, and the line (a share of
/// each ruler's leader, 1% to 100%). The line only chooses which point of a
/// curve already worked out is marked — the curve stands.
function setFormaReachScope(w, patch) {
  const x = { ...formaReachScope(w), ...patch };
  if ("threshold" in patch) x.threshold = Math.min(1, Math.max(0.01, Number(patch.threshold) || 0.8));
  storeFormaReachScope(w, x);
  if (formaReach && formaReach.at === w && formaReach.r && "threshold" in patch) {
    formaReach.scope = formaReachScope(w);
    const hit = (formaReach.r.curve || []).findIndex((p) => p.worst >= formaReach.scope.threshold - 1e-9);
    if (hit >= 0) formaReach.sel = hit;
    renderBuilderFormaPlan();
  }
}

/// A RULER'S BUILD AT THE MARKED POINT, saved as a build of its own, placed on
/// that point's polarities. Returns the name it was saved under.
function saveReachBuild(gi) {
  const x = formaReach;
  const pt = x.r.curve[x.sel];
  const k = pt.picks[gi];
  const b = x.groups[gi].builds[k.build];
  const st = JSON.parse(JSON.stringify(b.st));
  placeFormaPlan(st.slots, { layout: pt.plan.layout, loadouts: [k.placed] }, 0);
  const ps = loadPresetList(BUILDS);
  const name = freeName(ps, (n) => `${benchmarkShort(x.groups[gi].benchmark)} ${reachPct(k.ratio)}${n > 1 ? ` ${n}` : ""}`);
  ps.push({ name, savedAt: Date.now(), state: st });
  storePresetList(BUILDS, ps);
  renderPresetBar();
  return name;
}

function wireFormaReach(box) {
  const w = presetWeapon();
  box.querySelectorAll("[data-rb]").forEach((el) => el.addEventListener("change", () => {
    const on = new Set(formaReachScope(w).benchmarks || boardRulersOf(w));
    if (el.checked) on.add(el.dataset.rb); else on.delete(el.dataset.rb);
    setFormaReachScope(w, { benchmarks: boardRulersOf(w).filter((b) => on.has(b)) });
  }));
  box.querySelectorAll("[data-rs]").forEach((el) => el.addEventListener("change", () => {
    const k = el.dataset.rs;
    setFormaReachScope(w, { [k]: k === "threshold" ? (Number(el.value) || 80) / 100 : el.checked });
  }));
  const run = box.querySelector(".fp-reach-run");
  if (run) run.addEventListener("click", async () => { await runFormaReach(); renderBuilderFormaPlan(); });
  box.querySelectorAll("[data-pt]").forEach((el) => el.addEventListener("click", () => {
    formaReach.sel = Number(el.dataset.pt);
    renderBuilderFormaPlan();
  }));
  box.querySelectorAll("[data-save]").forEach((el) => el.addEventListener("click", () => {
    const name = saveReachBuild(Number(el.dataset.save));
    el.textContent = `✓ ${name}`;
    el.disabled = true;
  }));
  const apply = box.querySelector("[data-apply]");
  if (apply) apply.addEventListener("click", async () => {
    const pt = formaReach.r.curve[formaReach.sel];
    await autoForma({ onto: pt.plan });
    renderMods();
  });
}

