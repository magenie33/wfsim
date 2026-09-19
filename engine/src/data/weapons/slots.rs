use super::*;

/// The arcane pools this weapon SEATS, in slot order.
///
/// Keyed on the equipment slot, which is what the game keys it on — an Arch-Gun
/// seats a primary AND a secondary arcane, a sentinel weapon seats none, and
/// everything else seats one of its own slot. A category rule, not per-weapon
/// data, which is why it is computed rather than declared.
///
/// It lived in `webapi` until 2026-08-05, when `builds` needed it too: "every
/// arcane seat filled" is part of what a complete build means, and a second
/// copy of this rule in the validator is how the page and the board come to
/// disagree about how many seats a weapon has.
pub fn arcane_pools(weapon: &str) -> Vec<&'static str> {
    let Some(s) = spec(weapon) else { return Vec::new() };
    if s.class.contains("sentinel") {
        return Vec::new();
    }
    let own = match s.slot.as_str() {
        // "Archguns possess two Arcane Enhancement slots to equip one Primary
        // Arcane and one Secondary Arcane" (wiki, Arch-Gun).
        "archgun" => vec!["primary", "secondary"],
        "primary" => vec!["primary"],
        "secondary" => vec!["secondary"],
        "melee" => vec!["melee"],
        _ => return Vec::new(),
    };
    // A KITGUN SEATS ONE OF ITS OWN AS WELL, and the wiki states it as an
    // "as well" rather than an "instead": *"These can be installed
    // simultaneously with Secondary/Primary arcanes"* (`Kitgun` §Kitgun
    // Arcanes). Filing Pax and Residual under the weapon's own slot made the
    // two compete for one seat, so the page asked the reader to choose between
    // a Pax Charge and a Primary Merciless — a choice the game never puts to
    // them.
    //
    // FIRST, because it is the seat this weapon has that no other weapon does:
    // the ordinary one is the same seat every gun in the roster carries, and
    // putting the distinctive one after it reads as an afterthought.
    if s.kitgun.is_some() {
        let mut out = vec!["kitgun"];
        out.extend(own);
        return out;
    }
    own
}

/// Innate MAIN-slot polarities as an 8-slot layout (exilus excluded — the
/// UI/optimizer model treats the exilus slot separately).
pub fn innate_slots(id: &str) -> [Option<Polarity>; 8] {
    let mut out = [None; 8];
    if let Some(s) = spec(id) {
        for (i, p) in s.polarities.iter().take(8).enumerate() {
            out[i] = Some(polarity(p));
        }
    }
    out
}

/// The exilus slot's innate polarity, if the weapon has one (wiki panel's
/// "Exilus Polarity"; Dual Toxocyst: Naramon).
pub fn exilus_polarity(id: &str) -> Option<Polarity> {
    spec(id)?.exilus_polarity.as_deref().map(polarity)
}

/// Does the weapon HAVE an exilus slot? The adapter fits "a Primary, Secondary
/// or Melee weapon" (wiki, Exilus Weapon Adapter), so an Arch-Gun and a robotic
/// weapon have none.
///
/// It is the slot COUNT that needs this, not the polarity: a leftover innate
/// colour sits harmlessly on a mod-less slot, so counting a slot the weapon
/// does not have hides a mismatch the player would really be paying
/// (`rules::capacity::plan_forma`).
pub fn has_exilus_slot(id: &str) -> bool {
    let Some(s) = spec(id) else { return false };
    !s.class.contains("sentinel") && matches!(s.slot.as_str(), "primary" | "secondary" | "melee")
}

/// …AND THE STANCE SLOT'S, which is a capacity GRANT rather than a discount.
pub fn stance_polarity(id: &str) -> Option<Polarity> {
    spec(id)?.stance_polarity.as_deref().map(polarity)
}
