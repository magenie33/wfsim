// SPDX-License-Identifier: AGPL-3.0-or-later
//! `/api/meta`: everything the page needs before its first request.

use serde_json::{json, Value};
use wfsim_engine::model::{ModDef, ModEffect};
use crate::registry::{assets, default_weapon_id, enemies, evo_forbids, evo_group, form_unlock_evo, innate_slots_for, passives_of, weapons, wspec};
use crate::request::r3;
use crate::rivens::riven_class;
use crate::tenno::floor_json;

/// The PARTS a modular weapon can be assembled from, for `/api/meta`.
///
/// `Value::Null` for every weapon that is not one.
pub(crate) fn assembly_meta(id: &str) -> Value {
    use wfsim_engine::data::weapons::kitguns as kg;
    let Some(record) = wfsim_engine::data::weapons::spec(id).and_then(|s| s.kitgun.as_deref())
    else {
        return Value::Null;
    };
    let Some(c) = kg::chambers().iter().find(|c| c.id == record) else {
        return Value::Null;
    };
    json!({
        // The CHAMBER is the weapon and is not a choice here — it is named so
        // the page can say which one this is without knowing the id scheme.
        "chamber": c.chamber,
        "chamber_name": c.name,
        // ONLY THE GRIPS THAT FIT THIS SLOT. The other five compose into the
        // sibling entry and into nothing here.
        "grips": kg::grips()
            .iter()
            .filter(|g| g.slot == c.slot)
            .map(|g| json!({ "id": g.id, "name": g.name, "recoil": g.recoil }))
            .collect::<Vec<_>>(),
        // WHAT EACH GRIP IS WORTH ON THIS CHAMBER. The grip's only stat of its
        // own is recoil; what it actually decides — damage, fire rate and the
        // charge — is published per grip in the CHAMBER's tables, so a page
        // showing five names and a recoil figure would be hiding the whole
        // decision.
        "grip_stats": kg::grips()
            .iter()
            .filter(|g| g.slot == c.slot)
            .filter_map(|g| c.damage.get(&g.id).map(|d| (g, d)))
            .map(|(g, d)| (g.id.clone(), json!({
                "damage": d.values().sum::<f64>(),
                "fire_rate": c.fire_rate.get(&g.id).copied().unwrap_or(0.0),
                "charge_seconds": c.charge_seconds.get(&g.id),
                // ON A BEAM THE GRIP TRADES REACH, not fire rate (Gaze,
                // Vermisplicer) — null where the reach is not the grip's.
                "range_m": match &c.range_m {
                    Some(kg::Reach::PerGrip(t)) => t.get(&g.id).copied(),
                    _ => None,
                },
            })))
            .collect::<serde_json::Map<String, Value>>(),
        "loaders": kg::loaders()
            .iter()
            .map(|l| json!({
                "id": l.id,
                "name": l.name,
                "crit_chance": l.crit_chance,
                "crit_multiplier": l.crit_multiplier,
                "status_chance": l.status_chance,
                "magazine": l.magazine,
                "reload_seconds": l.reload_seconds,
                // WHAT THIS LOADER'S SIZE CLASS IS WORTH ON THIS CHAMBER. The
                // class is DE's own key and prices differently per chamber, so
                // a page showing `highest` would be showing a word instead of
                // a number.
                "rounds": c.magazine.get(&l.magazine).copied().unwrap_or(0.0),
            }))
            .collect::<Vec<_>>(),
        "default": kg::default_assembly(record).map(|a| json!({
            "grip": a.grip,
            "loader": a.loader,
        })),
    })
}

/// A coarse category for grouping mods in the picker UI.
fn mod_category(m: &ModDef) -> &'static str {
    let has = |f: fn(&ModEffect) -> bool| m.effects.iter().any(f);
    if has(|e| matches!(e, ModEffect::Element(..) | ModEffect::CombinedElement(..))) {
        "element"
    } else if has(|e| {
        matches!(
            e,
            ModEffect::CritChance(..)
                | ModEffect::CritDamage(..)
                | ModEffect::OnHeadshotCritChance { .. }
                | ModEffect::OnHeadshotKillCritChance { .. }
        )
    }) {
        "crit"
    } else if has(|e| {
        matches!(
            e,
            ModEffect::StatusChance(..)
                | ModEffect::StatusDamage(..)
                | ModEffect::ConditionOverload { .. }
        )
    }) {
        "status"
    } else if has(|e| matches!(e, ModEffect::FireRate(..) | ModEffect::ReloadSpeed(..))) {
        "handling"
    } else {
        "damage"
    }
}

fn mods_json(p: &[ModDef]) -> Vec<Value> {
    p.iter()
        .map(|m| {
            let mut j = json!({
                "id": m.id,
                // WHERE THIS SITS IN THE FROZEN SHARE ORDER, so a share
                // link can name it with a number instead of spelling it
                // out — see `engine::share_order`. Absent for an id the
                // manifest has not been told about yet, which a link
                // falls back to spelling.
                "si": wfsim_engine::data::share_order::index_of(m.id),

                // DE's own name, straight from the yaml, never a
                // title-cased id: "Semi-Shotgun Cannonade" loses its hyphen
                // that way and the card's wiki link 404s.
                "name": m.name,
                "drain": m.base_drain,
                "max_rank": m.max_rank,
                "polarity": format!("{:?}", m.polarity),
                "rarity": format!("{:?}", m.rarity).to_lowercase(),
                "exilus": m.exilus,
                // AN ELEMENT-BEARING MOD, whose ORDER is part of the build — the
                // Forma plan moves the others freely and keeps these in order.
                "elemental": m.primary_element().is_some(),
                // A STANCE, so the page can put it in the stance slot and keep
                // it out of the eight. The SCRIPTS are not sent: they are the
                // engine's answer to "what does this weapon swing", and the
                // page has never needed to know.
                "stance": m.stance.is_some(),
                // …AND WHAT EACH OF ITS COMBOS COMES TO, keyed by the form it
                // is. NOT the combo's NAME — a mode's name is fixed and its
                // STRENGTH is not, and these three numbers
                // are the strength: swapping this card is what moves them.
                "stance_combos": m.stance.map(|c| {
                    c.iter()
                        .map(|(form, hits)| (*form, combo_summary(hits)))
                        .collect::<std::collections::BTreeMap<_, _>>()
                }),

                "family": m.family,
                "category": mod_category(m),
                // THE PRIMARY ELEMENT IT ADDS, and how much — what the optimizer's
                // four default starts pick their card by (the strongest of each).
                "element": m.effects.iter().find_map(|e| match e {
                    wfsim_engine::model::ModEffect::Element(t, v) => Some(json!([t.name(), v])),
                    _ => None,
                }),
                "image": assets().mods.get(m.id),
                // WHAT WARFRAME.MARKET CALLS IT. Absent means it does not trade
                // there at all — Umbral and Galvanized cards carry no link, and
                // that absence is the only tradeability rule the app has.
                "market_slug": wfsim_engine::data::market::mod_slug(m.id),
                // One line per modeled effect — engine describe() stays the
                // model's own statement (search + panel attribution).
                "effects": m.effects.iter().map(|e| e.describe()).collect::<Vec<_>>(),
                // Equip restriction beyond the pool tag: "continuous" for the
                // beam-only mods. The picker filters on it, the same way the
                // engine's `pool_for_weapon` does.
                "requires_weapon": m.requires_weapon,
                // WHAT WE DO NOT MODEL, said out loud. The card prefers DE's
                // own text, so an "out of scope" line that only lived in the
                // model description was never rendered — the mod looked like
                // it worked and did nothing.
                "not_modeled": m.unmodeled,
                "out_of_scope": m.out_of_scope,
                // ...and the PARTLY modelled case, which neither flag above can
                // say: Winds of Purity lands its Purity radial and does not
                // model its life steal, so calling the whole card unmodelled
                // would be a second untruth. Derived from what the loader
                // actually dropped.
                "unmodeled_effects": wfsim_engine::data::mods::unmodeled_effects(m.id),
            });
            // The verbatim in-game DESCRIPTION per rank (X filled) — what
            // the picker and the configured slot display. Absent for pools
            // without yaml descriptions (the hardcoded rifle pool): the UI
            // falls back to the effect lines.
            if let Some(info) = wfsim_engine::data::mods::desc_info(m.id) {
                let dr: Vec<String> = (0..=info.max_rank).map(|r| info.at(r)).collect();
                j["desc_ranks"] = json!(dr);
            }
            j
        })
        .collect()
}

