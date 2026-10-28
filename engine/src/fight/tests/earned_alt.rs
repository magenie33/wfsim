use super::*;

fn panel(id: &str) -> crate::build::loadout::ResolvedPanel {
    let base = crate::model::WeaponBase::from_data(id, false, &[]);
    crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent)
}

fn cycle(id: &str, duration: f64) -> FightParams {
    let mode = crate::data::weapons::play_modes(id).into_iter().find(|m| m.id == "cycle").expect("cycle");
    let base = panel(mode.weapon_id);
    let alt = panel(mode.other_id.expect("alternate"));
    let mut arena = crate::arena::Arena::training(duration);
    arena.target_at = crate::rules::space::Vec2::ORIGIN;
    let mut p = FightParams::incarnon_cycle_from_panels(&alt, &base, false, LockMode::Initial(0),
        &arena, &ArcaneFx::none());
    p.apl_inserted = mode.rules();
    p
}

#[test]
fn earned_single_shot_weapons_offer_primary_and_full_charge_cycle() {
    for id in ["nunchasa", "higasa", "mausolon", "cortege", "morgha", "battacor", "trumna", "trumna_prime", "sepulcrum"] {
        let modes = crate::data::weapons::play_modes(id);
        assert_eq!(modes.iter().map(|m| m.id).collect::<Vec<_>>(), ["base", "cycle"], "{id}");
        assert!(modes[1].declared.is_some(), "{id}: play policy is declared data");
        assert!(modes.iter().all(|m| m.sustainable));
    }
}

#[test]
fn nunchasa_primary_and_full_charge_arrow_resolve_their_own_stats() {
    let primary = panel("nunchasa");
    let alt = panel("nunchasa_alt");
    assert_eq!(primary.charge_seconds, Some(0.6));
    assert_eq!(primary.modified_base, 400.0);
    assert_eq!(alt.charge_seconds, Some(1.5));
    assert_eq!(alt.modified_base, 1000.0);
    assert_eq!(alt.crit_chance, 0.36);
    assert_eq!(alt.base_crit_damage, crate::rules::damage::quantize_base_crit_damage(3.0));
    assert_eq!(alt.status_chance, 0.36);
    assert_eq!(alt.projectile_width_m, 2.0);
    assert!(alt.infinite_body_punch_through);
    assert_eq!(alt.forced_procs, vec![DamageType::Cold]);
    assert_eq!(alt.headshot_multiplier, Some(1.0));
    assert_eq!(alt.cold_trail.unwrap().duration_seconds, 10.0);
}

#[test]
fn nunchasa_cycle_needs_ten_body_hits_or_five_weakpoint_hits() {
    for (heads, first_fill) in [(false, 10), (true, 5)] {
        let mut p = cycle("nunchasa", 40.0);
        p.body_parts = vec![BodyPart { name: "part".into(), aim_weight: 1.0, multiplier: 1.0,
            is_head: heads, is_weak_point: heads, crit_bonus: false }];
        p.gauge_charging_body_parts = p.body_parts.clone();
        p.cycle.as_mut().unwrap().base_form.body_parts = p.body_parts.clone();
        p.foe.base_health = 1e12;
        let rec = crate::fight::record(&p, 3, 0.0, 40.0, 10000, 0);
        let fired: Vec<_> = rec.events().iter().filter(|e| matches!(e.kind, crate::record::Kind::Strike { .. })).collect();
        assert!(fired.len() > first_fill + 1, "the fixture fires a full cycle");
        // WeaponAt records the form that paid for each trigger pull.
        let forms: Vec<_> = fired.iter().map(|e| e.weapon.transmuted).collect();
        assert!(forms[..first_fill].iter().all(|f| !*f));
        assert!(forms[first_fill]);
        assert!(!forms[first_fill + 1]);
        assert!((fired[first_fill].t - fired[first_fill - 1].t - 2.1).abs() < 1e-9,
            "the alternate pays its own 1.5 s draw plus the 0.6 s nock");
        assert!((fired[first_fill + 1].t - fired[first_fill].t - 1.2).abs() < 1e-9,
            "returning to primary pays its 0.6 s draw plus the nock");
    }
}

