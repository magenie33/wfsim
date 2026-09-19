use super::*;

/// A CHARGE THAT EATS THE MAGAZINE MAKES MAGAZINE CAPACITY A DAMAGE STAT —
/// the only weapon in the roster where it is, and the reason the mechanic
/// is worth a field rather than a number.
///
/// Wiki Notes, verbatim: *"Charging consumes ammo, up to a full magazine on
/// full charge"*, *"Damage dealt by the plasma bomb is directly
/// proportional to the amount of ammo consumed during the charge"*, and
/// *"Charge rate consumes a set 11 ammo per second. Modding to increase
/// magazine capacity will allow a longer total charge, and thus more
/// damage."* Confirmed in play.
///
/// Three things move together and this asserts all three, because any one
/// of them alone would be a different weapon: the TIME (magazine / 11), the
/// PRICE (the magazine), and the DAMAGE (x magazine / 11) — on the direct
/// hit AND on the explosion, which is the larger half of the bomb.
#[test]
fn a_magazine_mod_buys_the_phantasmas_charged_shot_more_damage() {
    let pool = crate::data::mods::pool_for_weapon("phantasma_prime_charged");
    let shot = |want: &[&str]| {
        let b = WeaponBase::from_data("phantasma_prime_charged", true, &[]);
        let multishot: Vec<_> = want
            .iter()
            .filter_map(|m| pool.iter().find(|d| d.id == *m))
            .collect();
        resolve(&b, &multishot, StackPolicy::Emergent)
    };
    // STOCK is the arsenal's own line, and nothing about it moved: 11 in
    // the magazine, one second at eleven a second, 15 + 73.
    let base = shot(&[]);
    assert_eq!(base.magazine_size, 11.0);
    assert!((base.charge_seconds.expect("a charge") - 1.0).abs() < 1e-9);
    assert!((base.ammo_cost - 11.0).abs() < 1e-9, "a full charge costs the magazine");
    assert!((base.damage.total() - 15.0).abs() < 1e-6);
    assert!((base.radial.as_ref().expect("the bomb").damage.total() - 73.0).abs() < 1e-6);

    // …AND A MAGAZINE MOD MOVES ALL THREE, in the same ratio.
    let big = shot(&["burdened_magazine"]);
    let k = big.magazine_size / base.magazine_size;
    assert!(k > 1.0, "the mod has to do something: {k}");
    assert!((big.charge_seconds.unwrap() / base.charge_seconds.unwrap() - k).abs() < 1e-9);
    assert!((big.ammo_cost / base.ammo_cost - k).abs() < 1e-9);
    assert!((big.damage.total() / base.damage.total() - k).abs() < 1e-6, "the direct hit");
    assert!(
        (big.radial.as_ref().unwrap().damage.total()
            / base.radial.as_ref().unwrap().damage.total()
            - k)
            .abs()
            < 1e-6,
        "the explosion"
    );
}

/// …AND NO OTHER WEAPON IS TOUCHED BY IT. The field is opt-in, so a charge
/// weapon that does not declare it keeps stating its own time and paying
/// its own price — a bow's draw is not a magazine.
#[test]
fn a_magazine_mod_does_not_move_an_ordinary_charge_weapon() {
    let pool = crate::data::mods::pool_for_weapon("cernos_prime");
    let shot = |want: &[&str]| {
        let b = WeaponBase::from_data("cernos_prime", true, &[]);
        let multishot: Vec<_> = want
            .iter()
            .filter_map(|m| pool.iter().find(|d| d.id == *m))
            .collect();
        resolve(&b, &multishot, StackPolicy::Emergent)
    };
    let a = shot(&[]);
    let b = shot(&["primed_fast_hands"]);
    assert_eq!(a.charge_seconds, b.charge_seconds);
    assert!((a.damage.total() - b.damage.total()).abs() < 1e-9);
}
