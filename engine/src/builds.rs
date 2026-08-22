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
//! ...but what counts is the PAIRING, not the positions. `elements::combine`
//! chunks the POOLED element list and combines each chunk with `combined_of`,
//! which is symmetric and pools both amounts; the secondaries are then ADDED
//! into a vector. So neither the order inside a pair nor the order of the pairs
//! among themselves is part of the fight — only which elements share a chunk,
//! and which one trails.
//!
//! Both of those were treated as significant, and it put the Ocucor on the
//! board twice at the same score with Frostbite and Pistol Pestilence swapped
//! (owner, 2026-08-06). The rule was right and one notch too fine.
//!
//! CANONICALISE ON THE POOLED SEQUENCE, never on mod slots. Two mods of one
//! element are ONE entry to the engine, so every position after them shifts —
//! a canonicaliser that chunks the MOD list agrees with the engine until a
//! build carries a duplicate element and then quietly scores a different
//! fight. That shipped and was reverted (5669040): Primed Heated Charge and
//! Scorch pooled, and Viral + Heat was published as Blast + Toxin.
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

/// ONE AXIS OF A BUILD. See [`BUILD_AXES`].
pub struct BuildAxis {
    /// The stable id, and the only name shared across the product. Never a
    /// wire field and never a display string: each protocol spells its own
    /// half — `arcane` on a request, `arcanes` on a board record, `arcaneRank`
    /// in the page's state — and a spelling is a detail of the protocol that
    /// carries it, not a fact about builds.
    pub id: &'static str,
    /// Where a SIMULATE REQUEST carries it, which is the one spelling the
    /// engine itself answers to.
    pub request_field: &'static str,
    /// Does the BOARD keep it? A ruler fixes some of these rather than
    /// recording them — every row is scored at full mod rank and at the
    /// valence roll's ceiling — and a riven is an item that exists on one
    /// machine, so it can never identify a public row.
    pub on_board: bool,
}

