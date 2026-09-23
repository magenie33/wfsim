use super::*;

/// Settle ONE damage instance's status procs onto the target — MECHANICS §6.
///
/// Every instance kind applies status by identical rules — a direct pellet, a
/// radial stage and a lingering-FIELD tick all land here. `at` is the
/// INSTANCE's own time rather than the shot clock, because a cloud ticks
/// between shots and its procs' durations run from the tick; `scale` carries
/// the DoT payloads' scaling (ModifiedBase × crit × body part) and `active` the
/// ACTIVE form, whose element brackets differ.
#[allow(clippy::too_many_arguments)]
/// How many times a FACTION bonus has been applied by the time a payload lands.
///
/// Faction damage is re-applied at every DERIVATION step, so this is a count of
/// how far a number is from the hit that started it: a direct hit is depth 1
/// (×f), a status the hit applied is 2 (×f²), a status a DERIVED instance
/// applied is 3 (×f³). Depth 3 is arithmetic rather than a quirk to hardcode —
/// a DoT at ×f³ PROVES an intermediate damage instance exists, which is how
/// Primary Debilitate's extra status is known to deal one rather than add a
/// stack. A depth rather than `fm2` and a future `fm3` keeps the next spreading
/// mechanic from inventing its own multiplier.
pub(super) const DEPTH_HIT: u32 = 1;
pub(super) const DEPTH_PROC: u32 = 2;
/// A status applied by a damage instance that was itself derived from a hit.
pub(super) const DEPTH_DERIVED_PROC: u32 = 3;

/// The faction multiplier a payload at `depth` carries.
pub(super) fn faction_at(faction_multiplier: f64, depth: u32) -> f64 {
    faction_multiplier.powi(depth as i32)
}

/// THE SAME NUMBER, SAID AS THE TWO THINGS A READER CAN CHECK — the shooter's
/// bracket and the target's own multiplier, each raised to the payload's depth.
///
/// Their product is exactly `faction_at(params.faction_at_time(t), depth)`, so
/// this changes no damage; it exists because `x1.24 faction` with a +55% Bane
/// equipped is a number nobody can trace back to a card. `mul_layers` drops a
/// x1.00, so on every target that declares no multiplier this is one layer and
/// reads exactly as it did before.
pub(super) fn faction_layers(params: &FightParams, t: f64, depth: u32) -> [(crate::record::Factor, f64); 2] {
    [
        (crate::record::Factor::Faction, faction_at(params.faction_bracket_at(t), depth)),
        (
            crate::record::Factor::TargetMultiplier,
            faction_at(params.foe.faction_bracket_multiplier, depth),
        ),
    ]
}

/// How many stacks of a COMBINED status the target currently holds.
///
/// Only the six combinations answer — a primary or a physical proc has no
/// components, so nothing else can be split and nothing else is counted.
pub(super) fn combined_stacks(debuffs: &DebuffState, t: DamageType) -> usize {
    match t {
        DamageType::Viral => debuffs.virus.len(),
        DamageType::Corrosive => debuffs.corrosion.len(),
        DamageType::Magnetic => debuffs.disrupt.len(),
        DamageType::Radiation => debuffs.confusion.len(),
        DamageType::Blast => debuffs.blast.len(),
        // Gas lives in the DoT list rather than a stack vector.
        DamageType::Gas => debuffs
            .dots
            .iter()
            .filter(|d| d.dtype == DamageType::Gas && d.ticks_left > 0)
            .count(),
        _ => 0,
    }
}

/// IS THIS STATUS ON THE TARGET RIGHT NOW?
///
/// A status lives in one of two places depending on what it is: the combined
/// ones keep their own stack vectors, while the damaging primaries are DoT
/// entries. `combined_stacks` answers the first group; this answers both, which
/// is what a target-conditional buff needs (Stormburst asks about Electricity,
/// a DoT).
pub(super) fn has_status(debuffs: &DebuffState, t: DamageType) -> bool {
    if combined_stacks(debuffs, t) > 0 {
        return true;
    }
    // …AND THE PILES THAT ARE NOT COMBINED ELEMENTS. `combined_stacks` answers
    // for the six combined ones, which is all Primary Debilitate ever needed to
    // ask; every physical and primary status lives in a list of its own and
    // fell through its `_ => 0` arm. So `has_status(Puncture)` was FALSE on a
    // target covered in Puncture, and a perk keyed on one — the Latron family's
    // Riddled Target — could never fire. Nothing was keyed on one
    // before, which is why it went unnoticed and not why it was right.
    let own = match t {
        DamageType::Impact => !debuffs.stagger.is_empty(),
        DamageType::Puncture => !debuffs.weakened.is_empty(),
        DamageType::Cold => !debuffs.freeze.is_empty() || debuffs.frozen_until.is_some(),
        DamageType::Heat => debuffs.heat.is_some(),
        DamageType::Void => !debuffs.attractor.is_empty(),
        // Slash, Toxin, Electricity and Gas are DoTs and are answered below.
        _ => false,
    };
    own || debuffs.dots.iter().any(|d| d.dtype == t && d.ticks_left > 0)
}

/// Stacks of a combined status the target must be AT, counting the one this
/// instance is applying, before Primary Debilitate can split it. The wiki states
/// 10 and states no scaling.
pub const DEBILITATE_STACKS: usize = 10;

/// PRIMARY DEBILITATE, decided: does this damage instance also inflict one of
/// the combined status's components, and which one? A pure function on
/// purpose — the DECISION is testable without a fight, and only the damage
/// PAYLOAD needs a measurement.
///
/// - the status just applied must be a COMBINED element, since a primary or a
///   physical proc has no components to split into
/// - the target must be AT [`DEBILITATE_STACKS`] **counting the stack this
///   instance is applying**: at nine, the shot that makes it ten splits
/// - roll `chance` (0.5 at rank 0 → 1.0 at rank 5)
/// - pick between the two components 50/50
///
/// **The threshold is why BLAST is not a special case.** Read as "already
/// holds ten" it is dead on Blast alone — reaching ten DETONATES and drains
/// every stack, so a pre-application count is 0..=9 forever. The tenth
/// APPLICATION is the trigger for every combination (M34).
///
/// Once per DAMAGE INSTANCE: "only activate once per damage instance, making it
/// less effective than it 'should' be when used on a Beam weapon, due to how
/// Multishot affects such weapons."
pub(super) fn debilitate_split(
    landed: DamageType,
    // INCLUDING the one being applied — named so the caller cannot get the
    // off-by-one back by passing the wrong count.
    stacks_with_this: usize,
    chance: f64,
    rng: &mut Rng,
) -> Option<DamageType> {
    if chance <= 0.0 || stacks_with_this < DEBILITATE_STACKS {
        return None;
    }
    let (a, b) = crate::rules::elements::components_of(landed)?;
    if rng.next_f64() >= chance {
        return None;
    }
    // 50/50, and drawn AFTER the chance roll so a failed roll consumes exactly
    // one number — a reader comparing two seeds should not have to reason about
    // how many draws a miss cost.
    Some(if rng.next_f64() < 0.5 { a } else { b })
}

