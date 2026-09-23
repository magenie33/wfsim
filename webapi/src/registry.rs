// SPDX-License-Identifier: AGPL-3.0-or-later
//! The weapon registry and the data it is read from: the enemy library, the
//! image map, and the lookups every endpoint makes about one weapon.

use serde_json::{json, Value};
use wfsim_engine::data::enemies::EnemySpec;
use wfsim_engine::model::WeaponBase;
use wfsim_engine::model::ModDef;
use wfsim_engine::rules::capacity::Polarity;
use crate::kitgun::{apply_valence_from, assembly_of};
use crate::request::get_str;

// ---- Enemy library (the engine's embedded data/enemies/**) -------------
// Single source of truth: the same data/ files the CLI and optimizer read,
// embedded by the engine's build script. The UI lists the classics first;
// anything new in data/enemies/ appends after them in path order.
pub(crate) fn enemies() -> Vec<EnemySpec> {
    let preferred = ["thrax_centurion"];
    let mut specs = wfsim_engine::data::enemies::all();
    specs.sort_by_key(|s| {
        preferred
            .iter()
            .position(|p| *p == s.id)
            .unwrap_or(preferred.len())
    });
    specs
}

#[derive(serde::Deserialize, Default)]
pub(crate) struct Assets {
    #[serde(default)]
    pub(crate) weapons: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub(crate) mods: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub(crate) arcanes: std::collections::HashMap<String, String>,
    /// DE's own icon per damage type, keyed by the lowercase type name. Wiki-
    /// hosted (the CDN 404s every one), so each carries the `wiki:` prefix.
    #[serde(default)]
    pub(crate) damage_types: std::collections::HashMap<String, String>,
    /// THE WARFRAME BUILDER'S cards, one section per data directory.
    #[serde(default)]
    pub(crate) warframes: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub(crate) operators: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub(crate) warframe_mods: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub(crate) warframe_arcanes: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub(crate) auras: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub(crate) artifacts: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub(crate) artifact_mods: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub(crate) artifact_arcanes: std::collections::HashMap<String, String>,
}

// ---- Image asset map (data/assets.yaml, embedded by the engine) --------
// id -> WFCD imageName; the frontend builds https://cdn.warframestat.us/img/<name>.
pub(crate) fn assets() -> &'static Assets {
    use std::sync::OnceLock;
    static A: OnceLock<Assets> = OnceLock::new();
    A.get_or_init(|| {
        let yaml = wfsim_engine::data::file("assets.yaml").expect("embedded data/assets.yaml");
        serde_norway::from_str(yaml).unwrap_or_default()
    })
}

// ---- weapon registry ---------------------------------------------------
// The UI is weapon-aware. Each weapon declares its mod class (which mod pool
// the picker shows), whether it takes an arcane / Evolution II, its available
// forms, and whether it is a sentinel (BaseOnly resolution — Galvanized
// conditionals never fire).

