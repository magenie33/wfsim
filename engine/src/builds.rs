//! IS THIS A BUILD SOMEONE COULD ACTUALLY EQUIP?
//!
//! The simulator does not ask. That is deliberate and stays that way — it is a
//! calculator, and `parse_simulate` says so: "the sim runs whatever it is given
//! — slot legality (8 main + 1 exilus) is the UI's job, and the engine resolves
//! any mod list honestly." Answering "what would this do" for a loadout nobody
//! can build is a legitimate thing to want.
//!
//! A SUBMISSION is the other case. A public board is fed over a network, where
//! the UI is not on the path and no answer can be assumed, so the rules the
//! arsenal enforces have to be checked here instead. Two jobs, two places: this
//! module never runs inside `simulate`.
//!
//! # Normalise, then reject — in that order
//!
//! [`normalize`] runs first and is not a courtesy. The evolution ladder is
//! applied by TRUNCATION rather than by an error ([`webapi::chosen_evolutions`]
//! → `ladder_prefix`), so a build carrying a tier nothing unlocked is scored as
//! the trimmed build. If the identity were hashed before that, a board row
//! would name one build and hold another's number. Hashing the NORMALISED form
//! makes the row and the score the same object by construction.
//!
//! # What identity means here
//!
//! Two builds are the SAME FIGHT when they produce the same number, and the
//! wire payload already says most of that: it carries no polarities, no Forma,
//! no slot positions and no mod ranks (every mod simulates at max rank).
//!
//! ORDER DOES COUNT, and this said the opposite for a day on the strength of
//! ONE measurement — eight mods reversed scored 0.96478 both ways, which was
//! true of that build and not of builds. Mods combine ELEMENTS in listed order:
//! Heat, Cold, Toxin, Electric is Blast + Corrosive; Heat, Toxin, Cold,
//! Electric is Gas + Magnetic, and on the Torid that is 12,424 DPS against
//! 46,583 (measured 2026-08-04). A sorted identity collapsed those into one row
//! and scored whichever the sort happened to produce.
//!
//! ...but only BETWEEN pairs. `elements::combine` walks the sequence in
//! `chunks_exact(2)` and combines each chunk with `combined_of`, which is
//! symmetric and pools both amounts — so swapping two elementals INSIDE one
//! pair is the same damage by construction. Treating that as a second build put
//! the Ocucor on the board twice at the same score, differing only in Frostbite
//! and Pistol Pestilence trading places (owner, 2026-08-06). The rule was right
//! and one notch too fine.
//!
//! The same goes for the order of the PAIRS among themselves — `combine` adds
//! each chunk's secondary into a vector, so Blast-then-Corrosive and
//! Corrosive-then-Blast are one fight (12,773.473 DPS either way, Torid). They
//! are ordered by the element each pair MAKES, in the wiki's own table order.
//!
//! So the identity is the weapon, the mod sequence CANONICALISED to one
//! representative per PAIRING, the evolution set, and the arcanes.
//!
//! Rivens are absent on purpose (user, 2026-08-04): they are personal random
//! items, so a board that counted them would rank luck. That also removes the
//! one free-text field a player authors — a riven's name — from anything that
//! would ever be uploaded.

use std::collections::BTreeSet;

use crate::mods::PlannedMod;

/// A benchmark build is judged with a Catalyst installed and polarized to the
/// weapon's own ceiling — the state a build worth submitting is in.
///
/// It used to be the constant 60, which is only a rank-30 weapon's answer.
/// Capacity "correlates to their Rank" and a rank-40 weapon reaches 80, so a
/// board that assumed 60 would have refused builds the game allows the moment
/// a Kuva weapon joined the roster (`crate::mods::fit`).
pub const BENCHMARK_INVESTMENT: crate::mods::Investment = crate::mods::Investment {
    catalyst: true,
    polarize_to_max: true,
    use_omni: false,
    use_umbra: false,
};

/// Slots a benchmark build may fill. EIGHT — the exilus slot is out of scope;
/// see the slot check in [`validate`].
pub const MAIN_SLOTS: usize = 8;

/// A build that passed, and what it costs to actually own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidBuild {
    /// Weapon id.
    pub weapon: String,
    /// Mod ids IN ORDER. The order is the build — it pairs the elements — so
    /// this is what arrived, minus anything the weapon cannot hold.
    pub mods: Vec<String>,
    /// Evolution ids after the ladder is applied, in tier order.
    pub evolutions: Vec<String>,
    /// Arcane ids, one per pool slot, `none` included so position is stable.
    pub arcanes: Vec<String>,
    /// Forma the cheapest legal polarity layout needs. Not a legality term —
    /// two builds that are the same FIGHT can cost different amounts to reach,
    /// and the board should show the cheaper one.
    pub forma: u32,
    /// Capacity that layout uses, out of [`CAPACITY`].
    pub drain: u32,
}

