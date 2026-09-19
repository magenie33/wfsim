use super::*;

/// A parsed arcane definition (rank-parameterized).
#[derive(Debug, Clone)]
pub struct ArcaneDef {
    pub id: String,
    pub name: String,
    pub rarity: Rarity,
    pub max_rank: u32,
    pub requires: Option<String>,
    /// Weapon CLASSES that may equip this arcane at all. Empty = any weapon
    /// whose slot seats the arcane, which is almost all of them.
    ///
    /// An EQUIP rule, not a calc-layer gate. `requires` is
    /// the other thing — it lets the arcane equip and go inert, which is right
    /// for Akimbo Slip Shot and WRONG for these two: the game does not offer
    /// them at all. A picker that offers what the arsenal refuses is a worse
    /// way to say the same thing.
    pub equip_classes: Vec<&'static str>,
    /// …AND THE WEAPON TRAITS, for a family that is not a class.
    ///
    /// A CLASS could not say "Kitgun": a primary Tombfinger is a `rifle` and a
    /// secondary one a `pistol`, exactly like a Braton and a Lex, and the eight
    /// Pax and Residual arcanes go on neither. What separates them is that the
    /// weapon is MODULAR, which is a `traits:` entry on the roster and the same
    /// thing `incarnon` already is.
    ///
    /// An EQUIP rule like `equip_classes` beside it, and for its reason: the
    /// arsenal does not offer these on anything else, so a picker that did
    /// would be a worse way of saying the same thing. Both are ANDed — an
    /// arcane may narrow by either, or by both.
    pub equip_traits: Vec<&'static str>,
    /// WHICH ARCANE SEATS this arcane fits — `primary`, `secondary`, `melee`.
    ///
    /// Almost always the one directory it is filed under, which is the default
    /// and needs no yaml. The KITGUN family is why it can be a list: a Kitgun is
    /// one weapon with an entry in both slots, so a primary Tombfinger has a
    /// PRIMARY arcane seat and a secondary one a SECONDARY seat, and Pax Charge
    /// goes in either. Ids are globally unique across slots — a test asserts it,
    /// and `secondary(id)` is safe because of it — so filing the same arcane in
    /// two directories was never an option.
    pub seats: Vec<&'static str>,
    /// Verbatim in-game text with rank-varying numbers as `X`.
    pub description: String,
    /// WHAT THIS ARCANE DOES IN THE LIVE GAME THAT DE DID NOT MEAN IT TO.
    ///
    /// A FOURTH admission, and the only one that is not a shortfall: the sim
    /// computes this, it matches the game, and it is a bug — so a hotfix
    /// changes the answer and the number a player is reading today rests on
    /// something that can be taken away.
    ///
    /// Text rather than a flag, and rendered rather than a comment, for the
    /// same reason a weapon's `unmodeled:` lines are: this is the sentence the
    /// PLAYER needs, and a maintainer-only note would leave the page silent.
    /// The mechanism itself stays in comments and in MEASUREMENTS.
    pub live_bugs: Vec<String>,
    pub(super) perk: Option<String>,
    pub(super) effects: Vec<ArcEffect>,
}

impl ArcaneDef {
    /// Does this arcane have an effect the sim knowingly does NOT model?
    ///
    /// `describe_at` already emitted "not modeled", but the card prefers DE's
    /// own text when there is one — and there always is — so the admission sat
    /// in the payload and never reached the screen. This gives the page a fact
    /// it can show ALONGSIDE the official text rather than instead of it
    /// (reported 2026-08-05: Primary Debilitate "doesn't work", and it does not).
    pub fn has_unmodeled(&self) -> bool {
        self.effects
            .iter()
            .any(|e| matches!(e, ArcEffect::Unmodeled { .. }))
    }

