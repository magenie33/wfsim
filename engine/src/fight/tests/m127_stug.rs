//! Stug's measured damage and independent entity clocks (M127).
use super::*;
use crate::record::{Kind, Origin};

fn measured(id: &str, shots: u32, hornet: bool, head: bool) -> FightParams {
    let base = crate::model::WeaponBase::from_data(id, true, &["stug_overcharge_blast"]);
    let pool = crate::data::mods::pool_for_weapon(id);
    let mods: Vec<_> = pool.iter().filter(|m| hornet && m.id == "hornet_strike").collect();
    let panel = crate::build::loadout::resolve(&base, &mods, crate::model::StackPolicy::Emergent);
    let arc = ArcaneFx { headshot_multiplier_bonus: if head { 0.3 } else { 0.0 }, ..ArcaneFx::none() };
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(4.0), &arc);
    p.magazine_size = f64::from(shots);
    p.reload_seconds = 100.0;
    p.fire_rate = 10.0;
    p.body_parts = if head { all_head() } else { mono_body(1.0) };
    p.projectile_speed_mps = None;
    p.foe.base_health = 1e15;
    // The reported numbers are non-crits; status ticks are not this reading.
    let b = p.blob.as_mut().expect("both forms carry their entities");
    b.hit.crit_chance = 0.0;
    b.hit.status_chance = 0.0;
    b.explosion.crit_chance = 0.0;
    b.explosion.status_chance = 0.0;
    p
}

fn numbers(p: &FightParams) -> Vec<(f64, bool, f64)> {
    record(p, 127, 0.0, f64::INFINITY, 100_000, 0).events().iter()
        .filter_map(|e| match &e.kind {
            Kind::Damage(d) if d.origin == Origin::Blob => Some((e.t, d.radial, d.raw)),
            Kind::Damage(d) if d.origin == Origin::Own => panic!("blob contact dealt damage: {}", d.raw),
            _ => None,
        }).collect()
}

#[test]
fn m127_dead_hosts_leave_their_blob_for_replacement_enemies() {
    for id in ["stug", "stug_incarnon"] {
        let mut p = measured(id, 1, false, false);
        p.foe.mode = TargetMode::InstantRespawn;
        p.foe.base_health = 200.0;
        p.duration_seconds = 1.6;
        let r = run_once(&p, &mut Rng::new(127));
        assert_eq!(r.shots, 1);
        assert_eq!(r.kills, 3, "creation tick, corpse tick, corpse explosion: {id}");
        let got = numbers(&p);
        assert_eq!(got.len(), 3);
        assert_eq!(got.iter().map(|h| h.1).collect::<Vec<_>>(), [false, false, true]);
        assert!((got[2].0 - 1.5).abs() < 1e-9);
    }
}

#[test]
fn m127_enervate_resets_on_blob_damage_and_reads_the_live_ramp() {
    for id in ["stug", "stug_incarnon"] {
        let mut p = measured(id, 1, false, false);
        p.arcane.enervate_rank = Some(0);
        p.enervate_stacks = 20;
        for n in 0..8 {
            p.others.push(crate::formation::FoeSpec {
                id: format!("neighbour{n}"), params: p.foe.clone(),
                body_parts: mono_body(1.0), at: p.target_at,
            });
        }
        let rec = record(&p, 127, 0.0, f64::INFINITY, 100_000, 0);
        let hits: Vec<_> = rec.events().iter().filter_map(|e| match &e.kind {
            Kind::Damage(d) if d.origin == Origin::Blob => Some((e.t, d.crit_tier)),
            _ => None,
        }).collect();
        assert_eq!(hits.len(), 27);
        assert!(hits[..9].iter().all(|h| h.1 == 2), "same AoE shares pre-hit ramp: {hits:?}");
        assert!(hits[9..18].iter().all(|h| h.1 == 0), "next tick sees the reset during reload: {hits:?}");
        assert!(hits[18..].iter().all(|h| h.1 <= 1), "explosion sees one gain, not nine: {hits:?}");
    }
}

