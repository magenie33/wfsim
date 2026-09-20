use super::*;

/// THE SHOT COMBO COUNTER as it stands at `t`, given the count as of the last
/// landing hit. Decay is computed rather than ticked because this loop has no
/// clock of its own: it advances shot to shot, and a counter that lost one per
/// elapsed period is exactly what *"reduced by 1 after a short period of time
/// that no successful hits have been made"* describes.
///
/// A MISS never happens here. The wiki's other decay trigger — *"or if the
/// player misses a shot"* — needs an accuracy model this arena does not have
/// (docs/UNMODELLED.md: no distance), so every shot lands and the counter only
/// ever decays through time. It is recorded on the weapon's card as the one
/// way this runs generous.
pub(super) fn combo_at(
    spec: Option<crate::model::SniperCombo>,
    held: bool,
    count: u32,
    last_hit: f64,
    t: f64,
) -> u32 {
    let Some(c) = spec else { return 0 };
    if held || c.seconds <= 0.0 {
        return count;
    }
    let lost = ((t - last_hit) / c.seconds).floor();
    if lost <= 0.0 {
        count
    } else if lost >= f64::from(u32::MAX) {
        0
    } else {
        count.saturating_sub(lost as u32)
    }
}

/// A stacking spec's decay period, or 0 when the spec is absent.
/// PYRANA PRIME'S STREAK, as the bar holds it: `stacks` kills on one clock.
pub(super) fn kill_streak(s: crate::model::KillStreakSummonSpec, stacks: u32, t: f64) -> crate::rules::buffs::Buff {
    crate::rules::buffs::Buff {
        id: crate::model::KillStreakSummonSpec::STREAK_BUFF_ID.into(),
        scope: crate::rules::buffs::BuffScope::Weapon,
        stacks,
        expiry_seconds: Some(t + s.kill_window_seconds),
        contributions: crate::rules::buffs::Contributions::default(),
    }
}

/// PYRANA PRIME'S SECOND GUN, as the bar holds it: the fire-rate half is a
/// bar multiplier like Frenzy's, and the magazine half reads whether it is up.
pub(super) fn summoned_gun(s: crate::model::KillStreakSummonSpec, t: f64) -> crate::rules::buffs::Buff {
    crate::rules::buffs::Buff {
        id: crate::model::KillStreakSummonSpec::BUFF_ID.into(),
        scope: crate::rules::buffs::BuffScope::Weapon,
        stacks: 1,
        expiry_seconds: Some(t + s.duration_seconds),
        contributions: crate::rules::buffs::Contributions {
            fire_rate_multiplier: s.fire_rate_multiplier,
            ..Default::default()
        },
    }
}

pub(super) fn dur(spec: &Option<crate::model::StackSpec>) -> f64 {
    spec.as_ref().map_or(0.0, |s| s.duration)
}

pub fn run_once(params: &FightParams, rng: &mut Rng) -> RunResult {
    run_once_traced(params, rng, None, &mut crate::record::Record::off())
}

/// WHAT THE STACKING BUFFS OF ONE GRANT ARE WORTH at `t`, summed over the
/// cards that grant it, with the live stacks the run holds.
///
/// IT TAKES THE PANEL EXPLICITLY, and that is not a style choice: in a CYCLE
/// the two forms resolve the same buff against different base rates (the
/// Furis Incarnon's 12 ticks/s against the base form's 10), so a FireRate
/// buff's absolute `per_stack` differs per form. Reading the outer `params`
/// instead of the ACTIVE form handed the base form the Incarnon form's rate.
/// The STACKS stay shared (one buff, one count, across the whole engagement);
/// only the conversion is per form.
#[inline]
pub(super) fn buff_total(from: &FightParams, grant: crate::model::BuffGrant, stacks: &mut [LiveStacks], t: f64) -> f64 {
    from.stacking_buffs
        .iter()
        .enumerate()
        .filter(|(_, b)| b.grant == grant)
        .map(|(i, b)| b.per_stack * stacks[i].current(t, b.duration) as f64)
        .sum::<f64>()
}





/// …AND A BUMP THAT COUNTS SHELLS. `bump_buffs!` grants a whole magazine
/// per trigger, which is what an ordinary reload loads; the Incarnon route
/// loads a KNOWN number of shells and splits them across two moments, so it
/// needs to say how many. Reload-counting buffs are left out — they are
/// waiting for the reload to finish, and it has not.
#[inline]
pub(super) fn bump_shells(params: &FightParams, stacks: &mut [LiveStacks], n: u32, t: f64, rng: &mut Rng) {
    for (i, b) in params.stacking_buffs.iter().enumerate() {
        if b.per_shell
            && b.trigger == crate::model::BuffTrigger::ReloadComplete
            && (b.chance >= 1.0 || rng.chance(b.chance))
        {
            for _ in 0..n {
                stacks[i].bump(t, b.duration, b.max_stacks);
            }
        }
    }
}

/// THE MAGAZINE IS FULL AGAIN — a reload that COMPLETED, or either Incarnon
/// transform completing, since swapping either way fully reloads the base
/// form's magazine (wiki).
///
/// One function rather than the same three lines at four sites: everything that
/// a refill ends, ends here. Ready Retaliation is spent, Reaver's Rapture is
/// reset, and the burst count restarts. THE MOMENT IS THE COMPLETION — a
/// reload that has begun has refilled nothing.
///
/// `also_a_reload` is false where the event refills without being a reload:
/// swapping OUT of the Incarnon form refills the base magazine, and Blazing
/// Barrel is stated to survive it.
#[inline]
pub(super) fn magazine_refilled(
    params: &FightParams,
    ammo: &mut Ammo,
    r: &mut RunResult,
    stacks: &mut [LiveStacks],
    rs_armed: &mut bool,
    opening_closed: &mut bool,
    also_a_reload: bool,
) {
    *rs_armed = false;
    ammo.rounds_this_mag = 0;
    // HOW MANY TIMES THE MAGAZINE HAS BEEN FULL AGAIN. Counted here
    // rather than derived from `r.reloads` and `r.transforms`, because
    // those two miss the fourth site: the Incarnon EXIT refills the
    // base magazine and increments neither.
    ammo.refills += 1;
    if !*opening_closed {
        *opening_closed = true;
        r.first_magazine_damage = r.effective_damage();
    }
    for (i, b) in params.stacking_buffs.iter().enumerate() {
        let cleared = match b.cleared_by {
            crate::model::ClearedBy::MagazineRefilled => true,
            crate::model::ClearedBy::Reload => also_a_reload,
            _ => false,
        };
        if cleared {
            stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
        }
    }
}

/// THE WEAPON AS IT STANDS, stamped onto the combat record.
///
/// CALLED WHERE STATE CHANGES, and those are six
/// places that each own a different set of locals: the shot that spends a
/// round, the two ends of a reload, and the two ends of each transform. It
/// Written at every EVENT and not at the shot only: otherwise every event
/// between two shots carries the previous shot's weapon, and a
/// `transform_end` that has just put 216 charges in an Incarnon magazine
/// reports `base 0/12` — the one row a reader opens the record to see.
#[inline]
pub(super) fn record_weapon(
    params: &FightParams,
    rec: &mut crate::record::Record,
    ammo: &Ammo,
    incarnon: &IncarnonState,
) {
    if rec.is_on() {
        rec.set_weapon(crate::record::WeaponAt {
            transmuted: !incarnon.in_base_form,
            magazine: (if incarnon.in_base_form { incarnon.base_magazine } else { ammo.loaded }).max(0.0) as u32,
            magazine_max: params
                .cycle
                .as_ref()
                .filter(|_| incarnon.in_base_form)
                .map_or(ammo.cap, |cy| cy.base_form.magazine_size)
                as u32,
            // THE FORM THAT IS NOT FIRING, so the free reload a
            // transmute performs on the base magazine is visible rather
            // than inferred.
            idle_magazine: params.cycle.as_ref().map(|cy| {
                if incarnon.in_base_form {
                    // THE CHARGES ARE THE GAUGE IN ANOTHER UNIT. A Laetum's 216 charges over a
                    // 12-unit gauge is 18 a unit exactly, and the row
                    // must not print `216 / 216` from the moment the
                    // fight opens — a full Incarnon magazine on a
                    // weapon that has not earned a single unit of it is
                    // the one column a reader opens this panel to
                    // check. The gauge decides it; `charges` can run
                    // past the fill mark, so the share is clamped.
                    let to_fill = match cy.arms {
                        Arms::Gauge { charges_to_fill, .. } => charges_to_fill,
                        // A melee Incarnon has no gauge, and this
                        // column is never drawn for one: melee has no
                        // magazine either.
                        Arms::HeavyAtCombo(_) => 0,
                    };
                    let share = if to_fill > 0 {
                        (f64::from(incarnon.charges) / f64::from(to_fill)).min(1.0)
                    } else {
                        0.0
                    };
                    ((share * ammo.cap).round().max(0.0) as u32, ammo.cap as u32)
                } else {
                    (incarnon.base_magazine.max(0.0) as u32, cy.base_form.magazine_size as u32)
                }
            }),
            // THE GAUGE, and it starts EMPTY — which is the model's own
            // rule and was nowhere on screen.
            // INFINITE IS `None` rather than a very large number: a
            // ruler grants it, and a column reading "1e9" is a column a
            // reader has to decode.
            reserve: (!params.infinite_reserve).then_some(ammo.reserve),
            // …AND ONLY A GAUGE IS DRAWN AS ONE. A melee Incarnon's
            // way in is a swing, so there is no bar to fill and the
            // column is absent rather than pinned at zero.
            gauge: params.cycle.as_ref().and_then(|cy| match cy.arms {
                Arms::Gauge { charges_to_fill, .. } => Some((incarnon.charges, charges_to_fill)),
                Arms::HeavyAtCombo(_) => None,
            }),
        });
    }
}

/// SAMPLING HAPPENS TWICE, and the second time is
/// body mutably borrows `arc`, `gal`, `buff_stacks`, `target`, `r`,
/// `debuffs`, `others` and `trace` at once, which a closure cannot hold
/// together.
///
/// It takes the run's state by reference rather than closing over it: the
/// sampler reads nine pieces at once and writes one frame, which is the whole
/// reason it was a macro.
#[allow(clippy::too_many_arguments)]
pub(super) fn sample_frames_up_to(
    until: f64,
    params: &FightParams,
    trace: &mut Option<&mut Replay>,
    next_frame: &mut f64,
    frame_seconds: f64,
    arc: &mut ArcRuntime,
    gal: &mut GalStacks,
    buff_stacks: &mut [LiveStacks],
    bar: &BuffBar,
    windows: &CardWindows,
    tendril: &Tendrils,
    crit_per_hit: &CritPerHit,
    sniper_combo: &SniperComboCount,
    combo_spec: Option<crate::model::SniperCombo>,
    incarnon: &IncarnonState,
    influence_until: f64,
    target: &TargetState,
    r: &RunResult,
    debuffs: &DebuffState,
    others: &[SpreadFoe],
) {
    if let Some(rep) = trace.as_deref_mut() {
        while *next_frame <= until && *next_frame < params.duration_seconds {
            let stacks = sample_stacks(
                params, &rep.buffs, *next_frame, arc, gal, buff_stacks,
                &windows.crit_on_headshot_stacks, windows.crit_on_headshot, windows.fire_rate_after_reload, windows.base_damage_after_reload,
                windows.base_damage_eximus, windows.streak, tendril.count, crit_per_hit.stacks, bar,
                combo_at(combo_spec, params.combo_held, sniper_combo.count,
                    sniper_combo.last_hit, *next_frame),
                incarnon.incarnon_until,
        influence_until,
            );
            rep.frames.push(Frame {
                t: *next_frame,
                overguard: target.overguard,
                shield: target.shield,
                health: target.health,
                damage: r.effective_damage(),
                kills: r.kills,
                shots: r.shots,
                pellets: r.pellets,
                crits: r.crits,
                big_crits: r.big_crits,
                crit_tier_sum: r.crit_tier_sum,
                headshots: r.headshots,
                procs: r.procs,
                field_ticks: r.field_ticks,
                reloads: r.reloads,
                transforms: r.transforms,
                sources: r.sources,
                stacks: stacks.iter().map(|(n, _)| *n).collect(),
                // ONE SERIES PER FOLLOWED BODY, in `Replay::tracked`'s
                // order — the aimed one first.
                debuffs: rep
                    .follow
                    .iter()
                    .map(|&bi| {
                        // THE CURVES WANT COUNTS. The expiry beside
                        // each one is the RECORD's — a chart of a
                        // stack count has no use for it.
                        let one = if bi == 0 {
                            debuffs.sample(*next_frame)
                        } else {
                            others[bi - 1].debuffs.sample(*next_frame)
                        };
                        one.into_iter().map(|(n, _)| n).collect::<Vec<u16>>()
                    })
                    .collect(),
            });
            *next_frame += frame_seconds;
        }
    }
}

/// ONE TRIGGER'S BUFFS, bumped. The per-shell family is the other arm
/// (`bump_shells`); this one is every buff a single event grants.
pub(super) fn bump_on_trigger(
    params: &FightParams,
    stacks: &mut [LiveStacks],
    want: crate::model::BuffTrigger,
    t: f64,
    rng: &mut Rng,
) {
    for (i, b) in params.stacking_buffs.iter().enumerate() {
        if !b.per_shell && b.trigger == want && (b.chance >= 1.0 || rng.chance(b.chance)) {
            for _ in 0..b.stacks_per_trigger.max(1) {
                stacks[i].bump(t, b.duration, b.max_stacks);
            }
        }
    }
}

