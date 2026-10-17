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
    let own = match s.rules_slot() {
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
    !s.class.contains("sentinel") && !s.no_exilus_slot && matches!(s.rules_slot(), "primary" | "secondary" | "melee")
}

/// …AND THE STANCE SLOT'S, which is a capacity GRANT rather than a discount.
pub fn stance_polarity(id: &str) -> Option<Polarity> {
    spec(id)?.stance_polarity.as_deref().map(polarity)
}

/// Does firing `form` on `weapon` need a stance in the slot? A GROUND COMBO
/// DOES: the stanceless combos publish damage and no duration
/// (`Module:Stances/data`), so an empty slot has no script to time — and the
/// entry's own script is a stance's, which an empty slot would get for free.
/// Heavy, slide and slam are the same whatever is seated; a fixed stance is
/// always seated.
pub fn combo_needs_a_stance(weapon: &str, form: crate::model::FormKind) -> bool {
    use crate::model::FormKind as F;
    let Some(w) = spec(weapon) else { return false };
    w.rules_slot() == "melee"
        && w.fixed_stance.is_none()
        && matches!(form, F::Neutral | F::Forward | F::Block | F::BlockForward)
}

/// The board's refusal of a melee row with an empty stance slot.
pub const STANCE_REQUIRED: &str =
    "no stance in the slot, and a board row on a melee weapon carries one — equip a stance";

/// The refusal a stanceless ground combo gets, in one wording for every caller.
pub const STANCELESS_COMBO: &str =
    "no stance in the slot: stanceless combos are not modelled yet — equip a stance, or play heavy attacks, slide attacks or heavy slams";

/// …and does the SEATED stance supply this form's combo? A card carries a
/// combo only where `Module:Stances/data` publishes its duration, so a card
/// can lack one form — and the entry's own script is then an empty slot's.
pub fn stance_supplies(stance: Option<crate::model::StanceCombos>, form: FormKind) -> bool {
    stance.is_some_and(|c| c.iter().any(|(f, _)| *f == form.id()))
}

/// The refusal a combo gets when the seated stance does not supply it.
pub const STANCE_LACKS_COMBO: &str =
    "this stance publishes no timing for this combo, so it is not modelled — pick another combo or another stance";
