// SPDX-License-Identifier: AGPL-3.0-or-later
use wfsim_engine::{arena::Arena, build::loadout, data::{arcanes::ArcaneFx, mods, weapons}, fight::{FightParams, record}, formation::FoeSpec, model::{Falloff, FalloffSpec, MeleeEventKind, StackPolicy, WeaponBase}, record::{Kind, Origin}, rules::space::{Vec2, BODY_RADIUS_M}};

#[test]
fn each_scythe_heavy_keeps_its_blade_hit_and_launches_a_separate_wave() {
    for id in ["caustacyst_heavy", "coda_caustacyst_heavy"] {
        let source = weapons::spec(id).unwrap();
        let script = &source.attack.combo_script;
        assert_eq!(script.len(), 4);
        assert_eq!(script.iter().filter(|h| h.ends_input()).count(), 2);
        for input in script.chunks_exact(2) {
            assert_eq!(input[0].profile.as_deref(), Some("wave"));
            assert!(matches!(input[0].event_timing.unwrap().kind, MeleeEventKind::Ranged));
            assert_eq!(input[1].profile, None);
            assert_eq!(input[1].multiplier, 6.0);
            assert!(input[1].forced_procs.iter().any(|p| p == "slash"));
        }
        let base = WeaponBase::from_data(id, false, &[]);
        let pool = mods::pool_for_weapon(id);
        let pressure = pool.iter().find(|m| m.id == "pressure_point").unwrap();
        let plain = loadout::resolve(&base, &[], StackPolicy::Emergent);
        let modded = loadout::resolve(&base, &[pressure], StackPolicy::Emergent);
        assert!(modded.modified_base > plain.modified_base);
        assert_eq!(plain.melee_profiles["wave"].modified_base, modded.melee_profiles["wave"].modified_base);
        let wave = &plain.melee_profiles["wave"];
        assert_eq!(wave.modified_base, 225.0);
        assert_eq!(wave.projectile_speed_mps, Some(15.0));
        assert_eq!(wave.range_m, 37.5);
        assert_eq!(wave.damage_loss_per_body, 0.2);
    }
}

fn line(loss: f64, distance_loss: bool) -> FightParams {
    let panel = loadout::resolve(&WeaponBase::from_data("braton", true, &[]), &[], StackPolicy::Emergent);
    let mut arena = Arena::training(0.1);
    for i in 1..=5 {
        arena.others.push(FoeSpec { id: format!("e{}", i + 1), params: arena.target.clone(),
            body_parts: arena.body_parts.clone(), at: Vec2::new(arena.target_at.x + 3.0 * f64::from(i), arena.target_at.y) });
    }
    let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
    p.damage_loss_per_body = loss;
    p.infinite_body_punch_through = true;
    p.status_chance = 0.0;
    p.base_crit_chance = 0.0;
    p.spread = None;
    if distance_loss {
        p.falloff = Some(Falloff::from_spec(&FalloffSpec { start_m: 0.0, end_m: 37.5, reduction: 1.0, points: Vec::new() }, 1.0));
    }
    p
}

fn damage(p: &FightParams) -> [f64; 6] {
    let mut totals = [0.0; 6];
    for e in record(p, 3, 0.0, p.duration_seconds, 10_000, 0).events() {
        if let Kind::Damage(d) = &e.kind {
            if d.origin == Origin::Own && !d.radial {
                if let Some(body) = e.subject { totals[usize::from(body)] += d.effective; }
            }
        }
    }
    totals
}

#[test]
fn each_crossed_body_spends_a_flat_share_alongside_travel_decay() {
    for distance_loss in [false, true] {
        let p = line(0.2, distance_loss);
        let values = damage(&p);
        let control = damage(&line(0.0, false));
        for (i, value) in values.iter().enumerate() {
            let gap = if i == 0 { 0.0 } else { (p.range_to(p.others[i - 1].at) - BODY_RADIUS_M).max(0.0) };
            let wanted = (1.0 - 0.2 * i as f64 - if distance_loss { gap / 37.5 } else { 0.0 }).max(0.0);
            assert!((value / control[i] - wanted).abs() < 1e-8, "body {i}: {values:?}, expected share {wanted}");
        }
        assert_eq!(values[5], 0.0);
        assert!(control[5] > 0.0);
    }
}
