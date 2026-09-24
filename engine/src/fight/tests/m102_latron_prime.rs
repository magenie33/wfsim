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

fn with_double_tap(id: &str) -> crate::build::loadout::ResolvedPanel {
    // NO RIDDLED TARGET here: its multishot rolls extra pellets, and every
    // pellet is hits of its own, which would blur the count read below.
    let evos = ["latron_prime_evo1_incarnon_form"];
    let base = crate::model::WeaponBase::from_data(id, false, &evos);
    let pool = crate::data::mods::pool_for_weapon("latron_prime");
    let dt = pool.iter().find(|m| m.id == "double_tap").expect("double_tap on the Latron Prime");
    crate::build::loadout::resolve(&base, &[dt], crate::model::StackPolicy::Emergent)
}

fn head_only() -> Vec<BodyPart> {
    vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: true,
        is_weak_point: true,
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
    let b = crate::model::WeaponBase::from_data("latron_prime", false, &evo);
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

    let i = crate::model::WeaponBase::from_data("latron_prime_incarnon", false, &evo);
    assert_eq!(i.co_behavior, crate::model::CoBehavior::Independent);
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
    p.foe.base_health = 1e15;
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
    p.foe.base_health = 1e15;
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

/// PRIMARY COMPRESSION ON THE INCARNON FORM, measured end to end (M104):
/// 2097 on the collision and 8042 on the explosion, with a rank-2 card, a
/// +240% base-damage bracket, Galvanized Aptitude at three stacks over two
/// status types and Double Tap full.
///
/// FOUR RULES IN ONE PAIR OF NUMBERS, and the most useful is that the bonus
/// MULTIPLIES rather than joining the base-damage bucket: adding it reads 1074
/// on the collision where the game reads 2097. The others are the 0.8 share of
/// the radius, the rank ramp between the two published ranks, and M102's split
/// — Condition Overload on the collision alone, Double Tap on the explosion.
#[test]
fn m104_primary_compression_on_the_incarnon_form() {
    let evo = ["latron_prime_riddled_target", "latron_prime_evo1_incarnon_form"];
    let b = crate::model::WeaponBase::from_data("latron_prime_incarnon", false, &evo);
    let (collision, boom) =
        (b.base_vector.total(), b.radial.as_ref().expect("the explosion").base_vector.total());
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::build::loadout::resolve_for(
        &b,
        &refs,
        crate::model::StackPolicy::Emergent,
        crate::data::tenno::default_tenno(),
    );
    let c = panel.compression.expect("a compression row on this attack");
    // 4.0 m kept to a fifth: the arcane pays for 3.2 of them.
    assert!((c.radius_lost_m - 3.2).abs() < 1e-9, "radius lost {}", c.radius_lost_m);
    assert!(!c.adds, "the Latron row's class is Multiplies");
    let per_m = crate::data::arcanes::for_slot("primary", "primary_compression")
        .expect("the arcane")
        .fx(2, crate::model::StackPolicy::Emergent, &[], crate::data::tenno::default_tenno())
        .compression_damage_per_m;
    // Rank 2 of a 0..5 ramp from +50% to +100% a metre.
    assert!((per_m - 0.7).abs() < 1e-9, "per metre {per_m}");
    let comp = 1.0 + per_m * c.radius_lost_m;
    assert!((comp - 3.24).abs() < 1e-9, "+224% on this weapon at this rank: {comp}");
    // THE TWO READINGS. `base` is the build's base-damage bracket, `co` is
    // Galvanized Aptitude at 3 stacks x 2 types, `dt` Double Tap's full pile.
    let (base, co, dt) = (3.4, 1.0 + 0.4 * 3.0 * 2.0, 5.0);
    assert!((collision * base * comp * co - 2097.0).abs() < 1.0, "collision reads 2097");
    assert!((boom * base * comp * dt - 8042.0).abs() < 1.0, "explosion reads 8042");
    // …AND THE CLASS IS WHAT THE READING SETTLES: in the base-damage bucket the
    // same build reads 1074, which is not what the game shows.
    let added = collision * (base + per_m * c.radius_lost_m) * co;
    assert!((added - 1074.0).abs() < 1.0, "the Adds reading is {added:.0}, not 2097");
}

/// …AND THE RADIUS IT BOUGHT THAT WITH IS ACTUALLY GONE (M104).
///
/// The arcane's whole trade is damage for reach — *"x0.2 explosion radius"* —
/// and the fight kept the full sphere while the panel paid the bonus, which is
/// free damage in a crowd. Single-target it changes nothing, so the reading
/// above cannot see it: the blast detonates ON the aimed body either way.
#[test]
fn m104_compression_shrinks_the_explosion_it_is_paid_for() {
    let with_mods = |fx: crate::data::arcanes::ArcaneFx, weapon: &str, mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data(weapon, false, &[]);
        let pool = crate::data::mods::pool_for_weapon("latron_prime");
        let refs: Vec<&crate::model::ModDef> = mods
            .iter()
            .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("{m}")))
            .collect();
        let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let p = FightParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &fx);
        let r = p.radial.expect("the explosion");
        (r.radius_m, p.compression_multiplier)
    };
    let with_arcane = |fx: crate::data::arcanes::ArcaneFx, weapon: &str| with_mods(fx, weapon, &[]);
    let compression = |rank: u32| {
        crate::data::arcanes::for_slot("primary", "primary_compression")
            .expect("the arcane")
            .fx(rank, crate::model::StackPolicy::Emergent, &[], crate::data::tenno::default_tenno())
    };
    // NO CARD: the weapon's row is data and the arcane is a choice, so the
    // sphere is whole and the bonus is nothing.
    let (bare, bare_mult) = with_arcane(ArcaneFx::none(), "latron_prime_incarnon");
    assert!((bare - 4.0).abs() < 1e-9, "the published radius: {bare}");
    assert!((bare_mult - 1.0).abs() < 1e-9, "no card, no bonus");
    // WITH IT: a fifth of the sphere, and the bonus is still computed off the
    // FULL 4 m — the arcane pays for metres it is taking, not for what is left.
    let (kept, mult) = with_arcane(compression(2), "latron_prime_incarnon");
    assert!((kept - 0.8).abs() < 1e-9, "a fifth of 4 m: {kept}");
    assert!((mult - 3.24).abs() < 1e-9, "+224% off the 3.2 m it took: {mult}");
    // …AND IT IS THE MODDED RADIUS THAT IS TRADED, which is the whole reason
    // the catalog's Primed Firestorm column is 1.44x its base column: the mod
    // grows the sphere, the arcane takes four fifths of the bigger one and pays
    // for every metre of it. 4 x 1.44 x 0.2 = 1.152 m left, +322.6% bought.
    let (grown, grown_mult) =
        with_mods(compression(2), "latron_prime_incarnon", &["primed_firestorm"]);
    assert!((grown - 1.152).abs() < 1e-9, "4 x 1.44 x 0.2: {grown}");
    assert!((grown_mult - (1.0 + 0.7 * 4.608)).abs() < 1e-9, "+322.6%: {grown_mult}");
    // A ROW THE TABLE MARKS `doesnt_work` TAKES NOTHING AND SHRINKS NOTHING —
    // the Komorex's zoom explosion, at 0% effectiveness.
    let (komorex, komorex_mult) = with_arcane(compression(5), "komorex");
    assert!((komorex - 3.5).abs() < 1e-9, "the row pays nothing: {komorex}");
    assert!((komorex_mult - 1.0).abs() < 1e-9, "…and multiplies nothing");
}

