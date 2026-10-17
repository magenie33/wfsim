// SPDX-License-Identifier: AGPL-3.0-or-later
//! ARCHON SHARDS — five sockets on a Warframe, and a PICK rather than a mod.
//!
//! A socket holds ONE of its colour's effects, and a TAUFORGED shard of that
//! colour grants the larger number (*"50% higher stat bonuses"*). So a frame
//! carries up to five `(colour, effect, tauforged)` picks, which is what
//! [`ShardPick`] is.
//!
//! It rides on the fight's [`Tenno`] for the same reason an aura does — it is
//! the WARFRAME's and not the weapon's — so it reaches the simulator and the
//! optimizer through `parse_fight` and neither module learns it exists.
//!
//! EVERY EFFECT IS LISTED, INCLUDING THE ONES THAT PAY NOTHING HERE. A shard
//! left out of the data reads exactly like one that does nothing, and the
//! reader cannot tell those apart — which is the rule `docs/UNMODELLED.md` is
//! built on. `OutOfScope` is a value, not an omission.
//!
//! [`Tenno`]: crate::data::tenno::Tenno

use serde::Deserialize;

/// What one shard effect does, typed. An unknown kind is refused.
#[derive(Debug, Clone, PartialEq)]
pub enum ShardEffect {
    /// Straight into a mod bucket, for the weapon class named.
    StatusChance(f64, String),
    CritChance(f64, String),
    /// Vital Sense's bucket — *"additive with similar buffs such as Organ
    /// Shatter"* (W`Archon_Shard`).
    CritDamage(f64),
    /// Status DAMAGE for ONE element: *"additive with other sources of Status
    /// Damage such as Rifle Elementalist"*, paid to that element's procs only.
    StatusDamageOfType(f64, String),
    /// Element damage for ONE type on ONE weapon slot (Violet's Electricity),
    /// which "will combine with Mods" — see [`Tenno::shard_elements`].
    ///
    /// [`Tenno::shard_elements`]: crate::data::tenno::Tenno::shard_elements
    ElementDamageOfType(f64, String),
    /// A Warframe stat the WIELDER BUILD resolves (`data::warframes::resolve`),
    /// so a fight already holds it in the Tenno's own field: strength and
    /// duration for casts and Exalted weapons, casting speed for cast time,
    /// health and shields for the cards that read them.
    WielderStat(String),
    /// Azure's flat additions to the two Warframe stats a weapon arcane reads.
    Armor(f64),
    EnergyMax(f64),
    /// THE ONE THAT IS NOT A BONUS. Emerald: *"Increase max stacks of Corrosion
    /// Status by +2 (+3)"*, and the page is explicit that a WEAPON's corrosion
    /// may exceed ten because of it. It moves a CEILING, so it is the only
    /// effect here that can change what a build is able to do rather than how
    /// much it does — 10 stacks strip 80% of armour and 14 strip all of it.
    StatusStackCap(f64, String),
    /// Crit chance earned per kill on a target carrying a status. A stacking
    /// buff, which the engine has the shape for and this does not yet build.
    CritOnStatusKill(f64, String),
    /// Real, and about a layer this simulator does not have. It carries the
    /// entry's OWN reason: five of these are five different admissions
    /// ("no melee in this arena", "nobody shoots back", "pickups"), and a
    /// single fixed sentence for all of them says none of the five.
    OutOfScope(String),
}

#[derive(Debug, Clone, Deserialize)]
struct RawEffect {
    id: String,
    text: String,
    kind: String,
    #[serde(default)]
    applies_to: Option<String>,
    /// THE EQUIPMENT SLOT this effect pays, or every slot when absent. It is
    /// its own field because `applies_to` already carries the ELEMENT, and
    /// "Primary Electricity Damage" names both.
    #[serde(default)]
    slot: Option<String>,
    value: f64,
    tauforged: f64,
    /// `pct` or `flat`. DECLARED rather than derived: the obvious rule ("under
    /// 1 is a percent") is wrong on Amber's *"+100% Health Orb Effectiveness"*,
    /// whose value is exactly 1.0.
    unit: String,
    /// A `condition:` spelling (`TennoCondition::from_id`) under which the
    /// value DOUBLES — Violet's *"When Max Energy is over 500, the damage
    /// boost doubles"*.
    #[serde(default)]
    doubled_when: Option<String>,
    #[serde(default)]
    per_shard: Option<PerShard>,
}