/// A RELOAD FROM EMPTY, and what rides on it: the trigger's own buffs, and
/// Resonant Restore, which is not a `StackingGrant` because what it grants is
/// the capacity every other line of the loop reads.
pub(super) fn bump_reload_from_empty(
    params: &FightParams,
    stacks: &mut [LiveStacks],
    ammo: &mut Ammo,
    t: f64,
    rng: &mut Rng,
) {
    bump_on_trigger(params, stacks, crate::model::BuffTrigger::ReloadFromEmpty, t, rng);
    // RESONANT RESTORE rides the same event, because it is the same
    // event: "On Reload From Empty: Increase Base Magazine Capacity by
    // +15. Stacks up to 3x". It is not a `StackingGrant` because what
    // it grants is not a term in a bracket — it is the capacity every
    // other line of this loop reads, so it moves `mag_cap` itself.
    //
    // MONOTONIC AND CAPPED: no card in this family carries a clock, and
    // the stack count is the only thing that stops it. The magazine
    // GROWS but does not fill — a reload draws from the reserve as it
    // always did, and the extra room is what the next draw can use.
    if let Some((per, max)) = params.magazine_growth_on_empty_reload {
        if ammo.growth_stacks < max {
            ammo.growth_stacks += 1;
            ammo.cap += per * ammo.summon_multiplier;
        }
    }
}

