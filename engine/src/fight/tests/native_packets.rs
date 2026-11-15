use super::*;

#[test]
fn encumber_suppression_is_weapon_state_across_targets_and_delayed_arrivals() {
    let mut p = flat_base();
    p.arcane = arc("secondary_encumber");
    p.arcane.encumber_chance = 1.0;
    let mut runtime = ArcRuntime::init(&p);
    let mut target = p.foe.clone();
    target.valid_double_procs = 1 << 12;
    let mut first = vec![DamageType::Corrosive];
    runtime.encumber(&p, 1.0, &target, &mut first);
    assert_eq!(first, [DamageType::Corrosive, DamageType::Corrosive]);
    let after = runtime.native_random.state();
    let mut second = vec![DamageType::Cold];
    runtime.encumber(&p, 1.0, &p.foe, &mut second);
    assert_eq!(second, [DamageType::Cold]);
    assert_eq!(runtime.native_random.state(), after);
    runtime.encumber(&p, 1.01, &p.foe, &mut second);
    assert_eq!(second.len(), 2, "a later arrival supplies a positive effective update");
}

#[test]
fn ineligible_or_empty_encumber_payloads_consume_no_native_draws() {
    let mut p = flat_base();
    p.arcane = arc("secondary_encumber");
    let mut runtime = ArcRuntime::init(&p);
    let before = runtime.native_random.state();
    runtime.encumber(&p, 0.0, &p.foe, &mut Vec::new());
    let mut target = p.foe.clone();
    target.valid_double_procs = 0;
    let mut payload = vec![DamageType::Magnetic];
    runtime.encumber(&p, 0.0, &target, &mut payload);
    assert_eq!(payload, [DamageType::Magnetic]);
    assert_eq!(runtime.native_random.state(), before);
    assert!(!runtime.encumber_state.active);
}

#[test]
fn encumber_selection_installs_suppression_even_when_the_recipient_is_immune() {
    let mut p = flat_base();
    p.arcane = arc("secondary_encumber");
    p.arcane.encumber_chance = 1.0;
    let mut target = p.foe.clone();
    target.valid_double_procs = (1 << 3) | (1 << 4);
    target.status_immunities = vec![DamageType::Heat, DamageType::Cold];
    let mut runtime = ArcRuntime::init(&p);
    let mut payload = vec![DamageType::Cold];
    runtime.encumber(&p, 0.0, &target, &mut payload);
    assert_eq!(payload, [DamageType::Cold]);
    assert!(runtime.encumber_state.active);
    assert_ne!(runtime.native_random.state(), 0);
}

#[test]
fn one_submitted_packet_with_twenty_magnetic_counters_has_one_primary_selection() {
    let mut p = flat_base();
    p.arcane = arc("primary_debilitate");
    for count in [1, 20] {
        let mut rec = crate::record::Record::off();
        let (mut fight, mut me) = open(&p, Seat::WIELDER, &mut Rng::new(24301), &rec, &None);
        let here = &mut fight.bodies[0];
        here.debuffs.disrupt = vec![10.0; 10];
        let mit = here.debuffs.mitigation(0.0, 1.0, 0.0, 1.0);
        settle_procs(vec![DamageType::Magnetic; count], Seat::WIELDER, 0.0, 0,
            InstanceScale { native_source: None, native_part: None, mb_live: 75.0, crit_multiplier: 1.0, part_factor: 1.0,
                landing: ProcLanding::default(), attrition: 1.0, xh_bracket: 1.0,
                status_damage_live: 0.0 },
            &mut here.debuffs, &mut me.gal, &mut me.arc, &mut here.state, &p, &p, &mit,
            &mut fight.r, &mut rec, &mut me.d.status, &p.foe, DEPTH_PROC);
        assert_eq!(fight.r.procs, count as u32 + 1);
    }
}

