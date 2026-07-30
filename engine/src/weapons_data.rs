//! Weapon data loader: `data/weapons/*.yaml` → [`WeaponBase`] + registry
//! metadata (CORE.md §2.3: weapon numbers are DATA; the engine only holds
//! rules). The yamls are the source of record — `loadout`'s per-weapon
//! constructors delegate here, and the web registry derives its weapon list,
//! tags, polarities and form descriptors from the same specs.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::damage::{DamageType, DamageVector};
use crate::loadout::{
    ChargeOn, CoBehavior, FieldStacking, IncarnonForm, LingeringBase, RadialBase, WeaponBase,
};
use crate::mods::Polarity;

#[derive(Debug, Clone, Deserialize)]
pub struct AttackSpec {
    pub trigger: String,
    #[serde(default)]
    pub shot_type: Option<String>,
    pub fire_rate: f64,
    #[serde(default = "one")]
    pub multishot: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub status_chance: f64,
    pub damage: BTreeMap<String, f64>,
    #[serde(default)]
    pub ricochet: Option<RicochetSpec>,
    /// A radial (AoE) part fired with every projectile of this attack.
    #[serde(default)]
    pub radial: Option<RadialSpec>,
    /// A LINGERING FIELD left by every landed projectile (Torid's cloud).
    #[serde(default)]
    pub lingering: Option<LingeringSpec>,
}

/// A lingering damage FIELD — MECHANICS §7 "Lingering damage FIELDS". Unlike
/// the radial this is not one instance at impact: it persists and TICKS.
#[derive(Debug, Clone, Deserialize)]
pub struct LingeringSpec {
    pub damage: BTreeMap<String, f64>,
    /// Ticks per second (the data module's per-attack `FireRate`).
    pub tick_rate: f64,
    /// Field lifetime in seconds (`EffectDuration`).
    pub duration_s: f64,
    pub radius_m: f64,
    #[serde(default)]
    pub crit_chance: Option<f64>,
    #[serde(default)]
    pub crit_multiplier: Option<f64>,
    #[serde(default)]
    pub status_chance: Option<f64>,
    #[serde(default)]
    pub falloff_start_m: Option<f64>,
    #[serde(default)]
    pub falloff_reduction: Option<f64>,
    /// `stack` (default) or `refresh` — UNVERIFIED, see MEASUREMENTS M12. It
    /// is a data field precisely so one measurement can flip it.
    #[serde(default = "stack")]
    pub stacking: String,
}

fn stack() -> String {
    "stack".to_string()
}

