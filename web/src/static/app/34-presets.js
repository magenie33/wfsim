// ---- Presets ----------------------------------------------------------
// Every preset collection is owned by one of the three modules or by an EDITOR
// that feeds them, and its DOMAIN id is "<owner>-<collection>" — `rivens` alone
// is the owner name by itself, because that editor is ALL collection and there
// is no second one to tell it apart from. Every durable name derives from the
// domain mechanically, and full words only:
//
//   localStorage  wfsim-presets-<weapon>-<domain>        the list
//                 wfsim-preset-active-<weapon>-<domain>  the active pointer
//   DOM id        preset-bar-<domain>
//
// A preset BELONGS TO ONE WEAPON, so the STORAGE key carries the weapon while
// the domain still names the collection — a weapon-less key is one global list
// where edits on the Laetum show up on the Dual Toxocyst. There is no copy
// ACROSS weapons: what survives a rescope is the mods every gun shares, and a
// build worth having comes from the board or from a share link. No count cap:
// presets live in the reader's localStorage rather than with us.
const presetWeapon = () => ($("weapon") && $("weapon").value) || "";
// PRESETS vs CUSTOMS — two kinds of collection, and the difference is who
// consumes them.
//
// A PRESET is a saved state of something that always exists: the builder
// always has a build, the simulator a fight, the optimizer a search. Only its
// own module reads it, there is always at least one, and "active" means the
// state you are currently in.
//
// A CUSTOM is a thing you MADE, and the other modules consume it: a riven
// becomes a mod in the pool, a custom enemy becomes an entry in the scenario's
// enemy list. Owning none is ordinary, each one carries its own identity
// rather than a label you invented, and deleting one breaks references
// elsewhere — which a preset delete can never do. The mental model is a FILE:
// it sits in a list, you open one to edit it, and you can have none open.
//
// Different noun, different key. Everything BELOW the key is shared —
// storage, undo, per-weapon scoping — because none of that depends on which
// kind it is.
const CUSTOM_DOMAINS = new Set(["rivens", "enemies"]);
const isCustomDomain = (d) => CUSTOM_DOMAINS.has(d);

// …AND ONE COLLECTION THAT IS NOT A WEAPON'S: the FIGHT.
//
// The enemy, its level, Steel Path, the wielder's state, the duration and how
// many runs describe a FIGHT, and a fight is not about any particular gun —
// which the OFFICIAL rulers always were: one `single_target` applies to every
// weapon on the board.
//
// It NARROWS "nothing crosses between weapons" rather than weakening it. That
// rule exists because a BUILD, a SEARCH and a RIVEN are statements about one
// weapon, and inheriting the last weapon's is how you measure a gun you are not
// looking at. A fight is no such statement, so there is nothing to inherit
// wrongly — and the one weapon-scoped knob it still holds, headshot %, is
// handled the way the rulers handle it: the SERVER forces 0 on a weapon that
// cannot headshot.
// …and a TARGET is not a weapon's either, for exactly the reason a fight is
// not: an enemy you built has no opinion about what is shooting it. Same
// consequence — one list for the whole roster.

/// …and a RIVEN, which is ABOUT a family and not FILED under one.
///
/// A STORE IS ADDRESSED BY WHAT IT IS AND FILTERED BY WHAT IT IS ABOUT.
/// Content data has no business being a storage address: a key computed from
/// `riven_family` and the mod class makes correcting either one MOVE everybody's
/// saved cards to an address the page no longer computes. So the store is one
/// list, the card carries its own `scope`, and changing a family does exactly
/// what it says — it changes which weapons the card appears under.
const SHARED_DOMAINS = new Set(["simulator-scenarios", "enemies", "rivens", "operators"]);
const isSharedDomain = (d) => SHARED_DOMAINS.has(d);

