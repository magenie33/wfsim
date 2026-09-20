use super::*;

/// A BODY IN THE FORMATION THAT IS NOT BEING AIMED AT — its own state, and
/// nothing else.
///
/// WHERE THE LINE IS: a counter belongs to whoever it counts
/// on. The pools, the procs, the DoTs and the armour a hit strips are the
/// BODY's, so they are here; the buff bar, the Galvanized stacks, the arcane
/// runtime and the damage-instance number are the SHOOTER's and stay in the run
/// loop, shared by every body because one weapon is firing at all of them.
///
/// That split is also the shape a second TENNO slots into: the loop's own
/// player-side locals become one source's, a `Vec` of them, and nothing on this
/// side of the line has to change.
pub(super) struct SpreadFoe {
    pub(super) state: TargetState,
    pub(super) debuffs: DebuffState,
}

/// WHICH MECHANISM PUT AN INSTANCE ON A BODY — one of the five in
/// MECHANICS §12.
///
/// AN ENUM AND NOT A BOOL: `spread_from_seeds` also takes
/// `multishot_half: bool`, one argument away, in calls that look alike. Two
/// bools there are two things that can be handed to the wrong parameter and
/// still compile; an enum cannot be.
///
/// It decides exactly ONE thing, and that is the whole of why it exists: a
/// TENDRIL's own kill spawns no tendril, and every other kind of kill does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SpreadBy {
    Chain,
    Blast,
    /// A LINGERING CLOUD goes through `field_tick` rather than `spread_hit` —
    /// it is a tick of a thing that is already there, not an instance this shot
    /// produced — so nothing constructs this today. It is listed because the
    /// five mechanisms are five, and a missing arm would read as an oversight
    /// rather than as a routing fact.
    #[allow(dead_code)]
    Cloud,
    Tendril,
    Echo,
    /// …AND THE OTHER ONE THAT IS NOT A SPREAD: the same shot, ARRIVING AGAIN
    /// somewhere else. It is the only one that may land on a head.
    Ricochet,
    /// …AND THE ONE THAT IS NOT A SPREAD: the same shot, still travelling.
    /// It is here because it routes through `spread_hit` like the others, and
    /// it is told apart from them because it is the only one that may headshot.
    PunchThrough,
    /// …AND THE MELEE ONE: the same SWING, reaching past the first body.
    ///
    /// *"Each melee weapon has a Follow Through statistic that tells what
    /// proportion of damage is dealt to successive targets in a single melee
    /// strike"*, `FT^(n-1)` (wiki, Melee). It is punch through's opposite in
    /// every way that matters: nothing is SPENT, there is no budget to run out,
    /// the decay is geometric rather than distance-based, and it never
    /// headshots — so it is its own arm rather than a flag on that one.
    FollowThrough,
}

impl SpreadBy {
    /// WHICH METER ROW THIS DAMAGE IS, and the split is what the reader is
    /// being told: an EXPLOSION is the radial attack part however many bodies
    /// it reaches, while a chain hop, a punched body and a ricochet are the
    /// weapon's own hit ARRIVING SOMEWHERE ELSE — the same instance, not a
    /// second kind of damage.
    pub(super) fn meter_row(self) -> bool {
        matches!(self, SpreadBy::Blast | SpreadBy::Echo)
    }

    /// Does a kill by this mechanism spawn a tendril? Everything but a
    /// tendril's own hit — including a status a tendril applied, which kills
    /// through the DoT path rather than through this one.
    pub(super) fn spawns_a_tendril(self) -> bool {
        self != SpreadBy::Tendril
    }

    /// IS THIS THE SHOT ITSELF? Punch through is the same round still
    /// travelling, which is why a charged shot that finishes three bodies
    /// leaves three ghosts. Everything else here is a second thing it produced.
    pub(super) fn is_the_shot_itself(self) -> bool {
        self == SpreadBy::PunchThrough
    }
}

/// CAN MELEE INFLUENCE CARRY THIS STATUS?
///
/// The wiki lists both halves, and they partition the elements exactly: the
/// four primaries (*"Cold, Electricity, Heat, Toxin"*) and the six combinations
/// (*"Blast, Corrosive, Gas, Magnetic, Radiation, Viral"*) spread; *"Physical
/// (Impact, Puncture, Slash), Void, Tau, Knockdown, Stagger, Ragdoll, Lifted,
/// and Microwave"* do not.
///
/// WRITTEN AS THE PAGE'S OWN LIST rather than as "not physical", because the
/// exclusions include types this engine has (`Void`, `Tau`) and states it
/// tracks outside the proc list — so a negative test would let the next
/// `DamageType` in silently.
pub(super) fn influence_can_spread(ty: DamageType) -> bool {
    matches!(
        ty,
        DamageType::Cold
            | DamageType::Electricity
            | DamageType::Heat
            | DamageType::Toxin
            | DamageType::Blast
            | DamageType::Corrosive
            | DamageType::Gas
            | DamageType::Magnetic
            | DamageType::Radiation
            | DamageType::Viral
    )
}

