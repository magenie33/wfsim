use super::*;

fn panel(id: &str, mods: &[&str]) -> crate::build::loadout::ResolvedPanel {
    let base = crate::model::WeaponBase::from_data(id, false, &[]);
    let pool = crate::data::mods::pool_for_weapon(id);
    let chosen: Vec<_> = mods.iter().map(|id| pool.iter().find(|m| m.id == *id).unwrap()).collect();
    crate::build::loadout::resolve(&base, &chosen, crate::model::StackPolicy::Emergent)
}

fn fight(id: &str, mods: &[&str], seconds: f64) -> FightParams {
    let mut arena = crate::arena::Arena::training(seconds);
    arena.target_at = crate::rules::space::Vec2::ORIGIN;
    FightParams::from_panel(&panel(id, mods), &arena, &ArcaneFx::none())
}

fn arrows(p: &FightParams) -> Vec<f64> {
    crate::fight::record(p, 42, 0.0, p.duration_seconds, 10000, 0).events().iter()
        .filter(|e| matches!(e.kind, crate::record::Kind::Strike { .. }))
        .map(|e| e.t).collect()
}

#[test]
fn bows_and_magazine_fed_charge_weapons_have_distinct_cycles() {
    for id in ["kuva_bramma", "cernos_prime", "cernos_prime_uncharged", "nataruk_perfect"] {
        let p = panel(id, &[]);
        assert!(p.bow_cycle.unwrap().uses_reserve, "{id}");
    }
    for id in ["ballistica", "rakta_ballistica", "ballistica_prime", "ballistica_prime_uncharged",
        "ballistica_incarnon", "attica", "zhuge", "opticor"] {
        assert!(panel(id, &[]).bow_cycle.is_none(), "{id} must keep its magazine");
    }
}

#[test]
fn a_bow_finishes_its_first_draw_before_releasing() {
    let p = fight("kuva_bramma", &[], 4.0);
    let draw = p.charge_seconds.unwrap();
    let fired = arrows(&p);
    assert!((fired[0] - draw).abs() < 1e-9, "first arrow: {} vs draw {draw}", fired[0]);
    let short = FightParams { duration_seconds: draw * 0.5, ..p };
    assert_eq!(run_once(&short, &mut Rng::new(42)).shots, 0);
}

#[test]
fn nocking_never_counts_as_a_reload_or_triggers_reload_buffs() {
    let plain = fight("kuva_bramma", &[], 10.0);
    let mut with_buff = plain.clone();
    with_buff.fire_rate_on_reload = Some(crate::model::TimedBuff {
        value: 10.0, duration: 60.0, initial_active: false,
    });
    with_buff.base_damage_on_reload = with_buff.fire_rate_on_reload;
    let a = run_once(&plain, &mut Rng::new(42));
    let b = run_once(&with_buff, &mut Rng::new(42));
    assert_eq!(a.reloads, 0);
    assert_eq!(b.reloads, 0);
    assert_eq!(a.shots, b.shots);
    assert_eq!(a.total_damage(), b.total_damage());
    let rec = crate::fight::record(&with_buff, 42, 0.0, 10.0, 10000, 0);
    assert!(!rec.events().iter().any(|e| matches!(e.kind,
        crate::record::Kind::ReloadStart { .. } | crate::record::Kind::ReloadEnd)));
}

#[test]
fn ammo_efficiency_saves_arrows_without_skipping_nocks() {
    let mut p = fight("kuva_bramma", &[], 20.0);
    let bare = arrows(&p);
    p.arcane.ammo_efficiency = 0.75;
    assert_eq!(arrows(&p), bare);
    p.infinite_reserve = false;
    p.reserve_ammo = 3.0;
    let efficient = arrows(&p);
    assert_eq!(efficient.len(), 12);
    assert_eq!(efficient, bare[..12]);
    p.arcane.ammo_efficiency = 0.0;
    assert_eq!(arrows(&p), bare[..3]);
    p.reserve_ammo = 0.0;
    assert!(arrows(&p).is_empty(), "no free arrow in a fictional magazine");
}

