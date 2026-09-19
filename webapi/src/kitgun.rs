// SPDX-License-Identifier: AGPL-3.0-or-later
//! A MODULAR weapon's parts and a progenitor's valence, read off a request.

use serde_json::Value;
use wfsim_engine::model::WeaponBase;
use crate::request::{get_f64, get_str};

/// THE ASSEMBLY a request asked for, for a MODULAR weapon.
///
/// `None` for everything that is not one, so an ordinary weapon cannot be
/// handed parts by a request — the same shape `apply_valence_from` has, and for
/// the same reason.
///
/// A REQUEST THAT NAMES NO ASSEMBLY GETS THE DEFAULT, never the chamber's
/// preview: the preview is the module's no-grip row and is a stat line no
/// player can reproduce, so simulating it would answer a question about a
/// weapon nobody has. `valence_element_of` decides the same thing for the same
/// reason. It also REPAIRS a part this entry cannot take — a grip from the
/// other slot, a loader that does not exist — which would otherwise compose to
/// nothing and panic the panel, and which arrives from a stale share link
/// rather than from an omission.
pub(crate) fn assembly_of(v: &Value, id: &str) -> Option<wfsim_engine::data::weapons::kitguns::Assembly> {
    let spec = wfsim_engine::data::weapons::spec(id)?;
    let record = spec.kitgun.as_deref()?;
    let fallback = wfsim_engine::data::weapons::kitguns::default_assembly(record);
    let Some(a) = v.get("assembly") else { return fallback };
    let mut asked = wfsim_engine::data::weapons::kitguns::Assembly {
        // THE CHAMBER IS THE WEAPON'S, never the request's. A request that could
        // name one would be naming a different weapon than the id it sent.
        chamber: fallback.as_ref().map(|f| f.chamber.clone()).unwrap_or_default(),
        grip: get_str(a, "grip", "").to_string(),
        loader: get_str(a, "loader", "").to_string(),
    };
    // PART BY PART, not all or nothing: a stale link naming a grip from the
    // other slot must not also throw away a loader it named correctly. That is
    // `valence_element_of`'s rule — it repairs the ELEMENT and leaves the bonus
    // alone — and the difference is visible, since a discarded loader moves the
    // magazine, the reload and all three of crit, crit damage and status.
    let f = fallback?;
    if !wfsim_engine::data::weapons::kitguns::grips()
        .iter()
        .any(|g| g.id == asked.grip && Some(g.slot.as_str()) == kitgun_slot(spec))
    {
        asked.grip = f.grip.clone();
    }
    if !wfsim_engine::data::weapons::kitguns::loaders().iter().any(|l| l.id == asked.loader) {
        asked.loader = f.loader.clone();
    }
    // …and if the pair still does not compose, the whole default, because a
    // panel is about to be derived from it and there is nothing else to give.
    match wfsim_engine::data::weapons::spec_assembled(spec, Some(&asked)) {
        Some(_) => Some(asked),
        None => Some(f),
    }
}

/// THE PARTS A BOARD PAYLOAD NAMES, as the engine's own shape.
///
/// FLAT ON THE WIRE, because a record's fields are flat: `grip` and `loader`
/// are two ids the worker stores beside every other axis (`AXES`). The CHAMBER
/// is not among them — it is the weapon, and `weapon` already carries it.
///
/// NO REPAIR HERE, unlike `assembly_of`. That one is answering a FIGHT, where
/// there has to be something to draw; this is answering whether a BUILD is one
/// the board can take, and quietly swapping a part would accept a record that
/// scores as something else.
pub(crate) fn board_assembly_of(v: &Value) -> Option<wfsim_engine::data::weapons::kitguns::Assembly> {
    let grip = get_str(v, "grip", "");
    let loader = get_str(v, "loader", "");
    if grip.is_empty() && loader.is_empty() {
        return None;
    }
    // THE CHAMBER'S WEAPON ID, not the per-slot RECORD id `spec.kitgun` holds:
    // `Assembly::chamber_record` looks the record up by (chamber, slot) and the
    // slot follows from the grip, so a record id here resolves to nothing and
    // every legal pair reads as "these do not make a Tombfinger".
    let chamber = wfsim_engine::data::weapons::spec(get_str(v, "weapon", ""))
        .and_then(|s| s.kitgun.clone())
        .and_then(|r| wfsim_engine::data::weapons::kitguns::default_assembly(&r))
        .map(|d| d.chamber)
        .unwrap_or_default();
    Some(wfsim_engine::data::weapons::kitguns::Assembly {
        chamber,
        grip: grip.to_string(),
        loader: loader.to_string(),
    })
}

/// Which slot a modular entry's grips must belong to.
fn kitgun_slot(spec: &wfsim_engine::data::weapons::WeaponSpec) -> Option<&str> {
    let record = spec.kitgun.as_deref()?;
    wfsim_engine::data::weapons::kitguns::chambers()
        .iter()
        .find(|c| c.id == record)
        .map(|c| c.slot.as_str())
}

/// THE VALENCE BONUS a request asked for, applied to a base.
///
/// Its own function because two paths build a base for a request — this one and
/// the optimizer's `deployed` — and a valence that reached only one of them
/// would score a search against a weapon the replay never fires. It is the same
/// pairing `apply_deployment` already lives in.
///
/// A weapon with no valence spec ignores both fields, so an ordinary weapon
/// cannot be handed one by a request.
pub(crate) fn apply_valence_from(v: &Value, id: &str, b: &mut WeaponBase) {
    let Some(s) = wfsim_engine::data::weapons::valence_of(id) else { return };
    let el = valence_element_of(v, id);
    if el.is_empty() {
        return;
    }
    // NO DEFAULT PERCENTAGE. A request that names an element and no number gets
    // the roll's FLOOR, because a valence nobody stated is the one every Lich
    // hands out and not the one five fusions bought — the optimistic reading
    // belongs to the player who typed it.
    let bonus = get_f64(v, "valence_bonus", s.min);
    wfsim_engine::data::weapons::apply_valence(b, id, &el, bonus);
}

/// The progenitor element a request is for.
///
/// AN ADVERSARY WEAPON ALWAYS HAS ONE. Every copy in the
/// game comes out of a Lich carrying an element, so "no element" is not a
/// weaker build of that weapon — it is a weapon nobody has, and the printed
/// panel the wiki's infobox shows is a number no player can reproduce. A
/// request naming none therefore gets the weapon's FIRST element, which is the
/// one the page itself opens on, rather than a fight against a weapon that
/// does not exist.
///
/// It also repairs an element this weapon cannot roll, which would otherwise
/// fall through `apply_valence`'s own rejection and silently apply nothing —
/// the same shape, arriving from a stale link instead of from an omission.
///
/// Empty survives for exactly one case, and it is the case that means it: a
/// weapon with no valence spec, where the axis does not exist. Two paths read
/// this field — the panel's base and the optimizer's variant table — which is
/// why they read it through one function.
pub(crate) fn valence_element_of(v: &Value, id: &str) -> String {
    let Some(s) = wfsim_engine::data::weapons::valence_of(id) else {
        return String::new();
    };
    let el = get_str(v, "valence_element", "");
    match s.elements.iter().find(|e| *e == el) {
        Some(e) => e.clone(),
        None => s.elements.first().cloned().unwrap_or_default(),
    }
}