/// MELEE INFLUENCE — one swing's statuses arriving on the whole room.
///
/// Every spreadable status the swing applied lands again on each body within
/// the arcane's radius of the one that took it, dealt *"damage equal to that
/// element's damage from the original attack"*. The epicentre is a BODY:
/// *"spread radius extends from the position of the enemy hit"*.
///
/// AN EXTRA HIT PER BODY, with two clauses of its own: its Condition Overload
/// is the STRUCK body's rather than the receiver's, and its status burns off
/// the swing's own base rather than this instance's — which is why it is not a
/// `SpreadBy` arm. Both are the page's, and both are argued in
/// docs/EXTRA_HIT.md §"…and the one that is not a percentage".
#[allow(clippy::too_many_arguments)]
pub(super) fn spread_from_influence(
    // WHERE EVERY BODY STANDS, built once for the fight rather than per landed
    // hit — Influence spreads 20 m across a 361-body formation, so that was the
    // allocation this engine could least afford.
    bodies: &[crate::rules::space::Vec2],
    others: &mut [SpreadFoe],
    target: &mut TargetState,
    debuffs: &mut DebuffState,
    params: &FightParams,
    ap: &FightParams,
    // WHICH BODY IT SPREADS FROM — 0 is the aimed one, `i + 1` is `others[i]`,
    // the numbering `RunResult::damage_by_body` uses.
    from: usize,
    // WHAT THAT BODY TOOK, per spreadable element: the element's share of the
    // instance's damage, with the struck body's own Condition Overload and
    // crit already inside it.
    landed: &[(DamageType, f64)],
    // The scale the STRUCK body's own statuses were settled at.
    scale: InstanceScale,
    radius_m: f64,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
) {
    if landed.is_empty() || radius_m <= 0.0 {
        return;
    }
    let Some(&epicentre) = bodies.get(from) else { return };
    // ONE MORE RUNG THAN THE HIT ALREADY CARRIES. `raw` came out of an
    // instance that was multiplied by `f` once, so this takes it to `f^2` —
    // `faction_at(f, DEPTH_PROC) / faction_at(f, DEPTH_HIT)`, said as the one
    // factor that is missing.
    let f = params.faction_at_time(t);
    let status_damage = params.status_duration_multiplier;
    // THE BODY THAT WAS STRUCK TAKES IT TOO, and the epicentre is zero metres
    // from itself so it is always in range.
    //
    // The arcane is one instance dealt to everything within the radius, and the
    // one that was hit is inside it — the wiki's example counts "every other
    // enemy" because that is what its sentence is about, not because the host
    // is excluded. Measured by the owner: the number lands on the host as well,
    // and it force-procs there like everywhere else.
    for (b, at) in bodies.iter().copied().enumerate() {
        // ANY PART OF A BODY TOUCHING IS ENOUGH — the rule every sphere in
        // this engine uses.
        if !crate::rules::space::caught_by_blast(epicentre.distance(at), radius_m) {
            continue;
        }
        let (state, dbf, tparams) = match b.checked_sub(1) {
            None => (&mut *target, &mut *debuffs, &params.target),
            Some(i) => match (others.get_mut(i), params.others.get(i)) {
                (Some(foe), Some(spec)) => (&mut foe.state, &mut foe.debuffs, &spec.params),
                _ => continue,
            },
        };
        for &(ty, elem_raw) in landed {
            let raw = elem_raw * f;
            if raw <= 0.0 {
                continue;
            }
            let mit = dbf.mitigation(
                t,
                status_damage,
                params.armor_strip_per_puncture,
                params.squad.enemy_armor_multiplier,
            );
            let mut breakdown = Breakdown::default();
            let settled = state.apply(
                raw,
                TypeShares::single(ty),
                // A SPREAD LANDS ON A BODY. Melee puts nothing on a head
                // anywhere, and this is further from one than a swing is.
                false,
                t,
                tparams,
                false,
                &mit,
                1.0,
                watching(rec, &mut breakdown),
            );
            let (eff, killed) = (settled.effective, settled.killed);
            r.sources.extra_hit += eff;
            r.sources.extra_hit_by_type[ty as usize] += eff;
            ledger::settle(
                r, rec, t, b, ty, PopKind::Extra, &breakdown, settled, Some(dbf),
                ledger::Clock::Hit,
                || Instance {
                    origin: crate::record::Origin::Influence,
                    // THE STRUCK BODY'S OWN NUMBER IN THIS ELEMENT, which is
                    // what the arcane copies — its Condition Overload included,
                    // and this body's excluded.
                    base: elem_raw,
                    layers: mul_layers(elem_raw, &faction_layers(params, t, DEPTH_HIT)),
                    head: false,
                    ..Instance::default()
                },
            );
            // `at` IS THIS BODY'S PLACE — the loop is over every body the
            // sphere caught, not over the aimed one.
            r.note_kills(u32::from(killed), t, params.drop_is_in_reach(at));
            if killed {
                // A SPREAD KILL IS THE WEAPON'S KILL. The host is the melee
                // weapon that swung: Influence copies its number into the
                // neighbours, so a body that dies to the copy died to that
                // swing. Every on-kill grant fires — the same rule the gas
                // cloud states (MEASUREMENTS M70), for the same reason, and it
                // was counted here without firing anything.
                gal.bump_on_kill(params, t);
                arc.on_kill(params, t);
                continue;
            }
            // …AND THE STATUS ITSELF, which is the half the card is named for.
            // Forced: the arcane does not roll, it copies what landed.
            let forced = std::slice::from_ref(&ty);
            let procs = crate::rules::status::procs_for_hit(
                forced,
                0.0,
                &DamageVector::new().with(ty, raw),
                &tparams.status_immunities,
                &mut d.status,
            );
            settle_procs(
                procs,
                t,
                scale,
                dbf,
                gal,
                arc,
                state,
                params,
                ap,
                &mit,
                r,
                rec,
                &mut d.status,
                tparams,
                // ONE DERIVATION PAST THE HIT'S OWN STATUS, which is the whole
                // of *"thrice on damaging status procs caused by it"*.
                DEPTH_DERIVED_PROC,
            );
        }
    }
}

