//! Synthetic hit regressions for the Wiki-transcribed adaptive element.
use super::*;
use crate::model::{AdaptiveElement, FormKind, StackPolicy, WeaponBase};
use crate::record::{Kind, Origin, Record};

fn adaptive(bonus: f64) -> AdaptiveElement {
    AdaptiveElement {
        bonus,
        by_damage_column: vec![
            ("corpus", DamageType::Magnetic),
            ("anarchs", DamageType::Electricity),
            ("infested", DamageType::Heat),
            ("narmer", DamageType::Toxin),
        ],
        overguard: DamageType::Corrosive,
    }
}

fn target(column: &str, overguard: f64) -> Foe {
    Foe {
        damage_column: column.into(),
        faction: crate::model::Faction::Corpus,
        base_health: 1e12,
        base_overguard: overguard,
        mode: TargetMode::InstantRespawn,
        ..Foe::training_dummy()
    }
}

fn fixture(column: &str) -> FightParams {
    FightParams {
        form: FormKind::Neutral,
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        dot_modified_base: Some(100.0),
        adaptive_element: Some(adaptive(1.0)),
        foe: target(column, 0.0),
        duration_seconds: 0.01,
        ..flat_base()
    }
}

fn traced(p: &FightParams) -> (RunResult, Record) {
    let mut rec = Record::window(0.0, p.duration_seconds, 10_000, 0);
    let result = run_once_traced(p, &mut Rng::new(42), None, &mut rec);
    (result, rec)
}

fn procs(rec: &Record, body: u16) -> Vec<DamageType> {
    rec.events().iter().filter_map(|e| match &e.kind {
        Kind::Damage(d) if e.subject == Some(body) => Some(d.procs.iter().copied()),
        _ => None,
    }).flatten().collect()
}

fn close(got: f64, expected: f64) {
    assert!((got - expected).abs() < 1e-8, "{got} != {expected}");
}

fn swing() -> crate::model::ComboHit {
    serde_norway::from_str("multiplier: 1\ndelay_seconds: 10\ncombo_points: 1\ncombo_points_base: 1")
        .expect("a single immediate synthetic swing")
}

#[test]
fn the_selected_element_remains_separate_from_combinable_weapon_damage() {
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Heat, 100.0),
        ..fixture("anarchs")
    };
    let r = run_once(&p, &mut Rng::new(42));
    close(r.sources.direct_by_type[DamageType::Heat as usize], 100.0);
    close(r.sources.direct_by_type[DamageType::Electricity as usize], 100.0);
    close(r.sources.direct_by_type[DamageType::Radiation as usize], 0.0);
    close(r.sources.direct, 200.0);
}

#[test]
fn the_next_hit_reselects_after_overguard_breaks() {
    let p = FightParams {
        foe: target("corpus", 200.0),
        duration_seconds: 1.01,
        ..fixture("corpus")
    };
    let (r, rec) = traced(&p);
    assert_eq!(r.shots, 2);
    close(r.sources.direct_by_type[DamageType::Corrosive as usize], 100.0);
    close(r.sources.direct_by_type[DamageType::Magnetic as usize], 100.0);
    let before = rec.events().iter().filter_map(|e| match &e.kind {
        Kind::Damage(d) if d.origin == Origin::Own && !d.radial => Some(d.before.overguard),
        _ => None,
    }).collect::<Vec<_>>();
    assert_eq!(before.len(), 2);
    close(before[0], 200.0);
    close(before[1], 0.0);
}

fn mixed_crowd() -> FightParams {
    let mut p = FightParams {
        foe: target("corpus", 1e9),
        target_at: crate::rules::space::Vec2::new(0.0, 1.0),
        range_m: 2.0,
        follow_through: Some(1.0),
        combo_script: vec![swing()],
        status_chance: 1.0,
        base_status_chance: 1.0,
        ..fixture("corpus")
    };
    p.foe.status_immunities.push(DamageType::Impact);
    let mut other = target("corpus", 0.0);
    other.status_immunities.push(DamageType::Impact);
    p.others.push(crate::formation::FoeSpec {
        id: "unguarded".into(),
        params: other,
        body_parts: mono_body(1.0),
        at: crate::rules::space::Vec2::new(0.0, 1.5),
    });
    p
}

#[test]
fn follow_through_selects_for_each_bodys_own_overguard() {
    let p = mixed_crowd();
    assert_eq!(p.foe.faction, p.others[0].params.faction);
    assert_eq!(p.foe.damage_column, p.others[0].params.damage_column);
    let (r, rec) = traced(&p);
    assert!(r.taken.by_body().0[0] > 0.0 && r.taken.by_body().0[1] > 0.0);
    let guarded = procs(&rec, 0);
    assert!(guarded.contains(&DamageType::Corrosive), "{guarded:?}");
    assert!(!guarded.contains(&DamageType::Magnetic), "{guarded:?}");
    close(r.sources.direct_by_type[DamageType::Corrosive as usize], 100.0);
    close(r.sources.direct_by_type[DamageType::Magnetic as usize], 100.0);
}

