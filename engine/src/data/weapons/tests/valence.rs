use super::*;
use crate::rules::damage::DamageType;
use crate::model::WeaponBase;

/// THE VALENCE BONUS IS BASE DAMAGE, and the arithmetic is the wiki's own
/// sentence: *"ranging from 25-60% of the weapon's base damage … This
/// additional bonus damage applies as weapon base damage, meaning elemental
/// mods and status that scale from base / modified base damage will be
/// affected."*
///
/// The Kuva Nukor's 21 Radiation is the whole fixture: a Toxin progenitor
/// at 60% adds 12.6 Toxin BESIDE it, and a Radiation one at 60% MERGES into
/// it for 33.6 — the two cases that a naive "push a new element" would get
/// half right.
#[test]
fn a_valence_bonus_is_base_damage_and_merges_with_the_element_it_matches() {
    let bare = WeaponBase::from_data("kuva_nukor", true, &[]);
    assert!((bare.base_vector.total() - 21.0).abs() < 1e-9, "the fixture moved");

    let mut toxin = bare.clone();
    apply_valence(&mut toxin, "kuva_nukor", "toxin", 0.60);
    assert!((toxin.base_vector.get(DamageType::Radiation) - 21.0).abs() < 1e-9);
    assert!((toxin.base_vector.get(DamageType::Toxin) - 12.6).abs() < 1e-9);
    assert!((toxin.base_vector.total() - 33.6).abs() < 1e-9);

    // …AND THE SAME ELEMENT MERGES rather than appearing twice.
    let mut rad = bare.clone();
    apply_valence(&mut rad, "kuva_nukor", "radiation", 0.60);
    assert!((rad.base_vector.get(DamageType::Radiation) - 33.6).abs() < 1e-9);
    assert!((rad.base_vector.total() - 33.6).abs() < 1e-9);

    // THE ROLL'S RANGE IS THE GAME'S, so a request outside it is clamped
    // rather than obeyed — 100% is not a bonus a Lich can hand out.
    let mut over = bare.clone();
    apply_valence(&mut over, "kuva_nukor", "heat", 1.0);
    assert!((over.base_vector.get(DamageType::Heat) - 21.0 * 0.60).abs() < 1e-9);
    let mut under = bare.clone();
    apply_valence(&mut under, "kuva_nukor", "heat", 0.0);
    assert!((under.base_vector.get(DamageType::Heat) - 21.0 * 0.25).abs() < 1e-9);

    // AN ELEMENT THE SPEC DOES NOT OFFER IS REFUSED. A Kuva bonus is never
    // Puncture or Slash, and a request that says so leaves the weapon
    // alone rather than inventing a progenitor group.
    let mut slash = bare.clone();
    apply_valence(&mut slash, "kuva_nukor", "slash", 0.60);
    assert!((slash.base_vector.total() - 21.0).abs() < 1e-9, "slash is not a progenitor element");

    // …AND THE CO TERM READS THE WHOLE OF IT, measured (MEASUREMENTS M78).
    // A valenced copy is what every player owns, so its GunCO base is its
    // own printed damage and the fraction stays the ordinary 1.0 — the
    // panel said 62% and blamed an evolution this weapon does not have.
    assert!((rad.co_base - 33.6).abs() < 1e-9, "the CO base is {}", rad.co_base);
    assert!((rad.co_base_fraction() - 1.0).abs() < 1e-9);
    assert!((toxin.co_base_fraction() - 1.0).abs() < 1e-9);
    // …ON THE EXPLOSION TOO, and a weapon that declares a fraction of its
    // own keeps it: the Kuva Drakgoon's charged shot reads half its base
    // before any Lich and half of it after.
    let mut drak = WeaponBase::from_data("kuva_drakgoon", true, &[]);
    let declared = drak.co_base_fraction();
    apply_valence(&mut drak, "kuva_drakgoon", "heat", 0.60);
    assert!((drak.co_base_fraction() - declared).abs() < 1e-9, "{declared} moved");
    let mut ogris = WeaponBase::from_data("kuva_ogris", true, &[]);
    apply_valence(&mut ogris, "kuva_ogris", "heat", 0.60);
    let r = ogris.radial.as_ref().expect("the Kuva Ogris explodes");
    assert!((r.co_base_fraction() - 1.0).abs() < 1e-9, "{}", r.co_base_fraction());

    // …AND A WEAPON WITH NO SPEC CANNOT BE HANDED ONE.
    let mut torid = WeaponBase::from_data("torid", true, &[]);
    let before = torid.base_vector.total();
    apply_valence(&mut torid, "torid", "heat", 0.60);
    assert!((torid.base_vector.total() - before).abs() < 1e-9);
    assert!(valence_of("torid").is_none());
    assert!(valence_of("kuva_nukor").is_some());
}

/// THE FOUR READINGS, and the only arithmetic that lands on all of them.
///
/// MEASUREMENTS M78: a Kuva Nukor on a 60% Magnetic Lich with +220% base
/// damage reads **108 / 148 / 188 / 228** across Galvanized Strike's three
/// stacks, with TWO statuses on screen. Every number below the weapon comes
/// from data — the base, Hornet Strike, Galvanized Strike's rate — so the
/// measurement is what this test adds and the rest cannot drift under it.
///
/// It separates two claims at once, and neither alternative is close: the
/// term computing on the unvalenced 21 reads 133 / 158 / 183, and counting
/// the two VISIBLE statuses instead of three reads 134 / 161 / 188.
#[test]
fn the_kuva_nukors_measured_condition_overload_ladder() {
    let mut base = WeaponBase::from_data("kuva_nukor", true, &[]);
    apply_valence(&mut base, "kuva_nukor", "magnetic", 0.60);
    let pool = crate::data::mods::pool_for_weapon("kuva_nukor");
    let by = |id: &str| pool.iter().find(|m| m.id == id).expect(id);
    let mods = [by("hornet_strike"), by("galvanized_shot")];
    let r = crate::build::loadout::resolve(&base, &mods, crate::model::StackPolicy::Emergent);
    let stack = r.co_stack.expect("Galvanized Strike grants CO stacks");

    // MICROWAVE IS THE THIRD, and it is the weapon's own: this vector is
    // Radiation plus the Lich's Magnetic and has no other type to offer.
    assert!(spec("kuva_nukor").is_some_and(|s| s.applies_microwave));
    let types = 3.0;
    for (stacks, reading) in [(0u32, 108.0), (1, 148.0), (2, 188.0), (3, 228.0)] {
        assert!(stacks <= stack.max_stacks, "the card holds {stacks}");
        let co = base.base_vector.total()
            * stack.per_stack
            * types
            * f64::from(stacks)
            * base.co_base_fraction();
        let got = r.modified_base + co;
        assert!(
            (got - reading).abs() < 0.5,
            "{stacks} stacks: computed {got:.2} against a measured {reading}"
        );
    }
}
