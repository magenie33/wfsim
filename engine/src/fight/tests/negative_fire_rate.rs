use super::*;
use crate::model::{ChargeCadence, RefireRule};

fn interval(p: &FightParams) -> f64 {
    let (_, mut me) = open(p, Seat::WIELDER, &mut Rng::new(7), &crate::record::Record::off(), &None);
    seconds_to_next_shot(p, 0.0, p.fire_rate, p.fire_rate, 0.0, None, false, false,
        &me.ammo, &me.incarnon, &mut me.melee)
}

#[test]
fn invalid_fire_rate_does_not_schedule_a_backwards_or_same_time_pull() {
    for rate in [-10.0, -0.0, 0.0, 0.001, f64::NEG_INFINITY, f64::INFINITY, f64::NAN] {
        let p = FightParams { fire_rate: rate, ..no_status() };
        let check = |p: &FightParams| {
            let seconds = interval(p);
            assert!(seconds.is_finite() && seconds >= 59.99, "fire rate {rate}: {seconds}");
        };
        check(&p);
        let charged = FightParams { charge_seconds: Some(0.5), ..p.clone() };
        check(&charged);
        let draw = interval(&FightParams { charge_cadence: ChargeCadence::DrawOnly, ..charged });
        assert_eq!(draw, 0.5, "draw-only uses the same bounded rate on both sides");
        for refire in [RefireRule::NativeRate, RefireRule::SemiAutomatic { skip_wait_on_empty: false },
            RefireRule::SemiAutomatic { skip_wait_on_empty: true }] {
            check(&FightParams { refire, ..p.clone() });
        }
    }
}

#[test]
fn negative_fire_rate_finishes_the_fight_and_leaves_other_seats_running() {
    let parked = FightParams { fire_rate: -1.0, ammo_cost: 0.0, no_magazine: true,
        duration_seconds: 3.0, ..no_status() };
    let healthy = FightParams { fire_rate: 4.0, ammo_cost: 0.0, no_magazine: true,
        duration_seconds: 3.0, ..no_status() };
    let solo = run_once(&parked, &mut Rng::new(7));
    assert_eq!(solo.shots, 1, "the opening pull has no rate-driven successor");
    assert!(solo.total_damage().is_finite());
    let alone = run_once(&healthy, &mut Rng::new(7));
    let pair = run_once(&parked.and_also(healthy), &mut Rng::new(7));
    assert_eq!(pair.shots, 1 + alone.shots);
    assert!(pair.dealt.by_combatant().0[1] > 0.0);
}

#[test]
fn nonpositive_rate_from_mod_resolution_cannot_hang_an_ammo_free_attack() {
    let mut base = crate::model::WeaponBase::from_data("braton", false, &[]);
    base.ammo_cost = 0.0;
    let pool = crate::data::mods::pool_for_weapon("braton");
    let mut penalty = pool.iter().find(|m| m.id == "speed_trigger").unwrap().clone();
    for bonus in [-1.0, -1.5] {
        penalty.effects = vec![crate::model::ModEffect::FireRate(bonus)];
        let panel = crate::build::loadout::resolve(&base, &[&penalty], crate::model::StackPolicy::Emergent);
        assert!(panel.fire_rate <= 0.0, "the static bucket retains its negative balance");
        let p = FightParams::from_panel(&panel, &crate::arena::Arena::training(3.0), &ArcaneFx::none());
        let r = run_once(&p, &mut Rng::new(7));
        assert_eq!(r.shots, 1);
        assert!(r.total_damage().is_finite());
    }
}

#[test]
fn nonpositive_reload_buckets_do_not_create_negative_or_nonfinite_durations() {
    use crate::model::{ModEffect, StackPolicy, WeaponBase};
    let mut penalty = crate::data::mods::pool_for_weapon("braton").into_iter()
        .find(|m| m.id == "speed_trigger").unwrap();
    for bonus in [-1.0, -1.5] {
        penalty.effects = vec![ModEffect::ReloadSpeed(bonus)];
        for id in ["strun", "corinth", "nataruk"] {
            let base = WeaponBase::from_data(id, false, &[]);
            let panel = crate::build::loadout::resolve(&base, &[&penalty], StackPolicy::Emergent);
            assert!(panel.reload_seconds.is_finite() && panel.reload_seconds >= 0.0, "{id}: {}", panel.reload_seconds);
            if let Some(bow) = panel.bow_cycle {
                assert!(bow.nock_seconds.is_finite() && bow.nock_seconds > 0.0);
            }
            let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(3.0), &ArcaneFx::none());
            p.magazine_size = 1.0;
            let r = run_once(&p, &mut Rng::new(7));
            assert!(r.shots <= 2, "{id}: the stalled reload cannot feed repeated shots");
            assert!(r.total_damage().is_finite());
        }
        let p = no_status().with_toggle(no_status(), 1.0, bonus);
        let toggle = p.toggle.as_ref().unwrap();
        assert!(toggle.switch_seconds.is_finite() && toggle.switch_seconds > 0.0);
        assert_eq!(toggle.reload_bucket, bonus);
    }
}

