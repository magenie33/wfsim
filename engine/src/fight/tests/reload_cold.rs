use super::*;

fn pulse(rounds: u32, center: crate::rules::space::Vec2) -> ReloadCold {
    ReloadCold { owner: Seat::WIELDER, at: 2.0, center, rounds, radius_m: f64::from(rounds) }
}

fn resolve(mods: &[&str]) -> crate::build::loadout::ResolvedPanel {
    let pool = crate::data::mods::pool_for_build("aksondol", &[]);
    let refs: Vec<_> = mods.iter().map(|id| pool.iter().find(|m| m.id == *id).expect(id)).collect();
    let base = crate::model::WeaponBase::from_data("aksondol", false, &[]);
    crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
}

fn apply(p: &FightParams, wave: ReloadCold, before: usize) -> (Fight, Combatant<'_>) {
    let mut rec = crate::record::Record::off();
    let (mut fight, mut me) = open(p, Seat::WIELDER, &mut Rng::new(3), &rec, &None);
    fight.bodies[0].debuffs.freeze = vec![100.0; before];
    reload_cold_wave(wave, &mut turn!(me, p, &mut me.d), &mut fight.r, &mut rec, &mut fight.bodies);
    (fight, me)
}

#[test]
fn aksondol_wave_resolves_magazine_range_duration_and_projectiles() {
    let plain = resolve(&[]);
    assert_eq!(plain.reload_cold_radius_per_round_m, Some(1.0));
    assert_eq!(plain.magazine_size, 8.0);
    assert_eq!(plain.fire_rate, 4.0);
    assert_eq!(plain.projectile_speed_mps, Some(110.0));
    let bigger = resolve(&["slip_magazine", "primed_fulmination", "perpetual_agony", "augur_seeker"]);
    assert_eq!(bigger.magazine_size, 10.0);
    assert!((bigger.reload_cold_radius_per_round_m.unwrap() - 1.44).abs() < 1e-9);
    assert!((bigger.status_duration_multiplier - 2.8).abs() < 1e-9);
}

#[test]
fn aksondol_wave_uses_player_position_and_reaches_each_body_once() {
    use crate::rules::space::Vec2;
    let mut p = FightParams { player_at: Vec2::new(20.0, 0.0), target_at: Vec2::new(28.0 + crate::rules::space::BODY_RADIUS_M, 0.0), ..Default::default() };
    p.others = vec![crate::formation::FoeSpec {
        id: "outside".into(), params: Foe::training_dummy(), body_parts: BodyPart::humanoid(), at: Vec2::new(28.01 + crate::rules::space::BODY_RADIUS_M, 0.0),
    }, crate::formation::FoeSpec {
        id: "inside".into(), params: Foe::training_dummy(), body_parts: BodyPart::humanoid(), at: Vec2::new(20.0, 7.0),
    }];
    let (fight, _) = apply(&p, pulse(8, p.player_at), 0);
    assert_eq!(fight.bodies[0].debuffs.freeze.len(), 8);
    assert_eq!(fight.bodies[1].debuffs.freeze.len(), 0);
    assert_eq!(fight.bodies[2].debuffs.freeze.len(), 8);
    assert_eq!(fight.r.procs, 16);
    assert!(fight.bodies[0].debuffs.freeze.iter().all(|&at| at == 8.0));
}

#[test]
fn aksondol_wave_freezes_at_ten_and_duration_mods_scale_both_clocks() {
    let p = FightParams { status_duration_multiplier: 2.8, ..Default::default() };
    let wave = pulse(8, p.player_at);
    let (plain, _) = apply(&p, wave, 0);
    assert_eq!(plain.bodies[0].debuffs.frozen_until, None);
    let (mut frozen, _) = apply(&p, wave, 2);
    let debuffs = &mut frozen.bodies[0].debuffs;
    assert!((debuffs.frozen_until.unwrap() - 10.4).abs() < 1e-9);
    debuffs.prune(10.4 + 1e-9, 2.8);
    assert_eq!(debuffs.freeze.len(), 3);
    assert!(debuffs.freeze.iter().all(|&at| (at - 27.2).abs() < 1e-9));
    let (large, _) = apply(&p, pulse(10, p.player_at), 0);
    assert!(large.bodies[0].debuffs.frozen_until.is_some());
}

#[test]
fn aksondol_wave_obeys_cold_immunity_caps_and_frozen_inertia() {
    let mut p = FightParams::default();
    p.foe.status_immunities.push(DamageType::Cold);
    let (immune, _) = apply(&p, pulse(20, p.player_at), 0);
    assert_eq!(immune.r.procs, 0);
    p.foe.status_immunities.clear();
    p.foe.base_overguard = 1000.0;
    let (capped, _) = apply(&p, pulse(20, p.player_at), 0);
    assert_eq!(capped.bodies[0].debuffs.freeze.len(), 4);
    assert_eq!(capped.bodies[0].debuffs.frozen_until, None);
    p.foe.base_overguard = 0.0;
    p.foe.cannot_be_frozen = true;
    let (boss, _) = apply(&p, pulse(20, p.player_at), 0);
    assert_eq!(boss.bodies[0].debuffs.freeze.len(), 10);
    assert_eq!(boss.bodies[0].debuffs.frozen_until, None);
    p.foe.cannot_be_frozen = false;
    let (frozen, _) = apply(&p, pulse(20, p.player_at), 0);
    assert_eq!(frozen.bodies[0].debuffs.freeze.len(), 10);
    assert_eq!(frozen.r.procs, 10);
}

