use super::*;

/// A grenade part whose crit changes nothing, so a throw's damage is exact
/// arithmetic — the fixture carries flat crit chance of its own (Secondary
/// Enervate), and a x1 multiplier is what keeps that out of the sum.
fn part(total: f64, radius_m: f64, falloff_reduction: f64) -> crate::build::loadout::ResolvedRadial {
    crate::build::loadout::ResolvedRadial {
        damage: DamageVector::new().with(DamageType::Corrosive, total),
        modified_base: total,
        crit_damage: 1.0,
        base_crit_damage: 1.0,
        radius_m,
        falloff_reduction,
        ..Default::default()
    }
}

fn grenade(count: u32, fan_deg: f64, blast: f64, radius_m: f64) -> crate::build::loadout::ResolvedReloadGrenade {
    crate::build::loadout::ResolvedReloadGrenade {
        count,
        fan_deg,
        contact: part(11.0, crate::rules::space::BODY_RADIUS_M, 0.0),
        blast: part(blast, radius_m, 0.5),
        last_round_factor: 1.0,
        crit_per_kill: None,
    }
}

/// The fixture reloading every three shots at a target that never dies, with
/// the weapon's own damage taken out so what is left is the throw.
fn thrower(g: crate::build::loadout::ResolvedReloadGrenade, gap_m: f64) -> FightParams {
    let mut p = FightParams {
        damage: DamageVector::new(),
        magazine_size: 3.0,
        reload_seconds: 1.0,
        target_at: crate::rules::space::Vec2::new(gap_m, 0.0),
        reload_grenade: Some(g),
        ..FightParams::default()
    };
    p.foe.base_health = 1e15;
    p
}

/// EVERY RELOAD THROWS THE MAGAZINE, and one grenade that lands on the reticle
/// deals its contact and its whole explosion to the body standing there.
#[test]
fn every_reload_throws_one_grenade_onto_the_reticle() {
    let p = thrower(grenade(1, 0.0, 1000.0, 7.0), crate::rules::space::CONTACT_RANGE_M);
    let r = run_once(&p, &mut crate::rules::rng::Rng::new(3));
    assert!(r.reloads >= 2, "the fixture reloads: {}", r.reloads);
    assert!(
        (r.sources.radial - f64::from(r.reloads) * 1011.0).abs() < 1e-6,
        "{} reloads, {} of grenade damage",
        r.reloads,
        r.sources.radial
    );
    // A THROW THE ENGAGEMENT ENDS INSIDE still lands: the third shot empties
    // the magazine at 2 s, the reload starts at 3 s and outlasts the clock, so
    // no shot follows to settle it.
    let cut = FightParams { duration_seconds: 3.5, ..p.clone() };
    let r = run_once(&cut, &mut crate::rules::rng::Rng::new(3));
    assert_eq!(r.reloads, 1);
    assert!((r.sources.radial - 1011.0).abs() < 1e-6, "{}", r.sources.radial);
    // …and a weapon that throws nothing deals none of it.
    let none = FightParams { reload_grenade: None, ..p };
    assert_eq!(run_once(&none, &mut crate::rules::rng::Rng::new(3)).sources.radial, 0.0);
}

/// THE CODA'S FAN, and it is the geometry that decides what the outer two are
/// worth. Each lands at the reticle's distance, 22.5 degrees off the aim, so it
/// comes down a CHORD away from the aimed body — and the blast falls off from
/// where it landed, "from 100% to 50% from central impact".
#[test]
fn the_codas_outer_grenades_reach_the_target_only_where_their_blast_does() {
    use crate::rules::space::BODY_RADIUS_M;
    let per_throw = |gap_m: f64| {
        let p = thrower(grenade(3, 45.0, 1000.0, 5.0), gap_m);
        let r = run_once(&p, &mut crate::rules::rng::Rng::new(3));
        r.sources.radial / f64::from(r.reloads)
    };
    // The muzzle is a body radius in front of the shooter, so the grenades fly
    // `gap - radius` and land that far out on a 22.5 degree offset.
    let outer = |gap_m: f64| {
        let reach = gap_m - BODY_RADIUS_M;
        let chord = 2.0 * reach * (22.5_f64 / 2.0).to_radians().sin();
        let into = (chord - BODY_RADIUS_M).max(0.0);
        if chord <= 5.0 + BODY_RADIUS_M { 1.0 - 0.5 * into / 5.0 } else { 0.0 }
    };
    let near = per_throw(8.0);
    let want = 11.0 + 1000.0 * (1.0 + 2.0 * outer(8.0));
    assert!(outer(8.0) > 0.5 && outer(8.0) < 1.0, "the fixture puts the outer two inside: {}", outer(8.0));
    assert!((near - want).abs() < 1e-6, "at 8 m: {near} against {want}");
    // Far enough out, the chord is wider than the blast and only the middle
    // grenade finds the body.
    assert_eq!(outer(30.0), 0.0);
    let far = per_throw(30.0);
    assert!((far - 1011.0).abs() < 1e-6, "at 30 m: {far}");
}

/// CRITICAL MUTATION'S PILE: every kill pays in, capped on the way in, and a
/// throw that strikes fewer than three enemies takes one step back — never
/// below nothing.
#[test]
fn critical_mutation_pays_per_kill_caps_and_charges_a_lonely_throw() {
    let mut m = crate::fight::grenades::Mutation::default();
    assert!((m.pay_in(4, 0.3, 3.0) - 1.2).abs() < 1e-9, "four kills");
    m.charge(1, 0.3);
    assert!((m.bonus - 0.9).abs() < 1e-9, "one enemy struck costs a step");
    m.charge(3, 0.3);
    assert!((m.bonus - 0.9).abs() < 1e-9, "three struck costs nothing");
    // Only the kills SINCE the last throw pay in…
    assert!((m.pay_in(4, 0.3, 3.0) - 0.9).abs() < 1e-9, "no new kills");
    // …and the cap holds however many there were.
    assert!((m.pay_in(40, 0.3, 3.0) - 3.0).abs() < 1e-9, "the 300% cap");
    for _ in 0..20 {
        m.charge(0, 0.3);
    }
    assert_eq!(m.bonus, 0.0, "never below nothing");
}