#[test]
fn kill_fed_cycle_waits_for_kills_and_fires_one_alt_before_resuming() {
    let mut p = cycle("battacor", 15.0);
    p.foe.base_health = 1e12;
    assert_eq!(run_once(&p, &mut Rng::new(3)).transforms, 0);
    p.foe.base_health = 1.0;
    p.foe.mode = TargetMode::InstantRespawn;
    let r = run_once(&p, &mut Rng::new(3));
    assert!(r.transforms > 1);
    assert_eq!(r.reloads, 0, "switching to the earned shot must not refill the magazine");
}

#[test]
fn cold_trail_has_ten_ticks_and_survives_the_return_to_primary() {
    let p = cycle("nunchasa", 30.0);
    let mut rec = crate::record::Record::off();
    let (mut fight, mut me) = open(&p, Seat::WIELDER, &mut Rng::new(3), &rec, &None);
    let mut entities = vec![];
    lay_cold_trail(me.seat, &p, 2.0, &mut entities);
    assert_eq!(entities.len(), fight.bodies.len());
    let Entity::ColdTrail(tick) = entities[0] else { panic!("trail") };
    assert_eq!(tick.at, 3.0);
    assert_eq!(tick.left, 10);
    assert_eq!(tick.interval_seconds, 1.0);
    let base = &p.cycle.as_ref().unwrap().base_form;
    process_entities(&mut entities, 13.0, &mut turn!(me, base, &mut me.d),
        &mut fight.r, &mut rec, &mut fight.bodies);
    assert!(entities.is_empty());
    assert_eq!(fight.r.procs, 10, "one Cold status per second for ten seconds");
    assert!(fight.r.taken.by_body().0.iter().all(|&v| v == 0.0), "the trail has no direct damage");
}

#[test]
fn nunchasa_multishot_and_punch_through_charge_every_contact() {
    let mut p = cycle("nunchasa", 20.0);
    p.target_at = crate::rules::space::Vec2::new(0.0, 5.0);
    p.foe.base_health = 1e12;
    p.spread = None;
    p.projectile_speed_mps = None;
    p.body_parts = vec![BodyPart { name: "body".into(), aim_weight: 1.0, multiplier: 1.0,
        is_head: false, is_weak_point: false, crit_bonus: false }];
    p.gauge_charging_body_parts = p.body_parts.clone();
    p.others = vec![crate::formation::FoeSpec { id: "back".into(), params: p.foe.clone(),
        body_parts: p.body_parts.clone(), at: crate::rules::space::Vec2::new(0.0, 5.5) }];
    let base = &mut p.cycle.as_mut().unwrap().base_form;
    base.target_at = p.target_at;
    base.others = p.others.clone();
    base.multishot = 2.0;
    base.spread = None;
    base.projectile_speed_mps = None;
    base.body_parts = p.body_parts.clone();
    let rec = crate::fight::record(&p, 3, 0.0, 20.0, 10000, 0);
    let shots: Vec<_> = rec.events().iter().filter(|e| matches!(e.kind, crate::record::Kind::Strike { .. })).collect();
    assert!(shots[..3].iter().all(|e| !e.weapon.transmuted));
    assert!(shots[3].weapon.transmuted, "three primary pulls land twelve body contacts");
}

#[test]
fn nunchasa_blast_status_hits_also_fill_the_gauge() {
    let mut p = cycle("nunchasa", 10.0);
    p.foe.base_health = 1e12;
    p.cycle.as_mut().unwrap().base_form.forced_procs = vec![DamageType::Blast; 10];
    let rec = crate::fight::record(&p, 3, 0.0, 10.0, 10000, 0);
    let shots: Vec<_> = rec.events().iter().filter(|e| matches!(e.kind, crate::record::Kind::Strike { .. })).collect();
    assert!(!shots[0].weapon.transmuted);
    assert!(shots[1].weapon.transmuted, "the ten Blast hits, beside the arrow contact, fill the gauge");
}