#[test]
fn aksondol_wave_never_triggers_empowered_or_frostbite() {
    let mut p = FightParams { arcane: arc("primary_frostbite"), ..Default::default() };
    p.arcane.flat_damage_on_status = 750.0;
    let (fight, me) = apply(&p, pulse(8, p.player_at), 0);
    assert_eq!(fight.r.sources.arcane_on_status, 0.0);
    assert!(me.arc.states.iter().all(|s| s.stacks == 0));
}

#[test]
fn aksondol_wave_encumber_grants_at_most_one_bonus_proc() {
    let mut p = FightParams::default();
    p.arcane.encumber_chance = 1.0;
    let (fight, _) = apply(&p, pulse(8, p.player_at), 0);
    assert_eq!(fight.r.procs, 9);
}

#[test]
fn aksondol_wave_starts_with_reload_even_when_no_shot_follows() {
    let p = FightParams {
        damage: DamageVector::new(), status_chance: 0.0, forced_procs: vec![],
        fire_rate: 4.0, magazine_size: 8.0, reload_seconds: 2.6,
        duration_seconds: 2.01, reload_cold_radius_per_round_m: Some(1.0),
        arcane: crate::data::arcanes::ArcaneFx::none(), ..Default::default()
    };
    let before = FightParams { duration_seconds: 1.99, ..p.clone() };
    assert_eq!(run_once(&before, &mut Rng::new(3)).procs, 0);
    assert_eq!(run_once(&p, &mut Rng::new(3)).procs, 8);
    let no_wave = FightParams { reload_cold_radius_per_round_m: None, ..p };
    assert_eq!(run_once(&no_wave, &mut Rng::new(3)).procs, 0);
}

#[test]
fn aksondol_partial_reload_scales_with_rounds_spent() {
    let p = FightParams { reload_cold_radius_per_round_m: Some(1.44), ..Default::default() };
    let rec = crate::record::Record::off();
    let (_, mut me) = open(&p, Seat::WIELDER, &mut Rng::new(3), &rec, &None);
    schedule_reload_effects(&p, &mut me.ammo, 5.0, 8.0, 1.0, 2.6, false);
    let wave = me.ammo.reload_cold.unwrap();
    assert_eq!(wave.at, 1.0);
    assert_eq!(wave.rounds, 3);
    assert!((wave.radius_m - 4.32).abs() < 1e-9);
    schedule_reload_effects(&p, &mut me.ammo, 0.0, 20.0, 4.0, 2.6, true);
    assert!((me.ammo.reload_cold.unwrap().radius_m - 28.8).abs() < 1e-9);
}

#[test]
fn aksondol_encumber_uses_the_weapons_status_payload_and_live_buffs() {
    let mut damaging = std::collections::BTreeSet::new();
    for live_arcane in [false, true] {
        for live_reload in [false, true] {
            let mut p = FightParams {
                dot_modified_base: Some(224.0), base_damage_bonus: 0.6,
                status_damage_multiplier: 1.7,
                base_damage_on_reload: Some(crate::model::TimedBuff { value: 0.8, duration: 10.0, initial_active: false }),
                arcane: if live_arcane { arc_stacked("secondary_merciless") } else { ArcaneFx::none() },
                ..Default::default()
            };
            p.arcane.encumber_chance = 1.0;
            let base = 224.0 + if live_arcane { 504.0 } else { 0.0 }
                + if live_reload { 112.0 } else { 0.0 };
            for seed in 0..64 {
                let mut rec = crate::record::Record::off();
                let (mut wave_fight, mut wave_me) = open(&p, Seat::WIELDER, &mut Rng::new(seed), &rec, &None);
                wave_me.windows.base_damage_after_reload = if live_reload { 10.0 } else { 1.0 };
                reload_cold_wave(pulse(1, p.player_at), &mut turn!(wave_me, &p, &mut wave_me.d),
                    &mut wave_fight.r, &mut rec, &mut wave_fight.bodies);

                let (mut shot_fight, mut shot_me) = open(&p, Seat::WIELDER, &mut Rng::new(seed), &rec, &None);
                let bonus = encumber_proc(1.0, true, &mut false, &mut shot_me.d.extra).unwrap();
                let here = &mut shot_fight.bodies[0];
                let mit = here.debuffs.mitigation(2.0, p.status_duration_multiplier,
                    p.armor_strip_per_puncture, p.squad.enemy_armor_multiplier);
                settle_procs(vec![DamageType::Cold, bonus], Seat::WIELDER, 2.0,
                    InstanceScale {
                        mb_live: base, crit_multiplier: 1.0, part_factor: 1.0, landing: 1.0,
                        attrition: 1.0, xh_bracket: p.extra_hit_bracket(2.0, &shot_me.windows), status_damage_live: 0.0,
                    }, &mut here.debuffs, &mut shot_me.gal, &mut shot_me.arc, &mut here.state,
                    &p, &p, &mit, &mut shot_fight.r, &mut rec, &mut shot_me.d.status, &p.foe, DEPTH_PROC);
                let got = &wave_fight.bodies[0].debuffs;
                let want = &shot_fight.bodies[0].debuffs;
                assert_eq!(got.dots.len(), want.dots.len());
                for (a, b) in got.dots.iter().zip(&want.dots) {
                    damaging.insert(a.dtype as usize);
                    assert!((a.frozen - b.frozen).abs() < 1e-8, "{bonus:?}: {} != {}", a.frozen, b.frozen);
                    assert_eq!(a.bracket, b.bracket);
                    assert_eq!(a.depth, b.depth);
                }
                if let (Some(a), Some(b)) = (&got.heat, &want.heat) {
                    damaging.insert(DamageType::Heat as usize);
                    assert!((a.value - b.value).abs() < 1e-8, "heat: {} != {}", a.value, b.value);
                }
            }
        }
    }
    assert_eq!(damaging.len(), 5, "all five damaging DoT families were exercised");
}
