use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct ModFile {
    pub(super) id: String,
    #[allow(dead_code)]
    pub(super) name: String,
    pub(super) polarity: String,
    pub(super) rarity: String,
    pub(super) base_drain: u32,
    pub(super) max_rank: u32,
    /// Verbatim in-game text, rank-varying numbers as `X` (schema).
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) exilus: bool,
    /// The card cannot be simulated below max rank: a number on its ladder is
    /// not linear in rank, so [`at_rank`] refuses every lower rank.
    #[serde(default)]
    pub(super) lower_ranks_unmodelled: bool,
    #[serde(default)]
    pub(super) family: Option<String>,
    /// Mod SET membership — the bonus itself lives in `data/mod_sets/`.
    #[serde(default)]
    pub(super) set: Option<String>,
    /// Weapon property required to EQUIP this mod ("continuous").
    #[serde(default)]
    pub(super) requires_weapon: Option<String>,
    /// WEAPON IDS this mod may be equipped on, and nothing else. Distinct from
    /// `requires_weapon`, which names a PROPERTY several weapons can share:
    /// this names the weapons themselves, because some mods are written for
    /// exactly one ("Can equip the Ocucor-exclusive Sentient Surge mod").
    #[serde(default)]
    pub(super) exclusive_to: Vec<String>,
    /// DE's own INCOMPATIBILITY tags, lowercased ("sentinel_weapon",
    /// "power_weapon") — the mirror of `requires_weapon`. NOT the existing
    /// `incompatible_with:` key, which names other MODS and duplicates
    /// `family`; this one names weapon KINDS.
    #[serde(default)]
    pub(super) excludes_weapon: Vec<String>,
    /// Weapon trait required for the mod to apply (calc-layer gate).
    #[serde(default)]
    pub(super) requires: Option<String>,
    /// Stats this mod locks from being modified.
    #[serde(default)]
    pub(super) disables: Vec<String>,
    /// A STANCE'S COMBO SCRIPTS, one per form it supplies — see
    /// [`crate::model::ModDef::stance`]. Absent on every other mod.
    #[serde(default)]
    pub(super) combos: Option<std::collections::BTreeMap<String, Vec<crate::model::ComboHit>>>,
    pub(super) effects: Vec<Value>,
}

pub(super) fn polarity(name: &str) -> Polarity {
    match name {
        "madurai" => Polarity::Madurai,
        "naramon" => Polarity::Naramon,
        "vazarin" => Polarity::Vazarin,
        "zenurik" => Polarity::Zenurik,
        "unairu" => Polarity::Unairu,
        "penjaga" => Polarity::Penjaga,
        "umbra" => Polarity::Umbra,
        other => panic!("unknown polarity: {other}"),
    }
}

pub(super) fn rarity(name: &str) -> Rarity {
    match name {
        "common" => Rarity::Common,
        "uncommon" => Rarity::Uncommon,
        "rare" => Rarity::Rare,
        "legendary" => Rarity::Legendary,
        other => panic!("unknown rarity: {other}"),
    }
}

pub(super) fn element(name: &str) -> Option<DamageType> {
    use DamageType::*;
    Some(match name {
        "cold" => Cold,
        "heat" => Heat,
        "electricity" => Electricity,
        "toxin" => Toxin,
        "magnetic" => Magnetic,
        "viral" => Viral,
        "corrosive" => Corrosive,
        "gas" => Gas,
        "radiation" => Radiation,
        "blast" => Blast,
        "impact" => Impact,
        "puncture" => Puncture,
        "slash" => Slash,
        _ => return None,
    })
}

pub(super) fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}
pub(super) fn u(v: &Value, k: &str) -> u32 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0) as u32
}
/// Any YAML number, integer or float. `as_f64` alone returns None for a plain
/// integer scalar, which is why `duration: 9` silently read as absent and left
/// a literal "X" in the rendered description.
pub(super) fn n(v: &Value, k: &str) -> Option<f64> {
    let x = v.get(k)?;
    x.as_f64().or_else(|| x.as_i64().map(|i| i as f64))
}