pub(crate) struct WeaponInfo {
    pub(crate) id: String,
    pub(crate) name: String,
    // The MOD-ELIGIBILITY group, not a cosmetic label. "pistol" = the Pistol
    // Mods pool, which (wiki Pistol_Mods) equips on secondary Pistols, Dual
    // Pistols, Shotgun Sidearms, Crossbows, and Tomes. This is the ACTUAL way
    // mods take effect, so the eligibility group is what drives the pool.
    /// The pools this weapon draws from, as a union ("primary" + "rifle" for a
    /// launcher). Compatibility is not one list: DE tags a mod PRIMARY, Rifle,
    /// or narrower (Assault Rifle / Bow / Sniper), and a weapon takes every
    /// tag that applies to it.
    pub(crate) mod_pools: Vec<String>,
    /// Continuous (beam) weapon, from the BASE form's trigger — what the
    /// beam-only mods gate on.
    pub(crate) continuous: bool,
    /// Riven disposition. 1.0 when the data does not say, so a weapon with no
    /// disposition yet reads as neutral rather than as zero.
    pub(crate) disposition: f64,
    /// WHOSE RIVEN THIS IS — the weapon FAMILY, never the entry. A weapon that
    /// declares none is its own family. See the meta field for the wiki's own
    /// sentence.
    pub(crate) riven_family: String,
    /// WHAT THIS WEAPON'S ENTRY DOES NOT MODEL, one sentence per gap, straight
    /// from the weapon file. Shown to the reader — a number that omits
    /// something owes them the sentence, not just the omission.
    pub(crate) unmodeled: Vec<String>,
    /// WHAT THIS ENTRY DOES THAT NOBODY CAN EXPLAIN and the engine reproduces
    /// anyway. The OPPOSITE of `unmodeled` beside it: that says the number is a
    /// floor, this says the number is right and the reason is unknown — see
    /// `data::weapons::WeaponSpec::live_bugs`.
    pub(crate) live_bugs: Vec<String>,
    // Precise weapon type within that group (Dual Toxocyst = Dual Pistols).
    pub(crate) subtype: String,
    pub(crate) sentinel: bool,
    /// The forms this weapon REGISTERS (`data/weapons/*.yaml` `form:`), default
    /// first: `(wire id, display name, is the arsenal's default)`, entirely
    /// data-driven. The default travels with the list because it is the form a
    /// weapon is FIRED in when nothing else is asked for.
    pub(crate) forms: Vec<(&'static str, String, bool)>,
    /// Does a form have to be TRANSFORMED into (gauge + transmute animations)?
    /// Only then is there a two-form cycle to simulate; without it the weapon
    /// is fired in one form and asking for a cycle is meaningless.
    pub(crate) has_cycle: bool,
    pub(crate) uses_arcane: bool,
    /// Which arcane pool this weapon draws from — its own slot
    /// ("secondary" / "primary"). The picker filters on it.
    /// The EQUIPMENT slot — primary / secondary / sentinel / archgun. What
    /// the home grid groups by, and a FIELD OF ITS OWN rather than a reading of
    /// `arcane_slot`: an Arch-Gun seats two arcane pools and is neither of
    /// them, so the pool and the slot are two facts.
    pub(crate) slot: String,
    /// The arcane POOLS this weapon seats, one arcane each. Almost always a
    /// single pool named after the equipment slot; a sentinel seats none; an
    /// Arch-Gun seats TWO — "Archguns possess two Arcane Enhancement slots to
    /// equip one Primary Arcane and one Secondary Arcane" (wiki Arch-Gun),
    /// which is also why it is "not considered either primary or secondary".
    pub(crate) arcane_pools: Vec<String>,
    pub(crate) uses_evo2: bool,
}

/// "dual_pistols" → "Dual Pistols".
fn title_case(snake: &str) -> String {
    snake
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One line stating a form's trigger/shot mechanics, from the weapon data.
pub(crate) fn attack_desc(s: &wfsim_engine::data::weapons::WeaponSpec) -> String {
    let mut parts = vec![title_case(&s.attack.trigger).replace(' ', "-")];
    if let Some(st) = s.attack.shot_type {
        parts.push(st.label().to_string());
    }
    if let Some(r) = &s.attack.ricochet {
        // THE COUNT IS WHAT THE PAGE STATES, and the reach usually is not: a
        // bounce ordinarily finds the nearest body it has not hit, however far
        // (`RicochetSpec::range_m`), so a line naming a distance would be
        // stating a number nobody published.
        parts.push(match r.range_m {
            Some(m) => format!("ricochets {} more time{} within {m} m",
                r.bounces, if r.bounces == 1 { "" } else { "s" }),
            None => format!("ricochets {} more time{}",
                r.bounces, if r.bounces == 1 { "" } else { "s" }),
        });
    }
    parts.join(" · ")
}

/// Every FORM's passive lines, deduped.
///
/// A roster row is one weapon and both halves are the reader's to know about —
/// the same rule `unmodeled` follows. This was the base entry's lines alone, so
/// a passive belonging to an Incarnon form had nowhere to appear: the Phenmor's
/// spool-down is declared on `phenmor_incarnon` and the page said nothing about
/// it. Deduped, because a group's forms can carry the same perk.
pub(crate) fn passives_of(id: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in wfsim_engine::data::weapons::forms_of(id) {
        for line in wfsim_engine::data::weapons::passive_lines(f.weapon_id) {
            if !out.contains(&line) {
                out.push(line);
            }
        }
    }
    out
}

// The weapon registry, derived from data/weapons/*.yaml (roster = transform
// group base entries; an Incarnon form is a form, not a roster row).
pub(crate) fn weapons() -> &'static [WeaponInfo] {
    use std::sync::OnceLock;
    static W: OnceLock<Vec<WeaponInfo>> = OnceLock::new();
    W.get_or_init(|| {
        wfsim_engine::data::weapons::roster()
            .map(|s| {
                let sentinel = s.class.contains("sentinel");
                let incarnon = s.transforms_to.is_some();
                // The weapon's OWN forms, in its own order — every entry of
                // its transform group, each registering a kind from the
                // closed vocabulary. The two-form CYCLE is not in this list:
                // it is a mode over two of these forms, published separately
                // as `has_cycle`.
                let forms = wfsim_engine::data::weapons::forms_of(&s.id)
                    .into_iter()
                    .map(|f| {
                        // THE GAME'S NAME FOR IT where the entry states one —
                        // a tapped shot is a "Normal Strike", never a "Base Form".
                        let label = wfsim_engine::data::weapons::spec(f.weapon_id)
                            .map_or_else(|| f.kind.label().to_string(),
                                |x| x.form_label().to_string());
                        (f.kind.id(), label, f.is_default)
                    })
                    .collect();
                WeaponInfo {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    mod_pools: if s.mod_pools.is_empty() {
                        vec![s.slot.clone()]
                    } else {
                        s.mod_pools.clone()
                    },
                    continuous: s.attack.trigger == "held",
                    disposition: s.disposition.unwrap_or(1.0),
                    riven_family: s.riven_family.clone().unwrap_or_else(|| s.id.to_string()),
                    // The BASE entry's gaps and its Incarnon form's, together:
                    // a reader is looking at one weapon and both halves are
                    // theirs to know about.
                    // THE FORMS' TOO, on the same terms as `unmodeled` below: a
                    // reader is looking at one weapon and both halves are
                    // theirs to know about — and the Laetum's doubling is on
                    // the INCARNON entry, which is not the one the page names.
                    live_bugs: wfsim_engine::data::weapons::forms_of(&s.id)
                        .iter()
                        .filter_map(|f| wfsim_engine::data::weapons::spec(f.weapon_id))
                        .flat_map(|x| x.live_bugs.iter().cloned())
                        .collect(),
                    unmodeled: wfsim_engine::data::weapons::forms_of(&s.id)
                        .iter()
                        .filter_map(|f| wfsim_engine::data::weapons::spec(f.weapon_id))
                        .flat_map(|x| x.unmodeled.iter().cloned())
                        .collect(),
                    // WHAT KIND OF WEAPON IT IS, for the picker's filter and
                    // the home card's tag. Normally the CLASS — a Rifle, a
                    // Pistol — and for a MODULAR weapon the fact that it is
                    // one: a Tombfinger is a `rifle` in the primary slot and a
                    // `pistol` in the secondary, and neither of those is the
                    // answer a reader looking for a Kitgun is after.
                    subtype: if s.kitgun.is_some() {
                        "Kitgun".to_string()
                    } else {
                        title_case(&s.class)
                    },
                    sentinel,
                    forms,
                    has_cycle: wfsim_engine::data::weapons::has_gauge_switched_form(&s.id),
                    slot: s.slot.clone(),
                    uses_arcane: !sentinel,
                    // THE ENGINE'S ANSWER, not a second copy of the rule:
                    // `board::builds::validate_for_board` needs the same seat count to
                    // decide whether every arcane seat is filled.
                    arcane_pools: wfsim_engine::data::weapons::arcane_pools(&s.id)
                        .into_iter()
                        .map(String::from)
                        .collect(),
                    uses_evo2: incarnon,
                }
            })
            .collect()
    })
}

pub(crate) fn weapon(id: &str) -> &'static WeaponInfo {
    weapons()
        .iter()
        .find(|w| w.id == id)
        .unwrap_or(&weapons()[0])
}

