use super::*;

impl ModEffect {
    /// One display line for this effect — OUR statement of what the model
    /// actually computes (true values; tooltip lies already corrected in
    /// the data). The single source for every effect list in the UI.
    pub fn describe(&self) -> String {
        use ModEffect::*;
        match *self {
            // The gate is stated as a suffix so the inner line reads normally.
            WhileTenno(c, ref inner) => format!("{} ({})", inner.describe(), c.describe()),
            // WHAT THE MODEL ACTUALLY COMPUTES, which is the armour half of the
            // card: this engine has no term for a shield POOL shrinking, so the
            // line says armour and the mod's own `unmodeled` says the rest.
            AbilityStat(stat, per, max) => match stat.unmodelled_reason() {
                None => format!(
                    "+{} {} for 20s per enemy an alt-fire strike hits, up to {max} — taken at                      the cap, which one orb reaches",
                    pct(per * f64::from(max)),
                    stat.label()
                ),
                Some(why) => format!("+{} {} — {why}", pct(per * f64::from(max)), stat.label()),
            },
            StripOnKillInRange(f, r) => format!(
                "each kill takes {} off the armour of every enemy within {r:.0} m of you, and                  keeps taking it — the shares compose, so two kills leave                  (1−x)² of it",
                pct(f)
            ),
            TennoScaled { stat, above, unit, per_unit, cap, grant } => format!(
                "{} {} per {:.0} of the Warframe's {}{} — capped at {}, and NOTHING with no                  frame, because the neutral player has none of it",
                pct(per_unit),
                grant.locked_stat(),
                unit,
                stat.name(),
                if above > 0.0 { format!(" past {above:.0}") } else { String::new() },
                pct(cap)
            ),
            LastRoundDamage(v) => format!(
                "{} damage on the magazine's LAST round — its own multiplier, not the                  base-damage bucket (nothing on a continuous weapon or an Incarnon form)",
                pct(v)
            ),
            FirstRoundDamage(v) => format!(
                "{} damage on the magazine's FIRST round — its own multiplier, not the base-damage bucket, and it reaches status damage",
                pct(v)
            ),
            AddedSpread(v) => format!(
                "+{v} degrees of spread — added after accuracy bonuses, which do not reach it"
            ),
            // A FLAT GRANT SAYS ITS UNIT: `+1 m` of reach, `+20` combo points.
            GrantsStackingBuff(b) => format!(
                "{} {} per stack, up to {} ({}), for {}s — earned {}",
                match b.grant {
                    BuffGrant::MeleeRange => format!("+{} m", b.per_stack),
                    BuffGrant::InitialCombo => format!("+{}", b.per_stack),
                    _ => pct(b.per_stack),
                },
                b.grant.label(),
                b.max_stacks,
                match b.grant {
                    BuffGrant::MeleeRange => format!("+{} m", b.per_stack * f64::from(b.max_stacks)),
                    BuffGrant::InitialCombo => format!("+{}", b.per_stack * f64::from(b.max_stacks)),
                    _ => pct(b.per_stack * f64::from(b.max_stacks)),
                },
                b.duration,
                b.trigger.label(),
            ),
            ConsecutiveHitDamage { per_stack, max_stacks, duration } => format!(
                "{} damage per consecutive hit, up to {max_stacks} ({}), for {duration}s — its own multiplier, not the base-damage bucket",
                pct(per_stack), pct(per_stack * f64::from(max_stacks))
            ),
            BaseDamage(v) => format!("{} Base Damage", pct(v)),
            Multishot(v) => format!("{} Multishot", pct(v)),
            CritChance(v) => format!("{} Crit Chance", pct(v)),
            // Both halves in one line, because they are one column on the card
            // and always equal; the refill is a separate sentence because it
            // answers a different question.
            PerTendril { crit_chance, .. } => {
                format!("{} Crit Chance and Status Chance per active tendril", pct(crit_chance))
            }
            // THE CEILING IS THE CARD'S OWN NUMBER, and the hit count under it
            // is the thing the card never states — so both are said, in that
            // order.
            // THE FOUR COMBO LINES SAY WHICH COUNTER, because a reader with a
            // sniper in their hands and a reader with a hammer are reading the
            // same word for two different systems.
            CritChancePerCombo(v) => format!(
                "+{} critical chance per melee combo tier above 1x — additive with Point Strike                  inside the same bracket, so it scales the weapon's own base",
                pct(v)
            ),
            StatusChancePerCombo(v) => format!(
                "+{} status chance per melee combo tier above 1x — additive with the status mods                  inside the same bracket",
                pct(v)
            ),
            MeleeComboDuration(v) => {
                format!("+{v:.0}s on the melee combo counter's clock")
            }
            InitialCombo(v) => format!(
                "the melee combo counter opens at {v:.0} and returns to it, regenerating 40 points                  a second — which is what a pure heavy build spends"
            ),
            MeleeComboDurationMultiplier(v) => format!(
                "{} on the melee combo counter's clock", pct(v)
            ),
            MeleeRange(v) => format!("+{v:.1} m of melee reach"),
            SlamDamage(v) => format!(
                "+{} slam damage — which pays on a slam and on nothing else, so it is worth                  zero in every mode that swings",
                pct(v)
            ),
            HeavyAttackDamage(v) => format!(
                "+{} damage on a heavy attack, and nothing on an ordinary swing", pct(v)
            ),
            ComboCountChance(v) => format!(
                "+{} chance of an extra melee combo point per landed hit", pct(v)
            ),
            ComboGainChance(v) => format!(
                "{:+.1}% chance to gain combo count — each base combo point a hit earns is kept {:.1}% of the time",
                v * 100.0,
                (1.0 + v).clamp(0.0, 1.0) * 100.0
            ),
            ComboCountChanceOnLifted(v) => format!(
                "+{} chance of an extra melee combo point per hit on a LIFTED target",
                pct(v)
            ),
            StatusChanceOnLifted(v) => format!("+{} status chance on a LIFTED target", pct(v)),
            Tennokai {
                enabled, chance, every_n_hits, window_seconds, damage, crit_damage,
                status_chance, chain_seconds, damage_needs_chain, curse_resets_combo,
                curse_heat_per_second, curse_seconds,
            } => {
                let mut parts: Vec<String> = Vec::new();
                if enabled {
                    parts.push("enables Tennokai — a 15% chance on a direct hit to open a 2s                                 window in which a heavy attack costs no combo".into());
                }
                if every_n_hits > 0 {
                    parts.push(format!("the window opens every {every_n_hits} hits instead of at random"));
                }
                if chance > 0.0 { parts.push(format!("+{} chance of it", pct(chance))); }
                if window_seconds > 0.0 { parts.push(format!("a {window_seconds}s window")); }
                if damage > 0.0 { parts.push(format!("+{} damage on a Tennokai attack", pct(damage))); }
                if crit_damage > 0.0 { parts.push(format!("+{} critical damage on one", pct(crit_damage))); }
                if status_chance > 0.0 { parts.push(format!("+{} status chance on one", pct(status_chance))); }
                // THE CARD THAT IS A GAMBLE, said as one. Every clause above is
                // a bonus; these three are the terms it comes on.
                if chain_seconds > 0.0 {
                    parts.push(format!(
                        "a Tennokai KILL re-opens the window for {chain_seconds}s{}",
                        if damage_needs_chain { ", and the damage bonus pays only in that one" } else { "" },
                    ));
                }
                if curse_resets_combo {
                    parts.push(format!(
                        "a Tennokai attack that FAILS to kill empties the combo counter{}",
                        if curse_heat_per_second > 0.0 {
                            format!(" and burns you for {curse_heat_per_second:.0} Heat a second for {curse_seconds:.0}s")
                        } else {
                            String::new()
                        },
                    ));
                }
                parts.join(", ")
            }
            CritChanceHeavyDoubled(v) => format!(
                "+{} critical chance, and +{} on a heavy attack — the card's own x2",
                pct(v), pct(v * 2.0)
            ),
            CritChanceOnSlide(v) => format!(
                "+{} critical chance on a SLIDE attack, and nothing on any other swing",
                pct(v)
            ),
            HeavyWindUpSpeed(v) => format!(
                "+{} heavy attack wind up speed — the charge before a heavy swing, which                  attack speed does not touch",
                pct(v)
            ),
            HeavyAttackEfficiency(v) => format!(
                "a heavy attack spends {} less of the combo counter (the game caps the total at 90%)",
                pct(v)
            ),
            CritChancePerHit(c) => format!(
                "On Hit: {} Crit Chance per stack, capped at {} ({} hits), cleared by a reload",
                pct(c.per_stack),
                pct(c.max_bonus),
                c.max_stacks()
            ),
            MagazineRefillOnKill(v) => format!("on kill, {} of the magazine back", pct(v)),
            SyndicateRadial { syndicate, amount } => {
                let d = crate::data::syndicates::get(syndicate);
                match d {
                    Some(d) => format!(
                        "+{amount} {} — {:.0} {:?} in {:.0} m once the weapon earns {:.0} affinity, every {:.0}s",
                        d.name, d.damage, d.element, d.radius_m, d.affinity_to_fill, d.cooldown_seconds
                    ),
                    None => format!("+{amount} {syndicate}"),
                }
            }
            CritDamage(v) => format!("{} Crit Damage", pct(v)),
            StatusChance(v) => format!("{} Status Chance", pct(v)),
            FireRate(v) => format!("{} Fire Rate", pct(v)),
            ChargeRate(v) => format!("{} Charge Rate", pct(v)),
            ReloadSpeed(v) => format!("{} Reload Speed", pct(v)),
            StatusDamage(v) => format!("{} Status Damage", pct(v)),
            AmmoEfficiency(v) => format!("{} Ammo Efficiency", pct(v)),
            SlashOnCrit(v) => format!("{} chance to apply Slash on Critical", pct(v)),
            Element(t, v) => format!("{} {t:?}", pct(v)),
            CombinedElement(t, v) => format!("{} {t:?}", pct(v)),
            Physical(t, v) => format!("{} {t:?}", pct(v)),
            CondBuff(b, v) => {
                format!("{} {} (conditional, assumed active)", pct(v), b.label())
            }
            OnKillMultishot { per_stack, max_stacks, duration } => {
                format!("On Kill: {} Multishot per stack ×{max_stacks}, {duration}s", pct(per_stack))
            }
            // TWO SENTENCES FOR TWO CARDS, because they are two mechanics that
            // share a payload: melee's Condition Overload is unconditional and
            // the Galvanized family earns the same term on a kill.
            ConditionOverload { per_stack, max_stacks, earned_on: None, .. } => format!(
                "{} Damage per status type on the target, on direct hits{}",
                pct(per_stack * f64::from(max_stacks)),
                ""
            ),
            ConditionOverload { per_stack, max_stacks, duration, .. } => {
                format!(
                    "On Kill: {} Damage per status type ×{max_stacks}, {duration}s (direct hits)",
                    pct(per_stack)
                )
            }
            OnHeadshotCritChance { bonus, duration } => {
                format!("On Headshot: {} Crit Chance, {duration}s", pct(bonus))
            }
            OnWeakpointElementAndStatus { element, bonus, duration } => format!(
                "On Weak Point: {} {} Damage and Status Chance, {duration}s",
                pct(bonus),
                element.name(),
            ),
            OnHeadshotKillCritChance { per_stack, max_stacks, duration } => {
                format!("On Headshot Kill: {} Crit Chance per stack ×{max_stacks}, {duration}s", pct(per_stack))
            }
            Indirect(stat, v) => format!("{} {}", stat.format(v), stat.label()),
            OnEquipHandling { recoil, accuracy, duration } => {
                format!("On Equip: {} Recoil, {} Accuracy, {duration}s", pct(recoil), pct(accuracy))
            }
            FactionDamage(fac, v) => format!("{} Damage to {fac:?}", pct(v)),
            GrantsLingering(field) => format!(
                "leaves a field: {} per tick for {}s",
                field.base_vector.total(),
                field.duration_seconds
            ),
            LingeringAreaFraction(v) => format!("over {} of the explosion area", pct(v)),
            AcidShells(part) => match part {
                AcidShellsPart::FlatDamage(v) => {
                    format!("on kill, the corpse explodes for {v} Corrosive")
                }
                AcidShellsPart::HealthFraction(v) => {
                    format!("…plus {} of its max health as Blast", pct(v))
                }
                AcidShellsPart::RadiusM(v) => format!("…within {v} m, falling to nothing at the rim"),
            },
            // Seconds, not a percentage — the card's own unit.
            ComboDuration(v) => format!("+{v}s Combo Duration"),
            MagazineCapacity(v) => format!("{} Magazine Capacity", pct(v)),
            BlastRadius(v) => format!("{} Blast Range (radius)", pct(v)),
            StatusDuration(v) => format!("{} Status Duration", pct(v)),
            WeakpointDamage(v) => format!("{} Weak Point Damage", pct(v)),
            WeakpointCritChance(v) => format!("{} Weak Point Crit Chance", pct(v)),
            OnKillCritDamage { bonus, duration } => {
                format!("On Kill: {} Crit Damage, {duration}s", pct(bonus))
            }
            OnReloadDamage { bonus, duration } => {
                format!("On reload from empty: {} Damage, {duration}s", pct(bonus))
            }
            OnEximusWeakpointDamage { bonus, duration } => {
                format!("On Eximus weak-point hit: {} Damage, {duration}s", pct(bonus))
            }
            OnReloadFireRate { bonus, duration } => {
                format!("On Reload: {} Fire Rate, {duration}s", pct(bonus))
            }
            ProcConversion { from, to, chance, low_rate_threshold, low_rate_multiplier } => {
                format!(
                    "{from:?} status: {} chance to also apply {to:?} (×{low_rate_multiplier} below {low_rate_threshold} fire rate)",
                    pct(chance)
                )
            }
        }
    }
}