/// WHAT A BUILD CONSISTS OF, declared once for the whole product (owner,
/// 2026-08-16).
///
/// A build travels through eight representations — the page's live state, a
/// stored preset, a simulate request, an optimize scope, a ranked row, a board
/// submission, a board record, a share link — and until this list existed, each
/// of them held its own hand-written answer to "which axes are there". Adding
/// one meant editing every copy, and the copy nobody edited dropped that axis
/// in silence, because a missing axis and a defaulted axis are the same absence
/// on the wire.
///
/// It happened four times: `mode` lost from the board submission (2026-08-09),
/// `valence` from the worker's table (2026-08-14), both from the share tuple
/// (2026-08-15), and `valence` from the optimizer's "+ add" (2026-08-16) — the
/// last one measured by a player, who was shown 26 KPM on a ranking and 15 in
/// the simulator for what he had been told was the same build.
///
/// This does not unify the SPELLINGS, which are protocol details and would cost
/// a migration of every stored preset to change. It unifies the LIST: it is
/// served at `/api/meta.build_axes`, every surface declares which axis each of
/// its own fields carries, and `scripts/check_build_axes.mjs` asserts the
/// coverage is total. A surface that has never heard of a new axis then fails
/// on the day the axis is added, instead of quietly halving somebody's damage.
///
/// The other half of the guarantee is not a list at all and cannot go stale:
/// every ranked row carries a simulate request that reproduces it, and
/// `scripts/check_opt_replay.mjs` asserts the number comes back. A list can be
/// forgotten; an answer that has to match cannot.
pub const BUILD_AXES: &[BuildAxis] = &[
    BuildAxis { id: "mods", request_field: "mods", on_board: true },
    BuildAxis { id: "evolutions", request_field: "evolutions", on_board: true },
    BuildAxis { id: "arcanes", request_field: "arcane", on_board: true },
    // NOT on the board: a ruler scores every arcane at its own maximum, the
    // same rule that scores every row fully forma'd — investment is not a
    // choice, so it is not part of what a row states.
    BuildAxis { id: "arcane_ranks", request_field: "arcane_rank", on_board: false },
    BuildAxis { id: "mode", request_field: "mode", on_board: true },
    // A MODULAR WEAPON'S PARTS. On the board because the assembly IS the stat
    // line — two assemblies of one chamber are two different weapons in every
    // number a row states — which is the same reason `mode` is there and the
    // opposite of `arcane_ranks`, where a ruler fixes the answer for everyone.
    BuildAxis { id: "assembly", request_field: "assembly", on_board: true },
    // The ELEMENT only on the board, for the same reason: the roll is scored at
    // its ceiling, which every player can Valence-fuse to.
    BuildAxis { id: "valence", request_field: "valence_element", on_board: true },
    // A RIVEN IS A MOD, and rides in `mods` as an id — but the item itself
    // exists only on the machine that rolled it, so the request carries its
    // definition too and no public record can ever hold one.
    BuildAxis { id: "rivens", request_field: "rivens", on_board: false },
];

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
    /// An ADVERSARY weapon's VALENCE ELEMENT — the progenitor bonus this copy
    /// came out of its Lich with. Empty on every weapon that has no valence.
    ///
    /// Part of the build for the same reason an evolution is (owner,
    /// 2026-08-13): a different element is a different weapon, not a
    /// weaker one. The PERCENTAGE is not here — the board scores every
    /// row at the roll's maximum, which every player can reach.
    pub valence: String,
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
/// So: the PAIRING of the elemental mods is the build, and nothing else about
/// position is — which is their relative order between pairs, but not inside
/// one (see the loop below). `primary_element` is the same predicate `resolve` uses to
/// walk the hierarchy, so this cannot drift from what the sim does.
///
/// The rest are ordered biggest-drain first, then by DE's own English name
/// (owner, 2026-08-04) — a rule chosen so the representative is stable and
/// readable, not because the engine cares.
pub fn canonical_mods(weapon: &str, mods: &[String]) -> Vec<String> {
    let pool = crate::mods_data::pool_for_weapon(weapon);
    let def = |id: &String| pool.iter().find(|m| m.id == id.as_str());
    let (mut plain, elemental): (Vec<&String>, Vec<&String>) = mods
        .iter()
        .partition(|id| def(id).is_none_or(|m| m.primary_element().is_none()));
    // Biggest drain first, then DE's own English name — stable and readable.
    let rank = |a: &&String, b: &&String| {
        let (da, db) = (def(a).map_or(0, |m| m.base_drain), def(b).map_or(0, |m| m.base_drain));
        db.cmp(&da)
            .then_with(|| def(a).map_or("", |m| m.name).cmp(def(b).map_or("", |m| m.name)))
    };
    plain.sort_by(rank);

    // THE ELEMENTALS ARE CANONICALISED ON THE **POOLED ELEMENT** SEQUENCE, not
    // on their own positions — and that distinction is the whole of it.
    //
    // `elements::combine` does not chunk the mod list. It chunks the list of
    // ELEMENTS after `ElementalInput::push` has merged duplicates, so two Heat
    // mods are ONE Heat entry and everything after them shifts up by one. A
    // rule written in terms of mod positions agrees with the engine right up
    // until a build carries the same element twice, and then silently scores a
    // different fight (that bug shipped: 5669040, reverted — Primed Heated
    // Charge and Scorch pooled, and Viral + Heat was published as Blast +
    // Toxin, 4.7511 down to 0.1293).
    //
    // So: pool first, decide the canonical ELEMENT order, then lay the mods
    // out to match it.
    let element_of = |x: &&String| def(x).and_then(|m| m.primary_element());
    // Distinct elements in first-appearance order — exactly what `push` builds.
    let mut seq: Vec<crate::damage::DamageType> = Vec::new();
    for e in elemental.iter().filter_map(element_of) {
        if !seq.contains(&e) {
            seq.push(e);
        }
    }
    // Two freedoms, both provably free, and one hard constraint.
    //
    // FREE: the order inside a pair (`combined_of` is symmetric and pools both
    // amounts) and the order of the pairs among themselves (`combine` ADDS each
    // secondary into a vector). Measured on the Torid: the same four mods with
    // their two pairs swapped give 12,773.473 DPS either way.
    //
    // FIXED: which elements share a pair, and which one trails. Moving an
    // element across a boundary re-pairs everything after it — 12,424 against
    // 46,583 on the same weapon — so the PARTITION is never touched.
    let odd = seq.len() % 2;
    let tail: Vec<crate::damage::DamageType> = seq.split_off(seq.len() - odd);
    let mut pairs: Vec<[crate::damage::DamageType; 2]> =
        seq.chunks(2).map(|c| [c[0], c[1]]).collect();
    for p in &mut pairs {
        p.sort_by_key(|&t| crate::elements::wiki_order(t));
    }
    // By the element each pair MAKES, in the wiki's own table order (owner,
    // 2026-08-06). Pooling guarantees the two are distinct, so `combined_of`
    // always answers here — unlike the reverted version, which asked it about
    // two mods rather than two elements and had to invent a fallback.
    pairs.sort_by_key(|p| {
        crate::elements::wiki_order(
            crate::elements::combined_of(p[0], p[1]).expect("pooled elements are distinct"),
        )
    });
    let canonical_elements: Vec<crate::damage::DamageType> =
        pairs.into_iter().flatten().chain(tail).collect();
    // Lay the mods out in that element order, same-element mods together (they
    // pool anyway, so their order among themselves changes nothing) and ranked
    // by the usual rule so the representative is stable.
    let elemental: Vec<&String> = canonical_elements
        .iter()
        .flat_map(|&want| {
            let mut group: Vec<&String> = elemental
                .iter()
                .copied()
                .filter(|m| element_of(m) == Some(want))
                .collect();
            group.sort_by(rank);
            group
        })
        .collect();
    plain.into_iter().chain(elemental).cloned().collect()
}

/// One way a build's elements can PAIR, and what it makes.
///
/// The optimizer searches this dimension (`subset_candidates` permutes the
/// distinct primary elements and dedups on the resulting vector); the quick
/// calc has to report it, because a mod's marginal value is measured under a
/// pairing and a chip that named none would be unattributable.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementOrder {
    /// A representative mod order that produces this pairing — what a caller
    /// hands to `simulate` to measure it.
    pub mods: Vec<String>,
    /// The COMBINED elements it makes, in the wiki's table order.
    pub combined: Vec<crate::damage::DamageType>,
    /// Primary elements left uncombined (the trailing odd one, and any the
    /// weapon carries that nothing paired with).
    pub leftover: Vec<crate::damage::DamageType>,
}

/// The permutation cap. 6 distinct elements is 720 resolves — already more
/// than any real build (there are only six primaries), and the guard is here
/// so a future element cannot turn this into a hang.
const MAX_ELEMENT_PERMUTATIONS: usize = 720;

