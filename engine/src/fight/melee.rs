use super::*;

/// POINTS ON THE MELEE COMBO COUNTER PER TIER above the first.
///
/// The wiki publishes the ladder rather than the formula — 2x at 20 hits, 3x at
/// 40, one more every 20 up to 12x at 220 — and this is that ladder as
/// arithmetic. Venka Prime's 13x and Dex Nikana's shortened 110 are the two
/// exceptions the page names and neither is in the roster; when one arrives it
/// states its own numbers rather than bending this.
pub(super) const MELEE_COMBO_POINTS_PER_TIER: f64 = 20.0;

/// The cap. *"you will also increment a Melee Combo Multiplier from 2x to 12x"*.
pub(super) const MELEE_COMBO_MAX: f64 = 12.0;

/// TENNOKAI'S BASE CHANCE, per landed direct hit.
///
/// *"Landing direct melee hits ... have a 15% chance of flashing a sword icon
/// resembling the Skana Prime on the reticle for 2 seconds"* (wiki, Melee).
pub(super) const TENNOKAI_BASE_CHANCE: f64 = 0.15;

/// …AND HOW LONG THE FLASH LASTS, from the same sentence. Opportunity's Reach
/// replaces it with 4.0.
pub(super) const TENNOKAI_WINDOW_SECONDS: f64 = 2.0;

/// POINTS A HEAVY ATTACK'S FLOOR REFILLS AT, per second.
///
/// *"Heavy attacks spend initial combo, which regenerates at a rate of 40 combo
/// points per second"* (wiki, Melee Combo). It is what makes a pure-heavy build
/// work at all: a Magistar in Incarnon Form carries +30, which is back inside
/// 0.75 s against a 1.07 s cycle, so every heavy lands at 2x rather than 1x.
pub(super) const INITIAL_COMBO_REGEN_PER_SECOND: f64 = 40.0;

/// THE MULTIPLIER THE COUNTER IS WORTH RIGHT NOW.
///
/// `1 + floor(points / 20)`, capped at 12. One at 0..19 points, which is the
/// wiki's own table read as a step function — and 1x is a real state rather
/// than "no combo": a heavy attack at 1x deals its class multiplier and nothing
/// more.
pub(super) fn melee_combo_multiplier(points: f64) -> f64 {
    (1.0 + (points / MELEE_COMBO_POINTS_PER_TIER).floor()).clamp(1.0, MELEE_COMBO_MAX)
}

/// THE COUNTER AS THIS SWING SEES IT: what was earned, or the floor, whichever
/// is higher.
///
/// The floor is `initial_combo` filling at 40 points a second since the last
/// heavy attack emptied the counter — so it is a FLOOR rather than a second
/// pool, which is what makes "spend it and it comes back" and "build on top of
/// it" the same number.
pub(super) fn melee_combo_points(held: f64, initial: f64, since_spend_seconds: f64) -> f64 {
    // IT REGENERATES FROM WHAT IS THERE, not from zero: *"Heavy attacks spend
    // initial combo, which REGENERATES at a rate of 40 combo points per
    // second"*. Heavy Attack Efficiency is what makes the difference visible —
    // it leaves points behind, and 40 a second climbs from those toward the
    // floor rather than racing them from nothing.
    //
    // ABOVE THE FLOOR IT ONLY WAITS. Points earned past the initial-combo value
    // are not regeneration's business; they stand until the counter's own clock
    // takes them.
    let regen = since_spend_seconds.max(0.0) * INITIAL_COMBO_REGEN_PER_SECOND;
    held.max(initial.min(held + regen))
}

/// KILLING BLOW'S BRACKET — what a `+X% Melee Damage on Heavy Attack` card is
/// worth, as a term in the BASE-DAMAGE bucket.
///
/// *"Damage bonus is additive to mods such as Pressure Point"* (wiki, Killing
/// Blow), so it is diluted by everything else in that bucket rather than
/// multiplying the finished swing. SEISMIC WAVE IS THE CONTRAST and the reason
/// the two cannot share a line: *"Slam damage bonus is multiplicative to base
/// damage (e.g. Pressure Point)"* (wiki, Seismic Wave). Both cards read "+X%
/// Melee Damage on <kind of attack>" and they land in different places.
///
/// Zero on every mode that does not spend the counter, which is what "heavy
/// attack" means here.
pub(super) fn heavy_attack_base_damage(active: &FightParams) -> f64 {
    if active.spends_combo { active.heavy_attack_damage } else { 0.0 }
}

