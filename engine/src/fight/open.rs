// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE ENGAGEMENT BEFORE ITS FIRST SHOT — the streams, the arena's geometry,
//! the quantized damage vectors, and every counter the fight starts holding.
//!
//! What splits the setup in two is what the loop then does with it: `Run` is
//! what the fight CHANGES, `Fixed` is what it only reads. A value that cannot
//! differ between two shots belongs in `Fixed`, and putting it there is what
//! keeps it from being recomputed per pellet — the geometry here was measured
//! at a 35% slowdown when the aim angle was read per pellet.

use super::*;

/// WHAT THE FIGHT CHANGES.
pub(super) struct Run {
    pub(super) d: crate::rules::rng::Draws,
    pub(super) next_frame: f64,
    pub(super) bar: BuffBar,
    pub(super) enervate: Option<SecondaryEnervate>,
    pub(super) frenzy: Frenzy,
    pub(super) target: TargetState,
    pub(super) debuffs: DebuffState,
    pub(super) others: Vec<SpreadFoe>,
    pub(super) gal: GalStacks,
    pub(super) buff_stacks: Vec<LiveStacks>,
    pub(super) rs_armed: bool,
    pub(super) opening_closed: bool,
    pub(super) r: RunResult,
    pub(super) ammo: Ammo,
    pub(super) kill_buff_mark: u32,
    pub(super) double_tap: DoubleTap,
    pub(super) arc: ArcRuntime,
    pub(super) windows: CardWindows,
    pub(super) weakpoint_pile: LiveStacks,
    pub(super) beam: BeamRamp,
    pub(super) field_duration_boost: bool,
    pub(super) fields: Vec<FieldState>,
    pub(super) orbs: Vec<OrbState>,
    pub(super) field_ctx: FieldCtx,
    pub(super) meter: Meter,
    pub(super) strip_kills_seen: u32,
    pub(super) t: f64,
    pub(super) super_crit_armed: bool,
    pub(super) incarnon: IncarnonState,
    pub(super) ghost_pile: Ghosts,
    pub(super) syndicate: Syndicate,
    pub(super) crit_per_hit: CritPerHit,
    pub(super) sniper_combo: SniperComboCount,
    pub(super) tendril: Tendrils,
    pub(super) spool: Spool,
    pub(super) melee: MeleeState,
    pub(super) influence_until: f64,
    pub(super) last_shot_t: f64,
}

/// WHAT IT ONLY READS — resolved once, constant for the whole engagement.
pub(super) struct Fixed<'a> {
    pub(super) aim_off_axis: f64,
    pub(super) body_at: Vec<crate::rules::space::Vec2>,
    pub(super) area_near: crate::rules::space::Neighbours,
    pub(super) bounce_bodies: Vec<crate::rules::space::Vec2>,
    pub(super) ricochet_layout: Option<crate::rules::chain::Layout>,
    pub(super) chain_layout: Option<crate::rules::chain::Layout>,
    pub(super) struck: Vec<usize>,
    /// The same, for the cycle's base form — see `open`.
    pub(super) base_struck: Option<Vec<usize>>,
    pub(super) frame_seconds: f64,
    pub(super) rec_roster: Vec<BuffSeries>,
    pub(super) rec_buff_index: Vec<Option<usize>>,
    pub(super) main_variants: Vec<(crate::rules::damage::DamageVector, f64, f64)>,
    pub(super) main_variant_rad: Vec<crate::rules::damage::DamageVector>,
    pub(super) main_pre: (crate::rules::damage::DamageVector, f64, f64),
    pub(super) base_pre: Option<(crate::rules::damage::DamageVector, f64, f64)>,
    pub(super) base_variants: Vec<(crate::rules::damage::DamageVector, f64, f64)>,
    pub(super) base_variant_rad: Vec<crate::rules::damage::DamageVector>,
    pub(super) status_damage: f64,
    pub(super) field_active: &'a FightParams,
    pub(super) combo_spec: Option<crate::model::SniperCombo>,
}