/// The radial (explosion) part of an attack — MECHANICS §7. Crit/status
/// default to the direct part's when the data does not state them.
#[derive(Debug, Clone, Deserialize)]
pub struct RadialSpec {
    pub damage: BTreeMap<String, f64>,
    pub radius_m: f64,
    #[serde(default)]
    pub crit_chance: Option<f64>,
    #[serde(default)]
    pub crit_multiplier: Option<f64>,
    #[serde(default)]
    pub status_chance: Option<f64>,
    #[serde(default)]
    pub falloff_start_m: Option<f64>,
    /// Fraction of damage REMOVED at maximum distance (Laetum: 0.2 → 80%).
    #[serde(default)]
    pub falloff_reduction: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RicochetSpec {
    pub targets: u32,
    pub range_m: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IncarnonSpec {
    pub gauge: GaugeSpec,
    /// Transition animations, unmodded; both scale by the reload formula.
    pub transmute_in_seconds: f64,
    pub transmute_out_seconds: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GaugeSpec {
    pub max_rounds: f64,
    /// Hits needed to fill the gauge (DT 9, Laetum 12, Torid 5).
    pub charges_to_fill: f64,
    /// WHICH hits count — `weakpoint_hits` (the Zariman default) or
    /// `direct_hits` (Torid). A REAL field: it was documentation-only in the
    /// yaml before, so every weapon silently charged off weak-point hits.
    #[serde(default = "weakpoint_hits")]
    pub charge_on: String,
}

fn weakpoint_hits() -> String {
    "weakpoint_hits".to_string()
}

/// The locked-gauge magazine/reload reduction of an Incarnon form.
#[derive(Debug, Clone, Deserialize)]
pub struct PseudoReloadSpec {
    pub magazine: f64,
    pub reload_seconds: f64,
}

/// A perk definition — either a `data/perks/*.yaml` entry or an inline
/// block in a weapon's `perks:` list. Both modes register the perk in the
/// GLOBAL namespace: define once anywhere, reference by bare id from
/// everywhere else (data/README.md).
#[derive(Debug, Clone, Deserialize)]
pub struct PerkSpec {
    pub id: String,
    #[serde(default)]
    pub grants: Option<GrantsSpec>,
}

/// One entry of a weapon's `perks:` list: a bare id string (a reference —
/// resolved against the table AND every inline definition) or a full
/// inline definition.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PerkRef {
    Id(String),
    Inline(PerkSpec),
}

impl PerkRef {
    /// Resolve to the perk definition (global-namespace lookup for ids).
    pub fn resolve(&self) -> &PerkSpec {
        match self {
            PerkRef::Inline(p) => p,
            PerkRef::Id(id) => {
                perk(id).unwrap_or_else(|| panic!("missing perk yaml: {id}"))
            }
        }
    }

    pub fn id(&self) -> &str {
        match self {
            PerkRef::Id(id) => id,
            PerkRef::Inline(p) => &p.id,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GrantsSpec {
    #[serde(default)]
    pub injected_element: Option<InjectedElementSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InjectedElementSpec {
    #[serde(rename = "type")]
    pub element: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WeaponSpec {
    pub id: String,
    pub name: String,
    pub slot: String,
    pub class: String,
    #[serde(default)]
    pub mod_eligibility: Option<String>,
    #[serde(default)]
    pub polarities: Vec<String>,
    #[serde(default)]
    pub exilus_polarity: Option<String>,
    #[serde(default)]
    pub magazine: Option<f64>,
    #[serde(default)]
    pub reload_seconds: Option<f64>,
    #[serde(default)]
    pub co_behavior: Option<String>,
    #[serde(default)]
    pub transform_group: Option<String>,
    #[serde(default)]
    pub transforms_from: Option<String>,
    #[serde(default)]
    pub transforms_to: Option<String>,
    pub attack: AttackSpec,
    #[serde(default)]
    pub incarnon: Option<IncarnonSpec>,
    #[serde(default)]
    pub pseudo_reload: Option<PseudoReloadSpec>,
    /// The weapon's perks: id references into `data/perks/` or inline
    /// one-off definitions (each entry of a transform group lists its own —
    /// Frenzy is active in both Dual Toxocyst forms).
    #[serde(default)]
    pub perks: Vec<PerkRef>,
}

fn one() -> f64 {
    1.0
}

/// Every weapon entry in `data/weapons/` (embedded), parsed once.
pub fn all() -> &'static [WeaponSpec] {
    static SPECS: OnceLock<Vec<WeaponSpec>> = OnceLock::new();
    SPECS.get_or_init(|| {
        crate::data::files_under("weapons/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                serde_norway::from_str::<WeaponSpec>(text)
                    .unwrap_or_else(|e| panic!("parse {p}: {e}"))
            })
            .collect()
    })
}

pub fn spec(id: &str) -> Option<&'static WeaponSpec> {
    all().iter().find(|s| s.id == id)
}

/// Every perk entry in `data/perks/` (embedded), parsed once.
pub fn perks() -> &'static [PerkSpec] {
    static PERKS: OnceLock<Vec<PerkSpec>> = OnceLock::new();
    PERKS.get_or_init(|| {
        crate::data::files_under("perks/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                let spec = serde_norway::from_str::<PerkSpec>(text)
                    .unwrap_or_else(|e| panic!("parse {p}: {e}"));
                // Convention (data/README.md): the id matches the filename.
                let stem = p.rsplit('/').next().unwrap_or(p).trim_end_matches(".yaml");
                assert!(spec.id == stem, "{p}: id '{}' != filename", spec.id);
                spec
            })
            .collect()
    })
}

/// Find an inline perk definition among the given weapon specs.
fn inline_perk_in<'a>(
    id: &str,
    specs: impl Iterator<Item = &'a WeaponSpec>,
) -> Option<&'a PerkSpec> {
    specs
        .flat_map(|w| w.perks.iter())
        .find_map(|pr| match pr {
            PerkRef::Inline(p) if p.id == id => Some(p),
            _ => None,
        })
}

/// Perk lookup over the GLOBAL namespace: the `data/perks/` table first,
/// then every weapon's inline definitions. Defining a perk inline registers
/// it globally — any other entry may reference it by bare id (uniqueness is
/// enforced by the engine test suite, so a bare id is never ambiguous).
pub fn perk(id: &str) -> Option<&'static PerkSpec> {
    perks()
        .iter()
        .find(|p| p.id == id)
        .or_else(|| inline_perk_in(id, all().iter()))
}

/// Does this weapon carry a given perk? Weapon PASSIVES are per weapon —
/// Dual Toxocyst lists `frenzy`, the Laetum lists none — so anything that
/// applies a passive must ask, never assume. A transform group's second
/// form lists its own perks (Frenzy is active in both DT forms).
pub fn has_perk(weapon_id: &str, perk_id: &str) -> bool {
    spec(weapon_id).is_some_and(|s| s.perks.iter().any(|p| p.id() == perk_id))
}

/// Registry view: the SELECTABLE weapons (transform-group base entries; an
/// Incarnon form is a form of its base weapon, not its own roster row).
pub fn roster() -> impl Iterator<Item = &'static WeaponSpec> {
    all().iter().filter(|s| s.transforms_from.is_none())
}

fn damage_type(name: &str) -> DamageType {
    match name {
        "impact" => DamageType::Impact,
        "puncture" => DamageType::Puncture,
        "slash" => DamageType::Slash,
        "heat" => DamageType::Heat,
        "cold" => DamageType::Cold,
        "electricity" => DamageType::Electricity,
        "toxin" => DamageType::Toxin,
        // Innate COMBINED elements (Laetum Incarnon's radial is 300
        // Radiation). They do not re-enter the elemental hierarchy — an
        // innate combined element stays as it is and mod elements combine
        // among themselves (wiki Damage/Elemental combination).
        "blast" => DamageType::Blast,
        "corrosive" => DamageType::Corrosive,
        "gas" => DamageType::Gas,
        "magnetic" => DamageType::Magnetic,
        "radiation" => DamageType::Radiation,
        "viral" => DamageType::Viral,
        "true" => DamageType::True,
        "void" => DamageType::Void,
        other => panic!("unknown damage type in weapon data: {other}"),
    }
}