/// WHOSE RIVEN THIS IS — the weapon FAMILY, never the entry: *"Riven mods can
/// be used on variants of a particular weapon, including MK1, Prime, Vandal,
/// Wraith, Dex, Prisma, Mara, and Syndicate variants"* (wiki `Riven Mods`).
/// Filing it under the weapon makes a player build the same card twice and
/// gives them two cards free to drift apart.
///
/// THE NUMBERS FOLLOW BY THEMSELVES, which is why this is a storage change
/// rather than a feature: a saved riven holds ROLLS on 0..1 and the shown value
/// is `roll` against THIS weapon's disposition, computed by `/api/riven` on
/// every render. So one card reads 1.45's worth on a Burston and 1.35's on its
/// Prime with nothing converted — *"the cycling screen allows players to view
/// the Riven stats on every owned variant of said weapon"*. A weapon that
/// declares no family is its own and keeps the key it already had.
///
/// THE RIVEN CLASS IS PART OF THE SCOPE, so a family whose members took two
/// kinds of card would keep them apart. A KITGUN does not: its two slots take
/// the chamber's one pistol riven (`riven_class` in the weapon data), so both
/// land in one scope. An engine test holds the other half: every weapon
/// sharing a (family, class) rolls the same pool.
const rivenSlug = (x) => String(x || "")
  .toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "");
const rivenScope = (id) => {
  // NOT `weaponInfo`: that falls back to the FIRST weapon in the roster for an
  // id it does not know, which would file a riven under a stranger's family.
  const w = ((META && META.weapons) || []).find((x) => x.id === id);
  const fam = rivenSlug((w && w.riven_family) || id);
  const cls = rivenSlug(w && (w.riven_class || w.mod_class));
  return cls ? `${fam}-${cls}` : fam;
};
/// Every scope a riven can legitimately be filed under today.
const rivenScopes = () =>
  new Set(((META && META.weapons) || []).map((w) => rivenScope(w.id)));

const domainScope = (d, w) => {
  if (isSharedDomain(d)) return "";
  return (w ?? presetWeapon()) + "-";
};
const presetListKey = (d, w) =>
  (isCustomDomain(d) ? "wfsim-customs-" : "wfsim-presets-") + domainScope(d, w) + d;
/// …AND WHICH ONE IS OPEN IS THE FOLDER'S, even where the store is not.
///
/// A riven's cards live in one list, but "the card I am looking at" is a
/// property of the weapon in front of you — leaving Burston Prime and coming
/// back should find the same one open, and going to a Laetum should not. This
/// is the ONE riven key still built from the scope, and it is the one place
/// that is safe: a scope that moves loses a pointer, and the worst a lost
/// pointer does is open nothing.
const presetActiveKey = (d, w) =>
  (isCustomDomain(d) ? "wfsim-custom-open-" : "wfsim-preset-active-")
  + (d === RIVENS ? `${rivenScope(w ?? presetWeapon())}-` : domainScope(d, w)) + d;

// ONE-TIME MERGE of every weapon's scenario list into the shared one. A player
// who made a fight on the Torid must not have to make it again — and the lists
// are additive, so this reads them all and keeps every entry, renaming a
// collision rather than dropping either side.
(function mergeScenarioLists() {
  const D = "simulator-scenarios";
  const shared = presetListKey(D);
  const per = [];
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i);
    const m = k && /^wfsim-presets-(.+)-simulator-scenarios$/.exec(k);
    if (m) per.push([k, m[1]]);
  }
  if (!per.length) return;
  let out = [];
  try { out = JSON.parse(localStorage.getItem(shared) || "[]") || []; } catch (_) { out = []; }
  const seen = new Set(out.map((p) => JSON.stringify(p.state)));
  for (const [k, weapon] of per) {
    let list = [];
    try { list = JSON.parse(localStorage.getItem(k) || "[]") || []; } catch (_) { list = []; }
    for (const p of list) {
      // IDENTICAL FIGHTS COLLAPSE. Most players' per-weapon copies are the same
      // fight made twice, and carrying six of them across would turn a merge
      // into a mess the player has to clean up.
      const sig = JSON.stringify(p.state);
      if (seen.has(sig)) continue;
      seen.add(sig);
      let name = p.name;
      if (out.some((q) => q.name === name)) name = `${name} (${weapon})`;
      out.push({ ...p, name });
    }
    localStorage.removeItem(k);
    localStorage.removeItem(`wfsim-preset-active-${weapon}-${D}`);
  }
  if (out.length) localStorage.setItem(shared, JSON.stringify(out));
})();

