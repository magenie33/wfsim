// ---- i18n --------------------------------------------------------------
// English is the SOURCE (each entity's own name / the literal UI strings);
// other languages are overlays. UI strings: tr() over the catalog below.
// Game-entity names: LN() over /api/i18n (data/i18n/<locale>.yaml — ids are
// never translated; missing entries fall back to English).
// The language on a FIRST visit is the browser's, not English. Every zh-* maps to our one Chinese: a reader of Traditional
// is closer to Simplified than to English, and the choice is one click away
// either way.
//
// The detected value is deliberately NOT written to storage. The key holds a
// CHOICE and nothing else, so "never picked" stays distinguishable from
// "picked English" — a visitor who lands in the wrong language once and
// fixes it is remembered, and one who never touches it follows their
// browser if they later switch systems. Writing the guess would make those
// two states the same and freeze the guess forever.
const LOCALES = ["en", "zh"];
function detectLang() {
  const want = navigator.languages && navigator.languages.length
    ? navigator.languages : [navigator.language || "en"];
  for (const raw of want) {
    const tag = String(raw).toLowerCase();
    const hit = LOCALES.find((l) => l !== "en" && (tag === l || tag.startsWith(l + "-")));
    if (hit) return hit;
    if (tag === "en" || tag.startsWith("en-")) return "en";
  }
  return "en";
}
let LANG = localStorage.getItem("wfsim-lang") || detectLang();
let I18N = null; // active locale's name overlay, fetched in init()
// EVERY OTHER LOCALE'S NAMES, for SEARCH ONLY — never for display.
//
// The English page had no Chinese in it at all, so a player typing 私法 got "no
// matches" while the mod sat in the list one row down (group report,
// 2026-08-12). The reverse already worked, because a localized overlay keeps
// `name_en` beside the translated name; only English lacked the other side.
//
// `/api/i18n` already returns every locale in one response, so this costs one
// request on the English page and nothing anywhere else.
let ALT_NAMES = null;
// UI strings and effect phrases live in data/i18n/<locale>.yaml (served at
// /api/i18n) — nothing hardcoded here. English needs no catalog: the source
// string is the fallback.
const tr = (s) => (I18N && I18N.ui && I18N.ui[s]) || s;
/// ONE ADMISSION, in the display language.
///
/// A gap that repeats is a REASON with parameters rather than a sentence
/// (data/unmodelled/reasons.yaml), so the overlay carries its TEMPLATE and this
/// fills the same {named} holes. A weapon whose falloff starts at a new
/// distance then costs no translation at all — before this, every new number
/// was a new string somebody had to translate.
///
/// Falls back the way everything else here does: the template if the overlay
/// has it, else the finished English through the ordinary lookup.
/// A weapon's admissions, preferring the STRUCTURED form. `unmodeled` is the
/// finished English and is still what an older payload carries, so this is the
/// one place that knows the difference.
const gapsOf = (w) => (w && w.unmodeled_parts && w.unmodeled_parts.length)
  ? w.unmodeled_parts
  : ((w && w.unmodeled) || []);
const trGap = (g) => {
  if (typeof g === "string") return tr(g);
  if (g && g.template) {
    const t = tr(g.template);
    if (t !== g.template) {
      return Object.entries(g.params || {}).reduce(
        (s, [k, v]) => s.split(`{${k}}`).join(v), t);
    }
  }
  return tr((g && g.text) || "");
};
/// Translate a template and fill its `{named}` holes — the same contract
/// `trGap` uses for an unmodelled reason, hoisted out so anything else that
/// needs a sentence with numbers in it costs ONE translated string rather than
/// one per set of numbers.
const trF = (template, params) => Object.entries(params || {}).reduce(
  (s, [k, v]) => s.split(`{${k}}`).join(v), tr(template));