/// Every DISTINCT pairing this mod set can produce on this weapon ENTRY.
///
/// `weapon` is the entry that FIRES — for a cycling weapon that is the
/// Incarnon half, which is what `parse_fight` resolves and what the optimizer
/// dedups on. Passing the base entry of a transform weapon would label the
/// wrong form's elements: the Burston Prime's base damage is IPS and its
/// Incarnon form's is Heat, so the two do not even have the same innate.
///
/// It RESOLVES rather than reasoning about positions, which is the whole point:
/// the innate rules (an innate element trails, unless a mod already placed that
/// element and it pools FORWARD onto the mod's position — `elements::combine`
/// rules 2 and 3) live in one place and this is not a second copy of them. On
/// the Burston Prime's Incarnon form the base damage is Heat, so a build of
/// Cold + Toxin is already Viral + Heat before a Heat mod is equipped — no
/// rule written over mod ids could have known that.
///
/// Deduped on the resolved damage VECTOR, the same key the optimizer uses.
/// Orders that resolve alike are one build, and a set with fewer than two
/// distinct elements yields exactly one entry — never zero, so a caller always
/// has something to measure.
pub fn element_orders(weapon: &str, mods: &[String], evolutions: &[String]) -> Vec<ElementOrder> {
    let pool = crate::mods_data::pool_for_weapon(weapon);
    let def = |id: &String| pool.iter().find(|m| m.id == id.as_str());
    let evo_refs: Vec<&str> = evolutions.iter().map(String::as_str).collect();
    let base = crate::loadout::WeaponBase::from_data(weapon, true, &evo_refs);

    // Distinct MOD elements in first-appearance order. Same-element mods pool
    // (`ElementalInput::push` merges them), so they are one entry and move
    // together — the distinction that broke 5669040.
    let mut seq: Vec<crate::damage::DamageType> = Vec::new();
    for e in mods.iter().filter_map(|m| def(m).and_then(|d| d.primary_element())) {
        if !seq.contains(&e) {
            seq.push(e);
        }
    }
    let mut orders = Vec::new();
    if seq.len() <= 1 || (1..=seq.len()).product::<usize>() > MAX_ELEMENT_PERMUTATIONS {
        orders.push(seq.clone());
    } else {
        permutations(&seq, &mut Vec::new(), &mut orders);
    }

    let mut out: Vec<ElementOrder> = Vec::new();
    let mut seen: Vec<Vec<(crate::damage::DamageType, i64)>> = Vec::new();
    for order in &orders {
        // Lay the mods out in this element order: elementals grouped by the
        // chosen element, then everything order-free after.
        let mut laid: Vec<String> = Vec::new();
        for &t in order {
            laid.extend(
                mods.iter()
                    .filter(|m| def(m).and_then(|d| d.primary_element()) == Some(t))
                    .cloned(),
            );
        }
        laid.extend(
            mods.iter()
                .filter(|m| def(m).is_none_or(|d| d.primary_element().is_none()))
                .cloned(),
        );
        let refs: Vec<&crate::loadout::ModDef> = laid.iter().filter_map(&def).collect();
        let panel = crate::loadout::resolve(&base, &refs, crate::loadout::StackPolicy::AssumedMax);
        let key: Vec<(crate::damage::DamageType, i64)> = panel
            .damage
            .iter_nonzero()
            .map(|(t, v)| (t, (v * 1e6).round() as i64))
            .collect();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        // The LABEL is read off the resolved vector rather than off the order,
        // so it states what the fight actually contains — innate included.
        let mut combined: Vec<crate::damage::DamageType> = Vec::new();
        let mut leftover: Vec<crate::damage::DamageType> = Vec::new();
        for (t, _) in panel.damage.iter_nonzero() {
            if t.is_secondary_element() {
                combined.push(t);
            } else if t.is_primary_element() {
                leftover.push(t);
            }
        }
        combined.sort_by_key(|&t| crate::elements::wiki_order(t));
        leftover.sort_by_key(|&t| crate::elements::wiki_order(t));
        out.push(ElementOrder { mods: laid, combined, leftover });
    }
    out
}

