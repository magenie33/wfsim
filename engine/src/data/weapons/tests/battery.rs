use super::*;

/// A BATTERY'S `reload_seconds` IS THE EMPTY CASE — `delay_empty + magazine /
/// regen_per_second` ([`crate::model::Battery`]), because the fight spends it
/// as the reload from empty. An arsenal figure built on the PARTIAL delay
/// reads plausible and makes every empty battery wait the wrong delay: the
/// Bubonico's 4.5 is `1.5 + 27/9`, where an empty one comes back in 3.1.
/// A stated empty animation outlasting the refill is the empty case instead.
#[test]
fn a_battery_reload_is_the_empty_delay_plus_the_refill() {
    let mut checked = 0;
    for s in all() {
        let Some(b) = s.battery else { continue };
        let (Some(reload), Some(magazine)) = (s.reload_seconds, s.magazine) else {
            panic!("{}: a battery states its magazine and reload", s.id);
        };
        let refill = b.delay_empty_seconds + magazine / b.regen_per_second;
        let want = b.empty_animation_seconds.map_or(refill, |a| a.max(refill));
        assert!(
            (reload - want).abs() < 0.01,
            "{}: reload_seconds {reload} is not {} + {magazine}/{} = {want:.3}",
            s.id,
            b.delay_empty_seconds,
            b.regen_per_second,
        );
        checked += 1;
    }
    assert!(checked >= 10, "only {checked} batteries were read");
}

/// A FORM OF A BATTERY WEAPON CARRIES THE BATTERY, because `battery:` is not
/// inherited and a form without one reloads like a magazine — no refill
/// between shots, and nothing reads wrong on the page. The one exception
/// states its whole wait itself: the Tenet Plinx's charged slug spends the
/// magazine and waits the page's 2.5 s, which a battery would count twice.
#[test]
fn a_form_of_a_battery_weapon_carries_the_battery() {
    const STATES_ITS_OWN_WAIT: &[&str] = &["tenet_plinx_charged"];
    let mut checked = 0;
    for s in all() {
        let Some(parent) = s.inherits.as_deref() else { continue };
        let Some(p) = spec(parent) else { continue };
        if p.battery.is_none() || STATES_ITS_OWN_WAIT.contains(&s.id.as_str()) {
            continue;
        }
        assert!(s.battery.is_some(), "{}: a form of {parent} draws on its battery", s.id);
        checked += 1;
    }
    assert!(checked >= 5, "only {checked} battery forms were read");
}