/// *"Gain an additional +10% (+15%) per Crimson, Azure, or Violet Archon Shard
/// equipped"*: a term per socket of the named colours, sized by THIS shard's
/// Tauforging. The socket carrying it is one of them, as the sentence reads.
#[derive(Debug, Clone, Deserialize)]
pub struct PerShard {
    pub value: f64,
    pub tauforged: f64,
    pub colours: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawShard {
    id: String,
    name: String,
    colour: String,
    effects: Vec<RawEffect>,
}

/// One effect a socket of this colour may be set to.
#[derive(Debug, Clone)]
pub struct ShardOption {
    pub id: String,
    /// The card's own sentence.
    pub text: String,
    pub value: f64,
    pub tauforged: f64,
    pub kind_of: fn(f64, Option<&str>) -> ShardEffect,
    pub applies_to: Option<String>,
    pub slot: Option<String>,
    pub unit: String,
    pub doubled_when: Option<crate::model::TennoCondition>,
    pub per_shard: Option<PerShard>,
}

impl ShardOption {
    /// The effect at the size actually socketed.
    pub fn at(&self, tauforged: bool) -> ShardEffect {
        let v = if tauforged { self.tauforged } else { self.value };
        (self.kind_of)(v, self.applies_to.as_deref())
    }

}

#[derive(Debug, Clone)]
pub struct ShardDef {
    pub id: String,
    pub name: String,
    pub colour: String,
    pub options: Vec<ShardOption>,
}

/// ONE SOCKET: a colour, which of its effects it is set to, and whether the
/// shard is Tauforged.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
pub struct ShardPick {
    /// The shard's id, e.g. `emerald_archon_shard`.
    pub shard: String,
    /// The effect id inside it, e.g. `corrosion_stack_cap`.
    pub effect: String,
    #[serde(default)]
    pub tauforged: bool,
}

fn kind_fn(kind: &str) -> fn(f64, Option<&str>) -> ShardEffect {
    match kind {
        "status_chance" => |v, a| ShardEffect::StatusChance(v, a.unwrap_or("").into()),
        "crit_chance" => |v, a| ShardEffect::CritChance(v, a.unwrap_or("").into()),
        "crit_damage" => |v, _| ShardEffect::CritDamage(v),
        "status_damage_of_type" => |v, a| ShardEffect::StatusDamageOfType(v, a.unwrap_or("").into()),
        "element_damage_of_type" => |v, a| ShardEffect::ElementDamageOfType(v, a.unwrap_or("").into()),
        "wf_armor" => |v, _| ShardEffect::Armor(v),
        "wf_energy" => |v, _| ShardEffect::EnergyMax(v),
        "status_stack_cap" => |v, a| ShardEffect::StatusStackCap(v, a.unwrap_or("").into()),
        "crit_chance_on_heat_kill" => |v, a| ShardEffect::CritOnStatusKill(v, a.unwrap_or("").into()),
        "ability_strength" => |_, _| ShardEffect::WielderStat("ability_strength".into()),
        "ability_duration" => |_, _| ShardEffect::WielderStat("ability_duration".into()),
        "casting_speed" => |_, _| ShardEffect::WielderStat("casting_speed".into()),
        "wf_health" => |_, _| ShardEffect::WielderStat("health".into()),
        "wf_shield" => |_, _| ShardEffect::WielderStat("shield".into()),
        // NAMED, not silent: a real effect about a layer this simulator does
        // not have, and saying which layer is the whole value of listing it.
        "ability_damage" => |_, _| ShardEffect::OutOfScope(
            "Ability Damage — no ability deals damage in this fight, and the card excludes Exalted weapons".into()),
        "out_of_scope" => |_, a| ShardEffect::OutOfScope(
            a.unwrap_or("not a weapon-damage quantity").into()),
        other => panic!("unknown shard effect kind: {other}"),
    }
}

impl ShardEffect {
    /// WHY THIS PAYS NOTHING HERE, or `None` when it does pay.
    ///
    /// `OutOfScope` alone could not answer this: an effect can be a real
    /// weapon-damage quantity, transcribed correctly, and STILL not applied. A
    /// rule stated and not applied reads, to anyone auditing, as a rule being
    /// applied.
    pub fn unmodelled_reason(&self) -> Option<String> {
        match self {
            ShardEffect::OutOfScope(w) => Some(w.clone()),
            // A stacking buff earned per KILL of a Heat-affected enemy. A fight's
            // kill does not yet say which statuses the body carried, nor whether
            // the killing blow applied Heat, which the card also counts.
            ShardEffect::CritOnStatusKill(..) => {
                Some("earned per kill of a Heat-affected enemy — a kill here does not yet say which statuses the body carried".into())
            }
            _ => None,
        }
    }
}

pub fn all() -> &'static [ShardDef] {
    use std::sync::OnceLock;
    static S: OnceLock<Vec<ShardDef>> = OnceLock::new();
    S.get_or_init(|| {
        crate::data::files_under("shards/")
            .map(|(p, text)| {
                let r: RawShard = serde_norway::from_str(text)
                    .unwrap_or_else(|e| panic!("{p}: {e}"));
                ShardDef {
                    id: r.id,
                    name: r.name,
                    colour: r.colour,
                    options: r
                        .effects
                        .into_iter()
                        .map(|e| ShardOption {
                            kind_of: kind_fn(&e.kind),
                            id: e.id,
                            text: e.text,
                            value: e.value,
                            tauforged: e.tauforged,
                            applies_to: e.applies_to,
                            slot: e.slot,
                            unit: e.unit,
                            doubled_when: e.doubled_when.as_deref().map(|c| {
                                crate::model::TennoCondition::from_id(c)
                                    .unwrap_or_else(|| panic!("{p}: unknown condition `{c}`"))
                            }),
                            per_shard: e.per_shard,
                        })
                        .collect(),
                }
            })
            .collect()
    })
}