// One-time rename of every custom collection out of the preset namespace.
// The data is unchanged; only the noun was wrong.
(function migrateCustomKeys() {
  const moves = [];
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i);
    let m = /^wfsim-presets-(.+)-([^-]+)$/.exec(k);
    if (m && isCustomDomain(m[2])) moves.push([k, `wfsim-customs-${m[1]}-${m[2]}`]);
    m = /^wfsim-preset-active-(.+)-([^-]+)$/.exec(k);
    if (m && isCustomDomain(m[2])) moves.push([k, `wfsim-custom-open-${m[1]}-${m[2]}`]);
  }
  moves.forEach(([from, to]) => {
    const v = localStorage.getItem(from);
    if (v !== null && localStorage.getItem(to) === null) localStorage.setItem(to, v);
    localStorage.removeItem(from);
  });
})();
/// ONE-TIME FOLD of every riven list this app has ever written into the ONE
/// list, tagging each card with the scope it was filed under. It RUNS AFTER
/// `META`, because a scope is something only the roster knows.
///
/// IT ALSO GIVES EVERY CARD ITS IDENTITY. A build referenced a riven by NAME
/// once, so this is where `riven:<name>` becomes `riven:<id>` — resolved per
/// old key, which is what makes it unambiguous: two variants of one family
/// could each hold a "riven 1", and only the key says whose build meant which.
/// After this a name is a label and nothing points at it.
function foldRivensIntoOneList() {
  const KEY = "wfsim-customs-rivens";
  const olds = [];
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i) || "";
    const m = /^wfsim-customs-(.+)-rivens$/.exec(k);
    if (m) olds.push(m[1]);
  }
  const live = rivenScopes();
  const roster = new Set(((META && META.weapons) || []).map((w) => w.id));
  const bySlug = new Map();
  for (const sc of live) bySlug.set(rivenSlug(sc), sc);
  // THE TOKEN IS LOOKED UP, NOT PARSED. Three key shapes have existed and no
  // pattern can tell them apart — a migration that tried matched the key it
  // WROTE as well as the one it read, moved every good list onto a dead key and
  // deleted the original. A token nothing answers to keeps its cards.
  const resolve = (token) => {
    if (live.has(token)) return token;                 // already a scope
    if (roster.has(token)) return rivenScope(token);   // the pre-family shape
    return bySlug.get(token) || token;                 // the slugged one, or itself
  };
  const read = (k) => {
    try { return JSON.parse(localStorage.getItem(k) || "[]") || []; } catch (_) { return []; }
  };
  const all = read(KEY);
  let touched = olds.length > 0;
  const same = (p, q) => q.name === p.name
    && JSON.stringify(q.state) === JSON.stringify(p.state);
  // WHOSE BUILDS POINTED AT THIS CARD BY NAME: the one weapon whose list is
  // moving when the token names one, and otherwise the family it resolves to.
  const owners = (token, scope) => (roster.has(token)
    ? [token]
    : ((META && META.weapons) || []).filter((w) => rivenScope(w.id) === scope).map((w) => w.id));
  for (const token of olds) {
    const scope = resolve(token);
    for (const p of read(`wfsim-customs-${token}-rivens`)) {
      // THE SAME CARD FROM TWO OLD KEYS IS ONE CARD: a player who built the
      // Burston's and the Prime's separately built one riven, and both keys
      // resolve to the same scope now.
      const twin = all.find((q) => (q.scope || "") === scope && same(p, q));
      if (twin) {
        if (twin.id) repointRivenInBuilds(owners(token, scope), p.name, twin.id);
        continue;
      }
      const id = p.id || newRivenId(all);
      if (!p.id) repointRivenInBuilds(owners(token, scope), p.name, id);
      all.push({ ...p, id, scope });
    }
    localStorage.removeItem(`wfsim-customs-${token}-rivens`);
    // WHICH CARD IS OPEN stays scoped, and it is the one thing here that may
    // safely miss: the worst a stale pointer does is open nothing.
    const from = `wfsim-custom-open-${token}-rivens`;
    const to = `wfsim-custom-open-${scope}-rivens`;
    const open = localStorage.getItem(from);
    if (open !== null && from !== to) {
      if (localStorage.getItem(to) === null) localStorage.setItem(to, open);
      localStorage.removeItem(from);
    }
  }
  // A SCOPE THE ROSTER NO LONGER COMPUTES MOVES TO ITS FAMILY'S ONE SCOPE — a
  // primary Kitgun's card was filed under its own class, and a chamber has one
  // card. A family with two live scopes is ambiguous and keeps what it has.
  const familyOf = (sc) => sc.replace(/-[^-]*$/, "");
  for (const p of all) {
    if (!p.scope || live.has(p.scope)) continue;
    const to = [...live].filter((sc) => familyOf(sc) === familyOf(p.scope));
    if (to.length !== 1) continue;
    const from = `wfsim-custom-open-${p.scope}-rivens`;
    const open = localStorage.getItem(from);
    if (open !== null && localStorage.getItem(`wfsim-custom-open-${to[0]}-rivens`) === null) {
      localStorage.setItem(`wfsim-custom-open-${to[0]}-rivens`, open);
    }
    localStorage.removeItem(from);
    p.scope = to[0];
    touched = true;
  }
  // …AND THE CARDS ALREADY IN THE ONE LIST, which is every card once the loop
  // above has run and the whole store on a second visit. A card with no id
  // predates identities and its scope's builds still name it.
  for (const p of all) {
    if (p.id) continue;
    p.id = newRivenId(all);
    repointRivenInBuilds(owners("", p.scope || ""), p.name, p.id);
    touched = true;
  }
  if (touched) localStorage.setItem(KEY, JSON.stringify(all));
}

