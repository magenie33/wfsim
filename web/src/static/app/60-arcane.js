// ---- Arcane ----
// Full parity with mods: ONE slot card (rank stepper, ⋯ menu) → click opens a
// searchable picker that matches name OR effect, with effect lines, rarity
// frames, and the equipped arcane highlighted in the accent background family.
// The arcanes the CURRENT weapon can equip: its own slot's pool. Arcane ids
// are globally unique, so lookups stay unfiltered — only the PICKERS narrow.
//
// The EQUIPPABLE arcanes of this weapon's slot. "none" is the empty-slot
// sentinel, not an arcane, and it is offered in NO list: the builder slot has
// its own "Remove arcane", and in the optimizer an empty arcane scope already
// means "run no arcane" — a "None" row there was a second way to say the same
// thing, sitting in a list of real choices.
const arcanePools = (weaponId) =>
  (weaponInfo(weaponId || $("weapon").value) || {}).arcane_pools || [];
// The arcanes SLOT i may hold — pool i's, and only pool i's. A picker that
// offers the whole set and then refuses the pick is a worse way to say the
// same thing.
/// WHICH SEATS an arcane fits, by id. Almost always the one directory it is
/// filed under; a KITGUN arcane fits both, because a Kitgun is one weapon with
/// a roster entry per slot. `slot` is the fallback for a meta served before
/// seats existed, so a page loaded against an older build still sees what it
/// always saw.
const arcaneSeats = (a) => (a && (a.seats || (a.slot ? [a.slot] : []))) || [];
/// Do two arcanes compete for the same seat? The optimizer groups its marks by
/// this, because a pin in the Primary seat must not clear a Secondary mark.
const sameArcaneSeat = (a, b) =>
  arcaneSeats(a).some((s) => arcaneSeats(b).includes(s));

