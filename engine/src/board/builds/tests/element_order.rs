//! The pairing dimension the optimizer searches and the quick calc reports.
//! Every number quoted here was measured through `/api/simulate` on the
//! Burston Prime at Thrax Lv 9999 SP, 300 s, 10 runs (kill rate).
use super::*;
use crate::rules::damage::DamageType;
use crate::rules::damage::DamageType::*;

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

/// EACH ATTACK PART COMBINES ITS OWN ELEMENT, so one mod can make two
/// different combinations at once.
///
/// The Shedu is where this stops being an implementation detail: its direct
/// hit is Heat and its explosion is Electricity, and the wiki gives the
/// consequence as a worked example (Tips, verbatim):
///
/// > The Heat and Electricity damage portions are separate from one
/// > another, the Shedu can get a combination of Gas and Corrosive with
/// > only a Toxin damage mod, or a combination of Blast and Magnetic with
/// > only a Cold mod.
///
/// Both pairs are asserted, because either could be right by accident: a
/// build that combined ONE element set and handed it to both parts would
/// produce Gas twice, and one that ignored the radial's innate would
/// produce Gas and plain Toxin.
#[test]
fn one_elemental_mod_makes_two_combinations_on_a_two_element_weapon() {
    let types = |mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("shedu", true, &[]);
        let pool = crate::data::mods::pool_for_weapon("shedu");
        let picked: Vec<&crate::model::ModDef> = mods
            .iter()
            .map(|id| pool.iter().find(|m| m.id == *id).expect("mod"))
            .collect();
        let p = crate::build::loadout::resolve(&base, &picked, crate::model::StackPolicy::Emergent);
        let live = |v: &crate::rules::damage::DamageVector| {
            let mut out: Vec<DamageType> = DamageType::ALL
                .iter()
                .copied()
                .filter(|d| v.get(*d) > 1e-9)
                .collect();
            out.sort_by_key(|d| *d as usize);
            out
        };
        (live(&p.damage), live(&p.radial.as_ref().expect("the Shedu explodes").damage))
    };

    // STOCK: the two innates, untouched.
    assert_eq!(types(&[]), (vec![Heat], vec![Electricity]));
    // ONE TOXIN MOD -> Gas on the hit, CORROSIVE on the explosion.
    assert_eq!(types(&["infected_clip"]), (vec![Gas], vec![Corrosive]));
    // ONE COLD MOD -> Blast on the hit, MAGNETIC on the explosion.
    assert_eq!(types(&["primed_cryo_rounds"]), (vec![Blast], vec![Magnetic]));
}

/// THREE elemental mods are not one build — they are three, and on this
/// weapon the best is 3.3x the worst (2.074 against 0.627). That spread is
/// the whole reason the chip has to name its pairing.
#[test]
fn three_elements_make_three_pairings() {
    let orders = element_orders(
        "burston_prime_incarnon",
        &ids(&["primed_cryo_rounds", "infected_clip", "hellfire"]),
        &ids(&["burston_prime_evo1_incarnon_form"]),
    );
    assert_eq!(orders.len(), 3, "3 distinct elements pair 3 ways");
    let mut made: Vec<Vec<DamageType>> = orders.iter().map(|o| o.combined.clone()).collect();
    made.sort_by_key(|c| c.iter().map(|&t| crate::rules::elements::wiki_order(t)).collect::<Vec<_>>());
    assert_eq!(made, vec![vec![Blast], vec![Gas], vec![Viral]]);
}

/// ...and the leftover is read off the RESOLVED vector, not off the mod
/// order — which is what catches the innate. The Incarnon form's base
/// damage is Heat, so Cold + Toxin alone is already Viral + Heat with no
/// Heat mod equipped at all.
#[test]
fn an_innate_element_shows_up_in_the_leftover() {
    let orders = element_orders(
        "burston_prime_incarnon",
        &ids(&["primed_cryo_rounds", "infected_clip"]),
        &ids(&["burston_prime_evo1_incarnon_form"]),
    );
    assert_eq!(orders.len(), 1, "two elements pair one way");
    assert_eq!(orders[0].combined, vec![Viral]);
    assert!(
        orders[0].leftover.contains(&Heat),
        "the Incarnon form's own Heat is still there: {:?}",
        orders[0].leftover
    );
}

/// Same element twice POOLS — it is one entry in the sequence, so it adds
/// no pairing. Getting this wrong is what shipped 5669040 (Viral + Heat
/// published as Blast + Toxin, 4.7511 down to 0.1293).
#[test]
fn two_mods_of_one_element_do_not_multiply_the_pairings() {
    let one = element_orders("burston_prime_incarnon", &ids(&["hellfire", "primed_cryo_rounds"]), &[]);
    let two = element_orders(
        "burston_prime_incarnon",
        &ids(&["hellfire", "wildfire", "primed_cryo_rounds"]),
        &[],
    );
    assert_eq!(one.len(), 1);
    assert_eq!(two.len(), 1, "a second Heat mod pools; still one pairing");
    assert_eq!(two[0].combined, vec![Blast]);
}

/// A set with no elemental mod still answers — one entry, so a caller
/// always has something to measure rather than a special case to write.
#[test]
fn a_set_with_no_elements_still_yields_one_order() {
    let o = element_orders("burston_prime_incarnon", &ids(&["serration", "split_chamber"]), &[]);
    assert_eq!(o.len(), 1);
    assert_eq!(o[0].mods.len(), 2);
    assert!(o[0].combined.is_empty());
}

/// The order handed back is one that PRODUCES the pairing — a caller
/// simulates it verbatim, so it has to carry every mod it was given,
/// elemental or not.
#[test]
fn every_order_carries_the_whole_set() {
    let set = ids(&["primed_cryo_rounds", "infected_clip", "hellfire", "serration"]);
    for o in element_orders("burston_prime_incarnon", &set, &ids(&["burston_prime_evo1_incarnon_form"])) {
        let mut got = o.mods.clone();
        got.sort();
        let mut want = set.clone();
        want.sort();
        assert_eq!(got, want, "an order dropped a mod");
    }
}
