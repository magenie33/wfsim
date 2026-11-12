use super::*;

/// Live on-kill stack state (Galvanized graceful decay: on timeout lose
/// ONE stack and reset the duration for the remainder).
#[derive(Default)]
pub(super) struct LiveStacks {
    pub(super) stacks: u32,
    pub(super) expiry: f64,
    /// [`BuffDecay::PerStackExpiry`] only: one expiry per live stack, oldest
    /// first. Empty for the Galvanized family, which shares a single clock.
    pub(super) each: Vec<f64>,
    pub(super) per_stack: bool,
    /// [`BuffDecay::AllAtOnce`]: the shared clock takes the WHOLE pile when it
    /// falls due, instead of shedding one stack and restarting for the rest.
    pub(super) all_at_once: bool,
    /// WHEN EACH STACK FELL, kept only for a pile that asked (`logging_shed`):
    /// the initial-combo floor has to know the instant it stepped down, and
    /// any reader at a later time would otherwise have taken that instant.
    pub(super) shed_log: bool,
    pub(super) shed: Vec<(f64, u32)>,
}

#[wfsim_gas_macro::metered]
impl LiveStacks {
    /// Apply pending decay and return the current stack count. An INFINITE
    /// expiry never falls due, which is the whole of what a locked buff is.
    pub(super) fn current(&mut self, now: f64, duration: f64) -> u32 {
        if self.per_stack {
            // Each stack on its own clock: drop every one that has fallen due,
            // which is FIFO because they were pushed in time order.
            if self.shed_log {
                self.shed.extend(self.each.iter().filter(|&&e| e <= now).map(|&e| (e, 1)));
            }
            self.each.retain(|&e| e > now);
            self.stacks = self.each.len() as u32;
            return self.stacks;
        }
        if self.all_at_once {
            // Split Flights: "Stacks expire all at once after 2 seconds
            // without a hit." Nothing is shed one at a time, so there is no
            // remainder to restart the clock for.
            if self.stacks > 0 && self.expiry <= now {
                if self.shed_log {
                    self.shed.push((self.expiry, self.stacks));
                }
                self.stacks = 0;
            }
            return self.stacks;
        }
        while self.stacks > 0 && self.expiry <= now {
            if self.shed_log {
                self.shed.push((self.expiry, 1));
            }
            self.stacks -= 1;
            self.expiry += duration;
        }
        self.stacks
    }

    /// Seed from a configured buff card: its stacks, on its own clock. A
    /// LOCKED card arrives here as [`crate::model::NO_TIMEOUT`], so nothing
    /// on this path has to know what locking is.
    /// WHEN THE NEXT STACK FALLS DUE, or `None` where nothing is up.
    ///
    /// An ABSOLUTE time rather than a remaining one, because that is what
    /// travels well: an expiry only moves when the buff is actually refreshed,
    /// so a row that carries it is identical to the row before it most of the
    /// time and the wire drops the repeat. A countdown changes every row.
    pub(super) fn expires_at(&self) -> Option<f64> {
        if self.per_stack {
            self.each.iter().copied().fold(None, |a: Option<f64>, e| {
                Some(a.map_or(e, |x| x.min(e)))
            })
        } else if self.stacks > 0 {
            Some(self.expiry)
        } else {
            None
        }
    }

    pub(super) fn seed(initial: u32, max: u32, duration: f64) -> Self {
        LiveStacks { stacks: initial.min(max), expiry: duration, ..LiveStacks::default() }
    }

    /// The same pile, keeping the time and size of every drop for `take_shed`.
    pub(super) fn logging_shed(self) -> Self {
        LiveStacks { shed_log: true, ..self }
    }

    /// Every drop since the last call, oldest first.
    pub(super) fn take_shed(&mut self) -> Vec<(f64, u32)> {
        std::mem::take(&mut self.shed)
    }