    /// The effects on this arcane that load but do NOTHING — what the card
    /// must admit it does not do.
    ///
    /// `ArcEffect::Inert` is where an effect goes when the loader has no arm
    /// for its kind, or has one and cannot use the shape it was given. That
    /// was invisible: `describe_at` prints nothing for it, `has_unmodeled`
    /// does not count it, and so Primary Deadhead's recoil reduction, Primary
    /// Dexterity's combo duration and Secondary Fortifier's overguard were
    /// silently doing zero on a card that promised them.
    ///
    /// DERIVED, never listed — the same rule the mod side follows
    /// (`data::mods::unmodeled_effects`): it reads the effects the loader
    /// actually built, so an arcane that starts dropping one discloses it
    /// without anyone remembering to come back here.
    pub fn unmodeled_effects(&self) -> Vec<String> {
        self.effects
            .iter()
            .filter_map(|e| match e {
                // `combo_duration_bonus` -> "combo duration bonus": the kind
                // IS the description, in the vocabulary the yaml chose.
                ArcEffect::Inert(why) => Some(why.replace('_', " ")),
                _ => None,
            })
            .collect()
    }

    /// Does it act on something this simulator does not have at all?
    pub fn has_out_of_scope(&self) -> bool {
        self.effects
            .iter()
            .any(|e| matches!(e, ArcEffect::OutOfScope { .. }))
    }

