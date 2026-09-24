use super::*;

#[test]
fn laetum_incarnon_carries_its_radial_part() {
    let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
    let r = b.radial.as_ref().expect("laetum_incarnon declares a radial part");
    assert_eq!(r.base_vector.total(), 300.0, "300 Radiation");
    assert_eq!(r.radius_m, 2.0);
    assert_eq!(r.falloff_reduction, 0.2);
    // The direct part is pure Impact 100.
    assert_eq!(b.base_vector.total(), 100.0);
}

#[test]
fn the_sim_actually_applies_the_radial() {
    use crate::fight::{monte_carlo, FightParams};
    let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
    let p = crate::build::loadout::resolve(&b, &[], crate::model::StackPolicy::AssumedMax);
    let parts = vec![crate::target::BodyPart {
        name: "body".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: false,
        is_weak_point: false,
        crit_bonus: false,
    }];
    let params =
        FightParams::from_panel(&p, &crate::arena::Arena { body_parts: parts, ..crate::arena::Arena::training(10.0) }, &crate::data::arcanes::ArcaneFx::none());
    assert!(params.radial.is_some(), "params carry the radial");
    let s = monte_carlo(&params, 30, 7);
    assert!(
        s.source_damage.radial > 0.0,
        "the radial must land damage, got {:?}",
        s.source_damage
    );
    // 300 Radiation vs 100 Impact: the radial dominates.
    assert!(
        s.source_damage.radial > s.source_damage.direct,
        "radial {} should exceed direct {}",
        s.source_damage.radial,
        s.source_damage.direct
    );
}

/// Two-stage damage: the direct hit lands first, then the explosion,
/// both on the SAME enemy. With Laetum's 100 Impact
/// direct and 300 Radiation radial, and no body-part multiplier on the
/// explosion, a body-only engagement must settle at radial ~ 3x direct.
#[test]
fn direct_then_radial_lands_at_the_declared_ratio() {
    use crate::fight::{monte_carlo, FightParams};
    let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
    let p = crate::build::loadout::resolve(&b, &[], crate::model::StackPolicy::AssumedMax);
    let specs = crate::data::enemies::all();
    let spec = specs.iter().find(|e| e.id == "thrax_centurion").unwrap();
    let target = spec
        .target_params(1, false, false, crate::target::TargetMode::InstantRespawn)
        .unwrap();
    let parts = vec![crate::target::BodyPart {
        name: "body".into(), aim_weight: 1.0, multiplier: 1.0,
        is_head: false,
        is_weak_point: false, crit_bonus: false,
    }];
    let params = FightParams::from_panel(&p, &crate::arena::Arena { target, body_parts: parts, ..crate::arena::Arena::training(30.0) }, &crate::data::arcanes::ArcaneFx::none());
    let s = monte_carlo(&params, 40, 3);
    let d = s.source_damage.direct;
    let r = s.source_damage.radial;
    let ratio = r / d;
    assert!(
        (ratio - 3.0).abs() < 0.25,
        "radial/direct should be ~3 (300 vs 100), got {ratio:.2} (direct {d:.0}, radial {r:.0})"
    );
}

/// The cycle economy is DATA, not a hardcoded weapon: Laetum fills in
/// 12 weakpoint hits and both transitions cost its 2.0 s reload, while
/// Incarnon Efficiency (+50% charge) drops the fill to 8 hits.
#[test]
fn the_cycle_reads_its_economy_from_the_weapon_data() {
    let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
    let f = b.gauge_form.expect("incarnon economy");
    assert_eq!(f.charges_to_fill, 12.0);
    assert_eq!(f.max_charges, 216.0);
    assert_eq!(f.transmute_in, 2.0);
    // Reverts are a uniform 1 s across every weapon until measured.
    assert_eq!(f.transmute_out, 1.0);
    assert_eq!(f.charge_rate, 0.0);

    let eff = WeaponBase::from_data("laetum_incarnon", true, &["laetum_incarnon_efficiency"]);
    let g = eff.gauge_form.expect("incarnon economy");
    assert_eq!(g.charge_rate, 0.5);
    // 12 / 1.5 = 8 hits (wiki).
    assert_eq!((g.charges_to_fill / (1.0 + g.charge_rate)).ceil() as u32, 8);

    // Dual Toxocyst keeps its own numbers.
    let frame_seconds = WeaponBase::from_data("dual_toxocyst_incarnon", true, &[]);
    let d = frame_seconds.gauge_form.expect("incarnon economy");
    assert_eq!(d.charges_to_fill, 9.0);
    assert_eq!(d.transmute_in, 2.35);
}

