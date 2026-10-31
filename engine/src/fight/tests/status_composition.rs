use super::*;
use crate::record::{Kind, Origin};

fn weapon(id: &str, seconds: f64) -> FightParams {
    let base = crate::model::WeaponBase::from_data(id, false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let arena = crate::arena::Arena { body_parts: mono_body(1.0), ..crate::arena::Arena::training(seconds) };
    FightParams::from_panel(&panel, &arena, &ArcaneFx::none())
}

fn residual(amount: f64) -> DamageVector {
    DamageVector::new().with(DamageType::Puncture, amount).with(DamageType::Radiation, 10.0)
}

// M130 reports presence, not a proc frequency. Extra rolls expose the same
// rare-event path deterministically without asserting a measured frequency.
fn ocucor(amount: f64, crowd: bool) -> FightParams {
    let mut p = weapon("ocucor", 180.0);
    p.damage = residual(amount);
    p.status_chance = 10.0;
    p.base_crit_chance = 0.0;
    p.crit_multiplier = 1.0;
    if crowd {
        p.others = vec![crate::formation::FoeSpec {
            id: "other".into(), params: Foe::training_dummy(), body_parts: mono_body(1.0),
            at: crate::rules::space::Vec2::new(0.25, 3.0),
        }];
        p.tendrils_initial = 1;
        p.tendrils_held = true;
    }
    p
}

fn traced(p: &FightParams) -> (RunResult, crate::record::Record) {
    let mut rec = crate::record::Record::window(0.0, p.duration_seconds, 100_000, 0);
    let r = run_once_traced(p, &mut Rng::new(42), None, &mut rec);
    assert!(!rec.events().is_empty());
    (r, rec)
}

fn proc_count(rec: &crate::record::Record, ty: DamageType, body: u16) -> usize {
    rec.events().iter().filter_map(|e| match &e.kind {
        Kind::Damage(row) if e.subject == Some(body) => Some(row.procs.iter().filter(|&&t| t == ty).count()),
        _ => None,
    }).sum()
}

fn has_debuff(rec: &crate::record::Record, id: &str, body: u16) -> bool {
    let index = DEBUFF_ROSTER.iter().position(|&(name, _)| name == id).unwrap();
    rec.events().iter().any(|e| match &e.kind {
        Kind::Damage(row) if e.subject == Some(body) => row.debuffs.get(index).is_some_and(|s| s.0 > 0),
        _ => false,
    })
}

#[test]
fn ocucor_residual_puncture_procs_on_main_beam_and_tendril_without_direct_puncture() {
    let p = ocucor(1.0 - 0.982, true);
    assert!(p.damage.get(DamageType::Puncture) > 0.0);
    assert_eq!(p.damage.quantized_against(11.0).get(DamageType::Puncture), 0.0);
    let (r, rec) = traced(&p);
    assert!(r.taken.by_body().0[1] > 0.0, "the tendril acquired its target");
    assert!(proc_count(&rec, DamageType::Puncture, 0) > 0, "main beam keeps the residual weight");
    assert!(has_debuff(&rec, "weakened", 1), "tendril keeps the residual weight");
    assert_eq!(r.sources.direct_by_type[DamageType::Puncture as usize], 0.0);
}

#[test]
fn removed_puncture_never_procs() {
    let (_, rec) = traced(&ocucor(0.0, true));
    assert_eq!(proc_count(&rec, DamageType::Puncture, 0), 0);
    assert!(!has_debuff(&rec, "weakened", 1));
}

#[test]
fn immune_puncture_is_excluded_even_when_its_raw_weight_is_positive() {
    let mut p = ocucor(0.018, true);
    p.foe.status_immunities.push(DamageType::Puncture);
    p.others[0].params.status_immunities.push(DamageType::Puncture);
    let (_, rec) = traced(&p);
    assert_eq!(proc_count(&rec, DamageType::Puncture, 0), 0);
    assert!(!has_debuff(&rec, "weakened", 1));
    assert!(proc_count(&rec, DamageType::Radiation, 0) > 0);
}

#[test]
fn live_added_element_keeps_its_raw_weight_without_direct_element_damage() {
    let mut p = ocucor(0.0, true);
    p.arcane.added_elements = vec![(DamageType::Toxin, 0.001)];
    let (r, rec) = traced(&p);
    assert!(proc_count(&rec, DamageType::Toxin, 0) > 0);
    assert!(has_debuff(&rec, "poison", 1));
    assert_eq!(r.sources.direct_by_type[DamageType::Toxin as usize], 0.0);
}

#[test]
fn resting_form_uses_its_own_unquantized_composition() {
    let mut p = ocucor(0.0, false);
    p.frenzy = true;
    p.resting = Some(crate::build::loadout::ResolvedResting {
        damage: residual(0.018), elem_dot_bonus: Vec::new(), injected: Vec::new(),
    });
    let (r, rec) = traced(&p);
    assert!(proc_count(&rec, DamageType::Puncture, 0) > 0);
    assert_eq!(r.sources.direct_by_type[DamageType::Puncture as usize], 0.0);
}

#[test]
fn each_projectile_keeps_its_own_unquantized_composition() {
    let mut p = ocucor(0.0, false);
    p.beam = None;
    p.continuous = false;
    p.multishot = 2.0;
    p.base_multishot = 2.0;
    p.pellet_damage = vec![
        (residual(0.018), DamageVector::new()),
        (residual(0.0), DamageVector::new()),
    ];
    let (r, rec) = traced(&p);
    assert!(proc_count(&rec, DamageType::Puncture, 0) > 0);
    assert_eq!(r.sources.direct_by_type[DamageType::Puncture as usize], 0.0);
    let rows: Vec<_> = rec.events().iter().filter_map(|e| match &e.kind {
        Kind::Damage(row) if matches!(row.origin, Origin::Own | Origin::Multishot) && !row.radial => Some(row),
        _ => None,
    }).collect();
    assert!(rows.len() > 2);
    assert!(rows.iter().any(|r| r.pellet == Some(2)));
    assert!(rows.iter().filter(|r| r.pellet == Some(2)).all(|r| !r.procs.contains(&DamageType::Puncture)));
}

#[test]
fn explosion_and_its_spread_use_the_explosions_raw_composition() {
    let mut p = weapon("akarius", 180.0);
    p.damage = residual(0.0);
    p.status_chance = 0.0;
    p.base_crit_chance = 0.0;
    p.others = vec![crate::formation::FoeSpec {
        id: "other".into(), params: Foe::training_dummy(), body_parts: mono_body(1.0),
        at: crate::rules::space::Vec2::new(1.0, 1.0),
    }];
    let rad = p.radial.as_mut().unwrap();
    rad.damage = residual(0.018);
    rad.modified_base = 11.0;
    rad.status_chance = 10.0;
    rad.crit_chance = 0.0;
    rad.radius_m = 10.0;
    let (r, rec) = traced(&p);
    assert!(proc_count(&rec, DamageType::Puncture, 0) > 0);
    assert!(has_debuff(&rec, "weakened", 1));
    assert_eq!(r.sources.radial_by_type[DamageType::Puncture as usize], 0.0);
}

#[test]
fn field_ticks_keep_the_fields_raw_composition() {
    let mut p = weapon("torid", 30.0);
    p.damage = DamageVector::new();
    p.status_chance = 0.0;
    p.magazine_size = 1.0;
    p.infinite_reserve = false;
    p.reserve_ammo = 0.0;
    p.reload_seconds = 999.0;
    let field = p.lingering.as_mut().unwrap();
    field.damage = residual(0.018);
    field.modified_base = 11.0;
    field.status_chance = 10.0;
    field.crit_chance = 0.0;
    field.tick_rate = 100.0;
    field.duration_seconds = 30.0;
    let (r, rec) = traced(&p);
    assert!(has_debuff(&rec, "weakened", 0));
    assert_eq!(r.sources.field_by_type[DamageType::Puncture as usize], 0.0);
}

#[test]
fn per_hit_element_pile_keeps_its_unquantized_composition() {
    let mut p = ocucor(0.0, false);
    p.per_hit_element = Some(crate::build::loadout::ResolvedPerHitElement {
        spec: crate::model::ElementPerHit {
            element: DamageType::Heat, per_stack: 0.001, max_bonus: 0.001, reset_seconds: 5.0,
        },
        damage: vec![residual(0.0).with(DamageType::Heat, 0.011)],
        radial: Vec::new(),
    });
    let (r, rec) = traced(&p);
    assert!(proc_count(&rec, DamageType::Heat, 0) > 0);
    assert_eq!(r.sources.direct_by_type[DamageType::Heat as usize], 0.0);
}

#[test]
fn melee_swings_keep_unquantized_status_weights() {
    let mut p = weapon("magistar", 180.0);
    p.damage = residual(0.018);
    p.dot_modified_base = Some(11.0);
    p.status_chance = 100.0;
    p.base_crit_chance = 0.0;
    p.crit_multiplier = 1.0;
    let (r, rec) = traced(&p);
    assert!(proc_count(&rec, DamageType::Puncture, 0) > 0);
    assert_eq!(r.sources.direct_by_type[DamageType::Puncture as usize], 0.0);
}

#[test]
fn laetum_direct_damage_matches_all_three_measurements_without_a_component_floor() {
    for (negative, viral, base_bonus, white, yellow) in [
        (0.0, 0.0, 0.0, 160.0, 353.0),
        (0.993, 0.961, 0.0, 250.0, 551.0),
        (0.993, 0.961, 3.85, 1213.0, 2672.0),
    ] {
        for (cc, displayed) in [(0.0, white), (1.0, yellow)] {
            let mut p = weapon("laetum", 0.1);
            let b = 1.0 + base_bonus;
            p.damage = DamageVector::new()
                .with(DamageType::Impact, 64.0 * (1.0 - negative) * b)
                .with(DamageType::Slash, 96.0 * b)
                .with(DamageType::Viral, 160.0 * viral * b);
            p.dot_modified_base = Some(160.0 * b);
            p.base_damage_bonus = base_bonus;
            p.base_crit_chance = cc;
            p.status_chance = 0.0;
            let (_, rec) = traced(&p);
            let hits: Vec<_> = rec.events().iter().filter_map(|e| match &e.kind {
                Kind::Damage(row) if matches!(row.origin, Origin::Own | Origin::Multishot) && !row.radial => Some(row),
                _ => None,
            }).collect();
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].effective.round(), displayed, "negative {negative}, base {base_bonus}, cc {cc}");
        }
    }
}