const arcanePool = (i = 0) => {
  const pool = arcanePools()[i];
  // WHICH ARCANES THIS WEAPON MAY SEAT is the ENGINE's answer, per weapon —
  // the CONSEQUENCE, not a rule restated here, which is `evo_forbids`' and
  // `auras`' own pattern. It was `equip_classes` until 2026-08-22, and that was
  // enough while every narrowing arcane named a CLASS (Shotgun Vendetta,
  // Longbow Sharpshot). The eight Kitgun arcanes narrow by a TRAIT instead,
  // because no class can say "Kitgun" — a secondary Tombfinger is a `pistol`
  // exactly like a Lex — so the picker offered all eight on a Lex, and a third
  // kind of gate would have gone stale the same way.
  //
  // `equip_classes` stays as the fallback for a meta served before this field
  // existed, so a page loaded against an older build still narrows the two it
  // could always narrow.
  const w = weaponInfo($("weapon").value) || {};
  const allowed = w.arcanes;
  return (META.arcanes || []).filter(
    (a) =>
      a.id !== "none" &&
      // THE SEATS IT FITS, not the directory it is filed under. A Kitgun
      // arcane fits both, because a Kitgun is one weapon with a roster entry
      // per slot; `slot` is the fallback for a meta served before seats
      // existed.
      arcaneSeats(a).includes(pool) &&
      (allowed
        ? allowed.includes(a.id)
        : !(a.equip_classes || []).length || a.equip_classes.includes(w.class)),
  );
};
// An arcane belongs to ONE slot, so another slot's arcane is not a
// questionable choice on this weapon — it cannot be equipped at all. Ids reach
// the page from saved states, presets, shared URLs and optimizer results, so
// every one of them goes through here instead of being trusted (a SECONDARY
// arcane rode a saved state onto the first primary weapon).
// The engine refuses the same thing independently
// (`data::arcanes::for_slot`); this keeps the UI from ever showing a build the
// sim would not run.
// Pre-data short names, from builds saved before arcane ids were data. They
// are rewritten HERE, at the one point an id enters state, so the wire format
// stays a single shape and nothing downstream reads two spellings. A row in someone's localStorage is history, not a format.
const ARCANE_RENAMED = {
  enervate: "secondary_enervate",
  deadhead: "secondary_deadhead",
  flare: "cascadia_flare",
};
function arcaneFor(weaponId, id, i = 0) {
  if (!id || id === "none") return "none";
  const canon = ARCANE_RENAMED[id] || id;
  const a = arcaneById(canon);
  return a && arcaneSeats(a).includes(arcanePools(weaponId)[i]) ? canon : "none";
}
/// Can this weapon seat this arcane in ANY of its slots?
///
/// `arcaneFor` answers a different question — "does it fit slot i" — and its
/// `i` defaults to 0. The optimizer's SCOPE is not per-slot (an arcane belongs
/// to exactly one pool, so the flat mark map already says which), so asking
/// `arcaneFor(w, id)` there silently meant "does it fit the FIRST pool": every
/// secondary mark was dropped on restoring an optimizer-arcanes preset, and
/// the search then had nothing to put in the second slot.
const arcaneFitsWeapon = (weaponId, id) => {
  const a = arcaneById(ARCANE_RENAMED[id] || id);
  // BY SEAT, NOT BY DIRECTORY. `a.slot` is where the file LIVES, which was the
  // same answer as where it fits for every arcane in the game until the eight
  // Kitgun ones — they are filed under `secondary/` because ids are globally
  // unique, and they seat in `kitgun`. Reading `slot` here dropped every
  // Kitgun mark an optimizer scope carried, the same shape as the
  // secondary-mark bug this function's own comment is about.
  return !!a && arcanePools(weaponId).some((p) => arcaneSeats(a).includes(p));
};
/// SEAT AN ARCANE LIST ON A WEAPON — by POSITION first, then by POOL.
///
/// Position is the wire format and stays it: a build's arcane list is one id
/// per seat in the weapon's own seat order, and a list written under today's
/// seats is left exactly where it is.
///
/// WHAT A SAVED BUILD CANNOT KNOW IS THAT THE ORDER MOVED UNDER IT. A Kitgun
/// gained a seat of its own IN FRONT of its ordinary one,
/// so every Tombfinger build written before that has its Primary arcane
/// sitting in the Kitgun seat — where it fits nothing, and where the old
/// index-only rule dropped it without a word. An id that no longer fits where
/// it sits is offered the seats that are still empty; one that fits nowhere is
/// still dropped, because there is nowhere for it to go.
///
/// Returns the mapping as well, so a positional array BESIDE the ids — the
/// ranks — can follow the move instead of staying behind on an index that now
/// means something else.
function seatArcanes(weaponId, list) {
  const pools = arcanePools(weaponId);
  const raw = Array.isArray(list) ? list.slice() : list == null ? [] : [list];
  const ids = pools.map(() => "none");
  const from = pools.map(() => -1);
  const spare = [];
  raw.forEach((id, i) => {
    const fit = i < pools.length ? arcaneFor(weaponId, id, i) : "none";
    if (fit !== "none") { ids[i] = fit; from[i] = i; return; }
    if (id && id !== "none") spare.push({ id, i });
  });
  spare.forEach(({ id, i }) => {
    const j = pools.findIndex((_, k) => ids[k] === "none" && arcaneFor(weaponId, id, k) !== "none");
    if (j < 0) return;
    ids[j] = arcaneFor(weaponId, id, j);
    from[j] = i;
  });
  return { ids, from };
}
/// Every slot's id, validated against the pool that slot draws from.
const arcanesFor = (weaponId, list) => seatArcanes(weaponId, list).ids;
/// The builder picker's list. Same set — kept as its own name because the
/// picker is where a reader looks for it.
const arcanePickPool = arcanePool;
/// Which slot the picker is filling — the popover is shared, the slot is not.
let arcaneSlotIdx = 0;
const arcaneById = (id) => META.arcanes.find((x) => x.id === id);
/// Seat `i`'s arcane as a ranked id — the spelling a list row carries.
const arcaneRankedId = (i) => {
  const a = arcaneById(arcanes[i]);
  const r = arcaneRanks[i];
  return a && r != null && r < (a.max_rank || 0) ? `${a.id}@${r}` : arcanes[i];
};
/// `a` at each rank below its max when it is on the every-rank list — the
/// arcane twin of `lowerRanks`.
const lowerArcaneRanks = (a) => (everyRank().arcanes.includes(a.id)
  ? Array.from({ length: a.max_rank || 0 }, (_, r) => ({ ...a, id: `${a.id}@${r}`, card: a.id, rank: r }))
  : []);