#[test]
fn m127_enervate_gains_once_per_blob_event_not_contact_or_victim() {
    for id in ["stug", "stug_incarnon"] {
        let mut p = measured(id, 1, false, false);
        p.arcane.enervate_rank = Some(5);
        p.enervate_stacks = 20;
        for n in 0..8 {
            p.others.push(crate::formation::FoeSpec {
                id: format!("neighbour{n}"), params: p.foe.clone(),
                body_parts: mono_body(1.0), at: p.target_at,
            });
        }
        let rep = replay(&p, 127, 16);
        let i = rep.buffs_of(0).iter().position(|b| b.id == "arcane:secondary_enervate").unwrap();
        assert_eq!(rep.frames.last().unwrap().stacks_of(0)[i], 23,
            "two ticks and one explosion; crowd counts each instant once");
    }
}

#[test]
fn m127_measured_body_and_head_damage_in_both_forms() {
    for (id, shots, hornet, head, tick, blast) in [
        ("stug", 1, false, false, 304.0, 375.0),
        ("stug", 3, false, false, 312.0, 525.0),
        ("stug", 1, true, false, 973.0, 1200.0),
        ("stug", 2, true, false, 986.0, 1440.0),
        ("stug", 4, true, false, 1011.0, 1920.0),
        ("stug", 1, true, true, 973.0, 1560.0),
        ("stug_incarnon", 1, true, false, 1120.0, 1600.0),
        ("stug_incarnon", 1, true, true, 1120.0, 2080.0),
        ("stug_incarnon", 2, true, false, 1280.0, 2240.0),
        ("stug_incarnon", 2, true, true, 1280.0, 2912.0),
    ] {
        let got = numbers(&measured(id, shots, hornet, head));
        let ticks: Vec<_> = got.iter().filter(|(_, radial, _)| !radial).collect();
        let blasts: Vec<_> = got.iter().filter(|(_, radial, _)| *radial).collect();
        assert_eq!(ticks.len(), 2, "{id} {shots}: {got:?}");
        assert_eq!(blasts.len(), 1, "{id} {shots}: {got:?}");
        assert_eq!(ticks.last().unwrap().2.round(), tick, "{id} {shots}: {got:?}");
        assert_eq!(blasts[0].2.round(), blast, "{id} {shots}: {got:?}");
        assert!((ticks[0].0 - 0.0).abs() < 1e-9);
        assert!((ticks[1].0 - 1.0).abs() < 1e-9);
        assert!((blasts[0].0 - (1.5 + f64::from(shots - 1) * 0.1)).abs() < 1e-9);
    }
}

#[test]
fn m127_refresh_does_not_move_tick_phase_or_round_expiry() {
    let mut p = measured("stug", 2, false, false);
    p.fire_rate = 1.0 / 1.4;
    let got = numbers(&p);
    assert_eq!(got.iter().map(|x| x.1).collect::<Vec<_>>(), [false, false, false, true]);
    for (got, want) in got.iter().zip([0.0, 1.0, 2.0, 2.9]) {
        assert!((got.0 - want).abs() < 1e-9, "{got:?} vs {want}");
    }
    assert_eq!(got[0].2.round(), 304.0);
    assert_eq!(got[2].2.round(), 308.0);
    assert_eq!(got[3].2.round(), 450.0);
}

#[test]
fn m127_the_head_explosion_bracket_reaches_neighbours() {
    let mut p = measured("stug", 1, true, true);
    p.others.push(crate::formation::FoeSpec {
        id: "body_only".into(), params: p.foe.clone(), body_parts: mono_body(1.0), at: p.target_at,
    });
    let rec = record(&p, 127, 0.0, f64::INFINITY, 100_000, 0);
    let blasts: Vec<_> = rec.events().iter().filter_map(|e| match &e.kind {
        Kind::Damage(d) if d.origin == Origin::Blob && d.radial => Some((e.subject, d.raw.round())),
        _ => None,
    }).collect();
    assert_eq!(blasts, [(Some(0), 1560.0), (Some(1), 1560.0)]);
}

