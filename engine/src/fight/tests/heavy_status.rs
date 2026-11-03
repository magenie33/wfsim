use super::*;

fn stored_dot(dtype: DamageType, ticks_left: u32, frozen: f64) -> Dot {
    Dot {
        owner: Seat::WIELDER, cause: u32::MAX, seed: u32::MAX,
        next_tick: 1.0, ticks_left, frozen, landing: 1.0, bracket: 1.0,
        depth: DEPTH_PROC, source_scaled: true, unit: 0.0,
        dtype, ignores_armor: dtype == DamageType::Slash,
    }
}

#[test]
fn cashout_takes_only_pending_ticks_and_removes_them() {
    let p = FightParams::default();
    let mut db = DebuffState::default();
    db.push_dot(stored_dot(DamageType::Slash, 3, 10.0));
    db.push_dot(stored_dot(DamageType::Toxin, 2, 20.0));
    db.push_dot(stored_dot(DamageType::Electricity, 4, 30.0));
    db.push_dot(stored_dot(DamageType::Gas, 1, 40.0));
    db.heat = Some(HeatEntity { born: 0.0, expiry: 6.0, next_tick: 4.0,
        value: 50.0, bracket: 1.0, depth: DEPTH_PROC, unit: 0.0, recent: Vec::new(), stacks: 1 });
    let w = windows_for(&p);
    let got = db.take_status_damage(&p, &w, 0.0);
    assert_eq!(got.get(DamageType::Cinematic), 30.0);
    assert_eq!(got.get(DamageType::Toxin), 40.0);
    assert_eq!(got.get(DamageType::Electricity), 120.0);
    assert_eq!(got.get(DamageType::Gas), 40.0);
    assert_eq!(got.get(DamageType::Heat), 150.0);
    assert!(db.dots.is_empty() && db.heat.is_none());
    assert_eq!(db.take_status_damage(&p, &w, 0.0).total(), 0.0);
}

#[test]
fn consolidated_cashout_counts_one_accumulator_per_future_tick() {
    let p = FightParams::default();
    let mut db = DebuffState::default();
    let mut first = stored_dot(DamageType::Electricity, 2, 10.0);
    first.unit = 0.5;
    let mut last = stored_dot(DamageType::Electricity, 4, 20.0);
    last.unit = 0.5;
    db.push_dot(first);
    db.push_dot(last);
    assert_eq!(db.take_status_damage(&p, &windows_for(&p), 0.0).total(), 102.0);
}

#[test]
fn cinematic_fraction_of_one_cashout_bypasses_armour() {
    let mut p = FightParams::default();
    p.foe.base_armor = 2700.0;
    p.foe.base_health = 10000.0;
    p.foe.base_shield = 0.0;
    let mut target = TargetState::spawn(&p.foe, p.target_at);
    let mitigation = DebuffState::default().mitigation(0.0, 1.0, 0.0, 0.0);
    let damage = DamageVector::new().with(DamageType::Cinematic, 100.0).with(DamageType::Heat, 100.0);
    let got = target.apply(200.0, TypeShares::of(&damage), false, 0.0, &p.foe,
        false, &mitigation, 1.0, None);
    let expected = 100.0 + 100.0 * (1.0 - crate::rules::scaling::armor_damage_reduction(p.foe.armor()));
    assert!((got.effective - expected).abs() < 1e-8, "{} != {expected}", got.effective);
}

#[test]
fn heavy_kill_duration_grant_refreshes_and_expires() {
    let p = FightParams { heavy_status: Some(crate::model::HeavyStatusSpec {
        detonate: true, duration_bonus: 0.3, duration_seconds: 30.0, initial_active: false,
    }), ..FightParams::default() };
    let mut arc = ArcRuntime::init(&p);
    assert_eq!(arc.status_duration(&p, 0.0), 1.0);
    arc.heavy_kill(&p, 2.0);
    assert_eq!(arc.status_duration(&p, 2.0), 1.3);
    arc.heavy_kill(&p, 20.0);
    assert_eq!(arc.status_duration(&p, 40.0), 1.3);
    assert_eq!(arc.status_duration(&p, 50.0), 1.0);
}

#[test]
fn heavy_duration_card_config_replay_and_denial_agree() {
    let mut p = FightParams { heavy_status: Some(crate::model::HeavyStatusSpec {
        detonate: true, duration_bonus: 0.3, duration_seconds: 30.0, initial_active: false,
    }), ..FightParams::default() };
    let mut config = BuffConfig::new();
    config.insert("heavy_status_duration".to_string(), (1, true));
    p.apply_buff_config(&config);
    assert_eq!(ArcRuntime::init(&p).status_duration(&p, 1000.0), 1.3);
    let rep = replay(&p, 5, 3);
    let index = rep.buffs_of(0).iter().position(|b| b.id == "heavy_status_duration").unwrap();
    assert_eq!(rep.frames[0].stacks_of(0)[index], 1);
    p.deny_buff_triggers(&["melee_kill".to_string()]);
    assert!(!p.buff_roster().iter().any(|b| b.id == "heavy_status_duration"));
    assert_eq!(ArcRuntime::init(&p).status_duration(&p, 0.0), 1.0);
    assert!(p.heavy_status.unwrap().detonate);
}

#[test]
fn harmony_heavy_cashes_its_own_proc_before_the_next_attack() {
    let base = crate::model::WeaponBase::from_data("harmony_heavy", false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let arena = crate::arena::Arena::training(0.9);
    let mut p = FightParams::for_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none(), || panel.clone());
    p.base_crit_chance = 0.0;
    p.status_chance = 0.0;
    p.target_at = p.player_at;
    p.combo_script.iter_mut().for_each(|h| { h.windup_seconds = 0.0; h.hit_delay_seconds = Some(0.0); });
    let with = run_once(&p, &mut Rng::new(7));
    p.target_at = crate::rules::space::Vec2::new(0.0, 1.0);
    p.others = (1..=2).map(|i| crate::formation::FoeSpec {
        id: format!("other{i}"), params: Foe::training_dummy(), body_parts: BodyPart::humanoid(),
        at: crate::rules::space::Vec2::new(f64::from(i) * 0.3, 1.0),
    }).collect();
    let crowd = run_once(&p, &mut Rng::new(7));
    assert!(crowd.dot_ticks > with.dot_ticks, "follow-through bodies must detonate their own procs");
    p.others.clear();
    p.heavy_status = None;
    let without = run_once(&p, &mut Rng::new(7));
    assert!(with.total_damage() > without.total_damage(), "cashout={}, plain={}", with.total_damage(), without.total_damage());
}