    /// Resolve this arcane at `rank` into the sim parameter block.
    ///
    /// Relative crit bonuses stay RELATIVE here (they join the mod buckets):
    /// resolving them against the weapon's base at this layer means resolving
    /// them against the DIRECT part's base, which silently excludes the
    /// explosion — every attack part has its own base crit stats, so only the
    /// sim can multiply them out. `traits` gates `requires` (calc-layer inert,
    /// like mods).
    pub fn fx(
        &self,
        rank: u32,
        policy: StackPolicy,
        traits: &[&str],
        tenno: &crate::data::tenno::Tenno,
    ) -> ArcaneFx {
        let rank = rank.min(self.max_rank);
        let mut fx = ArcaneFx {
            id: self.id.clone(),
            ..ArcaneFx::none()
        };
        if let Some(req) = &self.requires {
            if !traits.contains(&req.as_str()) {
                return fx; // required trait absent: effects inert, id kept
            }
        }
        if self.perk.as_deref() == Some("secondary_enervate") {
            fx.enervate_rank = Some(rank as u8);
        }
        let assumed = policy == StackPolicy::AssumedMax;
        for e in &self.effects {
            match e {
                ArcEffect::Buff { trigger, grant, scale, max_stacks, duration, all_drop, one_per_instance } => {
                    if policy == StackPolicy::BaseOnly {
                        continue; // sentinel: conditional never fires
                    }
                    // Every grant is stored as the raw per-rank value: a plain
                    // ratio, or a RELATIVE bonus the sim multiplies by the
                    // attack part's own base (CritDamage, StatusChance).
                    let per_stack = scale.at(rank, self.max_rank);
                    fx.buffs.push(ArcBuffSpec {
                        owner: self.id.clone(),
                        grant: *grant,
                        trigger: *trigger,
                        per_stack,
                        max_stacks: *max_stacks,
                        // AssumedMax = 100% uptime, which is a buff with no
                        // clock. Said as a duration rather than as a flag,
                        // like every other never-expires in the engine.
                        duration: if assumed {
                            crate::model::NO_TIMEOUT
                        } else {
                            *duration
                        },
                        all_drop: *all_drop,
                        one_per_instance: *one_per_instance,
                        // EARNED from zero: an arcane's stacks come from kills
                        // and procs, and a fight that cannot produce them must
                        // not be credited with them (docs/BUFFS.md).
                        initial_stacks: 0,
                    });
                }
                ArcEffect::TennoScaled { stat, above, per_unit, min_energy_pct, grant, cap } => {
                    if policy == StackPolicy::BaseOnly {
                        continue; // sentinel: no Tenno stands behind it
                    }
                    // The gate first, then the value, then the rank's cap.
                    let bonus = if tenno.state.energy_pct + 1e-12 < *min_energy_pct {
                        0.0
                    } else {
                        (per_unit * (stat.of(tenno) - above).max(0.0))
                            .min(cap.at(rank, self.max_rank))
                    };
                    if bonus <= 0.0 {
                        continue; // no frame, or below the threshold: nothing to list
                    }
                    fx.buffs.push(ArcBuffSpec {
                        owner: self.id.clone(),
                        grant: *grant,
                        trigger: ArcTrigger::Passive,
                        per_stack: bonus,
                        max_stacks: 1,
                        // A Warframe stat does not decay mid-fight. It was a
                        // `pinned` flag beside a 0 s duration, which is a
                        // decay loop that would spin if anything ever read it.
                        duration: crate::model::NO_TIMEOUT,
                        all_drop: false,
                        // A passive has no instance to be one-per.
                        one_per_instance: false,
                        initial_stacks: 1,
                    });
                }
                ArcEffect::CondCritChance(sc) => {
                    if assumed {
                        fx.crit_chance_relative += sc.at(rank, self.max_rank);
                    }
                }
                ArcEffect::CondCritChanceStacked { scale, max_stacks } => {
                    if assumed {
                        fx.crit_chance_relative += scale.at(rank, self.max_rank) * *max_stacks as f64;
                    }
                }
                ArcEffect::CondCritDamageStacked { scale, max_stacks } => {
                    if assumed {
                        fx.crit_damage_relative += scale.at(rank, self.max_rank) * *max_stacks as f64;
                    }
                }
                ArcEffect::WeakpointCritChance(sc) => {
                    if assumed {
                        fx.weakpoint_crit_chance_relative += sc.at(rank, self.max_rank);
                    }
                }
                ArcEffect::AddedElement { element, scale, max_stacks } => {
                    // NOT GATED ON `assumed`, and that is the owner's call
                    // rather than a reading of the card: the trigger is a
                    // Warframe cast, which this arena has no way to perform, so
                    // the alternative to holding every stack is paying nothing
                    // at all. A melee player casts, so this is held for the
                    // whole engagement and the card's timer never runs out.
                    //
                    // The pile expires WHOLE in game, so holding the cap costs
                    // four casts a window rather than one — which makes this
                    // generous rather than neutral, and the data file says so.
                    let v = scale.at(rank, self.max_rank) * f64::from(*max_stacks);
                    match fx.added_elements.iter_mut().find(|(t, _)| t == element) {
                        Some(slot) => slot.1 += v,
                        None => fx.added_elements.push((*element, v)),
                    }
                }
                ArcEffect::Debilitate(sc) => {
                    // NOT gated on `assumed`: the roll is per damage instance
                    // and the sim rolls it, so this is emergent either way.
                    fx.debilitate_chance = sc.at(rank, self.max_rank);
                }
                ArcEffect::FinalDamageCap(sc) => {
                    if assumed {
                        fx.final_multiplier = 1.0 + sc.at(rank, self.max_rank);
                    }
                }
                ArcEffect::CondReloadSpeed(sc) => {
                    if assumed {
                        fx.reload_bonus += sc.at(rank, self.max_rank);
                    }
                }
                ArcEffect::HeadshotMultiplier { value, unlocks_at } => {
                    if rank >= *unlocks_at {
                        fx.headshot_multiplier_bonus += value;
                    }
                }
                ArcEffect::RechargeableMagazine { scale } => {
                    // THE DELAY IS THE RELOAD, shortened by reload speed like
                    // any other: "recharge delay is further reduced (stacking
                    // additively) by reload speed mods". So the bonus joins the
                    // bucket the mods already feed, and the panel's own
                    // `reload_seconds` IS the delay with nothing to recompute.
                    fx.reload_bonus += scale.at(rank, self.max_rank);
                    fx.rechargeable_magazine = true;
                }
                ArcEffect::ReloadSpeed { value, unlocks_at } => {
                    if rank >= *unlocks_at {
                        fx.reload_bonus += value;
                    }
                }
                ArcEffect::PerColdDamage { scale, max_stacks } => {
                    fx.per_cold_base_damage = scale.at(rank, self.max_rank);
                    fx.cold_cap = *max_stacks;
                }
                ArcEffect::FlatDamageOnStatus(sc) => {
                    fx.flat_damage_on_status = sc.at(rank, self.max_rank);
                }
                ArcEffect::EncumberChance(sc) => {
                    fx.encumber_chance = sc.at(rank, self.max_rank);
                }
                ArcEffect::ColdBurst { scale, .. } => {
                    fx.cold_bursts_on_puncture = scale.at(rank, self.max_rank).round() as u32;
                }
                // Team-context / out-of-scope — no sim payload.
                ArcEffect::PerAllyCritChance(_)
                | ArcEffect::Unmodeled { .. }
                | ArcEffect::OutOfScope { .. } => {}
                // AN ECHO NEEDS SOMEBODY TO ECHO TO, and until 2026-08-17 the
                // arena had one body — so this sat in the group above with the
                // team buffs, correctly worth nothing. A formation is what
                // turns it on: *"deal X% of the hit damage to enemies within
                // Xm"*, and there are enemies now.
                ArcEffect::AoeEcho { scale, radius0, radius1, needs_radiation } => {
                    fx.echo_share = scale.at(rank, self.max_rank);
                    fx.echo_needs_radiation_stacks = *needs_radiation;
                    fx.echo_radius_m = radius0
                        + (radius1 - radius0)
                            * if self.max_rank == 0 {
                                1.0
                            } else {
                                rank as f64 / self.max_rank as f64
                            };
                }
                // …AND SO DOES A SPREAD. Melee Influence is worth exactly
                // nothing against one body — the statuses it copies have
                // nowhere to go — so the ruler that can see it is the group
                // one, the same as the echo above.
                ArcEffect::StatusSpread { scale, radius0, radius1, seconds0, seconds1 } => {
                    let lerp = |a: f64, b: f64| {
                        a + (b - a)
                            * if self.max_rank == 0 {
                                1.0
                            } else {
                                rank as f64 / self.max_rank as f64
                            }
                    };
                    fx.influence_chance = scale.at(rank, self.max_rank);
                    fx.influence_radius_m = lerp(*radius0, *radius1);
                    fx.influence_seconds = lerp(*seconds0, *seconds1);
                }
                ArcEffect::OverguardDamage(sc) => {
                    fx.overguard_multiplier = 1.0 + sc.at(rank, self.max_rank);
                }
                ArcEffect::AmmoEfficiency(sc) => {
                    if assumed {
                        fx.ammo_efficiency += sc.at(rank, self.max_rank);
                    }
                }
                // PER METRE, under BOTH policies. The arcane's own condition is
                // "on aim", which is a Tenno state the resolver already asks
                // (`TennoCondition::Aiming`) rather than a fight event the sim
                // would have to watch — so there is nothing here for Emergent
                // to hold back, unlike a stacking buff that has to be earned.
                ArcEffect::CompressionDamage(sc) => {
                    fx.compression_damage_per_m += sc.at(rank, self.max_rank);
                }
                ArcEffect::CompressionAmmoEfficiency(sc) => {
                    fx.compression_effectiveness_per_m += sc.at(rank, self.max_rank);
                }
                ArcEffect::Inert(_) | ArcEffect::Elsewhere(_) => {}
            }
        }
        fx
    }