// ---- spec-derived lookups: no weapon ids are hardcoded anywhere below ----
pub(crate) fn wspec(id: &str) -> &'static wfsim_engine::data::weapons::WeaponSpec {
    wfsim_engine::data::weapons::spec(id).expect("weapon data")
}

/// The transform group's second-form entry (the Incarnon form), if any.
pub(crate) fn incarnon_id(info: &WeaponInfo) -> Option<&'static str> {
    wspec(&info.id).transforms_to.as_deref()
}

/// Evolutions-data key for this weapon: the transform group name.
pub(crate) fn evo_group(info: &WeaponInfo) -> &'static str {
    let s = wspec(&info.id);
    s.transform_group.as_deref().unwrap_or(&s.id)
}

/// The tier-1 evolution that unlocks the second form (deselecting it means
/// no transformation).
/// Per evolution of this weapon's group, the mods installing it takes OFF the
/// weapon — `{ "<evo id>": ["<mod id>", …] }`, entries with nothing to say
/// omitted.
///
/// Only a form-unlocking evolution can say anything today (it is the one that
/// gives the weapon a second firing mode), but it is computed by ASKING the
/// pool, not by assuming that: a stat evolution that ever changed a trigger
/// would be answered correctly without a line changing here.
pub(crate) fn evo_forbids(info: &WeaponInfo) -> serde_json::Map<String, Value> {
    let bare = wfsim_engine::data::mods::pool_for_weapon(&info.id);
    let group = evo_group(info);
    let mut out = serde_json::Map::new();
    for e in wfsim_engine::data::evolutions::pool().iter().filter(|e| e.weapon == group) {
        let with = wfsim_engine::data::mods::pool_for_build(&info.id, &[e.id.as_str()]);
        let lost: Vec<&str> = bare
            .iter()
            .map(|m| m.id)
            .filter(|id| !with.iter().any(|m| m.id == *id))
            .collect();
        if !lost.is_empty() {
            out.insert(e.id.clone(), json!(lost));
        }
    }
    out
}

