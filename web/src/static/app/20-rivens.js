// ---- Rivens ------------------------------------------------------------
// A CONSTRUCTOR, not a roller. Every control is bounded by the formula, so
// the only rivens it can express are legal ones — including the corner where
// every bonus rolls maximal and the malus minimal, which is not an edge
// case here but the CEILING, the riven an optimizer wants to know about.
//
// The arithmetic is NOT duplicated: every change asks `/api/riven`, so a
// slider can never drift from what the sim would build, and a typed value
// comes back as the roll it implies. Under wasm that call is local.
let riven = null;      // the riven being edited
let rivenResolved = null;
const RIVENS = "rivens";   // its preset domain, per weapon like the builds

// The stat pool this weapon's rivens draw from. NOT its mod class: a bow's
// mods are `bow` and its rivens are `rifle`, so the server derives which pool
// applies and says so.
// `riven_excludes` takes out what THIS weapon cannot roll (MEASUREMENTS M35):
// a sentinel weapon has no Zoom and no Recoil, a hit-scan one no flight speed,
// an infinite-ammo one no Ammo Maximum, and a PHYSICAL stat is out only where
// the family's cards say it never rolls. `riven_unconfirmed` is the physical
// stats nobody has counted: offered, and marked. The class table stays shared;
// only the weapon's view of it narrows.
const rivenPoolAll = () => {
  const w = weaponInfo($("weapon").value);
  return (META.riven_stats || {})[w.riven_class || w.mod_class] || [];
};
const rivenPool = () => {
  const out = (weaponInfo($("weapon").value).riven_excludes) || [];
  const all = rivenPoolAll();
  return out.length ? all.filter((s) => !out.includes(s.id)) : all;
};
const rivenRules = () => META.riven_rules || { roll_min: 0.9, roll_max: 1.1, max_rank: 8 };
// Lookup goes through the UNFILTERED pool: a riven saved before its weapon
// learned it could not roll that stat still has to render and still resolves
// server-side (`resolved_slots` finds by id in the whole class pool). Only
// what the picker OFFERS narrows.
const rivenStat = (id) => rivenPoolAll().find((s) => s.id === id);
// The stat's NAME, without the placeholder or the unit: the row already
// shows the value, so repeating "X%" in the picker is noise.
//
// A riven line is OUR sentence built from DE's template, so it localizes the
// way every other engine-generated effect line does — through the locale's
// effect_phrases, which are the official client's words (data/i18n). There is
// nothing riven-specific to translate: "+150% Critical Chance" reads the same
// on a riven as on Point Strike.
// The unit sits between the hole and the name — `%` on most, `s` on Combo
// Duration — and is not part of what the stat is called.
const rivenStatNameEn = (s) =>
  s.text.replace("|val|", "").replace(/^\s*[%s]\s*/, "").replace(/\s+/g, " ").trim();
const rivenStatName = (s) => tf(rivenStatNameEn(s));
// The disclaimer every spliced stat carries (see notes: spliced_riven_stat).
const RIVEN_SPLICED_NOTE = "A spliced stat's value uses the formula every rolled stat uses, on DE's own base number, and no in-game card has been checked against it yet — the number on your card may differ.";

// The shape, in the notation everyone already uses: 2, 3, 2+1, 3+1 — the
// count of bonuses, and a +1 for the malus. It leads because it is the
// ONLY thing that decides the multipliers; a 2 and a 2+1 pay their bonuses
// differently before a single stat is chosen.
// Best first, left to right: 3+1, 3, 2+1, 2. A riven is
// shopped for from the top — the third bonus is what makes one worth having,
// and the malus is the price, so the pairs read as "with / without price"
// rather than as a count that happens to climb.
const RIVEN_SHAPES = [
  { id: "3+1", bonuses: 3, malus: true },
  { id: "3", bonuses: 3, malus: false },
  { id: "2+1", bonuses: 2, malus: true },
  { id: "2", bonuses: 2, malus: false },
];
// A new card starts at 3+1 — the shape a riven worth
// making is in. Stated by ID, not by position, so the display order above
// stays a display decision and never doubles as the default.
const RIVEN_SHAPE_DEFAULT = "3+1";

