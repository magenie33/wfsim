//! Pipeline layer [1]: mod resolution — a chosen mod set becomes panel stats.
//!
//! Buckets (docs/MECHANICS.md, docs/GLOSSARY.md): every relative bonus of one
//! kind sums additively into its bucket, then buckets combine by their real
//! rules (crit chance = base × (1 + Σcc); elemental amount = ModifiedBase ×
//! bonus; reload time = base / (1 + Σreload); …). Elemental entries enter the
//! layer-[2] hierarchy in **mod order** ([`crate::elements`]).
//!
//! Conditional/stacking effects resolve under a [`StackPolicy`] — today only
//! `AssumedMax` (docs/OPTIMIZER.md §3: every stacking buff at full stacks).

use crate::damage::{DamageType, DamageVector};
use crate::elements::{self, ElementalInput};
use crate::mods::Polarity;

/// One resolved effect of a mod at its equipped rank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModEffect {
    /// Additive base-damage bucket (Hornet Strike).
    BaseDamage(f64),
    /// Additive multishot bucket (total pellets = base × (1 + Σ)).
    Multishot(f64),
    /// Relative crit chance (base_cc × (1 + Σ)).
    CritChance(f64),
    /// Relative crit damage (base_cd × (1 + Σ)).
    CritDamage(f64),
    /// Relative status chance.
    StatusChance(f64),
    /// Relative fire rate (negative for Creeping Bullseye's downside).
    FireRate(f64),
    /// Reload speed bonus (time = base / (1 + Σ)).
    ReloadSpeed(f64),
    /// Status-damage bucket (Pistol Elementalist) — scales status payloads.
    StatusDamage(f64),
    /// Primary element: ModifiedBase × bonus enters the hierarchy at this
    /// mod's position.
    Element(DamageType, f64),
    /// Combined-element mod (Magnetic Might): added outside the hierarchy.
    CombinedElement(DamageType, f64),
    /// Galvanized Diffusion's on-kill multishot stacks.
    OnKillMultishot { per_stack: f64, max_stacks: u32 },
    /// Condition Overload payload (Galvanized Shot): +per_stack direct
    /// damage per status TYPE on the target, per stack.
    ConditionOverload { per_stack: f64, max_stacks: u32 },
}

/// A mod as the resolver sees it (stats at the equipped rank).
#[derive(Debug, Clone)]
pub struct ModDef {
    pub id: &'static str,
    pub base_drain: u32,
    pub polarity: Polarity,
    /// Mods sharing a family are mutually exclusive (wiki Incompatible).
    pub family: Option<&'static str>,
    pub effects: Vec<ModEffect>,
}

impl ModDef {
    /// The primary element this mod adds, if any (position-sensitive).
    pub fn primary_element(&self) -> Option<DamageType> {
        self.effects.iter().find_map(|e| match e {
            ModEffect::Element(t, _) => Some(*t),
            _ => None,
        })
    }
}

/// How stacking/conditional effects are valued (docs/OPTIMIZER.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackPolicy {
    /// Full stacks / 100% uptime on every conditional buff.
    AssumedMax,
}

/// How the Condition Overload bonus behaves — PER WEAPON (user,
/// 2026-07-24: "有的武器是独立的加成，有的武器是当基础伤害的，有的还
/// 加成不到"; the wiki CO-mechanic catalog classifies weapons):
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoBehavior {
    /// Joins the base-damage bucket (additive with Hornet Strike):
    /// direct hit × (1 + bd + co × types) / (1 + bd).
    AdditiveWithBaseDamage,
    /// A free-standing final multiplier on direct hits:
    /// direct hit × (1 + co × types).
    Independent,
    /// The bonus simply does not apply on this weapon.
    Inert,
}

/// A weapon's unmodded panel (fixed evolutions folded in — they alter the
/// weapon's BASE stats before mods).
#[derive(Debug, Clone)]
pub struct WeaponBase {
    pub base_vector: DamageVector,
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    pub base_fire_rate: f64,
    /// Stored pellet count (wiki Multishot).
    pub base_multishot: f64,
    /// Extra additive multishot from non-mod sources at assumed-max
    /// (Fevered Frenzy's 20 stacks = +1.0).
    pub buff_multishot_bonus: f64,
    pub magazine_size: f64,
    pub base_reload: f64,
    /// This weapon's Condition Overload behavior class.
    pub co_behavior: CoBehavior,
    /// Buff-injected elements as RELATIVE bonuses (element, bonus): each
    /// contributes ModifiedBase × bonus at the END of the hierarchy
    /// (rule 8) — Frenzy's +100% Toxin on the base Dual Toxocyst.
    pub injected_elements: Vec<(DamageType, f64)>,
}

