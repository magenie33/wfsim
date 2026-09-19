use super::*;

/// The guard on [`orderings`]. A benchmark build has eight main slots, so this
/// cannot be reached today; it is here so that a longer build can never turn
/// canonicalisation into a hang.
pub(super) const MAX_ELEMENTAL_PERMUTED: usize = 8;

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
    pub combined: Vec<crate::rules::damage::DamageType>,
    /// Primary elements left uncombined (the trailing odd one, and any the
    /// weapon carries that nothing paired with).
    pub leftover: Vec<crate::rules::damage::DamageType>,
}

/// The permutation cap. 6 distinct elements is 720 resolves — already more
/// than any real build (there are only six primaries), and the guard is here
/// so a future element cannot turn this into a hang.
pub(super) const MAX_ELEMENT_PERMUTATIONS: usize = 720;

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
/// element and it pools FORWARD onto the mod's position — `rules::elements::combine`
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
    let pool = crate::data::mods::pool_naming(weapon, mods);
    let def = |id: &String| pool.iter().find(|m| m.id == id.as_str());
    let evo_refs: Vec<&str> = evolutions.iter().map(String::as_str).collect();
    let base = crate::model::WeaponBase::from_data(weapon, true, &evo_refs);

    // Distinct MOD elements in first-appearance order. Same-element mods pool
    // (`ElementalInput::push` merges them), so they are one entry and move
    // together — the distinction that broke 5669040.
    let mut seq: Vec<crate::rules::damage::DamageType> = Vec::new();
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
    let mut seen: Vec<Vec<(crate::rules::damage::DamageType, i64)>> = Vec::new();
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
        let refs: Vec<&crate::model::ModDef> = laid.iter().filter_map(&def).collect();
        let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::AssumedMax);
        let key: Vec<(crate::rules::damage::DamageType, i64)> = panel
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
        let mut combined: Vec<crate::rules::damage::DamageType> = Vec::new();
        let mut leftover: Vec<crate::rules::damage::DamageType> = Vec::new();
        for (t, _) in panel.damage.iter_nonzero() {
            if t.is_secondary_element() {
                combined.push(t);
            } else if t.is_primary_element() {
                leftover.push(t);
            }
        }
        combined.sort_by_key(|&t| crate::rules::elements::wiki_order(t));
        leftover.sort_by_key(|&t| crate::rules::elements::wiki_order(t));
        out.push(ElementOrder { mods: laid, combined, leftover });
    }
    out
}

/// All orderings of `rest`, appended to `acc`. Mirrors the optimizer's own.
pub(super) fn permutations(
    rest: &[crate::rules::damage::DamageType],
    acc: &mut Vec<crate::rules::damage::DamageType>,
    out: &mut Vec<Vec<crate::rules::damage::DamageType>>,
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
pub(super) fn normalize_with(
    weapon: &str,
    mods: &[String],
    evolutions: &[String],
    riven: Option<&crate::build::rivens::RivenShape>,
) -> (Vec<String>, Vec<String>) {
    let mut pool = crate::data::mods::pool_naming(weapon, mods);
    // A RIVEN IS IN THE POOL FOR THIS WEAPON BY DEFINITION — it is rolled for
    // it — so the "drop anything foreign" filter below must not throw the slot
    // away. Its legality is a question about the SHAPE and is asked in
    // `check_riven_shape`, where the answer can say why.
    if riven.is_some() {
        pool.push(crate::model::ModDef { id: RIVEN_SLOT, ..pool[0].clone() });
    }
    // CANONICALISED, not sorted and not left raw — see `canonical_mods`. A
    // plain sort scored a pairing nobody submitted; raw order made two spellings
    // of one fight into two rows.
    let kept: Vec<String> = mods
        .iter()
        .filter(|id| pool.iter().any(|m| m.id == id.as_str()))
        .cloned()
        .collect();
    let multishot = canonical_mods_with(weapon, &kept, riven);

    // The ladder: tier N is only open when the tiers below it are filled, so a
    // set is trimmed to its longest legal prefix. One option per tier.
    // Evolutions belong to the TRANSFORM GROUP, not to a form: the two entries
    // of a two-weapon pair share one ladder.
    let spec = crate::data::weapons::spec(weapon);
    let group = spec
        .and_then(|s| s.transform_group.clone())
        .unwrap_or_else(|| weapon.to_string());
    let mut evos = Vec::new();
    for tier in 1..=crate::data::evolutions::tier_count(&group) {
        let pick = evolutions.iter().find(|id| {
            crate::data::evolutions::get(id).is_some_and(|e| e.weapon == group && e.tier == tier)
        });
        match pick {
            Some(id) => evos.push(id.clone()),
            None => break, // the ladder stops at the first empty rung
        }
    }
    (multishot, evos)
}