/// HOW OFTEN A HEAVY MODE SWINGS, in seconds — and it is not always as soon as
/// it can.
///
/// A HEAVY ATTACK SPENDS THE COUNTER, so the swing is worth waiting for when
/// the counter is about to be worth more. The counter climbs in STEPS
/// (`1 + floor(points / 20)`, refilling at 40 a second), so the candidates are
/// the animation's own floor and each rung the initial-combo floor can still
/// reach — and waiting PAST a rung buys nothing, which is why "wait for the
/// full refill" is the intuitive rule and the wrong one.
///
/// IT IS DERIVED FROM SPENDING THE COUNTER, not from the mode: a heavy slam
/// pays nothing for the wait because its climb was free time anyway, and a
/// standing heavy pays it as idle seconds. Both are the same decision.
///
/// MULTIPLIER PER SECOND IS THE PROXY, and it UNDER-states a wait: Blood Rush
/// reads the same counter and is not in it.
pub(super) fn heavy_cycle_seconds(floor_seconds: f64, earned: f64, initial: f64) -> f64 {
    let per_second = |t: f64| melee_combo_multiplier(melee_combo_points(earned, initial, t)) / t;
    let mut best = floor_seconds.max(1e-6);
    let mut best_rate = per_second(best);
    let rungs = (initial / MELEE_COMBO_POINTS_PER_TIER).floor().max(0.0) as u32;
    for k in 1..=rungs {
        let t = f64::from(k) * MELEE_COMBO_POINTS_PER_TIER / INITIAL_COMBO_REGEN_PER_SECOND;
        if t > best && per_second(t) > best_rate {
            best_rate = per_second(t);
            best = t;
        }
    }
    best
}

/// HOW MANY TIMES ONE SWING LANDS — the stance row's count, or ONE where the
/// Tennokai window has replaced the swing with a heavy attack.
///
/// *"Performing a Heavy Attack or Heavy Slam during this flash"* is what the
/// window buys, so the light swing does not happen: a heavy attack's multiplier
/// is the CLASS's whole total, and multiplying it by a stance row's hit count
/// would pay the same swing five times over on a Rogue Edict spin.
pub(super) fn swing_instances(hits: u32, tennokai_heavy: bool) -> u32 {
    if tennokai_heavy {
        1
    } else {
        hits.max(1)
    }
}

/// WHAT ONE INSTANCE LEFT ON A BODY, for the one mechanism that has to know.
///
/// Melee Influence spreads from every body a SWING struck rather than from the
/// aimed one alone, so a Follow Through instance has to report what it applied
/// and what it was worth. Every other caller of [`spread_hit`] drops it.
#[derive(Debug, Default, Clone)]
pub(super) struct Landed {
    pub(super) procs: Vec<DamageType>,
    /// The instance's own damage before mitigation — what a share of it is
    /// taken from.
    pub(super) raw: f64,
    pub(super) killed: bool,
}

/// WHAT ONE HIT EARNS THE COUNTER (MEASUREMENTS M96, M97). `points` is what the
/// row shows and `base` how many of them are base points, the unit every chance
/// acts on; `chance` is Additional Combo Count Chance and `gain` is Chance to Gain
/// Combo Count (0, or a malus). Each base point, carrying its share of `points`:
/// - survives the gain roll (`1 + gain`), or is lost with its share;
/// - pays its share once more per whole 100% of `chance`;
/// - and rolls what is left of `chance` for one more point.
///
/// NO ROLL IS SPENT where neither chance is set, so a build without them draws
/// the same random stream it always did.
pub(super) fn swing_combo_gain(points: f64, base: f64, chance: f64, gain: f64, roll: &mut impl FnMut(f64) -> bool) -> f64 {
    if base <= 0.0 {
        return 0.0;
    }
    let share = points / base;
    let chance = chance.max(0.0);
    let whole = chance.floor();
    let rest = chance - whole;
    let keep = (1.0 + gain).clamp(0.0, 1.0);
    let mut out = 0.0;
    for _ in 0..(base.round() as u32) {
        if keep < 1.0 && !roll(keep) {
            continue;
        }
        out += share * (1.0 + whole);
        if rest > 0.0 && roll(rest) {
            out += 1.0;
        }
    }
    out
}

/// A HIT HOLDS THE COUNTER ONLY IF IT EARNED SOMETHING: one that came to 0 points
/// leaves the combo timer running down (MEASUREMENTS M97). Asked of a swing that
/// earns at all; a heavy attack keeps the refresh it had.
pub(super) fn refreshes_combo_timer(landed: f64, earns: bool, gained: f64) -> bool {
    landed > 0.0 && (!earns || gained > 0.0)
}

/// THE FILL RULE for a row's `combo_points`, and only that: the fight reads each
/// row's own number (see notes: combo_points_from_multiplier). The multiplier
/// ROUNDED UP, once per instance — *"100% = 1 point"* (wiki, Melee Combo), and a
/// swing under 100% still earns one: Rogue Edict opens `200%` then `5x 50%` and
/// the counter shows SEVEN, against 4.5 proportional and 6 flat. Rounded rather
/// than floored at one: the two agree wherever this roster can tell them apart.
#[cfg(test)]
pub(super) fn combo_points_for(multiplier: f64, instances: f64) -> f64 {
    multiplier.ceil().max(1.0) * instances
}