// new arcane → max rank, in the slot the picker was opened from
// EVERY ARCANE MUTATION REFRESHES, because the mutation owns the consequence.
//
// Redrawing the arcane slots and nothing else leaves the panel, its stat rows
// and the SIM'S BUFF BAR showing the previous arcane until some unrelated edit
// happens to call `refreshPanel` — toggling a mod, usually.
//
// `refreshPanel` is the funnel — its own comment says "every build change
// funnels through here" — so the fix is not to add the call at each picker but
// to make it impossible to mutate an arcane without it. A future control that
// sets an arcane gets the refresh for free; one that assigns `arcanes[i]`
// directly is the thing to look for in review.
function setArcane(id, i = arcaneSlotIdx) {
  arcanes[i] = id;
  arcaneRanks[i] = null;
  refreshPanel();
}
/// An arcane's RANK is a build change too: its numbers scale per rank, so the
/// panel and the buff bar are wrong until they are re-asked.
function setArcaneRank(i, rank) {
  arcaneRanks[i] = rank;
  refreshPanel();
}
// Effect lines for a specific rank (clamped). Arcane strengths scale per rank
// (wiki), so the slot shows the SELECTED rank; the picker shows max rank.
const effectsAt = (a, r) => {
  const rk = a && a.ranks || [];
  if (!rk.length) return [];
  return rk[Math.max(0, Math.min(rk.length - 1, r))] || [];
};
// Renders card lines that are ALREADY in the display language (cardLines
// did the choosing) — this is layout, not translation.
const effLines = (arr) => arr.length ? `<div class="me">${arr.map((x) => `<div>${x}</div>`).join("")}</div>` : "";

function renderArcanes() {
  const box = $("arcane-slots");
  box.innerHTML = "";
  weaponAxes().arcanes.forEach((ax, i) => box.appendChild(arcaneSlotEl(ax.pool, i)));
}

// One arcane slot: the card if filled, the "+ add" plate if not. The POOL is
// named on the plate only when a weapon seats more than one, because that is
// the only time it tells you anything.
function arcaneSlotEl(pool, i) {
  const many = arcanePools().length > 1;
  const a = arcaneById(arcanes[i]);
  const none = !a || a.id === "none";
  const el = document.createElement("div");
  if (none) {
    el.className = "slot empty arc";
    el.innerHTML = `<span class="plus">+ ${escHtml(
      many ? tr("add {pool} arcane").replace("{pool}", tr(ARC_POOL_LABEL[pool] || pool)) : tr("add arcane"),
    )}</span>`;
  } else {
    const maxr = a.max_rank || 0;
    const r = arcaneRanks[i] == null ? maxr : Math.max(0, Math.min(maxr, arcaneRanks[i]));
    const lowered = r < maxr;
    const rank = maxr > 0
      ? `<span class="rank ${lowered ? "lowered" : ""}"><button class="rk" data-d="-1">−</button><b>R${r}${lowered ? "/" + maxr : ""}</b><button class="rk" data-d="1">+</button></span>`
      : "";
    el.className = "slot filled arc" + (a.rarity ? " rar-" + a.rarity : "");
    // The slot shows the verbatim DESCRIPTION at the selected rank (like
    // the mod cards); model effect lines remain the search text.
    el.innerHTML = imgTag(IMG(a.image), "mod") +
      `<div class="info"><div class="mn">${wl(a.name, wikiUrl(a.name_en || a.name))}${arcaneMarketLink(a)}</div>${effLines(cardLines(a, r, effectsAt(a, r)))}${rank}</div>` +
      `<button class="dots" title="options">⋯</button>`;
    el.querySelector(".dots").addEventListener("click", (e) => { e.stopPropagation(); openArcaneMenu(e.currentTarget, i); });
    el.querySelectorAll(".rk").forEach((b) => b.addEventListener("click", (e) => {
      e.stopPropagation();
      setArcaneRank(i, Math.max(0, Math.min(maxr, r + Number(b.dataset.d))));
      renderArcanes();
    }));
  }
  // Mod-slot parity: only the EMPTY slot opens the picker on click; a
  // filled card swaps via its ⋯ menu — so its text stays selectable.
  if (none) {
    el.addEventListener("click", (e) => { e.stopPropagation(); openArcanePicker(el, i); });
  }
  return el;
}

