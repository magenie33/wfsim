use super::*;

/// One live arcane stacking-buff state, driven by its [`ArcBuffSpec`]
/// (data/arcanes/secondary; all stacking arcane buffs start FULL — user
/// setting — and then run on their own mechanics).
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ArcState {
    pub(super) stacks: u32,
    pub(super) expiry: f64,
    /// The damage instance that last granted this buff a stack, for the specs
    /// capped at one per instance. 0 = none yet; `ArcRuntime::pull` counts from
    /// 1, so a fresh state can never collide with it.
    pub(super) last_instance: u64,
}

impl ArcState {
    /// Apply pending decay per the spec's family and return live stacks.
    ///
    /// A LOCKED buff needs no branch here: its duration is
    /// [`crate::loadout::NO_TIMEOUT`], so every expiry it computes is infinite
    /// and neither family below can ever fall due.
    pub(super) fn current(&mut self, spec: &ArcBuffSpec, now: f64) -> u32 {
        if spec.all_drop {
            // On-status family (Cascadia Flare, Conjunction Voltage): one
            // shared timer; on timeout ALL stacks drop at once.
            if now >= self.expiry {
                self.stacks = 0;
            }
        } else {
            // Kill family (Merciless/Deadhead/Dexterity): lose ONE stack
            // and reset the timer — the Galvanized-style graceful decay.
            while self.stacks > 0 && self.expiry <= now {
                self.stacks -= 1;
                self.expiry += spec.duration;
            }
        }
        self.stacks
    }

    /// A trigger fired: grant one stack and refresh the timer.
    pub(super) fn bump(&mut self, spec: &ArcBuffSpec, now: f64) {
        self.current(spec, now);
        self.stacks = (self.stacks + 1).min(spec.max_stacks);
        self.expiry = now + spec.duration;
    }
}

/// The run's live arcane runtime: one state per spec in
/// `params.arcane.buffs` (weapon-scoped: shared by both transform forms,
/// like [`GalStacks`]), plus the Sharpened Bullets on-kill CD buff clock.
#[derive(Default)]
pub(super) struct ArcRuntime {
    pub(super) states: Vec<ArcState>,
    /// Sharpened Bullets' single refreshable on-kill buff expiry.
    pub(super) crit_damage_kill_expiry_seconds: f64,
    /// The current DAMAGE INSTANCE, counted from 1. A trigger pull is ONE
    /// instance however many pellets it puts out — which is the whole point,
    /// since Cascadia Flare's rule names multishot as the case that must not
    /// multiply its stacks. A field tick and a syndicate blast each open their
    /// own, because they are their own instances at their own times.
    pub(super) instance: u64,
    /// WHEN A BLAST WENT OFF, and ONCE PER MOMENT however many stacks did.
    ///
    /// A Blast fuse pays out a damage number, and what an arcane counting
    /// "hits" sees is the MOMENT, not the pile: nine stacks expiring one at a
    /// time are nine, and any number expiring together are one. Kept here
    /// because the three functions that fire fuses already carry this and the
    /// perk that reads them lives a level up.
    ///
    /// A MAX-STACK DETONATION IS NOT IN HERE. It is fired where the tenth stack
    /// is applied rather than by a fuse, so it never reaches this — which is
    /// also what the wiki says of it.
    pub(super) blast_pops: Vec<f64>,
    /// THE WIELDER'S RAGE, when the wielder has one. Here because every path
    /// that reads the live base-damage bucket already carries this runtime.
    pub(super) rage: Option<crate::rage::Rage>,
}

