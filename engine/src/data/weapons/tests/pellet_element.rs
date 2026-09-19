use super::*;

/// SIX MISSILES, SIX ELEMENTS, AND SIX RESOLVES.
///
/// The Arbucep fires six homing missiles at once, each carrying a
/// different combined element. One blended vector would get the damage
/// right and everything else wrong — a proc is drawn once per instance, so
/// six missiles draw six and a blend draws one — so the panel resolves per
/// element and the fight picks by pellet index.
#[test]
fn each_projectile_resolves_its_own_element() {
    let base = crate::model::WeaponBase::from_data("arbucep", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let p = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    assert_eq!(p.pellet_damage.len(), 6, "six missiles, six vectors");

    use crate::rules::damage::DamageType::*;
    for (i, want) in [Blast, Corrosive, Gas, Magnetic, Radiation, Viral].iter().enumerate() {
        let (direct, radial) = &p.pellet_damage[i];
        assert!(direct.get(*want) > 0.0, "missile {i} carries {want:?}: {direct:?}");
        assert!(radial.get(*want) > 0.0, "...and so does its explosion: {radial:?}");
        // …and ONLY it, unmodded: each missile IS its element rather than
        // a blend containing it.
        assert!(
            (direct.total() - direct.get(*want)).abs() < 1e-9,
            "missile {i} is nothing but {want:?}: {direct:?}"
        );
    }
    // The published per-missile numbers, ground column.
    assert!((p.pellet_damage[0].0.total() - 32.0).abs() < 1e-9);
    assert!((p.pellet_damage[0].1.total() - 228.0).abs() < 1e-9);

    // NOTHING ELSE IN THE ROSTER HAS THEM, which is what keeps six resolves
    // off every other weapon's build.
    let with: Vec<&str> = all()
        .iter()
        .filter(|w| !w.attack.pellet_elements.is_empty())
        .map(|w| w.id.as_str())
        .collect();
    assert_eq!(with, ["arbucep"]);
}

/// THE LIST IS THE PROJECTILE COUNT. `multishot` and `pellet_elements`
/// describe the same six missiles from two directions, and a weapon whose
/// two disagree would cycle its elements against its own pellet count.
#[test]
fn the_element_list_is_as_long_as_the_volley() {
    for w in all() {
        if w.attack.pellet_elements.is_empty() {
            continue;
        }
        assert_eq!(
            w.attack.pellet_elements.len(),
            w.attack.multishot.round() as usize,
            "{}: {} elements against {} projectiles",
            w.id,
            w.attack.pellet_elements.len(),
            w.attack.multishot
        );
        // …and a weapon that lists them is one whose multishot pays in
        // DAMAGE, or the seventh projectile would have no element.
        assert!(
            w.attack.multishot_adds_damage,
            "{}: lists per-projectile elements but lets multishot add projectiles",
            w.id
        );
    }
}
