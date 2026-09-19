use crate::model::{AbilityStat, ModEffect};

fn effects(id: &str) -> Vec<ModEffect> {
    crate::data::mods::pool_for_build("grimoire_active", &[])
        .iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("{id} is in the Tome pool"))
        .effects
        .clone()
}

/// ALL EIGHT TOME MODS LOAD WITH SOMETHING TO SAY.
///
/// They were transcribed and then all eight filed as paying nothing, on
/// readings that turned out to be wrong twice: "there are no allies" (the
/// player is one) and "a drop is not a damage model" (`engine::ammo` makes
/// one). What each is worth now is a per-card question and this asserts the
/// ROSTER rather than a list — a ninth Tome mod fails here on the day it
/// lands rather than quietly loading as an empty card.
#[test]
fn every_tome_mod_carries_a_modelled_effect_or_says_why_not() {
    let pool = crate::data::mods::class_pool("tome");
    assert_eq!(pool.len(), 8, "eight Tome mods: {:?}", pool.iter().map(|m| m.id).collect::<Vec<_>>());
    for m in &pool {
        assert!(
            !m.effects.is_empty(),
            "{}: a card that loads with no effects is one the builder offers and cannot explain",
            m.id
        );
    }
}

/// THE TWO CANTICLES THAT REACH A NUMBER, and they reach different ones.
///
/// Lohk is a bracket — fire rate, on kill, for 15 s — and Jahu is not: it
/// takes armour off OTHER bodies, so it is a property of the fight rather
/// than of the build and lands in its own field. Asserted as the SHAPE each
/// takes, because filing one as the other is the mistake that would still
/// produce a plausible number.
#[test]
fn lohk_is_a_bracket_and_jahu_is_a_strip() {
    let lohk = effects("lohk_canticle");
    assert!(
        lohk.iter().any(|e| matches!(e, ModEffect::GrantsStackingBuff(b)
            if b.grant == crate::model::BuffGrant::FireRate
                && b.trigger == crate::model::BuffTrigger::Kill
                && (b.per_stack - 0.3).abs() < 1e-9
                && (b.duration - 15.0).abs() < 1e-9)),
        "Lohk: +30% fire rate on kill for 15 s, got {lohk:?}"
    );
    let jahu = effects("jahu_canticle");
    assert!(
        jahu.iter().any(|e| matches!(e, ModEffect::StripOnKillInRange(f, r)
            if (f - 0.05).abs() < 1e-9 && (r - 50.0).abs() < 1e-9)),
        "Jahu: 5% of the armour of everyone within Affinity Range, got {jahu:?}"
    );
}

/// THE FOUR INVOCATIONS ARE ONE CARD WITH THE STAT SWAPPED, and exactly two
/// of them reach a number.
///
/// The pair that pays nothing is transcribed with the pair that does and
/// carries its own reason — `AbilityStat::unmodelled_reason`, the shape
/// `ShardEffect` established: an effect is applied or it says why not,
/// never neither and never both.
#[test]
fn the_invocations_are_four_stats_and_two_of_them_pay() {
    let stat = |id: &str| {
        effects(id)
            .iter()
            .find_map(|e| match e {
                ModEffect::AbilityStat(s, per, max) => Some((*s, *per, *max)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{id} carries an ability stat"))
    };
    for (id, want, per, max) in [
        ("vome_invocation", AbilityStat::Strength, 0.04, 15),
        ("ris_invocation", AbilityStat::Duration, 0.04, 15),
        ("netra_invocation", AbilityStat::Efficiency, 0.04, 15),
        ("xata_invocation", AbilityStat::EnergyRegen, 1.0, 10),
    ] {
        let (got, got_per, got_max) = stat(id);
        assert_eq!(got, want, "{id}");
        assert!((got_per - per).abs() < 1e-9, "{id}: {got_per} a stack");
        assert_eq!(got_max, max, "{id}: stacks");
    }
    // …AND THE SPLIT IS EXACTLY TWO AND TWO. A reason on one that pays, or
    // none on one that does not, is the failure this catches — and it
    // catches it in the direction that matters, since a missing reason is
    // an effect the panel shows as working and never applies.
    assert!(stat("vome_invocation").0.unmodelled_reason().is_none());
    assert!(stat("ris_invocation").0.unmodelled_reason().is_none());
    assert!(stat("netra_invocation").0.unmodelled_reason().is_some());
    assert!(stat("xata_invocation").0.unmodelled_reason().is_some());
}
