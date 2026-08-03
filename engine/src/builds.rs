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
//! wire payload already says exactly that much: it carries no polarities, no
//! Forma, no slot positions and no mod ranks (every mod simulates at max
//! rank). Order does not count either — measured, not assumed: the same eight
//! mods reversed score 0.96478 both ways. So the identity is the weapon, the
//! SORTED mod ids, the evolution set, and the arcanes.
//!
//! Rivens are absent on purpose (user, 2026-08-04): they are personal random
//! items, so a board that counted them would rank luck. That also removes the
//! one free-text field a player authors — a riven's name — from anything that
//! would ever be uploaded.

use std::collections::BTreeSet;

use crate::mods::{plan_forma, PlannedMod};

/// The arsenal's capacity with an Orokin Catalyst on a rank-30 weapon. Forma
/// is UNCAPPED against it, which is the real constraint: you may re-polarise
/// forever, but you cannot exceed the pool.
pub const CAPACITY: u32 = 60;

/// A build that passed, and what it costs to actually own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidBuild {
    /// Weapon id.
    pub weapon: String,
    /// Mod ids, SORTED — the canonical form. Hash this, not what arrived.
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

/// Trim a submitted build to what the game would actually give it.
///
/// Never fails: unknown or foreign ids are DROPPED rather than rejected, since
/// an id we do not know is one this weapon cannot have either way. What is left
/// is what [`validate`] then judges.
fn normalize(weapon: &str, mods: &[String], evolutions: &[String]) -> (Vec<String>, Vec<String>) {
    let pool = crate::mods_data::pool_for_weapon(weapon);
    let mut ms: Vec<String> = mods
        .iter()
        .filter(|id| pool.iter().any(|m| m.id == id.as_str()))
        .cloned()
        .collect();
    ms.sort();
    ms.dedup();

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

    // SLOTS. Eight take anything; the ninth takes an exilus mod only. So a
    // ninth mod is legal exactly when one of them is exilus-eligible.
    let exilus_capable = ms.iter().filter(|id| def(id).exilus).count();
    let has_exilus_slot = crate::weapons_data::exilus_polarity(weapon).is_some()
        || !spec.exilus_polarity.as_deref().unwrap_or("").is_empty();
    match ms.len() {
        n if n > 9 => return Err(format!("{n} mods, and a weapon has 9 slots")),
        9 if exilus_capable == 0 => {
            return Err("9 mods, but none of them can go in the exilus slot".into())
        }
        9 if !has_exilus_slot => return Err(format!("{} has no exilus slot", spec.name)),
        _ => {}
    }

    // CAPACITY, with Forma unlimited. `plan_forma` answers both halves at once:
    // whether ANY layout fits, and how many Forma the cheapest one costs.
    let mut innate: Vec<Option<crate::mods::Polarity>> =
        crate::weapons_data::innate_slots(weapon).to_vec();
    innate.push(crate::weapons_data::exilus_polarity(weapon));
    let planned: Vec<PlannedMod> = ms
        .iter()
        .map(|id| {
            let m = def(id);
            PlannedMod { base_drain: m.base_drain, polarity: m.polarity }
        })
        .collect();
    let plan = plan_forma(CAPACITY, &innate, &planned)
        .map_err(|e| format!("does not fit {CAPACITY} capacity even with Forma: {e}"))?;

    // ARCANES: one per pool the weapon seats, and each from that pool.
    let slots = crate::arcanes_data::slots();
    let seats: Vec<&str> = slots
        .into_iter()
        .filter(|s| !crate::arcanes_data::slot_pool(s).is_empty())
        .collect();
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
        forma: plan.forma_used,
        drain: plan.total_drain,
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
        assert!(b.drain <= CAPACITY);
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
        // Ten mods.
        let ten = v(&["primed_point_blank", "hells_chamber", "blunderbuss", "primed_ravage",
                      "scattering_inferno", "toxic_barrage", "galvanized_savvy", "vicious_spread",
                      "shell_shock", "ammo_stock"]);
        let e = validate("boar_prime", &ten, &[], &[]).unwrap_err();
        assert!(e.contains("9 slots"), "{e}");
        // An arcane the weapon cannot seat.
        assert!(validate("boar_prime", &[], &[], &v(&["secondary_enervate"])).is_err());
    }

    /// CAPACITY is the rule the sim does not have, and the reason this module
    /// exists at all. Built from the pool's OWN numbers rather than a
    /// hand-picked list: the priciest family-distinct mods, nine of them with
    /// an exilus-capable one so the slot rule is satisfied and capacity is the
    /// only thing left to fail on. (Eight of them DO fit — 119 drain halves to
    /// 60 exactly — which is why this needs the ninth, and why guessing at the
    /// numbers instead of asking the pool would have written a test that
    /// asserted the wrong thing.)
    #[test]
    fn a_build_that_cannot_fit_is_refused_however_much_forma_you_own() {
        let mut pool = crate::mods_data::pool_for_weapon("boar_prime");
        pool.sort_by_key(|m| std::cmp::Reverse(m.base_drain));
        let mut fams: Vec<&str> = Vec::new();
        let take = |want_exilus: bool, picked: &mut Vec<String>, fams: &mut Vec<&str>| {
            for m in &pool {
                if m.exilus != want_exilus || picked.iter().any(|p| p == m.id) {
                    continue;
                }
                if let Some(f) = m.family {
                    if fams.contains(&f) {
                        continue;
                    }
                    fams.push(f);
                }
                picked.push(m.id.to_string());
                return true;
            }
            false
        };
        let mut picked = Vec::new();
        assert!(take(true, &mut picked, &mut fams), "the pool has an exilus mod");
        while picked.len() < 9 && take(false, &mut picked, &mut fams) {}
        assert_eq!(picked.len(), 9);

        let drain: u32 = picked
            .iter()
            .map(|id| pool.iter().find(|m| m.id == id.as_str()).unwrap().base_drain)
            .sum();
        // Halving is the best any polarity layout can do, so this is the floor.
        assert!(drain / 2 > CAPACITY, "the priciest nine must be over budget: {drain}");
        let e = validate("boar_prime", &picked, &[], &[]).unwrap_err();
        assert!(e.contains("capacity"), "{e}");
    }

    /// The identity is the FIGHT. Order is not part of it — which is not an
    /// assumption: the same eight mods reversed score 0.96478 either way
    /// (measured 2026-08-04, benchmark parameters).
    #[test]
    fn the_same_fight_written_differently_is_one_identity() {
        let a = validate("boar_prime", &v(&["hells_chamber", "blunderbuss", "primed_ravage"]), &[], &[]).unwrap();
        let b = validate("boar_prime", &v(&["primed_ravage", "hells_chamber", "blunderbuss"]), &[], &[]).unwrap();
        assert_eq!(identity(&a), identity(&b), "order is not part of the fight");

        // ...and a different SET is a different identity.
        let c = validate("boar_prime", &v(&["hells_chamber", "blunderbuss"]), &[], &[]).unwrap();
        assert_ne!(identity(&a), identity(&c));
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
}