/// THE MELEE COMBO COUNTER AND TENNOKAI — what one fight carries from swing to
/// swing. A gun never reads it.
pub(super) struct MeleeState {
    /// POINTS, not tiers. *"Stance attacks add combo points, scaling with the
    /// attack's stance damage multiplier (100% stance damage multiplier = 1
    /// point)"*, and the tier is `1 + floor(points / 20)` capped at 12 — see
    /// `melee_combo_multiplier`.
    /// ONE COUNTER, TWO READERS THAT WANT OPPOSITE THINGS. A heavy swing SPENDS
    /// it as a damage multiplier; Blood Rush and Weeping Wounds read it as a
    /// bracket term and never touch it. That is the whole reason the seven melee
    /// forms are seven builds.
    pub(super) combo_points: f64,
    /// THE KILL COUNT AT THE LAST SWING, so the kills since are what Rage is paid.
    pub(super) rage_kill_mark: u32,
    /// **THE SECONDS AN AUGMENT HAS ADDED TO A GROWING ABILITY WINDOW**, and the
    /// kill count it was last paid at — the same watermark pair Rage keeps, for
    /// the same reason: kills are counted on the run, and what is owed is the
    /// ones since. Eternal War is the only augment that does this, and the
    /// ability's own cap is what stops it (`ActiveAbility::extend_cap_seconds`).
    pub(super) ability_extra_seconds: f64,
    pub(super) ability_kill_mark: u32,
    /// WHEN THE COUNTER DIES with nothing added to it. Refreshed by any landed
    /// swing; five seconds on almost every weapon.
    pub(super) combo_expiry: f64,
    /// WHEN THE COUNTER WAS LAST EMPTIED BY A HEAVY ATTACK, which is what the
    /// initial-combo floor regenerates from.
    /// THE FIGHT OPENS WITH THE FLOOR FULL: *"Initial Combo grants a minimum
    /// value of combo points when IDLE or after a combo reset. Heavy attacks
    /// spend initial combo, which regenerates at a rate of 40 combo points per
    /// second"* (wiki, Melee Combo). The 40 a second is what a heavy attack owes
    /// back, not what a player walks in owing — so a build carrying +30 opens
    /// its first heavy at 2x rather than reaching it 0.75 s in.
    pub(super) combo_spent_t: f64,
    /// WHICH SWING OF THE SCRIPT IS NEXT. A gun leaves the script empty and
    /// never reads this.
    pub(super) swing_idx: usize,
    /// A window a landed hit opens, in which a HEAVY attack costs no combo. The
    /// owner settled what to do with it in one clause — use it the moment it
    /// fires — so the loop takes the very next swing rather than inventing a
    /// policy.
    /// TWO NUMBERS AND NOTHING ELSE: when the window closes, and how many hits
    /// have landed since the last one opened (Discipline's Merit replaces the
    /// roll with "every 4 hits", which is the one card that makes the count
    /// load-bearing).
    pub(super) tennokai_until: f64,
    /// WAS THIS WINDOW OPENED BY A TENNOKAI KILL? Truth's Flame pays its damage
    /// only in one that was: *"the damage bonus is only active following the
    /// first kill"*, so the swing that earns the chain does not carry it.
    pub(super) tennokai_chained: bool,
    pub(super) tennokai_hits: u32,
}