// An EMPTY card. Nothing is pre-picked: a default stat is a claim the visitor
// did not make. Two bonuses is the game's floor for a riven rather than a
// suggestion, and the rolls sit at 1.0 because a slider has to be somewhere.
// FOUR drafts, one per shape, and only the active one is the riven.
//
// Without them, switching 3+1 -> 2+1 -> 3+1 loses the third stat: the slot is
// popped and there is nowhere for it to have gone.
// Keeping each shape's own stats means a shape switch is a switch, not an
// edit — you can compare a 2+1 against a 3+1 by clicking between them.
//
// `bonuses` / `malus` stay the ACTIVE shape's, so everything downstream —
// the rows, the payload, the engine — keeps reading a riven the same way.
const RIVEN_BLANK_DRAFT = (n, malus) => ({
  bonuses: Array.from({ length: n }, () => ({ id: null, roll: 1.0 })),
  malus: malus ? { id: null, roll: 1.0 } : null,
});
function blankRiven() {
  const drafts = {};
  RIVEN_SHAPES.forEach((s) => { drafts[s.id] = RIVEN_BLANK_DRAFT(s.bonuses, s.malus); });
  return {
    shape: RIVEN_SHAPE_DEFAULT,
    drafts,
    bonuses: drafts[RIVEN_SHAPE_DEFAULT].bonuses,
    malus: drafts[RIVEN_SHAPE_DEFAULT].malus,
    rank: rivenRules().max_rank,
    polarity: "madurai",
  };
}

/// Bring a riven up to the four-draft shape — older saved ones carry only the
/// active stats, and a missing draft is simply an empty one.
function withDrafts(r) {
  const out = JSON.parse(JSON.stringify(r || {}));
  out.rank = out.rank ?? rivenRules().max_rank;
  out.polarity = out.polarity || "madurai";
  // A riven saved before the wiki's Bonus/Malus wording still says
  // positives/curse. It is the visitor's own item and outlives our
  // vocabulary, so it is read either way and re-saved in the new words.
  out.bonuses = out.bonuses || out.positives || [];
  out.malus = out.malus || out.curse || null;
  delete out.positives;
  delete out.curse;
  Object.values(out.drafts || {}).forEach((d) => {
    d.bonuses = d.bonuses || d.positives || [];
    d.malus = d.malus || d.curse || null;
    delete d.positives;
    delete d.curse;
  });
  out.shape = out.shape || `${out.bonuses.length || 2}${out.malus ? "+1" : ""}`;
  out.drafts = out.drafts || {};
  RIVEN_SHAPES.forEach((s) => {
    if (!out.drafts[s.id]) out.drafts[s.id] = RIVEN_BLANK_DRAFT(s.bonuses, s.malus);
  });
  // The stats it was saved with belong to the shape it was saved in.
  if (out.bonuses.length) {
    out.drafts[out.shape] = { bonuses: out.bonuses, malus: out.malus };
  }
  const d = out.drafts[out.shape];
  out.bonuses = d.bonuses;
  out.malus = d.malus;
  return out;
}

/// Every slot filled? An unfinished card is not an ILLEGAL riven — it is one
/// that has not been described yet, and it must not be reported as an error.
const rivenComplete = () =>
  riven && riven.bonuses.every((s) => s.id) && (!riven.malus || riven.malus.id);

/// THE SAME CARD ON EVERY OTHER WEAPON IT FITS — a Prime and its base, a
/// Kitgun's other slot — each read at its own disposition.
const rivenKin = (w) => ((META && META.weapons) || []).filter((x) =>
  w && x.riven_family && x.riven_family === w.riven_family && x.id !== w.id
  // …AND THE SAME CARD, which a family whose members took two kinds of riven
  // would not share.
  && (x.riven_class || x.mod_class) === (w.riven_class || w.mod_class));
/// A kin weapon's name, with its slot where the roster has two of that name
/// (a Kitgun's two entries are both the chamber).
const kinName = (x) => (((META && META.weapons) || []).filter((y) => y.name === x.name).length > 1
  ? `${x.name} (${tr(x.slot === "secondary" ? "Secondary" : "Primary")})` : x.name);
/// The open card as each kin weapon reads it, filled beside `rivenResolved`.
let rivenKinResolved = [];

async function resolveRiven(pending) {
  if (!riven) return;
  try {
    // `pending` carries a typed VALUE for one slot; the server turns it into
    // the roll it implies, clamped, so the formula stays in one place.
    const body = { weapon: $("weapon").value, ...riven };
    if (pending) {
      const clone = JSON.parse(JSON.stringify(body));
      if (pending.slot === "malus") clone.malus.value = pending.value;
      else clone.bonuses[Number(pending.slot)].value = pending.value;
      rivenResolved = await api("/api/riven", clone);
      // Adopt the roll the value implied, so slider and box agree.
      (rivenResolved.stats || []).forEach((s) => {
        const at = s.slot === "malus" ? riven.malus : riven.bonuses[Number(s.slot)];
        if (at) at.roll = s.roll;
      });
    } else {
      rivenResolved = await api("/api/riven", body);
    }
  } catch (e) {
    rivenResolved = { ok: false, illegal: [String(e)], stats: [] };
  }
  const kin = rivenComplete() ? rivenKin(weaponInfo($("weapon").value)) : [];
  rivenKinResolved = await Promise.all(kin.map((x) =>
    api("/api/riven", { ...riven, weapon: x.id }).then((r) => ({ w: x, r }), () => null)));
  renderRivenCard();
}