/// EXTRA HIT — the second damage instance an ability grants, fired off the one
/// that triggered it. Returns whether it killed the target.
///
/// The factor-by-factor account is docs/EXTRA_HIT.md §"How the multipliers
/// stack"; the shape here is that it MULTIPLIES the triggering instance's
/// finished `trigger_raw` rather than rebuilding anything, so faction and the
/// body part are applied a second time and crit is inherited rather than
/// rolled. `part_again` is the caller's, because only the caller knows whether
/// what triggered this struck a body part at all.
pub(super) fn extra_hit_status_base(extra_hit_damage: f64, level_above: f64) -> f64 {
    if extra_hit_damage > 0.0 {
        extra_hit_damage
    } else {
        level_above
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn fire_extra_hits(
    trigger_raw: f64,
    bracket: f64,
    part_again: f64,
    head_direct: bool,
    status_chance: f64,
    at: f64,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    target: &mut TargetState,
    params: &FightParams,
    active: &FightParams,
    mit: &Mitigation,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
) -> bool {
    let hits = crate::data::abilities::extra_hits_at(&params.abilities, at);
    if hits.is_empty() || trigger_raw <= 0.0 {
        return false;
    }
    let f = params.faction_at_time(at);
    for crate::data::abilities::ExtraHitLive { element: ty, fraction, forced_status } in hits {
        let raw = trigger_raw * fraction * bracket * part_again * f;
        let mut breakdown = Breakdown::default();
        let settled = target.apply(
            raw,
            TypeShares::single(ty),
            head_direct,
            at,
            &params.foe,
            false,
            mit,
            1.0,
            watching(rec, &mut breakdown),
        );
        let (eff, killed, broke) = (settled.effective, settled.killed, settled.broken);
        r.sources.extra_hit += eff;
        r.sources.extra_hit_by_type[ty as usize] += eff;
        ledger::settle(
            r, rec, at, Combatant::WIELDER, 0, ty, PopKind::Extra, &breakdown, settled, Some(debuffs),
            ledger::Clock::Hit,
            || Instance {
                origin: crate::record::Origin::ExtraHit,
                // THE TRIGGERING HIT'S OWN DAMAGE is the base, and the four
                // factors below are the whole of docs/EXTRA_HIT.md: an
                // Extra Hit is a percentage of something else, in one
                // element, on the body part that thing struck.
                base: trigger_raw,
                layers: mul_layers(trigger_raw, &[
                    (crate::record::Factor::ExtraHitShare, fraction),
                    (crate::record::Factor::ElementBracket, bracket),
                    (crate::record::Factor::BodyPart, part_again),
                ].into_iter().chain(faction_layers(params, at, DEPTH_HIT)).collect::<Vec<_>>()),
                head: head_direct,
                ..Instance::default()
            },
        );
        r.note_kills(u32::from(killed), at, params.drop_is_in_reach(target.at));
        if let Some(pool) = broke {
            push_break_proc(debuffs, params, at, pool);
        }
        if killed {
            gal.bump_on_kill(params, at);
            arc.on_kill(params, at);
            debuffs.on_death(params.acid_shells, &params.foe);
            // A fresh individual, so the remaining extra hits of this trigger
            // are gone with the one that earned them — the same rule the wiki
            // states for the trigger itself ("If a hit that would trigger an
            // Extra Hit kills the enemy, the Extra Hit will not be triggered").
            return true;
        }
        // ITS OWN STATUS ROLL, from its own one-type vector. Through
        // `settle_procs` like everything else, so if an extra hit ever grants a
        // DAMAGING element the payload rules are already right: the wiki's
        // "Damage over Time status effects created by an Extra Hit will use the
        // Extra Hit Damage as Modded Base Damage" is exactly `mb_live: raw`.
        // ITS OWN STATUS, and whether that is a ROLL or a CERTAINTY is the
        // member's business: Xata's rolls the weapon's chance, Toxic Lash is
        // "100% (Toxin status chance)" and Resupply grants "the selected
        // Elemental Damage and Status Effect". A forced one goes through the
        // same `forced` channel a weapon's guaranteed proc uses, so the caps,
        // the immunities and Condition Overload all see it the same way.
        let forced: &[DamageType] = if forced_status { std::slice::from_ref(&ty) } else { &[] };
        let procs = status::procs_for_hit(
            forced,
            if forced_status { 0.0 } else { status_chance },
            &DamageVector::new().with(ty, raw),
            &params.foe.status_immunities,
            rng,
        );
        settle_procs(
            procs,
            at,
            InstanceScale {
                // THE CATEGORY'S RULE, not this function's: an extra hit that
                // deals damage replaces the base its status burns off, and one
                // that deals none leaves the level above standing. `raw` is
                // always positive here (the caller returns early at 0), so this
                // reads as `raw` — it is written through the rule so the two
                // members of the category cannot drift apart.
                mb_live: extra_hit_status_base(raw, trigger_raw),
                // Both already inside `raw`. Passing them again would square
                // what the trigger's own procs took once.
                crit_multiplier: 1.0,
                part_factor: 1.0,
                landing: 1.0,
                // **AND DEVOURING ATTRITION IS NOT ROLLED AGAIN** — measured. An extra hit is a percentage of a hit
                // that has ALREADY taken its x21, so it inherits that and
                // stops there: Xata's Whisper cannot re-trigger the roll and
                // reach x441.
                //
                // It was a reasonable place to be wrong. The perk's own rule is
                // "per damage instance that did not crit" and an extra hit IS a
                // second instance, so a second roll was the reading a careful
                // person would have argued for; this line carried the other one
                // on the strength of the sentence above it, which is about crit
                // and the body part. It is now the measured answer rather than
                // an inherited assumption.
                //
                // THE ONE THING THAT DOES REACH x441 is Primary Debilitate's
                // DoT, and for a different reason entirely: its zero-damage
                // instance leaks its multipliers into the burn it leaves, which
                // is a LIVE BUG in the game and is declared as one (M37).
                attrition: 1.0,
                xh_bracket: bracket,
            },
            debuffs,
            gal,
            arc,
            target,
            params,
            active,
            mit,
            r,
            rec,
            rng,
            // The extra hit is one derivation past the hit, so a status IT
            // applies is one past that. Void applies none that pay damage, so
            // this is a claim nothing collects on yet — written as the ladder
            // rather than as a number so the first one that does is right.
            &params.foe,
            DEPTH_DERIVED_PROC,
        );
    }
    false
}

/// HAND EVERY GAS CLOUD AND TESLA ARC TO THE BODIES STANDING IN IT.
///
/// The one place that knows which body is which — `settle_procs` holds one
/// body's state and posts to that body's `area_out` (see
/// [`DebuffState::area_out`] for the mechanic and its sources). THE ORIGIN IS
/// SKIPPED: it already has the DoT, and that IS the cloud's damage to the body
/// standing in it. A cloud reaches a body when any part of it touches
/// (`rules::space::caught_by_blast`), the rule every sphere here uses.
#[allow(clippy::too_many_arguments)]
/// HOW MANY DoTs OF ONE KIND A BODY CAN CARRY: the unit's own cap where it
/// declares one, and TEN otherwise — "Up to 10 instances of the effect can
/// stack on the same target" is the Gas page's wording and the rule the
/// ten-stack families in `DEBUFF_ROSTER` follow.
///
/// IT IS ALSO WHAT BOUNDS THE WORK: an uncapped list grows with every proc and
/// `process_ticks` walks it per body per shot, so a spread reaching a dozen
/// neighbours turns a linear cost quadratic.
pub(super) fn dot_cap_for(p: &Foe, dtype: DamageType) -> Option<usize> {
    // THE UNIT'S OWN CAP, where it declares one — that is a property of the
    // enemy and applies to everything it carries.
    let unit = p.stack_caps.map(|c| c.general);
    // …AND THE FAMILY'S, which is GAS AND ONLY GAS. See `dot_family_cap`.
    match (dot_family_cap(dtype), unit) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, u) => u,
    }
}

