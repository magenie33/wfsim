//! MODULAR WEAPONS — a Kitgun's parts reaching the panel a fight reads.
//!
//! Its own module rather than a corner of the CO catalog's: what it is about is
//! [`spec_assembled`], and a test's home is part of what it says.
/// **THE LAETUM'S INCARNON FORM DOUBLES SECONDARY IRRADIATE'S ECHO**, and
/// its base form does not. 1.8x on a pure
/// single-target weapon, 3.6x here.
///
/// The pair is the whole point: a test asserting only that the Incarnon
/// form is 2.0 passes just as well on a build that applied it to the entire
/// weapon, which the base form's own measurement contradicts.
#[test]
fn the_laetums_incarnon_form_doubles_the_echo_and_its_base_form_does_not() {
    let m = |id: &str| super::spec(id).unwrap_or_else(|| panic!("{id}")).echo_multiplier;
    assert_eq!(m("laetum_incarnon"), 2.0);
    assert_eq!(m("laetum"), 1.0, "the base form measures the ordinary 1.8x");
    // …AND IT REACHES THE FIGHT. A number in a yaml that no panel carries
    // is a number nothing computes.
    let base = crate::model::WeaponBase::from_data("laetum_incarnon", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    assert_eq!(panel.echo_multiplier, 2.0);
}

/// **IT IS THE ONLY ONE, AND THAT IS ASSERTED RATHER THAN ASSUMED.** The
/// owner's reading is that the game counts the attack's damage components —
/// a direct hit and a radial give 1.8 + 1.8 — which would make every
/// direct+radial weapon in the roster a candidate. NONE of the others has
/// been measured, so none of them carries the field, and generalising one
/// measurement to a class is what `docs/CATALOGS.md` forbids.
///
/// This test is the note to come back to: the day somebody measures a
/// second weapon it fails, names both, and forces the decision to be made
/// on purpose rather than by a default.
#[test]
fn only_the_measured_entry_carries_an_echo_coefficient() {
    let odd: Vec<&str> = super::all()
        .iter()
        .filter(|s| (s.echo_multiplier - 1.0).abs() > 1e-9)
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(odd, ["laetum_incarnon"], "an unmeasured entry gained a coefficient");
}