pub fn polarity(name: &str) -> Polarity {
    match name {
        "madurai" => Polarity::Madurai,
        "naramon" => Polarity::Naramon,
        "vazarin" => Polarity::Vazarin,
        "umbra" => Polarity::Umbra,
        other => panic!("unknown polarity in weapon data: {other}"),
    }
}

/// Innate MAIN-slot polarities as an 8-slot layout (exilus excluded — the
/// UI/optimizer model treats the exilus slot separately).
pub fn innate_slots(id: &str) -> [Option<Polarity>; 8] {
    let mut out = [None; 8];
    if let Some(s) = spec(id) {
        for (i, p) in s.polarities.iter().take(8).enumerate() {
            out[i] = Some(polarity(p));
        }
    }
    out
}

/// The exilus slot's innate polarity, if the weapon has one (wiki panel's
/// "Exilus Polarity"; Dual Toxocyst: Naramon).
pub fn exilus_polarity(id: &str) -> Option<Polarity> {
    spec(id)?.exilus_polarity.as_deref().map(polarity)
}

/// Weapon behavior traits consumed by arcane/mod `requires` gates. Traits
/// describe the WEAPON (its base form's trigger family), so both forms of a
/// transform group report the base entry's trigger.
fn traits_for(s: &WeaponSpec) -> &'static [&'static str] {
    let base = s
        .transforms_from
        .as_deref()
        .and_then(spec)
        .unwrap_or(s);
    match base.attack.trigger.as_str() {
        "semi_auto" => &["semi_auto"],
        "auto" => &["auto"],
        _ => &[],
    }
}