/// One engagement, optionally SAMPLED into a [`Replay`].
///
/// `trace` is `Some` for exactly one run per `monte_carlo` — the median one,
/// replayed afterwards from its recorded RNG state. Threading an `Option`
/// through rather than duplicating the loop is the point: a traced run and a
/// scored run must be the same code, or the replay shows a fight that did not
/// happen.
pub fn run_once_traced(
    params: &FightParams,
    rng: &mut Rng,
    mut trace: Option<&mut Replay>,
    // THE COMBAT RECORD — see [`crate::record`]. `Record::off()` for every run
    // nobody is reading, which is 999 of a thousand: it allocates nothing and
    // costs one branch per event.
    rec: &mut crate::record::Record,
) -> RunResult {
    // THE ENGAGEMENT'S SEED, and the three streams derived from it. The master
    // `rng` is only a seed source from here on: it is advanced once so the next
    // run in a `monte_carlo` differs, and every roll below comes off `d`. See
    // [`Draws`] for why the streams are split — in short, a build that changes
    // only its status chance must not re-roll this engagement's crits.
    let started_at = rng.state();
    let _ = rng.next_f64();
    let d = &mut crate::rules::rng::Draws::new(started_at);
    // HOW FAR THE TARGET SITS OFF THE AIM LINE — a CONSTANT for the whole
    // engagement, because neither body moves. It was read per PELLET when the
    // aim line arrived, which costs an acos and two hypots on every pellet of
    // every fight and showed up as a 35% slowdown in `one_fight` with no answer
    // changed. Nothing about it can differ between two pellets.
    let aim_off_axis = params.off_axis_deg();
    // A COMBO THAT TAKES NO TIME NEVER ENDS: the clock only advances by the
    // rows' delays, so a zero sum loops forever instead of failing.
    assert!(
        params.combo_script.is_empty()
            || params.combo_script.iter().any(|h| h.delay_seconds + h.windup_seconds > 0.0),
        "a combo script whose rows take no time never advances the clock",
    );
    // THE CHAIN'S STATIC HALF (`rules::chain::Layout`). Nothing in this arena moves,
    // so which body the sphere catches and which body is nearest to which are
    // constants — asked once here instead of once per landing pellet, which on
    // a 19x19 grid was ~11,000 distance computations a pellet to reach the same
    // thirteen bodies a 7x7 reaches.
    //
    // PER RUN RATHER THAN PER ENGAGEMENT, deliberately. It was a field on
    // `FightParams` for an hour and a test caught the trap immediately: widen
    // `beam.damage_radius_m` after the params are built and the cached layout
    // is silently stale, which is the two-declarations bug wearing a cache. It
    // is O(N^2) once — 130k distance computations for a 19x19, ~0.13 s over the
    // rulers' 1000 runs against the 188 s it removes — so there is nothing to
    // buy by holding it longer and a correctness trap to pay for.
    // WHO IS NEAR WHOM, for the three AREA status effects (gas, the Tesla
    // chain, a Blast detonation). Static like the chain's, and for the same
    // reason: nothing in this arena moves.
    // WHERE EVERY BODY STANDS, once. Static for the same reason the index is,
    // and hoisted because MELEE INFLUENCE rebuilt this list on every landed hit
    // — a 361-element allocation per proc per swing, in the mechanic this
    // engine is least willing to be slow at.
    let body_at: Vec<crate::rules::space::Vec2> = std::iter::once(params.target_at)
        .chain(params.others.iter().map(|f| f.at))
        .collect();
    let area_near = if params.others.is_empty() {
        crate::rules::space::Neighbours::default()
    } else {
        crate::rules::space::Neighbours::build(&body_at)
    };
    // WHERE A BOUNCE GOES, precomputed for the same reason the chain's is:
    // nothing in this arena moves, so "which body is nearest to this one" is a
    // constant being asked once per shot. Its own layout because its RANGE is
    // its own — ordinarily unbounded, where a chain's is a published metre
    // figure.
    // WHERE EVERY BODY IS, built once. The SEEKING path reads the neighbour
    // layout below; the REFLECTING one reads these positions, because a bounce
    // is geometry and a neighbour list has thrown the geometry away.
    let bounce_bodies: Vec<crate::rules::space::Vec2> = if params.ricochet.is_some() {
        let mut v = Vec::with_capacity(params.others.len() + 1);
        v.push(params.target_at);
        v.extend(params.others.iter().map(|f| f.at));
        v
    } else {
        Vec::new()
    };
    let ricochet_layout = match (params.ricochet, params.others.is_empty()) {
        (Some(rc), false) => {
            let mut bodies = Vec::with_capacity(params.others.len() + 1);
            bodies.push(params.target_at);
            bodies.extend(params.others.iter().map(|f| f.at));
            Some(crate::rules::chain::Layout::build(
                &bodies,
                // NO SPLASH SEEDS. A bounce starts from the body the projectile
                // struck and from nowhere else, so the sphere here is empty —
                // the explosion each bounce sets off is fired separately, by
                // the blast path, at that bounce's own body.
                crate::rules::chain::Splash { at: params.target_at, radius_m: 0.0 },
                crate::rules::chain::Spec {
                    hops: rc.bounces,
                    range_m: rc.range_m,
                    falloff: 1.0,
                    compounds: false,
                },
            ))
        }
        _ => None,
    };
    let chain_layout = match (params.beam, params.others.is_empty()) {
        (Some(b), false) => {
            let mut bodies = Vec::with_capacity(params.others.len() + 1);
            bodies.push(params.target_at);
            bodies.extend(params.others.iter().map(|f| f.at));
            // THE SPLASH CENTRE IS STATIC TOO: the round goes off on the aimed
            // body's surface facing the shooter, and neither of them moves.
            let at = crate::rules::space::detonation_point(params.target_at, params.player_at);
            let layout = crate::rules::chain::Layout::build(
                &bodies,
                crate::rules::chain::Splash { at, radius_m: b.damage_radius_m },
                crate::rules::chain::Spec {
                    hops: b.chain_hops,
                    range_m: b.chain_range_m,
                    falloff: b.chain_damage_per_hop,
                    compounds: b.chain_compounds,
                },
            );
            // AND WHO ELSE THE SHOT TAKES ON ITS OWN — `rules::chain::acquired`, the
            // same rule the Ocucor's tendrils are picked by.
            Some(layout.acquiring(
                &bodies,
                params.player_at,
                params.aim_at.unwrap_or(params.target_at),
                crate::rules::chain::Acquire {
                    count: b.beams_count,
                    cone_deg: b.beams_acquire_deg,
                    range_m: b.beams_range_m,
                },
            ))
        }
        _ => None,
    };
    // …AND WHO IS ON THE LINE, for the same reason: nobody moves, so the bodies
    // a shot passes through are the same for every pellet of every shot.
    let struck = params.struck_bodies();
    let mut next_frame = 0.0f64;
    let frame_seconds = trace.as_ref().map_or(f64::INFINITY, |r| r.frame_seconds);
    let mut bar = BuffBar::new();
    let mut enervate = params
        .arcane
        .enervate_rank
        .map(SecondaryEnervate::from_rank);
    // The configured pile, put on the bar before the first shot. The perk
    // reads its own stacks back off the bar, so seeding it here is all it
    // takes for the ramp to continue from that count.
    if let Some(en) = enervate.as_ref() {
        en.seed(params.enervate_stacks, &mut bar);
    }
    let mut frenzy = Frenzy::new();
    let mut target = TargetState::spawn(&params.target, params.target_at);
    let mut debuffs = DebuffState::default();
    // THE REST OF THE FORMATION — empty for every fight this engine has run,
    // and every line that reads it below is behind that check.
    //
    // BESIDE the aimed body's state rather than holding it too, which is the
    // aim policy showing through: the beam is on ONE body and every other is
    // reached only by what spreads (`formation`,). When the
    // aimed one dies the nearest of these takes its place, so `target` and
    // `debuffs` above stay what the whole loop already reads.
    let mut others: Vec<SpreadFoe> = params
        .others
        .iter()
        .map(|f| SpreadFoe {
            state: TargetState::spawn(&f.params, f.at),
            debuffs: DebuffState::default(),
        })
        .collect();
    // NO PROMOTION, AND NONE IS NEEDED: `TargetState::apply` respawns a body
    // instantly where it stood, so no body is ever left dead for the aim policy
    // to switch away from. The formation is N streams of targets rather than N
    // corpses, which is what a room-clear measurement wants and what the
    // single-target arena has always been.
    //
    // `formation::Formation::retarget` is the policy for the day respawn
    // becomes a setting; it is written and tested and nothing calls it.
    // On-kill stack buffs start at their configured initial stacks (full
    // per the user's setting) with a fresh duration from t = 0.
    let mut gal = GalStacks::default();
    if let Some(s) = &params.co_stack {
        gal.co = LiveStacks::seed(s.initial_stacks, s.max_stacks, s.duration);
    }
    if let Some(s) = &params.multishot_stack {
        gal.multishot = LiveStacks::seed(s.initial_stacks, s.max_stacks, s.duration);
    }
    // Overwhelming Attrition's stacks are EARNED in the run — the default
    // config seeds 0 so no trigger is invented at t = 0 — but a configured
    // buff card seeds them like any other stacking buff.
    // ONE LiveStacks per declared buff, in the same order — index i is buff i.
    // The parallel Vec is what lets the sampler answer by ID without a match.
    let mut buff_stacks: Vec<LiveStacks> = params
        .stacking_buffs
        .iter()
        .map(|b| match b.decay {
            crate::model::BuffDecay::PerStackExpiry => {
                LiveStacks::seed_per_stack(b.initial_stacks, b.max_stacks, b.duration)
            }
            crate::model::BuffDecay::LoseOneAndReset => {
                LiveStacks::seed(b.initial_stacks, b.max_stacks, b.duration)
            }
            crate::model::BuffDecay::AllAtOnce => {
                LiveStacks::seed_all_at_once(b.initial_stacks, b.max_stacks, b.duration)
            }
        })
        .collect();
    // BUMP BY TRIGGER, TOTAL BY GRANT — the two operations the whole family
    // needs, and the only two. `ArcRuntime` has had exactly this pair since the
    // arcanes were written; these are its weapon-side twins.
    let mut rs_armed = false;
    // THE OPENING WINDOW closes the first time the magazine is refilled, and
    // that is not always a reload: a weapon that TRANSMUTES instead of
    // reloading — the Torid, played as its cycle — never performs one in the
    // base form, and the window would never close at all. Measured 2026-08-11:
    // 0 reloads in the median run, 4.4 s of downtime, and an opening magazine
    // reading zero. The refill is the moment, whichever event caused it.
    let mut opening_closed = false;
    let mut r = RunResult {
        rng_state: started_at,
        ..Default::default()
    };
    // THE BUFF ROSTER the record's stack lists are positional against, built
    // ONCE. It is read per damage instance while a record is being taken, and
    // `buff_roster` allocates — so on a dense build the list would be rebuilt a
    // few hundred thousand times for an answer that cannot change inside a run.
    let rec_roster: Vec<BuffSeries> =
        if rec.is_on() { params.buff_roster() } else { Vec::new() };
    // WHERE EACH STACKING BUFF SITS IN THE ROSTER. `bump_buffs!` indexes
    // `stacking_buffs` and a row's list is positional against the roster, so
    // the two are joined once here rather than searched per bump.
    let rec_buff_index: Vec<Option<usize>> = if rec.is_on() {
        params
            .stacking_buffs
            .iter()
            .map(|b| rec_roster.iter().position(|r| r.id == b.id))
            .collect()
    } else {
        Vec::new()
    };
    let mut ammo = Ammo {
        drop_kill_mark: 0u32,
        refill_kill_mark: 0u32,
        rounds_this_mag: 0,
        refills: 0u32,
        cap: params.magazine_size,
        growth_stacks: 0,
        summon_multiplier: 1.0f64,
        owed_shells: 0,
        loaded: params.magazine_size,
        reserve: params.reserve_ammo,
        instant_reload_now: false,
    };
    // Kills already paid to on-kill stacking buffs. See `BuffTrigger::Kill`.
    let mut kill_buff_mark: u32 = 0;
    let mut double_tap = DoubleTap {
        hits: 0,
        expiry: f64::NEG_INFINITY,
        other: (0, 0.0),
    };

    // …and the FROM-EMPTY half, which is deliberately NOT folded into the macro
    // above. Of that macro's three sites only two are reloads from empty: the
    // third is the Incarnon EXIT completing the reload that the transform IN
    // began, and whether THAT was from empty is a question about the magazine
    // one transform ago. So this fires at the two real reload sites and at the
    // transform, where it reads the magazine it actually refilled.

    // Stacking arcanes start FULL (user setting) with a fresh timer; the
    // states run each spec's own decay family from there.
    let mut arc = ArcRuntime::init(params);
    let mut windows = CardWindows {
        fire_rate_after_reload: params
            .fire_rate_on_reload
            .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 }),
        crit_on_headshot: params
            .crit_chance_on_headshot
            .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 }),
        crit_on_headshot_stacks: params
            .crit_chance_stack
            .as_ref()
            .map_or(Vec::new(), |s| vec![s.duration; s.initial_stacks as usize]),
        headshot_times: Vec::new(),
        streak: f64::NEG_INFINITY,
        base_damage_after_reload: params
            .base_damage_on_reload
            .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 }),
        base_damage_eximus: params
            .base_damage_on_eximus_weakpoint
            .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 }),
    };
    // DEATH KNELL — the weak-point pile. `LiveStacks`'s default decay is this
    // buff's: one stack off on the clock, and it restarts for the rest.
    let mut weakpoint_pile = LiveStacks::default();


    // Per-phase precomputation: the quantized vector is static per phase
    // (no dynamic mods); ModdedBase for proc payload formulas stays
    // pre-quantization and EXCLUDES elemental portions (base × (1 + dmg)).
    let precompute = |p: &FightParams| {
        // `mb` is read FIRST because it is also the quantization denominator —
        // it was computed one line below the `quantized()` that needed it.
        let mb = p.dot_modified_base.unwrap_or_else(|| p.damage.total());
        let qvec = p.damage.quantized_against(mb);
        let qtotal = qvec.total();
        (qvec, qtotal, mb)
    };
    // ...and one per PROJECTILE, for a weapon whose missiles differ. Built
    // here for the same reason the single one is: the vector is static for the
    // whole fight, and quantizing it six times a shot would be six times the
    // work for the same answer.
    let variant_pre = |p: &FightParams| -> Vec<(crate::rules::damage::DamageVector, f64, f64)> {
        p.pellet_damage
            .iter()
            .map(|(d, _)| {
                let mb = p.dot_modified_base.unwrap_or_else(|| d.total());
                let q = d.quantized_against(mb);
                (q, q.total(), mb)
            })
            .collect()
    };
    let variant_rad = |p: &FightParams| -> Vec<crate::rules::damage::DamageVector> {
        // An explosion is its OWN attack part, so its denominator is the
        // radial's ModifiedBase and not the direct hit's (MECHANICS §7).
        p.pellet_damage
            .iter()
            .map(|(_, r)| {
                r.quantized_against(p.radial.as_ref().map_or_else(|| r.total(), |x| x.modified_base))
            })
            .collect()
    };
    let main_variants = variant_pre(params);
    let main_variant_rad = variant_rad(params);
    let main_pre = precompute(params);
    let base_pre = params.cycle.as_ref().map(|c| precompute(&c.base_form));
    let base_variants = params.cycle.as_ref().map_or_else(Vec::new, |c| variant_pre(&c.base_form));
    let base_variant_rad =
        params.cycle.as_ref().map_or_else(Vec::new, |c| variant_rad(&c.base_form));
    let status_damage = params.status_duration_multiplier;
    // The per-unit status stack caps (Acolytes: any 4, Impact 3) and the
    // status-payload scaling now live in `settle_procs`, which every instance
    // kind shares.
    // A continuous weapon's damage ramp, per run.
    let mut beam = BeamRamp::default();
    // Renewed Horror: armed by a reload from empty, spent by the next shot's
    // field. The sim always reloads from empty (it fires until dry), so on the
    // Torid this is the first shot of every magazine after the first.
    let mut field_duration_boost = false;
    // Live lingering FIELDS (Torid's clouds), one entry per grenade that stuck.
    let mut fields: Vec<FieldState> = Vec::new();
    // …AND LIVE ORBS, which are not fields: entities with a
    // place of their own that move, strike ONE body inside their reach, and
    // detonate where they have got to.
    let mut orbs: Vec<OrbState> = Vec::new();
    let mut field_ctx = FieldCtx::default();

    // The form whose panel SPAWNED the fields. Only one form of a transform
    // group has a lingering part (Torid's cloud belongs to the base form; its
    // Incarnon beam leaves none), so this is unambiguous - and it is the right
    // answer even after a transmute, because a cloud outlives one.
    let field_ap: &FightParams = match &params.cycle {
        Some(cy) if cy.base_form.lingering.is_some() => &cy.base_form,
        _ => params,
    };

    let mut meter = Meter {
        seconds: field_ap.meter.map_or(0.0, |m| m.seconds_to_fill),
        clocked: 0.0f64,
    };
    // KILLS JAHU CANTICLE HAS ALREADY STRIPPED FOR, read as a delta off the
    // run's own counter for the same reason the meter's pickups are: there are
    // nine places a body can die and a tenth would silently stop paying.
    let mut strip_kills_seen = 0u32;

    // Initial locks: one natural-duration grant at t = 0 (at the set
    // stack count); afterwards only the buff's own mechanics govern it.
    for lock in &params.locked_buffs {
        if matches!(lock.mode, LockMode::Initial(_)) {
            match lock.buff {
                LockedBuff::Frenzy => frenzy.on_event(
                    &Event::Hit(Hit {
                        big_crit: false,
                        headshot: true,
                        target_alive: true,
                    }),
                    0.0,
                    &mut bar,
                ),
            }
        }
    }
    if let Some(s) = params.kill_streak_summon {
        if params.kill_streak_summon_opens_active {
            bar.upsert(summoned_gun(s, 0.0));
        } else if params.kill_streak_opens_at > 0 {
            bar.upsert(kill_streak(s, params.kill_streak_opens_at, 0.0));
        }
    }

    // Fire while t < duration; the inter-shot interval is 1/(base rate x
    // live BuffBar fire-rate multiplier), evaluated after each shot (a buff
    // expiring mid-interval is approximated to the shot boundary).
    //
    // …AND THE FIRST ROUND LEAVES AFTER THE WIND-UP. `t` is when a shot
    // RESOLVES, and the interval between resolutions is the fire rate's
    // exactly — so the whole engagement is the wind-up later rather than each
    // shot being delayed one at a time, and shot `k` lands at
    // `windup + k / rate` by construction.
    //
    // Zero for every gun but the Grimoire's primary fire, so nothing else moves
    // by so much as a bit.
    let mut t = field_ap.windup_seconds;
    // GOTVA PRIME'S PASSIVE, armed. Set by a pellet that landed a status, spent
    // by the next pellet that lands. It survives across shots and reloads: the
    // card says the chance "remains until landing another successful shot", and
    // nothing but a landing shot spends it.
    let mut super_crit_armed = false;
    let mut incarnon = IncarnonState {
        kill_mark: 0u32,
        in_base_form: params.cycle.as_ref().is_some_and(|c| !c.starts_primed),
        incarnon_until: match params.cycle.as_ref().map(|c| (c.starts_primed, c.ends)) {
            Some((true, Ends::After(seconds))) => seconds,
            _ => 0.0,
        },
        charges: 0u32,
        base_magazine: params
            .cycle
            .as_ref()
            .map_or(0.0, |c| c.base_form.magazine_size),
    };


    let mut ghost_pile = Ghosts {
        standing: Vec::new(),
        kill_mark: 0u32,
    };
    let mut syndicate = Syndicate {
        kill_mark: 0u32,
        points: 0.0f64,
        ready_at: 0.0f64,
    };
    let mut crit_per_hit = CritPerHit {
        hit_mark: 0u32,
        refill_mark: 0u32,
        seed: params
            .crit_chance_per_hit
            .map_or(0, |c| params.crit_chance_per_hit_initial_stacks.min(c.max_stacks())),
        stacks: params
            .crit_chance_per_hit
            .map_or(0, |c| params.crit_chance_per_hit_initial_stacks.min(c.max_stacks())),
    };
    let mut sniper_combo = SniperComboCount {
        count: params.combo_initial,
        last_hit: 0.0f64,
    };
    // THE COUNTER IS THE WEAPON'S, NOT THE FORM'S. Its spec — the minimum and
    // the decay period — comes from whichever form declares one, so a cycle
    // that spends half the engagement in a form with no combo does not lose
    // the count it built. What the form DOES decide is whether a hit in it
    // counts and whether it pays, which is read off `ap` at the hit itself:
    // the Incarnon forms declare no combo (nothing published says whether it
    // survives the transform — see their `unmodeled:`), so their hits do
    // neither while the two-second clock keeps running.
    let combo_spec = params
        .sniper_combo
        .or_else(|| params.cycle.as_ref().and_then(|c| c.base_form.sniper_combo));
    let mut tendril = Tendrils {
        kill_mark: 0u32,
        reload_mark: 0u32,
        seed: params.tendrils_initial.min(params.tendril_max),
        count: params.tendrils_initial.min(params.tendril_max),
    };
    let mut spool = Spool {
        shots: 0.0f64,
        due: f64::NEG_INFINITY,
    };
    // ---- THE MELEE COMBO COUNTER AND TENNOKAI -- see `MeleeState` ----------
    let mut melee = MeleeState {
        combo_points: 0.0f64,
        rage_kill_mark: r.kills,
        combo_expiry: f64::NEG_INFINITY,
        combo_spent_t: f64::NEG_INFINITY,
        swing_idx: 0usize,
        tennokai_until: f64::NEG_INFINITY,
        tennokai_chained: false,
        tennokai_hits: 0u32,
    };
    // MELEE INFLUENCE'S WINDOW. One number, and the clause that makes it one:
    // *"Cannot refresh while active"* — so a roll that lands while it is open
    // buys nothing at all, and the arcane's real uptime is a fraction of the
    // fight rather than the 18 s its card names.
    // OPEN IF THE READER SAID SO, otherwise shut until a roll opens it.
    let mut influence_until = params.influence_open.unwrap_or(f64::NEG_INFINITY);
    // WHEN THE LAST SHOT ACTUALLY WENT OFF, which is NOT `spool_due`. That one
    // is when the next shot was DUE, so the interval is already inside it and
    // the difference is zero on every ordinary pull — right for a spool, which
    // asks "did anything intervene", and useless for a battery, which asks how
    // long the weapon spent not firing.
    let mut last_shot_t = f64::NEG_INFINITY;

    loop {
        // SAMPLE first, so a frame shows the fight as it stood BEFORE the
        // shot at `t` — the same convention the timeline buckets use.
        // Sampling here rather than on a fixed clock is deliberate: this loop
        // is the only place that advances time, and a buff can only change on
        // an event this loop drives. A gap between shots emits repeated
        // frames, which is exactly what a fight with nothing happening in it
        // looks like.
            // NOTHING TO SAMPLE WITHOUT A TRACE, and the check is HERE rather than
        // only inside: the sampler reads twenty pieces of the run, and a fight
        // nobody is replaying should not pay for passing them.
        if trace.is_some() {
            sample_frames_up_to(
                t, params, &mut trace, &mut next_frame, frame_seconds, &mut arc, &mut gal, &mut buff_stacks,
                &bar, &windows, &tendril, &crit_per_hit, &sniper_combo, combo_spec, &incarnon,
                influence_until, &target, &r, &debuffs, &others,
            );
        }
        if t >= params.duration_seconds {
            break;
        }

        // The buff bar has to be settled BEFORE the reload decision, not after:
        // whether an empty magazine reloads depends on what the NEXT shot would
        // cost, and that is a live buff read. Expiry is monotone, so the second
        // `bar.expire` below (at the post-reload t) is still correct.
        // Whether the NEXT shot costs zero ammo — it decides whether an empty
        // magazine reloads, so it has to be known before that branch.
        // A BATTERY REFILLS WHILE NOBODY IS SHOOTING, counted BEFORE anything
        // asks whether this shot can be fired — otherwise an empty magazine
        // goes straight to the reload branch and the mechanic never gets a
        // turn. The gap is the one a spool reads: `t - spool_due` is what the
        // weapon spent not firing (`data::weapons::Battery`).
        //
        // THE EMPTY CASE IS NOT HERE — that is the ordinary reload, whose
        // `reload_seconds` already IS `delay_empty + magazine/rate` (1.25 s on
        // the Shedu). What this adds is the battery filling BETWEEN shots,
        // which on a weapon slowed below one shot per `delay_partial` means it
        // never empties at all.
        if let Some(b) = params.battery {
            let idle = t - last_shot_t;
            let delay = if ammo.loaded < 1e-9 { b.delay_empty_seconds } else { b.delay_partial_seconds };
            if last_shot_t.is_finite() && b.regen_per_second > 0.0 && idle > delay {
                let gained = (idle - delay) * b.regen_per_second;
                ammo.loaded = (ammo.loaded + gained).min(ammo.cap);
            }
        }

        let next_cost = {
            let ap: &FightParams = match &params.cycle {
                Some(cy) if incarnon.in_base_form => &cy.base_form,
                _ => params,
            };
            bar.expire(t);
            for lock in &params.locked_buffs {
                if lock.mode == LockMode::Permanent {
                    match lock.buff {
                        LockedBuff::Frenzy => {
                            if ap.frenzy {
                                bar.upsert(Frenzy::permanent_buff());
                            }
                        }
                    }
                }
            }
            // Does the next shot cost anything? Feeds `can_fire` below. Capped
            // at 100%, so this is "exactly free", never "more than free".
            let eff = ammo_efficiency(
                ap.ammo_efficiency_applies,
                bar.total_contributions().ammo_efficiency
                    + weakpoint_ammo(params.weakpoint_stacks, &mut weakpoint_pile, t),
                params.arcane.ammo_efficiency,
                arc.total(&params.arcane.buffs, ArcGrant::AmmoEfficiency, t),
                crate::data::abilities::ammo_efficiency_at(&params.abilities, t),
            );
            // `ap` already picks the form whose magazine is about to be
            // checked, so this is THAT form's cost.
            if eff >= 1.0 - 1e-9 {
                0.0
            } else {
                ap.ammo_cost * (1.0 - eff)
            }
        };

        // A KILL IS A KILL WHEREVER IT CAME FROM, so on-kill stacks are read
        // off the counter rather than bumped at each of the six places one can
        // happen — a direct hit, a DoT tick, a field tick and three more. The
        // same mark-and-diff Sentient Surge's refill and the tendrils use, two
        // blocks down, and for the same reason: a list of call sites is a list
        // to forget one from.
        if r.kills != kill_buff_mark {
            let fresh = r.kills - kill_buff_mark;
            kill_buff_mark = r.kills;
            for _ in 0..fresh {
                bump_on_trigger(params, &mut buff_stacks, crate::model::BuffTrigger::Kill, t, &mut d.extra);
                // EXACT PENANCE, on the same counter and for the same reason:
                // "Kills from status effects can also trigger the effect", and
                // a DoT kill happens nowhere near the direct-hit site that
                // `instant_reload_on_headshot` is wired to.
                if let Some(chance) = params.instant_reload_on_kill {
                    if d.extra.chance(chance) {
                        ammo.instant_reload_now = true;
                    }
                }
                // PYRANA PRIME'S STREAK, off the same counter and held on the bar:
                // each kill restarts the clock `bar.expire` already runs, so a
                // lapse drops the whole streak. A kill while the second gun is
                // up starts nothing — "will not refresh the duration".
                if let Some(s) = params.kill_streak_summon {
                    use crate::model::KillStreakSummonSpec as K;
                    if bar.get(K::BUFF_ID).is_none() {
                        let streak = bar.get(K::STREAK_BUFF_ID).map_or(0, |b| b.stacks) + 1;
                        if streak >= s.kills {
                            bar.remove(K::STREAK_BUFF_ID);
                            bar.upsert(summoned_gun(s, t));
                        } else {
                            bar.upsert(kill_streak(s, streak, t));
                        }
                    }
                }
            }
        }
        ammo.resize_for_summon(params, &bar);

        // TENDRILS and the magazine they keep alive, both re-derived from the
        // counters above before anything decides to reload.
        ammo.refill_on_kill(params, &r);
        tendril.settle(params, &r);

        // …AND THE SAME MARK-AND-DIFF FOR WHAT IS STANDING. The kills were
        // counted where they happened; this is where they become a duration.
        // The DURATION is the weapon's, so it is read off whichever form
        // carries it — the kills were already filtered by the form that fired.
        let spawner = params
            .spawn_on_kill
            .or_else(|| params.cycle.as_ref().and_then(|c| c.base_form.spawn_on_kill));
        if let Some(g) = spawner {
            ghost_pile.settle(g, t, &mut r);
        }

        // HATA-SATYA's pile, the same mark-and-diff one block up with the
        // counter swapped: hits instead of kills. `r.pellets` is the count of
        // DIRECT pellet hits, which is exactly the qualifying set — "Additional
        // hits from Multishot and Punch Through also count towards the bonus",
        // and both land here.
        //
        // Read at the START of the shot, so the hit that earns a stack does not
        // carry it. That is the rule every other trigger in this loop follows.
        if params.crit_chance_per_hit.is_some() {
            crit_per_hit.settle(params, &r, &ammo);
        }

        // THE SYNDICATE GAUGE. Affinity the WEAPON earned, which is half of
        // each kill's — "Kill with weapons: Half Affinity goes to the Warframe
        // and half to the killing weapon" (wiki Affinity).
        if let Some(sy) = params.syndicate_radial {
            let fresh = r.kills - syndicate.kill_mark;
            syndicate.kill_mark = r.kills;
            if fresh > 0 && t >= syndicate.ready_at {
                // Per kill: base affinity x the level multiplier, FLOORED to a
                // whole number ("the base affinity multiplied by the Affinity
                // Multiplier value is also rounded down"), then halved.
                let per_kill = (params.target.base_affinity
                    * scaling::affinity_multiplier(params.target.level, params.target.eximus))
                .floor()
                    * WEAPON_AFFINITY_SHARE;
                syndicate.points += f64::from(fresh) * per_kill;
            }
            if syndicate.points >= sy.affinity_to_fill && t >= syndicate.ready_at {
                // Fires, then BOTH rules: points to zero and no conversion at
                // all until the cooldown is out.
                syndicate.points = 0.0;
                syndicate.ready_at = t + sy.cooldown_seconds;
                fire_syndicate_radial(
                    &sy,
                    &mut r,
                    rec,
                    &mut target,
                    &mut debuffs,
                    &mut gal,
                    &mut arc,
                    params,
                    &mut d.status,
                    t,
                );
            }
        }

        // Phase transitions and reloads.
        // A WEAPON WITH NO MAGAZINE NEVER EMPTIES, so it never reaches the
        // reload below. Topping it up here rather than making `can_fire` lie is
        // what keeps the whole reload path — the event, the animation, the
        // buffs, the windows, the ammo draw — from happening at all, which is
        // the only way to be sure none of it fires: there is no list of
        // reload-keyed effects to keep in step, because the reload does not
        // occur.
        //  rather than the active form: having a clip is a property of
        // the WEAPON, not of which of its forms is being fired, and this is
        // read before the form is decided anyway.
        if params.no_magazine {
            ammo.loaded = ammo.cap;
        }
        // …AND A CLOCK ENDS THE OTHER KIND, before the magazine block below,
        // which is entirely about a magazine a melee weapon does not have.
        // *"activate Incarnon Form for 90 seconds"* — the window runs from the
        // swing that armed it, is not refreshed by anything, and re-arms the
        // same way it armed the first time.
        if let Some(cy) = &params.cycle {
            if let Ends::After(_) = cy.ends {
                if !incarnon.in_base_form && t >= incarnon.incarnon_until {
                    incarnon.in_base_form = true;
                    record_weapon(params, rec, &ammo, &incarnon);
                }
            }
        }
        match charge_magazine_cycle(
            params, rec, rng, next_cost, &mut t, &mut r, &mut ammo, &mut incarnon, &mut double_tap,
            &mut windows, &mut arc, &mut buff_stacks, &mut rs_armed, &mut opening_closed,
            &mut field_duration_boost,
        ) {
            Flow::Continue => continue,
            Flow::Break => break,
            Flow::Go => {}
        }

        // Active-phase view: the base form's panel during the rebuild
        // phase, the outer params otherwise. Target/aim/locks are shared
        // from the outer params.
        let ap: &FightParams = match &params.cycle {
            Some(cy) if incarnon.in_base_form => &cy.base_form,
            _ => params,
        };
        // The instance total and its SHAPE (Toxin's shield bypass, the
        // vulnerability column) are derived PER STAGE now — each attack part
        // has its own vector — so only the vector and ModifiedBase survive at
        // pellet scope.
        let (qvec, modded_base) = if incarnon.in_base_form {
            let p = base_pre.as_ref().expect("cycle state needs base pre");
            (&p.0, p.2)
        } else {
            (&main_pre.0, main_pre.2)
        };
        // ---- THE MELEE SWING THIS SHOT IS -------------------------------
        //
        // A gun's script is empty and every line below is a no-op for it: the
        // swing is `None`, the multiplier is 1.0, and the counter never moves.
        let swing = ap.combo_script.get(melee.swing_idx % ap.combo_script.len().max(1)).cloned();
        // THE COUNTER, BEFORE THIS SWING. A heavy attack reads it and then
        // empties it, so the multiplier it pays is the one that was standing
        // when the trigger went down — the same rule the game states by
        // spending "all or part of the combo counter" as part of the attack.
        // …OR THE CLOCK RAN OUT ZERO. *"A zero or negative combo duration
        // prevents increasing the combo counter"* — so the counter is cleared
        // HERE, upstream of the one place it is read, rather than by each of
        // the four swings that earn into it remembering to ask.
        if t > melee.combo_expiry || ap.combo_frozen {
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
            ap.initial_combo + buff_total(ap, crate::model::BuffGrant::InitialCombo, &mut buff_stacks, t);
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
        let tennokai = ap.tennokai.enabled && t < melee.tennokai_until;
        let tennokai_heavy = tennokai && !ap.spends_combo && ap.heavy.is_some();
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
        let is_slam = ap
            .radial
            .as_ref()
            .is_some_and(|r| r.blast_kind == crate::model::BlastKind::Slam);
        let mode_damage = 1.0 + if is_slam { ap.slam_damage } else { 0.0 };
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
            ap.heavy.map_or(1.0, |h| h.multiplier)
                * combo_multiplier
                // KILLING BLOW ON A LIGHT FORM'S FREE HEAVY. The card's bucket
                // is the base-damage one, so what it is worth here is the
                // RATIO that bucket grows by — `heavy_attack_base_damage` reads
                // zero on a form that does not spend the counter, which this
                // one is.
                * (1.0 + ap.base_damage_bonus + ap.heavy_attack_damage)
                / (1.0 + ap.base_damage_bonus)
                // …AND ONLY WHERE THE CARD PAYS IT. Truth's Flame's bonus is
                // the CHAINED window's; every other card's is unconditional.
                * (1.0
                    + if ap.tennokai.damage_needs_chain && !tennokai_was_chained {
                        0.0
                    } else {
                        ap.tennokai.damage
                    })
        } else {
            swing.as_ref().map_or(1.0, |h| h.multiplier)
                * if ap.spends_combo { combo_multiplier } else { 1.0 }
                * mode_damage
        };
        // WHO THIS SWING REACHES. A `360deg` swing is a spin and takes
        // everything within the weapon's range; an ordinary one sweeps in
        // front. Empty for a gun, which never asks.
        let melee_struck = match &swing {
            Some(h) if ap.follow_through.is_some() => params
                .melee_struck(h.all_around, buff_total(ap, crate::model::BuffGrant::MeleeRange, &mut buff_stacks, t)),
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
        let unswung = ap.unswung_fraction.clamp(0.0, 1.0);
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
        // …AND THE CO BRACKET FOLLOWS THE WEAPON'S HALF. `ap`'s fraction is
        // read against the UNSWUNG base; once the swing has landed on one half
        // only, the share the term reads is a different number.
        let co_base = if swing_eff > 0.0 {
            ap.co_base.against(ap.co_base.of() * swing_eff / swing_on_weapon)
        } else {
            ap.co_base
        };
        // The per-projectile vectors belong to the FORM that is firing, like
        // everything else at this scope. A cycle whose base form has them and
        // whose Incarnon form does not simply reads an empty slice there.
        let (variants, variant_rad): (&[_], &[_]) = if incarnon.in_base_form {
            match params.cycle.as_ref() {
                Some(_) => (&base_variants, &base_variant_rad),
                None => (&main_variants, &main_variant_rad),
            }
        } else {
            (&main_variants, &main_variant_rad)
        };

        // Status events scheduled before this shot land first.
        process_ticks(
            &mut debuffs,
            &mut gal,
            &mut arc,
            t + 1e-9,
            &mut target,
            params,
            ap,
            &mut r,
            rec,
            &mut d.status,
            &params.target,
            0,
        );

        // Timed buffs (Frenzy) lapse before this shot reads the bar;
        // Permanent locks re-assert — only in phases where the perk exists
        // (Frenzy belongs to the base form).
        bar.expire(t);
        for lock in &params.locked_buffs {
            if lock.mode == LockMode::Permanent {
                match lock.buff {
                    LockedBuff::Frenzy => {
                        if ap.frenzy {
                            bar.upsert(Frenzy::permanent_buff());
                        }
                    }
                }
            }
        }

        // Crit chance: base + Enervate stacks (attacker BuffBar) + Weakened
        // stacks (target DebuffBar: flat crit chance received, weapon direct
        // damage only — which our shots are).
        let contribs = bar.total_contributions();
        // Ammo: consume (1 - efficiency) per shot; Frenzy's +100% efficiency
        // zeroes consumption (unless this magazine is charge-backed).
        // Efficiency is a DIVIDED COST, not a chance to save a round: the cost
        // is `1 x (1 - efficiency)` and the magazine keeps the fraction (wiki
        // Energized Munitions: "dividing the ammo cost … and keeps track of the
        // fractions as well"). A partial round still fires — the Exergis's
        // 1-round magazine takes four 0.25 shots — which is why the gate above
        // is "anything left" rather than "a whole round left".
        //
        // A lapsing buff does NOT strand the remainder — ✅ measured
        // (MEASUREMENTS M14): the shot fires at full cost off whatever is left,
        // the counter goes NEGATIVE, and the reload carries that debt into the
        // fresh magazine (see the `+=` above).
        // BuffBar (Frenzy) + static arcane (Akimbo Slip Shot, assumed-max) +
        // live arcane stacks (Primary Crux). Summed and capped by
        // `ammo_efficiency`, which is also what `next_cost` above reads — one
        // definition, so the two cannot drift apart.
        // …AND DEATH KNELL'S: on a one-round magazine, the reload not happening.
        let efficiency = ammo_efficiency(
            ap.ammo_efficiency_applies,
            contribs.ammo_efficiency
                + weakpoint_ammo(params.weakpoint_stacks, &mut weakpoint_pile, t),
            params.arcane.ammo_efficiency,
            arc.total(&params.arcane.buffs, ArcGrant::AmmoEfficiency, t),
            crate::data::abilities::ammo_efficiency_at(&params.abilities, t),
        );
        // Final Fusillade's gate, read BEFORE the round is spent: this pull is
        // the magazine's last round if there is at most one left to fire. On a
        // charge-backed form `multishot_on_last_round` is 0.0 anyway (the
        // evolution loader dropped it), so the flag costs nothing there.
        //
        // On a BURST weapon the window is the last BURST, not the last round —
        // Forceful Finality reads "+5 Base Multishot on final magazine burst",
        // and a Burston's final burst is three rounds. Taking the wiki
        // literally as one round would have understated a full magazine's
        // pellets by a fifth (42 + 3x6 = 60 real, against 44 + 6 = 50), which
        // is far too big to wave through as a rounding difference.
        let last_n = ap.burst.map_or(1.0, |b| f64::from(b.count));
        // …AND THE WINDOW IS THE ACTIVE MAGAZINE'S, whichever that is.
        // `in_base_form` is only ever true inside an Incarnon CYCLE, so this
        // branch must not read "the cycle's base phase" against "everything
        // else": everything else includes a plain base-form run, which is how a
        // `base`-mode board row is played and how anyone measures the weapon on
        // its own. A burst window written for the cycle silently becomes one
        // round outside it — 5 pellets a magazine instead of 15 on a Burston.
        let mag_left = if incarnon.in_base_form { incarnon.base_magazine } else { ammo.loaded };
        let last_round = mag_left <= last_n + 1e-9;
        // THE CHAMBER FAMILY'S GATE, and it is READ AFTER THE ROUND IS PAID
        // FOR. Both cards say "+X% Damage on first shot in Magazine" and both
        // pages say what that really means: the bonus lands *"as long as the
        // magazine counter is at Max Magazine - 1 AFTER a shot is fired"*, and
        // *"the buff doesn't apply on a completely full one"*.
        //
        // On an ordinary weapon that is exactly the first shot out of a fresh
        // magazine — full goes to full-1 — so the plain case needs no thought.
        // It is AMMO EFFICIENCY that makes the wording load-bearing: a free
        // shot leaves the counter where it was, so a FULL magazine pays
        // nothing however often you fire it and one sitting at max-1 pays
        // every single shot. That is the wiki's Vulkar example, and it is the
        // reason this cannot be `mag_left == mag_max` at the top of the pull.
        //
        // The cost is `ap.ammo_cost * (1 - efficiency)`, spelled the same way
        // the spend below spells it — one expression, so the reading and the
        // payment cannot drift.
        let mag_max = if incarnon.in_base_form {
            params.cycle.as_ref().map_or(0.0, |c| c.base_form.magazine_size)
        } else {
            ammo.cap
        };
        let first_round = ap.first_round_damage > 0.0
            && ((mag_left - ap.ammo_cost * (1.0 - efficiency)) - (mag_max - 1.0)).abs() < 1e-9;
        // The round itself is spent BELOW, once the multishot roll is known:
        // Plentiful Mayhem makes the extra projectiles cost ammo too, so the
        // draw cannot be settled before the roll.

        // CRIT SOURCES SPLIT BY KIND, because an attack part has its OWN base
        // crit stats (§7 Radial): a RELATIVE bonus joins the crit bucket and
        // therefore scales each part's own base, while an ABSOLUTE add (a
        // target-side debuff, a flat grant) lands the same on every part. Both
        // reach the explosion — the crit things a radial loses are the
        // body-part/headshot layer, which it has no hit location for, and
        // Puncture's Weakened, which the wiki excludes from AoE by name.
        //
        // Absolute, shared by every stage:
        // …AND A WARFRAME'S. Wrathful Advance is *"a flat value applied AFTER
        // mods"*, which is this bucket exactly: it lands on every attack part
        // and is never multiplied by a crit-chance card.
        let flat_crit = contribs.flat_crit_chance
            + crate::data::abilities::flat_crit_at(&params.abilities, t);
        let weakened_cc = WEAKENED_FLAT_CC_PER_STACK * debuffs.weakened_active(t) as f64;
        // Relative, shared by every stage: Crosshairs' on-headshot buff and
        // its per-stack-expiry kill stacks (assumes constant aiming), plus the
        // arcane's assumed-max conditionals (Overcharge/Outburst).
        let crit_chance_relative = params.crit_chance_on_headshot.map_or(0.0, |b| {
            if t < windows.crit_on_headshot {
                b.value
            } else {
                0.0
            }
        }) + params.crit_chance_stack.as_ref().map_or(0.0, |s| {
            windows.crit_on_headshot_stacks.retain(|&e| e > t);
            s.per_stack * windows.crit_on_headshot_stacks.len() as f64
        }) + params.arcane.crit_chance_relative
            // SENTIENT SURGE: "Additive to other crit chance and status chance
            // mods", so it belongs in the RELATIVE bucket beside Pistol
            // Gambit's — multiplying the unmodded base, not the modded one.
            + params.crit_chance_per_tendril * f64::from(tendril.count)
            // HATA-SATYA: "additive with similar mods. For example, a max rank,
            // max bonus Hata-Satya and Point Strike will have a 30% × (1 + 500%
            // + 150%) critical chance" — the wiki does the bracket for us, and
            // it is the same one Point Strike is in.
            + params
                .crit_chance_per_hit
                .map_or(0.0, |c| c.bonus(crit_per_hit.stacks))
            // BLOOD RUSH. `Crit Chance = Weapon Crit Chance x [1 + Mod Crit
            // Bonus + Blood Rush Bonus x (Combo Multi - 1)] + Static Crit
            // Bonus` (wiki, verbatim) — so it belongs in this bracket beside
            // Point Strike's, multiplying the UNMODDED base, and nowhere else.
            //
            // IT READS THE COUNTER AND NEVER SPENDS IT, which is why it is
            // worth everything in the four combo modes and nothing in the two
            // heavy ones: there the counter is emptied by the swing that reads
            // it, so it is standing at the floor when the next one starts.
            + ap.crit_chance_per_combo * (combo_multiplier - 1.0)
            // …AND EVERY STACKING GRANT OF IT, the bracket Prolific
            // Perforation's card puts itself in by naming Pistol Gambit.
            + buff_total(ap, crate::model::BuffGrant::CritChance, &mut buff_stacks, t);
        // VICIOUS PROMISE, both halves of it. VERBATIM (wiki, Paris Incarnon
        // Genesis): "Enemies are undamaged as long as their health and shield
        // have not been damaged. Damaging Overguard is not taken into account."
        // So OVERGUARD IS EXCLUDED from the test — a target being chewed
        // through its overguard is still undamaged, and reading all three pools
        // would have switched this off on the first shot of every Eximus fight.
        //
        // Read per SHOT beside `effective_cc`, which is where the weapon's crit
        // chance is decided; the grants are already converted by `resolve` into
        // the post-mod numbers the card's "Base" wording earns.
        let undamaged = (ap.crit_chance_on_undamaged > 0.0 || ap.crit_damage_on_undamaged > 0.0)
            && target_undamaged(&target, &params.target);
        // THE ARCANE'S STATUS BONUS, hoisted to SHOT level so a derived stat can
        // read the live status chance the same way it reads the live crit one.
        // The pellet loop below re-reads it for its own roll; this is the same
        // number, one scope out.
        // DEATH KNELL'S TWO NUMBERS, read once per SHOT — every stacking buff
        // in this loop follows that rule. Both add to the FINISHED value.
        let (weakpoint_cd, weakpoint_sc) = params.weakpoint_stacks.map_or((0.0, 0.0), |w| {
            let n = f64::from(weakpoint_pile.current(t, w.duration_seconds));
            (w.crit_multiplier * n, w.status_chance * n)
        });
        let sc_arc_shot = arc.total(&params.arcane.buffs, ArcGrant::StatusChance, t)
            + params.sc_per_tendril * f64::from(tendril.count)
            // WEEPING WOUNDS, the same sentence on the status side: `Status
            // Chance = Weapon Status Chance x [1 + Mod Status Bonus + Weeping
            // Wounds Bonus x (Combo Multi - 1)]`. It rides `sc_arc_shot`
            // because that is this loop's name for "relative status the panel
            // could not fold in", which is exactly what a live counter is.
            + ap.status_chance_per_combo * (combo_multiplier - 1.0)
            // ENDURING AFFLICTION, whose gate is a status the engine tracks:
            // every heavy slam forces `Lifted`, so from the second slam on the
            // target is carrying it and the card pays.
            + if debuffs.lifted.is_some_and(|e| e > t) { ap.status_chance_on_lifted } else { 0.0 }
            // …AND AN ON-KILL STATUS BUFF (Galvanized Elementalist), which is
            // relative like every other card in this bracket.
            + buff_total(ap, crate::model::BuffGrant::StatusChance, &mut buff_stacks, t);
        // HIGH GROUND, LIVE: "+25% of CURRENT Status Chance". The panel folded
        // in what it could see; this takes that back and pays what the shot
        // actually has, which is the panel's status plus whatever the arcanes
        // are adding right now.
        let derived_cc = match ap.derived_crit_from_status {
            Some((rate, cap, folded)) => {
                let live_sc = ap.status_chance + ap.base_status_chance * sc_arc_shot;
                (rate * live_sc).min(cap) - folded
            }
            None => 0.0,
        };
        let effective_cc = ap.base_crit_chance
            + flat_crit
            + weakened_cc
            + ap.unmodded_crit_chance * crit_chance_relative
            + derived_cc
            + if undamaged { ap.crit_chance_on_undamaged } else { 0.0 };

        // Live fire rate (base + Pressurized Magazine's on-reload buff, ×
        // the BuffBar multiplier) — schedules shots below and gates
        // Hemorrhage's below-2.5 doubled chance.
        let fr_reload_add = match ap.fire_rate_on_reload {
            Some(b) if t < windows.fire_rate_after_reload => b.value,
            _ => 0.0,
        };
        // A LOCKED fire rate is the weapon's default and nothing else: not
        // Pressurized Magazine's on-reload add, not Frenzy's x2.5 in the bar.
        let live_rate = if params.locks("fire_rate") {
            ap.fire_rate
        } else {
            (ap.fire_rate + fr_reload_add) * contribs.fire_rate_multiplier
        };
        // Deadly Efficiency's live share of the BASE-DAMAGE bucket. Zero until
        // a reload has finished, and zero again when the window closes.
        let bd_reload_add = match ap.base_damage_on_reload {
            Some(b) if t < windows.base_damage_after_reload => b.value,
            _ => 0.0,
        };
        // …and Eximus Advantage's share of the same bucket. "Stacks additively
        // with base damage bonuses like Hornet Strike", so it joins here rather
        // than forming a factor of its own.
        let bd_eximus_add = match ap.base_damage_on_eximus_weakpoint {
            Some(b) if t < windows.base_damage_eximus => b.value,
            _ => 0.0,
        };

        // Multishot: pellets this pull = floor + fractional chance; every
        // pellet is an independent damage instance. Earned Galvanized
        // stacks and arcane multishot stacks (Conjunction Voltage: a
        // RELATIVE bonus × base pellets) add live.
        // ...unless MULTISHOT IS LOCKED, in which case the weapon fires its
        // default pellet count and nothing adds to it — an Acuity's sentence is
        // "set to its default ignoring other bonuses", and an arcane's stacks
        // are other bonuses. `resolve` has already emptied the panel's own
        // buckets; this is the live half it cannot reach.
        let ms_locked = params.locks("multishot");
        let ms_eff = ap.multishot
            + params
                .multishot_stack
                .as_ref()
                .map_or(0.0, |s| s.per_stack * gal.multishot.current(t, s.duration) as f64)
            + if ms_locked {
                0.0
            } else {
                ap.base_multishot * arc.total(&params.arcane.buffs, ArcGrant::Multishot, t)
            }
            // Final Fusillade: a FLAT add on the magazine's last round. It
            // joins `ms_eff` rather than the multishot BUCKET because the
            // evolution grants multishot outright ("+3 Multishot"), not a
            // percentage of the weapon's base.
            + if last_round { ap.multishot_on_last_round } else { 0.0 }
            // FORCEFUL FINALITY IS THE OTHER BRACKET, and the card says which:
            // "+5 BASE Multishot on final magazine burst", with the wiki noting
            // on that same row that it is "added before mods, and is thus
            // multiplied by multishot bonuses". So for that burst the weapon's
            // base pellet count IS higher, and everything relative reads the
            // raised number — the mod bucket AND the live grants below it.
            //
            // The bucket is recovered as `multishot / base_multishot`, the
            // ratio the panel already resolved, rather than carried a second
            // time: two copies of one factor is how they come to disagree.
            // `base_multishot` is a weapon stat and never zero.
            + if last_round && ap.base_multishot_on_last_round > 0.0 && !ms_locked {
                ap.base_multishot_on_last_round
                    * (ap.multishot / ap.base_multishot.max(1e-9)
                        + arc.total(&params.arcane.buffs, ArcGrant::Multishot, t))
            } else {
                0.0
            }
            // Stormburst: "+0.4 Multishot", flat — same reason Final Fusillade
            // sits here rather than in the bucket above.
            + buff_total(ap, crate::model::BuffGrant::FlatMultishot, &mut buff_stacks, t)
            // BLAZING BARREL, both of its shapes, and they are two brackets.
            //
            // "+0.05 BASE Multishot" is added before mods and is therefore
            // MULTIPLIED by them — the bucket recovered as `multishot /
            // base_multishot`, exactly as Forceful Finality does two arms up
            // and for the same quoted reason. "+5% Multishot" is what a
            // multishot MOD grants, so it is a share of the weapon's base.
            //
            // Both are silenced by an Acuity lock, like every other live
            // multishot grant here.
            + if ms_locked {
                0.0
            } else {
                buff_total(ap, crate::model::BuffGrant::BaseMultishot, &mut buff_stacks, t)
                    * (ap.multishot / ap.base_multishot.max(1e-9))
                    + buff_total(ap, crate::model::BuffGrant::Multishot, &mut buff_stacks, t)
                        * ap.base_multishot
            };
        let rolled = ms_eff.floor() as u32 + d.spine.chance(ms_eff.fract()) as u32;
        // DOUBLE TAP, computed ONCE for the whole pull and applied to every
        // pellet of it. VERBATIM: "the bonus is applied on hit to all pellets as
        // damage * 20% * (hits - 1)", worked through on the card as "with a
        // modded multishot of 3, the first trigger pull would do +40% bonus
        // damage, the second +100%, the third +160%" — so the count INCLUDES
        // this pull's own hits, and one is taken off, which is why an unmodded
        // weapon gets nothing from its first shot.
        //
        // Every pellet reaching the one target IS a hit here ("additional hits
        // caused by punch through or Multishot will allow each bullet to
        // trigger multiple stacks"), so the pull's hits are its pellets.
        // SYNTH CHARGE's window, read off the SAME gate Final Fusillade uses —
        // so a burst weapon's "last round" is its last BURST, and a cycle's
        // window is whichever magazine is actually being fired.
        let sc_mult = if last_round { 1.0 + ap.last_round_damage } else { 1.0 };
        // THE CHAMBERS' multiplier, on the magazine's FIRST round only. The two
        // cards are already summed into one number by `resolve` — "stacks
        // additively … for up to 140% bonus damage" — so this is one factor
        // beside Synth Charge's rather than a second bracket.
        let cc_mult = if first_round { 1.0 + ap.first_round_damage } else { 1.0 };
        let dt_mult = match ap.consecutive_hit_damage {
            Some((per_stack, max_stacks, duration)) => {
                if t >= double_tap.expiry {
                    double_tap.hits = 0;
                }
                // AN EXPLODING PROJECTILE IS TWO HITS where the weapon says so
                // — its collision and its explosion, +40% a projectile at rank
                // 3 (M102). Only the aimed landing counts; a bounce adds none.
                let per_projectile = if ap.consecutive_hit_radial_only && ap.radial.is_some() { 2 } else { 1 };
                let hits = double_tap.hits + rolled * per_projectile;
                double_tap.hits = hits;
                double_tap.expiry = t + duration;
                1.0 + per_stack * f64::from(hits.saturating_sub(1).min(max_stacks))
            }
            None => 1.0,
        };
        // CONTINUOUS weapons MERGE. VERBATIM (wiki Multishot §Continuous
        // Weapons): "additional beams that hit the same target instead merge
        // into a singular damage tick. This combined tick has damage AND Status
        // Chance equal to the SUM of the individual beams, but the Critical
        // Chance is still equal to that of a single beam."
        //
        // The multiplier is the ROLLED count, not the fractional average, so
        // damaging statuses are "affected TWICE by multishot" while forced
        // procs are "applied after the damage instances are merged", one per
        // tick.
        //
        // PLENTIFUL MAYHEM, continuous branch: "In the Incarnon form, instead
        // of increasing the damage of additional projectiles created by
        // multishot, all multishot bonuses are increased by 60%." A merged beam
        // has no separable generated projectile, so the perk scales the
        // multishot BONUS — and the two readings agree in expectation:
        //   base form   1 + (1+v)(M-1)     [1 original + (M-1) generated]
        //   Incarnon    1 + (1+v)(M-1)     [merged, so damage ∝ multishot]
        // The identity needs base multishot = 1; both Torid forms are.
        let merge_bonus = if ap.multishot_ammo_bonus > 0.0 {
            let base_ms = ap.base_multishot.max(1.0);
            base_ms + (rolled.max(1) as f64 - base_ms) * (1.0 + ap.multishot_ammo_bonus)
        } else {
            rolled.max(1) as f64
        };
        // MULTISHOT THAT IS NOT MORE PROJECTILES. VERBATIM (wiki `Arbucep`):
        // "Multishot increases weapon damage instead of creating additional
        // projectiles. Damage bonus is multiplicative to other sources of
        // damage." So the COUNT stays the weapon's own — which is what keeps
        // its six elements six — and the bucket becomes a factor instead.
        //
        // The factor is the rolled count over the weapon's own, so an unmodded
        // weapon pays 1.0 and every multishot source scales it from there. It
        // is applied as its own multiplier and never joins a bucket, which is
        // what "multiplicative to other sources of damage" says.
        let own_pellets = ap.base_multishot.max(1.0);
        let ms_damage = if ap.multishot_adds_damage {
            (ms_eff / own_pellets).max(1.0)
        } else {
            1.0
        };
        let (n_pellets, mut beam_merge) = if ap.continuous {
            (1, merge_bonus)
        } else if ap.multishot_adds_damage {
            (own_pellets.round() as u32, 1.0)
        } else {
            (rolled, 1.0)
        };
        // Ammo, settled now that the roll is known.
        //
        // The ROUND itself always comes from the magazine and always takes ammo
        // efficiency — that path is unchanged by any perk.
        //
        // PLENTIFUL MAYHEM bills the EXTRA projectiles on top, one round
        // each, and the draw follows the RAW rolled count rather than the
        // 60%-scaled one: the bonus is paid in damage, not billed twice. Ammo
        // efficiency does NOT reach the surcharge (measured), so the magazine
        // round keeps its discount, every generated projectile pays full price,
        // and a 100% efficiency source does not make multishot free.
        //
        // AMMO STARVATION IS REAL, and is why this is a loop rather than one
        // subtraction: the projectiles are produced in order, each paying as it
        // goes, and one that cannot pay IS NOT FIRED. A 4-multishot pull
        // against 3 charges fires three pellets and lands on empty.
        // `ammo_cost` scales the whole spend: efficiency is a DISCOUNT on the
        // cost, not a separate round. A beam paying 0.5 with 20% efficiency
        // spends 0.4, which is what "0.5 ammo per trace" plus an efficiency
        // mod has to mean.
        let spend = ap.ammo_cost * (1.0 - efficiency);
        if incarnon.in_base_form {
            incarnon.base_magazine -= spend;
        } else {
            ammo.loaded -= spend;
        }
        ammo.rounds_this_mag += 1;
        // THE TRIGGER PULL ITSELF — the row every pellet, every explosion and
        // every status this shot goes on to cause points back at
        // (`record::Event::cause`). It is also where the weapon's state is
        // stamped, so a row four seconds later still says which form fired it
        // and what was left in the magazine at the time.
        if rec.is_on() {
            record_weapon(params, rec, &ammo, &incarnon);
            // …AND WHAT THE SHOOTER HAS UP. Sampled at the SHOT, which is the
            // one place in this loop that holds every local the sampler reads —
            // the same reason `weapon_now!` is a macro. A row between two shots
            // carries the count as of the shot before it, which is exact for
            // everything a shot changes and up to one shot stale for a buff
            // that expires on a clock of its own.
            let stacks = sample_stacks(
                params, &rec_roster, t, &mut arc, &mut gal, &mut buff_stacks,
                &windows.crit_on_headshot_stacks, windows.crit_on_headshot, windows.fire_rate_after_reload,
                windows.base_damage_after_reload, windows.base_damage_eximus,
                windows.streak, tendril.count, crit_per_hit.stacks, &bar,
                combo_at(combo_spec, params.combo_held, sniper_combo.count, sniper_combo.last_hit, t),
                incarnon.incarnon_until,
                influence_until,
            );
            rec.set_stacks(stacks);
            rec.begin_shot(t, n_pellets);
        }
        // BLAZING BARREL: the round is SPENT, so it was fired. Here and not in
        // the pellet loop — one shot is one stack however many pellets it threw
        // — and after `ms_eff` was rolled, so the shot that earns the stack does
        // not carry it.
        bump_buffs!(params, buff_stacks, rec_buff_index, rec, crate::model::BuffTrigger::Firing, t, d.spine);
        // READY RETALIATION IS ARMED THE MOMENT THE MAGAZINE RUNS OUT, which is
        // HERE — the shot that spends the last round — and not at the reload
        // that follows. The two are the same instant for a reload and are not
        // the same instant for a TRANSFORM: the shot that fills the gauge can
        // also be the shot that empties the magazine, and the transform is
        // decided before any reload is. Arming at the reload would have left
        // that transform at the plain speed, which is the case the owner used
        // to state the rule.
        if !can_fire(if incarnon.in_base_form { incarnon.base_magazine } else { ammo.loaded }, ap.ammo_cost) {
            rs_armed = true;
        }
        // AN ORB ATTACK FIRES NO PELLETS — when the TRIGGER is what deploys it.
        // The shot settles no collision and no explosion, because everything it
        // deals is delivered later by the orb from wherever the orb is.
        //
        // A METER MOVES THE THROW OFF THE TRIGGER, and then the trigger goes
        // back to doing what it does: in a Tome's CYCLE the weapon is firing
        // its primary the whole time and the orb goes out on the clock, so the
        // pellets are the primary's and this must not touch them. Zeroing them
        // regardless was worth the whole base form — a cycle reporting 169 DPS
        // against the 1,419 its gun alone deals.
        //
        // The COUNT is zeroed rather than the loop jumped past, and that is not
        // a style choice — a `continue` here skips the rest of the shot's own
        // body, which is where the clock, the magazine and the reload live. It
        // hung the engine on the first run.
        let mut n_pellets = if ap.orb.is_some() && ap.meter.is_none() { 0 } else { n_pellets };
        // A SWING THAT LANDS TWICE IS TWO INSTANCES. `Hits = { 1, 2 }` in the
        // wiki's own module — Crushing Ruin's forward combo lands its second
        // 100% twice and Shattered Village lands two 50% spins per attack — and
        // each is its own crit roll, its own status roll and its own combo
        // point, which is what makes it a COUNT here rather than a multiplier
        // on the damage.
        //
        // It rides the pellet loop because that loop already means "this many
        // instances of this attack", which is the same thing multishot means.
        // Melee has no multishot mod in its pool, so the two can never compete.
        // …AND A TENNOKAI SWING LANDS ONCE — see `swing_instances`.
        if let Some(h) = &swing {
            let n = swing_instances(h.hits, tennokai_heavy);
            if n > 1 {
                n_pellets = n_pellets.saturating_mul(n);
            }
        }
        if ap.multishot_ammo_bonus > 0.0 && rolled > 1 {
            // `ammo_efficiency_applies == false` IS the charge-backed marker —
            // such a magazine is "outside the ammo economy entirely", so it has
            // no Capacity behind it and the surcharge comes out of the charge
            // pool itself. That is what shortens the Incarnon window.
            let charge_backed = !ap.ammo_efficiency_applies;
            let mut afforded = 0u32;
            for _ in 0..rolled - 1 {
                let pool = if charge_backed {
                    if incarnon.in_base_form { &mut incarnon.base_magazine } else { &mut ammo.loaded }
                } else {
                    // From CAPACITY. With infinite reserves — which the Incarnon
                    // cycle's base phase always assumes — nothing can starve,
                    // which is correct rather than missing.
                    if params.infinite_reserve {
                        afforded += 1;
                        continue;
                    }
                    &mut ammo.reserve
                };
                if *pool < 1.0 - 1e-9 {
                    break;
                }
                *pool -= 1.0;
                afforded += 1;
            }
            // A beam merges its multishot into ONE instance, so starvation
            // shows up as a smaller merge multiplier, not as fewer instances.
            if ap.continuous {
                let base_ms = ap.base_multishot.max(1.0);
                let live = (1 + afforded) as f64;
                beam_merge = base_ms + (live - base_ms) * (1.0 + ap.multishot_ammo_bonus);
            } else {
                n_pellets = 1 + afforded;
            }
        }
        // The damage ramp, evaluated once per tick. A FINAL multiplier on the
        // instance and NOT on ModifiedBase: it is a transient scaling of the
        // beam's output, not a weapon-stat change, so the status payloads are
        // left out of it. Nothing sources that either way — flagged in
        // MECHANICS — but it is a sub-2% question on sustained fire, unlike the
        // merge above.
        let beam_ramp = if ap.continuous {
            beam.tick(t, 1.0 / live_rate.max(1e-9), ap.beam_ramp_floor)
        } else {
            1.0
        };
        let (mut any_head, mut any_big) = (false, false);
        let headshots_before = r.headshots;
        let pellets_before = r.pellets;
        // THE HEADSHOT-DAMAGE BRACKETS as of this shot: the field's head ladder
        // below, and what a Tesla arc is worth on a neighbour's head.
        let (shot_hb, shot_hi) = {
            let streak = match params.headshot_streak {
                Some(s) if t < windows.streak => s.value,
                _ => 0.0,
            } + buff_total(ap, crate::model::BuffGrant::HeadshotDamage, &mut buff_stacks, t);
            if ap.headshot_bonus_multiplicative {
                (params.arcane.headshot_multiplier_bonus + streak, ap.headshot_damage_bonus)
            } else {
                (
                    params.arcane.headshot_multiplier_bonus + streak + ap.headshot_damage_bonus,
                    0.0,
                )
            }
        };
        let shot_head_landing = (1.0 + shot_hb) * (1.0 + shot_hi);
        // Field ticks due before this shot, with the buff state as of now.
        field_ctx = FieldCtx {
            flat_crit,
            crit_chance_relative_mods: crit_chance_relative - params.arcane.crit_chance_relative,
            base_damage_add_mods: bd_reload_add
                + bd_eximus_add
                + ap.compression_base_damage
                + buff_total(ap, crate::model::BuffGrant::BaseDamage, &mut buff_stacks, t)
                + buff_total(ap, crate::model::BuffGrant::FlatBaseDamage, &mut buff_stacks, t),
            // The same ladder the pellet loop builds below, on the aimed body's
            // own head — see the field's note on why it is computed twice
            // rather than shared.
            head_factor: {
                let m = params
                    .body_parts
                    .iter()
                    .find(|p| p.is_head)
                    .map_or(1.0, |p| p.multiplier);
                let m = ap.headshot_multiplier.unwrap_or(m);
                (m + 1.5 * ap.weakpoint_damage) * (1.0 + shot_hb) * (1.0 + shot_hi)
            },
            head_landing: shot_head_landing,
        };
        process_field_ticks(
            &mut fields,
            &mut debuffs,
            &mut gal,
            &mut arc,
            t,
            &mut target,
            params,
            field_ap,
            &field_ctx,
            &mut r,
            rec,
            d,
            &mut others,
        );
        // JAHU CANTICLE. Every kill takes a share off the armour of every enemy
        // inside Affinity Range — which is measured from the PLAYER, not from
        // the corpse (wiki `Affinity`: the squad shares within a 50 m radius),
        // so which body died does not matter and a count is enough.
        //
        // THE SHARES COMPOSE rather than adding: each kill removes a share of
        // what is LEFT, which is the rule every other strip in this engine
        // follows and the only one under which repeated kills cannot take
        // armour past zero. Two kills at 5% leave 0.9025 of it, not 0.90.
        //
        // NO CLOCK. The card states no duration, so what it takes it keeps.
        if let Some((share, radius)) = ap.strip_on_kill_in_range {
            let fresh = r.kills.saturating_sub(strip_kills_seen);
            strip_kills_seen = r.kills;
            if fresh > 0 && share > 0.0 {
                let keep = (1.0 - share).powi(fresh as i32);
                if crate::rules::space::gap(params.player_at, params.target_at) <= radius {
                    debuffs.canticle_armor_strip =
                        1.0 - (1.0 - debuffs.canticle_armor_strip) * keep;
                }
                for (bi, spec) in params.others.iter().enumerate() {
                    if crate::rules::space::gap(params.player_at, spec.at) > radius {
                        continue;
                    }
                    if let Some(SpreadFoe { debuffs: fd, .. }) = others.get_mut(bi) {
                        fd.canticle_armor_strip =
                            1.0 - (1.0 - fd.canticle_armor_strip) * keep;
                    }
                }
            }
        }
        // WHAT THE BODIES DROPPED since this was last looked at — ONE roll per
        // kill IN REACH, read by everything that cares (docs/MECHANICS.md
        // §"THE AMMO ECONOMY"). Rolled whether or not anything reads it, which
        // is what keeps two builds of one weapon on the same dice.
        let (mut dropped_primary, mut dropped_secondary) = (0u32, 0u32);
        for _ in 0..r.kills_in_reach.saturating_sub(ammo.drop_kill_mark) {
            let (p, s) = crate::rules::ammo::on_kill(
                params.squad_size,
                params.landscape,
                params.target.eximus,
                &mut d.drops,
            );
            dropped_primary += p;
            dropped_secondary += s;
        }
        ammo.drop_kill_mark = r.kills_in_reach;
        // …AND WHAT THIS WEAPON DOES WITH THEM (`rules::ammo::credit`). Nothing at all
        // while the reserve is infinite: the house rule already hands the
        // weapon everything a pack could.
        ammo.credit_pickups(params, &mut r, dropped_primary, dropped_secondary);
        // THE RECHARGE METER, credited with the seconds since it was last
        // looked at. A shot boundary is where every other clock in this loop is
        // read, and the meter is coarse enough not to care: it is 45 seconds
        // long and the fastest thing that fills it is worth one.
        if let Some(m) = ap.meter {
            meter.tick(m, ap, params, dropped_secondary, &mut t, &mut orbs);
        }
        // …AND EVERY ORB EVENT DUE BEFORE THIS SHOT. Same boundary and the same
        // buff snapshot the field walk takes; an orb's clock is its own and no
        // fire-rate bucket reaches it, which is the wiki's *"Tick rate is not
        // affected by Fire Rate"* holding by construction.
        process_orbs(
            &mut orbs,
            &mut debuffs,
            &mut gal,
            &mut arc,
            t,
            &mut target,
            params,
            field_ap,
            &field_ctx,
            &mut r,
            rec,
            d,
            &mut others,
        );
        // Secondary Encumber: at most ONE extra proc per instant — pellets
        // of one pull land simultaneously, so one roll per pull.
        let mut encumber_done = false;
        r.shots += 1;
        // ...and the same boundary for a per-instance arcane cap: the whole
        // pull is ONE damage instance, pellets and radial included.
        arc.next_instance();

        // DID THIS SHOT HIT ANYTHING AT ALL — the question the SHOT COMBO
        // COUNTER asks, and it is the shot's rather than the pellet's: a
        // multishot pull that puts one pellet of six on the target is a hit.
        let mut landed_this_shot = false;

        // THE SHOT'S FACTORS, filled by its first landing pellet — see
        // `SpreadShot`. `None` when nothing landed, and then nothing spreads.
        let mut shot_spread: Option<SpreadShot> = None;

        // A SHOT THAT DEPLOYS RATHER THAN ARRIVES. An orb attack settles no
        // collision and no explosion here: everything it deals is delivered
        // later, by the orb, from wherever the orb is at the time. So the
        // pellet loop is skipped whole.
        //
        // ONE ORB, whatever multishot says. On this weapon multishot buys CHAIN
        // TARGETS instead (*"Number of chains is affected by Multishot"*), and
        // that is already inside `ResolvedOrb::chain_bodies` — so the count is
        // spent where the game spends it rather than on a second projectile.
        // A METER MOVES THE THROW OFF THE TRIGGER. With one, the shot loop fires
        // the weapon's ordinary attack and the orb goes out when the clock says
        // so — which is the whole weapon: you shoot, and every so often you
        // throw. Without one, the trigger deploys, which is what the form's own
        // `transformed` mode shows.
        if let Some(o) = ap.orb.filter(|_| ap.meter.is_none()) {
            throw_orb(o, params, t, &mut orbs);
        }

        {
            let shot = Shot {
                ap,
                qvec,
                direct_pre_snap,
                variants,
                variant_rad,
                modded_base,
                co_base,
                combo_multiplier,
                combo_spec,
                swing: &swing,
                swing_mult,
                swing_forced_types: &swing_forced_types,
                swing_forced_independent: &swing_forced_independent,
                tennokai,
                tennokai_heavy,
                melee_struck: &melee_struck,
                struck: &struck,
                body_at: &body_at,
                bounce_bodies: &bounce_bodies,
                chain_layout: &chain_layout,
                ricochet_layout: &ricochet_layout,
                aim_off_axis,
                effective_cc,
                crit_chance_relative,
                flat_crit,
                cc_mult,
                sc_mult,
                sc_arc_shot,
                weakpoint_cd,
                weakpoint_sc,
                bd_reload_add,
                bd_eximus_add,
                dt_mult,
                ms_damage,
                status_damage,
                live_rate,
                beam_ramp,
                undamaged,
                t,
                bar: &bar,
                rec_roster: &rec_roster,
                rec_buff_index: &rec_buff_index,
                params,
            };
            let mut live = Live {
                r: &mut r,
                rec,
                d,
                target: &mut target,
                others: &mut others,
                debuffs: &mut debuffs,
                gal: &mut gal,
                arc: &mut arc,
                buff_stacks: &mut buff_stacks,
                windows: &mut windows,
                ammo: &mut ammo,
                incarnon: &mut incarnon,
                meter: &mut meter,
                tendril: &mut tendril,
                crit_per_hit: &mut crit_per_hit,
                sniper_combo: &mut sniper_combo,
                weakpoint_pile: &mut weakpoint_pile,
                shot_spread: &mut shot_spread,
                fields: &mut fields,
                orbs: &mut orbs,
                any_big: &mut any_big,
                any_head: &mut any_head,
                beam_merge: &mut beam_merge,
                encumber_done: &mut encumber_done,
                field_duration_boost: &mut field_duration_boost,
                influence_until: &mut influence_until,
                landed_this_shot: &mut landed_this_shot,
                super_crit_armed: &mut super_crit_armed,
            };
            for pellet_idx in 0..n_pellets {
                settle_pellet(pellet_idx, &shot, &mut live);
            }
        }

        // …AND THE TENDRILS, ONCE FOR THE SHOT. They are extra BEAMS rather
        // than a spread of this one, so they neither take its multishot nor
        // fire per pellet — and they only exist once there is a body that is
        // not the one the main beam is on (`spread_from_tendrils`).
        if let (Some(s), false) = (&shot_spread, others.is_empty()) {
            spread_from_tendrils(
                &mut others,
                params,
                ap,
                tendril.count,
                s.raw_per_bucket,
                s.shares,
                s.crit_multiplier,
                s.crit_tier,
                s.attrition,
                s.modded_base,
                s.status_chance,
                &s.forced,
                &s.vector,
                &mut gal,
                &mut arc,
                &mut r,
                rec,
                d,
                t,
            );
        }
        // …AND THE RADIUS-CAUGHT SEEDS' CHAINS, ONCE FOR THE SHOT. The other
        // half of the multishot rule: "beams chaining from targets that were in
        // the damage radius but not directly struck by the initial beam itself
        // will also not benefit from multishot", so these fire here rather than
        // inside the pellet loop above.
        if let (Some(s), Some(beam), false) = (&shot_spread, params.beam, others.is_empty()) {
            spread_from_seeds(
                &mut others,
                params,
                ap,
                beam,
                s.raw_per_bucket,
                s.shares,
                s.crit_multiplier,
                s.crit_tier,
                s.attrition,
                s.modded_base,
                s.status_chance,
                &s.forced,
                &s.vector,
                &mut gal,
                &mut arc,
                &mut r,
                rec,
                d,
                t,
                chain_layout.as_ref(),
                false,
                // THE SAME STRUCK LIST the per-pellet half used — this pass
                // fires the seeds the RADIUS caught, and it tells them apart by
                // filtering on `multishot`, so it has to agree with that half
                // about who was struck directly.
                &struck,
            );
        }

        // EVERY BODY'S STATUS BURNS, not just the aimed one's. A formation
        // body's DoTs were pushed and never ticked until 2026-08-17 — recorded
        // and never paid — so a chain hop's Slash, a splash's Heat and a gas
        // cloud all landed on a ledger nobody read.
        //
        // The PLAYER's buff state (`gal`, `arc`) is shared, which is right: a
        // kill is a kill whichever body it was.
        for (bi, f) in others.iter_mut().enumerate() {
            // NOTHING TO BURN, NOTHING TO DO. A formation is up to 400 bodies
            // and a shot reaches a handful; walking the rest once per shot is
            // the whole difference between a crowd being affordable and not.
            if f.debuffs.idle() {
                continue;
            }
            process_ticks(
                &mut f.debuffs,
                &mut gal,
                &mut arc,
                t + 1e-9,
                &mut f.state,
                params,
                ap,
                &mut r,
                rec,
                &mut d.status,
                &params.others[bi].params,
                bi + 1,
            );
        }

        // …AND THE CLOUDS AND ARCS THIS SHOT LEFT reach the bodies standing in
        // them. Once per SHOT, after every instance of it has settled: a gas
        // cloud posted by the first pellet and one posted by the fourth are the
        // same instant, and the drain is where a body's outbox becomes its
        // neighbours' DoTs (`DebuffState::area_out`).
        drain_area_procs(
            &mut debuffs,
            &mut target,
            &mut others,
            params,
            &area_near,
            &mut r,
            rec,
            t,
            shot_head_landing,
            &mut d.arc_landing,
        );

        // A SHOT THAT HIT NOTHING DROPS THE SHOT COMBO COUNTER.
        //
        // A counter that decays only through its own timer runs slightly
        // generous, and is the right model only in an arena where nothing can
        // miss. This one can, so the other half of the mechanic is here.
        //
        // PER SHOT, not per pellet: the counter counts trigger pulls that
        // connected, and a multishot pull that lands one pellet connected.
        // Nothing happens on a weapon with no combo, and nothing happens at
        // point blank — `landed_this_shot` cannot be false there.
        if ap.sniper_combo.is_some() && !landed_this_shot {
            sniper_combo.count = 0;
        }

        // THE SPEAR PLANTS ITS FIELD, and the shot that planted it does not
        // benefit from it — hence here, after every pellet of this throw has
        // landed. The next one is 1.6 s away against a 4.7 s field, so from
        // the second throw on it is simply up; only the opening throw is
        // affected by the ordering, and this is the ordering that claims the
        // least (the wiki has the field pulse ON impact, not before it).
        //
        // ONE FIELD, replaced rather than stacked, which is already what
        // `push_capped(.., 1, ..)` does — and it is the right shape for a
        // second reason the owner measured: a new throw destroys
        // the OLD FIELD at the moment it starts, but what that field already
        // applied to an enemy runs its own clock. A field the sim never
        // re-pulses and a debuff that survives its field are the same thing
        // from the target's side, which is the only side this arena has.
        if let Some(secs) = ap.attractor_seconds {
            DebuffState::push_capped(&mut debuffs.attractor, t + secs, 1, t);
        }

        // REAVER'S RAPTURE: THE ROUND THAT COMPLETED A BURST, counted here —
        // after every pellet of it has landed, so the burst that earns the
        // stack does not carry it. The next burst does.
        //
        // "Not affected by multishot or punch through" is why this is outside
        // the pellet loop; "counts object hits" and "activates even if the
        // first hit of a burst kills the target" are both already true of this
        // arena, where one target respawns and every round reaches it. So a
        // completed burst IS a full burst hit, with nothing left to condition
        // on.
        //
        // A weapon with no burst has a count of one, and then every round
        // completes its own burst — which is what the trigger means there.
        let burst_len = ap.burst.map_or(1, |b| b.count.max(1));
        if ammo.rounds_this_mag.is_multiple_of(burst_len) {
            bump_buffs!(params, buff_stacks, rec_buff_index, rec, crate::model::BuffTrigger::FullBurst, t, rng);
        }

        // EXECUTIONER'S FORTUNE, SPENT. The roll is per pellet, the effect is
        // not: a magazine fills once however many pellets rolled it, so this is
        // a flag the pellet loop sets and the shot consumes.
        //
        // It is an INSTANT reload, so no time passes — which is the whole perk,
        // and why it is not in the reload bucket. It draws from the reserve
        // like every other refill here (a dry reserve gives nothing), and it
        // fills whole rounds to capacity the way `reload_draw` defines a
        // reload, so an overdrawn counter comes back where a real reload would
        // leave it.
        //
        // A REFILL IS NOT A RELOAD, the rule Sentient Surge established above:
        // `r.reloads` is untouched, so nothing keyed on reloads — Mounting
        // Momentum's shells, Deadly Efficiency's window, Ready Retaliation's —
        // is triggered by it. That is a reading, not a measurement: DE's own
        // text calls it a reload, and if it turns out to arm those buffs this
        // is the one line to change.
        //
        // The form was already checked at the roll — an Incarnon form never
        // sets this flag — so this only has to fill the right counter.
        if ammo.instant_reload_now {
            ammo.instant_reload(params, &mut incarnon);
        }

        // Renewed Horror is spent by the shot that follows the reload, however
        // many grenades that shot put out.
        field_duration_boost = false;

        // GALVANIC RELOAD: "On hitting a target affected by an Electricity
        // status, 40% chance to restore 1 round in the magazine from ammo pool."
        //
        // ONCE PER SHOT, which is the card's own qualifier — "The bonus can only
        // apply once per enemy hit" — and on a shotgun family the difference
        // between that and once per pellet is tenfold. So it is rolled HERE,
        // outside the pellet loop, beside the other per-pull events.
        //
        // "FROM AMMO POOL", so a dry reserve restores nothing: the round is
        // drawn like any other. And a restore is NOT a reload — nothing that
        // watches reloads sees it, the same rule `magazine_refill_on_kill` follows.
        if let Some((st, chance, rounds)) = ap.round_restore_on_status {
            if has_status(&debuffs, st) && d.extra.chance(chance) {
                let room = (ammo.cap - ammo.loaded).max(0.0);
                let want = rounds.min(room);
                if want > 0.0 {
                    ammo.loaded += draw_from(&mut ammo.reserve, params.infinite_reserve, want);
                }
            }
        }

        // ONE Hit event per trigger pull (hitscan pellets are not separate
        // Hits - GLOSSARY): headshot/big-crit flags aggregate any pellet.
        let hit = Event::Hit(Hit {
            big_crit: any_big,
            headshot: any_head,
            target_alive: true,
        });
        // A BLAST THAT WENT OFF SINCE THE LAST SHOT IS A HIT TOO — one per
        // MOMENT, however many stacks shared it, which is what the pile records.
        //
        // A fuse paying out is a damage number at a moment, and that is what an
        // arcane counting hits sees: nine stacks expiring one at a time are
        // nine, ten going off together are one. AND BLAST NEVER CRITS, so a pop
        // only ever BUILDS the ramp — it can never fill the big-crit counter
        // that resets it, which is why `big_crit` is false here and not read
        // from anything.
        //
        // Fed at the pop's own time and before this shot's, because both are
        // true: the arcane is rate-limited, and the ramp this shot fires under
        // is the one the pops left behind.
        for pop in arc.blast_pops.drain(..) {
            if let Some(en) = enervate.as_mut() {
                en.on_event(
                    &Event::Hit(Hit { big_crit: false, headshot: false, target_alive: true }),
                    pop,
                    &mut bar,
                );
            }
        }
        if let Some(en) = enervate.as_mut() {
            en.on_event(&hit, t, &mut bar);
        }
        if ap.frenzy {
            frenzy.on_event(&hit, t, &mut bar);
        }

        // Gauge charging (base phase): every weakpoint PELLET builds one
        // charge (charge_rules); a full gauge transmutes back immediately.
        //
        // A GAUGE IS ONE OF TWO WAYS IN, so the whole block is the GUN's. A
        // melee Incarnon arms on the swing itself (`Arms::HeavyAtCombo`, below
        // beside the combo counter it reads) and skips every line of this: it
        // has no gauge to fill, no charge magazine to fill, and no transmute
        // animation to spend.
        if let Some((cy, charge_on, charges_to_fill)) =
            params.cycle.as_ref().and_then(|cy| match cy.arms {
                Arms::Gauge { charge_on, charges_to_fill } => Some((cy, charge_on, charges_to_fill)),
                Arms::HeavyAtCombo(_) => None,
            })
        {
            charge_the_gauge(
                params, rec, rng, d, cy, charge_on, charges_to_fill, pellets_before, headshots_before,
                &mut t, &mut r, &mut ammo, &mut incarnon, &mut double_tap, &mut buff_stacks,
                &mut rs_armed, &mut opening_closed,
            );
        }

        // ---- WHAT THIS SWING DID TO THE COMBO COUNTER -------------------
        //
        // GAIN FIRST, THEN SPEND, and the order is the game's: a heavy attack
        // pays the multiplier that was standing when it went down (read above,
        // into `combo_multiplier`), lands, and then empties the counter.
        //
        // POINTS ARE THE STANCE MULTIPLIER. *"Stance attacks add combo points,
        // scaling with the attack's stance damage multiplier (100% stance
        // damage multiplier = 1 point)"* — one number doing two jobs, and it is
        // the same number in game. PER BODY LANDED, which is the wiki's own
        // reading of the Rauta: *"generates 2 combo points per pellet landing
        // on enemy (max 28 points across 14 pellets)"*.
        //
        // ONLY A LANDED SWING COUNTS: *"Only successful strikes against enemies
        // award points"*, so a miss neither adds nor refreshes.
        if let Some(h) = &swing {
            after_swing(
                h, ap, params, rec, d, combo_now, combo_multiplier, tennokai, tennokai_kill_mark,
                tennokai_heavy, pellets_before, t,
                &mut r, &mut melee, &mut incarnon, &ammo, &mut arc, &mut target, &mut debuffs,
                &mut others,
            );
        }

        // Next shot: cadence reflects the bar as of now (Frenzy just
        // granted/refreshed counts immediately), plus Pressurized
        // Magazine's live on-reload fire-rate buff.
        bar.expire(t);
        let mut fr_add = match ap.fire_rate_on_reload {
            Some(b) if t < windows.fire_rate_after_reload => b.value,
            _ => 0.0,
        };
        // THE SAME BUCKET fire-rate mods and a static `fire_rate_bonus`
        // evolution live in — `base * (1 + fr + evo + 0.05n)`. `per_stack` is
        // already the absolute rate that fraction is worth, so adding it here,
        // inside the bracket rather than outside it, is what keeps it additive
        // with mods instead of multiplicative with them.
        fr_add += buff_total(ap, crate::model::BuffGrant::FireRate, &mut buff_stacks, t);
        let rate = if params.locks("fire_rate") {
            ap.fire_rate
        } else {
            (ap.fire_rate + fr_add) * bar.total_contributions().fire_rate_multiplier
        };
        // THE TRIGGER CAME OFF, DERIVED rather than listed. Every pause in
        // this loop — a reload, a transform, a dry magazine, a stall on a dry
        // reserve — leaves this shot LATER than the moment the last one made it
        // due, and that is precisely what releasing the trigger is. Asking the
        // clock here, rather than clearing the count in each branch that
        // pauses, is what stops the next pause anyone adds from silently
        // keeping the spool alive — and the reload branch already proves the
        // point: it does not `continue`, it falls through and fires in the same
        // iteration, so a check at the top of the loop would never have seen
        // it (the test caught this: 66 shots against the 80 a released trigger
        // owes).
        if t > spool.due + 1e-9 {
            spool.shots = 0.0;
        }
        last_shot_t = t;
        // …and then the SPOOL, which is a fraction of whatever that rate came
        // to: a fire-rate mod raises the ceiling and the floor together, so the
        // Phenmor's Incarnon form still spends most of its 408-round magazine
        // at 60% of whatever it was built to.
        let rate = rate * spool_factor(ap.sustained_fire_rate, spool.shots);
        spool.shots += 1.0;
        // On a CHARGE weapon the pull costs a draw, not a rate: divide the
        // modded charge time by whatever the live buffs did to the rate
        // (`rate / ap.fire_rate` is exactly that factor, and it is 1.0 when no
        // buff is up). Same bucket, reciprocal application — see `charge_seconds`.
        t += seconds_to_next_shot(
            ap, rate, initial_now, swing, tennokai, tennokai_heavy, &ammo, &incarnon, &mut melee,
        );
        spool.due = t;
    }

    // THE METER'S LAST FILLS, after the trigger stops. A weapon that is out of
    // ammo still recharges, and an orb earned at t = 179 is an orb the fight
    // gets — so the clock is run out to the end and every throw it buys is
    // thrown, before the orbs are drained below.
    if let (Some(m), Some(o)) = (field_ap.meter, field_ap.orb) {
        meter.seconds += params.duration_seconds - meter.clocked;
        while meter.seconds >= m.seconds_to_fill {
            meter.seconds -= m.seconds_to_fill;
            // AT THE INSTANT IT FILLED, not at the end: the orb has a six
            // second fuse and the difference is whether its strikes land inside
            // the engagement at all.
            let at = params.duration_seconds - meter.seconds;
            // WHAT IS ALREADY IN THE AIR GETS TO LIVE UNTIL IT IS REPLACED.
            // Only one orb exists at a time, so throwing them all and walking
            // the list afterwards would leave the LAST one and silently drop
            // every other — which is what happened the day the replace rule
            // landed. Settling up to the throw is the same order
            // the shot loop settles them in.
            process_orbs(
                &mut orbs, &mut debuffs, &mut gal, &mut arc, at, &mut target,
                params, field_ap, &field_ctx, &mut r, rec, d, &mut others,
            );
            throw_orb(o, params, at, &mut orbs);
        }
    }
    // The orbs still in the air after the last shot — every strike they have
    // left and the detonation that ends them. Before the clouds for the same
    // reason the clouds come before the status drain: each event settles what
    // preceded it and pushes procs of its own.
    process_orbs(
        &mut orbs,
        &mut debuffs,
        &mut gal,
        &mut arc,
        params.duration_seconds,
        &mut target,
        params,
        field_ap,
        &field_ctx,
        &mut r,
        rec,
        d,
        &mut others,
    );
    // The clouds still burning after the last shot, with the buff snapshot from
    // that shot (nothing refreshes it once firing stops). FIRST, because each
    // tick settles the status events before it and pushes procs of its own…
    process_field_ticks(
        &mut fields,
        &mut debuffs,
        &mut gal,
        &mut arc,
        params.duration_seconds,
        &mut target,
        params,
        field_ap,
        &field_ctx,
        &mut r,
        rec,
        d,
        &mut others,
    );
    // …then drain what is left up to the end of the engagement.
    process_ticks(
        &mut debuffs,
        &mut gal,
        &mut arc,
        params.duration_seconds,
        &mut target,
        params,
        field_ap,
        &mut r,
        rec,
        &mut d.status,
        &params.target,
        0,
    );

    // THE REPLAY COVERS THE WHOLE FIGHT, INCLUDING THE PART WITH NO SHOOTING
    // IN IT. The firing loop `break`s the moment a finite reserve runs dry,
    // and the sampler must NOT go with it: a 180-second engagement that runs
    // out of ammo at 58.5 would be drawn as a 58.5-second one, and every rate
    // the replay derives would divide by that shorter clock — 378 KPM beside
    // its own `852.83 kill score in 180s`, which is 284.
    //
    // Not only the divisor: the two `process_*` calls above settle the
    // burning clouds and the remaining DoTs to the end, so KILLS land after the
    // last shot too.
    //
    // THE TAIL IS FLAT, a real limitation rather than a rounding one: the drain
    // runs to the end in one step, so every frame after it carries the settled
    // state and the DoT decline is drawn as a single step where the firing
    // stopped. Sampling INSIDE the drain would mean stepping both `process_*`
    // calls a frame at a time, which reorders how status settles — a
    // golden-value change rather than a rendering one.
    sample_frames_up_to(
        params.duration_seconds, params, &mut trace, &mut next_frame, frame_seconds, &mut arc, &mut gal, &mut buff_stacks,
        &bar, &windows, &tendril, &crit_per_hit, &sniper_combo, combo_spec, &incarnon, influence_until,
        &target, &r, &debuffs, &others,
    );

    // Partial credit: the fraction of the current individual's TOTAL bar
    // already depleted — overguard + shield + health: the
    // whole bar counts, so shield damage earns progress and shield REGEN
    // gives it back). If health has hit 0 the unit is DEAD, so the whole bar
    // is gone regardless of any overguard/shield left (e.g. a Toxin-bypass
    // kill that never broke the shield) — full credit.
    // InfiniteHealth pools never deplete -> 0.
    //
    // A THRAX'S BAR IS BOTH OF ITS FORMS. With the spectral switch on, emptying
    // the physical form is real progress and is not the kill, so the spectre's
    // health joins the denominator: a gun that cannot touch it stalls at the
    // physical form's share of the individual instead of scoring a kill.
    let spectral_health =
        params.target.spectral.map_or(0.0, |sp| params.target.max_health() * sp.health_share);
    let pool = params.target.overguard()
        + params.target.max_shield()
        + params.target.max_health()
        + spectral_health;
    let remaining = if target.health <= 0.0 {
        0.0
    } else {
        target.overguard + target.shield + target.health
    };
    let partial = if pool > 0.0 {
        (1.0 - remaining / pool).clamp(0.0, 1.0)
    } else {
        0.0
    };
    r.kill_progress = r.kills as f64 + partial;

    r
}
