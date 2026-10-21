// ---- Presets ----------------------------------------------------------
// Every preset collection is owned by one of the three modules or by an EDITOR
// that feeds them, and its DOMAIN id is "<owner>-<collection>" — `rivens` alone
// is the owner name by itself, because that editor is ALL collection and there
// is no second one to tell it apart from. Every durable name derives from the
// domain mechanically, and full words only:
//
//   localStorage  wfsim-presets-<domain>                 the list (customs: wfsim-customs-)
//                 wfsim-preset-active-<weapon>-<domain>  the active pointer
//   DOM id        preset-bar-<domain>
//
// ONE LIST PER COLLECTION, and each entry carries its `scope` — the weapon,
// frame, companion or riven family it is about (`entryScope`). A preset still
// BELONGS TO ONE WEAPON: the READ filters by scope, so edits on the Laetum never
// show up on the Dual Toxocyst. There is no copy ACROSS weapons: what survives a
// rescope is the mods every gun shares, and a build worth having comes from the
// board or from a share link. No count cap: presets live in the reader's
// localStorage rather than with us.
const presetWeapon = () => ($("weapon") && $("weapon").value) || "";
/// EVERY SAVED COLLECTION, declared once — docs/UI.md §"Presets and customs".
/// `kind` is which of the two it is: a PRESET is a saved state of a module that
/// always has one, read by that module alone; a CUSTOM is a thing you made that
/// OTHER modules consume, so owning none is ordinary and deleting one breaks
/// references. `scope` is what one entry is ABOUT — a weapon, a riven family, a
/// frame, a companion, or nothing (`global`). Anything new a reader can save is
/// one more row, and storage, undo, sync and the counts read it.
const COLLECTIONS = [
  { domain: "builder-builds", kind: "preset", scope: "weapon" },
  { domain: "simulator-scenarios", kind: "preset", scope: "global" },
  { domain: "optimizer", kind: "preset", scope: "weapon" },
  { domain: "warframes", kind: "preset", scope: "frame" },
  { domain: "companions", kind: "preset", scope: "companion" },
  { domain: "operators", kind: "preset", scope: "global" },
  { domain: "rivens", kind: "custom", scope: "riven_family", ref: "riven:" },
  { domain: "enemies", kind: "custom", scope: "global", ref: "custom:" },
];
const CUSTOM_DOMAINS = new Set(COLLECTIONS.filter((c) => c.kind === "custom").map((c) => c.domain));
const isCustomDomain = (d) => CUSTOM_DOMAINS.has(d);

/// HOW AN ENTRY NAMES A CUSTOM: `ref` + the custom's id, wherever it sits in
/// the entry. A custom declares its `ref` above and sync carries it with what
/// names it, and a page that cannot resolve it holds it (`holdAbsent`).
const CUSTOM_REFS = COLLECTIONS.filter((c) => c.ref);
const CUSTOM_REF_RE = new RegExp(`"((?:${CUSTOM_REFS.map((c) => c.ref).join("|")})[^"\]+)"`, "g");
/// Every custom reference in `v`, each once: `[{ domain, id, ref }]`.
function customRefs(v) {
  const out = new Map();
  let s;
  try { s = JSON.stringify(v); } catch (_) { return []; }
  for (const m of s.matchAll(CUSTOM_REF_RE)) {
    const c = CUSTOM_REFS.find((x) => m[1].startsWith(x.ref));
    if (c && !out.has(m[1])) out.set(m[1], { domain: c.domain, id: m[1].slice(c.ref.length), ref: m[1] });
  }
  return [...out.values()];
}
/// Whether this browser holds the custom `ref` names.
function customHeld(ref) {
  const c = CUSTOM_REFS.find((x) => ref.startsWith(x.ref));
  if (!c) return false;
  let ps;
  try { ps = JSON.parse(localStorage.getItem(presetListKey(c.domain))); } catch (_) { return false; }
  const id = ref.slice(c.ref.length);
  return Array.isArray(ps) && ps.some((p) => p && p.id === id);
}