const LN = (table, id, en) => (I18N && I18N[table] && I18N[table][id]) || en;
// A damage type's NAME. The English fallback is CAPITALISED rather than echoed:
// callers arrive with either spelling — the server sends "Void" in a damage
// meter row and a yaml token is "void" — and echoing put a lowercase "void" on
// the English buff card while the Chinese one read 虚空. One helper,
// one answer, whichever spelling reaches it.
const DT = (ty) => {
  const k = String(ty).toLowerCase();
  return LN("damage_types", k, k.charAt(0).toUpperCase() + k.slice(1));
};
// A damage type's OFFICIAL colour and icon — DE's own, transcribed from the
// wiki's `Module:DamageTypes/data` (see style.css for the palette and
// data/assets.yaml for the files).
//
// Keyed on the TYPE and never on a row's position: colouring by index makes
// Heat one colour under a direct hit and another under a field. `null` for
// anything that is not a damage type — a
// source row like "Direct hits" is not one, and asking for its colour should
// return nothing rather than a wrong one.
const DT_TYPES = new Set(["impact", "puncture", "slash", "cold", "electricity",
  "heat", "toxin", "blast", "corrosive", "gas", "magnetic", "radiation",
  "viral", "true", "void", "tau"]);
const dtKey = (ty) => {
  const k = String(ty || "").toLowerCase();
  return DT_TYPES.has(k) ? k : null;
};
const dtColor = (ty) => (dtKey(ty) ? `var(--dt-${dtKey(ty)})` : null);