// The collection, guaranteed non-empty and with a live active name. There is
// ALWAYS one riven, exactly as the builder always has "build 1": a bar whose
// only option is "+ new" makes the visitor do a step the page could have done, and the first one is the empty card they were going to
// fill in anyway.
// Rivens are an OPTIONAL collection: zero is a legal, and the ordinary,
// number to own. Nothing is auto-created — a blank card
// standing in for "no riven" is a claim the visitor never made, and it put a
// phantom legendary in every weapon's mod pool. Custom enemies will be the
// same shape when they arrive.
// The stored list. It does NOT open anything: closing the last file is a
// state, and re-opening one behind the user's back would make "← all rivens" a
// button that does nothing.
//
// AND IT NO LONGER DELETES THE POINTER ON A MISS. It did, which made every
// reason the card was momentarily not in this list — a render that lands
// before the weapon switch completes, a family whose scope has moved — a
// silent forget of which card was open. A pointer at nothing opens nothing,
// which is the whole behaviour that was wanted; throwing it away as well is
// the part that could only lose something.
function ensureRivenList() {
  return loadPresetList(RIVENS);
}

/// OPEN A SAVED RIVEN for editing, or "" for the list. Its card is re-read from
/// storage by the render.
function openRiven(id) {
  flushPresetSaves();
  activeRiven = id;
  if (id) localStorage.setItem(presetActiveKey(RIVENS), id);
  else localStorage.removeItem(presetActiveKey(RIVENS));
  riven = null;
  renderRivens();
}

/// "+ new riven": a blank card, saved and opened. Returns its id.
function newRiven() {
  const ps = loadPresetList(RIVENS);
  const name = freeName(ps, (n) => autoPresetName("riven", n));
  const id = newRivenId(loadPresetWhole(RIVENS));
  riven = { ...withDrafts(blankRiven()), __weapon: $("weapon").value };
  ps.push({ id, name, savedAt: Date.now(), state: snapshotRiven() });
  storePresetList(RIVENS, ps);
  openRiven(id);
  // THE LIST SHOWS NUMBERS, and they are the engine's — so a card that has
  // just appeared has to ask for them.
  refreshRivenNames();
  return id;
}

/// ⧉ on the open riven: a copy, saved and opened. Returns its id. The copy is
/// worth what the original is, so its printed values are seeded from it — a
/// copy of an identical spec cannot answer differently.
function copyRiven() {
  const open = activeRivenId();
  const ps = loadPresetList(RIVENS);
  const from = ps.find((x) => x.id === open) || {};
  const name = freeName(ps, (n) => (from.name || "riven") + " copy" + (n > 1 ? " " + n : ""));
  const id = newRivenId(loadPresetWhole(RIVENS));
  ps.push({ id, name, savedAt: Date.now(), state: snapshotRiven() });
  if (rivenNames[open]) rivenNames[id] = rivenNames[open];
  storePresetList(RIVENS, ps);
  openRiven(id);
  refreshRivenNames();
  return id;
}

function renderRivens() {
  if (!META || !$("riven-block")) return;
  const w = weaponInfo($("weapon").value);
  const ps = ensureRivenList();
  const open = ps.find((p) => p.id === activeRivenId());
  // The weapon and its disposition, and nothing else: that the values below
  // are scaled by it is what a disposition IS, so saying it was noise.
  //
  // …AND WHOSE RIVEN IT IS, once a family has more than one member on the
  // roster. A card is the FAMILY's, so a riven the player
  // built on the Burston is in the Burston Prime's list — and silent sharing
  // reads as a bug the first time a riven you never made here turns up. It
  // names the other variants rather than counting them, because "also fits 1
  // other" is the one thing a reader cannot act on, and it states the
  // disposition rule, since that is why the same card shows two sets of
  // numbers.
  const kin = rivenKin(w);
  // THE FAMILY IS NAMED IN THE READER'S LANGUAGE, by borrowing the localized
  // name of its BASE member rather than printing the family string. That
  // string is DE's module `Family` field and is always English, so a Chinese
  // page said "一张 Burston 紫卡" beside 伯斯顿 Prime — and inventing a
  // Chinese name for it would be a TRANSLATION of an id. The base member's
  // name is DE's own transcription and already in `names.yaml`.
  const base = [w, ...kin].find((x) => x.id === (w.riven_family || "").toLowerCase()
    .replace(/[^a-z0-9]+/g, "_"));
  $("riven-sub").textContent =
    `${w.name} · ${tr("disposition")} ${(w.disposition || 1).toFixed(2)}`
    + (kin.length
      ? ` · ${tr("a {family} riven — it fits {others} too, each at its own disposition")
          .replace("{family}", (base && base.name) || w.riven_family)
          .replace("{others}", kin.map(kinName).join(", "))}`
      : "");
  renderRivenTools();
  if (!open) {
    // LIST MODE — nothing is being edited, so nothing pretends to be. The
    // editor's boxes are emptied rather than hidden: an empty container in
    // the flow keeps the page from jumping when a file is opened.
    riven = null;
    ["riven-shape", "riven-stats", "riven-foot", "riven-card"].forEach((id) => {
      if ($(id)) $(id).innerHTML = "";
    });
    renderRivenAll();
    return;
  }
  // EDIT MODE — the open file, re-read when the weapon changed under it.
  if (!riven || riven.__weapon !== w.id) {
    riven = { ...withDrafts(open.state || blankRiven()), __weapon: w.id };
  }
  renderRivenShape();
  renderRivenStats();
  renderRivenFoot();
  renderRivenAll();
  resolveRiven();
}

