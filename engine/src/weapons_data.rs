//! Weapon data loader: `data/weapons/*.yaml` → [`WeaponBase`] + registry
//! metadata (CORE.md §2.3: weapon numbers are DATA; the engine only holds
//! rules). The yamls are the source of record — `loadout`'s per-weapon
//! constructors delegate here, and the web registry derives its weapon list,
//! tags, polarities and form descriptors from the same specs.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::damage::{DamageType, DamageVector};
use crate::loadout::{CoBehavior, IncarnonForm, RadialBase, WeaponBase};
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
    /// Weakpoint hits needed to fill the gauge (DT 9, Laetum 12).
    pub charges_to_fill: f64,
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
        // All raised by evolutions, never by the raw weapon data.
        evo_fire_rate_bonus: 0.0,
        post_mod_crit_chance: 0.0,
        post_mod_status_chance: 0.0,
        headshot_damage_bonus: 0.0,
        noncrit_bonus: None,
        plain_hit_bonus: None,
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
        assert_eq!(f.transmute_out, 2.0);
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