    /// The verbatim in-game DESCRIPTION with its `X` placeholders filled at
    /// `rank` — what the config page shows (docs: description-X schema).
    ///
    /// `X`s map to the effects' display values in yaml order. Two derived
    /// cases close the gaps: adjacent effects sharing one number collapse
    /// (Outburst's cc+cd "by X% per Combo"), and a trailing "Stacks up to
    /// X%" cap is per_stack × max_stacks (Cascadia Flare).
    pub fn desc_at(&self, rank: u32) -> String {
        let rank = rank.min(self.max_rank);
        let at = |sc: &Scale| sc.at(rank, self.max_rank);
        let lerp = |a: f64, b: f64| a + (b - a) * rank as f64 / self.max_rank.max(1) as f64;
        let mut vals: Vec<f64> = Vec::new();
        if self.perk.as_deref() == Some("secondary_enervate") {
            vals.push((rank + 1) as f64); // "Resets after X Big Critical Hit"
        }
        for e in &self.effects {
            match e {
                ArcEffect::Buff { scale, .. }
                | ArcEffect::TennoScaled { cap: scale, .. }
                | ArcEffect::CondCritChance(scale)
                | ArcEffect::CondCritChanceStacked { scale, .. }
                | ArcEffect::CondCritDamageStacked { scale, .. }
                | ArcEffect::WeakpointCritChance(scale)
                | ArcEffect::PerColdDamage { scale, .. }
                | ArcEffect::AddedElement { scale, .. }
                | ArcEffect::RechargeableMagazine { scale }
                | ArcEffect::FlatDamageOnStatus(scale)
                | ArcEffect::EncumberChance(scale)
                | ArcEffect::AmmoEfficiency(scale)
                | ArcEffect::CompressionDamage(scale)
                | ArcEffect::CompressionAmmoEfficiency(scale)
                | ArcEffect::PerAllyCritChance(scale)
                | ArcEffect::CondReloadSpeed(scale)
                // Out of the sim's world, but it still owns its `X`.
                | ArcEffect::Unmodeled { scale, .. }
                | ArcEffect::OutOfScope { scale, .. }
                // Multiplier kinds ("xX"): stored as the bonus — fill_x's
                // xX rule renders the +1.
                | ArcEffect::Debilitate(scale)
                | ArcEffect::FinalDamageCap(scale) => vals.push(at(scale)),
                // …AND THE ONE WHOSE CARD PRINTS THE EXTRA. `fill_x`'s "xX"
                // rule exists because DE usually prints the TOTAL over a stored
                // bonus; Secondary Fortifier's card says "Deals x8 Extra Damage
                // to Overguard", where 8 IS the bonus and 9 is the total
                // (MEASUREMENTS M38). So the stored value is already what the
                // card shows, and the +1 has to be undone rather than the data
                // bent to fit a formatting rule.
                ArcEffect::OverguardDamage(scale) => vals.push(at(scale) - 1.0),
                ArcEffect::ColdBurst { scale, radius0, radius1 }
                | ArcEffect::AoeEcho { scale, radius0, radius1, .. } => {
                    vals.push(at(scale));
                    vals.push(lerp(*radius0, *radius1));
                }
                // NO `X` TO FILL: this card's text carries its numbers
                // literally, the way every melee arcane's does.
                ArcEffect::StatusSpread { .. }
                | ArcEffect::HeadshotMultiplier { .. }
                | ArcEffect::ReloadSpeed { .. }
                | ArcEffect::Inert(_) | ArcEffect::Elsewhere(_) => {}
            }
        }
        let xs = count_x(&self.description);
        // Outburst: cc + cd share the single "by X% per Combo" number.
        while vals.len() > xs {
            let before = vals.len();
            vals.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
            if vals.len() == before {
                break;
            }
        }
        // Cascadia Flare: the trailing stack cap is per_stack × max_stacks.
        if vals.len() + 1 == xs && self.description.contains("Stacks up to X%") {
            if let Some(ArcEffect::Buff { scale, max_stacks, .. }) = self
                .effects
                .iter()
                .find(|e| matches!(e, ArcEffect::Buff { .. }))
            {
                vals.push(at(scale) * *max_stacks as f64);
            }
        }
        fill_x(&self.description, &vals)
    }