/// ONE CHAIN OR SPLASH INSTANCE LANDING ON A BODY OTHER THAN THE AIMED ONE.
///
/// It is the SAME hit the aimed body took, scaled — a chain hop is *"a beam
/// with a smaller base damage"* — so nothing here re-derives a damage rule.
/// What it does re-derive belongs to the RECEIVING body:
///
///   · ITS OWN CONDITION OVERLOAD. The bucket is one multiplicative factor of
///     `raw` and `gunco_bucket` takes a debuff state, so
///     `raw x share x bucket_here / bucket_there` is EXACT. A body carrying
///     four status types takes more from the same chain than a clean one.
///   · ITS OWN HALF-HEALTH TERM, off its own health line.
///   · ITS OWN MITIGATION — armour, shields, overguard, and whatever this
///     body's own procs have stripped.
///   · ITS OWN PROCS AND ITS OWN DEATH.
///
/// NEVER A HEADSHOT. `rules::chain::Instance::headshot` is true for the directly
/// struck body alone, so `part_factor` is 1.0 here and the hit lands on the
/// body — which is the clause that reorders builds in a crowd (MECHANICS §12).
#[allow(clippy::too_many_arguments)]
pub(super) fn spread_hit(
    inst: &crate::rules::chain::Instance,
    foe: &mut SpreadFoe,
    spec: &crate::formation::FoeSpec,
    // The aimed hit with its OWN CO bucket divided back out, so this body can
    // multiply its own in.
    raw_per_bucket: f64,
    shares: TypeShares,
    crit_multiplier: f64,
    crit_tier: u32,
    attrition: f64,
    modded_base: f64,
    status_chance: f64,
    forced: &[DamageType],
    vector: &DamageVector,
    params: &FightParams,
    ap: &FightParams,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
    // WHICH MECHANISM PUT IT HERE — see [`SpreadBy`]. The only thing it
    // changes is which kills spawn another tendril.
    by: SpreadBy,
) -> Landed {
    let base_damage = ap.base_damage_bonus;
    let arcane_base_damage =
        arc.total(&params.arcane.buffs, ArcGrant::BaseDamage, t) + heavy_attack_base_damage(ap) + arc.rage_bonus(t);
    let arc_ratio = (1.0 + base_damage + arcane_base_damage) / (1.0 + base_damage);
    let half_hp = if spec.params.max_health() > 0.0
        && foe.state.health < 0.5 * spec.params.max_health()
    {
        ap.base_damage_below_half_health
    } else {
        0.0
    };
    let bucket = gunco_bucket(
        params,
        ap,
        &mut foe.debuffs,
        gal,
        t,
        base_damage,
        arcane_base_damage,
        arc_ratio,
        half_hp,
        ap.co_base,
        crate::model::CoStage::Direct,
    );
    // THE PART FACTOR IS A SEPARATE MULTIPLIER FROM THE SHARE, and the two are
    // not interchangeable: `share` scales the hit AND the modded base its DoTs
    // are computed from, while a head multiplier scales the hit alone. 1.0 for
    // every mechanism but the ricochet, so this changes nothing anywhere else.
    let raw = raw_per_bucket * bucket.bucket * inst.share * inst.part_factor;
    if raw <= 0.0 {
        return Landed::default();
    }
    let status_damage = params.status_duration_multiplier;
    let mit = foe.debuffs.mitigation(t, status_damage, params.armor_strip_per_puncture, params.squad.enemy_armor_multiplier);
    // BEFORE `apply`, like the aimed path: breaking overguard changes which
    // column the next read returns, and the split belongs to the hit that
    // broke it rather than to the state it left behind.
    let col = foe.state.incoming_column(&spec.params);
    let mut breakdown = Breakdown::default();
    let settled = foe.state.apply(
        raw,
        shares,
        // THE SHIELD GATE'S QUESTION, and the only thing this bool decides: a
        // headshot goes through a gated shield.
        //
        // TWO MECHANISMS SET IT, and for opposite reasons. A RICOCHET arrives
        // at a new angle, so it rolls its own. PUNCH THROUGH is the same round
        // still flying in a straight line at one height, so it CARRIES the
        // aimed pellet's answer — a round that entered a head enters the head
        // of whatever is behind it, and a body shot stays a body shot all the
        // way down the line. Reading `true` unconditionally would punch a body
        // shot THROUGH a shield gate.
        inst.headshot,
        t,
        &spec.params,
        false,
        &mit,
        1.0,
        watching(rec, &mut breakdown),
    );
    let (eff, killed, _broke) = (settled.effective, settled.killed, settled.broken);
    // A SPREAD INSTANCE IS A WHOLE VECTOR, so the number reads as its biggest
    // component — see `DamageVector::dominant`. Computed only while tracing.
    ledger::settle(
        r, rec, t, inst.target, shares.dominant(),
        // WHAT THE GAME DRAWS THIS AS, and the crit half of it is carried the
        // way the headshot is: the crit multiplier is already inside
        // `raw_per_bucket`, so a body a shot punched through takes the crit and
        // was POPPING A WHITE NUMBER for it. The record is the one output that
        // can be laid beside a recording, and it was drawing an orange hit as a
        // plain one on every body but the aimed one.
        match (inst.headshot, crit_tier > 0) {
            (true, true) => PopKind::HeadCrit,
            (true, false) => PopKind::Head,
            (false, true) => PopKind::Crit,
            (false, false) => PopKind::Direct,
        },
        &breakdown, settled, Some(&foe.debuffs),
        ledger::Clock::Hit,
        || Instance {
            // WHY THIS BODY GOT HIT AT ALL — the one column a formation
            // fight cannot be read without. `SpreadBy` already names the
            // five mechanisms; this is the same list said to a reader.
            origin: match by {
                SpreadBy::Chain => crate::record::Origin::Chain,
                SpreadBy::Blast => crate::record::Origin::Splash,
                SpreadBy::Cloud => crate::record::Origin::Field,
                SpreadBy::Tendril => crate::record::Origin::Chain,
                SpreadBy::Echo => crate::record::Origin::Echo,
                SpreadBy::Ricochet => crate::record::Origin::Ricochet,
                SpreadBy::PunchThrough => crate::record::Origin::PunchThrough,
                SpreadBy::FollowThrough => crate::record::Origin::FollowThrough,
            },
            // THE AIMED PELLET'S OWN NUMBER with its Condition Overload
            // bucket divided back out, so this body multiplies in its own —
            // which is why a neighbour's row can read a different CO term
            // from the body that was aimed at.
            base: raw_per_bucket,
            layers: mul_layers(raw_per_bucket, &[
                (crate::record::Factor::ConditionOverload, bucket.bucket),
                (crate::record::Factor::HopFalloff, inst.share),
                (crate::record::Factor::BodyPart, inst.part_factor),
            ]),
            head: inst.headshot,
            // …AND THE TIER, so the row states the multiplier it was built
            // with rather than leaving a reader to infer it from the size.
            crit_tier,
            crit_damage: crit_multiplier,
            ..Instance::default()
        },
    );
    // …AND IT REACHES THE DAMAGE METER.
    //
    // `effective_damage`, the score and the DPS count every body; the METER is
    // written on the aimed body's path (`r.sources.direct`/`.radial` at the end
    // of the pellet loop) and needs its own arm here. Without one a four-body
    // fight reports 5304 of damage by source against 15980 actually dealt, and
    // a reader comparing the headline with the breakdown correctly concludes
    // that one of them is made up.
    //
    // THE CROWD IS NOT ITS OWN ROW. An explosion that reaches nine bodies is
    // the radial attack part nine times, not a tenth kind of damage — the
    // meter answers "what hurt them", and a per-body split is the ROLL CALL's
    // question, which has its own panel.
    if by.meter_row() {
        r.sources.radial += eff;
        add_by_type(&mut r.sources.radial_by_type, vector, eff, &col);
    } else {
        r.sources.direct += eff;
        add_by_type(&mut r.sources.direct_by_type, vector, eff, &col);
    }
    if killed && by.is_the_shot_itself() && leaves_one(ap, params.player_at, spec.at) {
        r.ghost_kills += 1;
    }
    if by.spawns_a_tendril() {
        r.note_kills(u32::from(killed), t, params.drop_is_in_reach(foe.state.at));
    } else {
        r.note_tendril_kills(u32::from(killed), t, params.drop_is_in_reach(foe.state.at));
    }
    // …AND IT IS STILL THE WEAPON'S KILL, whichever mechanism carried it there.
    // A chain, a blast and a spread all deal the weapon's own number to a
    // second body, so a kill by one is a kill by the weapon and every on-kill
    // grant fires. This counted them and fired nothing.
    if killed {
        gal.bump_on_kill(params, t);
        arc.on_kill(params, t);
        // …AND THE BODY THAT STANDS BACK UP IS A NEW INDIVIDUAL, which the
        // aimed path has said since the engine had statuses and this one did
        // not say at all. `apply` respawns it with full pools; leaving the
        // pile standing gave the formation an enemy at full health wearing six
        // seconds of somebody else's Viral. On an instant-respawn ruler that
        // is most of the fight: every body but the aimed one snowballed, and
        // the aimed one — the only one obeying the rule — read as the weak one.
        //
        // NO ACID SHELLS HERE. The radial path fires them under its own
        // once-per-corpse guard (`exploded`); this path has none, so passing
        // them would detonate a Sobek's corpse once per spread instance. It
        // does not fire them today and this does not start.
        foe.debuffs.on_death(None, &spec.params);
        // A FRESH INDIVIDUAL TAKES NO STATUS FROM THE HIT THAT KILLED THE LAST
        // ONE — the aimed path returns here for the same reason.
        return Landed { procs: Vec::new(), raw, killed };
    }

    // …AND ITS OWN STATUS ROLL, at FULL chance. The share scales the damage and
    // nothing else, so a hop dealing 24% of the hit still
    // rolls the whole status chance — which is why a chain is worth more in
    // procs than it is in damage.
    arc.next_instance();
    let procs = crate::rules::status::procs_for_hit(
        forced,
        status_chance,
        vector,
        &spec.params.status_immunities,
        &mut d.status,
    );
    // WHAT THIS INSTANCE LEFT, for Melee Influence — see [`Landed`]. Taken
    // before `settle_procs` consumes the list.
    let landed = Landed { procs: procs.clone(), raw, killed };
    settle_procs(
        procs,
        t,
        InstanceScale {
            // THE HEAD FACTOR IS NOT IN HERE, which is the point of keeping it
            // off `share`: a Slash bleed off a headshot is the same size as one
            // off a bodyshot, because a status effect reads the modded base.
            mb_live: modded_base * arc_ratio * inst.share,
            crit_multiplier,
            // …AND IT IS IN HERE, because Heat and Blast are computed as a
            // fraction of the HIT rather than of the base — the same two the
            // direct path multiplies by its own part factor.
            //
            // `status_part_factor` AND NOT `part_factor`: they are the same
            // number for every mechanism but punch through, which carries the
            // head's multiplier inside the raw it was handed and therefore
            // reads 1.0 for the hit. Reading that 1.0 here burned a punched
            // body for a third of what the aimed body took from the same
            // round, on a ruler whose own rule is that a punched body IS a
            // weak-point hit.
            part_factor: inst.status_part_factor,
            landing: inst.status_landing,
            attrition,
            // THE FIRING FORM'S bracket, like any other instance of this shot —
            // a chain hop is the same shot, and the Extra Hit it may set off is
            // the same weapon's.
            xh_bracket: ap.extra_hit_bracket(t),
        },
        &mut foe.debuffs,
        gal,
        arc,
        &mut foe.state,
        params,
        ap,
        &mit,
        r,
        rec,
        &mut d.status,
        &spec.params,
        DEPTH_PROC,
    );
    landed
}