/// All orderings of `rest`, appended to `acc`. Mirrors the optimizer's own.
fn permutations(
    rest: &[crate::damage::DamageType],
    acc: &mut Vec<crate::damage::DamageType>,
    out: &mut Vec<Vec<crate::damage::DamageType>>,
) {
    if rest.is_empty() {
        out.push(acc.clone());
        return;
    }
    for (i, &t) in rest.iter().enumerate() {
        let mut r = rest.to_vec();
        r.remove(i);
        acc.push(t);
        permutations(&r, acc, out);
        acc.pop();
    }
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
    let multishot = canonical_mods(weapon, &kept);

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
    (multishot, evos)
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
    valence: &str,
) -> Result<ValidBuild, String> {
    let b = validate(weapon, mods, evolutions, arcanes, valence)?;
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

    // NO VALENCE CLAUSE HERE. It used to ask, gated on `requires_full`, and a
    // ruler has nothing to have an opinion about: an adversary weapon with no
    // progenitor element is not a build this board declines, it is not a build.
    // `validate` refuses it for every caller, which is where a legality rule
    // belongs (owner, 2026-08-14).

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
    valence: &str,
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
    let (multishot, evos) = normalize(weapon, mods, evolutions);
    if multishot.len() != mods.len() {
        // Loud, because a silently dropped mod is a build the submitter did not
        // send being scored under their name.
        return Err(format!(
            "{} of {} mods are not in {}'s pool",
            mods.len() - multishot.len(),
            mods.len(),
            spec.name
        ));
    }
    let pool = crate::mods_data::pool_for_weapon(weapon);
    let def = |id: &str| pool.iter().find(|m| m.id == id).expect("normalised into the pool");

    // FAMILIES. Two mods of one family cannot be equipped together.
    let mut fams: Vec<&str> = multishot.iter().filter_map(|id| def(id).family).collect();
    fams.sort_unstable();
    for w in fams.windows(2) {
        if w[0] == w[1] {
            return Err(format!("two mods of the {} family", w[0]));
        }
    }

    // EIGHT SLOTS. The exilus slot is OUT OF SCOPE for a benchmark build
    // (user, 2026-08-04), and the reason is that it does not measure
    // anything: exilus mods are handling and mobility, with no single-target
    // damage model — the optimizer already excludes them from its pool for
    // exactly that reason. It also costs a separate adapter, so
    // counting it would price a build against a resource the ranking cannot
    // see the value of.
    //
    // An exilus MOD is still legal here: the game lets one sit in a regular
    // slot, and spending a main slot on it is the submitter's business.
    //
    // AT MOST eight. Whether a build must be FULL is a board policy and not a
    // legality fact — four mods is a legal build in the game — so it lives in
    // `validate_for_board` rather than here.
    if multishot.len() > MAIN_SLOTS {
        return Err(format!("{} mods, and a benchmark build has {MAIN_SLOTS}", multishot.len()));
    }

    // CAPACITY, with Forma unlimited. `plan_forma` answers both halves at once:
    // whether ANY layout fits, and how many Forma the cheapest one costs.
    //
    // NINE POLARITIES FOR EIGHT SLOTS, and the exilus one is in the pool even
    // though the exilus SLOT is out of scope.
    //
    // A POLARITY BELONGS TO THE WEAPON, NOT TO THE SLOT IT SITS ON (owner,
    // 2026-08-16). It can be swapped with another slot's without changing what
    // either slot IS — the exilus slot stays exilus — so a build with no exilus
    // mod at all can still spend that polarity: swap it onto a main slot, and
    // the exilus slot carries whatever came back and sits empty.
    //
    // This line read the other way and said "the slot is out of scope, so its
    // polarity is not a discount this build gets to spend". The `so` was the
    // error: it assumed the polarity was attached to the slot. And the board
    // already assumes the Exilus adapter is installed (docs/INVESTMENT.md), so
    // the slot exists and its polarity is reachable.
    //
    // It over-charged 699 of the 928 stored rows by one Forma each — three
    // quarters of the board, the Torid alone 95 of them.
    let mut innate: Vec<Option<crate::mods::Polarity>> =
        crate::weapons_data::innate_slots(weapon).to_vec();
    innate.push(crate::weapons_data::exilus_polarity(weapon));
    let planned: Vec<PlannedMod> = multishot
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

    // THE VALENCE ELEMENT, checked against the weapon's own spec in both
    // directions: an adversary weapon may only take one of ITS progenitor
    // elements, and an ordinary weapon may not take one at all. A silent drop
    // would let a submission claim a bonus the game never hands out.
    let val = match crate::weapons_data::valence_of(weapon) {
        Some(s) => {
            if valence.is_empty() {
                // AND IT IS MANDATORY (owner, 2026-08-14). Every copy of an
                // adversary weapon comes out of a Lich carrying an element, so
                // a build with none is not a weaker build of that weapon — it
                // is a weapon nobody has. It used to be accepted here and
                // refused one layer up, by the board and only when the ruler
                // asked; there is nothing for the ruler to have an opinion
                // about, so the rule moved down to where legality lives.
                return Err(format!(
                    "{} has no Valence element, and every copy of it comes out of a Lich with one ({})",
                    spec.name,
                    s.elements.join(", ")
                ));
            } else if s.elements.iter().any(|e| e == valence) {
                valence.to_string()
            } else {
                return Err(format!(
                    "{valence} is not a progenitor element of {} ({})",
                    spec.name,
                    s.elements.join(", ")
                ));
            }
        }
        None if valence.is_empty() => String::new(),
        None => {
            return Err(format!("{} has no Valence bonus to set", spec.name));
        }
    };

    Ok(ValidBuild {
        weapon: weapon.to_string(),
        mods: multishot,
        evolutions: evos,
        arcanes: arcs,
        forma: plan.cost.total(),
        drain: plan.drain,
        valence: val,
    })
}

/// The FIGHT this build is, as one stable string.
///
/// Everything that changes the number and nothing that does not — see the
/// module header for why polarity, Forma, slot position, mod rank and order are
/// all absent. Two submissions with the same key are one board row.
pub fn identity(b: &ValidBuild) -> String {
    let set = |xs: &[String]| xs.iter().cloned().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(",");
    // THE VALENCE IS PART OF THE IDENTITY, and it has to be: two Kuva Nukors
    // differing only in progenitor element are two builds with two scores, and
    // an identity that could not tell them apart would file the second under
    // the first's number. Appended rather than inserted, so every identity
    // already computed for an ordinary weapon is unchanged — it ends in `|`
    // and nothing else moved.
    format!(
        "{}|{}|{}|{}|{}",
        b.weapon,
        b.mods.join(","),
        set(&b.evolutions),
        b.arcanes.join(","),
        b.valence
    )
}

