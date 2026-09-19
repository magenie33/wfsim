//! THE FURIS FAMILY SPLITS ON CONDITION OVERLOAD, and the split is the point.
//!
//! One Incarnon Genesis upgrades either weapon, so the tempting move is to give
//! them the same CO treatment. The catalog says otherwise by saying nothing:
//! its row names "Furis" and carries that weapon's numbers, there is no
//! MK1-Furis row, and absence from that table is a positive statement that the
//! attack behaves normally (owner confirmed 2026-08-06 — the MK1 does not have
//! the restriction). Lato Vandal has a row and Lato Prime does not, same family
//! and same Genesis, which is what a per-entry slip in DE's code looks like.
//!
//! Pinned in BOTH directions so a later tidy-up cannot quietly align them.
use super::*;

fn excludes(id: &str) -> bool {
    get(id)
        .unwrap_or_else(|| panic!("{id}"))
        .excludes_co_base(
            crate::model::FormKind::Incarnon,
            crate::model::CoBehavior::AdditiveWithBaseDamage,
        )
}

#[test]
fn the_furis_tier2_pair_excludes_its_own_base_from_condition_overload() {
    // "100 or 128 (with Evolution II) | 100 | 100% or 78%" — the CO term
    // keeps multiplying the unevolved 100. On the TIER, because the row
    // names "Evolution II" with no perk number and both options grant +28.
    assert!(excludes("furis_haven_foray"));
    assert!(excludes("furis_stormburst"));
}

/// …AND SO DOES THE MK1's, by the DEFAULT rather than by a row.
///
/// It read `!excludes(...)` and was the tidiest illustration of the old
/// default: same Genesis, same two perk NAMES, and the catalog has a Furis
/// row and no MK1 Furis row — so the pair differed on nothing but whether
/// somebody had written them down. That is a description of a survey's
/// coverage, not of a game mechanic, and an Adding entry no longer reads it
/// as one. MULTIPLYING entries still do, and deliberately: see
/// `excludes_co_base`.
#[test]
fn the_mk1_tier2_pair_excludes_it_too() {
    assert!(excludes("mk1_furis_haven_foray"));
    assert!(excludes("mk1_furis_stormburst"));
}

/// AN EXPLICIT `false` IS STILL HONOURED, so a measured exception has
/// somewhere to go. Nothing in `data/` uses it — asserted here so that
/// stays a fact about the roster rather than an assumption, and so the
/// opt-out is known to work on the day something needs it.
#[test]
fn the_opt_out_exists_and_nothing_uses_it() {
    let opted_out: Vec<&str> = pool()
        .iter()
        .filter(|d| d.co_base_excludes_this_evolution == Some(false))
        .map(|d| d.id.as_str())
        .collect();
    // …and a FORM-SCOPED declaration is the other half of the machinery:
    // the Torid's pair, measured on the Incarnon form and silent about the
    // base one (M50).
    let scoped: Vec<&str> = pool()
        .iter()
        .filter(|d| d.co_base_excludes_only_form.is_some())
        .map(|d| d.id.as_str())
        .collect();
    assert_eq!(scoped, ["torid_final_fusillade", "torid_plentiful_mayhem"], "{scoped:?}");
    assert!(
        opted_out.is_empty(),
        "these declare `co_base_excludes_this_evolution: false` — each needs a              measurement in docs/MEASUREMENTS.md: {opted_out:?}"
    );
}

/// A QUALIFIER NEVER STANDS ALONE, which is the whole reason it is not
/// counted as a gap.
///
/// "Stacks up to 4x" is a cap on the bonus above it, and in every perk
/// that carries one, that bonus is ITSELF inert — so counting the cap said
/// "partly modelled" twice for one thing, and put a third of the roster's
/// inert total on a fragment of a sentence.
///
/// The day one appears beside a WORKING effect the argument stops holding:
/// the bonus applies and its cap does not, which is a real gap and has to
/// be counted again. That is what this fails on.
#[test]
fn a_qualifier_never_stands_alone() {
    let mut alone = Vec::new();
    let mut seen = 0;
    for def in pool() {
        let quals = def
            .effects
            .iter()
            .filter(|e| matches!(e, EvoEffect::Qualifier(_)))
            .count();
        if quals == 0 {
            continue;
        }
        seen += quals;
        // EITHER ADMISSION counts as something to qualify. The cap on
        // "On Punch Through Hit: +10% Critical Chance for 3s. Stacks up to
        // 8x" is not orphaned because the clause it caps became an EDGE
        // rather than a todo — the perk still does nothing and still says
        // so, which is all this test is protecting.
        let gaps = def.unmodeled_effects().len() + def.out_of_scope_effects().len();
        if gaps == 0 {
            alone.push(def.id.clone());
        }
    }
    // A FLOOR, not a count. This number FALLS as cards get modelled — a
    // modelled stacking card carries its own `max_stacks:` and needs no
    // orphaned "Stacks up to Nx" beside it, which is what took it from 21
    // to 16 when the five Resonant Restores landed. The
    // assertion only ever protected against the loader dropping the shape
    // entirely, so the floor is set well below the live count and lowered
    // when it is genuinely passed rather than raised to meet it.
    assert!(seen > 10, "only {seen} qualifiers — did the loader stop reading them?");
    assert!(
        alone.is_empty(),
        "a qualifier with nothing to qualify — the cap is real and uncounted: {alone:?}"
    );
}

