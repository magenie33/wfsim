use super::*;

/// One live LINGERING FIELD attached to the target — one entity per grenade
/// that stuck, since each multishot projectile is its own grenade and its own
/// cloud. `FieldStacking::Refresh` keeps this list at length 1.
#[derive(Debug, Clone, Copy)]
pub(super) struct FieldState {
    pub(super) next_tick: f64,
    pub(super) ticks_left: u32,
    /// The part AS RESOLVED BY THE FORM THAT SPAWNED IT. A cloud outlives a
    /// transmute, and only one form of a transform group has a field at all, so
    /// the field cannot be re-read from the active form.
    pub(super) part: crate::build::loadout::ResolvedLingering,
    /// Plentiful Mayhem: the independent damage multiplier the SPAWNING pellet
    /// carried (1.0 for the weapon's own projectile, 1+bonus for one multishot
    /// generated). Per field, because within one pull some grenades have it and
    /// some do not.
    pub(super) damage_multiplier: f64,
}

/// The attacker-side buff state a FIELD tick reads, as of the most recent
/// shot.
///
/// The parts that MATTER are live, not snapshotted: Condition Overload and the
/// arcane runtime are both read at the tick itself (the CO catalog's Pox row is
/// explicit — "damage recalculates on every tick"). What this carries is the
/// MOD-side buffs whose state lives in the shot loop's locals — Galvanized
/// Scope's crit buff, Overwhelming Attrition's stacks — snapshotted at the
/// shot. At Torid's 1.5 shots/s that is under a second of staleness on buffs
/// measured in seconds; it is recorded here rather than hidden because it IS an
/// approximation.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct FieldCtx {
    /// Attacker BuffBar flat crit chance — ABSOLUTE, lands on every part.
    pub(super) flat_crit: f64,
    /// Σ RELATIVE crit-chance bonuses from MOD buffs.
    pub(super) crit_chance_relative_mods: f64,
    /// Σ live base-damage bucket additions from MOD/evolution buffs.
    pub(super) base_damage_add_mods: f64,
    /// WHAT A HEAD ON THIS FIGHT'S TARGET IS WORTH — the location multiplier
    /// with the whole additive headshot ladder folded on (M60: they ADD).
    ///
    /// Read by a field whose ticks can find a head at all (the Grimoire's orb;
    /// [`crate::build::loadout::ResolvedLingering::headshot_chance`]), and by nothing
    /// else — a cloud carries no body part.
    ///
    /// DELIBERATELY NOT the pellet loop's own copy of this ladder, which stays
    /// per PELLET: Lingering Judgement's window can open on a headshot in the
    /// middle of a pull, so the two are computed at different instants on
    /// purpose. Snapshotting it here is this struct's whole convention, and it
    /// is under a second of staleness for the same reason the crit buffs are.
    pub(super) head_factor: f64,    /// The same brackets over a 1x base: what an Electricity or Gas tick is
    /// worth when the field's hit found a head (`Dot::landing`).
    pub(super) head_landing: f64,
}

impl FightParams {
    /// **THE ACTION PRIORITY LIST THIS FIGHT RUNS** — the player's inserted
    /// rules above the fight's own half (`data::apl::for_fight`).
    ///
    /// COMPOSED, NEVER STORED. The fight's half turns on one fact this struct
    /// already carries — whether there is a cycle to decide — and a stored copy
    /// is the second source that would disagree with it.
    pub fn apl(&self) -> crate::data::apl::Apl {
        crate::data::apl::for_fight(&self.apl_inserted, self.cycle.is_some())
    }

    /// A TOME'S CYCLE: fire the weapon, fill the meter, throw an orb, carry on.
    ///
    /// The params are the BASE form's — a Tome shoots its primary fire the
    /// whole engagement and never leaves it — with the other form's ORB and
    /// METER laid on top. There is no transformation and no second magazine,
    /// which is what makes this a constructor rather than a second
    /// [`IncarnonCycle`]: the orb is an ENTITY you throw, not a form you enter,
    /// so nothing about the shot loop has to switch.
    ///
    /// That is the whole cycle. The meter's three terms (a second a second, a
    /// second per landing pellet, ten a secondary ammo pickup) are read in the
    /// run loop, and every one of them is a thing the BASE form is doing —
    /// which is why they only ever pay here and not in `transformed`.
    pub fn tome_cycle_from_panels(
        base: &crate::build::loadout::ResolvedPanel,
        orb_form: &crate::build::loadout::ResolvedPanel,
        arena: &crate::arena::Arena,
        arcane: &crate::data::arcanes::ArcaneFx,
    ) -> Self {
        let mut d = Self::from_panel(base, arena, arcane);
        let thrown = Self::from_panel(orb_form, arena, arcane);
        d.meter = orb_form.meter;
        d.orb = thrown.orb;
        d.orb_strike = thrown.orb_strike;
        d.orb_blast = thrown.orb_blast;
        // THE BASE FORM'S OWN AIM IS LEFT ALONE. You POINT a Tome's primary
        // fire, so its pellets follow the scenario's headshot_pct; the orb does
        // not, and its strikes read the chance carried on the orb itself. One
        // field could not hold both answers, which is why that one lives on
        // `ResolvedOrb` rather than beside this.
        d
    }
}

