/// HEADCRACKER'S TEN STACKS EACH CARRY THEIR OWN CLOCK.
///
/// Owner observed it in game, which is the same rule
/// Stormburst carries and the same way it was found. The loader HARDCODED
/// the Galvanized rule here, on the reading that the card says nothing
/// else — and the two are not close: under Galvanized decay one headshot
/// inside the window holds
/// all ten, under FIFO it holds exactly one.
///
/// Asserted on the DECAY the buff carries rather than on a fight, because
/// that is the fact that was wrong; `dummy`'s `LiveStacks` already has both
/// families and is tested on each.
#[test]
fn every_headcracker_decays_stack_by_stack() {
    let mut seen = 0;
    for e in crate::data::evolutions::pool() {
        if !e.id.ends_with("_headcracker") {
            continue;
        }
        seen += 1;
        let base = crate::model::WeaponBase::from_data(&e.weapon, true, &[e.id.as_str()]);
        let b = base
            .stacking_buffs
            .iter()
            .find(|b| b.id == "on_headshot_fire_rate")
            .unwrap_or_else(|| panic!("{}: no fire-rate buff", e.id));
        assert_eq!(
            b.decay,
            crate::model::BuffDecay::PerStackExpiry,
            "{}: each of its {} stacks runs its own clock",
            e.id,
            b.max_stacks
        );
        assert_eq!(b.max_stacks, 10, "{}", e.id);
        assert!((b.chance - 0.5).abs() < 1e-9, "{}: the 50% roll is in the notes cell", e.id);
    }
    assert_eq!(seen, 5, "the Headcracker roster moved");
}
