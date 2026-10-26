use super::*;

/// One stack's explosion, 75 Corrosive at 0.3 m, with a x1 crit so a pile's
/// number is exact arithmetic.
fn blob(cap: u32) -> crate::build::loadout::ResolvedBlob {
    crate::build::loadout::ResolvedBlob {
        explosion_direct_events: false,
        hit: crate::build::loadout::ResolvedRadial {
            damage: DamageVector::new().with(DamageType::Corrosive, 4.0),
            modified_base: 4.0,
            crit_damage: 1.0,
            base_crit_damage: 1.0,
            radius_m: 2.0,
            falloff_reduction: 1.0,
            ..Default::default()
        },
        flat_base_damage: 0.0,
        hit_base_per_stack: 4.0,
        explosion_base_per_stack: 75.0,
        cap,
        lifespan_seconds: 1.5,
        explosion: crate::build::loadout::ResolvedRadial {
            damage: DamageVector::new().with(DamageType::Corrosive, 75.0),
            modified_base: 75.0,
            crit_damage: 1.0,
            base_crit_damage: 1.0,
            radius_m: 0.3,
            falloff_reduction: 0.7,
            ..Default::default()
        },
        radius_at_cap_m: 2.8,
    }
}

/// A Stug-shaped gun at 4 shots a second against a target that never dies.
fn stug(cap: u32, multishot: f64, magazine: f64) -> FightParams {
    let mut p = FightParams {
        damage: DamageVector::new().with(DamageType::Corrosive, 1.0),
        fire_rate: 4.0,
        multishot,
        magazine_size: magazine,
        reload_seconds: 2.0,
        duration_seconds: 10.0,
        blob: Some(blob(cap)),
        body_parts: mono_body(1.0),
        ..no_status()
    };
    p.foe.base_health = 1e15;
    p
}

/// Every pile that went off: when, and its number before the target's
/// defences.
fn pops(p: &FightParams) -> Vec<(f64, f64)> {
    record(p, 7, 0.0, f64::INFINITY, 1_000_000, 0)
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) if d.origin == crate::record::Origin::Blob && d.radial => Some((e.t, d.base)),
            _ => None,
        })
        .collect()
}

/// A PILE THAT REACHES THE CAP GOES OFF AT ONCE, ten stacks of 75: one stack a
/// shot at four shots a second fills it on the tenth, at 2.25 s.
#[test]
fn a_pile_that_reaches_the_cap_goes_off_at_once() {
    let got = pops(&stug(10, 1.0, 100.0));
    assert!(!got.is_empty(), "no pile went off");
    assert!((got[0].0 - 2.25).abs() < 1e-6, "first pile at {}", got[0].0);
    assert!(got.iter().all(|&(_, v)| (v - 750.0).abs() < 1e-6), "{got:?}");
    // …and the next pile starts empty: ten more shots, 2.5 s later.
    assert!((got[1].0 - 4.75).abs() < 1e-6, "second pile at {}", got[1].0);
}

/// A SHOT THAT CROSSES THE CAP pays the cap, at once: four stacks a shot is 4,
/// 8, then 12 — which goes off on the third shot at ten stacks.
#[test]
fn a_shot_that_crosses_the_cap_pays_the_cap_at_once() {
    let got = pops(&stug(10, 4.0, 100.0));
    assert!((got[0].0 - 0.5).abs() < 1e-6, "first pile at {}", got[0].0);
    assert!((got[0].1 - 750.0).abs() < 1e-6, "pays {}", got[0].1);
}

/// A SHOT ALREADY OVER THE CAP NEVER DETONATES A PILE (M109): at multishot 12
/// every shot refreshes a pile held at ten, so nothing goes off while the
/// magazine lasts — and the pile goes off 1.5 s after the last shot, inside
/// the 2 s reload, at ten stacks however many were laid.
#[test]
fn a_shot_over_the_cap_waits_out_its_lifespan() {
    let got = pops(&stug(10, 12.0, 20.0));
    let last_shot = 19.0 * 0.25;
    assert!(!got.is_empty(), "the pile never went off");
    assert!(got.iter().all(|&(t, _)| t > last_shot + 1e-9 || t < 0.0), "a pile went off while firing: {got:?}");
    assert!((got[0].0 - (last_shot + 1.5)).abs() < 1e-6, "first pile at {}", got[0].0);
    assert!((got[0].1 - 750.0).abs() < 1e-6, "pays {}", got[0].1);
}

/// A LONE SHOT'S PILE GOES OFF WHEN ITS LIFESPAN RUNS OUT, at one stack.
#[test]
fn a_lone_shot_goes_off_after_its_lifespan() {
    let mut p = stug(10, 1.0, 100.0);
    p.fire_rate = 0.2;
    p.duration_seconds = 4.0;
    let got = pops(&p);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!((got[0].0 - 1.5).abs() < 1e-6, "at {}", got[0].0);
    assert!((got[0].1 - 75.0).abs() < 1e-6, "pays {}", got[0].1);
}

/// THE RADIUS GROWS LINEARLY from one stack to the cap — "0.3 - 2.8 meter
/// radius, scaling with the number of stacks".
#[test]
fn the_radius_grows_from_one_stack_to_the_cap() {
    let b = blob(10);
    assert!((b.radius_at(1) - 0.3).abs() < 1e-12);
    assert!((b.radius_at(10) - 2.8).abs() < 1e-12);
    assert!((b.radius_at(4) - (0.3 + 2.5 * 3.0 / 9.0)).abs() < 1e-12);
}

/// THE STUG'S DATA CARRIES THE ECONOMY, both forms: ten stacks of 75 on the
/// base, five of 200 on the Incarnon, and neither form keeps an explosion on
/// every shot.
#[test]
fn the_stug_carries_its_blob_on_both_forms() {
    for (id, cap, per_stack) in [("stug", 10, 75.0), ("stug_incarnon", 5, 200.0)] {
        let w = crate::data::weapons::spec(id).unwrap_or_else(|| panic!("{id}"));
        let b = w.attack.blob.as_ref().unwrap_or_else(|| panic!("{id} carries no blob"));
        assert_eq!(b.cap, cap, "{id}");
        assert!((b.explosion.damage.values().sum::<f64>() - per_stack).abs() < 1e-9, "{id}");
        assert!(w.attack.radial.is_none(), "{id} still explodes on every shot");
    }
}