// Parsed lists, memoised on the RAW STRING. The stored text IS the
// invalidation — nothing to keep in sync, and a stale read is impossible.
// Worth having because `gainKey()` resolves a whole scenario and is called
// from a sort comparator, i.e. O(n log n) times per picker render.
const presetParseCache = new Map();
const loadPresetWhole = (d, w) => {
  const k = presetListKey(d, w);
  let raw;
  try { raw = localStorage.getItem(k); } catch (_) { return []; }
  const hit = presetParseCache.get(k);
  if (hit && hit.raw === raw) return hit.list;
  let list = [];
  try { const p = JSON.parse(raw); if (Array.isArray(p)) list = p; } catch (_) { /* empty */ }
  presetParseCache.set(k, { raw, list });
  return list;
};
/// WHAT THIS WEAPON CAN SEE. Every other collection is a whole key; a riven's
/// store holds the roster's and the QUERY is what makes it this weapon's, which
/// is the whole point of filing it by what it IS. Callers are unchanged: they
/// asked for "this weapon's list" before and they still get one.
const loadPresetList = (d, w) => {
  const list = loadPresetWhole(d, w);
  if (d !== RIVENS) return list;
  const scope = rivenScope(w ?? presetWeapon());
  return list.filter((p) => (p.scope || "") === scope);
};
// The first "<thing> N" this collection does not already hold. Shared by both
// kinds — naming a new item is the same problem whatever it is called.
const freeName = (ps, mk) => {
  for (let n = 1; ; n++) { const nm = mk(n); if (!ps.some((p) => p.name === nm)) return nm; }
};
/// THE NAME "+ new" GIVES A PRESET, written down once.
const autoPresetName = (noun, n) => `${noun} ${n}`;
/// EVERY PRESET IS BORN "preset N", WHATEVER IT IS A PRESET OF: a build, a
/// search, a fight and an Operator build are one concept on this site, so they
/// carry one name. The noun (`cfg.noun`) only words a tooltip. A custom (a
/// riven, an enemy) is a different kind of thing and keeps its own.
const PRESET_NAME = "preset";
/// THE FIRST PRESET OF A COLLECTION HAS ONE IDENTITY, WRITTEN OR NOT. The bar
/// draws it before anything is stored (a virtual "preset 1"), and it is stored
/// under this id on the first real edit — so a link to it, made while it was
/// only drawn, is a link to the stored one afterwards. No link ever names "no
/// preset".
const PRESET_SEED_ID = "preset-1";
/// WHICH PRESET A PAGE OPENS ON: the one `?build=` names — else the first, the
/// same repair a link gets when its preset is gone — and with none named, the
/// last one open. `null` is the virtual one.
const presetToOpen = (list, want, last) => (want
  ? list.find((x) => x.id === want) || list[0] || null
  : list.find((x) => x.name === last) || list[0] || null);