/// ONE SHOT'S FACTORS, carried out of the pellet loop so the radius-caught
/// seeds' chains can fire once for the shot rather than once per pellet.
///
/// TAKEN FROM THE FIRST LANDING PELLET, and that is a simplification with one
/// moving part in it: crit is rolled per pellet, so a shot whose pellets
/// critted differently has no single crit multiplier and this takes the first
/// one's. Everything else in here is identical across a shot's pellets. The
/// alternative — rolling a fresh crit for the spread — would be inventing an
/// instance the game does not describe, so the shot it actually belongs to is
/// the honest source.
///
/// …EXCEPT WHERE IT DOES DESCRIBE ONE, and one member's page does: Secondary
/// Irradiate's notes say its spread *"will roll a separate critical hit chance
/// per enemy"* (re-read 2026-08-21). Inheriting is still what happens, so every
/// body in that radius crits together or not at all. The MEAN is unaffected
/// either way; the CORRELATION is not, and a kill count reads correlation where
/// a damage total does not. Recorded rather than changed, because re-rolling
/// moves the seeded draw stream and therefore every golden value after it —
/// which is the owner's call and needs a measurement, not a wiki line.
pub(super) struct SpreadShot {
    pub(super) raw_per_bucket: f64,
    pub(super) shares: TypeShares,
    pub(super) crit_multiplier: f64,
    pub(super) crit_tier: u32,
    pub(super) attrition: f64,
    pub(super) modded_base: f64,
    pub(super) status_chance: f64,
    pub(super) forced: Vec<DamageType>,
    pub(super) vector: DamageVector,
}