/// A REFERENCE THIS BROWSER CANNOT RESOLVE IS HELD, NEVER DROPPED. The page
/// stands something in for it (`stand`) and the save puts the reference back
/// wherever the stand-in is still standing: a synced entry opened before its
/// custom arrived would otherwise be saved without it, and sync would carry
/// that loss to every browser. docs/UI.md §"Build sync".
const absentHolds = new Map();
/// Hold `ref` at `path` in `entry` of `domain`, with `stand` standing in for it.
function holdAbsent(domain, entry, path, ref, stand) {
  let h = absentHolds.get(domain);
  if (!h || h.entry !== entry) absentHolds.set(domain, h = { entry, list: [] });
  h.list.push({ path, ref, stand });
}
const releaseAbsent = (domain) => absentHolds.delete(domain);
/// What `entry` of `domain` holds — nothing for any other entry, so a hold
/// never follows the reader onto the next one.
const absentOf = (domain, entry) => {
  const h = absentHolds.get(domain);
  return h && h.entry === entry ? h.list : [];
};
const absentAt = (domain, entry, path) => absentOf(domain, entry).find((h) => h.path.join("/") === path.join("/"));
/// `state` with each held reference back where its stand-in still is. One the
/// reader has replaced is theirs to replace, and the hold ends.
function keepAbsent(domain, entry, state) {
  const hs = absentOf(domain, entry);
  if (!hs.length) return state;
  const out = JSON.parse(JSON.stringify(state));
  absentHolds.get(domain).list = hs.filter((h) => {
    let o = out;
    for (const k of h.path.slice(0, -1)) o = o && o[k];
    const last = h.path[h.path.length - 1];
    if (!o || typeof o !== "object" || (o[last] ?? null) !== h.stand) return false;
    o[last] = h.ref;
    return true;
  });
  return out;
}

// …AND ONE COLLECTION THAT IS NOT A WEAPON'S: the FIGHT.
//
// The enemy, its level, Steel Path, the wielder's state, the duration and how
// many runs describe a FIGHT, and a fight is not about any particular gun —
// which the OFFICIAL rulers always were: one `standard_single_target` applies to every
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
/// ONE LIST FOR THE ROSTER where an entry is about no weapon of its own — and a
/// riven's family, which its card carries as `scope` (below).
const SHARED_DOMAINS = new Set(COLLECTIONS.filter((c) => c.scope === "global" || c.scope === "riven_family")
  .map((c) => c.domain));
const isSharedDomain = (d) => SHARED_DOMAINS.has(d);

/// WHOSE RIVEN THIS IS — the weapon FAMILY, never the entry: *"Riven mods can
/// be used on variants of a particular weapon, including MK1, Prime, Vandal,
/// Wraith, Dex, Prisma, Mara, and Syndicate variants"* (wiki `Riven Mods`).
/// Filing it under the weapon makes a player build the same card twice and
/// gives them two cards free to drift apart.
///
/// THE NUMBERS FOLLOW BY THEMSELVES, which is why this is a storage change
/// rather than a feature: a saved riven holds ROLLS and the shown value is
/// computed from them by `/api/riven` on every render, so one card reads the
/// same on a Burston and its Prime with nothing converted — *"the cycling
/// screen allows players to view the Riven stats on every owned variant of
/// said weapon"*. A weapon that declares no family is its own and keeps the
/// key it already had.
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
/// WHAT AN ENTRY OF THIS COLLECTION SAYS IT IS ABOUT, for the owner `w` (the
/// open weapon by default) — `null` for a collection about nothing, whose list
/// is read whole.
const entryScope = (d, w) => {
  const c = COLLECTIONS.find((x) => x.domain === d);
  if (c && c.scope === "global") return null;
  if (d === RIVENS) return rivenScope(w ?? presetWeapon());
  return w ?? presetWeapon();
};
/// THE STORE IS THE COLLECTION'S, never an owner's: an owner in the key made a
/// store per weapon, which is a list per weapon to count, sync and migrate.
const presetListKey = (d) => (isCustomDomain(d) ? "wfsim-customs-" : "wfsim-presets-") + d;
/// The collections whose list was filed per owner, as `wfsim-presets-<owner>-<domain>`.
const OWNER_KEYED = COLLECTIONS.filter((c) => c.scope === "weapon" || c.scope === "frame"
  || c.scope === "companion").map((c) => c.domain);