// "3 Bonus, 1 Malus" — the shape said in words, in whichever language.
const shapeWords = (s) =>
  `${s.bonuses} ${tr("Bonus")}${s.malus ? `, 1 ${tr("Malus")}` : ""}`;

function renderRivenShape() {
  const now = riven.shape || `${riven.bonuses.length}${riven.malus ? "+1" : ""}`;
  const shapeNow = RIVEN_SHAPES.find((x) => x.id === now)
    || RIVEN_SHAPES.find((x) => x.id === RIVEN_SHAPE_DEFAULT);
  $("riven-shape").innerHTML =
    `<span class="rv-lbl">${escHtml(tr("Shape"))}</span><span class="oseg">` +
    RIVEN_SHAPES.map((s) =>
      `<span class="seg ${s.id === now ? "on" : ""}" data-rv="${s.id}"
             title="${escHtml(shapeWords(s))}">${s.id}</span>`).join("") +
    `</span><span class="rv-lbl dim">${escHtml(shapeWords(shapeNow))}</span>`;
  $("riven-shape").querySelectorAll("[data-rv]").forEach((el) => el.onclick = () => {
    const want = el.dataset.rv;
    if (want === riven.shape) return;
    // Park the current shape's stats in its own draft, then adopt the target
    // shape's. Nothing is discarded, so clicking back and forth is free.
    riven.drafts[riven.shape] = { bonuses: riven.bonuses, malus: riven.malus };
    riven.shape = want;
    const s = RIVEN_SHAPES.find((x) => x.id === want);
    const d = riven.drafts[want] || RIVEN_BLANK_DRAFT(s.bonuses, s.malus);
    riven.drafts[want] = d;
    riven.bonuses = d.bonuses;
    riven.malus = d.malus;
    markRivenDirty();
    renderRivens();
  });
}

// One row per rolled stat: the stat, where in its 0.9-1.1 band it landed, and
// what that comes to. The slider's ENDS are the band and the number box is
// clamped to the same, so an illegal roll is not something a control can
// express — "any legal riven and only legal ones" holds by construction.
function renderRivenStats() {
  const rules = rivenRules();
  const row = (slot, s, isMalus) => {
    const def = rivenStat(s.id);
    return `<div class="rv-row ${isMalus ? "malus" : ""}">
      <span class="rv-tag">${escHtml(tr(isMalus ? "Malus" : "Bonus"))}${def && def.spliced
        ? `<span class="rv-spl" title="${escHtml(tr(RIVEN_SPLICED_NOTE))}">${escHtml(tr("spliced"))}</span>` : ""}</span>
      <button class="rv-pick" data-slot="${slot}">${def ? escHtml(rivenStatName(def)) : escHtml(tr("choose a stat"))}</button>
      <input class="rv-roll" type="range" data-slot="${slot}"
             min="${rules.roll_min}" max="${rules.roll_max}" step="0.001" value="${s.roll}">
      <input class="rv-num" type="number" data-slot="${slot}" step="0.1" placeholder="—">
      <span class="rv-pct" data-slot="${slot}" title="${escHtml(tr("where this roll landed in its 0.9-1.1 band"))}"></span>
      <span class="rv-mult" data-slot="${slot}" title="${escHtml(tr("the roll itself — the random multiplier this stat drew, 0.900 to 1.100"))}"></span>
      <span class="rv-unit" data-slot="${slot}"></span>
    </div>`;
  };
  // A SPLICED STAT IS NOT MEASURED, and the card says so beside it.
  const spliced = riven.bonuses.concat(riven.malus ? [riven.malus] : []).some((s) => (rivenStat(s.id) || {}).spliced);
  $("riven-stats").innerHTML =
    riven.bonuses.map((s, i) => row(String(i), s, false)).join("") +
    (riven.malus ? row("malus", riven.malus, true) : "") +
    (spliced ? `<div class="rv-disclaim"><b>${escHtml(tr("For reference only"))}</b> ${escHtml(tr(RIVEN_SPLICED_NOTE))}</div>` : "");

  const at = (slot) => (slot === "malus" ? riven.malus : riven.bonuses[Number(slot)]);
  $("riven-stats").querySelectorAll(".rv-pick").forEach((el) =>
    el.onclick = () => openRivenPicker(el, el.dataset.slot));
  $("riven-stats").querySelectorAll(".rv-roll").forEach((el) => el.oninput = () => {
    at(el.dataset.slot).roll = Number(el.value);
    markRivenDirty();
    resolveRiven();
  });
  // The number box takes the VALUE, not the roll — the number printed on a
  // riven you own. Out of range snaps to the nearest legal end rather than
  // being refused, so typing is never a dead end.
  $("riven-stats").querySelectorAll(".rv-num").forEach((el) => el.onchange = () => {
    if (!at(el.dataset.slot).id || el.value === "") { renderRivenCard(); return; }
    markRivenDirty();
    resolveRiven({ slot: el.dataset.slot, value: Number(el.value) });
  });
}