/// …AND IT REACHES THE FIGHT: the smaller sphere touches fewer bodies.
///
/// The claim a formation exists to make. A body count, not a total: the
/// collision alone still lands, so damage would move for two reasons at once.
#[test]
fn m104_a_compressed_blast_reaches_fewer_bodies() {
    let touched = |fx: crate::data::arcanes::ArcaneFx| {
        let evo = ["latron_prime_evo1_incarnon_form"];
        let base = crate::model::WeaponBase::from_data("latron_prime_incarnon", false, &evo);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        // A LINE INSIDE THE FULL 4 m AND OUTSIDE THE COMPRESSED 0.8 m, which is
        // the only spacing that can tell the two spheres apart.
        arena.others = (1..=3)
            .map(|i| crate::formation::FoeSpec {
                id: String::new(),
                params: Foe::training_dummy(),
                body_parts: BodyPart::humanoid(),
                at: crate::rules::space::Vec2::new(
                    // 2, 3 and 4 m out: inside the whole sphere, outside the
                    // compressed one even with a body's own width added to it.
                    f64::from(i) + 1.0,
                    crate::rules::space::CONTACT_RANGE_M,
                ),
            })
            .collect();
        let p = FightParams::from_panel(&panel, &arena, &fx);
        run_once(&p, &mut Rng::new(0x5EED)).taken.touched()
    };
    let whole = touched(ArcaneFx::none());
    let compressed = touched(
        crate::data::arcanes::for_slot("primary", "primary_compression")
            .expect("the arcane")
            .fx(5, crate::model::StackPolicy::Emergent, &[], crate::data::tenno::default_tenno()),
    );
    assert_eq!(whole, 4, "the 4 m sphere reaches the whole line");
    assert_eq!(compressed, 1, "a fifth of it reaches the aimed body alone");
}
