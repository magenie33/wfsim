//! THE CARD IS RIGHT WHICHEVER ORDER THE EFFECTS ARE IN.
//!
//! The Winds of Purity failure, pinned by its outcome rather than by its cause:
//! the wiki's ladder is life steal 5/10/15/20% and Purity 0.25/0.5/0.75/1, and
//! the card printed "+100% Life Steal / +0.2 Purity" — the two ladders in each
//! other's slots. Both numbers are the kind a mod could have, which is why
//! reading the card could not catch it and why the value is pinned here.
use super::*;

#[test]
fn winds_of_purity_prints_the_wikis_ladder() {
    let info = desc_info("winds_of_purity").expect("the mod has a description");
    assert_eq!(info.at(0), "+5% Life Steal\n+0.25 Purity");
    assert_eq!(info.at(info.max_rank), "+20% Life Steal\n+1 Purity");
}

/// The filler finds an effect by the words the card uses for it, so a
/// two-word kind is matched and a lone word is not evidence.
#[test]
fn an_effect_is_found_by_the_words_its_card_uses() {
    let steal = serde_norway::from_str::<Value>("kind: life_steal_on_own_damage").unwrap();
    assert!(effect_spoken_at(&steal, "+x% life steal").is_some());
    assert!(effect_spoken_at(&steal, "+x purity").is_none());
    // A syndicate radial answers to its SYNDICATE, never to its kind.
    let radial = serde_norway::from_str::<Value>("kind: syndicate_radial\nsyndicate: purity").unwrap();
    assert!(effect_spoken_at(&radial, "+x purity").is_some());
    assert!(effect_spoken_at(&radial, "+x% life steal").is_none());
}

/// EVERY MOD EFFECT THE LOADER DROPS IS ON THE CARD, and the list of them
/// is short enough to argue about.
///
/// `unmodeled_effects` derives the disclosure, so a mod that starts
/// dropping one says so without anyone remembering to come back here —
/// this pins WHICH ones, so a new gap arrives in review rather than only
/// on a card nobody happens to open. The arcane side carries the same pin
/// (`an_arcane_that_does_nothing_with_an_effect_says_so`), added after
/// three of them promised a stat they silently never applied.
#[test]
fn a_mod_that_drops_an_effect_says_so_and_the_list_is_argued() {
    let mut found: Vec<String> = Vec::new();
    for (_, text) in crate::data::files_under("mods/") {
        let Ok(mf) = serde_norway::from_str::<ModFile>(text) else { continue };
        for why in unmodeled_effects(&mf.id) {
            found.push(format!("{} :: {why}", mf.id));
        }
    }
    found.sort();
    assert_eq!(
        found,
        [
            // TWO THINGS ACID SHELLS' EXPLOSION DOES THAT THIS ARENA
            // CANNOT. Line of sight is not a simplification here, it is a
            // missing dimension: there are no walls at all, so every body
            // in the radius is in sight of every other. On the group
            // ruler's open grid that is the game's own answer too; in a
            // corridor it is not.
            //
            // The Extra Hit is the other, and it compounds: the explosion
            // triggers Toxic Lash and Xata's Whisper, and "because these
            // effects are considered Sobek's damage, they too can trigger
            // Acid Shells". `fire_extra_hits` is not wired into the area
            // path, so a build running one of those abilities is
            // understated by the extra instances AND by the chain they
            // would start.
            "acid_shells :: a line of sight rule this arena has no walls to enforce",
            "acid_shells :: an extra hit the corpse explosion should also trigger",
            // A HEAL, and nothing damages the Tenno here — the same edge
            // Winds of Purity's life steal sits on.
            "bhisaj_bal :: health restore nothing damages the tenno here",
            // TWO EDGES AT ONCE: no distance and no finishers. The stun is
            // crowd control against a target that never acts.
            "dizzying_rounds :: a stun that opens finishers no distance and no \
                 finishers here",
            // Two clauses that need a SECOND thing in the world — a
            // bubble to hit, an ability to cast. The third, the Latron
            // Incarnon's, is modelled since M102.
            "double_tap :: a bullet attraction bubble makes each hit count twice",
            "double_tap :: hitting an object counts as a miss and clears the stacks",
            // Crowd control again, and for the same reason it is worth
            // nothing: the target never acts, so stone changes nothing it
            // TAKES.
            "metamorphic_magazine :: petrify after 20 hits crowd control against a \
                 target that never acts",
            // The card's whole headline needs a Nullifier, and the wiki
            // says outright it "has no effect on any other enemy in
            // Warframe".
            "neutralizing_justice :: destroys a nullifier shield generator no such \
                 enemy in this roster",
            // THE TWO HALVES OF THE NAPALM NOBODY PUBLISHED, and they are
            // different kinds of gap. The tick rate is the whole DPS of the
            // field and NOTHING states it — the page gives damage per tick
            // and a duration in seconds and never joins them, and the
            // Ogris's page, Napalm Grenades and DE's own card text are all
            // silent — so one a second is an assumption a measurement
            // settles rather than something the engine can derive.
            //
            // The Heat proc is the opposite: it is STATED and this engine
            // cannot express it. "ticking for 50% of napalm's damage per
            // second for 3 seconds" — the rate is what every Heat proc here
            // already does; the LENGTH is this mod's own and a per-proc
            // duration is not something a field can carry yet, so the burn
            // runs the standard time and this weapon is overstated by the
            // difference.
            "nightwatch_napalm :: a heat proc that should burn for three seconds and burns for the standard time",
            // A PER-WEAPON CATALOG ROW, of the kind docs/CATALOGS.md is
            // about: the wiki tabulates an ADDITIONAL spread penalty for
            // the Cernos Prime (and, commented out, four crossbows this
            // roster does not carry) on top of the flat ladder every bow
            // takes. One row for one weapon in the roster, so the mod's
            // own number is right for eight of the nine bows and a third
            // of a degree tight on the ninth.
            "split_flights :: the cernos prime takes an additional spread row of its own",
            // TWO THINGS THE PAGE STATES AND THE MODEL CANNOT. One is an
            // open question — whether the bonus reaches a status payload —
            // and the other is a family of alt-fire and reload mechanics
            // that inherit the last round's bonus, of which this roster
            // carries exactly one weapon (the Dual Toxocyst's Frenzy).
            "synth_charge :: the alt fire and reload mechanics that inherit the last rounds bonus",
            "synth_charge :: whether the bonus reaches a status payload is unmeasured",
            // ONE TARGET, so "3 or more enemies with a single projectile"
            // is unreachable — but the crit damage half is modelled and
            // pays to an invisible Tenno, which is why only this line is
            // here and not the whole mod.
            "unseen_dread :: invisibility on striking 3 enemies with one shot only \
                 one target here",
            // Its Purity radial lands 1,000 damage a blast and its life
            // steal heals a Tenno this arena does not have — so the
            // disclosure has to be per effect. Flagging the whole mod
            // would say the card does nothing, which is worse than saying
            // nothing at all.
            "winds_of_purity :: life steal on own damage",
        ],
        "the partly-modelled mod list moved"
    );
}