    /// Display lines at a rank — OUR statement of what the model computes
    /// (mirrors [`crate::model::ModEffect::describe`]).
    pub fn describe_at(&self, rank: u32) -> Vec<String> {
        let rank = rank.min(self.max_rank);
        let mut out = Vec::new();
        if self.perk.as_deref() == Some("secondary_enervate") {
            out.push("On Hit: +10% flat Crit Chance per stack".to_string());
            out.push(format!(
                "Resets after {} big crit{}",
                rank + 1,
                if rank == 0 { "" } else { "s" }
            ));
        }
        for e in &self.effects {
            let at = |sc: &Scale| sc.at(rank, self.max_rank);
            match e {
                ArcEffect::Buff { trigger, grant, scale, max_stacks, duration, all_drop, one_per_instance } => {
                    let what = match grant {
                        ArcGrant::BaseDamage => "Base Damage",
                        ArcGrant::Multishot => "Multishot",
                        ArcGrant::ReloadSpeed => "Reload Speed",
                        ArcGrant::CritDamage => "Critical Damage",
                        ArcGrant::StatusChance => "Status Chance",
                        ArcGrant::AmmoEfficiency => "Ammo Efficiency",
                    };
                    let when = match trigger {
                        ArcTrigger::Kill => "On Kill",
                        ArcTrigger::HeadshotKill => "On Precision Headshot Kill",
                        ArcTrigger::MeleeKill => "On Melee Kill",
                        ArcTrigger::HeatStatus => "On Heat Status",
                        ArcTrigger::ElectricityStatus => "On Electricity Status",
                        ArcTrigger::ToxinStatus => "On Toxin Status",
                        ArcTrigger::ColdStatus => "On Cold Status",
                        ArcTrigger::WeakpointHit => "On Weak Point Hit",
                        ArcTrigger::Passive => "Always",
                    };
                    let decay = if *all_drop { "all drop on timeout" } else { "lose one on timeout" };
                    // The per-instance cap belongs on the CARD: it is the
                    // difference between a shotgun gaining one stack a shot and
                    // gaining twelve, and nothing else on screen says which.
                    let rate = if *one_per_instance { ", one per shot" } else { "" };
                    out.push(format!(
                        "{when}: {} {what} per stack ×{max_stacks}{rate}, {duration}s ({decay})",
                        pct(at(scale))
                    ));
                }
                ArcEffect::TennoScaled { stat, above, per_unit, min_energy_pct, grant, cap } => {
                    let what = match grant {
                        ArcGrant::Multishot => "Multishot",
                        _ => "Damage",
                    };
                    let of = match stat {
                        TennoStat::Armor => "Warframe Armor",
                        TennoStat::MaxEnergy => "Warframe Max Energy",
                        TennoStat::Health => "Warframe Health",
                        TennoStat::Shields => "Warframe Shields",
                    };
                    let past = if *above > 0.0 { format!(" past {above}") } else { String::new() };
                    let gate = if *min_energy_pct > 0.0 {
                        format!(" while at or above {}% Energy", (min_energy_pct * 100.0).round())
                    } else {
                        String::new()
                    };
                    out.push(format!(
                        "{} {what} per point of {of}{past}, up to {}{gate}",
                        pct(*per_unit),
                        pct(at(cap))
                    ));
                }
                ArcEffect::CondCritChance(sc) => {
                    out.push(format!("{} Crit Chance (conditional)", pct(at(sc))));
                }
                ArcEffect::CondCritChanceStacked { scale, max_stacks } => out.push(format!(
                    "{} Crit Chance per combo consumed ×{max_stacks} (on swap)",
                    pct(at(scale))
                )),
                ArcEffect::CondCritDamageStacked { scale, max_stacks } => out.push(format!(
                    "{} Crit Damage per combo consumed ×{max_stacks} (on swap)",
                    pct(at(scale))
                )),
                ArcEffect::WeakpointCritChance(sc) => out.push(format!(
                    "On Roll: {} Crit Chance on weak-point hits",
                    pct(at(sc))
                )),
                ArcEffect::FinalDamageCap(sc) => out.push(format!(
                    "On Ability Cast: next shot ×{:.0} damage cap (0.5%/energy)",
                    1.0 + at(sc)
                )),
                ArcEffect::CondReloadSpeed(sc) => out.push(format!(
                    "On Ability Cast: {} Reload Speed (conditional)",
                    pct(at(sc))
                )),
                ArcEffect::HeadshotMultiplier { value, unlocks_at } => {
                    if rank >= *unlocks_at {
                        out.push(format!("{} to Headshot Multiplier", pct(*value)));
                    }
                }
                ArcEffect::RechargeableMagazine { scale } => out.push(format!(
                    "the magazine RECHARGES instead of reloading, {} Recharge Delay",
                    pct(at(scale))
                )),
                ArcEffect::ReloadSpeed { value, unlocks_at } => {
                    if rank >= *unlocks_at {
                        out.push(format!("{} Reload Speed", pct(*value)));
                    }
                }
                ArcEffect::PerColdDamage { scale, max_stacks } => out.push(format!(
                    "{} Damage per Cold status on the target (×{max_stacks})",
                    pct(at(scale))
                )),
                ArcEffect::AddedElement { element, scale, max_stacks } => out.push(format!(
                    "{} {element:?} per Ability Cast (×{max_stacks}), held all fight",
                    pct(at(scale))
                )),
                ArcEffect::FlatDamageOnStatus(sc) => out.push(format!(
                    "On Status: +{:.0} flat damage of the proc's type",
                    at(sc)
                )),
                ArcEffect::EncumberChance(sc) => out.push(format!(
                    "On Status: {} chance of one extra random status",
                    pct(at(sc))
                )),
                ArcEffect::ColdBurst { scale, .. } => out.push(format!(
                    "On Puncture: apply {:.0} Cold stacks",
                    at(scale)
                )),
                ArcEffect::PerAllyCritChance(sc) => out.push(format!(
                    "{} Crit Chance per ally-affecting buff (team context)",
                    pct(at(sc))
                )),
                // …AND THE CARD STATES THE GATE. Not a footnote: the echo is
                // worth exactly nothing until the target is at 10 stacks, so a
                // line printing only the payout would describe the good half of
                // a conditional as if it were the whole thing.
                ArcEffect::AoeEcho { scale, needs_radiation, .. } => out.push(if *needs_radiation > 0 {
                    format!(
                        "on a target at {needs_radiation} Radiation stacks: {} of the hit damage echoed to nearby enemies (AoE)",
                        pct(at(scale))
                    )
                } else {
                    format!("{} of the hit damage echoed to nearby enemies (AoE)", pct(at(scale)))
                }),
                // THE THREE NUMBERS THE CARD IS, in the order they happen:
                // the roll, the clock it opens and the radius that clock makes
                // matter. A line printing only the chance would read as a
                // damage bonus, which is the one thing this card is not.
                ArcEffect::StatusSpread { scale, radius0, radius1, seconds0, seconds1 } => {
                    let ramp = |a: f64, b: f64| {
                        a + (b - a) * f64::from(rank) / f64::from(self.max_rank.max(1))
                    };
                    out.push(format!(
                        "on a melee Electricity status: {} chance that elemental statuses also land on everything within {:.0} m, for {:.0} s",
                        pct(at(scale)),
                        ramp(*radius0, *radius1),
                        ramp(*seconds0, *seconds1),
                    ));
                }
                // DE PRINTS THE EXTRA, and so does this: "x8 Extra Damage to
                // Overguard" is the card, ×9 is what it does (M38). Printing
                // the total here would put a number on the panel that appears
                // nowhere in game.
                ArcEffect::OverguardDamage(sc) => out.push(format!(
                    "×{:.0} extra damage to Overguard (×{:.0} in total)",
                    at(sc),
                    1.0 + at(sc)
                )),
                ArcEffect::AmmoEfficiency(sc) => out.push(format!(
                    "{} ammo efficiency while sliding/aim gliding (Dual Pistols)",
                    pct(at(sc))
                )),
                // Per METRE OF RADIUS LOST, and the card says so — the number a
                // player can check against the panel is the weapon's, and the
                // panel is where it appears.
                ArcEffect::CompressionDamage(sc) => out.push(format!(
                    "while aiming: ×0.2 explosion radius, {} damage per metre of radius lost",
                    pct(at(sc))
                )),
                ArcEffect::CompressionAmmoEfficiency(sc) => out.push(format!(
                    "while aiming: {} ammo efficiency per metre of radius lost",
                    pct(at(sc))
                )),
                // Say so, rather than silently listing nothing: the panel's
                // job is to state what the model does, and "this one is out of
                // scope" is part of that.
                ArcEffect::Debilitate(sc) => out.push(format!(
                    "{} chance to split a 10-stack combined status into one of its parts",
                    pct(at(sc))
                )),
                ArcEffect::Unmodeled { .. } => out.push("not modeled".to_string()),
                ArcEffect::OutOfScope { .. } => {
                    out.push("outside this simulator".to_string())
                }
                ArcEffect::Inert(_) | ArcEffect::Elsewhere(_) => {}
            }
        }
        out
    }
}