const newPresetName = (ps) => freeName(ps, (n) => autoPresetName(PRESET_NAME, n));
/// …and whether a name is one. It ASKS the generator rather than matching a
/// shape of its own, so changing the shape above cannot leave this behind.
///
/// A share link needs the answer: a name nobody typed means nothing to the
/// reader, `importShare` names an unnamed build anyway, and the SPACE in it
/// costs the payload its compact form entirely.
const isAutoPresetName = (noun, name) => {
  const s = String(name || "");
  const n = Number(s.slice(String(noun).length + 1));
  return Number.isInteger(n) && n >= 1 && s === autoPresetName(noun, n);
};
/// Whether a name is one the app generated — "preset N", or the per-collection
/// nouns a stored preset may still carry.
const isGeneratedName = (name) => [PRESET_NAME, "build", "search", "scenario", "operator"]
  .some((noun) => isAutoPresetName(noun, name));
/// The builds collection's noun, which words its tooltips.
const BUILD_NOUN = "build";

/// WHAT A COLLECTION SHEDS when it will not fit, in the order of what it costs
/// the reader to lose. Each stage is applied, the write retried, and the first
/// one that fits wins.
///
/// localStorage is a few megabytes and this app stores MEASUREMENTS in it, so
/// "it does not fit" is a state a long-lived install reaches. Throwing there is
/// the worst outcome available: the edit is on screen and never persisted.
/// THE ACTIVE PRESET'S RESULT IS NEVER SHED: it is what the write is usually
/// FOR, and dropping it throws away the run that just finished — after which
/// the first thing to re-read the collection hides the whole block.
///
/// A REPLAY NEVER REACHES THE DISK.
///
/// It is the biggest thing this app produces by a wide margin — 600 frames of
/// debuff series per followed body — and the ONE part of a result a button
/// regenerates. Stripped on the way to `localStorage` and kept in `resultMem`
/// for the session, which makes the stored footprint of a measurement a few
/// kilobytes of summary and BOUNDED. Not the shed being tidy: shedding responds
/// to a full disk, this is why it stops being full.
const stripReplays = (ps) => ps.map((x) => (x && x.lastResult && x.lastResult.r
  && x.lastResult.r.replay
  ? { ...x, lastResult: { ...x.lastResult, r: { ...x.lastResult.r, replay: null } } }
  : x));

/// EVERY WFSIM KEY, because a quota is the ORIGIN's and a shed was the list's.
///
/// This is what the first version got wrong. Writing
/// `wfsim-presets-phantasma_prime-builder-builds` would fail on a disk filled
/// by `wfsim-presets-boar_prime-builder-builds`, shed its own list down to
/// nothing, still not fit, and give up — while the space it needed sat in
/// another weapon's key that nothing was ever going to look at. So a shed
/// sweeps the ORIGIN, and the list being written is simply the last one it is
/// allowed to touch.
const otherPresetKeys = (skip) => Object.keys(localStorage)
  .filter((k) => k.startsWith("wfsim-presets-") && k !== skip);