/// Build the RAW (no evolutions, no mods) [`WeaponBase`] panel for a weapon
/// entry. `frenzy_active` folds passive-granted element injections in
/// (resolved from the transform group's base entry, where passives live).
pub fn base_panel(id: &str, frenzy_active: bool) -> WeaponBase {
    let s = spec(id).unwrap_or_else(|| panic!("unknown weapon id: {id}"));

    let mut vector = DamageVector::new();
    for (name, amount) in &s.attack.damage {
        vector = vector.with(damage_type(name), *amount);
    }

    let injected_elements = if frenzy_active {
        s.perks
            .iter()
            .filter_map(|p| p.resolve().grants.as_ref())
            .filter_map(|g| g.injected_element.as_ref())
            .map(|inj| (damage_type(&inj.element), inj.amount))
            .collect()
    } else {
        Vec::new()
    };

    let co_behavior = match s.co_behavior.as_deref() {
        Some("additive_with_base_damage") => CoBehavior::AdditiveWithBaseDamage,
        Some("inert") => CoBehavior::Inert,
        _ => CoBehavior::Independent,
    };

    // An Incarnon form's rounds are charge-backed; the locked-gauge
    // pseudo-reload supplies the sim's magazine/reload reduction.
    let (magazine_size, base_reload) = match &s.pseudo_reload {
        Some(pr) => (pr.magazine, pr.reload_seconds),
        None => (
            s.magazine.unwrap_or_else(|| panic!("{id}: magazine missing")),
            s.reload_seconds.unwrap_or_else(|| panic!("{id}: reload_seconds missing")),
        ),
    };

    let incarnon = s.incarnon.as_ref().map(|inc| IncarnonForm {
        max_charges: inc.gauge.max_rounds,
        charge_on: match inc.gauge.charge_on.as_str() {
            "weakpoint_hits" => ChargeOn::WeakpointHits,
            "direct_hits" => ChargeOn::DirectHits,
            other => panic!("{id}: unknown incarnon charge_on: {other}"),
        },
        charges_to_fill: inc.gauge.charges_to_fill,
        transmute_in: inc.transmute_in_seconds,
        transmute_out: inc.transmute_out_seconds,
        charge_rate: 0.0, // raised by evolutions (Incarnon Efficiency)
    });

    // The radial (AoE) attack part, when the weapon data declares one.
    let radial = s.attack.radial.as_ref().map(|r| {
        let mut v = DamageVector::new();
        for (t, val) in &r.damage {
            v.add(damage_type(t), *val);
        }
        RadialBase {
            base_vector: v,
            // Each stat falls back to the direct part's when unstated.
            base_crit_chance: r.crit_chance.unwrap_or(s.attack.crit_chance),
            base_crit_damage: r.crit_multiplier.unwrap_or(s.attack.crit_multiplier),
            base_status_chance: r.status_chance.unwrap_or(s.attack.status_chance),
            radius_m: r.radius_m,
            falloff_start_m: r.falloff_start_m.unwrap_or(0.0),
            falloff_reduction: r.falloff_reduction.unwrap_or(0.0),
        }
    });

    // The lingering FIELD (Torid's Toxin cloud). Each stat falls back to the
    // direct part's when unstated, same rule as the radial.
    let lingering = s.attack.lingering.as_ref().map(|f| {
        let mut v = DamageVector::new();
        for (t, val) in &f.damage {
            v.add(damage_type(t), *val);
        }
        LingeringBase {
            base_vector: v,
            base_crit_chance: f.crit_chance.unwrap_or(s.attack.crit_chance),
            base_crit_damage: f.crit_multiplier.unwrap_or(s.attack.crit_multiplier),
            base_status_chance: f.status_chance.unwrap_or(s.attack.status_chance),
            tick_rate: f.tick_rate,
            duration_s: f.duration_s,
            radius_m: f.radius_m,
            falloff_start_m: f.falloff_start_m.unwrap_or(0.0),
            falloff_reduction: f.falloff_reduction.unwrap_or(0.0),
            stacking: match f.stacking.as_str() {
                "stack" => FieldStacking::Stack,
                "refresh" => FieldStacking::Refresh,
                other => panic!("{id}: unknown lingering stacking: {other}"),
            },
        }
    });

    WeaponBase {
        base_vector: vector,
        base_crit_chance: s.attack.crit_chance,
        base_crit_damage: s.attack.crit_multiplier,
        base_status_chance: s.attack.status_chance,
        base_fire_rate: s.attack.fire_rate,
        base_multishot: s.attack.multishot,
        buff_multishot_bonus: 0.0,
        buff_ms_max_stacks: 0,
        magazine_size,
        base_reload,
        innate_co_per_type: 0.0,
        co_behavior,
        co_base_fraction: 1.0,
        injected_elements,
        traits: traits_for(s),
        incarnon,
        radial,
        lingering,
        // All raised by evolutions, never by the raw weapon data.
        evo_fire_rate_bonus: 0.0,
        post_mod_crit_chance: 0.0,
        post_mod_status_chance: 0.0,
        headshot_damage_bonus: 0.0,
        noncrit_bonus: None,
        plain_hit_bonus: None,
        reload_on_headshot: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_weapon_roster() {
        assert!(spec("dual_toxocyst").is_some());
        assert!(spec("dual_toxocyst_incarnon").is_some());
        // The roster lists base entries only.
        assert!(roster().all(|s| s.transforms_from.is_none()));
    }

    /// The Torid is the first PRIMARY weapon and the first weapon with a
    /// lingering FIELD, so this pins what the loader must produce for it — every
    /// number cross-checked wiki data module == WFCD.
    #[test]
    fn torid_loads_both_forms_with_its_field_and_direct_hit_gauge() {
        use crate::loadout::{ChargeOn, FieldStacking};
        let b = base_panel("torid", false);
        assert!((b.base_vector.get(DamageType::Toxin) - 100.0).abs() < 1e-9);
        assert!((b.base_crit_chance - 0.15).abs() < 1e-9);
        assert!((b.base_crit_damage - 2.0).abs() < 1e-9);
        assert!((b.base_status_chance - 0.23).abs() < 1e-9);
        assert!((b.base_fire_rate - 1.5).abs() < 1e-9);
        assert!((b.magazine_size - 5.0).abs() < 1e-9);
        assert!((b.base_reload - 1.7).abs() < 1e-9);
        // "Multiplying" in the CO catalog = an INDEPENDENT multiplier, the
        // opposite of the Laetum's "Adding".
        assert_eq!(b.co_behavior, CoBehavior::Independent);
        // The FIELD, with its own stats: note status 25% where the impact is 23%.
        let f = b.lingering.as_ref().expect("torid leaves a cloud");
        assert!((f.base_vector.get(DamageType::Toxin) - 40.0).abs() < 1e-9);
        assert!((f.tick_rate - 1.0).abs() < 1e-9);
        assert!((f.duration_s - 10.0).abs() < 1e-9);
        assert!((f.base_status_chance - 0.25).abs() < 1e-9);
        assert!((f.radius_m - 3.0).abs() < 1e-9);
        // To ZERO at the rim, unlike the Laetum radial's 0.2.
        assert!((f.falloff_reduction - 1.0).abs() < 1e-9);
        assert_eq!(f.stacking, FieldStacking::Stack, "unverified default (M12)");
        assert!(b.radial.is_none(), "the cloud is a field, not an explosion");

        // The Incarnon form: a continuous beam, ONE attack part (its 2.3 m
        // radius is explicitly not a separate instance), charged by DIRECT hits.
        let i = base_panel("torid_incarnon", false);
        assert!((i.base_vector.get(DamageType::Toxin) - 51.0).abs() < 1e-9);
        assert!((i.base_crit_chance - 0.29).abs() < 1e-9);
        assert!((i.base_crit_damage - 3.1).abs() < 1e-9);
        assert!((i.base_status_chance - 0.39).abs() < 1e-9);
        assert!((i.base_fire_rate - 8.0).abs() < 1e-9, "ticks per second");
        assert!(i.radial.is_none(), "the damage radius is not its own instance");
        assert!(i.lingering.is_none(), "no cloud in Incarnon form");
        let g = i.incarnon.as_ref().expect("torid_incarnon has a gauge");
        assert_eq!(g.charge_on, ChargeOn::DirectHits);
        assert!((g.charges_to_fill - 5.0).abs() < 1e-9);
        assert!((g.max_charges - 170.0).abs() < 1e-9);
        assert!((g.transmute_in - 1.7).abs() < 1e-9, "= the base reload");
        // Charge-backed magazine, so the pseudo-reload supplies the sim's.
        assert!((i.magazine_size - 170.0).abs() < 1e-9);
        assert!((i.base_reload - 2.7).abs() < 1e-9);
    }

    /// The roster is data-driven: dropping in `data/weapons/primary/` publishes
    /// a primary weapon with no code change, and its mod pool and arcane slot
    /// follow from `mod_eligibility` and `slot`.
    #[test]
    fn the_primary_slot_needed_no_code() {
        let t = spec("torid").expect("torid");
        assert_eq!(t.slot, "primary");
        assert_eq!(t.mod_eligibility.as_deref(), Some("rifle_mods"));
        assert!(roster().any(|s| s.id == "torid"), "selectable");
        assert!(
            !roster().any(|s| s.id == "torid_incarnon"),
            "a form is not its own roster row"
        );
        assert!(
            !crate::mods_data::class_pool("rifle").is_empty(),
            "the rifle pool has to exist for it to be equippable"
        );
    }

    #[test]
    fn base_panels_match_the_wiki_values() {
        let b = base_panel("dual_toxocyst", true);
        assert!((b.base_vector.get(DamageType::Puncture) - 60.0).abs() < 1e-9);
        assert!((b.base_crit_chance - 0.05).abs() < 1e-9);
        assert!((b.magazine_size - 12.0).abs() < 1e-9);
        assert_eq!(b.injected_elements, vec![(DamageType::Toxin, 1.0)]);
        assert_eq!(b.traits, &["semi_auto"]);
        assert!(b.incarnon.is_none());

        let i = base_panel("dual_toxocyst_incarnon", false);
        assert!((i.base_crit_damage - 3.0).abs() < 1e-9);
        assert!((i.magazine_size - 270.0).abs() < 1e-9);
        assert!((i.base_reload - 3.35).abs() < 1e-9);
        let inc = i.incarnon.expect("incarnon block");
        assert!((inc.max_charges - 270.0).abs() < 1e-9);
        assert!((inc.transmute_in - 2.35).abs() < 1e-9);
        assert!((inc.transmute_out - 1.0).abs() < 1e-9);
        assert!(i.injected_elements.is_empty());
        // Traits come from the transform group's BASE entry.
        assert_eq!(i.traits, &["semi_auto"]);
    }

    /// data/README.md's promotion rule, enforced: perk ids are GLOBALLY
    /// unique. A table perk may be referenced by many items; an inline perk
    /// may exist in exactly ONE item and must not shadow a table id — two
    /// carriers means it should have been promoted to data/perks/.
    #[test]
    fn perk_ids_are_globally_unique_across_table_and_inlines() {
        use std::collections::HashMap;
        let mut home: HashMap<&str, String> = HashMap::new();
        for p in perks() {
            let prev = home.insert(&p.id, format!("data/perks/{}.yaml", p.id));
            assert!(prev.is_none(), "duplicate table perk id: {}", p.id);
        }
        for w in all() {
            for pr in &w.perks {
                if let PerkRef::Inline(p) = pr {
                    if let Some(other) = home.get(p.id.as_str()) {
                        panic!(
                            "inline perk '{}' in weapon '{}' collides with {} — \
                             promote it to data/perks/ and reference the id",
                            p.id, w.id, other
                        );
                    }
                    home.insert(&p.id, format!("inline in weapon '{}'", w.id));
                }
            }
        }
        // Every bare-id reference must resolve (no dangling perk refs —
        // caught here at test time instead of a runtime panic mid-sim).
        for w in all() {
            for pr in &w.perks {
                if let PerkRef::Id(id) = pr {
                    assert!(
                        perk(id).is_some(),
                        "weapon '{}' references unknown perk '{}'",
                        w.id, id
                    );
                }
            }
        }
    }

    #[test]
    fn perks_accept_both_reference_and_inline_forms() {
        let yaml = r#"
id: test_gun
name: Test Gun
slot: secondary
class: pistols
magazine: 10
reload_seconds: 1.0
attack: { trigger: auto, fire_rate: 5.0, crit_chance: 0.1, crit_multiplier: 2.0, status_chance: 0.1, damage: { toxin: 10.0 } }
perks:
  - frenzy
  - id: one_off
    grants: { injected_element: { type: heat, amount: 0.5 } }
"#;
        let s: WeaponSpec = serde_norway::from_str(yaml).unwrap();
        assert_eq!(s.perks[0].id(), "frenzy");
        assert!(s.perks[0].resolve().grants.is_some()); // table lookup works
        assert_eq!(s.perks[1].id(), "one_off");
        let g = s.perks[1].resolve().grants.as_ref().unwrap();
        let inj = g.injected_element.as_ref().unwrap();
        assert_eq!(inj.element, "heat");
        // An inline definition registers in the perk namespace: a bare-id
        // reference from ANOTHER entry finds it.
        let found = inline_perk_in("one_off", std::iter::once(&s)).expect("inline registered");
        assert!(found.grants.is_some());
        assert!(inline_perk_in("nope", std::iter::once(&s)).is_none());
    }

    #[test]
    fn innate_slots_come_from_the_yaml_polarities() {
        let s = innate_slots("dual_toxocyst");
        assert_eq!(s[0], Some(Polarity::Madurai));
        assert_eq!(s[1], Some(Polarity::Naramon));
        assert_eq!(s[2], None);
    }
}

#[cfg(test)]
mod laetum_tests {
    use super::*;

    #[test]
    fn laetum_incarnon_carries_its_radial_part() {
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        let r = b.radial.as_ref().expect("laetum_incarnon declares a radial part");
        assert_eq!(r.base_vector.total(), 300.0, "300 Radiation");
        assert_eq!(r.radius_m, 2.0);
        assert_eq!(r.falloff_reduction, 0.2);
        // The direct part is pure Impact 100.
        assert_eq!(b.base_vector.total(), 100.0);
    }

    #[test]
    fn the_sim_actually_applies_the_radial() {
        use crate::dummy::{monte_carlo, DummyParams, TargetParams};
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::AssumedMax);
        let parts = vec![crate::dummy::BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        }];
        let params =
            DummyParams::from_panel(&p, TargetParams::training_dummy(), parts, 10.0);
        assert!(params.radial.is_some(), "params carry the radial");
        let s = monte_carlo(&params, 30, 7);
        assert!(
            s.source_damage.radial > 0.0,
            "the radial must land damage, got {:?}",
            s.source_damage
        );
        // 300 Radiation vs 100 Impact: the radial dominates.
        assert!(
            s.source_damage.radial > s.source_damage.direct,
            "radial {} should exceed direct {}",
            s.source_damage.radial,
            s.source_damage.direct
        );
    }

    /// Two-stage damage: the direct hit lands first, then the explosion,
    /// both on the SAME enemy (user, 2026-07-29). With Laetum's 100 Impact
    /// direct and 300 Radiation radial, and no body-part multiplier on the
    /// explosion, a body-only engagement must settle at radial ~ 3x direct.
    #[test]
    fn direct_then_radial_lands_at_the_declared_ratio() {
        use crate::dummy::{monte_carlo, DummyParams};
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::AssumedMax);
        let specs = crate::enemy_data::all();
        let spec = specs.iter().find(|e| e.id == "thrax_centurion").unwrap();
        let target = spec
            .target_params(1, false, false, crate::dummy::TargetMode::InstantRespawn)
            .unwrap();
        let parts = vec![crate::dummy::BodyPart {
            name: "body".into(), aim_weight: 1.0, multiplier: 1.0,
            is_head: false, crit_bonus: false,
        }];
        let params = DummyParams::from_panel(&p, target, parts, 30.0);
        let s = monte_carlo(&params, 40, 3);
        let d = s.source_damage.direct;
        let r = s.source_damage.radial;
        let ratio = r / d;
        assert!(
            (ratio - 3.0).abs() < 0.25,
            "radial/direct should be ~3 (300 vs 100), got {ratio:.2} (direct {d:.0}, radial {r:.0})"
        );
    }

    /// The cycle economy is DATA, not a hardcoded weapon: Laetum fills in
    /// 12 weakpoint hits and both transitions cost its 2.0 s reload, while
    /// Incarnon Efficiency (+50% charge) drops the fill to 8 hits.
    #[test]
    fn the_cycle_reads_its_economy_from_the_weapon_data() {
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        let f = b.incarnon.expect("incarnon economy");
        assert_eq!(f.charges_to_fill, 12.0);
        assert_eq!(f.max_charges, 216.0);
        assert_eq!(f.transmute_in, 2.0);
        // Reverts are a uniform 1 s across every weapon until measured.
        assert_eq!(f.transmute_out, 1.0);
        assert_eq!(f.charge_rate, 0.0);

        let eff = WeaponBase::from_data("laetum_incarnon", true, &["laetum_incarnon_efficiency"]);
        let g = eff.incarnon.expect("incarnon economy");
        assert_eq!(g.charge_rate, 0.5);
        // 12 / 1.5 = 8 hits (wiki).
        assert_eq!((g.charges_to_fill / (1.0 + g.charge_rate)).ceil() as u32, 8);

        // Dual Toxocyst keeps its own numbers.
        let dt = WeaponBase::from_data("dual_toxocyst_incarnon", true, &[]);
        let d = dt.incarnon.expect("incarnon economy");
        assert_eq!(d.charges_to_fill, 9.0);
        assert_eq!(d.transmute_in, 2.35);
    }

    /// Overwhelming Attrition earns its stacks in-run: a plain hit (no
    /// crit, no status) grants one, the buff multiplies later instances,
    /// and a timeout drops ONE stack rather than the whole buff.
    #[test]
    fn overwhelming_attrition_earns_and_pays_out() {
        use crate::dummy::{monte_carlo, DummyParams, TargetParams};
        let parts = vec![crate::dummy::BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        }];
        let run = |evos: &[&str]| {
            let b = WeaponBase::from_data("laetum_incarnon", true, evos);
            let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::AssumedMax);
            let params =
                DummyParams::from_panel(&p, TargetParams::training_dummy(), parts.clone(), 20.0);
            (params.plain_hit_bonus.is_some(), monte_carlo(&params, 40, 11).mean_effective_damage)
        };
        let (has_none, without) = run(&[]);
        let (has_buff, with) = run(&["laetum_overwhelming_attrition"]);
        assert!(!has_none && has_buff, "the evolution must carry the buff");
        assert!(
            with > without * 1.5,
            "the earned stacks must show up: {without:.0} -> {with:.0}"
        );
    }

    /// Lethal Rearmament used to load INERT. It is an on-headshot stacking
    /// reload-speed buff, and reload speed also scales the Incarnon
    /// transmute animations — so on a weapon whose whole cycle is
    /// reload-bound it must buy back real time.
    #[test]
    fn lethal_rearmament_shortens_the_cycle_not_just_reloads() {
        use crate::dummy::{monte_carlo, DummyParams, TargetParams};
        // 100% headshots so the trigger fires on every landed pellet.
        let parts = vec![crate::dummy::BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        }];
        let run = |evos: &[&str]| {
            let b = WeaponBase::from_data("laetum_incarnon", true, evos);
            let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::Emergent);
            let params =
                DummyParams::from_panel(&p, TargetParams::training_dummy(), parts.clone(), 60.0);
            let m = monte_carlo(&params, 24, 7);
            (params.reload_on_headshot.is_some(), m.mean_effective_damage)
        };
        let (has_none, without) = run(&["laetum_evo1_incarnon_form"]);
        let (has_buff, with) = run(&["laetum_evo1_incarnon_form", "laetum_lethal_rearmament"]);
        assert!(!has_none, "no evolution, no buff");
        assert!(has_buff, "the evolution must carry the buff (it loaded INERT before)");
        assert!(
            with > without,
            "less time reloading and transmuting = more damage in the same 60 s:              {without:.0} -> {with:.0}"
        );
    }

    /// M10 (in-game, 2026-07-30): a reload-speed buff is live in BOTH
    /// forms; it does NOT touch the gauge, but it DOES shorten transmute
    /// in and out. The observable consequence in a fixed engagement is
    /// that more cycles fit — the gauge requirement staying put.
    #[test]
    fn a_reload_buff_shortens_the_transmutes_but_never_the_gauge() {
        use crate::dummy::{run_once, DummyParams, TargetParams};
        use crate::rng::Rng;
        let parts = vec![crate::dummy::BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        }];
        let params = |evos: &[&str], pin: bool| {
            let inc = WeaponBase::from_data("laetum_incarnon", true, evos);
            let base = WeaponBase::from_data("laetum", true, evos);
            let pol = crate::loadout::StackPolicy::Emergent;
            let pi = crate::loadout::resolve(&inc, &[], pol);
            let pb = crate::loadout::resolve(&base, &[], pol);
            let mut d = DummyParams::incarnon_cycle_from_panels(
                &pi,
                &pb,
                false,
                crate::dummy::LockMode::Initial(0),
                TargetParams::training_dummy(),
                parts.clone(),
                300.0,
            );
            if !pin {
                if let Some(b) = d.reload_on_headshot.as_mut() {
                    b.initial_stacks = 0;
                }
            } else if let Some(b) = d.reload_on_headshot.as_mut() {
                b.pinned = true;
            }
            d
        };
        let cycle_charges = |d: &DummyParams| {
            d.cycle.as_ref().expect("the incarnon cycle").charges_to_fill
        };

        let off = params(&["laetum_evo1_incarnon_form"], false);
        let on = params(
            &["laetum_evo1_incarnon_form", "laetum_lethal_rearmament"],
            true,
        );
        assert_eq!(
            cycle_charges(&off),
            cycle_charges(&on),
            "reload speed must NOT shorten gauge building (M10)"
        );

        // Less downtime in a fixed engagement = more shots fired. (Transform
        // COUNT is gauge-bound, not animation-bound, so it barely moves.)
        let s_off = run_once(&off, &mut Rng::new(5)).shots;
        let s_on = run_once(&on, &mut Rng::new(5)).shots;
        assert!(
            s_on > s_off,
            "shorter transmutes = more shots in the same 300 s: {s_off} -> {s_on}"
        );
    }

    /// The two tier-5 Attritions sit in DIFFERENT brackets, and the wiki
    /// says so explicitly: Overwhelming is "additive to base damage bonuses
    /// such as Hornet Strike", Devouring is "multiplicative" to them. The
    /// observable difference is DILUTION — an additive bonus loses relative
    /// value as the base-damage bucket grows, a multiplicative one does not.
    #[test]
    fn overwhelming_attrition_is_diluted_by_base_damage_mods() {
        use crate::dummy::{monte_carlo, DummyParams, TargetParams};
        let parts = vec![crate::dummy::BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        }];
        let pool = crate::mods_data::pistol_pool();
        let hornet: Vec<&crate::loadout::ModDef> =
            pool.iter().filter(|m| m.id == "hornet_strike").collect();
        let gain = |evos: &[&str], mods: &[&crate::loadout::ModDef]| {
            let run = |e: &[&str]| {
                let b = WeaponBase::from_data("laetum_incarnon", true, e);
                let p = crate::loadout::resolve(&b, mods, crate::loadout::StackPolicy::AssumedMax);
                let params =
                    DummyParams::from_panel(&p, TargetParams::training_dummy(), parts.clone(), 20.0);
                monte_carlo(&params, 60, 5).mean_effective_damage
            };
            run(evos) / run(&[])
        };
        let bare = gain(&["laetum_overwhelming_attrition"], &[]);
        let modded = gain(&["laetum_overwhelming_attrition"], &hornet);
        assert!(
            bare > modded * 1.5,
            "an ADDITIVE bonus must lose relative value once Hornet Strike              fills the same bucket: bare {bare:.2}x vs modded {modded:.2}x"
        );
        // …and the MULTIPLICATIVE sibling must NOT be diluted at all.
        let d_bare = gain(&["laetum_devouring_attrition"], &[]);
        let d_modded = gain(&["laetum_devouring_attrition"], &hornet);
        let drift = (d_bare / d_modded - 1.0).abs();
        assert!(
            drift < 0.15,
            "a MULTIPLICATIVE bonus keeps its relative value: bare {d_bare:.2}x              vs modded {d_modded:.2}x (drift {drift:.2})"
        );
    }

    /// Rapid Wrath's +20% fire rate joins the ORDINARY fire-rate bucket —
    /// the same additive one the mods feed (user, 2026-07-29). Additive with
    /// Gunslinger's +72%: 6.67 x (1 + 0.72 + 0.20) = 12.81, NOT the
    /// multiplicative 6.67 x 1.72 x 1.20 = 13.77.
    #[test]
    fn rapid_wrath_is_additive_with_fire_rate_mods() {
        let pool = crate::mods_data::pistol_pool();
        let gunslinger: Vec<&crate::loadout::ModDef> =
            pool.iter().filter(|m| m.id == "gunslinger").collect();
        let fr = |evos: &[&str], mods: &[&crate::loadout::ModDef]| {
            let b = WeaponBase::from_data("laetum_incarnon", true, evos);
            crate::loadout::resolve(&b, mods, crate::loadout::StackPolicy::AssumedMax).fire_rate
        };
        let base = fr(&[], &[]);
        assert!((base - 6.67).abs() < 1e-9, "base fire rate {base}");
        assert!((fr(&["laetum_rapid_wrath"], &[]) - 6.67 * 1.20).abs() < 1e-9);
        assert!((fr(&[], &gunslinger) - 6.67 * 1.72).abs() < 1e-9);
        let both = fr(&["laetum_rapid_wrath"], &gunslinger);
        assert!(
            (both - 6.67 * 1.92).abs() < 1e-9,
            "must be ADDITIVE (12.81), got {both} — multiplicative would be {}",
            6.67 * 1.72 * 1.20
        );
    }

    /// The wiki CO catalog rates BOTH Laetum forms "Adding" at a 100% base
    /// fraction, and notes the bonus "multiplies properly with Devouring
    /// Attrition". Two consequences the engine must honour: CO reaches the
    /// DIRECT hit only (never the 300 Radiation radial), and Devouring — its
    /// own multiplicative bracket — multiplies the already-CO-boosted value.
    #[test]
    fn condition_overload_is_adding_direct_only_and_devouring_stacks_on_top() {
        use crate::dummy::{monte_carlo, DummyParams, TargetParams};
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        assert_eq!(b.co_behavior, crate::loadout::CoBehavior::AdditiveWithBaseDamage);
        // 160/160 and 100/100 in the catalog: the whole base feeds the bonus.
        assert!((b.co_base_fraction - 1.0).abs() < 1e-9);

        let pool = crate::mods_data::pistol_pool();
        let co: Vec<&crate::loadout::ModDef> =
            pool.iter().filter(|m| m.id == "galvanized_shot").collect();
        let parts = vec![crate::dummy::BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        }];
        let sources = |evos: &[&str], mods: &[&crate::loadout::ModDef]| {
            let b = WeaponBase::from_data("laetum_incarnon", true, evos);
            let p = crate::loadout::resolve(&b, mods, crate::loadout::StackPolicy::AssumedMax);
            let params =
                DummyParams::from_panel(&p, TargetParams::training_dummy(), parts.clone(), 20.0);
            let s = monte_carlo(&params, 60, 17).source_damage;
            (s.direct, s.radial)
        };
        // The radial is INDIFFERENT to a CO mod; the direct hit is not.
        let (d0, r0) = sources(&[], &[]);
        let (d1, r1) = sources(&[], &co);
        assert!(d1 > d0 * 1.05, "CO must lift the direct hit: {d0:.0} -> {d1:.0}");
        assert!(
            (r1 / r0 - 1.0).abs() < 0.05,
            "CO must NOT reach the radial: {r0:.0} -> {r1:.0}"
        );
    }

    #[test]
    fn resolving_keeps_the_radial() {
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::AssumedMax);
        let r = p.radial.expect("resolved panel keeps the radial");
        assert!((r.damage.total() - 300.0).abs() < 1e-9, "got {}", r.damage.total());
    }
}

#[cfg(test)]
mod passive_tests {
    use super::*;

    /// A weapon PASSIVE belongs to the weapon that lists it. Frenzy is Dual
    /// Toxocyst's (both forms); the Laetum has none. Hardcoding it handed
    /// DT's x2.5-on-headshot fire rate to every transform weapon.
    #[test]
    fn frenzy_belongs_only_to_the_weapon_that_lists_it() {
        assert!(has_perk("dual_toxocyst", "frenzy"));
        assert!(has_perk("dual_toxocyst_incarnon", "frenzy"));
        assert!(!has_perk("laetum", "frenzy"));
        assert!(!has_perk("laetum_incarnon", "frenzy"));
        assert!(!has_perk("dual_toxocyst", "no_such_perk"));
    }
}