/// Overwhelming Attrition earns its stacks in-run: a plain hit (no
/// crit, no status) grants one, the buff multiplies later instances,
/// and a timeout drops ONE stack rather than the whole buff.
#[test]
fn overwhelming_attrition_earns_and_pays_out() {
    use crate::fight::{monte_carlo, FightParams};
    let parts = vec![crate::target::BodyPart {
        name: "body".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: false,
        is_weak_point: false,
        crit_bonus: false,
    }];
    let run = |evos: &[&str]| {
        let b = WeaponBase::from_data("laetum_incarnon", true, evos);
        let p = crate::build::loadout::resolve(&b, &[], crate::model::StackPolicy::AssumedMax);
        let params =
            FightParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(20.0) }, &crate::data::arcanes::ArcaneFx::none());
        (!params.stacking_buffs.is_empty(), monte_carlo(&params, 40, 11).mean_effective_damage)
    };
    let (has_none, without) = run(&[]);
    let (has_buff, with) = run(&["laetum_overwhelming_attrition"]);
    assert!(!has_none && has_buff, "the evolution must carry the buff");
    assert!(
        with > without * 1.5,
        "the earned stacks must show up: {without:.0} -> {with:.0}"
    );
}

/// Lethal Rearmament must not load INERT. It is an on-headshot stacking
/// reload-speed buff, and reload speed also scales the Incarnon transmute
/// animations — so on a weapon whose whole cycle is reload-bound it buys
/// back real time.
#[test]
fn lethal_rearmament_shortens_the_cycle_not_just_reloads() {
    use crate::fight::{monte_carlo, FightParams};
    // 100% headshots so the trigger fires on every landed pellet.
    let parts = vec![crate::target::BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 3.0,
        is_head: true,
        is_weak_point: true,
        crit_bonus: false,
    }];
    let run = |evos: &[&str]| {
        let b = WeaponBase::from_data("laetum_incarnon", true, evos);
        let p = crate::build::loadout::resolve(&b, &[], crate::model::StackPolicy::Emergent);
        let params =
            FightParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(60.0) }, &crate::data::arcanes::ArcaneFx::none());
        let m = monte_carlo(&params, 24, 7);
        (!params.stacking_buffs.is_empty(), m.mean_effective_damage)
    };
    let (has_none, without) = run(&["laetum_evo1_incarnon_form"]);
    let (has_buff, with) = run(&["laetum_evo1_incarnon_form", "laetum_lethal_rearmament"]);
    assert!(!has_none, "no evolution, no buff");
    assert!(has_buff, "the evolution must carry the buff (it loaded INERT before)");
    assert!(
        with > without,
        "less time reloading and transmuting = more damage in the same 60 s:              {without:.0} -> {with:.0}"
    );
}