fn magnetic_beam() -> FightParams {
    let base = crate::model::WeaponBase::from_data("amprex", false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let arena = crate::arena::Arena { body_parts: mono_body(1.0), ..crate::arena::Arena::training(2.0) };
    let mut p = FightParams::from_panel(&panel, &arena, &arc("primary_debilitate"));
    assert!(p.continuous);
    p.damage = DamageVector::new().with(DamageType::Magnetic, 100.0);
    p.base_crit_chance = 0.0;
    if let Some(inputs) = p.critical_inputs.as_mut() { inputs.set_supplied_chance(p.base_crit_chance, p.unmodded_crit_chance); }
    p.status_chance = 0.0;
    p.forced_procs = vec![DamageType::Magnetic];
    p.multishot = 1.0;
    p.infinite_reserve = true;
    p
}

#[test]
fn merged_beam_multishot_changes_damage_without_multiplying_primary_callbacks() {
    let mut p = magnetic_beam();
    let single = run_once(&p, &mut Rng::new(42));
    p.multishot = 5.0;
    let merged = run_once(&p, &mut Rng::new(42));
    assert!(single.shots > 10);
    assert_eq!(single.shots, merged.shots);
    assert_eq!(single.procs, single.shots * 2 - 9);
    assert_eq!(single.procs, merged.procs);
    assert!(merged.tally.raw() > single.tally.raw() * 4.9);
}

#[test]
fn a_beam_tick_with_twenty_status_rolls_submits_one_primary_damage_instance() {
    let mut p = magnetic_beam();
    p.forced_procs.clear();
    p.status_chance = 20.0;
    let split = run_once(&p, &mut Rng::new(42));
    p.arcane = ArcaneFx::none();
    let plain = run_once(&p, &mut Rng::new(42));
    assert_eq!(plain.procs, plain.shots * 20);
    assert_eq!(split.procs - plain.procs, split.shots);
}

fn phantasma_primary_packet(ordinary_status_chance: f64, force_radiation: bool) -> FightParams {
    let base = crate::model::WeaponBase::from_data("phantasma_prime", false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let arena = crate::arena::Arena { body_parts: mono_body(1.0), ..crate::arena::Arena::training(2.0) };
    let mut p = FightParams::from_panel(&panel, &arena, &arc("primary_debilitate"));
    assert!(p.continuous);
    assert_eq!(p.base_multishot, 6.0);
    assert_eq!(p.multishot, 6.0);
    p.status_chance = ordinary_status_chance;
    p.forced_procs = if force_radiation { vec![DamageType::Radiation] } else { Vec::new() };
    p.infinite_reserve = true;
    p
}

#[test]
fn phantasma_prime_scene_merges_six_and_twenty_four_beams_before_primary_callbacks() {
    for multishot in [6.0, 24.0] {
        let mut p = phantasma_primary_packet(0.0, true);
        p.multishot = multishot;
        let split = run_once(&p, &mut Rng::new(42));
        p.arcane = ArcaneFx::none();
        let plain = run_once(&p, &mut Rng::new(42));
        assert_eq!(split.shots, plain.shots);
        assert_eq!(plain.pellets, plain.shots);
        assert_eq!(plain.procs, plain.shots);
        assert_eq!(split.procs - plain.procs, split.shots - 9,
            "one selection from the tenth Radiation application, multishot={multishot}");
    }
}

#[test]
fn phantasma_prime_scene_many_radiation_counters_have_one_primary_selection_per_tick() {
    for multishot in [6.0, 24.0] {
        let mut p = phantasma_primary_packet(20.0 / multishot, false);
        p.multishot = multishot;
        let split = run_once(&p, &mut Rng::new(42));
        p.arcane = ArcaneFx::none();
        let plain = run_once(&p, &mut Rng::new(42));
        assert_eq!(split.shots, plain.shots);
        assert_eq!(plain.pellets, plain.shots);
        assert_eq!(plain.procs, plain.shots * 20);
        assert_eq!(split.procs - plain.procs, split.shots,
            "repeated Radiation counters select once, multishot={multishot}");
    }
}

#[test]
fn one_primary_callback_can_select_multiple_compound_types_and_coalesce_shared_components() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..128 {
        let mut results = Vec::new();
        for count in [1, 20] {
            let mut p = flat_base();
            p.arcane = arc("primary_debilitate");
            let mut rec = crate::record::Record::off();
            let (mut fight, mut me) = open(&p, Seat::WIELDER, &mut Rng::new(42), &rec, &None);
            me.arc.shared_native_random = None;
            me.arc.native_random = crate::rules::rng::NativeRng::new(seed);
            let here = &mut fight.bodies[0];
            here.debuffs.confusion = vec![10.0; 10];
            here.debuffs.disrupt = vec![10.0; 10];
            let mit = here.debuffs.mitigation(0.0, 1.0, 0.0, 1.0);
            let procs = [vec![DamageType::Radiation; count], vec![DamageType::Magnetic; count]].concat();
            settle_procs(procs, Seat::WIELDER, 0.0, 0,
                InstanceScale { native_source: None, native_part: None, mb_live: 75.0,
                    crit_multiplier: 1.0, part_factor: 1.0, landing: ProcLanding::default(),
                    attrition: 1.0, xh_bracket: 1.0, status_damage_live: 0.0 },
                &mut here.debuffs, &mut me.gal, &mut me.arc, &mut here.state, &p, &p, &mit,
                &mut fight.r, &mut rec, &mut me.d.status, &p.foe, DEPTH_PROC);
            let extra = fight.r.procs - (2 * count) as u32;
            assert!((1..=2).contains(&extra));
            results.push(extra);
            seen.insert(extra);
        }
        assert_eq!(results[0], results[1], "counter multiplicity must not multiply selections");
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), [1, 2]);
}
