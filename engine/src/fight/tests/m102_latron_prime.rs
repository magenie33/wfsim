//! LATRON PRIME, both forms, against the owner's readings (MEASUREMENTS M102):
//! Galvanized Aptitude and Riddled Target's +6 on the panel, Double Tap on
//! each form and across the swap, and Flensing Spikes counting bullets.
use super::*;
use crate::record::{Factor, Kind, Layer, Origin};

/// Double Tap's factor on one row — absent is 1.0.
fn dt(d: &crate::record::Damage) -> f64 {
    d.layers
        .iter()
        .filter_map(|l| match l {
            Layer::Mul { factor: Factor::DoubleTap, value, .. } => Some(*value),
            _ => None,
        })
        .product()
}

fn with_double_tap(id: &str) -> crate::loadout::ResolvedPanel {
    // NO RIDDLED TARGET here: its multishot rolls extra pellets, and every
    // pellet is hits of its own, which would blur the count read below.
    let evos = ["latron_prime_evo1_incarnon_form"];
    let base = crate::loadout::WeaponBase::from_data(id, false, &evos);
    let pool = crate::mods_data::pool_for_weapon("latron_prime");
    let dt = pool.iter().find(|m| m.id == "double_tap").expect("double_tap on the Latron Prime");
    crate::loadout::resolve(&base, &[dt], crate::loadout::StackPolicy::Emergent)
}

fn head_only() -> Vec<BodyPart> {
    vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: true,
        crit_bonus: false,
    }]
}

/// THE PANEL, both forms: +165% base damage, Riddled Target's +6,
/// Galvanized Aptitude at 40% a stack per type. Written `stacks-types`.
/// Base form: the CO term reads the weapon's own 90, not the 96 — so it
/// ADDS to the bracket. Incarnon: a free-standing factor on the collision
/// alone, and the explosion (146 with the +6) takes none.
#[test]
fn m102_galvanized_and_the_plus_six_on_both_forms() {
    let evo = ["latron_prime_riddled_target"];
    let b = crate::loadout::WeaponBase::from_data("latron_prime", false, &evo);
    let (panel, f) = (b.base_vector.total(), b.co_base_fraction());
    assert!((panel - 96.0).abs() < 1e-9, "panel {panel}");
    assert!((panel * f - 90.0).abs() < 1e-9, "CO reads {}", panel * f);
    for (stacks, types, measured) in
        [(0.0, 0.0, 254.0), (1.0, 2.0, 326.0), (2.0, 2.0, 398.0), (2.0, 3.0, 470.0)]
    {
        let got = panel * (1.0 + 1.65 + 0.4 * stacks * types * f);
        assert!((got - measured).abs() < 0.5, "base {stacks}-{types}: {got} vs {measured}");
        // DOUBLE TAP FULL is x5 on the whole hit, measured at 0-0, 1-3, 2-3.
    }
    for (hit, measured) in [(254.4_f64, 1272.0_f64), (362.4, 1812.0), (470.4, 2352.0)] {
        assert!((hit * 5.0 - measured).abs() < 0.5);
    }

    let i = crate::loadout::WeaponBase::from_data("latron_prime_incarnon", false, &evo);
    assert_eq!(i.co_behavior, crate::loadout::CoBehavior::Independent);
    let direct = i.base_vector.total();
    assert!((direct - 56.0).abs() < 1e-9, "collision {direct}");
    for (stacks, types, measured) in [(0.0, 0.0, 148.0), (1.0, 3.0, 326.0), (2.0, 3.0, 505.0)] {
        let got = direct * 2.65 * (1.0 + 0.4 * stacks * types);
        assert!((got - measured).abs() < 0.5, "incarnon {stacks}-{types}: {got} vs {measured}");
    }
    let r = i.radial.as_ref().expect("the explosion");
    assert!((r.base_vector.total() - 146.0).abs() < 1e-9, "explosion {}", r.base_vector.total());
    assert!(!r.takes_condition_overload, "the explosion reads 387 at 2-3 as at 0-0");
    assert!((146.0_f64 * 2.65 - 387.0).abs() < 0.5 && (146.0_f64 * 2.65 * 5.0 - 1935.0).abs() < 1.0);
}