/// Drop `lastResult` from every OTHER collection, oldest first, until the write
/// fits. A card falls back to "not measured yet", which is true and is one
/// click from false — and it is the right thing to lose, because it is a
/// measurement of a build the reader is not looking at.
function shedOtherResults(skip) {
  const rows = [];
  for (const k of otherPresetKeys(skip)) {
    let list = null;
    try { list = JSON.parse(localStorage.getItem(k)); } catch (_) { continue; }
    if (!Array.isArray(list)) continue;
    list.forEach((x, i) => {
      if (x && x.lastResult) rows.push({ k, i, at: x.lastResult.at || 0, list });
    });
  }
  rows.sort((a, b) => a.at - b.at);
  return rows.map((row) => () => {
    row.list[row.i].lastResult = null;
    try { localStorage.setItem(row.k, JSON.stringify(row.list)); } catch (_) {}
  });
}

/// …and the ladder for the list actually being written, outside in. The active
/// preset's own result is the LAST thing to go: it is usually what the write is
/// FOR, and dropping it means the run that just finished was thrown away by the
/// act of saving it.
const PRESET_SHED = [
  (ps, active) => ps.forEach((x) => { if (x.name !== active) x.lastResult = null; }),
  (ps, active) => ps.forEach((x) => { if (x.name === active) x.lastResult = null; }),
];

/// A ONE-LINE NOTICE, in the page. No native dialog — `alert` is blocked in the
/// owner's browser, and a message nobody can see is not a message. It replaces
/// itself, so a repeated failure is one line rather than a stack.
function noteInline(msg) {
  let el = document.getElementById("page-note");
  if (!el) {
    el = document.createElement("div");
    el.id = "page-note";
    el.className = "page-note";
    el.addEventListener("click", () => el.remove());
    document.body.appendChild(el);
  }
  el.textContent = msg;
}

const storePresetList = (d, ps, w) => {
  const weapon = w ?? presetWeapon();
  const key = presetListKey(d, weapon);
  // A RIVEN WRITE IS A REPLACEMENT OF THIS SCOPE'S SLICE, not of the file: the
  // caller was handed this weapon's cards and hands them back, and every other
  // family's sit in the same list untouched. The tag is re-applied on the way
  // in so a card copied from another weapon lands under the one it is being
  // saved for.
  if (d === RIVENS) {
    const scope = rivenScope(weapon);
    ps = loadPresetWhole(d, weapon).filter((p) => (p.scope || "") !== scope)
      .concat(ps.map((p) => ({ ...p, scope })));
  }
  const isQuota = (e) => !!e && (e.name === "QuotaExceededError"
    || e.name === "NS_ERROR_DOM_QUOTA_REACHED" || e.code === 22 || e.code === 1014);
  // The REPLAY never travels. `ps` itself keeps it, because the caller and
  // `resultMem` are still holding that object and the panel reads it.
  const flat = stripReplays(ps);
  // UNDO COMPARES LIKE WITH LIKE. Its "is this a no-op write" test is against
  // what is ON THE DISK, which now never has a replay — so a `ps` that still
  // carried one would differ from the stored copy every single time and push a
  // step that undoes nothing.
  recordUndo(d, weapon, flat);
  const put = () => {
    try {
      localStorage.setItem(key, JSON.stringify(flat));
      return true;
    } catch (e) {
      if (!isQuota(e)) throw e;
      return false;
    }
  };
  if (put()) return;
  // OTHER COLLECTIONS FIRST — see `shedOtherResults`. A quota is the origin's,
  // so the space this write needs is usually not in this write's list.
  for (const drop of shedOtherResults(key)) {
    drop();
    if (put()) return;
  }
  for (const shed of PRESET_SHED) {
    shed(flat, activePreset);
    if (put()) return;
  }
  // NOTHING LEFT TO DROP. Say so where the reader is rather than throwing into a
  // console nobody has open: the edit is on screen and it is not saved, and that
  // is the one thing they need to know.
  noteInline(tr("this browser's storage is full - the change is on screen but was not saved"));
};

