use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct EvoFile {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) weapon: String,
    pub(super) tier: u32,
    /// Wiki `File:` name for the evolution's icon.
    #[serde(default)]
    pub(super) icon: Option<String>,
    /// Verbatim in-game/wiki effect text (evolutions have no ranks, so no
    /// X templating).
    #[serde(default)]
    pub(super) description: Option<String>,
    /// Wiki-flagged non-functional evolutions apply NOTHING.
    #[serde(default)]
    pub(super) currently_broken: bool,
    /// Does THIS evolution's flat base damage stay out of the weapon's GunCO
    /// term? **DEFAULT YES** — an omitted field means EXCLUDED, and `false` is
    /// the explicit opt-out nothing uses yet.
    ///
    /// The other reading — that the CO catalog "lists only discrepant attacks",
    /// so an unlisted perk feeds the term in full — loses 15 to 0. Eleven
    /// catalog rows print a DOUBLE value ("100 or 124"), the only rows where
    /// anyone measured an evolved weapon, and all eleven exclude; four measured
    /// perks agree (M49, M50), three of them unlisted. The Torid's is the
    /// decisive shape: two tier-2 perks giving panels of 102 and 82 both solve
    /// to a CO base of ~51.
    ///
    /// AND THE ERROR IS ASYMMETRIC — including it OVERSTATES, the worse
    /// direction for a calculator promising in-game numbers, and reaches 186
    /// weapon+perk pairs by 37% on average. THE FLAG IS THE PERK'S rather than
    /// the weapon's, because the catalog names perks. ON A `Multiplying` ENTRY
    /// THE CLASS ANSWERS FIRST and this is never read (M51): the two forms of
    /// one group can be different classes with OPPOSITE answers.
    #[serde(default)]
    pub(super) co_base_excludes_this_evolution: Option<bool>,
    /// …and on WHICH FORM it was measured, when the reading covers one of them.
    /// `base` or `incarnon`; omitted means the perk's whole transform group.
    ///
    /// A perk belongs to a GROUP and a reading comes off an ENTRY. Usually that
    /// gap does not matter, because the catalog rows name a weapon and both its
    /// forms behave alike. The Torid is where it does: both its forms are now
    /// measured on the same two perks and they answer OPPOSITELY — the Incarnon
    /// form is `Adding` and excludes (M50), the base form is `Multiplying` and
    /// feeds in full (M51). Recording the first without this scope would have
    /// asserted the second and been wrong.
    #[serde(default)]
    pub(super) co_base_excludes_only_form: Option<String>,
    /// *"Does not affect Incarnon Form"* — the whole perk is the BASE form's.
    ///
    /// It is the EVOLUTION's flag and not an effect's because that is how the
    /// card reads it: on all eleven entries carrying the sentence it is the
    /// last clause and it qualifies everything before it, magazine and ammo
    /// and range together.
    #[serde(default)]
    pub(super) base_form_only: bool,
    pub(super) effects: Vec<Value>,
}

pub(super) fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

