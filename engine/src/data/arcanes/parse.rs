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
    /// Weapon traits on which the effects are inert though the arcane equips.
    #[serde(default)]
    pub(super) inert_on: Vec<String>,
    #[serde(default)]
    pub(super) equip_classes: Vec<String>,
    #[serde(default)]
    pub(super) equip_traits: Vec<String>,
    #[serde(default)]
    pub(super) excludes_weapon: Vec<String>,
    #[serde(default)]
    pub(super) seats: Vec<String>,
    /// Custom perk implementation id (Secondary Enervate's ramp/reset lives
    /// in `engine::perks`, not in the declarative effect vocabulary).
    #[serde(default)]
    pub(super) perk: Option<String>,
    /// Verbatim in-game text, rank-varying numbers as `X` (schema).
    #[serde(default)]
    pub(super) description: Option<crate::prose::Prose>,
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

pub(super) fn scale(v: &Value, max_rank: u32) -> Scale {
    let native_chance = v.get("UpgradeChance").is_some();
    let native_upgrade = v.get("UpgradeType").is_some();
    if native_chance || native_upgrade {
        assert!(!native_chance || !native_upgrade, "one native source per scale");
        assert!(v.get("rank0").is_none() && v.get("rankMax").is_none() && v.get("ranks").is_none(),
            "a native scale cannot carry a second rank source");
        let values: Vec<f64> = if native_chance {
            let spec: crate::model::ConditionalChance = serde_norway::from_value(v.clone()).expect("native conditional chance");
            (0..=max_rank).map(|rank| f64::from(spec.at(rank))).collect()
        } else {
            let spec: crate::model::RankedUpgrade = serde_norway::from_value(v.clone()).expect("native ranked upgrade");
            (0..=max_rank).map(|rank| f64::from(spec.query(spec.entry.UpgradeType, 0.0, rank, max_rank))).collect()
        };
        return Scale { rank0: Some(values[0]), rank_max: values[max_rank as usize], ranks: Some(values) };
    }
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
        "sliding_or_aim_gliding" => ArcCondition::Tenno(crate::model::TennoCondition::SlidingOrAimGliding),
        // The Tenno states still ASSUMED, NAMED rather than matched by shape: a
        // new one has to be added here, which is the whole mechanism.
        "overshields"
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

pub(super) fn effect(v: &Value, max_rank: u32) -> Option<ArcEffect> {
    let kind = s(v, "kind")?;
    let inert = |why: &str| Some(ArcEffect::Inert(why.to_string()));
    Some(match kind {
        "combo_count_chance" => ArcEffect::ComboCountChance(scale(v, max_rank)),
        "combo_count_chance_on_lifted" => ArcEffect::ComboCountChanceOnLifted(scale(v, max_rank)),
        "buff" => {
            let trigger = s(v, "trigger")?;
            let grants = s(v, "grants")?;
            let all_drop = crate::model::BuffDecay::from_id(s(v, "decay")) == crate::model::BuffDecay::AllAtOnce;
            let trig = match trigger {
                // Longbow Sharpshot: armed by a weak point hit, spent on the
                // next shot, and MULTIPLICATIVE — "Damage bonus is
                // multiplicative to mods like Serration". The sim arms it.
                "weakpoint_hit" if grants == "final_damage" => {
                    return Some(ArcEffect::WeakpointNextShotDamage(scale(v, max_rank)))
                }
                // NOTHING HERE SWAPS, so the stack count is the card's and it
                // never times out: the file states no duration for it to have.
                "swap_consume_combo" => {
                    return Some(ArcEffect::Buff {
                        trigger: ArcTrigger::ComboSwap,
                        grant: match grants {
                            "crit_chance" => ArcGrant::CritChance,
                            "crit_damage" => ArcGrant::CritDamage,
                            other => return inert(&format!("on_swap grant {other}")),
                        },
                        scale: scale(v, max_rank),
                        max_stacks: u(v, "max_stacks"),
                        duration: crate::model::NO_TIMEOUT,
                        all_drop: false,
                        one_per_instance: false,
                    })
                }
                "roll" if grants == "weakpoint_crit_chance" => {
                    return Some(ArcEffect::WeakpointCritChance(scale(v, max_rank)))
                }
                "ability_cast" if grants == "final_damage" => {
                    return Some(ArcEffect::FinalDamageCap(scale(v, max_rank)))
                }
                "ability_cast" if grants == "reload_speed" => {
                    return Some(ArcEffect::CondReloadSpeed(scale(v, max_rank)))
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
                scale: scale(v, max_rank),
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
        "crit_chance_bonus" if s(v, "per").is_some() => ArcEffect::PerAllyCritChance(scale(v, max_rank)),
        "crit_chance_bonus" => ArcEffect::CondCritChance(scale(v, max_rank)),
        "headshot_multiplier_bonus" => ArcEffect::HeadshotMultiplier {
            value: f(v, "rankMax").unwrap_or(0.0),
            unlocks_at: u(v, "unlocks_at_rank"),
        },
        "rechargeable_magazine" => ArcEffect::RechargeableMagazine { scale: scale(v, max_rank) },
        "reload_speed_bonus" => ArcEffect::ReloadSpeed {
            value: f(v, "rankMax").unwrap_or(0.0),
            unlocks_at: u(v, "unlocks_at_rank"),
        },
        "per_status_damage_bonus" => ArcEffect::PerColdDamage {
            scale: scale(v, max_rank),
            max_stacks: u(v, "max_stacks"),
        },
        "added_element" => ArcEffect::AddedElement {
            element: crate::rules::damage::DamageType::from_name(s(v, "element")?)?,
            scale: scale(v, max_rank),
            max_stacks: u(v, "max_stacks").max(1),
        },
        "flat_damage_on_status" => ArcEffect::FlatDamageOnStatus(scale(v, max_rank)),
        "proc_conversion" => ArcEffect::EncumberChance {
            scale: scale(v, max_rank),
            condition: serde_norway::from_value(v.clone()).expect("native Encumber condition"),
        },
        "proc_burst" => ArcEffect::ColdBurst {
            scale: scale(v, max_rank),
            radius0: f(v, "radius_rank0").unwrap_or(0.0),
            radius1: f(v, "radius_rankMax").unwrap_or(0.0),
        },
        "aoe_echo" => ArcEffect::AoeEcho {
            scale: scale(v, max_rank),
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
        "duplicate_on_base_crit" => ArcEffect::DuplicateOnBaseCrit(scale(v, max_rank)),
        "heavy_crit_per_input" => ArcEffect::HeavyCritPerInput {
            scale: scale(v, max_rank),
            max_stacks: u(v, "max_stacks").max(1),
        },
        "status_spread" => ArcEffect::StatusSpread {
            scale: scale(v, max_rank),
            radius0: f(v, "radius_rank0").unwrap_or(0.0),
            radius1: f(v, "radius_rankMax").unwrap_or(0.0),
            seconds0: f(v, "duration_rank0").unwrap_or(0.0),
            seconds1: f(v, "duration_rankMax").unwrap_or(0.0),
        },
        "overguard_damage_bonus" => ArcEffect::OverguardDamage(scale(v, max_rank)),
        "damage_vs_frozen" => ArcEffect::DamageVsFrozen(scale(v, max_rank)),
        "ammo_efficiency" => ArcEffect::AmmoEfficiency {
            scale: scale(v, max_rank),
            when: match arc_condition(v) {
                Some(ArcCondition::Tenno(c)) => Some(c),
                _ => None,
            },
        },
        "compression_damage" => ArcEffect::CompressionDamage(scale(v, max_rank)),
        "compression_ammo_efficiency" => ArcEffect::CompressionAmmoEfficiency(scale(v, max_rank)),
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
            cap: scale(v, max_rank),
        },
        "debilitate" => ArcEffect::Debilitate(scale(v, max_rank)),
        "unmodelled" => ArcEffect::Unmodeled { scale: scale(v, max_rank) },
        "out_of_scope" => ArcEffect::OutOfScope { scale: scale(v, max_rank) },
        other => return inert(other),
    })
}