impl ArcRuntime {
    pub(super) fn init(params: &FightParams) -> Self {
        Self {
            states: params
                .arcane
                .buffs
                .iter()
                .map(|s| ArcState {
                    stacks: s.initial_stacks.min(s.max_stacks),
                    expiry: s.duration,
                    last_instance: 0,
                })
                .collect(),
            // Seed active only if configured so (Sharpened Bullets defaults
            // inactive). "Active" is ONE question with one answer everywhere —
            // is `now` before the window's end — and a locked buff simply has
            // no end, so a locked buff that has not fired yet is still off,
            // which is what the label promises.
            crit_damage_kill_expiry_seconds: params
                .crit_damage_on_kill
                .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 }),
            instance: 0,
            blast_pops: Vec::new(),
            rage: crate::warframes_data::warframe(&params.tenno.id).and_then(|f| f.rage).map(|s| {
                let (start, held) = params.rage_open.unwrap_or((0.0, false));
                crate::rage::Rage::new(s, start, held)
            }),
        }
    }

    /// Rage's share of the base-damage bucket at `now`; 0 without one.
    pub(super) fn rage_bonus(&self, now: f64) -> f64 {
        self.rage.map_or(0.0, |g| g.bonus(now))
    }

    /// Sharpened Bullets' on-kill window end — the replay reads it to say
    /// whether that buff is up.
    pub(super) fn crit_damage_kill_expiry_seconds(&self) -> f64 {
        self.crit_damage_kill_expiry_seconds
    }

    /// Live stacks of whichever specs belong to `owner`. They share one count
    /// by construction (one arcane is one card), so the first answers for all.
    pub(super) fn owner_stacks(&mut self, fx: &ArcaneFx, owner: &str, now: f64) -> u32 {
        fx.buffs
            .iter()
            .zip(self.states.iter_mut())
            .find(|(s, _)| {
                let o = if s.owner.is_empty() { fx.id.as_str() } else { s.owner.as_str() };
                o == owner
            })
            .map_or(0, |(s, st)| st.current(s, now))
    }

    /// Σ per_stack × live stacks over every buff granting `grant`.
    pub(super) fn total(&mut self, specs: &[ArcBuffSpec], grant: ArcGrant, now: f64) -> f64 {
        specs
            .iter()
            .zip(self.states.iter_mut())
            .filter(|(s, _)| s.grant == grant)
            .map(|(s, st)| s.per_stack * st.current(s, now) as f64)
            .sum()
    }

    /// Open the next damage instance. Called once per trigger pull, and once
    /// per off-pull instance (a field tick, a syndicate blast) — never per
    /// pellet and never per proc, which is exactly what the cap below means.
    pub(super) fn next_instance(&mut self) {
        self.instance += 1;
    }

    /// Fire `trigger`: every matching buff gains a stack.
    ///
    /// A spec marked `one_per_instance` gains AT MOST ONE per damage instance,
    /// however many procs of that type this instance applied and however many
    /// pellets applied them — wiki (Cascadia Flare): *"Only one stack can be
    /// added per damage instance; applying multiple Heat status effects, such
    /// as via Multishot or Archon Vitality in a single hit will not generate
    /// multiple stacks."* Every other spec still bumps per proc, because
    /// nothing says otherwise about it.
    pub(super) fn bump_trigger(&mut self, specs: &[ArcBuffSpec], trigger: ArcTrigger, now: f64) {
        let instance = self.instance;
        for (s, st) in specs.iter().zip(self.states.iter_mut()) {
            if s.trigger != trigger {
                continue;
            }
            if s.one_per_instance {
                if st.last_instance == instance {
                    continue;
                }
                st.last_instance = instance;
            }
            st.bump(s, now);
        }
    }

    /// ANY kill: arcane on-kill buffs stack; Sharpened Bullets refreshes.
    pub(super) fn on_kill(&mut self, params: &FightParams, now: f64) {
        self.bump_trigger(&params.arcane.buffs, ArcTrigger::Kill, now);
        if let Some(b) = params.crit_damage_on_kill {
            self.crit_damage_kill_expiry_seconds = now + b.duration;
        }
    }

    /// Sharpened Bullets' live RELATIVE crit-damage addition (it joins the
    /// crit-damage bucket, so each attack part scales its own base by it).
    pub(super) fn cd_bonus(&self, params: &FightParams, now: f64) -> f64 {
        match params.crit_damage_on_kill {
            Some(b) if now < self.crit_damage_kill_expiry_seconds => b.value,
            _ => 0.0,
        }
    }
}
