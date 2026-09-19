use super::*;

/// PUT AN ORB IN THE AIR at `t`, from the muzzle, along the aim.
///
/// ONE PLACE, because there are two ways to throw one and they must not differ:
/// a TRIGGER pull throws it in the form's own `transformed` mode, and a full
/// RECHARGE METER throws it in the cycle that is how the weapon is really
/// played. Where it starts and how it flies is the same question both times,
/// and the first version of the meter answered it by copying thirty lines.
///
/// IT LEAVES THE MUZZLE, which is a point on the shooter's own circumference —
/// the same place every other shot in this arena leaves from, and the reason
/// the orb's reach is measured from somewhere real rather than from the target. A degenerate aim (nowhere to face) throws along +x.
pub(super) fn throw_orb(
    o: crate::build::loadout::ResolvedOrb,
    params: &FightParams,
    t: f64,
    live: &mut Vec<OrbState>,
) {
    // ONE ORB AT A TIME. Throw again while the previous orb is alive and the one
    // already out vanishes at once, with no detonation and no strikes it had
    // left.
    //
    // It matters most where you would least expect it: in the CYCLE the meter
    // puts throws tens of seconds apart and nothing ever overlaps, so this is
    // free. In the `transformed` mode — the form held, throwing every second —
    // it is the difference between thirty orbs living out six-second fuses on
    // top of each other and one orb that gets a single strike before the next
    // one replaces it.
    live.clear();
    let aim = params.aim_point();
    let muzzle = crate::rules::space::muzzle(params.player_at, aim);
    let (dx, dy) = (aim.x - muzzle.x, aim.y - muzzle.y);
    let len = dx.hypot(dy);
    let dir = if len > 0.0 {
        crate::rules::space::Vec2::new(dx / len, dy / len)
    } else {
        crate::rules::space::Vec2::new(1.0, 0.0)
    };
    // …AND IT LEAVES AFTER THE WIND-UP, not on the trigger pull. `t` is when
    // you pressed; the orb exists `throw_seconds` later and its strike clock
    // starts there.
    let t = t + o.throw_seconds;
    live.push(OrbState {
        part: o,
        at: muzzle,
        dir,
        at_time: t,
        contacted: false,
        // THE FIRST STRIKE IS AT THE THROW, and the clock runs from there
        // rather than from a contact. A tick with nobody in reach is spent
        // striking nobody, which is what turns "six strikes" into the owner's
        // `ceil(6 - flight)` without anything having to know about flights.
        next_strike: t,
        strikes_left: (o.fuse_seconds / o.strike_interval_seconds).round() as u32,
        fuse_at: t + o.fuse_seconds,
        damage_multiplier: 1.0,
    });
}

/// AN ORB IN THE AIR — a position, a heading, a clock and what is left of a
/// fuse.
///
/// IT IS NOT A `FieldState`, and the two are kept apart on purpose. A FIELD is an AREA: it sits where it landed and burns everyone
/// standing in it, at each body's own falloff distance. This is an ENTITY: it
/// has a place of its own, it MOVES, and each of its strikes reaches exactly
/// ONE body inside its reach, all six for the same number. They share the
/// arithmetic of settling a damage instance and nothing else.
#[derive(Debug, Clone, Copy)]
pub(super) struct OrbState {
    pub(super) part: crate::build::loadout::ResolvedOrb,
    /// Where it is. Advanced to each event's time as that event is settled,
    /// which is all the resolution this needs — nothing between two strikes
    /// asks where it is.
    pub(super) at: crate::rules::space::Vec2,
    /// Unit heading, fixed at the throw: touching a body slows it and does not
    /// turn it.
    pub(super) dir: crate::rules::space::Vec2,
    /// The clock `at` was last advanced to.
    pub(super) at_time: f64,
    /// Has it touched a body yet? The one thing that changes its speed.
    pub(super) contacted: bool,
    pub(super) next_strike: f64,
    pub(super) strikes_left: u32,
    /// When it detonates — the last event of its life and the only one that is
    /// not a strike.
    pub(super) fuse_at: f64,
    /// Plentiful Mayhem's independent multiplier, carried from the shot that
    /// threw it, exactly as a field carries it.
    pub(super) damage_multiplier: f64,
}

impl OrbState {
    /// MOVE IT TO `to` at whichever speed it is travelling, noticing a contact
    /// on the way.
    ///
    /// The speed change is a STEP at the first body it touches, so a leg
    /// spanning that contact is walked in two: up to the body at the launch
    /// speed, onward at the slowed one. Covering it in one piece at one speed
    /// would put the orb somewhere it never was, which is the one thing a
    /// position model exists to get right.
    /// This orb `metres` further along its heading. `dir` is a unit vector, so
    /// there is nothing to normalise.
    pub(super) fn step(&self, metres: f64) -> crate::rules::space::Vec2 {
        crate::rules::space::Vec2::new(
            self.at.x + self.dir.x * metres,
            self.at.y + self.dir.y * metres,
        )
    }