function openArcanePicker(anchor, i = 0) {
  arcaneSlotIdx = i;
  closePopovers();
  const pop = $("arcane-popover");
  place(pop, anchor);
  const search = $("arcane-search");
  search.value = "";
  search.oninput = () => renderArcaneMenu(search.value);
  renderArcaneMenu("");
  search.focus();
  ensureGains({ kind: "arcane", idx: i },
    () => { if (!$("arcane-popover").hidden) renderArcaneMenu($("arcane-search").value); },
    true);
}

/// THE ARCANE CARD as a picker row — `modRow`'s counterpart, shared by the
/// arcane slot's picker and the every-rank list so the two cannot drift.
const arcaneRow = (a, { cls = "", attrs = "", chips = "", trailing = "", rank = a.max_rank } = {}) =>
  `<div class="opt ${cls} ${a.rarity ? "rar-" + a.rarity : ""}" data-id="${a.id}" ${attrs}>
      ${imgTag(IMG(a.image), "mod")}
      <div class="info"><div class="mn">${wl(a.name, wikiUrl(a.name_en || a.name))}${arcaneMarketLink(a)}${chips}</div>${effLines(cardLines(a, rank, effectsAt(a, rank)))}</div>${trailing}</div>`;

// Search matches NAME or any EFFECT line (like the mod picker). "None" always
// stays listed as the clear-out option.
function renderArcaneMenu(query) {
  const menu = $("arcane-menu");
  const q = query.trim().toLowerCase();
  // Search matches NAME (localized or English), ANY rank's effect text,
  // or the description — in either language (searchBlob).
  // Same rule as the mod picker, minus a key an arcane does not have: there
  // is no drain on an arcane, so it is effect then name.
  const here = arcaneRankedId(arcaneSlotIdx);
  const hits = arcanePickPool(arcaneSlotIdx)
    .filter((a) => !q || searchBlob(a).includes(q))
    .flatMap((a) => [a, ...lowerArcaneRanks(a)])
    .sort((a, b) => (a.id === here ? -1 : b.id === here ? 1 : 0)
      || gainSort(a, b, ["gain", "name"]));
  menu.innerHTML = scanStrip(gainScan, { kind: "arcane", idx: arcaneSlotIdx }, hits.map((a) => a.id))
    + (hits.length ? hits.map((a) => {
    const isCur = a.id === here;
    return arcaneRow(a, {
      cls: isCur ? "cur" : "",
      chips: `${a.card ? ` <span class="rkchip">R${a.rank}</span>` : ""}${isCur ? ' <span class="slotchip cur">equipped</span>' : ""}${gainChipFor(a.id, tr("Arcane"))}`,
      rank: a.card ? a.rank : a.max_rank,
    });
  }).join("") : `<div class="opt dis">no matches</div>`);
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", () => {
    const [id, rank] = splitRank(o.dataset.id);
    const i = arcaneSlotIdx;
    setArcane(id, i);
    if (rank != null) setArcaneRank(i, rank);
    closePopovers(); renderArcanes();
  }));
}

// ⋯ on a filled arcane slot: mirror the mod slot menu (remove).
// THE ARCANE'S OWN MENU, on the shared one. It was the model for
// `rankedSlot` and had drifted from it in two ways worth ending: its two items
// were HARDCODED ENGLISH — no `tr()` at all, so a Chinese page said "Swap
// arcane" — and a second copy of this markup is how the two would keep
// drifting. The picker stays its own, because an arcane's list carries images
// and a rank that the ranked list does not.
function openArcaneMenu(anchor, i = 0) {
  arcaneSlotIdx = i;
  openSlotMenu(anchor, null, {
    label: tr("Arcane"),
    removable: true,
    onSwap: () => openArcanePicker(anchor, i),
    onPick: () => { setArcane("none", i); renderArcanes(); },
  });
}

