use super::*;

/// EXACTLY THREE BUFFS OPEN FULL, and the list is closed.
///
/// Every buff in this app opens EARNED at zero (docs/BUFFS.md). Three are
/// exempt because keeping them up is not something a player has to think
/// about, which makes a fight that opens without them the less realistic of
/// the two — Fevered Frenzy, Reified Bane and Fresh Havoc. That is an
/// allowance the owner grants per buff, so the only thing that can hold it
/// is a test that reads the WHOLE roster back and refuses a fourth.
///
/// It is written this way round on purpose. A test naming the three and
/// checking they open full would pass just as well on a build that opened
/// thirty of them — and the shape these three share (no clock, nothing
/// clears them) is the DEFAULT for a card that states neither, carried by
/// twenty evolutions, so a rule derived from it would sweep in eighteen on
/// evidence nobody wrote down.
#[test]
fn only_the_three_named_buffs_open_full() {
    let mut rows: Vec<String> = Vec::new();
    let mut perks: Vec<&str> = Vec::new();
    for def in pool() {
        for card in def.buff_cards() {
            if card.opens_at == CardOpens::Full {
                rows.push(format!("{} ({})", def.id, card.id));
                perks.push(def.name.as_str());
            }
        }
    }
    perks.sort_unstable();
    perks.dedup();
    rows.sort();
    // THREE PERKS, however many weapons carry them — which is the unit the
    // allowance was granted in. Reified Bane is one buff on five weapons
    // (both Boars and the three Latos) with a different number on each, and
    // Fresh Havoc is one on both Somas; a sixth weapon inheriting one of
    // them is the same buff and needs no decision.
    assert_eq!(
        perks,
        ["Fevered Frenzy", "Fresh Havoc", "Reified Bane"],
        "the list of buffs that open full is CLOSED — adding one is a              decision about how the game is played, not a property of the              data. Rows: {rows:?}"
    );
}
