// SPDX-License-Identifier: AGPL-3.0-or-later
use wfsim_engine::{
    arena::Arena,
    build::loadout,
    data::{
        arcanes::ArcaneFx,
        mods,
        tenno::default_tenno,
        weapons::{ShotType, apply_deployment, spec as weapon_spec},
    },
    fight::{FightParams, run_once},
    formation::FoeSpec,
    model::{BounceSeek, Falloff, FalloffSpec, SpreadFan, StackPolicy, WeaponBase},
    rules::{
        geometry::ballistics::{
            bounce_seek_score, restitution_reflect, spawn_speed_mps, step_kinematic_flight,
            wave_dimensions,
        },
        rng::Rng,
        space::Vec2,
    },
};

fn resolve_with(id: &str, mod_ids: &[&str]) -> loadout::ResolvedPanel {
    let pool = mods::pool_for_weapon(id);
    let refs: Vec<_> = mod_ids
        .iter()
        .map(|mid| {
            pool.iter()
                .find(|m| m.id == *mid)
                .unwrap_or_else(|| panic!("mod {mid} missing from {id} pool"))
        })
        .collect();
    loadout::resolve(
        &WeaponBase::from_data(id, true, &[]),
        &refs,
        StackPolicy::Emergent,
    )
}

#[test]
fn track1_hitscan_falloff_radial_on_miss_penetration_ricochet_and_archgun_switch() {
    // Unicorn Suite 1: native 0x703bd2 / 0x8c8f45 distance falloff curve max(keep, 1 - t).
    let spec = FalloffSpec {
        start_m: 10.0,
        end_m: 20.0,
        reduction: 0.5,
        points: Vec::new(),
    };
    let unmodded = Falloff::from_spec(&spec, 1.0);
    for (d, want) in [
        (0.0, 1.0),
        (10.0, 1.0),
        (12.5, 0.75),
        (15.0, 0.5),
        (20.0, 0.5),
        (50.0, 0.5),
    ] {
        assert!(
            (unmodded.factor(d) - want).abs() < 1e-12,
            "10..20 min 0.5 at {d}m: {} != {want}",
            unmodded.factor(d)
        );
    }
    let plus_60 = Falloff::from_spec(&spec, 1.6);
    for (d, want) in [(16.0, 1.0), (20.0, 0.75), (24.0, 0.5), (32.0, 0.5)] {
        assert!((plus_60.factor(d) - want).abs() < 1e-12);
    }
    let neg_riven = Falloff::from_spec(&spec, -0.1);
    assert_eq!(
        neg_riven.factor(0.0),
        0.5,
        "non-positive proj speed collapses to keep"
    );

    // Redeemer Prime heavy trace: [10, 30], minDamage=1 on 80 base -> floor 1.0.
    let redeemer_p = WeaponBase::from_data("redeemer_prime_heavy", true, &[]);
    let rp_falloff = Falloff::from_spec(&redeemer_p.falloff.unwrap(), 1.0);
    for (d, want) in [
        (0.0, 80.0_f64),
        (10.0, 80.0),
        (12.0, 72.0),
        (20.0, 40.0),
        (29.5, 2.0),
        (29.75, 1.0),
        (30.0, 1.0),
        (300.0, 1.0),
    ] {
        assert!(
            (80.0 * rp_falloff.factor(d) - want).abs() < 1e-5,
            "Redeemer Prime at {d}m: {} != {want}",
            80.0 * rp_falloff.factor(d)
        );
    }

    // Zenith semi-auto InfinitePunctureDepth & Vulcax PunchthroughIgnoresAvatars + TraceRadius.
    let zenith = resolve_with("zenith", &[]);
    assert_eq!(
        zenith.punch_through_m,
        wfsim_engine::rules::space::INFINITE_PUNCH_THROUGH_M
    );
    let vulcax = WeaponBase::from_data("vulcax", true, &[]);
    assert!(vulcax.infinite_body_punch_through);
    assert!((vulcax.projectile_width_m - 0.24).abs() < 1e-9);
    assert!(resolve_with("vulcax", &["primed_shred"]).punch_through_m > 4.0);

    // RadialOnMiss punch-through gate (0x2d5a30): fire.mRadius == 0 accepts PT mods; > 0 suppresses.
    assert!(resolve_with("ambassador_charged", &["primed_shred"]).punch_through_m > 2.0);
    assert!(resolve_with("opticor_quick", &["primed_shred"]).punch_through_m > 3.0);
    assert!(resolve_with("zylok_incarnon", &["seeker"]).punch_through_m > 2.0);
    assert_eq!(
        resolve_with("battacor_charged", &["primed_shred"]).punch_through_m,
        0.0
    );
    assert_eq!(
        resolve_with("mausolon", &["sabot_rounds"]).punch_through_m,
        0.0
    );
    assert_eq!(
        resolve_with("mausolon_charged", &["sabot_rounds"]).punch_through_m,
        0.0
    );

    // All 9 hitscan auto-ricochet Incarnon forms (0x1ae090).
    let expected_ricochet = [
        ("bronco_incarnon", 0.5, 6.0),
        ("bronco_prime_incarnon", 1.0, 8.0),
        ("dual_toxocyst_incarnon", 1.0, 5.0),
        ("lato_incarnon", 1.0, 10.0),
        ("lato_prime_incarnon", 1.0, 12.0),
        ("lato_vandal_incarnon", 1.0, 10.0),
        ("sicarus_incarnon", 1.0, 6.0),
        ("sicarus_prime_incarnon", 1.0, 6.0),
        ("soma_incarnon", 0.2, 4.0),
    ];
    for (id, chance, range_m) in expected_ricochet {
        let w = WeaponBase::from_data(id, true, &[]);
        let rc = w.ricochet.unwrap_or_else(|| panic!("{id} missing ricochet"));
        assert!(
            (rc.chance - chance).abs() < 1e-6,
            "{id} chance {}",
            rc.chance
        );
        assert!(
            (rc.range_m - range_m).abs() < 1e-6,
            "{id} range {}",
            rc.range_m
        );
        assert_eq!(rc.bounces, 1);
    }
    let mut arena = Arena::training(5.0);
    arena.target_at = Vec2::new(0.0, 8.0);
    arena.others = vec![FoeSpec {
        id: "e2".into(),
        at: Vec2::new(3.0, 8.0),
        params: arena.target.clone(),
        body_parts: arena.body_parts.clone(),
    }];
    let lato_p = FightParams::from_panel(
        &resolve_with("lato_prime_incarnon", &[]),
        &arena,
        &ArcaneFx::none(),
    );
    let res = run_once(&lato_p, &mut Rng::new(42));
    assert!(
        res.taken.by_body().0[1] > 0.0,
        "ricochet must strike bystander"
    );

    // Archgun space-projectile -> ground-hitscan deployment switch.
    let cyngas_spec = weapon_spec("cyngas").expect("cyngas spec");
    assert_eq!(cyngas_spec.attack.shot_type, Some(ShotType::HitScan));
    let cyngas_ground = WeaponBase::from_data("cyngas", true, &[]);
    assert_eq!(cyngas_ground.projectile_speed_mps, None);
    let mut cyngas_space = cyngas_ground.clone();
    apply_deployment(&mut cyngas_space, "cyngas", "archwing");
    assert_eq!(
        cyngas_spec.deployments["archwing"].shot_type,
        Some(ShotType::Projectile)
    );
    assert_eq!(cyngas_space.projectile_speed_mps, Some(220.0));
    assert_eq!(cyngas_space.falloff.as_ref().unwrap().start_m, 1500.0);
}

