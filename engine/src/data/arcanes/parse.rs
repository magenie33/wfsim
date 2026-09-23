use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct ArcaneFile {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) rarity: String,
    pub(super) max_rank: u32,
    /// Weapon trait required for the effects to apply (Akimbo Slip Strike →
    /// `dual_pistols`); calc-layer gate like a mod's `requires`.
    #[serde(default)]
    pub(super) requires: Option<String>,
    #[serde(default)]
    pub(super) equip_classes: Vec<String>,
    #[serde(default)]
    pub(super) equip_traits: Vec<String>,
    #[serde(default)]
    pub(super) seats: Vec<String>,
    /// Custom perk implementation id (Secondary Enervate's ramp/reset lives
    /// in `engine::perks`, not in the declarative effect vocabulary).
    #[serde(default)]
    pub(super) perk: Option<String>,
    /// Verbatim in-game text, rank-varying numbers as `X` (schema).
    #[serde(default)]
    pub(super) description: Option<String>,
    /// See [`ArcaneDef::live_bugs`].
    #[serde(default)]
    pub(super) live_bugs: Vec<String>,
    pub(super) effects: Vec<Value>,
}

pub(super) fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}
pub(super) fn u(v: &Value, k: &str) -> u32 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0) as u32
}
pub(super) fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

pub(super) fn scale(v: &Value) -> Scale {
    let ranks = v.get("ranks").and_then(Value::as_sequence).map(|seq| {
        seq.iter().filter_map(Value::as_f64).collect::<Vec<f64>>()
    });
    Scale {
        rank0: f(v, "rank0"),
        rank_max: f(v, "rankMax").unwrap_or(0.0),
        ranks,
    }
}

/// The `condition:` an effect declares, if it declares one.
pub fn arc_condition(v: &Value) -> Option<ArcCondition> {
    let s = v.get("condition").and_then(Value::as_str)?;
    Some(match s {
        "target_has_10_radiation_stacks" => ArcCondition::TargetRadiationStacks(10),
        // The three Tenno states, NAMED rather than matched by shape: a new one
        // has to be added here, which is the whole mechanism.
        "sliding_or_aim_gliding"
        | "overshields"
        | "buffing_ally_warframes"
        // AIRBORNE, which is the same family: a state of the TENNO that this
        // arena does not model, so the house reading treats it as satisfied. It
        // costs nothing to be optimistic here because Pax Soar's three grants —
        // accuracy, recoil and aim glide — all pay zero either way; the gate is
        // recorded so that the day one of them can be paid, the condition is
        // already the right kind.
        | "airborne" => ArcCondition::AssumedTennoState,
        other => ArcCondition::Unknown(other.to_string()),
    })
}

