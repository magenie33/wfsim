use super::*;

/// HOW MUCH BIGGER A SLAM IS FOR HAVING BEEN DROPPED FROM HEIGHT.
///
/// *"Slam radius is determined from the height at which the slam attack is
/// used, with the radius scaling up to 150% of the listed value, achieved by
/// slamming from a height of at least 15 meters"* (wiki, Melee).
///
/// A SLAM MODE'S LOOP IS `climb -> slam -> recover`, and the climb is the
/// player's own time — the same freedom [`crate::fight::heavy_cycle_seconds`]
/// spends on the combo counter — so it reaches the 15 m that sentence names.
/// The climb itself is not charged, which is the model's declared gap.
///
/// IT PAYS A COMBO'S TRAILING SLAM NOTHING: that one is swung on the ground at
/// the end of a ground combo, which is why this multiplies the attack's own
/// `radial` and not every explosion whose epicentre is the wielder.
pub(super) const SLAM_HEIGHT_RADIUS_SCALE: f64 = 1.5;

/// Resolve a mod set in slot order against a weapon base.
/// Resolve a build for the NEUTRAL Tenno (`data/tenno/default.yaml`): aiming,
/// no frame chosen, no ability running. The panel's and the optimizer's
/// default view, and the historical behaviour of every caller.
pub fn resolve(base: &WeaponBase, mods: &[&ModDef], policy: StackPolicy) -> ResolvedPanel {
    resolve_for(base, mods, policy, crate::data::tenno::default_tenno())
}