/// THE CARDS AS THE DATA STATES THEM: the throw each weapon resolves, the
/// innate -20% on its reload, and Critical Mutation reaching the grenade of
/// the two weapons it fits and no other.
#[test]
fn the_catabolyst_family_resolves_its_throw_and_its_augment() {
    let resolved = |id: &str, mods: &[&str]| {
        let pool = crate::data::mods::pool_for_build(id, &[]);
        let refs: Vec<&crate::model::ModDef> = mods
            .iter()
            .map(|m| pool.iter().find(|x| x.id == *m).unwrap_or_else(|| panic!("{m} not on {id}")))
            .collect();
        let base = crate::model::WeaponBase::from_data(id, true, &[]);
        crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
    };
    let one = resolved("catabolyst", &[]);
    let g = one.reload_grenade.expect("the Catabolyst throws");
    assert_eq!(g.count, 1);
    assert!((g.blast.damage.total() - 1997.0).abs() < 1e-9 && (g.blast.radius_m - 7.0).abs() < 1e-9);
    assert!((g.contact.damage.total() - 11.0).abs() < 1e-9 && (g.contact.crit_chance - 0.31).abs() < 1e-9);
    // "increasing the time it takes to reload to 2.125 seconds"
    assert!((one.reload_seconds - 2.125).abs() < 1e-9, "{}", one.reload_seconds);

    let three = resolved("coda_catabolyst", &["critical_mutation"]);
    let g = three.reload_grenade.expect("the Coda throws");
    assert_eq!((g.count, g.fan_deg), (3, 45.0));
    assert!((g.blast.damage.total() - 658.0).abs() < 1e-9 && (g.blast.radius_m - 5.0).abs() < 1e-9);
    assert_eq!(g.crit_per_kill, Some((0.3, 3.0, 0.3)));
    // The beam carries none of it.
    assert!((three.crit_chance - 0.11).abs() < 1e-9, "{}", three.crit_chance);
    // "Radius is not affected by Fulmination" — the beam's 0.7 m is.
    let fulminated = resolved("coda_catabolyst", &["fulmination"]);
    assert!((fulminated.reload_grenade.expect("throws").blast.radius_m - 5.0).abs() < 1e-9);

    assert!(
        !crate::data::mods::pool_for_build("ocucor", &[]).iter().any(|m| m.id == "critical_mutation"),
        "an augment fits the weapons it names"
    );
}


/// …AND THE PILE LANDS IN THE CRIT MODS' OWN BUCKETS, scaled by the GRENADE'S
/// base: "additive with mods such as Pistol Gambit" and "such as Target
/// Cracker". A grenade at 31% base under a +150% card and a full 300% pile is
/// 31% x (1 + 1.5 + 3.0), not 31% x 2.5 x 4.
#[test]
fn critical_mutation_joins_the_crit_buckets_on_the_grenades_own_base() {
    let modded = crate::build::loadout::ResolvedRadial {
        crit_chance: 0.31 * 2.5,
        base_crit_chance: 0.31,
        crit_damage: 2.9 * 1.6,
        base_crit_damage: 2.9,
        ..Default::default()
    };
    let at_cap = crate::fight::grenades::lingering_of(&modded, 3.0);
    assert!((at_cap.crit_chance - 0.31 * (1.0 + 1.5 + 3.0)).abs() < 1e-9, "{}", at_cap.crit_chance);
    assert!((at_cap.crit_damage - 2.9 * (1.0 + 0.6 + 3.0)).abs() < 1e-9, "{}", at_cap.crit_damage);
    let none = crate::fight::grenades::lingering_of(&modded, 0.0);
    assert!((none.crit_chance - modded.crit_chance).abs() < 1e-12);
}

/// A BLAST ON A NEIGHBOUR IS BOOKED TO THE NEIGHBOUR. The explosion reaches
/// every body inside it at that body's own falloff, and the ledger files each
/// share under the body that took it — not under the aimed one.
#[test]
fn a_neighbour_in_the_blast_takes_its_share_under_its_own_number() {
    use crate::rules::space::{Vec2, BODY_RADIUS_M, CONTACT_RANGE_M};
    let mut p = thrower(grenade(1, 0.0, 1000.0, 7.0), CONTACT_RANGE_M);
    let beside = Vec2::new(CONTACT_RANGE_M, 3.0);
    let mut foe = p.foe.clone();
    foe.base_health = 1e15;
    p.others = vec![crate::formation::FoeSpec {
        id: String::new(),
        params: foe,
        body_parts: BodyPart::humanoid(),
        at: beside,
    }];
    let r = run_once(&p, &mut crate::rules::rng::Rng::new(3));
    let throws = f64::from(r.reloads);
    let by_body = r.taken.by_body().0;
    // The grenade lands on the aimed body, so the neighbour stands 3 m from the
    // centre: 1 - 0.5 x (3 - radius) / 7 of the blast, and no contact.
    let share = 1.0 - 0.5 * (3.0 - BODY_RADIUS_M) / 7.0;
    assert!((by_body[0] - throws * 1011.0).abs() < 1e-6, "aimed: {by_body:?}");
    assert!((by_body[1] - throws * 1000.0 * share).abs() < 1e-6, "beside: {by_body:?}");
}