/// WHAT THIS SWING DID TO THE COMBO COUNTER, and what the counter did back.
///
/// GAIN FIRST, THEN SPEND, and the order is the game's: a heavy attack pays
/// the multiplier that was standing when it went down, lands, and then empties
/// the counter. A melee Incarnon arms here too — its way in is a swing rather
/// than a gauge.
#[allow(clippy::too_many_arguments)]
pub(super) fn after_swing(
    h: &crate::model::ComboHit,
    active: &FightParams,
    params: &FightParams,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    combo_now: f64,
    combo_multiplier: f64,
    tennokai: bool,
    tennokai_kill_mark: u32,
    tennokai_heavy: bool,
    pellets_before: u32,
    t: f64,
    r: &mut RunResult,
    melee: &mut MeleeState,
    incarnon: &mut IncarnonState,
    ammo: &Ammo,
    arc: &mut ArcRuntime,
    bodies: &mut [Body],
) {
        let landed = (r.pellets - pellets_before) as f64;
        // RAGE BUILDS ON EVERY BODY A HIT LANDED ON AND EVERY KILL SINCE THE
        // LAST SWING — a status kill "still counts as a melee kill". The kill
        // is paid at this swing rather than at the death.
        if let Some(g) = arc.rage.as_mut() {
            let s = g.spec();
            g.build(t, landed * s.per_hit + f64::from(r.kills - melee.rage_kill_mark) * s.per_kill);
        }
        melee.rage_kill_mark = r.kills;
        // …AND A LANDED HIT MAY OPEN THE TENNOKAI WINDOW.
        //
        // *"Triggering Tennokai requires directly striking an enemy ...
        // striking multiple enemies from a single hit and multi-strike
        // attacks do not count as hits"* — so it is ONE roll per swing that
        // landed, not one per body, which is why `landed > 0` rather than
        // `landed` times anything.
        //
        // A CADENCE REPLACES THE ROLL where a card sets one (Discipline's
        // Merit: every 4 hits), and the count only advances while the
        // window is SHUT: a hit landed during the flash is a hit the player
        // is about to spend it on.
        // TRUTH'S FLAME'S TWO TERMS, settled before the ordinary roll
        // because a chain replaces it: a Tennokai KILL re-opens the window
        // with no hit in between, which is the only way in this mechanic to
        // swing it twice in a row.
        if tennokai && active.tennokai.chain_seconds > 0.0 && r.kills > tennokai_kill_mark {
            melee.tennokai_until = t + active.tennokai.chain_seconds;
            melee.tennokai_chained = true;
            melee.tennokai_hits = 0;
        }
        // …AND THE CURSE, which is the whole cost of the card: a Tennokai
        // attack that FAILS to kill empties the counter. Status immunity
        // does not save it — *"your combo will still be reset"* — so it is
        // unconditional here, and the Heat is COUNTED rather than applied
        // because nothing in this arena damages the Tenno.
        if tennokai && active.tennokai.curse_resets_combo && r.kills == tennokai_kill_mark {
            melee.combo_points = 0.0;
            r.self_damage.add(
                DamageType::Heat,
                active.tennokai.curse_heat_per_second * active.tennokai.curse_seconds,
            );
        }
        if active.tennokai.enabled && landed > 0.0 && t >= melee.tennokai_until {
            melee.tennokai_hits += 1;
            let opens = if active.tennokai.every_n_hits > 0 {
                melee.tennokai_hits.is_multiple_of(active.tennokai.every_n_hits)
            } else {
                // 15% BASE, and the cards add to it.
                d.spine.chance(TENNOKAI_BASE_CHANCE + active.tennokai.chance)
            };
            if opens {
                let w = if active.tennokai.window_seconds > 0.0 {
                    active.tennokai.window_seconds
                } else {
                    TENNOKAI_WINDOW_SECONDS
                };
                melee.tennokai_until = t + w;
                melee.tennokai_hits = 0;
            }
        }
        // A HEAVY ATTACK EARNS NOTHING. *"connecting with a heavy attack
        // does not add to the combo counter"* (wiki, Melee), and it is the
        // swing's KIND that says so rather than the form: a Tennokai heavy
        // on a light combo is one too. On a spending form it is visible
        // only through Melee Combo Efficiency, which is the share of the
        // counter the swing does NOT empty.
        let earns = !(active.spends_combo || tennokai_heavy);
        let mut gained = 0.0;
        if landed > 0.0 && earns {
            // EVERY LANDED INSTANCE IS A HIT OF ITS OWN: its base points take
            // the gain roll, Additional Combo Count Chance and what is left
            // of it — `swing_combo_gain`. ENDURING STRIKE adds to that chance
            // while the target is LIFTED, a status this engine tracks rather
            // than a state it has to assume.
            let chance_now = active.combo_count_chance
                + if bodies[0].debuffs.lifted.is_some_and(|e| e > t) {
                    active.combo_count_chance_on_lifted
                } else {
                    0.0
                };
            for _ in 0..(landed as u32) {
                gained += swing_combo_gain(
                    h.combo_points,
                    h.combo_points_base,
                    chance_now,
                    active.combo_gain_chance,
                    &mut |p| d.spine.chance(p),
                );
            }
            melee.combo_points += gained;
        }
        if refreshes_combo_timer(landed, earns, gained) {
            melee.combo_expiry = t + active.combo_duration_seconds;
        }
        // …AND A HEAVY SWING EMPTIES IT. `heavy_attack_efficiency` is the
        // share NOT spent — *"40% heavy attack efficiency will change the
        // amount spent to 60% combo points"* — already clamped to the
        // game's 90% cap where it was resolved.
        //
        // IT SPENDS WHETHER OR NOT IT LANDED, because the counter is paid
        // at the swing rather than at the hit, and it restarts the
        // initial-combo floor's clock either way.
        // A TENNOKAI HEAVY SPENDS NOTHING — *"does not consume Combo
        // Counter"* — which is the difference that makes it worth having
        // at all: the counter it read is still there for the next one.
        // …AND A TENNOKAI SWING SPENDS NOTHING — *"does not consume Combo
        // Counter"* — which on a HEAVY mode is the whole of what the window
        // buys, and on a light one is what makes a free 12x heavy free.
        if active.spends_combo && !tennokai {
            // …AND IT SPENDS WHAT THE SWING READ, floor included. The
            // counter is ONE number: *"40% heavy attack efficiency will
            // change the amount spent to 60% combo points"*, and the points
            // an initial-combo floor put there are points like any other.
            // Spending only the EARNED half left a heavy mode at zero after
            // every swing — it earns none — so efficiency bought nothing at
            // all in the one family of modes whose cards sell it.
            melee.combo_points = combo_now * active.heavy_attack_efficiency;
            melee.combo_spent_t = t;
        }
        // …AND A HEAVY SWING IS WHAT ARMS A MELEE INCARNON.
        //
        // *"Reach 6x Combo and then Heavy Attack to activate Incarnon Form
        // for 180 seconds"* — so the number read is the multiplier the
        // swing WENT DOWN with, which is the one it was paid at, taken
        // before the spend above emptied the counter. A Tennokai heavy is a
        // heavy attack and arms it too.
        //
        // IT IS A BUFF AND NOT A TRANSFORM, so nothing is announced and
        // nothing is counted: a melee Incarnon changes numbers rather than
        // attacks, there is no animation to play and none to bill, and the
        // reader sees it where the other windows are — the `melee_incarnon`
        // series in the buff roster. `transforms` counts transmutes into a
        // second WEAPON, which this is not.
        //
        // A TENNOKAI HEAVY ARMS IT TOO: the heavy attack is the
        // condition, and a Tennokai swing is a heavy attack — which is what
        // gives a light combo mode any way in at all, since its loop
        // performs no heavy of its own.
        if let Some(cy) = &params.cycle {
            if let (Arms::HeavyAtCombo(at), Ends::After(window)) = (cy.arms, cy.ends) {
                if incarnon.in_base_form && (active.spends_combo || tennokai_heavy) && combo_multiplier >= at {
                    incarnon.in_base_form = false;
                    incarnon.incarnon_until = t + window;
                    record_weapon(params, rec, ammo, incarnon);
                }
            }
        }
        // SHOCKWAVE SYNERGY — THE ONE THING THAT EARNS COMBO ON A HEAVY
        // MODE, and it is AFTER the spend on purpose: the heavy attack
        // empties the counter and the slam lands after it, so a grant
        // written above would be overwritten and the perk would be worth
        // exactly nothing on the mode it is bought for.
        //
        // *"For each enemy hit by Slam radius, gain 4 Combo Count"*, scaled
        // by combo count chance: *"True Punishment affects Shockwave
        // Synergy, effectively doubling the Combo Count gain from 4 to 8"*
        // (wiki, Praedos). A crowd is what pays it, which is why the count
        // is over the bodies the sphere actually reached.
        //
        // …AND A HEAVY SLAM EARNS NOTHING FROM IT. The
        // same rule the stance points above obey — a swing that SPENDS the
        // counter adds nothing to it — read off the same flag, so the perk
        // and every other earner cannot disagree about what a heavy is.
        if active.combo_count_on_slam_hit > 0.0 && !(active.spends_combo || tennokai_heavy) {
            let slam_rad = match (h.slam_multiplier, active.slam) {
                (Some(_), Some(s)) => Some(s),
                _ => active.radial.filter(|r| {
                    r.blast_kind == crate::model::BlastKind::Slam
                }),
            };
            if let Some(rad) = slam_rad {
                // A SLAM GOES OFF AT THE WIELDER'S OWN FEET — the same
                // epicentre the explosion itself used, so the count and the
                // damage agree on who was in it.
                let det = crate::rules::space::Detonation {
                    at: params.player_at,
                    height_m: 0.0,
                };
                let reached = (0..bodies.len())
                    .filter(|&b| {
                        params.body(b).is_some_and(|spec| {
                            bodies[b].state.health > 0.0
                                && crate::rules::space::caught_by_blast(
                                    det.distance_to(spec.at),
                                    rad.radius_m,
                                )
                        })
                    })
                    .count() as u32;
                if reached > 0 {
                    melee.combo_points += active.combo_count_on_slam_hit
                        * f64::from(reached)
                        * (1.0 + active.combo_count_chance);
                    melee.combo_expiry = t + active.combo_duration_seconds;
                }
            }
        }
}

