use super::*;

/// A DEBILITATE DoT EATS ATTRITION TWICE — a BUG of DE's, measured, and
/// derived in MEASUREMENTS M37. Three claims:
///
///   1. an ORDINARY status DoT carries the applying hit's roll and only
///      that — exactly 21x;
///   2. a SPLIT's DoT carries a second one — 21x21 = 441x;
///   3. and it lands EVEN ON A CRITTING HIT, where the parent's own roll is
///      worth nothing. The zero instance has no crit of its own, so the
///      perk's condition holds whatever the parent did — confirmed in game
///      on a second weapon, which is what makes the split permanently
///      non-critical rather than a zero that rolls crit.
///
/// The roll is forced to 1.0 throughout, because the question is which
/// layers apply rather than the odds, and health is enormous so nothing
/// dies — a 21x direct hit would end the fight and drop the DoT totals
/// while every tick grew.
#[test]
fn the_debilitate_dot_carries_two_attrition_layers() {
    let base = crate::loadout::WeaponBase::from_data("felarx", true, &[]);
    let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::AssumedMax);
    let arena = crate::arena::Arena::training(30.0);
    // AVERAGED OVER 200 RUNS. Turning the perk on consumes an extra RNG
    // draw per instance, so the two fights diverge shot for shot and a
    // single pair of runs compares two different fights — the ratio is only
    // a statement about the multiplier in the aggregate.
    let dots = |attrition: bool, debilitate: f64, crit: bool| {
        (0..200u64)
            .map(|seed| {
                let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
                p.target.base_health = 1e15;
                p.crit_tier_upgrade_chance = 0.0;
                p.super_crit_on_status = None;
                p.base_crit_chance = if crit { 1.0 } else { 0.0 };
                p.unmodded_crit_chance = 0.0;
                // PUNCTURE IS IMMUNE, so the crit rate is the one this test
                // sets. Weakened is a flat crit-chance buff on the victim and
                // a saturating status build keeps it up — a critical instance
                // is not eligible for Attrition, so leaving it in would mix
                // untouched instances into every ratio below.
                p.target.status_immunities = vec![DamageType::Puncture];
                p.status_chance = 4.0;
                p.arcane.debilitate_chance = debilitate;
                if debilitate > 0.0 {
                    // PURE CORROSIVE, so every DoT in the run is a SPLIT's.
                    // The arcane splits a COMBINED element into a component,
                    // and the Felarx's own vector is IPS — nothing to split,
                    // which is why claim 1's fight sees no splits at all.
                    // With no Toxin of its own the weapon cannot apply a
                    // Toxin DoT by any other route, so the whole of
                    // `dot_damage` here went through the split.
                    let total = p.damage.total();
                    p.damage = crate::damage::DamageVector::new()
                        .with(DamageType::Corrosive, total);
                }
                p.noncrit_bonus = attrition.then_some((1.0, 20.0));
                let mut rng = crate::rng::Rng::new(seed);
                run_once(&p, &mut rng).meter.dot()
            })
            .sum::<f64>()
    };
    // 1. ordinary DoTs: one layer, and the whole of it.
    let plain = dots(false, 0.0, false);
    assert!(plain > 0.0);
    let one_layer = dots(true, 0.0, false) / plain;
    assert!(
        (one_layer - 21.0).abs() < 1.0,
        "an ordinary DoT takes the hit's roll and nothing else: x{one_layer:.2}"
    );
    // 2. a split's DoT: two layers, 21x21 = 441x — the owner's own number.
    let plain_split = dots(false, 1.0, false);
    assert!(plain_split > 0.0, "the Corrosive fight has to produce split DoTs");
    let two_layers = dots(true, 1.0, false) / plain_split;
    assert!(
        (two_layers - 441.0).abs() < 25.0,
        "the split's DoT takes the hit's roll AND its own: x{two_layers:.1},              measured 441 (one layer is x{one_layer:.2})"
    );
    // 2b. AND THE SPLIT'S ROLL IS ITS OWN — the half the forced-chance
    // runs above cannot see.
    //
    //     At the perk's real 50% the two readings are far apart, and a mean
    //     tells them apart on its own:
    //
    //       two INDEPENDENT rolls   E[hit] x E[split] = 11 x 11 = 121
    //       the split COPYING it    E[hit^2] = .5x441 + .5x1    = 221
    //
    //     Nothing else in the fight moves, so the ratio against a run with
    //     the perk off is that expectation.
    // THE SAME 200 RUNS the baseline used. A ratio against a different
    // number of runs is a ratio against a different fight and reads as a
    // mechanic: at half the runs it prints x243, which is 121 x 2.
    let half = |seed: u64| {
        (0..200u64)
            .map(|k| {
                let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
                p.target.base_health = 1e15;
                p.crit_tier_upgrade_chance = 0.0;
                p.super_crit_on_status = None;
                p.base_crit_chance = 0.0;
                p.unmodded_crit_chance = 0.0;
                p.target.status_immunities = vec![DamageType::Puncture];
                p.status_chance = 4.0;
                p.arcane.debilitate_chance = 1.0;
                let total = p.damage.total();
                p.damage = crate::damage::DamageVector::new()
                    .with(DamageType::Corrosive, total);
                p.noncrit_bonus = Some((0.5, 20.0));
                let mut rng = crate::rng::Rng::new(seed + k);
                run_once(&p, &mut rng).meter.dot()
            })
            .sum::<f64>()
    };
    let coin = half(1000) / plain_split;
    assert!(
        (coin - 121.0).abs() < 25.0,
        "at a real 50% the split's own roll is independent: x{coin:.0}              (121 = two coins, 221 = the split copying the hit)"
    );

    // 3. every hit crits, so the HIT's roll is worth nothing — and the
    //    split's is worth its full 21x anyway.
    let crit_split = dots(false, 1.0, true);
    assert!(crit_split > 0.0);
    let on_crit = dots(true, 1.0, true) / crit_split;
    assert!(
        (on_crit - 21.0).abs() < 1.0,
        "the zero instance has no crit to disqualify it: x{on_crit:.2} on a              fight that crits every shot (x441 when nothing crits)"
    );
}

