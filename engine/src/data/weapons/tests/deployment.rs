use super::*;

/// A DEPLOYMENT CHANGES THE DAMAGE. VERBATIM (wiki `Archgun`): *"most Heavy
/// Weapons (a.k.a. Archguns when used via the Archgun Deployer) have had
/// their damage doubled"*.
///
/// The axis was built as a SUSTAIN axis on a reading of the two-column
/// infobox that said "same damage, same crit, same status — only the
/// sustain differs". Crit, multiplier and status ARE identical in both
/// columns, which is why the wrong half went unnoticed: three of the four
/// stats checked out, and the Larkspur Prime posted 112 board rows at half
/// its ground damage.
#[test]
fn a_deployment_moves_the_damage_and_the_sustain() {
    let ground = |id: &str| crate::model::WeaponBase::from_data(id, false, &[]);
    let space = |id: &str| {
        let mut b = ground(id);
        apply_deployment(&mut b, id, "archwing");
        b
    };
    // The entry's own column is Atmosphere, so the ground build is the file
    // and the Archwing one is the override.
    let (g, s) = (ground("larkspur_prime"), space("larkspur_prime"));
    assert_eq!(g.base_vector.total(), 180.0, "the ground column is 20 + 160");
    assert_eq!(s.base_vector.total(), 90.0, "and Archwing is half of it");
    // The sustain half, which was right all along.
    assert_eq!(g.base_reload, 2.5);
    assert_eq!(s.base_reload, 4.5);
    assert!(g.no_resupply && !s.no_resupply, "a ground Arch-Gun cannot be resupplied");
    // What DOES NOT move — and the reason the wrong reading survived.
    assert_eq!(g.base_crit_chance, s.base_crit_chance);
    assert_eq!(g.base_status_chance, s.base_status_chance);

    // EVERY ATTACK PART. The alt-fire form doubles its impact AND its
    // explosion; a multiplier that reached only the bullet would leave the
    // explosion at half of what the same infobox prints beside it.
    let (gc, sc) = (ground("larkspur_prime_charged"), space("larkspur_prime_charged"));
    assert_eq!(gc.base_vector.total(), 840.0);
    assert_eq!(sc.base_vector.total(), 420.0);
    let rad = |b: &crate::model::WeaponBase| b.radial.as_ref().expect("it explodes").base_vector.total();
    assert_eq!(rad(&gc), 1600.0);
    assert_eq!(rad(&sc), 800.0);

    // ...and a weapon with one deployment is untouched by the axis, which
    // is what keeps this off the rest of the roster.
    let mut torid = ground("torid");
    let before = torid.base_vector.total();
    apply_deployment(&mut torid, "torid", "archwing");
    assert_eq!(torid.base_vector.total(), before);
}

/// NO HALF-APPLIED COLUMN. A deployment that restates the direct damage
/// must restate every OTHER attack part the entry has, or the explosion is
/// left on the tab the bullet just left — which is the same shape of error
/// as the one that started this, one level down.
///
/// Checked over the whole roster rather than on the two entries that have
/// a deployment today, because the next Arch-Gun with an explosion is the
/// one that would get it wrong.
#[test]
fn a_deployment_restates_every_attack_part_or_none() {
    for w in all() {
        for (name, d) in &w.deployments {
            if d.damage.is_none() {
                continue;
            }
            assert_eq!(
                w.attack.radial.is_some(),
                d.radial_damage.is_some(),
                "{}/{name}: the entry has a radial and this column does not restate it",
                w.id
            );
            assert_eq!(
                w.attack.lingering.is_some(),
                d.lingering_damage.is_some(),
                "{}/{name}: the entry has a lingering field and this column does not restate it",
                w.id
            );
        }
    }
}

/// THE DOUBLING IS NOT A RULE, so this does not assert it.
///
/// The wiki says *"MOST Heavy Weapons … have had their damage doubled"*, and
/// the roster proves "most" is doing real work: the Phaedra is x2.071, the
/// Dual Decurion x1.727, and the Cyngas doubles its TOTAL while changing its
/// split (39.6/39.6/40.8 becomes an even 80/80/80, so Slash goes x1.961 and
/// the other two x2.020). The Prisma Dual Decurions is exactly x2 off the
/// SAME Archwing vector its ordinary is x1.727 off. No single multiplier
/// expresses this class, which is why both columns are transcribed per
/// entry.
///
/// What IS worth asserting is that a transcription slip cannot pass. A
/// dropped digit, a factor of ten, a column pasted into the wrong file: all
/// of those land far outside the band the game's own numbers occupy, and a
/// ground column that came out SMALLER than its Archwing one would be the
/// original bug with the two tabs swapped.
#[test]
fn every_stated_column_is_in_the_band_the_game_uses() {
    let mut seen = 0;
    for w in all() {
        let Some(d) = w.deployments.get("archwing") else { continue };
        let Some(m) = d.damage.as_ref() else { continue };
        let space: f64 = m.values().sum();
        let ground: f64 = w.attack.damage.values().sum();
        assert!(space > 0.0, "{}: a stated column with no damage in it", w.id);
        let r = ground / space;
        // THE OBSERVED RANGE, after reading all twenty pages: 1.000 (Corvas
        // Prime, whose two tabs are identical) through 1.489 (Kuva
        // Grattler) and 1.5 (Grattler, Mausolon) up to 2.071 (Phaedra).
        // The invariant worth asserting is only the DIRECTION — a ground
        // column is never weaker than its Archwing one — plus a ceiling
        // that a dropped digit or a factor of ten cannot pass.
        assert!(
            (1.0..=2.5).contains(&r),
            "{}: ground is x{r:.3} of Archwing ({ground} against {space}).                  Under 1.0 means the two tabs are swapped; over 2.5 is a                  transcription slip. The game's observed range is 1.000-2.071.",
            w.id
        );
        seen += 1;
    }
    assert!(seen >= 8, "every Arch-Gun states both columns: only {seen} did");
}