/// Settle every FIELD tick due strictly before `until`, oldest first.
///
/// A separate pass from [`process_ticks`] on purpose — a field tick is weapon
/// damage that rolls its own crit and its own status, not a status settlement —
/// but the two are INTERLEAVED here: every status event preceding a field tick
/// is settled before it. That matters in both directions. A field tick's own
/// procs become DoTs that must still burn — an end-of-run drain running before
/// the last clouds tick pushes those procs and never settles them — and the CO
/// bonus a tick reads has to include the statuses its predecessors applied.
#[allow(clippy::too_many_arguments)]
pub(super) fn process_field_ticks(
    // See `process_ticks` — a cloud reads the shooter's live windows.
    w: &CardWindows,
    fields: &mut Vec<FieldState>,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    until: f64,
    target: &mut TargetState,
    params: &FightParams,
    ap: &FightParams,
    ctx: &FieldCtx,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    // A field tick decides BOTH a crit and its procs, so it takes the whole
    // set of streams rather than one of them.
    d: &mut crate::rules::rng::Draws,
    // THE REST OF THE FORMATION. A cloud is an AREA, so everyone standing in
    // it burns — which is the base form's half of what a radius mod buys, and
    // the half a grenade lives on. Empty for every fight
    // this engine ran before a formation existed.
    others: &mut [SpreadFoe],
) {
    // Oldest due tick first, re-scanned each time: a tick's own procs change
    // what the NEXT tick sees, so the order has to be resolved live.
    while let Some((i, at)) = fields
        .iter()
        .enumerate()
        .filter(|(_, f)| f.ticks_left > 0 && f.next_tick < until)
        .min_by(|a, b| a.1.next_tick.total_cmp(&b.1.next_tick))
        .map(|(i, f)| (i, f.next_tick))
    {
        // Status events strictly before this tick land first.
        process_ticks(
            w,
            debuffs,
            gal,
            arc,
            at + 1e-9,
            target,
            params,
            ap,
            r,
            rec,
            &mut d.status,
            &params.target,
            0,
        );
        let part = fields[i].part;
        let damage_multiplier = fields[i].damage_multiplier;
        fields[i].next_tick += 1.0 / part.tick_rate;
        fields[i].ticks_left -= 1;
        let killed = field_tick(
            w,
            &part,
            damage_multiplier,
            at,
            ctx,
            debuffs,
            gal,
            arc,
            target,
            params,
            ap,
            r,
            rec,
            d,
            &params.target,
            crate::record::Origin::Field,
            None,
            false,
        );
        // …AND EVERY OTHER BODY STANDING IN IT. The cloud is stuck to the body
        // it was left on, so that is its centre, and the three blast rules
        // decide the rest: any body touching it burns, at its own falloff
        // distance measured to its own nearest point (`engine::space`).
        //
        // A KILL HERE DOES NOT CLEAR THE CLOUD, which is the one asymmetry: the
        // clouds belong to the body they are stuck to, so only THAT one dying
        // takes them with it. Another body dying inside one is just a body
        // dying inside it.
        for (bi, spec) in params.others.iter().enumerate() {
            let dist = spec.at.distance(params.target_at);
            if !crate::rules::space::caught_by_blast(dist, part.radius_m) {
                continue;
            }
            let SpreadFoe { state, debuffs: fd } = &mut others[bi];
            field_tick(
            w,
                &part,
                damage_multiplier * part.falloff_at(crate::rules::space::blast_reach(dist)),
                at,
                ctx,
                fd,
                gal,
                arc,
                state,
                params,
                ap,
                r,
                rec,
                d,
                &spec.params,
                crate::record::Origin::Field,
                None,
                false,
            );
        }
        if killed {
            // A CLOUD IS A PLACE, AND IT OUTLIVES WHAT IT STUCK TO.
            //
            // The fields are NOT cleared with the corpse they stuck to. The
            // weapon's own page says so, verbatim: "Torid projectiles can
            // also attach to corpses and will remain at their position even if
            // they disintegrate, granting a fixed position mid-air and allowing
            // a greater spread of toxin damage onto enemies" — and the
            // respawned individual stands where the dead one did, so the cloud
            // is on it (asking the same question of the GAS
            // proc, which the Gas page answers the same way).
            //
            // IT MOVES SCORES, and that is the point: the cloud was wiped on
            // every respawn, which on a fight with instant respawns is most of
            // its uptime. `one_fight`'s three shapes do not see it because
            // their Thrax never dies.
            debuffs.on_death(params.acid_shells, &params.target);
            return;
        }
    }
    fields.retain(|f| f.ticks_left > 0);
}