#[test]
fn duplicate_reselects_after_its_first_instance_breaks_overguard() {
    let p = FightParams {
        foe: target("corpus", 200.0),
        base_crit_chance: 1.0,
        combo_script: vec![swing()],
        arcane: ArcaneFx { duplicate_chance: 1.0, ..ArcaneFx::none() },
        ..fixture("corpus")
    };
    let (r, rec) = traced(&p);
    assert_eq!(r.shots, 1, "both instances must belong to the same swing");
    close(r.sources.direct_by_type[DamageType::Corrosive as usize], 100.0);
    close(r.sources.direct_by_type[DamageType::Magnetic as usize], 100.0);
    let rows = rec.events().iter().filter_map(|e| match &e.kind {
        Kind::Damage(d) if !d.radial => Some((e.t, d.crit_tier, d.before.overguard)),
        _ => None,
    }).collect::<Vec<_>>();
    assert_eq!(rows.len(), 2, "a guaranteed yellow crit must duplicate once: {rows:?}");
    assert_eq!(rows[0].1, 1);
    assert_eq!(rows[1].1, 1);
    close(rows[0].0, rows[1].0);
    close(rows[0].2, 200.0);
    close(rows[1].2, 0.0);
}

#[test]
fn influence_copies_the_follow_through_bodys_new_element_and_its_own_share() {
    let mut p = mixed_crowd();
    p.range_m = 2.0;
    p.influence_open = Some(10.0);
    p.arcane = ArcaneFx {
        influence_chance: 1.0,
        influence_radius_m: 10.0,
        influence_seconds: 10.0,
        ..ArcaneFx::none()
    };
    p.others.push(crate::formation::FoeSpec {
        id: "outside_melee".into(),
        params: target("corpus", 0.0),
        body_parts: mono_body(1.0),
        at: crate::rules::space::Vec2::new(0.0, 6.0),
    });
    let plain = FightParams { adaptive_element: None, ..p.clone() };
    let without = run_once(&plain, &mut Rng::new(42));
    close(without.taken.by_body().0[2], 0.0);
    let (_, rec) = traced(&p);
    let copies = rec.events().iter().filter_map(|e| match &e.kind {
        Kind::Damage(d) if e.subject == Some(2) && d.origin == Origin::Influence => Some(d),
        _ => None,
    }).collect::<Vec<_>>();
    let magnetic = copies.iter().find(|d| d.dtype == DamageType::Magnetic)
        .expect("the unguarded follow-through body's Magnetic proc must spread");
    close(magnetic.raw, 100.0);
    close(magnetic.effective, 100.0);
    assert!(copies.iter().any(|d| d.dtype == DamageType::Corrosive),
        "the aimed guarded body contributes its different Corrosive element");
    assert!(!rec.events().iter().any(|e| matches!(&e.kind,
        Kind::Damage(d) if e.subject == Some(2) && matches!(d.origin, Origin::Own | Origin::FollowThrough))),
        "the receiver must be outside the swing");
}

#[test]
fn adaptive_damage_does_not_inflate_forced_dot_bases_or_element_brackets() {
    for dtype in [DamageType::Slash, DamageType::Heat, DamageType::Electricity,
        DamageType::Toxin, DamageType::Gas] {
        let boosted = FightParams {
            forced_procs: vec![dtype],
            fire_rate: 0.1,
            duration_seconds: 1.1,
            ..fixture("infested")
        };
        let plain = FightParams { adaptive_element: None, ..boosted.clone() };
        let a = run_once(&plain, &mut Rng::new(42));
        let b = run_once(&boosted, &mut Rng::new(42));
        assert!(a.tally.dot() > 0.0, "{dtype:?}: the forced status must tick");
        close(b.tally.dot(), a.tally.dot());
        close(b.sources.direct, a.sources.direct * 2.0);
    }
}

#[test]
fn the_new_element_participates_in_the_status_weight_draw() {
    let mut boosted = FightParams {
        status_chance: 1.0,
        base_status_chance: 1.0,
        ..fixture("anarchs")
    };
    boosted.foe.status_immunities.push(DamageType::Impact);
    let plain = FightParams { adaptive_element: None, ..boosted.clone() };
    let (_, before) = traced(&plain);
    let (_, after) = traced(&boosted);
    assert!(!procs(&before, 0).contains(&DamageType::Electricity));
    assert_eq!(procs(&after, 0), [DamageType::Electricity]);
}