/// Open the engagement: advance the master `rng` past this run's seed, lay out
/// the arena, and hand back the state the loop advances beside it.
/// ALWAYS INLINED, and measured. The setup runs ONCE per engagement, so the
/// call itself is free — what is not is handing the loop back 38 pieces of
/// state through a struct: left to the compiler that move costs 2.7% a shot
/// across `one_fight`'s four shapes, and inlined it is inside the noise.
#[inline(always)]
pub(super) fn open<'a>(
    params: &'a FightParams,
    rng: &mut Rng,
    rec: &crate::record::Record,
    trace: &Option<&mut Replay>,
) -> (Run, Fixed<'a>) {
    // THE ENGAGEMENT'S SEED, and the three streams derived from it. The master
    // `rng` is only a seed source from here on: it is advanced once so the next
    // run in a `monte_carlo` differs, and every roll below comes off `d`. See
    // [`Draws`] for why the streams are split — in short, a build that changes
    // only its status chance must not re-roll this engagement's crits.
    let started_at = rng.state();
    let _ = rng.next_f64();
    let d = crate::rules::rng::Draws::new(started_at);
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
    // …AND THE BASE FORM'S OWN LINE. Punch through is a property of the FORM
    // rather than of the weapon — the Latron's base form pierces and its
    // Incarnon projectile does not (M103) — so one list for the engagement
    // answered the whole fight with the TRANSFORMED form's reach.
    let base_struck = params.cycle.as_ref().map(|cy| cy.base_form.struck_bodies());
    let next_frame = 0.0f64;
    let frame_seconds = trace.as_ref().map_or(f64::INFINITY, |r| r.frame_seconds);
    let mut bar = BuffBar::new();
    let enervate = params
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
    let target = TargetState::spawn(&params.target, params.target_at);
    let debuffs = DebuffState::default();
    // THE REST OF THE FORMATION — empty for every fight this engine has run,
    // and every line that reads it below is behind that check.
    //
    // BESIDE the aimed body's state rather than holding it too, which is the
    // aim policy showing through: the beam is on ONE body and every other is
    // reached only by what spreads (`formation`,). When the
    // aimed one dies the nearest of these takes its place, so `target` and
    // `debuffs` above stay what the whole loop already reads.
    let others: Vec<SpreadFoe> = params
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
    let buff_stacks: Vec<LiveStacks> = params
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
    let rs_armed = false;
    // THE OPENING WINDOW closes the first time the magazine is refilled, and
    // that is not always a reload: a weapon that TRANSMUTES instead of
    // reloading — the Torid, played as its cycle — never performs one in the
    // base form, and the window would never close at all. Measured 2026-08-11:
    // 0 reloads in the median run, 4.4 s of downtime, and an opening magazine
    // reading zero. The refill is the moment, whichever event caused it.
    let opening_closed = false;
    let r = RunResult {
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
    let ammo = Ammo {
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
    let kill_buff_mark: u32 = 0;
    let double_tap = DoubleTap {
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
    let arc = ArcRuntime::init(params);
    let windows = CardWindows {
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
        // EARNED, like every timed buff: a weak point has to land first — and
        // a card that seeds it open says so, a LOCKED one never shutting.
        weakpoint_buff: match (params.on_weakpoint, params.weakpoint_open) {
            (Some(_), Some((true, true))) => f64::INFINITY,
            (Some(b), Some((true, false))) => b.duration,
            _ => f64::NEG_INFINITY,
        },
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
    let weakpoint_pile = LiveStacks::default();


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
    let beam = BeamRamp::default();
    // Renewed Horror: armed by a reload from empty, spent by the next shot's
    // field. The sim always reloads from empty (it fires until dry), so on the
    // Torid this is the first shot of every magazine after the first.
    let field_duration_boost = false;
    // Live lingering FIELDS (Torid's clouds), one entry per grenade that stuck.
    let fields: Vec<FieldState> = Vec::new();
    // …AND LIVE ORBS, which are not fields: entities with a
    // place of their own that move, strike ONE body inside their reach, and
    // detonate where they have got to.
    let orbs: Vec<OrbState> = Vec::new();
    let field_ctx = FieldCtx::default();

    // The form whose panel SPAWNED the fields. Only one form of a transform
    // group has a lingering part (Torid's cloud belongs to the base form; its
    // Incarnon beam leaves none), so this is unambiguous - and it is the right
    // answer even after a transmute, because a cloud outlives one.
    let field_active: &FightParams = match &params.cycle {
        Some(cy) if cy.base_form.lingering.is_some() => &cy.base_form,
        _ => params,
    };

    let meter = Meter {
        seconds: field_active.meter.map_or(0.0, |m| m.seconds_to_fill),
        clocked: 0.0f64,
    };
    // KILLS JAHU CANTICLE HAS ALREADY STRIPPED FOR, read as a delta off the
    // run's own counter for the same reason the meter's pickups are: there are
    // nine places a body can die and a tenth would silently stop paying.
    let strip_kills_seen = 0u32;

    // Initial locks: one natural-duration grant at t = 0 (at the set
    // stack count); afterwards only the buff's own mechanics govern it.
    //
    // TWO GUARDS, and each is a way the card says NOTHING. ZERO STACKS IS OFF:
    // this loop grants by firing a synthetic headshot, so a card left at zero
    // opened the fight with the buff UP. And THE WEAPON MUST OWN THE PASSIVE —
    // Frenzy is a weapon perk, and an Incarnon cycle carries the lock whatever
    // weapon it was built for, so without this a Latron built with
    // `Initial(0)` fired at Frenzy's x2.5 rate.
    for lock in &params.locked_buffs {
        if let LockMode::Initial(stacks) = lock.mode {
            if stacks == 0 {
                continue;
            }
            match lock.buff {
                LockedBuff::Frenzy if !params.frenzy => {}
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
    let t = field_active.windup_seconds;
    // GOTVA PRIME'S PASSIVE, armed. Set by a pellet that landed a status, spent
    // by the next pellet that lands. It survives across shots and reloads: the
    // card says the chance "remains until landing another successful shot", and
    // nothing but a landing shot spends it.
    let super_crit_armed = false;
    let incarnon = IncarnonState {
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


    let ghost_pile = Ghosts {
        standing: Vec::new(),
        kill_mark: 0u32,
    };
    let syndicate = Syndicate {
        kill_mark: 0u32,
        points: 0.0f64,
        ready_at: 0.0f64,
    };
    let crit_per_hit = CritPerHit {
        hit_mark: 0u32,
        refill_mark: 0u32,
        seed: params
            .crit_chance_per_hit
            .map_or(0, |c| params.crit_chance_per_hit_initial_stacks.min(c.max_stacks())),
        stacks: params
            .crit_chance_per_hit
            .map_or(0, |c| params.crit_chance_per_hit_initial_stacks.min(c.max_stacks())),
    };
    let sniper_combo = SniperComboCount {
        count: params.combo_initial,
        last_hit: 0.0f64,
    };
    // THE COUNTER IS THE WEAPON'S, NOT THE FORM'S. Its spec — the minimum and
    // the decay period — comes from whichever form declares one, so a cycle
    // that spends half the engagement in a form with no combo does not lose
    // the count it built. What the form DOES decide is whether a hit in it
    // counts and whether it pays, which is read off `active` at the hit itself:
    // the Incarnon forms declare no combo (nothing published says whether it
    // survives the transform — see their `unmodeled:`), so their hits do
    // neither while the two-second clock keeps running.
    let combo_spec = params
        .sniper_combo
        .or_else(|| params.cycle.as_ref().and_then(|c| c.base_form.sniper_combo));
    let tendril = Tendrils {
        kill_mark: 0u32,
        reload_mark: 0u32,
        seed: params.tendrils_initial.min(params.tendril_max),
        count: params.tendrils_initial.min(params.tendril_max),
    };
    let spool = Spool {
        casts_paid: 0,
        shots: 0.0f64,
        due: f64::NEG_INFINITY,
    };
    // ---- THE MELEE COMBO COUNTER AND TENNOKAI -- see `MeleeState` ----------
    let melee = MeleeState {
        combo_points: 0.0f64,
        rage_kill_mark: r.kills,
        ability_extra_seconds: 0.0,
        ability_kill_mark: r.kills,
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
    let influence_until = params.influence_open.unwrap_or(f64::NEG_INFINITY);
    // WHEN THE LAST SHOT ACTUALLY WENT OFF, which is NOT `spool_due`. That one
    // is when the next shot was DUE, so the interval is already inside it and
    // the difference is zero on every ordinary pull — right for a spool, which
    // asks "did anything intervene", and useless for a battery, which asks how
    // long the weapon spent not firing.
    let last_shot_t = f64::NEG_INFINITY;
    (
        Run {
            d,
            next_frame,
            bar,
            enervate,
            frenzy,
            target,
            debuffs,
            others,
            gal,
            buff_stacks,
            rs_armed,
            opening_closed,
            r,
            ammo,
            kill_buff_mark,
            double_tap,
            arc,
            windows,
            weakpoint_pile,
            beam,
            field_duration_boost,
            fields,
            orbs,
            field_ctx,
            meter,
            strip_kills_seen,
            t,
            super_crit_armed,
            incarnon,
            ghost_pile,
            syndicate,
            crit_per_hit,
            sniper_combo,
            tendril,
            spool,
            melee,
            influence_until,
            last_shot_t,
        },
        Fixed {
            aim_off_axis,
            body_at,
            area_near,
            bounce_bodies,
            ricochet_layout,
            chain_layout,
            struck,
            base_struck,
            frame_seconds,
            rec_roster,
            rec_buff_index,
            main_variants,
            main_variant_rad,
            main_pre,
            base_pre,
            base_variants,
            base_variant_rad,
            status_damage,
            field_active,
            combo_spec,
        },
    )
}
