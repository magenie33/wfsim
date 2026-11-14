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
            InstanceScale { mb_live: 75.0, crit_multiplier: 1.0, part_factor: 1.0,
                landing: ProcLanding::default(), attrition: 1.0, xh_bracket: 1.0,
                status_damage_live: 0.0 },
            &mut here.debuffs, &mut me.gal, &mut me.arc, &mut here.state, &p, &p, &mit,
            &mut fight.r, &mut rec, &mut me.d.status, &p.foe, DEPTH_PROC);
        assert_eq!(fight.r.procs, count as u32 + 1);
    }
}