/// Resolve a build for a GIVEN Tenno — the fight's second actor. Every
/// `condition:` a mod card states is a question about this player: aiming,
/// invisible, airborne. A gated effect whose condition is false is absent from
/// the static buckets AND from the emergent specs handed to the sim, so the
/// buff never arms rather than arming and contributing zero.
pub fn resolve_for(
    base: &WeaponBase,
    mods: &[&ModDef],
    policy: StackPolicy,
    tenno: &crate::data::tenno::Tenno,
) -> ResolvedPanel {
    // A GATED FLAT BASE-DAMAGE ADD IS A CHANGE TO THE WEAPON, folded BEFORE
    // anything reads the panel: Haven Foray's "With Overshields: Increase Base
    // Damage by +40" makes the same weapon a plain "+40" would. The only gated
    // grant that cannot be a term added later, since the other four join a
    // bucket and this moves the number every bucket multiplies.
    // A FORM THAT CANNOT ZOOM CANNOT BE AIMING. The wiki's word for aiming IS
    // "Zoom", and DE settled the consequence in a patch note about Mesa's
    // Regulators: the buffs "never actually applied due to the 'on aim'
    // criteria not being fulfilled". Answered HERE because it is per FORM — the
    // Vasto aims and its Incarnon form does not.
    //
    // A SPRINT GATE READS THE MOD ON THE GUN, which is why the Tenno is
    // adjusted before any gate is asked. VERBATIM, from the Notes cell of both
    // Swift Punishment and Deadly Pace: *"Equipping Amalgam Serration will
    // allow any Warframe to reach the threshold."*
    //
    // Its "+25% Sprint Speed" is a WARFRAME stat the weapon carries. The
    // roster's sprint figures are UNMODDED and the slowest frame is 0.9, so
    // 0.9 x 1.25 = 1.125 clears the 1.1 those perks require and not the 1.2
    // their cards print. Sprint mods are a share of the BASE, added together.
    let mut adjusted: Option<crate::data::tenno::Tenno> = None;
    let sprint_bonus: f64 = mods
        .iter()
        .flat_map(|m| m.effects.iter())
        .filter_map(|e| match e {
            ModEffect::Indirect(IndirectStat::SprintSpeed, v) => Some(*v),
            _ => None,
        })
        .sum();
    if sprint_bonus > 0.0 {
        let mut t = tenno.clone();
        t.sprint *= 1.0 + sprint_bonus;
        adjusted = Some(t);
    }
    if base.cannot_zoom && tenno.state.aiming {
        let mut t = adjusted.take().unwrap_or_else(|| tenno.clone());
        t.state.aiming = false;
        adjusted = Some(t);
    }
    let tenno = adjusted.as_ref().unwrap_or(tenno);
    let open_flat = || {
        base.gated
            .iter()
            .filter(|t| t.grant == GatedGrant::FlatBaseDamage && t.gate.holds(tenno))
    };
    let gated_flat: f64 = open_flat().map(|t| t.value).sum();
    // …AND HOW MUCH OF IT THE GunCO TERM'S BASE GROWS BY, which is the perk's
    // own answer carried on the term. Two sums, for the reason `apply` keeps
    // two: a build can hold one gated add that feeds and one that does not.
    let gated_into_co: f64 = open_flat().map(|t| t.into_co).sum();
    // …AND THE SAME QUESTION FOR THE MAGAZINE. Folded into the BASE here rather
    // than into the modded size below, so a gated +14 and a plain +14 are the
    // same weapon everywhere the magazine is read — the mods multiply it, the
    // by-shell reload counts it, and a charged form's ammo ratio has it in both
    // halves of its fraction.
    //
    // "Increased Base Magazine Capacity does not affect Incarnon Form" (Lone
    // Gun, Extended Volley): the same `incarnon.is_none()` guard `apply` puts
    // on the ungated spelling, kept here because this add cannot happen there —
    // `apply` never sees a Tenno.
    let gated_mag: f64 = if base.gauge_form.is_none() {
        base.gated
            .iter()
            .filter(|t| t.grant == GatedGrant::FlatBaseMagazine && t.gate.holds(tenno))
            .map(|t| t.value)
            .sum()
    } else {
        0.0
    };
    let owned;
    let base = if gated_flat > 0.0 || gated_mag > 0.0 {
        let mut b = base.clone();
        if gated_flat > 0.0 {
            // A GATED FLAT ADD ASKS THE SAME QUESTION THE UNGATED ONE DOES,
            // and Guardian's Might is why it must: one card, +20 unconditional
            // and +74 with overshields, and answering the two halves
            // differently made the perk's own gate move the CO base
            // (MEASUREMENTS M83).
            b.add_flat_base_damage(gated_flat, gated_into_co);
        }
        b.magazine_size += gated_mag;
        owned = b;
        &owned
    } else {
        base
    };
    // THE FIGHT'S OWN BONUSES SEED THE BUCKETS, before a single mod is read —
    // which is the whole of what "the effect equals stuffing in another mod"
    // means. They are ADDITIVE with the mods by
    // construction, because they are in the same variable, so nothing
    // downstream had to learn the concept: every bucket's arithmetic, every
    // lock, every panel row and the optimizer's own scoring treat them as one
    // more card in the build.
    //
    // A LOCK STILL WINS. `locks("multishot")` zeroes the bucket further down,
    // and a fight bonus is in it — which is right: "set to its default ignoring
    // other bonuses" does not make an exception for where the bonus came from.
    let fb = &tenno.bonuses;
    // …AND THE ARCHON SHARDS, which land in the SAME buckets a mod of that stat
    // feeds — the two Crimson ones are the whole family that does, and
    // `unmodelled_reason` names why each of the others does not. Added into
    // `fb`'s own terms rather than beside them, so every lock, every panel line
    // and the optimizer's scoring treat a socket as one more card.
    let shards = tenno.shard_bonuses(base.slot);
    let (mut base_damage, mut multishot, mut cc, mut cd, mut sc, mut fr, mut status_damage) = (
        // THE AMP AURAS JOIN THIS BUCKET, which is where their own page puts
        // them: "Rifle Amp adds to the base damage as Serration and Heavy
        // Caliber do", with the formula spelled out —
        // `Base x (1 + Serration + Heavy Caliber + Rifle Amp)`. So the amps are
        // worth LESS the more Serration is already in the sum, which is the
        // whole reason a reader wants to see them in it rather than as a final
        // multiplier.
        // …AND A MELEE GENESIS, for the same reason and in the same bucket: the
        // Magistar Incarnon Form's `+100% Melee Damage` is a bracket term
        // beside Pressure Point, so it is worth less the more Pressure Point is
        // already in the sum. That is the whole difference from a gun Genesis's
        // `+14 Base Damage`, which is added to the weapon's own number.
        fb.base_damage
            + tenno.aura_damage_bonus(base.class, base.mod_pools)
            + base.evo_base_damage_bonus,
        fb.multishot,
        fb.crit_chance + shards.crit_chance,
        fb.crit_damage,
        fb.status_chance + shards.status_chance,
        // NOT doubled by `fire_rate_mod_multiplier`: the bow x2 is printed on
        // the CARD of a fire-rate mod, and a fight bonus has no card.
        fb.fire_rate,
        fb.status_damage,
    );
    // THE SCOPE'S OWN CRIT, and it joins the ordinary buckets. Most snipers'
    // zoom grants a critical CHANCE or MULTIPLIER rather than headshot damage
    // (the Lanka +50% chance at 8x, the Rubico family +50% multiplier), and
    // "these zoom buffs ... generally stack additively with similar buffs from
    // mods" (wiki `Sniper Rifle`) — so they are bucket terms, not their own
    // factor. Added HERE, above the lock site, so a mod that pins crit chance
    // wipes this too: "set to its default ignoring other bonuses" does not make
    // an exception for where the bonus came from.
    if tenno.state.aiming {
        cc += base.scope_crit_chance;
        cd += base.scope_crit_multiplier;
    }
    // …and the Lanka's, which is NOT a bucket term. Its bonus is "a flat
    // +20/30/50 critical chance, applied after mods", so it lands on the
    // post-mod layer beside the other flat grants rather than being multiplied
    // by the weapon's unmodded 25%.
    // THE FIGHT'S FLAT CRIT CHANCE joins the same post-mod layer: an ability's
    // "+X% critical chance" is percentage points, never scaled by the base.
    let post_mod_cc_extra = if tenno.state.aiming { base.scope_crit_chance_post_mod } else { 0.0 }
        + fb.flat_crit_chance;
    // RELOAD STARTS AT THE EVOLUTION'S BONUS, not at zero. Rapid Reinforcement
    // and its family feed the SAME additive bucket the mods do — one bucket, so
    // an evolution's +60% and Primed Fast Hands' +55% sum rather than
    // multiplying, which is the shape every other shared stat here has.
    let mut rl = base.evo_reload_bonus + fb.reload_speed;
    // Magazine-capacity and status-duration additive buckets.
    let (mut mag, mut sdur) = (fb.magazine, fb.status_duration);
    // Sentient Surge's three, carried to the sim rather than spent here: all
    // three depend on fight state (how many tendrils are up, whether anything
    // died) that the panel cannot know.
    let (mut per_tendril_cc, mut per_tendril_sc, mut mag_refill) = (0.0, 0.0, 0.0);
    // A syndicate augment's radial, resolved from the six-effect table.
    let mut syndicate_radial: Option<crate::data::syndicates::SyndicateDef> = None;
    // Blast RANGE bucket (Firestorm / Fulmination): + the sum, of base radius.
    let mut br = 0.0;
    // Hunter Munitions: its own bucket, because its roll is its own.
    let mut slash_on_crit = 0.0;
    // Unconditional weapon-level CO (Carnage Reign) seeds the static rate.
    // …AND THE HALF THAT ASKS ABOUT THE PLAYER. "With Sprint Speed 1.2 or
    // Higher" is a question about who is carrying the gun, so it is answered
    // here rather than in `apply`, which never sees a Tenno. The neutral player
    // sprints at 0.9 — the slowest frame — so a perk gated on speed pays
    // nothing until someone says which frame is holding it.
    // …AND THE HALF THAT ASKS ABOUT THE PLAYER, summed once for every bracket.
    // The neutral Tenno opens none of these gates, so a build that does not say
    // which frame is holding the gun pays nothing for them — which is the
    // honest default and the same rule every other Tenno field here follows.
    let gate = |g: GatedGrant| -> f64 {
        base.gated
            .iter()
            .filter(|t| t.grant == g && t.gate.holds(tenno))
            .map(|t| t.value)
            .sum()
    };
    let mut co = base.innate_co_per_type + gate(GatedGrant::ConditionOverload);
    let (mut co_stack, mut multishot_stack): (Option<StackSpec>, Option<StackSpec>) = (None, None);
    let mut crit_chance_on_headshot: Option<TimedBuff> = None;
    // LEADED GAS' window — an element and a status bonus a weak point turns on.
    let mut on_weakpoint: Option<crate::model::WeakpointBuff> = None;
    let mut crit_chance_stack: Option<StackSpec> = None;
    // …and the weak-point crit bucket starts at the EVOLUTION's, not at zero,
    // because the card says it is additive with the mods that write here.
    let mut consecutive_hit: Option<(f64, u32, f64)> = None;
    // SYNTH CHARGE. Summed, though only one such mod exists — a bucket of one
    // is still a bucket, and a second card would otherwise silently replace the
    // first.
    let mut last_round_damage = 0.0f64;
    let mut first_round_damage = 0.0f64;
    // STACKING BUFFS THE MODS GRANT — see `ModEffect::GrantsStackingBuff`.
    // They join the weapon's own list below rather than replacing anything, so
    // a build can carry a perk's buff and a mod's at once.
    let mut mod_buffs: Vec<StackingBuff> = Vec::new();
    // Degrees of cone added AFTER the accuracy divisor — see
    // `ModEffect::AddedSpread`.
    let mut added_spread = 0.0f64;
    let (mut wp_dmg, mut wp_cc) = (0.0, base.evo_weakpoint_crit_chance_relative);
    let mut crit_damage_on_kill: Option<TimedBuff> = None;
    let mut fire_rate_on_reload: Option<TimedBuff> = None;
    let mut base_damage_on_reload: Option<TimedBuff> = None;
    let mut base_damage_on_eximus_weakpoint: Option<TimedBuff> = None;
    let mut crit_chance_per_hit: Option<CritPerHit> = None;
    // MELEE COMBO. Four buckets and one clock — all zero on every gun, which is
    // what keeps this function's answer unchanged for them.
    let mut cc_per_combo = 0.0f64;
    let mut sc_per_combo = 0.0f64;
    let mut combo_duration_add = 0.0f64;
    let mut initial_combo = 0.0f64;
    let mut heavy_efficiency = 0.0f64;
    let mut combo_duration_mult = 1.0f64;
    let mut melee_range_add = 0.0f64;
    let mut slam_damage = 0.0f64;
    let mut heavy_damage = 0.0f64;
    let mut combo_count_chance = 0.0f64;
    let mut combo_count_chance_on_lifted = 0.0f64;
    let mut combo_gain_chance = 0.0f64;
    let mut status_chance_on_lifted = 0.0f64;
    let mut windup_speed = 0.0f64;
    // TENNOKAI: off until a card says otherwise, and every knob a sum.
    let mut tk = Tennokai::default();
    // Seconds a mod adds to the sniper combo's decay window (Harkonar Scope).
    let mut combo_seconds_bonus = 0.0f64;
    // Assembled from the card's three columns; `seen` is what tells "no augment"
    // from "an augment whose radius happens to be zero".
    let mut acid = AcidShells::default();
    let mut acid_seen = false;
    // CARRIED, NOT SPENT — the mods that scale off the PLAYER. Folded in
    // `resolve_for`, which has the Tenno, the same way `base.gated` is.
    let mut tenno_scaled: Vec<TennoScaledTerm> = base.tenno_scaled.clone();
    // A field a MOD leaves on a weapon that has none (Nightwatch Napalm), with
    // the share of the blast AREA it covers.
    let mut granted_lingering: Option<&'static LingeringBase> = None;
    let mut granted_area_fraction = 1.0f64;
    // READY RETALIATION arrives on the BASE (an evolution wrote it there),
    // unlike the two above which arrive from mods — so the policy split is
    // here rather than in the mod loop.
    //
    // AssumedMax spends it into the RELOAD BUCKET, which is what the panel and
    // the optimizer's ranking read — a panel is a statement about a reload, and
    // in this arena every reload is from empty. Emergent hands it to the sim,
    // which applies it to each reload and to nothing else: a transmute
    // animation is scaled by the same bucket and is NOT a reload, so folding it
    // into `rl` there would have sped up an animation this perk never touches.
    // A sentinel's conditional never fires.
    let mut rs_on_reload = match policy {
        StackPolicy::Emergent => base.rs_on_empty_reload,
        StackPolicy::AssumedMax => {
            rl += base.rs_on_empty_reload;
            0.0
        }
        _ => 0.0,
    };
    // SEEDED FROM THE EVOLUTION, so a Genesis and a mod reach the one runtime.
    // A mod overwrites it — a build cannot hold two of these and a rule for a
    // case that cannot arise is a rule nobody can check, which is the same
    // argument `StripOnKillInRange` makes one screen down.
    let mut proc_conv: Option<ProcConv> = base.evo_proc_conversion.map(|(from, to, chance)| {
        ProcConv { from, to, chance, low_rate_threshold: 0.0, low_rate_multiplier: 1.0 }
    });
    let mut elem_bonus: Vec<(DamageType, f64)> = Vec::new();
    // SEEDED from the weapon, not empty: an evolution's indirect stat is a
    // property of the weapon by the time `resolve` runs (evolutions are folded
    // into `WeaponBase` first), and it shares its bucket with the mods'.
    let mut indirect: Vec<(IndirectStat, f64)> = base.indirect.clone();
    // …plus any the PLAYER's state opens. Same bucket the mods and the
    // unconditional evolutions feed, so it sums rather than multiplying.
    {
        let ps = gate(GatedGrant::ProjectileSpeed);
        if ps != 0.0 {
            match indirect.iter_mut().find(|(s, _)| *s == IndirectStat::ProjectileSpeed) {
                Some((_, v)) => *v += ps,
                None => indirect.push((IndirectStat::ProjectileSpeed, ps)),
            }
        }
        let acc = gate(GatedGrant::Accuracy);
        if acc != 0.0 {
            match indirect.iter_mut().find(|(s, _)| *s == IndirectStat::Accuracy) {
                Some((_, v)) => *v += acc,
                None => indirect.push((IndirectStat::Accuracy, acc)),
            }
        }
    }
    // Charge-rate bonuses, summed apart from fire rate: both shorten the draw,
    // only fire rate also raises an uncharged form's cadence.
    let mut cr = 0.0f64;
    // JAHU CANTICLE's strip, `(fraction, radius_m)` — see
    // `ModEffect::StripOnKillInRange`.
    let mut strip_on_kill_in_range: Option<(f64, f64)> = None;
    // THE INVOCATIONS' contributions to the FIGHT's own knobs.
    let (mut ability_strength_bonus, mut ability_duration_bonus) = (0.0f64, 0.0f64);
    let mut faction_bonus: Vec<(Faction, f64)> = Vec::new();
    // Physical (IPS) bonuses, per type — scale the base of that physical type.
    let mut phys_bonus: Vec<(DamageType, f64)> = Vec::new();
    // Stats LOCKED from modding by an equipped mod (Pistol Acuity → multishot,
    // Semi-Pistol Cannonade → fire_rate). Their bucket is zeroed after the loop.
    let mut disabled: Vec<&str> = Vec::new();

    for m in mods {
        // `requires`: a mod whose required weapon trait is absent is INERT here
        // (calc-layer, not an equip block) — skip all of its effects/locks.
        if let Some(req) = m.requires {
            if !base.traits.contains(&req) {
                continue;
            }
        }
        for &d in &m.disables {
            if !disabled.contains(&d) {
                disabled.push(d);
            }
        }
        // …AND A SET THAT ENHANCES ITS OWN MEMBERS scales what this one grants.
        // Completion, not per member: the Sacrificial pair is worth 25% more
        // EACH once both are in, and one alone is worth its face.
        let self_scale = crate::data::mod_sets::self_scale_for(m, mods);
        for e in &m.effects {
            let boosted;
            let e = if self_scale > 1.0 {
                boosted = e.scaled(self_scale);
                boosted.as_ref().unwrap_or(e)
            } else {
                e
            };
            // Unwrap the player gates HERE so no arm below has to know about
            // them: a gated effect either becomes its inner effect or vanishes.
            // `aiming` is the older, bare form of the same idea; the Tenno one
            // asks the player's state instead of a threaded bool.
            let e: &ModEffect = match e {
                ModEffect::WhileTenno(c, inner) if c.holds(tenno) => inner,
                ModEffect::WhileTenno(..) => continue,
                other => other,
            };
            match *e {
                ModEffect::WhileTenno(..) => {
                    unreachable!("unwrapped above")
                }
                // DOUBLE TAP does not join a bucket — it carries its own
                // multiplier to the sim, which is the whole point of the card's
                // "multiplicatively stacks with damage bonuses like Serration".
                ModEffect::AddedSpread(v) => added_spread += v,
                ModEffect::GrantsStackingBuff(b) => mod_buffs.push(b),
                ModEffect::LastRoundDamage(v) => last_round_damage += v,
                // THE CHAMBERS SUM, which is the wiki's own "stacks additively
                // with … for up to 140% bonus damage" — one factor, two cards.
                ModEffect::FirstRoundDamage(v) => first_round_damage += v,
                ModEffect::ConsecutiveHitDamage { per_stack, max_stacks, duration } => {
                    consecutive_hit = Some((per_stack, max_stacks, duration));
                }
                // CARRIED, NOT SPENT — the player is not here. It is folded in
                // `resolve_for`, which has the Tenno, exactly as `gated` is.
                ModEffect::TennoScaled { stat, above, unit, per_unit, cap, grant } => {
                    tenno_scaled.push(TennoScaledTerm { stat, above, unit, per_unit, cap, grant });
                }
                ModEffect::BaseDamage(v) => base_damage += v,
                ModEffect::Multishot(v) => multishot += v,
                ModEffect::CritChance(v) => cc += v,
                // The per-tendril halves do NOT join `cc`/`sc` here: their
                // size depends on how many tendrils are up, which is a fact
                // about the fight and not about the build. They travel to the
                // sim as rates and are spent there, the same way an on-reload
                // buff is.
                ModEffect::PerTendril { crit_chance, status_chance } => {
                    per_tendril_cc += crit_chance;
                    per_tendril_sc += status_chance;
                }
                // HATA-SATYA. Same split as the tendrils above and for the
                // same reason — how many hits are in the pile is a fact about
                // the fight — except that the panel HAS an honest maximum to
                // show, because the card publishes one (500%). Assumed-max is
                // that number itself rather than a stack count times a rate:
                // the ceiling is what DE published and the arithmetic under it
                // is ours.
                ModEffect::CritChancePerHit(c) => match policy {
                    StackPolicy::AssumedMax => cc += c.max_bonus,
                    StackPolicy::Emergent => crit_chance_per_hit = Some(c),
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                // THE FOUR MELEE COMBO BUCKETS. Each is a plain sum: the wiki
                // writes Blood Rush and Weeping Wounds INSIDE the same bracket
                // as Point Strike and Rime Rounds, heavy efficiency "stacks
                // additively", and combo duration is seconds added to seconds.
                ModEffect::CritChancePerCombo(v) => cc_per_combo += v,
                ModEffect::StatusChancePerCombo(v) => sc_per_combo += v,
                ModEffect::MeleeComboDuration(v) => combo_duration_add += v,
                ModEffect::InitialCombo(v) => initial_combo += v,
                ModEffect::HeavyAttackEfficiency(v) => heavy_efficiency += v,
                // MULTIPLICATIVE, so two of them compose rather than cancel —
                // which is what a relative card means and is why it is not in
                // the seconds bucket beside Body Count.
                ModEffect::MeleeComboDurationMultiplier(v) => combo_duration_mult *= 1.0 + v,
                ModEffect::MeleeRange(v) => melee_range_add += v,
                ModEffect::SlamDamage(v) => slam_damage += v,
                ModEffect::HeavyAttackDamage(v) => heavy_damage += v,
                ModEffect::ComboCountChance(v) => combo_count_chance += v,
                ModEffect::ComboCountChanceOnLifted(v) => combo_count_chance_on_lifted += v,
                ModEffect::ComboGainChance(v) => combo_gain_chance += v,
                ModEffect::StatusChanceOnLifted(v) => status_chance_on_lifted += v,
                ModEffect::HeavyWindUpSpeed(v) => windup_speed += v,
                ModEffect::Tennokai {
                    enabled, chance, every_n_hits, window_seconds, damage, crit_damage,
                    status_chance, chain_seconds, damage_needs_chain, curse_resets_combo,
                    curse_heat_per_second, curse_seconds,
                } => {
                    // ONE CARD CARRIES THESE and they compose the way the rest
                    // do: the longest chain, and a term either card states.
                    tk.chain_seconds = tk.chain_seconds.max(chain_seconds);
                    tk.damage_needs_chain |= damage_needs_chain;
                    tk.curse_resets_combo |= curse_resets_combo;
                    tk.curse_heat_per_second = tk.curse_heat_per_second.max(curse_heat_per_second);
                    tk.curse_seconds = tk.curse_seconds.max(curse_seconds);
                    tk.enabled |= enabled;
                    tk.chance += chance;
                    // A CADENCE REPLACES THE ROLL and the SHORTEST one wins,
                    // which is the only reading that composes: two cards each
                    // promising "every N hits" cannot both be waited for.
                    if every_n_hits > 0 {
                        tk.every_n_hits = if tk.every_n_hits == 0 {
                            every_n_hits
                        } else {
                            tk.every_n_hits.min(every_n_hits)
                        };
                    }
                    tk.window_seconds = tk.window_seconds.max(window_seconds);
                    tk.damage += damage;
                    tk.crit_damage += crit_damage;
                    tk.status_chance += status_chance;
                }
                // …AND THE ONE THAT NAMES THE SLIDE. It joins the ordinary
                // relative crit bucket, but only on the form that IS a slide —
                // which is a question `resolve` can answer and the fight loop
                // would have to ask again on every swing.
                // …AND THE ONE THAT DOUBLES ON A HEAVY. The card's own words,
                // so it is the card that carries the rule — Blood Rush sits in
                // the same bracket and does not double.
                ModEffect::CritChanceHeavyDoubled(v) => {
                    cc += v;
                    if base.form.is_heavy() {
                        cc += v;
                    }
                }
                ModEffect::CritChanceOnSlide(v) => {
                    if base.form == crate::model::FormKind::Slide {
                        cc += v;
                    }
                }
                ModEffect::MagazineRefillOnKill(v) => mag_refill += v,
                // The card names one of six; the payload is the syndicate's.
                ModEffect::SyndicateRadial { syndicate, .. } => {
                    syndicate_radial = crate::data::syndicates::get(syndicate).copied();
                }
                ModEffect::CritDamage(v) => cd += v,
                ModEffect::StatusChance(v) => sc += v,
                // "(x2 for Bows)" is on the CARD of every fire-rate mod, so it
                // is the mod's own bonus that doubles — penalties included
                // (Critical Delay reads −40% on a bow). Buff-granted fire rate
                // joins the bucket further down UNDOUBLED: no such card says it.
                ModEffect::FireRate(v) => fr += v * base.fire_rate_mod_multiplier,
                // ONE CARD, so the last one wins rather than the two adding —
                // a build cannot hold two Jahus, and a rule for a case that
                // cannot arise is a rule nobody can check.
                ModEffect::StripOnKillInRange(f, r) => strip_on_kill_in_range = Some((f, r)),
                // AT THE CAP. See `ModEffect::AbilityStat` for why that is
                // nearly exact on the one weapon that can carry these.
                ModEffect::AbilityStat(stat, per, max) => {
                    let total = per * f64::from(max);
                    match stat {
                        AbilityStat::Strength => ability_strength_bonus += total,
                        AbilityStat::Duration => ability_duration_bonus += total,
                        // The other two are transcribed and pay nothing — the
                        // card says which, and there is no bucket to add to.
                        AbilityStat::Efficiency | AbilityStat::EnergyRegen => {}
                    }
                }
                // The draw's own bucket. `fire_rate_mod_multiplier` is the
                // bow x2, which belongs to fire-rate mods and not to this.
                ModEffect::ChargeRate(v) => cr += v,
                ModEffect::ReloadSpeed(v) => rl += v,
                ModEffect::StatusDamage(v) => status_damage += v,
                // Independent of everything else: it is its own roll on a
                // crit, so it is its own bucket rather than joining status.
                ModEffect::SlashOnCrit(v) => slash_on_crit += v,
                // Physical (IPS) bonus: accumulate per type; applied to the
                // BASE physical component below (NOT the elemental hierarchy).
                ModEffect::Physical(t, v) => {
                    if let Some(x) = phys_bonus.iter_mut().find(|(a, _)| *a == t) {
                        x.1 += v;
                    } else {
                        phys_bonus.push((t, v));
                    }
                }
                ModEffect::Element(t, v) | ModEffect::CombinedElement(t, v) => {
                    if let Some(x) = elem_bonus.iter_mut().find(|(a, _)| *a == t) {
                        x.1 += v;
                    } else {
                        elem_bonus.push((t, v));
                    }
                }
                ModEffect::OnKillMultishot {
                    per_stack,
                    max_stacks,
                    duration,
                } => match policy {
                    StackPolicy::AssumedMax => multishot += per_stack * max_stacks as f64,
                    StackPolicy::Emergent => {
                        multishot_stack = Some(StackSpec {
                            per_stack: base.base_multishot * per_stack,
                            max_stacks,
                            duration,
                            initial_stacks: 0, // EARNED — docs/BUFFS.md §Activation policy
                            earned_on: Some("kill"),
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::ConditionOverload {
                    per_stack,
                    max_stacks,
                    duration,
                    earned_on,
                } => match policy {
                    StackPolicy::AssumedMax => co += per_stack * max_stacks as f64,
                    StackPolicy::Emergent => {
                        co_stack = Some(StackSpec {
                            per_stack,
                            max_stacks,
                            duration,
                            // EARNED — docs/BUFFS.md §Activation policy — UNLESS
                            // the card has nothing to earn, which is what
                            // `earned_on` states and melee's Condition Overload
                            // is: routing it through the Galvanized family's
                            // earned-on-a-kill path made it pay nothing at all,
                            // in all seven modes.
                            //
                            // NOT DERIVED FROM `duration == NO_TIMEOUT`, which
                            // would have been the cheap test and is wrong:
                            // LOCKING a buff card writes exactly that duration,
                            // and locking "removes the expiry and nothing else
                            // — the count still starts where the card sets it".
                            initial_stacks: if earned_on.is_none() { max_stacks } else { 0 },
                            earned_on,
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::OnHeadshotCritChance { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => cc += bonus,
                    StackPolicy::Emergent => {
                        crit_chance_on_headshot = Some(TimedBuff {
                            // RELATIVE, deliberately: `cc += bonus` is what
                            // the AssumedMax arm does, and that bonus reaches
                            // EVERY attack part through the bucket. Resolving
                            // it against the direct part's base here made the
                            // same mod skip the explosion under Emergent.
                            value: bonus,
                            duration,
                            initial_active: false, // EARNED, like every timed buff
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                // LEADED GAS. Under AssumedMax the status half joins its
                // bucket and the ELEMENT does not: an element is a share of the
                // base added to the damage VECTOR, and the vector is built
                // above this loop. So the assumed-max panel understates it, and
                // the fight — which has the window — is where it pays.
                ModEffect::OnWeakpointElementAndStatus { element, bonus, duration } => {
                    match policy {
                        StackPolicy::AssumedMax => sc += bonus,
                        StackPolicy::Emergent => {
                            on_weakpoint = Some(crate::model::WeakpointBuff {
                                element,
                                bonus,
                                duration,
                            })
                        }
                        StackPolicy::BaseOnly => {}
                    }
                }
                ModEffect::OnHeadshotKillCritChance {
                    per_stack,
                    max_stacks,
                    duration,
                } => match policy {
                    StackPolicy::AssumedMax => cc += per_stack * max_stacks as f64,
                    StackPolicy::Emergent => {
                        crit_chance_stack = Some(StackSpec {
                            per_stack, // RELATIVE — see crit_chance_on_headshot above
                            max_stacks,
                            duration,
                            initial_stacks: 0, // EARNED — docs/BUFFS.md §Activation policy
                            earned_on: Some("headshot_kill"),
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::Indirect(stat, v) => {
                    if let Some(x) = indirect.iter_mut().find(|(s, _)| *s == stat) {
                        x.1 += v;
                    } else {
                        indirect.push((stat, v));
                    }
                }
                // Conditional handling buff (Reflex Draw): handling-only and
                // temporary — listed from the card, never a static panel stat.
                ModEffect::OnEquipHandling { .. } => {}
                // Faction bonus: additive within a faction (Bane + Roar share
                // one bracket); the matching-faction multiply happens at sim
                // time. Merge by faction here.
                ModEffect::FactionDamage(fac, v) => {
                    if let Some(x) = faction_bonus.iter_mut().find(|(f, _)| *f == fac) {
                        x.1 += v;
                    } else {
                        faction_bonus.push((fac, v));
                    }
                }
                ModEffect::ComboDuration(v) => combo_seconds_bonus += v,
                ModEffect::AcidShells(part) => {
                    acid_seen = true;
                    match part {
                        AcidShellsPart::FlatDamage(v) => acid.flat_damage += v,
                        AcidShellsPart::HealthFraction(v) => acid.health_fraction += v,
                        AcidShellsPart::RadiusM(v) => acid.radius_m += v,
                    }
                }
                ModEffect::GrantsLingering(field) => granted_lingering = Some(field),
                ModEffect::LingeringAreaFraction(v) => granted_area_fraction = v,
                ModEffect::MagazineCapacity(v) => mag += v,
                ModEffect::BlastRadius(v) => br += v,
                ModEffect::StatusDuration(v) => sdur += v,
                // Weak-point effects: conditional on the PART HIT, not on an
                // uptime — so they are active under EVERY policy and the sim
                // gates them on `is_head`. AssumedMax is about a buff's stack
                // count, not about where a bullet lands: no policy can make a
                // body shot into a head shot.
                //
                // Folding the CC half into the plain bucket under
                // AssumedMax splits ONE mod down the middle — Acuity's Weak
                // Point Damage stays conditional while its Weak Point Crit
                // Chance becomes unconditional. On the panel (always
                // AssumedMax) that read the Burston Incarnon's 28% as 126%,
                // and handed the same 126% to the RADIAL, which can never
                // weak-point-hit at all. The arcane source of the same effect
                // (Cascadia Accuracy) was already in this bucket, so the mod
                // was the only thing in the engine claiming otherwise.
                ModEffect::WeakpointDamage(v) => wp_dmg += v,
                ModEffect::WeakpointCritChance(v) => wp_cc += v,
                ModEffect::OnKillCritDamage { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => cd += bonus,
                    StackPolicy::Emergent => {
                        crit_damage_on_kill = Some(TimedBuff {
                            value: bonus, // RELATIVE — see crit_chance_on_headshot above
                            duration,
                            initial_active: false, // Sharpened Bullets seeds inactive
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::OnReloadDamage { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => base_damage += bonus,
                    StackPolicy::Emergent => {
                        base_damage_on_reload = Some(TimedBuff {
                            // RELATIVE: the sim adds it into the base-damage
                            // bucket alongside Serration, which is where the
                            // card's "+X% Damage" belongs.
                            value: bonus,
                            duration,
                            initial_active: false, // no reload has happened yet
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                // EXIMUS ADVANTAGE, the same three-policy shape as Deadly
                // Efficiency above: the panel folds it in at its maximum, the
                // sim earns it. What the sim adds that the panel cannot is the
                // TARGET's half — a fight against a non-Eximus never opens the
                // window at all, and the panel has no target to ask.
                ModEffect::OnEximusWeakpointDamage { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => base_damage += bonus,
                    StackPolicy::Emergent => {
                        base_damage_on_eximus_weakpoint = Some(TimedBuff {
                            value: bonus,
                            duration,
                            initial_active: false, // no weak point has been hit yet
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::OnReloadFireRate { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => fr += bonus,
                    StackPolicy::Emergent => {
                        fire_rate_on_reload = Some(TimedBuff {
                            value: base.base_fire_rate * bonus,
                            duration,
                            initial_active: false, // Pressurized Magazine seeds inactive
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                // Event mechanic — carried to the sim under every policy;
                // contributes no static panel stat.
                ModEffect::ProcConversion { from, to, chance, low_rate_threshold, low_rate_multiplier } => {
                    proc_conv = Some(ProcConv { from, to, chance, low_rate_threshold, low_rate_multiplier });
                }
                // Conditional buff at its assumed-max total — applied only under
                // AssumedMax (panel/optimizer); emergent leaves it to the sim.
                ModEffect::CondBuff(bucket, v) => {
                    if policy == StackPolicy::AssumedMax {
                        match bucket {
                            CondBucket::BaseDamage => base_damage += v,
                            CondBucket::Multishot => multishot += v,
                            CondBucket::CritChance => cc += v,
                            CondBucket::CritDamage => cd += v,
                            CondBucket::StatusChance => sc += v,
                            CondBucket::StatusDamage => status_damage += v,
                            CondBucket::FireRate => fr += v,
                            CondBucket::ReloadSpeed => rl += v,
                        }
                    }
                }
            }
        }
    }

    // …AND THE BONUSES THE PLAYER DECIDES, folded once the whole build is in.
    //
    // AFTER the loop rather than inside it, because a term is a function of
    // the Tenno and not of the mod that carried it: two cards reading the same
    // stat pay their own steps and the sum joins the bucket once, which is what
    // "Both the Damage and Status chance bonuses are additive" means on the
    // Basmu's own card.
    //
    // THE NEUTRAL PLAYER PAYS FOR NONE OF IT. `TennoStat::of` reads the fight's
    // Tenno, and a build that has not said which frame is holding the gun has
    // 250 health and 105 armor — the honest default, and the same one every
    // gated grant above already follows.
    for t in &tenno_scaled {
        let v = t.value(tenno);
        match t.grant {
            crate::model::ArcGrant::BaseDamage => base_damage += v,
            crate::model::ArcGrant::StatusChance => sc += v,
            crate::model::ArcGrant::Multishot => multishot += v,
            crate::model::ArcGrant::CritDamage => cd += v,
            // A grant no card has asked for yet. Loudly nothing rather than
            // quietly the wrong bucket: `data::mods` refuses an unknown grant
            // at load, so reaching here means one was added to that list and
            // not to this one.
            _ => {}
        }
    }

    // Apply `disables`: a locked stat cannot be modified — zero its mod bucket
    // (and any conditional stacks feeding it); the weapon's base value stays.
    //
    // A LOCK IS ABSOLUTE, AND THE MOD BUCKET IS NOT THE WHOLE OF IT. Both
    // families that carry one say the same sentence: "Equipping this mod will
    // set weapon's Fire Rate to its default ignoring other bonuses, EVEN
    // NEGATIVE EFFECTS" (wiki, Semi-Rifle/Shotgun/Pistol Cannonade) and
    // "...will set weapon's Multishot to its default ignoring other bonuses,
    // even negative effects" (Primary/Pistol Acuity). "Its default" is the
    // WEAPON'S value, so a source that never passed through the mod bucket —
    // an evolution's permanent bonus, an arcane's live stacks, the weapon's own
    // Frenzy passive — is not exempt just because the loop above cannot see it. The out-of-bucket layers are shadowed here, and
    // `locked` carries the fact to the SIM, which owns the live ones.
    let locked_stat = |s: &str| disabled.contains(&s);
    let evo_ms_bonus = if locked_stat("multishot") {
        0.0
    } else {
        base.buff_multishot_bonus + gate(GatedGrant::Multishot)
    };
    let evo_ms_stacks = if locked_stat("multishot") { 0 } else { base.buff_multishot_max_stacks };
    let ms_last_round = if locked_stat("multishot") { 0.0 } else { base.multishot_on_last_round };
    let evo_fr_bonus = if locked_stat("fire_rate") {
        0.0
    } else {
        // …plus the half that asks about the PLAYER. Answered here, where the
        // Tenno is; the neutral player sprints at 0.9 — the slowest frame — so
        // a perk gated on speed pays nothing until someone says which frame is
        // holding the gun.
        base.evo_fire_rate_bonus + gate(GatedGrant::FireRate)
    };
    // PRELUDE OF MIGHT, resolved here because it is the one evolution whose
    // condition is the BUILD's own output: "with Critical Chance below 40%".
    // Computed against the same expression the panel publishes, so the tile and
    // the number can never disagree about whether it is on.
    //
    // It joins the BASE multiplier, so crit-damage mods multiply it — the raw
    // wikitext says "Increase Base Critical Damage Multiplier by +3x", the same
    // wording `flat_base_crit_multiplier` already models for Critical Parallel.
    // It shipped for one commit added AFTER the mods instead, which on a
    // Primed Target Cracker build is 10.14x against the correct 13.44x. The
    // difference only appears once a crit-damage mod is on, which is why
    // reading the rendered page rather than the wikitext missed it: the word
    // that decides it is "Base".
    // ONE STAT DERIVED FROM THE OTHER, both grants computed from the PRE-GRANT
    // modded values. No weapon carries both — the Dera has one and the
    // Cestra/Sicarus/Vectis the other — so the order cannot matter, and
    // computing them this way means it never will.
    let modded_cc_pre = (base.base_crit_chance * (1.0 + cc) + (base.post_mod_crit_chance + post_mod_cc_extra)).max(0.0);
    let modded_sc_pre =
        (base.base_status_chance * (1.0 + sc) + base.post_mod_status_chance).max(0.0);
    let derived = |spec: Option<(f64, f64)>, from: f64| -> f64 {
        spec.map_or(0.0, |(rate, cap)| (rate * from).min(cap))
    };
    let cc_from_sc = derived(base.base_crit_from_status, modded_sc_pre);
    let sc_from_cc = derived(base.base_status_from_crit, modded_cc_pre);

    let resolved_cc =
        ((base.base_crit_chance + cc_from_sc) * (1.0 + cc) + (base.post_mod_crit_chance + post_mod_cc_extra)).max(0.0);
    let prelude_cd = match base.crit_multiplier_below_crit_chance {
        Some((bonus, below)) if resolved_cc < below => bonus,
        _ => 0.0,
    };
    for &d in &disabled {
        match d {
            "multishot" => {
                multishot = 0.0;
                multishot_stack = None;
            }
            "fire_rate" => {
                fr = 0.0;
                fire_rate_on_reload = None;
            }
            "crit_chance" => {
                cc = 0.0;
                crit_chance_on_headshot = None;
                crit_chance_stack = None;
                crit_chance_per_hit = None;
                wp_cc = 0.0;
            }
            "crit_damage" => {
                cd = 0.0;
                crit_damage_on_kill = None;
            }
            "status_chance" => sc = 0.0,
            "base_damage" => {
                base_damage = 0.0;
                base_damage_on_reload = None;
                base_damage_on_eximus_weakpoint = None;
            }
            // A LOCK TAKES THE WINDOW TOO. "Set to its default ignoring other
            // bonuses" cannot mean the static half only — that was the bug the
            // Cannonades taught (MEASUREMENTS M30), and this is the third
            // on-reload buff to need the same line.
            "reload_speed" => {
                rl = 0.0;
                rs_on_reload = 0.0;
            }
            _ => {}
        }
    }

    // The vector build, shared by the direct hit and the radial part: both
    // run the SAME mod math on their OWN base vector (elemental mods are a
    // percentage of THAT part's base damage — MECHANICS §7 "radial attack
    // parts"). Returns (resolved vector, that part's ModifiedBase).
    //
    // Split the (base-damage-scaled) innate vector. Physical IPS stays a fixed
    // component; innate PRIMARY elements (Torid's Toxin, Verglas Prime's Cold)
    // go into their own bucket, which the hierarchy places LAST — the mods
    // combine among themselves and the innate takes what is left over
    // (MECHANICS §3 rule 2; placing them FIRST is the superseded draft the doc
    // calls out). They still scale with base-damage
    // mods like the rest of the base. (A physical-innate weapon leaves `input`
    // empty here, so this is a no-op for Dual Toxocyst.)
    // THE MODDED MAGAZINE, computed HERE because on one weapon it is a damage
    // stat and the damage is built below. A charge-backed Incarnon magazine is
    // a fixed resource outside the ammo system, so magazine mods never scale it.
    let mag_size = if base.gauge_form.is_some() {
        base.magazine_size
    } else {
        (base.magazine_size * (1.0 + mag)).floor()
    };
    // …AND WHAT A FULL CHARGE IS WORTH. The Phantasma's alt fire spends the
    // magazine to buy damage — "directly proportional to the amount of ammo
    // consumed" — so a bigger magazine is a longer charge and a bigger bomb.
    // The listed numbers are a full charge of the UNMODDED magazine, which is
    // what the arsenal shows, so this is a ratio against that.
    //
    // Applied to the BASE VECTOR, before the elemental hierarchy: the whole
    // attack is bigger, so ModifiedBase, the elements and every status payload
    // ride along without a second rule. See
    // `data::weapons::AttackSpec::charge_ammo_per_second`.
    let charge_scale = match base.charge_ammo_per_second {
        Some(_) if base.magazine_size > 0.0 => mag_size / base.magazine_size,
        _ => 1.0,
    };
    // ---- PRIMARY COMPRESSION ------------------------------------------
    // The arcane shrinks the explosion to a fifth while aiming and pays for
    // every metre given up. What it is worth is a property of the WEAPON, so
    // the two halves meet HERE and nowhere else: the arcane brings two ramps
    // per METRE, the weapon brings the radius those metres come off and its own
    // row in the published table (docs/CATALOGS.md §2).
    //
    //   radius_lost_m  = radius_considered × (1 − 0.2)     # continuous
    //   damage_bonus = damage_per_metre(rank) × radius_lost_m
    //
    // MODDED, not base: the table's Primed Firestorm column is exactly 1.44×
    // its base column on every row that can take the mod, which is the same
    // 1 + br this build already spent on the radius below.
    //
    // AND IT IS AIM-GATED, which is not a footnote on a weapon like this: the
    // whole card reads "on aim", so a scenario whose Tenno is not aiming gets
    // nothing at all rather than a reduced share. Same treatment every
    // `aiming` mod gets — the condition is a question about the player.
    let mut compression = None;
    if let Some(c) = base.compression.as_ref().filter(|_| tenno.state.aiming) {
        // WHICH radius. The attack's own, modded — unless the row names one
        // this weapon's data does not carry (the Vectis pair read a 0.1 m embed
        // radial instead of their 6.7 m headshot explosion), in which case the
        // row's metres ARE the answer and `effectiveness` is the transcribed
        // account of how far off that is rather than a second multiplication.
        let attack_radius = base
            .radial
            .as_ref()
            .map(|r| r.radius_m * if r.takes_blast_radius_mods { 1.0 + br } else { 1.0 })
            .or_else(|| base.lingering.as_ref().map(|f| f.radius_m * (1.0 + br)))
            .unwrap_or(0.0);
        let considered = c
            .reads_radius_m
            .unwrap_or(attack_radius * c.effectiveness);
        compression = Some(Compression {
            radius_lost_m: considered * (1.0 - COMPRESSION_RADIUS_KEPT),
            // THE ROW'S OTHER COLUMN, and the same split Condition Overload
            // has: `adds` joins the base-damage bucket and is diluted by
            // Serration, `multiplies` stands beside it. Most weapons multiply;
            // Ambassador, Battacor, Ferrox, Opticor, Trumna and every
            // Braton/Burston Incarnon add.
            adds: c.stacking == "adds",
        });
    }

    let build = |base_vector: &DamageVector,
                     elem_bonus: Option<&mut Vec<(DamageType, f64)>>|
     -> (DamageVector, f64) {
        let modified_base = base_vector.total() * (1.0 + base_damage);
        let scale = 1.0 + base_damage;
        let mut physical = DamageVector::new();
        let mut input = ElementalInput::default();
        for (t, v) in base_vector.iter_nonzero() {
            if t.is_primary_element() {
                input.innate.push((t, v * scale));
            } else {
                // Physical (IPS): base_t × (1 + Σ physical mods) × (1 + base dmg).
                // `scale` carries the base-damage multiplier; the physical bucket is
                // multiplicative with it (wiki Damage/Calculation).
                let pb = phys_bonus.iter().find(|(a, _)| *a == t).map_or(0.0, |(_, x)| *x);
                physical.add(t, v * scale * (1.0 + pb));
            }
        }

        // Mod-added elements append AFTER the innate ones, in mod order (first
        // placement establishes an element's position; later same-element mods
        // merge there).
        for m in mods {
            // ONE MOD, TWO ELEMENTS: THE LAST ONE LISTED GOES FIRST.
            //
            // Wiki, Damage §combining: "the hierarchy priority will be given to
            // the LAST elemental stat listed on the Riven mod" — its worked
            // example is a riven with "+100% Electricity first and +90% Toxin
            // last", where the TOXIN combines with a mod higher up and the
            // Electricity with one lower down. So a mod's own elements enter
            // the hierarchy in REVERSE of how the card prints them.
            //
            // Only a riven can carry two, so this reverses nothing else.
            // A Phantasma Prime with Magnetic / Cold / riven(Toxin,
            // Electricity) / Electricity reads Magnetic + Toxin in game;
            // listed-order pairing gives Viral + Electricity, because Cold
            // meets the riven's Toxin where the game has it meet the
            // Electricity.
            //
            // The wiki's other half needs no code: "if no other elemental
            // damage mods are present, the elements on the Riven mod will
            // combine with itself" — reversed or not they stay adjacent, so
            // they pair with each other exactly as they did.
            let mut own: Vec<(DamageType, f64)> = Vec::new();
            for e in &m.effects {
                match *e {
                    ModEffect::Element(t, v) => own.push((t, modified_base * v)),
                    ModEffect::CombinedElement(t, v) => {
                        input.direct_secondary.push((t, modified_base * v))
                    }
                    _ => {}
                }
            }
            for (t, v) in own.into_iter().rev() {
                input.push(t, v);
            }
        }
        let mut elem_bonus = elem_bonus;
        for &(t, bonus) in &base.injected_elements {
            input.injected.push((t, modified_base * bonus));
            // The injection "behaves like a Toxin mod, additive with
            // elemental mods" (frenzy.yaml) — so it ALSO raises that
            // element's DoT tick bracket (1 + element bonuses). Recorded
            // once, from the direct part's pass.
            if let Some(eb) = elem_bonus.as_deref_mut() {
                if let Some(x) = eb.iter_mut().find(|(a, _)| *a == t) {
                    x.1 += bonus;
                } else {
                    eb.push((t, bonus));
                }
            }
        }
        (elements::combine(&physical, &input), modified_base)
    };

    let (damage, modified_base) =
        build(&base.base_vector.scale(charge_scale), Some(&mut elem_bonus));
    // The radial part (Laetum Incarnon's 300 Radiation explosion): its own
    // base vector, crit and status stats, modded by the same buckets.
    let a_resolved = |r: &RadialBase| {
        // THE EXPLOSION RIDES THE CHARGE TOO. "Damage dealt by the plasma bomb
        // is directly proportional to the amount of ammo consumed" — the bomb
        // IS the explosion on this weapon, and the direct hit is the smaller
        // half of it.
        let (rd, rmb) = build(&r.base_vector.scale(charge_scale), None);
        ResolvedRadial {
            blast_kind: r.blast_kind,
            damage: rd,
            modified_base: rmb,
            // The post-mod flat layer (Elemental Excess) is a WEAPON stat
            // change, so the explosion takes it too.
            crit_chance: (r.base_crit_chance * (1.0 + cc) + (base.post_mod_crit_chance + post_mod_cc_extra)).max(0.0),
            crit_damage: r.base_crit_damage * (1.0 + cd),
            base_crit_chance: r.base_crit_chance,
            base_crit_damage: r.base_crit_damage,
            status_chance: (r.base_status_chance * (1.0 + sc) + base.post_mod_status_chance)
                .max(0.0),
            base_status_chance: r.base_status_chance,
            forced_procs: r.forced_procs,
            // Blast RANGE mods scale the radius; the falloff FLOOR is
            // unchanged ("Only mods that increase the explosion radius change
            // how far the falloff reaches; they do not change the floor").
            // THE BUCKET REACHES MOST EXPLOSIONS AND NOT ALL OF THEM. The
            // Shedu's "cannot benefit from Firestorm (Primed) despite being
            // area of effect" is the roster's first exception, and it is worth
            // a branch rather than a comment because Primary Compression pays
            // per metre of this number.
            // …AND A SLAM'S OWN RADIUS CARD. Seismic Slam is `+100% Slam
            // Radius`, which is a different bucket from the blast-radius mods
            // above: those are Firestorm and Fulmination, which no melee weapon
            // can hold, and this one pays on a slam and on nothing else.
            radius_m: r.radius_m
                * if r.takes_blast_radius_mods { 1.0 + br } else { 1.0 }
                * if r.blast_kind == crate::model::BlastKind::Slam {
                    1.0 + base.evo_slam_radius_bonus
                } else {
                    1.0
                },
            falloff_start_m: r.falloff_start_m
                * if r.takes_blast_radius_mods { 1.0 + br } else { 1.0 },
            falloff_reduction: r.falloff_reduction,
            takes_condition_overload: r.takes_condition_overload,
            takes_multishot: r.takes_multishot,
            co_base: r.co_base_pair(),
        }
    };
    let radial = base.radial.as_ref().map(&a_resolved).map(|mut r| {
        if r.blast_kind == crate::model::BlastKind::Slam {
            r.radius_m *= SLAM_HEIGHT_RADIUS_SCALE;
        }
        r
    });
    // THE WEAPON'S OWN SLAM, through the same buckets — it is the same attack
    // part seen from a different swing, and a combo that ends on one should not
    // read a different Serration from the swing it ends.
    let slam = base.slam.as_ref().map(&a_resolved);
    // THE BOMBLETS, both halves through the same buckets: they are the weapon's
    // damage arriving in a smaller package, so a Serration the shell reads is a
    // Serration they read.
    let cluster = base.cluster.as_ref().map(|c| ResolvedCluster {
        count: c.count,
        contact: a_resolved(&c.contact),
        blast: a_resolved(&c.blast),
    });

    // The lingering FIELD (Torid's Toxin cloud): its own base vector, crit and
    // status stats, through the SAME mod buckets — three patch notes settle
    // that ("Fixed Torid gas clouds not receiving damage buffs from mods";
    // "…the Torid's gas cloud not allowing for criticals"). Tick rate and
    // duration are NOT mod-scaled: fire-rate mods change shots per second, not
    // the cloud's own clock, and the cloud is not a status effect so status
    // duration does not reach it either.
    // A MOD MAY LEAVE A FIELD ON A WEAPON THAT HAS NONE (Nightwatch Napalm),
    // and its radius is a share of the BLAST rather than a number of its own —
    // DE's card: "across 90% of the explosion area". Area, not radius, so the
    // radius is `sqrt(fraction)` of the blast's, read AFTER blast-radius mods
    // so that "Firestorm increases the effective area" needs no arm of its own.
    //
    // The weapon's own field wins if it has one: nothing in the roster is in
    // that position, and a mod silently replacing an attack part would be the
    // worse answer if one ever is.
    // Read off the weapon's OWN blast, not the resolved one: the field's radius
    // goes through `* (1 + br)` a few lines down like every other field, so
    // taking the modded radial here would grow it twice.
    let granted = granted_lingering.filter(|_| base.lingering.is_none()).map(|f| {
        let blast = base.radial.as_ref().map_or(0.0, |r| r.radius_m);
        // THE VALENCE REACHES IT, and it reaches the BASE rather than the
        // base-damage bucket. Nightwatch Napalm on a Kuva Ogris: 150 a tick
        // becomes 240 on a 60% roll, and 612 with +155% base damage — which is
        // `240 x 2.55` and not `150 x 3.15`, so the two orders are told apart
        // by the measurement rather than assumed.
        //
        // A GRANTED part cannot be reached by `apply_valence`: the mod is
        // resolved here, long after the weapon's own vectors were scaled. This
        // is that gap closed, and `base.lingering` — a weapon's OWN field — is
        // scaled over there beside the radial, for the day an adversary weapon
        // carries one.
        LingeringBase {
            radius_m: blast * granted_area_fraction.max(0.0).sqrt(),
            base_vector: f.base_vector.scale(1.0 + base.valence_bonus),
            ..f.clone()
        }
    });
    let lingering = base.lingering.as_ref().or(granted.as_ref()).map(|f| {
        // …AND A FIELD MAY BE OUTSIDE THOSE BUCKETS, which is a fact about the
        // field rather than about fields. Nightwatch Napalm's fire takes the
        // base-damage bucket and nothing else — no element goes near it, and no
        // status mod moves its 68% — so both flags exist and both default to
        // the Torid's YES.
        let (fd, fmb) = if f.elemental_mods_apply {
            build(&f.base_vector, None)
        } else {
            let scaled = f.base_vector.scale(1.0 + base_damage);
            let total = scaled.total();
            (scaled, total)
        };
        ResolvedLingering {
            damage: fd,
            modified_base: fmb,
            // A FIELD THAT CANNOT CRIT TAKES NEITHER BUCKET AND NO FLAT ADD.
            // Leaving the chance at zero is not enough on its own: the crit
            // DAMAGE bucket still multiplied a 1.0 up to 2.2, and a post-mod
            // additive chance would then have something to spend it on.
            crit_chance: if f.can_crit {
                (f.base_crit_chance * (1.0 + cc) + (base.post_mod_crit_chance + post_mod_cc_extra))
                    .max(0.0)
            } else {
                0.0
            },
            crit_damage: if f.can_crit { f.base_crit_damage * (1.0 + cd) } else { 1.0 },
            status_chance: if f.status_mods_apply {
                (f.base_status_chance * (1.0 + sc) + base.post_mod_status_chance).max(0.0)
            } else {
                f.base_status_chance
            },
            base_crit_chance: f.base_crit_chance,
            base_crit_damage: f.base_crit_damage,
            base_status_chance: f.base_status_chance,
            tick_rate: f.tick_rate,
            duration_seconds: f.duration_seconds,
            first_tick_delay_seconds: f.first_tick_delay_seconds,
            forced_procs: f.forced_procs,
            radius_m: f.radius_m * (1.0 + br),
            falloff_start_m: f.falloff_start_m * (1.0 + br),
            falloff_reduction: f.falloff_reduction,
            stacking: f.stacking,
            takes_condition_overload: f.takes_condition_overload,
        }
    });

    // MOD SET bonuses. Every equipped member adds its own share — the set
    // does not have to be complete to be worth carrying (wiki: 5% per
    // Vigilante mod, 30% at six). A mod cannot be equipped twice, so counting
    // members is just counting the mods that name the set.
    let crit_tier_upgrade_chance: f64 = mods
        .iter()
        .filter_map(|m| m.set)
        .filter_map(crate::data::mod_sets::set_def)
        .filter(|s| s.kind == crate::data::mod_sets::SetBonusKind::CritTierUpgrade)
        .map(|s| s.per_mod)
        .sum();
    // …AND THE GLADIATOR SET READS THE COMBO COUNTER, into Blood Rush's own
    // bracket: `base_cc x [1 + mods + this x (combo - 1)]`. Three of its six
    // members are Warframe mods, so a weapon build alone reaches 30% here and
    // the ceiling is stated in `data/mod_sets/gladiator.yaml` rather than
    // pretended away — the same disclosure the Vigilante set carries.
    let set_crit_per_combo: f64 = mods
        .iter()
        .filter_map(|m| m.set)
        .filter_map(crate::data::mod_sets::set_def)
        .filter(|s| s.kind == crate::data::mod_sets::SetBonusKind::CritChancePerCombo)
        .map(|s| s.per_mod)
        .sum();

    // DAMAGE FALLOFF, with Projectile Speed moving the whole window — see
    // [`Falloff`] for the two wiki lines that make this the one bucket
    // Projectile Speed pays into. A negative roll can only shorten the window,
    // never invert it, so the scale is floored at zero.
    // THE CONE, with the accuracy mods narrowing it. Wiki (`Accuracy`),
    // verbatim: *"Bonuses that increase accuracy decrease the deviation
    // (spread) of a shot"* — and accuracy is `100 / spread`, so a +30% accuracy
    // card divides the angle by 1.3 rather than subtracting from it.
    //
    // A NEGATIVE roll widens the cone and cannot invert it: the divisor is
    // floored just above zero, and a pinpoint attack (0 / 0) stays pinpoint
    // under any bonus, which is the arithmetic agreeing with the weapon.
    let spread = base.spread.map(|s| {
        let bonus = indirect
            .iter()
            .find(|(st, _)| *st == IndirectStat::Accuracy)
            .map_or(0.0, |(_, v)| *v);
        let k = (1.0 + bonus).max(0.05);
        // ADDED SPREAD LANDS OUTSIDE THE DIVISOR, which is the whole of what
        // "not affected by bonuses that increase accuracy" means: a Twitch on
        // the same build narrows the weapon's own cone and leaves this alone.
        Spread {
            min_deg: s.min_deg / k + added_spread,
            max_deg: s.max_deg / k + added_spread,
        }
    });

    // BEAM LENGTH: the weapon's reach, and the ONE bracket the wiki states for
    // it — VERBATIM (Galvanized Acceleration, Notes): *"Sinister Reach will not
    // be affected by this mod as it gives a flat beam length increase AFTER
    // percent bonuses."* So the flat metres land outside the percentage, which
    // is the opposite of how every damage bucket in this engine works and is
    // worth 3.6 m on a 20 m beam carrying both.
    //
    // INFINITY IS THE ORDINARY CASE and needs no arm of its own: a weapon that
    // declares no range keeps one under any bonus, which is also what makes
    // "only affects the Panthera's alt-fire" and "only the Phantasma's primary
    // fire" fall out of the data rather than being transcribed — the entry
    // those notes exclude is the one with no range to extend.
    //
    // A CHAIN IS NOT THE BEAM. "Weapon Range Mods (like Sinister Reach) no
    // longer affect the distance of chains for chainable Beam weapons (like the
    // Amprex). Main Beam distance is still affected by them" (Update 22.14), and
    // `chain_range_m` is a separate field this never touches.
    let beam_range_m = {
        let flat: f64 = indirect
            .iter()
            .filter(|(s, _)| *s == IndirectStat::BeamRange)
            .map(|(_, v)| *v)
            .sum();
        let pct: f64 = indirect
            .iter()
            .filter(|(s, _)| *s == IndirectStat::BeamRangePercent)
            .map(|(_, v)| *v)
            .sum();
        // …AND MELEE REACH, which is the same quantity by another name: a
        // hammer's 2.5 m is the distance a body has to stand within to be hit,
        // exactly as a beam's range is. Reach and Primed Reach add METRES —
        // DE's own cards say `+2.3 Range` and `+3 Range` — so they join `flat`
        // rather than `pct`, and on a hammer Primed Reach more than doubles it.
        (base.range_m * (1.0 + pct) + flat + melee_range_add + base.evo_melee_range_m).max(0.0)
    };

    // PUNCH THROUGH: the weapon's own plus every grant, in metres of material.
    //
    // AN AoE ATTACK GETS NEITHER, and the punch-through page states both
    // halves: *"weapon projectiles with an area of effect (AoE) component will
    // not Punch Through enemies or level geometry at all"*, and *"Projectile
    // AoE weapons cannot have their Punch Through stat modified"*.
    //
    // The page's *"very few exceptions"* announce themselves in the DATA: an
    // exception is an entry whose infobox carries a punch-through figure, and
    // the roster has one (the Vulcax's 2 m). "AN AREA OF EFFECT COMPONENT" IS
    // BOTH SHAPES this engine models: the Torid is a LINGERING cloud with no
    // `radial:`, so a rule naming only radials would let it take Primed Shred.
    // THE ENTRY DECIDES and the class rule is only the fallback: the Torid's
    // Incarnon form is a BEAM with a damage radius — neither `radial:` nor
    // `lingering:` — and its page says "Punch Through mods have no effect on
    // the behavior of the beam".
    // A TERMINAL BLAST IS NOT AN AREA-OF-EFFECT PROJECTILE for this rule: the
    // page's sentence is about a round that goes off on contact, and one that
    // bores through and detonates where the FLIGHT ends is the opposite case.
    // See `data::weapons::BlastKind` and MEASUREMENTS M50.
    let contact_blast = base
        .radial
        .as_ref()
        .is_some_and(|r| r.blast_kind != crate::model::BlastKind::Terminal);
    let takes_mods = base
        .punch_through_mods
        .unwrap_or(!contact_blast && base.lingering.is_none());
    let punch_through_m = if !takes_mods {
        base.punch_through_m
    } else {
        base.punch_through_m
            + gate(GatedGrant::PunchThrough)
            + indirect
                .iter()
                .filter(|(s, _)| *s == IndirectStat::PunchThrough)
                .map(|(_, v)| *v)
                .sum::<f64>()
    }
    .max(0.0);

    let falloff = base.falloff.as_ref().map(|f| {
        let ps = 1.0
            + indirect
                .iter()
                .find(|(s, _)| *s == IndirectStat::ProjectileSpeed)
                .map_or(0.0, |(_, v)| *v);
        let ps = ps.max(0.0);
        // DE's `Reduction` is the share REMOVED at the far end, so the share
        // kept is its complement: Hek's 0.8 is the page's "100% to 20%".
        Falloff { start_m: f.start_m * ps, end_m: f.end_m * ps, keep: 1.0 - f.reduction }
    });

    ResolvedPanel {
        class: base.class,
        slot: base.slot,
        mod_pools: base.mod_pools,
        punch_through_m,
        projectile_width_m: base.projectile_width_m,
        range_m: beam_range_m,
        damage,
        radial,
        cluster,
        spread,
        falloff,
        lingering,
        slash_on_crit,
        crit_tier_upgrade_chance,
        continuous: base.continuous,
        field_duration_on_empty_reload: base.field_duration_on_empty_reload,
        multishot_beyond_range: base.multishot_beyond_range,
        multishot_on_last_round: ms_last_round,
        // Locked the same way: an Acuity says "set to its default ignoring
        // other bonuses", and a bigger base for one burst is a bonus.
        base_multishot_on_last_round: if locked_stat("multishot") {
            0.0
        } else {
            base.base_multishot_on_last_round
        },
        multishot_ammo_bonus: base.multishot_ammo_bonus,
        gauge_form: base.gauge_form,
        // Blast Range reaches the beam's sphere too — wiki: "The 2.3 meter
        // damage radius from the point of impact CAN benefit from Firestorm
        // (Primed)." Same `br` bucket the radial and the field use.
        beam: base.beam.map(|b| BeamGeometry {
            damage_radius_m: b.damage_radius_m * (1.0 + br),
            ..b
        }),
        ricochet: base.ricochet,
        unaimed_headshot_chance: base.unaimed_headshot_chance,
        // A FIRE-RATE MOD SHORTENS THE WIND-UP, the same bucket and the same
        // reciprocal application the throw animation and a charge draw get.
        windup_seconds: base.windup_seconds / (1.0 + fr).max(1e-9),
        // ---- MELEE ------------------------------------------------------
        //
        // THE SCRIPT COMES ACROSS AT 1.0x, which is what the stance publishes,
        // and the FIGHT divides by the live attack speed.
        //
        // Scaling it here by `1 + fr` is wrong twice over: it misses the
        // WEAPON's own attack speed (a Magistar swings at 0.833x, so its 3.00 s
        // combo takes 3.60 s — 66 swings a minute against 80), and it freezes
        // the answer at build time where no live buff can reach it.
        // `panel.fire_rate` is the whole attack speed in one number, so the
        // loop divides by it as a charge weapon divides its draw.
        // THE WIND-UP IS SHORTENED HERE and the DELAY is not, which is the two
        // clocks made concrete: this bucket is Killing Blow's and the two
        // evolutions', and the delay divides by the live attack speed in the
        // fight loop where a buff can still reach it.
        // AND THE STANCE DECIDES WHICH SCRIPT IT IS. A stance mod publishes a
        // combo per FORM, and installing one REPLACES the entry's own — so the
        // same weapon in the same mode is a different sequence of swings under
        // Crushing Ruin and under Shattering Storm. The entry's own script is
        // the UNSTANCED fallback, which is what an empty stance slot gives you.
        combo_script: mods
            .iter()
            .find_map(|m| m.stance)
            .and_then(|c| c.iter().find(|(f, _)| *f == base.form.id()).map(|(_, h)| h))
            .map_or_else(|| base.combo_script.clone(), |h| h.to_vec())
            .iter()
            .map(|h| crate::model::ComboHit {
                windup_seconds: h.windup_seconds
                    / (1.0 + windup_speed + base.evo_heavy_windup_speed).max(1e-9),
                ..h.clone()
            })
            .collect(),
        // FOLLOW THROUGH IS NOT MODDABLE — no card in the pool moves it, and
        // the wiki's own table is per weapon class — but an EVOLUTION is
        // (Crushing Verdict's `+40% Follow Through`), so the entry's value is
        // the base and a Genesis scales it.
        follow_through: base
            .follow_through
            .map(|f| f * (1.0 + base.evo_follow_through_bonus)),
        spends_combo: base.spends_combo,
        // *"Melee Combo duration cannot be reduced below 0.1 seconds"* (wiki),
        // which is a floor rather than a clamp on the bucket: a negative mod
        // could otherwise stop the counter existing at all.
        // SECONDS FIRST, THEN THE RELATIVE CARD, which is the order the cards
        // themselves imply: Body Count adds twelve seconds to the weapon's
        // five, and Corrupt Charge halves what you have. The wiki's floor is
        // applied last — *"Melee Combo duration cannot be reduced below 0.1
        // seconds"* — so a build stacking penalties still has a counter.
        // ZERO IS ITS OWN RULE: *"A zero or negative combo duration PREVENTS
        // INCREASING the combo counter"*, which is a harder stop than a 0.1 s
        // clock and is reported beside it in `combo_frozen`. A melee riven's
        // Combo Duration malus is what reaches it — -8.2 s at disposition 1.35
        // against a five-second weapon.
        combo_duration_seconds: ((base.combo_duration_seconds + combo_duration_add)
            * combo_duration_mult)
            .max(0.1),
        combo_frozen: (base.combo_duration_seconds + combo_duration_add) * combo_duration_mult
            <= 0.0,
        // STRAIGHT THROUGH: no mod touches it, and the caller is what needs it
        // — resolving the same weapon a second time without the tier that
        // states it is how the un-armed half of the fight is built.
        melee_incarnon: base.melee_incarnon,
        // …PLUS WHAT A GENESIS OPENS THE COUNTER AT. The Magistar's Incarnon
        // Form is +30, which is 0.75 s of refill against a 0.8 s wind-up.
        initial_combo: initial_combo + base.evo_initial_combo,
        // *"stacks additively and is capped at 90%; with both Focus Energy and
        // Reflex Coil, 10% of the combo counter will still be consumed"*.
        heavy_attack_efficiency: heavy_efficiency.clamp(0.0, 0.9),
        slam,
        // THE HEAVY'S WIND-UP TAKES THE SAME BUCKET the script's does, because
        // it is the same clock: a Tennokai swing is a heavy attack, and the
        // window's own speed bonus is on top of whatever the build bought.
        heavy: base.heavy.map(|h| crate::model::HeavyAttack {
            windup_seconds: h.windup_seconds
                / (1.0 + windup_speed + base.evo_heavy_windup_speed).max(1e-9),
            ..h
        }),
        tennokai: Tennokai {
            // …AND THERE IS NO CHARGE AT ALL. Measured: the window's swing goes
            // out on the press. Not the build's bucket and not the class's
            // either — see the field, and `notes: tonfa_heavy_timing` for the
            // two clocks a heavy is made of.
            windup_seconds: 0.0,
            ..tk
        },
        crit_chance_per_combo: cc_per_combo + set_crit_per_combo,
        status_chance_per_combo: sc_per_combo,
        combo_count_chance,
        combo_count_chance_on_lifted,
        combo_gain_chance,
        combo_count_on_slam_hit: base.evo_combo_count_on_slam_hit,
        status_chance_on_lifted,
        heavy_attack_damage: heavy_damage,
        slam_damage,
        no_magazine: base.no_magazine,
        // THE ORB'S GEOMETRY, MODDED — and one of the three distances is NOT.
        //
        // The orb's REACH and its detonation RADIUS take the blast-radius
        // bucket; a chain HOP does not, and stays at the six metres the page
        // gives it however much Fulmination is on the build. That asymmetry is the whole reason the hop's range is
        // carried as its own field rather than read off the reach: the two are
        // the same number today and only one of them moves.
        //
        // MULTISHOT lands in the chain COUNT rather than making a second orb,
        // which is why it is added here, where the resolved multishot is,
        // instead of in the fight.
        orb: base.orb.map(|o| ResolvedOrb {
            fuse_seconds: o.fuse_seconds,
            strike_interval_seconds: o.strike_interval_seconds,
            strike_radius_m: o.strike_radius_m * (1.0 + br),
            launch_speed_mps: o.speed_mps,
            speed_after_contact_mps: o.speed_after_contact_mps,
            // FLOOR, not a coin — measured (M63). A panel reading x2.1 hits
            // six bodies every strike, never five and never seven, which is
            // what tells `3 x multishot` apart from `multishot + 2`.
            chain_bodies: (o.chain_bodies_per_multishot
                * base.base_multishot
                * (1.0 + evo_ms_bonus + multishot))
                .floor()
                .max(1.0) as u32,
            chain_range_m: o.chain_range_m,
            chain_damage_per_hop: o.chain_damage_per_hop,
            // A FIRE-RATE MOD SPEEDS THE THROW UP, which is
            // the same bucket and the same reciprocal application a charge
            // draw gets: the animation is divided by what the bucket did.
            throw_seconds: o.throw_seconds / (1.0 + fr).max(1e-9),
            recovery_seconds: o.recovery_seconds / (1.0 + fr).max(1e-9),
            unaimed_headshot_chance: base.unaimed_headshot_chance,
        }),
        // THE RECHARGE CLOCK, unmodded — see `ResolvedMeter`.
        meter: base.meter.map(|m| ResolvedMeter {
            seconds_to_fill: m.seconds_to_fill,
            seconds_per_hit: m.seconds_per_hit,
            seconds_per_ammo_pickup: m.seconds_per_ammo_pickup,
        }),
        modified_base,
        // Elemental Excess adds its crit/status FLAT, after the mod
        // multiply (wiki) — a different layer from the base-stat one.
        crit_chance: resolved_cc,
        // A GATED "+Nx Base Critical Damage Multiplier" joins the BASE, so the
        // crit-damage mods multiply it — which is what "Base" earns on the card.
        crit_damage: (base.base_crit_damage + prelude_cd + gate(GatedGrant::BaseCritDamage))
            * (1.0 + cd),
        // What the line above added, in the same post-mod units, so the sim
        // subtracts exactly what was granted — including through a crit-damage
        // LOCK, which zeroes `cd` for both expressions at once.
        crit_multiplier_below_crit_chance: base
            .crit_multiplier_below_crit_chance
            .filter(|_| prelude_cd > 0.0)
            .map(|(_, below)| (prelude_cd * (1.0 + cd), below)),
        // No upper clamp: status chance ABOVE 100% is meaningful (a
        // guaranteed proc plus an extra roll) — DT resolves to 129%.
        status_chance: ((base.base_status_chance + sc_from_cc) * (1.0 + sc)
            + base.post_mod_status_chance)
            .max(0.0),
        base_crit_chance: base.base_crit_chance,
        base_crit_damage: base.base_crit_damage,
        base_status_chance: base.base_status_chance,
        fire_rate: base.base_fire_rate * (1.0 + fr + evo_fr_bonus),
        // A charged weapon's fire-rate bonuses DIVIDE the draw instead of
        // multiplying a rate (wiki Fire Rate: on charge weapons the bonus
        // "decreases the charge time"). Same bucket, reciprocal application —
        // and on a bow the bucket already carries the x2.
        // The DRAW's divisor. Charge-rate bonuses always count; fire-rate
        // ones count only where the weapon lets them (an Arch-Gun does not).
        charge_ammo_per_second: base.charge_ammo_per_second,
        sustained_fire_rate: base.sustained_fire_rate,
        // Whichever one the field above resolved from — the weapon's if it has
        // one, otherwise the mod's.
        lingering_base: base.lingering.clone().or(granted),
        battery: base.battery,
        recharge_per_second: base.recharge_per_second,
        echo_multiplier: base.echo_multiplier,
        // A MAGAZINE-EATING CHARGE STATES ITS OWN TIME. `magazine / rate`
        // seconds, because that is how long the magazine takes to be spent —
        // so a magazine mod lengthens the charge as well as paying for the
        // damage it buys. Charge-speed bonuses still divide it: they raise the
        // RATE, which is the same thing as shortening the draw.
        charge_seconds: match (base.charge_ammo_per_second, base.charge_seconds) {
            (Some(rate), _) if rate > 0.0 => {
                let from_rate = if base.fire_rate_shortens_draw {
                    fr + evo_fr_bonus
                } else {
                    0.0
                };
                Some(mag_size / rate / (1.0 + cr + from_rate).max(1e-9))
            }
            _ => base.charge_seconds.map(|c| {
            let from_rate = if base.fire_rate_shortens_draw {
                fr + evo_fr_bonus
            } else {
                0.0
            };
            c / (1.0 + cr + from_rate).max(1e-9)
            }),
        },
        // …AND IT COSTS THE WHOLE MAGAZINE. "Charging consumes ammo, up to a
        // full magazine on full charge" — so the shot's price is the magazine
        // it just spent, which is also why a bigger magazine is not free.
        ammo_cost: match base.charge_ammo_per_second {
            Some(rate) if rate > 0.0 => mag_size,
            _ => base.ammo_cost,
        },
        headshot_bonus_multiplicative: base.headshot_bonus_multiplicative,
        charge_cadence: base.charge_cadence,
        // A fire-rate bonus shortens the gap WITHIN a burst as well as the gap
        // between bursts (wiki: it "affect[s] both the speed of the burst as
        // well as the time between bursts"), which is what makes a burst
        // weapon scale linearly like every other gun.
        //
        // The `.max(1.0)` is the wiki's one exception, stated outright: "Burst
        // Delay is not affected by net negative Fire Rate bonuses." So Critical
        // Delay stretches the gap between bursts and leaves the burst itself
        // alone — the weapon keeps more of its rate than the card's number
        // suggests, and only a burst weapon does that.
        burst: base.burst.map(|b| crate::model::BurstSpec {
            count: b.count,
            delay_seconds: b.delay_seconds / (1.0 + fr + evo_fr_bonus).max(1.0),
        }),
        // THE SCOPE'S OWN HEADSHOT BONUS joins this bracket rather than
        // multiplying it — *"These zoom buffs ... generally stack additively
        // with similar buffs from mods"* (wiki `Sniper Rifle` §Zoom Buffs) —
        // and it is paid only while aiming, for the same reason the combo is.
        // `tenno` here is already the aim-corrected one, so an Incarnon form
        // that cannot zoom pays nothing without knowing why.
        headshot_damage_bonus: base.headshot_damage_bonus
            + if tenno.state.aiming { base.scope_headshot_damage } else { 0.0 },
        noncrit_bonus: base.noncrit_bonus,
        stacking_buffs: base
            .stacking_buffs
            .iter()
            .chain(mod_buffs.iter())
            .filter(|b| !locked_stat(b.grant.locked_stat()))
            // A LOCKED STAT TAKES ITS BUFFS WITH IT, and they go rather than
            // going quiet: a card that opens, stacks and grants nothing is a
            // measurement a player cannot make (the rule `check_equip_rules`
            // asserts — "a buff whose only grant is that stat is not offered").
            // Every one of these grants exactly one stat, so the filter is the
            // whole rule.
            .map(|b| StackingBuff {
                per_stack: match b.grant {
                    // ONE conversion: a FireRate buff arrives as a fraction of
                    // the base rate and leaves as the absolute rate it is worth,
                    // because the sim adds it inside the bracket fire-rate mods
                    // live in.
                    BuffGrant::FireRate => base.base_fire_rate * b.per_stack,
                    // …and the same trick for a FLAT base-damage add, which
                    // arrives as the number on the card and leaves as the share
                    // of the base-damage bucket worth the same thing. The mods
                    // are in by now, so `(1 + base_damage) / base` is a constant — and it
                    // is what preserves the one difference that matters: this
                    // is not diluted by Serration, because the equivalent share
                    // grows with `base_damage` exactly as fast as the bucket does.
                    BuffGrant::FlatBaseDamage => {
                        let unmodded = base.base_vector.total();
                        if unmodded > 0.0 {
                            b.per_stack * (1.0 + base_damage) / unmodded
                        } else {
                            0.0
                        }
                    }
                    // …and once more for a BASE crit-damage add, which arrives
                    // as the "+1x" on the card and leaves as the post-mod
                    // multiplier it is worth. Same reason as the two above: the
                    // sim adds it to a total the mods are already inside.
                    BuffGrant::BaseCritDamage => b.per_stack * (1.0 + cd),
                    _ => b.per_stack,
                },
                // …and the same conversion for HOW MANY a trigger grants: 0
                // means "one per shell loaded", which is the modded magazine.
                // It cannot be resolved earlier — the magazine does not exist
                // until the mods are in — and it is what makes a magazine mod
                // a trade rather than a free stat on a by-round reloader.
                stacks_per_trigger: if b.stacks_per_trigger == 0 {
                    mag_size.max(1.0) as u32
                } else {
                    b.stacks_per_trigger
                },
                per_shell: b.stacks_per_trigger == 0,
                // NO OPENING STACKS. It briefly seeded one reload's worth,
                // which was wrong for the same reason Secondary Enervate opens
                // at zero: this is a pile you can be caught without, and the
                // fight is what earns it. An empty magazine takes the whole
                // thing, so "how many you walk in with" is not a state the
                // weapon has — it is a state the last few seconds decided.
                initial_stacks: b.initial_stacks,
                ..*b
            })
            .collect(),
        multishot: base.base_multishot * (1.0 + evo_ms_bonus + multishot),
        base_multishot: base.base_multishot,
        // Magazine capacity: +% of base, floored to whole rounds (in-game).
        // A charge-backed Incarnon magazine is a fixed resource OUTSIDE the
        // ammo system — magazine mods are inert, so it never scales.
        magazine_size: mag_size,
        // Reserve: +% of base, the same shape as the magazine, and floored
        // for the same reason — a fraction of a round is not a round. The
        // bonus is the Ammo Reserve bucket the panel already shows, which is
        // where Ammo Chain and a riven's Ammo Maximum land.
        ammo_reserve: (base.ammo_reserve
            * (1.0
                + indirect
                    .iter()
                    .find(|(s, _)| *s == IndirectStat::AmmoMax)
                    .map_or(0.0, |(_, v)| *v)))
        .floor(),
        has_reserve: base.has_reserve,
        ammo_pickup: base.ammo_pickup,
        ammo_conversion: indirect
            .iter()
            .find(|(s, _)| *s == IndirectStat::AmmoConversion)
            .map_or(0.0, |(_, v)| *v),
        no_resupply: base.no_resupply,
        super_crit_on_status: base.super_crit_on_status,
        weakpoint_stacks: base.weakpoint_stacks,
        spawn_on_kill: base.spawn_on_kill,
        kill_streak_summon: base.kill_streak_summon,
        beam_ramp_floor: base.beam_ramp_floor,
        applies_microwave: base.applies_microwave,
        independent_procs: base.independent_procs,
        forced_procs: base.forced_procs.clone(),
        multishot_adds_damage: base.multishot_adds_damage,
        // ONE RESOLVE PER ELEMENT. The recursion terminates because the clone
        // clears `pellet_elements` — and it has to be a re-resolve rather than
        // a retyped result, because an innate element enters the elemental
        // HIERARCHY (MECHANICS §3 rule 2) and a finished vector cannot say
        // which of its Blast the mods put there.
        //
        // The whole base vector becomes `total` of the named element, which is
        // what "each projectile deals one of six elements" means: the missile
        // is that element, not a blend with it.
        pellet_damage: base
            .pellet_elements
            .iter()
            .map(|e| {
                let mut one = base.clone();
                one.pellet_elements = Vec::new();
                one.base_vector = crate::rules::damage::DamageVector::new()
                    .with(*e, base.base_vector.total());
                if let Some(r) = one.radial.as_mut() {
                    r.base_vector =
                        crate::rules::damage::DamageVector::new().with(*e, r.base_vector.total());
                }
                let p = resolve_for(&one, mods, policy, tenno);
                (p.damage, p.radial.map_or_else(DamageVector::new, |r| r.damage))
            })
            .collect(),
        attractor_seconds: base.attractor_seconds,
        tendril_max: base.tendril_max,
        tendril_range_m: base.tendril_range_m,
        tendril_acquire_deg: base.tendril_acquire_deg,
        // THE COUNTER IS THE AIMED FIGHT'S, and the mod's seconds ride the
        // weapon's own window — see `ModEffect::ComboDuration`. A hip-fired
        // build has no counter at all, so a Harkonar Scope on one adds seconds
        // to nothing, which is what the mechanic's own gate says and not this
        // mod's doing.
        sniper_combo: if tenno.state.aiming {
            base.sniper_combo.map(|c| crate::model::SniperCombo {
                seconds: c.seconds + combo_seconds_bonus,
                ..c
            })
        } else {
            None
        },
        crit_chance_per_tendril: per_tendril_cc,
        crit_chance_per_hit,
        acid_shells: acid_seen.then_some(acid),
        sc_per_tendril: per_tendril_sc,
        magazine_refill_on_kill: mag_refill,
        syndicate_radial,
        // A BY-ROUND RELOAD IS PAID PER SHELL, so it grows with the modded
        // magazine. `mag_size` is the same number the magazine field above
        // reports, which is what keeps the two from disagreeing.
        reload_seconds: match base.by_round_reload {
            Some((start, per, end)) => (start + per * mag_size + end) / (1.0 + rl),
            None => base.base_reload / (1.0 + rl),
        },
        reload_bonus: rl,
        base_damage_bonus: base_damage,
        // A LOCKED base damage takes this with it, the same rule the live
        // buffs follow: a lock is "set to its default ignoring other bonuses".
        base_damage_below_half_health: if locked_stat("base_damage") { 0.0 } else { base.base_damage_below_half_health },
        // CONVERTED, and each by its OWN bucket: "+40% Base Critical Chance" is
        // multiplied by the crit-chance mods and "+2x Base Critical Damage
        // Multiplier" by the crit-damage ones, exactly as the unconditional
        // `flat_base_crit_*` grants beside them are.
        crit_chance_on_undamaged: if locked_stat("critical_chance") { 0.0 } else { base.crit_chance_on_undamaged * (1.0 + cc) },
        crit_damage_on_undamaged: if locked_stat("critical_damage") { 0.0 } else { base.crit_damage_on_undamaged * (1.0 + cd) },
        co_behavior: base.co_behavior,
        compression,
        co_base: base.co_base_pair(),
        unswung_fraction: base.unswung_fraction(),
        co_per_type: co,
        co_stack,
        multishot_stack,
        crit_chance_on_headshot,
        on_weakpoint,
        crit_chance_stack,
        status_damage_multiplier: 1.0 + status_damage,
        status_duration_multiplier: 1.0 + sdur,
        elem_dot_bonus: elem_bonus.into_iter().map(|(t, v)| (t, 1.0 + v)).collect(),
        indirect,
        faction_damage: faction_bonus,
        weakpoint_damage: wp_dmg,
        headshot_multiplier: base.headshot_multiplier,
        // RELATIVE; direct-head only, so the sim uses the direct base.
        weakpoint_crit_chance_relative: wp_cc,
        bodyshot_crit_chance_multiplier: base.bodyshot_crit_chance_multiplier,
        consecutive_hit_damage: consecutive_hit.or(base.consecutive_hit_damage),
        consecutive_hit_radial_only: base.consecutive_hit_radial_only,
        // OFF ON A CONTINUOUS WEAPON AND ON AN INCARNON FORM, both the mod's
        // own words. `base.form` is the entry being resolved, so an Incarnon
        // half of a cycle drops it while the base half keeps it — which is
        // exactly what "does not have an effect on any Incarnon fire modes,
        // whether on the last shot in their magazine or if activated with one
        // bullet left in the primary mode's magazine" describes.
        last_round_damage: if base.continuous
            || base.form == crate::model::FormKind::Incarnon
        {
            0.0
        } else {
            last_round_damage
        },
        first_round_damage,
        // The card's numbers converted into POST-MOD units, so the sim adds one
        // term and never has to know about the status bucket. Same units trick
        // `BuffGrant::FlatBaseDamage` and `BaseCritDamage` take.
        derived_status_from_crit: base
            .base_status_from_crit
            .map(|(rate, cap)| (rate * (1.0 + sc), cap * (1.0 + sc), sc_from_cc * (1.0 + sc))),
        derived_crit_from_status: base
            .base_crit_from_status
            .map(|(rate, cap)| (rate * (1.0 + cc), cap * (1.0 + cc), cc_from_sc * (1.0 + cc))),
        round_restore_on_status: base.round_restore_on_status,
        instant_reload_on_kill: base.instant_reload_on_kill,
        magazine_growth_on_empty_reload: base.magazine_growth_on_empty_reload.map(|(per, max)| {
            // A charge-backed Incarnon magazine is outside the ammo system and
            // the mods never scale it — the same exception `mag_size` makes two
            // hundred lines up, so the grant follows the magazine it grows.
            let scaled = if base.gauge_form.is_some() { per } else { per * (1.0 + mag) };
            (scaled, max)
        }),
        crit_damage_on_kill,
        fire_rate_on_reload,
        rs_on_reload,
        armor_strip_per_puncture: base.armor_strip_per_puncture,
        strip_on_kill_in_range,
        ability_strength_bonus,
        ability_duration_bonus,
        instant_reload: base.instant_reload_on_headshot,
        headshot_streak: base.headshot_streak,
        crit_damage_below_status_count: base.crit_damage_below_status_count,
        base_damage_on_reload,
        base_damage_on_eximus_weakpoint,
        proc_conversion: proc_conv,
        // Reified Bane: the vector already carries the +14 (evolutions apply
        // before mods), so the buff opens FULL and the card scales it back.
        evo_base_damage: (base.reload_damage_buff > 0.0).then_some(EvoBdBuff {
            full: base.reload_damage_buff,
            without: (base.base_vector.total() - base.reload_damage_buff).max(0.0),
            max_stacks: 1,
            stacks: 1,
        }),
        evo_multishot: (evo_ms_bonus > 0.0 && evo_ms_stacks > 0).then_some(
            EvoMsBuff {
                full: base.base_multishot * evo_ms_bonus,
                max_stacks: evo_ms_stacks,
                // Full until a buff card says otherwise — it is permanent, so
                // "the count in play" starts at the count the panel resolved.
                stacks: evo_ms_stacks,
            },
        ),
        locked: disabled.to_vec(),
    }
}
