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
pub(crate) fn assembly_of(v: &Value, id: &str) -> Option<wfsim_engine::data::weapons::assembly::Assembly> {
    let spec = wfsim_engine::data::weapons::spec(id)?;
    let a = v.get("assembly").unwrap_or(&Value::Null);
    wfsim_engine::data::weapons::assembly::repaired(spec,
        get_str(a, "grip", ""), get_str(a, "loader", ""), get_str(a, "link", ""))
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
pub(crate) fn board_assembly_of(v: &Value) -> Option<wfsim_engine::data::weapons::assembly::Assembly> {
    wfsim_engine::data::weapons::assembly::of(get_str(v,"weapon",""),
        get_str(v,"grip",""), get_str(v,"loader",""), get_str(v,"link",""))
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
