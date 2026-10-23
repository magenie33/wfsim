//! THE PRECOMBAT LIST A FIGHT REPORTS, composed off the params it runs.
use super::*;

fn lines(p: &FightParams) -> (Vec<String>, String) {
    (p.precombat().iter().map(crate::data::apl::Precombat::to_simc).collect(), p.apl().to_simc())
}

/// **A MODE THE TOGGLE ENTERS IS SWITCHED INTO BEFORE THE FIGHT AND FIRED ON
/// THE TRIGGER**; an attack Alternate Fire fires itself is pressed all fight.
#[test]
fn a_toggled_mode_switches_first_and_then_shoots() {
    let press = FightParams { form: crate::model::FormKind::AltFire, ..no_status() };
    let toggled = FightParams { alt_fire_toggles: true, ..press.clone() };
    let (pre, apl) = lines(&press);
    assert!(pre.is_empty(), "{pre:?}");
    assert!(apl.ends_with("alt_fire"), "{apl}");
    let (pre, apl) = lines(&toggled);
    assert_eq!(pre, ["switch_mode,form=alt_fire"]);
    assert!(apl.ends_with("shoot"), "{apl}");
}

/// **A PILE EARNED BEFORE THE FIGHT IS NAMED BY THE PRESS THAT EARNS IT, AT
/// THE COUNT THE FIGHT OPENS ON** — and a pile opening at zero is not named.
#[test]
fn a_precombat_pile_is_listed_at_its_opening_count() {
    let base = crate::model::WeaponBase::from_data("soma", false, &["soma_fresh_havoc"]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let p = FightParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &crate::data::arcanes::ArcaneFx::none());
    assert_eq!(lines(&p).0, ["reload_from_empty,until=buff.on_empty_reload_damage.stack>=2"]);
    let mut zero = p.clone();
    zero.apply_buff_config(&[("on_empty_reload_damage".to_string(), (0, false))].into_iter().collect());
    assert!(lines(&zero).0.is_empty(), "the reader set it to zero");
}