/// THE CARD'S SIGN IS NOT THE BUILD'S DIRECTION — the case the whole design
/// rests on, measured through the simulator rather than argued.
///
/// Three weapons pay *"50% chance to deal +2000% damage on non-critical hits"*
/// in their Incarnon form (Felarx, Laetum, Phenmor). On those, critical chance
/// is a LIABILITY — a Laetum Incarnon crit is worth x2.2 where a non-crit is
/// worth `0.5 x 21 + 0.5 x 1 = 11` — so a riven whose MALUS is critical chance
/// wants that malus as DEEP as it goes, and the same shape on an ordinary
/// weapon wants it as shallow as it goes.
///
/// `perfect` is handed the fight and asked; it is never told which way is up.
///
/// A MALUS'S ROLL SCALES ITS MAGNITUDE, NOT ITS VALUE, and that caught the
/// first version of this test: `ROLL_MAX` on a malus is the DEEPEST one, so
/// "the top of the band" and "the better stat" are opposites there. The band
/// ends are named by their roll below for exactly that reason — `deep` and
/// `shallow` are a reading of the number and belong in prose, not in a
/// variable somebody has to get right twice.
#[cfg(test)]
mod riven_perfection_tests {
    use crate::rivens_data::{perfect, RivenShape, RivenSpec, ROLL_MAX, ROLL_MIN};

    /// THE WHOLE LADDER, because Devouring Attrition is TIER 5 and a tier is
    /// only open when the ones below it are filled — a set with a gap is
    /// trimmed to its longest legal prefix, so naming tier 1 and tier 5 alone
    /// applies neither.
    const ATTRITION: &[&str] = &[
        "laetum_evo1_incarnon_form",
        "laetum_rapid_wrath",
        "laetum_awakened_readiness",
        "laetum_incarnon_efficiency",
        "laetum_devouring_attrition",
    ];

    /// A riven whose MALUS is critical chance, and nothing else that could
    /// confuse the reading.
    fn negative_crit() -> RivenShape {
        RivenShape {
            bonuses: vec!["damage".into(), "multishot".into()],
            malus: Some("critical_chance".into()),
        }
    }

    /// That shape with every bonus at its ceiling and the malus at `malus_roll`.
    fn at(shape: &RivenShape, malus_roll: f64) -> RivenSpec {
        let mut sp = perfect(shape, "pistol", |_| 0.0);
        for b in sp.bonuses.iter_mut() {
            b.roll = ROLL_MAX;
        }
        if let Some(m) = sp.malus.as_mut() {
            m.roll = malus_roll;
        }
        sp
    }