/// ONE REPRESENTATIVE PER BUILD: elements last, in the order that pairs them;
/// everything else ahead of them, ordered by a rule rather than by chance.
///
/// Raw order is too fine and sorted order is too coarse, and both were wrong
/// here in the same day. What actually matters is measured (2026-08-04, Torid,
/// six mods): moving three elementals from slots 1-3 to 4-6, interleaving them
/// with the rest, and reshuffling the non-elementals all give the IDENTICAL
/// 146,707.582 DPS — while swapping two elementals with each other gives
/// 12,424 against 46,583, because it pairs Blast + Corrosive instead of Gas +
/// Magnetic.
///
/// So: the PAIRING of the elemental mods is the build — WHICH of them share a
/// chunk, and which one trails — and nothing else about position is. Neither
/// the order inside a pair nor the order of the pairs among themselves changes
/// the damage vector, so both are normalised away below (measured: the same
/// four Torid mods with their two pairs swapped give 12,773.473 DPS either
/// way). `primary_element` is the same predicate `resolve` uses to walk the
/// hierarchy, so this cannot drift from what the sim does.
///
/// The rest are ordered biggest-drain first, then by DE's own English name
/// (owner, 2026-08-04) — a rule chosen so the representative is stable and
/// readable, not because the engine cares.
pub fn canonical_mods(weapon: &str, mods: &[String]) -> Vec<String> {
    let pool = crate::mods_data::pool_for_weapon(weapon);
    let def = |id: &String| pool.iter().find(|m| m.id == id.as_str());
    let (mut plain, mut elemental): (Vec<&String>, Vec<&String>) = mods
        .iter()
        .partition(|id| def(id).is_none_or(|m| m.primary_element().is_none()));
    // Biggest drain first, then DE's own English name — stable and readable.
    let rank = |a: &&String, b: &&String| {
        let (da, db) = (def(a).map_or(0, |m| m.base_drain), def(b).map_or(0, |m| m.base_drain));
        db.cmp(&da)
            .then_with(|| def(a).map_or("", |m| m.name).cmp(def(b).map_or("", |m| m.name)))
    };
    plain.sort_by(rank);
    // ...AND WITHIN EACH PAIR, because a pair's internal order decides nothing.
    //
    // `elements::combine` walks the elemental sequence with `chunks_exact(2)`
    // and calls `combined_of(a, b)`, which is SYMMETRIC, pooling both amounts
    // into the one secondary. So [Cold, Toxin] and [Toxin, Cold] are the same
    // Viral by construction, not by coincidence — and leaving them as two
    // representatives put the same build on the board twice, at the same score
    // to four decimals (owner, 2026-08-06: Ocucor, Frostbite and Pistol
    // Pestilence swapped).
    //
    // What is never touched is the PARTITION — which elements share a chunk,
    // and which one trails. Moving an elemental ACROSS a chunk boundary
    // re-pairs everything after it, which is the 12,424-against-46,583
    // measurement this function was written for. The rule was right and one
    // notch too fine: the PAIRING is the build, and position is not.
    for pair in elemental.chunks_mut(2) {
        pair.sort_by(rank);
    }
    // ...AND THE PAIRS AMONG THEMSELVES, by the element each one MAKES, in the
    // wiki's own table order (owner, 2026-08-06: "我们参考wiki的排序").
    //
    // A pair's position among other pairs decides nothing either: `combine`
    // walks the chunks and ADDS each secondary into a damage vector, so
    // Viral-then-Radiation and Radiation-then-Viral are the same vector. That
    // makes chunk order the last freedom left in this representation, and
    // pinning it is what turns "one fight" into "one string".
    //
    // THE ODD ONE OUT IS NOT SORTED WITH THEM. It is the remainder BY
    // POSITION — `chunks_exact(2)` leaves whatever trails — so moving it to
    // the front would re-pair every element after it, which is the one thing
    // this function must never do. It stays last.
    let odd = elemental.len() % 2;
    let split = elemental.len() - odd;
    let tail: Vec<&String> = elemental.split_off(split);
    let mut pairs: Vec<Vec<&String>> = elemental.chunks(2).map(<[&String]>::to_vec).collect();
    pairs.sort_by_key(|p| {
        let el = |x: &&String| def(x).and_then(|m| m.primary_element());
        // The secondary the pair makes; two mods of one element combine into
        // nothing, so those fall back to the element itself and still sort.
        match (el(&p[0]), el(&p[1])) {
            (Some(a), Some(b)) => crate::elements::combined_of(a, b)
                .map_or_else(|| crate::elements::wiki_order(a), crate::elements::wiki_order),
            _ => usize::MAX,
        }
    });
    let elemental: Vec<&String> = pairs.into_iter().flatten().chain(tail).collect();
    plain.into_iter().chain(elemental).cloned().collect()
}

/// Trim a submitted build to what the game would actually give it.
///
/// Never fails: unknown or foreign ids are DROPPED rather than rejected, since
/// an id we do not know is one this weapon cannot have either way. What is left
/// is what [`validate`] then judges.
fn normalize(weapon: &str, mods: &[String], evolutions: &[String]) -> (Vec<String>, Vec<String>) {
    let pool = crate::mods_data::pool_for_weapon(weapon);
    // CANONICALISED, not sorted and not left raw — see `canonical_mods`. A
    // plain sort scored a pairing nobody submitted; raw order made two spellings
    // of one fight into two rows.
    let kept: Vec<String> = mods
        .iter()
        .filter(|id| pool.iter().any(|m| m.id == id.as_str()))
        .cloned()
        .collect();
    let ms = canonical_mods(weapon, &kept);

    // The ladder: tier N is only open when the tiers below it are filled, so a
    // set is trimmed to its longest legal prefix. One option per tier.
    // Evolutions belong to the TRANSFORM GROUP, not to a form: the two entries
    // of a two-weapon pair share one ladder.
    let spec = crate::weapons_data::spec(weapon);
    let group = spec
        .and_then(|s| s.transform_group.clone())
        .unwrap_or_else(|| weapon.to_string());
    let mut evos = Vec::new();
    for tier in 1..=crate::evolutions_data::tier_count(&group) {
        let pick = evolutions.iter().find(|id| {
            crate::evolutions_data::get(id).is_some_and(|e| e.weapon == group && e.tier == tier)
        });
        match pick {
            Some(id) => evos.push(id.clone()),
            None => break, // the ladder stops at the first empty rung
        }
    }
    (ms, evos)
}

