use super::*;

impl EvolutionDef {

    /// Σ unconditional CO rate per status type (Carnage Reign).
    pub fn co_per_type(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::ConditionOverload { per_type, .. } => Some(*per_type),
                _ => None,
            })
            .sum()
    }

    pub(super) fn active_effects(&self) -> impl Iterator<Item = &EvoEffect> {
        // Broken evolutions contribute nothing (same rule as `apply`).
        self.effects
            .iter()
            .filter(move |_| !self.currently_broken)
    }

    /// WHAT THIS PERK DOES NOT DO YET — the effects that loaded as `Inert`,
    /// named.
    ///
    /// DERIVED, never declared. An `unmodeled: true` field beside the effects
    /// would be a second copy of the truth, free to disagree with the loader
    /// the moment somebody implements one and forgets the flag. This asks the
    /// loaded effects, so a perk stops confessing the instant it is modelled
    /// and starts the instant a new unknown kind is written.
    ///
    /// Empty means every effect is modelled. It is the honest thing for the
    /// UI to show and the honest thing to grep for (如果有
    /// 的东西没做完，得说这个东西未完成 …… 不要隐瞒欺骗自己).
    pub fn unmodeled_effects(&self) -> Vec<&str> {
        self.effects
            .iter()
            .filter_map(|e| match e {
                EvoEffect::Inert(name) => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The other admission: clauses that CANNOT pay out here, each with the
    /// class that says why.
    ///
    /// Separate from `unmodeled_effects` because the two are different promises
    /// — one is work someone can do, the other is the edge of what a
    /// single-target damage simulator is — and because the ratchet must only
    /// count the first. See [`EvoEffect::OutOfScope`].
    pub fn out_of_scope_effects(&self) -> Vec<String> {
        self.effects
            .iter()
            .filter_map(|e| match e {
                EvoEffect::OutOfScope { clause, reason } => {
                    Some(format!("{clause} — {}", reason.why()))
                }
                _ => None,
            })
            .collect()
    }

    /// DOES THIS PERK'S FLAT BASE DAMAGE STAY OUT OF THE GunCO TERM, on an
    /// entry of this form and this CO class?
    ///
    /// **A DECLARATION WINS, AND IT IS SCOPED TO WHAT WAS MEASURED.**
    /// `only_form` exists because a perk reaches BOTH entries of its transform
    /// group while a reading comes off ONE: the Torid's Incarnon form was
    /// measured (M50) and its base form was not, and the two are not even the
    /// same CO class. A declaration that does not reach this form falls through.
    ///
    /// **AN UNDECLARED PERK IS ANSWERED BY THE ENTRY'S CO CLASS**: `Adding`
    /// EXCLUDES (15 measurements to 0, see [`EvolutionDef::excludes_co_base`])
    /// and `Multiplying` INCLUDES, measured on the Torid's base form (M51).
    ///
    /// So THE CLASS ANSWERS FIRST ON A `Multiplying` ENTRY, above the
    /// declaration: a reading off an `Adding` form must not reach across and
    /// dilute a `Multiplying` one. The generalisation runs ahead of the catalog
    /// deliberately, to be revisited PER WEAPON in that weapon's
    /// `co_base_fraction:`. `Inert` gets the Adding answer and never reads it.
    pub fn excludes_co_base(
        &self,
        form: crate::model::FormKind,
        behavior: crate::model::CoBehavior,
    ) -> bool {
        if behavior == crate::model::CoBehavior::Independent {
            return false;
        }
        match self.co_base_excludes_this_evolution {
            Some(v) if self.co_base_excludes_only_form.is_none_or(|f| f == form) => v,
            _ => true,
        }
    }

    /// WHAT THE GAME ITSELF DOES NOT DO — the clauses measured to pay nothing,
    /// each with the note that says how we know.
    ///
    /// NOT counted by [`Self::fully_unmodeled`], deliberately: that question is
    /// about OUR shortfalls, and a reader deciding whether to wait for us is
    /// asking something different from a reader deciding whether to pick a
    /// perk DE has broken. Both reach the page; only one is work.
    /// WHERE THIS PERK'S CARD IS WRONG — see [`Self::misprints`] the field.
    ///
    /// NOT counted by [`Self::fully_unmodeled`] and not reported beside the
    /// live bugs: a perk whose card misstates a threshold is fully modelled and
    /// fully working, and filing it with the broken ones would tell a reader to
    /// avoid the one thing they should be told to trust.
    pub fn misprints(&self) -> &[String] {
        &self.misprints
    }

    pub fn live_bugs(&self) -> Vec<String> {
        self.effects
            .iter()
            .filter_map(|e| match e {
                EvoEffect::LiveBug { clause, note } => Some(format!("{clause} — {note}")),
                _ => None,
            })
            .collect()
    }

    /// Does this perk do NOTHING the sim can see? A perk whose every effect is
    /// inert is not a weaker choice, it is not a choice — and the tile you pick
    /// from should say so rather than look like its working tier-mates.
    pub fn fully_unmodeled(&self) -> bool {
        // BOTH admissions count here. The tile asks "is this a choice at all",
        // and a perk whose every clause is an EDGE is no more of one than a
        // perk whose every clause is a todo — the difference is why, not
        // whether.
        !self.effects.is_empty()
            && self.unmodeled_effects().len() + self.out_of_scope_effects().len()
                == self.effects.len()
    }

    /// One display line per effect — what the model computes (broken
    /// evolutions state the zero honestly at the call site, not here).
    pub fn describe(&self) -> Vec<String> {
        self.effects
            .iter()
            .map(|e| match e {
                EvoEffect::FlatBaseDamage(v) => {
                    format!("+{v:.0} base damage (pro-rata, before mods)")
                }
                EvoEffect::BaseDamageBonus(v) => format!(
                    "+{:.0}% damage, in the same bucket Pressure Point is in",
                    v * 100.0
                ),
                EvoEffect::InitialCombo(v) => format!(
                    "the melee combo counter opens at +{v:.0} and returns to it, regenerating 40                      points a second — which is what a heavy build spends"
                ),
                EvoEffect::IncarnonWindow { arm_at_combo, seconds } => match (arm_at_combo, seconds) {
                    (Some(a), Some(s)) => format!(
                        "on for {s:.0}s from the heavy attack that arms it, which needs {a:.0}x combo"
                    ),
                    (Some(a), None) => format!("arms at {a:.0}x combo instead"),
                    (None, Some(s)) => format!("on for {s:.0}s from the heavy attack that arms it"),
                    (None, None) => String::new(),
                },
                EvoEffect::ComboCountOnSlamHit(v) => format!(
                    "+{v:.0} combo points for every body the slam reached, scaled by combo                      count chance — the only thing that earns combo on a heavy mode"
                ),
                EvoEffect::MeleeRange(v) => format!("+{v:.1} m of melee reach"),
                EvoEffect::FollowThroughBonus(v) => format!(
                    "+{:.0}% follow through — a bigger share for every body a swing reaches                      past the first",
                    v * 100.0
                ),
                EvoEffect::SlamRadiusBonus(v) => {
                    format!("+{:.0}% slam radius", v * 100.0)
                }
                EvoEffect::ProcConversion { from, to, chance } => format!(
                    "{:.0}% chance for a {} status to also apply a {} one, rolled once per                      damage instance",
                    chance * 100.0,
                    from.name(),
                    to.name()
                ),
                EvoEffect::HeavyWindUpSpeed(v) => format!(
                    "+{:.0}% heavy attack wind up speed — the charge before a heavy swing,                      which attack speed does not touch",
                    v * 100.0
                ),
                EvoEffect::FlatBaseCritChance(v) => {
                    format!("+{:.0}% BASE crit chance (crit mods multiply it)", v * 100.0)
                }
                EvoEffect::FlatBaseMultishot(v) => {
                    format!("+{v:.2} BASE multishot (multishot mods multiply it)")
                }
                EvoEffect::StackingFireRatePerShellReloaded { per_stack, max_stacks } => {
                    format!(
                        "+{:.0}% fire rate per shell reloaded, up to {max_stacks} stacks                          (+{:.0}% at the cap) — a full magazine is one reload's worth",
                        per_stack * 100.0,
                        per_stack * *max_stacks as f64 * 100.0
                    )
                }
                EvoEffect::FlatBaseStatusChanceByForm { base, incarnon } => format!(
                    "+{:.0}% BASE status chance ({:.0}% in Incarnon Form)",
                    base * 100.0,
                    incarnon * 100.0
                ),
                EvoEffect::FlatBaseCritMultiplier(v) => {
                    format!("+{v:.2}x BASE crit multiplier (crit damage mods multiply it)")
                }
                EvoEffect::Indirect(stat, v) => {
                    // Percent for the fractional stats, a bare number for the
                    // ones measured in their own unit (punch through: metres).
                    if matches!(stat, crate::model::IndirectStat::PunchThrough) {
                        format!("{:+.1} m {}", v, stat.label())
                    } else {
                        format!("{:+.0}% {}", v * 100.0, stat.label())
                    }
                }
                EvoEffect::AmmoMaxSet(v) => format!("ammo reserve set to {v:.0}"),
                EvoEffect::FlatBaseDamageOnEmptyReload(v) => format!(
                    "+{v:.0} base damage from the moment an empty reload starts — held all run"
                ),
                EvoEffect::FlatBaseStatusChance(v) => format!(
                    "+{:.0}% BASE status chance (status mods multiply it)",
                    v * 100.0
                ),
                EvoEffect::FlatBaseMagazine(v) => {
                    format!("+{v:.0} base magazine (magazine mods multiply it)")
                }
                EvoEffect::MultishotBeyondRange { value, metres } => format!(
                    "+{:.0}% multishot with no enemy inside {metres:.0} m — worth nothing at point blank, which is where both boards are scored",
                    value * 100.0
                ),
                EvoEffect::FieldDurationOnEmptyReload(v) => format!(
                    "On reload from empty: x{v:.0} lingering-field duration on the next shot"
                ),
                EvoEffect::ArmorStripPerPunctureStatus(v) => format!(
                    "removes {:.0}% of the target's armour per Puncture status",
                    v * 100.0
                ),
                EvoEffect::BaseDamagePerFullBurst { per_stack, max_stacks } => format!(
                    "+{:.0}% base damage per full burst, x{max_stacks} (+{:.0}% at the cap), \
                     reset when the magazine is refilled",
                    per_stack * 100.0,
                    per_stack * f64::from(*max_stacks) * 100.0
                ),
                EvoEffect::MultishotOnLastRound { value, base } => format!(
                    "+{value:.0} {}multishot on the last round of the magazine (base form only)",
                    if *base { "base " } else { "" }
                ),
                EvoEffect::MultishotConsumesAmmo(v) => format!(
                    "+{:.0}% damage on multishot-generated projectiles; multishot consumes ammo",
                    v * 100.0
                ),
                EvoEffect::AssumedMaxMultishot { total, max_stacks } => format!(
                    "+{:.0}% multishot ({max_stacks} on-ability-cast stacks, full by default)",
                    total * 100.0
                ),
                EvoEffect::ConditionOverload { per_type, min_sprint } => format!(
                    "+{:.0}% direct damage per status type on the target{}",
                    per_type * 100.0,
                    if *min_sprint > 0.0 {
                        format!(", at sprint speed {min_sprint} or higher")
                    } else {
                        String::new()
                    }
                ),
                EvoEffect::CritOnUndamaged { crit_chance, crit_multiplier } => format!(
                    "+{:.0}% BASE crit chance and +{crit_multiplier}x BASE crit damage while the                      target is undamaged (mods multiply both)",
                    crit_chance * 100.0
                ),
                EvoEffect::MagGrowthOnEmptyReload { per_stack, max_stacks } => format!(
                    "+{per_stack:.0} BASE magazine capacity on each reload from empty, up to {max_stacks} times (magazine mods multiply each stack)"
                ),
                EvoEffect::InstantReloadOnKill { chance } => format!(
                    "{:.0}% chance of an instant reload on any kill, including a status kill",
                    chance * 100.0
                ),
                EvoEffect::RoundRestoreOnStatusHit { status, chance, rounds } => format!(
                    "{:.0}% chance per shot to restore {rounds:.0} round from the ammo pool when the target carries a {status:?} status",
                    chance * 100.0
                ),
                EvoEffect::CritChanceByBodyPart { bodyshot_multiplier, weakpoint_bonus } => format!(
                    "x{bodyshot_multiplier:.0} crit chance on body shots, +{:.0}% BASE crit chance on weak points (additive with the crit mods)",
                    weakpoint_bonus * 100.0
                ),
                EvoEffect::DerivedStat { from_crit, rate, cap } => format!(
                    "+{:.0}% of current {} as base {}, up to +{:.0}%",
                    rate * 100.0,
                    if *from_crit { "crit chance" } else { "status chance" },
                    if *from_crit { "status chance" } else { "crit chance" },
                    cap * 100.0
                ),
                // ONE ARM PER BRACKET, and each says what the UNGATED
                // spelling of the same grant says: the brackets do not share
                // units, so a single line multiplies every one of them by 100
                // and prints the Rust identifier. Haven Foray's "+50 base
                // damage with overshields" then reads `+5000% FlatBaseDamage
                // with overshields`, and Paladin Virtue's +0.5x crit multiplier
                // reads `+50% BaseCritDamage` —
                // wrong on twenty cards, on the one line a player can check. The `>= 1.0` special case was the shape of the
                // bug: a unit chosen by the SIZE of the number rather than by
                // the bracket it lands in.
                EvoEffect::GatedByTenno { gate, grant, value } => {
                    use crate::model::GatedGrant as G;
                    let what = match grant {
                        G::FlatBaseDamage => {
                            format!("+{value:.0} base damage (pro-rata, before mods)")
                        }
                        G::FlatBaseMagazine => {
                            format!("+{value:.0} base magazine (magazine mods multiply it)")
                        }
                        G::BaseCritDamage => {
                            format!("+{value}x BASE crit damage (crit-damage mods multiply it)")
                        }
                        G::ConditionOverload => format!(
                            "+{:.0}% direct damage per status type on the target",
                            value * 100.0
                        ),
                        G::FireRate => format!("+{:.0}% fire rate", value * 100.0),
                        G::PunchThrough => format!("+{value}m punch through"),
                        G::Multishot => format!("+{:.0}% multishot", value * 100.0),
                        // ACCURACY narrows the cone a pellet draws inside, so
                        // the card is stated as what it does rather than as the
                        // stat's name: at point blank it is worth nothing and
                        // at a range it is the difference between landing and
                        // not (`space`, `build::loadout::Spread`).
                        G::Accuracy => format!(
                            "+{:.0}% accuracy — a tighter cone, so more pellets land at a distance",
                            value * 100.0
                        ),
                        G::ProjectileSpeed => {
                            format!("+{:.0}% projectile speed", value * 100.0)
                        }
                    };
                    format!("{what} {}", gate.describe())
                }
                EvoEffect::BaseDamageBelowHalfHealth { rate: v, .. } => format!(
                    "+{:.0}% damage while the target is under half health",
                    v * 100.0
                ),
                EvoEffect::FireRateBonus { value, min_sprint, needs_melee_equipped } => format!(
                    "+{:.0}% fire rate{}",
                    value * 100.0,
                    if *min_sprint > 0.0 {
                        format!(" at sprint speed {min_sprint} or higher")
                    } else if *needs_melee_equipped {
                        " with the melee weapon drawn".to_string()
                    } else {
                        String::new()
                    }
                ),
                EvoEffect::ReloadSpeedBonus(v) => format!("+{:.0}% reload speed", v * 100.0),
                EvoEffect::HeadshotDamageOnStreak { hits, within, value, duration } => format!(
                    "+{:.0}% headshot damage for {duration:.0}s after {hits} headshots in {within:.0}s",
                    value * 100.0
                ),
                EvoEffect::CritDamageBelowStatusCount { threshold, value } => format!(
                    "+{:.0}% critical damage while the target has fewer than {threshold} status types",
                    value * 100.0
                ),
                EvoEffect::InstantReloadOnHeadshot { chance, needs_kill } => format!(
                    "{:.0}% chance to fill the magazine on a headshot{}",
                    chance * 100.0,
                    if *needs_kill { " kill" } else { "" }
                ),
                EvoEffect::ReloadSpeedOnEmptyReload { value } => format!(
                    "+{:.0}% reload speed on a reload from empty, for that reload",
                    value * 100.0
                ),
                EvoEffect::StackingGrant { trigger, grant, per_stack, max_stacks, duration, chance, .. } => format!(
                    "{per_stack} {grant:?} per stack x{max_stacks} on {trigger:?}{}{}",
                    if duration.is_finite() { format!(" for {duration:.1}s") } else { String::new() },
                    if *chance < 1.0 { format!(", {:.0}% of the time", chance * 100.0) } else { String::new() }
                ),
                EvoEffect::StackingMultishotOnFiring { per_stack, max_stacks, base } => format!(
                    "+{per_stack} {} multishot per stack x{max_stacks} on firing, until a reload",
                    if *base { "base" } else { "bucket" }
                ),
                EvoEffect::StackingMultishotOnStatus { status, per_stack, max_stacks, duration } => format!(
                    "+{per_stack} multishot per stack x{max_stacks} for {duration:.0}s while the                      target carries {status:?} (flat, like Final Fusillade's)"
                ),
                EvoEffect::StackingFireRateOnHeadshot { per_stack, max_stacks, duration, chance, .. } => format!(
                    "+{:.0}% fire rate per stack x{max_stacks} for {duration:.0}s on headshot, \
                     {:.0}% chance each (additive with fire-rate mods)",
                    per_stack * 100.0,
                    chance * 100.0
                ),
                EvoEffect::CritMultiplierBelowCritChance { value, below } => format!(
                    "+{value:.1}x BASE crit multiplier while crit chance stays under {:.0}% \
                     (crit damage mods multiply it; the condition is checked per shot \
                     against the LIVE crit chance, so Puncture's Weakened can push a \
                     build over the line and take it away)",
                    below * 100.0
                ),
                EvoEffect::PostModCritChance(v) => format!(
                    "{}{:.0}% crit chance, flat AFTER mods",
                    if *v >= 0.0 { "+" } else { "" },
                    v * 100.0
                ),
                EvoEffect::PostModStatusChance(v) => format!(
                    "{}{:.0}% status chance, flat AFTER mods",
                    if *v >= 0.0 { "+" } else { "" },
                    v * 100.0
                ),
                EvoEffect::HeadshotDamage(v) => {
                    format!("+{:.0}% headshot damage (direct hits only)", v * 100.0)
                }
                EvoEffect::StackingReloadSpeedOnHeadshot {
                    per_stack,
                    max_stacks,
                    duration,
                } => format!(
                    "+{:.0}% reload speed per headshot ({max_stacks} stacks, {duration:.0}s) — shortens the transmutes too",
                    per_stack * 100.0
                ),
                EvoEffect::ChanceDamageOnNoncrit { chance, value } => format!(
                    "{:.0}% chance of +{:.0}% damage on a NON-crit instance (own multiplier, radial included)",
                    chance * 100.0,
                    value * 100.0
                ),
                EvoEffect::IncarnonChargeRate(v) => format!(
                    "weakpoint hits build +{:.0}% Incarnon charge",
                    v * 100.0
                ),
                EvoEffect::StackingDamageOnPlainHit {
                    per_stack,
                    max_stacks,
                    duration,
                } => format!(
                    "+{:.0}% damage per stack ({max_stacks} max, {duration:.0} s) on a hit that neither crits nor procs",
                    per_stack * 100.0
                ),
                EvoEffect::UnlocksForm(w) => {
                    format!("unlocks the {w} form — its stats are that form's own")
                }
                EvoEffect::Inert(what) => {
                    format!("{} (no single-target DPS effect)", what.replace('_', " "))
                }
                EvoEffect::OutOfScope { clause, reason } => {
                    format!("{clause} — {}", reason.why())
                }
                // NAMED AS DEAD ON THE CARD ITSELF. A reader comparing two
                // tier-2 options must see which half of this one pays nothing
                // — printing it like a working clause is the one thing this
                // line must never do.
                EvoEffect::LiveBug { clause, note } => {
                    format!("{clause} — DOES NOT WORK IN GAME: {note}")
                }
                // Said as what it is — a cap on the line above, not a line of
                // its own claiming the perk does less than it does.
                EvoEffect::Qualifier(what) => {
                    format!("{} (a cap on the bonus above)", what
                        .trim_start_matches("unmodelled_").replace('_', " "))
                }
            })
            .collect()
    }
}
