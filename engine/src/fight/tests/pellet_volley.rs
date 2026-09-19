use super::*;

/// THE VIRAL AVERAGE IS DAMAGE-WEIGHTED AND WHOLE-FIGHT — and its
/// denominator is what a booking site can silently drop.
///
/// Against the training dummy every point lands on health, so the
/// denominator must equal the meter EXACTLY: a damage site that spends
/// health and forgets to declare it would leave the average reading high
/// and nothing else disagreeing.
#[test]
fn the_viral_average_is_weighted_by_the_health_damage_it_multiplied() {
    let soma = |mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("soma_prime", true, &[]);
        let pool = crate::mods_data::pool_for_weapon("soma_prime");
        let refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        assert_eq!(refs.len(), mods.len(), "every named mod is in this weapon's pool");
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        monte_carlo(
            &FightParams::from_panel(
                &panel,
                &crate::arena::Arena::training(60.0),
                &crate::arcanes_data::ArcaneFx::none(),
            ),
            20,
            5,
        )
    };

    // Toxin and Cold COMBINE into Viral; either alone is a different status
    // and neither is this one.
    let viral = soma(&["malignant_force", "primed_cryo_rounds", "vital_sense"]);
    let none = soma(&["vital_sense"]);

    for (what, s) in [("viral", &viral), ("no viral", &none)] {
        assert!(s.mean_effective_damage > 0.0, "{what}: the fixture fires");
        assert!(
            (s.mean_health_damage - s.mean_effective_damage).abs() < 1e-6,
            "{what}: the dummy is health and nothing else, so every point booked as                  damage is booked as health damage — {} against {}",
            s.mean_health_damage,
            s.mean_effective_damage
        );
    }

    assert_eq!(
        none.mean_virus_stacks, 0.0,
        "a build that cannot apply Viral averages no stacks at all"
    );
    assert!(
        none.mean_procs > 0.0,
        "…and that zero is the absence of Viral, not the absence of a fight"
    );
    assert!(
        viral.mean_virus_stacks > 0.0
            && viral.mean_virus_stacks <= TEN_STACK_CAP as f64,
        "a Viral build averages a real pile, and no pile is above the cap: {}",
        viral.mean_virus_stacks
    );

    // …AND THE ARMOUR HALF OF THE SAME QUESTION. Toxin with Electricity is
    // Corrosive, which strips; Toxin with Cold is Viral, which does not —
    // so the pair says the figure tracks the STRIP and not the fight.
    let corrosive = soma(&["malignant_force", "high_voltage", "vital_sense"]);
    assert!(
        (viral.mean_armor_left - 1.0).abs() < 1e-9,
        "a build that strips nothing leaves the armour whole: {}",
        viral.mean_armor_left
    );
    assert!(
        corrosive.mean_armor_left < 1.0 && corrosive.mean_armor_left > 0.0,
        "a Corrosive build lands its damage on armour it has taken down: {}",
        corrosive.mean_armor_left
    );
}

fn arbucep(mods: &[&str]) -> FightParams {
    let base = crate::model::WeaponBase::from_data("arbucep", false, &[]);
    let pool = crate::mods_data::pool_for_weapon("arbucep");
    let refs: Vec<&crate::model::ModDef> =
        mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
    assert_eq!(refs.len(), mods.len(), "every named mod is in this weapon's pool");
    let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    FightParams::from_panel(
        &panel,
        &crate::arena::Arena::training(30.0),
        &crate::arcanes_data::ArcaneFx::none(),
    )
}