pub(super) fn effect(v: &Value) -> Option<EvoEffect> {
    let kind = v.get("kind").and_then(Value::as_str)?;
    // A LIVE BUG SHORT-CIRCUITS THE KIND. Declared beside the effect it kills
    // rather than as a flag on the evolution, because a perk's two clauses can
    // disagree — Carnage Reign's +60 base damage works and its "+33% per
    // Status Type" does not (MEASUREMENTS M49). Intercepting here means every
    // effect kind gets it for free and no arm has to remember to check.
    // A MISPRINT DOES NOT SHORT-CIRCUIT, which is the whole difference between
    // it and `live_bug` one line below: the effect works and is parsed exactly
    // as it would be without the note. Collected by the caller, because it
    // belongs to the perk's card rather than to the effect's arithmetic.
    if let Some(note) = v.get("live_bug").and_then(Value::as_str) {
        return Some(EvoEffect::LiveBug {
            clause: kind.replace('_', " "),
            note: note.to_string(),
        });
    }
    Some(match kind {
        "flat_base_damage" => EvoEffect::FlatBaseDamage(f(v, "value").unwrap_or(0.0)),
        // …AND THE RELATIVE ONE, which is how a MELEE Genesis is written.
        "base_damage_bonus" => EvoEffect::BaseDamageBonus(f(v, "value").unwrap_or(0.0)),
        "initial_combo" => EvoEffect::InitialCombo(f(v, "value").unwrap_or(0.0)),
        "incarnon_window" => EvoEffect::IncarnonWindow {
            arm_at_combo: f(v, "arm_at_combo"),
            seconds: f(v, "seconds"),
        },
        "combo_count_on_slam_hit" => {
            EvoEffect::ComboCountOnSlamHit(f(v, "value").unwrap_or(0.0))
        }
        "melee_range_bonus_m" => EvoEffect::MeleeRange(f(v, "value").unwrap_or(0.0)),
        "follow_through_bonus" => EvoEffect::FollowThroughBonus(f(v, "value").unwrap_or(0.0)),
        "slam_radius_bonus" => EvoEffect::SlamRadiusBonus(f(v, "value").unwrap_or(0.0)),
        "heavy_windup_speed_bonus" => EvoEffect::HeavyWindUpSpeed(f(v, "value").unwrap_or(0.0)),
        "proc_conversion" => {
            let ty = |k: &str| {
                v.get(k).and_then(Value::as_str).and_then(crate::rules::damage::DamageType::from_name)
            };
            match (ty("from"), ty("to")) {
                (Some(from), Some(to)) => EvoEffect::ProcConversion {
                    from,
                    to,
                    chance: f(v, "value").unwrap_or(0.0),
                },
                // A TYPE THE ENGINE DOES NOT KNOW IS INERT, not a panic: an
                // evolution is transcribed by hand and a typo should read as a
                // perk that does nothing rather than take the roster down.
                _ => EvoEffect::Inert("proc_conversion with an unknown damage type".into()),
            }
        }
        "flat_base_crit_chance" => EvoEffect::FlatBaseCritChance(f(v, "value").unwrap_or(0.0)),
        "flat_base_multishot" => EvoEffect::FlatBaseMultishot(f(v, "value").unwrap_or(0.0)),
        "stacking_fire_rate_per_shell_reloaded" => {
            EvoEffect::StackingFireRatePerShellReloaded {
                per_stack: f(v, "per_stack").unwrap_or(0.0),
                max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(0) as u32,
            }
        }
        "flat_base_status_chance" => {
            EvoEffect::FlatBaseStatusChance(f(v, "value").unwrap_or(0.0))
        }
        "flat_base_status_chance_by_form" => EvoEffect::FlatBaseStatusChanceByForm {
            base: f(v, "base").unwrap_or(0.0),
            incarnon: f(v, "incarnon").unwrap_or(0.0),
        },
        "flat_base_crit_multiplier" => {
            EvoEffect::FlatBaseCritMultiplier(f(v, "value").unwrap_or(0.0))
        }
        "flat_base_damage_on_empty_reload" => {
            EvoEffect::FlatBaseDamageOnEmptyReload(f(v, "value").unwrap_or(0.0))
        }
        // A CONDITION IS NEVER IGNORED. This arm read `value` and nothing else
        // for as long as it existed, so Fortress Salvo's "With Armor Over 450"
        // paid out to everybody — the exact failure `gated_by_tenno` was written
        // to prevent, on a kind that predates it. The
        // conditional form is a GATE now, so the neutral frame's 105 armor
        // shuts it and a real frame opens it with no code change.
        "punch_through_bonus" if v.get("condition").is_some() => {
            match tenno_condition(v) {
                Some(gate) => EvoEffect::GatedByTenno {
                    gate,
                    grant: crate::model::GatedGrant::PunchThrough,
                    value: f(v, "value").unwrap_or(0.0),
                },
                None => EvoEffect::Inert(
                    "punch_through_bonus with an unreadable `condition:`".into(),
                ),
            }
        }
        // THE HANDLING FAMILY — the same kinds a mod states them with, from the
        // one table (`IndirectStat::from_kind`). NEGATIVE recoil means less, the
        // convention the mods carry (Primed Stabilizer ramps -0.15 -> -0.9).
        k if crate::model::IndirectStat::from_kind(k).is_some() => EvoEffect::Indirect(
            crate::model::IndirectStat::from_kind(k).expect("guarded"),
            f(v, "value").unwrap_or(0.0),
        ),
        "multishot_beyond_range" => EvoEffect::MultishotBeyondRange {
            value: f(v, "value").unwrap_or(0.0),
            metres: f(v, "metres").unwrap_or(0.0),
        },
        "ammo_reserve_set" => EvoEffect::AmmoMaxSet(f(v, "value").unwrap_or(0.0)),
        "flat_base_magazine" => EvoEffect::FlatBaseMagazine(f(v, "value").unwrap_or(0.0)),
        "field_duration_on_empty_reload" => {
            EvoEffect::FieldDurationOnEmptyReload(f(v, "value").unwrap_or(1.0))
        }
        // `base:` IS REQUIRED, with no default, because the two spellings of
        // this perk are two different mechanics and a default would silently
        // pick one. A yaml that does not say loads as Inert and is reported as
        // unmodelled, which is the honest outcome for a card nobody has read.
        "armor_strip_per_puncture_status" => {
            EvoEffect::ArmorStripPerPunctureStatus(f(v, "value").unwrap_or(0.0))
        }
        "multishot_on_last_round" => match v.get("base").and_then(serde_norway::Value::as_bool) {
            Some(base) => EvoEffect::MultishotOnLastRound {
                value: f(v, "value").unwrap_or(0.0),
                base,
            },
            None => EvoEffect::Inert("multishot_on_last_round without `base:`".into()),
        },
        "multishot_consumes_ammo" => {
            EvoEffect::MultishotConsumesAmmo(f(v, "value").unwrap_or(0.0))
        }
        // REAVER'S RAPTURE, and the trigger is what picks this arm: a
        // `base_damage` grant on a `full_burst` trigger. Both are read rather
        // than assumed — the same grant on another trigger is a different perk
        // and stays inert until someone models it.
        "stacking_buff"
            if v.get("trigger").and_then(Value::as_str) == Some("full_burst")
                && v.get("grants").and_then(Value::as_str) == Some("base_damage") =>
        {
            EvoEffect::BaseDamagePerFullBurst {
                per_stack: f(v, "per_stack").unwrap_or(0.0),
                max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            }
        }
        // THE GENERAL ARM. Everything the sim's own vocabulary can already
        // express, named by a yaml: trigger, grant, size, cap, clock, decay and
        // what takes the pile. It sits BELOW the arm above, which carries
        // reasoning a generic one cannot.
        "stacking_buff"
            if v.get("trigger").and_then(Value::as_str).and_then(crate::model::BuffTrigger::from_id).is_some()
                && v.get("grants").and_then(Value::as_str).and_then(crate::model::BuffGrant::from_id).is_some() =>
        {
            let trigger = crate::model::BuffTrigger::from_id(v.get("trigger").and_then(Value::as_str).unwrap()).unwrap();
            let grant = crate::model::BuffGrant::from_id(v.get("grants").and_then(Value::as_str).unwrap()).unwrap();
            let duration = f(v, "duration_seconds")
                .or_else(|| f(v, "duration"))
                .unwrap_or(crate::model::NO_TIMEOUT);
            EvoEffect::StackingGrant {
                trigger,
                grant,
                per_stack: f(v, "per_stack").unwrap_or(0.0),
                max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
                duration,
                // Default 1.0 so a perk that does NOT roll reads as certain
                // rather than as never firing — and a hidden roll is the one
                // thing the rendered wiki card omits (Headcracker's 50%), so
                // this default is only ever right when someone checked.
                chance: f(v, "chance").unwrap_or(1.0),
                // The Galvanized family unless the card says otherwise: one
                // stack drops on timeout and the timer restarts.
                decay: crate::model::BuffDecay::from_id(v.get("decay").and_then(Value::as_str)),
                cleared_by: crate::model::ClearedBy::from_id(v.get("cleared_by").and_then(Value::as_str)),
                // TRANSCRIBED FROM THE CARD, and false is not "it decays" — it
                // is "the card does not say". Two of the twenty buffs that
                // reach this arm with no clock and no clear actually state it,
                // and the other eighteen simply never filled the key in.
                card_opens_full: v
                    .get("card_opens_full")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }
        }
        "stacking_buff" => {
            // Only the multishot grant is modeled here (Fevered Frenzy, whose
            // trigger the sim has no event for); other payloads load inert.
            let max = v.get("max_stacks").and_then(Value::as_u64).unwrap_or(0);
            match v.get("grants").and_then(Value::as_str) {
                Some("multishot") => EvoEffect::AssumedMaxMultishot {
                    total: f(v, "per_stack").unwrap_or(0.0) * max as f64,
                    max_stacks: max as u32,
                },
                // NAME the payload. "unmodeled payload" told the pinned inert
                // list nothing: two different unmodelled buffs read as the
                // same entry, and neither said what it granted.
                other => EvoEffect::Inert(format!("stacking_buff {}", other.unwrap_or("no payload"))),
            }
        }
        // THE CONDITION IS READ NOW, and it is a question about the PLAYER
        // rather than about this weapon: "With Sprint Speed 1.2 or Higher".
        // Unread, the perk paid out on every build including the ones that
        // cannot reach the threshold.
        "condition_overload" => EvoEffect::ConditionOverload {
            per_type: f(v, "value").unwrap_or(0.0),
            min_sprint: sprint_condition(v),
        },
        "crit_on_undamaged" => EvoEffect::CritOnUndamaged {
            crit_chance: f(v, "crit_chance").unwrap_or(0.0),
            crit_multiplier: f(v, "crit_multiplier").unwrap_or(0.0),
        },
        // ONE KIND FOR EVERY "WITH <player stat>" PERK. The `grant:` names the
        // bracket, so a multishot gate and a crit-damage gate cannot be
        // confused for one another, and an unreadable `condition:` falls to
        // Inert rather than paying out unconditionally.
        "magazine_growth_on_empty_reload" => EvoEffect::MagGrowthOnEmptyReload {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
        },
        "instant_reload_on_kill" => {
            EvoEffect::InstantReloadOnKill { chance: f(v, "chance").unwrap_or(0.0) }
        }
        "round_restore_on_status_hit" => {
            let Some(status) = v.get("status").and_then(Value::as_str).and_then(crate::rules::damage::DamageType::from_name)
            else {
                return Some(EvoEffect::Inert(
                    "round_restore_on_status_hit with an unreadable `status:`".into(),
                ));
            };
            EvoEffect::RoundRestoreOnStatusHit {
                status,
                chance: f(v, "chance").unwrap_or(0.0),
                rounds: f(v, "rounds").unwrap_or(1.0),
            }
        }
        "crit_chance_by_body_part" => EvoEffect::CritChanceByBodyPart {
            // No defaults that pay out: a missing multiplier is 1 (ordinary),
            // a missing bonus is 0.
            bodyshot_multiplier: f(v, "bodyshot_multiplier").unwrap_or(1.0),
            weakpoint_bonus: f(v, "weakpoint_bonus").unwrap_or(0.0),
        },
        "status_chance_from_crit_chance" => EvoEffect::DerivedStat {
            from_crit: true,
            rate: f(v, "rate").unwrap_or(0.0),
            cap: f(v, "cap").unwrap_or(0.0),
        },
        "crit_chance_from_status_chance" => EvoEffect::DerivedStat {
            from_crit: false,
            rate: f(v, "rate").unwrap_or(0.0),
            cap: f(v, "cap").unwrap_or(0.0),
        },
        "gated_by_tenno" => {
            let Some(gate) = tenno_condition(v) else {
                return Some(EvoEffect::Inert("gated_by_tenno with an unreadable `condition:`".into()));
            };
            let grant = match v.get("grants").and_then(Value::as_str) {
                Some("condition_overload") => crate::model::GatedGrant::ConditionOverload,
                Some("fire_rate") => crate::model::GatedGrant::FireRate,
                Some("multishot") => crate::model::GatedGrant::Multishot,
                Some("base_crit_damage") => crate::model::GatedGrant::BaseCritDamage,
                Some("projectile_speed") => crate::model::GatedGrant::ProjectileSpeed,
                Some("accuracy") => crate::model::GatedGrant::Accuracy,
                Some("flat_base_damage") => crate::model::GatedGrant::FlatBaseDamage,
                Some("flat_base_magazine") => crate::model::GatedGrant::FlatBaseMagazine,
                other => {
                    return Some(EvoEffect::Inert(format!(
                        "gated_by_tenno grants {}, which is not a bracket this engine has",
                        other.unwrap_or("nothing")
                    )))
                }
            };
            EvoEffect::GatedByTenno { gate, grant, value: f(v, "value").unwrap_or(0.0) }
        }
        "base_damage_below_half_health" => {
            EvoEffect::BaseDamageBelowHalfHealth {
                rate: f(v, "value").unwrap_or(0.0),
                excludes_own_flat: v
                    .get("excludes_own_flat")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }
        }
        "fire_rate_bonus" => EvoEffect::FireRateBonus {
            value: f(v, "value").unwrap_or(0.0),
            // THE SAME `condition:` VOCABULARY the CO kind reads. One syntax
            // for "this perk asks about the player", so the second grant to
            // need it did not invent a second spelling.
            min_sprint: sprint_condition(v),
            // …AND THE ONE THAT ASKS WHETHER THE WEAPON IS DRAWN. *"With Melee
            // Weapon Equipped"* is not satisfied by a quick-melee swing, and
            // the arena's default is drawn.
            needs_melee_equipped: v
                .get("condition")
                .and_then(Value::as_str)
                .is_some_and(|c| c == "melee_equipped"),
        },
        // The CONDITION is the only thing that varies between the roster's
        // three copies, and it is read rather than assumed: absent means any
        // headshot pays (the Furis pair), `headshot_kill` means only a killing
        // one does (the Phenmor).
        "headshot_damage_on_headshot_streak" => EvoEffect::HeadshotDamageOnStreak {
            hits: v.get("hits").and_then(Value::as_u64).unwrap_or(0) as u32,
            within: f(v, "within_seconds").unwrap_or(0.0),
            value: f(v, "value").unwrap_or(0.0),
            duration: f(v, "duration_seconds").unwrap_or(0.0),
        },
        "crit_multiplier_below_status_count" => EvoEffect::CritDamageBelowStatusCount {
            threshold: v.get("threshold").and_then(Value::as_u64).unwrap_or(0) as u32,
            value: f(v, "value").unwrap_or(0.0),
        },
        "instant_reload_on_headshot" => EvoEffect::InstantReloadOnHeadshot {
            chance: f(v, "chance").unwrap_or(0.0),
            needs_kill: v.get("condition").and_then(Value::as_str) == Some("headshot_kill"),
        },
        // CONDITIONAL ONES STAY INERT. Ready Retaliation spells the same kind
        // with a `condition:`, which nothing here reads — so it falls through to
        // `Inert` and keeps saying so on its tile.
        "reload_speed_bonus" if v.get("condition").is_none() => {
            EvoEffect::ReloadSpeedBonus(f(v, "value").unwrap_or(0.0))
        }
        // …AND THE CONDITIONAL ONE, which needs a WINDOW to be a buff at all.
        // Only the Phenmor's page publishes one ("for 6 seconds"); the other
        // eleven Ready Retaliations state the bonus and no duration, and a
        // window nobody published is not one to invent — those files say so and
        // stay inert. So the duration is REQUIRED here rather than defaulted:
        // a missing one falls through to `Inert`, which is the honest answer
        // and the one whose tile says why.
        // NO `duration_seconds` REQUIRED any more, and that is the whole reason
        // eleven of the twelve weapons carrying this perk were inert. Only the
        // Phenmor's page publishes a window; the rest state the bonus and
        // nothing else — which read as missing data while the model was a
        // timer, and reads as "there is nothing to state" now that the window
        // IS the reload action.
        "reload_speed_bonus"
            if v.get("condition").and_then(Value::as_str) == Some("reload_from_empty") =>
        {
            EvoEffect::ReloadSpeedOnEmptyReload { value: f(v, "value").unwrap_or(0.0) }
        }
        // ON FIRING. `base:` is REQUIRED rather than defaulted, for the same
        // reason `multishot_on_last_round` requires it: the two brackets differ
        // only on builds that carry a multishot mod, so a wrong default is a
        // number that looks right on a bare weapon and is wrong on every real
        // build.
        // AN EDGE, DECLARED. The clause is transcribed as written and the
        // reason is one of UNMODELLED.md's classes — so the page can say "this
        // cannot pay out here, and here is why" instead of "not modelled yet".
        "out_of_scope" => {
            let Some(reason) = v.get("reason").and_then(Value::as_str).and_then(Scope::parse) else {
                return Some(EvoEffect::Inert("out_of_scope without a known `reason:`".into()));
            };
            EvoEffect::OutOfScope {
                clause: v
                    .get("clause")
                    .and_then(Value::as_str)
                    .unwrap_or("(no clause)")
                    .to_string(),
                reason,
            }
        }
        "stacking_multishot_on_firing" => {
            let Some(base) = v.get("base").and_then(Value::as_bool) else {
                return Some(EvoEffect::Inert("stacking_multishot_on_firing without `base:`".into()));
            };
            EvoEffect::StackingMultishotOnFiring {
                per_stack: f(v, "per_stack").unwrap_or(0.0),
                max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
                base,
            }
        }
        // THE STATUS IS IN THE KIND, and it is READ rather than matched one
        // spelling at a time. This arm was hardcoded to Electricity for the
        // Furis's Stormburst, so the Latron family's Riddled Target — the same
        // mechanic, triggered by PUNCTURE — sat inert beside machinery that
        // already did everything it needed.
        k if k.starts_with("stacking_multishot_on_") && k.ends_with("_status") => {
            let name = &k["stacking_multishot_on_".len()..k.len() - "_status".len()];
            let Some(status) = crate::rules::damage::DamageType::from_name(name) else {
                // A type this engine does not know is reported, not silently
                // dropped: the kind NAMES it, so a typo would otherwise read as
                // a perk DE never wrote.
                return Some(EvoEffect::Inert(format!("stacking multishot on {name} status")));
            };
            EvoEffect::StackingMultishotOnStatus {
                status,
                per_stack: f(v, "per_stack").unwrap_or(0.0),
                max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
                // Two spellings in the roster, both meaning seconds.
                duration: f(v, "duration").or_else(|| f(v, "duration_seconds")).unwrap_or(0.0),
            }
        }
        "on_headshot_fire_rate" => EvoEffect::StackingFireRateOnHeadshot {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            duration: f(v, "duration").unwrap_or(0.0),
            // Default 1.0 so a perk that does NOT roll reads as certain rather
            // than as never firing.
            chance: f(v, "chance").unwrap_or(1.0),
            // The Galvanized family unless the card says otherwise — the same
            // default and the same word every other stacking buff here uses.
            decay: crate::model::BuffDecay::from_id(v.get("decay").and_then(Value::as_str)),
        },
        "crit_multiplier_below_crit_chance" => EvoEffect::CritMultiplierBelowCritChance {
            value: f(v, "value").unwrap_or(0.0),
            below: f(v, "below_crit_chance").unwrap_or(0.0),
        },
        "flat_crit_chance_after_mods" => {
            EvoEffect::PostModCritChance(f(v, "value").unwrap_or(0.0))
        }
        "flat_status_chance_after_mods" => {
            EvoEffect::PostModStatusChance(f(v, "value").unwrap_or(0.0))
        }
        "headshot_damage" => EvoEffect::HeadshotDamage(f(v, "value").unwrap_or(0.0)),
        "chance_damage_on_noncrit" => EvoEffect::ChanceDamageOnNoncrit {
            chance: f(v, "chance").unwrap_or(0.0),
            value: f(v, "value").unwrap_or(0.0),
        },
        "incarnon_charge_rate" => EvoEffect::IncarnonChargeRate(f(v, "value").unwrap_or(0.0)),
        "stacking_damage_on_plain_hit" => EvoEffect::StackingDamageOnPlainHit {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            duration: f(v, "duration").unwrap_or(0.0),
        },
        "on_headshot_reload_speed" => EvoEffect::StackingReloadSpeedOnHeadshot {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            duration: f(v, "duration").unwrap_or(0.0),
        },
        "unlocks_weapon" => EvoEffect::UnlocksForm(
            v.get("weapon")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        // A QUALIFIER, not an effect: "Stacks up to 4x" caps the bonus above
        // it. See `EvoEffect::Qualifier`.
        other if other.starts_with("unmodelled_stacks_up_to") => {
            EvoEffect::Qualifier(other.to_string())
        }
        other => EvoEffect::Inert(other.to_string()),
    })
}

/// `condition: "sprint_speed >= 1.2"`, as a number — 0 when the card states no
/// speed. Kept for the two kinds that spell their gate this way.
pub(super) fn sprint_condition(v: &Value) -> f64 {
    match tenno_condition(v) {
        Some(crate::model::TennoCondition::SprintAtLeast(x)) => x,
        _ => 0.0,
    }
}

/// The `condition:` a perk asks of the player. An unknown word is `None`, which
/// the caller turns into an inert effect rather than a silently ungated grant.
pub(super) fn tenno_condition(v: &Value) -> Option<crate::model::TennoCondition> {
    v.get("condition").and_then(Value::as_str).and_then(crate::model::TennoCondition::from_id)
}