// The same searchable popover the mod and arcane pickers use — a riven stat
// is picked the way everything else on this page is picked.
function openRivenPicker(anchor, slot) {
  closePopovers();
  // ITS OWN popover. Borrowing the mod picker's nodes dragged the mod
  // picker's sort header in with them, and using it rendered the mod list
  // into this menu. Separate elements is the only
  // isolation that cannot leak.
  const pop = $("riven-popover");
  const search = $("riven-search");
  const menu = $("riven-menu");
  const at = slot === "malus" ? riven.malus : riven.bonuses[Number(slot)];
  const used = new Set(riven.bonuses.map((x) => x.id).concat(riven.malus ? [riven.malus.id] : []));
  // ONE SPLICED STAT A CARD: another slot holding one takes the rest off the list.
  const splicedElsewhere = riven.bonuses.concat(riven.malus ? [riven.malus] : [])
    .some((x) => x !== at && (rivenStat(x.id) || {}).spliced);
  const unconfirmed = weaponInfo($("weapon").value).riven_unconfirmed || [];
  const draw = (q) => {
    const f = (q || "").trim().toLowerCase();
    menu.innerHTML = rivenPool()
      // A stat cannot appear twice on one riven, and five are bonus-only
      // and can never be the malus.
      // A MALUS IS NOT ANY STAT and neither is a bonus: five roll as a bonus
      // only, and one melee stat rolls as the malus only.
      .filter((x) => (!used.has(x.id) || x.id === at.id)
        && (slot === "malus" ? x.malus : x.bonus !== false)
        && !(x.spliced && splicedElsewhere))
      // Both languages match, exactly as the mod picker does.
      .filter((x) => !f || `${rivenStatNameEn(x)} ${rivenStatName(x)}`.toLowerCase().includes(f))
      .map((x) => `<div class="opt ${x.id === at.id ? "search" : ""}" data-rvid="${x.id}">
        <div class="info"><div class="mn">${escHtml(rivenStatName(x))}</div>
        <div class="me">${x.spliced ? `<div>${escHtml(tr("spliced — for reference only"))}</div>` : ""}${x.modeled ? "" : `<div>${escHtml(tr("not modeled — it rolls and it names the riven, but it adds no damage"))}</div>`}${
          unconfirmed.includes(x.id) ? `<div>${escHtml(tr("unconfirmed — no card of this riven family says whether it rolls"))}</div>` : ""}</div></div>
      </div>`).join("") || `<div class="opt dis">${escHtml(tr("no matching stat"))}</div>`;
    menu.querySelectorAll("[data-rvid]").forEach((el) => el.onclick = () => {
      at.id = el.dataset.rvid;
      closePopovers();
      markRivenDirty();
      renderRivens();
    });
  };
  const r = anchor.getBoundingClientRect();
  pop.style.left = `${Math.max(8, Math.min(window.innerWidth - 340, r.left))}px`;
  pop.style.top = `${r.bottom + window.scrollY + 4}px`;
  pop.hidden = false;
  search.value = "";
  search.oninput = () => draw(search.value);
  draw("");
  search.focus();
}