    pub(super) fn advance(&mut self, to: f64, bodies: &[crate::rules::space::Vec2]) {
        while self.at_time < to - 1e-12 {
            let speed = if self.contacted {
                self.part.speed_after_contact_mps
            } else {
                self.part.launch_speed_mps
            };
            if speed <= 0.0 {
                self.at_time = to;
                return;
            }
            let reach = speed * (to - self.at_time);
            let hit = if self.contacted {
                None
            } else {
                crate::rules::space::first_hit(self.at, self.dir, bodies).filter(|(_, d)| *d <= reach + 1e-9)
            };
            match hit {
                Some((_, d)) => {
                    self.at = self.step(d);
                    self.at_time += d / speed;
                    self.contacted = true;
                }
                None => {
                    self.at = self.step(reach);
                    self.at_time = to;
                }
            }
        }
    }
}

/// SETTLE EVERY ORB EVENT DUE STRICTLY BEFORE `until`, oldest first.
///
/// The same shape as [`process_field_ticks`] and for the same reason: an event
/// changes what the next one sees, so the order is resolved live rather than
/// planned. Everything that differs is about WHO —
///
///   * the orb is MOVED to the event's time first, because where it is decides
///     who is a candidate at all
///   * a STRIKE takes ONE body inside the reach, at random — *"Orb will shock 1
///     enemy within 6 meters of it every 1 second"* — and chains from it
///   * the last event is the DETONATION: the attack's own explosion, fired
///     from wherever the orb had got to
///
/// A TICK WITH NOBODY IN REACH STRIKES NOBODY AND IS SPENT, which is what makes
/// the measured count fall out of the geometry rather than being written down.
/// Six ticks over a six second fuse: a throw that connects in under a second
/// loses none of them, one that takes 2.5 s lands four, and against a body at
/// contact it is always six — the owner's `ceil(6 - flight)` arriving on its
/// own (MEASUREMENTS M63).
#[allow(clippy::too_many_arguments)]
pub(super) fn process_orbs(
    orbs: &mut Vec<OrbState>,
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
    d: &mut crate::rules::rng::Draws,
    others: &mut [SpreadFoe],
) {
    if orbs.is_empty() {
        return;
    }
    // WHERE EVERY BODY STANDS, the aimed one first — the same numbering
    // `RunResult::damage_by_body` uses, so an index here is an index there.
    let mut bodies = Vec::with_capacity(params.others.len() + 1);
    bodies.push(params.target_at);
    bodies.extend(params.others.iter().map(|f| f.at));

    while let Some((i, at, is_strike)) = orbs
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let strike_due = o.strikes_left > 0 && o.next_strike <= o.fuse_at;
            let next = if strike_due { o.next_strike } else { o.fuse_at };
            (next < until).then_some((i, next, strike_due))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        // Status events strictly before this one land first, exactly as they do
        // before a field tick.
        process_ticks(
            debuffs, gal, arc, at + 1e-9, target, params, ap, r, rec, &mut d.status,
            &params.target, 0,
        );
        orbs[i].advance(at, &bodies);
        let orb = orbs[i];
        if !is_strike {
            orbs.remove(i);
            orb_detonation(&orb, at, ctx, debuffs, gal, arc, target, params, ap, r, rec, d, others, &bodies);
            continue;
        }
        orbs[i].next_strike += orb.part.strike_interval_seconds;
        orbs[i].strikes_left -= 1;

        // WHO IS IN REACH. Any part of a body touching is enough — the rule
        // every sphere in this engine uses.
        let in_reach: Vec<usize> = (0..bodies.len())
            .filter(|&b| {
                crate::rules::space::caught_by_blast(bodies[b].distance(orb.at), orb.part.strike_radius_m)
            })
            .collect();
        if in_reach.is_empty() {
            continue;
        }
        // ONE OF THEM, AT RANDOM. The page says the orb picks and does not say
        // how; a uniform draw is the only reading that invents no preference.
        let pick = ((d.spine.next_f64() * in_reach.len() as f64) as usize).min(in_reach.len() - 1);
        let seed = in_reach[pick];

        // …AND THE CHAIN OFF IT. `chain_bodies` is the TOTAL a strike reaches,
        // the struck body included, with multishot already inside it — which is
        // what *"Number of chains is affected by Multishot"* buys on this
        // weapon instead of a second orb.
        //
        // NO DRAW. The count is `floor(3 x multishot)` and it is exact, so a
        // strike reaches the same number of bodies every time (M63).
        let reached = orb.part.chain_bodies.max(1) as usize;
        let mut path = vec![seed];
        if reached > 1 {
            let mut rest: Vec<usize> = in_reach.into_iter().filter(|&b| b != seed).collect();
            // Nearest first from wherever the path has got to, ties by index —
            // the same walk `rules::chain::Layout` does, and nobody twice.
            while path.len() < reached {
                let from = *path.last().expect("seeded");
                let Some((k, _)) = rest
                    .iter()
                    .enumerate()
                    .map(|(k, &b)| (k, bodies[from].distance(bodies[b])))
                    .filter(|(_, dist)| *dist <= orb.part.chain_range_m + 1e-9)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                else {
                    break;
                };
                path.push(rest.remove(k));
            }
        }

        let mut aimed_died = false;
        let mut share = 1.0;
        for (hop, &b) in path.iter().enumerate() {
            if hop > 0 {
                share *= orb.part.chain_damage_per_hop;
            }
            let killed = orb_strike(
                &orb, share, at, ctx, b, debuffs, gal, arc, target, params, ap, r, rec, d, others,
            );
            aimed_died |= killed && b == 0;
        }
        if aimed_died {
            debuffs.on_death(params.acid_shells, &params.target);
            return;
        }
    }
}