/// Map one YAML effect entry to a [`ModEffect`] at max rank (None = no damage
/// effect / not modeled — the mod still loads).
pub(super) fn effect(id: &str, v: &Value) -> Option<ModEffect> {
    let kind = v.get("kind").and_then(Value::as_str)?;
    let max = |k: &str| f(v, k).unwrap_or(0.0);
    // `condition:` gates ANY effect, not only a triggered one. `aiming`
    // has its own wrapper (it predates the Tenno); every other player state is
    // a `TennoCondition`, asked of `data/tenno/` at resolve time.
    // Critical Focus is a flat crit bonus that simply does not exist unless
    // you are aiming — there is no event to wait for, so `kind: buff` (which
    // requires a trigger) cannot say it. The wrapper already existed; only
    // the data path was missing. The `buff` arm reads the same key itself,
    // for the effect it builds, and is skipped here so nothing double-wraps.
    let cond = v.get("condition").and_then(Value::as_str);
    // A `kind: buff` reads its own condition below (it wraps what the trigger
    // resolves to); every other kind wraps here.
    let tenno_cond = if kind == "buff" { None } else { cond.and_then(crate::model::TennoCondition::from_id) };
    let out = match kind {
        // A BONUS THE PLAYER DECIDES — Dreadful Killshot, and the mod-side twin
        // of the arcanes' `tenno_scaled`. The value is a step function of one of
        // the Tenno's stats, so it cannot be a number here: it is carried to
        // `resolve_for`, which has the player.
        //
        // AN UNKNOWN `stat:` OR `grants:` IS A REFUSAL, not a default. A card
        // whose rule the engine cannot read must pay NOTHING and say so — the
        // arcane loader's own rule, and the reason a data file cannot state a
        // rule that quietly does not apply.
        "tenno_scaled" => {
            let stat = match v.get("stat").and_then(Value::as_str)? {
                "armor" => crate::model::TennoStat::Armor,
                "max_energy" => crate::model::TennoStat::MaxEnergy,
                "health" => crate::model::TennoStat::Health,
                _ => return None,
            };
            let grant = match v.get("grants").and_then(Value::as_str)? {
                "base_damage" => crate::model::ArcGrant::BaseDamage,
                "status_chance" => crate::model::ArcGrant::StatusChance,
                "multishot" => crate::model::ArcGrant::Multishot,
                "crit_damage" => crate::model::ArcGrant::CritDamage,
                _ => return None,
            };
            ModEffect::TennoScaled {
                stat,
                above: max("above"),
                // A unit of ZERO would divide the player's stat by nothing and
                // pay the cap to anybody, so it is required rather than
                // defaulted.
                unit: f(v, "unit").filter(|u| *u > 0.0)?,
                per_unit: max("rankMax"),
                // NO CAP IS A REAL STATE, and it is infinity rather than zero:
                // zero would silently pay nothing, which is the failure this
                // whole arm exists to avoid.
                cap: f(v, "cap").unwrap_or(f64::INFINITY),
                grant,
            }
        }
        "base_damage_bonus" => ModEffect::BaseDamage(max("rankMax")),
        "multishot_bonus" => ModEffect::Multishot(max("rankMax")),
        "crit_chance_bonus" => ModEffect::CritChance(max("rankMax")),
        // ---- MELEE COMBO. Five kinds, and the two that read the counter
        // without spending it are the reason the counter matters to a light
        // build at all — the multiplier itself does not touch a normal swing.
        "crit_chance_per_combo" => ModEffect::CritChancePerCombo(max("rankMax")),
        "crit_chance_on_slide" => ModEffect::CritChanceOnSlide(max("rankMax")),
        // THE CARD CARRIES THE RULE, so the yaml states it per card rather than
        // the bucket doubling for everyone: `(x2 for Heavy Attacks)` is printed
        // on True Steel, Sacrificial Steel and Galvanized Steel, and on nothing
        // else in the melee pool.
        "crit_chance_bonus_heavy_doubled" => ModEffect::CritChanceHeavyDoubled(max("rankMax")),
        // TENNOKAI. Every card turns a different subset of the same seven
        // knobs, so one kind with seven optional fields rather than seven kinds
        // — a build SUMS them, and a missing knob is a zero.
        "tennokai" => ModEffect::Tennokai {
            enabled: v.get("enables").and_then(Value::as_bool).unwrap_or(true),
            chance: f(v, "chance").unwrap_or(0.0),
            every_n_hits: v.get("every_n_hits").and_then(Value::as_u64).unwrap_or(0) as u32,
            window_seconds: f(v, "window_seconds").unwrap_or(0.0),
            damage: f(v, "damage").unwrap_or(0.0),
            crit_damage: f(v, "crit_damage").unwrap_or(0.0),
            status_chance: f(v, "status_chance").unwrap_or(0.0),
            chain_seconds: f(v, "chain_seconds").unwrap_or(0.0),
            damage_needs_chain: v
                .get("damage_needs_chain")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            curse_resets_combo: v
                .get("curse_resets_combo")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            curse_heat_per_second: f(v, "curse_heat_per_second").unwrap_or(0.0),
            curse_seconds: f(v, "curse_seconds").unwrap_or(0.0),
        },
        "status_chance_per_combo" => ModEffect::StatusChancePerCombo(max("rankMax")),
        // MELEE'S CONDITION OVERLOAD, which is the ORIGINAL one and is not a
        // buff at all: no trigger, no stacks, no clock — it reads the target's
        // status types on every swing and always has. The Galvanized family
        // spells the same payload as a `buff` with `grants: condition_overload`
        // because on a GUN it is earned on a kill and decays; here there is
        // nothing to earn, so a trigger would be a fiction.
        //
        // It reaches the engine as the same `ConditionOverload` effect at ONE
        // permanent stack, so the weapon's own `co_behavior` — which base the
        // term reads, which attack parts take it — decides the arithmetic
        // exactly as it does for a gun.
        "condition_overload" => ModEffect::ConditionOverload {
            per_stack: max("rankMax"),
            max_stacks: 1,
            duration: crate::model::NO_TIMEOUT,
            // NOTHING TO EARN, so it opens full and no switch can deny it.
            // Routing it through the Galvanized family's earned-on-a-kill path
            // made it pay zero in all seven melee modes.
            earned_on: None,
        },
        "melee_combo_duration_bonus" => ModEffect::MeleeComboDuration(max("rankMax")),
        "initial_combo" => ModEffect::InitialCombo(max("rankMax")),
        // THE TWO LIFTED CARDS. `Lifted` is a status this engine tracks, so the
        // gate is simulated rather than assumed — a condition about the TARGET.
        "combo_count_chance_on_lifted" => ModEffect::ComboCountChanceOnLifted(max("rankMax")),
        "status_chance_on_lifted" => ModEffect::StatusChanceOnLifted(max("rankMax")),
        "heavy_attack_efficiency" => ModEffect::HeavyAttackEfficiency(max("rankMax")),
        "melee_combo_duration_multiplier" => {
            ModEffect::MeleeComboDurationMultiplier(max("rankMax"))
        }
        // METRES, not a percentage — DE's own card reads `+3 Range`.
        "melee_range_bonus_m" => ModEffect::MeleeRange(max("rankMax")),
        "slam_damage_bonus" => ModEffect::SlamDamage(max("rankMax")),
        "heavy_attack_damage_bonus" => ModEffect::HeavyAttackDamage(max("rankMax")),
        "combo_count_chance" => ModEffect::ComboCountChance(max("rankMax")),
        "heavy_windup_speed_bonus" => ModEffect::HeavyWindUpSpeed(max("rankMax")),
        "crit_damage_bonus" => ModEffect::CritDamage(max("rankMax")),
        "status_chance_bonus" => ModEffect::StatusChance(max("rankMax")),
        "status_damage_bonus" => ModEffect::StatusDamage(max("rankMax")),
        "ammo_efficiency_bonus" => ModEffect::AmmoEfficiency(max("rankMax")),
        // Hunter Munitions / Internal Bleeding: a Slash status rolled off a
        // CRITICAL hit, independently of status chance.
        "slash_on_crit" => ModEffect::SlashOnCrit(max("rankMax")),
        // DOUBLE TAP. The rank table moves BOTH halves — 5%/80x at rank 0 and
        // 20%/20x at rank 3 — and the product is +400% at every rank, so the
        // per-stack value and the cap are read together and neither alone.
        "consecutive_hit_damage" => ModEffect::ConsecutiveHitDamage {
            per_stack: max("rankMax"),
            max_stacks: v.get("max_stacks").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
            duration: v.get("duration_seconds").and_then(|x| x.as_f64()).unwrap_or(0.0),
        },
        // HATA-SATYA. Read together for the same reason Double Tap's pair is:
        // the cap is what the rate is worth, not a separate fact. Here the
        // cap is the one thing rank does NOT move — "capped at 500% at all mod
        // ranks" — so the yaml states it and the rate ladders under it.
        //
        // `max_bonus` AND NOT `max_stacks`, which is the difference between a
        // ceiling DE published and one we computed from it: a stack count would
        // have to be re-derived at every rank, and at rank 0 it is 2,500 rather
        // than the 417 the card's own rate suggests.
        "crit_chance_per_hit" => ModEffect::CritChancePerHit(crate::model::CritPerHit {
            per_stack: max("rankMax"),
            max_bonus: f(v, "max_bonus").unwrap_or(0.0),
        }),
        // EXIMUS ADVANTAGE. The duration is fixed at every rank, so it is a
        // plain number beside the ladder.
        "eximus_weakpoint_damage" => ModEffect::OnEximusWeakpointDamage {
            bonus: max("rankMax"),
            duration: v.get("duration_seconds").and_then(|x| x.as_f64()).unwrap_or(0.0),
        },
        // SYNTH CHARGE. Its own multiplier on the magazine's last round — see
        // `ModEffect::LastRoundDamage` for the three things that switch it off.
        // A MOD THAT GRANTS A LIVE STACKING BUFF, in the vocabulary the weapon
        // perks already speak. Deliberately its own kind rather than a new arm
        // on `kind: buff`: that one contributes at the ASSUMED MAX through
        // `CondBuff`, which is right for a card whose trigger the sim has no
        // event for and wrong the moment it does — so opting in per mod is what
        // keeps every existing card exactly where it was.
        //
        // The buff's ID is the MOD's, leaked once. It is the key the card, the
        // replay curve, the stack config and the sampler all share, so deriving
        // it is what stops those four from drifting.
        "stacking_buff" => ModEffect::GrantsStackingBuff(crate::model::StackingBuff {
            id: Box::leak(id.to_string().into_boxed_str()),
            trigger: crate::model::BuffTrigger::from_id(v.get("trigger").and_then(Value::as_str)?)?,
            grant: crate::model::BuffGrant::from_id(v.get("grants").and_then(Value::as_str)?)?,
            per_stack: max("rankMax"),
            max_stacks: u(v, "max_stacks").max(1),
            duration: n(v, "duration").unwrap_or(0.0),
            chance: n(v, "chance").unwrap_or(1.0),
            decay: crate::model::BuffDecay::from_id(v.get("decay").and_then(Value::as_str)),
            initial_stacks: 0,
            stacks_per_trigger: 1,
            per_shell: false,
            cleared_by: crate::model::ClearedBy::Nothing,
            // Read here as well, so a MOD that states it needs no second edit
            // — no mod does today; the two that claim it are evolutions.
            card_opens_full: v
                .get("card_opens_full")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        // DEGREES, not an accuracy fraction — see `ModEffect::AddedSpread`.
        // `max_stacks` multiplies it, the same assumed-max reading a `kind:
        // buff` with an indirect grant already takes: a build carrying this
        // mod is played at its cap.
        "added_spread" => {
            ModEffect::AddedSpread(max("rankMax") * f64::from(u(v, "max_stacks").max(1)))
        }
        "last_round_damage" => ModEffect::LastRoundDamage(max("rankMax")),
        "first_round_damage" => ModEffect::FirstRoundDamage(max("rankMax")),
        // JAHU CANTICLE. `range_m` is the card's Affinity Range, transcribed
        // rather than assumed, so a card with another radius costs a number
        // instead of a branch.
        // THE INVOCATIONS. One arm for four cards, because they are one card
        // with the stat swapped — and the two that pay nothing are loaded the
        // same way so the builder offers them and states why.
        "ability_stat" => ModEffect::AbilityStat(
            match v.get("stat").and_then(Value::as_str)? {
                "strength" => crate::model::AbilityStat::Strength,
                "duration" => crate::model::AbilityStat::Duration,
                "efficiency" => crate::model::AbilityStat::Efficiency,
                "energy_regen" => crate::model::AbilityStat::EnergyRegen,
                _ => return None,
            },
            max("rankMax"),
            u(v, "max_stacks").max(1),
        ),
        "strip_on_kill_in_range" => ModEffect::StripOnKillInRange(
            max("rankMax"),
            n(v, "range_m").unwrap_or(50.0),
        ),
        "fire_rate_bonus" => ModEffect::FireRate(max("rankMax")),
        "reload_speed_bonus" => ModEffect::ReloadSpeed(max("rankMax")),
        "magazine_capacity_bonus" => ModEffect::MagazineCapacity(max("rankMax")),
        "blast_radius_bonus" => ModEffect::BlastRadius(max("rankMax")),
        "status_duration_bonus" => ModEffect::StatusDuration(max("rankMax")),
        // Faction damage (Bane/Expel): +max total damage vs the named faction.
        // An unrecognized faction (Unknown) drops the effect (mod still loads).
        "faction_damage_bonus" => {
            let fac = Faction::from_name(v.get("faction").and_then(Value::as_str)?);
            if fac == Faction::Unknown {
                return None;
            }
            ModEffect::FactionDamage(fac, max("rankMax"))
        }
        "elemental_damage_bonus" | "combined_element_bonus" | "physical_damage_bonus" => {
            let e = element(v.get("element").and_then(Value::as_str)?)?;
            // Physical (IPS) types are a DIFFERENT mechanic from elements: they
            // scale the base of that type and never combine — route to Physical
            // regardless of the kind name.
            match e {
                DamageType::Impact | DamageType::Puncture | DamageType::Slash => {
                    ModEffect::Physical(e, max("rankMax"))
                }
                _ if e.is_primary_element() => ModEffect::Element(e, max("rankMax")),
                _ => ModEffect::CombinedElement(e, max("rankMax")),
            }
        }
        // Unified declarative TRIGGERED BUFF (BUFFS.md model): a held perk
        // grants a buff on `trigger` (+ optional `condition`), contributing
        // `grants` (a bucket) per stack; `rank0`/`rankMax` are the per-stack
        // value. Maps to the modeled buff variants at max rank; triggers not yet
        // modeled keep their (uniform) data but resolve to a no-op.
        "buff" => {
            let trigger = v.get("trigger").and_then(Value::as_str)?;
            let grants = v.get("grants").and_then(Value::as_str)?;
            // The condition wraps whatever this buff resolves to, so the
            // fight's Tenno decides whether it arms at all.
            let tenno_cond = v.get("condition").and_then(Value::as_str).and_then(crate::model::TennoCondition::from_id);
            let per = max("rankMax"); // per-stack value at max rank
            let stacks = u(v, "max_stacks");
            let dur = f(v, "duration").unwrap_or(0.0);
            let wrap = |e: ModEffect| match tenno_cond {
                Some(c) => ModEffect::WhileTenno(c, Box::new(e)),
                None => e,
            };
            wrap(match (trigger, grants) {
                ("kill", "multishot") => {
                    ModEffect::OnKillMultishot { per_stack: per, max_stacks: stacks, duration: dur }
                }
                ("kill", "condition_overload") => {
                    // THE GALVANIZED FAMILY EARNS IT on a kill, so it opens at
                    // zero and a fight that denies kills denies it — the
                    // difference from melee's own card one screen up.
                    ModEffect::ConditionOverload {
                        per_stack: per, max_stacks: stacks, duration: dur,
                        earned_on: Some("kill"),
                    }
                }
                ("headshot", "crit_chance") => {
                    ModEffect::OnHeadshotCritChance { bonus: per, duration: dur }
                }
                // LEADED GAS, and the grant names both halves because the card
                // does: one column, one duration, two stats. `element:` says
                // which one — the same field every elemental effect reads.
                ("headshot", "element_and_status_chance") => {
                    ModEffect::OnWeakpointElementAndStatus {
                        element: element(v.get("element").and_then(Value::as_str)?)?,
                        bonus: per,
                        duration: dur,
                    }
                }
                ("headshot_kill", "crit_chance") => {
                    ModEffect::OnHeadshotKillCritChance { per_stack: per, max_stacks: stacks, duration: dur }
                }
                // Sharpened Bullets / Pressurized Magazine: the sim has kill
                // and reload events, so these run emergently (the aiming
                // condition is satisfied — the sim assumes constant aiming).
                // SENTIENT SURGE — one card, three numbers, so one effect.
                // The trigger word is `per_tendril` because that is what the
                // bonus scales with; it is not an EVENT like the others in
                // this table, and calling it `on_kill` would have been the
                // easy lie (kills spawn tendrils, but a reload takes them all
                // away without a kill anywhere).
                ("per_tendril", "crit_and_status") => {
                    ModEffect::PerTendril { crit_chance: per, status_chance: per }
                }
                ("kill", "magazine_refill") => ModEffect::MagazineRefillOnKill(per),
                ("kill", "crit_damage") => {
                    ModEffect::OnKillCritDamage { bonus: per, duration: dur }
                }
                // "On Reload From Empty: +X% Damage" — its own event, because
                // the window opens when the RELOAD COMPLETES and a CondBuff
                // would have to pretend it is always on.
                ("reload_complete", "base_damage") => {
                    ModEffect::OnReloadDamage { bonus: per, duration: dur }
                }
                ("reload_complete", "fire_rate") => {
                    ModEffect::OnReloadFireRate { bonus: per, duration: dur }
                }
                // Any other trigger (ability_cast / reload_complete / hit / …):
                // contribute at the assumed-max total via CondBuff when the grant
                // maps to a DPS bucket. Indirect grants (accuracy/recoil) → None.
                _ => {
                    let bucket = match grants {
                        "base_damage" => CondBucket::BaseDamage,
                        "multishot" => CondBucket::Multishot,
                        "crit_chance" => CondBucket::CritChance,
                        "crit_damage" => CondBucket::CritDamage,
                        "status_chance" => CondBucket::StatusChance,
                        "status_damage" => CondBucket::StatusDamage,
                        "fire_rate" => CondBucket::FireRate,
                        "reload_speed" => CondBucket::ReloadSpeed,
                        // An INDIRECT grant must not hit `return None` here:
                        // that throws the number away and leaves three mods
                        // (Twitch, Reflex Draw, Targeting Subsystem) loading
                        // with no effects at all. `CondBucket` is damage
                        // buckets only, so route these to the indirect
                        // bucket instead — flat, like every other indirect
                        // stat. The trigger stays on the card; a stat with no
                        // damage payload has nothing to gate in this sim, and
                        // the 2D world wants the magnitude either way.
                        // `wrap`, not a bare return: Targeting Subsystem is
                        // `condition: aiming`, and skipping the wrapper
                        // would report it on the panel as an unconditional
                        // stat change — the exact thing the buff shape exists
                        // to prevent. The outer `aim_gated` is false for
                        // `kind: buff`, so this cannot double-wrap.
                        _ => {
                            let stat = IndirectStat::from_id(grants)?;
                            let v = per * stacks.max(1) as f64;
                            return Some(wrap(ModEffect::Indirect(stat, v)));
                        }
                    };
                    ModEffect::CondBuff(bucket, per * stacks.max(1) as f64)
                }
            })
        }
        // Weak-point effects (Pistol Acuity): conditional on the part hit.
        "weakpoint_damage_bonus" => ModEffect::WeakpointDamage(max("rankMax")),
        "weakpoint_crit_chance_bonus" => ModEffect::WeakpointCritChance(max("rankMax")),
        // Hemorrhage: a `from` status rolls `rankMax` to also apply the `to`
        // status; `condition: fire_rate_below_<x>` doubles it.
        "proc_conversion" => {
            let from = element(v.get("from").and_then(Value::as_str)?)?;
            let to = element(v.get("to").and_then(Value::as_str)?)?;
            let (threshold, mult) = match v.get("condition").and_then(Value::as_str) {
                Some(c) if c.starts_with("fire_rate_below_") => (
                    c["fire_rate_below_".len()..].parse().ok()?,
                    f(v, "condition_multiplier").unwrap_or(1.0),
                ),
                _ => (0.0, 1.0),
            };
            ModEffect::ProcConversion {
                from,
                to,
                chance: max("rankMax"),
                low_rate_threshold: threshold,
                low_rate_multiplier: mult,
            }
        }
        // INDIRECT stats: outside the theoretical-DPS formula, but real
        // panel buckets a future shooter model consumes (aim, travel,
        // ammo sustain) — the panel states every bonus.
        // One table (`IndirectStat::from_kind`) names every such kind, here and
        // in the evolutions alike.
        k if IndirectStat::from_kind(k).is_some() => {
            ModEffect::Indirect(IndirectStat::from_kind(k).expect("guarded"), max("rankMax"))
        }
        // TOME MODS. Each carries its real number into the panel and pays
        // nothing, which is what `IndirectStat` is for — see the enum for why
        // the three ability stats are three buckets and not one.
        // 2D groundwork: these were `kind: unmodeled`, i.e. the
        // mod equipped and the number was thrown away. They carry no
        // SINGLE-TARGET damage, which is what `Indirect` is for.
        // NIGHTWATCH NAPALM: the mod LEAVES A FIELD. Every number the field
        // needs is stated here rather than borrowed from the weapon, because
        // this fire is not the rocket's — see `ModEffect::GrantsLingering`.
        "grants_lingering" => {
            let mut vector = crate::rules::damage::DamageVector::new();
            for (k, val) in v.get("damage")?.as_mapping()? {
                vector.add(crate::data::weapons::damage_type(k.as_str()?), val.as_f64()?);
            }
            let field = Box::leak(Box::new(crate::model::LingeringBase {
                base_vector: vector,
                base_crit_chance: n(v, "crit_chance").unwrap_or(0.0),
                base_crit_damage: n(v, "crit_multiplier").unwrap_or(1.0),
                base_status_chance: n(v, "status_chance").unwrap_or(0.0),
                tick_rate: n(v, "tick_rate")?,
                duration_seconds: n(v, "duration_seconds")?,
                // A GRANTED FIELD IS A CLOUD: it starts with the impact and
                // forces nothing. Both are the roster's default and none of the
                // three mods that grant one says otherwise; they are stated
                // here rather than defaulted so a mod that DOES say otherwise
                // has somewhere to say it.
                first_tick_delay_seconds: 0.0,
                forced_procs: crate::rules::damage::ForcedProcs::from_types([]),
                // Overwritten at resolve time from the weapon's own blast.
                radius_m: 0.0,
                falloff_start_m: n(v, "falloff_start_m").unwrap_or(0.0),
                falloff_reduction: n(v, "falloff_reduction").unwrap_or(0.0),
                stacking: match v.get("stacking").and_then(Value::as_str) {
                    Some("refresh") => crate::model::FieldStacking::Refresh,
                    _ => crate::model::FieldStacking::Stack,
                },
                takes_condition_overload: v
                    .get("takes_condition_overload")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                // A MOD-GRANTED field says so the same way a weapon's does.
                can_crit: v.get("can_crit").and_then(Value::as_bool).unwrap_or(true),
                elemental_mods_apply: v
                    .get("elemental_mods_apply")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                status_mods_apply: v
                    .get("status_mods_apply")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            }));
            ModEffect::GrantsLingering(field)
        }
        // …and the share of the blast AREA it covers, its own column on the
        // card and therefore its own effect.
        "lingering_area_fraction" => ModEffect::LingeringAreaFraction(max("rankMax")),
        // ACID SHELLS: the corpse explosion. Three numbers off one ladder,
        // read together because none of them means anything alone.
        "acid_shells_flat_damage" => {
            ModEffect::AcidShells(crate::model::AcidShellsPart::FlatDamage(max("rankMax")))
        }
        "acid_shells_health_fraction" => {
            ModEffect::AcidShells(crate::model::AcidShellsPart::HealthFraction(max("rankMax")))
        }
        "acid_shells_radius_m" => {
            ModEffect::AcidShells(crate::model::AcidShellsPart::RadiusM(max("rankMax")))
        }
        // HARKONAR SCOPE: seconds onto the sniper combo's decay window.
        "combo_duration_bonus" => ModEffect::ComboDuration(n(v, "duration_seconds")?),
        // …and the PERCENTAGE half, which is a different bucket because it
        // lands in a different place — see `range_m` in `build::loadout::resolve`.
        // A syndicate augment's radial scale ("+1 Truth"). Its damage is
        // real; its TRIGGER counts affinity, which the sim does not track.
        // A syndicate augment names one of the six effects; its payload lives
        // in data/syndicates/ and is looked up there.
        "syndicate_radial" => ModEffect::SyndicateRadial {
            syndicate: Box::leak(
                v.get("syndicate")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
                    .into_boxed_str(),
            ),
            amount: max("rankMax"),
        },
        // NOT indirect: a CHARGE-rate bonus shortens the draw, and a charged
        // form's cadence IS its draw (`ChargeCadence`), so this is DPS. It is
        // its own bucket rather than `fire_rate_bonus` because Shell Rush says
        // "Charge Rate" — it must not also speed up an uncharged form.
        "charge_rate_bonus" => ModEffect::ChargeRate(max("rankMax")),
        // Reflex Draw: on swap-in, −recoil/+accuracy for a few seconds.
        "on_equip_buff" => ModEffect::OnEquipHandling {
            recoil: -max("rankMax").abs(),
            accuracy: max("rankMax").abs(),
            duration: f(v, "duration_seconds").unwrap_or(0.0),
        },
        // Scoping markers (weapon_scoped) or an effect not yet modeled:
        // load the mod without this effect.
        _ => return None,
    };
    Some(match tenno_cond {
        Some(c) => ModEffect::WhileTenno(c, Box::new(out)),
        None => out,
    })
}

/// One effect entry with its rank-varying numbers read at `rank`: the value
/// (`rank0` → `rankMax`) and a laddered duration (`duration_rank0` →
/// `duration`/`duration_seconds`), both linear — the rule `ModDescInfo::at`
/// fills the card with, so the slot's text and the fight read one number.
pub(super) fn effect_at_rank(e: &Value, rank: u32, max_rank: u32) -> Value {
    let t = f64::from(rank.min(max_rank)) / f64::from(max_rank.max(1));
    let lerp = |a: f64, b: f64| a + (b - a) * t;
    let mut out = e.clone();
    let mut set = |k: &str, x: f64| {
        if let Value::Mapping(m) = &mut out {
            m.insert(Value::from(k), Value::from(x));
        }
    };
    if let (Some(a), Some(b)) = (n(e, "rank0"), n(e, "rankMax")) {
        set("rankMax", lerp(a, b));
    }
    if let Some(d0) = n(e, "duration_rank0") {
        for k in ["duration", "duration_seconds"] {
            if let Some(d) = n(e, k) {
                set(k, lerp(d0, d));
            }
        }
    }
    out
}

pub(super) fn to_moddef(mf: ModFile) -> ModDef {
    to_moddef_at(mf, None)
}

/// `rank: None` is the card at max rank under its own id; `Some(r)` is the
/// card at rank `r` under [`ranked_id`], drawing `base_drain + r`.
pub(super) fn to_moddef_at(mut mf: ModFile, rank: Option<u32>) -> ModDef {
    let card_id = mf.id.clone();
    if let Some(r) = rank {
        let max_rank = mf.max_rank;
        mf.effects = mf.effects.iter().map(|e| effect_at_rank(e, r, max_rank)).collect();
        // ONE CARD AT TWO RANKS IS ONE CARD: the family is what every
        // exclusivity check asks, so a variant carries one ([`with_ranks`]).
        mf.family = Some(mf.family.take().unwrap_or_else(|| card_id.clone()));
        mf.id = ranked_id(&card_id, r, max_rank);
    }
    let drain = mf.base_drain + rank.unwrap_or(mf.max_rank);
    let effects = mf.effects.iter().filter_map(|e| effect(&card_id, e)).collect();
    // WHAT WE KNOWINGLY DO NOT MODEL, kept rather than dropped. An `unmodeled`
    // effect returns None from `effect` and vanishes, so a mod carrying only
    // one loads as a mod that does nothing and says nothing — which is exactly
    // how it looks to a player who equips it and sees no change (reported
    // 2026-08-05 about Primary Debilitate; 12 mods and 5 arcanes are in this
    // state). The note travels so the card can admit it.
    let has = |k: &str| {
        mf.effects
            .iter()
            .any(|e| e.get("kind").and_then(Value::as_str) == Some(k))
    };
    // A STANCE'S COMBO SCRIPTS, keyed by form. Leaked because a `ModDef` is
    // `'static` for the life of the process, the same way every other string on
    // it is — the pool is built once at load.
    let stance: Option<crate::model::StanceCombos> = mf.combos.as_ref().map(|m| {
            let v: Vec<(&'static str, &'static [crate::model::ComboHit])> = m
                .iter()
                .map(|(form, hits)| {
                    // A FORM NAME THE ENGINE DOES NOT KNOW IS A LOUD FAILURE:
                    // a stance whose combo lands under a misspelt key would
                    // read as a stance that simply has no such combo.
                    let form: &'static str = crate::model::FormKind::parse(form).id();
                    let hits: &'static [crate::model::ComboHit] =
                        Box::leak(hits.clone().into_boxed_slice());
                    (form, hits)
                })
                .collect();
            &*Box::leak(v.into_boxed_slice())
        });
    let unmodeled = has("unmodelled");
    let out_of_scope = has("out_of_scope");
    ModDef {
        stance,
        unmodeled,
        out_of_scope,
        id: Box::leak(mf.id.into_boxed_str()),
        name: Box::leak(mf.name.into_boxed_str()),
        // ModDef.base_drain is the drain at the EQUIPPED rank: drain rises by 1
        // per rank from the rank-0 `base_drain`.
        base_drain: drain,
        max_rank: mf.max_rank,
        polarity: polarity(&mf.polarity),
        rarity: rarity(&mf.rarity),
        exilus: mf.exilus,
        family: mf.family.map(|s| &*Box::leak(s.into_boxed_str())),
        set: mf.set.map(|s| &*Box::leak(s.into_boxed_str())),
        requires_weapon: mf.requires_weapon.map(|s| &*Box::leak(s.into_boxed_str())),
        exclusive_to: Box::leak(
            mf.exclusive_to
                .into_iter()
                .map(|s| &*Box::leak(s.into_boxed_str()))
                .collect::<Vec<&'static str>>()
                .into_boxed_slice(),
        ),
        excludes_weapon: mf
            .excludes_weapon
            .into_iter()
            .map(|s| &*Box::leak(s.into_boxed_str()))
            .collect(),
        requires: mf.requires.map(|s| &*Box::leak(s.into_boxed_str())),
        disables: mf
            .disables
            .into_iter()
            .map(|s| &*Box::leak(s.into_boxed_str()))
            .collect(),
        effects,
    }
}
