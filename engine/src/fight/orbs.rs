use crate::rules::math::Portable;
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
#[wfsim_gas_macro::metered]
pub(super) fn throw_orb(
    o: crate::build::loadout::ResolvedOrb,
    // Whose orb it is, for the same reason a field carries it.
    owner: Seat,
    params: &FightParams,
    t: f64,
    live: &mut Vec<Entity>,
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
    live.retain(|e| !matches!(e, Entity::Orb(_)));
    let aim = params.aim_point();
    let muzzle = crate::rules::space::muzzle(params.player_at, aim);
    let (dx, dy) = (aim.x - muzzle.x, aim.y - muzzle.y);
    let len = dx.hypot_portable(dy);
    let dir = if len > 0.0 {
        crate::rules::space::Vec2::new(dx / len, dy / len)
    } else {
        crate::rules::space::Vec2::new(1.0, 0.0)
    };
    // …AND IT LEAVES AFTER THE WIND-UP, not on the trigger pull. `t` is when
    // you pressed; the orb exists `throw_seconds` later and its strike clock
    // starts there.
    let t = t + o.throw_seconds;
    live.push(Entity::Orb(OrbState {
        owner,
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
    }));
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
    /// WHOSE ORB IT IS — the same rule a field follows: it outlives the throw.
    pub(super) owner: Seat,
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

#[wfsim_gas_macro::metered]
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

/// ONE STRIKE, on body `b` — 0 is the aimed one and the rest index the
/// formation, the numbering everything else here uses.
///
/// It is [`field_tick`] with the body chosen for it. Sharing that function is
/// what stops a cloud's tick and an orb's strike drifting apart on the
/// arithmetic; WHICH body is the part an orb decides for itself.
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn orb_strike(
    // See `process_ticks`.
    w: &CardWindows,
    orb: &OrbState,
    share: f64,
    at: f64,
    ctx: &FieldCtx,
    b: usize,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    params: &FightParams,
    active: &FightParams,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    bodies: &mut [Body],
) -> bool {
    let Some(mut part) = active.orb_strike else { return false };
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
    let (Some(spec), Some(here)) = (params.body(b), bodies.get_mut(b)) else { return false };
    field_tick(
        w,
        orb.owner,
        &part, mult, at, ctx, here, b, gal, arc, params, active, r, rec, d,
        spec.params, crate::record::Origin::Orb, orb.part.unaimed_headshot_chance, false,
    )
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
#[wfsim_gas_macro::metered]
pub(super) fn orb_detonation(
    // See `process_ticks`.
    w: &CardWindows,
    orb: &OrbState,
    at: f64,
    ctx: &FieldCtx,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    params: &FightParams,
    active: &FightParams,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    bodies: &mut [Body],
    body_at: &[crate::rules::space::Vec2],
) {
    let Some(part) = active.orb_blast else { return };
    for (b, &pos) in body_at.iter().enumerate() {
        let dist = pos.distance(orb.at);
        if !crate::rules::space::caught_by_blast(dist, part.radius_m) {
            continue;
        }
        // LINEAR FALLOFF MEASURED FROM THE ORB rather than from a body.
        let mult = orb.damage_multiplier * part.falloff_at(crate::rules::space::blast_reach(dist));
        if let (Some(spec), Some(here)) = (params.body(b), bodies.get_mut(b)) {
            field_tick(
                w,
                orb.owner,
                &part, mult, at, ctx, here, b, gal, arc, params, active, r, rec, d,
                spec.params, crate::record::Origin::Orb, None, true,
            );
        }
    }
}
