use super::*;

/// A weapon PASSIVE belongs to the weapon that lists it. Frenzy is Dual
/// Toxocyst's (both forms); the Laetum has none. Hardcoding it handed
/// DT's x2.5-on-headshot fire rate to every transform weapon.
#[test]
fn frenzy_belongs_only_to_the_weapon_that_lists_it() {
    assert!(has_perk("dual_toxocyst", "frenzy"));
    assert!(has_perk("dual_toxocyst_incarnon", "frenzy"));
    assert!(!has_perk("laetum", "frenzy"));
    assert!(!has_perk("laetum_incarnon", "frenzy"));
    assert!(!has_perk("dual_toxocyst", "no_such_perk"));
}