function renderRivenFoot() {
  const rules = rivenRules();
  const pols = rules.polarities || ["madurai", "vazarin", "naramon"];
  // Polarity is a SYMBOL everywhere on this page; a name here would be the
  // one place it is spelled out.
  const cap = (s) => s.charAt(0).toUpperCase() + s.slice(1);
  $("riven-foot").innerHTML =
    `<label class="rv-lbl">${escHtml(tr("Rank"))} <input id="rv-rank" type="range" min="0" max="${rules.max_rank}" step="1" value="${riven.rank}">` +
    `<b id="rv-rank-n">${riven.rank}</b></label>` +
    `<span class="rv-lbl">${escHtml(tr("Polarity"))}</span><span class="oseg">` +
    pols.map((x) => `<span class="seg pol ${riven.polarity === x ? "on" : ""}" data-pol="${x}" title="${cap(x)}">${imgTag(POL(cap(x)), "pol")}</span>`).join("") +
    `</span>` +
    `<button class="ghost-btn small" id="rv-max">${escHtml(tr("roll everything maximal"))}</button>`;
  $("rv-rank").oninput = () => {
    riven.rank = Number($("rv-rank").value);
    $("rv-rank-n").textContent = riven.rank;
    markRivenDirty();
    resolveRiven();
  };
  $("riven-foot").querySelectorAll("[data-pol]").forEach((el) => el.onclick = () => {
    riven.polarity = el.dataset.pol; markRivenDirty(); renderRivenFoot(); resolveRiven();
  });
  // The ceiling, in one click: every bonus at the top of its band and the
  // malus at the bottom, which is the least harmful it can be.
  $("rv-max").onclick = () => {
    riven.bonuses.forEach((s) => { s.roll = rules.roll_max; });
    if (riven.malus) riven.malus.roll = rules.roll_min;
    markRivenDirty();
    renderRivens();
  };
}

// Every riven saved for this weapon, with what it actually rolls — the
// collection bar shows names, this shows the numbers you choose between. The values are the engine's, from the same refresh the
// mod lists use, so nothing here is a second opinion.
function renderRivenAll() {
  const box = $("riven-all");
  if (!box) return;
  const ps = loadPresetList(RIVENS);
  const active = activeRivenId();
  const cap = (s) => String(s || "").replace(/^./, (c) => c.toUpperCase());
  const weapon = (META.weapons || []).find((x) => x.id === $("weapon").value);
  // A stat may be saved as a bare id or as `{ id, roll }`; the auction search reads ids.
  const asStat = (x) => (typeof x === "string" ? { id: x } : x);
  box.innerHTML = ps.length
    ? ps.map((p) => {
        const st = p.state || {};
        const meta = rivenNames[p.id] || {};
        // THE SAME AUCTION SEARCH the equipped card carries, per saved riven.
        const wm = marketLink(rivenMarketUrl({
          bonuses: (st.bonuses || st.positives || []).map(asStat),
          malus: asStat(st.malus || st.curse || null),
        }, weapon), tr("find rivens with these stats on warframe.market"));
        const lines = meta.lines || [];
        const shape = st.shape || `${(st.bonuses || st.positives || []).length}${st.malus || st.curse ? "+1" : ""}`;
        const nBonus = (st.bonuses || st.positives || []).length;
        return `<div class="rv-all ${p.id === active ? "sel" : ""}" data-open="${escHtml(p.id)}">
          <div class="rv-all-h">
            ${imgTag(POL(cap(st.polarity || "madurai")), "pol")}
            <b>${escHtml(p.name)}</b>
            ${meta.name ? `<span class="rv-official">${escHtml(meta.name)}</span>` : ""}${wm}
            <span class="rv-meta">${shape} · ${escHtml(tr("Rank"))} ${st.rank ?? 8} · ${2 + 2 * (st.rank ?? 8)} ${escHtml(tr("capacity"))}</span>
          </div>
          <div class="rv-all-s">${
            lines.length
              ? lines.map((x, i) => `<span class="rv-chip ${i >= nBonus ? "neg" : ""}">${escHtml(tf(x))}</span>`).join("")
              : `<span class="sim-empty">${escHtml(tr("nothing rolled yet"))}</span>`
          }</div>
        </div>`;
      }).join("")
    : `<div class="sim-empty">${escHtml(tr("no rivens for this weapon yet"))}</div>`;
  // Clicking one opens it, the same as clicking its chip in the bar.
  // Clicking a card OPENS it — the list is the folder, this is the file.
  box.querySelectorAll("[data-open]").forEach((el) => el.onclick = () => {
    const p = loadPresetList(RIVENS).find((x) => x.id === el.dataset.open);
    if (!p) return;
    activeRiven = p.id;
    localStorage.setItem(presetActiveKey(RIVENS), activeRiven);
    riven = null;   // renderRivens re-reads the file it is being asked to open
    renderRivens();
  });
}

