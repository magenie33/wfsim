use super::*;

fn vectis_prime(duration: f64) -> FightParams {
    let base = crate::loadout::WeaponBase::from_data("vectis_prime", false, &[]);
    let refs: Vec<&crate::loadout::ModDef> = Vec::new();
    let panel =
        crate::loadout::resolve(&base, &refs, crate::loadout::StackPolicy::Emergent);
    FightParams::from_panel(
        &panel,
        &crate::arena::Arena::training(duration),
        &crate::arcanes_data::ArcaneFx::none(),
    )
}

/// THE COUNTER BUILDS ITSELF. A fight long enough to fire past the Vectis
/// Prime's five-hit minimum does more damage than the same fight cut off
/// before it, and a run that walks in holding a high count does more than
/// either — which is the whole reason the card exists.
#[test]
fn the_combo_multiplies_the_fight() {
    let cold = vectis_prime(60.0);
    assert!(cold.sniper_combo.is_some(), "the panel carried it into the fight");

    let mut held = cold.clone();
    held.combo_initial = 405;
    held.combo_held = true;

    let a = monte_carlo(&cold, 40, 7);
    let b = monte_carlo(&held, 40, 7);
    // Held at 405 the multiplier is 3.5x for the whole run; earned from
    // scratch it climbs through 1.0/1.5/2.0/2.5 and ends around 3.0 for a
    // fight this length, so the held run is ahead but not by 3.5x.
    assert!(
        b.mean_damage > a.mean_damage * 1.2,
        "a held combo is worth more than an earned one: {} vs {}",
        b.mean_damage,
        a.mean_damage
    );
    // ...and a weapon with no combo is untouched by either knob, which is
    // what keeps this mechanic from leaking into the rest of the roster.
    let mut none = cold.clone();
    none.sniper_combo = None;
    let mut none_held = none.clone();
    none_held.combo_initial = 405;
    none_held.combo_held = true;
    assert!(
        (monte_carlo(&none, 20, 7).mean_damage
            - monte_carlo(&none_held, 20, 7).mean_damage)
            .abs()
            < 1e-9,
        "no combo, no difference"
    );
}

/// AN INCARNON CYCLE'S COMBO IS THE BASE FORM'S, and it took a measurement
/// through the real endpoint to notice: the Vectis Prime's panel carried a
/// counter, every unit test above passed, and `/api/simulate` reported the
/// same damage whether the card said 0 or 405. `incarnon_cycle_from_panels`
/// builds the outer `FightParams` from the INCARNON panel and hangs the
/// base form off `cycle.base_form` — so `params.sniper_combo` was the
/// Incarnon form's `None` and the base form's counter was never read.
///
/// The cycle is how this weapon is actually played, so a mechanic that
/// works everywhere except there works nowhere that matters.
#[test]
fn a_cycle_pays_the_base_forms_combo() {
    let base = crate::loadout::WeaponBase::from_data("vectis_prime", false, &[]);
    let inc = crate::loadout::WeaponBase::from_data("vectis_prime_incarnon", false, &[]);
    let refs: Vec<&crate::loadout::ModDef> = Vec::new();
    let pol = crate::loadout::StackPolicy::Emergent;
    let arena = crate::arena::Arena::training(60.0);
    let cycle = |initial: u32, held: bool| {
        let mut p = FightParams::incarnon_cycle_from_panels(
            &crate::loadout::resolve(&inc, &refs, pol),
            &crate::loadout::resolve(&base, &refs, pol),
            false,
            LockMode::Initial(0),
            &arena,
            &crate::arcanes_data::ArcaneFx::none(),
        );
        p.combo_initial = initial;
        p.combo_held = held;
        p
    };
    // The card exists on a cycle at all — this is the assertion that was
    // false, and `replay.buffs` came back empty because of it.
    assert!(
        cycle(0, false).buff_roster().iter().any(|b| b.id == "sniper_combo"),
        "the cycle offers the counter its base form has"
    );
    let cold = monte_carlo(&cycle(0, false), 40, 3);
    let hot = monte_carlo(&cycle(405, true), 40, 3);
    assert!(
        hot.mean_damage > cold.mean_damage * 1.1,
        "the card moves a cycle's number: {} vs {}",
        hot.mean_damage,
        cold.mean_damage
    );
}

/// DECAY IS BY ONE, NOT A RESET — *"reduced by 1 after a short period of
/// time that no successful hits have been made"* — so a sniper interrupted
/// for a second keeps almost all of it, and one interrupted for a minute
/// keeps thirty fewer.
#[test]
fn the_counter_decays_one_at_a_time() {
    let c = crate::weapons_data::SniperCombo { min: 5, seconds: 2.0 };
    assert_eq!(combo_at(Some(c), false, 100, 0.0, 1.9), 100);
    assert_eq!(combo_at(Some(c), false, 100, 0.0, 2.0), 99);
    assert_eq!(combo_at(Some(c), false, 100, 0.0, 60.0), 70);
    // Past the bottom it stops rather than wrapping.
    assert_eq!(combo_at(Some(c), false, 3, 0.0, 600.0), 0);
    // LOCKED means the thing that ends this buff no longer does.
    assert_eq!(combo_at(Some(c), true, 100, 0.0, 600.0), 100);
    // A weapon without one has no counter to read.
    assert_eq!(combo_at(None, false, 100, 0.0, 1.0), 0);
}

/// It is a rostered buff, so it gets a card and a replay curve — and the
/// curve is the reason `Frame::stacks` is u16: a sniper past its fourth
/// tier has been counting for longer than a u8 can hold.
#[test]
fn the_counter_is_on_the_roster_and_in_the_replay() {
    let p = vectis_prime(60.0);
    let roster = p.buff_roster();
    let at = roster
        .iter()
        .position(|x| x.id == "sniper_combo")
        .expect("the combo is a rostered buff");
    assert_eq!(roster[at].max_stacks, 0, "uncapped: the tiers do not stop");

    let rep = replay(&p, 12345, 60);
    let peak = rep.frames.iter().map(|f| f.stacks[at]).max().unwrap_or(0);
    assert!(peak >= 5, "the curve shows the counter climbing past the minimum: {peak}");
}