impl WeaponBase {
    /// Dual Toxocyst Incarnon Form with the fixed evolution build
    /// (data/builds/dual_toxocyst_default.yaml): Fevered Frenzy +50 base
    /// pro-rata (75→125 scale on the form's 15/37.5/22.5) and 20 stacks
    /// (+100% multishot); Commodore's Fortune +0.20 into BASE crit chance;
    /// Evolved Autoloader regenerates only while holstered — no effect on
    /// the wielded pseudo-reload (1.0 s revert + 2.35 s transmute).
    /// `frenzy_active`: the Frenzy passive WORKS while transformed
    /// (user-confirmed 2026-07-24) — folds its +100% Toxin injection in.
    pub fn dual_toxocyst_incarnon(frenzy_active: bool) -> Self {
        Self {
            base_vector: DamageVector::new()
                .with(DamageType::Impact, 25.0)
                .with(DamageType::Puncture, 62.5)
                .with(DamageType::Slash, 37.5),
            base_crit_chance: 0.31, // 11% + Commodore's Fortune 20%
            base_crit_damage: 3.0,
            base_status_chance: 0.43,
            base_fire_rate: 4.5,
            base_multishot: 1.0,
            buff_multishot_bonus: 1.0, // Fevered Frenzy at 20 stacks
            magazine_size: 270.0,
            base_reload: 3.35,
            // Per Carnage Reign's recorded "adding behavior" (the CO
            // catalog entry for this weapon) — to re-verify per form.
            co_behavior: CoBehavior::AdditiveWithBaseDamage,
            injected_elements: if frenzy_active {
                vec![(DamageType::Toxin, 1.0)]
            } else {
                Vec::new()
            },
        }
    }

    /// Dual Toxocyst **base form** with the same fixed evolution build:
    /// vector ×5/3 (Fevered +50 pro-rata), Commodore 5% → 25% base cc.
    /// `frenzy_active` folds the passive's +100% Toxin injection into the
    /// panel (approximation: 100% Frenzy uptime — exact under a Permanent
    /// lock; near-exact at 100% headshot aim).
    pub fn dual_toxocyst_base(frenzy_active: bool) -> Self {
        Self {
            base_vector: DamageVector::new()
                .with(DamageType::Impact, 12.5)
                .with(DamageType::Puncture, 100.0)
                .with(DamageType::Slash, 12.5),
            base_crit_chance: 0.25, // 5% + Commodore's Fortune 20%
            base_crit_damage: 2.0,
            base_status_chance: 0.37,
            base_fire_rate: 1.0, // semi-auto; Frenzy ×2.5 applies live
            base_multishot: 1.0,
            buff_multishot_bonus: 1.0, // Fevered Frenzy at 20 stacks
            magazine_size: 12.0,
            base_reload: 2.35,
            co_behavior: CoBehavior::AdditiveWithBaseDamage,
            injected_elements: if frenzy_active {
                vec![(DamageType::Toxin, 1.0)]
            } else {
                Vec::new()
            },
        }
    }
}

/// The resolved panel: everything the dummy sim needs from layers [1]+[2].
#[derive(Debug, Clone)]
pub struct ResolvedPanel {
    /// Post-hierarchy damage vector (physical × (1+bd) + combined elements).
    pub damage: DamageVector,
    /// ModifiedBase = unmodded total × (1 + Σ base damage) — the base of
    /// every status-payload formula (elemental portions excluded).
    pub modified_base: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    pub status_chance: f64,
    pub fire_rate: f64,
    pub multishot: f64,
    pub magazine_size: f64,
    pub reload_seconds: f64,
    /// Σ reload-speed bonuses — transitions (Incarnon transmute/revert)
    /// scale by the same formula: time = base / (1 + this).
    pub reload_bonus: f64,
    /// Σ base-damage bonuses (needed live when CO joins this bucket).
    pub base_damage_bonus: f64,
    /// Σ (CO per_stack × stacks) under the policy — applied per this
    /// weapon's [`CoBehavior`], DIRECT HITS ONLY.
    pub co_per_type: f64,
    pub co_behavior: CoBehavior,
    /// (1 + Σ status damage) — multiplies status payload values.
    pub status_damage_mult: f64,
    /// (element, 1 + Σ that element's bonuses) — the elemental bracket of
    /// DoT tick formulas (only literal same-element mods count).
    pub elem_dot_bonus: Vec<(DamageType, f64)>,
}