/// ON THE INCARNON FORM DOUBLE TAP REACHES ONLY THE EXPLOSION, and each
/// landing projectile is two hits: the explosion reads +20%, +60%, +100%…
/// (hits 2, 4, 6 less one) up to the +400% cap, and the collision reads
/// x1 all along.
#[test]
fn m102_double_tap_on_the_incarnon_is_the_explosions_and_climbs_forty_a_shot() {
    let panel = with_double_tap("latron_prime_incarnon");
    assert!(panel.consecutive_hit_radial_only);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(4.0), &ArcaneFx::none());
    p.target.base_health = 1e15;
    let rec = record(&p, 3, 0.0, f64::INFINITY, 10_000, 0);
    let mut radial = Vec::new();
    for e in rec.events() {
        if let Kind::Damage(d) = &e.kind {
            if d.origin != Origin::Own || d.pellet.is_none() {
                continue;
            }
            if d.radial {
                radial.push(dt(d));
            } else {
                assert!((dt(d) - 1.0).abs() < 1e-9, "the collision took Double Tap: x{}", dt(d));
            }
        }
    }
    // One explosion per shot on one body; several rows (one per type) per
    // explosion carry the same factor, so collapse runs.
    radial.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert!(radial.len() >= 11, "{radial:?}");
    for (k, got) in radial.iter().enumerate().take(11) {
        let want = (1.0 + 0.2 * (2.0 * (k as f64 + 1.0) - 1.0)).min(5.0);
        assert!((got - want).abs() < 1e-9, "shot {}: x{got} vs x{want}", k + 1);
    }
}

/// THE PILE IS PER FORM AND FROZEN AT EACH SWAP. The first Incarnon window
/// starts from nothing whatever the base form had built; the second picks
/// up the pile the first left — capped, with the clock it had left when
/// the way out completed — rather than starting again.
#[test]
fn m102_double_tap_snapshots_each_form_at_the_swap() {
    let (pi, pb) = (with_double_tap("latron_prime_incarnon"), with_double_tap("latron_prime"));
    let mut p = FightParams::incarnon_cycle_from_panels(
        &pi, &pb, false, LockMode::Initial(0),
        &crate::arena::Arena::training(60.0), &ArcaneFx::none(),
    );
    p.body_parts = head_only();
    p.target.base_health = 1e15;
    let rec = record(&p, 5, 0.0, f64::INFINITY, 200_000, 0);
    let (mut transmuted, mut fresh, mut firsts) = (false, false, Vec::new());
    for e in rec.events() {
        match &e.kind {
            Kind::TransformEnd { transmuted: into } => {
                transmuted = *into;
                fresh = *into;
            }
            Kind::Damage(d) if transmuted && fresh && d.radial && d.origin == Origin::Own => {
                firsts.push(dt(d));
                fresh = false;
            }
            _ => {}
        }
    }
    assert!(firsts.len() >= 2, "two Incarnon windows: {firsts:?}");
    assert!((firsts[0] - 1.2).abs() < 1e-9, "the first window starts empty: {firsts:?}");
    assert!((firsts[1] - 5.0).abs() < 1e-9, "the second resumes the frozen pile: {firsts:?}");
}

/// FLENSING SPIKES COUNTS BULLETS, not stacks, and never gives back.
#[test]
fn m102_flensing_counts_bullets_and_keeps_the_armour() {
    // Five Puncture STACKS off two bullets are two bullets' worth.
    let mut d = DebuffState { weakened: vec![10.0; 5], flensed: 2, ..Default::default() };
    assert!((d.puncture_strip(0.2) - 0.4).abs() < 1e-9);
    // …and with every stack gone the strip stays.
    d.weakened.clear();
    assert!((d.puncture_strip(0.2) - 0.4).abs() < 1e-9);
    d.flensed = 7;
    assert!((d.puncture_strip(0.2) - 1.0).abs() < 1e-9, "five bullets take all of it");
}