#[test]
fn m127_fractional_multishot_rolls_discrete_stacks() {
    let mut p = measured("stug", 1, false, false);
    p.multishot = 1.6;
    let mut two = 0;
    for seed in 0..200 {
        let rec = record(&p, seed, 0.0, f64::INFINITY, 100_000, 0);
        let value = rec.events().iter().find_map(|e| match &e.kind {
            Kind::Damage(d) if d.origin == Origin::Blob => Some(d.raw.round()),
            _ => None,
        }).unwrap();
        assert!(value == 304.0 || value == 308.0, "discrete: {value}");
        two += u32::from(value == 308.0);
    }
    assert!((90..150).contains(&two), "both stack counts occur at the stated rate: {two}");
}

#[test]
fn m109_birth_at_the_cap_waits_but_a_growing_merge_detonates() {
    for (id, cap) in [("stug", 10), ("stug_incarnon", 5)] {
        for ms in [cap, cap + 1] {
            let mut p = measured(id, 1, false, false);
            p.multishot = f64::from(ms);
            let got = numbers(&p);
            let blast = got.iter().find(|(_, radial, _)| *radial).unwrap();
            assert!((blast.0 - 1.5).abs() < 1e-9, "{id}: born full, {got:?}");
        }
        let mut p = measured(id, 3, false, false);
        p.multishot = f64::from((cap + 1) / 2);
        let got = numbers(&p);
        let blast = got.iter().find(|(_, radial, _)| *radial).unwrap();
        assert!((blast.0 - 0.1).abs() < 1e-9, "{id}: merge reaches cap, {got:?}");
    }
}

#[test]
fn m127_forms_and_attachment_parts_keep_separate_piles() {
    let p = measured("stug", 1, false, false).blob.unwrap();
    let contact = BlobContact {
        body: 0, spot: 0, head: false, weakpoint: false, critical_head: false,
        base_form: true, generation: 0, head_bracket: 1.0,
    };
    let mut entities = Vec::new();
    let seat = crate::record::Seat(0);
    assert!(embed_blob(&mut entities, p, seat, contact, 1, 0.0).is_none());
    assert!(embed_blob(&mut entities, p, seat, BlobContact { spot: 1, ..contact }, 1, 0.1).is_none());
    assert!(embed_blob(&mut entities, p, seat, BlobContact { base_form: false, ..contact }, 1, 0.2).is_none());
    assert_eq!(entities.len(), 3);
    assert!(embed_blob(&mut entities, p, seat, contact, 1, 1.4).is_none());
    let Entity::Pile(first) = entities[0] else { unreachable!() };
    assert_eq!(first.stacks, 2);
    assert_eq!(first.next_tick, 0.0, "feeding never moves the entity's tick clock");
    assert!((first.fuse_at - 2.9).abs() < 1e-9);
}

#[test]
fn m127_evolution_four_reaches_both_entity_damage_parts() {
    for id in ["stug", "stug_incarnon"] {
        for perk in ["stug_survivors_edge", "stug_deathtrap_trigger", "stug_elemental_dominance"] {
            let base = crate::model::WeaponBase::from_data(id, true, &[perk]);
            let b = base.blob.as_ref().unwrap();
            for part in [&b.hit, &b.explosion] {
                assert_eq!(part.base_crit_chance, base.base_crit_chance, "{id} {perk}");
                assert_eq!(part.base_crit_damage, base.base_crit_damage, "{id} {perk}");
                assert_eq!(part.base_status_chance, base.base_status_chance, "{id} {perk}");
            }
        }
    }
}