pub(crate) fn form_unlock_evo(info: &WeaponInfo) -> Option<&'static str> {
    // BY ITS TAG, not by ladder position: "tier 1's first option" is a guess
    // that happens to hold for the Incarnon weapons in the roster and says
    // nothing about the next one.
    let group = evo_group(info);
    wfsim_engine::data::evolutions::pool()
        .iter()
        .find(|e| e.weapon == group && e.unlocks_form().is_some())
        .map(|e| e.id.as_str())
}

/// Whether the weapon's data declares the Frenzy perk (data/perks/).
pub(crate) fn has_frenzy(info: &WeaponInfo) -> bool {
    wspec(&info.id).perks.iter().any(|p| p.id() == "frenzy")
}

pub(crate) fn default_weapon_id() -> &'static str {
    &weapons()[0].id
}

// The FULL pool (exilus included) of a weapon's mod class — the picker and
// every id lookup go through here, so a weapon whose `mod_eligibility` names
// a class with no data yet gets an empty pool rather than another weapon's.
/// The pool a BUILD actually sees: the weapon's pools unioned, minus mods it
/// cannot equip (the beam-only mods need a continuous weapon; a Cannonade needs
/// semi-auto on every firing mode, so an unlocked Incarnon form takes it off).
///
/// `evos` is the build's chosen evolutions — the pool is a question about the
/// weapon AS CONFIGURED, not about the weapon.
pub(crate) fn mod_pool_for(weapon_id: &str, evos: &[&str]) -> Vec<ModDef> {
    wfsim_engine::data::mods::pool_for_build(weapon_id, evos)
}

/// Why a mod id did not resolve against THIS weapon's pool.
///
/// "unknown mod id: amalgam_serration" is true of the pool and false of the
/// world, and it is what a saved build gets the moment the pool learns a rule
/// (Amalgam mods off sentinel weapons). A mod that exists but does not fit
/// says so.
///
/// It is also what a saved build gets when an EVOLUTION takes a mod off the
/// weapon: the mod is in the weapon's own pool and out of this build's, so
/// "not in this weapon's pool" would be untrue. Asking the pool twice decides
/// which of the two it is.
pub(crate) fn mod_not_here(id: &str, weapon: &WeaponInfo, evos: &[&str]) -> String {
    if let (card, Some(rank)) = wfsim_engine::data::mods::split_rank(id) {
        if wfsim_engine::data::mods::at_rank(id).is_none() {
            return format!("{card} cannot be simulated at rank {rank}");
        }
        return mod_not_here(card, weapon, evos);
    }
    let known = wfsim_engine::data::mods::classes()
        .into_iter()
        .any(|c| wfsim_engine::data::mods::class_pool(c).iter().any(|m| m.id == id));
    if !known {
        return format!("unknown mod id: {id}");
    }
    let bare = wfsim_engine::data::mods::pool_for_weapon(&weapon.id);
    if !evos.is_empty() && bare.iter().any(|m| m.id == id) {
        let name = bare.iter().find(|m| m.id == id).map(|m| m.name).unwrap_or(id);
        return format!(
            "{name} cannot be equipped on {} with these evolutions installed — \
             it needs the same trigger on every firing mode",
            weapon.name
        );
    }
    format!("{id} cannot be equipped on {} — it is not in this weapon's pool", weapon.name)
}