/// Resolve a mod set in slot order against a weapon base.
pub fn resolve(base: &WeaponBase, mods: &[&ModDef], policy: StackPolicy) -> ResolvedPanel {
    let StackPolicy::AssumedMax = policy;
    let (mut bd, mut ms, mut cc, mut cd, mut sc, mut fr, mut rl, mut sd) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut co = 0.0;
    let mut elem_bonus: Vec<(DamageType, f64)> = Vec::new();

    for m in mods {
        for e in &m.effects {
            match *e {
                ModEffect::BaseDamage(v) => bd += v,
                ModEffect::Multishot(v) => ms += v,
                ModEffect::CritChance(v) => cc += v,
                ModEffect::CritDamage(v) => cd += v,
                ModEffect::StatusChance(v) => sc += v,
                ModEffect::FireRate(v) => fr += v,
                ModEffect::ReloadSpeed(v) => rl += v,
                ModEffect::StatusDamage(v) => sd += v,
                ModEffect::Element(t, v) | ModEffect::CombinedElement(t, v) => {
                    if let Some(x) = elem_bonus.iter_mut().find(|(a, _)| *a == t) {
                        x.1 += v;
                    } else {
                        elem_bonus.push((t, v));
                    }
                }
                ModEffect::OnKillMultishot {
                    per_stack,
                    max_stacks,
                } => ms += per_stack * max_stacks as f64,
                ModEffect::ConditionOverload {
                    per_stack,
                    max_stacks,
                } => co += per_stack * max_stacks as f64,
            }
        }
    }

    let modified_base = base.base_vector.total() * (1.0 + bd);
    let physical = base.base_vector.scale(1.0 + bd);

    // Elemental hierarchy input, in mod order (first placement establishes
    // an element's position; later same-element mods merge there).
    let mut input = ElementalInput::default();
    for m in mods {
        for e in &m.effects {
            match *e {
                ModEffect::Element(t, v) => input.push(t, modified_base * v),
                ModEffect::CombinedElement(t, v) => {
                    input.direct_secondary.push((t, modified_base * v))
                }
                _ => {}
            }
        }
    }
    for &(t, bonus) in &base.injected_elements {
        input.injected.push((t, modified_base * bonus));
    }
    let damage = elements::combine(&physical, &input);

    ResolvedPanel {
        damage,
        modified_base,
        crit_chance: base.base_crit_chance * (1.0 + cc),
        crit_damage: base.base_crit_damage * (1.0 + cd),
        status_chance: base.base_status_chance * (1.0 + sc),
        fire_rate: base.base_fire_rate * (1.0 + fr),
        multishot: base.base_multishot * (1.0 + base.buff_multishot_bonus + ms),
        magazine_size: base.magazine_size,
        reload_seconds: base.base_reload / (1.0 + rl),
        reload_bonus: rl,
        base_damage_bonus: bd,
        co_behavior: base.co_behavior,
        co_per_type: co,
        status_damage_mult: 1.0 + sd,
        elem_dot_bonus: elem_bonus.into_iter().map(|(t, v)| (t, 1.0 + v)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DamageType::*;

    fn m(id: &'static str, effects: Vec<ModEffect>) -> ModDef {
        ModDef {
            id,
            base_drain: 10,
            polarity: Polarity::Madurai,
            family: None,
            effects,
        }
    }

    #[test]
    fn dual_toxocyst_panel_resolves_by_hand() {
        // Hornet +220%, Frostbite (cold 60/SC 60), Jolt (elec 60/SC 60),
        // PPG +187% cc, PTC +110% cd, Lethal Torrent (FR 60/MS 60),
        // Galvanized Shot (+80% SC, CO 0.4×3), Galvanized Diffusion
        // (+110% MS, +30%×4 on kill).
        let mods = [
            m("hornet", vec![ModEffect::BaseDamage(2.20)]),
            m(
                "frostbite",
                vec![
                    ModEffect::Element(Cold, 0.60),
                    ModEffect::StatusChance(0.60),
                ],
            ),
            m(
                "jolt",
                vec![
                    ModEffect::Element(Electricity, 0.60),
                    ModEffect::StatusChance(0.60),
                ],
            ),
            m("ppg", vec![ModEffect::CritChance(1.87)]),
            m("ptc", vec![ModEffect::CritDamage(1.10)]),
            m(
                "lt",
                vec![ModEffect::FireRate(0.60), ModEffect::Multishot(0.60)],
            ),
            m(
                "gshot",
                vec![
                    ModEffect::StatusChance(0.80),
                    ModEffect::ConditionOverload {
                        per_stack: 0.40,
                        max_stacks: 3,
                    },
                ],
            ),
            m(
                "gdiff",
                vec![
                    ModEffect::Multishot(1.10),
                    ModEffect::OnKillMultishot {
                        per_stack: 0.30,
                        max_stacks: 4,
                    },
                ],
            ),
        ];
        let refs: Vec<&ModDef> = mods.iter().collect();
        let p = resolve(
            &WeaponBase::dual_toxocyst_incarnon(false),
            &refs,
            StackPolicy::AssumedMax,
        );

        // ModifiedBase: 125 × 3.2 = 400.
        assert!((p.modified_base - 400.0).abs() < 1e-9);
        // Physical keeps its 3.2×; Cold+Electricity (adjacent) -> Magnetic
        // of 2 × 0.6 × 400 = 480; total = 400 + 480 = 880.
        assert!((p.damage.get(Magnetic) - 480.0).abs() < 1e-9);
        assert!((p.damage.get(Puncture) - 200.0).abs() < 1e-9);
        assert!((p.damage.total() - 880.0).abs() < 1e-9);
        // cc 0.31 × 2.87; cd 3 × 2.1; sc 0.43 × 3.0; fr 4.5 × 1.6.
        assert!((p.crit_chance - 0.8897).abs() < 1e-9);
        assert!((p.crit_damage - 6.3).abs() < 1e-9);
        assert!((p.status_chance - 1.29).abs() < 1e-9);
        assert!((p.fire_rate - 7.2).abs() < 1e-9);
        // MS: 1 × (1 + 1.0 Fevered + 0.6 + 1.1 + 1.2 stacks) = 4.9.
        assert!((p.multishot - 4.9).abs() < 1e-9);
        // CO assumed max: 0.4 × 3 = 1.2. No status-damage mods.
        assert!((p.co_per_type - 1.2).abs() < 1e-9);
        assert!((p.status_damage_mult - 1.0).abs() < 1e-9);
        // Elemental DoT brackets: cold 1.6, electricity 1.6.
        assert!(p.elem_dot_bonus.contains(&(Cold, 1.6)));
        assert!(p.elem_dot_bonus.contains(&(Electricity, 1.6)));
    }

    #[test]
    fn element_mod_order_changes_the_combination() {
        let heat = m("scorch", vec![ModEffect::Element(Heat, 0.60)]);
        let cold = m("frostbite", vec![ModEffect::Element(Cold, 0.60)]);
        let tox = m("pestilence", vec![ModEffect::Element(Toxin, 0.60)]);
        let base = WeaponBase::dual_toxocyst_incarnon(false);

        // Heat,Cold,Toxin -> Blast + trailing Toxin.
        let p1 = resolve(&base, &[&heat, &cold, &tox], StackPolicy::AssumedMax);
        assert!(p1.damage.get(Blast) > 0.0);
        assert!(p1.damage.get(Toxin) > 0.0);
        // Cold,Toxin,Heat -> Viral + trailing Heat.
        let p2 = resolve(&base, &[&cold, &tox, &heat], StackPolicy::AssumedMax);
        assert!(p2.damage.get(Viral) > 0.0);
        assert!(p2.damage.get(Heat) > 0.0);
        // Totals identical either way.
        assert!((p1.damage.total() - p2.damage.total()).abs() < 1e-9);
    }
}