/// HOW LONG UNTIL THE NEXT SHOT — a draw, a swing's animation, or the fire
/// rate's own interval, whichever this form pays.
#[allow(clippy::too_many_arguments)]
pub(super) fn seconds_to_next_shot(
    active: &FightParams,
    rate: f64,
    initial_now: f64,
    swing: Option<crate::model::ComboHit>,
    tennokai: bool,
    tennokai_heavy: bool,
    ammo: &Ammo,
    incarnon: &IncarnonState,
    melee: &mut MeleeState,
) -> f64 {
    match active.charge_seconds {
        Some(c) => {
            let draw = c * active.fire_rate / rate.max(1e-9);
            match active.charge_cadence {
                // A bow's draw IS the cycle (wiki's bow formula).
                crate::model::ChargeCadence::DrawOnly => draw,
                // Everything else pays the draw AND the listed rate's
                // interval: "1 / (Modded Charge Time + 1 / Modded Fire
                // Rate)". The rate is what happens after the charge.
                crate::model::ChargeCadence::DrawThenRate => draw + 1.0 / rate,
            }
        }
        // A BURST pull fires `count` rounds and then waits; the listed
        // rate is BURSTS per second. PLAYED ROUND BY ROUND, not averaged:
        // inside a pull the next round waits the burst delay, and the
        // pull's LAST round waits `1 / rate`. A pull the magazine cannot
        // finish ends early — an Akarius with one rocket left fires it
        // and reloads — so a lone round pays the full wait.
        //
        // `b.delay_seconds` arrives already shortened by the mod layer
        // (the wiki's net-negative exception); the LIVE buff factor is
        // `rate / active.fire_rate`, clamped so a penalty does not stretch it.
        // A MELEE SWING HAS ITS OWN LENGTH. A stance publishes a
        // sequence and a per-combo damage-per-second, so the combo lasts
        // `sum of multipliers / that rate` and the swings share it EVENLY —
        // the script's one approximation, declared on every melee entry,
        // since nothing published states a swing's animation length.
        //
        // DIVIDED BY THE LIVE ATTACK SPEED: the script is published at
        // 1.0x, so Fury is an ordinary fire-rate mod here and an on-kill
        // speed buff shortens a swing without knowing melee exists.
        None if !active.combo_script.is_empty() => {
            // TWO CLOCKS, and only one of them is attack speed's.
            //
            // *"Increasing melee attack speed does not reduce the wind-up
            // time; rather, it reduces the interval between heavy
            // attacks"* (wiki, Melee) — so the charge before a heavy swing
            // is divided by its OWN bucket, already applied where the
            // script was resolved, and the animation after it is divided by
            // the live attack speed here. A light swing carries no wind-up
            // and is unaffected by the split.
            let (mut w, d) = swing
                .as_ref()
                .map_or((0.0, 0.0), |h| (h.windup_seconds, h.delay_seconds));
            // …AND A TENNOKAI ATTACK CHARGES AT ITS OWN SPEED, which is the
            // class's divided by the window's bonus and by nothing else:
            // *"the Wind-Up Speed of Tennokai attacks is not affected by
            // Wind-Up Speed bonuses from other sources"*. On a LIGHT form
            // it is a charge the swing did not have; on a HEAVY one it
            // REPLACES the build's, which a heavy build stacking wind-up
            // cards feels as a swing that is slower than its ordinary one.
            if tennokai && (tennokai_heavy || active.spends_combo) {
                w = active.tennokai.windup_seconds;
            }
            // A HEAVY ATTACK BREAKS THE CHAIN, so the next light swing
            // starts the combo over (owner — the wiki says
            // nothing about a stance chain's position, so this is his
            // answer and not a derivation).
            //
            // IT IS THE HALF THAT DECIDES WHICH SWINGS EVER HAPPEN. Raging
            // Whirlwind is `400 / 200 / 300 / 500`, and a build whose chain
            // restarts fires the opener over and over and reaches the 500%
            // finisher only when the window does not. With Discipline's
            // Merit — every four hits — it would reach it never, which is
            // the sharpest case and the reason this could not be left to a
            // default: `swing_idx += 1` was the whole difference.
            if tennokai {
                melee.swing_idx = 0;
            } else {
                melee.swing_idx += 1;
            }
            let cycle = (w + d / rate.max(1e-9)).max(1e-6);
            // …AND A HEAVY MODE SWINGS WHEN THE COUNTER IS WORTH SPENDING,
            // which is not always as soon as the animation allows — see
            // `heavy_cycle_seconds`. Derived from the swing SPENDING the
            // counter rather than from the mode, so a standing heavy and a
            // heavy slam make the same decision and the next weapon needs
            // no field.
            if active.spends_combo {
                heavy_cycle_seconds(cycle, melee.combo_points, initial_now)
            } else {
                cycle
            }
        }
        None => match active.burst {
            Some(b) if b.count > 1 => {
                let live = (rate / active.fire_rate.max(1e-9)).max(1.0);
                let mag_now = if incarnon.in_base_form { incarnon.base_magazine } else { ammo.loaded };
                let pull_goes_on =
                    !ammo.rounds_this_mag.is_multiple_of(b.count) && can_fire(mag_now, 1.0);
                if pull_goes_on { b.delay_seconds / live } else { 1.0 / rate }
            }
            _ => 1.0 / rate,
        },
    }
}