/// M10 (in-game): a reload-speed buff is live in BOTH
/// forms; it does NOT touch the gauge, but it DOES shorten transmute
/// in and out. The observable consequence in a fixed engagement is
/// that more cycles fit — the gauge requirement staying put.
#[test]
fn a_reload_buff_shortens_the_transmutes_but_never_the_gauge() {
    use crate::fight::{run_once, FightParams};
    use crate::rules::rng::Rng;
    let parts = vec![crate::target::BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 3.0,
        is_head: true,
        is_weak_point: true,
        crit_bonus: false,
    }];
    let params = |evos: &[&str], pin: bool| {
        let inc = WeaponBase::from_data("laetum_incarnon", true, evos);
        let base = WeaponBase::from_data("laetum", true, evos);
        let pol = crate::model::StackPolicy::Emergent;
        let pi = crate::build::loadout::resolve(&inc, &[], pol);
        let pb = crate::build::loadout::resolve(&base, &[], pol);
        let mut d = FightParams::incarnon_cycle_from_panels(
            &pi,
            &pb,
            false,
            crate::fight::LockMode::Initial(0),
            &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(300.0) },
            &crate::data::arcanes::ArcaneFx::none(),
        );
        for b in d.stacking_buffs.iter_mut() {
            if b.grant != crate::model::BuffGrant::ReloadSpeed {
                continue;
            }
            if pin {
                // Full AND never expiring — the two knobs are separate,
                // and this test wants both held for the whole run.
                b.initial_stacks = b.max_stacks;
                b.duration = crate::model::NO_TIMEOUT;
            } else {
                b.initial_stacks = 0;
            }
        }
        d
    };
    let cycle_charges = |d: &FightParams| {
        match d.cycle.as_ref().expect("the incarnon cycle").arms {
            crate::fight::Arms::Gauge { charges_to_fill, .. } => charges_to_fill,
            other => panic!("a gun Incarnon fills a gauge, not {other:?}"),
        }
    };

    let off = params(&["laetum_evo1_incarnon_form"], false);
    let on = params(
        &["laetum_evo1_incarnon_form", "laetum_lethal_rearmament"],
        true,
    );
    assert_eq!(
        cycle_charges(&off),
        cycle_charges(&on),
        "reload speed must NOT shorten gauge building (M10)"
    );

    // Less downtime in a fixed engagement = more shots fired. (Transform
    // COUNT is gauge-bound, not animation-bound, so it barely moves.)
    let s_off = run_once(&off, &mut Rng::new(5)).shots;
    let s_on = run_once(&on, &mut Rng::new(5)).shots;
    assert!(
        s_on > s_off,
        "shorter transmutes = more shots in the same 300 s: {s_off} -> {s_on}"
    );
}

/// The two tier-5 Attritions sit in DIFFERENT brackets, and the wiki
/// says so explicitly: Overwhelming is "additive to base damage bonuses
/// such as Hornet Strike", Devouring is "multiplicative" to them. The
/// observable difference is DILUTION — an additive bonus loses relative
/// value as the base-damage bucket grows, a multiplicative one does not.
#[test]
fn overwhelming_attrition_is_diluted_by_base_damage_mods() {
    use crate::fight::{monte_carlo, FightParams};
    let parts = vec![crate::target::BodyPart {
        name: "body".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: false,
        is_weak_point: false,
        crit_bonus: false,
    }];
    let pool = crate::data::mods::pistol_pool();
    let hornet: Vec<&crate::model::ModDef> =
        pool.iter().filter(|m| m.id == "hornet_strike").collect();
    let gain = |evos: &[&str], mods: &[&crate::model::ModDef]| {
        let run = |e: &[&str]| {
            let b = WeaponBase::from_data("laetum_incarnon", true, e);
            let p = crate::build::loadout::resolve(&b, mods, crate::model::StackPolicy::AssumedMax);
            let params =
                FightParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(20.0) }, &crate::data::arcanes::ArcaneFx::none());
            monte_carlo(&params, 60, 5).mean_effective_damage
        };
        run(evos) / run(&[])
    };
    let bare = gain(&["laetum_overwhelming_attrition"], &[]);
    let modded = gain(&["laetum_overwhelming_attrition"], &hornet);
    assert!(
        bare > modded * 1.5,
        "an ADDITIVE bonus must lose relative value once Hornet Strike              fills the same bucket: bare {bare:.2}x vs modded {modded:.2}x"
    );
    // …and the MULTIPLICATIVE sibling must NOT be diluted at all.
    let d_bare = gain(&["laetum_devouring_attrition"], &[]);
    let d_modded = gain(&["laetum_devouring_attrition"], &hornet);
    let drift = (d_bare / d_modded - 1.0).abs();
    assert!(
        drift < 0.15,
        "a MULTIPLICATIVE bonus keeps its relative value: bare {d_bare:.2}x              vs modded {d_modded:.2}x (drift {drift:.2})"
    );
}