/// MULTISHOT PAYS IN DAMAGE HERE, NOT IN PROJECTILES. VERBATIM (wiki
/// `Arbucep`): *"Multishot increases weapon damage instead of creating
/// additional projectiles. Damage bonus is multiplicative to other sources
/// of damage."*
///
/// Both halves are asserted because each alone would look right: a run
/// that gained damage AND projectiles would pass a damage-only check, and
/// one that gained neither would pass a pellet-only check.
#[test]
fn multishot_buys_damage_and_not_a_seventh_missile() {
    let plain = monte_carlo(&arbucep(&[]), 30, 5);
    // Dual Rounds is +60% multishot: six missiles would become 9.6 on an
    // ordinary weapon, and here they stay six and hit 1.6x as hard.
    let split = monte_carlo(&arbucep(&["dual_rounds"]), 30, 5);

    assert!(
        (split.mean_pellets - plain.mean_pellets).abs() < 1e-9,
        "the volley is still six: {} against {}",
        split.mean_pellets, plain.mean_pellets
    );
    // THE HITS SCALE BY EXACTLY THE BONUS. `mean_damage` includes the DoTs
    // the volley leaves, and a DoT's payload is computed from ModifiedBase
    // rather than from the finished instance — so a FINAL multiplier does
    // not reach it, here or for Double Tap or Eclipse. Taking the DoT out
    // is what makes the assertion exact rather than approximately right.
    // THE FACTOR ITSELF, read off the hit account rather than inferred from
    // an aggregate. The account lists every factor the instance paid, in
    // the order the engine applies them, and its product IS the number
    // that reached the meter — so this asserts the mechanic rather than a
    // consequence of it that a dozen other things also move.
    let factor = |mods: &[&str]| {
        let rec = record(&arbucep(mods), 12345, 0.0, f64::INFINITY, 5_000, 0);
        rec.events()
            .iter()
            .find_map(|e| match &e.kind {
                crate::record::Kind::Damage(d) if d.part.is_some() => Some(d.clone()),
                _ => None,
            })
            .expect("a direct hit")
            .layers
            .iter()
            .find_map(|l| match l {
                crate::record::Layer::Mul { factor, value, .. }
                    if *factor == crate::record::Factor::MultishotAsDamage => Some(*value),
                _ => None,
            })
            // ABSENT MEANS 1.0. A factor of exactly one is dropped when the
            // ledger is built rather than listed and hidden later, which is
            // the whole of the "no pile of x1.00" rule — so the baseline
            // build has no such layer at all and that IS its value.
            .unwrap_or(1.0)
    };
    assert!((factor(&[]) - 1.0).abs() < 1e-9, "unmodded pays nothing: {}", factor(&[]));
    assert!(
        (factor(&["dual_rounds"]) - 1.6).abs() < 1e-9,
        "+60% multishot is exactly +60% on the instance: {}",
        factor(&["dual_rounds"])
    );
    // …and the run as a whole moves too, by less, because a FINAL
    // multiplier does not reach a DoT payload (which is computed from
    // ModifiedBase) — here, for Double Tap, and for Eclipse alike.
    assert!(
        (split.mean_dot_damage - plain.mean_dot_damage).abs() < 1e-9,
        "the DoTs are untouched: {} against {}",
        split.mean_dot_damage, plain.mean_dot_damage
    );
    let whole = split.mean_damage / plain.mean_damage;
    assert!(whole > 1.4 && whole < 1.6, "the DoTs dilute it to x{whole:.3}");
}

/// SIX MISSILES ARE SIX INSTANCES, which is the whole reason the elements
/// are not blended into one vector: a proc is drawn once per instance.
///
/// The volley carries six DIFFERENT combined elements, so a run has to show
/// procs of several of them — a blended instance would draw one proc from
/// a six-way weighted mix and could not.
#[test]
fn the_volley_is_six_instances_and_six_elements() {
    let p = arbucep(&[]);
    assert_eq!(p.pellet_damage.len(), 6);
    assert!(p.multishot_adds_damage);
    let s = monte_carlo(&p, 40, 9);
    // Six a pull, and a pull is one round.
    assert!(
        (s.mean_pellets / s.mean_shots - 6.0).abs() < 1e-9,
        "six missiles a pull: {} pellets over {} shots",
        s.mean_pellets, s.mean_shots
    );
    assert!(s.mean_procs > 0.0, "a 34.9% status volley procs");
}