/// A CRIT TAKES THE HIT'S COIN AWAY AND KEEPS ITS OWN MULTIPLIER — so on
/// the Debilitate DoT, and only there, critting can be worth LESS than not.
///
/// Two true halves pulling opposite ways: a critical hit is not eligible
/// for Devouring Attrition, so the HIT's coin is gone; and the split
/// instance never crits, so ITS coin is always live while the DoT still
/// inherits the hit's crit multiplier and body part. That makes the
/// comparison arithmetic:
///
///     not critting   E = 11 x 11         = 121
///     critting       E = crit_multiplier x 11
///
/// **They cross at a crit multiplier of 11.** Measured: x121 with no crits
/// and x11 with them at ANY multiplier — the same x11 whether the crit is
/// 3x or 21x, which shows it is the hit's coin that went missing rather
/// than a scaled version of it. The DoT bucket alone; the direct damage
/// still wants crits.
#[test]
fn a_crit_costs_the_split_a_coin_and_pays_it_back_in_multiplier() {
    let base = crate::loadout::WeaponBase::from_data("felarx", true, &[]);
    let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::AssumedMax);
    let arena = crate::arena::Arena::training(30.0);
    let dots = |attrition: bool, cc: f64, cd: f64| {
        (0..200u64)
            .map(|seed| {
                let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
                p.target.base_health = 1e15;
                p.crit_tier_upgrade_chance = 0.0;
                p.super_crit_on_status = None;
                p.base_crit_chance = cc;
                p.unmodded_crit_chance = 0.0;
                p.crit_multiplier = cd;
                p.target.status_immunities = vec![DamageType::Puncture];
                p.status_chance = 4.0;
                p.arcane.debilitate_chance = 1.0;
                let tot = p.damage.total();
                p.damage = crate::damage::DamageVector::new()
                    .with(DamageType::Corrosive, tot);
                p.noncrit_bonus = attrition.then_some((0.5, 20.0));
                let mut rng = crate::rng::Rng::new(seed);
                run_once(&p, &mut rng).meter.dot()
            })
            .sum::<f64>()
    };
    let plain = dots(true, 0.0, 1.0) / dots(false, 0.0, 1.0);
    assert!((plain - 121.0).abs() < 12.0, "no crit: x{plain:.0}");
    for cd in [3.0, 11.0, 21.0] {
        let crit = dots(true, 1.0, cd) / dots(false, 1.0, cd);
        assert!((crit - 11.0).abs() < 1.5, "crit x{cd}: attrition worth x{crit:.1}");
    }
    // …AND THE CROSSOVER IS AT ELEVEN. Below it the DoT is bigger WITHOUT
    // the crit, which is the counterintuitive half and the reason this test
    // exists at all.
    let none = dots(true, 0.0, 1.0);
    assert!(dots(true, 1.0, 3.0) < none, "a 3x crit build should lose here");
    assert!(dots(true, 1.0, 21.0) > none, "and a 21x one should win");
}