    /// Seed a pile that expires WHOLE — [`crate::model::BuffDecay::AllAtOnce`].
    pub(super) fn seed_all_at_once(initial: u32, max: u32, duration: f64) -> Self {
        LiveStacks { all_at_once: true, ..LiveStacks::seed(initial, max, duration) }
    }

    /// Seed a per-stack-expiry buff. A seeded stack starts its own clock at
    /// `duration`, the same instant the shared-clock family starts its one.
    pub(super) fn seed_per_stack(initial: u32, max: u32, duration: f64) -> Self {
        LiveStacks {
            stacks: initial.min(max),
            expiry: duration,
            each: vec![duration; initial.min(max) as usize],
            per_stack: true,
            ..LiveStacks::default()
        }
    }

    /// One trigger: decay what is due, climb by one (capped), restart the
    /// clock. A locked buff takes this path too — it earns like any other,
    /// and its restart lands at infinity.
    pub(super) fn bump(&mut self, now: f64, duration: f64, max: u32) {
        self.current(now, duration);
        if self.per_stack {
            // At the cap the OLDEST goes — that is what FIFO means here, and
            // it is why a capped pile still rolls forward rather than freezing.
            if self.each.len() >= max as usize {
                self.each.remove(0);
            }
            self.each.push(now + duration);
            self.stacks = self.each.len() as u32;
            return;
        }
        self.stacks = (self.stacks + 1).min(max);
        self.expiry = now + duration;
    }

    pub(super) fn on_kill(&mut self, now: f64, spec: &crate::model::StackSpec) {
        self.bump(now, spec.duration, spec.max_stacks);
    }
}

/// The run's earned on-kill buffs (weapon-scoped: shared by both forms
/// of a transform group).
#[derive(Default)]
pub(super) struct GalStacks {
    pub(super) co: LiveStacks,
    pub(super) multishot: LiveStacks,
    /// Critical Mutation's pile — another kill-fed stack, charged at the throw.
    pub(super) mutation: Mutation,
}

#[wfsim_gas_macro::metered]
impl GalStacks {
    pub(super) fn bump_on_kill(&mut self, params: &FightParams, now: f64) {
        if let Some(spec) = &params.co_stack {
            self.co.on_kill(now, spec);
        }
        if let Some(spec) = &params.multishot_stack {
            self.multishot.on_kill(now, spec);
        }
        if let Some((per_kill, cap, _)) = params.reload_grenade.and_then(|g| g.crit_per_kill) {
            self.mutation.on_kill(per_kill, cap);
        }
    }
}

/// M8/M100: a pool break supplies a body hit's base to ordinary Electricity.
/// The current Magnetic count is frozen here; the breaking hit supplies
/// the proc context. No separate clock, landing or source-scaling rule.
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn push_break_proc(
    debuffs: &mut DebuffState,
    owner: Seat,
    params: &FightParams,
    now: f64,
    pool: BrokenPool,
    active: &FightParams,
    foe: &Foe,
    target: &mut TargetState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
    status_damage_live: f64,
) {
    let stacks = debuffs.disrupt.len();
    if stacks == 0 {
        return;
    }
    let pool_max = match pool {
        BrokenPool::Overguard => foe.overguard(),
        BrokenPool::Shield => foe.max_shield(),
    };
    let mb_live = (0.03 * stacks as f64).min(0.30) * pool_max
        / (BLEED_TICKS as f64 * DOT_COEFFICIENT);
    let mit = debuffs.mitigation(now, params.status_duration_multiplier,
        params.armor_strip_per_puncture, params.squad.enemy_armor_multiplier);
    settle_procs(vec![DamageType::Electricity], owner, now, InstanceScale {
        mb_live,
        crit_multiplier: 1.0,
        part_factor: 1.0,
        landing: ProcLanding::default(),
        attrition: 1.0,
        xh_bracket: 1.0,
        status_damage_live,
    }, debuffs, gal, arc, target, params, active, &mit, r, rec, rng, foe, DEPTH_PROC);
}