    fn fight(weapon: &str, evos: &[&str], spec: &RivenSpec) -> f64 {
        let disposition =
            crate::weapons_data::spec(weapon).and_then(|s| s.disposition).unwrap_or(1.0);
        let riven = spec.to_mod_def(
            Box::leak(format!("riven:{weapon}").into_boxed_str()), disposition);
        let base = crate::loadout::WeaponBase::from_data(weapon, true, evos);
        let tenno = crate::tenno_data::default_tenno();
        let panel = crate::loadout::resolve_for(
            &base, &[&riven], crate::loadout::StackPolicy::Emergent, tenno);
        let arena = crate::arena::Arena::training(12.0);
        let dp = crate::dummy::DummyParams::from_panel(
            &panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        crate::dummy::monte_carlo(&dp, 120, 7).mean_damage
    }

    #[test]
    fn a_negative_crit_riven_goes_the_other_way_on_a_devouring_attrition_weapon() {
        let shape = negative_crit();

        // THE LAETUM, Incarnon, with Devouring Attrition taken: the DEEPEST
        // crit malus pays MOST, because every crit is a roll that did not pay
        // 21x.
        let deep = fight("laetum_incarnon", ATTRITION, &at(&shape, ROLL_MAX));
        let shallow = fight("laetum_incarnon", ATTRITION, &at(&shape, ROLL_MIN));
        assert!(
            deep > shallow,
            "Devouring Attrition: the deepest crit malus should pay MOST ({deep} vs {shallow})"
        );

        // …AND AN ORDINARY WEAPON GOES THE OTHER WAY, which is what makes the
        // first half evidence rather than a coincidence: same shape, same stat,
        // same sign on the card, opposite end of the band.
        let deep_p = fight("laetum", &[], &at(&shape, ROLL_MAX));
        let shallow_p = fight("laetum", &[], &at(&shape, ROLL_MIN));
        assert!(
            shallow_p > deep_p,
            "without Devouring Attrition the crit malus should be SHALLOW \
             ({shallow_p} vs {deep_p})"
        );

        // AND `perfect` FINDS BOTH WITHOUT BEING TOLD — handed the fight and
        // nothing else. No per-stat table, no sign convention.
        let with = perfect(&shape, "pistol", |sp| fight("laetum_incarnon", ATTRITION, sp));
        assert_eq!(
            with.malus.as_ref().unwrap().roll,
            ROLL_MAX,
            "on Devouring Attrition the malus belongs at its deepest"
        );
        let without = perfect(&shape, "pistol", |sp| fight("laetum", &[], sp));
        assert_eq!(
            without.malus.as_ref().unwrap().roll,
            ROLL_MIN,
            "without it, at its shallowest"
        );
        // Both agree about the BONUSES, which is the uninteresting half and the
        // one a per-stat table would have got right.
        assert!(with.bonuses.iter().all(|b| b.roll == ROLL_MAX));
        assert!(without.bonuses.iter().all(|b| b.roll == ROLL_MAX));
    }
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
            &v(&["none"]), "")
        .expect("four ordinary shotgun mods are legal");
        assert_eq!(b.mods.len(), 4);
        assert!(b.drain <= cap_of("boar_prime"));
        // Forma is a COST, not a legality term: it is reported, never rejected.
        assert!(b.forma <= 4, "four mods cannot need more than four Forma");
    }

    /// A POLARITY BELONGS TO THE WEAPON, NOT TO THE SLOT — so a benchmark
    /// build spends the EXILUS slot's polarity even though the exilus SLOT is
    /// out of scope (owner, 2026-08-16).
    ///
    /// Two slots' polarities swap without changing what either slot IS, so the
    /// exilus one can be moved onto a main slot and the exilus slot left
    /// carrying whatever came back, empty. The board withheld it until this
    /// was known and over-charged 699 of its 928 stored rows by one Forma each.
    ///
    /// The Torid is the sharp case and the biggest one — 95 of those rows. Its
    /// exilus polarity is Madurai, which its two innate slots do not carry.
    #[test]
    fn a_benchmark_build_spends_the_exilus_slots_polarity() {
        let innate = crate::weapons_data::innate_slots("torid");
        let exilus = crate::weapons_data::exilus_polarity("torid")
            .expect("the Torid has an exilus polarity");
        assert!(
            !innate.iter().flatten().any(|p| *p == exilus),
            "this case only bites when the exilus polarity is not already innate"
        );

        // A pool of eight mods that all want the EXILUS polarity, so the ninth
        // is the only free match on offer and its absence costs a Forma.
        let planned: Vec<crate::mods::PlannedMod> = (0..8)
            .map(|_| crate::mods::PlannedMod { base_drain: 12, polarity: exilus })
            .collect();
        let spec = crate::weapons_data::spec("torid").unwrap();
        let with_it = {
            let mut v = innate.to_vec();
            v.push(Some(exilus));
            crate::mods::fit(spec.max_rank, &v, &planned, BENCHMARK_INVESTMENT).unwrap()
        };
        let without =
            crate::mods::fit(spec.max_rank, innate.as_ref(), &planned, BENCHMARK_INVESTMENT)
                .unwrap();
        assert_eq!(
            with_it.cost.total() + 1,
            without.cost.total(),
            "the ninth polarity is worth exactly one Forma here"
        );

        // …AND `validate` USES IT. The pool it builds is the nine, so this
        // cannot drift from the assertion above by someone editing one of them.
        let innate_used: Vec<Option<crate::mods::Polarity>> = {
            let mut v = crate::weapons_data::innate_slots("torid").to_vec();
            v.push(crate::weapons_data::exilus_polarity("torid"));
            v
        };
        assert_eq!(innate_used.len(), MAIN_SLOTS + 1);
        assert_eq!(innate_used[MAIN_SLOTS], Some(exilus));
    }

    #[test]
    fn the_arsenal_rules_are_the_ones_enforced() {
        // A mod from another class.
        assert!(validate("boar_prime", &v(&["serration"]), &[], &[], "").is_err());
        // Two of one family.
        let e = validate("boar_prime", &v(&["hells_chamber", "galvanized_hell"]), &[], &[], "")
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
        let e = validate("boar_prime", &nine, &[], &[], "").unwrap_err();
        assert!(e.contains("8"), "{e}");
        // An arcane the weapon cannot seat.
        assert!(validate("boar_prime", &[], &[], &v(&["secondary_enervate"]), "").is_err());
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
            // An adversary weapon has no legal build with no element, so the
            // sweep gives every weapon the one its own spec starts with — this
            // test is about capacity and must not trip over legality.
            let val = crate::weapons_data::valence_of(&w.id)
                .and_then(|s| s.elements.first().cloned())
                .unwrap_or_default();
            let cost: u32 = picked
                .iter()
                .map(|id| pool.iter().find(|m| m.id == id.as_str()).unwrap().base_drain.div_ceil(2))
                .sum();
            worst = worst.max(cost);
            // Whatever the number, the VERDICT and the cost must agree: the
            // planner is the authority, not this arithmetic.
            let got = validate(&w.id, &picked, &[], &[], &val);
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
        // The elementals are LAST, in canonical element order: Cold before
        // Heat inside the Blast pair, and Infected Clip is the odd one out so
        // it stays where its pairing put it — trailing.
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
        let a = validate("torid", &v(&["hellfire", "cryo_rounds", "infected_clip", "stormbringer"]), &[], &[], "").unwrap();
        let b = validate("torid", &v(&["hellfire", "infected_clip", "cryo_rounds", "stormbringer"]), &[], &[], "").unwrap();
        // Normalisation orders the ELEMENTS and never re-pairs them: Cold
        // before Heat and Electricity before Toxin inside their pairs, Blast
        // before Corrosive between them — all table order — while the pairing
        // itself (Blast + Corrosive) is exactly what arrived.
        assert_eq!(a.mods, v(&["cryo_rounds", "hellfire", "stormbringer", "infected_clip"]),
                   "canonical element order, same pairing");
        assert_ne!(identity(&a), identity(&b), "two pairings, two rows");

        // ...and a different SET is still a different identity.
        let c = validate("torid", &v(&["hellfire", "cryo_rounds"]), &[], &[], "").unwrap();
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
    /// (owner, 2026-08-06).
    #[test]
    fn swapping_two_elementals_inside_a_pair_is_one_build() {
        let one = validate("ocucor", &v(&["frostbite", "pistol_pestilence"]), &[], &[], "").unwrap();
        let two = validate("ocucor", &v(&["pistol_pestilence", "frostbite"]), &[], &[], "").unwrap();
        assert_eq!(identity(&one), identity(&two), "Cold + Toxin is Viral either way");

        // ...and the guard against over-collapsing: with FOUR elementals, the
        // same swap ACROSS a pair boundary re-pairs everything and must stay
        // two builds. Cold+Toxin / Heat+Electricity against Cold+Heat /
        // Toxin+Electricity — Viral+Radiation against Blast+Corrosive.
        let split = |x: &[&str]| identity(&validate("ocucor", &v(x), &[], &[], "").unwrap());
        assert_ne!(
            split(&["frostbite", "pistol_pestilence", "heated_charge", "convulsion"]),
            split(&["frostbite", "heated_charge", "pistol_pestilence", "convulsion"]),
            "moving an elemental across a pair boundary is a different fight"
        );
    }

    /// THE INVARIANT THAT MATTERS: canonicalising must never change the FIGHT.
    ///
    /// Every other test here compares strings, and a string test cannot tell a
    /// tidier spelling from a different build. This one resolves both orders
    /// and compares the DAMAGE VECTOR, which is the thing the board is really
    /// promising is unchanged.
    ///
    /// It exists because the string tests all passed while the board published
    /// a wrong number (5669040, reverted): the canonical order was tidy, valid,
    /// and a different fight.
    ///
    /// THE DUPLICATE-ELEMENT CASE IS THE POINT. Primed Heated Charge and Scorch
    /// are both Heat, so `ElementalInput::push` pools them and the engine sees
    /// THREE elements where the mod list has four — every position after the
    /// duplicate shifts. A canonicaliser reasoning about mod slots gets this
    /// wrong and nothing but a damage comparison says so.
    #[test]
    fn canonicalising_never_changes_the_damage() {
        let base = crate::loadout::WeaponBase::from_data("ocucor", false, &[]);
        let pool = crate::mods_data::pool_for_weapon("ocucor");
        let resolve_in_order = |ids: &[String]| {
            let refs: Vec<&crate::loadout::ModDef> = ids
                .iter()
                .filter_map(|id| pool.iter().find(|m| m.id == id.as_str()))
                .collect();
            crate::loadout::resolve(&base, &refs, crate::loadout::StackPolicy::BaseOnly).damage
        };
        for spelling in [
            // The build that broke: two Heat mods pooling behind a Cold/Toxin
            // pair. Viral + Heat, and it must stay Viral + Heat.
            &["ice_storm", "pistol_pestilence", "primed_heated_charge", "scorch"][..],
            // ...and the same four submitted every other way round. Each is
            // whatever fight it is; canonicalising must not turn it into
            // another one.
            &["scorch", "primed_heated_charge", "ice_storm", "pistol_pestilence"][..],
            &["ice_storm", "scorch", "pistol_pestilence", "primed_heated_charge"][..],
            // Two clean pairs, no duplicates.
            &["frostbite", "pistol_pestilence", "heated_charge", "convulsion"][..],
            // An odd one out, which must stay the odd one out.
            &["frostbite", "pistol_pestilence", "heated_charge"][..],
        ] {
            let submitted = v(spelling);
            let canon = canonical_mods("ocucor", &submitted);
            assert_eq!(
                resolve_in_order(&canon),
                resolve_in_order(&submitted),
                "canonicalising {spelling:?} changed the damage: {canon:?}"
            );
        }
    }

    /// ALL THREE AXES REACH THE BUILD — mods, evolutions AND arcanes (user,
    /// 2026-08-04).
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
        let base = validate("torid", &mods, &evos, &arc, "").expect("a legal torid build");
        // Everything arrived.
        assert_eq!(base.mods.len(), 3);
        assert_eq!(base.evolutions, evos, "the whole ladder prefix");
        assert_eq!(base.arcanes, arc);

        let key = identity(&base);
        let other_mods = v(&["hellfire", "serration", "point_strike"]);
        let other_evos = v(&["torid_evo1_incarnon_form", "torid_plentiful_mayhem"]);
        let other_arc = v(&["primary_merciless"]);
        for (what, b) in [
            ("mods", validate("torid", &other_mods, &evos, &arc, "")),
            ("evolutions", validate("torid", &mods, &other_evos, &arc, "")),
            ("arcanes", validate("torid", &mods, &evos, &other_arc, "")),
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
        let b = validate("boar_prime", &[], &v(&["boar_prime_reified_bane"]), &[], "").unwrap();
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
            &[], "")
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
        let ok = validate_for_board("single_target", "gotva_prime", &mods, &[], &arc, "");
        assert!(ok.is_ok(), "a full rifle build is admitted: {ok:?}");

        // ...and the same build with the arcane seat empty is not.
        let none = vec!["none".to_string()];
        let err = validate_for_board("single_target", "gotva_prime", &mods, &[], &none, "")
            .unwrap_err();
        assert!(err.contains("arcane"), "the reason names the axis: {err}");

        // One mod short is refused on the MOD axis, not the arcane one.
        let short = &mods[..MAIN_SLOTS - 1];
        let err = validate_for_board("single_target", "gotva_prime", short, &[], &arc, "")
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
                                     &["primary_crux".to_string()], "")
            .unwrap_err();
        assert!(err.contains("evolution"), "{err}");
    }

    /// AN ADVERSARY WEAPON'S VALENCE IS MANDATORY, and it is a LEGALITY rule
    /// rather than a board one (owner, 2026-08-14).
    ///
    /// Every copy in the game comes out of a Lich carrying an element, so a
    /// build with none is not a weaker build of that weapon — it is a weapon
    /// nobody has. It used to be accepted by `validate` and refused one layer
    /// up, by `validate_for_board` and only when the ruler asked for it, which
    /// left every other caller free to score a gun that does not exist.
    ///
    /// Both directions, because a rule that only ever refuses is a rule nobody
    /// can satisfy: an element the weapon rolls is admitted and survives into
    /// the identity, and one it does not roll is named in the error.
    #[test]
    fn an_adversary_weapon_has_no_build_without_an_element() {
        let mods: Vec<String> = crate::mods_data::pool_for_weapon("kuva_nukor")
            .iter()
            .filter(|m| !m.exilus)
            .take(2)
            .map(|m| m.id.to_string())
            .collect();

        let e = validate("kuva_nukor", &mods, &[], &[], "").unwrap_err();
        assert!(e.contains("Valence") && e.contains("Lich"), "{e}");

        let ok = validate("kuva_nukor", &mods, &[], &[], "heat").expect("heat is one of its seven");
        assert_eq!(ok.valence, "heat");
        assert!(identity(&ok).ends_with("|heat"), "{}", identity(&ok));

        let e = validate("kuva_nukor", &mods, &[], &[], "puncture").unwrap_err();
        assert!(e.contains("progenitor element"), "{e}");

        // ...and an ORDINARY weapon is untouched in both directions: none is
        // the only legal answer there.
        assert!(validate("torid", &v(&["serration"]), &[], &[], "").is_ok());
        assert!(validate("torid", &v(&["serration"]), &[], &[], "heat").is_err());
    }

    /// ...AND A FULL ONE IS ADMISSIBLE TO THE BOARD, which is the half a
    /// legality rule can quietly take away.
    ///
    /// The ruler asks for `valence: full` and used to be the only thing that
    /// asked; the clause moved into `validate` (above), so the requirement is
    /// now met by a build being legal at all. That is a strictly stronger rule
    /// and it would be worth nothing if the wrapper had stopped accepting the
    /// builds it now guarantees — the Kuva Nukor is the roster's first
    /// adversary weapon, so nothing else would notice.
    #[test]
    fn a_full_adversary_build_is_admissible() {
        // Eight mods from eight different families — two of one family is its
        // own refusal, and this test is not about that one.
        let mut fams: Vec<&str> = Vec::new();
        let mods: Vec<String> = crate::mods_data::pool_for_weapon("kuva_nukor")
            .iter()
            .filter(|m| !m.exilus)
            .filter(|m| match m.family {
                Some(f) if fams.contains(&f) => false,
                Some(f) => {
                    fams.push(f);
                    true
                }
                None => true,
            })
            .take(MAIN_SLOTS)
            .map(|m| m.id.to_string())
            .collect();
        assert_eq!(mods.len(), MAIN_SLOTS, "the pool can fill a build");
        // Its own slot's arcane, and every seat filled — the ruler asks for both.
        let arc: Vec<String> = crate::weapons_data::arcane_pools("kuva_nukor")
            .iter()
            .map(|slot| {
                crate::arcanes_data::pool_for_weapon("kuva_nukor", slot)
                    .first()
                    .map_or("none".to_string(), |a| a.id.to_string())
            })
            .collect();
        let ok = validate_for_board("single_target", "kuva_nukor", &mods, &[], &arc, "heat");
        assert!(ok.is_ok(), "a full adversary build is admitted: {ok:?}");
        assert_eq!(ok.unwrap().valence, "heat");

        // ...and the ruler's own `valence: full` still bites, now from one
        // layer down: the same build with no element is refused.
        let e = validate_for_board("single_target", "kuva_nukor", &mods, &[], &arc, "").unwrap_err();
        assert!(e.contains("Valence"), "{e}");
    }

    /// An unknown benchmark admits nothing: a number published against a ruler
    /// that does not exist has no standard behind it.
    #[test]
    fn an_unknown_benchmark_admits_nothing() {
        let e = validate_for_board("no_such_ruler", "gotva_prime", &[], &[], &[], "").unwrap_err();
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
            &["secondary_deadhead".to_string()], "");
        assert!(ok.is_ok(), "a secondary seats a secondary arcane: {ok:?}");

        // ...and it does NOT seat a primary one.
        let e = validate_for_board(
            "single_target", "dual_toxocyst", &mods("dual_toxocyst"), &evos,
            &["primary_deadhead".to_string()], "")
        .unwrap_err();
        assert!(e.contains("not an arcane"), "{e}");

        // A sentinel weapon seats none, so any arcane at all is refused.
        let e = validate_for_board(
            "single_target", "verglas_prime", &mods("verglas_prime"), &[],
            &["primary_crux".to_string()], "")
        .unwrap_err();
        assert!(e.contains("seats 0") || e.contains("not an arcane"), "{e}");
    }

}

/// The pairing dimension the optimizer searches and the quick calc reports.
/// Every number quoted here was measured through `/api/simulate` on the
/// Burston Prime at Thrax Lv 9999 SP, 300 s, 10 runs (kill rate).
#[cfg(test)]
mod element_order_tests {
    use super::*;
    use crate::damage::DamageType;
    use crate::damage::DamageType::*;

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
            let base = crate::loadout::WeaponBase::from_data("shedu", true, &[]);
            let pool = crate::mods_data::pool_for_weapon("shedu");
            let picked: Vec<&crate::loadout::ModDef> = mods
                .iter()
                .map(|id| pool.iter().find(|m| m.id == *id).expect("mod"))
                .collect();
            let p = crate::loadout::resolve(&base, &picked, crate::loadout::StackPolicy::Emergent);
            let live = |v: &crate::damage::DamageVector| {
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
        made.sort_by_key(|c| c.iter().map(|&t| crate::elements::wiki_order(t)).collect::<Vec<_>>());
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
}