/// The effect a pick resolves to, or `None` if the pick names nothing.
pub fn resolve(pick: &ShardPick) -> Option<ShardEffect> {
    let d = all().iter().find(|s| s.id == pick.shard)?;
    let o = d.options.iter().find(|o| o.id == pick.effect)?;
    Some(o.at(pick.tauforged))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SIX COLOURS, EVERY EFFECT TYPED, AND TAUFORGED IS 1.5x.
    ///
    /// The last one is the wiki's own rule — *"Tauforged variants … have 50%
    /// higher stat bonuses"* — and it is asserted over every option rather than
    /// on a named one, so a transcription that fat-fingered a pair is caught by
    /// the arithmetic instead of by somebody re-reading the page.
    #[test]
    fn every_shard_effect_is_typed_and_tauforged_is_half_again() {
        let s = all();
        assert_eq!(s.len(), 6, "six colours");
        let mut opts = 0;
        for d in s {
            for o in &d.options {
                opts += 1;
                let _ = o.at(false);
                let _ = o.at(true);
                // 1.5x, AND ROUNDED UP WHERE THE QUANTITY IS WHOLE. The Topaz
                // shard's "Gain 1 (2) Max Health per Blast kill" is the case:
                // 1.5 health is not a thing, so DE prints 2. Asserting the bare
                // ratio reddens on it, which is how the rounding was found
                // — the exception is stated rather than the
                // assertion loosened.
                let exact = o.value * 1.5;
                assert!(
                    (o.tauforged - exact).abs() < 1e-6
                        || (o.value.fract() == 0.0 && o.tauforged == exact.ceil()),
                    "{}/{}: tauforged {} is neither 1.5x {} nor its ceiling",
                    d.id, o.id, o.tauforged, o.value
                );
            }
        }
        assert!(opts >= 27, "every effect on every colour: {opts}");
    }

    /// THE ONE THAT MOVES A CEILING, and the reason this family was built.
    ///
    /// *"Increase max stacks of Corrosion Status by +2 (+3)"*, and the page is
    /// explicit that a WEAPON's corrosion may exceed ten because of it. Five
    /// Tauforged sockets are +15, which takes the cap from 10 to 25 — and the
    /// armour formula is `1 - (0.20 + 0.06 x stacks)`, so 14 stacks strip all
    /// of it where 10 strip 80%.
    #[test]
    fn the_emerald_shard_raises_the_corrosion_ceiling() {
        let pick = ShardPick {
            shard: "emerald_archon_shard".into(),
            effect: "corrosion_stack_cap".into(),
            tauforged: false,
        };
        assert_eq!(resolve(&pick), Some(ShardEffect::StatusStackCap(2.0, "corrosion".into())));
        let tau = ShardPick { tauforged: true, ..pick };
        assert_eq!(resolve(&tau), Some(ShardEffect::StatusStackCap(3.0, "corrosion".into())));
        // …and a pick that names nothing resolves to nothing rather than to a
        // zero, which would read as an effect that pays nothing.
        assert_eq!(resolve(&ShardPick {
            shard: "emerald_archon_shard".into(),
            effect: "no_such_effect".into(),
            tauforged: false,
        }), None);
    }
}