/// WHAT THIS SWING IS — everything a melee attack settles before its first
/// instance lands, and a no-op on a gun, whose combo script is empty.
pub(super) struct Swung {
    pub(super) swing: Option<crate::model::ComboHit>,
    pub(super) initial_now: f64,
    pub(super) combo_now: f64,
    pub(super) combo_multiplier: f64,
    pub(super) tennokai: bool,
    pub(super) tennokai_heavy: bool,
    pub(super) tennokai_kill_mark: u32,
    pub(super) swing_mult: f64,
    pub(super) melee_struck: Vec<usize>,
    pub(super) swing_forced_types: Vec<DamageType>,
    pub(super) swing_forced_independent: Vec<&'static str>,
    pub(super) qvec: DamageVector,
    pub(super) modded_base: f64,
    pub(super) direct_pre_snap: DamageVector,
    pub(super) co_base: crate::model::CoBase,
}

/// Read the stance's next swing, spend what it spends, and fold it into the
/// vector the instances are dealt from.
#[allow(clippy::too_many_arguments)]
pub(super) fn swing_this_shot(
    params: &FightParams,
    apl: &crate::data::apl::Apl,
    active: &FightParams,
    t: f64,
    qvec: &DamageVector,
    modded_base: f64,
    buff_stacks: &mut [LiveStacks],
    melee: &mut MeleeState,
    r: &RunResult,) -> Swung {
    // ---- THE MELEE SWING THIS SHOT IS -------------------------------
    //
    // A gun's script is empty and every line below is a no-op for it: the
    // swing is `None`, the multiplier is 1.0, and the counter never moves.
    let swing = active.combo_script.get(melee.swing_idx % active.combo_script.len().max(1)).cloned();
    // THE COUNTER, BEFORE THIS SWING. A heavy attack reads it and then
    // empties it, so the multiplier it pays is the one that was standing
    // when the trigger went down — the same rule the game states by
    // spending "all or part of the combo counter" as part of the attack.
    // …OR THE CLOCK RAN OUT ZERO. *"A zero or negative combo duration
    // prevents increasing the combo counter"* — so the counter is cleared
    // HERE, upstream of the one place it is read, rather than by each of
    // the four swings that earn into it remembering to ask.
    if t > melee.combo_expiry || active.combo_frozen {
        // *"Melee Combo resets after this time"*. Power Spike's partial
        // decay is a WARFRAME passive and is not modelled — declared,
        // because a build running it keeps far more of the counter than
        // this does and is therefore UNDER-reported here.
        melee.combo_points = 0.0;
    }
    // …AND THE FLOOR IS LIVE. Galvanized Reflex earns +20 initial combo per
    // melee kill to four stacks, so the number the counter returns to moves
    // during the fight — read here rather than at resolve, which is where
    // it was a static build-time value and the card's whole second half
    // went unpaid.
    let initial_now =
        active.initial_combo + buff_total(active, crate::model::BuffGrant::InitialCombo, buff_stacks, t);
    let combo_now = melee_combo_points(melee.combo_points, initial_now, t - melee.combo_spent_t);
    let combo_multiplier = melee_combo_multiplier(combo_now);
    // THE STANCE MULTIPLIER SCALES THE SWING'S OWN DAMAGE, and a HEAVY form
    // takes the combo multiplier on top of it.
    //
    // IT IS FOLDED INTO THE BASE rather than applied to the finished
    // instance, and the difference is the QUANTIZATION GRID — the snap is
    // per component against `ModdedBase / 32`. Folding is the reading with
    // an argument behind it (DE publishes a damage figure per combo attack,
    // so the swing IS the attack's damage) and the harmless one, since
    // quantizing `kX` against `ks` is `k` times quantizing `X` against `s`.
    // UNMEASURED either way, and flagged as such.
    // ---- IS THIS SWING A TENNOKAI HEAVY? --------------------------
    // Only on a form that is not ALREADY heavy: the window makes a heavy
    // attack free and a mode whose every swing is one has nothing to
    // convert. `heavy` is the CLASS's multiplier and wind-up.
    // THE WINDOW IS OPEN, and what it BUYS depends on the mode. On a LIGHT
    // form the swing becomes a heavy attack — the class's multiplier in
    // place of the stance's, times a combo multiplier it does not spend. On
    // an already-heavy form the other half pays: the swing costs no combo,
    // so the counter it read is there for the next one. ONE WINDOW, ONE
    // SWING either way — the flash goes out with it.
    let tennokai = active.tennokai.enabled && t < melee.tennokai_until;
    // **THE LIST DECIDES WHETHER THE SWING CONVERTS**, by the RULE it picks and
    // not by its action: `heavy,if=tennokai` on a light combo is a converted
    // swing, while the `heavy` a heavy-attack build presses all engagement is
    // the mode's own press and converts nothing. Two lines, one action, and
    // only the condition tells them apart.
    let tennokai_heavy = apl
        .pick_rule(&crate::data::apl::Now {
            can_fire: true,
            gauge_pct: 0.0,
            magazine_pct: 1.0,
            tennokai,
            in_base_form: true,
            remaining: &|_| 0.0,
        })
        .is_some_and(|r| {
            r.action == crate::data::apl::Action::Heavy && r.when == crate::data::apl::When::Tennokai
        });
    // …AND WHETHER THE ONE BEING SPENT WAS CHAINED, kept because spending
    // it clears the flag and the damage is decided after.
    let tennokai_was_chained = tennokai && melee.tennokai_chained;
    // WHAT THE COUNT WAS BEFORE IT, so "did this swing kill" is a
    // subtraction rather than a flag every path would have to set.
    let tennokai_kill_mark = r.kills;
    if tennokai {
        melee.tennokai_until = f64::NEG_INFINITY;
        melee.tennokai_chained = false;
    }
    // SEISMIC WAVE IS A MULTIPLIER OF ITS OWN: *"Slam damage bonus is
    // multiplicative to base damage (e.g. Pressure Point)"* (wiki). Killing
    // Blow's `+X% on Heavy Attack` reads the same on the card and lands in
    // the base-damage BUCKET instead — `heavy_attack_base_damage`.
    //
    // A SLAM IS A FORM WHOSE EXPLOSION IS A SLAM. Reading the form rather
    // than a flag is what keeps this true for the next slam weapon.
    let is_slam = active
        .radial
        .as_ref()
        .is_some_and(|r| r.blast_kind == crate::model::BlastKind::Slam);
    let mode_damage = 1.0 + if is_slam { active.slam_damage } else { 0.0 };
    // …AND WHAT THE SWING IS WORTH.
    //
    // A TENNOKAI SWING IS A HEAVY ATTACK: the class multiplier in place of
    // the stance's, times the combo multiplier it READS AND DOES NOT SPEND
    // — which is the whole of the mechanic. A light build that has climbed
    // to 12x fires free 12x heavy attacks between its swings.
    //
    // Killing Blow's `on Heavy Attack` bonus rides it too, because this IS
    // one; Master's Edge and Truth's Flame are the window's own.
    let swing_mult = if tennokai_heavy {
        active.heavy.map_or(1.0, |h| h.multiplier)
            * combo_multiplier
            // KILLING BLOW ON A LIGHT FORM'S FREE HEAVY. The card's bucket
            // is the base-damage one, so what it is worth here is the
            // RATIO that bucket grows by — `heavy_attack_base_damage` reads
            // zero on a form that does not spend the counter, which this
            // one is.
            * (1.0 + active.base_damage_bonus + active.heavy_attack_damage)
            / (1.0 + active.base_damage_bonus)
            // …AND ONLY WHERE THE CARD PAYS IT. Truth's Flame's bonus is
            // the CHAINED window's; every other card's is unconditional.
            * (1.0
                + if active.tennokai.damage_needs_chain && !tennokai_was_chained {
                    0.0
                } else {
                    active.tennokai.damage
                })
    } else {
        swing.as_ref().map_or(1.0, |h| h.multiplier)
            * if active.spends_combo { combo_multiplier } else { 1.0 }
            * mode_damage
    };
    // WHO THIS SWING REACHES. A `360deg` swing is a spin and takes
    // everything within the weapon's range; an ordinary one sweeps in
    // front. Empty for a gun, which never asks.
    let melee_struck = match &swing {
        Some(h) if active.follow_through.is_some() => params
            .melee_struck(h.all_around, buff_total(active, crate::model::BuffGrant::MeleeRange, buff_stacks, t)),
        _ => Vec::new(),
    };
    // WHAT THIS SWING FORCES, split into the two machines that carry it —
    // damage types compete for the proc roll, independent procs never do.
    // Resolved once per swing rather than per pellet: a melee swing is one
    // instance, and the split is a string comparison over a list of at most
    // two.
    let (swing_forced_types, swing_forced_independent) =
        swing.as_ref().map_or_else(|| (Vec::new(), Vec::new()), |h| h.split_forced());
    // …AND THE PHYSICAL BONUS SOME SWINGS CARRY. `ImpactMultiplier = { 1.5 }`
    // on three of Crushing Ruin's swings, `SlashMultiplier = { 1.25 }` on one
    // of Sovereign Outcast's — the wiki's own module, and a different thing
    // from the forced proc several of the same swings ALSO carry.
    //
    // SCALING THE FINISHED VECTOR IS EXACT here, not an approximation:
    // neither type enters the elemental hierarchy, so nothing can have
    // consumed it on the way and `Base x 1.5` on the component is the same
    // number as `Base x 1.5` before the mods that multiply the whole thing.
    let physical_bonus = [
        (DamageType::Impact, swing.as_ref().map_or(1.0, |h| h.impact_multiplier)),
        (DamageType::Slash, swing.as_ref().map_or(1.0, |h| h.slash_multiplier)),
    ];
    let any_physical = physical_bonus.iter().any(|(_, b)| (b - 1.0).abs() > 1e-12);
    // THE FLAT ADD RIDES BESIDE THE SWING, NEVER INSIDE IT (MEASUREMENTS
    // M79). A stance's multiplier, a slam's and a heavy's are the WEAPON's,
    // so the packet an Incarnon perk added keeps its size while the
    // weapon's own base is multiplied: `mods x (base x swing + flat)`.
    // Its IMPACT multiplier is treated the same way — the attack's own
    // shape, on the attack's own base — which is a generalisation and not
    // one of M79's four readings.
    let unswung = active.unswung_fraction.clamp(0.0, 1.0);
    let swung;
    // What the fold did to the whole base, and to the WEAPON's half of it.
    // The two differ exactly when a flat add is present, and the GunCO
    // bracket needs the second: it reads the weapon's base and takes the
    // swing with it, where the flat packet takes neither.
    let (mut swing_eff, mut swing_on_weapon) = (1.0, 1.0);
    let (qvec, modded_base) = if (swing_mult - 1.0).abs() > 1e-12 || any_physical {
        let flat = qvec.scale(unswung);
        let mut v = qvec.scale((1.0 - unswung) * swing_mult);
        for (ty, bonus) in physical_bonus {
            if (bonus - 1.0).abs() > 1e-12 {
                let extra = v.get(ty) * (bonus - 1.0);
                v.add(ty, extra);
            }
        }
        // The ModifiedBase grows by the same share the vector did, so
        // the quantization grid stays proportional to what is on it.
        let before = qvec.total() * (1.0 - unswung) * swing_mult;
        let shape = if before > 0.0 { v.total() / before } else { 1.0 };
        swing_on_weapon = swing_mult * shape;
        swing_eff = (1.0 - unswung) * swing_on_weapon + unswung;
        let mb = modded_base * swing_eff;
        for (ty, amount) in flat.iter_nonzero() {
            v.add(ty, amount);
        }
        swung = v;
        (&swung, mb)
    } else {
        (qvec, modded_base)
    };
    // THE SWUNG VECTOR, kept for the LEDGER. Its quantization layer is
    // drawn against `stage_mb`, which the fold has already grown, so
    // handing it the unswung vector printed a grid and a set of components
    // that were never on it — the ledger's own product then fell short of
    // the number by exactly the swing, on every melee row.
    let direct_pre_snap = *qvec;
    // …AND THE CO BRACKET FOLLOWS THE WEAPON'S HALF. `active`'s fraction is
    // read against the UNSWUNG base; once the swing has landed on one half
    // only, the share the term reads is a different number.
    let co_base = if swing_eff > 0.0 {
        active.co_base.against(active.co_base.of() * swing_eff / swing_on_weapon)
    } else {
        active.co_base
    };
    Swung {
        swing,
        initial_now,
        combo_now,
        combo_multiplier,
        tennokai,
        tennokai_heavy,
        tennokai_kill_mark,
        swing_mult,
        melee_struck,
        swing_forced_types,
        swing_forced_independent,
        qvec: *qvec,
        modded_base,
        direct_pre_snap,
        co_base,
    }
}