/// ONE tick of a lingering field, resolved as a full damage INSTANCE — returns
/// whether it killed the target. MECHANICS §7 "Lingering damage FIELDS".
///
/// It follows the radial's rules — no body-part multiplier and no crit-headshot
/// fold-in ("Explosion has a headshot multiplier of 1x and cannot trigger
/// headshot conditions"), its own crit roll ("…the Torid's gas cloud not
/// allowing for criticals" was a fixed BUG), its own status draw ("Toxin clouds
/// can proc Hunter Munitions on each tick of damage") — and adds the one thing
/// a field has that a radial does not: it TAKES Condition Overload, on the
/// attached target, which a single-target arena always is.
#[allow(clippy::too_many_arguments)]
pub(super) fn field_tick(
    w: &CardWindows,
    f: &crate::build::loadout::ResolvedLingering,
    // Plentiful Mayhem's independent multiplier, carried from the grenade that
    // left this cloud (1.0 = the weapon's own projectile, or no such perk).
    damage_multiplier: f64,
    at: f64,
    ctx: &FieldCtx,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    target: &mut TargetState,
    params: &FightParams,
    ap: &FightParams,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    // WHICH BODY THIS BURNS — `params.target` until a cloud could stand over
    // more than one. Its pools, its stack caps, its immunities.
    foe: &TargetParams,
    // WHICH MECHANISM PRODUCED IT. The arithmetic of a damage instance on a
    // clock of its own is the same for a cloud's tick and a deployed orb's
    // strike, so the two share this function; the RECORD must still say which,
    // because they are different mechanics and a reader laying the panel beside
    // the game needs to tell them apart.
    origin: crate::record::Origin,
    // WHERE IT MAY LAND. `None` is a cloud, which is an area and finds no body
    // part at all; a chance is an unaimed strike's, and it takes no draw unless
    // it is stated — which is what keeps every existing field's dice where they
    // were.
    head_chance: Option<f64>,
    // …AND WHICH METER IT GOES IN. An orb ends its fuse in an EXPLOSION, and an
    // explosion belongs in the radial bucket wherever it was fired from — a
    // reader looking for what the blast was worth must not have to know that
    // this engine happens to settle it through the same function a cloud's tick
    // goes through.
    is_blast: bool,
) -> bool {
    let status_damage = params.status_duration_multiplier;
    let mit = debuffs.mitigation(at, status_damage, params.armor_strip_per_puncture, params.squad.enemy_armor_multiplier);
    // The field is its own attack part, so the ability elements are sized off
    // ITS ModifiedBase — same rule as the explosion's.
    let qvec = params.with_live_elements(f.damage.quantized_against(f.modified_base), f.modified_base, at, w);
    let qtotal = qvec.total();
    let shares = TypeShares::of(&qvec);

    // WHERE THIS TICK LANDED. A CLOUD lands nowhere — it is an area, and the
    // radial's rule holds for it: *"Explosion has a headshot multiplier of 1x
    // and cannot trigger headshot conditions"*. That is every field in the
    // roster but one, and it takes NO draw, which is what keeps their dice
    // exactly where they were.
    //
    // The Grimoire's orb is the other kind. The wiki says outright that its
    // strikes find weak points — *"The strikes and the forced Electricity proc
    // can hit weakspots"* — so a strike picks a body part through the same
    // helper an unaimed shot does, and everything below treats it as that part.
    //
    // THE CHANCE IS AN ARGUMENT, not `ap.unaimed_headshot_chance`, because in a
    // TOME'S CYCLE the two disagree on purpose: you POINT the primary fire, so
    // its pellets follow the scenario's aim, and the orb you threw picks its
    // own body. One field on the params could hold one of those answers.
    let part = head_chance.map(|c| unaimed_part(&params.body_parts, c, &mut d.spine));

    // Crit: the field's OWN base stats. Relative bonuses scale its base,
    // absolute ones land flat (MECHANICS §7).
    let crit_chance_relative = ctx.crit_chance_relative_mods + params.arcane.crit_chance_relative;
    let cc = f.crit_chance + ctx.flat_crit + f.base_crit_chance * crit_chance_relative;
    let tier = upgrade_crit_tier(roll_crit_tier(cc, &mut d.spine), ap.crit_tier_upgrade_chance, &mut d.spine);
    let crit_damage_relative = arc.total(&params.arcane.buffs, ArcGrant::CritDamage, at)
        + arc.cd_bonus(ap, at)
        + params.arcane.crit_damage_relative;
    let cd = f.crit_damage + f.base_crit_damage * crit_damage_relative + debuffs.cold_cd_bonus(at);
    // …AND THE CRIT-HEADSHOT FOLD-IN, on a tick that found an eligible head.
    // No exception is invented for it: a strike that can hit a weak point is a
    // hit on a weak point, and `Critical_Hit` §Critical Headshots is the rule
    // for one. A cloud never reaches this, because a cloud has no part.
    let cd = match part {
        Some(p) if p.is_head && p.crit_bonus && p.multiplier > 1.0 => 2.0 * cd,
        _ => cd,
    };
    let crit_multiplier = 1.0 + tier as f64 * (cd - 1.0);

    // Damage buckets: the same live base-damage additions the direct hit reads,
    // then the GunCO bracket off the target's CURRENT status count.
    let base_damage = ap.base_damage_bonus;
    let arcane_base_damage = arc.total(&params.arcane.buffs, ArcGrant::BaseDamage, at)
        + ctx.base_damage_add_mods
        + heavy_attack_base_damage(ap)
        + arc.rage_bonus(at);
    let arc_ratio = (1.0 + base_damage + arcane_base_damage) / (1.0 + base_damage);
    // CO on an AoE part is the EXCEPTION, not the default. What the mods say is
    // direct hits only — which is why the radial path never takes it — and the
    // Torid's cloud is an anomaly the CO catalog gives its own row: in theory
    // an AoE would not get it, and DE let this one. So the field takes CO only
    // where the weapon declares it; otherwise it gets
    // the same bracket the radial does.
    let bucket = if f.takes_condition_overload {
        // A FIELD keeps the direct hit's base fraction: the CO catalog puts
        // the Torid's cloud on the same base as its main fire.
        // A FIELD TICK carries no half-health term: the bonus is a DIRECT-hit
        // bonus like CO itself, and nothing in the catalog says otherwise.
        gunco_bucket(params, ap, debuffs, gal, at, base_damage, arcane_base_damage, arc_ratio, 0.0,
            ap.co_base.borrowed_for(crate::model::CoStage::Field), crate::model::CoStage::Field)
            .bucket
    } else {
        arc_ratio
    };
    let mb_live = f.modified_base * arc_ratio;

    // WHAT THE PART IS WORTH — the head ladder as of this shot (`FieldCtx`), or
    // the part's own multiplier, or 1.0 for a field with no part at all.
    let part_factor = match part {
        Some(p) if p.is_head => ctx.head_factor,
        Some(p) => p.multiplier,
        None => 1.0,
    };

    // Falloff is 1.0: the grenade STICKS to the target, so the target stands at
    // the epicentre for every tick — which is exactly why the wiki calls a
    // direct hit "the maximum possible damage".
    //
    // `damage_multiplier` (Plentiful Mayhem) rides here rather than inside ModifiedBase:
    // the wiki calls it "multiplicative to base damage bonuses like Serration",
    // i.e. its own bracket. Consequence, recorded because nothing sources it:
    // the status payloads below are left OUT of it, the same treatment the beam
    // ramp and Devouring Attrition already get.
    // Depth 1: a direct hit carries the faction bonus once. Written through
    // `faction_at` like the other two rungs so the ladder is visible at every
    // level rather than only where it compounds.
    let raw =
        qtotal * crit_multiplier * part_factor * bucket * faction_at(params.faction_at_time(at), DEPTH_HIT)
            * damage_multiplier
            * params.ability_final_at(at);
    let col = target.incoming_column(foe);
    let mut breakdown = Breakdown::default();
    let settled = target.apply(
        raw,
        shares,
        false,
        at,
        foe,
        false,
        &mit,
        1.0,
        watching(rec, &mut breakdown),
    );
    let (effective, killed, broke) = (settled.effective, settled.killed, settled.broken);
    if is_blast {
        r.sources.radial += effective;
        add_by_type(&mut r.sources.radial_by_type, &qvec, effective, &col);
    } else {
        r.sources.field += effective;
        add_by_type(&mut r.sources.field_by_type, &qvec, effective, &col);
    }
    ledger::settle(
        r, rec, at, 0, DamageType::Cinematic,
        // THE NUMBER'S OWN SHAPE ON SCREEN: a blast draws as a blast.
        if is_blast { PopKind::BlastArea } else { PopKind::Field },
        &breakdown, settled,
        Some(debuffs),
        ledger::Clock::Hit,
        || Instance {
            origin,
            // A CLOUD'S TICK IS WEAPON DAMAGE ON ITS OWN CLOCK — neither a
            // hit nor a status — so it carries a hit's factors, and no falloff
            // term because the grenade sticks to the target.
            //
            // THE BODY PART IS x1 FOR EVERY FIELD BUT ONE, and it is written
            // down rather than omitted: a row whose number carries a x3 the
            // ledger does not name is a row the reader cannot check, which is
            // the one thing this stream exists to prevent.
            base: qtotal,
            layers: mul_layers(qtotal, &[
                (crate::record::Factor::BodyPart, part_factor),
                (crate::record::Factor::Critical, crit_multiplier),
                (crate::record::Factor::ConditionOverload, bucket),
            ].into_iter().chain(faction_layers(params, at, DEPTH_HIT)).chain([
                (crate::record::Factor::FieldDamage, damage_multiplier),
                (crate::record::Factor::WarframeAbility, params.ability_final_at(at)),
            ]).collect::<Vec<_>>()),
            ..Instance::default()
        },
    );
    // AND IT IS NOT A TICK. `field_ticks` is how many times a CLOCK paid out;
    // a fuse running out once is not one of them, and counting it there made
    // the orb report 6.88 strikes an orb when it makes at most six.
    r.field_ticks += u32::from(!is_blast);
    r.note_kills(killed as u32, at, params.drop_is_in_reach(target.at));
    if let Some(pool) = broke {
        push_break_proc(debuffs, params, at, pool);
    }
    if killed {
        gal.bump_on_kill(params, at);
        arc.on_kill(params, at);
        return true;
    }
    // Status per TICK, from the field's own vector and its own status chance,
    // plus THIS FIELD'S OWN forced procs — declared per attack part, which is
    // why a cloud (declaring none) passes an empty set and lands exactly where
    // it always did. The Grimoire's orb forces Electricity on every pulse and
    // its final explosion forces nothing, which is one attack answering the
    // question both ways.
    //
    // A tick is its OWN damage instance, at its own time — so a per-instance
    // arcane cap resets here rather than sharing the shot's allowance.
    arc.next_instance();
    let mut forced_buf = [DamageType::Impact; DamageType::ALL.len()];
    let forced_n = f.forced_procs.fill(&mut forced_buf);
    let procs = status::procs_for_hit(
        &forced_buf[..forced_n],
        f.status_chance,
        &qvec,
        &foe.status_immunities,
        &mut d.status,
    );
    settle_procs(
        procs,
        at,
        InstanceScale {
            mb_live,
            crit_multiplier,
            // A STATUS IS STAMPED WITH THE MULTIPLIERS OF THE HIT THAT APPLIED
            // IT — measured (M54), and 1.0 for every field that cannot find a
            // head, which is the value this passed before the orb arrived.
            part_factor,
            landing: match part {
                Some(p) if p.is_head => ctx.head_landing,
                _ => 1.0,
            },
            attrition: 1.0,
            // The BASE ATTACK's, not the cloud's: a Blast stack the cloud
            // applies still detonates off a gun, and the bracket its extra hit
            // takes is that gun's.
            xh_bracket: ap.extra_hit_bracket(at, w),
        },
        debuffs,
        gal,
        arc,
        target,
        params,
        ap,
        &mit,
        r,
        rec,
        &mut d.status,
        foe,
        DEPTH_PROC,
    );
    false
}