/// THE RATCHET. What the app does not model may go DOWN and not up.
///
/// The disclosure is derived, so the count is honest without anyone
/// maintaining it — and honest is not the same as improving: a tag nobody
/// is obliged to remove becomes a way of feeling finished. Lower this
/// number when a kind gets implemented; that is the only edit it takes.
/// *"Does not affect Incarnon Form"* — obeyed, on the two perks where it
/// is worth a number.
///
/// Eleven evolutions carry the sentence and nine qualify something this sim
/// does not model anyway, so the qualifier is a NAMED INERT effect and the
/// perk applies to both forms. On the two that raise a MAGAZINE that is a
/// real over-valuation: a Zylok Incarnon fired with 20 rounds where the
/// card gives it 12.
///
/// Asserted on BOTH forms of BOTH weapons, because a gate that skips
/// everything passes the half of this that only checks the Incarnon.
#[test]
fn a_base_form_only_evolution_reaches_the_base_form_and_stops_there() {
    for (base_id, form_id, perk, added) in [
        ("zylok", "zylok_incarnon", "zylok_extended_volley", 12.0),
        ("onos", "onos_incarnon", "onos_extended_volley", 10.0),
    ] {
        let b_off = crate::model::WeaponBase::from_data(base_id, true, &[]);
        let b_on = crate::model::WeaponBase::from_data(base_id, true, &[perk]);
        assert_eq!(b_off.form, crate::model::FormKind::Base);
        assert!(
            (b_on.magazine_size - b_off.magazine_size - added).abs() < 1e-9,
            "{base_id}: the base form takes the whole perk, {} -> {} (+{added} expected)",
            b_off.magazine_size, b_on.magazine_size
        );

        let f_off = crate::model::WeaponBase::from_data(form_id, true, &[]);
        let f_on = crate::model::WeaponBase::from_data(form_id, true, &[perk]);
        assert_eq!(f_off.form, crate::model::FormKind::Incarnon);
        assert!(
            (f_on.magazine_size - f_off.magazine_size).abs() < 1e-9,
            "{form_id}: the Incarnon form takes NONE of it, {} -> {}",
            f_off.magazine_size, f_on.magazine_size
        );
    }
}

#[test]
fn the_number_of_unmodelled_evolution_effects_only_goes_down() {
    // TWO OF THEM ARE THE PRAEDOS'S, and both are per-MODE facts this
    // engine has no shape for: Reaching Lunge's `+1.5 m Slide Attack Range`
    // is a reach bonus for one form of seven, and Transfigured Momentum's
    // trigger (a slide kill) and payoff (heavy attack efficiency) belong to
    // different modes, which are different BUILDS here. A per-form
    // evolution effect would retire both at once.
    //
    // …AND EIGHT CAME FROM RETIRING `one_target`, which is the one raise this
    // line has taken that added no gap. Those clauses were filed as EDGES
    // that could never pay out because the fight had one body; the arena
    // holds a formation, so they were work all along and the count was
    // hiding it (`notes: one_target_is_not_an_edge`). Eight of the sixteen
    // are computed now — a punch-through gate the Boltor Prime already had
    // — and these are what is left: a window a KILL opens, a trigger that
    // reads a punch-through hit, and two radius clauses.
    const CEILING: usize = 15;
    let n: usize = pool().iter().map(|d| d.unmodeled_effects().len()).sum();
    assert!(
        n <= CEILING,
        "{n} inert evolution effects, ceiling {CEILING} — a new gap needs \
             either an implementation or a deliberate raise of this line"
    );
    // …and it is not allowed to drift far BELOW without the ceiling
    // following it down, or the ratchet stops ratcheting.
    assert!(
        n + 25 >= CEILING,
        "{n} inert effects against a ceiling of {CEILING}: lower the ceiling"
    );
}