pub(super) fn effect(v: &Value) -> Option<ArcEffect> {
    let kind = s(v, "kind")?;
    let inert = |why: &str| Some(ArcEffect::Inert(why.to_string()));
    Some(match kind {
        "buff" => {
            let trigger = s(v, "trigger")?;
            let grants = s(v, "grants")?;
            let all_drop = crate::model::BuffDecay::from_id(s(v, "decay")) == crate::model::BuffDecay::AllAtOnce;
            let trig = match trigger {
                // Longbow Sharpshot: armed by a headshot, spent on the next
                // shot, and MULTIPLICATIVE — "Damage bonus is multiplicative to
                // mods like Serration". It reaches the same final-damage
                // multiplier as the ability-cast one because that is the
                // bucket, not because the trigger is alike.
                "weakpoint_hit" if grants == "final_damage" => {
                    return Some(ArcEffect::FinalDamageCap(scale(v)))
                }
                // Non-simmed triggers with modeled grants:
                "swap_consume_combo" => {
                    return Some(match grants {
                        "crit_chance" => ArcEffect::CondCritChanceStacked {
                            scale: scale(v),
                            max_stacks: u(v, "max_stacks"),
                        },
                        "crit_damage" => ArcEffect::CondCritDamageStacked {
                            scale: scale(v),
                            max_stacks: u(v, "max_stacks"),
                        },
                        other => ArcEffect::Inert(format!("on_swap grant {other}")),
                    })
                }
                "roll" if grants == "weakpoint_crit_chance" => {
                    return Some(ArcEffect::WeakpointCritChance(scale(v)))
                }
                "ability_cast" if grants == "final_damage" => {
                    return Some(ArcEffect::FinalDamageCap(scale(v)))
                }
                "ability_cast" if grants == "reload_speed" => {
                    return Some(ArcEffect::CondReloadSpeed(scale(v)))
                }
                // Enervate's on_hit buff is implemented by its perk.
                "hit" => return Some(ArcEffect::Elsewhere("on_hit".into())),
                other => match ArcTrigger::from_id(other) {
                    Some(t) => t,
                    None => return inert(&format!("trigger {other}")),
                },
            };
            let grant = match grants {
                "base_damage" => ArcGrant::BaseDamage,
                "multishot" => ArcGrant::Multishot,
                "reload_speed" => ArcGrant::ReloadSpeed,
                "crit_damage" => ArcGrant::CritDamage,
                "status_chance" => ArcGrant::StatusChance,
                "ammo_efficiency" => ArcGrant::AmmoEfficiency,
                other => return inert(&format!("grant {other}")),
            };
            ArcEffect::Buff {
                trigger: trig,
                grant,
                scale: scale(v),
                max_stacks: u(v, "max_stacks"),
                duration: f(v, "duration").unwrap_or(0.0),
                all_drop,
                one_per_instance: v
                    .get("one_stack_per_instance")
                    .and_then(serde_norway::Value::as_bool)
                    .unwrap_or(false),
            }
        }
        // Kinship carries `per: ally_buff` — team-context, uncapped: inert
        // in the sim; the value still renders in the description.
        "crit_chance_bonus" if s(v, "per").is_some() => ArcEffect::PerAllyCritChance(scale(v)),
        "crit_chance_bonus" => ArcEffect::CondCritChance(scale(v)),
        "headshot_multiplier_bonus" => ArcEffect::HeadshotMultiplier {
            value: f(v, "rankMax").unwrap_or(0.0),
            unlocks_at: u(v, "unlocks_at_rank"),
        },
        "rechargeable_magazine" => ArcEffect::RechargeableMagazine { scale: scale(v) },
        "reload_speed_bonus" => ArcEffect::ReloadSpeed {
            value: f(v, "rankMax").unwrap_or(0.0),
            unlocks_at: u(v, "unlocks_at_rank"),
        },
        "per_status_damage_bonus" => ArcEffect::PerColdDamage {
            scale: scale(v),
            max_stacks: u(v, "max_stacks"),
        },
        "added_element" => ArcEffect::AddedElement {
            element: crate::rules::damage::DamageType::from_name(s(v, "element")?)?,
            scale: scale(v),
            max_stacks: u(v, "max_stacks").max(1),
        },
        "flat_damage_on_status" => ArcEffect::FlatDamageOnStatus(scale(v)),
        "proc_conversion" => ArcEffect::EncumberChance(scale(v)),
        "proc_burst" => ArcEffect::ColdBurst {
            scale: scale(v),
            radius0: f(v, "radius_rank0").unwrap_or(0.0),
            radius1: f(v, "radius_rankMax").unwrap_or(0.0),
        },
        "aoe_echo" => ArcEffect::AoeEcho {
            scale: scale(v),
            radius0: f(v, "radius_rank0").unwrap_or(0.0),
            radius1: f(v, "radius_rankMax").unwrap_or(0.0),
            // THE GATE, READ. `arc_condition` refuses a string it does not
            // know, which is the half that matters: this effect carried a
            // `condition:` for as long as it existed and the loader dropped it
            // silently, so the next one has to be impossible to drop.
            needs_radiation: match arc_condition(v) {
                Some(ArcCondition::TargetRadiationStacks(n)) => n,
                _ => 0,
            },
        },
        "status_spread" => ArcEffect::StatusSpread {
            scale: scale(v),
            radius0: f(v, "radius_rank0").unwrap_or(0.0),
            radius1: f(v, "radius_rankMax").unwrap_or(0.0),
            seconds0: f(v, "duration_rank0").unwrap_or(0.0),
            seconds1: f(v, "duration_rankMax").unwrap_or(0.0),
        },
        "overguard_damage_bonus" => ArcEffect::OverguardDamage(scale(v)),
        "ammo_efficiency" => ArcEffect::AmmoEfficiency(scale(v)),
        "compression_damage" => ArcEffect::CompressionDamage(scale(v)),
        "compression_ammo_efficiency" => ArcEffect::CompressionAmmoEfficiency(scale(v)),
        "tenno_scaled" => ArcEffect::TennoScaled {
            stat: match s(v, "stat")? {
                "armor" => TennoStat::Armor,
                "max_energy" => TennoStat::MaxEnergy,
                "shields" => TennoStat::Shields,
                other => return inert(&format!("tenno stat {other}")),
            },
            above: f(v, "above").unwrap_or(0.0),
            per_unit: f(v, "per_unit")?,
            min_energy_pct: f(v, "min_energy_pct").unwrap_or(0.0),
            grant: match s(v, "grants")? {
                "base_damage" => ArcGrant::BaseDamage,
                "multishot" => ArcGrant::Multishot,
                other => return inert(&format!("grant {other}")),
            },
            cap: scale(v),
        },
        "debilitate" => ArcEffect::Debilitate(scale(v)),
        "unmodelled" => ArcEffect::Unmodeled { scale: scale(v) },
        "out_of_scope" => ArcEffect::OutOfScope { scale: scale(v) },
        other => return inert(other),
    })
}