/// HOW MANY INSTANCES OF ONE DoT FAMILY A BODY CAN CARRY — and the answer is
/// only ever "ten" for GAS.
///
/// TEN IS THE GAS PAGE'S AND NOBODY ELSE'S: *"Up to 10 instances of the effect
/// can stack on the same target … Any instances after the 10th will REPLACE THE
/// OLDEST"* — a real cap, and FIFO. Slash and Toxin say each instance keeps its
/// own timer and that ten is how many "tick numbers are actually SHOWN … to
/// save performance"; Electricity states no cap at all. Reading the Gas
/// sentence as a class rule is the Condition Overload lesson in another
/// mechanic, and it costs a Torid 34 live Gas instances where the game allows
/// ten.
///
/// The cap is also what bounds the work, since `process_ticks` walks a body's
/// list once per shot. Gas is the family that AREA-spreads to every neighbour,
/// so it is the one whose list grows fastest — and the one that keeps a cap.
pub(super) fn dot_family_cap(dtype: DamageType) -> Option<usize> {
    match dtype {
        DamageType::Gas => Some(TEN_STACK_CAP),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn drain_area_procs(
    debuffs: &mut DebuffState,
    target: &mut TargetState,
    others: &mut [SpreadFoe],
    params: &FightParams,
    // WHO IS NEAR WHOM, built once per run — see `rules::space::Neighbours`. Asking
    // per proc is `O(bodies)` and a dense grid produces thousands of procs a
    // second: the Phantasma Prime on a 19x19 ruler was 9,551 multishot a run against
    // 88 with the spread off, entirely on that scan.
    near: &crate::rules::space::Neighbours,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    at_now: f64,
    // WHAT A TESLA ARC IS WORTH ON A NEIGHBOUR'S HEAD — this shot's
    // headshot-damage brackets (`Dot::landing`) — and the stream that decides
    // whether it gets there.
    head_landing: f64,
    arc_landing: &mut Rng,
) {
    if params.others.is_empty() {
        // ONE BODY, so a cloud has nobody to reach and the queue is dropped
        // rather than walked. This is what keeps every single-target fight
        // byte-identical.
        debuffs.area_out.clear();
        debuffs.area_hit.clear();
        others.iter_mut().for_each(|f| {
            f.debuffs.area_out.clear();
            f.debuffs.area_hit.clear();
        });
        return;
    }
    // Collected first: handing a Dot from body i to body j needs both, and one
    // of them may be the aimed body, which is not in `others` at all.
    let mut pending: Vec<(usize, Dot, f64, u16)> = Vec::new();
    for (dot, r, n) in debuffs.area_out.drain(..) {
        pending.push((0, dot, r, n));
    }
    for (i, f) in others.iter_mut().enumerate() {
        for (dot, r, n) in f.debuffs.area_out.drain(..) {
            pending.push((i + 1, dot, r, n));
        }
    }
    // ONE BODY, ONE CORPSE, ONE EXPLOSION PER INSTANT. A cascade terminates in
    // game because the dead stay dead; this arena respawns a training dummy the
    // moment it dies, so without a bound one explosion killing seven bodies
    // produces seven explosions, then forty-nine, then three hundred and
    // forty-three — measured as a 10 GB allocation on an eight-body line.
    //
    // The bound is the game's own sentence rather than a cap somebody picked: a
    // body can only die once at one instant, so it can only explode once. Later
    // instants are later deaths and chain normally.
    let mut exploded = vec![false; others.len() + 1];
    // …AND THE INSTANT ONES: a simultaneous Blast detonation, and the corpse
    // explosion Acid Shells turns a kill into.
    let mut blows: Vec<(usize, AreaHit)> = Vec::new();
    for h in debuffs.area_hit.drain(..) {
        blows.push((0, h));
    }
    for (i, f) in others.iter_mut().enumerate() {
        for h in f.debuffs.area_hit.drain(..) {
            blows.push((i + 1, h));
        }
    }
    for (from, dot, radius_m, count) in pending {
        for j in near.within(from, radius_m) {
            // THE ORIGIN ALREADY HAS IT: the cloud's damage to the body
            // standing in it is the proc itself.
            if j == from {
                continue;
            }
            let (dbf, fp, parts) = if j == 0 {
                (&mut *debuffs, &params.foe, &params.body_parts)
            } else {
                match others.get_mut(j - 1) {
                    Some(f) => {
                        let fs = &params.others[j - 1];
                        (&mut f.debuffs, &fs.params, &fs.body_parts)
                    }
                    None => continue,
                }
            };
            let cap = dot_cap_for(fp, dot.dtype);
            // ONE LANDING PER BODY PER ARC, drawn only where a head pays more.
            let dot = if dot.dtype == DamageType::Electricity
                && head_landing > 1.0
                && parts.iter().any(|p| p.is_head)
                && arc_landing.chance(TESLA_HEAD_LANDING_CHANCE)
            {
                Dot { landing: head_landing, ..dot }
            } else {
                dot
            };
            // NEVER MORE THAN THE RECEIVER CAN HOLD: pushing an eleventh copy
            // of an identical cloud only evicts the first.
            let n = cap.map_or(usize::from(count), |c| usize::from(count).min(c));
            for _ in 0..n {
                dbf.push_dot_capped(dot, cap);
            }
        }
    }
    let status_damage = params.status_duration_multiplier;
    for (from, hit) in blows {
        for (j, centre_gap) in near.within_at(from, hit.radius_m) {
            // THE HOST IS NEVER ONE OF THEM: it took the single-target half and
            // the page excludes it from this one.
            if j == from {
                continue;
            }
            let (state, dbf, fp) = if j == 0 {
                (&mut *target, &mut *debuffs, &params.foe)
            } else {
                match others.get_mut(j - 1) {
                    Some(f) => (&mut f.state, &mut f.debuffs, &params.others[j - 1].params),
                    None => continue,
                }
            };
            // LINEAR TO NOTHING AT THE RIM where the mechanic says so, and
            // flat where it does not — a Blast detonation reaches everyone
            // inside its sphere for the same number.
            let share = if hit.linear_falloff {
                let reach = crate::rules::space::blast_reach(centre_gap);
                (1.0 - reach / hit.radius_m).clamp(0.0, 1.0)
            } else {
                1.0
            };
            if share <= 0.0 {
                continue;
            }
            let dmg = hit.damage * share;
            let mit = dbf.mitigation(at_now, status_damage, params.armor_strip_per_puncture, params.squad.enemy_armor_multiplier);
            let mut breakdown = Breakdown::default();
            let settled = state.apply(
                dmg,
                hit.shares,
                false,
                at_now,
                fp,
                false,
                &mit,
                1.0,
                watching(rec, &mut breakdown),
            );
            let (eff, killed, _broke) = (settled.effective, settled.killed, settled.broken);
            r.sources.add_status(hit.shares.dominant(), eff);
            ledger::settle(
                r, rec, at_now, Combatant::WIELDER, j, hit.shares.dominant(), PopKind::BlastArea,
                &breakdown, settled, Some(dbf),
                ledger::Clock::Dot,
                || Instance {
                    origin: crate::record::Origin::Splash,
                    // THE DETONATION'S OWN NUMBER, and how much of it
                    // reached this far. `share` is the falloff over the
                    // radius, which is the only thing that separates a
                    // neighbour's number from the body that carried the
                    // stack.
                    base: hit.damage,
                    layers: mul_layers(hit.damage, &[(crate::record::Factor::RadialFalloff, share)]),
                    ..Instance::default()
                },
            );
            r.note_kills(u32::from(killed), at_now, params.drop_is_in_reach(target.at));
            if killed {
                // A CHAIN, and it needs no arranging: this body's own corpse
                // explosion is queued here and drained on the next pass, which
                // is the wiki's "chain reaction that can lead to very large
                // areas being cleared from a single shot".
                let acid = params.acid_shells.filter(|_| !exploded[j]);
                exploded[j] = true;
                dbf.on_death(acid, fp);
            }
        }
    }
}

/// THE GAS CLOUD's reach, from that element's own page: 3 m, growing 0.3 m per
/// further proc on the same body, capped at 6 m.
pub(super) const GAS_RADIUS_M: f64 = 3.0;
pub(super) const GAS_RADIUS_STEP_M: f64 = 0.3;
pub(super) const GAS_RADIUS_MAX_M: f64 = 6.0;
/// THE TESLA CHAIN's, and it does not grow: "all enemies in a 3-meter radius".
pub(super) const TESLA_RADIUS_M: f64 = 3.0;

#[allow(clippy::too_many_arguments)]
pub(super) fn settle_procs(
    procs: Vec<DamageType>,
    at: f64,
    scale: InstanceScale,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    target: &mut TargetState,
    params: &FightParams,
    active: &FightParams,
    mit: &Mitigation,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
    // WHICH BODY THIS LANDS ON — `params.foe` until a formation could hold
    // more than one. Its stack caps and its status immunities are
    // its own; the weapon-side half stays on `params`, shared by every body in
    // the fight because the weapon is.
    foe: &Foe,
    // How far this batch of procs is from the hit that started it — see
    // `faction_at`. A hit's own procs are DEPTH_PROC; a proc that Primary
    // Debilitate split out of one is DEPTH_DERIVED_PROC, because it came
    // through an extra damage instance.
    depth: u32,
) {
    let InstanceScale { mb_live, crit_multiplier, part_factor, landing, attrition, xh_bracket } =
        scale;
    let status_damage = params.status_duration_multiplier;
    let sdm = params.status_damage_multiplier;
    let caps = foe.stack_caps;
    let gcap = |base: usize| caps.map_or(base, |c| base.min(c.general));
    let corrosion_cap_bonus = params.squad.cap_bonus("corrosion");
    let stagger_cap = caps.map_or(STAGGER_CAP, |c| STAGGER_CAP.min(c.impact));
    let heat_cap: Option<usize> = caps.map(|c| c.general);
    // PER FAMILY, not one number for all four — see `dot_family_cap`. This read
    // `caps.map(|c| c.general)`, which is None for every published enemy, so
    // the direct path capped nothing and a Gas build stacked past the ten the
    // game allows.
    let dot_cap_of = |dtype: DamageType| -> Option<usize> {
        match (dot_family_cap(dtype), caps.map(|c| c.general)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, u) => u,
        }
    };
    // Elemental DoT tick (data/debuffs): 0.5 × ModifiedBase ×
    // (1 + element bonuses) × (1 + status damage) × crit/part
    // snapshot. Delay-1 DoTs tick at +1..+6 s; delay-0 (Electricity/
    // Gas) at 0..+5 s (the +6 s event is a dud).
    // Signed until the end: a duration under the delay is floor(<0) + 1 = 0
    // ticks, and casting the floor first wrapped it to 0 + 1.
    let delayed_ticks = ((BLEED_TICKS as f64 * status_damage - BLEED_DELAY).floor() + 1.0).max(0.0) as u32;
    // ONE TICK AT LEAST, until the duration is gone: past -100% status duration
    // "all duration/DoT procs are nullified" (MECHANICS §6), Tesla and Gas included.
    let immediate_ticks = if status_damage <= 0.0 {
        0
    } else {
        ((BLEED_TICKS as f64 * status_damage).floor() as u32).max(1)
    };
    // Faction is re-applied at every DERIVATION step — see `faction_at`. A
    // status the hit applied is one step past it, so depth 2 (wiki
    // Faction_Damage_Bonus; MECHANICS §8). This was written `faction_multiplier *
    // faction_multiplier`, which is the same number and says nothing about why.
    // AT `at`, because Roar is in this bracket and Roar ends. A status takes
    // the faction bonus that was running when it was APPLIED — the proc is a
    // snapshot of its instance, which is why `crit_multiplier` is snapshotted here
    // too.
    let fm2 = faction_at(params.faction_at_time(at), depth);
    // ECLIPSE IS SNAPSHOTTED AND FACTION IS NOT, and that is a fact about the
    // BRACKET rather than about where the buff came from — both are abilities
    // with a duration.
    //
    // The wiki draws the same line for a different reason and in so many words:
    // *"Unlike faction damage, which double dips for status effects, the one
    // from Eclipse is applied once."* The FINAL multiplier is not part of what
    // a status inherits; the faction bracket is, twice over. So it is read HERE,
    // at `at`, and lives in the tick's frozen half.
    let ecl = params.ability_final_at(at);
    // WHICH SHOT IS APPLYING THESE. Read once, here, so every DoT this call
    // seeds points back at the same trigger pull — including the ones a
    // detonation or an echo applies, which are still that shot's doing.
    let seeded_by = rec.shot().unwrap_or(u32::MAX);
    // RETURNS THE DOT IT PUSHED, so an AREA proc can post a copy to
    // `DebuffState::area_out` for everybody standing near this body.
    let push_dot = |debuffs: &mut DebuffState,
                    dtype: DamageType,
                    coeff: f64,
                    bracket: f64,
                    delay: f64,
                    ticks: u32,
                    ignores_armor: bool| -> Dot {
        // THE WEAK-POINT MULTIPLIER IS PER STATUS, and Toxin does not take it. The wiki's
        // Toxin page says it does — "Additional Multipliers include ... Enemy
        // Body Parts multipliers" — and a measurement beats the wiki.
        //
        // BLAST IS THE CONTROL AND GOES THE OTHER WAY: the same session
        // measured a 10-stack detonation at 1050 to a neighbour off a BODY hit
        // and 3150 off a HEAD hit, exactly x3.00 (MEASUREMENTS M54). So this is
        // one status's rule and not a family's; the rest keep the wiki's answer
        // until somebody measures them, which is what `dot_takes_weakpoint`
        // exists to make a one-line change.
        let part = if dot_takes_weakpoint(dtype) { part_factor } else { 1.0 };
        let dot = Dot {
                next_tick: at + delay,
                ticks_left: ticks,
                cause: seeded_by,
                // `bracket` is `1 + Σ this element's bonuses`, and an ability
                // that adds this element is "additive with elemental mods" —
                // so its share belongs in the same sum, which is what makes
                // Fireball Frenzy "contribute to DoT" rather than only to the
                // hit.
                // THE INSTANCE'S HALF, and ECLIPSE IS IN IT. The element
                // bracket and the faction bonus are re-read at every tick
                // (`Dot::live`); the final multiplier is not, measured.
                frozen: coeff * mb_live * sdm * crit_multiplier * part * attrition * ecl,
                landing: if lands_on_a_part(dtype) { landing } else { 1.0 },
                // …and what the accumulator's initial 1 is worth: the same
                // chain with the SEED taken out of it. See
                // `Dot::accumulator_unit`.
                unit: coeff * sdm * ecl,
                bracket,
                depth,
                source_scaled: true,
                dtype,
                ignores_armor,
        };
        debuffs.push_dot_capped(dot, dot_cap_of(dtype));
        dot
    };
    // MICROWAVE, landed with this weapon's own procs and never taken off.
    //
    // Tied to a PROC rather than to a hit because the wiki's word is "can proc
    // an invisible Status Effect" — so a build with no status chance gets none,
    // which is the conservative reading. Its DURATION is infinite (Kuva Nukor
    // page), so the only thing this choice can be wrong about is how soon after
    // each respawn it lands: a fraction of a second out of a 180 s engagement.
    // What it is NOT wrong about is the count, which is the whole payload.
    if active.applies_microwave && !procs.is_empty() {
        debuffs.microwave = true;
    }
    for proc in procs {
        r.procs += 1;
        // Read before the match applies it — see the Debilitate hook below.
        let stacks_before = combined_stacks(debuffs, proc);
        match proc {
            DamageType::Impact => DebuffState::push_capped(
                &mut debuffs.stagger,
                at + STAGGER_DURATION * status_damage,
                stagger_cap,
                at,
            ),
            DamageType::Puncture => {
                DebuffState::push_capped(
                    &mut debuffs.weakened,
                    at + WEAKENED_DURATION * status_damage,
                    gcap(WEAKENED_CAP),
                    at,
                );
                // FLENSING SPIKES counts the BULLET, once per body. The pellet
                // counter has already ticked for the hit that carries these
                // procs, so its explosion and any second proc read the same
                // number and are not a second bullet.
                if debuffs.flensed_by != Some(r.pellets) {
                    debuffs.flensed_by = Some(r.pellets);
                    debuffs.flensed += 1;
                }
                // Secondary Cryogenic: each Puncture status applies
                // N Cold stacks to targets around the hit — the
                // single-target arena collapses that onto the main
                // target (the wiki confirms it is included). The
                // Cold procs scale with Status Duration.
                for _ in 0..params.arcane.cold_bursts_on_puncture {
                    debuffs.apply_cold_proc(at, status_damage, target.overguard > 0.0, caps, foe.cannot_be_frozen);
                }
            }
            DamageType::Slash => { push_dot(
                debuffs,
                DamageType::Slash,
                BLEED_COEFFICIENT,
                1.0, // Bleed: elemental mods never scale the ticks
                BLEED_DELAY,
                delayed_ticks,
                true, // Cinematic: ignores armor
            ); }
            DamageType::Toxin => {
                // Primary Blight: each Toxin status THIS WEAPON
                // applies grants one stack to both of its buffs
                // (crit damage + multishot). The weapon-only rule is
                // the arcane's own (wiki), not a sim limitation.
                arc.bump_trigger(&params.arcane.buffs, ArcTrigger::ToxinStatus, at);
                let _ = push_dot(
                    debuffs,
                    DamageType::Toxin,
                    DOT_COEFFICIENT,
                    active.elem_bracket(DamageType::Toxin),
                    1.0,
                    delayed_ticks,
                    false,
                );
            }
            DamageType::Electricity => {
                // Conjunction Voltage: each Electricity status this
                // weapon applies grants one stack to both of its
                // buffs (reload speed + multishot).
                arc.bump_trigger(&params.arcane.buffs, ArcTrigger::ElectricityStatus, at);
                let dot = push_dot(
                    debuffs,
                    DamageType::Electricity,
                    DOT_COEFFICIENT,
                    active.elem_bracket(DamageType::Electricity),
                    0.0,
                    immediate_ticks,
                    false,
                );
                // THE TESLA CHAIN, at a FIXED 3 m — "all enemies in a 3-meter
                // radius". The stun stays here: "only the original target will
                // be stunned ... others around it will only take damage".
                //
                // THE SEED KEEPS THE HIT'S PART; THE LANDING IS THE ARC'S OWN.
                // A head hit's arc reaches a neighbour's body for 130 where a
                // body hit's own tick is 24 — x5.4, the whole head ladder (M100).
                // Where the arc lands is drawn in `drain_area_procs`.
                debuffs.post_area(
                    Dot { landing: 1.0, ..dot },
                    TESLA_RADIUS_M,
                    dot_cap_of(DamageType::Electricity),
                );
            }
            DamageType::Gas => {
                let dot = push_dot(
                    debuffs,
                    DamageType::Gas,
                    DOT_COEFFICIENT,
                    // LITERAL GAS SOURCES ONLY — a Heat or Toxin mod adds
                    // nothing to a Gas tick, which is the wiki's own rule and
                    // the reason Leaded Gas is worth carrying: a bonus for GAS
                    // is one of the only things that ever reaches one, and it
                    // arrives through `element_at` at every tick.
                    1.0,
                    0.0,
                    immediate_ticks,
                    false,
                );
                // THE CLOUD, and its radius GROWS: "subsequent procs increase
                // the radius by 0.3 meters up to 6 meters". Counted BEFORE this
                // proc was applied, which is what makes the first cloud 3 m
                // rather than 3.3.
                let radius_m = (GAS_RADIUS_M + GAS_RADIUS_STEP_M * stacks_before as f64)
                    .min(GAS_RADIUS_MAX_M);
                // BODY-ONLY on a neighbour, seed and landing both: the cloud's
                // neighbours are unmeasured, and M100's arc is not a cloud.
                debuffs.post_area(
                    Dot { frozen: dot.frozen / part_factor.max(1e-9), landing: 1.0, ..dot },
                    radius_m,
                    dot_cap_of(DamageType::Gas),
                );
            }
            DamageType::Heat => {
                // Singleton accumulator: add the contribution and
                // refresh the shared clock; ticks stay anchored to
                // the first proc (ignite.yaml).
                // Cascadia Flare: each applied Heat status grants
                // one stack and refreshes the shared timer (own
                // procs only — the "any source" clause waits on a
                // multi-actor world).
                arc.bump_trigger(&params.arcane.buffs, ArcTrigger::HeatStatus, at);
                // THE INSTANCE'S HALF ONLY, like a `Dot`'s `frozen`: the
                // element bracket, the faction bonus and Eclipse are the
                // SOURCE's and are re-read when the entity pays. A Heat burn is
                // the commonest DoT in the game and the one a Lavos or a Bane
                // arriving mid-fight is most often meant to strengthen.
                let contrib = DOT_COEFFICIENT
                    * mb_live
                    * sdm
                    * crit_multiplier
                    // THE WEAK POINT, AND IT IS MEASURED HERE rather than taken
                    // from `dot_takes_weakpoint`, which this arm never asks:
                    // a Braton Prime at base 35 with +200% Heat popped 54 on the
                    // body and 159 on the head (M90). The accumulator's own 1 is
                    // outside this product, which is why 159 and not 162.
                    * part_factor
                    // …and Eclipse, applied ONCE at the proc. See `Dot::live`.
                    * ecl;
                let expiry = at + STATUS_DURATION * status_damage;
                debuffs.apply_heat(
                    at,
                    contrib,
                    expiry,
                    heat_cap,
                    HeatOrigin {
                        bracket: active.elem_bracket(DamageType::Heat),
                        depth,
                        // The seed taken out of `contrib`, exactly as a Dot's.
                        unit: DOT_COEFFICIENT * sdm * ecl,
                    },
                );
            }
            DamageType::Cold => {
                // Primary Frostbite: each Cold status THIS WEAPON APPLIES
                // grants one stack to both of its buffs (crit damage +
                // multishot). "Applies" is the whole of it: the bump runs AFTER
                // the proc and only when one exists, so a Cold proc landing on
                // a FROZEN target grants nothing — `frozen.yaml` says such a
                // proc is inert. Frozen lasts 3 s against a 12 s buff, so
                // getting this wrong never shows as the arcane going dark, only
                // as it never quite decaying.
                if debuffs.apply_cold_proc(at, status_damage, target.overguard > 0.0, caps, foe.cannot_be_frozen) {
                    arc.bump_trigger(&params.arcane.buffs, ArcTrigger::ColdStatus, at);
                }
            }
            DamageType::Magnetic => DebuffState::push_capped(
                &mut debuffs.disrupt,
                at + STATUS_DURATION * status_damage,
                gcap(TEN_STACK_CAP),
                at,
            ),
            DamageType::Viral => DebuffState::push_capped(
                &mut debuffs.virus,
                at + STATUS_DURATION * status_damage,
                gcap(TEN_STACK_CAP),
                at,
            ),
            DamageType::Corrosive => DebuffState::push_capped(
                &mut debuffs.corrosion,
                at + CORROSION_DURATION * status_damage,
                // THE ONE CEILING A PLAYER CAN MOVE. An Emerald Archon Shard is
                // "+2 (+3) max stacks of Corrosion", and its page is explicit
                // that a WEAPON's corrosion may exceed ten because of it. Five
                // Tauforged sockets are +15, and `corrosive_strip` is
                // `0.20 + 0.06 x stacks` capped at 1.0 — so this is the
                // difference between stripping 80% of armour and all of it.
                //
                // `gcap` still mins with the ENEMY's own cap, and that is
                // right: "No Status Effect will exceed a maximum of 4 stacks"
                // is a hard rule on the Acolytes, not a bonus to out-bid.
                gcap(TEN_STACK_CAP + corrosion_cap_bonus),
                at,
            ),
            DamageType::Radiation => DebuffState::push_capped(
                &mut debuffs.confusion,
                at + STATUS_DURATION * status_damage,
                gcap(TEN_STACK_CAP),
                at,
            ),
            // The Bullet Attractor, which only ever arrives from an EXTRA HIT
            // here: no weapon in the roster deals Void, and Xata's Whisper's
            // second instance is entirely Void. Worth one line in the CO
            // counter and nothing else — see `DebuffState::attractor`.
            DamageType::Void => DebuffState::push_capped(
                &mut debuffs.attractor,
                at + ATTRACTOR_DURATION * status_damage,
                1,
                at,
            ),
            DamageType::Blast => {
                if let Some(c) = caps {
                    if debuffs.blast.len() >= c.general {
                        debuffs.blast.remove(0); // FIFO replace-oldest
                    }
                }
                debuffs.blast.push(BlastStack {
                    fuse: at + BLAST_FUSE * status_damage,
                    value: BLAST_COEFFICIENT
                        * mb_live
                        * sdm
                        * crit_multiplier
                        * part_factor
                        * fm2,
                    xh_bracket,
                });
                if debuffs.blast.len() >= TEN_STACK_CAP {
                    // Early detonation: every stack's single-target
                    // hit at once, all stacks consumed (radial
                    // excluded — it never hits the host).
                    let fired: Vec<BlastStack> = debuffs.blast.drain(..).collect();
                    let total: f64 = fired.iter().map(|b| b.value).sum();
                    // …AND THE OTHER HALF, which is the bigger one: a
                    // SIMULTANEOUS detonation reaches 5 m at ten times the
                    // per-stack value the host takes. The host is excluded by
                    // the page's own words — "the initial target of the blast
                    // procs is not dealt this AoE damage" — and by the drain,
                    // which never hands a body its own.
                    debuffs.area_hit.push(AreaHit {
                        damage: total / BLAST_COEFFICIENT * BLAST_AOE_COEFFICIENT,
                        radius_m: BLAST_AOE_RADIUS_M,
                        shares: TypeShares::single(DamageType::Blast),
                        linear_falloff: false,
                    });
                    // …and each stack's OWN extra-hit contribution, pre-scaled
                    // by the bracket the gun that applied it had. Ten stacks
                    // land as one number here, but they are ten detonations and
                    // an expiring Nourish between the first and the tenth would
                    // make their brackets differ.
                    let xh_total: f64 = fired.iter().map(|b| b.value * b.xh_bracket).sum();
                    let mit = debuffs.mitigation(
                        at,
                        status_damage,
                        params.armor_strip_per_puncture,
                        params.squad.enemy_armor_multiplier,
                    );
                    // TEN NUMBERS, NOT ONE, and the total is the same either
                    // way — what differs is the INSTANCE, which is the unit
                    // attenuation clamps, a shield gate multiplies and overkill
                    // is measured against. Measured: the host pops ten numbers
                    // and the bodies around it pop one (MEASUREMENTS M91).
                    //
                    // IT STOPS AT THE KILL. This arena replaces a dead body at
                    // once, so paying the eleventh stack into a target that has
                    // already fallen would land it on a FRESH one — a number the
                    // game never dealt, to a body that was not there.
                    let mut killed = false;
                    let mut broke_any = None;
                    for b in &fired {
                        let mut breakdown = Breakdown::default();
                        let settled = target.apply(
                            b.value,
                            TypeShares::single(DamageType::Blast),
                            false,
                            at,
                            foe,
                            false,
                            &mit,
                            1.0,
                            watching(rec, &mut breakdown),
                        );
                        let (eff, k, broke) = (settled.effective, settled.killed, settled.broken);
                        r.sources.add_status(DamageType::Blast, eff);
                        let stack = b.value;
                        ledger::settle(
                            r, rec, at, Combatant::WIELDER, 0, DamageType::Blast, PopKind::Blast,
                            &breakdown, settled, Some(debuffs),
                            ledger::Clock::Dot,
                            || Instance {
                                origin: crate::record::Origin::Status,
                                // ONE STACK'S OWN NUMBER. The pile is ten of
                                // these at one instant, which is what the game
                                // draws and what an attenuated target feels.
                                base: stack,
                                layers: Vec::new(),
                                ..Instance::default()
                            },
                        );
                        if broke.is_some() {
                            broke_any = broke;
                        }
                        if k {
                            killed = true;
                            break;
                        }
                    }
                    // ONE MOMENT, ONE KILL, ONE BREAK — however many numbers
                    // shared it. The same rule M76 states for what an arcane
                    // counting hits sees.
                    r.note_kills(killed as u32, at, params.drop_is_in_reach(target.at));
                    if let Some(pool) = broke_any {
                        push_break_proc(debuffs, params, at, pool);
                    }
                    if killed {
                        gal.bump_on_kill(params, at);
                        arc.on_kill(params, at);
                        debuffs.on_death(params.acid_shells, &params.foe);
                    } else {
                        // THE ONE STATUS PAYLOAD THAT TRIGGERS AN EXTRA HIT.
                        // The bracket is already folded into `xh_total`, and no
                        // body part is re-applied — a detonation struck none.
                        fire_extra_hits(
                            xh_total,
                            1.0,
                            1.0,
                            false,
                            active.status_chance,
                            at,
                            debuffs,
                            gal,
                            arc,
                            target,
                            params,
                            active,
                            &mit,
                            r,
                            rec,
                            rng,
                        );
                    }
                }
            }
            _ => {}
        }
        // PRIMARY DEBILITATE: a saturated combined status splits into one of
        // its components. `stacks_before` is read BEFORE the match applied this
        // proc, so `+ 1` is the count the target is AT — which is the count the
        // threshold is about.
        //
        // RECURSION is how the faction ladder stays compositional: the split
        // proc is settled by this same function one DEPTH deeper, so its DoT
        // carries the bonus a third time without anyone writing a 3. It cannot
        // recurse further — a component is a primary, and `components_of`
        // answers None for those — so the ladder ends where the game's does.
        if params.arcane.debilitate_chance > 0.0 {
            // THE TENTH APPLICATION IS THE TRIGGER, for every combination —
            // "at nine, the next shot that makes it ten fires it, rather than
            // having to reach ten and shoot again". So the
            // count that goes in is the one the target is AT, and there is no
            // per-element branch: this line carried an `if proc == Blast` for
            // two days because Blast is where the off-by-one was VISIBLE
            // (detonating at ten, it never sits there, so the arcane was
            // silently dead on it) — and the fix for the visible case was the
            // rule all along. MEASUREMENTS M34.
            if let Some(part) =
                debilitate_split(proc, stacks_before + 1, params.arcane.debilitate_chance, rng)
            {
                // THE SPLIT PROC IS AN ORDINARY PROC: same match, same
                // `push_dot`, its OWN element bracket. Only the DEPTH differs,
                // so the "separate damage instance" the wiki names is the
                // status APPLICATION rather than a second number, and the extra
                // faction layer is the ×f³ the page reports.
                //
                // THE BASE IS THE WEAPON'S — `mb_live` is `ModifiedBase`, which
                // EXCLUDES the elemental portions, DE's rule for a status a
                // WEAPON's hit applied. The exponent is what is open:
                // MEASUREMENTS M33.
                settle_procs(
                    vec![part],
                    at,
                    InstanceScale {
                        // THE 0% MEMBER OF THE EXTRA HIT CATEGORY. This arcane
                        // "adds a 0-damage Extra Hit that applies a guaranteed
                        // status effect" (wiki, Extra_Hit), so the base its
                        // status burns off is the one nothing replaced — the
                        // level above, which is this instance's own
                        // ModifiedBase. Same rule the ability members read from
                        // the other direction; docs/EXTRA_HIT.md.
                        mb_live: extra_hit_status_base(0.0, mb_live),
                        crit_multiplier,
                        part_factor,
                        landing,
                        // A SECOND ATTRITION ROLL ON TOP OF THE HIT'S, and
                        // a BUG of DE's rather than a design: one instance's
                        // multipliers left on another instance's magnitude.
                        // MEASUREMENTS M37 derives it.
                        //
                        // IT ROLLS EVEN WHEN THE HIT CRIT — the zero instance
                        // has no crit of its own, so "on a hit that is not
                        // critical" holds whatever the parent did. The literal
                        // `0` is the CLAIM: permanently non-critical, not a
                        // zero rolling crit.
                        attrition: attrition * noncrit_mult(active.noncrit_bonus, 0, rng),
                        // Inherited unchanged: the split is the same weapon's,
                        // so a Blast it splits out detonates behind the same
                        // bracket the parent's would have.
                        xh_bracket,
                    },
                    debuffs,
                    gal,
                    arc,
                    target,
                    params,
                    active,
                    mit,
                    r,
                    rec,
                    rng,
                    &params.foe,
                    DEPTH_DERIVED_PROC,
                );
            }
        }
        // Cascadia Empowered: each applied status adds an EXTRA
        // FLAT damage instance of the proc's type — unaffected by
        // damage/element/crit mods, Galvanized stacks, parts, or
        // falloff; faction bonuses apply ONCE; enemy mitigation
        // still applies (wiki notes) — which now includes the
        // vulnerability column, since the instance IS of that type:
        // Toxin instances keep Toxin's shield bypass and Toxin's
        // column factor alike.
        if params.arcane.flat_damage_on_status > 0.0 {
            let amt = params.arcane.flat_damage_on_status * params.faction_at_time(at);
            let mut breakdown = Breakdown::default();
            let settled = target.apply(
                amt,
                TypeShares::single(proc),
                false,
                at,
                foe,
                false,
                mit,
                1.0,
                watching(rec, &mut breakdown),
            );
            let (eff, killed, broke) = (settled.effective, settled.killed, settled.broken);
            r.sources.arcane_on_status += eff;
            r.sources.arcane_by_type[proc as usize] += eff;
            ledger::settle(
                r, rec, at, Combatant::WIELDER, 0, proc, PopKind::Arcane, &breakdown, settled,
                Some(debuffs),
                ledger::Clock::Hit,
                || Instance {
                    origin: crate::record::Origin::Arcane,
                    // A FLAT NUMBER AND ONE MULTIPLIER, which is the whole
                    // of this arcane: "unaffected by damage/element/crit
                    // mods … faction bonuses apply ONCE".
                    base: params.arcane.flat_damage_on_status,
                    layers: mul_layers(params.arcane.flat_damage_on_status, &faction_layers(params, at, DEPTH_HIT)),
                    ..Instance::default()
                },
            );
            r.note_kills(killed as u32, at, params.drop_is_in_reach(target.at));
            if let Some(pool) = broke {
                push_break_proc(debuffs, params, at, pool);
            }
            if killed {
                gal.bump_on_kill(params, at);
                arc.on_kill(params, at);
                debuffs.on_death(params.acid_shells, &params.foe);
            }
        }
    }
}

/// EVERY PROC THIS INSTANCE FORCES, in one buffer: the weapon's, the radial's,
/// the swing's (a stance marks them per attack) and any a Warframe ability
/// adds. Returns how many of `buf` are filled; a type is never forced twice,
/// because `procs_for_hit` copies the list through and the game applies one.
#[allow(clippy::too_many_arguments)]
pub(super) fn forced_procs(
    active: &FightParams,
    rad: &Option<crate::build::loadout::ResolvedRadial>,
    direct: bool,
    swing: &Option<crate::model::ComboHit>,
    swing_forced_types: &[DamageType],
    ability_forced: &[DamageType],
    pellet_idx: u32,
    buf: &mut [DamageType; 17],
) -> usize {
        let mut n = match (&rad, direct) {
            (None, true) => {
                for (i, ty) in active.forced_procs.iter().enumerate() {
                    buf[i] = *ty;
                }
                active.forced_procs.len()
            }
            (Some(r), _) => r.forced_procs.fill(buf),
            _ => 0,
        };
        // …AND THE SWING'S OWN, on a DIRECT melee hit. A stance
        // marks them per attack — Crushing Ruin's first swing
        // forces Impact and its last forces Knockdown — so they
        // belong to the swing rather than to the weapon, which is
        // why `active.forced_procs` above cannot carry them. `forced_hits`
        // is how many of the row's hits carry them, when not all do.
        let swing_forces = direct
            && swing.as_ref().and_then(|h| h.forced_hits).is_none_or(|k| pellet_idx < k);
        for ty in swing_forced_types.iter().filter(|_| swing_forces) {
            if !buf[..n].contains(ty) && n < buf.len() {
                buf[n] = *ty;
                n += 1;
            }
        }
        for ty in ability_forced {
            // A weapon that already forces this element does not
            // force it twice: `procs_for_hit` copies the list
            // through, and a duplicate would be a second proc the
            // game does not apply.
            if !buf[..n].contains(ty) && n < buf.len() {
                buf[n] = *ty;
                n += 1;
            }
        }
        n
}

/// HEMORRHAGE'S ROLL: a proc the instance already applied can bring another
/// with it, at a chance the card doubles below a fire-rate threshold. The
/// converted type is never added twice.
pub(super) fn roll_proc_conversion(
    active: &FightParams,
    d: &mut crate::rules::rng::Draws,
    live_rate: f64,
    procs: &mut Vec<DamageType>,
) {
    if let Some(pc) = active.proc_conversion {
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
}