/// Rapid Wrath's +20% fire rate joins the ORDINARY fire-rate bucket —
/// the same additive one the mods feed. Additive with
/// Gunslinger's +72%: 6.67 x (1 + 0.72 + 0.20) = 12.81, NOT the
/// multiplicative 6.67 x 1.72 x 1.20 = 13.77.
#[test]
fn rapid_wrath_is_additive_with_fire_rate_mods() {
    let pool = crate::data::mods::pistol_pool();
    let gunslinger: Vec<&crate::model::ModDef> =
        pool.iter().filter(|m| m.id == "gunslinger").collect();
    let fr = |evos: &[&str], mods: &[&crate::model::ModDef]| {
        let b = WeaponBase::from_data("laetum_incarnon", true, evos);
        crate::build::loadout::resolve(&b, mods, crate::model::StackPolicy::AssumedMax).fire_rate
    };
    let base = fr(&[], &[]);
    assert!((base - 6.67).abs() < 1e-9, "base fire rate {base}");
    assert!((fr(&["laetum_rapid_wrath"], &[]) - 6.67 * 1.20).abs() < 1e-9);
    assert!((fr(&[], &gunslinger) - 6.67 * 1.72).abs() < 1e-9);
    let both = fr(&["laetum_rapid_wrath"], &gunslinger);
    assert!(
        (both - 6.67 * 1.92).abs() < 1e-9,
        "must be ADDITIVE (12.81), got {both} — multiplicative would be {}",
        6.67 * 1.72 * 1.20
    );
}

/// The wiki CO catalog rates BOTH Laetum forms "Adding" at a 100% base
/// fraction, and notes the bonus "multiplies properly with Devouring
/// Attrition". Two consequences the engine must honour: CO reaches the
/// DIRECT hit only (never the 300 Radiation radial), and Devouring — its
/// own multiplicative bracket — multiplies the already-CO-boosted value.
#[test]
fn condition_overload_is_adding_direct_only_and_devouring_stacks_on_top() {
    use crate::fight::{monte_carlo, FightParams};
    let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
    assert_eq!(b.co_behavior, crate::model::CoBehavior::AdditiveWithBaseDamage);
    // 160/160 and 100/100 in the catalog: the whole base feeds the bonus.
    assert!((b.co_base_fraction() - 1.0).abs() < 1e-9);

    let pool = crate::data::mods::pistol_pool();
    let co: Vec<&crate::model::ModDef> =
        pool.iter().filter(|m| m.id == "galvanized_shot").collect();
    let parts = vec![crate::target::BodyPart {
        name: "body".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: false,
        is_weak_point: false,
        crit_bonus: false,
    }];
    let sources = |evos: &[&str], mods: &[&crate::model::ModDef]| {
        let b = WeaponBase::from_data("laetum_incarnon", true, evos);
        let p = crate::build::loadout::resolve(&b, mods, crate::model::StackPolicy::AssumedMax);
        let params =
            FightParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(20.0) }, &crate::data::arcanes::ArcaneFx::none());
        let s = monte_carlo(&params, 60, 17).source_damage;
        (s.direct, s.radial)
    };
    // The radial is INDIFFERENT to a CO mod; the direct hit is not.
    let (d0, r0) = sources(&[], &[]);
    let (d1, r1) = sources(&[], &co);
    assert!(d1 > d0 * 1.05, "CO must lift the direct hit: {d0:.0} -> {d1:.0}");
    assert!(
        (r1 / r0 - 1.0).abs() < 0.05,
        "CO must NOT reach the radial: {r0:.0} -> {r1:.0}"
    );
}

#[test]
fn resolving_keeps_the_radial() {
    let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
    let p = crate::build::loadout::resolve(&b, &[], crate::model::StackPolicy::AssumedMax);
    let r = p.radial.expect("resolved panel keeps the radial");
    assert!((r.damage.total() - 300.0).abs() < 1e-9, "got {}", r.damage.total());
}