function renderRivenCard() {
  const r = rivenResolved;
  const box = $("riven-stats");
  // Blank every row first, so a slot that lost its stat does not keep an old
  // number sitting next to it.
  box.querySelectorAll(".rv-num").forEach((el) => { el.value = ""; el.disabled = true; });
  box.querySelectorAll(".rv-unit").forEach((el) => { el.textContent = ""; el.className = "rv-unit"; });
  box.querySelectorAll(".rv-pct").forEach((el) => { el.textContent = ""; el.className = "rv-pct"; });
  box.querySelectorAll(".rv-mult").forEach((el) => { el.textContent = ""; el.className = "rv-mult"; });
  (r && r.stats || []).forEach((s) => {
    const num = box.querySelector(`.rv-num[data-slot="${s.slot}"]`);
    const unit = box.querySelector(`.rv-unit[data-slot="${s.slot}"]`);
    const pct = box.querySelector(`.rv-pct[data-slot="${s.slot}"]`);
    const mult = box.querySelector(`.rv-mult[data-slot="${s.slot}"]`);
    // The CARD's precision, which is all anyone can read off a riven they
    // own. The roll behind it stays exact — this is the reading, not the
    // number the sim uses.
    const d = s.decimals ?? 2;
    if (num) {
      num.disabled = false;
      num.value = s.shown.toFixed(d);
      num.step = (0.1 ** d).toFixed(d);
      // The ends of the legal band, so the browser guards the box too.
      num.min = Math.min(s.min, s.max).toFixed(d);
      num.max = Math.max(s.min, s.max).toFixed(d);
      // In the box's OWN units: a faction stat holds a multiplier, so its
      // band reads x0.68 … x0.74 and never looks like a percentage.
      const u = (n) => (s.unit === "x" ? `x${n}` : `${n}${s.unit || ""}`);
      num.title = `legal range ${u(num.min)} … ${u(num.max)}`;
    }
    if (pct) {
      // How good the ROLL is, with disposition, shape and base divided out —
      // so two stats on one card compare, and so do two cards.
      const q = Math.round(s.percentile ?? 50);
      // A bare number in ANGLE brackets. An ordinal suffix read as clutter, and parentheses were taken — DE's own stat text
      // already uses them ("(x2 for Bows)").
      pct.textContent = `<${q}>`;
      pct.className = `rv-pct${q >= 90 ? " top" : ""}${q <= 10 ? " low" : ""}`;
      pct.title = tr("where this roll landed in its 0.9-1.1 band");
    }
    // ...and the ROLL ITSELF, next to it. The percentile says how good the
    // draw was, which is the reading you want when comparing two cards; the
    // multiplier is the number the game actually drew, which is the one you
    // can check against a riven you own (suggested by a player, 2026-08-03).
    // Both, because neither substitutes for the other: <100> and 1.100 are
    // the same fact, <50> and 1.000 are not obviously so, and a MALUS reads
    // backwards — its best draw is the SMALLEST multiplier.
    if (mult) {
      mult.textContent = Number(s.roll).toFixed(3);
      mult.className = "rv-mult";
      mult.title = tr("the roll itself — the random multiplier this stat drew, 0.900 to 1.100");
    }
    if (unit) {
      unit.textContent = tf(s.text);
      unit.className = `rv-unit${s.value < 0 ? " neg" : ""}${s.modeled ? "" : " unmodeled"}`;
    }
    const roll = box.querySelector(`.rv-roll[data-slot="${s.slot}"]`);
    if (roll) roll.value = s.roll;
  });
  const bad = (r && r.illegal) || [];
  if (!rivenComplete()) {
    const n = riven.bonuses.filter((s) => !s.id).length + (riven.malus && !riven.malus.id ? 1 : 0);
    const msg = n === 1
      ? tr("one more stat to finish this riven")
      : tr("pick {n} more stats to finish this riven").replace("{n}", n);
    $("riven-card").innerHTML = `<div class="rv-meta">${escHtml(msg)}</div>`;
    return;
  }
  $("riven-card").innerHTML = bad.length
    ? `<div class="error"><b>${escHtml(tr("not a legal riven"))}</b><ul>${bad.map((x) => `<li>${escHtml(x)}</li>`).join("")}</ul></div>`
    : `<div class="rv-name">${escHtml(r.name)}</div>
       <div class="rv-meta">${r.drain} ${escHtml(tr("capacity"))} · ${escHtml(tr(r.class))} ${escHtml(tr("riven"))} · ${escHtml(tr("disposition"))} ${Number(r.disposition).toFixed(2)}</div>`
      // LEGAL AND SAID SO: no card of the family settles these either way.
      + ((r.unconfirmed || []).length ? `<div class="rv-meta">${escHtml(tr("unconfirmed on this weapon: {stats}")
        .replace("{stats}", r.unconfirmed.map((id) => { const d = rivenStat(id); return d ? rivenStatName(d) : id; }).join(", ")))}</div>` : "")
      + rivenKinResolved.filter((k) => k && k.r && k.r.ok && !(k.r.illegal || []).length).map((k) =>
        `<div class="rv-kin"><span class="rv-meta">${escHtml(kinName(k.w))} · ${escHtml(tr("disposition"))} ${
          Number(k.r.disposition).toFixed(2)}</span><div class="rv-all-s">${(k.r.stats || []).map((x) =>
          `<span class="rv-chip ${x.value < 0 ? "neg" : ""}">${escHtml(tf(x.text))}</span>`).join("")}</div></div>`).join("");
}