/// A per-owner list's key, read into `{ domain, owner }` — or null.
const ownerKeyed = (k) => {
  for (const d of OWNER_KEYED) {
    const m = new RegExp(`^wfsim-presets-(.+)-${d}$`).exec(k);
    if (m) return { domain: d, owner: m[1] };
  }
  return null;
};
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

/// EVERY STORED ENTRY CARRIES AN `id`, and the id is what makes it the same
/// entry on another device: every device has a "preset 1", so a name cannot
/// be what a sync matches on. Opaque, never shown, never changed by a rename.
/// `randomUUID` exists only in a secure context; the fallback is as unique here.
const presetNewId = () => (crypto.randomUUID ? crypto.randomUUID()
  : Date.now().toString(36) + Math.random().toString(36).slice(2));
/// Gives each entry that has none, or shares one with an earlier entry, an id
/// of its own — IN PLACE, because the caller goes on holding these objects.
/// True when it minted any. RIVENS ARE NOT MINTED HERE: a card's id is what a
/// build's slot names, and `foldRivensIntoOneList` mints it together with
/// repointing those slots.
function mintPresetIds(ps) {
  const seen = new Set();
  let minted = false;
  for (const p of ps) {
    if (!p || typeof p !== "object") continue;
    if (!p.id || seen.has(p.id)) { p.id = presetNewId(); minted = true; }
    seen.add(p.id);
  }
  return minted;
}
const isRivenListKey = (k) => /^wfsim-customs-(.+-)?rivens$/.test(k);
/// A NEW ENTRY, with its id from the start: everything that points at an entry
/// — the open one, a link, a fight's target, a seat — points by id.
const presetEntry = (name, state) => ({ id: presetNewId(), name, savedAt: Date.now(), state });
/// THE ENTRY A POINTER NAMES: by id (or an official entry's `builtin`), and by
/// name only for what an older page stored or a caller typed.
const presetFind = (list, key) => (key
  ? list.find((p) => p && (p.builtin || p.id) === key) || list.find((p) => p && p.name === key) || null
  : null);
