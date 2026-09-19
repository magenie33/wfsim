use super::*;

/// Apply a chosen evolution set onto a weapon's RAW base panel.
///
/// Order-independent: flat base damage sums first, then the vector scales
/// pro-rata ONCE; `co_base_fraction` = original / evolved total — the wiki
/// CO-catalog rule that every GunCO source computes on the pre-evolution
/// base ("CO-bonus does not use base damage increase Evolution").
/// `currently_broken` evolutions apply nothing.
pub fn apply(base: &mut WeaponBase, evos: &[&EvolutionDef]) {
    let original_total = base.base_vector.total();
    let mut flat = 0.0;
    // …AND HOW MUCH OF IT THE GunCO TERM'S BASE GROWS BY. Two sums rather than
    // one plus a flag, because a build can carry two flat-damage perks that
    // DISAGREE — the catalog says the Despair is exactly that, one tier-2
    // option excluded and the other not. The old code held a single ratio and
    // could only have been right about that pair by accident.
    let mut flat_into_co = 0.0;
    // "…but does not take into account the Base Damage increase from THIS
    // perk". Held as a pair of sums and resolved once `evolved` exists: the
    // rate as written, and the same rate weighted by the perk's own flat add,
    // so the correction is `Σr - Σ(r·own) / evolved` and no perk needs to know
    // what the others granted.
    let (mut half_hp_rate, mut half_hp_rate_own) = (0.0f64, 0.0f64);
    for e in evos
        .iter()
        .filter(|e| !e.currently_broken)
        // "Does not affect Incarnon Form" — the perk is EQUIPPED either way
        // (it is the same Genesis ladder), so it is skipped HERE, on the form
        // it does not reach, rather than refused at selection. On the base
        // form's panel it applies in full.
        .filter(|e| !(e.base_form_only && base.form == crate::model::FormKind::Incarnon))
    {
        for eff in &e.effects {
            match eff {
                // NOTHING TO APPLY. The form it unlocks is a separate weapon
                // entry with its own stats, so applying anything here would
                // count them twice.
                EvoEffect::UnlocksForm(_) => {}
                // NOTHING TO APPLY, because the game applies nothing. The
                // clause is kept so the card can say so ().
                EvoEffect::LiveBug { .. } => {}
                // …and nothing to apply for an EDGE either, by definition.
                EvoEffect::OutOfScope { .. } => {}
                // EACH PERK DECIDES ITS OWN CONTRIBUTION to the CO base,
                // which is the whole point of holding an absolute: two perks on
                // one build may disagree and there is no single ratio that
                // describes the pair.
                EvoEffect::FlatBaseDamage(v) => {
                    flat += v;
                    if !e.excludes_co_base(base.form, base.co_behavior) {
                        flat_into_co += v;
                    }
                }
                // Same bucket as the line above: it is base damage, and the
                // run is modelled holding it (see the variant's note).
                // Into the base like any other flat damage — the buff OPENS
                // FULL — and recorded so the buff card can take it back off.
                EvoEffect::FlatBaseDamageOnEmptyReload(v) => {
                    flat += v;
                    // …AND SO DOES THIS ONE, by the same rule. It is a flat
                    // base add wearing a trigger, and nothing about the trigger
                    // changes which base the CO term reads.
                    if !e.excludes_co_base(base.form, base.co_behavior) {
                        flat_into_co += v;
                    }
                    base.reload_damage_buff += v;
                }
                // Into the SAME additive bucket a mod's indirect stat uses;
                // `resolve` seeds the panel from here.
                EvoEffect::Indirect(stat, v) => {
                    match base.indirect.iter_mut().find(|(s, _)| s == stat) {
                        Some(e) => e.1 += v,
                        None => base.indirect.push((*stat, *v)),
                    }
                }
                EvoEffect::AmmoMaxSet(v) => base.ammo_reserve = *v,
                // A base-stat evolution is a WEAPON stat change, so it lands
                // on EVERY attack part, not just the direct hit. That is the
                // same reading `resolve` already applies to Elemental Excess's
                // post-mod layer ("a WEAPON stat change, so the explosion takes
                // it too"), and the base layer is the more clearly weapon-wide
                // of the two.
                //
                // INFERENCE, not a citation: no source states whether Torid's
                // Commodore's Fortune / Survivor's Edge / Elemental Balance
                // reach its Toxin cloud. It matters — the cloud is most of that
                // weapon's damage — so it is called out here and in MECHANICS.
                // Nothing else in the roster is affected: only Dual Toxocyst
                // (no radial, no field) and the Torid have base-stat
                // evolutions at all.
                EvoEffect::FlatBaseCritChance(v) => {
                    base.base_crit_chance += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_crit_chance += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_crit_chance += v;
                    }
                }
                // BASE multishot, so the multishot MODS multiply it — the
                // same bracket a weapon's own innate multishot sits in. Not
                // pushed into the radial or the field: an explosion fires per
                // projectile already (`radius_takes_multishot`), so adding it
                // there would count the same pellets twice.
                EvoEffect::FlatBaseMultishot(v) => base.base_multishot += v,
                // ---- THE MELEE FIVE ------------------------------------
                //
                // Each one lands in the channel `resolve` reads it from, so an
                // Acuity that locks the stat can refuse the evolution's share
                // separately from the mods' — the reason `evo_fire_rate_bonus`
                // is a channel of its own rather than a term folded into `fr`.
                EvoEffect::BaseDamageBonus(v) => base.evo_base_damage_bonus += v,
                EvoEffect::InitialCombo(v) => base.evo_initial_combo += v,
                EvoEffect::ComboCountOnSlamHit(v) => base.evo_combo_count_on_slam_hit += v,
                EvoEffect::MeleeRange(v) => base.evo_melee_range_m += v,
                EvoEffect::IncarnonWindow { arm_at_combo, seconds } => {
                    let w = base.melee_incarnon.get_or_insert(crate::model::MeleeIncarnon {
                        arm_at_combo: f64::INFINITY,
                        seconds: 0.0,
                    });
                    if let Some(a) = arm_at_combo {
                        w.arm_at_combo = w.arm_at_combo.min(*a);
                    }
                    if let Some(s) = seconds {
                        w.seconds = *s;
                    }
                }
                EvoEffect::FollowThroughBonus(v) => base.evo_follow_through_bonus += v,
                EvoEffect::SlamRadiusBonus(v) => base.evo_slam_radius_bonus += v,
                EvoEffect::HeavyWindUpSpeed(v) => base.evo_heavy_windup_speed += v,
                // THE SAME RUNTIME HEMORRHAGE USES, reached from the evolution
                // side. `low_rate_*` is the mod's own clause and has no
                // evolution spelling, so it is the identity.
                EvoEffect::ProcConversion { from, to, chance } => {
                    base.evo_proc_conversion = Some((*from, *to, *chance));
                }
                EvoEffect::StackingFireRatePerShellReloaded { per_stack, max_stacks } => {
                    base.stacking_buffs.push(crate::model::StackingBuff {
                        id: "per_shell_fire_rate",
                        trigger: crate::model::BuffTrigger::ReloadComplete,
                        grant: crate::model::BuffGrant::FireRate,
                        // NOTHING TAKES THEM but holstering, and a holster is
                        // not something this arena does — so no clock, and the
                        // decay mode never runs.
                        decay: crate::model::BuffDecay::LoseOneAndReset,
                        duration: crate::model::NO_TIMEOUT,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        chance: 1.0,
                        initial_stacks: 0,
                        // 0 = ONE PER SHELL. `resolve` reads the modded
                        // magazine and turns it into a number, and `per_shell`
                        // keeps the RULE after the number replaces it — the
                        // Incarnon route loads a known count of shells and has
                        // to know which buffs are counting them.
                        stacks_per_trigger: 0,
                        per_shell: true,
                        cleared_by: crate::model::ClearedBy::EmptyMagazine,
                        card_opens_full: false,
                    });
                }
                EvoEffect::FlatBaseStatusChance(v) => {
                    base.base_status_chance += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_status_chance += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_status_chance += v;
                    }
                }
                EvoEffect::FlatBaseStatusChanceByForm { base: b, incarnon } => {
                    // The Incarnon entry is the one carrying the `incarnon:`
                    // block — the same gate `FlatBaseMagazine` uses to keep a
                    // magazine evolution off the charge pool.
                    let v = if base.gauge_form.is_some() { *incarnon } else { *b };
                    base.base_status_chance += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_status_chance += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_status_chance += v;
                    }
                }
                EvoEffect::FlatBaseCritMultiplier(v) => {
                    base.base_crit_damage += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_crit_damage += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_crit_damage += v;
                    }
                }
                // BASE FORM ONLY, and the gate is load-bearing: an Incarnon
                // form's `magazine_size` IS its charge pool (the pseudo-reload
                // rounds), so an ungated `+=` handed Extended Volley's +9 to
                // the 170-round gauge as well — "Does not apply to Incarnon
                // Form's Magazine" (wiki), and that magazine is outside the
                // ammo system entirely.
                EvoEffect::FlatBaseMagazine(v) => {
                    if base.gauge_form.is_none() {
                        base.magazine_size += v;
                    }
                }
                EvoEffect::FieldDurationOnEmptyReload(v) => {
                    base.field_duration_on_empty_reload = *v;
                }
                EvoEffect::MultishotBeyondRange { value, metres } => {
                    base.multishot_beyond_range = Some((*value, *metres));
                }
                // BASE FORM ONLY: `incarnon.is_some()` marks the charge-backed
                // form, whose magazine is the gauge's round pool rather than a
                // reloaded magazine — nothing there is "the last round".
                // IT LIVES ON BOTH FORMS, and "cannot be stacked in Incarnon
                // form" falls out rather than being enforced: the Burston's
                // Incarnon is an AUTO weapon, so no burst ever completes there
                // and the trigger cannot fire. Enforcing it by form id would be
                // a rule that has to be right; deriving it from the trigger is
                // a rule that cannot be wrong.
                //
                // "Resets when activating incarnon" is the ordinary
                // `MagazineRefilled` clear — swapping either way reloads the
                // base magazine — so it needs nothing of its own either.
                EvoEffect::ArmorStripPerPunctureStatus(v) => {
                    base.armor_strip_per_puncture = *v;
                }
                EvoEffect::BaseDamagePerFullBurst { per_stack, max_stacks } => {
                    base.stacking_buffs.push(crate::model::StackingBuff {
                        id: "full_burst_damage",
                        trigger: crate::model::BuffTrigger::FullBurst,
                        grant: crate::model::BuffGrant::BaseDamage,
                        decay: crate::model::BuffDecay::LoseOneAndReset,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        // NO CLOCK. "Resets on Reload" is not a duration, and a
                        // timeout would quietly drop stacks a player still has.
                        duration: crate::model::NO_TIMEOUT,
                        chance: 1.0,
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::model::ClearedBy::MagazineRefilled,
                        card_opens_full: false,
                    });
                }
                EvoEffect::MultishotOnLastRound { value, base: is_base } => {
                    if base.gauge_form.is_none() {
                        if *is_base {
                            base.base_multishot_on_last_round = *value;
                        } else {
                            base.multishot_on_last_round = *value;
                        }
                    }
                }
                // "Affects both modes" — unlike Final Fusillade this one lands
                // on the charge-backed form too; what differs is the RULE, and
                // the sim picks that off `continuous`, not off the form id.
                EvoEffect::MultishotConsumesAmmo(v) => base.multishot_ammo_bonus = *v,
                EvoEffect::AssumedMaxMultishot { total, max_stacks } => {
                    base.buff_multishot_bonus += total;
                    base.buff_multishot_max_stacks = base.buff_multishot_max_stacks.max(*max_stacks);
                }
                // CARRIED, NOT SPENT, when the perk states a speed: `apply`
                // works on the raw weapon and the player is not here — the
                // condition is answered in `resolve_for`, which has the Tenno.
                EvoEffect::ConditionOverload { per_type, min_sprint } => {
                    if *min_sprint > 0.0 {
                        base.gated.push(crate::model::GatedTerm {
                            gate: crate::model::TennoCondition::SprintAtLeast(*min_sprint),
                            grant: crate::model::GatedGrant::ConditionOverload,
                            value: *per_type,
                            into_co: 0.0,
                        });
                    } else {
                        base.innate_co_per_type += per_type;
                    }
                }
                // CARRIED, NOT SPENT, when the perk states a speed — the
                // player is not here. Answered in `resolve_for`, exactly like
                // the Condition Overload gate beside it.
                EvoEffect::CritOnUndamaged { crit_chance, crit_multiplier } => {
                    base.crit_chance_on_undamaged += crit_chance;
                    base.crit_damage_on_undamaged += crit_multiplier;
                }
                // THE PERK ANSWERS FOR ITS OWN GATED HALF, here where it still
                // exists. `resolve_for` opens the gate long after `e` is gone,
                // so a flat add that decided this there would answer
                // differently from the unconditional half of the same card.
                EvoEffect::GatedByTenno { gate, grant, value } => {
                    let feeds = *grant == crate::model::GatedGrant::FlatBaseDamage
                        && !e.excludes_co_base(base.form, base.co_behavior);
                    base.gated.push(crate::model::GatedTerm {
                        gate: *gate,
                        grant: *grant,
                        value: *value,
                        into_co: if feeds { *value } else { 0.0 },
                    });
                }
                EvoEffect::MagGrowthOnEmptyReload { per_stack, max_stacks } => {
                    base.magazine_growth_on_empty_reload = Some((*per_stack, *max_stacks));
                }
                EvoEffect::InstantReloadOnKill { chance } => {
                    base.instant_reload_on_kill = Some(*chance);
                }
                EvoEffect::RoundRestoreOnStatusHit { status, chance, rounds } => {
                    base.round_restore_on_status = Some((*status, *chance, *rounds));
                }
                EvoEffect::CritChanceByBodyPart { bodyshot_multiplier, weakpoint_bonus } => {
                    // MULTIPLICATIVE, so it composes rather than replaces —
                    // two such perks on one weapon would multiply, which is
                    // what "multiplicative with all sources" means.
                    base.bodyshot_crit_chance_multiplier *= *bodyshot_multiplier;
                    base.evo_weakpoint_crit_chance_relative += *weakpoint_bonus;
                }
                EvoEffect::DerivedStat { from_crit, rate, cap } => {
                    if *from_crit {
                        base.base_status_from_crit = Some((*rate, *cap));
                    } else {
                        base.base_crit_from_status = Some((*rate, *cap));
                    }
                }
                EvoEffect::BaseDamageBelowHalfHealth { rate, excludes_own_flat } => {
                    half_hp_rate += rate;
                    if *excludes_own_flat {
                        half_hp_rate_own += rate * e.flat_base_damage();
                    }
                }
                EvoEffect::FireRateBonus { value, min_sprint, needs_melee_equipped } => {
                    let gate = if *min_sprint > 0.0 {
                        Some(crate::model::TennoCondition::SprintAtLeast(*min_sprint))
                    } else if *needs_melee_equipped {
                        Some(crate::model::TennoCondition::MeleeEquipped)
                    } else {
                        None
                    };
                    match gate {
                        Some(g) => {
                            base.gated.push(crate::model::GatedTerm {
                                gate: g,
                                grant: crate::model::GatedGrant::FireRate,
                                value: *value,
                                into_co: 0.0,
                            });
                        }
                        None => base.evo_fire_rate_bonus += value,
                    }
                }
                EvoEffect::ReloadSpeedBonus(v) => base.evo_reload_bonus += v,
                EvoEffect::InstantReloadOnHeadshot { chance, needs_kill } => {
                    base.instant_reload_on_headshot =
                        Some(crate::model::InstantReload { chance: *chance, needs_kill: *needs_kill });
                }
                EvoEffect::HeadshotDamageOnStreak { hits, within, value, duration } => {
                    base.headshot_streak = Some(crate::model::HeadshotStreak {
                        hits: *hits,
                        within: *within,
                        value: *value,
                        duration: *duration,
                    });
                }
                EvoEffect::CritDamageBelowStatusCount { threshold, value } => {
                    base.crit_damage_below_status_count = Some((*threshold, *value));
                }
                EvoEffect::ReloadSpeedOnEmptyReload { value } => {
                    base.rs_on_empty_reload = *value;
                }
                // Carried, not applied: `apply` works on the RAW base panel and
                // the condition needs the crit chance the mods produce, which
                // does not exist until `resolve` runs — and not even there in
                // full, since the live half only exists once a shot lands.
                EvoEffect::CritMultiplierBelowCritChance { value, below } => {
                    base.crit_multiplier_below_crit_chance = Some((*value, *below));
                }
                EvoEffect::StackingGrant {
                    trigger, grant, per_stack, max_stacks, duration, chance, decay, cleared_by,
                    card_opens_full,
                } => {
                    base.stacking_buffs.push(crate::model::StackingBuff {
                        id: stacking_card_id(*trigger, *grant),
                        trigger: *trigger,
                        grant: *grant,
                        decay: *decay,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: *chance,
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: *cleared_by,
                        card_opens_full: *card_opens_full,
                    });
                }
                EvoEffect::StackingMultishotOnFiring { per_stack, max_stacks, base: is_base } => {
                    base.stacking_buffs.push(crate::model::StackingBuff {
                        id: "on_firing_multishot",
                        trigger: crate::model::BuffTrigger::Firing,
                        grant: if *is_base {
                            crate::model::BuffGrant::BaseMultishot
                        } else {
                            crate::model::BuffGrant::Multishot
                        },
                        // NO CLOCK, so the decay never runs; the reload is what
                        // ends it. Both wiki pages say so in the same words —
                        // "There is no timer" (Sybaris), "resets entirely upon
                        // reloading" (Strun).
                        decay: crate::model::BuffDecay::PerStackExpiry,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: crate::model::NO_TIMEOUT,
                        chance: 1.0,
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::model::ClearedBy::Reload,
                        card_opens_full: false,
                    });
                }
                EvoEffect::StackingMultishotOnStatus { status, per_stack, max_stacks, duration } => {
                    base.stacking_buffs.push(crate::model::StackingBuff {
                        id: "on_status_multishot",
                        trigger: crate::model::BuffTrigger::HitEnemyWithStatus(*status),
                        grant: crate::model::BuffGrant::FlatMultishot,
                        // FIFO, each stack on its own clock — owner
                        // observed in game, Stormburst and Riddled Target
                        // (M102) alike. Harsher than the Galvanized family:
                        // holding 3 needs 3 hits per window, not one.
                        decay: crate::model::BuffDecay::PerStackExpiry,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: 1.0,
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::model::ClearedBy::Nothing,
                        card_opens_full: false,
                    });
                }
                EvoEffect::StackingFireRateOnHeadshot { per_stack, max_stacks, duration, chance, decay } => {
                    base.stacking_buffs.push(crate::model::StackingBuff {
                        id: "on_headshot_fire_rate",
                        decay: *decay,
                        trigger: crate::model::BuffTrigger::Headshot,
                        grant: crate::model::BuffGrant::FireRate,
                        // A FRACTION here; `resolve` turns it into an absolute
                        // rate against the base, which is the bucket it joins.
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: *chance,
                        // EARNED from zero, like every other timed buff.
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::model::ClearedBy::Nothing,
                        card_opens_full: false,
                    });
                }
                EvoEffect::PostModCritChance(v) => base.post_mod_crit_chance += v,
                EvoEffect::PostModStatusChance(v) => base.post_mod_status_chance += v,
                EvoEffect::HeadshotDamage(v) => base.headshot_damage_bonus += v,
                EvoEffect::ChanceDamageOnNoncrit { chance, value } => {
                    base.noncrit_bonus = Some((*chance, *value));
                }
                EvoEffect::IncarnonChargeRate(v) => {
                    if let Some(i) = base.gauge_form.as_mut() {
                        i.charge_rate += v;
                    }
                }
                EvoEffect::StackingDamageOnPlainHit {
                    per_stack,
                    max_stacks,
                    duration,
                } => {
                    base.stacking_buffs.push(crate::model::StackingBuff {
                        id: "on_plain_hit_damage",
                        // The Galvanized family, as each perk's own wiki text says.
                        decay: crate::model::BuffDecay::LoseOneAndReset,
                        trigger: crate::model::BuffTrigger::PlainHit,
                        grant: crate::model::BuffGrant::BaseDamage,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: 1.0,
                        // EARNED from zero, like every other TIMED buff: it
                        // has a duration, so a lull empties it and the fight
                        // has to fill it again (docs/BUFFS.md).
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::model::ClearedBy::Nothing,
                        card_opens_full: false,
                    });
                }
                EvoEffect::StackingReloadSpeedOnHeadshot {
                    per_stack,
                    max_stacks,
                    duration,
                } => {
                    base.stacking_buffs.push(crate::model::StackingBuff {
                        id: "on_headshot_reload_speed",
                        // The Galvanized family, as each perk's own wiki text says.
                        decay: crate::model::BuffDecay::LoseOneAndReset,
                        trigger: crate::model::BuffTrigger::Headshot,
                        grant: crate::model::BuffGrant::ReloadSpeed,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: 1.0,
                        // EARNED from zero, like every other timed buff.
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::model::ClearedBy::Nothing,
                        card_opens_full: false,
                    });
                }
                EvoEffect::Inert(_) | EvoEffect::Qualifier(_) => {}
            }
        }
    }
    // …and the below-half-health rate, corrected against the base it will
    // actually multiply. With no flat damage anywhere the correction is nil and
    // the rate is the card's.
    if half_hp_rate != 0.0 {
        let evolved = original_total + flat;
        base.base_damage_below_half_health += if evolved > 0.0 {
            half_hp_rate - half_hp_rate_own / evolved
        } else {
            half_hp_rate
        };
    }
    // NOT GATED ON THE DIRECT VECTOR. A form whose whole attack is its explosion
    // states `damage: {impact: 0}` — the heavy slam does — and an
    // `original_total > 0` here dropped the add before it could reach that
    // explosion, which is where all of its damage is.
    if flat > 0.0 {
        // THE FOLD ITSELF LIVES ON `WeaponBase`, because a flat base-damage add
        // reaches this weapon by two routes — a plain perk here, and a perk the
        // player's state gates, which cannot be resolved until `resolve_for` has
        // the Tenno. Two implementations of "what +40 base damage does" is two
        // chances to be right about the vector and wrong about the explosion.
        // See `WeaponBase::add_flat_base_damage` for what it does and why.
        //
        // TWO SUMS GO IN: what the panel gains, and how much of that the CO
        // term's base gains with it. They are equal when every perk feeds and
        // zero apart when none does — and they are neither when a build carries
        // two that disagree, which is the case a single ratio could not state.
        base.add_flat_base_damage(flat, flat_into_co);
    }
}