#[test]
fn live_speed_bonuses_join_the_negative_bucket_before_the_clock_floor() {
    let p = FightParams { fire_rate: -2.0, refire: RefireRule::NativeRate,
        fire_rate_on_reload: Some(crate::model::TimedBuff { value: 3.0, duration: 2.0, initial_active: false }), ..no_status() };
    let (_, mut me) = open(&p, Seat::WIELDER, &mut Rng::new(7), &crate::record::Record::off(), &None);
    me.windows.fire_rate_after_reload = 2.0;
    assert_eq!(cadence_rate(&p, &p, 0.0, &mut me.bar, &mut me.buff_stacks, &me.windows), 1.0);
    assert_eq!(cadence_rate(&p, &p, 3.0, &mut me.bar, &mut me.buff_stacks, &me.windows),
        f64::from(3.0_f32 * (1.0_f32 / 60.0)));
    let stalled = 2.0 / crate::rules::sim::reload_divisor(-0.5);
    assert_eq!(reload_span(stalled, -1.5, 0.25), stalled);
    assert!((reload_span(stalled, -1.5, 1.0) - 4.0).abs() < 1e-9);
    assert_eq!(reload_span(2.0, 0.0, -0.5), 4.0);
}

#[test]
fn native_refire_minimum_does_not_become_a_universal_speed_cap() {
    let native = f64::from(1.0_f32 / 60.0);
    assert_eq!(RefireRule::NativeRate.clock_rate(-1.0), native);
    assert_eq!(RefireRule::SemiAutomatic { skip_wait_on_empty: false }.clock_rate(-1.0), native);
    assert_eq!(RefireRule::Rate.clock_rate(-1.0), 1e-9, "fallback is only a numerical guard");
    let assumed = RefireRule::AssumedInterval { assumed_seconds: 0.75 };
    assert_eq!(assumed.interval(-1.0, false), 0.75);
}

#[test]
fn negative_fire_rate_finishes_the_current_burst_without_rewinding_the_next_pull() {
    let p = FightParams { fire_rate: -1.0, ammo_cost: 0.0, no_magazine: true,
        burst: Some(crate::model::BurstSpec { count: 2, delay_seconds: 0.1 }),
        duration_seconds: 3.0, ..no_status() };
    assert_eq!(run_once(&p, &mut Rng::new(7)).shots, 2);
}

#[test]
fn nonpositive_charge_cast_and_transition_speeds_never_rewind_time() {
    use crate::model::{ModEffect, StackPolicy, WeaponBase};
    let mut penalty = crate::data::mods::pool_for_weapon("braton").into_iter()
        .find(|m| m.id == "speed_trigger").unwrap();
    let arena = crate::arena::Arena::training(3.0);
    for bonus in [-1.0, -1.5] {
        for (id, effect) in [("lanka", ModEffect::ChargeRate(bonus)),
            ("magistar_heavy", ModEffect::HeavyWindUpSpeed(bonus))] {
            penalty.effects = vec![effect];
            let base = WeaponBase::from_data(id, true, &[]);
            let panel = crate::build::loadout::resolve(&base, &[&penalty], StackPolicy::Emergent);
            for seconds in panel.charge_seconds.into_iter().chain(panel.combo_script.iter().map(|h| h.windup_seconds)) {
                assert!(seconds.is_finite() && seconds >= 0.0, "{id}: {seconds}");
            }
            let p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
            assert!(run_once(&p, &mut Rng::new(7)).total_damage().is_finite());
        }
        penalty.effects = vec![ModEffect::ReloadSpeed(bonus)];
        let build = |id| crate::build::loadout::resolve(&WeaponBase::from_data(id, false, &[]), &[&penalty], StackPolicy::Emergent);
        let p = FightParams::incarnon_cycle_from_panels(&build("torid_incarnon"), &build("torid"), false,
            LockMode::Initial(0), &arena, &ArcaneFx::none());
        let cycle = p.cycle.as_ref().unwrap();
        assert!(cycle.transmute_seconds.is_finite() && cycle.transmute_seconds >= 0.0);
        assert!(cycle.transmute_out_seconds.is_finite() && cycle.transmute_out_seconds >= 0.0);
        assert_eq!(cycle.reload_bucket, bonus);
        let spells = crate::data::abilities::resolve(&[crate::data::abilities::AbilityPick {
            id: "roar", duration_seconds: None, element: None }],
            &crate::data::abilities::Caster { casting_speed_bonus: bonus, ..Default::default() }, "rifle", "primary");
        assert!(spells[0].cast_seconds.is_finite() && spells[0].cast_seconds >= 0.0);
    }
    let p = FightParams { projectile_speed_mps: Some(-1.0), ..no_status() };
    assert_eq!(p.flight_seconds(8.0), f64::INFINITY);
}