/// ONE STRIKE, on body `b` — 0 is the aimed one and the rest index the
/// formation, the numbering everything else here uses.
///
/// It is [`field_tick`] with the body chosen for it. Sharing that function is
/// what stops a cloud's tick and an orb's strike drifting apart on the
/// arithmetic; WHICH body is the part an orb decides for itself.
#[allow(clippy::too_many_arguments)]
pub(super) fn orb_strike(
    orb: &OrbState,
    share: f64,
    at: f64,
    ctx: &FieldCtx,
    b: usize,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    target: &mut TargetState,
    params: &FightParams,
    ap: &FightParams,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    others: &mut [SpreadFoe],
) -> bool {
    let Some(mut part) = ap.orb_strike else { return false };
    // A CHAINED BODY TAKES A SMALLER STRIKE, and "smaller" means a smaller BASE
    // — not a multiplier on the finished number. `rules::chain::Instance::share` is
    // explicit about which: *"a beam with a smaller base damage, so it scales
    // the hit AND the status base that hit computes its DoTs from"*, and the
    // hop's own page states its fraction of the beam rather than of the hit.
    //
    // Riding `damage_multiplier` instead was wrong and measurably so: that
    // bracket is Plentiful Mayhem's and is documented as leaving the status
    // payload out, so a hop at 0.31 of the strike still seeded a full-size
    // Electricity DoT — 4.6% off the crowd number where it should have been a
    // third of it.
    if share != 1.0 {
        part.damage = part.damage.scale(share);
        part.modified_base *= share;
    }
    let mult = orb.damage_multiplier;
    match b.checked_sub(1) {
        None => field_tick(
            &part, mult, at, ctx, debuffs, gal, arc, target, params, ap, r, rec, d,
            &params.target, crate::record::Origin::Orb, orb.part.unaimed_headshot_chance, false,
        ),
        Some(bi) => {
            let Some(spec) = params.others.get(bi) else { return false };
            let Some(SpreadFoe { state, debuffs: fd }) = others.get_mut(bi) else { return false };
            field_tick(
                &part, mult, at, ctx, fd, gal, arc, state, params, ap, r, rec, d,
                &spec.params, crate::record::Origin::Orb, orb.part.unaimed_headshot_chance, false,
            )
        }
    }
}

/// THE FUSE RUNNING OUT — the attack's own explosion, fired from wherever the
/// orb had drifted to.
///
/// It is the ordinary radial with one thing changed, and that one thing is the
/// point of the whole entity: its CENTRE. Every other explosion in this engine
/// goes off AT a body, so where it goes off never had to be said; this one goes
/// off at a place, which may be past the body it touched and may be nowhere
/// near anybody.
#[allow(clippy::too_many_arguments)]
pub(super) fn orb_detonation(
    orb: &OrbState,
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
    others: &mut [SpreadFoe],
    bodies: &[crate::rules::space::Vec2],
) {
    let Some(part) = ap.orb_blast else { return };
    for (b, &pos) in bodies.iter().enumerate() {
        let dist = pos.distance(orb.at);
        if !crate::rules::space::caught_by_blast(dist, part.radius_m) {
            continue;
        }
        // LINEAR FALLOFF MEASURED FROM THE ORB rather than from a body.
        let mult = orb.damage_multiplier * part.falloff_at(crate::rules::space::blast_reach(dist));
        match b.checked_sub(1) {
            None => {
                field_tick(
                    &part, mult, at, ctx, debuffs, gal, arc, target, params, ap, r, rec, d,
                    &params.target, crate::record::Origin::Orb, None, true,
                );
            }
            Some(bi) => {
                if let (Some(spec), Some(SpreadFoe { state, debuffs: fd })) =
                    (params.others.get(bi), others.get_mut(bi))
                {
                    field_tick(
                        &part, mult, at, ctx, fd, gal, arc, state, params, ap, r, rec, d,
                        &spec.params, crate::record::Origin::Orb, None, true,
                    );
                }
            }
        }
    }
}