/// A weapon's base for THIS request, with the chosen DEPLOYMENT applied.
///
/// Where an Arch-Gun is fired changes its sustain and nothing else — same
/// damage, same mods, same riven — so the environment is a scenario knob and
/// not a second weapon. Absent or unknown leaves the
/// weapon on its own column, which is the one its fields state.
pub(crate) fn base_for(v: &Value, id: &str, evos: &[&str]) -> WeaponBase {
    let asm = assembly_of(v, id);
    let mut b = WeaponBase::from_data_assembled(id, true, evos, asm.as_ref());
    let dep = get_str(v, "deployment", "");
    if !dep.is_empty() {
        wfsim_engine::data::weapons::apply_deployment(&mut b, id, dep);
    }
    apply_valence_from(v, id, &mut b);
    b
}

// 8 main slots (innate polarities from the weapon yaml) + the exilus slot as
// the UI's 9th slot, carrying ITS innate polarity too (wiki "Exilus Polarity").
// Same model as autoForma and the optimizer — without the 9th slot a 9-mod
// build trips plan_forma's mods≤slots assert.
//
// AND NOT ON A WEAPON THAT HAS NO SUCH SLOT: the LENGTH of this list is the
// weapon's slot count, which is what decides where a leftover innate colour can
// sit for free.
pub(crate) fn innate_slots_for(id: &str) -> Vec<Option<Polarity>> {
    let mut v = wfsim_engine::data::weapons::innate_slots(id).to_vec();
    if wfsim_engine::data::weapons::has_exilus_slot(id) {
        v.push(wfsim_engine::data::weapons::exilus_polarity(id));
    }
    v
}

/// TWO WEAPONS MUST NOT WEAR ONE PICTURE.
///
/// `data/assets.yaml` is filled from WFCD's `imageName`, and for some weapons
/// that field is a SIBLING'S file: the export gives MK1-Furis `Furis.png` and
/// Ocucor `CrpSentExperimentPistol.png` (which the CDN does not serve at all).
/// Both are hand-overridden to `wiki:` entries, and both were silently
/// re-derived — wrongly — the one time `scripts/gen_assets.py --write` ran with
/// them absent. Nothing downstream notices: the file exists, the fetcher caches
/// it, the build's missing-art guard passes, and the page shows a Furis where
/// an MK1-Furis should be.
///
/// The one legitimate collision is two FORMS of one weapon — an Incarnon form
/// shows its base weapon's image on purpose ("not the Genesis adapter icon"),
/// and an uncharged bow is the same bow. That is what a TRANSFORM GROUP already
/// means, so the exemption is read off the weapon data rather than written as a
/// list of pairs or guessed from the id's suffix.
#[cfg(test)]
mod one_picture_one_weapon {
    use super::*;

    /// The weapon an id draws its art from — its transform group, so every
    /// form of one weapon is one subject.
    ///
    /// …AND A MODULAR WEAPON IS ITS CHAMBER. A Kitgun's two slots are two
    /// roster entries with two transform groups, because a slot is not a form —
    /// it decides which mods the weapon may hold, which is a question with a
    /// static answer. They are still ONE weapon by every measure DE has: one
    /// mastery track, one riven, one wiki page and therefore one picture. See
    /// data/kitguns/README.md.
    fn subject(id: &str) -> &str {
        let Some(s) = wfsim_engine::data::weapons::spec(id) else { return id };
        match s.kitgun.as_deref().and_then(|k| {
            wfsim_engine::data::weapons::kitguns::chambers().iter().find(|c| c.id == k)
        }) {
            Some(c) => c.chamber.as_str(),
            None => s.group(),
        }
    }

    #[test]
    fn no_two_weapons_share_an_image() {
        let mut by_image: std::collections::HashMap<&str, Vec<&str>> = Default::default();
        for (id, image) in &assets().weapons {
            by_image.entry(image.as_str()).or_default().push(id.as_str());
        }
        for (image, mut ids) in by_image {
            ids.sort_unstable();
            let subjects: std::collections::BTreeSet<&str> =
                ids.iter().map(|i| subject(i)).collect();
            assert!(
                subjects.len() <= 1,
                "`{image}` is worn by {ids:?}, which are different weapons — one of \
                 them has a sibling's picture. WFCD's `imageName` is wrong for these; \
                 set the right file by hand (a `wiki:` prefix if the CDN lacks it)."
            );
        }
    }

    /// ...and every weapon in the roster HAS one. The build fails on a missing
    /// file; this fails on a missing ENTRY, which is the earlier and clearer
    /// error.
    #[test]
    fn every_weapon_has_an_image() {
        for w in wfsim_engine::data::weapons::roster() {
            assert!(
                assets().weapons.contains_key(&w.id),
                "{} has no entry in data/assets.yaml",
                w.id
            );
        }
    }
}