pub fn meta_json() -> Value {
    let weapons: Vec<Value> = weapons()
        .iter()
        .map(|w| {
            let max_rank = wspec(&w.id).max_rank;
            let forma_min = wfsim_engine::rules::capacity::forma_to_max_rank(max_rank);
            let cap = wfsim_engine::rules::capacity::capacity(
                wfsim_engine::rules::capacity::rank_after(max_rank, forma_min),
                wfsim_engine::rules::capacity::Investment::default().catalyst,
            );
            json!({
                "id": w.id,
                // WHERE THIS SITS IN THE FROZEN SHARE ORDER, so a share
                // link can name it with a number instead of spelling it
                // out — see `engine::share_order`. Absent for an id the
                // manifest has not been told about yet, which a link
                // falls back to spelling.
                "si": wfsim_engine::data::share_order::index_of(&w.id),
                "name": w.name,
                // The pools to union, in order. `mod_class` stays as the
                // NARROWEST one, which is what labels and filters read.
                "mod_pools": w.mod_pools,
                // The BASE form's trigger decides this — the Torid's Incarnon
                // form is a beam and the weapon still is not a continuous one
                // for modding purposes.
                "continuous": w.continuous,
                "disposition": w.disposition,
                // WHAT WARFRAME.MARKET CALLS THIS WEAPON'S RIVEN AUCTIONS, so
                // the riven card can offer the one price we do not compute.
                // Absent where no riven exists for it — see `data::market`.
                "market_riven_slug": wfsim_engine::data::market::riven_weapon_slug(&w.id),
                // …AND THE WEAPON ITSELF, which is SOLD or AUCTIONED and never
                // both. A Prime's item is its SET; a Kuva or Tenet weapon has
                // no item at all, because the valence it rolled is part of what
                // changes hands, so it is auctioned like a riven.
                "market_slug": wfsim_engine::data::market::weapon_slug(&w.id),
                "market_auction": wfsim_engine::data::market::adversary_auction(&w.id)
                    .map(|(kind, slug)| json!({ "type": kind, "slug": slug })),
                // WHOSE RIVEN THIS IS. A riven belongs to a weapon FAMILY, not
                // to one entry in it: *"Riven mods can be used on variants of a
                // particular weapon, including MK1, Prime, Vandal, Wraith, Dex,
                // Prisma, Mara, and Syndicate variants"* — and each variant
                // carries its OWN disposition, so the same card reads different
                // numbers on each (wiki `Riven Mods`). A weapon that declares
                // none is its own family, which is what an entry with no
                // variants means.
                "riven_family": w.riven_family,
                // The riven stat pool this weapon draws from — not always its
                // mod class (a bow's mods are `bow`, its rivens are `rifle`).
                "riven_class": riven_class(w),
                // …minus the stats THIS weapon cannot roll. The pool is per
                // class, but a sentinel weapon has no Zoom and no Recoil, a
                // hit-scan one has no flight speed, an infinite-ammo one has
                // no Ammo Maximum, and a physical stat is out only where the
                // family's evidence says so. Sent as a list rather than a
                // filtered pool so the class table stays shared.
                "riven_excludes": wfsim_engine::build::rivens::excluded_for(&w.id),
                // …and the offered ones nobody has confirmed: legal, and marked.
                "riven_unconfirmed": wfsim_engine::build::rivens::unconfirmed_for(&w.id),
                // WHERE it is fired, when that changes the weapon. Fewer than
                // two means the axis does not exist for it and nothing should
                // offer a choice - the same rule every other axis follows.
                "deployments": wfsim_engine::data::weapons::deployments_of(&w.id),
                // THE VALENCE BONUS this weapon can carry — its progenitor
                // elements and the roll's floor and ceiling. Absent (null) on
                // every weapon that is not an adversary weapon, which is the
                // same "fewer than two means no axis" rule the deployments
                // follow: the block draws only where there is a choice.
                "valence": wfsim_engine::data::weapons::valence_of(&w.id).map(|s| json!({
                    "elements": s.elements,
                    "min": s.min,
                    "max": s.max,
                })),
                // WHAT THIS WEAPON HAS TO SPEND, and it is not 60 for
                // everybody. Capacity is twice the rank, and an ADVERSARY
                // weapon ranks to 40 where everything else stops at 30 — two
                // rank per Forma, so five polarizations buy it 80.
                //
                // `forma_min` is those five, and it is a MASTERY figure rather
                // than a capacity one: a build that would fit in three still
                // pays it, which is what `Investment::polarize_to_max` means
                // and it is the default. Since it is the default, the rank is
                // FIXED before any planning — which is why this can be sent as
                // a number at all rather than as a solver.
                //
                // Sent as the ANSWER, never as the formula. The client carried
                // a literal 60, so a legal Kuva Nukor build read as over
                // capacity in red while the server accepted it, and the auto
                // plan never spent the five Forma the engine already spends.
                "max_rank": max_rank,
                "capacity": cap,
                "forma_min": forma_min,
                // HOW THIS WEAPON CAN BE PLAYED, and which of those a ruler may
                // rank. Derived from its forms and one question about the second
                // one — does entering it cost a gauge you have to earn — so a
                // weapon added later needs no entry anywhere for the board to
                // hold it twice. See `data::weapons::play_modes`.
                "modes": wfsim_engine::data::weapons::play_modes(&w.id)
                    .iter()
                    .filter(|m| m.sustainable)
                    .map(|m| m.id)
                    .collect::<Vec<_>>(),
                // …AND WHICH KIND EACH ONE IS: what a ruler may rank, and what
                // the page labels. NOT the action list — that one names the
                // PRESS and carries the build's own rules, so only the fight
                // can answer it and it comes back on the response.
                "mode_kinds": wfsim_engine::data::weapons::play_modes(&w.id)
                    .iter()
                    .map(|m| (m.id.to_string(), json!(m.mode.id())))
                    .collect::<serde_json::Map<_, _>>(),
                // TWO FACTS ABOUT AMMO, and they were one until 2026-08-04.
                // `has_reserve` is whether there is a pool behind the magazine
                // at all — false only for a sentinel weapon ("Ammo Max: ∞ /
                // Ammo Type: None"), which is what makes the Infinite-ammo box
                // ticked-and-disabled there. `no_resupply` is whether the game
                // gives any way to refill it — false for everything but a
                // ground Arch-Gun, which is removed when empty.
                //
                // Reading one as the other disabled the box on the whole
                // roster, so the only weapon whose ammo you could adjust was
                // the one weapon whose ammo the game does not let you adjust.
                "has_reserve": wfsim_engine::data::weapons::spec(&w.id)
                    .and_then(|s| s.ammo_max)
                    .is_some_and(|a| a > 0.0),
                "no_resupply": wfsim_engine::data::weapons::spec(&w.id)
                    .is_some_and(|s| s.no_resupply),
                // …AND THE CONSEQUENCE, so the page stops deriving it. The two
                // flags above stay because the roster grid reads them, but a
                // CONTROL now asks this. A SETTLED axis is one the weapon
                // decides rather than the reader — derived from the
                // capabilities it lacks, never written down — as
                // `{ "<axis>": [value, "why", overridable] }`, absent when the
                // choice is the reader's.
                //
                // The VALUE keeps its json type: a flag arrives as a bool and a
                // number as a number, so a control can be drawn from it without
                // the page knowing which axis it is looking at.
                //
                // OVERRIDABLE is the third element and it is the guard made
                // visible: it says whether this weapon's class may argue with
                // the rule in a SCENARIO's own house rules, which is true only
                // where the capability's absence is OUR stand-in rather than
                // the game's rule. A reader looking at a greyed row wants to
                // know which of the two they are looking at.
                "settled": wfsim_engine::build::scenario::SCENARIO_AXES.iter()
                    .filter_map(|a| wfsim_engine::build::scenario::settled_for(a, &w.id)
                        .map(|(v, why)| (a.id.to_string(), json!([
                            match v {
                                wfsim_engine::build::scenario::AxisValue::Flag(b) => json!(b),
                                wfsim_engine::build::scenario::AxisValue::Number(n) => json!(n),
                            },
                            why,
                            wfsim_engine::build::scenario::overridable_pairs().iter().any(|(c, id)| {
                                Some(*c) == wfsim_engine::build::scenario::class_of(&w.id) && *id == a.id
                            }),
                        ]))))
                    .collect::<serde_json::Map<_, _>>(),
                // WHICH CLASS'S HOUSE RULES THIS WEAPON READS. The slot, served
                // rather than re-derived, so the page can point a Burston at
                // the `primary` column and an Arch-Gun at its own without
                // holding a copy of the mapping.
                "weapon_class": wfsim_engine::build::scenario::class_of(&w.id),
                // A PASSIVE WE DO NOT MODEL, so the page can say the number is
                // a floor rather than let it read as the weapon's real output.
                // Empty today — Gotva Prime's was the only one and it is
                // modelled now — and kept because the NEXT weapon with a prose
                // passive should have somewhere honest to sit while it waits.
                "passive_unmodeled": false,
                // WHAT THIS WEAPON DOES BEYOND ITS STATS, generated by the
                // engine from the data that implements it — never a sentence
                // stored in the weapon file.
                //
                // EVERY FORM'S, like `unmodeled` beside it and for the same
                // reason: a roster row is one weapon and the reader is owed
                // both halves. It was the base entry's alone, so a passive that
                // belongs to an Incarnon form had nowhere to appear — the
                // Phenmor's spool-down is declared on `phenmor_incarnon` and
                // the page said nothing about it. Deduped, since
                // a group's forms can carry the same perk.
                "passives": passives_of(&w.id),
                // WHAT THIS ENTRY DOES NOT MODEL, verbatim from the weapon file
                // — the one place a weapon yaml carries prose as a value, the
                // way the enemy files already do. A reader is owed the gap in
                // words, not only the number that omits it.
                "unmodeled": w.unmodeled.clone(),
                "live_bugs": w.live_bugs.clone(),
                // THE SAME ADMISSIONS, STRUCTURED. `unmodeled` above is the
                // finished English and stays for everything that just prints
                // it; this carries the reason's TEMPLATE and its parameters, so
                // a localized page translates the template once instead of once
                // per set of numbers (data/unmodelled/reasons.yaml).
                // EVERY FORM'S, exactly like `unmodeled` above — the banner
                // shows one weapon and both halves are the reader's to know
                // about. Taking only the base entry's leaves the two lists
                // different lengths, and the banner then draws three lines for
                // four gaps (`check_disclosure`).
                "unmodeled_parts": wfsim_engine::data::weapons::forms_of(&w.id)
                    .iter()
                    .filter_map(|f| wfsim_engine::data::weapons::spec(f.weapon_id))
                    .flat_map(|s| s.unmodeled_parts.iter())
                    .map(|u| json!({
                    "text": u.text,
                    "reason": u.reason,
                    "template": u.template,
                    "params": u.params,
                })).collect::<Vec<_>>(),
                // The mods this weapon can actually EQUIP, by id.
                // `pool_for_weapon` is the only place that decides and this is
                // it speaking: a client that unioned the class tables and
                // re-applied the rules in JS would be one fact stated twice,
                // and the copy goes stale the moment the engine learns a rule
                // (Amalgam mods off sentinel weapons, ammo mods off an infinite
                // reserve).
                "mods": wfsim_engine::data::mods::pool_for_weapon(&w.id)
                    .iter()
                    .map(|m| m.id)
                    .collect::<Vec<_>>(),
                // …AND WHICH KIND EACH ONE IS, so the page can show the list a
                // mode runs (`data::apl::preset`) without re-deriving the
                // policy from the mode's name.
                "mode_kinds": wfsim_engine::data::weapons::play_modes(&w.id)
                    .iter()
                    .map(|m| (m.id.to_string(), json!(m.mode.id())))
                    .collect::<serde_json::Map<_, _>>(),
                // ...and which of them each EVOLUTION takes away. An equip rule
                // is asked of every firing mode a weapon has, and installing the
                // Incarnon form adds one — so Dual Toxocyst wears a Cannonade
                // until tier 1 goes in (wiki, Semi-Pistol_Cannonade: "must have
                // Semi-Auto trigger type for both firing modes").
                //
                // The CONSEQUENCE, not the rule (see `mods` above): the
                // engine answers "what does picking this cost you", and the
                // picker just subtracts.
                "evo_forbids": evo_forbids(w),
                // A MODULAR WEAPON'S PARTS, and only for one. Absent everywhere
                // else, which is what the page tests to decide whether this
                // weapon needs assembling at all.
                //
                // THE ENGINE ANSWERS WHICH PARTS FIT, for `evo_forbids`' own
                // reason: a grip decides the SLOT, so only half the grips in the
                // game belong on this entry, and a page that re-derived that
                // would go stale the first time a chamber arrives that is
                // primary-only. The DEFAULT rides along because the server uses
                // it for a request that names none — a page that guessed a
                // different one would show a build that is not the one being
                // simulated.
                "assembly": assembly_meta(&w.id),
                // WHICH ARCANES THIS WEAPON MAY SEAT — the CONSEQUENCE,
                // computed by the engine, for `evo_forbids`' and `auras`'
                // reason. Re-deriving it from `equip_classes` on the page is
                // enough only while every narrowing arcane names a class: the
                // eight Kitgun ones narrow by a TRAIT, because no class can say
                // "Kitgun" — a secondary Tombfinger is a `pistol` exactly like
                // a Lex — and a second such rule would go the same way.
                "arcanes": w.arcane_pools
                    .iter()
                    .flat_map(|p| wfsim_engine::data::arcanes::pool_for_weapon(&w.id, p))
                    .map(|a| a.id.clone())
                    .collect::<Vec<_>>(),
                // WHICH AURAS PAY THIS WEAPON — the CONSEQUENCE, computed by
                // the engine, for the same reason `evo_forbids` is: the amp
                // family does not share one gate (Rifle Amp is a MOD POOL and
                // reaches bows and launchers, Dead Eye is a CLASS and does
                // not), and a page that re-derived that rule would go stale the
                // first time an aura arrived with a third kind of gate.
                "auras": wfsim_engine::data::auras::all().iter()
                    .filter(|a| wfsim_engine::data::weapons::spec(&w.id)
                        .is_some_and(|s| a.pays(&s.class,
                            &s.mod_pools.iter().map(|p| p.as_str()).collect::<Vec<_>>())))
                    .map(|a| a.id.clone()).collect::<Vec<_>>(),
                "mod_class": w.mod_pools.last().cloned().unwrap_or_default(),
                "subtype": w.subtype,
                // The RAW class, beside the display one. An arcane's
                // `equip_classes` is keyed on it, and title-casing for display
                // is exactly the kind of transform that makes a comparison
                // silently fail.
                "class": wfsim_engine::data::weapons::spec(&w.id)
                    .map(|s| s.class.clone())
                    .unwrap_or_default(),
                "sentinel": w.sentinel,
                // AN EXALTED WEAPON IS RANKED APART, and the page needs to know
                // which ones without a second list: its numbers are its
                // Warframe's ability's, so it is not a like term with a gun
                // under the same ruler even though the fight is the same fight.
                // `board::builds::carries_wielder` is the same answer the
                // board's door gives, read from one place.
                "exalted": wfsim_engine::board::builds::carries_wielder(&w.id),
                // The EQUIPMENT slot ("primary" / "secondary"), which is what
                // the home grid groups by. `arcane_slot` happens to hold the
                // same string today because a weapon draws its arcane from its
                // own slot — but that is a coincidence of the arcane rule, not
                // a name the UI should be reading for grouping.
                "slot": w.slot,
                "uses_arcane": w.uses_arcane,
                // The POOLS, in slot order — the page draws one picker per
                // entry and sends one arcane per entry.
                "arcane_pools": w.arcane_pools,
                // Evolution tiers THIS weapon has, keyed on its transform group
                // — the page needs it to tell a complete ladder from a partial
                // one, and the count differs per weapon (Laetum 5, a rifle 0).
                "evo_tiers": wfsim_engine::data::evolutions::tier_count(
                    wfsim_engine::data::weapons::spec(&w.id)
                        .and_then(|s| s.transform_group.as_deref())
                        .unwrap_or(&w.id),
                ),
                "uses_evo2": w.uses_evo2,
                // The tier-1 evolution that UNLOCKS the second form. Without
                // it there is nothing to transform into, and the sim already
                // falls back to the base form — but the client was offering
                // "Incarnon cycle" anyway, so the panel said one thing and the
                // run did another. Now it can ask.
                "unlock_evo": form_unlock_evo(w),
                // A sentinel weapon has no arcane slot. This was hardcoded to
                // 1 while every weapon in the roster had one.
                "arcane_slots": w.arcane_pools.len(),
                "image": assets().weapons.get(&w.id),
                // NO `board` HERE. The board changes hourly and `data/` is embedded
                // at COMPILE time, so serving it from meta made every board
                // update a full wasm rebuild — install wasm-bindgen, fetch 300
                // images, recompile — to change a few numbers. It is fetched at
                // runtime from `/board.json` instead (`loadBoard` in app.js),
                // written by `wfsim-board` beside the canonical yaml.
                "innate_polarities": innate_slots_for(&w.id).iter()
                    .map(|p| p.map(|x| format!("{x:?}")))
                    .collect::<Vec<_>>(),
                // …AND THE STANCE SLOT'S OWN, which is not one of those nine:
                // it decides a capacity GRANT rather than a discount, so the
                // page needs it to answer 5 or 10 (`rules::capacity::stance_capacity`).
                "stance_polarity": wfsim_engine::data::weapons::stance_polarity(&w.id)
                    .map(|p| format!("{p:?}")),
                // A STANCE THE WEAPON CANNOT TAKE OFF: the page seats it, offers
                // no removal and no polarity for its slot.
                "fixed_stance": wfsim_engine::data::weapons::spec(&w.id)
                    .and_then(|s| s.fixed_stance.clone()),
                // WHO MAY HOLD IT, and what it is called in whose hands — empty
                // on a weapon anyone carries.
                "wielders": wfsim_engine::data::weapons::spec(&w.id)
                    .map(|s| s.wielders.clone()).unwrap_or_default(),
                "wielder_names": wfsim_engine::data::weapons::spec(&w.id)
                    .map(|s| s.wielder_names.clone()).unwrap_or_default(),
                "forms": w.forms.iter()
                    .map(|(id, name, def)| {
                        // THE ENTRY BEHIND THIS FORM, once. Everything below is
                        // read off it rather than re-derived per field.
                        let s = wfsim_engine::data::weapons::forms_of(&w.id)
                            .iter()
                            .find(|f| f.kind.id() == *id)
                            .and_then(|f| wfsim_engine::data::weapons::spec(f.weapon_id));
                        // WHAT THIS FORM SWINGS, in the three numbers that
                        // decide between melee's seven modes: how many swings,
                        // what they come to, and how long they take. Absent on
                        // every gun.
                        //
                        // THE ENTRY'S OWN, which is what an EMPTY stance slot
                        // fires — a stance in the slot replaces it, and sends
                        // the same three numbers of its own (`stance_combos`).
                        let combo = s.map(|s| &s.attack.combo_script).filter(|c| !c.is_empty())
                            .map(|c| combo_summary(c));
                        json!({
                        "id": id, "name": name, "is_default": def,
                        "combo": combo,
                        // THE TWO FACTS THE SCRIPT CANNOT CARRY, both the
                        // ENTRY's rather than the stance's, and both the reason
                        // a reader would pick this mode over the one beside it.
                        //
                        // A SLAM'S DAMAGE IS NOT IN ITS SWING AT ALL: the
                        // entry states a zero direct vector and 630 Blast in
                        // `radial:`, so a summary counting swing multipliers
                        // says "100% of base" for an attack that deals 300%.
                        // The share is stated separately rather than folded
                        // into `total` because it is also what frees the mode
                        // from the weapon's reach.
                        "slam": s.and_then(|s| s.attack.radial.as_ref())
                            .is_some_and(|r| r.blast_kind
                                == wfsim_engine::model::BlastKind::Slam),
                        // THE DENOMINATOR IS THE WEAPON'S BASE, not this
                        // form's. A heavy slam states a ZERO direct vector —
                        // all of it is in `radial:` — so dividing by the form's
                        // own damage divides by nothing and the share comes
                        // back absent. Every other number on this line is
                        // already a share of the weapon's base, which is what
                        // makes them addable.
                        // WHAT A SWING MULTIPLIER OF 1.0 IS WORTH, as a
                        // share of the weapon's base. A combo script's
                        // multipliers are relative to the ENTRY they are
                        // written in, and a heavy slam's entry states
                        // `damage: { impact: 0.0 }` — the whole attack is its
                        // explosion — so its 1.0 swing is 100% of nothing and
                        // adding it to the explosion's 300% would say 400%.
                        // Every other melee entry carries the weapon's own
                        // vector, where this is 1.0 and nothing moves.
                        "swing_share": s.and_then(|f| {
                            let base: f64 = wfsim_engine::data::weapons::spec(&w.id)
                                .map(|b| b.attack.damage.values().sum())
                                .unwrap_or(0.0);
                            (base > 0.0)
                                .then(|| r3(f.attack.damage.values().sum::<f64>() / base))
                        }),
                        "radial_share": s.and_then(|f| {
                            let base: f64 = wfsim_engine::data::weapons::spec(&w.id)
                                .map(|b| b.attack.damage.values().sum())
                                .unwrap_or(0.0);
                            let r = f.attack.radial.as_ref()?;
                            (base > 0.0).then(|| r3(r.damage.values().sum::<f64>() / base))
                        }),
                        "spends_combo": s.is_some_and(|s| s.attack.spends_combo),
                        // Is this the form the GAUGE switches into? Then it
                        // exists only while its unlock is installed — and a mod
                        // that cannot be worn beside that unlock (`evo_forbids`)
                        // says the weapon does not have one, so the option goes
                        // with it rather than the sim refusing the build later.
                        "gauge_switched": s.is_some_and(wfsim_engine::data::weapons::WeaponSpec::has_gauge),
                        // HOW THIS FORM IS FIRED, and what a gauge costs to
                        // reach it. Sent so the builder can STATE what a mode
                        // is instead of naming it and leaving the reader to
                        // guess — and sent as the DATA
                        // rather than as a sentence, because the sentence has
                        // to be written once per language and not once per
                        // weapon.
                        "trigger": s.map(|x| x.attack.trigger.clone()),
                        "charge_seconds": s.and_then(|x| x.attack.charge_seconds),
                        "gauge": s.and_then(|x| x.gauge_form.as_ref()).map(|g| json!({
                            "charge_on": g.gauge.charge_on,
                            "charges_to_fill": g.gauge.charges_to_fill,
                            "max_rounds": g.gauge.max_rounds,
                            "transmute_in": g.transmute_in_seconds,
                            "transmute_out": g.transmute_out_seconds,
                        })),
                    })})
                    .collect::<Vec<_>>(),
                // Is there a form to TRANSFORM into? Then the sim can run the
                // real two-form loop as a MODE over the forms above; without
                // one the weapon is fired in a single form (`forms` may still
                // hold several — charged vs uncharged is a free choice, not a
                // transformation).
                "has_cycle": w.has_cycle,
                "evolutions": (1u32..=wfsim_engine::data::evolutions::tier_count(evo_group(w)))
                    .map(|tier| json!({
                        "tier": tier,
                        "options": wfsim_engine::data::evolutions::options(evo_group(w), tier)
                            .iter()
                            .map(|e| json!({
                                "id": e.id,
                                // See the `si` on a mod above.
                                "si": wfsim_engine::data::share_order::index_of(&e.id),
                                "name": e.name,
                                "icon": e.icon,
                                "broken": e.currently_broken,
                                // THE CAVEAT BELONGS WHERE THE CHOICE IS MADE.
                                // This perk's flat base damage does not feed
                                // the Condition Overload term, so a reader
                                // comparing it against the tier's other option
                                // sees "+60 base and +33% per status" and
                                // concludes it is strictly better. The stats
                                // panel states it on the CO row; the tile is
                                // where the question actually gets asked.
                                "co_excluded": e.co_base_excludes_this_evolution,
                                // WHAT IT DOES NOT DO YET, on the tile
                                // where the choice is made. An evolution with
                                // an inert effect is otherwise indistinguishable
                                // from a working one: same card, same tier, and
                                // a number that never moves. A tier where two
                                // of three options do nothing is not a choice,
                                // and the player is the last person who should
                                // have to discover that by measuring.
                                // DERIVED from the loaded effects, so it can
                                // never drift from what is actually modelled.
                                "unmodeled": e.unmodeled_effects(),
                                // …and the OTHER admission, which is not a todo:
                                // a clause that cannot pay out in a
                                // single-target fight, carrying the class that
                                // says why (docs/UNMODELLED.md). The mods have
                                // had this split since 2026-08-05; the
                                // evolutions said "not modelled yet" for both.
                                "out_of_scope": e.out_of_scope_effects(),
                                // …and the THIRD, which is not a shortfall at
                                // all: a clause the GAME does not pay out. It
                                // reaches the card because the reader's action
                                // differs — an unmodelled line says wait for
                                // us, this one says do not pick the perk for
                                // that half (MEASUREMENTS M49).
                                "live_bugs": e.live_bugs(),
                                // …AND THE FOURTH, which is not a shortfall
                                // either and is the OPPOSITE advice to a live
                                // bug: the effect works and its own CARD is
                                // wrong about it. A live bug says do not pick
                                // this; a misprint says pick it for a reason
                                // the card does not state (owner:
                                // anything that differs from what the game
                                // DISPLAYS is to be noted).
                                "misprints": e.misprints(),
                                "fully_unmodeled": e.fully_unmodeled(),
                                "desc": e.description.split('\n').collect::<Vec<_>>(),
                                "effects": e.describe(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .filter(|t| !t["options"].as_array().unwrap().is_empty())
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    let enemies: Vec<Value> = enemies()
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "name": e.name,
                "synthetic": e.synthetic,
                // The portrait comes from the enemy's own file, not from
                // assets.yaml: enemy art is wiki-hosted (see the yaml).
                "image": e.image,
                "base_level": e.stats.base_level,
                "can_be_eximus": e.can_be_eximus,
                // DOES THIS UNIT DIE TWICE? The page offers the switch only
                // where there is a second half to ask about.
                "has_spectral_form": e.spectral_form.is_some(),
                // WHAT A SQUAD DOES TO THIS UNIT, and empty on every unit a
                // squad does nothing to. The page offers the control only where
                // there is something to choose, for the reason it offers an
                // Eximus box only where an Eximus exists: a control that cannot
                // change the answer is a claim that it can.
                "squad_health_bonus": e.squad_health_bonus,
                // What the TARGET PICKER searches and shows. A name alone is
                // not enough to pick between units that differ in the two
                // things a build cares about — who they belong to and what
                // they are made of.
                // The COMBAT faction (what a Bane mod answers to), not the
                // scaling one — they differ, and the picker is about what a
                // build cares about.
                "faction": e.combat_faction.clone().unwrap_or_else(|| "unknown".into()),
                "scaling": format!("{:?}", e.scaling_faction).to_lowercase(),
                "health": e.stats.health,
                "shield": e.stats.shield,
                "armor": e.stats.armor,
                "overguard": e.stats.overguard,
                // Known gaps, stated on the card rather than left implicit.
                "unmodeled": e.unmodeled,
                // What cannot be PROC'd on it — a different question from the
                // column below, and it moves the whole status distribution.
                "status_immunities": e.status_immunities,
                // …and whether it takes the PLAYER's buffs away. A Demolisher
                // pulses every 5 s and dispels every Warframe ability in range,
                // so the ability section has to say so rather than let a player
                // tick Roar and be scored without it.
                "nullifies_abilities": e.nullifies_warframe_abilities,
                // …and the THIRD kind, which is neither: the proc lands and its
                // effect does nothing. It stays in the status roll and still
                // counts for Condition Overload, so a reader who saw it beside
                // the immunities would draw the wrong conclusion about both.
                "nullified_status_effects": e.nullified_status_effects,
                "cannot_be_frozen": e.cannot_be_frozen,
                // The post-U36 vulnerability COLUMN (System B), only the
                // entries that are not 1.0 — what this unit takes more or
                // less of, which is half of what picks a build's elements.
                // Keyed by FactionDamageOverride ?? Faction, so a Thrax shows
                // Zariman's Void x1.5 while answering to no faction mod.
                "type_modifiers": wfsim_engine::data::factions::columns_for(e.damage_column_key())
                    .faction
                    .listed()
                    .into_iter()
                    .map(|(t, m)| json!({ "type": t.name(), "mult": m }))
                    .collect::<Vec<_>>(),
                // …AND THE FOURTH KIND, which none of the three above covers: a
                // flat multiplier this unit applies INSIDE the faction bracket,
                // so it is squared on a status exactly as a Bane is. It moves
                // every number on the page and nothing else on this card would
                // mention it, which is the whole reason it is here.
                "faction_bracket_multiplier": e.faction_bracket_multiplier,
                "parts": e.body_parts.iter().map(|b| json!({
                    "name": b.name, "multiplier": b.multiplier, "is_head": b.is_head
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    // THE VULNERABILITY COLUMNS, so an enemy a player BUILDS can name a
    // faction and be shown what that faction means. The table is the whole of
    // what a faction does to incoming damage, and a copy of it in the UI would
    // be a second source for a number the engine already owns.
    let factions: Vec<Value> = wfsim_engine::data::factions::keys()
        .into_iter()
        .map(|k| {
            json!({
                "id": k,
                "modifiers": wfsim_engine::data::factions::column(k)
                    .listed()
                    .into_iter()
                    .map(|(t, m)| json!({ "type": t.name(), "mult": m }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    // Arcanes: every slot found under data/arcanes/ (secondary today,
    // primary next), each entry TAGGED with its slot so the picker can show
    // only the ones a weapon can equip. Per-rank effect lines come from the
    // same describe the model uses, so the picker states what the sim
    // computes. "none" belongs to every slot.
    let mut arcanes_json: Vec<Value> = vec![json!(
        {"id": "none", "name": "None", "image": null, "ranks": [], "max_rank": 0, "rarity": null, "slot": null}
    )];
    for slot in wfsim_engine::data::arcanes::slots() {
    for a in wfsim_engine::data::arcanes::slot_pool(slot) {
        let ranks: Vec<Vec<String>> = (0..=a.max_rank).map(|r| a.describe_at(r)).collect();
        // The verbatim in-game description per rank (X filled) — the display
        // text; `ranks` (model describe lines) stays for search.
        let desc_ranks: Vec<String> = (0..=a.max_rank).map(|r| a.desc_at(r)).collect();
        arcanes_json.push(json!({
            "id": a.id,
            // See the `si` on a mod above.
            "si": wfsim_engine::data::share_order::index_of(&a.id),
            "name": a.name,
            "image": assets().arcanes.get(&a.id),
            // See the `market_slug` on a mod above — absent means it does not
            // trade there, and that absence is the whole rule.
            "market_slug": wfsim_engine::data::market::arcane_slug(&a.id),
            "ranks": ranks,
            "desc_ranks": desc_ranks,
            "max_rank": a.max_rank,
            "rarity": format!("{:?}", a.rarity).to_lowercase(),
            "not_modeled": a.has_unmodeled(),
            // …and the PARTLY-modelled case. Same field name and same
            // meaning as a mod's, so the card renders both with one function:
            // everything else on this arcane works and these do not.
            "unmodeled_effects": a.unmodeled_effects(),
            // WHICH WEAPON CLASSES MAY EQUIP IT. Empty = any weapon whose slot
            // seats it. The page filters its picker on this so the arsenal and
            // the app offer the same set — `data::arcanes::pool_for_weapon` is
            // the engine's own answer and this is it speaking.
            "equip_classes": a.equip_classes,
            "out_of_scope": a.has_out_of_scope(),
            // …and the FOURTH admission, which is not a shortfall: this is
            // modelled, it matches the live game, and it is a bug (M37). A
            // player is reading a number a hotfix can take away, and only the
            // card can tell them which kind of number it is.
            "live_bugs": a.live_bugs,
            // WHICH SEATS IT FITS. Almost always the one directory it is filed
            // under, and a LIST because a Kitgun arcane fits both: a Kitgun is
            // one weapon with a roster entry per slot, so a primary Tombfinger
            // has a PRIMARY arcane seat and a secondary one a SECONDARY seat,
            // and Pax Charge goes in either. Arcane ids are globally unique
            // across slots, so filing the same one in two directories was never
            // an option — the engine answers with `seats` and the page tests
            // membership.
            "seats": a.seats,
            // The DIRECTORY, kept for a page that has not been rebuilt: a
            // stale `site/app.js` reads this field alone and still gets every
            // arcane.
            "slot": slot,
        }));
    }
    }

    json!({
        "weapons": weapons,
        // One pool per mod CLASS present in data/mods/ — a weapon's
        // `mod_class` (derived from its mod_eligibility) indexes into this.
        // Adding data/mods/rifle/ publishes a rifle pool with no code change.
        "mod_pools": wfsim_engine::data::mods::classes()
            .into_iter()
            .map(|c| (c.to_string(), json!(mods_json(&wfsim_engine::data::mods::class_pool(c)))))
            .collect::<serde_json::Map<String, Value>>(),
        "enemies": enemies,
        // OUR RIVEN STAT ID → THE SLUG AN AUCTION SEARCH FILTERS ON. The page
        // sends the stats a riven actually rolled, so the link lands on the
        // rivens that compete with the one on screen instead of on every
        // riven for the weapon — which is also the only way past the
        // auction's own 500-result ceiling.
        "market_riven_stats": wfsim_engine::data::market::riven_stats()
            .map(|(k, v)| (k.to_string(), Value::from(v)))
            .collect::<serde_json::Map<String, Value>>(),
        // WHAT THE WARFRAME BRINGS, so the page can OFFER it rather than make
        // the reader type a number. That is the whole argument for naming these
        // instead of folding them into the custom bonuses: a named shard has a
        // source that can be checked and updated with the wiki; a typed +45%
        // has nothing.
        "auras": wfsim_engine::data::auras::all().iter()
            .filter(|a| wfsim_engine::data::auras::in_fight(a))
            .map(|a| json!({
            "id": a.id,
            "name": a.name,
            "squad_stacking": a.squad_stacking,
        })).collect::<Vec<_>>(),
        "shards": wfsim_engine::data::shards::all().iter().map(|d| json!({
            "id": d.id,
            "name": d.name,
            "colour": d.colour,
            "options": d.options.iter().map(|o| json!({
                "id": o.id,
                "text": o.text,
                "value": o.value,
                "tauforged": o.tauforged,
                "unit": o.unit,
                // WHETHER IT PAYS ANYTHING HERE, and the ENGINE answers —
                // three of these are in scope, transcribed correctly, and still
                // not applied, so `OutOfScope` alone would have offered them as
                // working. A socket that quietly does nothing is worse than one
                // that says so.
                "modelled": o.at(false).unmodelled_reason().is_none(),
                "why_not": o.at(false).unmodelled_reason(),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        // THE FRAMES THE WARFRAME BUILDER SEATS — the home grid and the router
        // need only these three fields; the rest is `/api/warframe/catalog`.
        "warframes": wfsim_engine::data::warframes::warframes().iter().map(|f| json!({
            "id": f.id,
            "name": f.name,
            "image": assets().warframes.get(&f.id),
        })).collect::<Vec<_>>(),
        // THE COMPANIONS A ROBOTIC WEAPON CAN BE HELD BY — a Sentinel or a MOA;
        // their stat block is `sentinel_floor` below.
        "companions": wfsim_engine::data::companions::companions().iter().map(|c| json!({
            "id": c.id,
            "name": c.name,
        })).collect::<Vec<_>>(),
        // THE OPERATOR'S card, its own home group: a player has one Operator.
        "operator_image": assets().operators.get("operator"),
        // THE WIELDER'S ROSTER. Three numbers a weapon perk can ask about; the
        // panel fills its fields from whichever is picked.
        "frames": wfsim_engine::data::tenno::frames()
            .iter()
            .map(|f| json!({
                "id": f.id, "name": f.name,
                "armor": f.armor, "energy": f.energy, "sprint": f.sprint,
            }))
            .collect::<Vec<_>>(),
        // THE FLOOR THE FIGHT STARTS FROM — the neutral wielder, as five
        // numbers. It is `data/tenno/default.yaml`: THE WORST MAX-RANK FRAME
        // THAT DOES NOT EXIST, each stat the lowest any released Warframe has
        // at rank 30, served rather than repeated in the page.
        //
        // WHAT IT IS FOR: the fight's Warframe fields are OVERRIDES, and an
        // override has to be shown against what it overrides or the reader
        // cannot tell "0 because no frame" from "0 because that IS the floor".
        // TWO FLOORS, because there are two kinds of wielder: a Warframe holds
        // most of the roster and a SENTINEL the 21 companion weapons, with
        // different lowest values — 367/130/80 against 250/0/105.
        // **WHAT A FIGHT CONSISTS OF**, and which of it this weapon takes away.
        //
        // `engine::scenario::SCENARIO_AXES` is the one declaration; this
        // states its CONSEQUENCE per weapon, `evo_forbids`' own pattern.
        // Re-deriving the forcing rules on the page is two implementations of
        // one rule, and a forced field looks identical whoever forced it.
        //
        // The FORCED map is per weapon and only carries what is actually
        // forced: `{ "<weapon id>": { "<axis>": [value, "why"] } }`. Absent
        // means the reader's, which is the ordinary case for almost every pair.
        "scenario_axes": wfsim_engine::build::scenario::SCENARIO_AXES.iter().map(|a| {
            json!({
                "id": a.id,
                "group": match a.group {
                    wfsim_engine::build::scenario::Group::Target => "target",
                    wfsim_engine::build::scenario::Group::Engagement => "engagement",
                    wfsim_engine::build::scenario::Group::Wielder => "wielder",
                    wfsim_engine::build::scenario::Group::Squad => "squad",
                },
                "requires": a.requires.iter().map(|r| format!("{:?}", r.cap)).collect::<Vec<_>>(),
                "kind": match a.kind {
                    wfsim_engine::build::scenario::AxisKind::Flag => json!({ "t": "flag" }),
                    wfsim_engine::build::scenario::AxisKind::Number { min, max } =>
                        json!({ "t": "number", "min": min, "max": max }),
                    wfsim_engine::build::scenario::AxisKind::Id => json!({ "t": "id" }),
                    wfsim_engine::build::scenario::AxisKind::Structured => json!({ "t": "structured" }),
                },
            })
        }).collect::<Vec<_>>(),
        // **WHAT A SCENARIO IS ALLOWED TO SAY ABOUT A CLASS IT IS NOT POINTED
        // AT** — the "global edit" the whole-fight panel
        // draws, served rather than derived.
        //
        // `classes` is the order to list them in; `overridable` is the legal
        // (class, axis) pairs, which is the guard: a scenario may argue with
        // OUR stand-in for a mechanic and never with the game's own rule, so
        // "Arch-Guns have infinite ammo in here" is offered and "Sentinels land
        // headshots" is not offered at all. The page draws exactly what is
        // listed, which is why a capability reclassified in the engine moves
        // the editor by itself and a page that re-derived legality would go
        // stale the first time one was.
        //
        // Today it is one pair. That is not a placeholder — it is the honest
        // size of "things the sim simplifies that a fight might reasonably want
        // to unsimplify", and `overridable_pairs` grows only when a capability
        // is deliberately reclassified.
        "class_rules": {
            "classes": wfsim_engine::build::scenario::WEAPON_CLASSES,
            "overridable": wfsim_engine::build::scenario::overridable_pairs()
                .iter()
                .map(|(c, a)| json!([c, a]))
                .collect::<Vec<_>>(),
        },
        "tenno_floor": floor_json(wfsim_engine::data::tenno::default_tenno()),
        "sentinel_floor": floor_json(wfsim_engine::data::tenno::sentinel_wielder()),
        "factions": factions,
        // WARFRAME ABILITY BUFFS, the catalogue the scenario's own section
        // draws from (`data/abilities/`). `value` and `duration_seconds` are the
        // wiki's max-rank figures at 100% strength; the page multiplies by the
        // strength you set, so the numbers it SHOWS are computed on screen from
        // exactly these two fields and nothing hidden.
        //
        // `family` travels because the "only the strongest runs" rule has to be
        // visible while you tick the boxes, not just enforced afterwards — the
        // engine settles it either way (`data::abilities::resolve`), and a page
        // that showed both as active would be lying about a number it printed.
        "abilities": wfsim_engine::data::abilities::all().iter().map(|a| {
            // EVERY BRACKET THE CAST TOUCHES, as (kind, value, element). A LIST
            // because one ability can grant more than one — Redline sets fire
            // rate and reload speed off a single gauge — and the card has to
            // print both or it states half of what the sim runs.
            use wfsim_engine::data::abilities::AbilityEffect as AE;
            let grants: Vec<Value> = a.effects.iter().map(|e| {
                let (kind, v, element) = match *e {
                    AE::FactionDamage(v) => ("faction_damage", v, None),
                    AE::FinalDamage(v) => ("final_damage", v, None),
                    AE::AddElement(t, v, _) => ("add_element", v, Some(t.name())),
                    AE::AmmoEfficiency(v) => ("ammo_efficiency", v, None),
                    AE::FlatCritChance(v) => ("flat_crit_chance", v, None),
                    AE::FireRate(v) => ("fire_rate", v, None),
                    AE::ExtraHit { element, fraction, .. } => ("extra_hit", fraction, Some(element.name())),
                };
                json!({ "kind": kind, "value": v, "element": element })
            }).collect();
            // …and the FIRST one under the `kind`/`element` keys, which is
            // what the page's card reads and what every stored scenario's
            // element pick is keyed off.
            let (kind, element) = match grants.first() {
                Some(g) => (g["kind"].clone(), g["element"].clone()),
                None => (Value::Null, Value::Null),
            };
            json!({
                "id": a.id,
                "name": a.name,
                "frame": a.frame,
                "family": a.family,
                "helminth": a.helminth,
                "value": a.value,
                "duration_seconds": a.duration_seconds,
                // The elements this one lets you CHOOSE (Resupply's ten), empty
                // where it fixes one — the page draws its picker from this.
                "elements": a.elements,
                "class_bonus": a.class_bonus.map(|(c, x)| json!({ "class": c, "x": x })),
                "kind": kind,
                "element": element,
                "grants": grants,
                // Does the page's strength knob move this one? Energized
                // Munitions' 75% is flat and Redline's ramp is a battery, so
                // the card says so instead of quietly showing a number the
                // game never gives.
                "scales_with_strength": a.scales_with_strength,
                // THE SAME TWO ADMISSIONS A MOD AND AN ARCANE CARD CARRY, under
                // the same keys, so the page renders all three with one
                // function. Xata's Whisper is the first ability with either:
                // its Void proc is a Bullet Attractor this sim has nothing to
                // point at, and its Blast interaction is DE's own bug.
                "unmodeled_effects": a.unmodelled,
                "live_bugs": a.live_bugs,
                "url": a.url,
            })
        }).collect::<Vec<_>>(),
        // A RIVEN's card image, once. Rivens are made by the visitor, so no
        // per-riven entry could exist in data/assets.yaml — the game draws
        // every riven with the same card and so does this.
        "riven_image": assets().mods.get("riven"),
        // Arcanes mirror the mod pool: per-rank effect lines (`ranks[r]`),
        // max_rank, rarity — so the web picker searches effects and the slot
        // steps ranks with the strength updating per rank. `arcane_rank` in
        // the sim request selects the modeled rank (default: max).
        "arcanes": arcanes_json,
        // Riven stat pools, keyed by mod class. The builder needs the whole
        // pool to offer choices; the VALUES it must ask for, because the
        // formula lives in one place (`/api/riven`).
        "riven_stats": wfsim_engine::data::mods::classes()
            .into_iter()
            .filter_map(|c| {
                let p = wfsim_engine::build::rivens::pool(c);
                (!p.is_empty()).then(|| {
                    (
                        c.to_string(),
                        json!(p
                            .iter()
                            .map(|s| json!({
                                "id": s.id,
                                // See the `si` on a mod: a riven's SHAPE names
                                // its stats and a share link carries the shape.
                                "si": wfsim_engine::data::share_order::index_of(&s.id),
                                "text": s.text, "base": s.base,
                                "prefix": s.prefix, "suffix": s.suffix,
                                // BOTH DIRECTIONS. The picker draws two lists
                                // out of one pool: `malus` false = bonus-only,
                                // `bonus` false = malus-only.
                                "malus": s.malus, "bonus": s.bonus,
                                // An element stat makes the card ORDERED — see
                                // the mod's own `elemental`.
                                "elemental": s.kind == "elemental_damage_bonus",
                                "modeled": s.kind != "unmodelled",
                                // A Riven Splicer's stat: one a card, and unmeasured.
                                "spliced": s.spliced,
                            }))
                            .collect::<Vec<_>>()),
                    )
                })
            })
            .collect::<serde_json::Map<String, Value>>(),
        "riven_rules": {
            "roll_min": wfsim_engine::build::rivens::ROLL_MIN,
            "roll_max": wfsim_engine::build::rivens::ROLL_MAX,
            "max_rank": wfsim_engine::build::rivens::MAX_RANK,
            // The polarities a riven rolls (wiki: one of three).
            "polarities": ["madurai", "vazarin", "naramon"],
            "mastery_min": 8,
            "mastery_max": 16,
        },
        // Choosable evolution tiers from data/evolutions/*.yaml (tier 1 =
        // the Incarnon Form unlock — deselecting it means no transformation,
        // so the panel/sim fall back to the base form). Every tier also gets
        // an implicit EMPTY choice in the UI (nothing installed); `broken` =
        // wiki-flagged non-functional — the engine applies ZERO for those,
        // and the UI must say so in red. `desc` lines are the verbatim
        // effect text (like the mod/arcane cards).
        // THE OFFICIAL SCENARIOS (data/benchmarks/). They are not presets: no
        // weapon owns them, nothing stores them, and nobody can edit them —
        // they exist so a number has a ruler someone else can pick up. The
        // client shows them on every weapon alongside the player's own.
        // HOW MANY MAIN SLOTS A BUILD HAS. Not the admission rule — that is the
        // benchmark's, and travels with it below — just the one number the page
        // needs to count filled slots against.
        "board_build_mods": wfsim_engine::board::builds::MAIN_SLOTS,
        // WHAT A BUILD CONSISTS OF, from the one place that declares it
        // (`engine::builds::BUILD_AXES`). Served for the same reason
        // `board_build_mods` is: the page and the worker each carry a table of
        // their own spellings, and this is what a check measures those
        // tables against, so an axis added in Rust cannot stay invisible to a
        // surface that never heard of it.
        // HOW MANY BODIES A FIGHT CAN HOLD — declared once, in the engine
        // (`formation::MAX_BODIES`), and served so the canvas carries no copy
        // of it. A number written out on the page is the same shape as every
        // axis-list bug this file guards against: two declarations of one fact,
        // and the day one moves the other is silently wrong.
        "max_bodies": wfsim_engine::formation::MAX_BODIES,
        // …AND HOW MANY GUNS, for the same reason and out of the same rule.
        // The roster is a list on the page and a `Vec` in the fight, and the
        // ceiling on it is the engine's (`fight::MAX_COMBATANTS`).
        "max_combatants": wfsim_engine::fight::MAX_COMBATANTS,
        // WHICH TRIGGERS A FIGHT CAN SWITCH OFF (`engine::buff_events`), in the
        // order the panel draws them and with the group each sits under — a
        // vocabulary rather than a list on the page, because two declarations
        // of one set is one that goes stale.
        "buff_triggers": wfsim_engine::data::buff_events::ALL.iter()
            .map(|(id, group)| json!({ "id": id, "group": group }))
            .collect::<Vec<_>>(),
        // WHAT A RUN CAN BE JUDGED BY, from the one table that declares it
        // (`engine::metrics`). A vocabulary rather than a list on the page:
        // the Measure control, the headline's unit and the gain scan's label
        // all resolve an id against this, so a metric added here reaches every
        // surface without any of them naming it.
        "metrics": wfsim_engine::rules::metrics::ALL,
        "metric_default": wfsim_engine::rules::metrics::DEFAULT,
        // THE PART VALUE ANALYSIS'S CEILING, so the page refuses a selection
        // before it simulates 2^k subsets the endpoint would then reject.
        "shapley_max_participants": crate::shapley::MAX_PARTICIPANTS,
        // HOW BIG A BODY IS, because the PAGE draws the same floor the engine
        // fights on: the muzzle sits one radius forward, two circles touch at
        // two radii, and the distance a reader is shown is the gap between
        // their SURFACES. The page carried its own copy of this number and the
        // two drifted — the arena called a contact-range fight 0.1 m and the
        // crowd 2.6 m apart where the engine had 0 and 2.5.
        "body_radius_m": wfsim_engine::rules::space::BODY_RADIUS_M,
        // The cards the quick calc tries at every rank unless the page's own
        // list says otherwise, and the spelling a lower rank travels in.
        "every_rank": wfsim_engine::data::mods::every_rank(),
        "rank_mark": wfsim_engine::data::mods::RANK_MARK.to_string(),
        "build_axes": wfsim_engine::board::builds::BUILD_AXES.iter().map(|a| json!({
            "id": a.id,
            "request_field": a.request_field,
            // A WORD AND NOT A BOOLEAN, because there are three answers now and
            // the third one is the trap: a reader that kept testing this for
            // truth would read every string as "yes", including "never".
            "on_board": match a.on_board {
                wfsim_engine::board::builds::OnBoard::Fixed => "never",
                wfsim_engine::board::builds::OnBoard::Kept => "always",
                wfsim_engine::board::builds::OnBoard::KeptWhereTheRulerCannot =>
                    "where_the_ruler_cannot",
            },
        })).collect::<Vec<_>>(),
        "benchmarks": wfsim_engine::board::benchmarks::all().iter().map(|b| json!({
            "primary": b.primary, "id": b.id,
            "name": b.name,
            // HOW MANY BUILDS THE RUN THAT WROTE THIS BOARD READ. Paired with
            // the library's own size (`/api/board/pending`), it is what lets a
            // STATIC board say how far behind it is — see `data::boards::Board`.
            "submissions": wfsim_engine::data::boards::of(&b.id)
                .map(|x| x.submissions).unwrap_or(0),
            // …AND WHEN IT WAS SCORED. The count says how far behind the board
            // is in BUILDS; this says how old its numbers are, which no
            // fingerprint can answer — a fingerprint says whether an input
            // moved, never when a measurement was taken. Zero is "unknown", not
            // 1970: a board written before the field existed carries none.
            "scored_at_epoch_seconds": wfsim_engine::data::boards::of(&b.id)
                .map(|x| x.scored_at_epoch_seconds).unwrap_or(0),
            // The standard AT LENGTH — the name is the same thing in one line.
            // A reader deciding whether a ranking answers their question needs
            // the terms, and a term that only exists in a yaml comment is one
            // nobody can check the board against.
            "rules": b.rules,
            // …AND WHAT THE WORD "STANDARD" IN THE NAME RESTS ON. Empty on a
            // ruler that makes no such claim, which the page reads as nothing
            // to show rather than as a missing field.
            "standard": b.standard,
            // WHAT THIS RULER ADMITS, so the page can say what a build is still
            // missing instead of letting the server refuse in silence. It rides
            // with the benchmark because it IS the benchmark's — a second ruler
            // will answer differently and the page must not assume otherwise.
            "build": {
                "mods": b.build.mods,
                "evolutions": b.build.evolutions,
                "arcanes": b.build.arcanes,
                "exilus": b.build.exilus,
            },
            "scenario": b.scenario,
        })).collect::<Vec<_>>(),
        // DE's own icon per damage type — the meter and the charts colour and
        // label by TYPE, so both halves of that (colour in style.css, file
        // here) come from the same wiki module.
        "damage_type_icons": assets().damage_types,
        "defaults": {
            "weapon": default_weapon_id(),
            // Per-weapon, because "the form this is played in" is: the
            // Incarnon cycle where there is one, and the weapon's own default
            // form (`default_form` in data/weapons) where there is not. A
            // fixed string could only ever be right for one of the two.
            "form": "default",
            // The page starts EMPTY (user decision): no mods, no arcane, no
            // evolutions — a bare weapon. Reference builds live as presets /
            // data/builds, not as the initial state.
            "evolutions": {},
            "arcane": "none",
            "enemy": "thrax_centurion",
            "level": 9999,
            "steel_path": true,
            // NULL, not a boolean — "whatever this unit is by default", which
            // `parse_fight` resolves to `can_be_eximus`. A fixed `true` would
            // be a lie about the default target (a Thrax has no Eximus
            // variant) and a fixed `false` would contradict the rule that the
            // elite unit is the one you meet. The page renders the effective
            // answer per enemy and only stores a boolean once you say
            // otherwise.
            "eximus": Value::Null,
            "headshot_pct": 100.0,
            // HOW FAR AWAY THE TARGET STANDS, in metres — the fight's 2D layer
            // (`engine::space`). SUPERSEDED by `player_at`/`target_at`, and
            // kept only so a scenario saved before those existed opens as the
            // fight it was; the page has not sent it since 2026-08-16, when the
            // canvas became the only place a position is set.
            //
            // It is a GAP — surface to surface — which is both what it always
            // meant ("0 is point blank") and what the arena shows today.
            "distance": 0.0,
            // ---- the TENNO, the fight's other actor. Every field here is
            // `data/tenno/default.yaml`'s: the NEUTRAL player, aiming, no
            // frame chosen, no ability running. Aiming is true because that is
            // the sim's behaviour before the knob existed, so no stored preset
            // silently changes meaning; the rest are false/0 because "some
            // max-rank neutral Warframe" is doing none of them and wearing
            // nothing.
            "aiming": true,
            "invisible": false,
            "airborne": false,
            "overshields": false,
            "channeling": false,
            "melee_equipped": true,
            // The LOADOUT: false = a full one, which is the standing ruling and
            // the fight the board is scored under.
            "solo_weapon": false,
            // THE FIGHT'S OWN STAT BONUSES, all zero: whatever this weapon is
            // handed by something that is not its build. Empty is a fight that
            // hands it nothing, which is every ruler and every stored scenario.
            "extra_stats": {},
            // NO WARFRAME FIELDS AT ALL, which is what makes the floor reach a
            // fight. These were `0.0`, and 0 is an
            // OVERRIDE — the page carried them into every request, so the
            // neutral wielder in `data/tenno/default.yaml` was overwritten with
            // zero before anything could read it. "The neutral Tenno has 105
            // armor" was true of the data and false of what the simulator ran.
            //
            // An ABSENT key is the fallback: `tenno_from` reads
            // `get_f64(v, "wf_armor", t.armor)`, which has always meant "the
            // floor unless told otherwise" and never got the chance.
            // INFINITE AMMO by default — see `simulate_json` for why.
            "infinite_ammo": true,
            // …AND THE ECONOMY BEHIND IT, which decides nothing until that box
            // is unticked: the bodies drop as they do in game, and this arena's
            // Tenno collects at any distance because it never walks.
            "ammo_drops": true,
            "pickup_range_m": null,
            "landscape": false,
            // A THRAX'S SECOND HALF, off — see `parse_fight`.
            "spectral_form": false,
            // A GUARDIAN EXIMUS'S 90% aura, off — every number this app has
            // published assumes nobody is shielding the target.
            "guardian_aura": false,
            // AN ANCIENT PROTECTOR'S 800%-of-health Overguard, off.
            "ancient_protector_aura": false,
            // Test precision, and the optimizer's last
            // round is the run count on the top 10. Kept in step with
            // `simulate_json` / `parse_optimize`, whose own fallbacks are what
            // an API caller naming none of these gets.
            //
            // 100 RUNS, and DECOUPLED FROM THE SCENARIO.
            // It briefly matched the rulers' 1,000 so a first number would be
            // comparable with the board — but that made one field answer two
            // questions, and only one of them is the fight's. The RULERS
            // still say 1,000
            // in their own yaml and the SCORER still uses it, which is what
            // makes a board row reproducible; the page measures at whatever
            // the reader set, and the box says so.
            // WHAT THE RUN IS JUDGED BY. EVERY scenario carries one, so
            // whatever ranks — the headline number, the picker's gain scan, a
            // ruler's published row — ranks by the same thing. The default is
            // the table's, not a literal: `engine::metrics` is where a metric
            // is declared and where the first one is chosen.
            "metric": wfsim_engine::rules::metrics::DEFAULT,
            // 180 s, the same length as the official rulers. A default that disagreed with the board made every
            // first comparison a puzzle, and on a build that compounds the gap
            // is not small — the Felarx's board score moved 30% on this number
            // alone. Only the DEFAULT moves: a saved scenario carries its own
            // duration and keeps it.
            "duration": 180.0,
            "runs": 100,
            // The final-round contract, for an API caller. The WEB does not
            // read these: `final_runs` is the scenario's `runs` and
            // `finalists` is a fixed 10, because neither is a setting the
            // optimizer tab offers any more.
            "final_runs": 1000,
            "finalists": 10,
            "mods": [],
        },
    })
}

/// A COMBO IN THE THREE NUMBERS THAT DECIDE BETWEEN MODES: how many swings, what
/// they come to, and how long they take at 1.0x attack speed.
///
/// THE SLAM IS COUNTED AND TAKES NO TIME, which is how the wiki's own module
/// states it — a combo's `Duration` is its direct swings', and the slam three of
/// Crushing Ruin's four end on rides the swing it is listed with.
fn combo_summary(script: &[wfsim_engine::model::ComboHit]) -> Value {
    let swings = script.iter().filter(|h| h.slam_multiplier.is_none()).count();
    let total: f64 = script
        .iter()
        .map(|h| (h.multiplier + h.slam_multiplier.unwrap_or(0.0)) * f64::from(h.hits))
        .sum();
    // WHAT ACTUALLY TELLS THE MODES APART, beside the three numbers. Every melee
    // mode is free to hold and every one of them is ranked, so a line saying
    // either is a line that distinguishes nothing. What a
    // reader is choosing between is how much of the room a swing reaches and
    // what it forces on whatever it lands on.
    let spins = script.iter().filter(|h| h.all_around).count();
    let mut procs: Vec<&str> = Vec::new();
    for h in script {
        for p in &h.forced_procs {
            if !procs.contains(&p.as_str()) {
                procs.push(p.as_str());
            }
        }
    }
    // A SCRIPT AND NOTHING ELSE, which is what lets a STANCE send the same
    // shape: `stance_combos` has no weapon entry behind it, so anything read
    // off the entry (the explosion's share, whether the counter is spent)
    // belongs on the FORM instead — otherwise equipping a stance would erase
    // exactly the facts this block exists to state.
    json!({
        "swings": swings, "total": r3(total), "seconds": r3(seconds_of(script)),
        "spins": spins, "procs": procs,
    })
}

fn seconds_of(script: &[wfsim_engine::model::ComboHit]) -> f64 {
    script.iter().map(|h| h.windup_seconds + h.delay_seconds).sum()
}