/// AN ATTACKER'S NAME, from the engine's own id.
///
/// The engine ships stable English slugs and knows no weapon names at all, so
/// the page resolves them — the same division the body roll call runs on. An
/// id nobody has a label for prints ITSELF rather than a blank, which is still
/// a name a reader can act on.
const COMBATANT_LABEL = { wielder: "You" };
/// `seat` is what that seat BROUGHT, as the run reported it — never looked up
/// in the live roster, which has moved on by the time a saved result is read.
const combatantName = (id, seat) => {
  if (COMBATANT_LABEL[id]) return tr(COMBATANT_LABEL[id]);
  const w = seat && seat.weapon && weaponExists(seat.weapon) && weaponInfo(seat.weapon);
  return w ? tf(w.name) : String(id || "");
};
const dtIcon = (ty) => {
  const k = dtKey(ty);
  if (!k) return "";
  const file = (META.damage_type_icons || {})[k];
  return file ? `<img class="dt-ico" src="${IMG(file)}" alt="" loading="lazy">` : "";
};
// Effect-line phrase substitution ("+X% Critical Chance" → "+X% 暴击几率"):
// the ORDERED [regex, replacement(, flags)] table comes from the locale's
// effect_phrases (data/i18n). Compiled once on first use.
let EFFECT_RES = null;
const tf = (x) => {
  if (!I18N || typeof x !== "string") return x;
  // AN EXACT TRANSLATION BEATS A SUBSTITUTION. The phrase table is the
  // FALLBACK — it exists for the sentences DE never wrote — so a string the
  // overlay states in full is taken whole rather than word by word. Without
  // this, "Damage x1.5 at 5 hits, +0.5 every x3" came out as one Chinese word
  // followed by the English it was embedded in.
  if (I18N.ui && I18N.ui[x]) return I18N.ui[x];
  if (!EFFECT_RES) {
    EFFECT_RES = (I18N.effect_phrases || []).flatMap(([pat, cn, flags]) => {
      try { return [[new RegExp(pat, flags || "gi"), cn]]; } catch (_) { return []; }
    });
  }
  let s = x;
  for (const [re, cn] of EFFECT_RES) s = s.replace(re, cn);
  return s;
};
// One lowercase search haystack per entity: the LOCALIZED name, the
// ENGLISH name, the English effect lines AND their translated phrases —
// every search box matches in English and in the active language alike,
// names and effects both. Cached on the entity (names
// and overlay are fixed for the page's lifetime).
const searchBlob = (x) => {
  if (x._search) return x._search;
  const eff = (x.effects || []).concat(x.desc_ranks || [], (x.ranks || []).flat());
  // …plus DE's own card text when the locale has it, so a search for 弓类 or
  // 加倍 hits the mod whose card says it (the phrase table never produced
  // those words — see officialDesc).
  const official = (I18N && ((I18N.mod_descriptions || {})[x.id] || (I18N.arcane_descriptions || {})[x.id])) || [];
  // …plus the name every OTHER locale gives it, so a search works whichever
  // language the page happens to be in. Display never reads these.
  // …every OTHER locale's name, plus the ACTIVE one looked up by id. The second
  // half matters for objects the display overlay never touched — `META.mod_pools`
  // holds segment pools the picker does not build from, and searching those for
  // a localized name found nothing because the name was never written onto them.
  const byId = (tbl) => Object.values(tbl || {})
    .map((m) => m && m[x.id]).filter((s) => typeof s === "string");
  const alt = (ALT_NAMES || []).flatMap(byId).concat(I18N ? byId(I18N) : []);
  x._search = [x.name, x.name_en, x.subtype, eff.join(" "), tf(eff.join(" ")),
    official.join(" "), alt.join(" ")]
    .filter(Boolean).join(" ").toLowerCase();
  x._searchTight = squash(x._search);
  return x._search;
};
// SPACES ARE NOT PART OF THE WORD. DE's Chinese names carry one — "私法 军备",
// "野猪 Prime", "布尔斯顿 (虚坏形态)" — and 181 of the 516 names in
// `data/i18n/zh/names.yaml` do, so a player typing the name the way it reads
// (私法军备) found NOTHING while the mod sat in the list (group report,
// 2026-08-12). The name is transcribed correctly and stays as DE writes it;
// what was wrong is asking the query to reproduce a space nobody says.
//
// Squashing BOTH sides is a superset of the old match, never a subset: two
// strings that matched with their spaces still match without them.
const squash = (s) => String(s || "").replace(/\s+/g, "");
// One predicate for every list that filters by a searchable blob.
const searchHit = (x, q) => {
  if (!q) return true;
  const blob = searchBlob(x);
  return blob.includes(q) || (x._searchTight || squash(blob)).includes(squash(q));
};
// Static labels: translate the first text node of every [data-i18n] element
// (children like the .sim-hint spans stay untouched), and the placeholder of
// every [data-i18n-ph] input — a search box's prompt is a UI string like any
// other, and it was the one kind the sweep did not reach.
function applyI18n() {
  // A translated line with ONE word picked out — the hero's "Prime", gold
  // because that is the game's own colour for a Prime item. The whole
  // sentence stays a single key: the marked word is Latin in every language,
  // so it can be found after translation instead of being carved out of the
  // source into a key of its own.
  document.querySelectorAll("[data-i18n-gold]").forEach((el) => {
    if (!el.dataset.i18nSrc) el.dataset.i18nSrc = el.textContent.trim();
    const word = el.dataset.i18nGold;
    el.innerHTML = escHtml(tr(el.dataset.i18nSrc))
      .split(word)
      .join(`<span>${escHtml(word)}</span>`);
  });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => {
    if (!el.dataset.i18nPhSrc) el.dataset.i18nPhSrc = el.placeholder;
    el.placeholder = tr(el.dataset.i18nPhSrc);
  });
  // …and the tooltip of every [data-i18n-title]. A hover hint is a UI string
  // like any other; it was simply the kind nothing reached, so a fully
  // translated page still explained itself in English on hover.
  document.querySelectorAll("[data-i18n-title]").forEach((el) => {
    if (!el.dataset.i18nTitleSrc) el.dataset.i18nTitleSrc = el.title;
    el.title = tr(el.dataset.i18nTitleSrc);
  });
  document.querySelectorAll("[data-i18n]").forEach((el) => {
    const node = [...el.childNodes].find((n) => n.nodeType === 3 && n.textContent.trim());
    if (!node) return;
    if (!el.dataset.i18nSrc) el.dataset.i18nSrc = node.textContent.trim();
    node.textContent = node.textContent.replace(el.dataset.i18nSrc, tr(el.dataset.i18nSrc)) || tr(el.dataset.i18nSrc);
  });
}
// Mutate META once: every downstream renderer (home grid, selects, pickers,
// optimizer lists) shows overlay names with zero per-site changes. EVERY
// entity keeps its English name as name_en — URLs/wiki links always build
// from it (the wiki is English; a localized name in the URL 404s or lands
// on oddities).
function applyNameOverlay() {
  if (!I18N) return;
  const over = (x, table) => { x.name_en = x.name; x.name = LN(table, x.id, x.name); };
  for (const w of META.weapons || []) {
    over(w, "weapons");
    for (const t of w.evolutions || []) for (const o of t.options || []) over(o, "evolutions");
  }
  for (const e of META.enemies || []) over(e, "enemies");
  for (const pool of Object.values(META.mod_pools || {})) for (const m of pool) over(m, "mods");
  for (const a of META.arcanes || []) over(a, "arcanes");
}
// Polarities available on GUN slots. Zenurik/Unairu/Penjaga are Warframe-augment
// / melee-stance / companion-ability polarities — not gun slots. "Omni" is the
// Omni Forma universal polarity (matches any mod EXCEPT Umbra mods).
const GUN_POLS = ["Madurai", "Naramon", "Vazarin", "Umbra", "Omni"];
// WHAT THIS WEAPON HAS TO SPEND. Not a constant: an ADVERSARY weapon
// (Kuva/Tenet/Coda) ranks to 40 rather than 30 and finishes at 80, and the
// server sends the finished number per weapon (`/api/meta`) rather than the
// ladder, so this file holds no capacity arithmetic of its own.
//
// 60 is the fallback and nothing more — every weapon in the roster answers.
const capOf = (id) => (weaponInfo(id) || {}).capacity || 60;

