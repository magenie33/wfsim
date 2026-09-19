use super::*;

/// ONE REPRESENTATIVE PER BUILD: elements last, in the order that pairs them;
/// everything else ahead of them, ordered by a rule rather than by chance.
///
/// Raw order is too fine and sorted order is too coarse, and both were wrong
/// here in the same day. What actually matters is measured: moving three elementals from slots 1-3 to 4-6, interleaving them
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
/// — a rule chosen so the representative is stable and
/// readable, not because the engine cares.
pub fn canonical_mods(weapon: &str, mods: &[String]) -> Vec<String> {
    canonical_mods_with(weapon, mods, None)
}

/// THE SLOT A RIVEN OCCUPIES in a public record's mod list.
///
/// A board row cannot name somebody's riven — the item is on one machine — so
/// it carries the literal id and the SHAPE separately. The id is in `mods` at
/// the riven's own position because position is the build: an elemental riven
/// pairs with the build's other elementals, and where it sits decides what it
/// pairs WITH.
pub const RIVEN_SLOT: &str = "riven";

/// [`canonical_mods`], for a build carrying a riven of known SHAPE.
///
/// A riven is an ATOM: it may bring TWO elements, and they enter the pool
/// adjacent and in its own stat order, which no permutation of the build can
/// separate. That is the one assumption the rule below was built on — one mod,
/// one element — and it is why the layout step has a second path.
pub fn canonical_mods_with(
    weapon: &str,
    mods: &[String],
    riven: Option<&crate::build::rivens::RivenShape>,
) -> Vec<String> {
    let pool = crate::data::mods::pool_naming(weapon, mods);
    let def = |id: &String| pool.iter().find(|m| m.id == id.as_str());
    // WHAT EACH MOD PUTS INTO THE ELEMENT POOL, in order. One entry for an
    // ordinary elemental mod, none for a plain one, and up to TWO for a riven.
    let riven_elements: Vec<crate::rules::damage::DamageType> = match riven {
        Some(r) => crate::build::rivens::class_for_weapon(weapon)
            .map(|c| r.elements(c))
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let elements_of = |id: &String| -> Vec<crate::rules::damage::DamageType> {
        if id == RIVEN_SLOT {
            return riven_elements.clone();
        }
        def(id).and_then(|m| m.primary_element()).into_iter().collect()
    };
    // POSITIONAL: the mods whose PLACE is part of the build. Only the
    // element-bearing ones are today; the split is by that property rather than
    // by the property's one current cause.
    let (mut plain, positional): (Vec<&String>, Vec<&String>) =
        mods.iter().partition(|id| elements_of(id).is_empty());
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
    // `rules::elements::combine` does not chunk the mod list. It chunks the list of
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
    // Distinct elements in first-appearance order — exactly what `push` builds.
    // A riven contributes its own in one go, which is what makes it an atom.
    let seq_of = |order: &[&String]| {
        let mut seq: Vec<crate::rules::damage::DamageType> = Vec::new();
        for e in order.iter().flat_map(|x| elements_of(x)) {
            if !seq.contains(&e) {
                seq.push(e);
            }
        }
        seq
    };
    let mut seq = seq_of(&positional);
    // ONE CONSTRAINT, AND EVERYTHING ELSE IS FREE.
    //
    // FIXED: which elements share a pair, and which one trails. Moving an
    // element across a boundary re-pairs everything after it — 12,424 DPS
    // against 46,583 on the Torid — so the PARTITION is never touched, and the
    // trailing element stays LAST because chunking reads from the front.
    //
    // FREE: everything inside that. The order within a pair (`combined_of` is
    // symmetric and pools both amounts), the order of the pairs among
    // themselves (`combine` ADDS each secondary into a vector — the same four
    // mods with their pairs swapped give 12,773.473 either way), and the order
    // of the mods that feed one pair, since pooling makes their sequence one
    // entry per element however they are arranged.
    let odd = seq.len() % 2;
    let tail: Vec<crate::rules::damage::DamageType> = seq.split_off(seq.len() - odd);
    let pairs: Vec<[crate::rules::damage::DamageType; 2]> =
        seq.chunks(2).map(|c| [c[0], c[1]]).collect();

    // SO THE REPRESENTATIVE IS THE RANK-SMALLEST ARRANGEMENT THAT KEEPS THE
    // PARTITION, and one rule decides every position on the card: biggest drain
    // first, then DE's own English name. It is the rule the plain mods take and
    // the one the riven search below picks by, so the whole card answers to one
    // question rather than to the wiki's element table for half of it.
    //
    // BUILT AS BLOCKS. A pair owns every mod whose element is in it; sort the
    // block, then order the blocks the same way. That IS the smallest valid
    // arrangement rather than an approximation of it: an ordering that
    // interleaved two pairs would pool a different sequence and therefore a
    // different pairing, so every legal arrangement is block-shaped already.
    //
    // WHAT IT LOOKS LIKE, which is the point: the cards that have to combine sit
    // together at the front, heaviest first, and the odd one out follows. Two
    // Cold cards and a Toxin come out `Cold(10) / Toxin(5) / Cold(1)` rather
    // than grouped by element — they pool either way, and reading it back is
    // what a person does with it.
    let block_of = |want: &[crate::rules::damage::DamageType]| -> Vec<&String> {
        let mut g: Vec<&String> = positional
            .iter()
            .copied()
            .filter(|m| elements_of(m).iter().any(|e| want.contains(e)))
            .collect();
        g.sort_by(rank);
        g
    };
    let smaller = |a: &Vec<&String>, b: &Vec<&String>| {
        a.iter()
            .zip(b.iter())
            .find_map(|(x, y)| match rank(x, y) {
                std::cmp::Ordering::Equal => None,
                o => Some(o),
            })
            .unwrap_or_else(|| a.len().cmp(&b.len()))
    };

    // AN ATOM CANNOT BE LAID OUT BLOCK BY BLOCK. A riven bringing Heat AND Cold
    // occupies two consecutive places in the pool, in its own order, and it
    // belongs to one block only if both land in one pair — an arrangement no
    // mod order can produce.
    //
    // So when one is present the representative is SEARCHED instead of built:
    // every ordering of the positional mods, keeping those whose pooled
    // sequence makes the SAME pairing, and the smallest of those by the same
    // rank. The submitted order is always one of them, so the search cannot come
    // back empty, and both paths now compute the same thing — the fast path is
    // an optimisation of the search rather than a second answer.
    let positional: Vec<&String> = if positional.iter().all(|m| elements_of(m).len() <= 1) {
        let mut blocks: Vec<Vec<&String>> = pairs.iter().map(|p| block_of(p)).collect();
        blocks.sort_by(|a, b| smaller(a, b));
        blocks.into_iter().flatten().chain(block_of(&tail)).collect()
    } else {
        // THE INVARIANT IS `pairing_of`'S, on both sides. Built by hand from the
        // partition above it would be the SUBMITTED order's — `[Cold, Toxin]`
        // and `[Toxin, Cold]` are one pairing and two tuples — so a candidate
        // that matched would compare unequal and one fight would come back with
        // three representatives.
        let want = pairing_of(&seq_of(&positional));
        let mut best: Option<Vec<&String>> = None;
        for cand in orderings(&positional) {
            if pairing_of(&seq_of(&cand)) != want {
                continue;
            }
            let better = best.as_ref().is_none_or(|b| smaller(&cand, b).is_lt());
            if better {
                best = Some(cand);
            }
        }
        best.unwrap_or(positional)
    };
    // THE POSITION-BEARING CARDS FIRST. Their places are what the build IS, so
    // they sit at a fixed offset — `0..k` whatever else is carried — rather than
    // drifting every time a plain mod is added or dropped.
    positional.into_iter().chain(plain).cloned().collect()
}

/// THE PAIRING A POOLED ELEMENT SEQUENCE MAKES — which elements share a pair,
/// and which one trails.
///
/// Everything about position that the FIGHT can see, and nothing it cannot: the
/// order inside a pair is free (`combined_of` is symmetric) and so is the order
/// of the pairs among themselves (`combine` adds each secondary into a vector),
/// so both are normalised away here.
pub(super) fn pairing_of(
    seq: &[crate::rules::damage::DamageType],
) -> (Vec<[crate::rules::damage::DamageType; 2]>, Vec<crate::rules::damage::DamageType>) {
    let mut seq = seq.to_vec();
    let odd = seq.len() % 2;
    let tail = seq.split_off(seq.len() - odd);
    let mut pairs: Vec<[crate::rules::damage::DamageType; 2]> =
        seq.chunks(2).map(|c| [c[0], c[1]]).collect();
    for p in &mut pairs {
        p.sort_by_key(|&t| crate::rules::elements::wiki_order(t));
    }
    pairs.sort_by_key(|p| {
        crate::rules::elements::wiki_order(
            crate::rules::elements::combined_of(p[0], p[1]).expect("pooled elements are distinct"),
        )
    });
    (pairs, tail)
}

/// Every ordering of `xs`. Only ever called on the ELEMENTAL mods of one build,
/// which a benchmark caps at eight — 40,320 orderings of a six-entry sequence,
/// with no simulation behind any of them.
pub(super) fn orderings<'a>(xs: &[&'a String]) -> Vec<Vec<&'a String>> {
    if xs.len() > MAX_ELEMENTAL_PERMUTED {
        return vec![xs.to_vec()];
    }
    let mut out = Vec::new();
    let mut acc = Vec::new();
    let mut rest = xs.to_vec();
    fn go<'a>(rest: &mut Vec<&'a String>, acc: &mut Vec<&'a String>, out: &mut Vec<Vec<&'a String>>) {
        if rest.is_empty() {
            out.push(acc.clone());
            return;
        }
        for i in 0..rest.len() {
            let x = rest.remove(i);
            acc.push(x);
            go(rest, acc, out);
            acc.pop();
            rest.insert(i, x);
        }
    }
    go(&mut rest, &mut acc, &mut out);
    out
}