#[test]
fn m127_body_contact_and_explosion_charge_once_each_between_pulls() {
    for threshold in [2, 3] {
        let mut p = measured("stug_incarnon", 1, false, false);
        let mut base = measured("stug", 1, false, false);
        base.fire_rate = 0.2;
        base.multishot = 2.0;
        p.others.push(crate::formation::FoeSpec {
            id: "neighbour".into(), params: p.foe.clone(), body_parts: mono_body(1.0), at: p.target_at,
        });
        p.cycle = Some(IncarnonCycle {
            base_form: Box::new(base), starts_primed: false,
            arms: Arms::Gauge { charge_on: crate::model::ChargeOn::DirectHits, charges_to_fill: threshold },
            ends: Ends::ChargeMagazine, transmute_seconds: 0.5,
            transmute_out_seconds: 0.5, reload_bucket: 0.0,
        });
        let rec = record(&p, 127, 0.0, f64::INFINITY, 100_000, 0);
        let into = rec.events().iter().find_map(|e| match e.kind {
            Kind::TransformStart { into_transmuted: true, .. } => Some(e.t),
            _ => None,
        });
        if threshold == 2 {
            assert!((into.expect("the timed explosion fills the gauge") - 1.5).abs() < 1e-9);
        } else {
            assert!(into.is_none(), "no charge from attached ticks, multishot or extra victims: {into:?}");
        }
    }
}

#[test]
fn m127_explosions_trigger_weakpoint_events_but_attached_ticks_do_not() {
    for head in [false, true] {
        for tick_kill in [false, true] {
            for arcane in ["secondary_deadhead", "secondary_merciless"] {
                let mut p = measured("stug", 1, false, head);
                p.foe.mode = TargetMode::InstantRespawn;
                p.foe.base_health = if tick_kill { 500.0 } else { 900.0 };
                p.duration_seconds = if tick_kill { 1.1 } else { 1.6 };
                p.arcane = arc(arcane);
                for b in &mut p.arcane.buffs { b.initial_stacks = 0; }
                let r = run_once(&p, &mut Rng::new(127));
                assert_eq!(r.kills, 1, "{arcane} head={head} tick={tick_kill}");
                assert_eq!(r.weakpoint_hits, u32::from(head && !tick_kill));
                let rep = replay(&p, 127, 16);
                let id = format!("arcane:{arcane}");
                let index = rep.buffs_of(0).iter().position(|b| b.id == id).expect("the arcane's buff");
                let got = rep.frames.last().unwrap().stacks_of(0)[index];
                let expected = if arcane == "secondary_deadhead" { head && !tick_kill } else { true };
                assert_eq!(got, u16::from(expected), "{arcane} head={head} tick={tick_kill}");
            }
        }
    }
}

#[test]
fn m127_acuity_explosion_uses_critical_headshots_without_weakpoint_damage() {
    for (tier, expected) in [(1.0, 4681.0), (2.0, 7802.0)] {
        let mut p = measured("stug", 1, true, true);
        let base = crate::model::WeaponBase::from_data("stug", true, &["stug_overcharge_blast"]);
        let pool = crate::data::mods::pool_for_weapon("stug");
        let mods: Vec<_> = pool.iter().filter(|m| m.id == "hornet_strike" || m.id == "pistol_acuity").collect();
        let panel = crate::build::loadout::resolve(&base, &mods, crate::model::StackPolicy::Emergent);
        p.weakpoint_damage = panel.weakpoint_damage;
        p.weakpoint_crit_chance_relative = panel.weakpoint_crit_chance_relative;
        p.body_parts = BodyPart::humanoid();
        for part in &mut p.body_parts { part.aim_weight = if part.is_head { 1.0 } else { 0.0 }; }
        let b = p.blob.as_mut().unwrap();
        // Select each reported tier; the reading supplies the tier, not its frequency.
        b.explosion.crit_chance = tier;
        b.explosion.base_crit_chance = 0.0;
        let rec = record(&p, 127, 0.0, f64::INFINITY, 100_000, 0);
        let got: Vec<_> = rec.events().iter().filter_map(|e| match &e.kind {
            Kind::Damage(d) if d.origin == Origin::Blob => Some((d.radial, d.raw.round(), d.crit_tier)),
            _ => None,
        }).collect();
        assert_eq!(got, [(false, 973.0, 0), (false, 973.0, 0), (true, expected, tier as u32)]);
    }
}