#[test]
fn an_unpublished_damage_column_does_not_invent_a_fallback_element() {
    let p = fixture("unpublished");
    let r = run_once(&p, &mut Rng::new(42));
    close(r.sources.direct, 100.0);
    close(r.sources.direct_by_type[DamageType::Impact as usize], 100.0);
}

#[test]
fn a_stance_closing_slam_does_not_gain_the_adaptive_element() {
    let mut hit = swing();
    hit.multiplier = 0.0;
    hit.slam_multiplier = Some(1.0);
    let p = FightParams {
        combo_script: vec![hit],
        slam: Some(crate::build::loadout::ResolvedRadial {
            blast_kind: crate::model::BlastKind::Slam,
            ..radial_of(0.0, 0.0)
        }),
        ..fixture("corpus")
    };
    let plain = FightParams { adaptive_element: None, ..p.clone() };
    let a = run_once(&plain, &mut Rng::new(42));
    let b = run_once(&p, &mut Rng::new(42));
    assert!(a.sources.radial > 0.0, "the stance must actually land its slam");
    close(b.sources.radial, a.sources.radial);
    close(b.sources.radial_by_type[DamageType::Magnetic as usize], 0.0);
}

fn venato(form: &str, rank: u32) -> FightParams {
    let base = WeaponBase::from_data(form, false, &[]);
    let id = crate::data::mods::ranked_id("sentient_incision", rank, 5);
    let pool = crate::data::mods::pool_naming(form, &[id.as_str()]);
    let card = pool.get(&id).expect("the supported rank belongs to Venato");
    let panel = crate::build::loadout::resolve(&base, &[card], StackPolicy::Emergent);
    let arena = crate::arena::Arena {
        target: target("corpus", 0.0),
        body_parts: mono_body(1.0),
        ..crate::arena::Arena::training(4.0)
    };
    let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
    p.base_crit_chance = 0.0;
    p.crit_multiplier = 1.0;
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.forced_procs.clear();
    for hit in &mut p.combo_script { hit.forced_procs.clear(); }
    p
}

#[test]
fn the_real_rank_zero_card_reaches_the_hit_without_preselecting_a_target() {
    for (rank, bonus, units) in [(0, 0.2, 6.0), (5, 1.2, 38.0)] {
        let carried = venato("venato", rank).adaptive_element.expect("the panel carries the card");
        close(carried.bonus, bonus);
        assert_eq!(carried.element_for("corpus", false), Some(DamageType::Magnetic));
        let p = FightParams { adaptive_element: Some(carried), ..fixture("corpus") };
        let r = run_once(&p, &mut Rng::new(42));
        close(r.sources.direct_by_type[DamageType::Magnetic as usize], 100.0 * units / 32.0);
    }
}

#[test]
fn the_roster_damage_override_reaches_adaptive_selection_independently_of_combat_faction() {
    let spec = crate::data::enemies::all().into_iter().find(|e| e.id == "thrax_centurion")
        .expect("the roster supplies a Thrax Centurion");
    let mut foe = spec.target_params(55, false, false, TargetMode::InstantRespawn)
        .expect("a normal level-55 Thrax is legal");
    assert_eq!(foe.faction, crate::model::Faction::Unknown);
    assert_eq!(foe.damage_column, "zariman");
    assert!(foe.type_mods.faction.get(DamageType::Void) > 1.0);
    foe.base_overguard = 0.0;
    foe.base_armor = 0.0;
    foe.base_health = 1e12;
    let p = FightParams {
        foe,
        adaptive_element: venato("venato", 5).adaptive_element,
        ..fixture("corpus")
    };
    let r = run_once(&p, &mut Rng::new(42));
    close(r.sources.direct_by_type[DamageType::Corrosive as usize], 100.0 * 38.0 / 32.0);
    close(r.sources.direct_by_type[DamageType::Void as usize], 0.0);
}

#[test]
fn ordinary_and_heavy_swings_gain_damage_but_heavy_slams_do_not() {
    for form in ["venato", "venato_heavy", "venato_heavy_slam"] {
        let boosted = venato(form, 5);
        let plain = FightParams { adaptive_element: None, ..boosted.clone() };
        let a = run_once(&plain, &mut Rng::new(42));
        let b = run_once(&boosted, &mut Rng::new(42));
        assert!(a.sources.direct + a.sources.radial > 0.0, "{form}: the fixture must land");
        if form == "venato_heavy_slam" {
            close(b.sources.radial, a.sources.radial);
            close(b.sources.radial_by_type[DamageType::Magnetic as usize], 0.0);
        } else {
            assert!(b.sources.direct > a.sources.direct, "{form}: the adaptive bonus did not land");
            assert!(b.sources.direct_by_type[DamageType::Magnetic as usize] > 0.0, "{form}");
        }
    }
}