#[test]
fn track2_kinematic_flight_wave_resize_arming_airburst_restitution_seek_and_clusters() {
    // 0xb94f40 spawn speed 0.1x floor and MaxLife-invariant travel range.
    assert_eq!(spawn_speed_mps(100.0, 0.05), 10.0);
    assert_eq!(spawn_speed_mps(60.0, 1.6), 96.0);
    let plasmor_base = resolve_with("arca_plasmor", &[]);
    let plasmor_fast = resolve_with("arca_plasmor", &["fatal_acceleration"]);
    assert!((plasmor_base.range_m - 30.0).abs() < 1e-6);
    assert!((plasmor_fast.range_m - 42.0).abs() < 1e-5); // Fatal Acceleration +40% -> 42.0m

    // 0x1585e40 StepKinematicFlight: exact IEEE-754 binary32 Unicorn control trajectories.
    let dt = 0.05_f32;
    let run_traj = |v0: f32, vmax: f32, g_enabled: bool, g_mult: f32| {
        let mut pos = [0.0_f32, 0.0, 0.0];
        let mut vel = [v0, 0.0, 0.0];
        let mut delay = 0.0_f32;
        let mut out = Vec::new();
        for tick in 1..=10 {
            let (p, v) = step_kinematic_flight(
                pos, vel, dt, 0.0, 0.0, 0.0, vmax, g_enabled, g_mult, &mut delay,
            );
            pos = p;
            vel = v;
            if matches!(tick, 1 | 5 | 10) {
                out.push((pos[0], pos[1], vel[0], vel[1]));
            }
        }
        out
    };
    let plasmor_1_6x = run_traj(96.0, 96.0, false, 1.0);
    assert!((plasmor_1_6x[2].0 - 47.999_996).abs() < 1e-5 && plasmor_1_6x[2].1 == 0.0);

    let dread_1x = run_traj(100.0, 100.0, true, 0.17);
    for (got, want) in dread_1x.into_iter().zip([
        (5.0_f32, -0.004_165_000_7, 100.0, -0.083_300_01),
        (25.0, -0.062_475_007, 100.0, -0.416_500_03),
        (49.999_676, -0.229_073_18, 99.997_185, -0.832_982),
    ]) {
        assert!((got.0 - want.0).abs() < 2e-5 && (got.1 - want.1).abs() < 2e-6);
        assert!((got.2 - want.2).abs() < 2e-5 && (got.3 - want.3).abs() < 2e-6);
    }
    let dread_1_6x = run_traj(160.0, 160.0, true, 0.17);
    let last = dread_1_6x[2];
    assert!((last.0 - 79.999_84).abs() < 2e-5 && (last.1 - (-0.229_074_43)).abs() < 2e-6);

    // 0xf18c90 WaveProjectile::SweepAndResize linear interpolation.
    let wr = WeaponBase::from_data("arca_plasmor", true, &[])
        .ballistics
        .unwrap()
        .wave_resize
        .unwrap();
    assert_eq!(wave_dimensions(wr, 0.0, 1.0), (0.2, 0.2));
    let (mid_r, mid_h) = wave_dimensions(wr, 0.25, 1.0);
    assert!((mid_r - 0.6).abs() < 1e-6 && (mid_h - 1.7).abs() < 1e-6);
    assert_eq!(wave_dimensions(wr, 1.0, 1.0), (1.0, 3.2));

    // ArmingDistance: Tonkor (7m) duds radial at 3m and explodes at 10m; Kuva Tonkor (0m) explodes at 3m.
    let radial_at = |id: &str, gap: f64| {
        let mut a = Arena::training(6.0);
        a.target_at = Vec2::new(0.0, gap);
        let p = FightParams::from_panel(&resolve_with(id, &[]), &a, &ArcaneFx::none());
        let r = run_once(&p, &mut Rng::new(7));
        (r.sources.direct, r.sources.radial)
    };
    let (tk_near_d, tk_near_r) = radial_at("tonkor", 3.0);
    let (_, tk_far_r) = radial_at("tonkor", 10.0);
    assert!(
        tk_near_d > 0.0 && tk_near_r == 0.0,
        "Tonkor < 7m must suppress radial"
    );
    assert!(tk_far_r > 0.0, "Tonkor >= 7m must detonate");
    assert!(
        radial_at("kuva_tonkor", 3.0).1 > 0.0,
        "Kuva Tonkor has 0m arming distance"
    );

    // Corinth airburst (MaxLife=0.24s * 80m/s = 19.2m, ExplodeOnImpact=0, ExplodeOnLifeExpired=1).
    let (ca_near_d, ca_near_r) = radial_at("corinth_airburst", 8.0);
    let (ca_mid_d, ca_mid_r) = radial_at("corinth_airburst", 22.0);
    assert!(
        ca_near_d > 0.0 && ca_near_r == 0.0,
        "early impact < 19.2m duds airburst"
    );
    assert!(
        ca_mid_d == 0.0 && ca_mid_r > 0.0,
        "19.2m mid-air expiry catches enemy at 22m"
    );

    // 0x13c58d9 decomposed restitution & 0x136bab0 bounce seek score.
    let v_def = restitution_reflect([20.0, -20.0, 0.0], [0.0, 1.0, 0.0], 0.25, 0.8, 1.0);
    assert!((v_def[0] - 16.0).abs() < 1e-5 && (v_def[1] - 5.0).abs() < 1e-5);
    let v_el = restitution_reflect([20.0, -20.0, 0.0], [0.0, 1.0, 0.0], 0.25, 0.8, 1.5);
    assert!((v_el[0] - 20.0).abs() < 1e-5 && (v_el[1] - 7.5).abs() < 1e-5);
    let v_tk = restitution_reflect([25.0, -15.0, 0.0], [0.0, 1.0, 0.0], 0.1, 0.8, 1.0);
    assert!((v_tk[0] - 20.0).abs() < 1e-5 && (v_tk[1] - 1.5).abs() < 1e-5);

    let cedo_seek = BounceSeek {
        cone_deg: 180.0,
        range_m: 20.0,
    };
    let miter_seek = BounceSeek {
        cone_deg: 180.0,
        range_m: 25.0,
    };
    let s_a = bounce_seek_score([0.0; 3], [1.0, 0.0, 0.0], [8.0, 1.0, 0.0], cedo_seek).unwrap();
    let s_b = bounce_seek_score([0.0; 3], [1.0, 0.0, 0.0], [12.0, 3.0, 0.0], cedo_seek).unwrap();
    assert!(s_a < s_b);
    assert!(
        bounce_seek_score([0.0; 3], [1.0, 0.0, 0.0], [-10.0, 5.0, 0.0], miter_seek).is_some()
    );
    assert!(
        bounce_seek_score([0.0; 3], [1.0, 0.0, 0.0], [30.0, 0.0, 0.0], miter_seek).is_none()
    );

    // 0x4976c0 ClusterBombProjectile: N * min(K, 20) submunitions under multishot.
    let mut arena = Arena::training(2.0);
    arena.target_at = Vec2::new(0.0, 10.0);
    for bp in &mut arena.body_parts {
        bp.multiplier = 1.0;
        bp.is_head = false;
        bp.is_weak_point = false;
    }
    let mut p1 =
        FightParams::from_panel(&resolve_with("kuva_bramma", &[]), &arena, &ArcaneFx::none());
    p1.multishot = 1.0;
    p1.base_crit_chance = 0.0;
    p1.critical_inputs = None;
    p1.status_chance = 0.0;
    if let Some(r) = p1.radial.as_mut() {
        r.crit_chance = 0.0;
        r.critical_inputs = None;
        r.status_chance = 0.0;
    }
    if let Some(c) = p1.cluster.as_mut() {
        c.contact.crit_chance = 0.0;
        c.contact.critical_inputs = None;
        c.contact.status_chance = 0.0;
        c.blast.crit_chance = 0.0;
        c.blast.critical_inputs = None;
        c.blast.status_chance = 0.0;
    }
    let mut p2 = p1.clone();
    p2.multishot = 2.0;
    let r1 = run_once(&p1, &mut Rng::new(11));
    let r2 = run_once(&p2, &mut Rng::new(11));
    assert!(
        (r2.effective_damage() - 2.0 * r1.effective_damage()).abs() < 1e-6,
        "2 multishot parents must spawn 2 * K cluster submunitions: {} vs 2 * {}",
        r2.effective_damage(),
        r1.effective_damage()
    );
}

