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
    let frame_dt = trace.as_ref().map_or(f64::INFINITY, |r| r.frame_seconds);
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
    // ROUNDS FIRED SINCE THE MAGAZINE WAS FILLED, which is what says when a
    // BURST completes: Reaver's Rapture wants a full burst, and a burst is
    // `burst.count` consecutive rounds out of one magazine. It restarts with
    // the magazine, so a magazine that does not divide by the count leaves a
    // partial burst at the end and that burst earns nothing.
    //
    // The intra-burst SPACING is averaged here — the cadence code spreads a
    // burst's rounds evenly, which is the wiki's own effective-rate formula —
    // so this counts which round completes a burst rather than pinning the
    // instant it happened. That is the precise part and the part that decides
    // which shots carry which stack count.
    let mut rounds_this_mag: u32 = 0;
    // Kills already paid to on-kill stacking buffs. See `BuffTrigger::Kill`.
    let mut kill_buff_mark: u32 = 0;
    // DOUBLE TAP: consecutive hits, and when they lapse. Reset by the clock
    // here and never by a miss — this arena has one target that every pellet
    // reaches, which is the card's other reset ("if the next shot does not hit
    // an enemy") and it cannot fire.
    let mut dt_hits: u32 = 0;
    let mut dt_expiry = f64::NEG_INFINITY;
    // …AND THE OTHER FORM'S PILE, FROZEN. Each form of a transmuting weapon
    // keeps its own Double Tap, snapshotted at the instant a transform
    // COMPLETES and handed back, clock and all, when that form next completes
    // its way in (MEASUREMENTS M102): a pile at +300% with 0.5 s left comes
    // back at +300% with 0.5 s left, and a form never yet fired starts from
    // nothing. `(hits, seconds left)`.
    let mut dt_other: (u32, f64) = (0, 0.0);
    macro_rules! swap_dt_pile {
        ($t:expr) => {{
            let now: f64 = $t;
            let held = (dt_hits, (dt_expiry - now).max(0.0));
            dt_hits = dt_other.0;
            dt_expiry = if dt_other.1 > 0.0 { now + dt_other.1 } else { f64::NEG_INFINITY };
            dt_other = held;
        }};
    }
    macro_rules! bump_buffs {
        ($trigger:expr, $t:expr, $rng:expr) => {
            for (i, b) in params.stacking_buffs.iter().enumerate() {
                if b.trigger == $trigger && (b.chance >= 1.0 || $rng.chance(b.chance)) {
                    // …AND THE ROW SAYS SO. One line here rather than at seven
                    // call sites, so a trigger added later is covered by
                    // nobody having to remember it.
                    if let Some(k) = rec_buff_index.get(i).copied().flatten() {
                        rec.triggered(k);
                    }
                    // ONE TRIGGER, `stacks_per_trigger` STACKS. Every buff
                    // written before Mounting Momentum grants one, and that
                    // one grants a shell's worth each — so the bump repeats
                    // rather than the cap being bypassed.
                    for _ in 0..b.stacks_per_trigger.max(1) {
                        buff_stacks[i].bump($t, b.duration, b.max_stacks);
                    }
                }
            }
        };
    }
    // …AND A BUMP THAT COUNTS SHELLS. `bump_buffs!` grants a whole magazine
    // per trigger, which is what an ordinary reload loads; the Incarnon route
    // loads a KNOWN number of shells and splits them across two moments, so it
    // needs to say how many. Reload-counting buffs are left out — they are
    // waiting for the reload to finish, and it has not.
    macro_rules! bump_shells {
        ($n:expr, $t:expr, $rng:expr) => {
            for (i, b) in params.stacking_buffs.iter().enumerate() {
                if b.per_shell
                    && b.trigger == crate::model::BuffTrigger::ReloadComplete
                    && (b.chance >= 1.0 || $rng.chance(b.chance))
                {
                    for _ in 0..$n {
                        buff_stacks[i].bump($t, b.duration, b.max_stacks);
                    }
                }
            }
        };
    }
    // How many times the magazine has been full again — see the macro below.
    let mut mag_refills = 0u32;
    // THE MAGAZINE IS FULL AGAIN — a reload that COMPLETED, or either Incarnon
    // transform completing, since swapping either way fully reloads the base
    // form's magazine (wiki).
    //
    // One macro rather than the same three lines at four sites: everything that
    // a refill ends, ends here. Ready Retaliation is spent, Reaver's Rapture is
    // reset, and the burst count restarts. THE MOMENT IS THE COMPLETION — a
    // reload that has begun has refilled nothing.
    macro_rules! magazine_refilled {
        // The default: this event is a reload as well as a refill, which three
        // of the four sites are. Swapping OUT of the Incarnon form passes
        // `false` — it refills the base magazine and is not a reload, and
        // Blazing Barrel is stated to survive it.
        () => {
            magazine_refilled!(also_a_reload: true)
        };
        (also_a_reload: $reload:expr) => {
            rs_armed = false;
            rounds_this_mag = 0;
            // HOW MANY TIMES THE MAGAZINE HAS BEEN FULL AGAIN. Counted here
            // rather than derived from `r.reloads` and `r.transforms`, because
            // those two miss the fourth site: the Incarnon EXIT refills the
            // base magazine and increments neither.
            mag_refills += 1;
            if !opening_closed {
                opening_closed = true;
                r.first_magazine_damage = r.effective_damage();
            }
            for (i, b) in params.stacking_buffs.iter().enumerate() {
                let cleared = match b.cleared_by {
                    crate::model::ClearedBy::MagazineRefilled => true,
                    crate::model::ClearedBy::Reload => $reload,
                    _ => false,
                };
                if cleared {
                    buff_stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
                }
            }
        };
    }
    // …and the other half of that split: the reload FINISHED, for the buffs
    // that were counting reloads rather than shells.
    //
    // TWO TRIGGERS, ONE SITE. Every reload this loop performs is a reload from
    // empty — it only reloads when it cannot fire — so both fire here and the
    // difference between them lives at exactly one other place: the Incarnon
    // transform, which refills the base magazine whether or not it was empty
    // and therefore bumps `ReloadFromEmpty` alone, and only when it was.
    // THE MAGAZINE'S CAPACITY, LIVE. Resonant Restore grows it — "On Reload
    // From Empty: Increase Base Magazine Capacity by +15. Stacks up to 3x" —
    // so the capacity is a variable rather than `params.magazine_size`, and
    // EVERY read of it below goes through this name. It only ever rises, and
    // only at the one site that pays the stack, which is what lets it be a
    // plain number instead of a buff lookup at eight call sites.
    let mut mag_cap = params.magazine_size;
    let mut mag_growth_stacks: u32 = 0;
    // PYRANA PRIME'S SECOND GUN multiplies that capacity while it is up, so a
    // growth stack landing meanwhile is paid at the same multiple and comes
    // back out whole when the gun leaves.
    let mut summon_magazine_multiplier = 1.0f64;

    macro_rules! bump_on_trigger {
        ($want:expr, $t:expr, $rng:expr) => {
            for (i, b) in params.stacking_buffs.iter().enumerate() {
                if !b.per_shell && b.trigger == $want && (b.chance >= 1.0 || $rng.chance(b.chance))
                {
                    for _ in 0..b.stacks_per_trigger.max(1) {
                        buff_stacks[i].bump($t, b.duration, b.max_stacks);
                    }
                }
            }
        };
    }
    macro_rules! bump_reload_only {
        ($t:expr, $rng:expr) => {
            bump_on_trigger!(crate::model::BuffTrigger::ReloadComplete, $t, $rng);
        };
    }
    // …and the FROM-EMPTY half, which is deliberately NOT folded into the macro
    // above. Of that macro's three sites only two are reloads from empty: the
    // third is the Incarnon EXIT completing the reload that the transform IN
    // began, and whether THAT was from empty is a question about the magazine
    // one transform ago. So this fires at the two real reload sites and at the
    // transform, where it reads the magazine it actually refilled.
    macro_rules! bump_reload_from_empty {
        ($t:expr, $rng:expr) => {
            bump_on_trigger!(crate::model::BuffTrigger::ReloadFromEmpty, $t, $rng);
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
                if mag_growth_stacks < max {
                    mag_growth_stacks += 1;
                    mag_cap += per * summon_magazine_multiplier;
                }
            }
        };
    }
    // SHELLS OWED TO THE PLAYER FOR THE RELOAD THEY ARE HALFWAY THROUGH.
    //
    // Entering the Incarnon form IS a reload — the transmute animation is the
    // weapon's reload time, which is how you can tell — and
    // the whole reload runs across the cycle: one shell as you go in, the rest
    // as you come out. So this holds the rest. Zero while nothing is owed,
    // which is also what entering on a full magazine leaves it.
    let mut owed_shells: u32 = 0;
    // TAKES THE PANEL EXPLICITLY, and that is not a style choice: in a CYCLE
    // the two forms resolve the same buff against different base rates (the
    // Furis Incarnon's 12 ticks/s against the base form's 10), so a FireRate
    // buff's absolute `per_stack` differs per form. The old code read `ap` —
    // the ACTIVE form — and reading the outer `params` instead handed the base
    // form the Incarnon form's rate. The STACKS stay shared (one buff, one
    // count, across the whole engagement); only the conversion is per form.
    // ...and the target-conditional family, which needs the fight's debuff
    // state as well as the clock. One arm, however many buffs use it.
    macro_rules! bump_status_buffs {
        ($debuffs:expr, $t:expr, $rng:expr) => {
            for (i, b) in params.stacking_buffs.iter().enumerate() {
                if let crate::model::BuffTrigger::HitEnemyWithStatus(s) = b.trigger {
                    if has_status($debuffs, s) && (b.chance >= 1.0 || $rng.chance(b.chance)) {
                        buff_stacks[i].bump($t, b.duration, b.max_stacks);
                    }
                }
            }
        };
    }
    macro_rules! buff_total {
        ($from:expr, $grant:expr, $t:expr) => {
            $from
                .stacking_buffs
                .iter()
                .enumerate()
                .filter(|(_, b)| b.grant == $grant)
                .map(|(i, b)| b.per_stack * buff_stacks[i].current($t, b.duration) as f64)
                .sum::<f64>()
        };
    }

    // Stacking arcanes start FULL (user setting) with a fresh timer; the
    // states run each spec's own decay family from there.
    let mut arc = ArcRuntime::init(params);
    // Pressurized Magazine's on-reload fire-rate buff clock (seeded active
    // only if configured so; defaults inactive).
    let mut fire_rate_reload_expiry_seconds: f64 = params
        .fire_rate_on_reload
        .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 });
    // Crosshairs (per-stack expiry FIFO + one refreshable buff); the on-head
    // buff seeds active per its `initial_active` (default on).
    let mut ch_buff_expiry: f64 = params
        .crit_chance_on_headshot
        .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 });
    // Crosshairs keeps a per-stack expiry rather than one clock — and takes
    // an infinite duration exactly like the rest.
    let mut ch_stacks: Vec<f64> = params
        .crit_chance_stack
        .as_ref()
        .map_or(Vec::new(), |s| vec![s.duration; s.initial_stacks as usize]);
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

    // THE RECHARGE METER, in seconds toward `seconds_to_fill`. It opens FULL,
    // and that is the one place a Tome parts company with the rule next door.
    //
    // An INCARNON gauge opens empty because a full one is a consumable the
    // fight has not earned (docs/BUFFS.md) — and it matters most where the
    // gauge cannot be refilled, so a free opening magazine was pure gift. A
    // METER is a CLOCK: it fills at one second a second whether or not anyone
    // is shooting, so it was filling while you ran to the room, and a player
    // walks into an engagement with it full. Opening it empty would not be
    // conservative, it would be wrong — and it would cost the first 45 seconds
    // of every 180-second benchmark to model a state a player is rarely in.
    let mut meter_seconds = field_ap.meter.map_or(0.0, |m| m.seconds_to_fill);
    // …and how far the clock has already been credited, so the seconds are
    // counted once however many times the loop looks at it.
    let mut meter_clocked = 0.0f64;
    // KILLS JAHU CANTICLE HAS ALREADY STRIPPED FOR, read as a delta off the
    // run's own counter for the same reason the meter's pickups are: there are
    // nine places a body can die and a tenth would silently stop paying.
    let mut strip_kills_seen = 0u32;
    // KILLS ALREADY PAID FOR, so the pickup roll happens once per body.
    //
    // Read as a DELTA off the run's own counter rather than hooked onto each
    // place a body dies — there are nine of those, they are the same nine
    // `ledger::settle` guards, and a tenth would silently stop dropping ammo.
    // The counter cannot be missed because every kill already goes through it.
    // Kills already rolled for drops — the shared tally every reader works off.
    let mut drop_kills_seen = 0u32;

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
    let mut magazine = mag_cap;
    let mut reserve = params.reserve_ammo;
    // GOTVA PRIME'S PASSIVE, armed. Set by a pellet that landed a status, spent
    // by the next pellet that lands. It survives across shots and reloads: the
    // card says the chance "remains until landing another successful shot", and
    // nothing but a landing shot spends it.
    let mut super_crit_armed = false;
    // Deadly Efficiency's window. Opens at reload COMPLETION — `t` is already
    // past the reload when this is set, the same as `fire_rate_reload_expiry_seconds` — and
    // seeded from its card exactly like its three siblings.
    // READY RETALIATION's open window, or -inf while it is shut. Unlike the two
    // beside it this is not only read at a shot: it changes how long the NEXT
    // reload takes, so it is passed into every reload and every transmute.
    // Set by a pellet that rolled Executioner's Fortune, spent once by the shot.
    let mut instant_reload_now = false;
    // LINGERING JUDGEMENT: the recent headshots' timestamps, and the window
    // they have opened. The ring is at most `hits` long — older ones can never
    // matter, because a streak is the LAST `hits` inside `within`.
    let mut head_times: Vec<f64> = Vec::new();
    let mut streak_expiry: f64 = f64::NEG_INFINITY;
    let mut base_damage_reload_expiry_seconds: f64 = params
        .base_damage_on_reload
        .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 });
    // EXIMUS ADVANTAGE's window, the same clock as the one above with a
    // different key. It never opens at all unless the target is an Eximus.
    let mut base_damage_eximus_expiry_seconds: f64 = params
        .base_damage_on_eximus_weakpoint
        .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 });
    // Incarnon cycle state. The engagement opens in the BASE form and earns
    // its way in — see `IncarnonCycle::starts_primed` for why, and for the
    // reading that opens transformed.
    let mut in_base_form = params.cycle.as_ref().is_some_and(|c| !c.starts_primed);
    // When a CLOCK-ended Incarnon falls out of its window (`Ends::After`).
    // Unread by a gauge cycle, whose way out is a magazine.
    //
    // A RUN THAT OPENS WITH IT UP OPENS ITS CLOCK TOO — the card's `stacks`
    // knob is "you walked in with it", not "it is up and already expired".
    let mut incarnon_until = match params.cycle.as_ref().map(|c| (c.starts_primed, c.ends)) {
        Some((true, Ends::After(seconds))) => seconds,
        _ => 0.0,
    };
    // READY RETALIATION IS ARMED BY THE EMPTY MAGAZINE, not by the reload.
    //
    // The owner's evidence is the transmute: empty the magazine
    // and transform immediately, and the TRANSFORM is faster too — which it
    // could only be if the buff was already on the weapon before any reload
    // started. It is then spent by the next reload, and coming out of Incarnon
    // form counts as one — leaving Incarnon is a reload as far as this buff is
    // concerned, and it is spent.
    //
    // So it is a flag rather than a clock. This card states a bonus and no
    // duration, and that is not an omission — there is nothing to time.
    let mut charges = 0u32;
    let mut base_mag = params
        .cycle
        .as_ref()
        .map_or(0.0, |c| c.base_form.magazine_size);
    // THE WEAPON AS IT STANDS, stamped onto the combat record.
    //
    // A MACRO BECAUSE IT IS CALLED WHERE STATE CHANGES, and those are six
    // places that each own a different set of locals: the shot that spends a
    // round, the two ends of a reload, and the two ends of each transform. It
    // Written at every EVENT and not at the shot only: otherwise every event
    // between two shots carries the previous shot's weapon, and a
    // `transform_end` that has just put 216 charges in an Incarnon magazine
    // reports `base 0/12` — the one row a reader opens the record to see.
    macro_rules! weapon_now {
        () => {
            if rec.is_on() {
                rec.set_weapon(crate::record::WeaponAt {
                    transmuted: !in_base_form,
                    magazine: (if in_base_form { base_mag } else { magazine }).max(0.0) as u32,
                    magazine_max: params
                        .cycle
                        .as_ref()
                        .filter(|_| in_base_form)
                        .map_or(mag_cap, |cy| cy.base_form.magazine_size)
                        as u32,
                    // THE FORM THAT IS NOT FIRING, so the free reload a
                    // transmute performs on the base magazine is visible rather
                    // than inferred.
                    idle_magazine: params.cycle.as_ref().map(|cy| {
                        if in_base_form {
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
                                (f64::from(charges) / f64::from(to_fill)).min(1.0)
                            } else {
                                0.0
                            };
                            ((share * mag_cap).round().max(0.0) as u32, mag_cap as u32)
                        } else {
                            (base_mag.max(0.0) as u32, cy.base_form.magazine_size as u32)
                        }
                    }),
                    // THE GAUGE, and it starts EMPTY — which is the model's own
                    // rule and was nowhere on screen.
                    // INFINITE IS `None` rather than a very large number: a
                    // ruler grants it, and a column reading "1e9" is a column a
                    // reader has to decode.
                    reserve: (!params.infinite_reserve).then_some(reserve),
                    // …AND ONLY A GAUGE IS DRAWN AS ONE. A melee Incarnon's
                    // way in is a swing, so there is no bar to fill and the
                    // column is absent rather than pinned at zero.
                    gauge: params.cycle.as_ref().and_then(|cy| match cy.arms {
                        Arms::Gauge { charges_to_fill, .. } => Some((charges, charges_to_fill)),
                        Arms::HeavyAtCombo(_) => None,
                    }),
                });
            }
        };
    }
    // THE OCUCOR'S TENDRILS, tracked as two watermarks rather than as a
    // counter incremented at every kill.
    //
    // DERIVED, and deliberately: `r.kills` is already maintained by SIX
    // different sites (beam kills, status-proc kills, field-tick kills, the
    // cycle's own…), and a seventh will exist one day. Hooking each of them is
    // how one gets missed; reading the total they all feed cannot miss any.
    // Same for the clear, which keys off `r.reloads` — every reload path in
    // this loop increments it, including the cycle's.
    //
    // It also happens to be exactly right about WHICH kills count. The wiki
    // excludes one case — "Direct kills with tendrils will not generate an
    // additional tendril" — and a tendril deals no damage in a single-target
    // arena (its damage on the beam's own target is cosmetic), so a tendril
    // kills nothing here and `r.kills` is precisely the qualifying set.
    let mut tendril_kill_mark = 0u32;
    // WHAT THE KILLS LEFT STANDING, one expiry apiece. It exists to be
    // COUNTED: nothing reads it back into the fight.
    let mut ghosts: Vec<f64> = Vec::new();
    let mut ghost_mark = 0u32;
    // Which kills the magazine refill has already paid out — see the spend
    // below for why this cannot be the same watermark.
    let mut refill_kill_mark = 0u32;
    // A SYNDICATE RADIAL's gauge, in affinity, and the same derived-from-kills
    // trick the tendrils use: `r.kills` is maintained at six sites already.
    let mut syndicate_kill_mark = 0u32;
    // ...and the FORM gauge fed by kills rather than by hits (ChargeOn::Kills),
    // which needs its own because it advances in both forms while the others
    // only pay out in one.
    let mut gauge_kill_mark = 0u32;
    let mut syndicate_points = 0.0f64;
    // When the weapon may convert affinity again. During the cooldown it
    // converts NOTHING — "the weapon will not convert any affinity into
    // points, and all collected points are reset to zero" — so this gates the
    // accumulation, not just the firing.
    let mut syndicate_ready_at = 0.0f64;
    let mut tendril_reload_mark = 0u32;
    // HATA-SATYA: the pellet count at the last clear, plus the card's opening
    // pile. TWO marks — the hits, and the REFILL COUNTER that ends them.
    let mut cc_hit_mark = 0u32;
    let mut cc_hit_refill_mark = 0u32;
    let mut cc_hit_seed = params
        .crit_chance_per_hit
        .map_or(0, |c| params.crit_chance_per_hit_initial_stacks.min(c.max_stacks()));
    let mut crit_chance_hit_stacks = cc_hit_seed;
    // THE SHOT COMBO COUNTER: the count as of the last landing hit, and when
    // that was. `combo_at` turns the pair into the count at any later moment.
    // The seed is in hand at t = 0, so the clock starts there rather than at
    // minus infinity — otherwise the card's count would decay away before the
    // first shot.
    let mut combo_count = params.combo_initial;
    let mut combo_last_hit = 0.0f64;
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
    // The card's opening count, which the fight then treats exactly like an
    // earned one: it is spent by the magazine event that clears the rest.
    let mut tendril_seed = params.tendrils_initial.min(params.tendril_max);
    let mut tendrils = tendril_seed;
    // HELD-TRIGGER SPOOL — shots since the trigger was last released, and the
    // moment the next one was due. See `data::weapons::SustainedFireRate`.
    let mut spool_shots = 0.0f64;
    let mut spool_due = f64::NEG_INFINITY;
    // ---- THE MELEE COMBO COUNTER ----------------------------------------
    //
    // POINTS, not tiers. *"Stance attacks add combo points, scaling with the
    // attack's stance damage multiplier (100% stance damage multiplier = 1
    // point)"*, and the tier is `1 + floor(points / 20)` capped at 12 — see
    // `melee_combo_multiplier`.
    //
    // ONE COUNTER, TWO READERS THAT WANT OPPOSITE THINGS. A heavy swing SPENDS
    // it as a damage multiplier; Blood Rush and Weeping Wounds read it as a
    // bracket term and never touch it. That is the whole reason the seven melee
    // forms are seven builds.
    let mut combo_points = 0.0f64;
    // THE KILL COUNT AT THE LAST SWING, so the kills since are what Rage is paid.
    let mut rage_kill_mark = r.kills;
    // WHEN THE COUNTER DIES with nothing added to it. Refreshed by any landed
    // swing; five seconds on almost every weapon.
    let mut combo_expiry = f64::NEG_INFINITY;
    // WHEN THE COUNTER WAS LAST EMPTIED BY A HEAVY ATTACK, which is what the
    // initial-combo floor regenerates from.
    //
    // THE FIGHT OPENS WITH THE FLOOR FULL: *"Initial Combo grants a minimum
    // value of combo points when IDLE or after a combo reset. Heavy attacks
    // spend initial combo, which regenerates at a rate of 40 combo points per
    // second"* (wiki, Melee Combo). The 40 a second is what a heavy attack owes
    // back, not what a player walks in owing — so a build carrying +30 opens
    // its first heavy at 2x rather than reaching it 0.75 s in.
    let mut combo_spent_t = f64::NEG_INFINITY;
    // WHICH SWING OF THE SCRIPT IS NEXT. A gun leaves the script empty and
    // never reads this.
    let mut swing_idx = 0usize;
    // ---- TENNOKAI --------------------------------------------------------
    //
    // A window a landed hit opens, in which a HEAVY attack costs no combo. The
    // owner settled what to do with it in one clause — use it the moment it
    // fires — so the loop takes the very next swing rather than inventing a
    // policy.
    //
    // TWO NUMBERS AND NOTHING ELSE: when the window closes, and how many hits
    // have landed since the last one opened (Discipline's Merit replaces the
    // roll with "every 4 hits", which is the one card that makes the count
    // load-bearing).
    let mut tennokai_until = f64::NEG_INFINITY;
    // WAS THIS WINDOW OPENED BY A TENNOKAI KILL? Truth's Flame pays its damage
    // only in one that was: *"the damage bonus is only active following the
    // first kill"*, so the swing that earns the chain does not carry it.
    let mut tennokai_chained = false;
    let mut tennokai_hits = 0u32;
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
    // SAMPLING IS A MACRO because it happens TWICE, and the second time is
    // after the loop as well as inside it. A macro rather than a closure: the
    // body mutably borrows `arc`, `gal`, `buff_stacks`, `target`, `r`,
    // `debuffs`, `others` and `trace` at once, which a closure cannot hold
    // together.
    macro_rules! sample_frames_up_to {
        ($until:expr) => {
            if let Some(rep) = trace.as_deref_mut() {
                while next_frame <= $until && next_frame < params.duration_seconds {
                    let stacks = sample_stacks(
                        params, &rep.buffs, next_frame, &mut arc, &mut gal, &mut buff_stacks,
                        &ch_stacks, ch_buff_expiry, fire_rate_reload_expiry_seconds, base_damage_reload_expiry_seconds,
                        base_damage_eximus_expiry_seconds, streak_expiry, tendrils, crit_chance_hit_stacks, &bar,
                        combo_at(combo_spec, params.combo_held, combo_count,
                            combo_last_hit, next_frame),
                        incarnon_until,
                influence_until,
                    );
                    rep.frames.push(Frame {
                        t: next_frame,
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
                                    debuffs.sample(next_frame)
                                } else {
                                    others[bi - 1].debuffs.sample(next_frame)
                                };
                                one.into_iter().map(|(n, _)| n).collect::<Vec<u16>>()
                            })
                            .collect(),
                    });
                    next_frame += frame_dt;
                }
            }
        };
    }

    loop {
        // SAMPLE first, so a frame shows the fight as it stood BEFORE the
        // shot at `t` — the same convention the timeline buckets use.
        // Sampling here rather than on a fixed clock is deliberate: this loop
        // is the only place that advances time, and a buff can only change on
        // an event this loop drives. A gap between shots emits repeated
        // frames, which is exactly what a fight with nothing happening in it
        // looks like.
            sample_frames_up_to!(t);
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
            let delay = if magazine < 1e-9 { b.delay_empty_seconds } else { b.delay_partial_seconds };
            if last_shot_t.is_finite() && b.regen_per_second > 0.0 && idle > delay {
                let gained = (idle - delay) * b.regen_per_second;
                magazine = (magazine + gained).min(mag_cap);
            }
        }

        let next_cost = {
            let ap: &FightParams = match &params.cycle {
                Some(cy) if in_base_form => &cy.base_form,
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
                bump_on_trigger!(crate::model::BuffTrigger::Kill, t, d.extra);
                // EXACT PENANCE, on the same counter and for the same reason:
                // "Kills from status effects can also trigger the effect", and
                // a DoT kill happens nowhere near the direct-hit site that
                // `instant_reload_on_headshot` is wired to.
                if let Some(chance) = params.instant_reload_on_kill {
                    if d.extra.chance(chance) {
                        instant_reload_now = true;
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
        // …AND THE MAGAZINE FOLLOWS THE BAR, at one site for both edges. It
        // arrives with a modded magazine's worth of rounds, and "when the
        // ethereal Pyrana disappears, the magazine is reduced to the modded
        // magazine size" (wiki).
        if let Some(s) = params.kill_streak_summon {
            let want = if bar.get(crate::model::KillStreakSummonSpec::BUFF_ID).is_some() {
                s.magazine_multiplier
            } else {
                1.0
            };
            if (want - summon_magazine_multiplier).abs() > 1e-12 {
                if want > summon_magazine_multiplier {
                    magazine += mag_cap / summon_magazine_multiplier;
                }
                mag_cap = mag_cap / summon_magazine_multiplier * want;
                magazine = magazine.min(mag_cap);
                summon_magazine_multiplier = want;
            }
        }

        // TENDRILS and the magazine they keep alive, both re-derived from the
        // counters above before anything decides to reload.
        if params.tendril_max > 0 || params.magazine_refill_on_kill > 0.0 {
            // A reload — or an empty magazine, which in this sim always leads
            // to one — clears every tendril. "Tendrils disappear upon
            // reloading or emptying the magazine."
            //
            // ...unless the card says no event takes them (`tendrils_held`),
            // which is what "no timeout" means for a buff whose end is an
            // event rather than a clock. The seed dies with the earned ones:
            // it is the same buff.
            if r.reloads != tendril_reload_mark && !params.tendrils_held {
                tendril_reload_mark = r.reloads;
                // The mark tracks the SAME quantity the count reads.
                tendril_kill_mark = r.kills - r.kills_by_tendril;
                tendril_seed = 0;
            }
            // SENTIENT SURGE's refill, spent before the reload check below so
            // that a kill can genuinely save a reload — which is the whole
            // point of the mod. "Reloaded ammo is taken from the Ocucor's ammo
            // reserves. This mod does not generate ammo", so it draws like any
            // other reload and a dry reserve gives nothing.
            //
            // ITS OWN WATERMARK, and NOT the tendril one. The two answer
            // different questions: a tendril asks "how many kills since the
            // last reload" (so its mark moves when the magazine event clears
            // them), while a refill asks "which kills have I already been paid
            // for" (so its mark moves when it is SPENT). Sharing the tendril
            // mark made every loop iteration re-earn the same kills, which
            // topped the magazine up on every shot and handed the weapon an
            // effectively infinite one.
            //
            // And a REFILL IS NOT A RELOAD: it never touches `r.reloads`, so
            // the tendrils live through it. That is the whole reason this mod
            // pairs with this passive — the wiki says it from the other side,
            // "Magazine refill effects such as ... kills with Sentient Surge
            // ... will PREVENT the tendrils from disappearing."
            if params.magazine_refill_on_kill > 0.0 && r.kills > refill_kill_mark {
                let earned = f64::from(r.kills - refill_kill_mark)
                    * params.magazine_refill_on_kill
                    * mag_cap;
                refill_kill_mark = r.kills;
                // Capped at the magazine: a refill tops up, it does not bank.
                // Overflow is simply lost, which is what "Refill X% of the
                // Magazine" means on a magazine already near full.
                let room = (mag_cap - magazine).max(0.0);
                let want = earned.min(room);
                if want > 0.0 {
                    magazine += draw_from(&mut reserve, params.infinite_reserve, want);
                }
            }
            // A TENDRIL'S OWN KILL SPAWNS NOTHING, so the count is fed by
            // every OTHER kill — the beam's, and any status kill including one
            // a tendril's own proc caused.
            let spawning = r.kills - r.kills_by_tendril;
            tendrils = (tendril_seed + (spawning - tendril_kill_mark)).min(params.tendril_max);
        }

        // …AND THE SAME MARK-AND-DIFF FOR WHAT IS STANDING. The kills were
        // counted where they happened; this is where they become a duration.
        // The DURATION is the weapon's, so it is read off whichever form
        // carries it — the kills were already filtered by the form that fired.
        let spawner = params
            .spawn_on_kill
            .or_else(|| params.cycle.as_ref().and_then(|c| c.base_form.spawn_on_kill));
        if let Some(g) = spawner {
            ghosts.retain(|e| *e > t);
            for _ in 0..(r.ghost_kills - ghost_mark) {
                ghosts.push(t + g.seconds);
            }
            ghost_mark = r.ghost_kills;
            r.ghosts_peak = r.ghosts_peak.max(ghosts.len() as u32);
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
            // WHAT TAKES THE PILE: "Resets upon reloading or holstering", plus
            // "Swapping to Incarnon Form counts as reloading the Soma Prime and
            // will therefore end the bonus". Holstering is a weapon swap, which
            // this arena never does.
            //
            // KEYED ON THE REFILL, NOT ON THE RELOAD COUNTER (owner measured
            // 2026-08-22: coming OUT of the Incarnon form clears it too). The
            // wiki names only the way in, and reading its two events literally
            // left the way out counting as neither a reload nor a transform —
            // so a pile at its ceiling rode the revert into the base form and
            // spent a whole magazine there at +500% that the game would have
            // taken away. This is the engine's own rule about the cycle, which
            // was already written one screen down: swapping EITHER WAY fully
            // reloads the base form's magazine, so the buff is spent by
            // whatever refills it. One mark instead of two, and the event that
            // was missing is the one it now cannot miss — every refill in this
            // loop goes through `magazine_refilled!`.
            //
            // The seed dies with the earned stacks: it is the same buff.
            if mag_refills != cc_hit_refill_mark && !params.crit_chance_per_hit_held {
                cc_hit_refill_mark = mag_refills;
                cc_hit_mark = r.pellets;
                cc_hit_seed = 0;
            }
            // THE COUNTER IS NOT CAPPED — the BONUS is.
            // The pile takes every hit that lands, and 500% is what it is worth
            // once it passes the ceiling: a card of this class publishes a
            // NUMBER and lets the count run, so 417 stacks and 4,000 stacks are
            // both ordinary states of the same fight. Clamping the count would
            // be modelling a mechanic DE did not write, and the row is drawn as
            // the value anyway, so its ceiling is never on screen.
            crit_chance_hit_stacks = cc_hit_seed + (r.pellets - cc_hit_mark);
        }

        // THE SYNDICATE GAUGE. Affinity the WEAPON earned, which is half of
        // each kill's — "Kill with weapons: Half Affinity goes to the Warframe
        // and half to the killing weapon" (wiki Affinity).
        if let Some(sy) = params.syndicate_radial {
            let fresh = r.kills - syndicate_kill_mark;
            syndicate_kill_mark = r.kills;
            if fresh > 0 && t >= syndicate_ready_at {
                // Per kill: base affinity x the level multiplier, FLOORED to a
                // whole number ("the base affinity multiplied by the Affinity
                // Multiplier value is also rounded down"), then halved.
                let per_kill = (params.target.base_affinity
                    * scaling::affinity_multiplier(params.target.level, params.target.eximus))
                .floor()
                    * WEAPON_AFFINITY_SHARE;
                syndicate_points += f64::from(fresh) * per_kill;
            }
            if syndicate_points >= sy.affinity_to_fill && t >= syndicate_ready_at {
                // Fires, then BOTH rules: points to zero and no conversion at
                // all until the cooldown is out.
                syndicate_points = 0.0;
                syndicate_ready_at = t + sy.cooldown_seconds;
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
            magazine = mag_cap;
        }
        // …AND A CLOCK ENDS THE OTHER KIND, before the magazine block below,
        // which is entirely about a magazine a melee weapon does not have.
        // *"activate Incarnon Form for 90 seconds"* — the window runs from the
        // swing that armed it, is not refreshed by anything, and re-arms the
        // same way it armed the first time.
        if let Some(cy) = &params.cycle {
            if let Ends::After(_) = cy.ends {
                if !in_base_form && t >= incarnon_until {
                    in_base_form = true;
                    weapon_now!();
                }
            }
        }
        if let Some(cy) = params.cycle.as_ref().filter(|c| c.ends == Ends::ChargeMagazine) {
            if !in_base_form && magazine < 1e-9 {
                // Charge magazine spent: revert to the base form. The swap
                // fully reloads the base magazine (wiki side effect). The
                // revert does NOT count as a transform — `transforms` counts
                // TRANSMUTES INTO the Incarnon form only (user:
                // both-directions counting read as doubled).
                // COMING OUT OF INCARNON FORM IS A RELOAD too, and for the
                // same stated reason: the swap refills the base magazine. It
                // takes the speed if the buff is up and spends it — which is
                // also why this animation is scaled by reload speed at all.
                let spent = rescale_reload(cy.transmute_out_seconds, cy.reload_bucket,
                    live_reload_speed(params, &cy.base_form, rs_armed, &mut buff_stacks, t));
                rec.push(t, None, crate::record::Kind::TransformStart {
                    seconds: spent,
                    into_transmuted: false,
                });
                r.downtime_seconds += spent;
                t += spent;
                // …but it is NOT a reload, and one perk can tell the difference:
                // see `ClearedBy::Reload`.
                magazine_refilled!(also_a_reload: false);
                in_base_form = true;
                swap_dt_pile!(t);
                weapon_now!();
                rec.push(t, None, crate::record::Kind::TransformEnd { transmuted: false });
                charges = 0;
                // The swap's auto-reload is the SAME mechanism as a normal one, so it draws whole rounds rather than
                // filling to capacity: a base magazine sitting on 4.25 comes
                // back on 4.25, not 5.
                //
                // ...and it draws from the SAME RESERVE, because one weapon has
                // one supply. Until 2026-08-04 every draw inside the cycle was
                // free, so a finite reserve was silently ignored on every
                // Incarnon weapon — the Infinite-ammo setting did nothing on
                // five of the seven weapons in the roster.
                base_mag += draw_from(&mut reserve, params.infinite_reserve,
                    reload_draw(cy.base_form.magazine_size, base_mag));
                // THE REST OF THE SHELLS LAND HERE. The draw above is normally
                // zero — the magazine came back full on the way IN — so this is
                // the second half of that one reload, not a second reload.
                if owed_shells > 0 {
                    bump_shells!(owed_shells, t, rng);
                    owed_shells = 0;
                    // …and only now has a reload finished, for whatever was
                    // counting reloads instead of shells.
                    bump_reload_only!(t, rng);
                }
                continue;
            }
            if in_base_form && !can_fire(base_mag, next_cost) {
                // Base-form reload. A dry finite reserve stops the gun here
                // exactly as it does outside the cycle — the weapon is out of
                // ammo, not out of one of its two forms.
                if !params.infinite_reserve && reserve < 1e-9 {
                    break;
                }
                // THE SAME CLEAR as the plain path below: an empty magazine
                // takes the pile whichever branch notices it, and a CYCLE
                // reloads the base form here.
                for (i, b) in params.stacking_buffs.iter().enumerate() {
                    if b.cleared_by == crate::model::ClearedBy::EmptyMagazine {
                        buff_stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
                    }
                }
                let rs = live_reload_speed(params, &cy.base_form, rs_armed, &mut buff_stacks, t);
                let spent = live_reload_time(&cy.base_form, params, &mut arc, rs, t);
                // THE OPENING WINDOW closes when the first reload STARTS, which
                // is here — everything dealt up to this instant is what the
                // magazine you walked in with was worth.
                rec.push(t, None, crate::record::Kind::ReloadStart { seconds: spent });
                r.downtime_seconds += spent;
                t += spent;
                magazine_refilled!();
                r.reloads += 1;
                if let Some(b) = cy.base_form.fire_rate_on_reload {
                    fire_rate_reload_expiry_seconds = t + b.duration;
                }
                if let Some(b) = cy.base_form.base_damage_on_reload {
                    base_damage_reload_expiry_seconds = t + b.duration;
                }
                // Same whole-rounds rule as the plain reload below (M14), and
                // the same shared reserve: a short draw is a short magazine.
                let loaded = draw_from(&mut reserve, params.infinite_reserve,
                    reload_draw(cy.base_form.magazine_size, base_mag));
                base_mag += loaded;
                weapon_now!();
                rec.push(t, None, crate::record::Kind::ReloadEnd);
                // ONE STACK PER SHELL THIS RELOAD LOADED, counted here and not
                // from a number resolved once at the panel.
                //
                // The static `stacks_per_trigger` is the OUTER form's magazine,
                // and in a cycle the outer form is the INCARNON one — so a
                // base-form reload of 6 shells was granting 60 stacks, straight
                // to +600% fire rate (measured 61 on the Felarx).
                // Counting the draw is the same rule the Incarnon route already
                // used and it needs no second number to stay true: a dry
                // reserve loads fewer shells and pays fewer stacks, with
                // nothing written down to say so.
                bump_shells!(loaded.round().max(0.0) as u32, t, rng);
                bump_reload_only!(t, rng);
                bump_reload_from_empty!(t, rng);
                // Renewed Horror: "On Reload from Empty". This branch IS the
                // reload-from-empty path.
                field_duration_boost = true;
                continue;
            }
        } else if !can_fire(magazine, next_cost) {
            // AN EMPTY MAGAZINE TAKES THE WHOLE PILE, before the reload that
            // rebuilds it. Mounting Momentum is cleared the instant the count
            // reaches zero — not by the reload, and not by a clock — so firing
            // a magazine dry earns one magazine's worth and never more. The
            // 99-stack cap belongs to a player who tops up a magazine that
            // never empties, which is not what this loop does.
            for (i, b) in params.stacking_buffs.iter().enumerate() {
                if b.cleared_by == crate::model::ClearedBy::EmptyMagazine {
                    buff_stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
                }
            }
            // Cannot fire: reload (blocking) or, with dry finite reserves,
            // stop firing altogether (DoTs still drain below).
            if !params.infinite_reserve && reserve < 1e-9 {
                break;
            }
            // THE WINDOW OPENS WHEN THE RELOAD BEGINS — the player's reload
            // ACTION is the trigger, not its completion.
            // So it is armed BEFORE the line below, and the reload that armed
            // it is the first thing it speeds up.
            //
            // Every reload this loop performs is a reload from empty — it only
            // reloads when it cannot fire — which is exactly the condition.
            let rs = live_reload_speed(params, params, rs_armed, &mut buff_stacks, t);
            let spent = live_reload_time(params, params, &mut arc, rs, t);
            // TWO ROWS, THE START AND THE END, and nothing in between. What is between them is not a reload event — it is
            // whatever the fight went on doing while the weapon was down, which
            // for a status build is most of its damage.
            rec.push(t, None, crate::record::Kind::ReloadStart { seconds: spent });
            r.downtime_seconds += spent;
            t += spent;
            magazine_refilled!();
            r.reloads += 1;
            if let Some(b) = params.fire_rate_on_reload {
                fire_rate_reload_expiry_seconds = t + b.duration;
            }
            if let Some(b) = params.base_damage_on_reload {
                base_damage_reload_expiry_seconds = t + b.duration;
            }
            // Whole rounds only, and `+=` not `=` — both measured (M14). The
            // draw covers the overdraw debt for free: the counter is in (−1, 0]
            // here, so `floor(capacity − current)` is a full magazine, and a
            // −0.75 counter comes back at 4.25 rather than 5.00.
            let want = reload_draw(mag_cap, magazine);
            let loaded = draw_from(&mut reserve, params.infinite_reserve, want);
            magazine += loaded;
            // THE ROW LANDS HERE, after the rounds are actually in. Announced
            // one line earlier it read `0 / 6` — a reload that had just
            // finished reporting an empty magazine, which is the one thing that
            // row exists to deny (found by reading a Felarx record).
            weapon_now!();
            rec.push(t, None, crate::record::Kind::ReloadEnd);
            // …AND THE SHELLS IT LOADED PAY THEIR STACKS. One per shell, from
            // the count the draw actually produced — see the note at the cycle's
            // base-form reload for why this is per-site rather than a single
            // trigger at the top of the loop.
            bump_shells!(loaded.round().max(0.0) as u32, t, rng);
            bump_reload_only!(t, rng);
            bump_reload_from_empty!(t, rng);
            field_duration_boost = true; // reloaded from empty (Renewed Horror)
            if t >= params.duration_seconds {
                break;
            }
        }

        // Active-phase view: the base form's panel during the rebuild
        // phase, the outer params otherwise. Target/aim/locks are shared
        // from the outer params.
        let ap: &FightParams = match &params.cycle {
            Some(cy) if in_base_form => &cy.base_form,
            _ => params,
        };
        // The instance total and its SHAPE (Toxin's shield bypass, the
        // vulnerability column) are derived PER STAGE now — each attack part
        // has its own vector — so only the vector and ModifiedBase survive at
        // pellet scope.
        let (qvec, modded_base) = if in_base_form {
            let p = base_pre.as_ref().expect("cycle state needs base pre");
            (&p.0, p.2)
        } else {
            (&main_pre.0, main_pre.2)
        };
        // ---- THE MELEE SWING THIS SHOT IS -------------------------------
        //
        // A gun's script is empty and every line below is a no-op for it: the
        // swing is `None`, the multiplier is 1.0, and the counter never moves.
        let swing = ap.combo_script.get(swing_idx % ap.combo_script.len().max(1)).cloned();
        // THE COUNTER, BEFORE THIS SWING. A heavy attack reads it and then
        // empties it, so the multiplier it pays is the one that was standing
        // when the trigger went down — the same rule the game states by
        // spending "all or part of the combo counter" as part of the attack.
        // …OR THE CLOCK RAN OUT ZERO. *"A zero or negative combo duration
        // prevents increasing the combo counter"* — so the counter is cleared
        // HERE, upstream of the one place it is read, rather than by each of
        // the four swings that earn into it remembering to ask.
        if t > combo_expiry || ap.combo_frozen {
            // *"Melee Combo resets after this time"*. Power Spike's partial
            // decay is a WARFRAME passive and is not modelled — declared,
            // because a build running it keeps far more of the counter than
            // this does and is therefore UNDER-reported here.
            combo_points = 0.0;
        }
        // …AND THE FLOOR IS LIVE. Galvanized Reflex earns +20 initial combo per
        // melee kill to four stacks, so the number the counter returns to moves
        // during the fight — read here rather than at resolve, which is where
        // it was a static build-time value and the card's whole second half
        // went unpaid.
        let initial_now =
            ap.initial_combo + buff_total!(ap, crate::model::BuffGrant::InitialCombo, t);
        let combo_now = melee_combo_points(combo_points, initial_now, t - combo_spent_t);
        let combo_mult = melee_combo_multiplier(combo_now);
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
        let tennokai = ap.tennokai.enabled && t < tennokai_until;
        let tennokai_heavy = tennokai && !ap.spends_combo && ap.heavy.is_some();
        // …AND WHETHER THE ONE BEING SPENT WAS CHAINED, kept because spending
        // it clears the flag and the damage is decided after.
        let tennokai_was_chained = tennokai && tennokai_chained;
        // WHAT THE COUNT WAS BEFORE IT, so "did this swing kill" is a
        // subtraction rather than a flag every path would have to set.
        let tennokai_kill_mark = r.kills;
        if tennokai {
            tennokai_until = f64::NEG_INFINITY;
            tennokai_chained = false;
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
                * combo_mult
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
                * if ap.spends_combo { combo_mult } else { 1.0 }
                * mode_damage
        };
        // WHO THIS SWING REACHES. A `360deg` swing is a spin and takes
        // everything within the weapon's range; an ordinary one sweeps in
        // front. Empty for a gun, which never asks.
        let melee_struck = match &swing {
            Some(h) if ap.follow_through.is_some() => params
                .melee_struck(h.all_around, buff_total!(ap, crate::model::BuffGrant::MeleeRange, t)),
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
        let (variants, variant_rad): (&[_], &[_]) = if in_base_form {
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
        let mag_left = if in_base_form { base_mag } else { magazine };
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
        let mag_max = if in_base_form {
            params.cycle.as_ref().map_or(0.0, |c| c.base_form.magazine_size)
        } else {
            mag_cap
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
            if t < ch_buff_expiry {
                b.value
            } else {
                0.0
            }
        }) + params.crit_chance_stack.as_ref().map_or(0.0, |s| {
            ch_stacks.retain(|&e| e > t);
            s.per_stack * ch_stacks.len() as f64
        }) + params.arcane.crit_chance_relative
            // SENTIENT SURGE: "Additive to other crit chance and status chance
            // mods", so it belongs in the RELATIVE bucket beside Pistol
            // Gambit's — multiplying the unmodded base, not the modded one.
            + params.crit_chance_per_tendril * f64::from(tendrils)
            // HATA-SATYA: "additive with similar mods. For example, a max rank,
            // max bonus Hata-Satya and Point Strike will have a 30% × (1 + 500%
            // + 150%) critical chance" — the wiki does the bracket for us, and
            // it is the same one Point Strike is in.
            + params
                .crit_chance_per_hit
                .map_or(0.0, |c| c.bonus(crit_chance_hit_stacks))
            // BLOOD RUSH. `Crit Chance = Weapon Crit Chance x [1 + Mod Crit
            // Bonus + Blood Rush Bonus x (Combo Multi - 1)] + Static Crit
            // Bonus` (wiki, verbatim) — so it belongs in this bracket beside
            // Point Strike's, multiplying the UNMODDED base, and nowhere else.
            //
            // IT READS THE COUNTER AND NEVER SPENDS IT, which is why it is
            // worth everything in the four combo modes and nothing in the two
            // heavy ones: there the counter is emptied by the swing that reads
            // it, so it is standing at the floor when the next one starts.
            + ap.crit_chance_per_combo * (combo_mult - 1.0)
            // …AND EVERY STACKING GRANT OF IT, the bracket Prolific
            // Perforation's card puts itself in by naming Pistol Gambit.
            + buff_total!(ap, crate::model::BuffGrant::CritChance, t);
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
            + params.sc_per_tendril * f64::from(tendrils)
            // WEEPING WOUNDS, the same sentence on the status side: `Status
            // Chance = Weapon Status Chance x [1 + Mod Status Bonus + Weeping
            // Wounds Bonus x (Combo Multi - 1)]`. It rides `sc_arc_shot`
            // because that is this loop's name for "relative status the panel
            // could not fold in", which is exactly what a live counter is.
            + ap.status_chance_per_combo * (combo_mult - 1.0)
            // ENDURING AFFLICTION, whose gate is a status the engine tracks:
            // every heavy slam forces `Lifted`, so from the second slam on the
            // target is carrying it and the card pays.
            + if debuffs.lifted.is_some_and(|e| e > t) { ap.status_chance_on_lifted } else { 0.0 }
            // …AND AN ON-KILL STATUS BUFF (Galvanized Elementalist), which is
            // relative like every other card in this bracket.
            + buff_total!(ap, crate::model::BuffGrant::StatusChance, t);
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
            Some(b) if t < fire_rate_reload_expiry_seconds => b.value,
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
            Some(b) if t < base_damage_reload_expiry_seconds => b.value,
            _ => 0.0,
        };
        // …and Eximus Advantage's share of the same bucket. "Stacks additively
        // with base damage bonuses like Hornet Strike", so it joins here rather
        // than forming a factor of its own.
        let bd_eximus_add = match ap.base_damage_on_eximus_weakpoint {
            Some(b) if t < base_damage_eximus_expiry_seconds => b.value,
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
            + buff_total!(ap, crate::model::BuffGrant::Multishot, t)
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
                buff_total!(ap, crate::model::BuffGrant::BaseMultishot, t)
                    * (ap.multishot / ap.base_multishot.max(1e-9))
                    + buff_total!(ap, crate::model::BuffGrant::MultishotPercent, t)
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
                if t >= dt_expiry {
                    dt_hits = 0;
                }
                // AN EXPLODING PROJECTILE IS TWO HITS where the weapon says so
                // — its collision and its explosion, +40% a projectile at rank
                // 3 (M102). Only the aimed landing counts; a bounce adds none.
                let per_projectile = if ap.consecutive_hit_radial_only && ap.radial.is_some() { 2 } else { 1 };
                let hits = dt_hits + rolled * per_projectile;
                dt_hits = hits;
                dt_expiry = t + duration;
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
        if in_base_form {
            base_mag -= spend;
        } else {
            magazine -= spend;
        }
        rounds_this_mag += 1;
        // THE TRIGGER PULL ITSELF — the row every pellet, every explosion and
        // every status this shot goes on to cause points back at
        // (`record::Event::cause`). It is also where the weapon's state is
        // stamped, so a row four seconds later still says which form fired it
        // and what was left in the magazine at the time.
        if rec.is_on() {
            weapon_now!();
            // …AND WHAT THE SHOOTER HAS UP. Sampled at the SHOT, which is the
            // one place in this loop that holds every local the sampler reads —
            // the same reason `weapon_now!` is a macro. A row between two shots
            // carries the count as of the shot before it, which is exact for
            // everything a shot changes and up to one shot stale for a buff
            // that expires on a clock of its own.
            let stacks = sample_stacks(
                params, &rec_roster, t, &mut arc, &mut gal, &mut buff_stacks,
                &ch_stacks, ch_buff_expiry, fire_rate_reload_expiry_seconds,
                base_damage_reload_expiry_seconds, base_damage_eximus_expiry_seconds,
                streak_expiry, tendrils, crit_chance_hit_stacks, &bar,
                combo_at(combo_spec, params.combo_held, combo_count, combo_last_hit, t),
                incarnon_until,
                influence_until,
            );
            rec.set_stacks(stacks);
            rec.begin_shot(t, n_pellets);
        }
        // BLAZING BARREL: the round is SPENT, so it was fired. Here and not in
        // the pellet loop — one shot is one stack however many pellets it threw
        // — and after `ms_eff` was rolled, so the shot that earns the stack does
        // not carry it.
        bump_buffs!(crate::model::BuffTrigger::Firing, t, d.spine);
        // READY RETALIATION IS ARMED THE MOMENT THE MAGAZINE RUNS OUT, which is
        // HERE — the shot that spends the last round — and not at the reload
        // that follows. The two are the same instant for a reload and are not
        // the same instant for a TRANSFORM: the shot that fills the gauge can
        // also be the shot that empties the magazine, and the transform is
        // decided before any reload is. Arming at the reload would have left
        // that transform at the plain speed, which is the case the owner used
        // to state the rule.
        if !can_fire(if in_base_form { base_mag } else { magazine }, ap.ammo_cost) {
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
                    if in_base_form { &mut base_mag } else { &mut magazine }
                } else {
                    // From CAPACITY. With infinite reserves — which the Incarnon
                    // cycle's base phase always assumes — nothing can starve,
                    // which is correct rather than missing.
                    if params.infinite_reserve {
                        afforded += 1;
                        continue;
                    }
                    &mut reserve
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
                Some(s) if t < streak_expiry => s.value,
                _ => 0.0,
            } + buff_total!(ap, crate::model::BuffGrant::HeadshotDamage, t);
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
                + buff_total!(ap, crate::model::BuffGrant::BaseDamage, t)
                + buff_total!(ap, crate::model::BuffGrant::FlatBaseDamage, t),
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
        for _ in 0..r.kills_in_reach.saturating_sub(drop_kills_seen) {
            let (p, s) = crate::rules::ammo::on_kill(
                params.squad_size,
                params.landscape,
                params.target.eximus,
                &mut d.drops,
            );
            dropped_primary += p;
            dropped_secondary += s;
        }
        drop_kills_seen = r.kills_in_reach;
        // …AND WHAT THIS WEAPON DOES WITH THEM (`rules::ammo::credit`). Nothing at all
        // while the reserve is infinite: the house rule already hands the
        // weapon everything a pack could.
        if params.ammo_drops && !params.infinite_reserve {
            if let Some(takes) = params.ammo_class {
                for (kind, n) in [
                    (crate::rules::ammo::Pickup::Primary, dropped_primary),
                    (crate::rules::ammo::Pickup::Secondary, dropped_secondary),
                ] {
                    for _ in 0..n {
                        let got = crate::rules::ammo::credit(
                            kind,
                            takes,
                            reserve,
                            params.reserve_ammo,
                            params.ammo_pickup,
                            params.ammo_conversion,
                        );
                        if got > 0.0 {
                            reserve += got;
                            r.picked_up_ammo += got;
                        }
                    }
                }
            }
        }
        // THE RECHARGE METER, credited with the seconds since it was last
        // looked at. A shot boundary is where every other clock in this loop is
        // read, and the meter is coarse enough not to care: it is 45 seconds
        // long and the fastest thing that fills it is worth one.
        if let Some(m) = ap.meter {
            meter_seconds += t - meter_clocked;
            meter_clocked = t;
            // …AND WHAT THE BODIES DROPPED. *"Picking up secondary or universal
            // ammo reduces recharge time by 10 seconds"*.
            //
            // ONLY SECONDARY COUNTS. A primary pickup does nothing for a tome's
            // meter, and universal packs are placed in a Simulacrum rather than
            // dropped by anything, so a kill can only ever contribute through
            // the secondary half of its roll.
            //
            // INFINITE AMMO DOES NOT REMOVE THE PICKUP. The house rule is about
            // the reserve, and a real fight is under its cap almost all of the
            // time — the pack is still on the floor either way.
            meter_seconds += f64::from(dropped_secondary) * m.seconds_per_ammo_pickup;
            // A FULL METER IS ONE THROW. It is not a magazine — the page says
            // "requires a fully filled meter in order to fire", so what is
            // spent is the whole thing and what is bought is a single orb.
            if meter_seconds >= m.seconds_to_fill {
                meter_seconds -= m.seconds_to_fill;
                if let Some(o) = ap.orb {
                    throw_orb(o, params, t, &mut orbs);
                    // …AND THE PRIMARY FIRE STOPS FOR THE ANIMATION. A throw is
                    // a wind-up and a recovery, and the weapon can do nothing
                    // else until both are over — which is the cycle's whole
                    // price beyond the meter.
                    //
                    // THEN IT WINDS UP AGAIN. Coming back to the primary is
                    // pressing its trigger, and that costs what pressing it
                    // always costs; the interval only "corresponds exactly to
                    // the fire rate" while you are holding it down.
                    t += o.throw_seconds + o.recovery_seconds + ap.windup_seconds;
                }
            }
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

        for pellet_idx in 0..n_pellets {
            // PLENTIFUL MAYHEM, discrete branch: "Damage bonus from multishot
            // consuming ammo only applies to projectiles GENERATED BY
            // multishot" — pellet 0 is the weapon's own projectile and never
            // takes it. An INDEPENDENT multiplier ("multiplicative to base
            // damage bonuses like Serration"), so it multiplies the finished
            // instance rather than joining a bucket. With no multishot source
            // there is no pellet 1 and the perk is worth exactly nothing.
            let pm_mult = if pellet_idx > 0 {
                1.0 + ap.multishot_ammo_bonus
            } else {
                1.0
            };
            // EXPIRE WHAT HAS RUN OUT, before anything reads a stack count.
            //
            // ITS OWN CALL, not a side effect of the mitigation snapshot:
            // that snapshot lives in the stage loop, and the per-pellet reads
            // below — Cold's flat crit damage received and Condition Overload's
            // type count — would go with it and count a stack that had already
            // expired. `mitigation` still
            // prunes and pruning is idempotent, so this is the one call that
            // decides WHEN, rather than a second copy of the rule.
            debuffs.prune(t, status_damage);
            // Live target-side state for THIS pellet (earlier pellets' procs
            // already count): Cold's flat crit damage received, and Condition
            // Overload's type count. The MITIGATION amps are read one level
            // further in — see the stage loop.
            // Crit damage: resolved multiplier + Cold's flat bonus received
            // + Sharpened Bullets' live on-kill buff + the arcane's
            // assumed-max conditional (Outburst).
            // Primary Blight / Frostbite: a stacking crit-damage grant,
            // already resolved to an ABSOLUTE per-stack value against the
            // weapon's base crit damage (ArcaneDef::fx), so it adds straight
            // into the same total as Cold's flat bonus.
            // Same split as crit chance above: `crit_damage_relative` is a bucket bonus every
            // stage scales by its OWN base crit damage; `cd_abs` is a flat add
            // every stage takes as-is.
            let crit_damage_relative = arc.total(&params.arcane.buffs, ArcGrant::CritDamage, t)
                + arc.cd_bonus(ap, t)
                + buff_total!(ap, crate::model::BuffGrant::CritDamage, t)
                // DREAMER'S WRATH: `+32% critical damage for Tennokai attacks`
                // — on the one swing the window bought and no other.
                + if tennokai { ap.tennokai.crit_damage } else { 0.0 }
                + params.arcane.crit_damage_relative;
            // SPITEFUL DEFILEMENT rides the same after-mods FLAT bucket Cold's
            // received bonus does — "Bonus is added after mods as a flat value"
            // (wiki, supplied measured 2026-08-10), so it is added to the
            // finished multiplier rather than scaling the weapon's base.
            //
            // The counter is DISTINCT TYPES, which is what the card's own
            // example insists on ("having 5 corrosive and 5 radiation status
            // effects on a target will not disable this buff") — and it is
            // Condition Overload's counter, read here rather than recomputed,
            // so the anti-CO perk and CO can never disagree about the number
            // they are both reading.
            let spiteful = match params.crit_damage_below_status_count {
                Some((threshold, bonus))
                    if (debuffs.distinct_statuses() as u32) < threshold =>
                {
                    bonus
                }
                _ => 0.0,
            };
            let cd_abs = debuffs.cold_cd_bonus(t) + spiteful + weakpoint_cd;
            // PRELUDE OF MIGHT is the one perk whose condition is read at the
            // MOMENT OF THE HIT rather than off the arsenal: "With Critical
            // Chance below 40%", plus the wiki's note on the same row —
            // "Condition is affected by the critical chance increase effect of
            // Puncture status". So the value tested is `effective_cc`, the very
            // number the crit roll is about to use, and every live source is in
            // it: Weakened, a flat grant (Arcane Avenger), a relative one
            // (Crosshairs, an arcane's stacks). Puncture is only the one the
            // wiki names, being the sole source that sits on the TARGET and so
            // can raise your crit chance without your panel ever moving.
            //
            // Read PER SHOT, not per pellet: `effective_cc` is the weapon's
            // crit chance, while a pellet's may go higher on a weak point
            // (Pistol Acuity) or be REPLACED outright (Gotva Prime's set
            // chance). Those are properties of where a projectile landed, not
            // of the weapon the condition asks about.
            //
            // Nothing to take back when the perk is absent, and nothing to take
            // back when the panel already failed the condition — `resolve` then
            // never granted it and leaves this `None`.
            let prelude_lost = match ap.crit_multiplier_below_crit_chance {
                Some((granted, below)) if effective_cc >= below => granted,
                _ => 0.0,
            };
            let cd_total = ap.crit_multiplier - prelude_lost
                + ap.unmodded_crit_damage * crit_damage_relative
                + cd_abs
                // Mauler's Magazine, earned inside the fight — a BASE grant,
                // already multiplied by the crit-damage mods at `resolve`, the
                // same conversion `FlatBaseDamage` takes one bracket over.
                + buff_total!(ap, crate::model::BuffGrant::BaseCritDamage, t)
                // …and the other half of the same condition, decided by the
                // same shot: an absolute add, already multiplied by the
                // crit-damage mods at `resolve`.
                + if undamaged { ap.crit_damage_on_undamaged } else { 0.0 };
            // Live BASE-DAMAGE bucket additions, evaluated per instance:
            //  - arcane stacks (Merciless/Deadhead/Dexterity/Cascadia Flare)
            //  - Overwhelming Attrition's earned stacks — VERBATIM (wiki
            //    Laetum): "Damage bonus is ADDITIVE to base damage bonuses
            //    such as Hornet Strike", the opposite of its tier sibling
            //    Devouring Attrition, which the same page calls
            //    multiplicative. Both therefore also scale ModifiedBase, so
            //    status payloads follow, exactly like Hornet Strike.
            // The stacks read here are the ones EARNED so far; the hit that
            // grants a stack does not benefit from it (the bump happens
            // after the status roll below).
            let arcane_base_damage = arc.total(&params.arcane.buffs, ArcGrant::BaseDamage, t)
                + bd_reload_add
                + bd_eximus_add
                // Primary Compression's `adds` row: the same bracket a live
                // base-damage buff joins, so Serration dilutes it exactly as
                // the wiki's "additive with damage bonuses" says it should.
                + ap.compression_base_damage
                + buff_total!(ap, crate::model::BuffGrant::BaseDamage, t)
                // Striking Succession, already converted by `resolve` into the
                // share of this bucket its flat number is worth — so it lands
                // here and NOT diluted, which is the whole point of the
                // conversion.
                + buff_total!(ap, crate::model::BuffGrant::FlatBaseDamage, t)
                // …AND KILLING BLOW, which is a term in this bucket and not a
                // multiplier — see `heavy_attack_base_damage`.
                + heavy_attack_base_damage(ap)
                // …AND RAGE: "additive with mods like Pressure Point".
                + arc.rage_bonus(t);
            // FEIGNED RETREAT / SWIFT CONCLUSION: a condition on the TARGET,
            // evaluated per instance because the target's health is falling
            // while the shot is being resolved.
            //
            // HEALTH, not the pools in front of it: a target still on its
            // shields or overguard is not below half HEALTH, and reading the
            // total would have turned this on at the start of every fight
            // against an Eximus.
            //
            // WHERE IT LANDS IS THE WEAPON'S CO BRACKET, and the Kunai's page
            // is what says so: "additive with Hornet Strike in basic Kunai
            // form, and multiplicative in Incarnon form. It is also additive
            // with Galvanized Shot in BOTH forms." Galvanized Shot IS the CO
            // bonus — so the rule is not two rules, it is one: this bonus goes
            // wherever CO goes. `gunco_bucket` routes it.
            let half_hp = if params.target.max_health() > 0.0
                && target.health < 0.5 * params.target.max_health()
            {
                ap.base_damage_below_half_health
            } else {
                0.0
            };
            let base_damage = ap.base_damage_bonus;
            let arc_ratio = (1.0 + base_damage + arcane_base_damage) / (1.0 + base_damage);
            let mb_live = modded_base * arc_ratio;
            // GunCO family — ONE machinery (wiki CO catalog; user
            // 2026-07-27): every source contributes rate × TARGET-COUNTER
            // into the same bracket, is scaled by the original-base
            // fraction (evolution flat damage excluded), and combines per
            // the weapon's CoBehavior, direct hits only. Sources differ
            // ONLY in their counter:
            //   Condition Overload (Galvanized Shot + innate, one merged
            //     rate since they share it) → distinct status TYPES;
            //   Secondary Shiver → live Cold STACKS (Frozen counts as 10).
            let co_mult = gunco_bucket(
                params, ap, &mut debuffs, &mut gal, t, base_damage, arcane_base_damage, arc_ratio,
                half_hp,
                co_base,
                crate::model::CoStage::Direct,
            );
            // The explosion's own, and only when it differs — an evolution
            // that raises the radial's damage without raising its CO base
            // (the Burston's +42) makes these two numbers diverge. Computed
            // beside the direct hit's so both read the SAME counters at the
            // same instant.
            // ...off the ACTIVE form. `ap`, not `params`: in a cycle the two
            // differ for the whole base phase, and this line reading the outer
            // params gave a base-form shot the Incarnon's explosion (M32).
            let co_mult_radial = match &ap.radial {
                // AN EXPLOSION carries no half-health term either — a
                // DIRECT-hit bonus, like the CO it rides beside.
                Some(r) if r.takes_condition_overload => gunco_bucket(
                    params, ap, &mut debuffs, &mut gal, t, base_damage, arcane_base_damage, arc_ratio, 0.0,
                    r.co_base, crate::model::CoStage::Radial,
                ),
                _ => Gunco { bucket: arc_ratio, ..Default::default() },
            };

            // Part FIRST, crit roll second: weak-point crit chance (Pistol
            // Acuity; Cascadia Accuracy under assumed-max) exists only on
            // the pellet that actually lands on a weak point.
            //
            // The landing spot is rolled PER PELLET, not per trigger pull: aiming at the head does not put every
            // pellet of a spread on it, so `headshot_pct` is a per-pellet
            // aim weight. Consequences that follow from this and are
            // deliberate: the Incarnon gauge charges per headshot PELLET
            // (multishot fills it faster), on-headshot buffs trigger from
            // any one pellet, and the reported headshot rate is
            // pellets/pellets. Do NOT "fix" this into a per-pull roll.
            // WHERE THIS PELLET LANDED. An aimed shot draws against the
            // scenario's aim weights; an UNAIMED attack draws against its own
            // flat chance, because `headshot_pct` describes the player and the
            // player is not pointing this one. Same helper the field's ticks
            // use, so the six strikes of one orb cannot answer differently.
            let part = match ap.unaimed_headshot_chance {
                Some(c) => unaimed_part(&params.body_parts, c, &mut d.spine),
                None => pick_part(&params.body_parts, &mut d.spine),
            };
            let cc_pellet = effective_cc
                + if part.is_head {
                    // Weak-point-only crit chance is relative too, and it is
                    // DIRECT-only, so the direct part's base is the right one.
                    ap.unmodded_crit_chance
                        * (ap.weakpoint_crit_chance_relative + params.arcane.weakpoint_crit_chance_relative)
                } else {
                    0.0
                };
            // KING'S GAMBIT, the other half of the same bullet: "x0 Critical
            // Chance on Bodyshots". MULTIPLICATIVE, and the card's own note is
            // why it is applied here rather than folded into a bucket —
            // "Bodyshot modifier is multiplicative with all sources of Critical
            // Chance, effectively making non-headshot critical hits impossible".
            // A bucket term could be cancelled by enough crit chance; a x0 here
            // cannot, which is the whole perk.
            let cc_pellet =
                if part.is_head { cc_pellet } else { cc_pellet * ap.bodyshot_crit_chance_multiplier };
            // GOTVA PRIME: an armed pellet's crit chance is SET, replacing the
            // modded value and the weak-point bonus alike — "Set Critical
            // Chance ignores all other modifiers, whether from mods or Warframe
            // abilities". The tier UPGRADE still runs on the result, which is
            // how Vigilante reaches a Tier-4 hit off it, so the lock binds the
            // chance and not the ceiling.
            let cc_pellet = match params.super_crit_on_status {
                Some(sc) if super_crit_armed => {
                    super_crit_armed = false;
                    sc.crit_chance
                }
                _ => cc_pellet,
            };
            let tier =
                upgrade_crit_tier(roll_crit_tier(cc_pellet, &mut d.spine), ap.crit_tier_upgrade_chance, &mut d.spine);
            // Headshot bonuses form an additive bracket that MULTIPLIES
            // the base multiplier (Enemy_Body_Parts, verbatim template:
            // 3 × (1 + Deadhead 30% + Target Acquired 75%) = 6.15x). A 1x
            // head still benefits (1 × 1.3). Acuity's Weak Point Damage is
            // ADDED to the part multiplier first (at 1.5× the listed value
            // on true weak points — wiki Pistol_Acuity: 3 + 3.5×1.5 =
            // 8.25x) and the bracket multiplies the sum. Rides the part
            // context into DoT snapshots.
            // The additive bracket, and the ONE weapon whose innate share is
            // not in it: "Cernos Prime's headshot bonus is unique and stacks
            // MULTIPLICATIVELY with Primary Deadhead's headshot bonus" — a
            // per-weapon anomaly carried on the weapon. On a 3x head with
            // Deadhead that is 3 x 1.3 x 1.5 = 5.85x against 5.4x additive.
            // LINGERING JUDGEMENT and SEQUENTIAL SKULLBUSTER are ADDITIVE:
            // "Headshot damage bonus stacks additively with Primary Deadhead's
            // headshot damage bonus", so they sum with the arcane's before the
            // bracket is spent — which is why a Deadhead build gets much less
            // than +50% out of one. They sit with the ARCANE's term because
            // that is the group the card names; no weapon carries both a
            // multiplicative innate and one of these.
            let streak_bonus = match params.headshot_streak {
                Some(s) if t < streak_expiry => s.value,
                _ => 0.0,
            } + buff_total!(ap, crate::model::BuffGrant::HeadshotDamage, t);
            // WHAT A HEAD WOULD BE WORTH, computed whether or not THIS pellet
            // found one: a RICOCHET rolls its own head, on another body, later
            // in the same shot, and it is worth exactly what a head is worth
            // here. Split out rather than duplicated so the two can never say
            // different things.
            let (hb_head, hi_head) = if ap.headshot_bonus_multiplicative {
                (params.arcane.headshot_multiplier_bonus + streak_bonus, ap.headshot_damage_bonus)
            } else {
                (
                    params.arcane.headshot_multiplier_bonus + streak_bonus + ap.headshot_damage_bonus,
                    0.0,
                )
            };
            let (head_bonus, head_innate) =
                if part.is_head { (hb_head, hi_head) } else { (0.0, 0.0) };
            // …and the same arithmetic `part_factor` does below, for a head on
            // SOME OTHER body — whose own parts decide the multiplier, because a
            // formation may hold more than one kind of enemy.
            // PER PELLET, because each pellet is its own projectile with its own
            // flight. Filled on the first attack part that needs it and read by
            // the second, so the collision and the explosion are one flight.
            let mut ric_path: Option<Vec<(usize, bool)>> = None;
            let head_factor = |fs: &crate::formation::FoeSpec| -> f64 {
                let m = fs
                    .body_parts
                    .iter()
                    .find(|p| p.is_head)
                    .map_or(1.0, |p| p.multiplier);
                // The WEAPON may overrule what a head is worth — Tenet Arca
                // Plasmor, "1x headshot multiplier". Its own value REPLACES the
                // part's, and the additive brackets still pay on top of it.
                let m = ap.headshot_multiplier.unwrap_or(m);
                (m + 1.5 * ap.weakpoint_damage) * (1.0 + hb_head) * (1.0 + hi_head)
            };
            let head_mult = ap.headshot_multiplier.unwrap_or(part.multiplier);
            let wp_mult = if part.is_head {
                head_mult + 1.5 * ap.weakpoint_damage
            } else {
                part.multiplier
            };
            let part_factor = wp_mult * (1.0 + head_bonus) * (1.0 + head_innate);
            // …AND WHAT AN ELECTRICITY OR GAS TICK IS WORTH WHERE IT LANDS: the
            // same brackets over a 1x base, acuity left out (`lands_on_a_part`).
            let head_landing = (1.0 + hb_head) * (1.0 + hi_head);
            let landing = (1.0 + head_bonus) * (1.0 + head_innate);
            // Wiki Critical_Hit §Critical Headshots: a crit on an eligible
            // >1x location doubles cd inside the tier formula (a cd_total
            // that INCLUDES Cold's flat bonus — freeze.yaml notes).
            let cd = if part.crit_bonus && head_mult > 1.0 && part.multiplier > 1.0 {
                2.0 * cd_total
            } else {
                cd_total
            };
            let crit_multiplier = 1.0 + tier as f64 * (cd - 1.0);

            // Faction bonus (System A) is a total-damage multiplier applied
            // once per instance; DoT/status ticks apply it a SECOND time
            // (fm² below) — the wiki "double dip".
            // Secondary Surge (assumed-max): a FINAL multiplier on the shot,
            // multiplicative with Hornet Strike (wiki notes). Secondary
            // Fortifier: ×overguard_multiplier while the target's Overguard holds.
            // Primary Compression's `multiplies` row rides the same slot, and
            // it is `ap`'s rather than `params`' — the form being fired owns
            // it, because one arcane is worth +240% in the Torid's base form
            // and nothing in its Incarnon.
            // HOISTED so `apply` is handed the SAME number that went in, and
            // can take it back off the share that carries past a depleted
            // Overguard rather than re-deriving it from a pool it has already
            // spent.
            let og_mult = if target.overguard > 0.0 {
                params.arcane.overguard_multiplier
            } else {
                1.0
            };
            let arc_final =
                params.arcane.final_multiplier * ap.compression_multiplier * og_mult;

            // ---- ATTACK PARTS (MECHANICS §7) -------------------------
            // A projectile carries TWO instances where the weapon declares a
            // radial, resolved SEPARATELY because they ARE separate damage —
            // wiki (Laetum): "Initial hit and explosion apply status
            // separately". The per-stage bindings SHADOW the direct-hit names,
            // so the proc block below serves whichever is in flight. The radial
            // fires ONCE PER PULL where the weapon says so, from the ACTIVE
            // form (M32).
            // ---- WHERE THIS PELLET WENT (MECHANICS §11) --------------
            // Spread is an ANGLE, so what it costs is a function of range.
            // Drawn uniform over [0, 2 x spread], whose mean is the stat's own
            // definition; the direction is not drawn, because the target is a
            // circle. NOT DRAWN AT ALL unless a miss is possible, which is what
            // leaves `d.aim` untouched in every fight with no range in it.
            let range = params.range_to_centre();
            let gap_m = params.gap();
            // HOW FAR THE TARGET ALREADY IS FROM THE AIM LINE, before a degree
            // of spread is added. Zero whenever the weapon points at it, which
            // is every fight this engine ran before aim became a place you
            // choose — and zero is what makes the whole clause below collapse
            // to what it was.
            let off_axis = aim_off_axis;
            // THE DEVIATION AND ITS DIRECTION ARE KEPT, not collapsed into
            // one scalar. How far the pellet passed the aimed body is the only
            // thing ONE body can be asked; a crowd asks WHERE, so both survive
            // to build the epicentre below.
            let (dev, phi) = match ap.spread {
                Some(s) if !s.is_pinpoint() && range > 0.0 => {
                    let dev = s.draw(d.aim.next_f64());
                    // WHICH WAY IT WENT, drawn whenever the weapon points away
                    // from the body OR there is a crowd.
                    // Against one body only the magnitude decides anything;
                    // with a crowd the side decides who is in the blast. Off
                    // `blast_dir`, a stream of its own, so adding the draw
                    // shifts no other roll — see `rules::rng::Draws`. No board ruler
                    // sets an aim point, so nothing that was drawn from `aim`
                    // before is drawn from it now either.
                    let phi = if off_axis > 0.0 || !params.others.is_empty() {
                        d.blast_dir.next_f64()
                    } else {
                        0.0
                    };
                    (dev, phi)
                }
                // A PINPOINT WEAPON POINTED ELSEWHERE still misses: no spread
                // does not mean no aim.
                _ => (0.0, 0.0),
            };
            let aim_offset = match ap.spread {
                Some(s) if !s.is_pinpoint() && range > 0.0 => {
                    crate::rules::space::miss_distance_off_axis(range, off_axis, dev, phi)
                }
                // A PINPOINT WEAPON POINTED ELSEWHERE still misses: no spread
                // does not mean no aim.
                _ => crate::rules::space::miss_distance(range, off_axis),
            };
            // …AND A BODY IS A CIRCLE OF ONE RADIUS: hitting the circle is a
            // hit, so this is ray-versus-circle and nothing more
            // (`rules::space::miss_distance`). It asks only whether the pellet reached
            // the target; where on it a landed pellet went is `headshot_pct`'s
            // question, already pinned per pellet, and folding a silhouette's
            // height in here would ask that twice. `rules::space::BODY_RADIUS_M` is
            // the model's one free parameter.
            //
            // AT CONTACT THIS IS ALWAYS TRUE at any cone width, because the
            // muzzle is then one radius from the target's centre — a property
            // of the geometry rather than a special case in it.
            // …AND IT HAS TO REACH. A weapon's RANGE is a wall, not a ramp:
            // past it the shot does not exist and a target beyond takes
            // literally zero. The Phantasma's page lists *"Limited range of 20
            // meters"* and *"No Damage Falloff"* in one breath, which are two
            // separate facts.
            //
            // MEASURED TO THE SURFACE, like every distance a reader is shown:
            // `gap` is what the shot flies and what the arena prints, so "20 m"
            // is the number on the scene. `INFINITY` where none is declared.
            let in_range = gap_m <= ap.range_m;
            let pellet_lands = aim_offset <= crate::rules::space::BODY_RADIUS_M && in_range;
            // WHERE THE ROUND WENT OFF — ONE EPICENTRE FOR THE WHOLE
            // EXPLOSION.
            //
            // NOT TWO. With the aimed body reading its falloff from the miss
            // distance and every OTHER body reading its distance from the aimed
            // body's SURFACE, a shot nine metres wide drops the aimed body's
            // damage by 61% and leaves the body two metres behind it taking a
            // direct hit's blast. Measured on the
            // wire before this landed: 7115 with the shot on target, 7120 with
            // it nine metres off.
            //
            // A ROUND THAT HIT DETONATES ON THE SURFACE it touched; one that
            // missed detonates where it actually passed. The two agree at the
            // boundary, and `detonation_of_miss` is built so that its distance
            // back to the aimed body IS `aim_offset` — so this cannot move the
            // aimed body's number, which is asserted in `space`.
            let det = if pellet_lands {
                crate::rules::space::Detonation {
                    at: crate::rules::space::detonation_point(params.target_at, params.player_at),
                    height_m: 0.0,
                }
            } else {
                crate::rules::space::detonation_of_miss(
                    crate::rules::space::muzzle(params.player_at, params.aim_point()),
                    params.aim_point(),
                    params.target_at,
                    range,
                    off_axis,
                    dev,
                    phi,
                )
            };
            if pellet_lands {
                landed_this_shot = true;
            }
            // A PELLET THAT WENT NOWHERE IS STILL SOMETHING THAT HAPPENED.
            //
            // Three exits below produce no damage at all — outside the cone,
            // out of the weapon's range, an explosion that reached nobody — and
            // until this landed they produced no ROW either, so "why did a
            // three-pellet shot pop two numbers" had no answer anywhere in the
            // ledger. It is not a per-pellet row: on every official ruler the
            // target is at contact and this never fires at all.
            //
            // THE ARRIVAL ITSELF IS NOT A ROW. It was, for an afternoon, with
            // the flight on it — how far the round went, what was left of the
            // reach and of the punch-through budget. Against a target at
            // contact that is one row per pellet saying "it arrived, 0.00 m",
            // which is half the stream to say nothing, and the owner took it
            // back out the same day. What it was going to answer —
            // which numbers are one arrival — is still answerable from the shot
            // they share; what is genuinely lost is the flight, and no scenario
            // this app ships makes that a number worth a row.
            if rec.is_on() {
                if !in_range {
                    // OUT OF RANGE IS NOT A MISS in the aiming sense — the
                    // round never arrived, so its explosion does not go off
                    // either. Both facts are the same sentence to a reader.
                    rec.push(t, Some(0), crate::record::Kind::Miss {
                        reason: "out of the weapon's range",
                    });
                } else if !pellet_lands {
                    rec.push(t, Some(0), crate::record::Kind::Miss {
                        reason: "outside the cone",
                    });
                }
            }
            // A TERMINAL BLAST GOES OFF WHERE THE ROUND DISSIPATES, not on the
            // first body it touched — see `data::weapons::BlastKind` and
            // MEASUREMENTS M53. The budget buys MATERIAL,
            // so the round crosses bodies until it cannot get out of one and
            // detonates there; what is left over when it clears them all is
            // spent as flight, since this arena has no wall to stop it.
            // With no punch through nothing moves.
            //
            // Only on a pellet that LANDS. One that missed never touched
            // anything to punch through, so where it ends up is the ordinary
            // miss geometry's question and is left to it.
            // A SLAM GOES OFF AT THE WIELDER'S OWN FEET, whatever the swing
            // touched — which is what makes it the one melee attack with no
            // reach problem: nothing has to be hit for it to detonate, and the
            // 2.5 m a hammer swings has nothing to do with the 10 m the floor
            // does. It is checked FIRST because it does not care whether the
            // pellet landed.
            let det = if ap.radial.as_ref().map(|r| r.blast_kind)
                == Some(crate::model::BlastKind::Slam)
            {
                crate::rules::space::Detonation { at: params.player_at, height_m: det.height_m }
            } else if pellet_lands
                && ap.radial.as_ref().map(|r| r.blast_kind)
                    == Some(crate::model::BlastKind::Terminal)
            {
                let aim = params.aim_point();
                let mut bodies = Vec::with_capacity(params.others.len() + 1);
                bodies.push(params.target_at);
                bodies.extend(params.others.iter().map(|f| f.at));
                crate::rules::space::Detonation {
                    at: crate::rules::space::dissipation_point(
                        det.at,
                        crate::rules::space::muzzle(params.player_at, aim),
                        aim,
                        &bodies,
                        params.punch_through_m,
                        params.projectile_width_m,
                    ),
                    height_m: det.height_m,
                }
            } else {
                det
            };
            // A COMBO SWING THAT ENDS ON A SLAM fires the weapon's own, and
            // it REPLACES the attack's radial rather than joining it: a light
            // melee attack has no explosion of its own, so there is never a
            // second one to conflict with, and one radial stage per shot is
            // what the loop is built around.
            let attack_radial = match (swing.as_ref().and_then(|h| h.slam_multiplier), ap.slam) {
                (Some(k), Some(slam)) => Some(crate::build::loadout::ResolvedRadial {
                    damage: slam.damage.scale(k),
                    modified_base: slam.modified_base * k,
                    ..slam
                }),
                _ => ap.radial,
            };
            let radial_stage = match attack_radial {
                Some(r) if !r.takes_multishot && pellet_idx > 0 => None,
                other => other,
            };
            // …AND THE SWING SCALES THE EXPLOSION, which on a SLAM is the whole
            // attack. A heavy slam is `3x base` delivered as a radial and it
            // takes the combo multiplier like any other heavy attack, so a
            // swing multiplier that reached only the direct stage would leave
            // the one melee mode that is entirely radial reading its unswung
            // base — 630 at every combo tier.
            //
            // A STANCE SLAM CARRIES ITS OWN MULTIPLIER, already in `attack_radial`,
            // so the swing's — zero on a slam-only row — is not it. What it takes
            // is what every slam takes: the counter a heavy spends, and Seismic
            // Wave, which "also increase[s] the damage dealt by slam attacks
            // performed via Stance Combos" (W`Seismic_Wave`).
            let stance_slam = !tennokai_heavy
                && ap.slam.is_some()
                && swing.as_ref().is_some_and(|h| h.slam_multiplier.is_some());
            let radial_mult = if stance_slam {
                (if ap.spends_combo { combo_mult } else { 1.0 }) * (1.0 + ap.slam_damage)
            } else {
                swing_mult
            };
            let radial_stage = match radial_stage {
                Some(r) if (radial_mult - 1.0).abs() > 1e-12 => Some(crate::build::loadout::ResolvedRadial {
                    damage: r.damage.scale(radial_mult),
                    modified_base: r.modified_base * radial_mult,
                    ..r
                }),
                other => other,
            };
            // THE STAGES OF ONE PELLET, in the order they go off: the bullet,
            // the explosion, and then each bomblet the explosion threw — a
            // contact hit and an explosion of its own, `count` times over.
            //
            // NO BOMBLETS WITHOUT THE EXPLOSION THAT THREW THEM: they are what
            // a detonation releases, so a pellet that never detonated releases
            // none. `takes_multishot` is false on both halves, so the same
            // clause that keeps a once-per-shot explosion off pellets 1.. keeps
            // the bomblets off them (`ClusterSpec`: the count is the bomb's).
            //
            // COUNTED, NOT COLLECTED. This is the engine's hottest loop and a
            // `Vec` of stages is an allocation every pellet fires — the list is
            // `[direct, radial, (contact, blast) x count]`, which an index
            // answers for free.
            let cluster = ap
                .cluster
                .filter(|_| pellet_idx == 0 && radial_stage.is_some());
            let bomblets = cluster.map_or(0, |c| c.count.round().max(0.0) as usize);
            let n_stages = 1 + usize::from(radial_stage.is_some()) + 2 * bomblets;
            for stage in 0..n_stages {
                let rad = match stage {
                    0 => None,
                    1 => radial_stage,
                    s => cluster.map(|c| if s % 2 == 0 { c.contact } else { c.blast }),
                };
                let direct = rad.is_none();
                // EVERY INSTANCE RE-READS THE TARGET — not every shot, and not
                // even every pellet.
                //
                // A pellet that carries an explosion is TWO instances, and the
                // explosion settles against the state its own collision left
                // one instant earlier: a weapon forcing a Viral proc on both
                // halves pops `200 / 1,200 / 450 / 1,500`, which is the ladder
                // read at 0 / 1 / 2 / 3 stacks. Reading it once per pellet gave
                // `200 / 600 / 450 / 1,350` — each explosion sharing its
                // collision's snapshot, a step behind all fight. It is a few
                // per cent on a status build, always in the direction of "this
                // build is good", and invisible in every mean this engine
                // reports; the combat record is what made it visible, because
                // the row states the stacks it read.
                // `a_volley_settles_pellet_by_pellet_and_each_instance_re_reads_the_target`
                // is the golden test.
                rec.begin_instance();
                let mit = debuffs.amps(
                    t,
                    status_damage,
                    ap.armor_strip_per_puncture,
                    params.squad.enemy_armor_multiplier,
                );
                // …AND WHAT THE SHOOTER HAD UP, for THIS instance. The same
                // rule as the line above, on the other side of the fight: a
                // volley changes the SHOOTER's state too — Secondary Enervate
                // stacks on a headshot, so pellet 1's hit is what pellet 2
                // reads — and it was sampled once at the shot and stamped on
                // every row of it.
                //
                // That made the panel say something its own numbers denied: two
                // rows one pellet apart showed the same buffs and the same
                // stacks on the target, and one carried a Condition Overload
                // bracket the other did not, because the factor was current and
                // the column was not. A state column that does not match the
                // number beside it is worse than no column.
                if rec.wants(t) {
                    let stacks = sample_stacks(
                        params, &rec_roster, t, &mut arc, &mut gal, &mut buff_stacks,
                        &ch_stacks, ch_buff_expiry, fire_rate_reload_expiry_seconds,
                        base_damage_reload_expiry_seconds, base_damage_eximus_expiry_seconds,
                        streak_expiry, tendrils, crit_chance_hit_stacks, &bar,
                        combo_at(combo_spec, params.combo_held, combo_count, combo_last_hit, t),
                        incarnon_until,
                influence_until,
                    );
                    rec.set_stacks(stacks);
                }
                // A MISSED PELLET DEALS NOTHING AND DOES NOTHING. Skipping the
                // stage rather than zeroing its damage is the whole point: the
                // status draw, the gauge charge, the on-hit buffs and the combo
                // count all live inside it, and a hit that dealt zero is not
                // what a miss is.
                //
                // A BEAM THAT STRUCK NOBODY THEREFORE DEALS NOTHING, which is
                // the owner's own rule read straight: *"if it
                // hits, it hits; if it does not, it is zero"*. What it does NOT
                // do yet is leave its damage sphere on the floor where it
                // landed — see docs/UNMODELLED.md. Trying it by keeping the
                // instance alive and zeroing the damage made a MISS proc
                // status, charge the gauge and feed the on-kill buffs, which is
                // what the paragraph above exists to prevent.
                if direct && !pellet_lands {
                    continue;
                }
                // OUT OF RANGE IS NOT A MISS — the round never got here at all,
                // so the explosion does not go off either. A missed grenade
                // lands beside the target and still detonates (the clause
                // below); one fired past the weapon's reach does not arrive.
                //
                // A projectile that FALLS SHORT would really detonate at the
                // end of its flight, which is a different model and is not
                // invented here: no weapon in the roster declares both a
                // `range_m` and a `radial:`, so this is the honest answer
                // rather than the convenient one.
                if !in_range {
                    continue;
                }
                // …AND THE EXPLOSION STILL GOES OFF. A missed grenade lands
                // beside the target rather than vanishing, so the radial is
                // resolved from where the pellet actually crossed — which is
                // the first thing in this engine that ever gave the explosion's
                // own falloff a distance to read. Past the blast
                // radius there is no explosion to resolve at all.
                if let Some(r) = rad {
                    // DOES IT REACH ANYBODY? Asked of the whole formation
                    // and not of the AIMED body: a wide shot that leaves that
                    // one body out of range still detonates on the bodies
                    // standing where it landed. Asked with
                    // `blast_reach(aim_offset)`, the same reach the damage
                    // below uses, or it fires one body radius early.
                    let reaches = |b: crate::rules::space::Vec2| {
                        r.falloff_at(crate::rules::space::blast_reach(det.distance_to(b))) > 0.0
                    };
                    if !reaches(params.target_at)
                        && !params.others.iter().any(|f| reaches(f.at))
                    {
                        continue;
                    }
                }
                // Instance values — the shadowing happens here. The explosion
                // rolls its own crit tier off its own crit chance, and the live
                // crit buffs reach it: the relative ones scale ITS base, the
                // absolute ones add flat. Under AssumedMax those same bonuses
                // arrive through the mod bucket in `r.crit_chance`, so this is
                // what makes the two policies agree about one mod.
                // THIS PROJECTILE'S OWN VECTOR, when the weapon has one per
                // projectile. `pellet_idx` wraps, so a multishot source that
                // did add projectiles would cycle the elements again rather
                // than run off the end of the list.
                let own = if variants.is_empty() {
                    None
                } else {
                    Some(pellet_idx as usize % variants.len())
                };
                let (qvec, tier) = match &rad {
                    None => (own.map_or(*qvec, |i| variants[i].0), tier),
                    Some(r) => {
                        // NO `weakened_cc` here. Puncture's Weakened is a flat
                        // crit-chance buff on the VICTIM, and the wiki states
                        // its one exclusion outright: "This is a flat critical
                        // chance buff (like Arcane Avenger), but does not apply
                        // to Area of Effect damage or Warframe abilities"
                        // (Damage/Puncture_Damage). An explosion is Area of
                        // Effect damage, so it does not get it — the lingering
                        // field never did, and the radial's copy of this line
                        // was the odd one out.
                        let rcc = r.crit_chance + flat_crit + r.base_crit_chance * crit_chance_relative;
                        // The set promotes a critical hit "from Primary
                        // Weapons" with no qualifier about which attack part
                        // made it, and the direct hit and the lingering field
                        // both get it — an explosion left out would be an
                        // artifact of where the code was edited, not a rule.
                        let t2 = upgrade_crit_tier(
                            roll_crit_tier(rcc, &mut d.spine),
                            ap.crit_tier_upgrade_chance,
                            &mut d.spine,
                        );
                        (
                            own.map_or_else(
                                || r.damage.quantized_against(r.modified_base),
                                |i| variant_rad[i],
                            ),
                            t2,
                        )
                    }
                };
                // WARFRAME ABILITY ELEMENTS, added to the FINISHED vector.
                //
                // Not through the elemental hierarchy — "does not combine with
                // other elements" is stated on every one of the four augment
                // pages, and it is the whole reason they are worth having
                // separately from a mod. Sized off THIS
                // stage's own ModifiedBase, because "additive with elemental
                // mods" makes them a percentage of the part's base the same
                // way an elemental mod is (MECHANICS §7).
                //
                // Read at `t`: they expire, and after they do the weapon is
                // simply the weapon again.
                let stage_mb = match &rad {
                    None => modded_base,
                    Some(r) => r.modified_base,
                };
                // THE VECTOR BEFORE THE ABILITY'S ELEMENTS, kept so the row
                // can show how its base was built rather than asserting one
                // number. See `Damage::base_steps`.
                // THE VECTOR BEFORE THE SNAP, for the ledger's quantization
                // layer. Read from the ACTIVE stage rather than reconstructed:
                // the radial has its own, and a row that showed the direct
                // hit's grid for an explosion would be a different weapon's
                // arithmetic.
                let pre_quantization = match &rad {
                    None => direct_pre_snap,
                    Some(r) => r.damage,
                };
                let pre_ability_total = qvec.total();
                let qvec = params.with_ability_elements(qvec, stage_mb, t);
                // A merged beam tick carries the SUM of its beams. `qtotal`
                // is what the instance deals; the crit CHANCE that produced
                // `tier` above was deliberately left at one beam's.
                let qtotal = qvec.total() * beam_merge;
                // The SHAPE comes from the vector, never from `qtotal`: a
                // merged beam tick scales the total by `beam_merge` while the
                // composition is unchanged, and dividing by the scaled total
                // understated Toxin's shield bypass by exactly that factor.
                let shares = TypeShares::of(&qvec);
                let crit_multiplier = match &rad {
                    None => crit_multiplier,
                    Some(r) => {
                        // No `part.crit_bonus` doubling: that is the crit-
                        // HEADSHOT rule and an explosion has no hit location.
                        let rcd = r.crit_damage + r.base_crit_damage * crit_damage_relative + cd_abs;
                        1.0 + tier as f64 * (rcd - 1.0)
                    }
                };
                let part_factor = if direct { part_factor } else { 1.0 };
                let landing = if direct { landing } else { 1.0 };
                // ModifiedBase carries the merge too, which is what makes
                // damaging status effects "affected TWICE by multishot": more
                // procs from the summed status chance, and a bigger payload
                // each because the instance itself is bigger.
                let mb_live = beam_merge
                    * match &rad {
                        None => mb_live,
                        Some(r) => r.modified_base * arc_ratio,
                    }
                    // THE CHAMBERS REACH STATUS DAMAGE, which is stated and not
                    // inferred: *"The damage bonus applies to all Multishot
                    // hits and to Status Damage"* (wiki, Primed Chamber). It is
                    // the ONE line that separates this card from Synth Charge,
                    // whose page never says either way and which therefore
                    // multiplies the instance and leaves `mb_live` alone.
                    * cc_mult;
                // WHAT A SPREAD'S STATUSES BURN OFF, and it is THIS number
                // rather than the bare `modded_base` every spread was handed.
                // `spread_hit` multiplies the arcane ratio back in, so the one
                // term is divided out here and the rest — the merge above, the
                // Chamber — travels. A merged beam's DoT was `beam_merge` times
                // bigger on the body it was aimed at than on the body BEHIND
                // it, which is the same hit still flying.
                let spread_mb = mb_live / arc_ratio;
                // The direct hit always carries CO. An explosion does NOT by
                // default — the mods say direct hits only — but the engine
                // supports the case the mods forbid, because some entries do it
                // anyway and the CO catalog lists them one at a time: the
                // Zylok's Incarnon radial has a row reading "Radial hit only
                // receives CO bonus on target DIRECTLY HIT by bullet", which
                // the single-target arena always is. Per-entry weapon data, so
                // no roster weapon is affected until one declares it.
                // THE BRACKET, AND WHAT IT IS MADE OF. The engine multiplies
                // by `gunco.bucket`; the ledger shows the terms, which is the
                // difference between an algebraic rearrangement and a fiction.
                let gunco = match &rad {
                    None => co_mult,
                    Some(r) if r.takes_condition_overload => co_mult_radial,
                    Some(_) => Gunco { bucket: arc_ratio, ..Default::default() },
                };
                let bucket = gunco.bucket;
                // Primary Crux's stacks join the status-chance BUCKET (wiki:
                // "additive to mods like Rifle Aptitude"), so the relative
                // bonus multiplies THIS part's own unmodded base — the
                // explosion's differs from the direct hit's.
                // ...and SENTIENT SURGE's status half rides the same bucket
                // for the same reason ("Additive to other ... status chance
                // mods"). Summed with the arcane's before either is spent, so
                // the two cannot end up multiplying each other.
                // …AND IT IS THE SHOT'S OWN SUM, not a second one. Two sums
                // over one bracket is two answers: this one carried the arcane
                // and Sentient Surge and left out everything LIVE that lands in
                // the same place — Weeping Wounds' `(combo - 1)`, an on-kill
                // status buff, Enduring Affliction's Lifted gate — so those
                // cards rolled nothing at all while the panel showed them.
                let sc_arc = sc_arc_shot;
                // WISEMAN'S REGARD, LIVE: "30% of CURRENT Critical Chance",
                // and current means at this shot. The row names Secondary
                // Outburst, Cascadia Overcharge, Secondary Enervate and
                // Galvanized Crosshairs among the sources that feed it — all
                // live, none of them on the panel — and the owner's ruling is
                // that anything landing on the WEAPON's own crit chance counts. So the panel's static answer is taken back and
                // `effective_cc` pays instead: the same number the crit roll is
                // about to use, which is per SHOT and therefore excludes the
                // weak-point bonus the card's own row also excludes.
                //
                // The DIRECT part only. An explosion has its own status chance
                // and its own base, and the card is about the weapon's.
                let derived_sc = match ap.derived_status_from_crit {
                    Some((rate, cap, folded)) if rad.is_none() => {
                        (rate * effective_cc).min(cap) - folded
                    }
                    _ => 0.0,
                };
                let status_chance = beam_merge
                    * (match &rad {
                        None => ap.status_chance + ap.base_status_chance * sc_arc,
                        Some(r) => r.status_chance + r.base_status_chance * sc_arc,
                    } + derived_sc)
                    // Death Knell's, on the FINISHED number.
                    + weakpoint_sc;
                // EACH PART'S OWN. The direct hit reads the attack's list; an
                // EXPLOSION reads its own, because "Guaranteed Impact proc" is
                // said of one and not the other on the same weapon (the Scourge
                // pair says it of the spear explosion; the Astilla says it of
                // the direct hit and not of its radial).
                // …PLUS WHAT A WARFRAME ABILITY FORCES. Valence Formation imbues
                // its element *"with guaranteed Status"* (wiki), so that element
                // procs on every hit whatever the weapon's status chance.
                //
                // IT RIDES THE SAME LIST as a weapon's own "guaranteed Impact
                // proc", which is what keeps the rest of the status path — the
                // immunity renormalisation, the DoT bookkeeping — from having to
                // know an ability is involved. It is also what makes the
                // interaction fall out rather than be arranged: Overwhelming
                // Attrition asks for a hit that "is neither Critical nor
                // applies a Status Effect", and a hit that always procs can
                // never be one.
                let ability_forced =
                    crate::data::abilities::forced_status_elements_at(&params.abilities, t);
                // ONE SLOT PER DAMAGE TYPE IS ENOUGH: both sources are sets of
                // types and the merge below refuses a duplicate, so the union
                // can never be longer than the type list itself.
                let mut forced_buf = [DamageType::Impact; DamageType::ALL.len()];
                let forced: &[DamageType] = {
                    let mut n = match (&rad, direct) {
                        (None, true) => {
                            for (i, ty) in ap.forced_procs.iter().enumerate() {
                                forced_buf[i] = *ty;
                            }
                            ap.forced_procs.len()
                        }
                        (Some(r), _) => r.forced_procs.fill(&mut forced_buf),
                        _ => 0,
                    };
                    // …AND THE SWING'S OWN, on a DIRECT melee hit. A stance
                    // marks them per attack — Crushing Ruin's first swing
                    // forces Impact and its last forces Knockdown — so they
                    // belong to the swing rather than to the weapon, which is
                    // why `ap.forced_procs` above cannot carry them. `forced_hits`
                    // is how many of the row's hits carry them, when not all do.
                    let swing_forces = direct
                        && swing.as_ref().and_then(|h| h.forced_hits).is_none_or(|k| pellet_idx < k);
                    for ty in swing_forced_types.iter().filter(|_| swing_forces) {
                        if !forced_buf[..n].contains(ty) && n < forced_buf.len() {
                            forced_buf[n] = *ty;
                            n += 1;
                        }
                    }
                    for ty in &ability_forced {
                        // A weapon that already forces this element does not
                        // force it twice: `procs_for_hit` copies the list
                        // through, and a duplicate would be a second proc the
                        // game does not apply.
                        if !forced_buf[..n].contains(ty) && n < forced_buf.len() {
                            forced_buf[n] = *ty;
                            n += 1;
                        }
                    }
                    &forced_buf[..n]
                };

                // Devouring Attrition: an INDEPENDENT multiplier rolled per
                // INSTANCE that did not crit (wiki: "multiplicative to base
                // damage bonuses such as Hornet Strike"; "affects both
                // forms", the explosions included).
                let attrition = noncrit_mult(ap.noncrit_bonus, tier, &mut d.spine);
                // THE SHOT COMBO COUNTER, as the counter stood when this shot
                // was fired. Read BEFORE the hit is counted, because that is
                // the multiplier the player saw under the reticle when they
                // pulled — the hit that reaches Minimum Combo is the one that
                // ARMS it, not the one that spends it.
                //
                // It multiplies the whole shot, direct and radial alike: the
                // wiki calls it "a bonus to their total damage". Only the
                // DIRECT hit builds it — *"Area-of-effect and damage over time
                // do not affect the Shot Combo Counter"* — which is counted
                // below.
                let combo_now =
                    combo_at(combo_spec, params.combo_held, combo_count, combo_last_hit, t);
                let combo_mult = ap.sniper_combo.map_or(1.0, |c| c.multiplier(combo_now));
                // DAMAGE FALLOFF over the distance this instance travelled.
                //
                // THE DIRECT PART ONLY, and the range is asked of the POINT it
                // went off at rather than of the fight — see
                // `FightParams::range_to`. The explosion's own falloff is
                // measured from its epicentre, which sits on the target it just
                // hit, so a radial takes full damage here whatever the
                // engagement range is; that is the same thing its `unmodeled:`
                // line has always said and it stays true.
                //
                // 1.0 at point blank and for every weapon that lists no
                // falloff, which is the whole roster minus nineteen entries —
                // so this factor moves no number the engine reported before the
                // arena had a distance in it.
                let falloff = match (rad, ap.falloff) {
                    // THE EXPLOSION reads the distance from its EPICENTRE to
                    // the body's NEAREST POINT, not to its centre — a body
                    // standing across a falloff gradient takes the best number
                    // on it (`rules::space::blast_reach`,). Zero when
                    // the pellet hit, and zero for anything the blast is
                    // standing inside.
                    (Some(r), _) => {
                        r.falloff_at(crate::rules::space::blast_reach(det.distance_to(params.target_at)))
                    }
                    // THE DIRECT HIT reads the GAP, which IS the distance it
                    // flew: a bullet vanishes at the surface it hits.
                    (None, Some(f)) => f.factor(gap_m),
                    (None, None) => 1.0,
                };
                // A SPREAD INSTANCE LANDS ON A BODY, so the pellet's own
                // head factor comes back off before it is handed on.
                //
                // `raw` below is multiplied by `part_factor`, and every spread
                // mechanism was fed `raw / bucket` — so a chain hop, a splash
                // and an echo all inherited the aimed pellet's HEADSHOT on
                // their direct damage, while `spread_hit`'s own doc comment
                // said "NEVER A HEADSHOT ... `part_factor` is 1.0 here". It was
                // 1.0 where that comment looks (the PROC scale) and not in the
                // damage, so the claim was true of half the instance (found
                // 2026-08-17, building punch-through). Single-target fights are
                // untouched: with no formation, no spread mechanism runs.
                //
                // PUNCH THROUGH IS THE EXCEPTION and takes `raw` undivided: it
                // is the same pellet on the same line, so if it took a head it
                // keeps taking one.
                let body_only = |x: f64| x / part_factor.max(1e-9);
                let dt_here = if direct && ap.consecutive_hit_radial_only { 1.0 } else { dt_mult };
                let raw = qtotal
                    * part_factor
                    * crit_multiplier
                    * bucket
                    * params.faction_at_time(t)
                    * arc_final
                    * attrition
                    // DOUBLE TAP stands on its own: "multiplicatively stacks
                    // with damage bonuses like Serration and Faction Damage
                    // Bonus", so it is a factor here and never a bucket term.
                    // ON THE LATRON INCARNON ONLY THE EXPLOSION TAKES IT: the
                    // collision reads 148 with the pile empty and full (M102).
                    * dt_here
                    // SYNTH CHARGE, on the magazine's LAST round only: "Damage
                    // stacks multiplicatively with Hornet Strike, and any area
                    // damage the weapon may have is also affected" — so it is a
                    // factor here, beside Double Tap's, and it reaches the
                    // explosion because every part of the shot comes through
                    // this line.
                    * sc_mult
                    // THE CHAMBERS, on the magazine's FIRST round only: Charged
                    // Chamber is "multiplicative with other damage mods" and
                    // Primed Chamber "is applied multiplicatively after all
                    // other modifiers from mods and abilities", so it is a
                    // factor here beside Synth Charge's. It reaches the whole
                    // shot — "applies to all Multishot hits" — and the status
                    // payload takes it separately below, which is the one thing
                    // that tells it apart from the last-round card.
                    * cc_mult
                    // ECLIPSE: "an unique multiplier", so it stands beside the
                    // others rather than joining any of them — and CONDITION
                    // OVERLOAD IS THE ONE THING IT DOES NOT REACH, measured
                    // (MEASUREMENTS M79). `eclipse_at` spends it on everything
                    // but the share the CO term put in the bracket, which is
                    // the same arithmetic as Eclipse joining that bracket and
                    // is the only one of the two this reading can tell apart.
                    * eclipse_at(params.ability_final_at(t), co_mult.co_share)
                    * beam_ramp
                    * combo_mult
                    // Multishot paid in DAMAGE, for the weapon that works that
                    // way — "multiplicative to other sources of damage", so it
                    // stands here beside Double Tap rather than in a bucket.
                    * ms_damage
                    * pm_mult
                    * falloff;
                let head_direct = direct && part.is_head;
                let col = target.incoming_column(&params.target);
                // THE TARGET AS IT STOOD, before this instance touched it. Read
                // HERE and not afterwards: `apply` spends the pools and, on a
                // kill, respawns the body outright, so a snapshot taken on the
                // next line would be a snapshot of a different fight.
                let mut breakdown = Breakdown::default();
                let settled = target.apply(
                    raw,
                    shares,
                    head_direct,
                    t,
                    &params.target,
                    false,
                    &mit,
                    og_mult,
                    watching(rec, &mut breakdown),
                );
                let (effective, killed, broke) =
                    (settled.effective, settled.killed, settled.broken);
                // THE AIMED SEED'S CHAINS, HERE, so they take multishot the
                // only way that is honest: by being inside the pellet loop.
                // *"only targets directly hit by the beam benefit"*, and a
                // merged beam's multishot IS the pellet count — so a chain
                // launched from the body the beam struck fires once per landing
                // pellet, and one launched from a body the RADIUS caught fires
                // once for the shot. The other half is after the loop.
                // AN EXPLOSION REACHES THE WHOLE FORMATION, which is the
                // other half of what a radius mod buys and the half a grenade
                // lives on. Its own stage, because a blast has no path: every
                // body the sphere touches takes one instance at its own
                // falloff.
                if let (Some(rr), false) = (rad, others.is_empty()) {
                    spread_from_blast(
                        det,
                        &mut others,
                        params,
                        ap,
                        &rr,
                        if falloff > 0.0 {
                            body_only(raw / bucket / falloff)
                        } else {
                            0.0
                        },
                        shares,
                        crit_multiplier,
                        tier,
                        attrition,
                        spread_mb,
                        status_chance,
                        forced,
                        &qvec,
                        &mut gal,
                        &mut arc,
                        &mut r,
                        rec,
                        d,
                        t,
                    );
                }
                // …AND ONE EXPLOSION PER BOUNCE, centred on the body the
                // projectile came off rather than on the aimed one. "Dealing
                // damage once for any collision on enemies, and AGAIN FOR THE
                // EXPLOSION" — this is the second half of that sentence, and it
                // is the larger half on this family: 140 to the collision's 50.
                if let (Some(rr), Some(path)) = (rad, ric_path.as_deref()) {
                    for &(body, _) in path {
                        let Some(idx) = body.checked_sub(1) else { continue };
                        let Some(fs) = params.others.get(idx) else { continue };
                        blast_at(
                            crate::rules::space::Detonation {
                                at: fs.at,
                                height_m: 0.0,
                            },
                            &mut others,
                            params,
                            ap,
                            &rr,
                            if falloff > 0.0 {
                                body_only(raw / bucket / falloff)
                            } else {
                                0.0
                            },
                            shares,
                            crit_multiplier,
                            tier,
                            attrition,
                            spread_mb,
                            status_chance,
                            forced,
                            &qvec,
                            &mut gal,
                            &mut arc,
                            &mut r,
                            rec,
                            d,
                            t,
                            SpreadBy::Ricochet,
                        );
                    }
                }
                // WHERE THE PROJECTILE BOUNCED, and whether each arrival found
                // a head — decided ONCE for this pellet, because the collision
                // (direct part) and the explosion (radial part) are two passes
                // over the same flight and must not disagree about it.
                if direct && ric_path.is_none() {
                    if let Some(rc) = params.ricochet {
                        let from = struck.first().copied().unwrap_or(0);
                        let n = params.others.len() + 1;
                        // TWO MECHANICS, and the field each wiki page publishes
                        // is what tells them apart (MECHANICS §Bounce).
                        //
                        //   · RICOCHET is hitscan and SEEKS: it "redirects to
                        //     hit another enemy" inside a stated `range_m`.
                        //     The neighbour walk IS that mechanic.
                        //   · BOUNCE is a projectile and REFLECTS at the angle
                        //     of incidence, with no range at all. It gets the
                        //     geometry.
                        // `range_m` is INFINITY when the data states none, and
                        // stating none is exactly what a BOUNCE weapon does.
                        let hops = if rc.range_m.is_finite() {
                            ricochet_layout
                                .as_ref()
                                .map(|l| crate::rules::chain::bounce_path(l, n, from, rc.bounces))
                                .unwrap_or_default()
                        } else {
                            crate::rules::space::bounce_path(
                                params.player_at,
                                &bounce_bodies,
                                from,
                                rc.bounces,
                                // WHERE ON THE FIRST BODY IT LANDED, uniform
                                // across the width. The one assumption in the
                                // path; see `rules::space::bounce_path`.
                                d.spine.next_f64() * 2.0 - 1.0,
                            )
                        };
                        ric_path = Some(
                            hops.into_iter()
                                // ONE ROLL PER ARRIVAL, off the same stream the
                                // aimed pellet's body part comes from — a place
                                // the shot landed is a place the shot landed.
                                .map(|b| (b, d.spine.chance(rc.headshot_chance)))
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                // WHAT THIS PELLET LEFT ON EVERY BODY IT STRUCK — the aimed
                // one and each body the swing followed through to. Melee
                // Influence spreads from all of them, so they are gathered here
                // and fired once the aimed body's own statuses have landed.
                let mut influence_seeds: Vec<(usize, Landed)> = Vec::new();
                if direct && !others.is_empty() {
                    // …and the SHOT's own factors, kept for the half that fires
                    // once rather than per pellet — recorded on a MISS too, so
                    // the sphere that went off on the floor still has a number
                    // behind it.
                    if shot_spread.is_none() {
                        shot_spread = Some(SpreadShot {
                            raw_per_bucket: body_only(raw / bucket),
                            shares,
                            crit_multiplier,
                            crit_tier: tier,
                            attrition,
                            modded_base: spread_mb,
                            status_chance,
                            forced: forced.to_vec(),
                            vector: qvec,
                        });
                    }
                    // …AND EVERY BOUNCE'S COLLISION. The projectile arriving
                    // again, in full, at a body it has not hit — and the one
                    // spread that may land on a head.
                    if let Some(path) = ric_path.as_deref() {
                        spread_from_ricochet(
                            &mut others,
                            params,
                            ap,
                            path,
                            body_only(raw / bucket),
                            shares,
                            crit_multiplier,
                            tier,
                            attrition,
                            spread_mb,
                            status_chance,
                            forced,
                            &qvec,
                            &head_factor,
                            head_landing,
                            &mut gal,
                            &mut arc,
                            &mut r,
                            rec,
                            d,
                            t,
                        );
                    }
                    // …AND THE ECHO, per landing pellet, because the arcane
                    // says each one triggers it.
                    spread_from_echo(
                        &mut others,
                        debuffs.confusion.len(),
                        params,
                        ap,
                        body_only(raw / bucket),
                        shares,
                        crit_multiplier,
                        tier,
                        attrition,
                        spread_mb,
                        status_chance,
                        forced,
                        &qvec,
                        &mut gal,
                        &mut arc,
                        &mut r,
                        rec,
                        d,
                        t,
                    );
                    // …AND EVERYTHING BEHIND THE TARGET that this pellet
                    // punched through to, at full damage. Only where there is
                    // no beam: with one, these bodies are the chain's seeds
                    // and `spread_from_seeds` below already pays them.
                    // …AND EVERY BODY THIS SWING REACHED, at `FT^(n-1)`.
                    //
                    // MELEE'S OWN ANSWER TO "who else did that hit", and it is
                    // exclusive with punch through rather than beside it: a
                    // swing has no punch-through budget (nothing in the melee
                    // pool grants one and the wiki excludes slams and AoE from
                    // Follow Through by name), so the two can never both fire.
                    if let Some(ft) = ap.follow_through.filter(|_| direct) {
                        spread_from_follow_through(
                            &mut others,
                            params,
                            ap,
                            &melee_struck,
                            &mut influence_seeds,
                            ft,
                            raw / bucket,
                            shares,
                            crit_multiplier,
                            tier,
                            attrition,
                            spread_mb,
                            status_chance,
                            forced,
                            &qvec,
                            &mut gal,
                            &mut arc,
                            &mut r,
                            rec,
                            d,
                            t,
                        );
                    }
                    if params.beam.is_none() {
                        spread_from_punch_through(
                            &mut others,
                            params,
                            ap,
                            &struck,
                            raw / bucket,
                            shares,
                            crit_multiplier,
                            tier,
                            attrition,
                            spread_mb,
                            status_chance,
                            forced,
                            &qvec,
                            head_direct,
                            part_factor,
                            landing,
                            &mut gal,
                            &mut arc,
                            &mut r,
                            rec,
                            d,
                            t,
                        );
                    }
                    // THE AIMED SEED'S CHAINS take multishot by firing per
                    // landing pellet — so a pellet that landed on nobody starts
                    // none of them. The radius-caught seeds' half fires once
                    // after the loop, miss or not.
                    if let Some(beam) = params.beam {
                        spread_from_seeds(
                            &mut others,
                            params,
                            ap,
                            beam,
                            body_only(raw / bucket),
                            shares,
                            crit_multiplier,
                            tier,
                            attrition,
                            spread_mb,
                            status_chance,
                            forced,
                            &qvec,
                            &mut gal,
                            &mut arc,
                            &mut r,
                            rec,
                            d,
                            t,
                            chain_layout.as_ref(),
                            true,
                            &struck,
                        );
                    }
                }
                // THE ACCOUNT OF THIS HIT — written HERE because this is the
                // one place every factor exists at the same time. Anywhere else
                // and the list would be reconstructed, which is how a breakdown
                // comes to disagree with the number it explains.
                //
                // ONE WRITER, TWO READERS. The panel's worked example and the
                // combat record's row for this hit are the same list of
                // factors, so it is built once and handed to both. Two lists
                // would be two things to keep in step.
                let hit_steps: Vec<crate::record::Step> = if rec.is_on() {
                    vec![
                        (crate::record::Factor::BodyPart, part_factor),
                        (crate::record::Factor::Critical, crit_multiplier),
                        (crate::record::Factor::ConditionOverload, bucket),
                        faction_layers(params, t, DEPTH_HIT)[0],
                        faction_layers(params, t, DEPTH_HIT)[1],
                        (crate::record::Factor::ArcaneFinal, arc_final),
                        (crate::record::Factor::Attrition, attrition),
                        (crate::record::Factor::WarframeAbility, eclipse_at(params.ability_final_at(t), co_mult.co_share)),
                        (crate::record::Factor::BeamRamp, beam_ramp),
                        // DOUBLE TAP and SYNTH CHARGE were applied and
                        // never listed, which is the exact failure a ledger
                        // exists to catch — it only slipped because both are
                        // 1.0 in every build the check had run.
                        // `check_combat_record` asks it of EVERY row now.
                        (crate::record::Factor::DoubleTap, dt_here),
                        (crate::record::Factor::SynthCharge, sc_mult),
                        (crate::record::Factor::ChamberFirstRound, cc_mult),
                        (crate::record::Factor::SniperCombo, combo_mult),
                        (crate::record::Factor::MultishotAsDamage, ms_damage),
                        (crate::record::Factor::MultishotGenerated, pm_mult),
                        // DISTANCE. Listed even when it is 1.0 — this
                        // ledger keeps a factor of exactly 1.0 rather
                        // than dropping it, because "falloff ×1.00" is
                        // the answer to "why does range not hurt me"
                        // and a missing line is not.
                        (crate::record::Factor::DamageFalloff, falloff),
                    ]
                } else {
                    Vec::new()
                };
                if direct {
                    // ONE PER LANDING PELLET. *"Weapons with Multishot will
                    // count each successful hit from the same shot as multiple
                    // shot instances"* — and per enemy under Punch Through,
                    // which this arena has only one of. Counted here so the
                    // NEXT instance sees it, which is the order the previous
                    // paragraph reads it in.
                    if ap.sniper_combo.is_some() {
                        combo_count = combo_now + 1;
                        combo_last_hit = t;
                    }
                    r.sources.direct += effective;
                    add_by_type(&mut r.sources.direct_by_type, &qvec, effective, &col);
                } else {
                    r.sources.radial += effective;
                    add_by_type(&mut r.sources.radial_by_type, &qvec, effective, &col);
                }
                // THE ONE SITE THAT KNOWS ALL FOUR SHAPES: a hit is plain, a
                // crit, a weak point, or both — and "both" is its own number in
                // game, not a crit with a multiplier on it.
                r.note_kills(killed as u32, t, params.drop_is_in_reach(target.at));
                // …AND WHAT THE KILL LEAVES STANDING. `direct` because a ghost
                // is left by the shot rather than by anything it set off, and
                // the range is the card's own ("within 50 meters of the user").
                if killed && direct && leaves_one(ap, params.player_at, params.target_at) {
                    r.ghost_kills += 1;
                }
                // EXECUTIONER'S FORTUNE. Rolled HERE and nowhere else, because
                // this is the only place that knows both halves of its
                // condition: `head_direct` says the pellet landed in a head,
                // `killed` says it finished the target. An explosion never
                // headshots, so a radial pellet cannot pay.
                //
                // PER PELLET, like every on-hit roll here ("additional
                // shots from Multishot count as separate weakpoint hits"). AND
                // IT DOES NOT ROLL IN AN INCARNON FORM: "Does not affect
                // Incarnon Form", because what it refills is a MAGAZINE and an
                // Incarnon form has max CHARGES. Tested at the TRIGGER so the
                // roll is not even taken — one that can never be spent still
                // draws from `extra`.
                // LINGERING JUDGEMENT's streak, for the same reason:
                // `head_direct` is the only place a headshot is known to have
                // LANDED, per pellet, so a multishot pull can arm it alone. THE
                // ARMING HIT DOES NOT BENEFIT — its damage was settled above
                // and the window opens here.
                if let Some(s) = params.headshot_streak {
                    if head_direct && s.hits > 0 {
                        head_times.retain(|&x| t - x < s.within);
                        head_times.push(t);
                        if head_times.len() >= s.hits as usize {
                            streak_expiry = t + s.duration;
                            // SPENT. A streak is the last `hits` inside the
                            // window, so the ones that armed it cannot arm it
                            // again — otherwise every later headshot would
                            // re-arm on the same two and the "within 2 seconds"
                            // clause would never bind.
                            head_times.clear();
                        }
                    }
                }
                if let Some(ef) = params.instant_reload {
                    let has_magazine = match &params.cycle {
                        Some(_) => in_base_form,
                        None => ap.ammo_efficiency_applies,
                    };
                    if has_magazine
                        && head_direct
                        && (!ef.needs_kill || killed)
                        && d.extra.chance(ef.chance)
                    {
                        instant_reload_now = true;
                    }
                }
                // A LANDED grenade leaves its field, whatever it rolled:
                // "Grenades stick to allies, enemies and surfaces", and a stuck
                // grenade means the target "cannot move out of the cloud".
                // Per PELLET — each multishot projectile is its own grenade and
                // its own cloud, and stacking is MEASURED (M13).
                //
                // The first tick lands WITH the impact — ✅ measured (M13): a
                // hit shows the direct number and the field's first number
                // together, then 9 more over the remaining 9 s. The wiki's
                // "Clouds do not instantly do damage, so enemies that are quick
                // may run through the cloud" describes the grenade arming, not
                // the tick clock; reading it as a delayed first tick cost a
                // tenth of the field's damage.
                if direct {
                    if let Some(fp) = &ap.lingering {
                        // Renewed Horror doubles THIS field's lifetime, so it
                        // ticks 20 times instead of 10 — ✅ measured (M13): one
                        // direct number plus twenty field numbers.
                        let boost = if field_duration_boost {
                            ap.field_duration_on_empty_reload
                        } else {
                            1.0
                        };
                        let mut part = *fp;
                        part.duration_seconds *= boost;
                        let fresh = FieldState {
                            // …AND THE GRIMOIRE'S ORB IS THE OTHER SHAPE. Its
                            // contact is the direct hit this pellet already
                            // settled, and its pulses run on a one second clock
                            // from there — so its field opens one interval late
                            // rather than doubling the collision.
                            next_tick: t + part.first_tick_delay_seconds,
                            ticks_left: (part.duration_seconds * part.tick_rate).round() as u32,
                            part,
                            // Plentiful Mayhem follows a GENERATED grenade into
                            // the cloud it leaves — which is
                            // the whole value of the perk here, the cloud being
                            // most of this weapon's damage.
                            damage_multiplier: pm_mult,
                        };
                        match fp.stacking {
                            crate::model::FieldStacking::Stack => fields.push(fresh),
                            crate::model::FieldStacking::Refresh => {
                                fields.clear();
                                fields.push(fresh);
                            }
                        }
                    }
                }
                if direct {
                    // A HIT TAKES A SECOND OFF THE RECHARGE. *"Hitting enemies
                    // with the primary fire reduces recharge time by 1 second
                    // per hit"*, and the page settles both of the questions
                    // that raises without a field: *"Multishot will count as an
                    // additional hit"* — so it is per landing PELLET, which is
                    // what this branch counts — and *"Radial damage does not
                    // count an additional hit"*, which is why it is inside
                    // `direct` rather than beside it.
                    if let Some(m) = ap.meter {
                        meter_seconds += m.seconds_per_hit;
                    }
                    r.pellets += 1;
                    // A PELLET THAT WENT THROUGH: `struck` is who is on the
                    // line, so a second body means this bolt left the first.
                    if struck.len() > 1 {
                        bump_buffs!(crate::model::BuffTrigger::PunchThrough, t, d.extra);
                    }
                    r.crits += (tier >= 1) as u32;
                    r.big_crits += (tier >= 2) as u32;
                    r.crit_tier_sum += tier;
                    r.headshots += part.is_head as u32;
                    any_head |= part.is_head;
                    any_big |= tier >= 2;

                    // Crosshairs' on-HEADSHOT buff refreshes on every head
                    // hit (kills only matter for its stacks).
                    if part.is_head {
                        if let Some(b) = params.crit_chance_on_headshot {
                            ch_buff_expiry = t + b.duration;
                        }
                        // EXIMUS ADVANTAGE — the WEAK POINT is the trigger
                        // ("Despite the description specifying headshots, the
                        // effect can be trigger on weak-point hits"), and the
                        // second half of it is the TARGET. Against anything
                        // that is not an Eximus this line never runs, which is
                        // the whole reason the mod is not a plain on-headshot
                        // buff. It REFRESHES rather than stacking.
                        if params.target.eximus {
                            if let Some(b) = params.base_damage_on_eximus_weakpoint {
                                base_damage_eximus_expiry_seconds = t + b.duration;
                            }
                        }
                        // Lethal Rearmament: every headshot grants a stack —
                        // a LOCKED buff earns it too, it just never loses it.
                        // EVERY buff that triggers on a headshot, including
                        // its own chance roll. One line for the family.
                        bump_buffs!(crate::model::BuffTrigger::Headshot, t, d.extra);
                        // DEATH KNELL, per PELLET: "individual Multishot bullets
                        // can proc Death Knell" (wiki).
                        if let Some(w) = params.weakpoint_stacks {
                            weakpoint_pile.bump(t, w.duration_seconds, w.max_stacks);
                        }
                        // Primary Crux: a weak-point HIT (not a kill), per
                        // PELLET. Bumped here, AFTER this pellet's status
                        // chance was read above — the hit that grants a stack
                        // does not benefit from it, the same rule the
                        // base-damage stacks follow. A killing headshot still
                        // counts: this runs before the kill path's `continue`.
                        arc.bump_trigger(&params.arcane.buffs, ArcTrigger::WeakpointHit, t);
                        bump_buffs!(crate::model::BuffTrigger::ConsecutiveHeadshot, t, d.extra);
                    } else {
                        // …AND A BODY HIT TAKES THE PILE. The only trigger in
                        // this sim that the next shot can undo, and the reason
                        // it is not `Headshot` with a clock: what ends it is
                        // what you hit, not how long you waited.
                        for (i, b) in params.stacking_buffs.iter().enumerate() {
                            if b.trigger == crate::model::BuffTrigger::ConsecutiveHeadshot {
                                buff_stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
                            }
                        }
                    }
                }

                if let Some(pool) = broke {
                    push_break_proc(&mut debuffs, params, t, pool);
                }
                // THE ROW FOR THIS PELLET — a MACRO with two call sites,
                // because a pellet has two ways of ending and the row has to
                // survive both.
                //
                // A single write before `settle_procs` names what the instance
                // APPLIED, which is right for a pellet that leaves the target
                // standing and wrong for one that KILLS: the kill branch below
                // `continue`s — correctly, since the killing instance's procs
                // die with the old individual — and would take the row with it.
                // Against a level 1 Crewman that is most of the shots, and the
                // record would show a fight with no kills in it while the meter
                // beside it counted six.
                //
                // The killing call passes NO procs, which is not a shortcut: it
                // is the same sentence that branch already makes.
                macro_rules! log_this_pellet {
                    ($procs:expr) => {
                    // THE ROW FOR THIS PELLET, written HERE rather than beside
                    // `apply` — because a row says what the instance APPLIED, and
                    // the proc list is not final until the line above. Everything
                    // else it needs was snapshotted at the moment it landed
                    // (`before`, `breakdown`, `settled`), so waiting costs nothing and
                    // buys the one column a reader checks a status build with.
                    ledger::settle(
                        &mut r, rec,
                        t,
                        0,
                        qvec.dominant(),
                        match (part.is_head, tier > 0) {
                            (true, true) => PopKind::HeadCrit,
                            (true, false) => PopKind::Head,
                            (false, true) => PopKind::Crit,
                            (false, false) => PopKind::Direct,
                        },
                        &breakdown, settled,
                        Some(&debuffs),
                        ledger::Clock::Hit,
                        || Instance {
                            // WHICH PELLET OF THE PULL THIS WAS. The first is the one
                            // the trigger would have fired with no multishot at all;
                            // every one after it is multishot's, and telling them apart
                            // is the difference between "my multishot works" and
                            // "something is double-counting".
                            //
                            // THE ORIGIN IS THE PELLET'S, NOT THE STAGE'S. A
                            // pellet with an explosion produces two rows and
                            // they are the SAME pellet — `radial` says which
                            // half and `pellet` says whose — so a Laetum
                            // Incarnon's three pellets read as six numbers in
                            // three pairs rather than as three plus three. The engine already settles
                            // them in that order: the stage loop is inside the
                            // pellet loop, so only the LABEL was missing.
                            // A MELEE SWING'S SECOND LANDING IS NOT MULTISHOT.
                            // It rides the same loop, and the column that says
                            // WHY this instance exists must not answer with a
                            // mechanic melee has no card for.
                            origin: if pellet_idx == 0 {
                                crate::record::Origin::Own
                            } else if swing.as_ref().is_some_and(|h| h.hits > 1) {
                                crate::record::Origin::Own
                            } else {
                                crate::record::Origin::Multishot
                            },
                            pellet: Some(pellet_idx + 1),
                            radial: !direct,
                            // THE WEAPON'S OWN DAMAGE, before any bracket —
                            // the layers start here and the first one is the
                            // base-damage bracket. Never `qtotal`, which is the
                            // chain's MIDDLE.
                            base: if (1.0 + base_damage).abs() > 1e-12 {
                                stage_mb / (1.0 + base_damage)
                            } else {
                                stage_mb
                            },
                            layers: pellet_layers(
                                stage_mb,
                                gunco,
                                base_damage,
                                arcane_base_damage,
                                &pre_quantization,
                                if pre_ability_total > 0.0 { qvec.total() / pre_ability_total } else { 1.0 },
                                beam_merge,
                                &hit_steps,
                                part.multiplier,
                                params.headshot_damage_bonus,
                                part.is_head,
                                cd,
                                ap.unmodded_crit_damage,
                                (
                                    params.ability_final_at(t) - 1.0,
                                    1.0 - co_mult.co_share.clamp(0.0, 1.0),
                                ),
                            ),
                            crit_damage: cd,
                            part: Some(part.name.clone()),
                            head: part.is_head,
                            crit_tier: tier,
                            procs: $procs,
                        },
                    );
                    };
                }
                if killed {
                    gal.bump_on_kill(params, t);
                    arc.on_kill(params, t);
                    if head_direct {
                        // Deadhead's precision boundary: only direct-pellet
                        // HEADSHOT kills grant/refresh its stacks.
                        arc.bump_trigger(&params.arcane.buffs, ArcTrigger::HeadshotKill, t);
                        // Crosshairs stacks: headshot kills, per-stack FIFO.
                        if let Some(s) = &params.crit_chance_stack {
                            DebuffState::push_capped(
                                &mut ch_stacks,
                                t + s.duration,
                                s.max_stacks as usize,
                                t,
                            );
                        }
                    }
                    // The killing instance's procs die with the old
                    // individual; the CLOUDS do not — see the note in
                    // `field_tick`. What follows hits the fresh spawn, standing
                    // in whatever is still burning where it spawned.
                    debuffs.on_death(params.acid_shells, &params.target);
                    log_this_pellet!(Vec::new());
                    continue;
                }
                // THE EXTRA HIT, off a WEAPON damage instance — the direct
                // pellet and the explosion alike, since both are hits the gun
                // dealt ("Most non-standard weapon hits will trigger an Extra
                // Hit, including Acid Shells and Concealed Explosives").
                //
                // AFTER the kill check, which is the wiki's rule and costs one
                // line here rather than a condition: "If a hit that would
                // trigger an Extra Hit kills the enemy, the Extra Hit will not
                // be triggered."
                //
                // `stage_bracket` is the correction: the extra hit is scaled by
                // the BASE ATTACK's elemental/IPS bracket, and `raw` already
                // carries THIS stage's. They are the same number on the direct
                // hit — the ratio is exactly 1 and nothing moves — and differ on
                // an explosion whose damage type is not the gun's.
                let stage_bracket = if stage_mb > 0.0 { qvec.total() / stage_mb } else { 1.0 };
                let xh_bracket = ap.extra_hit_bracket(t) / stage_bracket.max(1e-12);
                // …and the BODY PART, a second time, on a direct hit only. DE's
                // CN card, in the same breath as the faction double-dip: "同理，
                // 弱点倍率也会被计算两次". A radial struck no body part, so
                // `part_factor` is already 1.0 there and this reads as it should.
                if fire_extra_hits(
                    raw,
                    xh_bracket,
                    part_factor,
                    head_direct,
                    status_chance,
                    t,
                    &mut debuffs,
                    &mut gal,
                    &mut arc,
                    &mut target,
                    params,
                    ap,
                    &mit,
                    &mut r,
                    rec,
                    &mut d.status,
                ) {
                    // …and the clouds stay where they were. See `field_tick`.
                    continue;
                }
                // Per-INSTANCE proc roll (wiki Multishot/Status_Effect):
                // forced ++ SC draws weighted by the QUANTIZED vector, unit
                // immunities renormalized.
                let mut procs = status::procs_for_hit(
                    forced,
                    status_chance,
                    &qvec,
                    &params.target.status_immunities,
                    &mut d.status,
                );
                // HUNTER MUNITIONS: a critical hit rolls its OWN Slash status,
                // per pellet, "not affected by the weapon's Status Chance, or
                // damage type distribution, besides being indirectly affected
                // by its Critical Chance" (wiki). So it is a separate draw
                // pushed onto this pellet's proc list rather than anything
                // that touches the status roll above — and a weapon with no
                // Slash in its vector still gets one, which is the whole point
                // of the mod.
                //
                // Pushed HERE rather than applied as a bleed, which makes
                // its damage right for free: a Slash proc is `0.35 x ModdedBase
                // x the PROCCING HIT's crit/part`, so the tier this pellet
                // rolled and the part it struck already scale it.
                //
                // It STACKS with an innate Slash ("but can stack with Slash
                // statuses applied using a weapon's innate status chance") and
                // does not check `procs`. It cannot double a FORCED one, and a
                // weapon-forced Slash is checked here at its source, since
                // `procs` cannot say which Slash came from where; Internal
                // Bleeding's own guard runs after and sees this push.
                if ap.slash_on_crit > 0.0
                    && tier >= 1
                    && !params.forced_procs.contains(&DamageType::Slash)
                    && !params
                        .target
                        .status_immunities
                        .contains(&DamageType::Slash)
                    && d.extra.chance(ap.slash_on_crit)
                {
                    procs.push(DamageType::Slash);
                }
            // Secondary Encumber: on a status this pellet applied, roll
            // ONE extra status of a uniformly random type (13-type pool,
            // independent of the weapon's vector — wiki), at most once per
            // instant (= per trigger pull, and the radial STAGE shares the
            // limit — the wiki names Explosions among the simultaneous
            // attacks that "only proc up to once on a single target").
            //
            // This reproduces the wiki's per-shot rate
            //   1 − (1 − chance × min(statusChance, 1)) ^ pellets
            // exactly, without implementing the min() as a cap: a pellet
            // either applied a status (`!procs.is_empty()`) or it did not, so
            // status chance above 100% guarantees the first proc but cannot
            // give a pellet two shots at Encumber. Trigger scope is
            // health-bar statuses only (U33 patch note) — the only kind a
            // proc list ever holds.
            if params.arcane.encumber_chance > 0.0
                && !encumber_done
                && !procs.is_empty()
                && d.extra.chance(params.arcane.encumber_chance)
            {
                const POOL: [DamageType; 13] = [
                    DamageType::Impact,
                    DamageType::Puncture,
                    DamageType::Slash,
                    DamageType::Heat,
                    DamageType::Cold,
                    DamageType::Electricity,
                    DamageType::Toxin,
                    DamageType::Blast,
                    DamageType::Corrosive,
                    DamageType::Magnetic,
                    DamageType::Viral,
                    DamageType::Gas,
                    DamageType::Radiation,
                ];
                let idx = (d.extra.next_f64() * POOL.len() as f64) as usize % POOL.len();
                procs.push(POOL[idx]);
                encumber_done = true;
            }
            // Internal Bleeding / Hemorrhage: one roll per damage INSTANCE
            // when a `from` status landed and no `to` status did; chance ×2
            // while the LIVE fire rate is strictly below 2.5.
            //
            // The `!procs.contains(to)` guard is the whole stacking rule, and
            // it is STRICTER than Hunter Munitions': this one "cannot produce
            // multiple procs in a single instance of damage alongside ANY
            // other Slash sources, such as a weapon's innate Slash, Hunter
            // Munitions, or the debuff from Seeking Talons" (wiki) — innate
            // Slash included, which is why it reads `procs` rather than
            // `forced_procs`. Hunter Munitions pushes above, so it is already
            // in `procs` here and this roll is skipped, which is exactly
            // "if both proc at the same time, only 1 slash proc is applied".
            if let Some(pc) = ap.proc_conversion {
                if procs.contains(&pc.from) && !procs.contains(&pc.to) {
                    let chance = pc.chance
                        * if live_rate < pc.low_rate_threshold {
                            pc.low_rate_multiplier
                        } else {
                            1.0
                        };
                    if d.status.chance(chance) {
                        procs.push(pc.to);
                    }
                }
            }
            // Overwhelming Attrition's TRIGGER, evaluated once the proc
            // list is final: "On Hit that is neither Critical nor applies
            // a Status Effect" (wiki). PER DAMAGE INSTANCE — measured
            // (MEASUREMENTS M11: one shot into a crowd fills all 3 stacks,
            // and one shot at a LONE target grants exactly 2 — the direct
            // hit and the explosion each arm it). So a shot whose direct hit
            // and whose explosion are both plain arms the buff twice,
            // bounded by the stack cap.
            // STRIKING SUCCESSION: "On Hit", with no qualifier at all — so it
            // is armed by the same damage instance the line below inspects, and
            // simply does not read what the instance did. Per instance for the
            // same measured reason (M11): a direct hit and its explosion are
            // two.
            bump_buffs!(crate::model::BuffTrigger::Hit, t, d.extra);
            if tier == 0 && procs.is_empty() {
                bump_buffs!(crate::model::BuffTrigger::PlainHit, t, d.extra);
            }
            // STORMBURST: the condition is on the TARGET, read here where the
            // debuffs are in hand. Bumped AFTER this pull's multishot was
            // rolled, so the hit that earns a stack does not fire it — the same
            // rule every other stacking buff in this loop follows.
            bump_status_buffs!(&debuffs, t, d.extra);
            // ...and ARM it for the next pellet. ONE roll per pellet that
            // landed at least one status — "Applying multiple status effects in
            // a single hit does not increase the chance for the effect" — and
            // per PELLET rather than per trigger pull, since the card says it
            // triggers "separately for each bullet when using Multishot".
            //
            // Rolled BEFORE `settle_procs` consumes `procs`, and after the spend
            // above: a pellet that spends the buff can re-arm it with its own
            // status, which is what makes a high-status weapon hold it up.
            if let Some(sc) = params.super_crit_on_status {
                if !procs.is_empty() && d.extra.chance(sc.chance) {
                    super_crit_armed = true;
                }
            }
            // PARAGON ESSENCE: "On Status Effect", one stack per status that
            // LANDS. Read literally — the card names the effect, not the hit —
            // so a pellet that procs twice earns two. Bumped before
            // `settle_procs` consumes the list, and only here: this is where a
            // PELLET's own statuses land, and a field tick's or an extra hit's
            // are a different sentence that no card in the roster has written
            // yet.
            for _ in 0..procs.len() {
                bump_buffs!(crate::model::BuffTrigger::StatusApplied, t, d.extra);
            }
            // INDEPENDENT PROCS land on the HIT, not on the roll — that is what
            // "independent from damage" means, and it is why this is here
            // rather than inside `settle_procs`: a weapon that lifts lifts with
            // no status chance at all, and a field tick or an Extra Hit that
            // borrows this weapon's vector does NOT lift, because it is not
            // this attack landing.
            //
            // Against one target at the centre the direct and the radial of the
            // Mausolon's laser always arrive together, so no distinction is
            // drawn between which of the two lifts.
            if ap.independent_procs.contains(&"lifted") {
                // A STATE, so it REFRESHES: the later expiry wins rather than
                // the count going up.
                let until = t + LIFTED_SECONDS * params.status_duration_multiplier;
                debuffs.lifted = Some(debuffs.lifted.map_or(until, |e| e.max(until)));
            }
            // …AND THE SWING'S OWN, which a stance marks per attack. Same rule
            // as the weapon-level pair one line up: applied on the HIT rather
            // than through the roll, and refreshed rather than stacked.
            if swing_forced_independent.contains(&"lifted") {
                let until = t + LIFTED_SECONDS * params.status_duration_multiplier;
                debuffs.lifted = Some(debuffs.lifted.map_or(until, |e| e.max(until)));
            }
            if swing_forced_independent.contains(&"knockdown") {
                let until = t + KNOCKDOWN_SECONDS * params.status_duration_multiplier;
                debuffs.knockdown = Some(debuffs.knockdown.map_or(until, |e| e.max(until)));
            }
            if ap.independent_procs.contains(&"knockdown") {
                let until = t + KNOCKDOWN_SECONDS * params.status_duration_multiplier;
                debuffs.knockdown = Some(debuffs.knockdown.map_or(until, |e| e.max(until)));
            }
                // …AND THE ORDINARY END OF A PELLET, which does get to name
                // what it applied.
                log_this_pellet!(procs.clone());
                // …AND WHAT MELEE INFLUENCE WILL COPY, taken before
                // `settle_procs` consumes the list. Cloned only when the arcane
                // is equipped: this is the hot loop and every other build pays
                // an empty `Vec`.
                let aimed_procs = if params.arcane.influence_chance > 0.0 {
                    procs.clone()
                } else {
                    Vec::new()
                };
                settle_procs(
                    procs,
                    t,
                    // THE HIT'S ATTRITION ROLL TRAVELS WITH ITS STATUSES. A proc's
                    // magnitude is the applying instance's — which is why
                    // `crit_multiplier` is already here — and Devouring/Devastating
                    // Attrition is a per-instance multiplier of exactly that shape,
                    // so a DoT applied by a 21x hit ticks for 21x.
                    //
                    // Measured through the Debilitate chain: the
                    // final DoT eats, i.e. 21x21. Two layers for three faction
                    // layers — the split instance rolls one, and the other can
                    // only be the applying hit's, carried here. A DoT is not a
                    // hit, so it never rolls one of its own. MEASUREMENTS M37.
                    InstanceScale {
                        mb_live,
                        crit_multiplier,
                        part_factor,
                        landing,
                        attrition,
                        // The BASE ATTACK's, so a Blast stack this instance applies
                        // remembers the bracket its detonation's extra hit takes —
                        // not this stage's, which the detonation itself never gets.
                        xh_bracket: ap.extra_hit_bracket(t),
                    },
                    &mut debuffs,
                    &mut gal,
                    &mut arc,
                    &mut target,
                    params,
                    ap,
                    &mit,
                    &mut r,
                    rec,
                    &mut d.status,
                    &params.target,
                    DEPTH_PROC,
                );
                // MELEE INFLUENCE — every eligible status this swing applied,
                // arriving on everything standing around the body that took it.
                //
                // THE WHOLE SWING SEEDS IT, aimed body and Follow Through
                // alike: *"Melee Influence only triggers from direct melee
                // strikes"*, and a swing reaching past the first body is one of
                // those. A body the swing KILLED seeds nothing — *"hits that
                // one-hit-kill enemies cannot trigger nor benefit"*, which the
                // wiki files under Bugs and which is the behaviour all the same.
                if params.arcane.influence_chance > 0.0 {
                    influence_seeds.push((0, Landed { procs: aimed_procs, raw, killed: false }));
                    // THE SPREAD READS THE WINDOW THIS SWING FOUND OPEN, and the
                    // grant below is rolled after: the buff is what an
                    // Electricity status GRANTS, so the swing that grants it has
                    // already resolved. Nothing published says the granting hit
                    // also spreads, and assuming it would invent a copy of the
                    // swing.
                    let open = t < influence_until;
                    let scale = InstanceScale {
                        // THE STATUS IS THE SWING'S OWN, one derivation further
                        // out — so it burns off the base the swing's statuses
                        // burn off, and only the faction rung differs. The wiki
                        // checks it from that side: an ordinary Electricity proc
                        // of 228 spreads as 353, and 353 / 228 is exactly one
                        // more faction multiplier.
                        mb_live,
                        crit_multiplier,
                        part_factor,
                        landing,
                        attrition,
                        xh_bracket: ap.extra_hit_bracket(t),
                    };
                    if open {
                        for (from, landed) in &influence_seeds {
                            if landed.killed {
                                continue;
                            }
                            let carried: Vec<(DamageType, f64)> = landed
                                .procs
                                .iter()
                                .copied()
                                .filter(|ty| influence_can_spread(*ty))
                                .map(|ty| (ty, landed.raw * shares.share(ty)))
                                .collect();
                            spread_from_influence(
                                &body_at,
                                &mut others,
                                &mut target,
                                &mut debuffs,
                                params,
                                ap,
                                *from,
                                &carried,
                                scale,
                                params.arcane.influence_radius_m,
                                &mut gal,
                                &mut arc,
                                &mut r,
                                rec,
                                d,
                                t,
                            );
                        }
                    }
                    // …AND THE ROLL THAT OPENS IT, off any Electricity status
                    // this swing landed. One roll for the swing rather than one
                    // per body: the card is *"On Melee Electricity Status"* and
                    // a window that is already open cannot be refreshed, so a
                    // second success in the same instant buys nothing anyway.
                    if !open
                        && influence_seeds.iter().any(|(_, l)| {
                            !l.killed && l.procs.contains(&DamageType::Electricity)
                        })
                        && d.extra.chance(params.arcane.influence_chance)
                    {
                        influence_until = t + params.arcane.influence_seconds;
                    }
                }
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
                tendrils,
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
            combo_count = 0;
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
        if rounds_this_mag.is_multiple_of(burst_len) {
            bump_buffs!(crate::model::BuffTrigger::FullBurst, t, rng);
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
        if instant_reload_now {
            instant_reload_now = false;
            match &params.cycle {
                Some(cy) => {
                    base_mag += draw_from(&mut reserve, params.infinite_reserve,
                        reload_draw(cy.base_form.magazine_size, base_mag));
                }
                None => {
                    magazine += draw_from(&mut reserve, params.infinite_reserve,
                        reload_draw(mag_cap, magazine));
                }
            }
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
                let room = (mag_cap - magazine).max(0.0);
                let want = rounds.min(room);
                if want > 0.0 {
                    magazine += draw_from(&mut reserve, params.infinite_reserve, want);
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
            // KILLS ADVANCE THEIR MARK IN EITHER FORM, and the two halves are
            // separate on purpose. A hit is bounded by the shot that caused it,
            // so `_before` is exact for one; a kill is not — a status tick
            // between two shots kills, and a delta taken at the top of the next
            // shot has already missed it. And the mark advances even while
            // TRANSFORMED, so a kill made with the earned form can never pay
            // for the next one: it is kills with the PRIMARY fire that recharge
            // the Mausolon's laser (wiki), not kills of any kind.
            let fresh_kills = r.kills - gauge_kill_mark;
            gauge_kill_mark = r.kills;
            if in_base_form {
                // Per PELLET, and per the WEAPON's rule: weak-point hits for
                // the Zariman pistols, any direct hit for the Torid, kills for
                // the Mausolon. A field or radial instance is neither of the
                // first two, so neither can charge those — but it CAN kill,
                // which is the whole difference the third one makes.
                charges += match charge_on {
                    crate::model::ChargeOn::WeakpointHits => r.headshots - headshots_before,
                    crate::model::ChargeOn::DirectHits => r.pellets - pellets_before,
                    crate::model::ChargeOn::Kills => fresh_kills,
                };
                // A FULL GAUGE ARMS THE TRANSFORM; the cadence below still
                // runs. A `continue` here would skip the completing shot's OWN
                // interval and let the next shot fire at the same instant,
                // making the transform a free extra shot. The moment is the end
                // of the shot that filled the gauge, which is the start of the
                // next one.
                //
                // The gauge also OVERSHOOTS and that is not a rounding: a
                // 7-pellet shot into a 30-charge gauge arrives at 35 on the
                // fifth shot, never at 30, so the comparison is `>=` and the
                // shot that crosses it is fired in the BASE form.
                if charges >= charges_to_fill {
                    // BOTH DIRECTIONS TAKE IT. The wiki says Ready
                    // Retaliation "can affect transition INTO Incarnon form
                    // with a well-timed manual reload" and not the way back;
                    // the second half is wrong, and transforming with an EMPTY
                    // magazine is the proof — the animation is faster, so the
                    // buff was there before any reload began.
                    //
                    // AND IT IS SPENT WHEN THE TRANSFORM COMPLETES, which
                    // collapses the rule to one line: swapping either way fully
                    // reloads the base form's magazine (wiki), so both
                    // transforms are reloads and the buff is spent by whatever
                    // refills the magazine.
                    // WAS THE BASE MAGAZINE ACTUALLY EMPTY? Read BEFORE the
                    // refill below, because that is the question the card asks:
                    // "Switching to Incarnon Form from empty will also trigger
                    // the buff" (wiki, Soma's Fresh Havoc). Transforming with
                    // rounds still in the magazine reloads it and earns nothing,
                    // which is the one place `ReloadFromEmpty` and
                    // `ReloadComplete` are different events.
                    let transformed_from_empty = !can_fire(base_mag, 1.0);
                    let spent = rescale_reload(cy.transmute_seconds, cy.reload_bucket,
                        live_reload_speed(params, &cy.base_form, rs_armed, &mut buff_stacks, t));
                    rec.push(t, None, crate::record::Kind::TransformStart {
                        seconds: spent,
                        into_transmuted: true,
                    });
                    r.downtime_seconds += spent;
                    t += spent;
                    magazine_refilled!();
                    if transformed_from_empty {
                        bump_on_trigger!(
                            crate::model::BuffTrigger::ReloadFromEmpty, t, d.spine);
                    }
                    r.transforms += 1;
                    in_base_form = false;
                    swap_dt_pile!(t);
                    // The CHARGE magazine is filled by the gauge, not reloaded
                    // from reserve — it is outside the ammo economy, takes no
                    // efficiency, and so is always whole anyway.
                    magazine = mag_cap;
                    // THE ROW LANDS HERE, not beside the push above: an event
                    // is stamped with the weapon AS IT NOW IS, and until this
                    // line the form and the magazine are still the old ones.
                    // The base magazine's refill IS a reload: whole rounds off whatever is already in it,
                    // and out of the same reserve as every other reload. This
                    // was the site that kept the base magazine topped up for
                    // free — with all three draws inside the cycle unbilled, a
                    // finite reserve never moved off its starting value.
                    let loaded = draw_from(&mut reserve, params.infinite_reserve,
                        reload_draw(cy.base_form.magazine_size, base_mag));
                    base_mag += loaded;
                    // THE ROW LANDS HERE, once BOTH magazines are what the
                    // transmute made them: the charge magazine it filled and the
                    // base one it silently reloaded. Announced any earlier and
                    // the free reload — the whole reason both magazines are on
                    // every row — is missing from the row that performed it.
                    weapon_now!();
                    rec.push(t, None, crate::record::Kind::TransformEnd { transmuted: true });
                    // …AND THAT RELOAD PAYS ITS SHELLS. One as you go in, the
                    // rest owed until you come out.
                    //
                    // Counting the shells the draw ACTUALLY loaded is what
                    // makes a dry reserve behave: no shells, no stacks, and no
                    // separate rule needed to say so.
                    let shells = loaded.round().max(0.0) as u32;
                    if shells > 0 {
                        bump_shells!(1, t, rng);
                        owed_shells = shells - 1;
                    }
                                                           // Frenzy persists across the transform.
                }
            }
        }

        // ---- WHAT THIS SWING DID TO THE COMBO COUNTER -------------------
        //
        // GAIN FIRST, THEN SPEND, and the order is the game's: a heavy attack
        // pays the multiplier that was standing when it went down (read above,
        // into `combo_mult`), lands, and then empties the counter.
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
            let landed = (r.pellets - pellets_before) as f64;
            // RAGE BUILDS ON EVERY BODY A HIT LANDED ON AND EVERY KILL SINCE THE
            // LAST SWING — a status kill "still counts as a melee kill". The kill
            // is paid at this swing rather than at the death.
            if let Some(g) = arc.rage.as_mut() {
                let s = g.spec();
                g.build(t, landed * s.per_hit + f64::from(r.kills - rage_kill_mark) * s.per_kill);
            }
            rage_kill_mark = r.kills;
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
            if tennokai && ap.tennokai.chain_seconds > 0.0 && r.kills > tennokai_kill_mark {
                tennokai_until = t + ap.tennokai.chain_seconds;
                tennokai_chained = true;
                tennokai_hits = 0;
            }
            // …AND THE CURSE, which is the whole cost of the card: a Tennokai
            // attack that FAILS to kill empties the counter. Status immunity
            // does not save it — *"your combo will still be reset"* — so it is
            // unconditional here, and the Heat is COUNTED rather than applied
            // because nothing in this arena damages the Tenno.
            if tennokai && ap.tennokai.curse_resets_combo && r.kills == tennokai_kill_mark {
                combo_points = 0.0;
                r.self_damage.add(
                    DamageType::Heat,
                    ap.tennokai.curse_heat_per_second * ap.tennokai.curse_seconds,
                );
            }
            if ap.tennokai.enabled && landed > 0.0 && t >= tennokai_until {
                tennokai_hits += 1;
                let opens = if ap.tennokai.every_n_hits > 0 {
                    tennokai_hits.is_multiple_of(ap.tennokai.every_n_hits)
                } else {
                    // 15% BASE, and the cards add to it.
                    d.spine.chance(TENNOKAI_BASE_CHANCE + ap.tennokai.chance)
                };
                if opens {
                    let w = if ap.tennokai.window_seconds > 0.0 {
                        ap.tennokai.window_seconds
                    } else {
                        TENNOKAI_WINDOW_SECONDS
                    };
                    tennokai_until = t + w;
                    tennokai_hits = 0;
                }
            }
            // A HEAVY ATTACK EARNS NOTHING. *"connecting with a heavy attack
            // does not add to the combo counter"* (wiki, Melee), and it is the
            // swing's KIND that says so rather than the form: a Tennokai heavy
            // on a light combo is one too. On a spending form it is visible
            // only through Melee Combo Efficiency, which is the share of the
            // counter the swing does NOT empty.
            let earns = !(ap.spends_combo || tennokai_heavy);
            let mut gained = 0.0;
            if landed > 0.0 && earns {
                // EVERY LANDED INSTANCE IS A HIT OF ITS OWN: its base points take
                // the gain roll, Additional Combo Count Chance and what is left
                // of it — `swing_combo_gain`. ENDURING STRIKE adds to that chance
                // while the target is LIFTED, a status this engine tracks rather
                // than a state it has to assume.
                let chance_now = ap.combo_count_chance
                    + if debuffs.lifted.is_some_and(|e| e > t) {
                        ap.combo_count_chance_on_lifted
                    } else {
                        0.0
                    };
                for _ in 0..(landed as u32) {
                    gained += swing_combo_gain(
                        h.combo_points,
                        h.combo_points_base,
                        chance_now,
                        ap.combo_gain_chance,
                        &mut |p| d.spine.chance(p),
                    );
                }
                combo_points += gained;
            }
            if refreshes_combo_timer(landed, earns, gained) {
                combo_expiry = t + ap.combo_duration_seconds;
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
            if ap.spends_combo && !tennokai {
                // …AND IT SPENDS WHAT THE SWING READ, floor included. The
                // counter is ONE number: *"40% heavy attack efficiency will
                // change the amount spent to 60% combo points"*, and the points
                // an initial-combo floor put there are points like any other.
                // Spending only the EARNED half left a heavy mode at zero after
                // every swing — it earns none — so efficiency bought nothing at
                // all in the one family of modes whose cards sell it.
                combo_points = combo_now * ap.heavy_attack_efficiency;
                combo_spent_t = t;
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
                    if in_base_form && (ap.spends_combo || tennokai_heavy) && combo_mult >= at {
                        in_base_form = false;
                        incarnon_until = t + window;
                        weapon_now!();
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
            if ap.combo_count_on_slam_hit > 0.0 && !(ap.spends_combo || tennokai_heavy) {
                let slam_rad = match (h.slam_multiplier, ap.slam) {
                    (Some(_), Some(s)) => Some(s),
                    _ => ap.radial.filter(|r| {
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
                    let reached = (target.health > 0.0
                        && crate::rules::space::caught_by_blast(
                            det.distance_to(params.target_at),
                            rad.radius_m,
                        )) as u32
                        + params
                            .others
                            .iter()
                            .enumerate()
                            .filter(|(i, spec)| {
                                others[*i].state.health > 0.0
                                    && crate::rules::space::caught_by_blast(
                                        det.distance_to(spec.at),
                                        rad.radius_m,
                                    )
                            })
                            .count() as u32;
                    if reached > 0 {
                        combo_points += ap.combo_count_on_slam_hit
                            * f64::from(reached)
                            * (1.0 + ap.combo_count_chance);
                        combo_expiry = t + ap.combo_duration_seconds;
                    }
                }
            }
        }

        // Next shot: cadence reflects the bar as of now (Frenzy just
        // granted/refreshed counts immediately), plus Pressurized
        // Magazine's live on-reload fire-rate buff.
        bar.expire(t);
        let mut fr_add = match ap.fire_rate_on_reload {
            Some(b) if t < fire_rate_reload_expiry_seconds => b.value,
            _ => 0.0,
        };
        // THE SAME BUCKET fire-rate mods and a static `fire_rate_bonus`
        // evolution live in — `base * (1 + fr + evo + 0.05n)`. `per_stack` is
        // already the absolute rate that fraction is worth, so adding it here,
        // inside the bracket rather than outside it, is what keeps it additive
        // with mods instead of multiplicative with them.
        fr_add += buff_total!(ap, crate::model::BuffGrant::FireRate, t);
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
        if t > spool_due + 1e-9 {
            spool_shots = 0.0;
        }
        last_shot_t = t;
        // …and then the SPOOL, which is a fraction of whatever that rate came
        // to: a fire-rate mod raises the ceiling and the floor together, so the
        // Phenmor's Incarnon form still spends most of its 408-round magazine
        // at 60% of whatever it was built to.
        let rate = rate * spool_factor(ap.sustained_fire_rate, spool_shots);
        spool_shots += 1.0;
        // On a CHARGE weapon the pull costs a draw, not a rate: divide the
        // modded charge time by whatever the live buffs did to the rate
        // (`rate / ap.fire_rate` is exactly that factor, and it is 1.0 when no
        // buff is up). Same bucket, reciprocal application — see `charge_seconds`.
        t += match ap.charge_seconds {
            Some(c) => {
                let draw = c * ap.fire_rate / rate.max(1e-9);
                match ap.charge_cadence {
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
            // `rate / ap.fire_rate`, clamped so a penalty does not stretch it.
            // A MELEE SWING HAS ITS OWN LENGTH. A stance publishes a
            // sequence and a per-combo damage-per-second, so the combo lasts
            // `sum of multipliers / that rate` and the swings share it EVENLY —
            // the script's one approximation, declared on every melee entry,
            // since nothing published states a swing's animation length.
            //
            // DIVIDED BY THE LIVE ATTACK SPEED: the script is published at
            // 1.0x, so Fury is an ordinary fire-rate mod here and an on-kill
            // speed buff shortens a swing without knowing melee exists.
            None if !ap.combo_script.is_empty() => {
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
                if tennokai && (tennokai_heavy || ap.spends_combo) {
                    w = ap.tennokai.windup_seconds;
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
                    swing_idx = 0;
                } else {
                    swing_idx += 1;
                }
                let cycle = (w + d / rate.max(1e-9)).max(1e-6);
                // …AND A HEAVY MODE SWINGS WHEN THE COUNTER IS WORTH SPENDING,
                // which is not always as soon as the animation allows — see
                // `heavy_cycle_seconds`. Derived from the swing SPENDING the
                // counter rather than from the mode, so a standing heavy and a
                // heavy slam make the same decision and the next weapon needs
                // no field.
                if ap.spends_combo {
                    heavy_cycle_seconds(cycle, combo_points, initial_now)
                } else {
                    cycle
                }
            }
            None => match ap.burst {
                Some(b) if b.count > 1 => {
                    let live = (rate / ap.fire_rate.max(1e-9)).max(1.0);
                    let mag_now = if in_base_form { base_mag } else { magazine };
                    let pull_goes_on =
                        !rounds_this_mag.is_multiple_of(b.count) && can_fire(mag_now, 1.0);
                    if pull_goes_on { b.delay_seconds / live } else { 1.0 / rate }
                }
                _ => 1.0 / rate,
            },
        };
        spool_due = t;
    }

    // THE METER'S LAST FILLS, after the trigger stops. A weapon that is out of
    // ammo still recharges, and an orb earned at t = 179 is an orb the fight
    // gets — so the clock is run out to the end and every throw it buys is
    // thrown, before the orbs are drained below.
    if let (Some(m), Some(o)) = (field_ap.meter, field_ap.orb) {
        meter_seconds += params.duration_seconds - meter_clocked;
        while meter_seconds >= m.seconds_to_fill {
            meter_seconds -= m.seconds_to_fill;
            // AT THE INSTANT IT FILLED, not at the end: the orb has a six
            // second fuse and the difference is whether its strikes land inside
            // the engagement at all.
            let at = params.duration_seconds - meter_seconds;
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
    sample_frames_up_to!(params.duration_seconds);

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