/// THE BOARD'S ADMISSION RULE: a legal build, and the shape THIS BENCHMARK asks
/// for.
///
/// `validate` answers whether a build could be equipped. This answers whether it
/// belongs on a particular leaderboard, and those are different questions — four
/// mods is a perfectly legal build and a meaningless board row.
///
/// THE RULE IS THE BENCHMARK'S, not a global constant (owner, 2026-08-05). A
/// benchmark owns its fight; it owns what it admits for the same reason, and a
/// second ruler may reasonably want something else entirely. What it must NOT
/// own is identity — `canonical_mods` is universal, because two boards that
/// disagreed about whether two builds are the same build would break dedup and
/// displacement on both.
///
/// "FULL" IS COMPUTED PER WEAPON. Eight main slots for everything, but the
/// evolution tiers this weapon actually has (Laetum 5, Boar Prime 4, an ordinary
/// rifle 0) and the arcane seats it actually has (an Arch-Gun 2, a sentinel
/// weapon 0). A weapon with nothing to fill is complete by having filled it,
/// which is what lets one rule cover a roster of different shapes.
///
/// THE EXILUS SLOT IS OUTSIDE IT, in both directions: a build is not more
/// complete for having one and not less for lacking one, and a submission that
/// arrives with one is accepted with the exilus dropped rather than refused
/// ("如果带着exilus测试，我们会收入然后去掉exilus"). The DROPPING happens in the
/// client and has to: this payload is a flat list with no slot positions, and an
/// exilus-eligible mod is legal in a main slot, so nothing here could tell which
/// entry came out of the exilus slot.
pub fn validate_for_board(
    benchmark: &str,
    weapon: &str,
    mods: &[String],
    evolutions: &[String],
    arcanes: &[String],
) -> Result<ValidBuild, String> {
    let b = validate(weapon, mods, evolutions, arcanes)?;
    let req = match crate::benchmarks_data::get(benchmark) {
        Some(bm) => bm.build.clone(),
        // An unknown benchmark admits nothing: scoring a build against a ruler
        // that does not exist would publish a number with no standard behind it.
        None => return Err(format!("unknown benchmark: {benchmark}")),
    };
    use crate::benchmarks_data::BuildRequirement as R;

    // MODS. The floor that stops an empty build being a weapon's only row —
    // there the single row IS the board, the builder presents it as "Benchmark
    // build #1" with a ⧉ that copies it, and an unmodded build in that position
    // is misinformation with nothing to displace it.
    if R::requires_full(&req.mods) && b.mods.len() != MAIN_SLOTS {
        return Err(format!(
            "{} mods, and this benchmark wants all {MAIN_SLOTS} main slots",
            b.mods.len()
        ));
    }

    // EVOLUTIONS. The same argument, and stronger: an Incarnon weapon with no
    // evolutions is not a weaker build of that weapon, it is the BASE FORM — a
    // different gun. `tier_count` is keyed on the TRANSFORM GROUP, since the two
    // entries of a two-weapon pair share one ladder.
    if R::requires_full(&req.evolutions) {
        let group = crate::weapons_data::spec(weapon)
            .and_then(|s| s.transform_group.clone())
            .unwrap_or_else(|| weapon.to_string());
        let want = crate::evolutions_data::tier_count(&group) as usize;
        if b.evolutions.len() != want {
            return Err(format!(
                "{} of {want} evolution tiers, and this benchmark wants every one",
                b.evolutions.len()
            ));
        }
    }

    // ARCANES. Seats come from the engine's own rule so the page and the board
    // cannot disagree about how many a weapon has. `none` is a filled slot only
    // in the sense that it holds a position — it is not an arcane.
    if R::requires_full(&req.arcanes) {
        let want = crate::weapons_data::arcane_pools(weapon).len();
        let have = b.arcanes.iter().filter(|a| a.as_str() != "none").count();
        if have != want {
            return Err(format!(
                "{have} of {want} arcane seats filled, and this benchmark wants every one"
            ));
        }
    }

    Ok(b)
}