#[test]
fn track3_melee_ranged_stropha_quassus_glaive_volatile_override_and_zenistar() {
    // Stropha native [8, 16], minDamage=80 on 700 + Whirlwind (+180% -> 2.8x) at 25m -> 618.75.
    let stropha_plain = resolve_with("stropha_heavy", &[]);
    let sf = stropha_plain.falloff.unwrap();
    assert!((700.0 * sf.factor(0.0) - 700.0).abs() < 1e-5);
    assert!((700.0 * sf.factor(8.0) - 700.0).abs() < 1e-5);
    assert!((700.0 * sf.factor(12.0) - 350.0).abs() < 1e-5);
    assert!((700.0 * sf.factor(16.0) - 80.0).abs() < 1e-4);
    assert!((700.0 * sf.factor(25.0) - 80.0).abs() < 1e-4);
    let glaive_pool = mods::pool_for_weapon("glaive_prime_charged_throw");
    let whirlwind = glaive_pool
        .iter()
        .find(|m| m.id == "whirlwind")
        .expect("whirlwind");
    let stropha_ww = loadout::resolve(
        &WeaponBase::from_data("stropha_heavy", true, &[]),
        &[whirlwind],
        StackPolicy::Emergent,
    );
    assert!((stropha_ww.range_m - 17.0 * 2.8).abs() < 1e-6);
    assert!((700.0 * stropha_ww.falloff.unwrap().factor(25.0) - 618.75).abs() < 1e-3);

    // Quassus Prime heavy fan spread & first-pellet pinpoint.
    let qp = resolve_with("quassus_prime_heavy", &[]);
    let rt = qp
        .spread_scaling
        .expect("quassus_prime_heavy spread_scaling")
        .runtime;
    assert!(rt.first_pellet_pinpoint);
    assert_eq!(
        rt.fan,
        Some(SpreadFan {
            start_deg: -9.0,
            end_deg: 9.0,
            vertical: false,
        })
    );
    assert!((76.0 * qp.falloff.unwrap().factor(20.0) - 1.0).abs() < 1e-4);

    // Glaive MaxReflections = 4 & Volatile Quick Return / Volatile Rebound SET 0 override.
    for gid in [
        "glaive_prime_charged_throw",
        "glaive_throw",
        "falcor_charged_throw",
        "cerata_charged_throw",
        "orvius_charged_throw",
        "pathocyst_charged_throw",
        "coda_pathocyst_charged_throw",
    ] {
        let g = resolve_with(gid, &[]);
        assert_eq!(g.thrown_bounce.unwrap().bounces, 4, "{gid} bounces");
    }
    let pt_only = resolve_with("glaive_prime_charged_throw", &["power_throw"]);
    assert!(pt_only.punch_through_m > 2.9);
    for mods in [
        ["power_throw", "volatile_quick_return"],
        ["volatile_quick_return", "power_throw"],
        ["power_throw", "volatile_rebound"],
        ["volatile_rebound", "power_throw"],
    ] {
        let p = resolve_with("glaive_prime_charged_throw", &mods);
        assert_eq!(
            p.punch_through_m, 0.0,
            "Volatile bounce mod must SET punch_through_m = 0 in order {mods:?}"
        );
        assert_eq!(
            p.consecutive_hit_damage,
            Some((1.0, 3, f64::INFINITY)),
            "Power Throw consecutive throw damage buff must remain active"
        );
        assert!(p.consecutive_hit_direct_only);
    }

    // Glaive heavy recall radial takes no Condition Overload, while direct throw does.
    let gp = loadout::resolve_for(
        &WeaponBase::from_data("glaive_prime_charged_throw", true, &[]),
        &[],
        StackPolicy::Emergent,
        default_tenno(),
    );
    assert!(!gp.radial.unwrap().takes_condition_overload);
}
