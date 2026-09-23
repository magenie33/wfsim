use super::*;

/// THE WIKI'S OWN TABLE, transcribed off the Vectis Prime page:
///
/// | tier | multiplier | hits |
/// |------|-----------|------|
/// | 1 | 1.5x | 5    |
/// | 2 | 2.0x | 15   |
/// | 3 | 2.5x | 45   |
/// | 4 | 3.0x | 135  |
/// | 5 | 3.5x | 405  |
/// | 6 | 4.0x | 1215 |
///
/// Its last two rows are 3675 and 11025, which are NOT `5 * 3^k` (3645 and
/// 10935) — the page's table disagrees with the page's own formula from
/// tier 7 up. The formula is implemented, because it is the rule and the
/// table looks like an arithmetic slip; the divergence is recorded here
/// rather than in a comment nobody runs, and it is unreachable either way
/// (3645 landing hits is over half an hour of a two-round magazine).
#[test]
fn the_combo_ladder_is_the_wikis() {
    let vp = SniperCombo { min: 5, seconds: 2.0 };
    for (hits, want) in [
        (0u32, 1.0),
        (4, 1.0),
        (5, 1.5),
        (14, 1.5),
        (15, 2.0),
        (44, 2.0),
        (45, 2.5),
        (135, 3.0),
        (405, 3.5),
        (1215, 4.0),
    ] {
        assert!(
            (vp.multiplier(hits) - want).abs() < 1e-12,
            "{hits} hits: {}, want {want}",
            vp.multiplier(hits)
        );
    }
    // The Vectis pays from the FIRST hit — the smallest minimum in the
    // game, and the reason the ordinary gun is not simply the worse one.
    let v = SniperCombo { min: 1, seconds: 2.0 };
    assert_eq!(v.multiplier(0), 1.0);
    assert!((v.multiplier(1) - 1.5).abs() < 1e-12);
    assert!((v.multiplier(3) - 2.0).abs() < 1e-12);
    assert!((v.multiplier(9) - 2.5).abs() < 1e-12);
    // A power of three is the case a `log3` implementation gets wrong: the
    // floor lands one short wherever the division rounds down.
    // 3^10, and 1.5 + 0.5*10 = 6.5.
    assert!((v.multiplier(59_049) - 6.5).abs() < 1e-12);
}

/// The roster's snipers carry both mechanics, with the wiki's numbers, and
/// nothing else in the roster carries either — a mechanic keyed on a class
/// the engine does not know is a mechanic that leaks.
#[test]
fn the_roster_declares_them_where_the_wiki_does() {
    let combo = |id: &str| spec(id).and_then(|w| w.sniper_combo);
    assert_eq!(combo("vectis").map(|c| c.min), Some(1));
    assert_eq!(combo("vectis_prime").map(|c| c.min), Some(5));
    // 2 s unless the weapon says otherwise (the Lanka, which is not here).
    assert_eq!(combo("vectis_prime").map(|c| c.seconds), Some(2.0));
    let scope = |id: &str| spec(id).and_then(|w| w.scope);
    assert_eq!(scope("vectis").map(|z| z.headshot_damage), Some(0.5));
    assert_eq!(scope("vectis_prime").map(|z| z.headshot_damage), Some(0.6));
    // THE COMBO IS THE SNIPER'S; THE SCOPE IS NOT. A Strike Combo Counter is
    // keyed on the class in game — the wiki's rule opens "Scoped in" and
    // the mechanic exists on no other family — so a combo outside the class
    // is a leak. A `scope:` is keyed on the fight's AIMING state and on
    // nothing else, which is why the Vesper 77's laser sight (+40%
    // critical damage while aiming, no published magnification) is one on a
    // pistol: same bucket, same gate, no scope on the gun.
    for w in all() {
        if w.sniper_combo.is_some() {
            assert_eq!(w.class, "sniper", "{} is not a sniper rifle", w.id);
        }
    }
    // …and every scope is either a sniper's or names itself in prose, so a
    // scope that turns up on an ordinary weapon by accident is still caught.
    for w in all() {
        if let Some(z) = w.scope {
            assert!(
                w.class == "sniper" || z.magnification.is_none(),
                "{}: a non-sniper scope must be an aim bonus with no published zoom",
                w.id
            );
        }
    }
}