#[test]
fn reload_speed_shortens_nocking_but_not_the_draw() {
    let bare = fight("kuva_bramma", &[], 5.0);
    let quick = fight("kuva_bramma", &["primed_fast_hands"], 5.0);
    let a = arrows(&bare);
    let b = arrows(&quick);
    assert_eq!(a[0], b[0]);
    let want = quick.charge_seconds.unwrap() + quick.bow_cycle.unwrap().nock_seconds;
    assert!((b[1] - b[0] - want).abs() < 1e-9);
    assert!(b[1] - b[0] < a[1] - a[0]);
}

#[test]
fn incarnon_arrows_nock_between_releases_and_spend_their_charge_pool() {
    for id in ["paris_prime_incarnon", "dread_incarnon"] {
        let mut p = fight(id, &[], 8.0);
        let bow = p.bow_cycle.unwrap();
        assert!(!bow.uses_reserve);
        p.infinite_reserve = false;
        p.reserve_ammo = 0.0;
        let fired = arrows(&p);
        let want = p.charge_seconds.unwrap() + bow.nock_seconds;
        assert!((fired[1] - fired[0] - want).abs() < 1e-9, "{id}");
        p.arcane.ammo_efficiency = 0.75;
        assert_eq!(arrows(&p), fired, "{id} charges ignore ammo efficiency");
    }
}

#[test]
fn ballistica_keeps_its_charge_interval_magazine_and_reload() {
    for id in ["ballistica", "rakta_ballistica", "ballistica_prime"] {
        let p = fight(id, &[], 40.0);
        let fired = arrows(&p);
        let interval = p.charge_seconds.unwrap() + 1.0 / p.fire_rate;
        assert!((fired[1] - fired[0] - interval).abs() < 1e-9, "{id}");
        assert!(run_once(&p, &mut Rng::new(42)).reloads > 0, "{id}");
        let bigger = fight(id, &["slip_magazine"], 40.0);
        assert!(bigger.magazine_size > p.magazine_size, "{id}");
        for mut finite in [p, bigger] {
            finite.infinite_reserve = false;
            finite.reserve_ammo = 0.0;
            assert_eq!(run_once(&finite, &mut Rng::new(42)).shots,
                (finite.magazine_size / finite.ammo_cost).floor() as u32, "{id}");
        }
    }
}

#[test]
fn an_incarnon_bow_earns_charges_then_returns_to_its_finite_arrow_supply() {
    let base = panel("paris_prime", &[]);
    let incarnon = panel("paris_prime_incarnon", &[]);
    let mut arena = crate::arena::Arena::training(60.0);
    arena.target_at = crate::rules::space::Vec2::ORIGIN;
    arena.body_parts = vec![BodyPart { name: "head".into(), aim_weight: 1.0, multiplier: 1.0,
        is_head: true, is_weak_point: true, crit_bonus: false }];
    let mut p = FightParams::incarnon_cycle_from_panels(&incarnon, &base, false,
        LockMode::Initial(0), &arena, &ArcaneFx::none());
    p.infinite_reserve = false;
    p.reserve_ammo = 4.0;
    let rec = crate::fight::record(&p, 42, 0.0, 60.0, 10000, 0);
    let fired: Vec<_> = rec.events().iter()
        .filter(|e| matches!(e.kind, crate::record::Kind::Strike { .. })).collect();
    assert_eq!(fired.iter().filter(|e| !e.weapon.transmuted).count(), 4);
    assert_eq!(fired.iter().filter(|e| e.weapon.transmuted).count(), 20);
    assert!(fired.iter().filter(|e| e.weapon.transmuted).all(|e| e.weapon.reserve == Some(0.0)));
    assert_eq!(run_once(&p, &mut Rng::new(42)).reloads, 0);
}