#[test]
fn m127_attached_ticks_roll_their_own_critical_hits() {
    let mut p = measured("stug", 1, true, true);
    p.blob.as_mut().unwrap().hit.crit_chance = 0.5;
    let mut pairs = std::collections::BTreeSet::new();
    for seed in 0..100 {
        let rec = record(&p, seed, 0.0, f64::INFINITY, 100_000, 0);
        let ticks: Vec<_> = rec.events().iter().filter_map(|e| match &e.kind {
            Kind::Damage(d) if d.origin == Origin::Blob && !d.radial => Some(d.crit_tier),
            _ => None,
        }).collect();
        assert_eq!(ticks.len(), 2);
        pairs.insert((ticks[0], ticks[1]));
    }
    assert_eq!(pairs, [(0, 0), (0, 1), (1, 0), (1, 1)].into_iter().collect());
}

#[test]
fn m127_galvanized_shots_damage_payload_is_excluded_from_both_parts() {
    for (id, types, tick, blast) in [("stug", 2, 1550.0, 1913.0), ("stug_incarnon", 1, 1785.0, 2550.0)] {
        let base = crate::model::WeaponBase::from_data(id, true, &["stug_overcharge_blast"]);
        let pool = crate::data::mods::pool_for_weapon(id);
        let mods: Vec<_> = pool.iter().filter(|m| ["hornet_strike", "frostbite", "galvanized_shot"].contains(&m.id)).collect();
        let panel = crate::build::loadout::resolve(&base, &mods, crate::model::StackPolicy::Emergent);
        for stacked in [false, true] {
            let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(2.0), &ArcaneFx::none());
            p.magazine_size = 1.0;
            p.reload_seconds = 100.0;
            p.body_parts = mono_body(1.0);
            p.projectile_speed_mps = None;
            p.foe.base_health = 1e15;
            p.co_stack.as_mut().unwrap().initial_stacks = if stacked { 3 } else { 0 };
            let b = p.blob.as_mut().unwrap();
            b.hit.crit_chance = 0.0;
            b.explosion.crit_chance = 0.0;
            b.hit.status_chance = 0.0;
            b.explosion.status_chance = 0.0;
            if stacked {
                b.hit.forced_procs = crate::rules::damage::ForcedProcs::from_types(
                    [DamageType::Cold, DamageType::Magnetic].into_iter().take(types));
            }
            let got = numbers(&p);
            assert_eq!(got[0].2.round(), tick, "{id} stacked={stacked}");
            assert_eq!(got[1].2.round(), tick, "{id} stacked={stacked}");
            assert_eq!(got[2].2.round(), blast, "{id} stacked={stacked}");
        }
    }
}

#[test]
fn m127_cold_critical_damage_reaches_only_the_main_explosion() {
    let mut p = measured("stug", 1, false, false);
    let b = p.blob.as_mut().unwrap();
    b.hit.crit_chance = 1.0;
    b.explosion.crit_chance = 1.0;
    b.hit.forced_procs = crate::rules::damage::ForcedProcs::from_types([DamageType::Cold]);
    let cd = b.explosion.crit_damage;
    let got = numbers(&p);
    assert!((got[0].2 - 304.0 * cd).abs() < 1e-8);
    assert!((got[1].2 - got[0].2).abs() < 1e-8, "the attached tick excludes Cold");
    assert!((got[2].2 - 375.0 * (cd + 0.15)).abs() < 1e-8, "two Cold stacks reach the direct explosion");
}

#[test]
fn m127_puncture_critical_chance_is_gated_by_direct_hit_class() {
    let mut p = measured("stug", 1, false, false);
    p.blob.as_mut().unwrap().hit.forced_procs = crate::rules::damage::ForcedProcs::from_types([DamageType::Puncture]);
    let mut critical_explosions = 0;
    for seed in 0..400 {
        let rec = record(&p, seed, 0.0, f64::INFINITY, 100_000, 0);
        for e in rec.events() {
            if let Kind::Damage(d) = &e.kind {
                if d.origin != Origin::Blob { continue; }
                if d.radial { critical_explosions += u32::from(d.crit_tier > 0); }
                else { assert_eq!(d.crit_tier, 0, "attached ticks exclude Puncture"); }
            }
        }
    }
    assert!((20..65).contains(&critical_explosions), "two Puncture stacks add ten percentage points: {critical_explosions}");
}
