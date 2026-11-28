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
            if spec.earned_on == Some("kill") {
                self.co.on_kill(now, spec);
            }
        }
        if let Some(spec) = &params.multishot_stack {
            self.multishot.on_kill(now, spec);
        }
        if let Some((per_kill, cap, _)) = params.reload_grenade.and_then(|g| g.crit_per_kill) {
            self.mutation.on_kill(per_kill, cap);
        }
    }
}

/// M8: a pool break deals Electricity and forces an ordinary body proc.
/// The proc inherits the packet's status-scaled source, then applies its own
/// status scaling. Both keep the breaking weapon's context.
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
    body_index: usize,
) -> bool {
    let stacks = debuffs.disrupt.len();
    if stacks == 0 {
        return false;
    }
    let pool_max = match pool {
        BrokenPool::Overguard => foe.overguard(),
        BrokenPool::Shield => foe.max_shield(),
    };
    let base = crate::rules::status::magnetic_break_source(pool_max, stacks);
    let base = match (params.weakness_bonus, foe.magnetic_break_proxy) {
        (Some(weakness), Some(proxy)) => crate::rules::status::magnetic_break_normalized_source(base, weakness, proxy) as f64,
        _ => base,
    };
    let (break_key, break_owner) = debuffs
        .last_disrupt_source
        .unwrap_or_else(|| (super::procs::radial_group_key(owner, active), owner));
    use crate::rules::upgrades::{NativeScalar, ScalarMutation};
    let magnetic = crate::model::ProcType::new(10).expect("native Magnetic id");
    let electric = crate::model::ProcType::new(5).expect("native Electric id");
    let mut parent = crate::model::DamageData::new(NativeScalar::new(base as f32));
    parent.origin_proc = Some(magnetic);
    parent.part = crate::model::HitPart::ANY_PART;
    parent.source = Some(crate::model::SourceId(break_owner.0 as u32));
    parent.source_weapon = Some(crate::model::WeaponId(break_owner.0 as u32));
    parent.procs.set_forced(electric);
    parent.apply_proc_upgrades(magnetic, proc_damage_entries(params, status_damage_live), ScalarMutation::Materialize);
    if rec.native_trace_is_on() {
        rec.trace_native(now, || crate::record::NativeStep {
            phase: "pool_break", proc_type: Some(magnetic.id()), part: Some(parent.part.0),
            entries: proc_damage_entries(params, status_damage_live).filter_map(|entry| entry.for_proc(magnetic)).collect(),
            materialized: Some(true),
            ..crate::record::NativeStep::new(now, body_index, "ALN-MAG-03.parent.WEAPON_PROC_DAMAGE",
                &[base as f32], parent.scalar.base())
        });
    }
    let generic_proxy = match (params.weakness_bonus, foe.magnetic_break_proxy) {
        (Some(_), Some(proxy)) => proxy.damage_multiplier.unwrap_or(1.0) as f32,
        _ => 1.0,
    };
    let before_proxy = parent.scalar.base();
    parent.scalar = NativeScalar::new(before_proxy * generic_proxy);
    if rec.native_trace_is_on() {
        rec.trace_native(now, || crate::record::NativeStep {
            phase: "pool_break", proc_type: Some(magnetic.id()), part: Some(parent.part.0),
            ..crate::record::NativeStep::new(now, body_index, "ALN-MAG-02.generic_proxy",
                &[before_proxy, generic_proxy], parent.scalar.base())
        });
    }
    // The remaining receiving-source adapter supplies the existing final-buff policy.
    parent.scalar = NativeScalar::new(parent.scalar.base() * params.ability_final_at(now) as f32);
    let mb_live = parent.scalar.base() as f64;
    let mit = debuffs.mitigation(now, params.status_duration_multiplier,
        params.armor_strip_per_puncture, params.squad.enemy_armor_multiplier);
    let forced: Vec<_> = parent.procs.types().collect();
    let procs = crate::rules::status::procs_for_hit(&forced,
        0.0, &DamageVector::new(), foe.proc_handlers(), rng);
    let generation = target.generation;
    debuffs.pending_break_group = Some(break_key);
    settle_procs(&procs, break_owner, now, body_index, InstanceScale { native_source: Some(parent.scalar), native_part: Some(parent.part),
        mb_live,
        crit_multiplier: 1.0,
        part_factor: 1.0,
        landing: ProcLanding::default(),
        attrition: 1.0,
        xh_bracket: 1.0,
        status_damage_live,
    }, debuffs, gal, arc, target, params, active, &mit, r, rec, rng, foe, DEPTH_PROC);
    debuffs.pending_break_group = None;
    if target.generation != generation {
        return true;
    }
    let status_factor = crate::rules::upgrades::query(crate::rules::upgrades::UpgradeType::WEAPON_PROC_DAMAGE, 1.0,
        proc_damage_entries(params, status_damage_live).filter_map(|entry| entry.for_proc(magnetic)), false) as f64;
    let layers = [
        (crate::record::Factor::StatusDamage, status_factor),
        (crate::record::Factor::BodyPart, generic_proxy as f64),
        (crate::record::Factor::WarframeAbility, params.ability_final_at(now)),
        (crate::record::Factor::Faction, params.faction_bracket_at(now)),
        (crate::record::Factor::TargetMultiplier, foe.faction_bracket_multiplier),
    ];
    let raw = parent.scalar.value(false) as f64 * params.faction_bracket_at(now) * foe.faction_bracket_multiplier;
    let mut breakdown = watching(rec);
    let settled = target.apply(raw, TypeShares::single(DamageType::Electricity), false,
        now, foe, false, &mit, params.overguard_multiplier(), breakdown.as_mut());
    r.sources.magnetic_break += settled.effective;
    ledger::settle(r, rec, now, owner, body_index, || DamageType::Electricity, PopKind::Direct,
        breakdown.as_ref(), settled, Some(debuffs), ledger::Clock::Hit,
        || Instance { origin: crate::record::Origin::MagneticBreak, base,
            part: Some("body".into()), layers: mul_layers(base, &layers), ..Instance::default() });
    r.note_kills(u32::from(settled.killed), now, params.drop_is_in_reach(target.at));
    if settled.killed {
        gal.bump_on_kill(params, now);
        arc.on_kill(params, now, params.player_distance_m(body_index, target.at));
        debuffs.on_death(owner, params.acid_shells, foe);
        return true;
    }
    if let Some(pool) = settled.broken {
        return push_break_proc(debuffs, owner, params, now, pool, active, foe,
            target, gal, arc, r, rec, rng, status_damage_live, body_index);
    }
    false
}