/// ONE-TIME MINT over every stored list, before anything reads one: undo
/// snapshots the raw stored text, and an undo back to a list without ids
/// would mint different ones — the same entry under a new identity.
(function mintStoredPresetIds() {
  for (const k of Object.keys(localStorage)) {
    if (!/^wfsim-(presets|customs)-/.test(k) || isRivenListKey(k)) continue;
    let list;
    try { list = JSON.parse(localStorage.getItem(k)); } catch (_) { continue; }
    if (!Array.isArray(list) || !mintPresetIds(list)) continue;
    try { localStorage.setItem(k, JSON.stringify(list)); } catch (_) { /* minted again next load */ }
  }
})();
/// ONE-TIME: every stored pointer to the open entry, from the NAME an older page
/// wrote to the entry's id. A riven's pointer was always an id.
(function pointersToIds() {
  for (const k of Object.keys(localStorage)) {
    const m = /^wfsim-(preset-active|custom-open)-(.+)$/.exec(k);
    if (!m) continue;
    let ps;
    try { ps = JSON.parse(localStorage.getItem((m[1] === "preset-active" ? "wfsim-presets-" : "wfsim-customs-") + m[2])); } catch (_) { continue; }
    const v = localStorage.getItem(k);
    if (!Array.isArray(ps) || !v || ps.some((p) => p && (p.id === v || p.builtin === v))) continue;
    const hit = ps.find((p) => p && p.name === v);
    if (hit && hit.id) localStorage.setItem(k, hit.id);
  }
})();
/// ONE-TIME: a weapon's Forma group ("plan together") listed its builds by NAME;
/// it lists them by id. A Warframe's group always did.
(function formaGroupsToIds() {
  for (const k of Object.keys(localStorage)) {
    const m = /^wfsim-forma-group-(.+)$/.exec(k);
    if (!m || m[1].startsWith("warframe-")) continue;
    let g, ps;
    try { g = JSON.parse(localStorage.getItem(k)); ps = JSON.parse(localStorage.getItem(`wfsim-presets-${m[1]}-builder-builds`)); } catch (_) { continue; }
    if (!Array.isArray(g) || !Array.isArray(ps)) continue;
    const out = g.map((v) => (ps.some((p) => p && (p.id === v || p.builtin === v)) ? v
      : ((ps.find((p) => p && p.name === v) || {}).id || v)));
    if (JSON.stringify(out) !== JSON.stringify(g)) localStorage.setItem(k, JSON.stringify(out));
  }
})();
/// ONE-TIME: a fight named its custom target `custom:<name>`, and now names it
/// `custom:<id>`, so a rename moves nothing and two targets may share a name.
(function enemyRefsToIds() {
  let en;
  try { en = JSON.parse(localStorage.getItem("wfsim-customs-enemies")); } catch (_) { return; }
  if (!Array.isArray(en)) return;
  const to = new Map(en.filter((p) => p && p.id && p.name).map((p) => [`custom:${p.name}`, `custom:${p.id}`]));
  if (!to.size) return;
  const walk = (v) => (typeof v === "string" ? (to.get(v) ?? v) : Array.isArray(v) ? v.map(walk)
    : v && typeof v === "object" ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, walk(x)])) : v);
  for (const k of Object.keys(localStorage)) {
    if (!k.startsWith("wfsim-presets-")) continue;
    const raw = localStorage.getItem(k);
    let list;
    try { list = JSON.parse(raw); } catch (_) { continue; }
    const out = JSON.stringify(walk(list));
    if (out !== raw) { try { localStorage.setItem(k, out); } catch (_) { /* again next load */ } }
  }
})();
/// ONE-TIME FOLD of every per-owner list into its collection's one list, each
/// entry tagged with the owner it was filed under. AN ENTRY'S OWN `scope` BEATS
/// ITS KEY: a page from before the fold files the one list under whichever
/// weapon it has open, and the scope is what files it back. Two copies of one id
/// keep the later save.
(function foldOwnerLists() {
  const into = new Map();
  for (const k of Object.keys(localStorage)) {
    const o = ownerKeyed(k);
    if (!o) continue;
    let list;
    try { list = JSON.parse(localStorage.getItem(k)); } catch (_) { list = null; }
    if (!into.has(o.domain)) {
      let one;
      try { one = JSON.parse(localStorage.getItem(presetListKey(o.domain))); } catch (_) { one = null; }
      into.set(o.domain, Array.isArray(one) ? one : []);
    }
    const all = into.get(o.domain);
    for (const p of Array.isArray(list) ? list : []) {
      if (!p || typeof p !== "object") continue;
      const scope = p.scope || o.owner;
      const e = { ...p, scope };
      if (o.domain === "builder-builds" && e.state && e.state.weapon && e.state.weapon !== scope) {
        e.state = { ...e.state, weapon: scope };
      }
      const at = all.findIndex((q) => q && p.id && q.id === p.id);
      if (at < 0) all.push(e);
      else if ((e.savedAt || 0) >= (all[at].savedAt || 0)) all[at] = e;
    }
    try { localStorage.removeItem(k); } catch (_) { /* folded again next load */ }
  }
  for (const [d, all] of into) {
    try { localStorage.setItem(presetListKey(d), JSON.stringify(all)); } catch (_) { /* again next load */ }
  }
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

/// HOW MUCH THIS BROWSER HAS SAVED, per pool the build sync will count:
/// `presets` (builds, fights, searches, Warframe, companion and Operator
/// builds) and `customs` (rivens, enemies). A board row opened into a bar is
/// the board's, not the reader's, and is not counted.
function savedCounts() {
  const out = { presets: 0, customs: 0 };
  for (const k of Object.keys(localStorage)) {
    const pool = k.startsWith("wfsim-presets-") ? "presets" : k.startsWith("wfsim-customs-") ? "customs" : null;
    if (!pool) continue;
    let list;
    try { list = JSON.parse(localStorage.getItem(k)); } catch (_) { continue; }
    if (Array.isArray(list)) out[pool] += list.filter((p) => p && !p.builtin).length;
  }
  return out;
}

// Parsed lists, memoised on the RAW STRING. The stored text IS the
// invalidation — nothing to keep in sync, and a stale read is impossible.
// Worth having because `gainKey()` resolves a whole scenario and is called
// from a sort comparator, i.e. O(n log n) times per picker render.
const presetParseCache = new Map();
const loadPresetWhole = (d) => {
  const k = presetListKey(d);
  let raw;
  try { raw = localStorage.getItem(k); } catch (_) { return []; }
  const hit = presetParseCache.get(k);
  if (hit && hit.raw === raw) return hit.list;
  let list = [];
  try { const p = JSON.parse(raw); if (Array.isArray(p)) list = p; } catch (_) { /* empty */ }
  presetParseCache.set(k, { raw, list });
  return list;
};
/// WHAT THIS OWNER CAN SEE. The store holds the roster's and the QUERY is what
/// makes it this weapon's (or frame's, or riven family's), which is the whole
/// point of filing an entry by what it IS.
const loadPresetList = (d, w) => {
  const list = loadPresetWhole(d);
  const scope = entryScope(d, w);
  return scope === null ? list : list.filter((p) => (p.scope || "") === scope);
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
const newPresetName = (ps) => freeName(ps, (n) => autoPresetName(PRESET_NAME, n));
/// THE DEFAULT: a preset that always exists and cannot be edited, stored or
/// deleted — the blank. Owning nothing, the editor IS it, and the first
/// effective edit there is what writes "preset 1"; a preset edited back to the
/// blank is deleted, so what is left is it again. It is where a link lands when
/// its preset is gone, and no collection's own bar lists it.
const DEFAULT_PRESET_ID = "default";
/// WHICH PRESET A PAGE OPENS ON: the one `?build=` names — the default, or one
/// that is gone, opening the blank — and with none named, the last one open.
/// `null` is the blank.
const presetToOpen = (list, want, last) => (want
  ? list.find((x) => x.id === want) || null
  : presetFind(list, last) || list[0] || null);
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
  const key = presetListKey(d);
  // A WRITE REPLACES THIS OWNER'S SLICE, not the file: the caller was handed
  // this weapon's entries and hands them back, and every other owner's sit in
  // the same list untouched. The tag is re-applied on the way in, IN PLACE —
  // the caller goes on holding these objects — so an entry copied from another
  // weapon lands under the one it is being saved for.
  const scope = entryScope(d, weapon);
  if (scope !== null) ps.forEach((p) => { if (p && typeof p === "object") p.scope = scope; });
  const others = scope === null ? [] : loadPresetWhole(d).filter((p) => (p.scope || "") !== scope);
  // A NEW ENTRY GETS ITS ID ON THE WAY IN, so no "+ new", copy or import has
  // to remember to mint one — and before `recordUndo`, so the step it records
  // is of the list as stored. Over the WHOLE list, so a copy of another
  // owner's entry is a new entry. A riven's id is minted with its slots.
  if (d !== RIVENS) mintPresetIds(others.concat(ps));
  const isQuota = (e) => !!e && (e.name === "QuotaExceededError"
    || e.name === "NS_ERROR_DOM_QUOTA_REACHED" || e.code === 22 || e.code === 1014);
  recordUndo(d, weapon, ps);
  try {
    localStorage.setItem(key, JSON.stringify(others.concat(ps)));
    syncSoon();
  } catch (e) {
    if (!isQuota(e)) throw e;
    // FULL. Say so where the reader is rather than throwing into a console
    // nobody has open: the edit is on screen and it is not saved, and that is
    // the one thing they need to know.
    noteInline(tr("this browser's storage is full - the change is on screen but was not saved"));
  }
};