/// PUNCH THROUGH — the sixth way a shot reaches a body, and the only one that
/// is not a spread at all: it is the SAME shot, still travelling.
///
/// *"The total distance of material (object or enemy) that a weapon's
/// projectile, bullet or beam can pass through before dissipating"* — so a body
/// behind the aimed one takes the shot itself, at FULL damage: the page names
/// no attenuation per body and the engine invents none. What the budget buys is
/// HOW MANY (`rules::space::struck_along`, priced by `rules::space::BODY_MATERIAL_M`).
///
/// A DIRECT HIT IN EVERY SENSE: it may HEADSHOT and it is per PELLET, the
/// opposite of a chain hop. ONLY FOR A WEAPON WITH NO BEAM — where there is
/// one, `spread_from_seeds` already emits these bodies as its seeds.
/// EVERY BODY A MELEE SWING REACHED PAST THE FIRST, at `FT^(n-1)`.
///
/// `struck` is `melee_struck`'s answer, nearest first with the aimed body at
/// index 0, so `n` is the position in it. NEVER A HEADSHOT: melee here puts
/// nothing on a head (`Capability::AimsAtHead`). A ZERO follow through gives
/// the aimed body alone.
#[allow(clippy::too_many_arguments)]
pub(super) fn spread_from_follow_through(
    others: &mut [SpreadFoe],
    params: &FightParams,
    ap: &FightParams,
    struck: &[usize],
    // WHAT EACH BODY TOOK, appended. Melee Influence spreads from every body a
    // SWING struck, and these are the ones the aimed path never sees.
    seeds: &mut Vec<(usize, Landed)>,
    follow_through: f64,
    raw_per_bucket: f64,
    shares: TypeShares,
    crit_multiplier: f64,
    crit_tier: u32,
    attrition: f64,
    modded_base: f64,
    status_chance: f64,
    forced: &[DamageType],
    vector: &DamageVector,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
) {
    for (n, &sidx) in struck.iter().enumerate().skip(1) {
        let share = follow_through.powi(n as i32);
        if share <= 0.0 {
            break;
        }
        let Some(idx) = sidx.checked_sub(1) else { continue };
        let Some(fs) = params.others.get(idx) else { continue };
        let inst = crate::rules::chain::Instance {
            target: sidx,
            share,
            multishot: true,
            headshot: false,
            part_factor: 1.0,
            status_part_factor: 1.0,
            status_landing: 1.0,
        };
        let foe = &mut others[idx];
        let landed = spread_hit(
            &inst, foe, fs, raw_per_bucket, shares, crit_multiplier, crit_tier, attrition,
            modded_base, status_chance, forced, vector, params, ap, gal, arc, r, rec, d, t,
            SpreadBy::FollowThrough,
        );
        seeds.push((sidx, landed));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spread_from_punch_through(
    others: &mut [SpreadFoe],
    params: &FightParams,
    ap: &FightParams,
    struck: &[usize],
    raw_per_bucket: f64,
    shares: TypeShares,
    crit_multiplier: f64,
    crit_tier: u32,
    attrition: f64,
    modded_base: f64,
    status_chance: f64,
    forced: &[DamageType],
    vector: &DamageVector,
    // WHETHER THE AIMED PELLET FOUND A HEAD, and WHAT THE PART WAS WORTH.
    // Both carried rather than re-rolled: the same round flying in a straight
    // line enters the same part of whatever is behind. The multiplier is
    // already inside `raw_per_bucket` for the HIT and is needed all the same
    // for the STATUSES — see `rules::chain::Instance::status_part_factor`.
    head_direct: bool,
    head_part_factor: f64,
    // …and what a tick its statuses leave is worth there (`Dot::landing`).
    head_status_landing: f64,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
) {
    // The FIRST is the body the rest of the engagement is already scored
    // against; everything behind it is what this function is for.
    for &s in struck.iter().skip(1) {
        let Some(idx) = s.checked_sub(1) else { continue };
        let Some(fs) = params.others.get(idx) else { continue };
        // …AND IT HAS TO BE INSIDE THE WEAPON'S RANGE. A range is a WALL, not a
        // ramp — the aimed path has said so since ranges were modelled, and
        // this path never asked at all: the gate was computed once off the
        // AIMED body's gap and every body behind it was punched through
        // whatever distance it stood at. On the group-clear ruler that is a
        // Phantasma Prime's 25 m beam reaching all nineteen ranks, the last of
        // them 54.4 m away — and it is the whole of why a beam-range card was
        // worth nothing in a crowd.
        //
        // BREAK, NOT CONTINUE: `struck_bodies` is in the order the ray meets
        // them, so the first one out of reach is the end of the line.
        let gap_here = (params.range_to(fs.at) - crate::rules::space::BODY_RADIUS_M).max(0.0);
        if gap_here > ap.range_m {
            break;
        }
        // ITS OWN DAMAGE FALLOFF, because it is FURTHER. The page names no
        // attenuation per body crossed, but a shot that keeps going keeps
        // flying — and the direct hit reads the GAP it flew (`falloff.factor`).
        // The aimed body's factor is already inside `raw_per_bucket`, so it is
        // divided back out and this body's own put in, which is the same
        // arithmetic the blast does with its epicentre's.
        //
        // 1.0 for the whole roster minus nineteen entries, and 1.0 at contact
        // for all of them — so this moves nothing on a weapon that lists no
        // falloff, and nothing on the first body of any fight.
        let ratio = match ap.falloff {
            None => 1.0,
            Some(f) => {
                let here = f.factor(gap_here);
                let there = f.factor(params.gap());
                if there > 0.0 { here / there } else { 0.0 }
            }
        };
        if ratio <= 0.0 {
            continue;
        }
        let inst = crate::rules::chain::Instance {
            target: s,
            // THE WHOLE SHOT, undiminished — except by the distance it flew.
            share: ratio,
            multishot: true,
            // A HEADSHOT IS CARRIED, NOT RE-ROLLED, and this is the one spread
            // where that is right. Punch through is the
            // SAME round still flying in a STRAIGHT LINE: this plane holds it
            // at one height, so a round that entered a head enters the head of
            // whatever is behind, and one that entered a body enters bodies.
            // A BOUNCE is the opposite — it leaves at a new angle, so where it
            // lands next is independent and gets its own roll. Conflating the
            // two is what this fixes.
            //
            // It read `headshot: true, part_factor: 1.0`, which was wrong in
            // both directions at once: every punched body counted as a headshot
            // for the conditions that read one even when the shot was a body
            // hit, and none of them took the head's MULTIPLIER when it was.
            headshot: head_direct,
            // ONE, AND THE MULTIPLIER IS STILL CARRIED. `raw` is built as
            // `qtotal * part_factor * ...`, and punch through is handed
            // `raw / bucket` rather than `body_only(raw / bucket)` — so the
            // aimed pellet's head factor is ALREADY inside it, which is the
            // propagation itself. Setting it here as well multiplies it twice;
            // that was tried on 2026-08-21 and caught by the test below refusing
            // to fail on the old code.
            part_factor: 1.0,
            // …AND THE STATUSES NEED IT ANYWAY. Heat and Blast read the hit's
            // weak point and are built from the modded base, which carries no
            // head factor — so the one place the hit must not see it again is
            // the one place its payloads must. See `rules::chain::Instance`.
            status_part_factor: head_part_factor,
            status_landing: head_status_landing,
        };
        let foe = &mut others[idx];
        spread_hit(
            &inst,
            foe,
            fs,
            raw_per_bucket,
            shares,
            crit_multiplier,
            crit_tier,
            attrition,
            modded_base,
            status_chance,
            forced,
            vector,
            params,
            ap,
            gal,
            arc,
            r,
            rec,
            d,
            t,
            SpreadBy::PunchThrough,
        );
    }
}

/// A RICOCHET — the projectile arriving again, at a body it has not hit yet.
///
/// The SEVENTH way a shot reaches a body, and the only one that is a second
/// arrival of the whole shot rather than a share of it: *"a traveling
/// projectile that can ricochet off enemies and terrain, exploding up to 6
/// times with a 4 meter radius, dealing damage once for any collision on
/// enemies, and again for the explosion"*.
///
/// TWO INSTANCES PER BOUNCE — this fires the COLLISION, the explosion rides the
/// radial part and walks the same path (`bounces`) — IN FULL, since the page
/// names no attenuation per bounce. AND IT MAY HEADSHOT, at a flat chance the
/// data states, alone among the spreads: a ricochet IS the shot and is not
/// aimed either, so `headshot_pct` is the wrong number for it.
///
/// It does NOT bounce off terrain (this arena has no walls) and does not return
/// to a body it has hit, which is the chain path's rule.
#[allow(clippy::too_many_arguments)]
pub(super) fn spread_from_ricochet(
    others: &mut [SpreadFoe],
    params: &FightParams,
    ap: &FightParams,
    // Where the projectile went and whether each arrival found a head, decided
    // ONCE for the pellet so the collision and the explosion agree.
    path: &[(usize, bool)],
    raw_per_bucket: f64,
    shares: TypeShares,
    crit_multiplier: f64,
    crit_tier: u32,
    attrition: f64,
    modded_base: f64,
    status_chance: f64,
    forced: &[DamageType],
    vector: &DamageVector,
    head_factor: &dyn Fn(&crate::formation::FoeSpec) -> f64,
    // What an Electricity or Gas tick is worth on a head (`Dot::landing`).
    head_landing: f64,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
) {
    for &(body, head) in path {
        // THE AIMED BODY IS NEVER A BOUNCE TARGET — it is where the projectile
        // came from, and `bounce_path` has it visited before the walk starts.
        let Some(idx) = body.checked_sub(1) else { continue };
        let Some(fs) = params.others.get(idx) else { continue };
        let inst = crate::rules::chain::Instance {
            target: body,
            // THE WHOLE COLLISION, undiminished.
            share: 1.0,
            // NO MULTISHOT. Multishot is pellets, and each pellet is its own
            // projectile with its own bounces — this is called once per landing
            // pellet, so the count is already there.
            multishot: false,
            headshot: head,
            part_factor: if head { head_factor(fs) } else { 1.0 },
            // A RICOCHET ROLLS ITS OWN, so the hit and its statuses read the
            // same answer — `raw_per_bucket` is handed to it body-only.
            status_part_factor: if head { head_factor(fs) } else { 1.0 },
            status_landing: if head { head_landing } else { 1.0 },
        };
        spread_hit(
            &inst,
            &mut others[idx],
            fs,
            raw_per_bucket,
            shares,
            crit_multiplier,
            crit_tier,
            attrition,
            modded_base,
            status_chance,
            forced,
            vector,
            params,
            ap,
            gal,
            arc,
            r,
            rec,
            d,
            t,
            SpreadBy::Ricochet,
        );
    }
}

/// AN ECHO — a fraction of THIS hit dealt to every other body near it.
///
/// Secondary Irradiate is the only member: *"deal X% of the hit damage to
/// enemies within Xm"*, 80% to 180% of it over a 4.5 m to 7 m sphere. It sat in
/// `data::arcanes` with the TEAM buffs — "no sim payload" — and that was right
/// while the arena held one body, because an echo needs somebody to echo to.
///
/// PER DIRECT HIT, which is per landing PELLET: the arcane's own file says
/// *"multishot pellets each trigger; beams/AoE don't spread"*. So it is called
/// from inside the pellet loop and only on the direct stage.
///
/// A BODY HIT, never a head, and it *"rolls its own crits"* — which this shares
/// with every other spread here and which is the one simplification they all
/// carry (see `spread_hit`).
///
/// THE BODY THAT WAS HIT TAKES NOTHING EXTRA: the echo is to *"enemies within
/// Xm"* of it, not to it.
#[allow(clippy::too_many_arguments)]
pub(super) fn spread_from_echo(
    others: &mut [SpreadFoe],
    // STACKS ON THE BODY THAT WAS HIT, read at the moment of the hit — the
    // arcane's gate is about the target, not about the shooter.
    hit_radiation_stacks: usize,
    params: &FightParams,
    ap: &FightParams,
    raw_per_bucket: f64,
    shares: TypeShares,
    crit_multiplier: f64,
    crit_tier: u32,
    attrition: f64,
    modded_base: f64,
    status_chance: f64,
    forced: &[DamageType],
    vector: &DamageVector,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
) {
    // THE ARCANE'S SHARE, TIMES THE WEAPON'S OWN COEFFICIENT — see
    // `data::weapons::WeaponSpec::echo_multiplier`. It is 1.0 for every entry in
    // the roster but the Laetum's Incarnon form, where the echo was MEASURED at
    // 3.6x a 1.8x arcane and nobody knows why.
    let share = params.arcane.echo_share * params.echo_multiplier;
    let radius = params.arcane.echo_radius_m;
    if share <= 0.0 || radius <= 0.0 {
        return;
    }
    // THE CARD'S OWN PRICE: *"On hitting enemies afflicted by 10 stacks of
    // Radiation"*. The hit body has to be WEARING them, and 10 is the cap, so
    // this is not a rider — it is most of the arcane.
    //
    // It fired unconditionally until 2026-08-18, on a target with no Radiation
    // on it at all, because the loader read `trigger`/`grants` and the per-rank
    // values and never `condition:` (a player reported it). Radiation's stacks
    // have been tracked as `confusion` since the engine had statuses, so this
    // was never a thing the sim could not know.
    if hit_radiation_stacks < params.arcane.echo_needs_radiation_stacks as usize {
        return;
    }
    // FROM THE BODY THAT WAS HIT, at the surface the round met — the same
    // epicentre every other sphere in this engine uses.
    let at = crate::rules::space::detonation_point(params.target_at, params.player_at);
    for (i, spec) in params.others.iter().enumerate() {
        if !crate::rules::space::caught_by_blast(spec.at.distance(at), radius) {
            continue;
        }
        let inst = crate::rules::chain::Instance {
            target: i + 1,
            share,
            multishot: false,
            headshot: false,
            part_factor: 1.0,
            status_part_factor: 1.0,
            status_landing: 1.0,
        };
        let (foe, fs) = (&mut others[i], &params.others[i]);
        spread_hit(
            &inst,
            foe,
            fs,
            raw_per_bucket,
            shares,
            crit_multiplier,
            crit_tier,
            attrition,
            modded_base,
            status_chance,
            forced,
            vector,
            params,
            ap,
            gal,
            arc,
            r,
            rec,
            d,
            t,
            SpreadBy::Echo,
        );
    }
}

/// TENDRILS — the fourth way a shot reaches a body, and the only one that is
/// not a spread at all: they are EXTRA BEAMS.
///
/// The Ocucor grows one per kill up to four, and the COUNT is modelled because
/// a mod reads it (Sentient Surge). Against ONE target their damage is worth
/// nothing: *"Tendrils homing in on the main beam's target are only COSMETIC,
/// and don't deal any additional damage or status effects"*, and every tendril
/// homes on the one body.
///
/// A FORMATION IS WHERE THEY BECOME REAL. Each locks a body that is NOT the
/// beam's, and *"their base damage equals the primary beam's, and they roll
/// their own crits and status"* — a full instance each. WHAT PICKS THE BODY:
/// *"home-in on enemies close to the targeting reticle"*, within `acquire_deg`
/// and `range_m`, nearest by ANGLE rather than distance, and one body per
/// tendril — a tendril locking an already-locked body is the cosmetic case
/// again.
///
/// NOT PER PELLET. A tendril is its own beam, so it fires once for the shot
/// however many pellets the main beam put out.
#[allow(clippy::too_many_arguments)]
pub(super) fn spread_from_tendrils(
    others: &mut [SpreadFoe],
    params: &FightParams,
    ap: &FightParams,
    live: u32,
    raw_per_bucket: f64,
    shares: TypeShares,
    crit_multiplier: f64,
    crit_tier: u32,
    attrition: f64,
    modded_base: f64,
    status_chance: f64,
    forced: &[DamageType],
    vector: &DamageVector,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
) {
    if live == 0 || params.tendril_max == 0 {
        return;
    }
    let aim = params.aim_at.unwrap_or(params.target_at);
    // EVERY BODY A TENDRIL COULD TAKE, nearest to the RETICLE first —
    // `rules::chain::acquired`, which is also how the Boar Incarnon's three beams pick
    // theirs. Two weapons, two pages saying the same thing, one rule.
    let bodies: Vec<crate::rules::space::Vec2> =
        std::iter::once(params.target_at).chain(params.others.iter().map(|f| f.at)).collect();
    let cand = crate::rules::chain::acquired(
        &bodies,
        params.player_at,
        aim,
        params.tendril_acquire_deg,
        params.tendril_range_m,
    );

    // …AND THE BEAM'S OWN TARGET IS NOT ONE OF THEM: "Tendrils homing in on the
    // main beam's target are only COSMETIC, and don't deal any additional
    // damage or status effects". Index 0 is that body.
    for i in cand.into_iter().filter(|&i| i != 0).map(|i| i as usize - 1).take(live as usize) {
        let inst = crate::rules::chain::Instance {
            target: i + 1,
            // A WHOLE BEAM, not a share of one.
            share: 1.0,
            // Its own beam, so the main one's multishot is not its.
            multishot: false,
            // …and it lands on a body, never a head.
            headshot: false,
            part_factor: 1.0,
            status_part_factor: 1.0,
            status_landing: 1.0,
        };
        let (foe, fs) = (&mut others[i], &params.others[i]);
        spread_hit(
            &inst,
            foe,
            fs,
            raw_per_bucket,
            shares,
            crit_multiplier,
            crit_tier,
            attrition,
            modded_base,
            status_chance,
            forced,
            vector,
            params,
            ap,
            gal,
            arc,
            r,
            rec,
            d,
            t,
            SpreadBy::Tendril,
        );
    }
}

/// AN EXPLOSION REACHES EVERY BODY IT TOUCHES, not only the one it went off on.
///
/// The chain's sibling and the simpler of the two: a blast has no path and no
/// hops, so every body the sphere catches takes one instance at its own
/// falloff. THE THREE BLAST RULES DECIDE ALL OF IT (`engine::space`): it goes
/// off on the aimed body's SURFACE, any body touching the sphere is caught, and
/// each one's falloff reads its own NEAREST point.
///
/// NO HEADSHOT AND NO MULTISHOT for anything but the body the pellet struck —
/// an explosion lands on a body rather than a head, and a radial takes
/// multishot only where its own spec says so.
///
/// THE EPICENTRE IS ON THE AIM LINE, exact for a pellet that hit and an
/// assumption for one that missed: a missed pellet's blast goes off
/// `aim_offset` metres to one SIDE, and nothing draws which side. Putting it on
/// the line is the only choice that invents nothing.
#[allow(clippy::too_many_arguments)]
pub(super) fn spread_from_blast(
    // WHERE THE ROUND WENT OFF, decided by the caller from the pellet's own
    // deviation — not assumed to be the aimed body's surface, which is what it
    // was until 2026-08-19 and the whole of the bug this signature ends.
    det: crate::rules::space::Detonation,
    others: &mut [SpreadFoe],
    params: &FightParams,
    ap: &FightParams,
    rad: &crate::build::loadout::ResolvedRadial,
    // The aimed hit with its own CO bucket AND its own falloff divided back
    // out, so each body can multiply in its own of both.
    raw_per_bucket_per_falloff: f64,
    shares: TypeShares,
    crit_multiplier: f64,
    crit_tier: u32,
    attrition: f64,
    modded_base: f64,
    status_chance: f64,
    forced: &[DamageType],
    vector: &DamageVector,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
) {
    blast_at(
        det,
        others,
        params,
        ap,
        rad,
        raw_per_bucket_per_falloff,
        shares,
        crit_multiplier,
        crit_tier,
        attrition,
        modded_base,
        status_chance,
        forced,
        vector,
        gal,
        arc,
        r,
        rec,
        d,
        t,
        SpreadBy::Blast,
    );
}

/// [`spread_from_blast`] AT AN EPICENTRE OF ITS OWN.
///
/// The aimed body's explosion goes off on its surface and that is the only
/// place a blast happened until a projectile started BOUNCING: a ricochet
/// explodes wherever it landed, which is another body entirely. Same rules from
/// there — any body touching the sphere is caught, each reads its own falloff.
#[allow(clippy::too_many_arguments)]
pub(super) fn blast_at(
    det: crate::rules::space::Detonation,
    others: &mut [SpreadFoe],
    params: &FightParams,
    ap: &FightParams,
    rad: &crate::build::loadout::ResolvedRadial,
    raw_per_bucket_per_falloff: f64,
    shares: TypeShares,
    crit_multiplier: f64,
    crit_tier: u32,
    attrition: f64,
    modded_base: f64,
    status_chance: f64,
    forced: &[DamageType],
    vector: &DamageVector,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
    by: SpreadBy,
) {
    for (i, spec) in params.others.iter().enumerate() {
        // THREE DIMENSIONS: the floor gap and how far over or under the shot
        // went are legs of the same triangle, and a pellet that sailed above
        // the crowd is genuinely farther from every one of them.
        let dist = det.distance_to(spec.at);
        if !crate::rules::space::caught_by_blast(dist, rad.radius_m) {
            continue;
        }
        let share = rad.falloff_at(crate::rules::space::blast_reach(dist));
        if share <= 0.0 {
            continue;
        }
        let inst = crate::rules::chain::Instance {
            target: i + 1,
            share,
            multishot: false,
            headshot: false,
            part_factor: 1.0,
            status_part_factor: 1.0,
            status_landing: 1.0,
        };
        spread_hit(
            &inst,
            &mut others[i],
            spec,
            raw_per_bucket_per_falloff,
            shares,
            crit_multiplier,
            crit_tier,
            attrition,
            modded_base,
            status_chance,
            forced,
            vector,
            params,
            ap,
            gal,
            arc,
            r,
            rec,
            d,
            t,
            by,
        );
    }
}

/// RESOLVE THE SHOT'S CHAIN AND LAND EVERY INSTANCE THAT IS NOT THE AIMED
/// BODY'S OWN.
///
/// `multishot_half` splits the work the way the wiki splits the mod:
///
///   · `true` — the instances launched from the body the BEAM struck. Called
///     from inside the pellet loop, so they fire once per landing pellet, which
///     is what "only targets directly hit by the beam benefit" means for a
///     merged beam whose multishot IS its pellet count.
///   · `false` — everything launched from a body the RADIUS caught. Called once
///     for the shot, because *"beams chaining from targets that were in the
///     damage radius but not directly struck by the initial beam itself will
///     also not benefit from multishot"*.
///
/// The aimed body's own instance is skipped in both passes: it already took the
/// hit through the ordinary path, and the splash is not a second instance.
#[allow(clippy::too_many_arguments)]
pub(super) fn spread_from_seeds(
    others: &mut [SpreadFoe],
    params: &FightParams,
    ap: &FightParams,
    beam: crate::model::BeamGeometry,
    raw_per_bucket: f64,
    shares: TypeShares,
    crit_multiplier: f64,
    crit_tier: u32,
    attrition: f64,
    modded_base: f64,
    status_chance: f64,
    forced: &[DamageType],
    vector: &DamageVector,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
    layout: Option<&crate::rules::chain::Layout>,
    multishot_half: bool,
    // EVERY BODY THE BEAM STRUCK, in ray order — one of them ordinarily, more
    // with punch through, and EMPTY when it struck the floor. It decides who
    // may headshot and who carries multishot, and nothing else.
    struck: &[usize],
) {
    let spec = crate::rules::chain::Spec {
        hops: beam.chain_hops,
        range_m: beam.chain_range_m,
        falloff: beam.chain_damage_per_hop,
        compounds: beam.chain_compounds,
    };
    // FROM THE PRECOMPUTED LAYOUT. Nothing in this arena moves, so which body
    // the sphere catches and which body is nearest to which are constants —
    // asked once per engagement instead of once per landing pellet. On a 19x19
    // grid the scan was ~11,000 distance computations a pellet to reach the
    // same thirteen bodies a 7x7 reaches. `resolve_in` answers instance for
    // instance what `resolve` does (`a_layout_answers_exactly_what_the_scan_does`),
    // which is what makes this an optimisation rather than a model change.
    let landed = match layout {
        Some(layout) => crate::rules::chain::resolve_in(layout, params.others.len() + 1, struck, spec),
        // No layout means no formation — the one-body fight, where the chain
        // has nowhere to go and the old path is as cheap as anything.
        None => {
            let mut bodies = Vec::with_capacity(others.len() + 1);
            bodies.push(params.target_at);
            bodies.extend(params.others.iter().map(|f| f.at));
            crate::rules::chain::resolve(
                &bodies,
                struck,
                crate::rules::chain::Splash {
                    at: match (struck.first(), params.aim_at) {
                        (Some(_), _) => {
                            crate::rules::space::detonation_point(params.target_at, params.player_at)
                        }
                        (None, Some(a)) => a,
                        (None, None) => params.target_at,
                    },
                    radius_m: beam.damage_radius_m,
                },
                spec,
            )
        }
    };
    for inst in landed.iter().filter(|i| i.multishot == multishot_half && i.target != 0) {
        let idx = inst.target - 1;
        let (foe, fs) = (&mut others[idx], &params.others[idx]);
        spread_hit(
            inst,
            foe,
            fs,
            raw_per_bucket,
            shares,
            crit_multiplier,
            crit_tier,
            attrition,
            modded_base,
            status_chance,
            forced,
            vector,
            params,
            ap,
            gal,
            arc,
            r,
            rec,
            d,
            t,
            SpreadBy::Chain,
        );
    }
}

/// FIRE A SYNDICATE RADIAL — 1000 of its element in 25 m, with a guaranteed
/// proc for five of the six.
///
/// A FLAT INSTANCE, like Cascadia Empowered's: the build does not scale it. No
/// damage mods, no crit, no multishot, no body part — the explosion is the
/// SYNDICATE's, not the weapon's, and nothing on the card changes its size.
/// Faction bonuses and the target's own mitigation still apply, because those
/// are properties of what is being hit rather than of what is hitting it.
///
/// The 25 m radius is why this lands whole: the arena's only enemy is always
/// inside it.
#[allow(clippy::too_many_arguments)]
pub(super) fn fire_syndicate_radial(
    sy: &crate::data::syndicates::SyndicateDef,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    target: &mut TargetState,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    params: &FightParams,
    rng: &mut Rng,
    at: f64,
) {
    let status_damage = params.status_duration_multiplier;
    let mit = debuffs.mitigation(at, status_damage, params.armor_strip_per_puncture, params.squad.enemy_armor_multiplier);
    let amt = sy.damage * params.faction_at_time(at);
    let mut breakdown = Breakdown::default();
    let settled = target.apply(
        amt,
        TypeShares::single(sy.element),
        false,
        at,
        &params.target,
        false,
        &mit,
        1.0,
        watching(rec, &mut breakdown),
    );
    let (eff, killed, _broke) = (settled.effective, settled.killed, settled.broken);
    r.sources.syndicate += eff;
    r.sources.syndicate_by_type[sy.element as usize] += eff;
    ledger::settle(
        r, rec, at, 0, sy.element, PopKind::Arcane, &breakdown, settled,
        Some(debuffs),
        ledger::Clock::Hit,
        || Instance {
            origin: crate::record::Origin::Arcane,
            base: sy.damage,
            layers: mul_layers(sy.damage, &faction_layers(params, at, DEPTH_HIT)),
            ..Instance::default()
        },
    );
    r.note_kills(u32::from(killed), at, params.drop_is_in_reach(target.at));
    // GUARANTEED, for five of the six — Justice stuns instead of applying
    // Blast, the one place these effects differ in kind rather than in element.
    //
    // Through `settle_procs` like any other proc, with THIS instance as the
    // scale: a Gas cloud from a syndicate blast burns off the blast's own 1000,
    // not off the weapon's modified base, because the blast is what applied it.
    // The five whose procs are multipliers rather than DoTs (Viral, Corrosive,
    // Magnetic, Radiation) do not read that number at all.
    if sy.guaranteed_status {
        // Its own instance, like the field tick above.
        arc.next_instance();
        settle_procs(
            vec![sy.element],
            at,
            InstanceScale {
                mb_live: sy.damage,
                crit_multiplier: 1.0,
                part_factor: 1.0,
                landing: 1.0,
                attrition: 1.0,
                // A syndicate blast is 1000 of one element and "the build does
                // not scale it", so a Blast stack it applies detonates with no
                // elemental bracket behind it either.
                xh_bracket: 1.0,
            },
            debuffs,
            gal,
            arc,
            target,
            params,
            params,
            &mit,
            r,
            rec,
            rng,
            &params.target,
            DEPTH_PROC,
        );
    }
}

/// WHAT DISTANCE LEAVES OF THIS INSTANCE — the explosion's own ramp from its
/// epicentre, or the direct hit's falloff over the gap it flew.
pub(super) fn falloff_factor(
    ap: &FightParams,
    params: &FightParams,
    rad: Option<&crate::build::loadout::ResolvedRadial>,
    det: crate::rules::space::Detonation,
    gap_m: f64,
) -> f64 {
    match (rad, ap.falloff) {
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
    }
}

/// WHAT THE SHOT REACHED BEYOND THE BODY IT WAS AIMED AT — the tendrils and
/// the radius-caught chains, which are the shot's and not any pellet's, and
/// every other body's own status burning.
///
/// NOTHING TO DO WITHOUT A FORMATION, and the caller checks that rather than
/// this: every line here is a no-op on an empty one, and paying a call for
/// them on every shot of the single-target fight cost 3.4%.
///
/// ONCE PER SHOT, which is the multishot rule these two spreads are named by:
/// a tendril is an extra BEAM rather than a spread of this one, so it neither
/// takes the multishot nor fires per pellet.
#[allow(clippy::too_many_arguments)]
pub(super) fn spread_beyond_the_target(
    params: &FightParams,
    ap: &FightParams,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
    shot_spread: &Option<SpreadShot>,
    others: &mut [SpreadFoe],
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    tendril_count: u32,
    chain_layout: Option<&crate::rules::chain::Layout>,
    struck: &[usize],
) {
    // …AND THE TENDRILS, ONCE FOR THE SHOT. They are extra BEAMS rather
    // than a spread of this one, so they neither take its multishot nor
    // fire per pellet — and they only exist once there is a body that is
    // not the one the main beam is on (`spread_from_tendrils`).
    if let (Some(s), false) = (&shot_spread, others.is_empty()) {
        spread_from_tendrils(
            others,
            params,
            ap,
            tendril_count,
            s.raw_per_bucket,
            s.shares,
            s.crit_multiplier,
            s.crit_tier,
            s.attrition,
            s.modded_base,
            s.status_chance,
            &s.forced,
            &s.vector,
            gal,
            arc,
            r,
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
            others,
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
            gal,
            arc,
            r,
            rec,
            d,
            t,
            chain_layout,
            false,
            // THE SAME STRUCK LIST the per-pellet half used — this pass
            // fires the seeds the RADIUS caught, and it tells them apart by
            // filtering on `multishot`, so it has to agree with that half
            // about who was struck directly.
            struck,
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
            gal,
            arc,
            t + 1e-9,
            &mut f.state,
            params,
            ap,
            r,
            rec,
            &mut d.status,
            &params.others[bi].params,
            bi + 1,
        );
    }
}