/// Could a player have this in the arsenal?
///
/// The checks, and each one is a rule the game enforces at the slot:
/// the mod is in this weapon's pool; no two mods share a family; the set fits
/// 8 main slots plus at most one EXILUS-eligible mod in the exilus slot; and
/// some polarity layout fits it into [`CAPACITY`].
pub fn validate(
    weapon: &str,
    mods: &[String],
    evolutions: &[String],
    arcanes: &[String],
) -> Result<ValidBuild, String> {
    let spec = crate::weapons_data::spec(weapon)
        .ok_or_else(|| format!("unknown weapon: {weapon}"))?;
    // DUPLICATES first, and separately: `normalize` collapses them, so a build
    // listing one mod nine times would otherwise be reported as "eight mods are
    // not in the pool" — a true count attached to the wrong reason, which is
    // worse than no reason at all.
    let mut uniq: Vec<&String> = mods.iter().collect();
    uniq.sort();
    uniq.dedup();
    if uniq.len() != mods.len() {
        return Err(format!("{} of {} mods are listed twice", mods.len() - uniq.len(), mods.len()));
    }
    let (ms, evos) = normalize(weapon, mods, evolutions);
    if ms.len() != mods.len() {
        // Loud, because a silently dropped mod is a build the submitter did not
        // send being scored under their name.
        return Err(format!(
            "{} of {} mods are not in {}'s pool",
            mods.len() - ms.len(),
            mods.len(),
            spec.name
        ));
    }
    let pool = crate::mods_data::pool_for_weapon(weapon);
    let def = |id: &str| pool.iter().find(|m| m.id == id).expect("normalised into the pool");

    // FAMILIES. Two mods of one family cannot be equipped together.
    let mut fams: Vec<&str> = ms.iter().filter_map(|id| def(id).family).collect();
    fams.sort_unstable();
    for w in fams.windows(2) {
        if w[0] == w[1] {
            return Err(format!("two mods of the {} family", w[0]));
        }
    }

    // EIGHT SLOTS. The exilus slot is OUT OF SCOPE for a benchmark build
    // (user, 2026-08-04: "不考虑 exilus 槽位"), and the reason is that it does
    // not measure anything: exilus mods are handling and mobility, with no
    // single-target damage model — the optimizer already excludes them from
    // its pool for exactly that reason. It also costs a separate adapter, so
    // counting it would price a build against a resource the ranking cannot
    // see the value of.
    //
    // An exilus MOD is still legal here: the game lets one sit in a regular
    // slot, and spending a main slot on it is the submitter's business.
    //
    // AT MOST eight. Whether a build must be FULL is a board policy and not a
    // legality fact — four mods is a legal build in the game — so it lives in
    // `validate_for_board` rather than here.
    if ms.len() > MAIN_SLOTS {
        return Err(format!("{} mods, and a benchmark build has {MAIN_SLOTS}", ms.len()));
    }

    // CAPACITY, with Forma unlimited. `plan_forma` answers both halves at once:
    // whether ANY layout fits, and how many Forma the cheapest one costs.
    //
    // The exilus slot's own innate polarity is NOT in the pool: the slot is out
    // of scope, so its polarity is not a discount this build gets to spend.
    let innate: Vec<Option<crate::mods::Polarity>> =
        crate::weapons_data::innate_slots(weapon).to_vec();
    let planned: Vec<PlannedMod> = ms
        .iter()
        .map(|id| {
            let m = def(id);
            PlannedMod { base_drain: m.base_drain, polarity: m.polarity }
        })
        .collect();
    let plan = crate::mods::fit(spec.max_rank, &innate, &planned, BENCHMARK_INVESTMENT)
        .map_err(|e| format!("does not fit this weapon's capacity even with Forma: {e}"))?;

    // ARCANES: one per pool THIS WEAPON seats, and each from that pool.
    //
    // The seats used to come from `arcanes_data::slots()` — every arcane
    // DIRECTORY that exists, sorted — so seat 0 was "primary" on every weapon
    // in the roster. A secondary weapon's arcane was therefore checked against
    // the primary pool and refused: `secondary_deadhead is not an arcane Dual
    // Toxocyst can seat`. Two real Dual Toxocyst submissions were thrown away
    // by it before anyone noticed (2026-08-05), and nothing noticed because the
    // scorer counted refusals without printing them.
    //
    // `weapons_data::arcane_pools` is the same answer the page shows, which is
    // the point of having moved it into the engine.
    let seats: Vec<&str> = crate::weapons_data::arcane_pools(weapon);
    if arcanes.iter().filter(|a| a.as_str() != "none" && !a.is_empty()).count() > seats.len() {
        return Err(format!(
            "{} arcanes, and {} seats {}",
            arcanes.len(),
            spec.name,
            seats.len()
        ));
    }
    let mut arcs = Vec::new();
    for (i, a) in arcanes.iter().enumerate() {
        if a == "none" || a.is_empty() {
            arcs.push("none".to_string());
            continue;
        }
        let seat = seats.get(i).copied().unwrap_or("");
        if crate::arcanes_data::for_slot(seat, a).is_none() {
            return Err(format!("{a} is not an arcane {} can seat", spec.name));
        }
        arcs.push(a.clone());
    }

    Ok(ValidBuild {
        weapon: weapon.to_string(),
        mods: ms,
        evolutions: evos,
        arcanes: arcs,
        forma: plan.cost.total(),
        drain: plan.drain,
    })
}