// Rivens are a PRESET COLLECTION, on the same bar as builds and the
// optimizer's scopes: "+ new" is a new riven, ⧉ duplicates one to branch a
// roll, ✎ renames, ✕ deletes. Edits auto-save into the active riven, so
// there is no save button — the same contract as everywhere else.
const markRivenDirty = () => { if (typeof markPresetDirty === "function") markPresetDirty(); saveRivenSoon(); };
let rivenSaveTimer = null;
// A riven is CONSUMED by the builder (it is a mod in the pool), so deleting one
// can leave a slot pointing at an id that no longer exists — which nothing
// downstream says: the slot renders blank and the panel prices a build with a
// hole in it.
/// EVERY BUILD THAT NAMES THIS RIVEN, MOVED — or cleared when it is gone.
///
/// A riven's id IS its name (`riven:<name>`), so renaming and deleting are one
/// operation seen from a build. Touching the LIVE build only is too narrow — a
/// weapon's OTHER saved builds keep an id nothing resolves — and filing rivens
/// by FAMILY widens that across weapons.
///
/// THE SCOPE IS PASSED IN rather than derived here: rename and delete pass the
/// FAMILY's members, which is every weapon whose pool offers this card.
///
/// A SLOT IS RECOGNISED BY ITS `mod` and everything else by the exact string.
/// Clearing drops the RANK with it, which a blind string walk cannot do, while
/// anything storing the id some other way still gets swapped.
function repointRivenInBuilds(weapons, from, to) {
  const before = RIVEN_PREFIX + from;
  const after = to ? RIVEN_PREFIX + to : null;
  let moved = 0;
  const walk = (v) => {
    if (Array.isArray(v)) return v.map(walk);
    if (v && typeof v === "object") {
      if (v.mod === before) {
        moved++;
        return after ? { ...v, mod: after } : { ...v, mod: null, rank: null };
      }
      return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, walk(x)]));
    }
    return v === before ? (moved++, after) : v;
  };
  for (const id of weapons) {
    const key = `wfsim-presets-${id}-builder-builds`;
    const raw = localStorage.getItem(key);
    // The cheap gate: most weapons have never heard of this name, and parsing
    // every build list on every rename would be the expensive way to find out.
    if (!raw || raw.indexOf(before) < 0) continue;
    let list;
    try { list = JSON.parse(raw); } catch (_) { continue; }
    localStorage.setItem(key, JSON.stringify(walk(list)));
  }
  return moved;
}

/// The weapons that could be pointing at one of THIS weapon's rivens — every
/// entry filed under the same riven scope, which is what makes the card theirs
/// too. Derived from `rivenScope`, so a family that gains a variant tomorrow is
/// covered by nobody.
const rivenKinWeapons = () => {
  const scope = rivenScope($("weapon").value);
  return ((META && META.weapons) || [])
    .filter((w) => rivenScope(w.id) === scope).map((w) => w.id);
};

function pruneDanglingRivens() {
  let hit = false;
  slots.forEach((s) => {
    if (isRivenId(s.mod) && !modById(s.mod)) {
      s.mod = null; s.rank = null; hit = true;
    }
  });
  if (hit) { renderMods(); refreshPanel(); markPresetDirty(); }
}

function saveRivenSoon() {
  rivenSaveTimer = deferSave("rivens", () => {
    const ps = loadPresetList(RIVENS);
    const open = activeRivenId();
    const i = ps.findIndex((p) => p.id === open);
    if (i >= 0) {
      ps[i].state = snapshotRiven();
      storePresetList(RIVENS, ps);
      renderRivenTools();
      // The mod lists show each riven's generated name and printed values, so
      // they have to be re-asked for after an edit.
      refreshRivenNames();
    }
  }, 250);
}
// The saved shape keeps `bonuses`/`malus` at the top level — that is what
// the engine reads — and carries the other three drafts alongside so a
// reload does not flatten them back into one.
const snapshotRiven = () => ({
  bonuses: riven.bonuses, malus: riven.malus, rank: riven.rank, polarity: riven.polarity,
  shape: riven.shape, drafts: riven.drafts,
});
let activeRiven = null;
/// WHICH CARD IS OPEN, by id — two cards may share a name now, so a name could
/// not say which.
const activeRivenId = () => activeRiven || localStorage.getItem(presetActiveKey(RIVENS)) || "";

// A custom is a FILE, and the tools strip says which of two modes you are in:
// LIST, nothing open — a real state rather than an empty editor, since most
// weapons own no riven; and EDIT, one open, named, with "← back" closing it.
//
// Deliberately NOT the preset bar: a preset is a label you invented for a state
// your module is always in, while a riven is a thing with its own name,
// capacity and polarity that other modules consume.