// …AND WHAT THE STANCE SLOT HANDS BACK, which is the one slot that ADDS
// capacity instead of spending it: "All Stances provide a bonus mod capacity of
// 5 when maxed, doubling it to 10 when placed on the matching polarity" (wiki,
// Stance), and the Aura page's third case — a slot of a DIFFERENT polarity
// grants "80% of listed drain, rounded down", which is 4. Mirrors
// `engine::rules::capacity::stance_capacity`.
const stancePolOf = (id) => (weaponInfo(id) || {}).stance_polarity || null;
/// A STANCE THE WEAPON CANNOT TAKE OFF (Valkyr Talons' Hysteria, MEASUREMENTS
/// M94): seated on every build, never removed, and its slot takes no Forma.
const fixedStanceOf = (id) => (weaponInfo(id) || {}).fixed_stance || null;
function stanceGrant() {
  const s = slots[STANCE];
  const m = s && s.mod ? modById(s.mod) : null;
  if (!m) return 0;
  // NULL IS NO POLARITY, not "whatever the weapon was born with": the slot
  // draws blank, and `engine::rules::capacity::stance_capacity` answers 5 for it. Reading
  // the factory colour here made a blanked slot grant 4 while showing nothing.
  const pol = s.pol;
  if (!pol) return 5;
  return pol === m.polarity || pol === "Omni" ? 10 : 4;
}
// The polarizations the weapon owes its own rank ceiling: FIVE on an adversary
// weapon, none on anyone else. It is a mastery figure, not a capacity one — a
// build that fits in three still pays it, because those three do not put the
// weapon at rank 40 and rank 40 is what the 80 above assumes.
const formaMin = (id) => (weaponInfo(id) || {}).forma_min || 0;
// EVERY CALLER IS A LIST, so this loads lazily. The picker and the optimizer's
// scope draw one `modRow` per eligible mod — two images each — and without this
// a scope of 300 mods opens 600 connections for art that is mostly below the
// fold. `loading="lazy"` costs the visible rows nothing: a browser fetches what
// is in or near the viewport immediately either way. The weapon render on the
// arena's inspector is a bare <img> for the same reason inverted — it is one
// image, always on screen, and deferring it would delay the largest paint.
const imgTag = (src, cls) => src ? `<img class="${cls||''}" src="${src}" loading="lazy" onerror="this.style.visibility='hidden'"/>` : `<span class="${cls||''}"></span>`;