/// The FIGHT this build is, as one stable string.
///
/// Everything that changes the number and nothing that does not — see the
/// module header for why polarity, Forma, slot position, mod rank and order are
/// all absent. Two submissions with the same key are one board row.
pub fn identity(b: &ValidBuild) -> String {
    let set = |xs: &[String]| xs.iter().cloned().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(",");
    format!(
        "{}|{}|{}|{}",
        b.weapon,
        b.mods.join(","),
        set(&b.evolutions),
        b.arcanes.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The capacity a benchmark build is judged against, for THIS weapon —
    /// derived exactly as `validate` derives it, so a test can never assert a
    /// number the rule does not use.
    fn cap_of(weapon: &str) -> u32 {
        let spec = crate::weapons_data::spec(weapon).expect("weapon");
        crate::mods::capacity(
            crate::mods::rank_after(spec.max_rank, crate::mods::forma_to_max_rank(spec.max_rank)),
            true,
        )
    }

    fn v(x: &[&str]) -> Vec<String> {
        x.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_legal_build_passes_and_reports_what_it_costs() {
        let b = validate(
            "boar_prime",
            &v(&["primed_point_blank", "hells_chamber", "blunderbuss", "primed_ravage"]),
            &[],
            &v(&["none"]),
        )
        .expect("four ordinary shotgun mods are legal");
        assert_eq!(b.mods.len(), 4);
        assert!(b.drain <= cap_of("boar_prime"));
        // Forma is a COST, not a legality term: it is reported, never rejected.
        assert!(b.forma <= 4, "four mods cannot need more than four Forma");
    }

    #[test]
    fn the_arsenal_rules_are_the_ones_enforced() {
        // A mod from another class.
        assert!(validate("boar_prime", &v(&["serration"]), &[], &[]).is_err());
        // Two of one family.
        let e = validate("boar_prime", &v(&["hells_chamber", "galvanized_hell"]), &[], &[])
            .unwrap_err();
        assert!(e.contains("family"), "{e}");
        // NINE mods — refused even though one of them is exilus-eligible,
        // because a benchmark build has eight slots and the exilus slot is out
        // of scope (it measures nothing a damage ranking can read).
        let nine = v(&["primed_point_blank", "hells_chamber", "blunderbuss", "primed_ravage",
                       "scattering_inferno", "toxic_barrage", "galvanized_savvy", "vicious_spread",
                       "lock_and_load"]);
        assert!(crate::mods_data::pool_for_weapon("boar_prime")
            .iter().any(|m| m.id == "lock_and_load" && m.exilus),
            "the ninth is exilus-eligible, so this tests the SLOT rule");
        let e = validate("boar_prime", &nine, &[], &[]).unwrap_err();
        assert!(e.contains("8"), "{e}");
        // An arcane the weapon cannot seat.
        assert!(validate("boar_prime", &[], &[], &v(&["secondary_enervate"])).is_err());
    }

    /// CAPACITY IS A LIVE CONSTRAINT at eight slots — asked of the data rather
    /// than assumed, and the answer was not the one I expected.
    ///
    /// Matching a polarity halves a mod's drain (rounded UP), so eight mods
    /// cost the sum of their halves and Boar Prime's priciest eight come to
    /// exactly 60 — which is why an earlier version of this test, built on
    /// that one weapon, concluded the check was slack. Across the roster the
    /// worst case is 64, so it refuses real builds. Both directions are
    /// asserted per weapon: the planner's verdict has to agree with the
    /// arithmetic, whichever way it falls.
    #[test]
    fn capacity_is_a_live_constraint_at_eight_slots() {
        let mut worst = 0u32;
        for w in crate::weapons_data::all() {
            let mut pool = crate::mods_data::pool_for_weapon(&w.id);
            pool.sort_by_key(|m| std::cmp::Reverse(m.base_drain));
            let (mut fams, mut picked): (Vec<&str>, Vec<String>) = (Vec::new(), Vec::new());
            for m in &pool {
                if picked.len() == MAIN_SLOTS {
                    break;
                }
                if let Some(f) = m.family {
                    if fams.contains(&f) {
                        continue;
                    }
                    fams.push(f);
                }
                picked.push(m.id.to_string());
            }
            if picked.len() < MAIN_SLOTS {
                continue;
            }
            let cap = cap_of(&w.id);
            let cost: u32 = picked
                .iter()
                .map(|id| pool.iter().find(|m| m.id == id.as_str()).unwrap().base_drain.div_ceil(2))
                .sum();
            worst = worst.max(cost);
            // Whatever the number, the VERDICT and the cost must agree: the
            // planner is the authority, not this arithmetic.
            let got = validate(&w.id, &picked, &[], &[]);
            match got {
                Ok(v) => assert!(
                    cost <= cap && v.drain <= cap,
                    "{}: accepted at {} but the halves come to {cost}", w.id, v.drain
                ),
                Err(e) => assert!(
                    cost > cap && e.contains("capacity"),
                    "{}: refused ({e}) though the halves come to {cost}", w.id
                ),
            }
        }
        // The rule is not dead code: somewhere in the roster, eight mods do not
        // fit however much Forma you own.
        assert!(
            worst > 60,
            "capacity never binds anywhere ({worst}) — this check would be decoration"
        );
    }

    /// THE REPRESENTATIVE: what differs is kept, what does not is not.
    ///
    /// Measured on the Torid (2026-08-04, six mods): three elementals in slots
    /// 1-3, the same three in 4-6, and the same three interleaved with the
    /// non-elementals all score an IDENTICAL 146,707.582 DPS, as does
    /// reshuffling the non-elementals among themselves. So position is not the
    /// build — only the elementals' order relative to EACH OTHER is.
    #[test]
    fn one_representative_per_build_elements_last_in_their_own_order() {
        let mods = |x: &[&str]| canonical_mods("torid", &v(x));
        let want = mods(&["split_chamber", "serration", "point_strike", "hellfire", "cryo_rounds", "infected_clip"]);

        // Elements moved, interleaved, and the rest reshuffled: one answer.
        for spelling in [
            &["hellfire", "cryo_rounds", "infected_clip", "serration", "split_chamber", "point_strike"][..],
            &["serration", "hellfire", "split_chamber", "cryo_rounds", "point_strike", "infected_clip"][..],
            &["point_strike", "split_chamber", "serration", "hellfire", "cryo_rounds", "infected_clip"][..],
        ] {
            assert_eq!(mods(spelling), want, "{spelling:?}");
        }
        // The elementals are LAST and keep the order of their PAIRS. Inside a
        // pair they are sorted, because `combined_of` is symmetric and pooling
        // makes the two spellings one fight — Hellfire and Cryo Rounds are one
        // pair (Blast either way) and come back name-ordered, while Infected
        // Clip is the odd one out and stays where its pairing put it.
        assert_eq!(&want[3..], &v(&["cryo_rounds", "hellfire", "infected_clip"])[..]);
        // Ahead of them, biggest MAX-RANK drain first: Split Chamber 15,
        // Serration 14, Point Strike 9. (Asserted against the pool rather than
        // from memory — I had Serration first and the pool says otherwise.)
        assert_eq!(&want[..3], &v(&["split_chamber", "serration", "point_strike"])[..]);
        let pool = crate::mods_data::pool_for_weapon("torid");
        let drain = |id: &str| pool.iter().find(|m| m.id == id).unwrap().base_drain;
        assert!(drain("split_chamber") > drain("serration"));
        assert!(drain("serration") > drain("point_strike"));

        // ...and swapping two ELEMENTS is a different build, because it pairs
        // differently: Gas + Magnetic against Blast + Corrosive.
        assert_ne!(
            mods(&["hellfire", "infected_clip", "cryo_rounds", "serration"]),
            mods(&["hellfire", "cryo_rounds", "infected_clip", "serration"])
        );
    }

    /// ORDER IS PART OF THE FIGHT, because mods combine ELEMENTS in the order
    /// they are listed. This test asserted the opposite for a day, on one
    /// measurement that happened to reorder mods whose pairing did not change.
    ///
    /// The Torid says it plainly: Heat, Cold, Toxin, Electric pairs to Blast +
    /// Corrosive and scores 12,424 DPS; the same four as Heat, Toxin, Cold,
    /// Electric pairs to Gas + Magnetic and scores 46,583 (measured
    /// 2026-08-04). One row for both would have published a number belonging
    /// to neither.
    #[test]
    fn the_order_of_the_mods_is_part_of_the_identity() {
        let a = validate("torid", &v(&["hellfire", "cryo_rounds", "infected_clip", "stormbringer"]), &[], &[]).unwrap();
        let b = validate("torid", &v(&["hellfire", "infected_clip", "cryo_rounds", "stormbringer"]), &[], &[]).unwrap();
        // Normalisation may reorder INSIDE a pair and never across one: both
        // pairs come back name-ordered, and the PAIRING is untouched.
        assert_eq!(a.mods, v(&["cryo_rounds", "hellfire", "infected_clip", "stormbringer"]),
                   "sorted within each pair, pairs left alone");
        assert_ne!(identity(&a), identity(&b), "two pairings, two rows");

        // ...and a different SET is still a different identity.
        let c = validate("torid", &v(&["hellfire", "cryo_rounds"]), &[], &[]).unwrap();
        assert_ne!(identity(&a), identity(&c));
    }

    /// ...BUT SWAPPING TWO ELEMENTALS INSIDE ONE PAIR IS THE SAME BUILD.
    ///
    /// The rule above ("order is part of the fight") was right and one notch
    /// too fine. `elements::combine` pairs the sequence with `chunks_exact(2)`
    /// and combines each pair with `combined_of`, which is SYMMETRIC and pools
    /// both amounts — so the two spellings of a pair are the same damage by
    /// construction.
    ///
    /// It reached the board: the Ocucor carried two rows differing only in
    /// Frostbite and Pistol Pestilence being swapped, both scoring 6.0779
    /// (owner, 2026-08-06 — "这在我们这里应该是实质相同的build").
    #[test]
    fn swapping_two_elementals_inside_a_pair_is_one_build() {
        let one = validate("ocucor", &v(&["frostbite", "pistol_pestilence"]), &[], &[]).unwrap();
        let two = validate("ocucor", &v(&["pistol_pestilence", "frostbite"]), &[], &[]).unwrap();
        assert_eq!(identity(&one), identity(&two), "Cold + Toxin is Viral either way");

        // ...and the guard against over-collapsing: with FOUR elementals, the
        // same swap ACROSS a pair boundary re-pairs everything and must stay
        // two builds. Cold+Toxin / Heat+Electricity against Cold+Heat /
        // Toxin+Electricity — Viral+Radiation against Blast+Corrosive.
        let split = |x: &[&str]| identity(&validate("ocucor", &v(x), &[], &[]).unwrap());
        assert_ne!(
            split(&["frostbite", "pistol_pestilence", "heated_charge", "convulsion"]),
            split(&["frostbite", "heated_charge", "pistol_pestilence", "convulsion"]),
            "moving an elemental across a pair boundary is a different fight"
        );
    }

    /// THE PAIRS THEMSELVES HAVE A FIXED ORDER — the wiki's table order, by the
    /// element each pair MAKES (owner, 2026-08-06: "我们参考wiki的排序").
    ///
    /// Safe for the same reason the within-pair sort is: `combine` walks the
    /// chunks and ADDS each secondary into a damage vector, so the pairs'
    /// order is not part of the fight. MEASURED on the Torid rather than
    /// argued — the same four mods with the two pairs swapped give 12,773.473
    /// DPS both ways, while moving one element ACROSS a pair boundary gives
    /// 49,681.947 because it re-pairs everything.
    #[test]
    fn the_pairs_are_ordered_by_the_element_they_make() {
        let id = |x: &[&str]| identity(&validate("torid", &v(x), &[], &[]).unwrap());
        // Blast (Heat+Cold) and Corrosive (Toxin+Electric), submitted both ways
        // round. One fight, so one row.
        assert_eq!(
            id(&["hellfire", "cryo_rounds", "infected_clip", "stormbringer"]),
            id(&["infected_clip", "stormbringer", "hellfire", "cryo_rounds"]),
            "the same two pairs in either order are the same damage vector"
        );
        // ...and the guard, which is the measurement above: crossing a pair
        // boundary re-pairs and must stay a different build.
        assert_ne!(
            id(&["hellfire", "cryo_rounds", "infected_clip", "stormbringer"]),
            id(&["hellfire", "infected_clip", "cryo_rounds", "stormbringer"]),
        );

        // The ORDER is the table's, not whoever submitted first: Blast (index
        // 7) before Corrosive (8), whichever way round it arrived.
        let want = v(&["cryo_rounds", "hellfire", "infected_clip", "stormbringer"]);
        for spelling in [
            &["hellfire", "cryo_rounds", "infected_clip", "stormbringer"][..],
            &["infected_clip", "stormbringer", "hellfire", "cryo_rounds"][..],
        ] {
            assert_eq!(canonical_mods("torid", &v(spelling)), want, "{spelling:?}");
        }
        assert!(
            crate::elements::wiki_order(crate::damage::DamageType::Blast)
                < crate::elements::wiki_order(crate::damage::DamageType::Corrosive),
            "the order comes from the wiki's table, so this is what it says"
        );
    }

    /// ALL THREE AXES REACH THE BUILD — mods, evolutions AND arcanes (user,
    /// 2026-08-04: "检查确定所有的包括 evo mod arcane 正常进入").
    ///
    /// Asserted through the IDENTITY rather than by reading fields back,
    /// because identity is what a board row is keyed on: if an axis did not
    /// enter, two builds differing only on that axis would collide into one
    /// row and the second submitter's build would silently become the first's.
    /// So each axis is changed alone, and each change must move the key.
    #[test]
    fn a_change_on_any_axis_is_a_different_build() {
        let mods = v(&["hellfire", "serration", "split_chamber"]);
        // Tier 1 then tier 2 — a LADDER, so the second has to be the rung
        // above the first or normalisation stops at the gap.
        let evos = v(&["torid_evo1_incarnon_form", "torid_final_fusillade"]);
        let arc = v(&["primary_deadhead"]);
        let base = validate("torid", &mods, &evos, &arc).expect("a legal torid build");
        // Everything arrived.
        assert_eq!(base.mods.len(), 3);
        assert_eq!(base.evolutions, evos, "the whole ladder prefix");
        assert_eq!(base.arcanes, arc);

        let key = identity(&base);
        let other_mods = v(&["hellfire", "serration", "point_strike"]);
        let other_evos = v(&["torid_evo1_incarnon_form", "torid_plentiful_mayhem"]);
        let other_arc = v(&["primary_merciless"]);
        for (what, b) in [
            ("mods", validate("torid", &other_mods, &evos, &arc)),
            ("evolutions", validate("torid", &mods, &other_evos, &arc)),
            ("arcanes", validate("torid", &mods, &evos, &other_arc)),
        ] {
            let b = b.unwrap_or_else(|e| panic!("{what}: {e}"));
            assert_ne!(identity(&b), key, "{what} does not reach the identity");
        }
    }

    /// The ladder is applied by TRUNCATION, so normalisation has to happen
    /// before the identity is taken — otherwise a row names a build the score
    /// does not belong to.
    #[test]
    fn an_evolution_set_is_trimmed_to_its_legal_prefix_before_it_is_identified() {
        // Tier 3 with nothing below it: the ladder opens nothing, so the whole
        // set drops rather than the build being scored with a tier-3 perk.
        let b = validate("boar_prime", &[], &v(&["boar_prime_reified_bane"]), &[]).unwrap();
        assert!(
            b.evolutions.is_empty(),
            "a tier nothing unlocked is not part of the build: {:?}",
            b.evolutions
        );
        // Filled from tier 1 up, it survives.
        let full = validate(
            "boar_prime",
            &[],
            &v(&["boar_prime_evo1_incarnon_form", "boar_prime_fortress_salvo"]),
            &[],
        )
        .unwrap();
        assert_eq!(full.evolutions.len(), 2, "{:?}", full.evolutions);
    }
    /// "FULL" IS PER WEAPON, and that is the whole point of computing it rather
    /// than writing a number down: a sentinel weapon seats no arcane and has no
    /// evolutions, so it is complete with eight mods and nothing else, while a
    /// Laetum needs five tiers and an Arch-Gun needs two arcanes.
    #[test]
    fn complete_means_something_different_on_every_weapon() {
        use crate::weapons_data::arcane_pools;
        // The shapes the rule has to cover, straight from the data.
        assert_eq!(arcane_pools("larkspur_prime").len(), 2, "an Arch-Gun seats two");
        assert_eq!(arcane_pools("boar_prime").len(), 1);
        assert_eq!(arcane_pools("verglas_prime").len(), 0, "a sentinel weapon seats none");
        assert_eq!(crate::evolutions_data::tier_count("laetum"), 5);
        assert_eq!(crate::evolutions_data::tier_count("boar_prime"), 4);
        assert_eq!(crate::evolutions_data::tier_count("gotva_prime"), 0, "no evolutions");

        // A full Gotva Prime: eight mods, no tiers to fill, one arcane seat.
        let mods: Vec<String> = crate::mods_data::pool_for_weapon("gotva_prime")
            .iter()
            .filter(|m| !m.exilus)
            .take(MAIN_SLOTS)
            .map(|m| m.id.to_string())
            .collect();
        assert_eq!(mods.len(), MAIN_SLOTS, "the pool can fill a build");
        let arc = vec!["primary_merciless".to_string()];
        let ok = validate_for_board("single_target", "gotva_prime", &mods, &[], &arc);
        assert!(ok.is_ok(), "a full rifle build is admitted: {ok:?}");

        // ...and the same build with the arcane seat empty is not.
        let none = vec!["none".to_string()];
        let err = validate_for_board("single_target", "gotva_prime", &mods, &[], &none)
            .unwrap_err();
        assert!(err.contains("arcane"), "the reason names the axis: {err}");

        // One mod short is refused on the MOD axis, not the arcane one.
        let short = &mods[..MAIN_SLOTS - 1];
        let err = validate_for_board("single_target", "gotva_prime", short, &[], &arc)
            .unwrap_err();
        assert!(err.contains("main slots"), "{err}");

        // An INCARNON weapon with no evolutions is the base form, not a weak
        // build of the same gun — refused on the evolution axis.
        let bp: Vec<String> = crate::mods_data::pool_for_weapon("boar_prime")
            .iter()
            .filter(|m| !m.exilus)
            .take(MAIN_SLOTS)
            .map(|m| m.id.to_string())
            .collect();
        let err = validate_for_board("single_target", "boar_prime", &bp, &[],
                                     &["primary_crux".to_string()])
            .unwrap_err();
        assert!(err.contains("evolution"), "{err}");
    }

    /// An unknown benchmark admits nothing: a number published against a ruler
    /// that does not exist has no standard behind it.
    #[test]
    fn an_unknown_benchmark_admits_nothing() {
        let e = validate_for_board("no_such_ruler", "gotva_prime", &[], &[], &[]).unwrap_err();
        assert!(e.contains("unknown benchmark"), "{e}");
    }

    /// A SECONDARY WEAPON'S ARCANE IS A SECONDARY ARCANE. The seats used to be
    /// every arcane DIRECTORY that exists, sorted, so seat 0 was "primary" on
    /// every weapon and a Dual Toxocyst build carrying `secondary_deadhead` was
    /// refused for seating an arcane it seats. Two real submissions were thrown
    /// away by it (2026-08-05).
    #[test]
    fn a_weapon_seats_its_own_slots_arcanes() {
        // Eight mods from EIGHT DIFFERENT FAMILIES — two of one family is its
        // own refusal, and this test is not about that one.
        let mods = |w: &str| -> Vec<String> {
            let mut fams: Vec<&str> = Vec::new();
            let mut out = Vec::new();
            for m in crate::mods_data::pool_for_weapon(w).iter().filter(|m| !m.exilus) {
                if out.len() == MAIN_SLOTS {
                    break;
                }
                if let Some(f) = m.family {
                    if fams.contains(&f) {
                        continue;
                    }
                    fams.push(f);
                }
                out.push(m.id.to_string());
            }
            out
        };
        // A SECONDARY, with a secondary arcane and its full evolution ladder.
        let evos: Vec<String> = (1..=crate::evolutions_data::tier_count("dual_toxocyst"))
            .filter_map(|t| {
                crate::evolutions_data::options("dual_toxocyst", t)
                    .first()
                    .map(|e| e.id.to_string())
            })
            .collect();
        let ok = validate_for_board(
            "single_target", "dual_toxocyst", &mods("dual_toxocyst"), &evos,
            &["secondary_deadhead".to_string()],
        );
        assert!(ok.is_ok(), "a secondary seats a secondary arcane: {ok:?}");

        // ...and it does NOT seat a primary one.
        let e = validate_for_board(
            "single_target", "dual_toxocyst", &mods("dual_toxocyst"), &evos,
            &["primary_deadhead".to_string()],
        )
        .unwrap_err();
        assert!(e.contains("not an arcane"), "{e}");

        // A sentinel weapon seats none, so any arcane at all is refused.
        let e = validate_for_board(
            "single_target", "verglas_prime", &mods("verglas_prime"), &[],
            &["primary_crux".to_string()],
        )
        .unwrap_err();
        assert!(e.contains("seats 0") || e.contains("not an arcane"), "{e}");
    }

}
