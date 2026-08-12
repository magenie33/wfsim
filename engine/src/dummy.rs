//! Minimal "shoot the training dummy" Monte Carlo — the first end-to-end sim.
//!
//! Scope (see devlog 2026-07-24):
//! - Dual Toxocyst **base form** (7.5 Impact / 60 Puncture / 7.5 Slash = 75,
//!   5% crit, 2.0x crit, 37% status), quantized damage vector.
//! - **Secondary Enervate** equipped (max rank): flat crit stacks on hit, resets
//!   after 6 big crits — driven through the real [`Perk`]/[`BuffBar`] machinery.
//! - The dummy is a **humanoid target made of body parts**; each shot lands on
//!   one part chosen by aim weight (default: 50% body 1x / 50% head 3x).
//! - **Status simulation v1** — the three procs an IPS vector can produce:
//!   - **Stagger** (Impact): counted; no combat effect on a dummy (its Parazon
//!     payload needs mercy flow).
//!   - **Weakened** (Puncture): +5% flat crit chance received per stack
//!     (cap 5, 10 s, FIFO replace-oldest) — feeds our shots' crit rolls.
//!   - **Bleed** (Slash): Cinematic DoT, 0.35 × ModdedBase × the proccing
//!     hit's crit/part multipliers (provenance snapshot), ticks at +1..+6 s,
//!     unlimited instances, ignores armor entirely.
//!     Proc selection uses the **quantized** vector's shares
//!     (`status::procs_for_hit`); status damage never procs status; a killing
//!     hit's procs are discarded (the respawned target is a fresh individual
//!     with a clean DebuffBar — decision 2026-07-24).
//! - **No** elements/mods yet, no Frenzy. Infinite ammo.
//!
//! Body parts, crit tiers, and headcrit fold-in as documented in
//! docs/MECHANICS.md §5/§7. All unverified until golden-tested.

use crate::arcanes_data::{ArcBuffSpec, ArcGrant, ArcTrigger, ArcaneFx};
use crate::buffs::BuffBar;
use crate::damage::{DamageType, DamageVector};
use crate::perks::frenzy::Frenzy;
use crate::perks::secondary_enervate::SecondaryEnervate;
use crate::perks::Perk;
use crate::rng::Rng;
use crate::scaling;
use crate::sim::{Event, Hit};
use crate::status;

/// A buff forced active by a "buff lock" simulation setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockedBuff {
    Frenzy,
}

/// Per-buff lock setting (user, 2026-07-24): each buff is configured
/// INDEPENDENTLY —
/// - `Permanent`: re-asserted every shot, overriding natural expiry
///   (100% uptime, full stacks).
/// - `Initial(stacks)`: granted once at t = 0 at the given stack count
///   with its NATURAL duration; afterwards only the buff's own mechanics
///   (triggers, decay, expiry) govern it. For non-stacking buffs
///   (Frenzy) the count is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Permanent,
    Initial(u32),
}

/// One buff-lock setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuffLock {
    pub buff: LockedBuff,
    pub mode: LockMode,
}

impl BuffLock {
    pub fn permanent(buff: LockedBuff) -> Self {
        Self {
            buff,
            mode: LockMode::Permanent,
        }
    }

    pub fn initial(buff: LockedBuff, stacks: u32) -> Self {
        Self {
            buff,
            mode: LockMode::Initial(stacks),
        }
    }
}

/// One live arcane stacking-buff state, driven by its [`ArcBuffSpec`]
/// (data/arcanes/secondary; all stacking arcane buffs start FULL — user
/// setting — and then run on their own mechanics).
#[derive(Debug, Clone, Copy, Default)]
struct ArcState {
    stacks: u32,
    expiry: f64,
    /// The damage instance that last granted this buff a stack, for the specs
    /// capped at one per instance. 0 = none yet; `ArcRuntime::pull` counts from
    /// 1, so a fresh state can never collide with it.
    last_instance: u64,
}

impl ArcState {
    /// Apply pending decay per the spec's family and return live stacks.
    ///
    /// A LOCKED buff needs no branch here: its duration is
    /// [`crate::loadout::NO_TIMEOUT`], so every expiry it computes is infinite
    /// and neither family below can ever fall due.
    fn current(&mut self, spec: &ArcBuffSpec, now: f64) -> u32 {
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
    fn bump(&mut self, spec: &ArcBuffSpec, now: f64) {
        self.current(spec, now);
        self.stacks = (self.stacks + 1).min(spec.max_stacks);
        self.expiry = now + spec.duration;
    }
}

/// The run's live arcane runtime: one state per spec in
/// `params.arcane.buffs` (weapon-scoped: shared by both transform forms,
/// like [`GalStacks`]), plus the Sharpened Bullets on-kill CD buff clock.
#[derive(Default)]
struct ArcRuntime {
    states: Vec<ArcState>,
    /// Sharpened Bullets' single refreshable on-kill buff expiry.
    cd_kill_expiry: f64,
    /// The current DAMAGE INSTANCE, counted from 1. A trigger pull is ONE
    /// instance however many pellets it puts out — which is the whole point,
    /// since Cascadia Flare's rule names multishot as the case that must not
    /// multiply its stacks. A field tick and a syndicate blast each open their
    /// own, because they are their own instances at their own times.
    instance: u64,
}

impl ArcRuntime {
    fn init(params: &DummyParams) -> Self {
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
            cd_kill_expiry: params
                .cd_on_kill
                .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 }),
            instance: 0,
        }
    }

    /// Sharpened Bullets' on-kill window end — the replay reads it to say
    /// whether that buff is up.
    fn cd_kill_expiry(&self) -> f64 {
        self.cd_kill_expiry
    }

    /// Live stacks of whichever specs belong to `owner`. They share one count
    /// by construction (one arcane is one card), so the first answers for all.
    fn owner_stacks(&mut self, fx: &ArcaneFx, owner: &str, now: f64) -> u32 {
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
    fn total(&mut self, specs: &[ArcBuffSpec], grant: ArcGrant, now: f64) -> f64 {
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
    fn next_instance(&mut self) {
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
    fn bump_trigger(&mut self, specs: &[ArcBuffSpec], trigger: ArcTrigger, now: f64) {
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
    fn on_kill(&mut self, params: &DummyParams, now: f64) {
        self.bump_trigger(&params.arcane.buffs, ArcTrigger::Kill, now);
        if let Some(b) = params.cd_on_kill {
            self.cd_kill_expiry = now + b.duration;
        }
    }

    /// Sharpened Bullets' live RELATIVE crit-damage addition (it joins the
    /// crit-damage bucket, so each attack part scales its own base by it).
    fn cd_bonus(&self, params: &DummyParams, now: f64) -> f64 {
        match params.cd_on_kill {
            Some(b) if now < self.cd_kill_expiry => b.value,
            _ => 0.0,
        }
    }
}

/// The real Incarnon combat cycle (user flow, 2026-07-24): the run STARTS
/// with a full gauge in Incarnon Form; when the charge magazine empties,
/// revert (`transmute_out_seconds`), fight in the base form until weakpoint hits
/// (each multishot pellet counts) rebuild the gauge, transmute
/// (`transmute_seconds`), repeat. Swapping either way fully reloads the
/// base form's magazine (wiki side effect). Frenzy EXISTS in the Incarnon
/// Form too (user-confirmed 2026-07-24): the buff persists across
/// transforms and headshots keep triggering it in both forms.
#[derive(Debug, Clone)]
pub struct IncarnonCycle {
    /// The base form's full engagement params (its own panel; target/aim/
    /// duration fields are ignored — the outer params' are shared).
    pub base_form: Box<DummyParams>,
    /// WHICH hits fill the gauge — weapon data (Zariman: weak-point; Torid:
    /// any direct hit).
    pub charge_on: crate::loadout::ChargeOn,
    /// Hits of `charge_on` to fill the gauge (Dual Toxocyst 9, Torid 5).
    pub charges_to_fill: u32,
    /// Incarnon → base transition (already reload-speed scaled).
    pub transmute_out_seconds: f64,
    /// Base → Incarnon transition (already reload-speed scaled).
    pub transmute_seconds: f64,
    /// The reload-speed bucket the two times above were divided by. A
    /// LIVE bonus (Lethal Rearmament) rescales them by
    /// `(1 + bucket) / (1 + bucket + live)`.
    pub reload_bucket: f64,
    /// Does the engagement OPEN already transformed, with a full charge
    /// magazine?
    ///
    /// False, and that is the fight the benchmark runs. A full gauge is a
    /// CONSUMABLE resource, and this project's own rule for those is that they
    /// start at zero and are earned in the fight (docs/BUFFS.md) — the cycle
    /// opening transformed was an exception to a rule already written down, and
    /// it handed every Incarnon weapon a magazine it had not paid for.
    ///
    /// It matters most where the gauge cannot be refilled: on a board with no
    /// weak-point hits, eight of the nine Incarnon forms can never charge, so
    /// a free opening magazine was the only Incarnon damage they would ever
    /// deal and it was pure gift (owner, 2026-08-07: 初始打完就歇菜).
    ///
    /// True is still a real way to play — you walk into the room having charged
    /// on the last one — which is why it is a field rather than a deletion.
    pub starts_primed: bool,
}

/// What happens when the target's health reaches zero.
///
/// These are simulator conveniences for calibration (the real Simulacrum has
/// neither an enemy-invincibility nor an instant-respawn toggle):
/// - `InfiniteHealth`: pools never deplete — measure steady per-shot damage
///   against a fixed defensive state.
/// - `InstantRespawn`: the target dies and instantly respawns in place at full
///   pools; overkill damage is lost. **Decision (2026-07-24): no on-death
///   transformation is modeled** (e.g. Thrax spectral forms are skipped — the
///   respawned target is always the physical form).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetMode {
    InfiniteHealth,
    InstantRespawn,
}

/// Damage attenuation (wiki U40 STRUCTURE: the enemy caps the damage it
/// can take per INSTANCE and per SECOND, both proportional to Max
/// Health, measured per player). The exact constants are UNPUBLISHED —
/// these fractions are recorded estimates pending in-game calibration
/// (the data file marks them as such).
#[derive(Debug, Clone, Copy)]
pub struct Attenuation {
    /// Max effective damage per damage instance / max health.
    pub instance_frac: f64,
    /// Max effective damage per second / max health.
    pub dps_frac: f64,
}

/// Per-unit status stack caps (Acolytes: any status 4, Impact 3).
#[derive(Debug, Clone, Copy)]
pub struct StackCaps {
    pub general: usize,
    pub impact: usize,
}

/// Which pool a damage instance just emptied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrokenPool {
    Overguard,
    Shield,
}

/// The simulated target: base stats + level, scaled via [`scaling`].
///
/// Prefer building this through `enemy_data::EnemySpec::target_params`, which
/// rejects combinations that do not exist in-game (e.g. an Eximus of a unit
/// with no Eximus variant). Hand-built values are re-checked at spawn.
#[derive(Debug, Clone)]
pub struct TargetParams {
    pub name: String,
    pub base_level: u32,
    pub level: u32,
    pub base_health: f64,
    pub base_armor: f64,
    pub base_overguard: f64,
    /// What this unit is worth in affinity BEFORE level scaling — the enemy
    /// file's own number. Scaled by `scaling::affinity_multiplier` at the kill.
    pub base_affinity: f64,
    /// Base shields (mitigation order: Overguard → Shield → Health;
    /// Toxin bypasses shields but NOT overguard).
    pub base_shield: f64,
    pub health_curve: scaling::Curve,
    pub shield_curve: scaling::Curve,
    /// Boss-type damage attenuation (Acolytes etc.); `None` = none.
    pub attenuation: Option<Attenuation>,
    /// Per-unit status stack caps; `None` = the normal per-status caps.
    pub stack_caps: Option<StackCaps>,
    /// Cold never converts on this target — see
    /// [`crate::enemy_data::EnemySpec::cannot_be_frozen`]. The stacks climb to
    /// the ordinary ten-stack cap and STAY there, so the Cold crit-damage bonus
    /// is up for the whole fight instead of being spent every tenth proc.
    pub cannot_be_frozen: bool,
    /// Steel Path: health ×2.5 (armor and overguard untouched). The +100 level
    /// shift is a mission-spawn effect — pick `level` accordingly.
    pub steel_path: bool,
    /// Eximus variant: boosted base health + overguard (wiki `Eximus`). Only
    /// legal when `can_be_eximus` — the combination is validated at spawn.
    pub eximus: bool,
    /// Whether this unit has an Eximus variant in-game (wiki
    /// `Eximus/Compatibilities`; Thrax units do not).
    pub can_be_eximus: bool,
    /// Unit-level status immunities: these types are EXCLUDED from the proc
    /// draw (weights renormalize — wiki `Status_Effect` §Immunity
    /// Interactions). Mechanic states (Frozen, Overguard suppression) are NOT
    /// immunities: those procs are drawn normally and nulled on landing.
    pub status_immunities: Vec<DamageType>,
    /// Combat faction — the match key for faction-damage mods (Bane/Expel).
    /// `Unknown` (the default for hand-made targets) means no faction mod
    /// applies. Set from the enemy YAML `combat_faction:` field.
    pub faction: crate::loadout::Faction,
    /// The post-U36 damage-type vulnerability columns (System B): this unit's
    /// own, keyed by `FactionDamageOverride ?? Faction`, plus the Overguard
    /// pool's. A per-COMPONENT multiplier, independent of `faction` above —
    /// the two systems have different keys and stack multiplicatively
    /// (docs/MECHANICS.md §8).
    pub type_mods: crate::factions_data::Columns,
    pub mode: TargetMode,
}

/// The SHAPE of one damage instance — what fraction of it is each type.
///
/// The defence side reads the shape twice: Toxin bypasses shields, and the
/// vulnerability column is per component. Both used to be answered by passing
/// a bare `toxin_frac` down; a second per-type question makes that a growing
/// list of scalars, so the shape travels as one value instead.
#[derive(Debug, Clone, Copy)]
pub struct TypeShares([f64; DamageType::ALL.len()]);

impl TypeShares {
    /// An instance that is entirely one type — a DoT tick, a Blast
    /// detonation, an arcane's flat instance.
    pub fn single(t: DamageType) -> Self {
        let mut s = [0.0; DamageType::ALL.len()];
        s[t as usize] = 1.0;
        TypeShares(s)
    }

    /// A hit's shape, from the vector it was quantized to. A zero vector has
    /// no shape and no damage; its multipliers come out 1.0 either way.
    pub fn of(v: &DamageVector) -> Self {
        let total = v.total();
        let mut s = [0.0; DamageType::ALL.len()];
        if total > 0.0 {
            for t in DamageType::ALL {
                s[t as usize] = v.get(t) / total;
            }
        }
        TypeShares(s)
    }

    /// The Toxin share — what bypasses shields straight to health.
    pub fn toxin(&self) -> f64 {
        self.0[DamageType::Toxin as usize]
    }

    /// Does this instance have a shape at all? A shapeless one (nothing but
    /// zeros) is treated as one untyped, neutral lump rather than as damage
    /// that vanishes — the shares are bookkeeping, never a gate on damage.
    fn shaped(&self) -> bool {
        self.0.iter().sum::<f64>() > 0.0
    }

    /// The whole instance's multiplier under a column. Each component takes
    /// its own factor, so with shares summing to 1 this is the weighted mean.
    fn whole(&self, col: &crate::factions_data::Column) -> f64 {
        if !self.shaped() {
            return 1.0;
        }
        DamageType::ALL
            .iter()
            .map(|&t| self.0[t as usize] * col.get(t))
            .sum()
    }

    /// The part that does NOT bypass shields, already column-scaled — a
    /// PORTION of the instance, not a multiplier on it.
    fn non_toxin_portion(&self, col: &crate::factions_data::Column) -> f64 {
        if !self.shaped() {
            return 1.0;
        }
        self.whole(col) - self.toxin_portion(col)
    }

    /// The Toxin part, already column-scaled. Goes straight to health.
    fn toxin_portion(&self, col: &crate::factions_data::Column) -> f64 {
        self.toxin() * col.get(DamageType::Toxin)
    }
}

impl TargetParams {
    /// A plain training dummy: no defenses, never dies. Damage passes through
    /// unmitigated, which keeps raw-damage calibration runs simple.
    pub fn training_dummy() -> Self {
        Self {
            name: "training dummy".into(),
            base_level: 1,
            level: 1,
            base_health: 1.0,
            base_armor: 0.0,
            base_overguard: 0.0,
            base_affinity: 0.0,
            base_shield: 0.0,
            health_curve: scaling::health::UNAFFILIATED,
            shield_curve: scaling::shield::GRINEER, // unused at 0 shields
            attenuation: None,
            stack_caps: None,
            cannot_be_frozen: false,
            steel_path: false,
            eximus: false,
            can_be_eximus: false,
            status_immunities: Vec::new(),
            faction: crate::loadout::Faction::Unknown,
            // A training dummy has no faction and takes damage as written.
            type_mods: crate::factions_data::Columns::NEUTRAL,
            mode: TargetMode::InfiniteHealth,
        }
    }

    /// Impossible-combination check (see `enemy_data` for the rigor rule).
    pub fn validate(&self) -> Result<(), String> {
        if self.eximus && !self.can_be_eximus {
            return Err(format!(
                "{} cannot be an Eximus: no such unit exists in-game",
                self.name
            ));
        }
        Ok(())
    }

    /// Effective base health: Eximus units replace theirs with the boosted
    /// level-dependent value before the faction curve applies.
    fn effective_base_health(&self) -> f64 {
        if self.eximus {
            scaling::eximus_base_health(self.base_health, self.level, self.base_armor > 0.0)
        } else {
            self.base_health
        }
    }

    /// Scaled max health at `level` (Steel Path ×2.5 applied).
    pub fn max_health(&self) -> f64 {
        let delta = self.level.saturating_sub(self.base_level) as f64;
        let sp = if self.steel_path {
            scaling::STEEL_PATH_HEALTH_MULT
        } else {
            1.0
        };
        self.effective_base_health() * self.health_curve.multiplier(delta) * sp
    }

    /// Scaled armor (spawn minimum 200, cap 2,700; Steel Path does not touch
    /// armor since U36).
    pub fn armor(&self) -> f64 {
        scaling::armor_at(self.base_armor, self.level, self.base_level)
    }

    /// Scaled max shields (Steel Path ×2.5, like health).
    pub fn max_shield(&self) -> f64 {
        let delta = self.level.saturating_sub(self.base_level) as f64;
        let sp = if self.steel_path {
            scaling::STEEL_PATH_SHIELD_MULT
        } else {
            1.0
        };
        self.base_shield * self.shield_curve.multiplier(delta) * sp
    }

    /// Scaled overguard (uses `level − 1`; no Steel Path bonus documented).
    /// Eximus base overguard is 12; no in-game unit combines innate overguard
    /// with Eximus status, so the max() is only a defensive guess.
    pub fn overguard(&self) -> f64 {
        let base = if self.eximus {
            scaling::EXIMUS_BASE_OVERGUARD.max(self.base_overguard)
        } else {
            self.base_overguard
        };
        scaling::overguard_at(base, self.level)
    }
}

/// Live pools of the target during a run.
#[derive(Clone)]
struct TargetState {
    overguard: f64,
    shield: f64,
    health: f64,
    /// Shield-gate window end (0.1 s after a shield break: all damage
    /// ×5% except direct weakpoint hits — user model, M1).
    gate_until: f64,
    /// Attenuation bookkeeping: 1 s buckets anchored at spawn.
    atten_window_start: f64,
    atten_window_damage: f64,
}

/// VICIOUS PROMISE'S CONDITION. VERBATIM (wiki, Paris Incarnon Genesis):
/// *"Enemies are undamaged as long as their health and shield have not been
/// damaged. Damaging Overguard is not taken into account."*
///
/// A free function rather than an inline expression because the OVERGUARD
/// EXCLUSION is the whole subtlety and it has to be assertable. It cannot be
/// asserted through a sim: every fixture that leaves health intact long enough
/// to see the difference does so by freezing every pool at once
/// (`TargetMode::InfiniteHealth`), and then a wrong implementation reads the
/// same "undamaged" a right one does.
fn target_undamaged(t: &TargetState, p: &TargetParams) -> bool {
    t.health >= p.max_health() - 1e-9 && t.shield >= p.max_shield() - 1e-9
}

impl TargetState {
    fn spawn(p: &TargetParams) -> Self {
        Self::spawn_at(p, 0.0)
    }

    fn spawn_at(p: &TargetParams, now: f64) -> Self {
        if let Err(e) = p.validate() {
            panic!("invalid target: {e}");
        }
        Self {
            overguard: p.overguard(),
            shield: p.max_shield(),
            health: p.max_health(),
            gate_until: 0.0,
            atten_window_start: now,
            atten_window_damage: 0.0,
        }
    }

    /// Which vulnerability column an instance landing RIGHT NOW would read.
    /// Same rule [`apply`](Self::apply) uses — the Overguard layer has its
    /// own table — exposed so a caller can split the reported damage by type
    /// the way the target actually took it. Call it BEFORE `apply`: the pool
    /// it answers for is the one that is still standing.
    fn incoming_column(&self, p: &TargetParams) -> crate::factions_data::Column {
        if self.overguard > 0.0 {
            p.type_mods.overguard
        } else {
            p.type_mods.faction
        }
    }

    /// Apply one damage instance under a live [`Mitigation`] snapshot.
    /// Returns `(effective_damage, killed, broken_pool)`.
    ///
    /// Mitigation model (docs/MECHANICS.md §8, unverified):
    /// - Order: Overguard → Shields → Health, no spill between pools.
    /// - Every component is first scaled by the vulnerability COLUMN the pool
    ///   reads (System B, `factions_data`): the Overguard pool's own, or the
    ///   unit's `FactionDamageOverride ?? Faction` one. This is what `shares`
    ///   is for — a per-type multiplier needs the instance's per-type shape.
    /// - Overguard takes raw × its column × Disrupt amp (ignores armor);
    ///   Toxin does NOT bypass it.
    /// - Shields take the non-Toxin portion × Disrupt amp (no armor);
    ///   the Toxin portion bypasses straight to health.
    /// - Health takes × Virus amp × (1 − 0.9·√(armor_eff/2700)) with
    ///   `armor_eff = armor × strip factors`, floored at 1 — unless the
    ///   instance ignores armor (Cinematic ticks).
    /// - Shield gate (user model, M1): for 0.1 s after a shield break,
    ///   ALL damage ×5% except direct weakpoint hits (`head_direct`).
    /// - Attenuation (boss types): the resulting effective damage is
    ///   clamped per instance and per 1 s bucket (fractions of max
    ///   health) — applied after every other layer.
    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        raw: f64,
        shares: TypeShares,
        head_direct: bool,
        now: f64,
        p: &TargetParams,
        ignores_armor: bool,
        mit: &Mitigation,
    ) -> (f64, bool, Option<BrokenPool>) {
        // Shield gate: a unit-state window multiplying incoming damage.
        let gated = if now < self.gate_until && !head_direct {
            raw * 0.05
        } else {
            raw
        };

        // Route into pools (no spill). The vulnerability COLUMN is chosen by
        // the pool, not only by the enemy: Overguard has its own table (wiki
        // Overguard — neutral but ×1.5 Void), and it is a layer over the unit
        // rather than part of it, so the unit's own column never reaches it.
        let mut shield_part = 0.0f64;
        let mut health_part = 0.0f64;
        let mut og_part = 0.0f64;
        if self.overguard > 0.0 {
            og_part = gated * shares.whole(&p.type_mods.overguard) * mit.disrupt_amp;
        } else {
            let col = &p.type_mods.faction;
            let toxin = gated * shares.toxin_portion(col);
            let rest = gated * shares.non_toxin_portion(col);
            if self.shield > 0.0 {
                shield_part = rest * mit.disrupt_amp;
            } else {
                health_part += rest;
            }
            health_part += toxin;
            // Health mitigation: virus amp + (live) armor.
            if health_part > 0.0 {
                let dr = if ignores_armor {
                    0.0
                } else {
                    scaling::armor_damage_reduction(p.armor() * mit.armor_multiplier)
                };
                let boosted = health_part * mit.virus_amp;
                health_part = if dr > 0.0 {
                    (boosted * (1.0 - dr)).max(1.0)
                } else {
                    boosted
                };
            }
        }

        // Attenuation: clamp the instance total, then the 1 s bucket.
        let mut effective = og_part + shield_part + health_part;
        if let Some(a) = p.attenuation {
            while now >= self.atten_window_start + 1.0 {
                self.atten_window_start += 1.0;
                self.atten_window_damage = 0.0;
            }
            let hp = p.max_health();
            let allowed = (a.instance_frac * hp)
                .min(a.dps_frac * hp - self.atten_window_damage)
                .max(0.0);
            if effective > allowed {
                let k = if effective > 0.0 {
                    allowed / effective
                } else {
                    0.0
                };
                og_part *= k;
                shield_part *= k;
                health_part *= k;
                effective = allowed;
            }
            self.atten_window_damage += effective;
        }

        if p.mode == TargetMode::InfiniteHealth {
            return (effective, false, None);
        }

        let mut broke = None;
        if og_part > 0.0 {
            self.overguard -= og_part;
            if self.overguard <= 0.0 {
                self.overguard = 0.0; // no spill
                broke = Some(BrokenPool::Overguard);
            }
        }
        if shield_part > 0.0 {
            self.shield -= shield_part;
            if self.shield <= 0.0 {
                self.shield = 0.0; // no spill
                self.gate_until = now + 0.1;
                broke = Some(BrokenPool::Shield);
            }
        }
        self.health -= health_part;
        if self.health <= 0.0 {
            *self = TargetState::spawn_at(p, now); // instant respawn
            (effective, true, broke)
        } else {
            (effective, false, broke)
        }
    }
}

/// A live DoT instance: the proccing hit's deferred damage (provenance
/// snapshot baked into `value`). Bleed (Cinematic ticks) ignores armor;
/// elemental DoTs (Toxin/Electricity/Gas, Disrupt's break-proc Tesla) take
/// live armor mitigation. `dtype` is the STATUS type (for CO counting).
struct Dot {
    next_tick: f64,
    ticks_left: u32,
    value: f64,
    dtype: DamageType,
    ignores_armor: bool,
}

/// The Heat singleton accumulator (data/debuffs/ignite.yaml): ONE entity
/// per target; each proc adds its contribution to the tick value and
/// refreshes the shared expiry; ticks stay anchored to the FIRST proc.
struct HeatEntity {
    born: f64,
    expiry: f64,
    next_tick: f64,
    /// Current consolidated tick value (sum of the live contributions).
    value: f64,
    /// Individual contributions, oldest first — only tracked when a per-unit
    /// stack cap applies, so a capped Heat can drop its OLDEST contribution
    /// (FIFO) when a new proc lands, exactly like every other capped status.
    /// Empty when uncapped (contributions just fold into `value`).
    recent: Vec<f64>,
}

/// A Blast (Detonate) stack: the fuse fires a single-target hit; the 10th
/// stack detonates everything early. The radial is ignored — single-target
/// sim, and the host is excluded from the radial anyway.
struct BlastStack {
    fuse: f64,
    value: f64,
    /// The applying weapon's [`InstanceScale::xh_bracket`], carried so that the
    /// EXTRA HIT this detonation triggers can be scaled by an elemental bracket
    /// the detonation itself never gets. The one thing a Blast stack has to
    /// remember about the gun that made it.
    xh_bracket: f64,
}

/// Target-side debuff state — the payloads of all proc types a combined
/// vector can produce (data/debuffs/*.yaml; simplifications noted inline).
#[derive(Default)]
struct DebuffState {
    /// Stagger stack expiries (6 s, FIFO). No combat payload on a dummy.
    stagger: Vec<f64>,
    /// Weakened stacks (10 s): +5% flat crit chance received per stack.
    weakened: Vec<f64>,
    /// Freeze (Cold) stacks: +0.10/+0.05 flat crit DAMAGE received; cap 4
    /// while overguard holds (never Frozen). The 10th proc consumes all
    /// stacks and enters the Frozen state (`frozen_until`).
    freeze: Vec<f64>,
    /// The Frozen state (data/debuffs/frozen.yaml): mutually exclusive
    /// with Freeze — the 10th Cold proc consumes the 9 stacks; while
    /// active, Cold procs are inert and crit damage received is +1.00
    /// flat; on expiry Freeze is SET to exactly 3 fresh 6 s stacks.
    frozen_until: Option<f64>,
    /// Disrupt (Magnetic) stacks: shield/overguard damage taken
    /// × (2 + 0.25·(stacks−1)), live.
    disrupt: Vec<f64>,
    /// Virus (Viral) stacks: HEALTH damage taken × (2 + 0.25·(stacks−1)),
    /// live per tick (the official live-evaluation example).
    virus: Vec<f64>,
    /// Corrosion stacks (8 s): armor × (1 − (0.20 + 0.06·stacks)).
    corrosion: Vec<f64>,
    /// Confusion (Radiation): no single-target combat payload; tracked for
    /// CO type counting.
    confusion: Vec<f64>,
    /// The Bullet Attractor (Void), same treatment as Confusion and for the
    /// same reason: no single-target combat payload — it redirects fire in a
    /// 2.5 m field and nobody shoots back here — but Void IS on Condition
    /// Overload's list of counting procs, so an extra hit that lands one is
    /// worth a CO stack and that is the whole of what it is worth.
    ///
    /// ONE ENTRY. The wiki describes a field on the target, not a stack count,
    /// and a re-proc moves it to where the new hit landed rather than adding a
    /// second one.
    attractor: Vec<f64>,
    blast: Vec<BlastStack>,
    dots: Vec<Dot>,
    heat: Option<HeatEntity>,
    /// Heat armor-strip ramp-DOWN (ignite.yaml): after the entity dies at
    /// `.0` with strip `.1`, armor returns 50→40→30→15→0% in 1.5 s steps.
    heat_decay: Option<(f64, f64)>,
}

const STAGGER_DURATION: f64 = 6.0;
const STAGGER_CAP: usize = 5;
const WEAKENED_DURATION: f64 = 10.0;
const WEAKENED_CAP: usize = 5;
const WEAKENED_FLAT_CC_PER_STACK: f64 = 0.05;
const BLEED_COEFFICIENT: f64 = 0.35;
const BLEED_DELAY: f64 = 1.0;
const BLEED_TICKS: u32 = 6;
const DOT_COEFFICIENT: f64 = 0.5; // Toxin/Electricity/Heat/Gas ticks
const STATUS_DURATION: f64 = 6.0; // the standard proc duration
const CORROSION_DURATION: f64 = 8.0;
/// Bullet Attractor (Void): "a small 2.5 metre radius field ... for 3 seconds"
/// (wiki Damage/Void_Damage). Shorter than the standard 6 s, which is why it is
/// its own constant rather than `STATUS_DURATION`.
const ATTRACTOR_DURATION: f64 = 3.0;
const TEN_STACK_CAP: usize = 10;
const BLAST_COEFFICIENT: f64 = 0.3;
const BLAST_FUSE: f64 = 1.5;
const FREEZE_CAP_UNDER_OVERGUARD: usize = 4;
const FREEZE_STACKS_BEFORE_FROZEN: usize = 9; // the 10th proc converts
const FROZEN_DURATION: f64 = 3.0;
const FROZEN_CRIT_DAMAGE_RECEIVED: f64 = 1.00;
const FROZEN_RESET_STACKS: usize = 3;
const HEAT_STRIP_DECAY: [f64; 5] = [0.50, 0.40, 0.30, 0.15, 0.0];
const HEAT_STRIP_DECAY_INTERVAL: f64 = 1.5;

/// Live target-side damage-taken modifiers at one instant (the tick-time
/// mitigation pipeline — defender-side, evaluated per hit/tick).
struct Mitigation {
    disrupt_amp: f64,
    virus_amp: f64,
    /// (1 − heat strip) × (1 − corrosive strip), applied to armor VALUE.
    armor_multiplier: f64,
}

/// The +100%/+25% ten-stack amp curve shared by Disrupt and Virus.
fn ten_stack_amp(stacks: usize) -> f64 {
    if stacks == 0 {
        1.0
    } else {
        2.0 + 0.25 * (stacks as f64 - 1.0)
    }
}

/// THE DEBUFF ROSTER — what the target can be carrying, and how much of each.
///
/// The mirror of [`DummyParams::buff_roster`], and deliberately the same shape:
/// `(id, cap)` pairs whose order the frames index into. The two tables on the
/// page are the same component fed from opposite sides of the fight (owner,
/// 2026-08-11: "你就和我们现在的buff列表对称").
///
/// It is a CONSTANT rather than a function of the build, because a debuff is
/// the TARGET's: the roster is every status this engine models, and a run that
/// never applies one draws a flat line at zero — which is the honest answer to
/// "was Corrosive ever up" and the same answer the buff table gives for a buff
/// nothing triggered.
///
/// A DEATH IS NOT A NEW ROW. The arena replaces the body it kills, and every
/// stack goes with it — so a respawn shows as the series dropping to zero and
/// climbing again, and `uptime` counts that gap against you. That is the point
/// (owner: "一个敌人死了又死的，算在一个id里").
pub const DEBUFF_ROSTER: [(&str, u32); 13] = [
    // The 10-stack families, and the three that are not.
    ("virus", TEN_STACK_CAP as u32),
    ("corrosion", TEN_STACK_CAP as u32),
    ("disrupt", TEN_STACK_CAP as u32),
    ("confusion", TEN_STACK_CAP as u32),
    ("blast", TEN_STACK_CAP as u32),
    // Cold's two states are two rows because they are mutually exclusive: the
    // tenth proc consumes the nine stacks and enters Frozen, so one series
    // falling to zero as the other rises is the mechanic, not a glitch.
    ("freeze", TEN_STACK_CAP as u32),
    ("frozen", 1),
    ("stagger", STAGGER_CAP as u32),
    ("weakened", WEAKENED_CAP as u32),
    // The Bullet Attractor is a FIELD, not a pile: a re-proc moves it rather
    // than adding a second one.
    ("attractor", 1),
    // The DoT families, counted as live instances of that type. Heat is a
    // singleton entity rather than a stack list, so its row is 0 or 1 — what a
    // reader wants from it is when it was burning, and the strip ramp is the
    // part `heat_decay` owns.
    ("bleed", TEN_STACK_CAP as u32),
    ("poison", TEN_STACK_CAP as u32),
    ("ignite", 1),
];

impl DebuffState {
    /// The roster's live counts at `now`, positionally matching
    /// [`DEBUFF_ROSTER`]. Expired entries are excluded rather than pruned —
    /// sampling must not change the fight it is sampling.
    fn sample(&self, now: f64) -> Vec<u8> {
        let live = |v: &Vec<f64>| v.iter().filter(|&&e| e > now).count() as u8;
        let dots_of = |t: DamageType| {
            self.dots
                .iter()
                .filter(|d| d.dtype == t && d.ticks_left > 0)
                .count() as u8
        };
        vec![
            live(&self.virus),
            live(&self.corrosion),
            live(&self.disrupt),
            live(&self.confusion),
            // A Blast stack is a FUSE rather than an expiry: it is waiting to go
            // off, not waiting to wear off.
            self.blast.iter().filter(|b| b.fuse > now).count() as u8,
            live(&self.freeze),
            u8::from(self.frozen_until.is_some_and(|e| e > now)),
            live(&self.stagger),
            live(&self.weakened),
            live(&self.attractor),
            dots_of(DamageType::Slash),
            dots_of(DamageType::Toxin),
            u8::from(self.heat.is_some()),
        ]
    }
}

impl DebuffState {
    /// FIFO push with the universal replace-oldest rule (application-time
    /// order; uniform durations make the front the oldest).
    fn push_capped(list: &mut Vec<f64>, expiry: f64, cap: usize, now: f64) {
        list.retain(|&e| e > now); // lazy prune of expired stacks
        if list.len() >= cap {
            list.remove(0); // replace the oldest APPLIED stack
        }
        list.push(expiry);
    }

    /// Push an independent DoT (Slash/Toxin/Electricity/Gas). These have no
    /// natural cap; under a per-unit cap the count of THIS TYPE is limited,
    /// FIFO replace-oldest (the oldest same-type instance drops).
    fn push_dot_capped(&mut self, dot: Dot, cap: Option<usize>) {
        if let Some(cap) = cap {
            while self.dots.iter().filter(|d| d.dtype == dot.dtype).count() >= cap {
                match self.dots.iter().position(|d| d.dtype == dot.dtype) {
                    Some(i) => {
                        self.dots.remove(i);
                    }
                    None => break,
                }
            }
        }
        self.dots.push(dot);
    }

    /// Apply a Heat proc to the singleton accumulator: add its contribution to
    /// the consolidated tick value and refresh the shared expiry (ticks stay
    /// anchored to the first proc). Under a per-unit cap, hold at most `cap`
    /// contributions FIFO — a new proc drops the OLDEST contribution instead
    /// of being ignored (the universal replace-oldest rule; user 2026-07-25).
    fn apply_heat(&mut self, t: f64, contrib: f64, expiry: f64, cap: Option<usize>) {
        match &mut self.heat {
            Some(h) => {
                match cap {
                    Some(c) => {
                        if h.recent.len() >= c {
                            h.value -= h.recent.remove(0);
                        }
                        h.recent.push(contrib);
                        h.value += contrib;
                    }
                    None => h.value += contrib,
                }
                h.expiry = expiry; // refresh regardless
            }
            None => {
                let mut recent = Vec::new();
                if cap.is_some() {
                    recent.push(contrib);
                }
                self.heat = Some(HeatEntity {
                    born: t,
                    expiry,
                    next_tick: t + 1.0,
                    value: contrib,
                    recent,
                });
            }
        }
    }

    fn weakened_active(&mut self, now: f64) -> usize {
        self.weakened.retain(|&e| e > now);
        self.weakened.len()
    }

    fn prune(&mut self, now: f64, sd: f64) {
        self.stagger.retain(|&e| e > now);
        self.weakened.retain(|&e| e > now);
        self.freeze.retain(|&e| e > now);
        self.disrupt.retain(|&e| e > now);
        self.virus.retain(|&e| e > now);
        self.corrosion.retain(|&e| e > now);
        self.confusion.retain(|&e| e > now);
        self.attractor.retain(|&e| e > now);
        if let Some(f) = self.frozen_until {
            if f <= now {
                // Thaw: Freeze is SET to exactly 3 stacks with FRESH 6 s
                // timers issued from the trigger's context (M6/M7).
                self.frozen_until = None;
                self.freeze = vec![f + STATUS_DURATION * sd; FROZEN_RESET_STACKS];
                self.freeze.retain(|&e| e > now); // long-idle prune
            }
        }
        if let Some(h) = &self.heat {
            // Strict: a tick scheduled at EXACTLY the expiry still lands
            // (the +6 s tick of an unrefreshed proc).
            if h.expiry < now {
                // Begin the armor-strip ramp-down from the strip level the
                // entity had reached when it died.
                self.heat_decay = Some((h.expiry, self.heat_ramp_up(h.expiry, sd)));
                self.heat = None;
            }
        }
        if let Some((t0, s0)) = self.heat_decay {
            let steps = ((now - t0) / (HEAT_STRIP_DECAY_INTERVAL * sd)).floor() as usize;
            let start = HEAT_STRIP_DECAY
                .iter()
                .position(|&s| s <= s0 + 1e-9)
                .unwrap_or(HEAT_STRIP_DECAY.len() - 1);
            if start + steps >= HEAT_STRIP_DECAY.len() - 1 {
                self.heat_decay = None; // fully returned
            }
        }
    }

    /// ACTIVE Cold statuses on the target for per-Cold-stack bonuses
    /// (Secondary Shiver): the live Freeze stack count; the Frozen state
    /// counts as the full 10 (it consumed 9 stacks + the trigger proc).
    fn cold_status_count(&mut self, now: f64) -> u32 {
        if self.frozen_until.is_some_and(|f| f > now) {
            return 10;
        }
        self.freeze.retain(|&e| e > now);
        self.freeze.len() as u32
    }

    /// Cold's flat crit-damage-received bonus, added into cd_total BEFORE
    /// the tier formula: +0.10 first stack, +0.05 each further; +1.00
    /// while Frozen (supersedes the table).
    fn cold_cd_bonus(&self, now: f64) -> f64 {
        if self.frozen_until.is_some_and(|f| f > now) {
            return FROZEN_CRIT_DAMAGE_RECEIVED;
        }
        match self.freeze.len() {
            0 => 0.0,
            n => 0.10 + 0.05 * (n as f64 - 1.0),
        }
    }

    /// Apply one Cold proc (data/debuffs/freeze.yaml + frozen.yaml):
    /// inert while Frozen; the 10th stack CONSUMES all Freeze stacks and
    /// enters Frozen; overguard holders cap at 4 (never Frozen).
    ///
    /// Returns whether a Cold status was actually APPLIED. `false` only while
    /// Frozen, where `frozen.yaml` is explicit — `refreshable: false`, "cannot
    /// be extended: Cold procs are inert". Anything that keys on "this weapon
    /// applied a Cold status" must read this rather than the attempt: no
    /// status landed, so Primary Frostbite has nothing to stack off (user,
    /// 2026-08-02). A CAPPED stack list still returns true — pushing past a
    /// cap replaces the oldest, which is an application.
    fn apply_cold_proc(
        &mut self,
        t: f64,
        sd: f64,
        under_overguard: bool,
        caps: Option<StackCaps>,
        no_frozen: bool,
    ) -> bool {
        if self.frozen_until.is_some_and(|f| f > t) {
            return false; // inert
        }
        self.freeze.retain(|&e| e > t);
        if under_overguard {
            let cap = caps.map_or(FREEZE_CAP_UNDER_OVERGUARD, |c| {
                FREEZE_CAP_UNDER_OVERGUARD.min(c.general)
            });
            DebuffState::push_capped(&mut self.freeze, t + STATUS_DURATION * sd, cap, t);
            return true;
        }
        if let Some(c) = caps {
            // A per-unit cap below 10 also means Frozen is unreachable.
            DebuffState::push_capped(&mut self.freeze, t + STATUS_DURATION * sd, c.general, t);
            return true;
        }
        if no_frozen {
            // NEVER CONVERTS. The tenth proc is an ordinary stack here, so the
            // ladder sits at its cap and the Cold bonus is up all fight rather
            // than being spent on a 3-second Frozen window every ten procs.
            DebuffState::push_capped(&mut self.freeze, t + STATUS_DURATION * sd, TEN_STACK_CAP, t);
        } else if self.freeze.len() >= FREEZE_STACKS_BEFORE_FROZEN {
            self.freeze.clear();
            self.frozen_until = Some(t + FROZEN_DURATION * sd);
        } else {
            self.freeze.push(t + STATUS_DURATION * sd);
        }
        true
    }

    /// Heat armor-strip ramp-UP: 15/30/40/50% at 0.5 s steps after the
    /// FIRST proc (steps scaled by status duration).
    fn heat_ramp_up(&self, now: f64, sd: f64) -> f64 {
        let Some(h) = &self.heat else { return 0.0 };
        let steps = ((now - h.born) / (0.5 * sd)).floor();
        match steps as i64 {
            i64::MIN..=0 => 0.0,
            1 => 0.15,
            2 => 0.30,
            3 => 0.40,
            _ => 0.50,
        }
    }

    /// Total Heat armor strip: the live entity's ramp-up, or the dead
    /// entity's ramp-down tail (whichever strips more if both exist).
    fn heat_strip(&self, now: f64, sd: f64) -> f64 {
        let up = self.heat_ramp_up(now, sd);
        let down = match self.heat_decay {
            Some((t0, s0)) if now >= t0 => {
                let steps = ((now - t0) / (HEAT_STRIP_DECAY_INTERVAL * sd)).floor() as usize;
                let start = HEAT_STRIP_DECAY
                    .iter()
                    .position(|&s| s <= s0 + 1e-9)
                    .unwrap_or(HEAT_STRIP_DECAY.len() - 1);
                HEAT_STRIP_DECAY[(start + steps).min(HEAT_STRIP_DECAY.len() - 1)]
            }
            _ => 0.0,
        };
        up.max(down)
    }

    fn corrosive_strip(&self) -> f64 {
        match self.corrosion.len() {
            0 => 0.0,
            n => (0.20 + 0.06 * n as f64).min(1.0),
        }
    }

    /// FLENSING SPIKES: a WEAPON removing armour off a status that strips
    /// none by itself. `per` is the perk's rate per live Puncture stack, and
    /// Puncture caps at five — so 20% a stack is the whole of the armour at the
    /// cap, which is what the card adds up to and not a rounding of it.
    fn puncture_strip(&self, per: f64) -> f64 {
        if per <= 0.0 {
            return 0.0;
        }
        (per * self.weakened.len() as f64).min(1.0)
    }

    /// Prune and compute the live mitigation snapshot for `now`.
    fn mitigation(&mut self, now: f64, sd: f64, puncture_strip_per: f64) -> Mitigation {
        self.prune(now, sd);
        Mitigation {
            disrupt_amp: ten_stack_amp(self.disrupt.len()),
            virus_amp: ten_stack_amp(self.virus.len()),
            // THREE SOURCES, MULTIPLIED — the two the game strips with and the
            // one a perk grants. They compose the way the first two already
            // did, rather than sharing a bucket: each removes a share of what
            // is LEFT, which is what "remove 20% of enemy Armor" means when
            // something else already removed some.
            armor_multiplier: (1.0 - self.heat_strip(now, sd))
                * (1.0 - self.corrosive_strip())
                * (1.0 - self.puncture_strip(puncture_strip_per)),
        }
    }

    /// Distinct status TYPES currently on the target (Condition Overload's
    /// multiplier input). Assumes `prune` ran at this instant.
    fn distinct_statuses(&self) -> usize {
        let mut n = 0;
        n += usize::from(!self.stagger.is_empty());
        n += usize::from(!self.weakened.is_empty());
        n += usize::from(!self.freeze.is_empty());
        n += usize::from(!self.disrupt.is_empty());
        n += usize::from(!self.virus.is_empty());
        n += usize::from(!self.corrosion.is_empty());
        n += usize::from(!self.confusion.is_empty());
        // Void counts. The mod's own list says so in full — "Impact, Puncture,
        // Slash, Cold, Electricity, Heat, Toxin, Blast, Corrosive, Gas,
        // Magnetic, Radiation, Viral, Void and Tau procs all count for the
        // damage bonus" — and it is the only damage a Bullet Attractor is worth
        // in this arena.
        n += usize::from(!self.attractor.is_empty());
        n += usize::from(!self.blast.is_empty());
        n += usize::from(self.heat.is_some());
        n += usize::from(self.frozen_until.is_some());
        let mut seen: Vec<DamageType> = Vec::new();
        for d in &self.dots {
            if d.ticks_left > 0 && !seen.contains(&d.dtype) {
                seen.push(d.dtype);
            }
        }
        n + seen.len()
    }
}

/// One aimable location on the target (wiki `Enemy_Body_Parts`).
#[derive(Debug, Clone)]
pub struct BodyPart {
    pub name: String,
    /// Relative probability of a shot landing here (weights are normalized).
    pub aim_weight: f64,
    /// Location damage multiplier.
    pub multiplier: f64,
    /// True head: fires on-headshot effects (`Hit::headshot`). Other weak
    /// spots never trigger headshot conditions.
    pub is_head: bool,
    /// Eligible for the critical-location bonus (the `2*cd` fold-in). False
    /// for e.g. MOA fanny packs and helmeted Corpus heads; locations at 1x
    /// never get the bonus regardless of this flag.
    pub crit_bonus: bool,
}

/// Parameters of the dummy engagement.
#[derive(Debug, Clone)]
pub struct DummyParams {
    /// The weapon's (modded) base damage vector. Quantized once per run for
    /// dealing damage and proc-type weighting.
    pub damage: DamageVector,
    /// RESOLVED (modded) crit chance of the DIRECT part — the name is
    /// historical; `unmodded_crit_chance` below is the real base.
    pub base_crit_chance: f64,
    /// MOD SET bonus: chance for a hit that ALREADY crit to move up one
    /// critical tier (Vigilante). 0.0 = no set member equipped.
    pub crit_tier_upgrade_chance: f64,
    /// Hunter Munitions: chance for a CRITICAL hit to apply a Slash status,
    /// rolled per pellet and independent of status chance.
    pub slash_on_crit: f64,
    pub crit_multiplier: f64,
    /// PRELUDE OF MIGHT's live gate — `(the part of `crit_multiplier` this
    /// perk is, the crit-chance threshold)`. Carried rather than settled
    /// because the condition is read at the moment of the hit; see
    /// [`crate::loadout::ResolvedPanel::crit_mult_below_cc`]. Per FORM, which
    /// is what a cycle needs: the Furis's base form sits at 5% and its
    /// Incarnon form at 26%, so the same Weakened stacks push one over the
    /// 40% line and leave the other under it.
    pub crit_mult_below_cc: Option<(f64, f64)>,
    /// UNMODDED crit stats of the DIRECT part — the bases a RELATIVE live crit
    /// buff multiplies (the radial carries its own pair on `ResolvedRadial`).
    pub unmodded_crit_chance: f64,
    pub unmodded_crit_damage: f64,
    /// Listed status chance per hit (may exceed 1.0).
    pub status_chance: f64,
    /// UNMODDED status chance — the base a RELATIVE live status-chance buff
    /// (Primary Crux) multiplies, exactly like `base_multishot`.
    pub base_status_chance: f64,
    /// Forced procs on every hit (weapon data, per attack part).
    pub forced_procs: Vec<DamageType>,
    /// Status duration multiplier (1.0 = unmodded).
    pub status_duration_mult: f64,
    /// Base fire rate; multiplied live by BuffBar fire-rate multipliers
    /// (Frenzy x2.5) to schedule the next shot.
    pub fire_rate: f64,
    /// CHARGE trigger (bows): the modded draw before the shot. When `Some` it
    /// REPLACES `1 / fire_rate` as the interval between pulls — the weapon
    /// fires the moment the draw finishes — and live fire-rate buffs divide it
    /// by the same factor they would have multiplied the rate by. `fire_rate`
    /// stays the listed stat, which is what Hemorrhage's below-2.5 gate reads.
    ///
    /// A bow's magazine is 1, so the reload lands between two draws either way
    /// and the cycle is `charge + reload` however the two are ordered.
    pub charge_seconds: Option<f64>,
    /// Which charge formula paces the shot — see `ChargeCadence`.
    pub charge_cadence: crate::weapons_data::ChargeCadence,
    /// A RATE THAT FALLS WHILE THE TRIGGER IS HELD — see
    /// [`crate::weapons_data::SustainedFireRate`]. It scales the CADENCE and
    /// nothing else: `fire_rate` stays the listed stat, which is what
    /// Hemorrhage's below-2.5 gate reads and what the panel prints, exactly as
    /// on a charge weapon.
    pub sustained_fire_rate: Option<crate::weapons_data::SustainedFireRate>,
    /// A MAGAZINE THAT REFILLS ITSELF — see [`crate::weapons_data::Battery`].
    /// The EMPTY case is already `reload_seconds` (delay + a full refill); what
    /// this adds is the BETWEEN-SHOTS one, which is where the mechanic stops
    /// being a differently-spelled reload.
    pub battery: Option<crate::weapons_data::Battery>,
    /// A BURST trigger's modded shape — see [`crate::weapons_data::BurstSpec`].
    pub burst: Option<crate::weapons_data::BurstSpec>,
    /// Whether the weapon's Frenzy passive is equipped (Dual Toxocyst base
    /// form). Wired: fire-rate x2.5 on true headshots (3 s, refreshable).
    /// NOT yet wired: +100% Toxin injection (needs the element layer) and
    /// ammo efficiency (ammo is infinite here anyway).
    pub frenzy: bool,
    /// Stats an equipped mod has LOCKED at the weapon's default — the panel's
    /// [`crate::loadout::ResolvedPanel::locked`], carried in because the panel's
    /// arithmetic is not the whole of the stat.
    ///
    /// "Equipping this mod will set weapon's Fire Rate to its default ignoring
    /// other bonuses, even negative effects" (wiki, the Cannonades); the Acuity
    /// pair says it of Multishot. `resolve` handles what it can see; the live
    /// sources are HERE — an arcane's multishot stacks and the Frenzy passive's
    /// x2.5 — and a lock that stopped at the mod bucket left them paying
    /// (user, 2026-08-04: "应该要锁定的，好像没锁").
    pub locked_stats: Vec<&'static str>,
    /// Buff-lock settings (see [`LockMode`]).
    pub locked_buffs: Vec<BuffLock>,
    /// The real Incarnon two-form cycle; `None` = single-phase run.
    pub cycle: Option<IncarnonCycle>,
    /// The RADIAL (AoE) attack part, fired by every projectile that lands
    /// (Laetum Incarnon: 300 Radiation beside the 100 Impact direct hit).
    /// The directly-hit enemy takes both (MECHANICS §7). It carries its own
    /// crit stats, never takes a body-part multiplier and never feeds
    /// Condition Overload.
    ///
    /// It is resolved as a SEPARATE damage instance — wiki (Laetum):
    /// "Initial hit and explosion apply status separately" — so it rolls
    /// its own crit tier and draws its own procs from its own damage
    /// vector. Those procs land on the same target and therefore DO feed
    /// Condition Overload on subsequent direct hits.
    pub radial: Option<crate::loadout::ResolvedRadial>,
    /// The LINGERING FIELD every landed projectile leaves (Torid's Toxin
    /// cloud). A third kind of attack part: it persists and TICKS instead of
    /// landing once, and each tick is a full damage instance — own crit roll,
    /// own status draw ("Toxin clouds can proc Hunter Munitions on each tick
    /// of damage"), the weapon's mod buckets, and Condition Overload live off
    /// the target's current status count. MECHANICS §7.
    pub lingering: Option<crate::loadout::ResolvedLingering>,
    /// CONTINUOUS (beam) weapon: `fire_rate` is ticks per second, and multishot
    /// beams on one target MERGE into a single damage instance.
    pub continuous: bool,
    /// Renewed Horror: what a reload-from-EMPTY does to the NEXT shot's field
    /// duration (1.0 = none). ✅ measured x2 (M13).
    pub field_duration_on_empty_reload: f64,
    /// Final Fusillade: FLAT multishot added on the magazine's last round only
    /// (0.0 = none). Base form only — the evolution loader already dropped it
    /// on a charge-backed form, so this field just carries what survived.
    pub multishot_on_last_round: f64,
    /// The same window in the BASE bracket — see
    /// [`crate::loadout::ResolvedPanel::base_multishot_on_last_round`]. It is
    /// carried separately rather than folded into the panel's `multishot`
    /// because it is conditional on the magazine position, which only the sim
    /// can evaluate.
    pub base_multishot_on_last_round: f64,
    /// Plentiful Mayhem: +v damage on multishot-GENERATED projectiles, and
    /// multishot spends ammo to make them (0.0 = none). See `run_once` for the
    /// two per-form rules this drives.
    pub multishot_ammo_bonus: f64,
    /// Evolution headshot-damage bonus (Caput Mortuum) — joins the
    /// headshot bracket. Direct hits only; a radial never headshots.
    pub headshot_damage_bonus: f64,
    /// The weapon's innate headshot bonus MULTIPLIES the additive bracket
    /// rather than joining it (wiki, Cernos Prime — a per-weapon anomaly).
    pub headshot_bonus_multiplicative: bool,
    /// Devouring Attrition: (chance, bonus) rolled on every instance that
    /// did NOT crit — its own multiplier, on the direct hit AND the radial.
    pub noncrit_bonus: Option<(f64, f64)>,
    /// Overwhelming Attrition: a hit that neither crits nor applies a
    /// status grants a stack worth `+per_stack` damage; on timeout ONE
    /// stack drops and the timer resets. The buff multiplies subsequent
    /// instances, the radial part included.
    /// Every stacking buff this build grants, keyed by its own id — see
    /// [`crate::loadout::StackingBuff`]. The roster, the config reader and the
    /// sampler all walk THIS, which is what stops a buff from existing in one
    /// of them and not the others.
    pub stacking_buffs: Vec<crate::loadout::StackingBuff>,
    /// Magazine size; when it runs dry a reload (below) blocks firing.
    pub magazine_size: f64,
    pub reload_seconds: f64,
    /// Default: infinite reserve ammo (decision 2026-07-24). Toggle off to
    /// simulate finite reserves - firing stops when magazine + reserve are
    /// both dry (DoTs keep ticking).
    pub infinite_reserve: bool,
    /// Ammo a shot COSTS (a beam tick included). 1.0 for almost everything;
    /// the wiki states 0.5 per trace for a beam and 10 for the Larkspur
    /// Prime's alt-fire. Multiplies the magazine spend, so it changes how
    /// often a weapon reloads even when the reserve is infinite.
    pub ammo_cost: f64,
    /// Reserve pool, consumed by reloads when `infinite_reserve` is off.
    pub reserve_ammo: f64,
    /// Whether BuffBar ammo efficiency (Frenzy's +100%) reduces consumption.
    /// False for charge-backed magazines (Incarnon) - they are outside the
    /// ammo economy entirely.
    pub ammo_efficiency_applies: bool,
    /// Multishot: pellets per trigger pull = floor + fractional chance
    /// (wiki Multishot). Each pellet is an independent damage instance
    /// (own crit roll, own part, own status roll); ammo cost and Hit
    /// events stay per pull (hitscan pellets are not separate Hits).
    pub multishot: f64,
    /// UNMODDED pellet count — the base a relative arcane multishot buff
    /// (Conjunction Voltage) multiplies live.
    pub base_multishot: f64,
    /// Σ base-damage bonuses on the panel — needed live when CO joins
    /// this bucket (the vector already includes it; only the CO ratio
    /// reads it).
    pub base_damage_bonus: f64,
    /// Condition Overload payload (assumed-max Σ per_stack × stacks),
    /// applied per `co_behavior`, direct hits only.
    pub co_per_type: f64,
    /// PER-WEAPON CO class (user, 2026-07-24): additive with base damage,
    /// an independent multiplier, or inert on this weapon.
    pub co_behavior: crate::loadout::CoBehavior,
    /// CO base effectiveness (wiki: the CO bonus excludes evolution flat
    /// damage — DT with Fevered = 75/125 = 0.6).
    pub co_base_fraction: f64,
    /// The evolution's PERMANENT stacked multishot (Fevered Frenzy): its
    /// full contribution is already inside `multishot`; `apply_buff_config`
    /// rescales it by the configured stacks ("evo_multishot"). No live
    /// machinery — the trigger (ability cast) cannot fire in the sim and the
    /// stacks never decay, so the count is static for the whole run.
    pub evo_ms: Option<crate::loadout::EvoMsBuff>,
    /// The evolution's PERMANENT flat base damage (Reified Bane): the vector
    /// already carries it, and `apply_buff_config` scales it back out.
    pub evo_bd: Option<crate::loadout::EvoBdBuff>,
    /// Live on-kill CO stacks, live per StackSpec (Emergent policy).
    pub co_stack: Option<crate::loadout::StackSpec>,
    /// Live on-kill multishot stacks, earned from zero.
    pub ms_stack: Option<crate::loadout::StackSpec>,
    /// Crosshairs on-headshot buff: absolute crit chance as a timed buff.
    pub cc_on_headshot: Option<crate::loadout::TimedBuff>,
    /// Crosshairs on-headshot-kill stacks: absolute cc per stack,
    /// per-stack expiry (FIFO), NOT the lose-one-reset decay.
    pub cc_stack: Option<crate::loadout::StackSpec>,
    /// (1 + status-damage bonuses): scales every status payload value.
    pub status_damage_mult: f64,
    /// (element, 1 + Σ its bonuses) brackets for elemental DoT ticks.
    pub elem_dot_bonus: Vec<(DamageType, f64)>,
    /// (1 + Σ faction bonuses matching THIS target's faction) — the resolved
    /// faction-damage multiplier (System A). 1.0 vs a non-matching / Unknown
    /// faction. Applied ×1 on direct hits, ×2 (squared) on DoT/status ticks
    /// (the wiki "double dip"). Computed in `from_panel` from the panel bucket
    /// + the target's faction.
    pub faction_mult: f64,
    /// WARFRAME ABILITY BUFFS running in this fight (`data/abilities/`).
    ///
    /// A property of the FIGHT, not of the build: it arrives on the Arena and
    /// is copied here, so the optimizer scores its candidates under the same
    /// Roar the replay will run (the house rule — the simulator is the truth).
    ///
    /// Each carries its own end time, so they are read AT `t` rather than
    /// folded into a scalar: [`DummyParams::faction_at_time`],
    /// [`DummyParams::ability_final_at`] and
    /// [`DummyParams::ability_element_at`] are the three reads, one per effect
    /// kind, and there is no fourth.
    pub abilities: Vec<crate::abilities_data::ActiveAbility>,
    /// ModifiedBase for status-payload formulas (base × (1 + damage mods),
    /// elemental portions excluded). `None` = the vector total (correct
    /// for purely physical vectors).
    pub dot_modified_base: Option<f64>,
    /// Σ reload-speed bonuses on the panel — needed live when arcane
    /// reload buffs (Merciless r5, Conjunction Voltage stacks) join the
    /// bucket: time = base_reload / (1 + this + arcane additions).
    pub reload_bonus: f64,
    /// Σ LISTED Weak Point damage (Pistol Acuity): +1.5× this on the part
    /// multiplier of true weak points, before the headshot bracket.
    pub weakpoint_damage: f64,
    /// ABSOLUTE crit chance added on weak-point pellets only (Acuity).
    pub weakpoint_cc_rel: f64,
    /// King's Gambit: MULTIPLIES a non-weak-point pellet's crit chance.
    /// 1.0 = ordinary; the card's x0 makes a body crit impossible.
    pub bodyshot_cc_mult: f64,
    /// Galvanic Reload: `(status, chance, rounds)` — a magazine restore rolled
    /// ONCE PER SHOT when the target carries that status.
    pub round_restore_on_status: Option<(DamageType, f64, f64)>,
    /// Exact Penance: the chance a KILL reloads instantly. Rolled off the kill
    /// COUNTER, so a status kill counts — which the card requires.
    pub instant_reload_on_kill: Option<f64>,
    /// Resonant Restore: `(per stack, max stacks)` — the magazine GROWS on each
    /// reload from empty, up to the cap. `per_stack` arrives already scaled by
    /// the magazine mods.
    pub mag_growth_on_empty_reload: Option<(f64, u32)>,
    /// Sharpened Bullets (Emergent): ABSOLUTE crit-damage add as a timed buff
    /// (starts inactive), granted/refreshed on every kill.
    pub cd_on_kill: Option<crate::loadout::TimedBuff>,
    /// Pressurized Magazine (Emergent): ABSOLUTE fire-rate add as a timed buff
    /// (starts inactive), granted on every reload.
    pub fr_on_reload: Option<crate::loadout::TimedBuff>,
    /// Deadly Efficiency: a RELATIVE base-damage bonus whose window opens when
    /// the reload COMPLETES (owner, 2026-08-01), not when the magazine empties.
    pub bd_on_reload: Option<crate::loadout::TimedBuff>,
    /// READY RETALIATION's window: STARTING a reload from empty opens
    /// `duration` seconds of `value` extra reload speed, and the reload that
    /// opened it is the first thing that spends it — the trigger is the reload
    /// ACTION, not its completion (owner, 2026-08-10).
    ///
    /// It reaches the transmute animations too, in BOTH directions. The
    /// Phenmor's page says it "does not affect transition from Incarnon back to
    /// base form"; that is wrong (owner, 2026-08-10) and nothing here could
    /// implement it anyway — this is an ordinary reload-speed bonus and the
    /// revert is an ordinary reload-speed-scaled animation.
    /// READY RETALIATION, as a bonus rather than a window (0.0 = none).
    ///
    /// It is scoped to the RELOAD ACTION — it arrives when the reload starts
    /// and is gone when it ends (owner, 2026-08-11) — so there is no expiry to
    /// carry, nothing to lapse mid-reload, and nothing left over afterwards for
    /// a transmute animation to pick up. A reload from empty is simply faster.
    pub rs_on_reload: f64,
    /// FLENSING SPIKES: armour removed per live Puncture status (0.0 = none).
    /// A third strip source beside Corrosive and Heat, multiplying with them.
    pub armor_strip_per_puncture: f64,
    /// EXECUTIONER'S FORTUNE — see [`crate::loadout::InstantReload`]. Rolled by
    /// the PELLET that headshots, because only there is it known whether the
    /// hit landed in a head and whether it killed.
    pub instant_reload: Option<crate::loadout::InstantReload>,
    /// LINGERING JUDGEMENT — see [`crate::loadout::HeadshotStreak`]. Counted
    /// per PELLET that lands in a head, like every other on-hit trigger here.
    pub headshot_streak: Option<crate::loadout::HeadshotStreak>,
    /// SPITEFUL DEFILEMENT: `(threshold, bonus)` — see
    /// [`crate::loadout::ResolvedPanel::cd_below_status_count`].
    pub cd_below_status_count: Option<(u32, f64)>,
    /// A syndicate augment's radial (Gilded Truth grants Truth) — armed by
    /// AFFINITY this weapon earns, fired on its own cooldown.
    pub syndicate_radial: Option<crate::syndicates_data::SyndicateDef>,
    /// Where a continuous weapon's damage ramp STARTS, as a fraction of full.
    /// 0.20 "for most weapons" (wiki); Phantasma Prime is 0.15.
    pub beam_ramp_floor: f64,
    /// The Ocucor's tendril cap (0 = no tendrils). Their own damage is not
    /// modelled and should not be — see `weapons_data::TendrilSpec`; the COUNT
    /// is what Sentient Surge reads.
    pub tendril_max: u32,
    /// Sentient Surge's crit chance per ACTIVE tendril, relative to the
    /// unmodded base (it joins the crit-chance bucket).
    pub cc_per_tendril: f64,
    /// ...and its status half, same bucket.
    pub sc_per_tendril: f64,
    /// The tendrils the run OPENS with — Sentient Surge's buff card, seeded
    /// exactly like every other buff's stack count. Without it the mod is
    /// unmeasurable in the fights it is played in: a tendril costs a kill and
    /// a reload takes every one back, so at a level where kills are slow the
    /// weapon's only augment contributes nothing and there was no way to say
    /// otherwise (player report, 2026-08-08).
    pub tendrils_initial: u32,
    /// ...and the card's "no timeout". A tendril has no clock — what ENDS it
    /// is the magazine event — so locking it means that event no longer
    /// clears them. Same reading as everywhere else: the count still starts
    /// where the card sets it and still climbs on every kill.
    pub tendrils_held: bool,
    /// ...and the fraction of the magazine a kill puts back.
    pub mag_refill_on_kill: f64,
    /// GOTVA PRIME'S PASSIVE: a pellet that lands a status has `chance` to arm
    /// the NEXT landing pellet's crit chance to `crit_chance`, exactly — the
    /// modded value and every crit bonus are ignored, because the card says
    /// "Set Critical Chance ignores all other modifiers".
    pub super_crit_on_status: Option<crate::weapons_data::SuperCritSpec>,
    /// Hemorrhage's status-conversion roll (per damage instance, max one).
    pub proc_conversion: Option<crate::loadout::ProcConv>,
    /// The equipped secondary arcane, resolved at its rank from
    /// data/arcanes/secondary (fixed equipment per scenario; the optimizer
    /// compares scenarios per arcane). `ArcaneFx::none()` = empty slot.
    pub arcane: ArcaneFx,
    /// Primary Compression's damage bonus when this weapon's row `multiplies`
    /// (1.0 = none) — a FINAL multiplier on the instance, the same slot
    /// Secondary Surge occupies: *"the damage bonus is multiplicative to other
    /// damage bonus sources"*. An `adds` row never reaches here; it is already
    /// inside the base-damage bucket the panel resolved.
    ///
    /// PER FORM. The Torid's cloud pays +240% and its Incarnon beam pays
    /// nothing, so the cycle's two `DummyParams` disagree on purpose.
    pub compression_mult: f64,
    /// The same arcane's `adds` row: a flat addition to the base-damage
    /// bracket, beside a live buff's. Per form, for the same reason.
    pub compression_bd: f64,
    /// See [`crate::loadout::ResolvedPanel::bd_below_half_health`]. Per ATTACK
    /// PART, like every other bracket here, so a radial that the catalog
    /// exempts can carry a different number than the direct hit.
    pub bd_below_half_health: f64,
    /// See [`crate::loadout::ResolvedPanel::cc_on_undamaged`].
    pub cc_on_undamaged: f64,
    /// See [`crate::loadout::ResolvedPanel::cd_on_undamaged`].
    pub cd_on_undamaged: f64,
    /// Secondary Enervate's stack count at t = 0. Its own field because the
    /// ramp lives in a PERK rather than in `arcane.buffs`, so the ordinary
    /// per-buff seeding never reached it — the arcane simply always started
    /// from nothing, with no card to say so (user, 2026-08-03).
    ///
    /// UNCAPPED, like the mechanic: a hit adds a stack with no ceiling until a
    /// big crit wipes the pile. There is no maximum to clamp to and inventing
    /// one would be the model disagreeing with the card.
    pub enervate_stacks: u32,
    pub body_parts: Vec<BodyPart>,
    /// The TARGET — one of the fight's two actors.
    pub target: TargetParams,
    /// The TENNO — the other one. Who is holding this weapon, and what they
    /// are doing: `resolve` has already asked its state which conditional mods
    /// pay, and the arcanes that scale off Warframe armor or energy read its
    /// stats. It rides on the params rather than being resolved away so the
    /// fight can be replayed, reported and shared as what it was: somebody,
    /// shooting somebody (user, 2026-08-02).
    pub tenno: crate::tenno_data::Tenno,
    pub duration_secs: f64,
}

/// ONE FRAME of a replayed engagement: where the fight stood at `t`.
///
/// The frames come from re-running the MEDIAN engagement — the one the result
/// already reports — so the curve a player scrubs is the same fight the
/// headline number came from, not an average of fights that never happened.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub t: f64,
    /// The target's pools as they stood. A respawn (InstantRespawn) shows as
    /// them jumping back up, which is the truth of that scenario.
    pub overguard: f64,
    pub shield: f64,
    pub health: f64,
    /// Cumulative EFFECTIVE damage dealt by `t`, and kills completed.
    pub damage: f64,
    pub kills: u32,
    /// Every counter the RESULT panel reports, as it stood at `t`. A replay
    /// that only moved a cursor would be a decoration; these are what let the
    /// whole panel — KPIs, the damage meter, the curves — be re-read at any
    /// instant of the fight (user, 2026-08-03: "点击 replay，应该可以重新把
    /// 上面的所有复原").
    pub shots: u32,
    pub pellets: u32,
    pub crits: u32,
    pub big_crits: u32,
    pub crit_tier_sum: u32,
    pub headshots: u32,
    pub procs: u32,
    pub field_ticks: u32,
    pub reloads: u32,
    pub transforms: u32,
    /// Effective damage by source, cumulative — the damage meter's own shape.
    pub sources: SourceDamage,
    /// Live stacks per buff, positionally matching [`Replay::buffs`].
    pub stacks: Vec<u8>,
    /// …and the same for the TARGET, positionally matching [`DEBUFF_ROSTER`].
    /// The mirror of the line above, because the page draws one table from each
    /// and the two are the same component (owner, 2026-08-11).
    pub debuffs: Vec<u8>,
}

/// A replay of one engagement: the buff roster it was fought with, and a frame
/// every `dt` seconds.
///
/// Sampling costs ONE extra run, not one per Monte Carlo iteration: `Rng` is
/// SplitMix64 with a single `u64` of state, so a run records the state it
/// STARTED from ([`RunResult::rng_state`]) and can be replayed bit-for-bit
/// afterwards. Carrying frames on every `RunResult` would have cost the trace
/// times `runs` — 20,000 of them at the cap (user, 2026-08-02).
#[derive(Debug, Clone, Default)]
pub struct Replay {
    /// Seconds between frames.
    pub dt: f64,
    /// (buff id, max stacks) — ids are the SAME vocabulary as [`BuffConfig`]
    /// and the web's buff cards, because they come from one place:
    /// [`DummyParams::buff_roster`].
    pub buffs: Vec<(String, u32)>,
    pub frames: Vec<Frame>,
    /// One worked example per attack part — see [`HitAccount`]. First come,
    /// first recorded: the first direct hit of the engagement and the first
    /// explosion, which is enough to check both paths and cheap enough to do
    /// inside the shot loop.
    pub accounts: Vec<HitAccount>,
}

/// ONE DAMAGE INSTANCE, FULLY DECOMPOSED — the account of a single hit.
///
/// Every other number this sim reports is an aggregate, and an aggregate hides
/// an error inside an average: a factor applied twice, or in the wrong bracket,
/// moves a mean by a few per cent and cannot be told from a build being good.
/// This is the one output that can be FALSIFIED — every line is a factor with
/// its value, the product is the number that went into the damage meter, and
/// anyone with the wiki and a calculator can check it by hand (owner,
/// 2026-08-11: "方便我可以根据数据里找出计算瑕疵").
///
/// It is recorded from the MEDIAN ENGAGEMENT, the same run the replay plays
/// back, so the account and the curves are the same fight.
#[derive(Debug, Clone, Default)]
pub struct HitAccount {
    /// Which attack part: `direct`, `radial`.
    pub source: &'static str,
    /// The body part it landed on, and whether that part is a head.
    pub part: String,
    pub head: bool,
    /// The crit TIER this instance rolled — 0 = no crit, 1 = crit, 2+ = red.
    pub tier: u32,
    /// When it happened, so it can be found in the replay.
    pub t: f64,
    /// The instance's own damage before anything below it: this attack part's
    /// modded vector, divided by nothing. On a multishot weapon it is ONE
    /// pellet's share.
    pub base: f64,
    /// `(what it is, the factor)`, in the order the engine applies them. A
    /// factor of exactly 1.0 is kept rather than dropped — "faction ×1.00" is
    /// the answer to "why is my Bane doing nothing", and a missing line is not.
    pub steps: Vec<(&'static str, f64)>,
    /// The product of `base` and every step — what the meter counts as dealt.
    pub raw: f64,
    /// …and what the target actually took, after armour, its damage-type
    /// column and any attenuation. `raw / effective` is the mitigation.
    pub effective: f64,
}

/// Frames in a replay, whatever the engagement length. 600 over 300 s is one
/// every half second — smooth enough to scrub, small enough to ship as JSON.
pub const REPLAY_FRAMES: usize = 600;

/// Per-buff configured policy: buff id → (initial stacks, locked). Ids match
/// the web's `enumerate_buffs` (`condition_overload`, `on_kill_multishot`,
/// `on_headshot_cc`, `on_headshot_kill_cc`, `on_kill_cd`, `on_reload_fr`,
/// `arcane:{id}`). Frenzy is configured via [`LockMode`], not here.
pub type BuffConfig = std::collections::HashMap<String, (u32, bool)>;

impl DummyParams {
    /// Is this stat LOCKED at the weapon's default by an equipped mod?
    ///
    /// One reader for every live source, so "even negative effects" cannot end
    /// up meaning "every source the resolver happened to see".
    pub fn locks(&self, stat: &str) -> bool {
        self.locked_stats.contains(&stat)
    }

    /// EVERY configurable buff this build carries, as `(id, max_stacks)`.
    ///
    /// Deliberately adjacent to [`Self::apply_buff_config`] and written in the
    /// same order off the same fields: the ids are one vocabulary shared by
    /// the config, the web's cards and the replay, and the way to keep three
    /// readers in step is to give them one writer. A buff that gains a config
    /// knob and not a roster entry would be configurable and invisible.
    pub fn buff_roster(&self) -> Vec<(String, u32)> {
        let mut out: Vec<(String, u32)> = Vec::new();
        if self.frenzy {
            out.push(("frenzy".into(), 1));
        }
        if let Some(ms) = self.evo_ms {
            out.push(("evo_multishot".into(), ms.max_stacks));
        }
        // UNCAPPED — 0 means "no ceiling", which the api and the UI both read
        // as such rather than as a maximum of zero.
        if self.arcane.enervate_rank.is_some() {
            out.push(("arcane:secondary_enervate".into(), 0));
        }
        if let Some(s) = &self.co_stack {
            out.push(("condition_overload".into(), s.max_stacks));
        }
        if let Some(s) = &self.ms_stack {
            out.push(("on_kill_multishot".into(), s.max_stacks));
        }
        if self.evo_bd.is_some() {
            out.push(("evo_reload_damage".into(), 1));
        }
        // READY RETALIATION HAS NO CARD any more. It was one while it was a
        // 6 s window that could be up or down; now it lasts exactly as long as
        // the reload that triggers it, which is not a state a player can be
        // caught without and not a stack count anyone can configure. A card
        // reading 0/1 for a perk that works every time would be a lie.
        // LINGERING JUDGEMENT, the same shape: a window that is open or not.
        if self.headshot_streak.is_some() {
            out.push(("evo_headshot_streak".into(), 1));
        }
        if let Some(s) = &self.cc_stack {
            out.push(("on_headshot_kill_cc".into(), s.max_stacks));
        }
        // TENDRILS — the Ocucor's passive, and a buff by every test that
        // matters: it is gained on a trigger (a kill), it is lost on one (a
        // magazine event), and it has a cap. It is rostered only when a mod
        // READS it, because the count buys nothing on its own (the tendrils'
        // own damage is cosmetic on the beam's target) — a card for it with
        // no Sentient Surge equipped would move no number.
        if self.tendril_max > 0 && (self.cc_per_tendril > 0.0 || self.sc_per_tendril > 0.0) {
            out.push(("tendrils".into(), self.tendril_max));
        }
        // EVERY stacking buff, by construction. A new one appears on the
        // replay the moment the data declares it — there is no arm to add.
        for b in &self.stacking_buffs {
            out.push((b.id.into(), b.max_stacks));
        }
        if self.cc_on_headshot.is_some() {
            out.push(("on_headshot_cc".into(), 1));
        }
        if self.cd_on_kill.is_some() {
            out.push(("on_kill_cd".into(), 1));
        }
        if self.bd_on_reload.is_some() {
            out.push(("on_reload_bd".into(), 1));
        }
        if self.fr_on_reload.is_some() {
            out.push(("on_reload_fr".into(), 1));
        }
        // One entry per ARCANE, not per grant — the same rule the cards
        // follow, and for the same reason: Frostbite's crit damage and
        // multishot are one stack count by construction.
        let arcane_id = self.arcane.id.clone();
        for spec in self.arcane.buffs.iter() {
            let owner = if spec.owner.is_empty() { &arcane_id } else { &spec.owner };
            let id = format!("arcane:{owner}");
            if !out.iter().any(|(x, _)| *x == id) {
                out.push((id, spec.max_stacks));
            }
        }
        out
    }

    /// Apply a per-buff configured policy onto the live specs: the card's
    /// stack count becomes the seed, and a LOCKED card OVERWRITES the buff's
    /// duration with [`crate::loadout::NO_TIMEOUT`].
    ///
    /// This function is the whole implementation of locking (user,
    /// 2026-08-04). Nothing downstream knows the concept: every clock in the
    /// sim is `expiry = now + duration`, so an infinite duration is a buff
    /// that earns normally and never falls off. The flag this replaced had to
    /// be re-read at every site that touched a stack count, and was missed at
    /// enough of them that "no timeout" could mean its own opposite.
    ///
    /// Weapon-scoped: recurses into the incarnon cycle's base form.
    pub fn apply_buff_config(&mut self, cfg: &BuffConfig) {
        /// The buff's own duration, or none at all when the card is locked.
        fn clock(duration: f64, locked: bool) -> f64 {
            if locked {
                crate::loadout::NO_TIMEOUT
            } else {
                duration
            }
        }
        fn set_stack(s: &mut crate::loadout::StackSpec, cfg: &BuffConfig, id: &str) {
            if let Some(&(stacks, locked)) = cfg.get(id) {
                s.initial_stacks = stacks.min(s.max_stacks);
                s.duration = clock(s.duration, locked);
            }
        }
        fn set_timed(b: &mut crate::loadout::TimedBuff, cfg: &BuffConfig, id: &str) {
            if let Some(&(stacks, locked)) = cfg.get(id) {
                b.initial_active = stacks > 0;
                b.duration = clock(b.duration, locked);
            }
        }
        // Fevered Frenzy-style permanent stacks: no in-sim trigger, no
        // decay — the configured count is a STATIC multishot choice for the
        // whole run. `locked` is meaningless here (the stacks cannot move
        // either way) and is deliberately ignored.
        // Secondary Enervate: untimed and UNCAPPED, but CONSUMABLE — a big
        // crit resets the pile — so it starts at 0 by the same rule as every
        // timed buff (user, 2026-08-03), and the card can say otherwise.
        if self.arcane.enervate_rank.is_some() {
            if let Some(&(stacks, _)) = cfg.get("arcane:secondary_enervate") {
                self.enervate_stacks = stacks;
            }
        }
        if let Some(bd) = self.evo_bd {
            if let Some(&(stacks, _)) = cfg.get("evo_reload_damage") {
                let stacks = stacks.min(bd.max_stacks);
                // ONE RATIO on the resolved vector. Flat base damage is added
                // pro-rata BEFORE mods, so the bonus rides the whole chain
                // multiplicatively and removing it needs no re-resolve — the
                // same argument `evo_multishot` makes for its scalar.
                let frac = f64::from(stacks) / f64::from(bd.max_stacks);
                let (now, want) = (bd.without + bd.full, bd.without + bd.full * frac);
                if now > 0.0 && want < now {
                    let k = want / now;
                    self.damage = self.damage.scale(k);
                    if let Some(d) = self.dot_modified_base.as_mut() {
                        *d *= k;
                    }
                }
                if let Some(b) = self.evo_bd.as_mut() {
                    b.stacks = stacks;
                }
            }
        }
        if let Some(ms) = self.evo_ms {
            if let Some(&(stacks, _)) = cfg.get("evo_multishot") {
                let stacks = stacks.min(ms.max_stacks);
                let frac = f64::from(stacks) / f64::from(ms.max_stacks);
                self.multishot -= ms.full * (1.0 - frac);
                if let Some(m) = self.evo_ms.as_mut() {
                    m.stacks = stacks;
                }
            }
        }
        if let Some(s) = self.co_stack.as_mut() {
            set_stack(s, cfg, "condition_overload");
        }
        if let Some(s) = self.ms_stack.as_mut() {
            set_stack(s, cfg, "on_kill_multishot");
        }
        if let Some(s) = self.cc_stack.as_mut() {
            set_stack(s, cfg, "on_headshot_kill_cc");
        }
        // The tendril count, seeded like any stack count. `locked` cannot be a
        // duration here — a tendril has no clock, it is cleared by a magazine
        // event — so it lands on the thing that ENDS this buff instead, which
        // is the same statement everywhere else makes: nothing takes it away.
        if let Some(&(stacks, locked)) = cfg.get("tendrils") {
            self.tendrils_initial = stacks.min(self.tendril_max);
            self.tendrils_held = locked;
        }
        // Every stacking buff takes the same two knobs the Galvanized family
        // does, and takes them by ID — so a buff the data adds is configurable
        // without a line here.
        for b in self.stacking_buffs.iter_mut() {
            if let Some(&(stacks, locked)) = cfg.get(b.id) {
                b.initial_stacks = stacks.min(b.max_stacks);
                b.duration = clock(b.duration, locked);
            }
        }
        if let Some(b) = self.cc_on_headshot.as_mut() {
            set_timed(b, cfg, "on_headshot_cc");
        }
        if let Some(b) = self.cd_on_kill.as_mut() {
            set_timed(b, cfg, "on_kill_cd");
        }
        if let Some(b) = self.fr_on_reload.as_mut() {
            set_timed(b, cfg, "on_reload_fr");
        }
        // Deadly Efficiency. It is in `buff_roster` and it gets a card, so
        // this arm is what makes that card mean anything — without it both
        // knobs were read, drawn, and dropped.
        if let Some(b) = self.bd_on_reload.as_mut() {
            set_timed(b, cfg, "on_reload_bd");
        }
        // Keyed off the buff's OWN arcane — not the merged set's id, because a
        // weapon may seat two (an Arch-Gun) and every buff would be renamed the
        // moment a second joined; and not by index either, because ONE ARCANE
        // IS ONE CARD (user, 2026-08-02). Frostbite's crit damage and multishot
        // come off the same Cold proc and are the same count by construction,
        // so one setting drives every spec that arcane owns.
        let arcane_id = self.arcane.id.clone();
        for spec in self.arcane.buffs.iter_mut() {
            let owner = if spec.owner.is_empty() { &arcane_id } else { &spec.owner };
            if let Some(&(stacks, locked)) = cfg.get(&format!("arcane:{owner}")) {
                spec.initial_stacks = stacks.min(spec.max_stacks);
                spec.duration = clock(spec.duration, locked);
            }
        }
        if let Some(cy) = self.cycle.as_mut() {
            cy.base_form.apply_buff_config(cfg);
        }
    }

    /// A generic humanoid: body 1x, head 3x (headshot-triggering, crit-bonus
    /// eligible), aimed at 50/50.
    pub fn humanoid_parts() -> Vec<BodyPart> {
        vec![
            BodyPart {
                name: "body".into(),
                aim_weight: 0.5,
                multiplier: 1.0,
                is_head: false,
                crit_bonus: false,
            },
            BodyPart {
                name: "head".into(),
                aim_weight: 0.5,
                multiplier: 3.0, // humanoid head (wiki: Enemy_Body_Parts)
                is_head: true,
                crit_bonus: true,
            },
        ]
    }

    /// Dual Toxocyst base form damage vector — TEST FIXTURE (the engine
    /// proper knows no specific weapon; production callers build params via
    /// `from_panel` on a data-resolved panel).
    #[cfg(test)]
    pub fn dual_toxocyst_base_vector() -> DamageVector {
        DamageVector::new()
            .with(DamageType::Impact, 7.5)
            .with(DamageType::Puncture, 60.0)
            .with(DamageType::Slash, 7.5)
    }

    /// Dual Toxocyst **base form** as played: Frenzy passive + the chosen
    /// build (Commodore's Fortune + Evolved Autoloader + Fevered Frenzy):
    /// +50 base damage scales the vector pro-rata (75 -> 125, x5/3) and
    /// Commodore's Fortune sets base crit to 25%. Evolution layers apply
    /// to BOTH guns of the transform group.
    #[cfg(test)]
    pub fn dual_toxocyst_base() -> Self {
        Self {
            damage: Self::dual_toxocyst_base_vector().scale(125.0 / 75.0),
            base_crit_chance: 0.25,
            frenzy: true,
            // Sim settings (user, 2026-07-24): Frenzy locked at 100% uptime
            // and Fevered Frenzy pre-stacked to 20 (+100% multishot).
            locked_buffs: vec![BuffLock::permanent(LockedBuff::Frenzy)],
            multishot: 2.0,
            ..Self::default()
        }
    }

    /// Dual Toxocyst **Incarnon Form** (data module: 15 I / 37.5 P / 22.5 S,
    /// 11% crit, 3.0x, 43% status, 4.5 fire rate, full-auto). Frenzy WORKS
    /// while transformed (user-confirmed 2026-07-24) — natural headshot
    /// trigger wired here; its Toxin injection needs the loadout layer, so
    /// this bare profile omits it.
    /// The gauge/ammo economy (9 weakpoint charges, 30 rounds each, max 270)
    /// is not cycled here: this profile measures the form in isolation.
    #[cfg(test)]
    pub fn dual_toxocyst_incarnon() -> Self {
        Self {
            // 15/37.5/22.5 x 5/3 (Fevered Frenzy +50 base, pro-rata).
            damage: DamageVector::new()
                .with(DamageType::Impact, 25.0)
                .with(DamageType::Puncture, 62.5)
                .with(DamageType::Slash, 37.5),
            base_crit_chance: 0.31, // 11% + Commodore's Fortune +20 (build)
            crit_multiplier: 3.0,
            status_chance: 0.43,
            fire_rate: 4.5,
            frenzy: true,
            // Pseudo-reload model (gauge locked full): 270 charge-backed
            // rounds, downtime = revert (1.0 s, M9-measured) + re-transmute
            // (2.35 s = base reload).
            magazine_size: 270.0,
            reload_seconds: 3.35,
            ammo_efficiency_applies: false,
            // Fevered Frenzy pre-stacked to 20 (+100% multishot) - the
            // evolution buff applies to both guns of the group.
            multishot: 2.0,
            ..Self::default()
        }
    }

    /// THE FACTION BRACKET AT `t`, Roar included.
    ///
    /// `faction_mult` is `1 + Σ bonuses`, and Roar is a bonus in that same
    /// bucket ("considered Faction Damage Bonus, additive with other sources of
    /// Faction Damage" — wiki), so it ADDS to the sum rather than multiplying
    /// the result. Everything the bracket already does then happens to it for
    /// free, the status double-dip included.
    pub fn faction_at_time(&self, t: f64) -> f64 {
        self.faction_mult + crate::abilities_data::faction_bonus_at(&self.abilities, t)
    }

    /// ECLIPSE'S OWN MULTIPLIER at `t`, or 1.0. Applied ONCE wherever it is
    /// applied — the wiki draws the contrast itself: "Unlike faction damage,
    /// which double dips for status effects, the one from Eclipse is applied
    /// once."
    pub fn ability_final_at(&self, t: f64) -> f64 {
        crate::abilities_data::final_mult_at(&self.abilities, t)
    }

    /// The ability-added share of ONE element's bonus bracket at `t`.
    ///
    /// "Additive with elemental mods" (every one of the four augment pages),
    /// so it lands in the same `1 + Σ` a Stormbringer does — which means it
    /// raises that element's DoT tick as well as adding damage. The repo has
    /// the precedent: `injected_elements` takes the identical line from
    /// frenzy.yaml for the identical reason.
    pub fn ability_element_at(&self, ty: DamageType, t: f64) -> f64 {
        crate::abilities_data::added_elements_at(&self.abilities, t)
            .iter()
            .filter(|(e, _)| *e == ty)
            .map(|(_, v)| v)
            .sum()
    }

    /// The finished vector with the ability elements ON TOP — never through
    /// [`crate::elements::combine`], because they do not combine (owner,
    /// 2026-08-08: "注意不合成"). A weapon whose mods make Radiation and whose
    /// squad has Volt deals Radiation AND pure Electricity.
    ///
    /// `stage_mb` is THAT attack part's ModifiedBase: an explosion's elemental
    /// mods are a percentage of the explosion's own base (MECHANICS §7), and
    /// an ability sized "additive with elemental mods" is sized the same way.
    fn with_ability_elements(&self, qvec: DamageVector, stage_mb: f64, t: f64) -> DamageVector {
        let added = crate::abilities_data::added_elements_at(&self.abilities, t);
        if added.is_empty() {
            return qvec;
        }
        let mut out = qvec;
        for (ty, frac) in added {
            out.add(ty, stage_mb * frac);
        }
        out.quantized()
    }

    /// Build engagement params from a resolved mod loadout (pipeline
    /// [1]+[2] output). Bare-frame scenario: no arcanes, no Frenzy passive
    /// (Incarnon Form), infinite reserve.
    /// A BUILD MEETS AN ARCANE HERE, and only here.
    ///
    /// The arcane is an argument rather than something the caller assigns
    /// afterwards, because two of its answers are not the arcane's alone and
    /// every site that assigned it had to remember both: Primary Compression is
    /// worth what THIS build's blast radius is worth, and a stat LOCK (Acuity)
    /// silences an arcane's buff exactly as it silences a mod's. Three sites
    /// built params — `simulate_json`, the optimizer's scorer, the tests — and
    /// the Incarnon cycle's inner base form was assigned by none of them
    /// (owner, 2026-08-11: "resolve_for 收赋能列表"; it landed one layer down,
    /// where the optimizer's one-panel-many-arcanes pairing actually happens).
    pub fn from_panel(
        panel: &crate::loadout::ResolvedPanel,
        arena: &crate::arena::Arena,
        arcane: &ArcaneFx,
    ) -> Self {
        // PRIMARY COMPRESSION: the panel brings the metres, the arcane brings
        // what a metre is worth. `adds` joins the live base-damage bracket
        // (diluted by Serration, and it reaches status payloads through
        // ModifiedBase); `multiplies` is a final multiplier on the instance,
        // the slot Secondary Surge occupies.
        let (compression_mult, compression_bd) = match panel.compression {
            Some(c) => {
                let bonus = arcane.compression_dmg_per_m * c.radius_lost;
                if c.adds { (1.0, bonus) } else { (1.0 + bonus, 0.0) }
            }
            None => (1.0, 0.0),
        };
        let arcane = {
            let mut fx = arcane.clone().without_locked(&panel.locked);
            fx.ammo_efficiency += panel
                .compression
                .map_or(0.0, |c| arcane.compression_eff_per_m * c.radius_lost);
            fx
        };
        let crate::arena::Arena { tenno, target, body_parts, duration_secs, abilities } =
            arena.clone();
        // Resolve the faction bucket against THIS target's faction (additive
        // within the matching faction; 1.0 vs a non-match / Unknown).
        let faction_mult = 1.0
            + panel
                .faction_damage
                .iter()
                .filter(|(f, _)| *f == target.faction)
                .map(|(_, v)| v)
                .sum::<f64>();
        Self {
            faction_mult,
            // Straight off the ARENA — the one place a fight is described.
            abilities: abilities.clone(),
            damage: panel.damage,
            radial: panel.radial,
            lingering: panel.lingering,
            continuous: panel.continuous,
            field_duration_on_empty_reload: panel.field_duration_on_empty_reload,
            multishot_on_last_round: panel.multishot_on_last_round,
            base_multishot_on_last_round: panel.base_multishot_on_last_round,
            multishot_ammo_bonus: panel.multishot_ammo_bonus,
            compression_mult,
            compression_bd,
            bd_below_half_health: panel.bd_below_half_health,
            cc_on_undamaged: panel.cc_on_undamaged,
            cd_on_undamaged: panel.cd_on_undamaged,
            arcane,
            headshot_damage_bonus: panel.headshot_damage_bonus,
            headshot_bonus_multiplicative: panel.headshot_bonus_multiplicative,
            noncrit_bonus: panel.noncrit_bonus,
            stacking_buffs: panel.stacking_buffs.clone(),
            base_crit_chance: panel.crit_chance,
            crit_multiplier: panel.crit_damage,
            crit_mult_below_cc: panel.crit_mult_below_cc,
            unmodded_crit_chance: panel.base_crit_chance,
            unmodded_crit_damage: panel.base_crit_damage,
            status_chance: panel.status_chance,
            base_status_chance: panel.base_status_chance,
            fire_rate: panel.fire_rate,
            charge_seconds: panel.charge_seconds,
            charge_cadence: panel.charge_cadence,
            sustained_fire_rate: panel.sustained_fire_rate,
            battery: panel.battery,
            burst: panel.burst,
            frenzy: false,
            magazine_size: panel.magazine_size,
            reload_seconds: panel.reload_seconds,
            // A CHARGE-BACKED form is "not affected by Ammo Efficiency" (wiki,
            // Torid Incarnon) and every other weapon is. `incarnon.is_some()`
            // is the same marker the magazine rule reads, so the two cannot
            // disagree about which pool is outside the ammo economy.
            //
            // This was hardcoded `false`, which switched ammo efficiency off
            // for EVERY weapon the API simulates — Primary Crux's +60% did
            // nothing, and neither did Frenzy's or a Deadly-Efficiency build.
            // Nothing caught it because every test here builds `DummyParams`
            // by hand, where the field defaults to `true` (2026-08-01).
            ammo_efficiency_applies: panel.incarnon.is_none(),
            multishot: panel.multishot,
            base_multishot: panel.base_multishot,
            evo_ms: panel.evo_ms,
            evo_bd: panel.evo_bd,
            base_damage_bonus: panel.base_damage_bonus,
            co_per_type: panel.co_per_type,
            co_behavior: panel.co_behavior,
            co_base_fraction: panel.co_base_fraction,
            co_stack: panel.co_stack,
            ms_stack: panel.ms_stack,
            cc_on_headshot: panel.cc_on_headshot,
            cc_stack: panel.cc_stack,
            status_damage_mult: panel.status_damage_mult,
            status_duration_mult: panel.status_duration_mult,
            elem_dot_bonus: panel.elem_dot_bonus.clone(),
            dot_modified_base: Some(panel.modified_base),
            reload_bonus: panel.reload_bonus,
            weakpoint_damage: panel.weakpoint_damage,
            crit_tier_upgrade_chance: panel.crit_tier_upgrade_chance,
            slash_on_crit: panel.slash_on_crit,
            weakpoint_cc_rel: panel.weakpoint_cc_rel,
            bodyshot_cc_mult: panel.bodyshot_cc_mult,
            round_restore_on_status: panel.round_restore_on_status,
            instant_reload_on_kill: panel.instant_reload_on_kill,
            mag_growth_on_empty_reload: panel.mag_growth_on_empty_reload,
            cd_on_kill: panel.cd_on_kill,
            fr_on_reload: panel.fr_on_reload,
            bd_on_reload: panel.bd_on_reload,
            rs_on_reload: panel.rs_on_reload,
            armor_strip_per_puncture: panel.armor_strip_per_puncture,
            instant_reload: panel.instant_reload,
            headshot_streak: panel.headshot_streak,
            cd_below_status_count: panel.cd_below_status_count,
            super_crit_on_status: panel.super_crit_on_status,
            beam_ramp_floor: panel.beam_ramp_floor,
            syndicate_radial: panel.syndicate_radial,
            forced_procs: panel.forced_procs.clone(),
            tendril_max: panel.tendril_max,
            cc_per_tendril: panel.cc_per_tendril,
            sc_per_tendril: panel.sc_per_tendril,
            // EARNED, like every other timed buff: a fight that has not been
            // in contact has no tendrils up. The card moves it.
            tendrils_initial: 0,
            tendrils_held: false,
            mag_refill_on_kill: panel.mag_refill_on_kill,
            proc_conversion: panel.proc_conversion,
            enervate_stacks: 0,
            body_parts,
            target,
            duration_secs,
            locked_stats: panel.locked.clone(),
            locked_buffs: Vec::new(),
            cycle: None,
            // A weapon runs dry only where the game gives no way to resupply
            // (a ground Arch-Gun). Everywhere else the reserve is a panel
            // figure and the sim keeps firing — we do not model pickups, so
            // stopping would be an artefact of the model, not the game.
            // THE WEAPON-ONLY ANSWER: no reserve at all, or one the game
            // refills. A scenario's Infinite-ammo setting is applied on top of
            // this by the caller — see `parse_fight` — and cannot give ammo
            // back to a weapon that has no way to get any.
            infinite_reserve: !panel.has_reserve || !panel.no_resupply,
            ammo_cost: panel.ammo_cost,
            reserve_ammo: panel.ammo_reserve,
            tenno,
        }
    }

    /// The REAL Incarnon cycle engagement from both forms' resolved panels
    /// (user flow, 2026-07-24): start transformed with a full gauge; dump
    /// the charge magazine; revert; rebuild 9 weakpoint charges in the
    /// base form (Frenzy per `frenzy_lock`); transmute; repeat. Both
    /// transitions scale by the reload formula (M9).
    ///
    /// `frenzy` is the WEAPON's passive, not a constant: it belongs to
    /// whichever weapon lists the perk (Dual Toxocyst does, the Laetum does
    /// not). Hardcoding it here handed DT's ×2.5-on-headshot fire rate to
    /// every transform weapon and made the caller's on/off knob dead in
    /// cycle mode.
    /// THE CYCLE ARMS BOTH FORMS. One arcane, two answers: the Torid's cloud
    /// pays Primary Compression +240% and its Incarnon beam pays nothing, so
    /// each form spends the arcane against its OWN radius.
    pub fn incarnon_cycle_from_panels(
        incarnon: &crate::loadout::ResolvedPanel,
        base: &crate::loadout::ResolvedPanel,
        frenzy: bool,
        frenzy_lock: LockMode,
        arena: &crate::arena::Arena,
        arcane: &ArcaneFx,
    ) -> Self {
        let rl = 1.0 + incarnon.reload_bonus;
        let inc_form = incarnon.incarnon;
        let base_form = DummyParams {
            frenzy,
            // No ammo-efficiency override here any more: `from_panel` derives
            // it from the panel, and a hardcoded `true` would outrank the data
            // the day a base form became charge-backed. That override existed
            // only to compensate for the broken default it sat next to.
            ..Self::from_panel(base, arena, arcane)
        };
        Self {
            // Frenzy exists in BOTH forms (user-confirmed 2026-07-24) — when
            // the weapon HAS it.
            frenzy,
            locked_buffs: vec![BuffLock {
                buff: LockedBuff::Frenzy,
                mode: frenzy_lock,
            }],
            cycle: Some(IncarnonCycle {
                // The standard reading: earn the first transmute like every
                // other consumable in this sim.
                starts_primed: false,
                base_form: Box::new(base_form),
                // The gauge economy is DATA (the engine knows no weapon
                // names): Dual Toxocyst 9 charges / 1.0 s revert / 2.35 s
                // transmute, Laetum 12 / 2.0 / 2.0. Both transition times
                // scale by the reload formula. An evolution that speeds up
                // charge building (Incarnon Efficiency: +50%) divides the
                // hits needed — 12 becomes 8.
                charge_on: inc_form
                    .map(|f| f.charge_on)
                    .unwrap_or_default(),
                charges_to_fill: inc_form
                    .map(|f| (f.charges_to_fill / (1.0 + f.charge_rate)).ceil() as u32)
                    .unwrap_or(9),
                transmute_out_seconds: inc_form.map_or(1.0, |f| f.transmute_out) / rl,
                transmute_seconds: inc_form.map_or(2.35, |f| f.transmute_in) / rl,
                reload_bucket: rl - 1.0,
            }),
            ..Self::from_panel(incarnon, arena, arcane)
        }
    }

    /// The EXTRA HIT bracket of this form's BASE ATTACK:
    /// `1 + Σ elemental bonuses + Σ (unmodded IPS share × that IPS bonus)`.
    ///
    /// The wiki's `Weapon Hit Damage` formula names it in full, and the term it
    /// spells `Unmodded Impact Distribution × Impact Bonuses` is why this is
    /// read off the BASE ATTACK rather than off whichever instance triggered
    /// the extra hit. DE's CN card states the consequence outright: a slam
    /// whose own damage is 100% Blast still scales its extra hit by the gun's
    /// Impact and Puncture mods, weighted by the shares of the ordinary
    /// attack ("即使该武器的其它攻击方式的初始伤害不包含该物理伤害……依然会根据
    /// 基本攻击方式的初始伤害受到物理伤害MOD加成"), with the Heliocor worked out
    /// line by line.
    ///
    /// This is a RATIO, so it needs no special handling for the base-damage
    /// bucket: `damage` is already `base × (1 + damage mods)` expanded by the
    /// element hierarchy, and `dot_modified_base` is the same number before the
    /// expansion. Dividing cancels everything but the bracket.
    ///
    /// AND IT IS READ AT `t`, because an ability-granted element is additive
    /// with elemental mods and therefore inside this bracket — a Nourish that
    /// has run out stops paying an extra hit at the moment it stops paying a
    /// mod.
    fn extra_hit_bracket(&self, t: f64) -> f64 {
        let mb = self.dot_modified_base.unwrap_or_else(|| self.damage.total());
        if mb <= 0.0 {
            return 1.0;
        }
        self.with_ability_elements(self.damage.quantized(), mb, t).total() / mb
    }

    /// The (1 + element bonuses) bracket for an elemental DoT's ticks.
    fn elem_bracket(&self, t: DamageType) -> f64 {
        self.elem_dot_bonus
            .iter()
            .find(|(x, _)| *x == t)
            .map_or(1.0, |(_, v)| *v)
    }
}

/// THE FIRST KILL IS A TIME, and it is recorded wherever a kill is counted —
/// seven places, because damage lands from seven kinds of source. One method so
/// the two can never disagree: a site that counts a kill and forgets the clock
/// would leave `first_kill_at` reading like a weapon that never killed.
impl RunResult {
    fn note_kills(&mut self, killed: u32, at: f64) {
        if killed > 0 && self.first_kill_at.is_none() {
            self.first_kill_at = Some(at);
        }
        self.kills += killed;
    }
}

#[cfg(test)]
impl Default for DummyParams {
    /// TEST FIXTURE baseline: Dual Toxocyst base form + Secondary Enervate,
    /// humanoid dummy, 10 s. Production code never default-constructs
    /// DummyParams — a default weapon would smuggle weapon knowledge into
    /// the engine.
    fn default() -> Self {
        Self {
            enervate_stacks: 0,
            damage: Self::dual_toxocyst_base_vector(),
            radial: None,
            lingering: None,
            continuous: false,
            field_duration_on_empty_reload: 1.0,
            multishot_on_last_round: 0.0,
            base_multishot_on_last_round: 0.0,
            multishot_ammo_bonus: 0.0,
            compression_mult: 1.0,
            compression_bd: 0.0,
            bd_below_half_health: 0.0,
            cc_on_undamaged: 0.0,
            cd_on_undamaged: 0.0,
            headshot_damage_bonus: 0.0,
            headshot_bonus_multiplicative: false,
            noncrit_bonus: None,
            stacking_buffs: Vec::new(),
            base_crit_chance: 0.05,
            crit_multiplier: 2.0,
            crit_mult_below_cc: None,
            unmodded_crit_chance: 0.05,
            unmodded_crit_damage: 2.0,
            status_chance: 0.37,
            base_status_chance: 0.37,
            forced_procs: Vec::new(),
            status_duration_mult: 1.0,
            fire_rate: 1.0,
            charge_seconds: None,
            charge_cadence: crate::weapons_data::ChargeCadence::DrawThenRate,
            sustained_fire_rate: None,
            battery: None,
            rs_on_reload: 0.0,
            armor_strip_per_puncture: 0.0,
            instant_reload: None,
            headshot_streak: None,
            cd_below_status_count: None,
            burst: None,
            frenzy: false,
            locked_buffs: Vec::new(),
            cycle: None,
            magazine_size: 12.0,
            reload_seconds: 2.35,
            infinite_reserve: true,
            ammo_cost: 1.0,
            reserve_ammo: 72.0,
            ammo_efficiency_applies: true,
            multishot: 1.0,
            base_multishot: 1.0,
            evo_ms: None,
            evo_bd: None,
            locked_stats: Vec::new(),
            base_damage_bonus: 0.0,
            co_per_type: 0.0,
            co_behavior: crate::loadout::CoBehavior::AdditiveWithBaseDamage,
            co_base_fraction: 1.0,
            co_stack: None,
            ms_stack: None,
            cc_on_headshot: None,
            cc_stack: None,
            status_damage_mult: 1.0,
            elem_dot_bonus: Vec::new(),
            faction_mult: 1.0,
            abilities: Vec::new(),
            dot_modified_base: None,
            reload_bonus: 0.0,
            weakpoint_damage: 0.0,
            crit_tier_upgrade_chance: 0.0,
            slash_on_crit: 0.0,
            weakpoint_cc_rel: 0.0,
            bodyshot_cc_mult: 1.0,
            round_restore_on_status: None,
            instant_reload_on_kill: None,
            mag_growth_on_empty_reload: None,
            cd_on_kill: None,
            fr_on_reload: None,
            bd_on_reload: None,
            super_crit_on_status: None,
            beam_ramp_floor: BEAM_RAMP_FLOOR,
            syndicate_radial: None,
            tendril_max: 0,
            cc_per_tendril: 0.0,
            sc_per_tendril: 0.0,
            tendrils_initial: 0,
            tendrils_held: false,
            mag_refill_on_kill: 0.0,
            proc_conversion: None,
            // Secondary Enervate at max rank — the historical calibration
            // profile's arcane (the ramp/reset mechanic is the perk).
            arcane: ArcaneFx {
                id: "secondary_enervate".to_string(),
                enervate_rank: Some(5),
                ..ArcaneFx::none()
            },
            body_parts: Self::humanoid_parts(),
            target: TargetParams::training_dummy(),
            tenno: crate::tenno_data::default_tenno().clone(),
            duration_secs: 10.0,
        }
    }
}

/// Time-bucketed effective damage for the results' DPS-over-time curve
/// (user, 2026-07-29). ONE-SECOND buckets (user: the precision should be
/// 1 s); the array is capacity for the longest supported engagement —
/// callers slice to the actual duration. A wrapper type because a large
/// array has no derived `Default`.
pub const TIMELINE_BUCKETS: usize = 600;

#[derive(Debug, Clone, Copy)]
pub struct Timeline(pub [f64; TIMELINE_BUCKETS]);

impl Default for Timeline {
    fn default() -> Self {
        Timeline([0.0; TIMELINE_BUCKETS])
    }
}

impl Timeline {
    fn add(&mut self, t: f64, v: f64) {
        let i = t.max(0.0) as usize;
        self.0[i.min(TIMELINE_BUCKETS - 1)] += v;
    }
}

/// Effective damage attributed by SOURCE — the WoW-damage-meter view
/// (user, 2026-07-29): direct pellet hits, each status settlement type
/// (Slash bleed, Heat/Toxin/Gas/Electricity DoTs, Blast detonations —
/// keyed by the proc's type), and the on-status arcane instance.
#[derive(Debug, Clone, Copy, Default)]
pub struct SourceDamage {
    pub direct: f64,
    /// The radial (AoE) attack part — MECHANICS §7.
    pub radial: f64,
    /// The lingering FIELD's ticks (Torid's Toxin cloud). Its own bucket
    /// because it is neither a direct hit nor a status DoT: it is weapon
    /// damage on its own clock, and on that weapon it is most of the output.
    pub field: f64,
    pub arcane_on_status: f64,
    /// EXTRA HITS (Xata's Whisper's Void instance) — wiki `Extra_Hit`. Its own
    /// bucket for the same reason the field has one: it is neither the weapon's
    /// hit nor a status tick, and on a build running the ability it is a fifth
    /// of the output. It is also the only bucket the BUILD cannot move directly
    /// — it moves everything else and this follows.
    pub extra_hit: f64,
    /// …split by type, kept parallel to the others. One type per instance
    /// today (Void), because one ability grants extra hits here.
    pub extra_hit_by_type: [f64; 15],
    /// A SYNDICATE RADIAL's explosion (Truth, Justice, …) — its own bucket
    /// because it is neither weapon damage nor a status tick: a flat 1000 of
    /// the syndicate's element, unscaled by anything the build does.
    pub syndicate: f64,
    /// The syndicate blast split by type — one entry in practice, since a
    /// blast is a single element, kept parallel to the other buckets.
    pub syndicate_by_type: [f64; 15],
    /// Indexed by `DamageType as usize` (15 variants).
    pub status: [f64; 15],
    /// The three WEAPON-damage buckets above, each split across the damage
    /// VECTOR that dealt them (same indexing as `status`).
    ///
    /// A status row is already one type — that is what a proc is. A weapon
    /// hit is not: "direct 3.1 G" hides that it was Corrosive and Magnetic in
    /// a 76/24 split, which is the part of a build a player actually tunes
    /// (user, 2026-08-01: "直伤也是有属性的").
    pub direct_by_type: [f64; 15],
    pub radial_by_type: [f64; 15],
    pub field_by_type: [f64; 15],
    /// `arcane_on_status` split the same way. Cascadia Empowered's instance
    /// takes the PROC's damage type ("matching the Damage Type of the Status
    /// Effect"), so this row has a vector too — it is just a vector of one
    /// type per instance rather than a mixed one.
    pub arcane_by_type: [f64; 15],
}

impl SourceDamage {
    fn add_status(&mut self, t: DamageType, v: f64) {
        self.status[t as usize] += v;
    }
}

/// Credit one weapon-damage instance to its bucket's per-type split.
///
/// Attribution is by each component's SHARE of the instance's vector, which
/// is exact wherever a pool takes the whole hit (Overguard, health) and an
/// approximation in exactly one place: while shields are up, Toxin bypasses
/// them and its siblings do not, so the components were mitigated
/// differently and a proportional split cannot see that. The bucket TOTAL is
/// unaffected either way — this only distributes a number already computed.
///
/// The vector passed here is the QUANTIZED one, so the shares are the ones
/// that actually landed and they will not match the panel's exactly: the
/// Torid build reading Corrosive 164.73 / Magnetic 52.02 (76/24) snaps to
/// 24/32 and 8/32 of the total, i.e. exactly 75/25. That gap is the wiki's
/// quantization, not an error in either number.
/// Split one instance's EFFECTIVE damage across the types that made it.
///
/// The share is each component's contribution AFTER the vulnerability column
/// — the same weights that produced `effective`. Splitting by the raw vector
/// instead would report a 50/50 Impact/Slash hit on a Grineer unit as 50/50
/// when Impact actually did 60% of the damage, which is exactly the number a
/// reader consults to decide what to add next.
fn add_by_type(
    dst: &mut [f64; 15],
    v: &DamageVector,
    effective: f64,
    col: &crate::factions_data::Column,
) {
    let weighted: f64 = v.iter_nonzero().map(|(t, a)| a * col.get(t)).sum();
    if weighted <= 0.0 {
        return;
    }
    for (t, amount) in v.iter_nonzero() {
        dst[t as usize] += effective * amount * col.get(t) / weighted;
    }
}

/// Result of a single engagement.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunResult {
    /// Raw damage dealt (pre-mitigation), direct hits + DoT ticks.
    pub total_damage: f64,
    /// Damage after target mitigation (overguard neutrality / armor DR).
    pub effective_damage: f64,
    /// Effective damage contributed by DoT ticks (subset of the above).
    pub dot_damage: f64,
    pub shots: u32,      // trigger pulls
    pub pellets: u32,    // multishot instances (>= shots)
    pub crits: u32,      // tier >= 1, counted per pellet
    pub big_crits: u32,  // tier >= 2
    /// Sum of every DIRECT pellet's crit TIER, normal hits included as 0.
    ///
    /// Its mean is the number `crits / pellets` stops being once a build
    /// passes 100% crit chance: the rate saturates at 1.0 while the tier
    /// keeps climbing, and it is the tier that multiplies the damage
    /// (`crit_mult = 1 + tier x (cd - 1)`, uncapped — red is not the top).
    /// Below 100% the two are the same number.
    pub crit_tier_sum: u32,
    pub headshots: u32,  // hits on an `is_head` part
    pub procs: u32,      // status procs applied (all types)
    pub dot_ticks: u32,  // bleed ticks that landed
    /// Lingering-FIELD ticks that landed (Torid's cloud) — its own counter
    /// because a field tick is weapon damage, not a status DoT tick.
    pub field_ticks: u32,
    pub reloads: u32,    // magazine reloads performed
    pub transforms: u32, // TRANSMUTES into the Incarnon form (reverts don't count)
    pub kills: u32,      // InstantRespawn deaths (0 with InfiniteHealth)
    /// Kills + the depleted fraction of the CURRENT target's total pool
    /// (overguard + health) at engagement end — partial credit so the
    /// objective is not a step function (user, 2026-07-24: "draining 80%
    /// of the total pool scores 0.8").
    pub kill_progress: f64,
    /// Effective damage by source (direct / per-proc-type / arcane).
    pub sources: SourceDamage,
    /// SECONDS THE WEAPON WAS NOT FIRING because it was reloading or mid
    /// transform. Burst DPS is the damage over the time that is left, which is
    /// what a room-clear is actually paced by — a weapon that reloads for a
    /// third of the fight has two very different numbers and only one of them
    /// is on the card.
    pub downtime_secs: f64,
    /// When the FIRST target died. `None` if none did — an honest absence
    /// rather than a zero, which would read as "instantly".
    pub first_kill_at: Option<f64>,
    /// Effective damage dealt before the first reload started: the opening
    /// window, which is what decides whether a room dies before it reacts.
    pub first_magazine_damage: f64,
    /// The single biggest damage INSTANCE of the run — the number people chase.
    pub max_hit: f64,
    /// Every hit sorted by what it was: `[head][tier]`, tier capped at 2 (red
    /// and above share a bucket, since that is where the multiplier stops
    /// naming itself). An impossible number stands out in a histogram and
    /// disappears in a mean.
    pub hit_count: [[u32; 3]; 2],
    pub hit_damage: [[f64; 3]; 2],
    /// Effective damage by time bucket (the damage-over-time curve).
    pub timeline: Timeline,
    /// The `Rng` state this run STARTED from. SplitMix64 keeps all of its
    /// state in one `u64`, so this is the whole of what it takes to replay
    /// the run bit-for-bit — which is how the MEDIAN engagement gets traced
    /// without every run carrying a trace (see [`Replay`]).
    pub rng_state: u64,
}

/// Devouring Attrition's multiplier for ONE damage instance: a
/// non-critical instance (tier 0) rolls `chance` for `1 + bonus`; a
/// critical instance is never eligible. Its own multiplicative bracket
/// (wiki: "multiplicative to base damage bonuses such as Hornet Strike"),
/// and it applies to the radial part too ("Affects both forms").
fn noncrit_mult(spec: Option<(f64, f64)>, tier: u32, rng: &mut Rng) -> f64 {
    match spec {
        Some((chance, bonus)) if tier == 0 && rng.chance(chance) => 1.0 + bonus,
        _ => 1.0,
    }
}

/// A mod SET may promote a hit by one critical tier (Vigilante: 5% per
/// equipped member). The wiki is explicit that it "triggers exclusively on
/// critical hits" — a normal hit is never promoted, so tier 0 is untouched.
/// The tier formula does the rest: crit_mult = 1 + tier x (cd - 1), so one
/// extra tier is worth exactly one more crit-damage step.
fn upgrade_crit_tier(tier: u32, chance: f64, rng: &mut Rng) -> u32 {
    if tier >= 1 && chance > 0.0 && rng.chance(chance) {
        tier + 1
    } else {
        tier
    }
}

/// Roll a critical tier for an effective crit chance that may exceed 1.0.
fn roll_crit_tier(effective_cc: f64, rng: &mut Rng) -> u32 {
    let guaranteed = effective_cc.floor().max(0.0);
    let extra_chance = effective_cc - guaranteed;
    guaranteed as u32 + rng.chance(extra_chance) as u32
}

/// Pick the body part a shot lands on, by normalized aim weight.
fn pick_part<'a>(parts: &'a [BodyPart], rng: &mut Rng) -> &'a BodyPart {
    let total: f64 = parts.iter().map(|p| p.aim_weight).sum();
    let mut x = rng.next_f64() * total;
    for p in parts {
        x -= p.aim_weight;
        if x < 0.0 {
            return p;
        }
    }
    parts.last().expect("dummy needs at least one body part")
}

/// Can a shot be taken right now? This — not "is the magazine empty" — is what
/// gates the reload: the weapon reloads exactly when it CANNOT fire.
///
/// The rule is `cost <= ceil(current)`, floored at zero (owner, 2026-08-01),
/// and it takes both of the facts already measured here as special cases.
///
/// A REMAINDER SMALLER THAN THE SHOT still fires. ✅ measured (MEASUREMENTS
/// M14): 0.25 left pays a full-cost shot and overdraws the counter negative.
/// The ceiling is why — `1 <= ceil(0.25)` — and the debt is bounded to
/// (−1, 0], so `reload_draw`'s whole-round rule brings the magazine back at
/// `capacity − 0.75` rather than full.
///
/// A SHOT THAT COSTS NOTHING needs no round at all (user, 2026-07-30). Dual
/// Toxocyst hits this exactly: the last round headshots, the magazine lands on
/// 0, and that same kill arms Frenzy's +100% ammo efficiency — so the next shot
/// is free and fires instead of forcing a reload. That used to be its own
/// `free_shot` flag threaded through this call; the ceiling subsumes it, since
/// `0 <= ceil(anything)` holds on an empty magazine too (owner, 2026-08-01:
/// the free shot is not the fundamental thing, the COST is).
///
/// What the ceiling ADDS is the case above one round. Seven left and a shot
/// costing ten: `10 <= ceil(7)` is false, so the weapon reloads. The old test
/// was "anything left", so it fired, and landed on −3 — a debt one whole-round
/// draw cannot clear. That hid for as long as it did because the two tests
/// agree for every cost at or below one round (`ceil(x) >= 1` on any positive
/// magazine): ammo efficiency put costs in the 0.x range and exposed nothing
/// (owner). The Larkspur Prime's alt-fire costs TEN.
fn can_fire(magazine: f64, cost: f64) -> bool {
    cost <= (magazine - 1e-9).max(0.0).ceil() + 1e-9
}

/// Take `want` rounds out of `reserve`, or all that is left of it.
///
/// The reserve is ONE pool for the whole weapon — both forms of an Incarnon
/// cycle, and an Arch-Gun's alt fire, draw from the same supply. Written once
/// because it was written zero times inside the cycle: every draw there was
/// free until 2026-08-04, which made the Infinite-ammo setting a no-op on every
/// Incarnon weapon (owner: the setting has to be adjustable).
fn draw_from(reserve: &mut f64, infinite: bool, want: f64) -> f64 {
    if infinite {
        return want;
    }
    let take = want.min(*reserve);
    *reserve -= take;
    take
}

/// How many WHOLE rounds a reload moves out of reserve.
///
/// Reserve is spent in whole rounds only — ✅ measured (user, 2026-07-30) — so a
/// reload tops the magazine up by `floor(capacity − current)` and a magazine
/// sitting on a fraction comes back still holding that fraction. Measured on a
/// 5-round magazine:
///
/// | current | draw | after |
/// | --- | --- | --- |
/// | 1.50 | `floor(3.50)` = 3 | 4.50 |
/// | 3.25 | `floor(1.75)` = 1 | 4.25 |
/// | 4.25 | `floor(0.75)` = 0 | 4.25 — the reload is refused outright |
///
/// The refusal at 4.25 is visible in game as the magazine reading FULL (the HUD
/// ceilings it to 5), which is the same rounding that made M14 readable.
///
/// This also subsumes the reload-from-empty case without special-casing it: a
/// shot can only overdraw by less than one round, so `current` is in (−1, 0]
/// there and the draw is a full `capacity` — which is how a −0.75 counter comes
/// back at 4.25 rather than 5.00.
///
/// It is the GLOBAL reload rule (user, 2026-07-30): the auto-reload an Incarnon
/// transform performs runs on the same mechanism, not a separate "fill to full".
fn reload_draw(capacity: f64, current: f64) -> f64 {
    (capacity - current).floor().max(0.0)
}

/// The ammo efficiency in force right now: the sum of every source, CAPPED.
///
/// Sources stack ADDITIVELY — VERBATIM (wiki `Ammo`): *"Sources of ammo
/// efficiency stack additively with each other except for Energized Munitions,
/// which stacks multiplicatively."* Energized Munitions is a Warframe ability
/// and out of scope; if it is ever modelled it must MULTIPLY, not join this sum.
///
/// **The cap is 100% and it is a real ceiling, not a clamp of convenience**
/// (user, 2026-07-30): a shot can cost nothing, never less. Stacking past 100%
/// buys nothing and in particular never starts REFUNDING ammo, so the magazine
/// cannot climb while firing.
///
/// Charge-backed magazines are outside the ammo economy entirely and take no
/// efficiency at all, which is why `applies` short-circuits to zero.
fn ammo_efficiency(applies: bool, bar: f64, arcane_static: f64, arcane_live: f64) -> f64 {
    if !applies {
        return 0.0;
    }
    (bar + arcane_static + arcane_live).clamp(0.0, 1.0)
}

/// Live on-kill stack state (Galvanized graceful decay: on timeout lose
/// ONE stack and reset the duration for the remainder).
#[derive(Default)]
struct LiveStacks {
    stacks: u32,
    expiry: f64,
    /// [`BuffDecay::PerStackExpiry`] only: one expiry per live stack, oldest
    /// first. Empty for the Galvanized family, which shares a single clock.
    each: Vec<f64>,
    per_stack: bool,
}

impl LiveStacks {
    /// Apply pending decay and return the current stack count. An INFINITE
    /// expiry never falls due, which is the whole of what a locked buff is.
    fn current(&mut self, now: f64, duration: f64) -> u32 {
        if self.per_stack {
            // Each stack on its own clock: drop every one that has fallen due,
            // which is FIFO because they were pushed in time order.
            self.each.retain(|&e| e > now);
            self.stacks = self.each.len() as u32;
            return self.stacks;
        }
        while self.stacks > 0 && self.expiry <= now {
            self.stacks -= 1;
            self.expiry += duration;
        }
        self.stacks
    }

    /// Seed from a configured buff card: its stacks, on its own clock. A
    /// LOCKED card arrives here as [`crate::loadout::NO_TIMEOUT`], so nothing
    /// on this path has to know what locking is.
    fn seed(initial: u32, max: u32, duration: f64) -> Self {
        LiveStacks {
            stacks: initial.min(max),
            expiry: duration,
            each: Vec::new(),
            per_stack: false,
        }
    }

    /// Seed a per-stack-expiry buff. A seeded stack starts its own clock at
    /// `duration`, the same instant the shared-clock family starts its one.
    fn seed_per_stack(initial: u32, max: u32, duration: f64) -> Self {
        LiveStacks {
            stacks: initial.min(max),
            expiry: duration,
            each: vec![duration; initial.min(max) as usize],
            per_stack: true,
        }
    }

    /// One trigger: decay what is due, climb by one (capped), restart the
    /// clock. A locked buff takes this path too — it earns like any other,
    /// and its restart lands at infinity.
    fn bump(&mut self, now: f64, duration: f64, max: u32) {
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

    fn on_kill(&mut self, now: f64, spec: &crate::loadout::StackSpec) {
        self.bump(now, spec.duration, spec.max_stacks);
    }
}

/// The run's earned on-kill buffs (weapon-scoped: shared by both forms
/// of a transform group).
#[derive(Default)]
struct GalStacks {
    co: LiveStacks,
    ms: LiveStacks,
}

impl GalStacks {
    fn bump_on_kill(&mut self, params: &DummyParams, now: f64) {
        if let Some(spec) = &params.co_stack {
            self.co.on_kill(now, spec);
        }
        if let Some(spec) = &params.ms_stack {
            self.ms.on_kill(now, spec);
        }
    }
}

/// Disrupt's on-break payload (data/debuffs/disrupt.yaml): breaking
/// shields OR overguard with Disrupt active fires a forced Tesla Chain
/// instance totalling 3% of the broken pool's MAX per Magnetic stack
/// (cap 30%), over 6 ticks; status-damage mods apply TWICE; base-damage
/// mods never.
fn push_break_proc(debuffs: &mut DebuffState, params: &DummyParams, now: f64, pool: BrokenPool) {
    let stacks = debuffs.disrupt.len();
    if stacks == 0 {
        return;
    }
    let pool_max = match pool {
        BrokenPool::Overguard => params.target.overguard(),
        BrokenPool::Shield => params.target.max_shield(),
    };
    let frac = (0.03 * stacks as f64).min(0.30);
    let total = frac * pool_max * params.status_damage_mult.powi(2);
    debuffs.dots.push(Dot {
        next_tick: now,
        ticks_left: 6,
        value: total / 6.0,
        dtype: DamageType::Electricity,
        ignores_armor: false,
    });
}

/// Settle ONE damage instance's status procs onto the target — MECHANICS §6.
///
/// Every instance kind applies status by identical rules, so this is one
/// function rather than three copies: a direct pellet, a radial stage, and a
/// lingering-FIELD tick all land here. `at` is the INSTANCE's own time, which
/// is why it is a parameter and not the shot clock — a cloud ticks between
/// shots, and its procs' durations have to run from the tick.
///
/// `scale` carries the instance's damage scaling for the DoT payloads
/// (ModifiedBase × crit × body part), `params` is the engagement and `ap` the
/// ACTIVE form (element brackets differ per form).
#[allow(clippy::too_many_arguments)]
/// How many times a FACTION bonus has been applied by the time a payload lands.
///
/// Faction damage is re-applied at every DERIVATION step, and that is the whole
/// rule — there is no special case anywhere, only a count of how far a number
/// is from the hit that started it:
///
/// | payload | depth | faction |
/// | --- | --- | --- |
/// | a direct hit | 1 | ×f |
/// | a status/DoT the hit applied | 2 | ×f² |
/// | a status/DoT applied by a DERIVED damage instance | 3 | ×f³ |
///
/// Depth 3 is not a quirk to hardcode. It is arithmetic: a DoT is always one
/// step past its source, so a DoT at ×f³ PROVES its source was already at ×f²,
/// i.e. that an intermediate damage instance exists (owner, 2026-08-05: "吃3次
/// 派系只有一个情况，那必然有个元素实例造成才有可能出现这个情况"). That is how
/// Primary Debilitate's extra status is known to deal an instance and not
/// merely add a stack — the wiki states ×f³ for it and never mentions the
/// instance.
///
/// Writing it as a depth rather than as `fm2` and a future `fm3` is what keeps
/// the next spreading mechanic (melee Influence, Secondary Encumber) from
/// inventing its own multiplier.
const DEPTH_HIT: u32 = 1;
const DEPTH_PROC: u32 = 2;
/// A status applied by a damage instance that was itself derived from a hit.
const DEPTH_DERIVED_PROC: u32 = 3;

/// The faction multiplier a payload at `depth` carries.
fn faction_at(faction_mult: f64, depth: u32) -> f64 {
    faction_mult.powi(depth as i32)
}

/// How many stacks of a COMBINED status the target currently holds.
///
/// Only the six combinations answer — a primary or a physical proc has no
/// components, so nothing else can be split and nothing else is counted.
fn combined_stacks(debuffs: &DebuffState, t: DamageType) -> usize {
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
fn has_status(debuffs: &DebuffState, t: DamageType) -> bool {
    if combined_stacks(debuffs, t) > 0 {
        return true;
    }
    // …AND THE PILES THAT ARE NOT COMBINED ELEMENTS. `combined_stacks` answers
    // for the six combined ones, which is all Primary Debilitate ever needed to
    // ask; every physical and primary status lives in a list of its own and
    // fell through its `_ => 0` arm. So `has_status(Puncture)` was FALSE on a
    // target covered in Puncture, and a perk keyed on one — the Latron family's
    // Riddled Target — could never fire (2026-08-12). Nothing was keyed on one
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
/// the combined status's components, and which one?
///
/// A pure function on purpose — the whole mechanic's DECISION is here, where it
/// can be tested without a fight. Only its damage PAYLOAD needs a measurement.
///
/// The rules, from the wiki and settled with the owner (2026-08-05, threshold
/// corrected 2026-08-10):
///
/// - the status just applied must be a COMBINED element — a primary or a
///   physical proc has no components to split into
/// - the target must be AT [`DEBILITATE_STACKS`] **counting the stack this
///   instance is applying**. At nine, the shot that makes it ten splits; you do
///   not reach ten and then have to shoot again (owner, 2026-08-10: "如果当前
///   是9层，下一发是10层的话，就可以立刻触发其中一个")
/// - roll `chance` (0.5 at rank 0 → 1.0 at rank 5)
/// - pick between the two components 50/50
///
/// **The threshold is where BLAST stopped being a special case.** Reading it as
/// "already holds ten" made the arcane dead on Blast and only on Blast —
/// reaching ten DETONATES and drains every stack (detonate.yaml), so a
/// pre-application count is 0..=9 forever and no eleventh application exists to
/// wait for. That was patched here as an `if proc == Blast` for two days. It is
/// not a Blast rule: the tenth APPLICATION is the trigger for every
/// combination, Blast is simply the one where the difference is the whole
/// mechanic rather than one shot, and the owner reports it firing about as often
/// as anything else ("并不像wiki说的那么rarely"). MEASUREMENTS M34.
///
/// Once per DAMAGE INSTANCE, which is what the wiki's own note about beams
/// describes: "only activate once per damage instance, making it less
/// effective than it 'should' be when used on a Beam weapon, due to how
/// Multishot affects such weapons."
fn debilitate_split(
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
    let (a, b) = crate::elements::components_of(landed)?;
    if rng.next_f64() >= chance {
        return None;
    }
    // 50/50, and drawn AFTER the chance roll so a failed roll consumes exactly
    // one number — a reader comparing two seeds should not have to reason about
    // how many draws a miss cost.
    Some(if rng.next_f64() < 0.5 { a } else { b })
}

/// EXTRA HIT — the second damage instance an ability grants, fired off the one
/// that triggered it (wiki `Extra_Hit`; MECHANICS §7 §"Extra Hit"). Returns
/// whether it killed the target.
///
/// The wiki's formula is one line —
///
/// > Extra Hit Damage = Weapon Hit Damage × Extra Hit Percentage
/// >                    × (1 + Faction Damage Bonuses)
///
/// — and every oddity people report about Xata's Whisper falls out of `Weapon
/// Hit Damage` ALREADY containing a faction layer, a crit multiplier and a
/// body-part multiplier. So this function takes the triggering instance's
/// finished `trigger_raw` and multiplies rather than rebuilding anything:
///
/// - **faction, again.** One `faction_at_time` here, on top of however many the
///   trigger already carried. A direct hit is at depth 1, so its extra hit is
///   at 2; a Blast detonation is at depth 2, so ITS extra hit is at 3 — which
///   is the "triple dip" both wikis name and neither has to be hardcoded.
/// - **the body part, again** — `part_again`, and it is the caller that knows.
///   A direct headshot passes its `part_factor`; a radial, a field tick and a
///   Blast detonation pass 1.0, since none of them struck a body part in the
///   first place. DE's CN card states both halves: "同理，弱点倍率也会被计算两
///   次" for a hit, and "弱点倍率只会被计算一次" off a Blast detonation.
/// - **crit, once and inherited.** The extra hit rolls no crit of its own (the
///   EN wiki files "Xata's Whisper's Extra Hits cannot crit" under Bugs) but
///   `trigger_raw` critted, so the number behind an orange hit is orange-sized
///   — which is what "affected by ... critical ... damage mods (e.g. Vital
///   Sense)" on the ability's own page means.
/// - **`bracket`** is the trigger's elemental correction, and it is 1.0
///   everywhere except where the trigger's own bracket differs from the base
///   attack's — see [`DummyParams::extra_hit_bracket`]. The Blast detonation is
///   the loud case: it takes NO elemental bonus, and the extra hit off it takes
///   the whole one.
///
/// It is a real instance, so it lands through [`TargetState::apply`] like any
/// other: Void's ×1.5 against Overguard is the vulnerability column doing its
/// job, not a rule written here.
///
/// AND IT ROLLS ITS OWN STATUS, at the weapon's own chance ("附加的虚空伤害具有
/// 基于武器本身触发几率的独立触发几率"). Its vector is one type, so the proc is
/// always that type — a Void proc, worth a Condition Overload stack and no
/// damage.
///
/// THE CALLER DECIDES WHETHER IT FIRES, and the rule is short: a WEAPON damage
/// instance triggers one, a status payload does not — except a Blast
/// detonation, which does and is filed under Bugs. Nothing here checks; a
/// function that guessed from its arguments which kind of instance it was
/// handed would be the third place that knowledge lives.
/// WHAT A STATUS LEFT BY AN EXTRA HIT BURNS OFF — the one place the rule lives,
/// because the category has two members and they answer it differently for the
/// same reason (docs/EXTRA_HIT.md).
///
/// The wiki states it for an Extra Hit that deals damage: *"Damage over Time
/// status effects created by an Extra Hit will use the Extra Hit Damage as
/// Modded Base Damage"* — which is why such a status takes the ELEMENTAL
/// bonuses an ordinary weapon status is denied: they are already inside that
/// number.
///
/// Read literally it gives ZERO for Primary Debilitate, which the same page
/// calls "a 0-damage Extra Hit", and that status plainly does damage. The rule
/// that covers both (owner, 2026-08-09: "如果为0，那么就找上一级去找base"):
///
/// > an Extra Hit REPLACES the base its status would have used. A 0% one
/// > replaces nothing, so the level above stands.
///
/// The owner's phrasing for the other direction is the clearest statement of
/// it there is — 上一级被 resupply 替换了.
fn extra_hit_status_base(extra_hit_damage: f64, level_above: f64) -> f64 {
    if extra_hit_damage > 0.0 {
        extra_hit_damage
    } else {
        level_above
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_extra_hits(
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
    params: &DummyParams,
    ap: &DummyParams,
    mit: &Mitigation,
    r: &mut RunResult,
    rng: &mut Rng,
) -> bool {
    let hits = crate::abilities_data::extra_hits_at(&params.abilities, at);
    if hits.is_empty() || trigger_raw <= 0.0 {
        return false;
    }
    let f = params.faction_at_time(at);
    for crate::abilities_data::ExtraHitLive { element: ty, frac, forced_status } in hits {
        let raw = trigger_raw * frac * bracket * part_again * f;
        let (eff, killed, broke) = target.apply(
            raw,
            TypeShares::single(ty),
            head_direct,
            at,
            &params.target,
            false,
            mit,
        );
        r.total_damage += raw;
        r.effective_damage += eff;
        r.sources.extra_hit += eff;
        r.sources.extra_hit_by_type[ty as usize] += eff;
        r.timeline.add(at, eff);
        r.note_kills(u32::from(killed), at);
        if let Some(pool) = broke {
            push_break_proc(debuffs, params, at, pool);
        }
        if killed {
            gal.bump_on_kill(params, at);
            arc.on_kill(params, at);
            *debuffs = DebuffState::default();
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
            &params.target.status_immunities,
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
                crit_mult: 1.0,
                part_factor: 1.0,
                attrition: 1.0,
                xh_bracket: bracket,
            },
            debuffs,
            gal,
            arc,
            target,
            params,
            ap,
            mit,
            r,
            rng,
            // The extra hit is one derivation past the hit, so a status IT
            // applies is one past that. Void applies none that pay damage, so
            // this is a claim nothing collects on yet — written as the ladder
            // rather than as a number so the first one that does is right.
            DEPTH_DERIVED_PROC,
        );
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn settle_procs(
    procs: Vec<DamageType>,
    at: f64,
    scale: InstanceScale,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    target: &mut TargetState,
    params: &DummyParams,
    ap: &DummyParams,
    mit: &Mitigation,
    r: &mut RunResult,
    rng: &mut Rng,
    // How far this batch of procs is from the hit that started it — see
    // `faction_at`. A hit's own procs are DEPTH_PROC; a proc that Primary
    // Debilitate split out of one is DEPTH_DERIVED_PROC, because it came
    // through an extra damage instance.
    depth: u32,
) {
    let InstanceScale { mb_live, crit_mult, part_factor, attrition, xh_bracket } = scale;
    let sd = params.status_duration_mult;
    let sdm = params.status_damage_mult;
    let caps = params.target.stack_caps;
    let gcap = |base: usize| caps.map_or(base, |c| base.min(c.general));
    let stagger_cap = caps.map_or(STAGGER_CAP, |c| STAGGER_CAP.min(c.impact));
    let heat_cap: Option<usize> = caps.map(|c| c.general);
    let dot_cap: Option<usize> = caps.map(|c| c.general);
    // Elemental DoT tick (data/debuffs): 0.5 × ModifiedBase ×
    // (1 + element bonuses) × (1 + status damage) × crit/part
    // snapshot. Delay-1 DoTs tick at +1..+6 s; delay-0 (Electricity/
    // Gas) at 0..+5 s (the +6 s event is a dud).
    let delayed_ticks = ((BLEED_TICKS as f64 * sd - BLEED_DELAY).floor() as u32) + 1;
    let immediate_ticks = ((BLEED_TICKS as f64 * sd).floor() as u32).max(1);
    // Faction is re-applied at every DERIVATION step — see `faction_at`. A
    // status the hit applied is one step past it, so depth 2 (wiki
    // Faction_Damage_Bonus; MECHANICS §8). This was written `faction_mult *
    // faction_mult`, which is the same number and says nothing about why.
    // AT `at`, because Roar is in this bracket and Roar ends. A status takes
    // the faction bonus that was running when it was APPLIED — the proc is a
    // snapshot of its instance, which is why `crit_mult` is snapshotted here
    // too.
    let fm2 = faction_at(params.faction_at_time(at), depth);
    // ECLIPSE, ONCE. Not `faction_at`-style repetition: the wiki draws the
    // contrast in so many words.
    let ecl = params.ability_final_at(at);
    let push_dot = |debuffs: &mut DebuffState,
                    dtype: DamageType,
                    coeff: f64,
                    bracket: f64,
                    delay: f64,
                    ticks: u32,
                    ignores_armor: bool| {
        debuffs.push_dot_capped(
            Dot {
                next_tick: at + delay,
                ticks_left: ticks,
                // `bracket` is `1 + Σ this element's bonuses`, and an ability
                // that adds this element is "additive with elemental mods" —
                // so its share belongs in the same sum, which is what makes
                // Fireball Frenzy "contribute to DoT" rather than only to the
                // hit.
                value: coeff * mb_live
                    * (bracket + params.ability_element_at(dtype, at))
                    * sdm * crit_mult * part_factor * fm2 * attrition * ecl,
                dtype,
                ignores_armor,
            },
            dot_cap,
        );
    };
    for proc in procs {
        r.procs += 1;
        // Read before the match applies it — see the Debilitate hook below.
        let stacks_before = combined_stacks(debuffs, proc);
        match proc {
            DamageType::Impact => DebuffState::push_capped(
                &mut debuffs.stagger,
                at + STAGGER_DURATION * sd,
                stagger_cap,
                at,
            ),
            DamageType::Puncture => {
                DebuffState::push_capped(
                    &mut debuffs.weakened,
                    at + WEAKENED_DURATION * sd,
                    gcap(WEAKENED_CAP),
                    at,
                );
                // Secondary Cryogenic: each Puncture status applies
                // N Cold stacks to targets around the hit — the
                // single-target arena collapses that onto the main
                // target (the wiki confirms it is included). The
                // Cold procs scale with Status Duration.
                for _ in 0..params.arcane.cold_burst_on_puncture {
                    debuffs.apply_cold_proc(at, sd, target.overguard > 0.0, caps, params.target.cannot_be_frozen);
                }
            }
            DamageType::Slash => push_dot(
                debuffs,
                DamageType::Slash,
                BLEED_COEFFICIENT,
                1.0, // Bleed: elemental mods never scale the ticks
                BLEED_DELAY,
                delayed_ticks,
                true, // Cinematic: ignores armor
            ),
            DamageType::Toxin => {
                // Primary Blight: each Toxin status THIS WEAPON
                // applies grants one stack to both of its buffs
                // (crit damage + multishot). The weapon-only rule is
                // the arcane's own (wiki), not a sim limitation.
                arc.bump_trigger(&params.arcane.buffs, ArcTrigger::ToxinStatus, at);
                push_dot(
                    debuffs,
                    DamageType::Toxin,
                    DOT_COEFFICIENT,
                    ap.elem_bracket(DamageType::Toxin),
                    1.0,
                    delayed_ticks,
                    false,
                )
            }
            DamageType::Electricity => {
                // Conjunction Voltage: each Electricity status this
                // weapon applies grants one stack to both of its
                // buffs (reload speed + multishot).
                arc.bump_trigger(&params.arcane.buffs, ArcTrigger::ElectricityStatus, at);
                push_dot(
                    debuffs,
                    DamageType::Electricity,
                    DOT_COEFFICIENT,
                    ap.elem_bracket(DamageType::Electricity),
                    0.0,
                    immediate_ticks,
                    false,
                )
            }
            DamageType::Gas => push_dot(
                debuffs,
                DamageType::Gas,
                DOT_COEFFICIENT,
                1.0, // literal Gas sources only; Heat/Toxin mods: nothing
                0.0,
                immediate_ticks,
                false,
            ),
            DamageType::Heat => {
                // Singleton accumulator: add the contribution and
                // refresh the shared clock; ticks stay anchored to
                // the first proc (ignite.yaml).
                // Cascadia Flare: each applied Heat status grants
                // one stack and refreshes the shared timer (own
                // procs only — the "any source" clause waits on a
                // multi-actor world).
                arc.bump_trigger(&params.arcane.buffs, ArcTrigger::HeatStatus, at);
                let contrib = DOT_COEFFICIENT
                    * mb_live
                    * ap.elem_bracket(DamageType::Heat)
                    * sdm
                    * crit_mult
                    * part_factor
                    * fm2; // faction double-dip
                let expiry = at + STATUS_DURATION * sd;
                debuffs.apply_heat(at, contrib, expiry, heat_cap);
            }
            DamageType::Cold => {
                // Primary Frostbite: each Cold status THIS WEAPON APPLIES
                // grants one stack to both of its buffs (crit damage +
                // multishot). "Applies" is the whole of it — the bump used to
                // run before the proc and unconditionally, so a Cold proc that
                // landed on a FROZEN target stacked the arcane even though
                // `frozen.yaml` says such a proc is inert and no status
                // exists (user, 2026-08-02). Frozen lasts 3 s against a 12 s
                // buff, so it never showed as the arcane going dark — it
                // showed as it never quite decaying.
                if debuffs.apply_cold_proc(at, sd, target.overguard > 0.0, caps, params.target.cannot_be_frozen) {
                    arc.bump_trigger(&params.arcane.buffs, ArcTrigger::ColdStatus, at);
                }
            }
            DamageType::Magnetic => DebuffState::push_capped(
                &mut debuffs.disrupt,
                at + STATUS_DURATION * sd,
                gcap(TEN_STACK_CAP),
                at,
            ),
            DamageType::Viral => DebuffState::push_capped(
                &mut debuffs.virus,
                at + STATUS_DURATION * sd,
                gcap(TEN_STACK_CAP),
                at,
            ),
            DamageType::Corrosive => DebuffState::push_capped(
                &mut debuffs.corrosion,
                at + CORROSION_DURATION * sd,
                gcap(TEN_STACK_CAP),
                at,
            ),
            DamageType::Radiation => DebuffState::push_capped(
                &mut debuffs.confusion,
                at + STATUS_DURATION * sd,
                gcap(TEN_STACK_CAP),
                at,
            ),
            // The Bullet Attractor, which only ever arrives from an EXTRA HIT
            // here: no weapon in the roster deals Void, and Xata's Whisper's
            // second instance is entirely Void. Worth one line in the CO
            // counter and nothing else — see `DebuffState::attractor`.
            DamageType::Void => DebuffState::push_capped(
                &mut debuffs.attractor,
                at + ATTRACTOR_DURATION * sd,
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
                    fuse: at + BLAST_FUSE * sd,
                    value: BLAST_COEFFICIENT
                        * mb_live
                        * sdm
                        * crit_mult
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
                    // …and each stack's OWN extra-hit contribution, pre-scaled
                    // by the bracket the gun that applied it had. Ten stacks
                    // land as one number here, but they are ten detonations and
                    // an expiring Nourish between the first and the tenth would
                    // make their brackets differ.
                    let xh_total: f64 = fired.iter().map(|b| b.value * b.xh_bracket).sum();
                    let mit = debuffs.mitigation(at, sd, params.armor_strip_per_puncture);
                    let (eff, killed, broke) =
                        target.apply(
                            total,
                            TypeShares::single(DamageType::Blast),
                            false,
                            at,
                            &params.target,
                            false,
                            &mit,
                        );
                    r.total_damage += total;
                    r.effective_damage += eff;
                    r.dot_damage += eff;
                    r.sources.add_status(DamageType::Blast, eff);
                    r.timeline.add(at, eff);
                    r.note_kills(killed as u32, at);
                    if let Some(pool) = broke {
                        push_break_proc(debuffs, params, at, pool);
                    }
                    if killed {
                        gal.bump_on_kill(params, at);
                        arc.on_kill(params, at);
                        *debuffs = DebuffState::default();
                    } else {
                        // THE ONE STATUS PAYLOAD THAT TRIGGERS AN EXTRA HIT.
                        // The bracket is already folded into `xh_total`, and no
                        // body part is re-applied — a detonation struck none.
                        fire_extra_hits(
                            xh_total,
                            1.0,
                            1.0,
                            false,
                            ap.status_chance,
                            at,
                            debuffs,
                            gal,
                            arc,
                            target,
                            params,
                            ap,
                            &mit,
                            r,
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
            // having to reach ten and shoot again" (owner, 2026-08-10). So the
            // count that goes in is the one the target is AT, and there is no
            // per-element branch: this line carried an `if proc == Blast` for
            // two days because Blast is where the off-by-one was VISIBLE
            // (detonating at ten, it never sits there, so the arcane was
            // silently dead on it) — and the fix for the visible case was the
            // rule all along. MEASUREMENTS M34.
            if let Some(part) =
                debilitate_split(proc, stacks_before + 1, params.arcane.debilitate_chance, rng)
            {
                // THE SPLIT PROC IS AN ORDINARY PROC (owner, 2026-08-05: "我
                // 倾向于类似正常触发dot的算法，而不是实例算法"). Nothing about
                // it is special-cased: it enters the same match below, takes
                // the same `push_dot`, and picks up its OWN element bracket —
                // a Corrosive that splits into Toxin is scaled by the TOXIN
                // mod bonus, and by 1.0 when the build carries no Toxin mod,
                // which is what "otherwise you only get the base portion"
                // means. The one thing that differs is the depth.
                //
                // So the "separate damage instance" the wiki names is the
                // status APPLICATION, not a second damage number to add: what
                // it buys is the extra faction layer, which is exactly the
                // ×f³ the page reports and the only observable it predicts.
                //
                // WHAT BASE IT BURNS OFF IS OPEN — see MEASUREMENTS M33, and
                // it is the whole of what is left to decide about this arcane.
                //
                // `mb_live` is `ModifiedBase`, "unmodded x (1 +
                // BaseDamageBonuses)", which EXCLUDES the elemental portions
                // (poison.yaml, quoting the wiki's Toxin page) — DE's rule for
                // a status a WEAPON's own hit applied. A status an ABILITY
                // applied reads that ability's own damage instead: Toxic Lash
                // on a 200-damage weapon deals 78 and its proc ticks for 39,
                // half of 78 and not half of 200.
                //
                // The community formula that decodes M33's 29551 is the
                // ABILITY case — its parent is Cyte-09's Extra Hit — so it
                // shows a Toxic-Lash-shaped chain and says nothing about a
                // plain weapon shot (owner, 2026-08-08: "那个resupply的例子就是
                // 说明，类似toxic lash的例子啊，不是常规武器的"). Reading the
                // full modded hit here was shipped for one commit on that
                // generalisation and reverted: it moved published board rows by
                // up to +112% on an inference.
                //
                // DECIDED (owner, 2026-08-08: "a版本吧，我觉得是对的"): the
                // weapon is the SOURCE, so the base is computed the weapon's
                // way. That is also the only one of the three readings that is
                // documented for a weapon-applied status, and this engine
                // matches the Toxin page's worked example to the digit.
                //
                // WHAT IS STILL OPEN IS THE EXPONENT, not the base — see M33.
                // If the base is the weapon's, an ordinary weapon status
                // double-dips faction and `f^3` looks like one layer too many
                // (owner: "理论应该是只有2的，而不是3"). The wiki states the
                // three outright and the source says the instance "has no
                // damage"; those are consistent in exactly one way — the
                // instance is real enough to add a layer and carries no damage,
                // so the magnitude comes from the weapon and the layer is the
                // only trace it leaves. Held at 3 because that is the stated
                // number; M33 has the Bane on/off ratio that settles it.
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
                        crit_mult,
                        part_factor,
                        // A SECOND ATTRITION ROLL, ON TOP OF THE HIT'S — and it
                        // is a BUG of DE's, not a design (owner, 2026-08-08:
                        // "有bug……这个+21好像还会作用在由衰弱产生的dot上面（非
                        // 本意）").
                        //
                        // MEASURED first: the DoT at the end of 直伤 -> 附加伤害
                        // -> dot eats three faction layers and "441倍强袭损耗" =
                        // 21x21. Then EXPLAINED, and the explanation is what
                        // this line follows: the split fires a damage instance
                        // whose damage is ZERO, and zero still gets multiplied
                        // by that instance's own faction bracket and its own
                        // Attrition roll. When the DoT is then computed, the
                        // zero is replaced by the parent hit's value — but the
                        // two multipliers already applied to it are left in.
                        // That is the whole leak: one instance's multipliers on
                        // another instance's magnitude.
                        //
                        // IT ROLLS EVEN WHEN THE HIT CRIT, which is the part of
                        // the story that is NOT the intuitive one. The zero
                        // instance carries no crit of its own — there is nothing
                        // to crit — so "on a hit that is not critical" is
                        // satisfied whatever the parent did (owner: "50%概率触
                        // 发，因为这个也没暴击"). A critting build therefore
                        // takes 1 x 21 here rather than 441, and never 1.
                        //
                        // THE LITERAL `0` IS THE CLAIM, and it is the tier
                        // rather than a rounded-down roll: the split is
                        // permanently non-critical, not a zero-damage hit that
                        // happens to be rolling crit against a zero. Those two
                        // readings are identical in damage and opposite in
                        // eligibility, and only the first leaves the perk live
                        // on a build that crits every shot.
                        //
                        // ✅ MEASURED IN GAME (owner, 2026-08-10): Phenmor,
                        // crit chance pushed to guaranteed, Devouring
                        // Attrition — every hit orange, and the Debilitate DoT
                        // still comes out x21 some of the time. Under the other
                        // reading it could never fire again. Two weapons now,
                        // since the deduction came off the Felarx's
                        // Devastating Attrition. MEASUREMENTS M37.
                        //
                        // `crit_mult` is still carried, in `InstanceScale` above:
                        // the parent's value is what the zero is replaced BY, and
                        // that value critted. MEASUREMENTS M37.
                        attrition: attrition * noncrit_mult(ap.noncrit_bonus, 0, rng),
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
                    ap,
                    mit,
                    r,
                    rng,
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
            let (eff, killed, broke) = target.apply(
                amt,
                TypeShares::single(proc),
                false,
                at,
                &params.target,
                false,
                mit,
            );
            r.total_damage += amt;
            r.effective_damage += eff;
            r.sources.arcane_on_status += eff;
            r.sources.arcane_by_type[proc as usize] += eff;
            r.timeline.add(at, eff);
            r.note_kills(killed as u32, at);
            if let Some(pool) = broke {
                push_break_proc(debuffs, params, at, pool);
            }
            if killed {
                gal.bump_on_kill(params, at);
                arc.on_kill(params, at);
                *debuffs = DebuffState::default();
            }
        }
    }
}

/// One instance's damage scaling, as the status payloads need it.
#[derive(Debug, Clone, Copy)]
struct InstanceScale {
    /// Live ModifiedBase (base-damage bucket applied).
    mb_live: f64,
    /// The instance's crit multiplier (1.0 when it did not crit).
    crit_mult: f64,
    /// Body-part multiplier — always 1.0 for a radial or a field.
    part_factor: f64,
    /// DEVOURING/DEVASTATING ATTRITION on THIS instance, or 1.0.
    ///
    /// 1.0 everywhere but the Primary Debilitate split, and that is the whole
    /// claim: an ordinary status DoT is a tick of an effect, while the arcane's
    /// split is a damage INSTANCE the wiki names as one — so it rolls the
    /// per-instance multipliers a hit rolls, this one included. Reported from
    /// play through the owner (2026-08-08): "衰弱触发的 dot 可以再次触发……外围
    /// 20 倍但是网站里的计算器显示不出来". MEASUREMENTS M37.
    attrition: f64,
    /// The EXTRA HIT bracket of the weapon that fired this instance —
    /// `1 + Σ elemental bonuses + Σ (base-attack IPS share × that IPS bonus)`,
    /// read off the ACTIVE FORM's base attack (`DummyParams::extra_hit_bracket`).
    ///
    /// It travels with the instance for ONE reason: a Blast stack detonates
    /// 1.5 s later, in `process_ticks`, where nothing about the weapon is in
    /// scope any more — and the extra hit that fires off that detonation is
    /// multiplied by this bracket even though the detonation itself takes no
    /// elemental bonus at all. Snapshotting it at application time is also what
    /// makes an ability-granted element (Nourish) that has since EXPIRED still
    /// count for a stack applied while it was up, the same rule `value`
    /// already follows.
    xh_bracket: f64,
}

/// The GunCO-family bracket for one damage instance — MECHANICS §6. Every
/// source contributes rate × its TARGET counter, is scaled by the
/// original-base fraction, and combines per the weapon's [`CoBehavior`].
///
/// Shared by the direct hit and the lingering FIELD, because the CO catalog
/// puts the cloud on the SAME rate and the SAME behavior as the main fire:
///
/// | weapon | attack | base | CO base | % | behavior |
/// | --- | --- | --- | --- | --- | --- |
/// | Torid | Main-fire | 100 | 100 | 100% | Multiplying |
/// | Torid | Toxin AoE Cloud | 40 | 40 | 100% | Multiplying |
///
/// The counter is read HERE rather than snapshotted when the field spawned —
/// Pox's row in the same catalog: "Damage recalculates on every tick".
#[allow(clippy::too_many_arguments)]
fn gunco_bucket(
    params: &DummyParams,
    ap: &DummyParams,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    at: f64,
    bd: f64,
    arc_bd: f64,
    arc_ratio: f64,
    // The below-half-health bonus, routed HERE rather than into `arc_bd`
    // because its bracket is the weapon's CO bracket — see the call site.
    half_hp: f64,
    // Which fraction of the EVOLVED base CO multiplies. The direct hit's lives
    // on `ap`; an explosion carries its own, because an evolution can raise
    // what the explosion deals without raising what CO reads.
    co_base_fraction: f64,
) -> f64 {
    let co_rate = ap.co_per_type
        + params
            .co_stack
            .as_ref()
            .map_or(0.0, |s| s.per_stack * gal.co.current(at, s.duration) as f64);
    let cold = debuffs
        .cold_status_count(at)
        .min(params.arcane.cold_cap);
    let gunco_total = [
        (co_rate, debuffs.distinct_statuses() as u32),
        (params.arcane.per_cold_bd, cold),
    ]
    .iter()
    .map(|(rate, count)| rate * *count as f64)
    .sum::<f64>()
        * co_base_fraction;
    // THE HALF-HEALTH BONUS SHARES THIS BRACKET, so it shares its base fraction
    // too. The Dread's page spells out all three halves of that: its conditional
    // bonus "ignores the base damage increase from the same perk, the 2x damage
    // from the charged shot (Primary Fire only) and Galvanized Aptitude's damage
    // bonus" — the second is `co_base_fraction` (0.5 on a bow's charged entry),
    // and the third falls out of being CO's SIBLING here rather than nested
    // inside it.
    let half_hp = half_hp * co_base_fraction;
    match ap.co_behavior {
        // Joins the base-damage bucket: diluted by Hornet Strike, sharing the
        // bracket with the arcane's bonus.
        crate::loadout::CoBehavior::AdditiveWithBaseDamage => {
            (1.0 + bd + arc_bd + gunco_total + half_hp) / (1.0 + bd)
        }
        crate::loadout::CoBehavior::Independent => arc_ratio * (1.0 + gunco_total + half_hp),
        // No CO bracket to join, so the ordinary one: the base-damage bucket.
        crate::loadout::CoBehavior::Inert => arc_ratio * (1.0 + bd + half_hp) / (1.0 + bd),
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
fn fire_syndicate_radial(
    sy: &crate::syndicates_data::SyndicateDef,
    r: &mut RunResult,
    target: &mut TargetState,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    params: &DummyParams,
    rng: &mut Rng,
    at: f64,
) {
    let sd = params.status_duration_mult;
    let mit = debuffs.mitigation(at, sd, params.armor_strip_per_puncture);
    let amt = sy.damage * params.faction_at_time(at);
    let (eff, killed, _broke) = target.apply(
        amt,
        TypeShares::single(sy.element),
        false,
        at,
        &params.target,
        false,
        &mit,
    );
    r.total_damage += amt;
    r.effective_damage += eff;
    r.sources.syndicate += eff;
    r.sources.syndicate_by_type[sy.element as usize] += eff;
    r.timeline.add(at, eff);
    r.note_kills(u32::from(killed), at);
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
                crit_mult: 1.0,
                part_factor: 1.0,
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
            rng,
            DEPTH_PROC,
        );
    }
}

/// A continuous weapon's damage RAMP — wiki Continuous_Weapon, verbatim:
/// "Initial damage starts at a lower percentage, and ramps up to 100% of its
/// damage over 0.6 seconds of hitting a target. 0.8 seconds after the weapon
/// stops hitting a target, the damage decays back to its initial point over 2
/// seconds. For most weapons, this lower percentage is 20%."
///
/// The floor is PER WEAPON — the same page lists exceptions (Convectrix 60/80%,
/// Phage 70%, Embolist 30%), and the roster now has one: Phantasma Prime ramps
/// "from 15% to 100%", not from 20%. So this constant is the DEFAULT the
/// sentence gives ("for most weapons"), and a weapon that disagrees says so in
/// its own file (`beam_ramp_floor`).
pub(crate) const BEAM_RAMP_FLOOR: f64 = 0.20;

/// The HELD-TRIGGER SPOOL after `shots` consecutive pulls — a fraction of the
/// live fire rate, 1.0 where the weapon has none.
///
/// BOTH DIRECTIONS, one line: the Phenmor falls from 1.0 to 0.6 and a Gorgon
/// climbs from 0.2 to 1.0, and nothing here needs to know which. It is linear
/// because every source gives two ends and a count and nothing in between.
///
/// The ramp above is a different animal despite the family resemblance: a beam
/// climbs in SECONDS and holds, this moves in SHOTS and is a cadence — which is
/// why a fire-rate mod does not buy its way out of it (the mod raises both ends
/// together).
fn spool_factor(spec: Option<crate::weapons_data::SustainedFireRate>, shots: f64) -> f64 {
    match spec {
        Some(s) if s.over_shots > 0.0 => {
            s.start + (s.end - s.start) * (shots / s.over_shots).min(1.0)
        }
        _ => 1.0,
    }
}

/// What a KILL gives the weapon that made it, as a fraction of the enemy's
/// affinity. VERBATIM (wiki Affinity): "Kill with weapons: Half Affinity goes
/// to the Warframe and half to the killing weapon."
///
/// The general 25/75 split on that page is for SHARED affinity — orbs, ally
/// kills, ability casts — and is not this. A weapon that killed something takes
/// half, whatever else is equipped.
const WEAPON_AFFINITY_SHARE: f64 = 0.5;
const BEAM_RAMP_SECONDS: f64 = 0.6;
const BEAM_DECAY_DELAY: f64 = 0.8;
const BEAM_DECAY_SECONDS: f64 = 2.0;

/// Progress along a continuous weapon's damage ramp: 0 = the 20% floor, 1 =
/// full damage. Advanced by holding fire on a target and decayed by stopping.
#[derive(Debug, Clone, Copy, Default)]
struct BeamRamp {
    progress: f64,
    last_tick: Option<f64>,
}

impl BeamRamp {
    /// The multiplier for a tick at `now`, then advance the ramp by one tick's
    /// worth of held fire. The tick landing NOW is scaled by the progress it
    /// arrives with, so the first tick of a burst deals the floor.
    fn tick(&mut self, now: f64, tick_seconds: f64, floor: f64) -> f64 {
        if let Some(prev) = self.last_tick {
            let idle = now - prev - tick_seconds;
            if idle > BEAM_DECAY_DELAY {
                self.progress =
                    (self.progress - (idle - BEAM_DECAY_DELAY) / BEAM_DECAY_SECONDS).max(0.0);
            }
        }
        let mult = floor + (1.0 - floor) * self.progress;
        self.progress = (self.progress + tick_seconds / BEAM_RAMP_SECONDS).min(1.0);
        self.last_tick = Some(now);
        mult
    }
}

/// One live LINGERING FIELD attached to the target — one entity per grenade
/// that stuck, since each multishot projectile is its own grenade and its own
/// cloud. `FieldStacking::Refresh` keeps this list at length 1.
#[derive(Debug, Clone, Copy)]
struct FieldState {
    next_tick: f64,
    ticks_left: u32,
    /// The part AS RESOLVED BY THE FORM THAT SPAWNED IT. A cloud outlives a
    /// transmute, and only one form of a transform group has a field at all, so
    /// the field cannot be re-read from the active form.
    part: crate::loadout::ResolvedLingering,
    /// Plentiful Mayhem: the independent damage multiplier the SPAWNING pellet
    /// carried (1.0 for the weapon's own projectile, 1+bonus for one multishot
    /// generated). Per field, because within one pull some grenades have it and
    /// some do not.
    damage_mult: f64,
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
struct FieldCtx {
    /// Attacker BuffBar flat crit chance — ABSOLUTE, lands on every part.
    flat_crit: f64,
    /// Σ RELATIVE crit-chance bonuses from MOD buffs.
    cc_rel_mods: f64,
    /// Σ live base-damage bucket additions from MOD/evolution buffs.
    bd_add_mods: f64,
}

/// Settle every FIELD tick due strictly before `until`, oldest first.
///
/// A separate pass from [`process_ticks`] on purpose — a field tick is weapon
/// damage that rolls its own crit and its own status, not a status settlement —
/// but the two are INTERLEAVED here: every status event preceding a field tick
/// is settled before it. That matters in both directions. A field tick's own
/// procs become DoTs that must still burn (the end-of-run drain used to run
/// before the last clouds ticked, so those procs were pushed and never settled
/// — a test caught it), and the CO bonus a tick reads has to include the
/// statuses its predecessors applied.
#[allow(clippy::too_many_arguments)]
fn process_field_ticks(
    fields: &mut Vec<FieldState>,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    until: f64,
    target: &mut TargetState,
    params: &DummyParams,
    ap: &DummyParams,
    ctx: &FieldCtx,
    r: &mut RunResult,
    // A field tick decides BOTH a crit and its procs, so it takes the whole
    // set of streams rather than one of them.
    d: &mut crate::rng::Draws,
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
        process_ticks(debuffs, gal, arc, at + 1e-9, target, params, ap, r, &mut d.status);
        let part = fields[i].part;
        let dmg_mult = fields[i].damage_mult;
        fields[i].next_tick += 1.0 / part.tick_rate;
        fields[i].ticks_left -= 1;
        let killed = field_tick(
            &part, dmg_mult, at, ctx, debuffs, gal, arc, target, params, ap, r, d,
        );
        if killed {
            // Fresh individual: the clouds were stuck to the one that died, and
            // its statuses go with it (the same rule the pellet path follows).
            *debuffs = DebuffState::default();
            fields.clear();
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
fn field_tick(
    f: &crate::loadout::ResolvedLingering,
    // Plentiful Mayhem's independent multiplier, carried from the grenade that
    // left this cloud (1.0 = the weapon's own projectile, or no such perk).
    dmg_mult: f64,
    at: f64,
    ctx: &FieldCtx,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    target: &mut TargetState,
    params: &DummyParams,
    ap: &DummyParams,
    r: &mut RunResult,
    d: &mut crate::rng::Draws,
) -> bool {
    let sd = params.status_duration_mult;
    let mit = debuffs.mitigation(at, sd, params.armor_strip_per_puncture);
    // The field is its own attack part, so the ability elements are sized off
    // ITS ModifiedBase — same rule as the explosion's.
    let qvec = params.with_ability_elements(f.damage.quantized(), f.modified_base, at);
    let qtotal = qvec.total();
    let shares = TypeShares::of(&qvec);

    // Crit: the field's OWN base stats. Relative bonuses scale its base,
    // absolute ones land flat (MECHANICS §7) — and no `part.crit_bonus`
    // doubling, which is the crit-HEADSHOT rule.
    let cc_rel = ctx.cc_rel_mods + params.arcane.cc_rel;
    let cc = f.crit_chance + ctx.flat_crit + f.base_crit_chance * cc_rel;
    let tier = upgrade_crit_tier(roll_crit_tier(cc, &mut d.spine), ap.crit_tier_upgrade_chance, &mut d.spine);
    let cd_rel = arc.total(&params.arcane.buffs, ArcGrant::CritDamage, at)
        + arc.cd_bonus(ap, at)
        + params.arcane.cd_rel;
    let cd = f.crit_damage + f.base_crit_damage * cd_rel + debuffs.cold_cd_bonus(at);
    let crit_mult = 1.0 + tier as f64 * (cd - 1.0);

    // Damage buckets: the same live base-damage additions the direct hit reads,
    // then the GunCO bracket off the target's CURRENT status count.
    let bd = ap.base_damage_bonus;
    let arc_bd = arc.total(&params.arcane.buffs, ArcGrant::BaseDamage, at) + ctx.bd_add_mods;
    let arc_ratio = (1.0 + bd + arc_bd) / (1.0 + bd);
    // CO on an AoE part is the EXCEPTION, not the default. What the mods say is
    // direct hits only — which is why the radial path never takes it — and the
    // Torid's cloud is an anomaly the CO catalog gives its own row (user,
    // 2026-07-30: in theory an AoE would not get it, but DE let this one). So
    // the field takes CO only where the weapon declares it; otherwise it gets
    // the same bracket the radial does.
    let bucket = if f.takes_condition_overload {
        // A FIELD keeps the direct hit's base fraction: the CO catalog puts
        // the Torid's cloud on the same base as its main fire.
        // A FIELD TICK carries no half-health term: the bonus is a DIRECT-hit
        // bonus like CO itself, and nothing in the catalog says otherwise.
        gunco_bucket(params, ap, debuffs, gal, at, bd, arc_bd, arc_ratio, 0.0, ap.co_base_fraction)
    } else {
        arc_ratio
    };
    let mb_live = f.modified_base * arc_ratio;

    // Falloff is 1.0: the grenade STICKS to the target, so the target stands at
    // the epicentre for every tick — which is exactly why the wiki calls a
    // direct hit "the maximum possible damage".
    //
    // `dmg_mult` (Plentiful Mayhem) rides here rather than inside ModifiedBase:
    // the wiki calls it "multiplicative to base damage bonuses like Serration",
    // i.e. its own bracket. Consequence, recorded because nothing sources it:
    // the status payloads below are left OUT of it, the same treatment the beam
    // ramp and Devouring Attrition already get.
    // Depth 1: a direct hit carries the faction bonus once. Written through
    // `faction_at` like the other two rungs so the ladder is visible at every
    // level rather than only where it compounds.
    let raw =
        qtotal * crit_mult * bucket * faction_at(params.faction_at_time(at), DEPTH_HIT)
            * dmg_mult
            * params.ability_final_at(at);
    let col = target.incoming_column(&params.target);
    let (effective, killed, broke) =
        target.apply(raw, shares, false, at, &params.target, false, &mit);
    r.total_damage += raw;
    r.effective_damage += effective;
    r.sources.field += effective;
    add_by_type(&mut r.sources.field_by_type, &qvec, effective, &col);
    r.timeline.add(at, effective);
    r.field_ticks += 1;
    r.note_kills(killed as u32, at);
    if let Some(pool) = broke {
        push_break_proc(debuffs, params, at, pool);
    }
    if killed {
        gal.bump_on_kill(params, at);
        arc.on_kill(params, at);
        return true;
    }
    // Status per TICK, from the field's own vector and its own status chance.
    // No forced procs: those are declared per attack part, and the cloud
    // declares none.
    //
    // A tick is its OWN damage instance, at its own time — so a per-instance
    // arcane cap resets here rather than sharing the shot's allowance.
    arc.next_instance();
    let procs = status::procs_for_hit(
        &[],
        f.status_chance,
        &qvec,
        &params.target.status_immunities,
        &mut d.status,
    );
    settle_procs(
        procs,
        at,
        InstanceScale {
            mb_live,
            crit_mult,
            part_factor: 1.0,
            attrition: 1.0,
            // The BASE ATTACK's, not the cloud's: a Blast stack the cloud
            // applies still detonates off a gun, and the bracket its extra hit
            // takes is that gun's.
            xh_bracket: ap.extra_hit_bracket(at),
        },
        debuffs,
        gal,
        arc,
        target,
        params,
        ap,
        &mit,
        r,
        &mut d.status,
        DEPTH_PROC,
    );
    false
}

/// Timed STATUS events due strictly before `until`, in chronological order:
/// DoT ticks (Bleed/Toxin/Electricity/Gas + break-proc Tesla), the Heat
/// singleton's anchored ticks, and Blast fuse expiries. Mitigation is evaluated
/// LIVE at each event (the snapshot boundary rule); status damage never procs
/// status.
///
/// Lingering-FIELD ticks are NOT here — they are weapon damage that rolls its
/// own crit and its own status, so they get their own pass
/// ([`process_field_ticks`]).
/// `rng` is the STATUS stream, and it is here for exactly one payload: a Blast
/// detonation triggers an EXTRA HIT, which rolls a status of its own. Every
/// other event this function settles is a payload already decided.
#[allow(clippy::too_many_arguments)]
fn process_ticks(
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    until: f64,
    target: &mut TargetState,
    params: &DummyParams,
    ap: &DummyParams,
    r: &mut RunResult,
    rng: &mut Rng,
) {
    enum Ev {
        Dot(usize),
        Heat,
        Blast(usize),
    }
    let p = &params.target;
    let sd = params.status_duration_mult;
    loop {
        let mut best: Option<(f64, Ev)> = None;
        let consider = |t: f64, ev: Ev, best: &mut Option<(f64, Ev)>| {
            if t < until && best.as_ref().is_none_or(|(bt, _)| t < *bt) {
                *best = Some((t, ev));
            }
        };
        for (i, d) in debuffs.dots.iter().enumerate() {
            if d.ticks_left > 0 {
                consider(d.next_tick, Ev::Dot(i), &mut best);
            }
        }
        if let Some(h) = &debuffs.heat {
            if h.next_tick <= h.expiry {
                consider(h.next_tick, Ev::Heat, &mut best);
            }
        }
        for (i, b) in debuffs.blast.iter().enumerate() {
            consider(b.fuse, Ev::Blast(i), &mut best);
        }
        let Some((now, ev)) = best else { break };

        let mit = debuffs.mitigation(now, sd, params.armor_strip_per_puncture);
        // A tick is one damage type — which is also the type the
        // vulnerability column reads. Bleed is the exception: it is stored
        // under Slash (that is the proc that made it) but the damage is
        // CINEMATIC, which takes no faction modifier anywhere, and
        // `ignores_armor` is this file's marker for it.
        // `xh` is the EXTRA HIT bracket, and `Some` is what says this payload
        // triggers one at all. A DoT tick and a Heat tick are `None` — no extra
        // hit fires off a status — and the Blast detonation is the documented
        // exception (wiki `Extra_Hit`, Bugs: "Only Xata's Whisper will be
        // triggered by blast Detonations, no other extra hit will").
        let (value, ignores_armor, is_dot_tick, hit_type, src, xh) = match &ev {
            Ev::Dot(i) => {
                let d = &mut debuffs.dots[*i];
                d.next_tick += 1.0;
                d.ticks_left -= 1;
                let d = &debuffs.dots[*i];
                let hit_type = if d.ignores_armor {
                    DamageType::Cinematic
                } else {
                    d.dtype
                };
                (d.value, d.ignores_armor, true, hit_type, d.dtype, None)
            }
            Ev::Heat => {
                let h = debuffs.heat.as_mut().expect("heat event needs entity");
                h.next_tick += 1.0;
                (h.value, false, true, DamageType::Heat, DamageType::Heat, None)
            }
            Ev::Blast(i) => {
                let b = debuffs.blast.remove(*i);
                (
                    b.value,
                    false,
                    false,
                    DamageType::Blast,
                    DamageType::Blast,
                    Some(b.xh_bracket),
                )
            }
        };

        // SECONDARY FORTIFIER REACHES A TICK TOO, and only while the Overguard
        // it is about is still there.
        //
        // The wiki's own words are what puts it here: the bonus is "DYNAMICALLY
        // APPLIED, so the effect is lost entirely after depleting the Overguard
        // from an enemy" — a live check at the moment damage lands, which is
        // exactly what a tick is. The card says "Deals x8 Extra Damage to
        // Overguard" with no qualifier about hits, and the same page says DoTs
        // do trigger the STEAL half.
        //
        // "Not inheritable" is not evidence against this: it names `Heat_Inherit`
        // — the mechanic that attributes later Heat damage to whoever applied the
        // first Heat status — and says the damage bonus does not travel down THAT
        // path (owner, 2026-08-09: "这里的继承应该是说……在warframe引擎看来，还是
        // 这把枪造成的").
        //
        // ONCE, NOT SQUARED. Faction damage is re-applied per derivation step
        // because DE re-applies it (`faction_at(f, depth)`); nothing says that
        // about this, and nothing here is a derivation — the tick is the same
        // instance's payload landing later (owner: "那dot也是9倍，而不是9*9倍率
        // 吧"). The multiplier's own VALUE is a separate open question: our data
        // reads DE's "x8" as the total and it may be a total of x9 (MEASUREMENTS
        // M38).
        let value = value
            * if target.overguard > 0.0 {
                params.arcane.overguard_mult
            } else {
                1.0
            };
        let (effective, killed, broke) = target.apply(
            value,
            TypeShares::single(hit_type),
            false,
            now,
            p,
            ignores_armor,
            &mit,
        );
        r.total_damage += value;
        r.effective_damage += effective;
        r.dot_damage += effective;
        r.sources.add_status(src, effective);
        r.timeline.add(now, effective);
        r.dot_ticks += is_dot_tick as u32;
        r.note_kills(killed as u32, now);
        if let Some(pool) = broke {
            push_break_proc(debuffs, params, now, pool);
        }
        if killed {
            // Status-proc kills grant Galvanized stacks too (GS rules),
            // and count for on-kill arcanes (Merciless — NOT Deadhead,
            // whose precision boundary excludes status-proc kills).
            gal.bump_on_kill(params, now);
            arc.on_kill(params, now);
            // Fresh individual: clean DebuffBar (decision 2026-07-24).
            *debuffs = DebuffState::default();
            break;
        }
        // …and the detonation's EXTRA HIT, off the value that actually landed —
        // Fortifier's multiplier included, since it multiplied the instance this
        // is a percentage of. No body part: a detonation struck none.
        if let Some(bracket) = xh {
            if fire_extra_hits(
                value, bracket, 1.0, false, ap.status_chance, now, debuffs, gal, arc, target,
                params, ap, &mit, r, rng,
            ) {
                break;
            }
        }
    }
    debuffs.dots.retain(|d| d.ticks_left > 0);
}

/// Live reload time for the active form: the arcane's reload-speed sources
/// (Merciless r5 static, Conjunction Voltage stacks) and Lethal
/// Rearmament's headshot stacks join the form's reload-speed BUCKET —
/// time = base / (1 + bucket + live additions).
fn live_reload_time(
    form: &DummyParams,
    outer: &DummyParams,
    arc: &mut ArcRuntime,
    live_rs: f64,
    t: f64,
) -> f64 {
    let add =
        outer.arcane.reload_bonus + arc.total(&outer.arcane.buffs, ArcGrant::ReloadSpeed, t) + live_rs;
    reload_span(form.reload_seconds, form.reload_bonus, add)
}

/// Rescale a time already divided by `(1 + bucket)` so it also carries a
/// LIVE addition to the same bucket — the transmute animations, which the
/// wiki ties to reload speed.
fn rescale_reload(secs: f64, bucket: f64, live: f64) -> f64 {
    reload_span(secs, bucket, live)
}

/// A RELOAD IS PAID FOR WHILE IT RUNS, not priced when it starts.
///
/// The arithmetic is a work integral, which is what makes a reload-speed total
/// mean anything: a reload is `secs x (1 + bucket)` seconds of WORK and a total
/// of `add` retires it at `1 + bucket + add` per second.
///
/// IT USED TO TAKE A LAPSING WINDOW, because Ready Retaliation was modelled as
/// a 6 s buff that could run out halfway through and leave the rest of the
/// reload at the slower rate. It cannot any more: that buff is scoped to the
/// reload ACTION — it arrives when the reload starts and is gone when it ends
/// (owner, 2026-08-11) — so there is nothing left that lapses mid-reload, and
/// the partial-rate branch went with it. If a lapsing reload buff ever exists
/// again, this is where it goes back.
///
/// `secs` arrives ALREADY divided by `(1 + bucket)` — it is the modded reload —
/// so multiplying it back out is what recovers the work, and a weapon with no
/// live bonus at all falls straight through to `secs`.
fn reload_span(secs: f64, bucket: f64, add: f64) -> f64 {
    if add <= 0.0 {
        return secs;
    }
    secs * (1.0 + bucket) / (1.0 + bucket + add)
}

/// EVERY LIVE ADDITION TO THE RELOAD-SPEED BUCKET, in one place.
///
/// `owner` is the form the perks belong to — the base half of a cycle, whose
/// evolutions are on it — while `params` carries the buff bar. They differ, and
/// reading the wrong one is not a small mistake: Ready Retaliation is dropped
/// on a charge-backed form, so taking it off the outer params gave every
/// transform animation a bonus of exactly nothing on the only weapon that has
/// the perk.
///
/// READY RETALIATION IS A BUFF THAT IS UP OR DOWN, with no duration and no
/// condition of its own (owner, 2026-08-11: "最正确的建模就是建立一个无限时长的
/// buff，只是这个buff换弹完成/进入灵化的时候消失，不应该在意此时是否换弹"). So
/// it is summed here, where the total is asked for, and NOT tested at each of
/// the four places that ask — the events that REMOVE it are the only place it
/// is reasoned about. Four copies of one `if` is how three of them come to
/// disagree with the fourth.
fn live_reload_speed(
    params: &DummyParams,
    owner: &DummyParams,
    rs_armed: bool,
    stacks: &mut [LiveStacks],
    t: f64,
) -> f64 {
    let ready = if rs_armed { owner.rs_on_reload } else { 0.0 };
    ready +
        params
            .stacking_buffs
            .iter()
            .enumerate()
            .filter(|(_, b)| b.grant == crate::loadout::BuffGrant::ReloadSpeed)
            .map(|(i, b)| b.per_stack * stacks[i].current(t, b.duration) as f64)
            .sum::<f64>()
}

/// The live stack count of every buff in [`Replay::buffs`], in that order.
///
/// The roster and this reader are two halves of one fact and sit as close
/// together as the code allows: `buff_roster` says what exists, this says
/// where it currently is. A `_ => 0` arm would let a rostered buff draw a flat
/// line forever and look like a finding, so the match is written to be read
/// against the roster, entry for entry.
///
/// Several containers hold "stacks" here because the sim's buffs genuinely
/// live in different shapes — decaying stack lists, single expiries, a
/// weapon passive. Normalising them into one number is this function's whole
/// job; nothing downstream should learn the difference.
#[allow(clippy::too_many_arguments)]
fn sample_stacks(
    params: &DummyParams,
    rep: &Replay,
    now: f64,
    arc: &mut ArcRuntime,
    gal: &mut GalStacks,
    buff_stacks: &mut [LiveStacks],
    ch_stacks: &[f64],
    ch_buff_expiry: f64,
    fr_reload_expiry: f64,
    bd_reload_expiry: f64,
    streak_expiry: f64,
    tendrils: u32,
    bar: &BuffBar,
) -> Vec<u8> {
    let cap = |n: u32| n.min(u8::MAX as u32) as u8;
    let live = |on: bool| u8::from(on);
    rep.buffs
        .iter()
        .map(|(id, _max)| match id.as_str() {
            // A weapon passive, not a stack: it is up or it is not.
            "frenzy" => live(bar.get(crate::perks::frenzy::BUFF_ID).is_some()),
            // PERMANENT (no trigger, no decay): whatever it was configured to,
            // for the whole run. `multishot` already carries it, so the count
            // is reconstructed from the fraction that survived the config.
            "evo_multishot" => params.evo_ms.map_or(0, |ms| cap(ms.stacks)),
            // Permanent like the one above: whatever it was configured to.
            "evo_reload_damage" => params.evo_bd.map_or(0, |bd| cap(bd.stacks)),
            "condition_overload" => cap(gal.co.current(now, dur(&params.co_stack))),
            "on_kill_multishot" => cap(gal.ms.current(now, dur(&params.ms_stack))),
            "on_headshot_kill_cc" => cap(ch_stacks.iter().filter(|&&e| e > now).count() as u32),

            // THE WHOLE STACKING FAMILY, by id. The roster pushed these ids
            // from the same Vec this reads, so a rostered buff can never fall
            // through to a zero it did not earn.
            other if params.stacking_buffs.iter().any(|b| b.id == other) => {
                let i = params.stacking_buffs.iter().position(|b| b.id == other).unwrap_or(0);
                let d = params.stacking_buffs[i].duration;
                cap(buff_stacks[i].current(now, d))
            }
            // Read off the loop's own counter rather than re-derived: the
            // fight is the only thing that knows how many are up.
            "tendrils" => cap(tendrils),
            "on_headshot_cc" => live(now < ch_buff_expiry),
            "on_kill_cd" => live(now < arc.cd_kill_expiry()),
            "on_reload_bd" => live(now < bd_reload_expiry),
            "on_reload_fr" => live(now < fr_reload_expiry),
            "evo_headshot_streak" => live(now < streak_expiry),
            // The perk keeps its stacks on the BAR, not in `arcane.buffs`.
            "arcane:secondary_enervate" => {
                cap(bar.get("secondary_enervate").map_or(0, |b| b.stacks))
            }
            other => match other.strip_prefix("arcane:") {
                // One card per arcane: every spec it owns shares a count, so
                // the first one answers for all of them.
                Some(owner) => cap(arc.owner_stacks(&params.arcane, owner, now)),
                None => 0,
            },
        })
        .collect()
}

/// A stacking spec's decay period, or 0 when the spec is absent.
fn dur(spec: &Option<crate::loadout::StackSpec>) -> f64 {
    spec.as_ref().map_or(0.0, |s| s.duration)
}

pub fn run_once(params: &DummyParams, rng: &mut Rng) -> RunResult {
    run_once_traced(params, rng, None)
}

/// One engagement, optionally SAMPLED into a [`Replay`].
///
/// `trace` is `Some` for exactly one run per `monte_carlo` — the median one,
/// replayed afterwards from its recorded RNG state. Threading an `Option`
/// through rather than duplicating the loop is the point: a traced run and a
/// scored run must be the same code, or the replay shows a fight that did not
/// happen (user, 2026-08-02).
pub fn run_once_traced(
    params: &DummyParams,
    rng: &mut Rng,
    mut trace: Option<&mut Replay>,
) -> RunResult {
    // THE ENGAGEMENT'S SEED, and the three streams derived from it. The master
    // `rng` is only a seed source from here on: it is advanced once so the next
    // run in a `monte_carlo` differs, and every roll below comes off `d`. See
    // [`Draws`] for why the streams are split — in short, a build that changes
    // only its status chance must not re-roll this engagement's crits.
    let started_at = rng.state();
    let _ = rng.next_f64();
    let d = &mut crate::rng::Draws::new(started_at);
    let mut next_frame = 0.0f64;
    let frame_dt = trace.as_ref().map_or(f64::INFINITY, |r| r.dt);
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
    let mut target = TargetState::spawn(&params.target);
    let mut debuffs = DebuffState::default();
    // On-kill stack buffs start at their configured initial stacks (full
    // per the user's setting) with a fresh duration from t = 0.
    let mut gal = GalStacks::default();
    if let Some(s) = &params.co_stack {
        gal.co = LiveStacks::seed(s.initial_stacks, s.max_stacks, s.duration);
    }
    if let Some(s) = &params.ms_stack {
        gal.ms = LiveStacks::seed(s.initial_stacks, s.max_stacks, s.duration);
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
            crate::loadout::BuffDecay::PerStackExpiry => {
                LiveStacks::seed_per_stack(b.initial_stacks, b.max_stacks, b.duration)
            }
            crate::loadout::BuffDecay::LoseOneAndReset => {
                LiveStacks::seed(b.initial_stacks, b.max_stacks, b.duration)
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
    let mut r = RunResult { rng_state: started_at, ..Default::default() };
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
    macro_rules! bump_buffs {
        ($trigger:expr, $t:expr, $rng:expr) => {
            for (i, b) in params.stacking_buffs.iter().enumerate() {
                if b.trigger == $trigger && (b.chance >= 1.0 || $rng.chance(b.chance)) {
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
                    && b.trigger == crate::loadout::BuffTrigger::ReloadComplete
                    && (b.chance >= 1.0 || $rng.chance(b.chance))
                {
                    for _ in 0..$n {
                        buff_stacks[i].bump($t, b.duration, b.max_stacks);
                    }
                }
            }
        };
    }
    // THE MAGAZINE IS FULL AGAIN — a reload that COMPLETED, or either Incarnon
    // transform completing, since swapping either way fully reloads the base
    // form's magazine (wiki).
    //
    // One macro rather than the same three lines at four sites: everything that
    // a refill ends, ends here. Ready Retaliation is spent, Reaver's Rapture is
    // reset, and the burst count restarts. THE MOMENT IS THE COMPLETION — a
    // reload that has begun has refilled nothing (owner, 2026-08-11: "时间节点
    // 一定要处理好…不可以含糊，要准确").
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
            if !opening_closed {
                opening_closed = true;
                r.first_magazine_damage = r.effective_damage;
            }
            for (i, b) in params.stacking_buffs.iter().enumerate() {
                let cleared = match b.cleared_by {
                    crate::loadout::ClearedBy::MagazineRefilled => true,
                    crate::loadout::ClearedBy::Reload => $reload,
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
            bump_on_trigger!(crate::loadout::BuffTrigger::ReloadComplete, $t, $rng);
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
            bump_on_trigger!(crate::loadout::BuffTrigger::ReloadFromEmpty, $t, $rng);
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
            if let Some((per, max)) = params.mag_growth_on_empty_reload {
                if mag_growth_stacks < max {
                    mag_growth_stacks += 1;
                    mag_cap += per;
                }
            }
        };
    }
    // SHELLS OWED TO THE PLAYER FOR THE RELOAD THEY ARE HALFWAY THROUGH.
    //
    // Entering the Incarnon form IS a reload — the transmute animation is the
    // weapon's reload time, which is how you can tell (owner, 2026-08-08) — and
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
                if let crate::loadout::BuffTrigger::HitEnemyWithStatus(s) = b.trigger {
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
    let mut fr_reload_expiry: f64 = params
        .fr_on_reload
        .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 });
    // Crosshairs (per-stack expiry FIFO + one refreshable buff); the on-head
    // buff seeds active per its `initial_active` (default on).
    let mut ch_buff_expiry: f64 = params
        .cc_on_headshot
        .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 });
    // Crosshairs keeps a per-stack expiry rather than one clock — and takes
    // an infinite duration exactly like the rest.
    let mut ch_stacks: Vec<f64> = params
        .cc_stack
        .as_ref()
        .map_or(Vec::new(), |s| vec![s.duration; s.initial_stacks as usize]);


    // Per-phase precomputation: the quantized vector is static per phase
    // (no dynamic mods); ModdedBase for proc payload formulas stays
    // pre-quantization and EXCLUDES elemental portions (base × (1 + dmg)).
    let precompute = |p: &DummyParams| {
        let qvec = p.damage.quantized();
        let qtotal = qvec.total();
        let mb = p.dot_modified_base.unwrap_or_else(|| p.damage.total());
        (qvec, qtotal, mb)
    };
    let main_pre = precompute(params);
    let base_pre = params.cycle.as_ref().map(|c| precompute(&c.base_form));
    let sd = params.status_duration_mult;
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
    let mut field_ctx = FieldCtx::default();
    // The form whose panel SPAWNED the fields. Only one form of a transform
    // group has a lingering part (Torid's cloud belongs to the base form; its
    // Incarnon beam leaves none), so this is unambiguous - and it is the right
    // answer even after a transmute, because a cloud outlives one.
    let field_ap: &DummyParams = match &params.cycle {
        Some(cy) if cy.base_form.lingering.is_some() => &cy.base_form,
        _ => params,
    };

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

    // Fire while t < duration; the inter-shot interval is 1/(base rate x
    // live BuffBar fire-rate multiplier), evaluated after each shot (a buff
    // expiring mid-interval is approximated to the shot boundary).
    let mut t = 0.0f64;
    let mut magazine = mag_cap;
    let mut reserve = params.reserve_ammo;
    // GOTVA PRIME'S PASSIVE, armed. Set by a pellet that landed a status, spent
    // by the next pellet that lands. It survives across shots and reloads: the
    // card says the chance "remains until landing another successful shot", and
    // nothing but a landing shot spends it.
    let mut super_crit_armed = false;
    // Deadly Efficiency's window. Opens at reload COMPLETION — `t` is already
    // past the reload when this is set, the same as `fr_reload_expiry` — and
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
    let mut bd_reload_expiry: f64 = params
        .bd_on_reload
        .map_or(0.0, |b| if b.initial_active { b.duration } else { 0.0 });
    // Incarnon cycle state. The engagement opens in the BASE form and earns
    // its way in — see `IncarnonCycle::starts_primed` for why, and for the
    // reading that opens transformed.
    let mut in_base_form = params.cycle.as_ref().is_some_and(|c| !c.starts_primed);
    // READY RETALIATION IS ARMED BY THE EMPTY MAGAZINE, not by the reload.
    //
    // The owner's evidence is the transmute (2026-08-11): empty the magazine
    // and transform immediately, and the TRANSFORM is faster too — which it
    // could only be if the buff was already on the weapon before any reload
    // started. It is then spent by the next reload, and coming out of Incarnon
    // form counts as one ("退出灵化以后，这时候相当于reload了一次，这个buff消
    // 失了").
    //
    // So it is a flag rather than a clock. This card states a bonus and no
    // duration, and that is not an omission — there is nothing to time.
    let mut charges = 0u32;
    let mut base_mag = params
        .cycle
        .as_ref()
        .map_or(0.0, |c| c.base_form.magazine_size);
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
    // Which kills the magazine refill has already paid out — see the spend
    // below for why this cannot be the same watermark.
    let mut refill_kill_mark = 0u32;
    // A SYNDICATE RADIAL's gauge, in affinity, and the same derived-from-kills
    // trick the tendrils use: `r.kills` is maintained at six sites already.
    let mut syndicate_kill_mark = 0u32;
    let mut syndicate_points = 0.0f64;
    // When the weapon may convert affinity again. During the cooldown it
    // converts NOTHING — "the weapon will not convert any affinity into
    // points, and all collected points are reset to zero" — so this gates the
    // accumulation, not just the firing.
    let mut syndicate_ready_at = 0.0f64;
    let mut tendril_reload_mark = 0u32;
    // The card's opening count, which the fight then treats exactly like an
    // earned one: it is spent by the magazine event that clears the rest.
    let mut tendril_seed = params.tendrils_initial.min(params.tendril_max);
    let mut tendrils = tendril_seed;
    // HELD-TRIGGER SPOOL — shots since the trigger was last released, and the
    // moment the next one was due. See `weapons_data::SustainedFireRate`.
    let mut spool_shots = 0.0f64;
    let mut spool_due = f64::NEG_INFINITY;
    // WHEN THE LAST SHOT ACTUALLY WENT OFF, which is NOT `spool_due`. That one
    // is when the next shot was DUE, so the interval is already inside it and
    // the difference is zero on every ordinary pull — right for a spool, which
    // asks "did anything intervene", and useless for a battery, which asks how
    // long the weapon spent not firing.
    let mut last_shot_t = f64::NEG_INFINITY;
    loop {
        // SAMPLE first, so a frame shows the fight as it stood BEFORE the
        // shot at `t` — the same convention the timeline buckets use.
        // Sampling here rather than on a fixed clock is deliberate: this loop
        // is the only place that advances time, and a buff can only change on
        // an event this loop drives. A gap between shots emits repeated
        // frames, which is exactly what a fight with nothing happening in it
        // looks like.
        if let Some(rep) = trace.as_deref_mut() {
            while next_frame <= t && next_frame < params.duration_secs {
                let stacks = sample_stacks(
                    params, rep, next_frame, &mut arc, &mut gal, &mut buff_stacks,
                    &ch_stacks, ch_buff_expiry, fr_reload_expiry, bd_reload_expiry,
                    streak_expiry, tendrils, &bar,
                );
                rep.frames.push(Frame {
                    t: next_frame,
                    overguard: target.overguard,
                    shield: target.shield,
                    health: target.health,
                    damage: r.effective_damage,
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
                    stacks,
                    debuffs: debuffs.sample(next_frame),
                });
                next_frame += frame_dt;
            }
        }
        if t >= params.duration_secs {
            break;
        }

        // The buff bar has to be settled BEFORE the reload decision, not after:
        // whether an empty magazine reloads depends on what the NEXT shot would
        // cost, and that is a live buff read. Expiry is monotone, so the second
        // `bar.expire` below (at the post-reload t) is still correct.
        // Whether the NEXT shot costs zero ammo — it decides whether an empty
        // magazine reloads, so it has to be known before that branch.
        // A BATTERY REFILLS WHILE NOBODY IS SHOOTING, and it has to be counted
        // BEFORE anything asks whether this shot can be fired — otherwise an
        // empty magazine goes straight to the reload branch and the mechanic
        // never gets a turn.
        //
        // The gap is the same one a spool reads: `spool_due` is when this shot
        // was due, so `t - spool_due` is what the weapon spent not firing.
        // Rounds a second, after a delay that depends on whether anything is
        // left — see `weapons_data::Battery`.
        //
        // THE EMPTY CASE IS NOT HERE. It is the ordinary reload, whose
        // `reload_seconds` already IS `delay_empty + magazine/rate` (1.25 s on
        // the Shedu, which is why the wiki lists that as its reload time). What
        // this adds is the case a reload cannot express — the battery filling
        // BETWEEN shots, which on a weapon slowed below one shot per
        // `delay_partial` means it never empties at all.
        if let Some(b) = params.battery {
            let idle = t - last_shot_t;
            let delay = if magazine < 1e-9 { b.delay_empty_s } else { b.delay_partial_s };
            if last_shot_t.is_finite() && b.regen_per_second > 0.0 && idle > delay {
                let gained = (idle - delay) * b.regen_per_second;
                magazine = (magazine + gained).min(mag_cap);
            }
        }

        let next_cost = {
            let ap: &DummyParams = match &params.cycle {
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
                bar.total_contributions().ammo_efficiency,
                params.arcane.ammo_efficiency,
                arc.total(&params.arcane.buffs, ArcGrant::AmmoEfficiency, t),
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
                bump_on_trigger!(crate::loadout::BuffTrigger::Kill, t, d.extra);
                // EXACT PENANCE, on the same counter and for the same reason:
                // "Kills from status effects can also trigger the effect", and
                // a DoT kill happens nowhere near the direct-hit site that
                // `instant_reload_on_headshot` is wired to.
                if let Some(chance) = params.instant_reload_on_kill {
                    if d.extra.chance(chance) {
                        instant_reload_now = true;
                    }
                }
            }
        }

        // TENDRILS and the magazine they keep alive, both re-derived from the
        // counters above before anything decides to reload.
        if params.tendril_max > 0 || params.mag_refill_on_kill > 0.0 {
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
                tendril_kill_mark = r.kills;
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
            if params.mag_refill_on_kill > 0.0 && r.kills > refill_kill_mark {
                let earned = f64::from(r.kills - refill_kill_mark)
                    * params.mag_refill_on_kill
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
            tendrils = (tendril_seed + (r.kills - tendril_kill_mark)).min(params.tendril_max);
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
                    &sy, &mut r, &mut target, &mut debuffs, &mut gal, &mut arc, params,
                    &mut d.status, t,
                );
            }
        }

        // Phase transitions and reloads.
        if let Some(cy) = &params.cycle {
            if !in_base_form && magazine < 1e-9 {
                // Charge magazine spent: revert to the base form. The swap
                // fully reloads the base magazine (wiki side effect). The
                // revert does NOT count as a transform — `transforms` counts
                // TRANSMUTES INTO the Incarnon form only (user, 2026-07-29:
                // both-directions counting read as doubled).
                // COMING OUT OF INCARNON FORM IS A RELOAD too, and for the
                // same stated reason: the swap refills the base magazine. It
                // takes the speed if the buff is up and spends it — which is
                // also why this animation is scaled by reload speed at all.
                let spent = rescale_reload(cy.transmute_out_seconds, cy.reload_bucket,
                    live_reload_speed(params, &cy.base_form, rs_armed, &mut buff_stacks, t));
                r.downtime_secs += spent;
                t += spent;
                // …but it is NOT a reload, and one perk can tell the difference:
                // see `ClearedBy::Reload`.
                magazine_refilled!(also_a_reload: false);
                in_base_form = true;
                charges = 0;
                // The swap's auto-reload is the SAME mechanism as a normal one
                // (user, 2026-07-30), so it draws whole rounds rather than
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
                    if b.cleared_by == crate::loadout::ClearedBy::EmptyMagazine {
                        buff_stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
                    }
                }
                let rs = live_reload_speed(params, &cy.base_form, rs_armed, &mut buff_stacks, t);
                let spent = live_reload_time(&cy.base_form, params, &mut arc, rs, t);
                // THE OPENING WINDOW closes when the first reload STARTS, which
                // is here — everything dealt up to this instant is what the
                // magazine you walked in with was worth.
                r.downtime_secs += spent;
                t += spent;
                magazine_refilled!();
                r.reloads += 1;
                if let Some(b) = cy.base_form.fr_on_reload {
                    fr_reload_expiry = t + b.duration;
                }
                if let Some(b) = cy.base_form.bd_on_reload {
                    bd_reload_expiry = t + b.duration;
                }
                // Same whole-rounds rule as the plain reload below (M14), and
                // the same shared reserve: a short draw is a short magazine.
                let loaded = draw_from(&mut reserve, params.infinite_reserve,
                    reload_draw(cy.base_form.magazine_size, base_mag));
                base_mag += loaded;
                // ONE STACK PER SHELL THIS RELOAD LOADED, counted here and not
                // from a number resolved once at the panel.
                //
                // The static `stacks_per_trigger` is the OUTER form's magazine,
                // and in a cycle the outer form is the INCARNON one — so a
                // base-form reload of 6 shells was granting 60 stacks, straight
                // to +600% fire rate (measured 61 on the Felarx, 2026-08-10).
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
                if b.cleared_by == crate::loadout::ClearedBy::EmptyMagazine {
                    buff_stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
                }
            }
            // Cannot fire: reload (blocking) or, with dry finite reserves,
            // stop firing altogether (DoTs still drain below).
            if !params.infinite_reserve && reserve < 1e-9 {
                break;
            }
            // THE WINDOW OPENS WHEN THE RELOAD BEGINS — the player's reload
            // ACTION is the trigger, not its completion (owner, 2026-08-10:
            // "是在换弹开始的时候触发…等于给自己上了一张100% reload speed的
            // mod"). So it is armed BEFORE the line below, and the reload that
            // armed it is the first thing it speeds up.
            //
            // Every reload this loop performs is a reload from empty — it only
            // reloads when it cannot fire — which is exactly the condition.
            let rs = live_reload_speed(params, params, rs_armed, &mut buff_stacks, t);
            let spent = live_reload_time(params, params, &mut arc, rs, t);
            r.downtime_secs += spent;
            t += spent;
            magazine_refilled!();
            r.reloads += 1;
            if let Some(b) = params.fr_on_reload {
                fr_reload_expiry = t + b.duration;
            }
            if let Some(b) = params.bd_on_reload {
                bd_reload_expiry = t + b.duration;
            }
            // Whole rounds only, and `+=` not `=` — both measured (M14). The
            // draw covers the overdraw debt for free: the counter is in (−1, 0]
            // here, so `floor(capacity − current)` is a full magazine, and a
            // −0.75 counter comes back at 4.25 rather than 5.00.
            let want = reload_draw(mag_cap, magazine);
            let loaded = draw_from(&mut reserve, params.infinite_reserve, want);
            magazine += loaded;
            // …AND THE SHELLS IT LOADED PAY THEIR STACKS. One per shell, from
            // the count the draw actually produced — see the note at the cycle's
            // base-form reload for why this is per-site rather than a single
            // trigger at the top of the loop.
            bump_shells!(loaded.round().max(0.0) as u32, t, rng);
            bump_reload_only!(t, rng);
            bump_reload_from_empty!(t, rng);
            field_duration_boost = true; // reloaded from empty (Renewed Horror)
            if t >= params.duration_secs {
                break;
            }
        }

        // Active-phase view: the base form's panel during the rebuild
        // phase, the outer params otherwise. Target/aim/locks are shared
        // from the outer params.
        let ap: &DummyParams = match &params.cycle {
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
            &mut d.status,
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
        let efficiency = ammo_efficiency(
            ap.ammo_efficiency_applies,
            contribs.ammo_efficiency,
            params.arcane.ammo_efficiency,
            arc.total(&params.arcane.buffs, ArcGrant::AmmoEfficiency, t),
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
        // branch used to read "the cycle's base phase" against "everything
        // else" — and everything else includes a plain base-form run, which is
        // how a `base`-mode board row is played and how anyone measures the
        // weapon on its own. The burst window was written for the cycle and
        // silently became one round outside it: 5 pellets a magazine instead of
        // 15 on a Burston (2026-08-11).
        let mag_left = if in_base_form { base_mag } else { magazine };
        let last_round = mag_left <= last_n + 1e-9;
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
        let flat_crit = contribs.flat_crit_chance;
        let weakened_cc = WEAKENED_FLAT_CC_PER_STACK * debuffs.weakened_active(t) as f64;
        // Relative, shared by every stage: Crosshairs' on-headshot buff and
        // its per-stack-expiry kill stacks (assumes constant aiming), plus the
        // arcane's assumed-max conditionals (Overcharge/Outburst).
        let cc_rel = params.cc_on_headshot.map_or(0.0, |b| {
            if t < ch_buff_expiry {
                b.value
            } else {
                0.0
            }
        }) + params.cc_stack.as_ref().map_or(0.0, |s| {
            ch_stacks.retain(|&e| e > t);
            s.per_stack * ch_stacks.len() as f64
        }) + params.arcane.cc_rel
            // SENTIENT SURGE: "Additive to other crit chance and status chance
            // mods", so it belongs in the RELATIVE bucket beside Pistol
            // Gambit's — multiplying the unmodded base, not the modded one.
            + params.cc_per_tendril * f64::from(tendrils);
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
        let undamaged = (ap.cc_on_undamaged > 0.0 || ap.cd_on_undamaged > 0.0)
            && target_undamaged(&target, &params.target);
        let effective_cc = ap.base_crit_chance
            + flat_crit
            + weakened_cc
            + ap.unmodded_crit_chance * cc_rel
            + if undamaged { ap.cc_on_undamaged } else { 0.0 };

        // Live fire rate (base + Pressurized Magazine's on-reload buff, ×
        // the BuffBar multiplier) — schedules shots below and gates
        // Hemorrhage's below-2.5 doubled chance.
        let fr_reload_add = match ap.fr_on_reload {
            Some(b) if t < fr_reload_expiry => b.value,
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
        let bd_reload_add = match ap.bd_on_reload {
            Some(b) if t < bd_reload_expiry => b.value,
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
                .ms_stack
                .as_ref()
                .map_or(0.0, |s| s.per_stack * gal.ms.current(t, s.duration) as f64)
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
            + buff_total!(ap, crate::loadout::BuffGrant::Multishot, t)
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
                buff_total!(ap, crate::loadout::BuffGrant::BaseMultishot, t)
                    * (ap.multishot / ap.base_multishot.max(1e-9))
                    + buff_total!(ap, crate::loadout::BuffGrant::MultishotPercent, t)
                        * ap.base_multishot
            };
        let rolled = ms_eff.floor() as u32 + d.spine.chance(ms_eff.fract()) as u32;
        // CONTINUOUS weapons MERGE. VERBATIM (wiki Multishot §Continuous
        // Weapons): "additional beams that hit the same target instead merge
        // into a singular damage tick. This combined tick has damage AND Status
        // Chance equal to the SUM of the individual beams, but the Critical
        // Chance is still equal to that of a single beam."
        //
        // The multiplier is the ROLLED count, not the fractional average — the
        // page works the example that way ("When multishot rolls a value of 2,
        // the status chance of that damage instance would be 2 x 40% = 80%").
        //
        // Two consequences the page names and this reproduces for free:
        // damaging status effects are "affected TWICE by multishot" (more procs
        // AND a bigger payload each, since the merged instance's ModifiedBase
        // carries the sum), and forced procs are "applied after the damage
        // instances are merged", so one per tick rather than one per beam.
        //
        // PLENTIFUL MAYHEM, continuous branch. VERBATIM: "In the Incarnon form,
        // instead of increasing the damage of additional projectiles created by
        // multishot, all multishot bonuses are increased by 60%." A merged beam
        // has no separable "generated projectile" to scale, so the perk scales
        // the multishot BONUS instead — and the two readings agree in
        // expectation, which is the tell that this is one perk stated twice:
        //   base form   1 + (1+v)(M-1)     [1 original + (M-1) generated]
        //   Incarnon    1 + (1+v)(M-1)     [merged, so damage ∝ multishot]
        // The identity needs base multishot = 1; both Torid forms are.
        let merge_bonus = if ap.multishot_ammo_bonus > 0.0 {
            let base_ms = ap.base_multishot.max(1.0);
            base_ms + (rolled.max(1) as f64 - base_ms) * (1.0 + ap.multishot_ammo_bonus)
        } else {
            rolled.max(1) as f64
        };
        let (n_pellets, mut beam_merge) = if ap.continuous {
            (1, merge_bonus)
        } else {
            (rolled, 1.0)
        };
        // Ammo, settled now that the roll is known.
        //
        // The ROUND itself always comes from the magazine and always takes ammo
        // efficiency — that path is unchanged by any perk.
        //
        // PLENTIFUL MAYHEM bills the EXTRA projectiles on top, one round each,
        // and the draw follows the RAW rolled count rather than the 60%-scaled
        // one (user, 2026-07-30): the bonus is paid in damage, not billed twice.
        // Ammo efficiency does NOT reach the surcharge — ✅ measured (user,
        // 2026-07-30): the extras take no efficiency at all. So the magazine
        // round keeps its discount while every generated projectile pays full
        // price, and a 100% efficiency source does NOT make multishot free.
        //
        // AMMO STARVATION IS REAL, and it is why this is a loop rather than one
        // subtraction (user, 2026-07-30): the projectiles are produced in order,
        // each paying as it goes, and one that cannot pay simply IS NOT FIRED.
        // So a 4-multishot pull against 3 remaining charges spends the round,
        // fires two extras, and drops the third — three pellets, not four, and
        // the pool lands on empty rather than going negative or silently
        // clamping while all four fly.
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
        // BLAZING BARREL: the round is SPENT, so it was fired. Here and not in
        // the pellet loop — one shot is one stack however many pellets it threw
        // — and after `ms_eff` was rolled, so the shot that earns the stack does
        // not carry it.
        bump_buffs!(crate::loadout::BuffTrigger::Firing, t, d.spine);
        // READY RETALIATION IS ARMED THE MOMENT THE MAGAZINE RUNS OUT, which is
        // HERE — the shot that spends the last round — and not at the reload
        // that follows. The two are the same instant for a reload and are not
        // the same instant for a TRANSFORM: the shot that fills the gauge can
        // also be the shot that empties the magazine, and the transform is
        // decided before any reload is. Arming at the reload would have left
        // that transform at the plain speed, which is the case the owner used
        // to state the rule (2026-08-11).
        if !can_fire(if in_base_form { base_mag } else { magazine }, ap.ammo_cost) {
            rs_armed = true;
        }
        let mut n_pellets = n_pellets;
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
        // Field ticks due before this shot, with the buff state as of now.
        field_ctx = FieldCtx {
            flat_crit,
            cc_rel_mods: cc_rel - params.arcane.cc_rel,
            bd_add_mods: bd_reload_add
                + ap.compression_bd
                + buff_total!(ap, crate::loadout::BuffGrant::BaseDamage, t)
                + buff_total!(ap, crate::loadout::BuffGrant::FlatBaseDamage, t),
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
            d,
        );
        // Secondary Encumber: at most ONE extra proc per instant — pellets
        // of one pull land simultaneously, so one roll per pull.
        let mut encumber_done = false;
        r.shots += 1;
        // ...and the same boundary for a per-instance arcane cap: the whole
        // pull is ONE damage instance, pellets and radial included.
        arc.next_instance();

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
            // Live target-side state for THIS pellet (earlier pellets'
            // procs already count): mitigation amps, Cold's flat crit
            // damage received, and Condition Overload's type count.
            let mit = debuffs.mitigation(t, sd, ap.armor_strip_per_puncture);
            // Crit damage: resolved multiplier + Cold's flat bonus received
            // + Sharpened Bullets' live on-kill buff + the arcane's
            // assumed-max conditional (Outburst).
            // Primary Blight / Frostbite: a stacking crit-damage grant,
            // already resolved to an ABSOLUTE per-stack value against the
            // weapon's base crit damage (ArcaneDef::fx), so it adds straight
            // into the same total as Cold's flat bonus.
            // Same split as crit chance above: `cd_rel` is a bucket bonus every
            // stage scales by its OWN base crit damage; `cd_abs` is a flat add
            // every stage takes as-is.
            let cd_rel = arc.total(&params.arcane.buffs, ArcGrant::CritDamage, t)
                + arc.cd_bonus(ap, t)
                + params.arcane.cd_rel;
            // SPITEFUL DEFILEMENT rides the same after-mods FLAT bucket Cold's
            // received bonus does — "Bonus is added after mods as a flat value"
            // (wiki, supplied by the owner 2026-08-10), so it is added to the
            // finished multiplier rather than scaling the weapon's base.
            //
            // The counter is DISTINCT TYPES, which is what the card's own
            // example insists on ("having 5 corrosive and 5 radiation status
            // effects on a target will not disable this buff") — and it is
            // Condition Overload's counter, read here rather than recomputed,
            // so the anti-CO perk and CO can never disagree about the number
            // they are both reading.
            let spiteful = match params.cd_below_status_count {
                Some((threshold, bonus))
                    if (debuffs.distinct_statuses() as u32) < threshold =>
                {
                    bonus
                }
                _ => 0.0,
            };
            let cd_abs = debuffs.cold_cd_bonus(t) + spiteful;
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
            let prelude_lost = match ap.crit_mult_below_cc {
                Some((granted, below)) if effective_cc >= below => granted,
                _ => 0.0,
            };
            let cd_total = ap.crit_multiplier - prelude_lost
                + ap.unmodded_crit_damage * cd_rel
                + cd_abs
                // Mauler's Magazine, earned inside the fight — a BASE grant,
                // already multiplied by the crit-damage mods at `resolve`, the
                // same conversion `FlatBaseDamage` takes one bracket over.
                + buff_total!(ap, crate::loadout::BuffGrant::BaseCritDamage, t)
                // …and the other half of the same condition, decided by the
                // same shot: an absolute add, already multiplied by the
                // crit-damage mods at `resolve`.
                + if undamaged { ap.cd_on_undamaged } else { 0.0 };
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
            let arc_bd = arc.total(&params.arcane.buffs, ArcGrant::BaseDamage, t)
                + bd_reload_add
                // Primary Compression's `adds` row: the same bracket a live
                // base-damage buff joins, so Serration dilutes it exactly as
                // the wiki's "additive with damage bonuses" says it should.
                + ap.compression_bd
                + buff_total!(ap, crate::loadout::BuffGrant::BaseDamage, t)
                // Striking Succession, already converted by `resolve` into the
                // share of this bucket its flat number is worth — so it lands
                // here and NOT diluted, which is the whole point of the
                // conversion.
                + buff_total!(ap, crate::loadout::BuffGrant::FlatBaseDamage, t);
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
                ap.bd_below_half_health
            } else {
                0.0
            };
            let bd = ap.base_damage_bonus;
            let arc_ratio = (1.0 + bd + arc_bd) / (1.0 + bd);
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
                params, ap, &mut debuffs, &mut gal, t, bd, arc_bd, arc_ratio,
                half_hp,
                ap.co_base_fraction,
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
                    params, ap, &mut debuffs, &mut gal, t, bd, arc_bd, arc_ratio, 0.0,
                    r.co_base_fraction,
                ),
                _ => arc_ratio,
            };

            // Part FIRST, crit roll second: weak-point crit chance (Pistol
            // Acuity; Cascadia Accuracy under assumed-max) exists only on
            // the pellet that actually lands on a weak point.
            //
            // The landing spot is rolled PER PELLET, not per trigger pull
            // (user, 2026-07-29): aiming at the head does not put every
            // pellet of a spread on it, so `headshot_pct` is a per-pellet
            // aim weight. Consequences that follow from this and are
            // deliberate: the Incarnon gauge charges per headshot PELLET
            // (multishot fills it faster), on-headshot buffs trigger from
            // any one pellet, and the reported headshot rate is
            // pellets/pellets. Do NOT "fix" this into a per-pull roll.
            let part = pick_part(&params.body_parts, &mut d.spine);
            let cc_pellet = effective_cc
                + if part.is_head {
                    // Weak-point-only crit chance is relative too, and it is
                    // DIRECT-only, so the direct part's base is the right one.
                    ap.unmodded_crit_chance
                        * (ap.weakpoint_cc_rel + params.arcane.weakpoint_cc_rel)
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
                if part.is_head { cc_pellet } else { cc_pellet * ap.bodyshot_cc_mult };
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
            // not in it. An innate headshot bonus normally joins the bracket
            // (the wiki lists Kuva Chakkhurr among the additive sources), but
            // "Cernos Prime's headshot bonus is unique and stacks
            // MULTIPLICATIVELY with Primary Deadhead's headshot bonus" — a
            // per-weapon anomaly, carried on the weapon rather than inferred
            // from anything about it (2026-08-01). On a 3x head with Primary
            // Deadhead that is 3 x 1.3 x 1.5 = 5.85x, against 5.4x additive.
            // LINGERING JUDGEMENT's open window, in the ADDITIVE bracket:
            // "Headshot damage bonus stacks additively with Primary Deadhead's
            // headshot damage bonus" (wiki, owner 2026-08-10). So it sums with
            // the arcane's before the bracket is spent — never multiplies it —
            // which is why a Deadhead build gets much less than +50% out of it.
            //
            // It sits with the ARCANE's term rather than the weapon's in both
            // branches, because that is the group the card names. The
            // multiplicative branch is Cernos Prime's anomaly and no weapon
            // carries both.
            let streak_bonus = match params.headshot_streak {
                Some(s) if t < streak_expiry => s.value,
                _ => 0.0,
            };
            let (head_bonus, head_innate) = if part.is_head {
                if ap.headshot_bonus_multiplicative {
                    (params.arcane.headshot_mult_bonus + streak_bonus, ap.headshot_damage_bonus)
                } else {
                    (
                        params.arcane.headshot_mult_bonus + streak_bonus + ap.headshot_damage_bonus,
                        0.0,
                    )
                }
            } else {
                (0.0, 0.0)
            };
            let wp_mult = if part.is_head {
                part.multiplier + 1.5 * ap.weakpoint_damage
            } else {
                part.multiplier
            };
            let part_factor = wp_mult * (1.0 + head_bonus) * (1.0 + head_innate);
            // Wiki Critical_Hit §Critical Headshots: a crit on an eligible
            // >1x location doubles cd inside the tier formula (a cd_total
            // that INCLUDES Cold's flat bonus — freeze.yaml notes).
            let cd = if part.crit_bonus && part.multiplier > 1.0 {
                2.0 * cd_total
            } else {
                cd_total
            };
            let crit_mult = 1.0 + tier as f64 * (cd - 1.0);

            // Faction bonus (System A) is a total-damage multiplier applied
            // once per instance; DoT/status ticks apply it a SECOND time
            // (fm² below) — the wiki "double dip".
            // Secondary Surge (assumed-max): a FINAL multiplier on the shot,
            // multiplicative with Hornet Strike (wiki notes). Secondary
            // Fortifier: ×overguard_mult while the target's Overguard holds.
            // Primary Compression's `multiplies` row rides the same slot, and
            // it is `ap`'s rather than `params`' — the form being fired owns
            // it, because one arcane is worth +240% in the Torid's base form
            // and nothing in its Incarnon.
            let arc_final = params.arcane.final_mult
                * ap.compression_mult
                * if target.overguard > 0.0 {
                    params.arcane.overguard_mult
                } else {
                    1.0
                };

            // ---- ATTACK PARTS ----------------------------------------
            // A projectile can carry TWO damage instances: the direct hit
            // and, when the weapon declares one, the radial explosion. They
            // are resolved SEPARATELY because they ARE separate damage —
            // wiki (Laetum): "Initial hit and explosion apply status
            // separately". Each rolls its own crit and its own status draw
            // from its OWN damage vector.
            //
            // The per-stage bindings SHADOW the direct-hit names, so the
            // proc-application block further down serves whichever instance
            // is in flight without knowing which it is.
            //
            // What the radial stage does differently (MECHANICS §7):
            //   · no body-part multiplier — an explosion never headshots,
            //     so it also never charges a weakpoint gauge and never
            //     feeds headshot-gated buffs;
            //   · no Condition Overload — CO is direct-damage only. It
            //     still takes the arcane's live base-damage bucket, which
            //     shares `co_mult`'s bracket on the direct side;
            //   · no forced procs — those are declared per attack part
            //     (Astilla: the direct hit forces Impact, the radial does
            //     not);
            //   · falloff is 1.0 here: the projectile detonates ON the
            //     target, so the epicentre distance is zero.
            // ...and ONCE PER PULL rather than per pellet where the weapon
            // says so. A radial normally rides its projectile, so two
            // projectiles detonate twice; the Burston's Incarnon is the
            // exception the wiki states outright ("The Radial Attack does not
            // benefit from Multishot bonuses"), and firing it on the first
            // pellet only is exactly that.
            // FROM THE ACTIVE FORM, for the same reason as `co_mult_radial`
            // above — this is the line that fired it (M32).
            let radial_stage = match ap.radial {
                Some(r) if !r.takes_multishot && pellet_idx > 0 => None,
                other => other,
            };
            for stage in 0..(1 + radial_stage.is_some() as usize) {
                let rad = if stage == 1 { radial_stage } else { None };
                let direct = rad.is_none();
                // Instance values — the shadowing happens here. The explosion
                // rolls its own crit tier off its own crit chance, and the live
                // crit buffs reach it: the relative ones scale ITS base, the
                // absolute ones add flat. Under AssumedMax those same bonuses
                // arrive through the mod bucket in `r.crit_chance`, so this is
                // what makes the two policies agree about one mod.
                let (qvec, tier) = match &rad {
                    None => (*qvec, tier),
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
                        let rcc = r.crit_chance + flat_crit + r.base_crit_chance * cc_rel;
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
                        (r.damage.quantized(), t2)
                    }
                };
                // WARFRAME ABILITY ELEMENTS, added to the FINISHED vector.
                //
                // Not through the elemental hierarchy — "does not combine with
                // other elements" is stated on every one of the four augment
                // pages, and it is the whole reason they are worth having
                // separately from a mod (owner, 2026-08-08: "注意不合成"). Sized
                // off THIS stage's own ModifiedBase, because "additive with
                // elemental mods" makes them a percentage of the part's base
                // the same way an elemental mod is (MECHANICS §7).
                //
                // Read at `t`: they expire, and after they do the weapon is
                // simply the weapon again.
                let stage_mb = match &rad {
                    None => modded_base,
                    Some(r) => r.modified_base,
                };
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
                let crit_mult = match &rad {
                    None => crit_mult,
                    Some(r) => {
                        // No `part.crit_bonus` doubling: that is the crit-
                        // HEADSHOT rule and an explosion has no hit location.
                        let rcd = r.crit_damage + r.base_crit_damage * cd_rel + cd_abs;
                        1.0 + tier as f64 * (rcd - 1.0)
                    }
                };
                let part_factor = if direct { part_factor } else { 1.0 };
                // ModifiedBase carries the merge too, which is what makes
                // damaging status effects "affected TWICE by multishot": more
                // procs from the summed status chance, and a bigger payload
                // each because the instance itself is bigger.
                let mb_live = beam_merge
                    * match &rad {
                        None => mb_live,
                        Some(r) => r.modified_base * arc_ratio,
                    };
                // The direct hit always carries CO. An explosion does NOT by
                // default — the mods say direct hits only — but the engine
                // supports the case the mods forbid, because some entries do it
                // anyway and the CO catalog lists them one at a time: the
                // Zylok's Incarnon radial has a row reading "Radial hit only
                // receives CO bonus on target DIRECTLY HIT by bullet", which
                // the single-target arena always is. Per-entry weapon data, so
                // no roster weapon is affected until one declares it.
                let bucket = match &rad {
                    None => co_mult,
                    Some(r) if r.takes_condition_overload => co_mult_radial,
                    Some(_) => arc_ratio,
                };
                // Primary Crux's stacks join the status-chance BUCKET (wiki:
                // "additive to mods like Rifle Aptitude"), so the relative
                // bonus multiplies THIS part's own unmodded base — the
                // explosion's differs from the direct hit's.
                // ...and SENTIENT SURGE's status half rides the same bucket
                // for the same reason ("Additive to other ... status chance
                // mods"). Summed with the arcane's before either is spent, so
                // the two cannot end up multiplying each other.
                let sc_arc = arc.total(&params.arcane.buffs, ArcGrant::StatusChance, t)
                    + params.sc_per_tendril * f64::from(tendrils);
                let status_chance = beam_merge
                    * match &rad {
                        None => ap.status_chance + ap.base_status_chance * sc_arc,
                        Some(r) => r.status_chance + r.base_status_chance * sc_arc,
                    };
                const NO_FORCED: &[DamageType] = &[];
                let forced: &[DamageType] = if direct { &ap.forced_procs } else { NO_FORCED };

                // Devouring Attrition: an INDEPENDENT multiplier rolled per
                // INSTANCE that did not crit (wiki: "multiplicative to base
                // damage bonuses such as Hornet Strike"; "affects both
                // forms", the explosions included).
                let attrition = noncrit_mult(ap.noncrit_bonus, tier, &mut d.spine);
                let raw = qtotal
                    * part_factor
                    * crit_mult
                    * bucket
                    * params.faction_at_time(t)
                    * arc_final
                    * attrition
                    // ECLIPSE: "an unique multiplier", so it stands beside the
                    // others rather than joining any of them.
                    * params.ability_final_at(t)
                    * beam_ramp
                    * pm_mult;
                let head_direct = direct && part.is_head;
                let col = target.incoming_column(&params.target);
                let (effective, killed, broke) = target.apply(
                    raw,
                    shares,
                    head_direct,
                    t,
                    &params.target,
                    false,
                    &mit,
                );
                r.total_damage += raw;
                r.effective_damage += effective;
                // WHAT KIND OF HIT THAT WAS. Sorted rather than averaged: a
                // number that cannot happen stands out in a histogram and
                // disappears in a mean. Tier is capped at 2 because that is
                // where the multiplier stops naming itself — red and above
                // share a bucket.
                let bucket_row = usize::from(part.is_head);
                let bucket_col = (tier as usize).min(2);
                r.hit_count[bucket_row][bucket_col] += 1;
                r.hit_damage[bucket_row][bucket_col] += effective;
                if effective > r.max_hit {
                    r.max_hit = effective;
                }
                // THE ACCOUNT OF THIS HIT, taken once per attack part and only
                // while a replay is being traced. Written HERE because this is
                // the one place every factor exists at the same time — anywhere
                // else and the list would be reconstructed, which is how a
                // breakdown comes to disagree with the number it explains.
                if let Some(rep) = trace.as_deref_mut() {
                    let kind = if direct { "direct" } else { "radial" };
                    if !rep.accounts.iter().any(|a| a.source == kind) {
                        rep.accounts.push(HitAccount {
                            source: kind,
                            part: part.name.clone(),
                            head: part.is_head,
                            tier,
                            t,
                            base: qtotal,
                            steps: vec![
                                ("body part", part_factor),
                                ("critical", crit_mult),

                                ("Condition Overload bracket", bucket),
                                ("faction", params.faction_at_time(t)),
                                ("arcane (final)", arc_final),
                                ("attrition", attrition),
                                ("Warframe ability", params.ability_final_at(t)),
                                ("beam ramp", beam_ramp),
                                ("multishot-generated", pm_mult),
                            ],
                            raw,
                            effective,
                        });
                    }
                }
                if direct {
                    r.sources.direct += effective;
                    add_by_type(&mut r.sources.direct_by_type, &qvec, effective, &col);
                } else {
                    r.sources.radial += effective;
                    add_by_type(&mut r.sources.radial_by_type, &qvec, effective, &col);
                }
                r.timeline.add(t, effective);
                r.note_kills(killed as u32, t);
                // EXECUTIONER'S FORTUNE. Rolled HERE and nowhere else, because
                // this is the only place that knows both halves of its
                // condition: `head_direct` says the pellet landed in a head,
                // `killed` says it finished the target. An explosion never
                // headshots, so a radial pellet cannot pay.
                //
                // PER PELLET, like every other on-hit roll in this loop — a
                // multishot pull that puts two pellets in a head gets two
                // chances, which is the same rule the wiki states from the
                // other side for charge gauges ("additional shots from
                // Multishot count as separate weakpoint hits").
                //
                // AND IT DOES NOT ROLL AT ALL IN AN INCARNON FORM. VERBATIM
                // (wiki, this perk): "Does not affect Incarnon Form" — because
                // what it refills is a MAGAZINE, and an Incarnon form has max
                // CHARGES instead (owner, 2026-08-10). The two are different
                // pools: the gauge is converted from weakpoint hits, sits
                // outside the ammo economy, and has no reload to make instant.
                //
                // The test is at the TRIGGER rather than at the refill so the
                // roll is not even taken — a roll that can never be spent still
                // draws from `extra`, and a perk that changes a fight it cannot
                // affect is worse than one that does nothing.
                // LINGERING JUDGEMENT's streak, counted at the same site and
                // for the same reason: `head_direct` is the only place a
                // headshot is known to have LANDED. Per pellet, so a multishot
                // pull can arm it on its own.
                //
                // THE ARMING HIT DOES NOT BENEFIT: its damage was settled
                // above, and the window opens here. That is the ordinary
                // reading of "on 2 headshots: +50% for 8 seconds" and it is
                // also the only one this loop can express without pricing a
                // hit twice.
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
                        part.duration_s *= boost;
                        let fresh = FieldState {
                            next_tick: t,
                            ticks_left: (part.duration_s * part.tick_rate).round() as u32,
                            part,
                            // Plentiful Mayhem follows a GENERATED grenade into
                            // the cloud it leaves (user, 2026-07-30) — which is
                            // the whole value of the perk here, the cloud being
                            // most of this weapon's damage.
                            damage_mult: pm_mult,
                        };
                        match fp.stacking {
                            crate::loadout::FieldStacking::Stack => fields.push(fresh),
                            crate::loadout::FieldStacking::Refresh => {
                                fields.clear();
                                fields.push(fresh);
                            }
                        }
                    }
                }
                if direct {
                    r.pellets += 1;
                    r.crits += (tier >= 1) as u32;
                    r.big_crits += (tier >= 2) as u32;
                    r.crit_tier_sum += tier;
                    r.headshots += part.is_head as u32;
                    any_head |= part.is_head;
                    any_big |= tier >= 2;

                    // Crosshairs' on-HEADSHOT buff refreshes on every head
                    // hit (kills only matter for its stacks).
                    if part.is_head {
                        if let Some(b) = params.cc_on_headshot {
                            ch_buff_expiry = t + b.duration;
                        }
                        // Lethal Rearmament: every headshot grants a stack —
                        // a LOCKED buff earns it too, it just never loses it.
                        // EVERY buff that triggers on a headshot, including
                        // its own chance roll. One line for the family.
                        bump_buffs!(crate::loadout::BuffTrigger::Headshot, t, d.extra);
                        // Primary Crux: a weak-point HIT (not a kill), per
                        // PELLET. Bumped here, AFTER this pellet's status
                        // chance was read above — the hit that grants a stack
                        // does not benefit from it, the same rule the
                        // base-damage stacks follow. A killing headshot still
                        // counts: this runs before the kill path's `continue`.
                        arc.bump_trigger(&params.arcane.buffs, ArcTrigger::WeakpointHit, t);
                        bump_buffs!(crate::loadout::BuffTrigger::ConsecutiveHeadshot, t, d.extra);
                    } else {
                        // …AND A BODY HIT TAKES THE PILE. The only trigger in
                        // this sim that the next shot can undo, and the reason
                        // it is not `Headshot` with a clock: what ends it is
                        // what you hit, not how long you waited.
                        for (i, b) in params.stacking_buffs.iter().enumerate() {
                            if b.trigger == crate::loadout::BuffTrigger::ConsecutiveHeadshot {
                                buff_stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
                            }
                        }
                    }
                }

                if let Some(pool) = broke {
                    push_break_proc(&mut debuffs, params, t, pool);
                }
                if killed {
                    gal.bump_on_kill(params, t);
                    arc.on_kill(params, t);
                    if head_direct {
                        // Deadhead's precision boundary: only direct-pellet
                        // HEADSHOT kills grant/refresh its stacks.
                        arc.bump_trigger(&params.arcane.buffs, ArcTrigger::HeadshotKill, t);
                        // Crosshairs stacks: headshot kills, per-stack FIFO.
                        if let Some(s) = &params.cc_stack {
                            DebuffState::push_capped(
                                &mut ch_stacks,
                                t + s.duration,
                                s.max_stacks as usize,
                                t,
                            );
                        }
                    }
                    // The killing instance's procs die with the old
                    // individual, and so do the clouds stuck to it; what
                    // follows hits the fresh spawn.
                    debuffs = DebuffState::default();
                    fields.clear();
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
                    &mut d.status,
                ) {
                    fields.clear();
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
                // Pushing it HERE, rather than applying a bleed directly, is
                // what makes its damage right for free: a Slash proc is
                // 0.35 x ModdedBase x THE PROCCING HIT's crit/part
                // multipliers, so the tier this pellet rolled (including a
                // Vigilante promotion) and the body part it struck already
                // scale it — "Headshots, orange and red Critical Hits will
                // greatly increase the damage dealt".
                //
                // It STACKS with a Slash the weapon's own status chance
                // applied — the wiki says so outright, "but can stack with
                // Slash statuses applied using a weapon's innate status
                // chance" — so this pushes a second bleed and does not check
                // `procs`. What it cannot double up with is a FORCED Slash:
                // "cannot produce multiple procs in a single instance of
                // damage alongside forced Slash from sources such as Internal
                // Bleeding or the debuff from Seeking Talons". Internal
                // Bleeding is handled below by its own guard, which sees this
                // push because it runs after; a weapon-forced Slash has to be
                // checked here, at its source, since `procs` cannot say which
                // Slash came from where.
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
                            pc.low_rate_mult
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
            bump_buffs!(crate::loadout::BuffTrigger::Hit, t, d.extra);
            if tier == 0 && procs.is_empty() {
                bump_buffs!(crate::loadout::BuffTrigger::PlainHit, t, d.extra);
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
                bump_buffs!(crate::loadout::BuffTrigger::StatusApplied, t, d.extra);
            }
            settle_procs(
                procs,
                t,
                // THE HIT'S ATTRITION ROLL TRAVELS WITH ITS STATUSES. A proc's
                // magnitude is the applying instance's — which is why
                // `crit_mult` is already here — and Devouring/Devastating
                // Attrition is a per-instance multiplier of exactly that shape,
                // so a DoT applied by a 21x hit ticks for 21x.
                //
                // Measured through the Debilitate chain (owner, 2026-08-08): the
                // final DoT eats "441倍强袭损耗", i.e. 21x21. Two layers for
                // three faction layers — the split instance rolls one, and the
                // other can only be the applying hit's, carried here. A DoT is
                // not a hit, so it never rolls one of its own. MEASUREMENTS M37.
                InstanceScale {
                    mb_live,
                    crit_mult,
                    part_factor,
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
                &mut d.status,
                DEPTH_PROC,
            );
            }
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
            bump_buffs!(crate::loadout::BuffTrigger::FullBurst, t, rng);
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
        // watches reloads sees it, the same rule `mag_refill_on_kill` follows.
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
        if let Some(en) = enervate.as_mut() {
            en.on_event(&hit, t, &mut bar);
        }
        if ap.frenzy {
            frenzy.on_event(&hit, t, &mut bar);
        }

        // Gauge charging (base phase): every weakpoint PELLET builds one
        // charge (charge_rules); a full gauge transmutes back immediately.
        if let Some(cy) = &params.cycle {
            if in_base_form {
                // Per PELLET, and per the WEAPON's rule: weak-point hits for
                // the Zariman pistols, any direct hit for the Torid. A field
                // or radial instance is neither, so neither can charge it.
                charges += match cy.charge_on {
                    crate::loadout::ChargeOn::WeakpointHits => r.headshots - headshots_before,
                    crate::loadout::ChargeOn::DirectHits => r.pellets - pellets_before,
                };
                // A FULL GAUGE ARMS THE TRANSFORM; the cadence below still
                // runs. This block used to `continue`, which skipped the
                // completing shot's OWN interval and let the next shot fire at
                // the same instant — the transform was a free extra shot
                // (2026-08-10). The moment is the end of the shot that filled
                // the gauge, which is the start of the next one (owner: "变身的
                // 时机应该是在完成之后射击的末尾（也就是下次射击的开头）").
                //
                // The gauge also OVERSHOOTS and that is not a rounding: a
                // 7-pellet shot into a 30-charge gauge arrives at 35 on the
                // fifth shot, never at 30, so the comparison is `>=` and the
                // shot that crosses it is fired in the BASE form.
                if charges >= cy.charges_to_fill {
                    // BOTH DIRECTIONS TAKE IT. The wiki says Ready Retaliation
                    // "can affect transition INTO Incarnon form with a
                    // well-timed manual reload" and not the way back; the
                    // second half is wrong (owner, 2026-08-10) and there is
                    // nothing here that could tell the two animations apart.
                    // TRANSFORMING WITH AN EMPTY MAGAZINE IS THE PROOF: the
                    // animation is faster, so the buff was already there before
                    // any reload began (owner, 2026-08-11).
                    //
                    // AND IT IS SPENT WHEN THE TRANSFORM COMPLETES (owner, same
                    // day: "那个buff应该在进入灵化完成的时候就消失，如果是在打
                    // 空的时候进入灵化的话"). Which collapses the rule to one
                    // line rather than a list of events: SWAPPING EITHER WAY
                    // FULLY RELOADS THE BASE FORM'S MAGAZINE (wiki), so both
                    // transforms are reloads, and the buff is spent by whatever
                    // refills the magazine. Nothing else has to be enumerated.
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
                    r.downtime_secs += spent;
                    t += spent;
                    magazine_refilled!();
                    if transformed_from_empty {
                        bump_on_trigger!(
                            crate::loadout::BuffTrigger::ReloadFromEmpty, t, d.spine);
                    }
                    r.transforms += 1;
                    in_base_form = false;
                    // The CHARGE magazine is filled by the gauge, not reloaded
                    // from reserve — it is outside the ammo economy, takes no
                    // efficiency, and so is always whole anyway.
                    magazine = mag_cap;
                    // The base magazine's refill IS a reload (user,
                    // 2026-07-30): whole rounds off whatever is already in it,
                    // and out of the same reserve as every other reload. This
                    // was the site that kept the base magazine topped up for
                    // free — with all three draws inside the cycle unbilled, a
                    // finite reserve never moved off its starting value.
                    let loaded = draw_from(&mut reserve, params.infinite_reserve,
                        reload_draw(cy.base_form.magazine_size, base_mag));
                    base_mag += loaded;
                    // …AND THAT RELOAD PAYS ITS SHELLS. One as you go in, the
                    // rest owed until you come out (owner, 2026-08-08: "假如我
                    // 现在的shell是13，进入的时候是10/13，那么进入的时候会叠加
                    // 1层，退出的时候会加上其余的层数（这里是2）。如果是13/13
                    // 进入的，进入退出都不会叠层").
                    //
                    // Counting the shells the draw ACTUALLY loaded is what
                    // makes a dry reserve behave: no shells, no stacks, and no
                    // separate rule needed to say so.
                    let shells = loaded.round().max(0.0) as u32;
                    if shells > 0 {
                        bump_shells!(1, t, rng);
                        owed_shells = shells - 1;
                    }
                                                           // Frenzy persists across the transform (user-confirmed
                                                           // 2026-07-24: it exists in both forms).
                }
            }
        }

        // Next shot: cadence reflects the bar as of now (Frenzy just
        // granted/refreshed counts immediately), plus Pressurized
        // Magazine's live on-reload fire-rate buff.
        bar.expire(t);
        let mut fr_add = match ap.fr_on_reload {
            Some(b) if t < fr_reload_expiry => b.value,
            _ => 0.0,
        };
        // THE SAME BUCKET fire-rate mods and a static `fire_rate_bonus`
        // evolution live in — `base * (1 + fr + evo + 0.05n)`. `per_stack` is
        // already the absolute rate that fraction is worth, so adding it here,
        // inside the bracket rather than outside it, is what keeps it additive
        // with mods instead of multiplicative with them.
        fr_add += buff_total!(ap, crate::loadout::BuffGrant::FireRate, t);
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
                    crate::weapons_data::ChargeCadence::DrawOnly => draw,
                    // Everything else pays the draw AND the listed rate's
                    // interval: "1 / (Modded Charge Time + 1 / Modded Fire
                    // Rate)". The rate is what happens after the charge.
                    crate::weapons_data::ChargeCadence::DrawThenRate => draw + 1.0 / rate,
                }
            }
            // A BURST pull fires `count` rounds and then waits. The listed
            // rate is BURSTS per second, so the cycle is the wait plus the
            // rounds' own spacing, and one round costs a `count`-th of it:
            //
            //   Effective Fire Rate = Burst Count / [1/Fire Rate + (Burst
            //   Count−1)·Burst Delay]                       (wiki, verbatim)
            //
            // `b.delay_seconds` arrives already shortened by the mod layer
            // (loadout::resolve), which is where the wiki's net-negative
            // exception lives. The LIVE buff factor is applied here, the same
            // reciprocal trick the draw above uses: `rate / ap.fire_rate` is
            // exactly what the live buffs did, 1.0 when none are up. It is
            // clamped the same way, so a live fire-rate PENALTY does not
            // stretch the burst either.
            None => match ap.burst {
                Some(b) if b.count > 1 => {
                    let live = (rate / ap.fire_rate.max(1e-9)).max(1.0);
                    let cycle = 1.0 / rate + f64::from(b.count - 1) * b.delay_seconds / live;
                    cycle / f64::from(b.count)
                }
                _ => 1.0 / rate,
            },
        };
        spool_due = t;
    }

    // The clouds still burning after the last shot, with the buff snapshot from
    // that shot (nothing refreshes it once firing stops). FIRST, because each
    // tick settles the status events before it and pushes procs of its own…
    process_field_ticks(
        &mut fields,
        &mut debuffs,
        &mut gal,
        &mut arc,
        params.duration_secs,
        &mut target,
        params,
        field_ap,
        &field_ctx,
        &mut r,
        d,
    );
    // …then drain what is left up to the end of the engagement.
    process_ticks(
        &mut debuffs,
        &mut gal,
        &mut arc,
        params.duration_secs,
        &mut target,
        params,
        field_ap,
        &mut r,
        &mut d.status,
    );

    // Partial credit: the fraction of the current individual's TOTAL bar
    // already depleted — overguard + shield + health (user 2026-07-25: the
    // whole bar counts, so shield damage earns progress and shield REGEN
    // gives it back). If health has hit 0 the unit is DEAD, so the whole bar
    // is gone regardless of any overguard/shield left (e.g. a Toxin-bypass
    // kill that never broke the shield) — full credit (user 2026-07-25).
    // InfiniteHealth pools never deplete -> 0.
    let pool = params.target.overguard() + params.target.max_shield() + params.target.max_health();
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

/// Replay ONE engagement, sampled into frames.
///
/// Deliberately NOT part of [`Summary`]: a summary is `Copy` and is produced
/// by the optimizer thousands of times a second, which must not pay for a
/// trace it never looks at. The simulator asks for this separately, with the
/// median run's `rng_state`, and gets the same fight back frame by frame.
///
/// ```ignore
/// let s = monte_carlo(&params, 100, seed);
/// let rep = replay(&params, s.median_run.rng_state, REPLAY_FRAMES);
/// ```
pub fn replay(params: &DummyParams, rng_state: u64, frames: usize) -> Replay {
    let frames = frames.max(1);
    let mut rep = Replay {
        dt: (params.duration_secs / frames as f64).max(1e-6),
        buffs: params.buff_roster(),
        frames: Vec::with_capacity(frames),
        accounts: Vec::new(),
    };
    run_once_traced(params, &mut Rng::new(rng_state), Some(&mut rep));
    rep
}

/// Aggregate statistics over many engagements.
#[derive(Debug, Clone, Copy)]
pub struct Summary {
    pub runs: u32,
    pub duration_secs: f64,
    pub mean_damage: f64,
    pub dps: f64,
    pub std_damage: f64,
    pub min_damage: f64,
    pub max_damage: f64,
    pub mean_effective_damage: f64,
    /// Per-run σ of THAT number, the same way `std_kill_progress` is the σ of
    /// the statistic beside it. Effective damage is what the DPS metric is
    /// built from, and DPS is what a caller falls back to when the build
    /// cannot kill the target at all — precisely the fight where nobody can
    /// eyeball the spread, so it has to be reported.
    pub std_effective_damage: f64,
    pub effective_dps: f64,
    pub mean_dot_damage: f64,
    pub mean_procs: f64,
    /// Mean lingering-FIELD ticks that landed (Torid's cloud).
    pub mean_field_ticks: f64,
    pub mean_reloads: f64,
    pub mean_transforms: f64,
    pub mean_kills: f64,
    pub std_kills: f64,
    pub min_kills: u32,
    pub max_kills: u32,
    /// Mean kill score with partial credit (kills + depleted fraction of
    /// the final target's pool).
    pub mean_kill_progress: f64,
    /// Per-run σ of THAT number. The optimizer ranks by `mean_kill_progress`,
    /// so every statistical decision it makes — the amnesty band at a cut
    /// line, the 3σ racing cull, "is this build actually better" — needs the
    /// spread of the statistic being ranked. It used to reach for `std_kills`,
    /// which is a different statistic (whole kills, no partial credit) that
    /// merely looks like it: a build that never finishes its second kill has
    /// `std_kills` 0 and a kill progress that moves all run long.
    pub std_kill_progress: f64,
    pub mean_shots: f64,
    pub mean_pellets: f64,
    pub mean_crit_rate: f64,
    pub mean_big_crit_rate: f64,
    /// Mean crit TIER over every direct pellet: 0 = a normal hit, 1 yellow,
    /// 2 orange, 3 red, and ABOVE red keeps going — the game shows those and
    /// so must we. Equal to `mean_crit_rate` below 100% crit chance and the
    /// only one of the two that still moves above it.
    pub mean_crit_tier: f64,
    pub mean_headshot_rate: f64,
    /// WHAT A ROOM-CLEAR IS PACED BY, as opposed to what the card says.
    ///
    /// `dps` is the whole engagement including the seconds the weapon was
    /// reloading or mid transform; this is the same damage over the time it was
    /// actually firing. A weapon that reloads for a third of the fight has two
    /// very different numbers and only one of them is on any card.
    pub burst_dps: f64,
    /// Seconds a run spent not firing, averaged.
    pub mean_downtime: f64,
    /// TIME TO THE FIRST KILL — mean, median and the 90th percentile over the
    /// runs that killed anything, and how many did. A mean alone would read as
    /// a promise; the spread is what says whether it is one.
    pub ttk_mean: f64,
    pub ttk_median: f64,
    pub ttk_p90: f64,
    pub ttk_runs: u32,
    /// Effective damage dealt before the first reload started: the opening
    /// window, which is what decides whether a room dies before it reacts.
    pub mean_first_magazine: f64,
    /// The biggest single damage instance any run produced — the number people
    /// chase — and the average of each run's own biggest.
    pub max_hit: f64,
    pub mean_max_hit: f64,
    /// Damage per trigger pull, per multishot instance, and per round of ammo
    /// spent. The last one is the ammo-economy number: on a weapon with a
    /// finite reserve it is the whole magazine's worth of a mod.
    pub damage_per_shot: f64,
    pub damage_per_pellet: f64,
    /// EVERY HIT SORTED BY WHAT IT WAS: `[head][tier]`, summed over every run.
    /// A number that cannot happen stands out here and vanishes in a mean.
    pub hit_count: [[u32; 3]; 2],
    pub hit_damage: [[f64; 3]; 2],
    /// Mean effective damage by source (the damage-meter view).
    pub source_damage: SourceDamage,
    /// The complete MEDIAN engagement (by total effective damage). The
    /// sim result DISPLAYS this run's numbers — kills, shots, procs,
    /// sources, timeline — so every shown stat is one internally
    /// consistent engagement (user, 2026-07-29); the mean fields above
    /// stay for the optimizer's objectives and the golden tests.
    pub median_run: RunResult,
}

/// Run `runs` engagements from a single seed and summarize.
pub fn monte_carlo(params: &DummyParams, runs: u32, seed: u64) -> Summary {
    let mut rng = Rng::new(seed);
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let (mut shots, mut pellets, mut crits, mut big_crits, mut headshots) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut crit_tier_sum = 0u64;
    let (mut effective, mut kills, mut kills_sq) = (0.0f64, 0u64, 0u64);
    let (mut dot, mut procs, mut reloads) = (0.0f64, 0u64, 0u64);
    let mut field_ticks = 0u64;
    let mut transforms = 0u64;
    let (mut min_kills, mut max_kills) = (u32::MAX, 0u32);
    let (mut kill_progress, mut kill_progress_sq) = (0.0f64, 0.0f64);
    let mut effective_sq = 0.0f64;
    let mut sources = SourceDamage::default();
    // Keep every engagement: the MEDIAN one (by effective damage) is
    // what the sim result displays — one real, internally consistent
    // engagement, not a smoothed cross-run average (user, 2026-07-29).
    let mut all_runs: Vec<RunResult> = Vec::with_capacity(runs as usize);
    // The speedrun set: time not firing, first kills, the opening magazine, the
    // biggest instance, and the hit histogram.
    let (mut downtime, mut first_mag, mut max_hit_sum, mut biggest) = (0.0, 0.0, 0.0, 0.0f64);
    let mut ttks: Vec<f64> = Vec::new();
    let mut hit_count = [[0u32; 3]; 2];
    let mut hit_damage = [[0.0f64; 3]; 2];

    for _ in 0..runs {
        let r = run_once(params, &mut rng);
        all_runs.push(r);
        sum += r.total_damage;
        sum_sq += r.total_damage * r.total_damage;
        min = min.min(r.total_damage);
        max = max.max(r.total_damage);
        effective += r.effective_damage;
        effective_sq += r.effective_damage * r.effective_damage;
        dot += r.dot_damage;
        procs += r.procs as u64;
        field_ticks += r.field_ticks as u64;
        reloads += r.reloads as u64;
        transforms += r.transforms as u64;
        kills += r.kills as u64;
        kills_sq += (r.kills as u64) * (r.kills as u64);
        kill_progress += r.kill_progress;
        kill_progress_sq += r.kill_progress * r.kill_progress;
        min_kills = min_kills.min(r.kills);
        max_kills = max_kills.max(r.kills);
        downtime += r.downtime_secs;
        first_mag += r.first_magazine_damage;
        max_hit_sum += r.max_hit;
        biggest = biggest.max(r.max_hit);
        if let Some(at) = r.first_kill_at {
            ttks.push(at);
        }
        for row in 0..2 {
            for col in 0..3 {
                hit_count[row][col] += r.hit_count[row][col];
                hit_damage[row][col] += r.hit_damage[row][col];
            }
        }
        shots += r.shots as u64;
        pellets += r.pellets as u64;
        crits += r.crits as u64;
        big_crits += r.big_crits as u64;
        crit_tier_sum += r.crit_tier_sum as u64;
        headshots += r.headshots as u64;
        sources.direct += r.sources.direct;
        sources.radial += r.sources.radial;
        sources.field += r.sources.field;
        sources.arcane_on_status += r.sources.arcane_on_status;
        sources.syndicate += r.sources.syndicate;
        for (acc, v) in sources.status.iter_mut().zip(r.sources.status) {
            *acc += v;
        }
        for (acc, v) in [
            (&mut sources.direct_by_type, r.sources.direct_by_type),
            (&mut sources.radial_by_type, r.sources.radial_by_type),
            (&mut sources.field_by_type, r.sources.field_by_type),
            (&mut sources.arcane_by_type, r.sources.arcane_by_type),
        ] {
            for (a, x) in acc.iter_mut().zip(v) {
                *a += x;
            }
        }
    }

    let n = runs.max(1) as f64;
    let mean = sum / n;
    let variance = (sum_sq / n - mean * mean).max(0.0);
    let total_pellets = pellets.max(1) as f64;
    let total_shots = shots;
    // A PERCENTILE OVER WHAT ACTUALLY HAPPENED. Sorted here rather than by the
    // caller: an unsorted percentile is a number that looks right.
    ttks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pct = |v: &[f64], q: f64| -> f64 {
        if v.is_empty() {
            return 0.0;
        }
        let i = ((v.len() as f64 - 1.0) * q).round() as usize;
        v[i.min(v.len() - 1)]
    };

    Summary {
        runs,
        duration_secs: params.duration_secs,
        mean_damage: mean,
        dps: mean / params.duration_secs,
        std_damage: variance.sqrt(),
        min_damage: if min.is_finite() { min } else { 0.0 },
        max_damage: if max.is_finite() { max } else { 0.0 },
        mean_effective_damage: effective / n,
        std_effective_damage: {
            let mean_e = effective / n;
            (effective_sq / n - mean_e * mean_e).max(0.0).sqrt()
        },
        effective_dps: effective / n / params.duration_secs,
        mean_dot_damage: dot / n,
        mean_procs: procs as f64 / n,
        mean_field_ticks: field_ticks as f64 / n,
        mean_reloads: reloads as f64 / n,
        mean_transforms: transforms as f64 / n,
        mean_kills: kills as f64 / n,
        std_kills: {
            let mean_k = kills as f64 / n;
            (kills_sq as f64 / n - mean_k * mean_k).max(0.0).sqrt()
        },
        min_kills: if min_kills == u32::MAX { 0 } else { min_kills },
        max_kills,
        mean_kill_progress: kill_progress / n,
        std_kill_progress: {
            let mean_kp = kill_progress / n;
            (kill_progress_sq / n - mean_kp * mean_kp).max(0.0).sqrt()
        },
        mean_shots: shots as f64 / n,
        mean_pellets: pellets as f64 / n,
        mean_crit_rate: crits as f64 / total_pellets,
        mean_big_crit_rate: big_crits as f64 / total_pellets,
        mean_crit_tier: crit_tier_sum as f64 / total_pellets,
        mean_headshot_rate: headshots as f64 / total_pellets,
        burst_dps: {
            // The time the weapon was NOT reloading, across every run. Guarded
            // at a hundredth of a second: a run that was reloading the whole
            // time has no burst to report and must not report infinity.
            let firing = (params.duration_secs * runs as f64 - downtime).max(1e-2);
            effective / firing
        },
        mean_downtime: downtime / runs as f64,
        ttk_mean: if ttks.is_empty() { 0.0 } else { ttks.iter().sum::<f64>() / ttks.len() as f64 },
        ttk_median: pct(&ttks, 0.5),
        ttk_p90: pct(&ttks, 0.9),
        ttk_runs: ttks.len() as u32,
        mean_first_magazine: first_mag / runs as f64,
        max_hit: biggest,
        mean_max_hit: max_hit_sum / runs as f64,
        damage_per_shot: effective / (total_shots as f64).max(1.0),
        damage_per_pellet: effective / total_pellets,
        hit_count,
        hit_damage,
        source_damage: {
            let mut s = sources;
            s.direct /= n;
            s.radial /= n;
            s.field /= n;
            s.arcane_on_status /= n;
            for v in s.status.iter_mut() {
                *v /= n;
            }
            s
        },
        median_run: {
            all_runs.sort_by(|a, b| a.effective_damage.total_cmp(&b.effective_damage));
            all_runs.get(all_runs.len() / 2).copied().unwrap_or_default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Default params with status disabled — for hand-computed expectations
    /// that predate the status sim.
    pub(super) fn no_status() -> DummyParams {
        DummyParams {
            status_chance: 0.0,
            // Zero the BASE too, or a relative status-chance buff (Primary
            // Crux) would resolve against 0.37 in a "no status" fixture.
            base_status_chance: 0.0,
            ..DummyParams::default()
        }
    }

    /// Cernos Prime's innate headshot bonus MULTIPLIES the additive bracket.
    ///
    /// "Cernos Prime's headshot bonus is unique and stacks multiplicatively
    /// with Primary Deadhead's headshot bonus" (wiki, Primary Deadhead). The
    /// word that matters is UNIQUE: the same note lists innate bonuses on
    /// weapons like Kuva Chakkhurr among the ADDITIVE sources, so this is a
    /// per-weapon anomaly and the flag rides on the weapon.
    ///
    /// On a 3x head with the arcane's +30% and an innate +50%:
    ///   additive       3 x (1 + 0.3 + 0.5) = 5.40
    ///   multiplicative 3 x 1.3 x 1.5       = 5.85   (+8.33%)
    #[test]
    fn cernos_primes_innate_headshot_bonus_multiplies_instead_of_adding() {
        let build = |mult: bool| DummyParams {
            duration_secs: 30.0,
            headshot_damage_bonus: 0.5,
            headshot_bonus_multiplicative: mult,
            arcane: crate::arcanes_data::ArcaneFx {
                headshot_mult_bonus: 0.3,
                ..crate::arcanes_data::ArcaneFx::none()
            },
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 3.0,
                is_head: true,
                crit_bonus: false,
            }],
            ..no_status()
        };
        let add = monte_carlo(&build(false), 1, 5).mean_damage;
        let mul = monte_carlo(&build(true), 1, 5).mean_damage;
        assert!(
            (mul / add - 5.85 / 5.40).abs() < 1e-6,
            "5.85 / 5.40 = 1.0833…, got {}",
            mul / add
        );

        // With NO arcane bonus the two readings agree — the anomaly is about
        // how the innate share COMBINES, not about its size.
        let bare = |mult: bool| DummyParams {
            arcane: crate::arcanes_data::ArcaneFx::none(),
            ..build(mult)
        };
        let (a, m) = (
            monte_carlo(&bare(false), 1, 5).mean_damage,
            monte_carlo(&bare(true), 1, 5).mean_damage,
        );
        assert!((a - m).abs() < 1e-9, "3 x 1.5 either way: {a} vs {m}");
    }

    #[test]
    fn reified_banes_empty_reload_damage_is_a_buff_that_starts_on() {
        // "On Reload From Empty: +14 Base Damage" is a BUFF, not a silent stat
        // (user, 2026-08-03): it belongs on the buff bar, it opens at one stack
        // because the modelled fight always reloads from empty, and it never
        // times out. What makes it a buff rather than a decoration is that
        // turning it off has to MOVE the damage.
        use crate::loadout::{resolve, StackPolicy, WeaponBase};
        let with = WeaponBase::from_data(
            "boar_prime", true, &["boar_prime_evo1_incarnon_form", "boar_prime_reified_bane"],
        );
        let panel = resolve(&with, &[], StackPolicy::Emergent);
        let bd = panel.evo_bd.expect("reified bane grants the evo bd buff");
        assert_eq!(bd.max_stacks, 1);
        assert_eq!(bd.stacks, 1, "it opens ON");
        assert!((bd.full - 14.0).abs() < 1e-9, "the empty-reload half is +14, got {}", bd.full);

        // It is ANNOUNCED, or no card is drawn for it.
        let params = DummyParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &ArcaneFx::none());
        assert!(
            params.buff_roster().iter().any(|(id, max)| id == "evo_reload_damage" && *max == 1),
            "the buff bar never hears about it: {:?}",
            params.buff_roster()
        );

        // And turning it off takes the damage back off — down to what the
        // build would be with only the unconditional +10 half.
        let off = {
            let mut p = DummyParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &ArcaneFx::none());
            let mut cfg = BuffConfig::new();
            cfg.insert("evo_reload_damage".into(), (0, false));
            p.apply_buff_config(&cfg);
            p
        };
        assert!(off.damage.total() < params.damage.total(), "0 stacks changed nothing");
        let ratio = off.damage.total() / params.damage.total();
        let want = bd.without / (bd.without + bd.full);
        assert!((ratio - want).abs() < 1e-9, "scaled by {ratio}, expected {want}");
        assert_eq!(off.evo_bd.expect("still declared").stacks, 0);
    }

    #[test]
    fn evo_multishot_config_rescales_the_permanent_stacks() {
        // Fevered Frenzy: base 1 pellet × (+100% at 20 stacks) is baked into
        // the resolved multishot; the per-buff config rescales it statically
        // (no in-sim trigger, no decay). Lock is meaningless and ignored.
        use crate::loadout::{resolve, StackPolicy, WeaponBase};
        let base = WeaponBase::from_data(
            "dual_toxocyst_incarnon",
            true,
            &["dual_toxocyst_evo1_incarnon_form", "dual_toxocyst_fevered_frenzy"],
        );
        let panel = resolve(&base, &[], StackPolicy::Emergent);
        let ms = panel.evo_ms.expect("fevered frenzy grants the evo ms buff");
        assert_eq!(ms.max_stacks, 20);
        assert!((ms.full - 1.0).abs() < 1e-12, "1 pellet × +100% = 1.0");

        let mk = |stacks: u32, locked: bool| {
            let mut p = DummyParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &ArcaneFx::none());
            let mut cfg = BuffConfig::new();
            cfg.insert("evo_multishot".into(), (stacks, locked));
            p.apply_buff_config(&cfg);
            p.multishot
        };
        let full = panel.multishot;
        assert!(
            (mk(20, true) - full).abs() < 1e-12,
            "full stacks = untouched"
        );
        assert!(
            (mk(0, false) - (full - 1.0)).abs() < 1e-12,
            "0 stacks removes the whole bonus"
        );
        assert!(
            (mk(10, false) - (full - 0.5)).abs() < 1e-12,
            "half stacks remove half"
        );
        assert!(
            (mk(10, true) - mk(10, false)).abs() < 1e-12,
            "lock is ignored (permanent)"
        );
    }

    /// A secondary arcane at max rank under the Emergent policy (crit-base
    /// 0 — none of these tests use the assumed-max relative crit paths).
    fn arc(id: &str) -> ArcaneFx {
        crate::arcanes_data::secondary(id).unwrap().fx(5, crate::loadout::StackPolicy::Emergent, &[], crate::tenno_data::default_tenno())
    }

    /// The same arcane with its stacks ALREADY EARNED.
    ///
    /// Buffs start at zero now (docs/BUFFS.md §Activation policy), which is
    /// the right default for a fight and the wrong fixture for a test about
    /// what a full stack is worth or how it decays. Seeding it here says which
    /// of the two a test is measuring, instead of leaning on whatever the
    /// default happens to be — the reason these tests moved when it changed.
    fn arc_stacked(id: &str) -> ArcaneFx {
        let mut fx = arc(id);
        for b in fx.buffs.iter_mut() {
            b.initial_stacks = b.max_stacks;
        }
        fx
    }

    /// Deterministic base: no crits, no procs, 1x body, no arcane.
    fn flat_base() -> DummyParams {
        DummyParams {
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            ..no_status()
        }
    }

    /// Every charge weapon that is NOT a bow pays the draw AND the listed
    /// rate's interval — the wiki's general formula, "Effective Fire Rate =
    /// 1 / (Modded Charge Time + 1 / Modded Fire Rate)". The listed rate is
    /// what happens AFTER the charge, which is why it is added and not
    /// replaced.
    ///
    /// Larkspur Prime's alt-fire is the case that brought this in: 0.5 s
    /// charge at a listed 2.0/s = 1.0 s a shot, where a bow would fire twice
    /// as often off the same two numbers.
    #[test]
    fn a_general_charge_weapon_pays_the_draw_and_the_rate() {
        let base = DummyParams {
            fire_rate: 2.0,
            charge_seconds: Some(0.5),
            magazine_size: 1000.0, // no reload inside the window
            duration_secs: 10.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let general = DummyParams {
            charge_cadence: crate::weapons_data::ChargeCadence::DrawThenRate,
            ..base.clone()
        };
        // 0.5 + 1/2.0 = 1.0 s: shots at 0, 1, 2 … 9 — ten inside 10 s.
        assert_eq!(run_once(&general, &mut Rng::new(1)).shots, 10);

        // The bow reading of the SAME two numbers is the draw alone, 0.5 s —
        // twice as many shots. One weapon's formula is not the other's.
        let bow = DummyParams {
            charge_cadence: crate::weapons_data::ChargeCadence::DrawOnly,
            ..base
        };
        assert_eq!(run_once(&bow, &mut Rng::new(1)).shots, 20);
    }

    /// A BURST weapon's listed fire rate is BURSTS per second, so its real
    /// cadence is the wiki's formula and not `1 / fire_rate`:
    ///
    ///   Effective Fire Rate = Burst Count / [1/Fire Rate + (Burst Count−1)⋅
    ///   Burst Delay]
    ///
    /// Burston Prime's numbers — 3 rounds, 5 bursts/s, 0.04 s apart — give
    /// 3 / (0.2 + 0.08) = 10.714 rounds/s, better than DOUBLE what the listed
    /// 5 would suggest. Reading the stat as a plain rate is not a rounding
    /// error on a burst weapon; it is wrong by the burst count.
    #[test]
    fn a_burst_weapon_fires_its_whole_burst_inside_the_listed_interval() {
        let burston = DummyParams {
            fire_rate: 5.0,
            burst: Some(crate::weapons_data::BurstSpec { count: 3, delay_seconds: 0.04 }),
            magazine_size: 100_000.0, // no reload inside the window
            duration_secs: 10.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        // 0.28 s a burst / 3 = 0.0933… s a round: 107 rounds in 10 s, +1 for
        // the shot at t=0.
        let r = run_once(&burston, &mut Rng::new(1));
        assert_eq!(r.shots, 108, "3 / (1/5 + 2 x 0.04) = 10.714 rounds/s");

        // The SAME two numbers read as an ordinary auto weapon: 5 rounds/s,
        // 51 shots. That is the mistake this field exists to prevent.
        let as_rate = DummyParams { burst: None, ..burston.clone() };
        assert_eq!(run_once(&as_rate, &mut Rng::new(1)).shots, 51);

        // A one-round "burst" IS an ordinary weapon — no delay is ever paid,
        // so the two readings must agree exactly.
        let single = DummyParams {
            burst: Some(crate::weapons_data::BurstSpec { count: 1, delay_seconds: 0.04 }),
            ..burston.clone()
        };
        assert_eq!(run_once(&single, &mut Rng::new(1)).shots, 51);
    }

    /// A HELD TRIGGER SPOOLS DOWN, and the loss is most of the magazine rather
    /// than a rounding error.
    ///
    /// The Phenmor's Incarnon numbers — 13.33 rounds/s falling to 60% over 51
    /// held shots (wiki, verbatim in `SustainedFireRate`). In ten seconds that
    /// is 93 rounds instead of 134: **31% fewer shots**, which is the whole of
    /// the difference between the rate the arsenal prints and the rate the
    /// weapon fires at once its 408-round pool is more than four seconds old.
    ///
    /// The spool is why the two forms compare the way they do at all. Reading
    /// 13.33 flat overstates the Incarnon form's sustained damage by half.
    #[test]
    fn a_held_trigger_spools_down_and_costs_most_of_the_magazine() {
        let phenmor = DummyParams {
            fire_rate: 13.33,
            magazine_size: 100_000.0, // no reload inside the window
            duration_secs: 10.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        assert_eq!(run_once(&phenmor, &mut Rng::new(1)).shots, 134, "the listed rate, flat");

        let spooled = DummyParams {
            sustained_fire_rate: Some(crate::weapons_data::SustainedFireRate {
                start: 1.00,
                end: 0.60,
                over_shots: 51.0,
            }),
            ..phenmor.clone()
        };
        assert_eq!(run_once(&spooled, &mut Rng::new(1)).shots, 93);

        // …AND IT RESETS WHEN FIRING STOPS. A magazine of one puts a pause
        // before every shot, so the spool never advances past its first step
        // and the count matches the unspooled weapon EXACTLY — the reset is
        // derived from the gap, not from anyone remembering to clear it in the
        // reload branch.
        let tapped = DummyParams {
            magazine_size: 1.0,
            reload_seconds: 0.05,
            ..spooled.clone()
        };
        let flat_tapped = DummyParams { sustained_fire_rate: None, ..tapped.clone() };
        assert_eq!(
            run_once(&tapped, &mut Rng::new(1)).shots,
            run_once(&flat_tapped, &mut Rng::new(1)).shots,
            "a pause between every shot is a trigger released between every shot"
        );
    }

    /// EXECUTIONER'S FORTUNE fills the magazine, and its CONDITION gates it.
    ///
    /// Two weapons, two readings of the same kind: the Furis pair pay on any
    /// headshot, the Phenmor only on one that kills. Against a target this sim
    /// cannot kill, the second must be worth exactly nothing while the first is
    /// worth a great deal — which is the assertion, because a version that
    /// ignored `needs_kill` would look perfectly healthy on the Furis.
    #[test]
    fn executioners_fortune_needs_the_kill_when_the_card_says_so() {
        let head = |chance: f64, needs_kill: bool| DummyParams {
            fire_rate: 10.0,
            magazine_size: 10.0,
            reload_seconds: 5.0,
            duration_secs: 30.0,
            // EVERY shot into a HEAD, which is what the official ruler does
            // and what makes the perk's own rate the only variable here.
            body_parts: all_head(),
            instant_reload: (chance > 0.0)
                .then_some(crate::loadout::InstantReload { chance, needs_kill }),
            ..no_status()
        };
        // A target that cannot die — `InfiniteHealth` says so outright, which
        // is stronger than a large number and is what the default fixture is.
        let unkillable = |p: DummyParams| DummyParams {
            target: frail_target(TargetMode::InfiniteHealth, 0.0, 0.0),
            ..p
        };

        let plain = run_once(&unkillable(head(0.0, false)), &mut Rng::new(7)).shots;
        // ANY headshot pays (the Furis): a 10% chance on every shot saves most
        // of the reloads, so far more rounds fit in the same 30 s.
        let furis = run_once(&unkillable(head(0.10, false)), &mut Rng::new(7)).shots;
        assert!(furis > plain, "{furis} shots with the perk, {plain} without");

        // ONLY A KILLING headshot pays (the Phenmor): nothing here ever dies,
        // so it must be worth precisely nothing — not "nearly nothing".
        let phenmor = run_once(&unkillable(head(0.20, true)), &mut Rng::new(7)).shots;
        assert_eq!(phenmor, plain, "a kill-gated perk paid without a kill");
    }

    /// LINGERING JUDGEMENT joins the ADDITIVE headshot bracket, beside Primary
    /// Deadhead's — it does not multiply it.
    ///
    /// VERBATIM (wiki, supplied by the owner 2026-08-10): "Headshot damage
    /// bonus stacks additively with Primary Deadhead's headshot damage bonus."
    /// That is the whole finding, because the two readings are far apart on a
    /// build that carries the arcane and identical on one that does not — so a
    /// test using the perk alone would pass either way.
    ///
    /// On a 2x head with the arcane's +30% and the perk's +50%, against the
    /// same build without the perk (2 x 1.3 = 2.60):
    ///   additive        2 x (1 + 0.3 + 0.5) = 3.60   ->  x1.3846
    ///   multiplicative  2 x 1.3 x 1.5       = 3.90   ->  x1.5000
    #[test]
    fn lingering_judgement_adds_to_deadheads_bracket_instead_of_multiplying_it() {
        let streak = crate::loadout::HeadshotStreak {
            hits: 2,
            within: 2.0,
            value: 0.50,
            duration: 8.0,
        };
        // Every shot into a 2x head, no crit, no status: the only thing moving
        // the number is the headshot bracket.
        let build = |deadhead: f64, perk: bool| DummyParams {
            fire_rate: 10.0,
            magazine_size: 1e9,
            duration_secs: 10.0,
            base_crit_chance: 0.0,
            unmodded_crit_chance: 0.0,
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 2.0,
                is_head: true,
                crit_bonus: false,
            }],
            headshot_streak: perk.then_some(streak),
            arcane: crate::arcanes_data::ArcaneFx {
                headshot_mult_bonus: deadhead,
                ..crate::arcanes_data::ArcaneFx::none()
            },
            target: TargetParams { base_health: 1e15, ..DummyParams::default().target },
            ..no_status()
        };
        let dmg = |p: &DummyParams| {
            let r = run_once(p, &mut Rng::new(5));
            r.total_damage / f64::from(r.shots)
        };
        // WITH DEADHEAD the two readings are 8% apart, and additive is the one
        // the card describes. The measured ratio sits a hair UNDER 1.3846
        // because the shot that arms the streak is not itself buffed — 1 of the
        // 100 shots in the window — which is the behaviour, not slack.
        let base = dmg(&build(0.30, false));
        let with = dmg(&build(0.30, true));
        let ratio = with / base;
        assert!(
            (ratio - 3.60 / 2.60).abs() < 0.02,
            "additive gives {:.4}, multiplicative would give {:.4}, got {ratio:.4}",
            3.60 / 2.60,
            3.90 / 2.60
        );

        // …and WITHOUT the arcane both readings agree at 3.0/2.0, which is why
        // the case above is the one that carries the claim.
        let solo = dmg(&build(0.0, true)) / dmg(&build(0.0, false));
        assert!((solo - 1.5).abs() < 0.02, "{solo}");
    }

    /// …and the streak has to be EARNED: two headshots inside two seconds.
    #[test]
    fn lingering_judgement_needs_two_headshots_inside_the_window() {
        let streak = crate::loadout::HeadshotStreak {
            hits: 2,
            within: 2.0,
            value: 0.50,
            duration: 8.0,
        };
        let at_rate = |fire_rate: f64| DummyParams {
            fire_rate,
            magazine_size: 1e9,
            duration_secs: 60.0,
            base_crit_chance: 0.0,
            unmodded_crit_chance: 0.0,
            body_parts: all_head(),
            headshot_streak: Some(streak),
            target: TargetParams { base_health: 1e15, ..DummyParams::default().target },
            ..no_status()
        };
        let per_shot = |p: &DummyParams| {
            let r = run_once(p, &mut Rng::new(5));
            r.total_damage / f64::from(r.shots)
        };
        // TEN SHOTS A SECOND: the second one arms it and it never lapses.
        let fast = per_shot(&at_rate(10.0));
        // ONE SHOT EVERY THREE SECONDS: no two headshots ever fall inside two,
        // so the perk is worth nothing at all — the "within" clause binding.
        let slow = per_shot(&at_rate(1.0 / 3.0));
        assert!(fast > slow * 1.4, "fast {fast}, slow {slow}");
        // The slow build must match a weapon without the perk EXACTLY.
        let bare = DummyParams { headshot_streak: None, ..at_rate(1.0 / 3.0) };
        assert!((slow - per_shot(&bare)).abs() < 1e-9, "a streak armed without a streak");
    }

    /// SPITEFUL DEFILEMENT counts status TYPES, and dies on the third.
    ///
    /// VERBATIM (wiki, owner 2026-08-10): "Multiple instances of the same
    /// status effect are not counted separately, e.g. having 5 corrosive and 5
    /// radiation status effects on a target will not disable this buff." That
    /// example is the test: ten procs of two types must leave it running, and
    /// one proc of a third type must kill it.
    ///
    /// It also lands AFTER MODS as a FLAT value — "+100% Critical Damage" is
    /// `+1.0` on the finished multiplier, not a doubling of it.
    #[test]
    fn spiteful_defilement_counts_types_not_stacks() {
        let build = |procs: Vec<DamageType>, perk: bool| DummyParams {
            fire_rate: 10.0,
            magazine_size: 1e9,
            duration_secs: 10.0,
            // ALWAYS crit, so crit damage is the only variable.
            base_crit_chance: 1.0,
            unmodded_crit_chance: 1.0,
            crit_multiplier: 2.0,
            unmodded_crit_damage: 2.0,
            forced_procs: procs,
            status_chance: 0.0,
            base_status_chance: 0.0,
            body_parts: mono_body(1.0),
            cd_below_status_count: perk.then_some((3, 1.0)),
            // `no_status()` inherits the default fixture's ARCANE, which has
            // crit terms of its own — they would land inside the ratio and make
            // "+1.0 flat" unmeasurable. Neutralised so the only crit-damage
            // sources are the weapon's 2.0 and the perk's flat add.
            arcane: crate::arcanes_data::ArcaneFx::none(),
            target: TargetParams { base_health: 1e15, ..DummyParams::default().target },
            ..no_status()
        };
        let dmg = |p: &DummyParams| {
            let r = run_once(p, &mut Rng::new(5));
            r.total_damage / f64::from(r.pellets.max(1))
        };
        // TWO TYPES, ten stacks each — the card's own example. The perk runs,
        // and a flat +1.0 on a 2.0 multiplier is exactly 1.5x the damage.
        let two = vec![DamageType::Corrosive, DamageType::Radiation];
        let on = dmg(&build(two.clone(), true)) / dmg(&build(two.clone(), false));
        assert!((on - 1.5).abs() < 0.02, "two types must keep it: {on}");

        // A THIRD TYPE turns it off, and nothing else changed.
        let three = vec![DamageType::Corrosive, DamageType::Radiation, DamageType::Viral];
        let off = dmg(&build(three.clone(), true)) / dmg(&build(three, false));
        assert!((off - 1.0).abs() < 0.02, "a third type must kill it: {off}");
    }

    /// AN INCARNON FORM GETS NOTHING FROM IT — the pool is the wrong one.
    ///
    /// VERBATIM (wiki, Executioner's Fortune): "Does not affect Incarnon Form".
    /// The reason is what makes it testable rather than a special case: the
    /// perk refills a MAGAZINE, and an Incarnon form has max CHARGES instead
    /// (owner, 2026-08-10). A charge pool is converted from weakpoint hits and
    /// sits outside the ammo economy — there is no reload there to make
    /// instant.
    ///
    /// A charge-backed form is marked by `ammo_efficiency_applies == false`,
    /// the same marker the ammo rules read, so this cannot disagree with them
    /// about which pool is which.
    #[test]
    fn executioners_fortune_does_not_touch_an_incarnon_charge_pool() {
        // A charge-backed form: its "magazine" is the gauge's round pool, and
        // `ammo_efficiency_applies` is the flag that says so.
        let charge_form = |chance: f64| DummyParams {
            fire_rate: 10.0,
            magazine_size: 10.0,
            reload_seconds: 5.0,
            duration_secs: 30.0,
            body_parts: all_head(),
            ammo_efficiency_applies: false, // charge-backed
            target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            instant_reload: (chance > 0.0)
                .then_some(crate::loadout::InstantReload { chance, needs_kill: false }),
            ..no_status()
        };
        assert_eq!(
            run_once(&charge_form(1.0), &mut Rng::new(3)).shots,
            run_once(&charge_form(0.0), &mut Rng::new(3)).shots,
            "a charge pool has no magazine to fill"
        );

        // …AND THE SAME WEAPON WITH A REAL MAGAZINE DOES take it, so the
        // assertion above is about the pool and not about a perk that never
        // fires. Identical in every other respect, including the seed.
        let with_mag = |chance: f64| DummyParams {
            ammo_efficiency_applies: true,
            ..charge_form(chance)
        };
        assert!(
            run_once(&with_mag(1.0), &mut Rng::new(3)).shots
                > run_once(&with_mag(0.0), &mut Rng::new(3)).shots
        );
    }

    /// …and it DOES pay once the target dies.
    ///
    /// The pair above proves the gate closes; this proves it opens, so the two
    /// together cannot be satisfied by a perk that simply never fires.
    #[test]
    fn a_killing_headshot_fills_the_magazine() {
        let p = DummyParams {
            fire_rate: 10.0,
            magazine_size: 10.0,
            reload_seconds: 5.0,
            duration_secs: 30.0,
            body_parts: all_head(),
            // `InstantRespawn` is the whole point and it is not a detail of
            // frailty: the DEFAULT fixture target is `InfiniteHealth`, so a
            // 1 HP version of it still never dies and a kill-gated perk reads
            // as broken. That is what the first draft of this test did.
            target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            ..no_status()
        };
        let without = run_once(&p, &mut Rng::new(11)).shots;
        let with = DummyParams {
            instant_reload: Some(crate::loadout::InstantReload { chance: 1.0, needs_kill: true }),
            ..p.clone()
        };
        let armed = run_once(&with, &mut Rng::new(11)).shots;
        assert!(armed > without, "{armed} shots with the perk, {without} without");
    }

    /// READY RETALIATION: the magazine that ran out is what pays for the reload.
    ///
    /// THE EMPTY MAGAZINE ARMS IT and the next reload spends it, so that reload
    /// is already faster — the first one of the fight included (owner,
    /// 2026-08-10: "等于给自己上了一张100% reload speed的mod"). The arming
    /// moment is the shot that empties the magazine rather than the reload that
    /// follows, which only matters when something else happens in between; see
    /// the transform test beside this one.
    ///
    /// The FIRST reload is the sharp case and it gets its own window here: the
    /// run is cut short so that exactly one reload is in it, and the perk is
    /// the difference between the second magazine having started and not. An
    /// end-to-end count over many reloads cannot tell rule 1 from "only later
    /// reloads count" — the first version of this test asserted the opposite
    /// rule and passed, because at those numbers both readings happened to fit
    /// the same whole number of magazines.
    #[test]
    fn ready_retaliation_speeds_up_the_reload_that_arms_it() {

        // 10 rounds at 10/s = 1 s of firing, then a 2 s reload — 1 s with the
        // perk. Stopping the clock at 2.5 s puts the second magazine's first
        // shots on one side of the line and nothing on the other.
        let p = DummyParams {
            fire_rate: 10.0,
            magazine_size: 10.0,
            reload_seconds: 2.0,
            duration_secs: 2.5,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let with = DummyParams { rs_on_reload: 1.0, ..p.clone() };
        let without = run_once(&p, &mut Rng::new(1)).shots;
        let armed = run_once(&with, &mut Rng::new(1)).shots;
        assert_eq!(without, 10, "the magazine, and the reload still running at 2.5 s");
        assert!(
            armed > 10,
            "the FIRST reload takes the buff it armed: {armed} shots, wanted more than 10"
        );

        // …and over a long run it compounds into whole extra magazines.
        let long = DummyParams { duration_secs: 60.0, ..p.clone() };
        let long_armed = DummyParams { rs_on_reload: 1.0, ..long.clone() };
        let a = run_once(&long, &mut Rng::new(1)).reloads;
        let b = run_once(&long_armed, &mut Rng::new(1)).reloads;
        assert!(b > a, "{b} reloads with the perk, {a} without");
    }

    /// REAVER'S RAPTURE, and every one of its moments.
    ///
    /// "On Full Burst Hit: +20% Damage, resets on Reload", capped at 5x. Four
    /// separate claims, and each is asserted on its own because each can be
    /// wrong by itself (owner, 2026-08-11: "时间节点一定要处理好…不可以含糊，
    /// 要准确"):
    ///
    /// 1. ONE STACK PER BURST, not per round and not per pellet — the card's
    ///    "not affected by multishot" — so a 3-round burst weapon earns a stack
    ///    every three rounds;
    /// 2. THE MOMENT IS THE LAST ROUND of the burst, so the burst that earns
    ///    the stack does not carry it;
    /// 3. it CAPS at five;
    /// 4. it is RESET BY THE REFILL, at the instant the reload completes.
    ///
    /// Measured on stacks rather than on damage, because a damage figure folds
    /// all four together and could be right for the wrong reason.
    #[test]
    fn reavers_rapture_counts_bursts_and_resets_on_the_refill() {
        let buff = crate::loadout::StackingBuff {
            id: "full_burst_damage",
            trigger: crate::loadout::BuffTrigger::FullBurst,
            grant: crate::loadout::BuffGrant::BaseDamage,
            decay: crate::loadout::BuffDecay::LoseOneAndReset,
            per_stack: 0.20,
            max_stacks: 5,
            duration: crate::loadout::NO_TIMEOUT,
            chance: 1.0,
            initial_stacks: 0,
            stacks_per_trigger: 1,
            per_shell: false,
            cleared_by: crate::loadout::ClearedBy::MagazineRefilled,
        };
        // 21 rounds = seven whole bursts; the cap is five, so a magazine that
        // long reaches it and sits there. The reload is long enough that the
        // reset is unambiguous in the trace.
        let p = DummyParams {
            fire_rate: 10.0,
            magazine_size: 21.0,
            reload_seconds: 2.0,
            burst: Some(crate::weapons_data::BurstSpec { count: 3, delay_seconds: 0.0 }),
            stacking_buffs: vec![buff],
            duration_secs: 10.0,
            ..no_status()
        };
        let mut rng = Rng::new(7);
        let r = run_once(&p, &mut rng);
        assert!(r.reloads >= 1, "the fixture has to reload at least once");

        // THE TRACE IS WHAT SAYS WHEN. `replay` seeds the roster from the
        // params — a hand-built `Replay` would have an empty one and the frames
        // would carry no stacks to read.
        let trace = replay(&p, Rng::new(7).state(), 600);
        let i = trace
            .buffs
            .iter()
            .position(|(id, _)| id == "full_burst_damage")
            .expect("the buff is on the roster");
        let series: Vec<u8> = trace.frames.iter().map(|f| f.stacks[i]).collect();
        assert!(series.contains(&5), "it reaches the cap: {series:?}");
        assert!(series.iter().all(|&v| v <= 5), "and never passes it: {series:?}");
        // RESET: the pile comes back DOWN to zero, which only the refill can do
        // — there is no clock on this buff.
        assert!(
            series.windows(2).any(|w| w[0] > 0 && w[1] == 0),
            "the refill takes the whole pile: {series:?}"
        );
        // ONE STACK PER BURST, and the arithmetic is the assertion. A burst
        // weapon's `fire_rate` is BURSTS per second (wiki), so 10 is ten bursts
        // — thirty rounds — a second, and five bursts of climbing is 0.5 s. At
        // one frame per 1/60 s the cap lands around frame 30. Per ROUND instead
        // of per burst would have reached it in a third of that.
        let first_cap = series.iter().position(|&v| v == 5).expect("reaches 5");
        let at = first_cap as f64 * trace.dt;
        assert!(
            at > 0.45 && at < 0.56,
            "five bursts at ten bursts a second is 0.5 s: capped at {at:.3} s (frame {first_cap})"
        );
    }

    /// ON RELOAD FROM EMPTY, on the two cards that grant different stats — and
    /// the one moment that tells this trigger apart from a plain reload.
    ///
    /// The Soma's Fresh Havoc is "+6 Base Damage, stacks up to 2x", and the
    /// Zylok's Mauler's Magazine "+1x Base Critical Damage Multiplier, stacks
    /// up to 2x". Both are held for the mission — "Buff lasts permanently
    /// throughout the mission but is lost on death" — so nothing here takes the
    /// pile, which is asserted rather than assumed: a `cleared_by` that fired
    /// would cap the run at one stack and still look like it worked.
    ///
    /// THE CONVERSIONS ARE THE OTHER HALF. "+6" is a flat base add and "+1x" a
    /// BASE crit multiplier, so both change units at `resolve` — the flat one
    /// into the share of the base-damage bucket worth the same, the crit one
    /// into the post-mod multiplier. Asserted against the card's own arithmetic:
    /// the Soma's "+96 in Incarnon Form" is 6 x 2 stacks x 8 pellets.
    #[test]
    fn on_reload_from_empty_pays_both_cards_and_only_from_empty() {
        let buffs = |weapon: &str, evo: &str| {
            let base = crate::loadout::WeaponBase::from_data(weapon, false, &[evo]);
            crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent)
                .stacking_buffs
        };

        // ---- the Soma: a FLAT base add, on the reload-from-empty trigger ----
        let sb = buffs("soma", "soma_fresh_havoc");
        assert_eq!(sb.len(), 1, "one buff on the card: {sb:?}");
        assert_eq!(sb[0].trigger, crate::loadout::BuffTrigger::ReloadFromEmpty);
        assert_eq!(sb[0].grant, crate::loadout::BuffGrant::FlatBaseDamage);
        assert_eq!(sb[0].max_stacks, 2);
        assert_eq!(sb[0].cleared_by, crate::loadout::ClearedBy::Nothing,
            "the card says it lasts the mission");
        assert_eq!(sb[0].duration, crate::loadout::NO_TIMEOUT, "and has no clock");
        // +6 on a weapon whose unmodded base is `total`, expressed as the share
        // of the base-damage bucket worth the same — unmodded, that is 6/total.
        let soma_base = crate::loadout::WeaponBase::from_data("soma", false, &[]);
        let want = 6.0 / soma_base.base_vector.total();
        assert!((sb[0].per_stack - want).abs() < 1e-9,
            "a flat +6 is {want} of an unmodded {} base, got {}",
            soma_base.base_vector.total(), sb[0].per_stack);

        // ---- the Zylok: a BASE crit-damage add, and the mods multiply it ----
        let zb = buffs("zylok", "zylok_maulers_magazine");
        let z = zb.iter().find(|b| b.grant == crate::loadout::BuffGrant::BaseCritDamage)
            .expect("the crit-damage half of the card");
        assert_eq!(z.trigger, crate::loadout::BuffTrigger::ReloadFromEmpty);
        assert_eq!(z.max_stacks, 2);
        assert!((z.per_stack - 1.0).abs() < 1e-9, "unmodded, +1x stays +1x: {}", z.per_stack);
        // …and WITH a crit-damage mod it is worth more, which is what "Base"
        // buys. Unmodded and modded are the same number for any other reading.
        let vs = crate::mods_data::class_pool("pistol").into_iter()
            .find(|m| m.id == "primed_target_cracker").expect("primed_target_cracker");
        let base = crate::loadout::WeaponBase::from_data("zylok", false, &["zylok_maulers_magazine"]);
        let modded = crate::loadout::resolve(&base, &[&vs], crate::loadout::StackPolicy::Emergent);
        let zm = modded.stacking_buffs.iter()
            .find(|b| b.grant == crate::loadout::BuffGrant::BaseCritDamage).expect("still there");
        let cd_mod = modded.crit_damage / base.base_crit_damage;
        assert!((zm.per_stack - cd_mod).abs() < 1e-6,
            "+1x BASE through a x{cd_mod} crit-damage bucket is worth that much: {}", zm.per_stack);

        // ---- it climbs to its cap over reloads, and NOTHING takes it back ----
        let p = DummyParams {
            fire_rate: 10.0,
            magazine_size: 5.0,
            reload_seconds: 0.5,
            stacking_buffs: vec![crate::loadout::StackingBuff {
                id: "on_empty_reload_damage", ..sb[0]
            }],
            duration_secs: 10.0,
            ..no_status()
        };
        let trace = replay(&p, Rng::new(7).state(), 600);
        let i = trace.buffs.iter().position(|(id, _)| id == "on_empty_reload_damage")
            .expect("on the roster");
        let series: Vec<u8> = trace.frames.iter().map(|f| f.stacks[i]).collect();
        assert_eq!(series[0], 0, "it opens empty — the fight earns it");
        assert!(series.contains(&2), "two reloads reach the cap: {series:?}");
        assert!(series.iter().all(|&v| v <= 2), "and never pass it");
        assert!(!series.windows(2).any(|w| w[0] > w[1]),
            "nothing takes the pile back — it lasts the mission: {series:?}");

        // ---- and the crit half REACHES THE DAMAGE, not just the panel ----
        // A crit-damage grant is invisible unless the weapon crits, so the
        // fixture crits every shot and the buff is the only difference.
        let crit_p = |b: Vec<crate::loadout::StackingBuff>| DummyParams {
            base_crit_chance: 1.0,
            unmodded_crit_chance: 1.0,
            crit_multiplier: 2.0,
            unmodded_crit_damage: 2.0,
            fire_rate: 10.0,
            magazine_size: 5.0,
            reload_seconds: 0.5,
            stacking_buffs: b,
            duration_secs: 10.0,
            ..no_status()
        };
        let z_buff = crate::loadout::StackingBuff { id: "on_empty_reload_crit_damage", ..*z };
        let with = monte_carlo(&crit_p(vec![z_buff]), 1, 3).mean_damage;
        let without = monte_carlo(&crit_p(vec![]), 1, 3).mean_damage;
        assert!(with > without * 1.10,
            "+1x/+2x base crit damage on a 2x weapon is worth a lot: {with} vs {without}");
    }

    /// THE ONE MOMENT `ReloadFromEmpty` IS NOT `ReloadComplete`.
    ///
    /// Entering the Incarnon form fully reloads the base magazine whether or not
    /// it had run out, and the Soma's card is explicit that only the empty case
    /// pays: "Switching to Incarnon Form from empty will ALSO trigger the buff".
    /// So a transform on a magazine with rounds left must earn nothing, where a
    /// plain reload trigger would earn a stack every cycle.
    ///
    /// The fixture is built so the two ANSWER DIFFERENTLY: the gauge fills in
    /// two hits and the base magazine holds ten, so every transform happens with
    /// eight rounds still in it. Both buffs are run in the same fight, so the
    /// difference cannot be a fixture accident.
    #[test]
    fn a_transform_on_a_full_magazine_is_not_a_reload_from_empty() {
        let head = vec![BodyPart {
            name: "head".into(), aim_weight: 1.0, multiplier: 1.0,
            is_head: true, crit_bonus: false,
        }];
        let buff = |id: &'static str, trigger| crate::loadout::StackingBuff {
            id,
            trigger,
            grant: crate::loadout::BuffGrant::BaseDamage,
            decay: crate::loadout::BuffDecay::LoseOneAndReset,
            per_stack: 0.10,
            max_stacks: 9,
            duration: crate::loadout::NO_TIMEOUT,
            chance: 1.0,
            initial_stacks: 0,
            stacks_per_trigger: 1,
            per_shell: false,
            cleared_by: crate::loadout::ClearedBy::Nothing,
        };
        let base_form = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 50.0),
            magazine_size: 10.0,
            body_parts: head.clone(),
            ..no_status()
        };
        let p = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            magazine_size: 2.0,
            body_parts: head,
            fire_rate: 10.0,
            reload_seconds: 0.5,
            stacking_buffs: vec![
                buff("on_empty_reload_damage", crate::loadout::BuffTrigger::ReloadFromEmpty),
                buff("on_reload_damage", crate::loadout::BuffTrigger::ReloadComplete),
            ],
            cycle: Some(IncarnonCycle {
                starts_primed: false,
                base_form: Box::new(base_form),
                charge_on: crate::loadout::ChargeOn::WeakpointHits,
                charges_to_fill: 2,
                transmute_out_seconds: 0.5,
                transmute_seconds: 1.0,
                reload_bucket: 0.0,
            }),
            duration_secs: 12.0,
            ..no_status()
        };
        let trace = replay(&p, Rng::new(9).state(), 900);
        let peak = |id: &str| {
            let i = trace.buffs.iter().position(|(b, _)| b == id).expect(id);
            trace.frames.iter().map(|f| f.stacks[i]).max().unwrap_or(0)
        };
        let (from_empty, on_reload) = (peak("on_empty_reload_damage"), peak("on_reload_damage"));
        assert!(on_reload > 0, "the fixture has to transform at all: {on_reload}");
        assert_eq!(from_empty, 0,
            "every transform here happens on eight rounds — none is a reload from \
             empty, yet it earned {from_empty} (the plain reload trigger earned {on_reload})");
    }

    /// KING'S GAMBIT: a body shot cannot crit, and a weak point crits more.
    ///
    /// VERBATIM (Sicarus_Incarnon_Genesis), the bullet and the two notes that
    /// name its brackets:
    ///   *'''x0''' [[Critical Chance]] on Bodyshots, '''+150%''' Critical Chance on
    ///    Weakpoint Hits.
    ///   * Bodyshot modifier is multiplicative with all sources of Critical
    ///     Chance, effectively making non-headshot critical hits impossible.
    ///   * Weakpoint modifier is additive with mods such as Pistol Gambit
    ///
    /// "Effectively making non-headshot critical hits impossible" is the sharp
    /// claim and it is asserted against a build carrying a crit-chance MOD: a
    /// bucket term would be cancelled by enough crit chance, a multiplier
    /// cannot, and only the second reading survives a Primed Pistol Gambit.
    #[test]
    fn kings_gambit_kills_body_crits_and_pays_the_weak_point_additively() {
        let perk = ["sicarus_prime_evo1_incarnon_form", "sicarus_prime_kings_gambit"];
        let panel = |evo: &[&str], mods: &[&crate::loadout::ModDef]| {
            let base = crate::loadout::WeaponBase::from_data("sicarus_prime", false, evo);
            crate::loadout::resolve(&base, mods, crate::loadout::StackPolicy::AssumedMax)
        };
        let pool = crate::mods_data::class_pool("pistol");
        let pg = pool.iter().find(|m| m.id == "primed_pistol_gambit").expect("primed_pistol_gambit");

        // THE PANEL IS UNTOUCHED. Both halves are decided by where a pellet
        // landed, so neither is a panel number — which is also what makes
        // Wiseman's Regard ignore this perk, as the same page says.
        let bare = panel(&[], &[]);
        let with = panel(&perk, &[]);
        assert!((bare.crit_chance - with.crit_chance).abs() < 1e-9,
            "the panel's crit chance does not move: {} vs {}", bare.crit_chance, with.crit_chance);
        assert!((with.weakpoint_cc_rel - 1.50).abs() < 1e-9,
            "the weak-point half seeds the bucket the crit mods write to: {}",
            with.weakpoint_cc_rel);
        assert!((with.bodyshot_cc_mult - 0.0).abs() < 1e-9, "x0: {}", with.bodyshot_cc_mult);
        assert!((panel(&[], &[]).bodyshot_cc_mult - 1.0).abs() < 1e-9, "without the perk, ordinary");

        // ADDITIVE WITH THE MODS: Primed Pistol Gambit is +187%, so the weak
        // point sees base x (1 + 1.87 + 1.50) and the body sees base x (1+1.87)
        // — before the x0 takes it. The two brackets are read off the panel
        // because the sim reads them from exactly there.
        let modded = panel(&perk, &[pg]);
        let b = modded.base_crit_chance;
        let want_wp = modded.crit_chance + b * 1.50;
        assert!(want_wp > modded.crit_chance, "the weak point is worth more");

        // …AND IN THE FIGHT. Two targets, one all head and one all body, so the
        // crit rate is a direct reading of the two branches rather than a blend.
        let run = |evo: &[&str], head: bool| {
            let base = crate::loadout::WeaponBase::from_data("sicarus_prime", false, evo);
            let panel = crate::loadout::resolve(&base, &[pg], crate::loadout::StackPolicy::AssumedMax);
            let mut p = DummyParams::from_panel(
                &panel, &crate::arena::Arena::training(20.0), &ArcaneFx::none());
            p.body_parts = vec![BodyPart {
                name: (if head { "head" } else { "body" }).into(),
                aim_weight: 1.0, multiplier: 1.0, is_head: head, crit_bonus: false,
            }];
            p.duration_secs = 20.0;
            let s = monte_carlo(&p, 8, 5);
            s.mean_crit_rate
        };
        let body_off = run(&[], false);
        assert!(body_off > 0.10, "sanity: without the perk a body shot crits ({body_off})");
        let body_on = run(&perk, false);
        assert_eq!(body_on, 0.0,
            "x0 is multiplicative, so a body crit is impossible even under Primed              Pistol Gambit — got a crit rate of {body_on}");
        let head_on = run(&perk, true);
        assert!(head_on > run(&[], true),
            "and a weak point crits MORE with the perk: {head_on} vs {}", run(&[], true));
    }

    /// GALVANIC RELOAD: the magazine lasts longer, ONCE PER SHOT, and only
    /// while the target is carrying the status.
    ///
    /// VERBATIM (Strun_Incarnon_Genesis) and its three notes:
    ///   *On hitting a target affected by an {{D|Electricity}} status, '''40%'''
    ///    chance to restore 1 round in the magazine from ammo pool.
    ///   *The status effect may originate from any source.
    ///   *The bonus can only apply once per enemy hit.
    ///   *The bonus does not affect the Incarnon form.
    ///
    /// The second note is the one worth a test of its own: this is a SHOTGUN
    /// family, so per-pellet instead of per-shot would be roughly ten rolls a
    /// trigger pull and a magazine that never empties. It is checked by
    /// counting RELOADS — the observable a player would notice — with the
    /// pellet count as the only thing that changes between two runs.
    #[test]
    fn galvanic_reload_restores_once_per_shot_not_once_per_pellet() {
        // A fixture that applies Electricity on every pellet, so the target is
        // always carrying one and the roll is the only variable.
        let fixture = |pellets: f64, restore: bool| {
            let mut p = DummyParams {
                damage: DamageVector::new().with(DamageType::Electricity, 100.0),
                status_chance: 1.0,
                multishot: pellets,
                magazine_size: 10.0,
                fire_rate: 5.0,
                reload_seconds: 2.0,
                duration_secs: 60.0,
                ..no_status()
            };
            if restore {
                p.round_restore_on_status = Some((DamageType::Electricity, 0.40, 1.0));
            }
            let s = monte_carlo(&p, 12, 3);
            // SHOTS PER MAGAZINE, which is the thing the perk actually changes.
            // Reload COUNT is the wrong observable here and measuring it says
            // why: a gun that reloads less also spends less time reloading, so
            // it fires more shots in the same 60 s and the count comes back up.
            s.mean_shots / s.mean_reloads.max(1.0)
        };

        // WITHOUT the perk, a 10-round magazine is 10 shots.
        let plain1 = fixture(1.0, false);
        assert!((plain1 - 10.0).abs() < 0.6, "ten rounds, ten shots: {plain1}");

        // WITH it, 40% of shots put a round back, so the magazine is worth
        // 10/(1-0.4) = 16.67 shots. The arithmetic is the assertion.
        let with1 = fixture(1.0, true);
        assert!((with1 - 16.67).abs() < 1.5,
            "a 40% refund makes a 10-round magazine 16.67 shots: {with1}");

        // AND TEN PELLETS CHANGE NOTHING, which is the note. Per pellet, ten
        // rolls a shot would refund on 99.4% of them and the magazine would
        // never empty; per shot, the pellet count is irrelevant.
        let with10 = fixture(10.0, true);
        assert!((with10 - with1).abs() < 1.5,
            "once per ENEMY HIT, so multishot does not multiply it: {with10} shots a              magazine at ten pellets against {with1} at one");

        // …AND NOTHING WITHOUT THE STATUS. Same perk, a target that never
        // catches one, so the condition is the only difference.
        let mut cold = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            status_chance: 0.0,
            magazine_size: 10.0,
            fire_rate: 5.0,
            reload_seconds: 2.0,
            duration_secs: 60.0,
            ..no_status()
        };
        let dry = monte_carlo(&cold, 12, 3).mean_reloads;
        cold.round_restore_on_status = Some((DamageType::Electricity, 0.40, 1.0));
        assert!((monte_carlo(&cold, 12, 3).mean_reloads - dry).abs() < 1e-9,
            "no Electricity on the target, no refund");
    }

    /// CRIMSON OVERTURE, and the claim that makes `BuffTrigger::Kill` worth a
    /// variant: A KILL COUNTS WHEREVER IT CAME FROM.
    ///
    /// VERBATIM (Boltor_Incarnon_Genesis, EVO2):
    ///   *Increase Base Damage by '''+X'''.
    ///   *On Kill: Increase Base Damage by '''+2''' and '''+20%''' [[Ammo Efficiency]]
    ///    for '''5''' seconds. Stacks up to '''Y'''x
    ///   | X = 12<br>Y = 4 | X = 0<br>Y = 3 | X = 0<br>Y = 3
    ///
    /// The +2 is in the BULLET and only X and the cap are per-variant — which
    /// is where the transcription went wrong before this: the Boltor's card had
    /// X in the per-stack slot and no unconditional half at all.
    ///
    /// The trigger is read off the kill COUNTER rather than bumped at each of
    /// the six sites a kill can happen, so the second half of this test kills
    /// the target with a DoT and nothing else: no direct hit lands the killing
    /// blow, and the stacks must still climb.
    #[test]
    fn on_kill_stacks_climb_from_a_kill_the_gun_did_not_land() {
        let buff = crate::loadout::StackingBuff {
            id: "on_kill_damage",
            trigger: crate::loadout::BuffTrigger::Kill,
            grant: crate::loadout::BuffGrant::BaseDamage,
            decay: crate::loadout::BuffDecay::LoseOneAndReset,
            per_stack: 0.10,
            max_stacks: 4,
            duration: 5.0,
            chance: 1.0,
            initial_stacks: 0,
            stacks_per_trigger: 1,
            per_shell: false,
            cleared_by: crate::loadout::ClearedBy::Nothing,
        };
        // A target that dies to every shot and comes straight back, so kills
        // are frequent and nothing else in the fixture is doing anything.
        let p = DummyParams {
            magazine_size: 100.0,
            fire_rate: 10.0,
            stacking_buffs: vec![buff],
            duration_secs: 10.0,
            body_parts: mono_body(1.0),
            target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            ..flat_base()
        };
        let trace = replay(&p, Rng::new(4).state(), 600);
        let i = trace.buffs.iter().position(|(id, _)| id == "on_kill_damage")
            .expect("on the roster");
        let series: Vec<u8> = trace.frames.iter().map(|f| f.stacks[i]).collect();
        assert_eq!(series[0], 0, "it opens empty — the fight earns it");
        assert!(series.contains(&4), "four kills reach the cap: {series:?}");
        assert!(series.iter().all(|&v| v <= 4), "and never pass it");

        // …AND A KILL THE GUN DID NOT LAND still counts. The shot does almost
        // nothing and a Slash DoT finishes the target, so every kill happens in
        // the DoT path — a trigger wired to the direct-hit site would score zero
        // here and look fine everywhere else.
        let dot = DummyParams {
            damage: DamageVector::new().with(DamageType::Slash, 1.0),
            status_chance: 1.0,
            base_status_chance: 1.0,
            magazine_size: 100.0,
            fire_rate: 1.0,
            stacking_buffs: p.stacking_buffs.clone(),
            duration_secs: 30.0,
            body_parts: mono_body(1.0),
            target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            ..flat_base()
        };
        // The claim rests on the SHOT being harmless, so it is asserted rather
        // than reasoned about: 1 damage against 50 health, so no direct hit can
        // ever be the killing blow and every kill in this run is a DoT's.
        assert!(dot.damage.total() < dot.target.base_health,
            "the shot must not be able to kill: {} damage against {} health",
            dot.damage.total(), dot.target.base_health);
        let s = monte_carlo(&dot, 4, 11);
        assert!(s.mean_kills > 0.0, "the fixture has to kill something: {}", s.mean_kills);
        let trace = replay(&dot, Rng::new(11).state(), 1200);
        let i = trace.buffs.iter().position(|(id, _)| id == "on_kill_damage").expect("roster");
        let peak = trace.frames.iter().map(|f| f.stacks[i]).max().unwrap_or(0);
        assert!(peak > 0,
            "a kill counts wherever it came from — {} kills and the pile never moved",
            s.mean_kills);
    }

    /// EXACT PENANCE, and the note that separates it from the effect it looks
    /// like: A STATUS KILL COUNTS.
    ///
    /// VERBATIM (Lato_Incarnon_Genesis):
    ///   *On Kill: '''50%''' chance for Instant Reload.
    ///   *Kills from status effects can also trigger the effect.
    ///   *The bonus does not affect the Incarnon form.
    ///
    /// `instant_reload_on_headshot` asks for a weak-point DIRECT hit and is
    /// wired to the direct-hit site, so a Slash DoT kill would score nothing
    /// there. This one is read off the kill counter, and the second fixture is
    /// the proof: the shot deals 1 damage against 50 health, so no direct hit
    /// can ever be the killing blow.
    #[test]
    fn exact_penance_reloads_on_a_kill_the_gun_did_not_land() {
        let build = |chance: Option<f64>, slash: bool| DummyParams {
            damage: if slash {
                DamageVector::new().with(DamageType::Slash, 1.0)
            } else {
                DamageVector::new().with(DamageType::Impact, 200.0)
            },
            status_chance: if slash { 1.0 } else { 0.0 },
            base_status_chance: if slash { 1.0 } else { 0.0 },
            magazine_size: 5.0,
            ammo_cost: 1.0,
            fire_rate: 2.0,
            reload_seconds: 3.0,
            duration_secs: 60.0,
            instant_reload_on_kill: chance,
            body_parts: mono_body(1.0),
            target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            ..flat_base()
        };

        // GUNFIRE KILLS: an instant reload should buy shots, because the three
        // seconds a reload costs are three seconds not spent shooting.
        let bare = monte_carlo(&build(None, false), 6, 5);
        let with = monte_carlo(&build(Some(0.5), false), 6, 5);
        assert!(bare.mean_reloads > 3.0, "the fixture has to reload: {}", bare.mean_reloads);
        assert!(with.mean_shots > bare.mean_shots * 1.3,
            "an instant reload on half the kills buys shots: {} against {}",
            with.mean_shots, bare.mean_shots);

        // …AND A KILL THE GUN DID NOT LAND COUNTS. 1 damage against 50 health,
        // asserted rather than assumed, so every kill here is a Slash DoT's.
        let dot = build(None, true);
        assert!(dot.damage.total() < dot.target.base_health,
            "the shot must not be able to kill: {} against {}",
            dot.damage.total(), dot.target.base_health);
        let dot_bare = monte_carlo(&dot, 6, 7);
        assert!(dot_bare.mean_kills > 0.0, "the DoT has to kill: {}", dot_bare.mean_kills);
        let dot_with = monte_carlo(&build(Some(1.0), true), 6, 7);
        assert!(dot_with.mean_shots > dot_bare.mean_shots,
            "a DoT kill triggers it too — {} shots against {}, on {} kills",
            dot_with.mean_shots, dot_bare.mean_shots, dot_bare.mean_kills);
    }

    /// RESONANT RESTORE: the magazine GROWS, up to the cap, and it does not
    /// FILL.
    ///
    /// VERBATIM (Gorgon_Incarnon_Genesis): "On Reload From Empty: Increase Base
    /// Magazine Capacity by '''+15'''. Stacks up to '''3'''x." Three families carry the
    /// card — the Atomos at +5/7x, the three Gorgons at +15/3x, the Stug at
    /// +10/3x — and none of them puts a clock on it.
    ///
    /// Measured as SHOTS PER MAGAZINE across the run, which is the observable
    /// and the one that catches the cap: a 10-round magazine growing by 15
    /// three times is 55 rounds and never 70.
    #[test]
    fn resonant_restore_grows_the_magazine_to_its_cap_and_stops() {
        let build = |growth: Option<(f64, u32)>| DummyParams {
            magazine_size: 10.0,
            ammo_cost: 1.0,
            fire_rate: 20.0,
            reload_seconds: 1.0,
            duration_secs: 60.0,
            mag_growth_on_empty_reload: growth,
            body_parts: mono_body(1.0),
            ..flat_base()
        };
        let bare = run_once(&build(None), &mut Rng::new(3));
        assert!(bare.reloads > 5, "the fixture has to reload: {}", bare.reloads);

        // FOUR RELOADS IN AND THE MAGAZINE IS 55, not 70: the first reload pays
        // the first stack, and the fourth pays nothing.
        let grown = run_once(&build(Some((15.0, 3))), &mut Rng::new(3));
        assert!(grown.reloads < bare.reloads,
            "a bigger magazine reloads less: {} against {}", grown.reloads, bare.reloads);
        // Shots per magazine averages over the climb, so it lands between the
        // starting 10 and the capped 55 — and ABOVE the uncapped average would
        // be if nothing stopped it. The cap is asserted separately below.
        let per_mag = grown.shots as f64 / grown.reloads.max(1) as f64;
        assert!(per_mag > 10.0, "it grows: {per_mag} shots a magazine");

        // THE CAP IS REAL. Run long enough that an uncapped version would be
        // far past 55, and compare against one whose cap is 3 either way: the
        // only difference is the number of stacks allowed.
        let long = |max: u32| {
            let r = run_once(&DummyParams { duration_secs: 300.0, ..build(Some((15.0, max))) },
                &mut Rng::new(3));
            r.shots as f64 / r.reloads.max(1) as f64
        };
        let capped = long(3);
        let higher = long(9);
        assert!(higher > capped * 1.5,
            "a higher cap must be worth more, or the cap is not being read: {higher} vs {capped}");
        assert!(capped < 55.0,
            "10 + 3 x 15 = 55 is the ceiling, and the average is under it: {capped}");

        // …AND IT DOES NOT FILL. Growing the capacity mid-fight must not hand
        // the weapon free rounds: the reload still draws from the reserve, so a
        // FINITE one runs out at the same total either way.
        let finite = |growth| {
            let mut p = build(growth);
            p.infinite_reserve = false;
            p.reserve_ammo = 60.0;
            p.duration_secs = 600.0;
            run_once(&p, &mut Rng::new(3)).shots
        };
        assert_eq!(finite(None), finite(Some((15.0, 3))),
            "a bigger magazine is not more ammo — 60 rounds is 60 shots either way");
    }

    /// VICIOUS PROMISE: the first arrow only, and OVERGUARD does not count.
    ///
    /// VERBATIM (wiki, Paris Incarnon Genesis): "Enemies are undamaged as long
    /// as their health and shield have not been damaged. Damaging Overguard is
    /// not taken into account." That exclusion is the assertion worth writing:
    /// reading all three pools would switch the perk off on the first shot of
    /// every Eximus fight, and the difference is invisible against a target
    /// with no overguard at all.
    #[test]
    fn vicious_promise_reads_health_and_shield_and_ignores_overguard() {
        let arena = crate::arena::Arena::training(60.0);
        let panel = |evo: &[&str]| {
            let base = crate::loadout::WeaponBase::from_data("paris_prime", true, evo);
            crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent)
        };
        let perk = ["paris_prime_vicious_promise"];

        // THE CONVERSION: "+40% BASE crit chance" with no mods is 0.40, and the
        // grant is the post-mod number so an unmodded panel carries it whole.
        let p = panel(&perk);
        assert!((p.cc_on_undamaged - 0.40).abs() < 1e-9, "{}", p.cc_on_undamaged);
        assert!((p.cd_on_undamaged - 2.0).abs() < 1e-9, "{}", p.cd_on_undamaged);
        assert_eq!(panel(&[]).cc_on_undamaged, 0.0, "no perk, no grant");

        // …AND IT REACHES THE FIGHT.
        let crit = |evo: &[&str]| {
            let p = DummyParams::from_panel(&panel(evo), &arena, &ArcaneFx::none());
            monte_carlo(&p, 24, 0x71C).mean_crit_rate
        };
        assert!(crit(&perk) > crit(&[]) + 0.05, "an untouched target crits more");

        // THE OVERGUARD EXCLUSION, asserted on the predicate itself. Three
        // states of one target: whole, chewed through the overguard, and hit
        // for real. Only the last one ends the perk.
        let tp = TargetParams { base_overguard: 5_000.0, ..TargetParams::training_dummy() };
        let whole = TargetState::spawn(&tp);
        assert!(target_undamaged(&whole, &tp), "a fresh target is undamaged");

        let mut chewed = whole.clone();
        chewed.overguard = 1.0;
        assert!(
            target_undamaged(&chewed, &tp),
            "overguard is not taken into account — a target down to its last point of it is still undamaged"
        );

        let mut hurt = whole.clone();
        hurt.health -= 1.0;
        assert!(!target_undamaged(&hurt, &tp), "one point of health ends it");
        let mut stripped = whole.clone();
        stripped.shield -= 1.0;
        assert!(!target_undamaged(&stripped, &tp), "…and so does one point of shield");
    }

    /// THE BELOW-HALF-HEALTH BONUS GOES WHEREVER CO GOES, which is one rule
    /// rather than the two the cards read like.
    ///
    /// VERBATIM (wiki, Kunai Incarnon Genesis, Swift Conclusion): *"Damage
    /// bonus if enemy has less than half health is additive with Hornet Strike
    /// in basic Kunai form, and multiplicative in Incarnon form. It is also
    /// additive with Galvanized Shot in both forms."*
    ///
    /// Galvanized Shot IS the CO bonus, and the Kunai's two forms are exactly
    /// the two CO classes — Adding on the base, Multiplying on the Incarnon.
    /// So "additive with Hornet Strike here, multiplicative there, additive
    /// with CO always" is the same sentence as "it lands in the CO bracket".
    ///
    /// Asserted as ARITHMETIC on one weapon whose two forms differ, because
    /// that is the only place the two readings give different numbers.
    #[test]
    fn the_half_health_bonus_lands_in_the_weapons_own_co_bracket() {
        // 200% below half health on both Kunai forms; base form Adding,
        // Incarnon form Multiplying (CATALOGS.md).
        let base = crate::loadout::WeaponBase::from_data("kunai", true, &["kunai_swift_conclusion"]);
        let inc = crate::loadout::WeaponBase::from_data(
            "kunai_incarnon", true, &["kunai_swift_conclusion"]);
        assert_eq!(base.co_behavior, crate::loadout::CoBehavior::AdditiveWithBaseDamage);
        assert_eq!(inc.co_behavior, crate::loadout::CoBehavior::Independent);
        assert!(base.bd_below_half_health > 1.0 && inc.bd_below_half_health > 1.0);

        // A target that is ALWAYS below half health, so the term is always on,
        // and a mod bucket big enough to tell the two brackets apart.
        let arena = crate::arena::Arena::training(60.0);
        let dmg = |weapon: &str, evo: &[&str], mods: &[&str]| {
            let b = crate::loadout::WeaponBase::from_data(weapon, true, evo);
            // THE POOL IS THE BASE WEAPON'S. An Incarnon FORM entry has none
            // of its own — a mod is equipped on the weapon, not on the form.
            let pool = crate::mods_data::pool_for_weapon("kunai");
            let ms: Vec<&crate::loadout::ModDef> = mods
                .iter()
                .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("no mod {m}")))
                .collect();
            let panel = crate::loadout::resolve(&b, &ms, crate::loadout::StackPolicy::Emergent);
            let mut p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
            // A POOL THE RUN CHEWS THROUGH SLOWLY. The condition is a live one
            // — health below half — so the fixture has to actually get there:
            // spawning at full and dying instantly would leave it never true,
            // and an unkillable target would leave it never true either.
            p.target.mode = crate::dummy::TargetMode::InstantRespawn;
            p.target.base_health = 40_000.0;
            p.target.base_armor = 0.0;
            p.target.base_shield = 0.0;
            monte_carlo(&p, 8, 0x4A1F).mean_damage
        };
        let perk = ["kunai_swift_conclusion"];

        // THE MEASUREMENT IS A DIFFERENCE OF DIFFERENCES, and it has to be.
        // Adding Hornet Strike changes how fast the target dies, so it changes
        // what FRACTION of the run is spent below half health — a contamination
        // both forms carry equally. What only the ADDING form carries is the
        // dilution itself, so the claim is that it loses measurably more.
        let drop = |w: &str| {
            let bare = dmg(w, &perk, &[]) / dmg(w, &[], &[]);
            let modded = dmg(w, &perk, &["hornet_strike"]) / dmg(w, &[], &["hornet_strike"]);
            modded / bare - 1.0
        };
        let base_drop = drop("kunai");
        let inc_drop = drop("kunai_incarnon");
        assert!(
            base_drop < inc_drop - 0.04,
            "the ADDING form is diluted by Hornet Strike and the MULTIPLYING one is not:              base {:.1}% against Incarnon {:.1}% — they should not be the same number",
            base_drop * 100.0,
            inc_drop * 100.0
        );
        assert!(
            inc_drop > -0.10,
            "…and the Incarnon form's small loss is the shared uptime effect, not dilution: {:.1}%",
            inc_drop * 100.0
        );
    }

    /// FEIGNED RETREAT: half the fight at a time, and the perk's own flat base
    /// damage is excluded from what its bonus multiplies.
    ///
    /// The exclusion is the part nobody can see. VERBATIM (wiki, Sicarus
    /// Incarnon Genesis): *"Bonus damage is additive with mods such as Hornet
    /// Strike but does not take into account the Base Damage increase from this
    /// perk."* The card grants BOTH — "+50 Base Damage" and "+40% Damage below
    /// half health" — so the naive reading multiplies the 40% by a base this
    /// perk itself raised, and is wrong by `0.40 x 50` on every low-health hit.
    ///
    /// Asserted through the loaded rate rather than through a sim, because the
    /// correction is arithmetic and a Monte Carlo would bury it in the
    /// half-the-fight condition.
    #[test]
    fn a_below_half_health_bonus_excludes_the_flat_damage_its_own_card_grants() {
        let bare = crate::loadout::WeaponBase::from_data("sicarus", true, &[]);
        let base_total = bare.base_vector.total();
        let with = crate::loadout::WeaponBase::from_data("sicarus", true, &["sicarus_feigned_retreat"]);

        // The card's own flat half, read from the same file the rate came from.
        let own_flat = crate::evolutions_data::get("sicarus_feigned_retreat")
            .expect("the perk")
            .flat_base_damage();
        assert!(own_flat > 0.0, "this card grants flat base damage too, or the test proves nothing");

        // What the rate must be: 0.40, minus the share of it that would have
        // landed on the perk's own contribution to the base.
        let evolved = base_total + own_flat;
        let expected = 0.40 - 0.40 * own_flat / evolved;
        assert!(
            (with.bd_below_half_health - expected).abs() < 1e-9,
            "corrected rate {:.6}, expected {expected:.6} (card 0.40, own flat {own_flat}, evolved base {evolved})",
            with.bd_below_half_health
        );
        assert!(
            with.bd_below_half_health < 0.40,
            "…and it is strictly less than the card's number, which is the whole correction"
        );

        // A LOCK TAKES IT, like every other base-damage bonus.
        assert_eq!(bare.bd_below_half_health, 0.0, "no perk, no bonus");
    }

    /// EVERY "WITH <player stat>" PERK GOES THROUGH ONE GATE, and each still
    /// lands in its own bracket.
    ///
    /// Three conditions and three grants, none of which the neutral Tenno
    /// opens: it sprints at 0.9 (the slowest a frame has), carries no armor and
    /// has no energy pool. That default is the honest one — a build that has
    /// not said which frame is holding the gun should not be paid for a
    /// threshold it may not reach.
    ///
    /// Asserted per BRACKET, because one list feeding five brackets is exactly
    /// the shape where a refactor quietly routes two of them to the same place.
    #[test]
    fn a_gated_perk_pays_only_the_frames_that_open_it() {
        let slow = crate::tenno_data::default_tenno().clone();
        assert!(slow.sprint < 1.2 && slow.armor <= 450.0 && slow.energy <= 700.0);

        let panel = |weapon: &str, evo: &[&str], tenno: &crate::tenno_data::Tenno| {
            let base = crate::loadout::WeaponBase::from_data(weapon, true, evo);
            crate::loadout::resolve_for(&base, &[], crate::loadout::StackPolicy::Emergent, tenno)
        };

        // MULTISHOT, on armor.
        let mut armoured = slow.clone();
        armoured.armor = 500.0;
        let off = panel("cestra", &["cestra_fortress_salvo"], &slow);
        let on = panel("cestra", &["cestra_fortress_salvo"], &armoured);
        let bare = panel("cestra", &[], &armoured);
        assert!((off.multishot - bare.multishot).abs() < 1e-9, "no armor, no multishot");
        assert!(
            (on.multishot - bare.multishot * 1.8).abs() < 1e-6,
            "+80% of base multishot with armor over 450: {} against {}",
            on.multishot,
            bare.multishot * 1.8
        );

        // BASE CRIT DAMAGE, on max energy — and it is the BASE, so it would be
        // multiplied by a crit-damage mod.
        let mut energetic = slow.clone();
        energetic.energy = 1000.0;
        let off = panel("atomos", &["atomos_paladin_virtue"], &slow);
        let on = panel("atomos", &["atomos_paladin_virtue"], &energetic);
        assert!((on.crit_damage - off.crit_damage - 1.0).abs() < 1e-9,
            "+1x with max energy over 700: {} against {}", on.crit_damage, off.crit_damage);

        // PROJECTILE SPEED, on sprint — a different bucket again.
        let mut fast = slow.clone();
        fast.sprint = 1.25;
        let ps = |t: &crate::tenno_data::Tenno| {
            panel("bronco", &["bronco_speeding_bullet"], t)
                .indirect
                .iter()
                .find(|(s, _)| *s == crate::loadout::IndirectStat::ProjectileSpeed)
                .map_or(0.0, |(_, v)| *v)
        };
        assert!((ps(&slow) - 0.0).abs() < 1e-9, "at 0.9 sprint it is worth nothing");
        assert!((ps(&fast) - 0.60).abs() < 1e-9, "at 1.25 it is the whole +60%: {}", ps(&fast));

        // …and the gates do not leak into each other: an armoured player gets
        // the Cestra's multishot and NOT the Atomos's crit damage.
        let a = panel("atomos", &["atomos_paladin_virtue"], &armoured);
        assert!((a.crit_damage - off.crit_damage).abs() < 1e-9, "armor is not energy");
    }

    /// DEADLY PACE ASKS WHO IS CARRYING THE BOW. "With Sprint Speed 1.2 or
    /// Higher: +80% Fire Rate" — the second perk in the roster to read a PLAYER
    /// stat, and it reads it through the same `condition:` spelling the first
    /// one uses.
    ///
    /// The neutral Tenno sprints at 0.9, the slowest a frame has, so the
    /// default build pays nothing. Asserted on BOTH sides, because a gate that
    /// is simply never open passes the first half on its own.
    #[test]
    fn a_sprint_gated_fire_rate_pays_only_the_frames_that_reach_it() {
        let slow = crate::tenno_data::default_tenno().clone();
        assert!(slow.sprint < 1.2, "the neutral player is the slowest one: {}", slow.sprint);
        let mut fast = slow.clone();
        fast.sprint = 1.25; // Loki Prime

        let rate = |evo: &[&str], tenno: &crate::tenno_data::Tenno| {
            let base = crate::loadout::WeaponBase::from_data("paris_prime", true, evo);
            crate::loadout::resolve_for(&base, &[], crate::loadout::StackPolicy::Emergent, tenno)
                .fire_rate
        };
        let perk = ["paris_prime_deadly_pace"];

        let bare = rate(&[], &slow);
        assert!(
            (rate(&perk, &slow) - bare).abs() < 1e-9,
            "at 0.9 sprint the perk is worth nothing: {} against {bare}",
            rate(&perk, &slow)
        );
        assert!(
            (rate(&perk, &fast) - bare * 1.8).abs() < 1e-6,
            "at 1.25 sprint it is the whole +80%: {} against {}",
            rate(&perk, &fast),
            bare * 1.8
        );
        // …and the same frame gets nothing extra without the perk, so what
        // moved is the perk and not the Tenno.
        assert!((rate(&[], &fast) - bare).abs() < 1e-9, "sprint speed alone changes no fire rate");
    }

    /// WELL REHEARSED: a body shot takes the pile, which is the one thing that
    /// makes this trigger different from "on headshot".
    ///
    /// Modelled as a stack that only CONSECUTIVE weak-point hits build, so the
    /// perk is worth its cap to a player who never misses the head, something
    /// less to one who mostly does, and exactly nothing to one who never hits
    /// it. All three are asserted, because a trigger that simply never fires
    /// would pass the third on its own.
    #[test]
    fn a_consecutive_weakpoint_buff_is_undone_by_a_body_shot() {
        let arena = crate::arena::Arena::training(120.0);
        let dmg = |evo: &[&str], head_share: f64| {
            let base = crate::loadout::WeaponBase::from_data("sybaris_prime", true, evo);
            let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
            let mut p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
            // ONE head and one body, both at 1x, so the only thing the aim
            // changes is which trigger fires — not how hard the hit lands.
            p.body_parts = vec![
                BodyPart { name: "head".into(), aim_weight: head_share, multiplier: 1.0,
                           is_head: true, crit_bonus: false },
                BodyPart { name: "body".into(), aim_weight: 1.0 - head_share, multiplier: 1.0,
                           is_head: false, crit_bonus: false },
            ];
            let s = monte_carlo(&p, 16, 0x5B15);
            s.mean_damage / s.mean_pellets.max(1e-9)
        };
        let perk = ["sybaris_prime_well_rehearsed"];

        let always = dmg(&perk, 1.0) / dmg(&[], 1.0);
        let half = dmg(&perk, 0.5) / dmg(&[], 0.5);
        let never = dmg(&perk, 0.0) / dmg(&[], 0.0);

        // NEVER A HEADSHOT, NEVER A STACK — and "nothing" here is not 1.0,
        // because the same card also grants a static +15 base damage. So the
        // floor is exactly that clause and not one point more, which is the
        // assertion that a trigger firing on the wrong event fails.
        let unmodded = crate::loadout::WeaponBase::from_data("sybaris_prime", true, &[])
            .base_vector
            .total();
        let static_only = (unmodded + 15.0) / unmodded;
        assert!(
            (never - static_only).abs() / static_only < 0.001,
            "body shots only: worth its static clause and nothing else — x{never:.4} against x{static_only:.4}"
        );
        assert!(
            always > never * 1.02,
            "every shot a headshot: the pile stands on top of the static clause, x{always:.4} against x{never:.4}"
        );
        // …AND HALF THE SHOTS TO THE BODY IS WORTH FAR LESS THAN HALF THE PERK,
        // which is the assertion that separates THREE IN A ROW from three in
        // total. At a 50% head rate a streak buff averages 0.5 + 0.25 + 0.125 =
        // 0.875 of its 3 stacks — under a third of the cap — while one that
        // merely accumulates sits near it. So the midpoint is the line.
        //
        // Measured: 1.2173 with the body reset, 1.3094 without it, against a
        // midpoint of 1.2474. Deleting the reset moves it across.
        let midpoint = never + 0.5 * (always - never);
        assert!(
            half < midpoint && half > never * 1.001,
            "three IN A ROW at a 50% head rate: x{half:.4}, which must sit below the              midpoint x{midpoint:.4} of (x{never:.4}, x{always:.4}) — above it means the              stacks are accumulating rather than streaking"
        );
    }

    /// COLD ON A TARGET THAT CANNOT BE FROZEN: the ladder climbs to the cap and
    /// STAYS, which makes Cold worth MORE here rather than less.
    ///
    /// The ordinary ladder spends itself. Nine stacks stand; the tenth proc
    /// consumes all of them for a 3-second Frozen window worth +1.00 crit
    /// damage, and drops back to three. So the bonus sawtooths. On a Demolisher
    /// nothing converts, so ten stacks stand permanently at +0.55 and the 3
    /// seconds of +1.00 never come.
    ///
    /// Asserted on the LADDER rather than through a sim, because what changed
    /// is a rule about stacks and a Monte Carlo would show it as a few per cent
    /// on a damage number.
    #[test]
    fn cold_never_converts_on_a_target_that_cannot_be_frozen() {
        let mut ordinary = DebuffState::default();
        let mut never = DebuffState::default();
        // Twelve procs, a tenth of a second apart — past the tenth either way.
        for k in 0..12 {
            let t = k as f64 * 0.1;
            ordinary.apply_cold_proc(t, 1.0, false, None, false);
            never.apply_cold_proc(t, 1.0, false, None, true);
        }
        let at = 1.2;

        // THE ORDINARY ONE CONVERTED: it is Frozen, and its stacks were spent.
        assert!(ordinary.frozen_until.is_some_and(|f| f > at), "the tenth proc converts");
        assert!(
            (ordinary.cold_cd_bonus(at) - 1.00).abs() < 1e-9,
            "…and Frozen is +1.00 while it lasts, {}",
            ordinary.cold_cd_bonus(at)
        );

        // THE DEMOLISHER DID NOT. Ten stacks, no Frozen, and the bonus is the
        // table's top row rather than the window's.
        assert!(never.frozen_until.is_none(), "nothing converts");
        assert_eq!(never.freeze.len(), 10, "it climbs to the ten-stack cap and stops");
        assert!(
            (never.cold_cd_bonus(at) - 0.55).abs() < 1e-9,
            "0.10 + 0.05 x 9 = 0.55, held all fight: {}",
            never.cold_cd_bonus(at)
        );

        // …and it KEEPS climbing back to the cap rather than being spent: two
        // more procs later it is still ten, where the ordinary one is rebuilding
        // from the three Frozen left it.
        never.apply_cold_proc(1.3, 1.0, false, None, true);
        never.apply_cold_proc(1.4, 1.0, false, None, true);
        assert_eq!(never.freeze.len(), 10, "the cap holds, and nothing consumes it");
    }

    /// A FLAT BASE-DAMAGE BUFF IS NOT DILUTED BY SERRATION, and that is the
    /// only reason it is a bracket of its own.
    ///
    /// Striking Succession grants "+15 Base Damage" a stack. A base add raises
    /// the number the base-damage bucket multiplies, so its RELATIVE worth is
    /// `(base + flat) / base` — the same figure whether or not a damage mod is
    /// equipped. A bucket grant of the same nominal size would be worth less
    /// with every mod added to that bucket.
    ///
    /// So the test is not a golden number: it is that the perk's gain is the
    /// SAME bare and modded. `resolve` converts the flat number into a bucket
    /// share once the mods are known, and getting that conversion wrong shows
    /// up here as a gain that shrinks.
    #[test]
    fn a_flat_base_damage_buff_keeps_its_worth_when_a_damage_mod_goes_in() {
        let arena = crate::arena::Arena::training(120.0);
        let dmg = |evo: &[&str], mods: &[&str]| {
            let base = crate::loadout::WeaponBase::from_data("paris_prime", true, evo);
            let pool = crate::mods_data::pool_for_weapon("paris_prime");
            let ms: Vec<&crate::loadout::ModDef> = mods
                .iter()
                .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("no mod {m}")))
                .collect();
            let panel = crate::loadout::resolve(&base, &ms, crate::loadout::StackPolicy::Emergent);
            let p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
            let s = monte_carlo(&p, 8, 0x5017);
            s.mean_damage / s.mean_pellets.max(1e-9)
        };
        let perk = ["paris_prime_striking_succession"];

        let bare = dmg(&perk, &[]) / dmg(&[], &[]);
        let modded = dmg(&perk, &["serration"]) / dmg(&[], &["serration"]);
        assert!(bare > 1.02, "the perk is worth something at all: x{bare:.4}");
        assert!(
            (bare - modded).abs() / bare < 0.02,
            "a FLAT base add is worth the same with a damage mod in: x{bare:.4} bare,              x{modded:.4} with Serration — a bucket grant would have shrunk"
        );
    }

    /// BLAZING BARREL, and the two brackets one perk name lands in.
    ///
    /// The Strun family's card reads "+0.05 BASE Multishot" and the Sybaris
    /// family's "+5% Multishot". On a bare weapon those are the same 0.05 a
    /// stack and the difference is invisible — which is exactly why `base:` is
    /// required in the yaml rather than defaulted. With a multishot MOD in, the
    /// base add is multiplied by the mod bucket and the percentage is not, and
    /// this test is the one place that separation is pinned.
    ///
    /// Hell's Chamber is +120%, so at five stacks the Strun's 0.25 becomes 0.55
    /// while a percentage grant of the same size would stay flat. Asserted as a
    /// RATIO between the two brackets rather than against a golden number, so
    /// it survives any change to the weapon's own pellet count.
    #[test]
    fn blazing_barrel_lands_in_the_bracket_its_card_names() {
        let arena = crate::arena::Arena::training(60.0);
        // A magazine large enough to reach the cap and stay there: the stacks
        // are cleared by the reload, so a 2-round shotgun would spend the run
        // climbing.
        let pellets = |evo: &[&str], mods: &[&str]| {
            let base = crate::loadout::WeaponBase::from_data("strun_prime", true, evo);
            let pool = crate::mods_data::pool_for_weapon("strun_prime");
            let ms: Vec<&crate::loadout::ModDef> = mods
                .iter()
                .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("no mod {m}")))
                .collect();
            let panel = crate::loadout::resolve(&base, &ms, crate::loadout::StackPolicy::Emergent);
            let p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
            let s = monte_carlo(&p, 12, 0xB1A2);
            s.mean_pellets / s.mean_shots
        };
        let perk = ["strun_prime_blazing_barrel"];

        // BARE: the grant is worth its face value, so the perk raises the
        // pellet count at all. Without this the ratio below is 1.0 for the
        // uninteresting reason that nothing happened.
        let (bare_off, bare_on) = (pellets(&[], &[]), pellets(&perk, &[]));
        assert!(
            bare_on > bare_off,
            "on firing, +0.05 base multishot a stack: {bare_off:.3} -> {bare_on:.3} pellets a shot"
        );

        // MODDED: the same perk is worth MORE, because a base add is multiplied
        // by the multishot bucket. A percentage grant would have gained exactly
        // the bare amount here, so the two brackets are told apart by whether
        // this ratio exceeds 1.
        let (mod_off, mod_on) = (pellets(&[], &["hells_chamber"]), pellets(&perk, &["hells_chamber"]));
        let bare_gain = bare_on - bare_off;
        let mod_gain = mod_on - mod_off;
        assert!(
            mod_gain > bare_gain * 1.5,
            "a BASE add is multiplied by the bucket: +{bare_gain:.3} pellets bare,              +{mod_gain:.3} with Hell's Chamber — a percentage grant would have given +{bare_gain:.3} in both"
        );
    }

    /// …AND THE RELOAD TAKES THEM, but swapping OUT of the Incarnon form does
    /// not. VERBATIM (wiki, Strun Incarnon Genesis): "resets entirely upon
    /// reloading. Entering Incarnon Form counts as reloading but exiting does
    /// not."
    ///
    /// That one exception is the whole reason `ClearedBy::Reload` exists beside
    /// `MagazineRefilled`, which fires on both transforms. Asserted through the
    /// SHAPE of the buff rather than by replaying a swap, because what the two
    /// clearers disagree about is a single event and the disagreement is
    /// declared, not emergent.
    #[test]
    fn blazing_barrel_is_cleared_by_a_reload_and_not_by_a_refill() {
        let base = crate::loadout::WeaponBase::from_data("strun_prime", true, &["strun_prime_blazing_barrel"]);
        let b = base
            .stacking_buffs
            .iter()
            .find(|b| b.id == "on_firing_multishot")
            .expect("the perk pushes its buff");
        assert_eq!(b.cleared_by, crate::loadout::ClearedBy::Reload);
        assert_eq!(b.trigger, crate::loadout::BuffTrigger::Firing);
        assert_eq!(b.grant, crate::loadout::BuffGrant::BaseMultishot);
        // No clock: the card states none and the reset is what ends it.
        assert!(b.duration.is_infinite(), "no timer — the reload is the end");
        assert_eq!(b.max_stacks, 5);

        // …and the Sybaris's same-named perk is the OTHER bracket.
        let syb = crate::loadout::WeaponBase::from_data("sybaris_prime", true, &["sybaris_prime_blazing_barrel"]);
        let s = syb
            .stacking_buffs
            .iter()
            .find(|b| b.id == "on_firing_multishot")
            .expect("the Sybaris carries it too");
        assert_eq!(s.grant, crate::loadout::BuffGrant::MultishotPercent);
        assert_eq!(s.max_stacks, 10);
    }

    /// THE LATRON'S TWO PUNCTURE PERKS, both measured against the status they
    /// read rather than against a build that happens to be good.
    ///
    /// Riddled Target is "+25% Multishot for 8s per Puncture Status, 4x" and
    /// Flensing Spikes is "Remove 20% of enemy Armor per Puncture Status". Both
    /// were inert for the same reason and neither for a good one: the stacking
    /// buff existed but its loader arm named Electricity, and armour stripping
    /// existed but only for the two statuses that strip on their own.
    ///
    /// A TARGET WITH ARMOUR is the fixture, because that is the only place the
    /// second perk can be seen at all — and the two are asserted separately, so
    /// one carrying the other cannot pass for both.
    #[test]
    fn the_latrons_puncture_perks_read_the_status_they_name() {
        let arena = crate::arena::Arena::training(30.0);
        let run = |evo: &[&str]| {
            let base = crate::loadout::WeaponBase::from_data("latron_prime", true, evo);
            let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
            let mut p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
            // ARMOUR, and a target that survives long enough to carry five
            // Puncture stacks. The training dummy has none of either.
            p.target.base_armor = 500.0;
            p.target.base_health = 2_000_000.0;
            monte_carlo(&p, 20, 0x1A7)
        };
        let off = run(&[]);
        let riddled = run(&["latron_prime_riddled_target"]);
        let flensing = run(&["latron_prime_flensing_spikes"]);

        // RIDDLED TARGET pays in PELLETS: +25% multishot a stack, four stacks,
        // on a weapon whose own damage is mostly Puncture — so the pellet count
        // per shot has to rise, and that is a thing no damage bonus can fake.
        let per_shot = |s: &Summary| s.mean_pellets / s.mean_shots;
        assert!(
            per_shot(&riddled) > per_shot(&off) * 1.2,
            "four stacks of +25% multishot: {:.3} -> {:.3} pellets a shot",
            per_shot(&off), per_shot(&riddled)
        );

        // FLENSING SPIKES pays in MITIGATION: the same pellets, landing harder,
        // because the armour in front of the health is gone. Asserted on damage
        // per pellet so the multishot perk above cannot be mistaken for it.
        let per_pellet = |s: &Summary| s.mean_effective_damage / s.mean_pellets;
        assert!(
            per_pellet(&flensing) > per_pellet(&off) * 1.1,
            "20% of the armour a Puncture stack: {:.1} -> {:.1} a pellet",
            per_pellet(&off), per_pellet(&flensing)
        );
        // …and it is the ARMOUR it removed, not damage it added: against a
        // target with none, the perk is worth nothing at all.
        let bare = |evo: &[&str]| {
            let base = crate::loadout::WeaponBase::from_data("latron_prime", true, evo);
            let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
            let mut p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
            p.target.base_armor = 0.0;
            p.target.base_health = 2_000_000.0;
            monte_carlo(&p, 20, 0x1A7).mean_effective_damage
        };
        let (a, b) = (bare(&[]), bare(&["latron_prime_flensing_spikes"]));
        assert!(
            (a - b).abs() / a < 0.01,
            "unarmoured, an armour strip is worth nothing: {a:.0} vs {b:.0}"
        );
    }

    /// SWIFT PUNISHMENT ASKS ABOUT THE PLAYER, and the neutral one cannot
    /// answer yes.
    ///
    /// "With Sprint Speed 1.2 or Higher: +30% Direct Damage per Status Type" —
    /// a question about who is carrying the gun rather than about the gun, so
    /// it is answered where the Tenno exists. The default wielder sprints at
    /// 0.9, the slowest a frame has (owner, 2026-08-12), so the perk's second
    /// half pays NOTHING until a frame is named; a Volt at 1.2 turns it on.
    ///
    /// The flat +6 it also grants is untouched either way, which is what says
    /// the gate is on the right half.
    #[test]
    fn swift_punishment_pays_only_a_frame_that_can_run() {
        let base = crate::loadout::WeaponBase::from_data(
            "latron_prime", true, &["latron_prime_swift_punishment"],
        );
        let bare = crate::loadout::WeaponBase::from_data("latron_prime", true, &[]);
        let with = |sprint: f64| {
            let mut t = crate::tenno_data::default_tenno().clone();
            t.sprint = sprint;
            crate::loadout::resolve_for(&base, &[], crate::loadout::StackPolicy::Emergent, &t)
        };
        let slow = with(0.9);
        let fast = with(1.2);
        assert_eq!(slow.co_per_type, 0.0, "0.9 cannot reach 1.2");
        assert!((fast.co_per_type - 0.30).abs() < 1e-9, "{}", fast.co_per_type);
        // …and the flat half is the perk's either way.
        let plain = crate::loadout::resolve(&bare, &[], crate::loadout::StackPolicy::Emergent);
        assert!(slow.modified_base > plain.modified_base, "the +6 pays regardless");
        assert!((slow.modified_base - fast.modified_base).abs() < 1e-9);
    }

    /// THE EMPTY MAGAZINE ARMS IT, and the TRANSFORM is what proves that.
    ///
    /// Owner, 2026-08-11: "这个buff应该是在空弹夹的时候就有了，因为这时候如果我
    /// 立刻变身灵化，灵化速度也会吃到，说明这时候就是有buff的。接着退出灵化以
    /// 后，这时候相当于reload了一次，这个buff消失了."
    ///
    /// A reload alone cannot tell the two readings apart: armed at the empty
    /// magazine and armed when the reload starts both make that reload faster.
    /// THE TRANSMUTE CAN — it happens between the two, so it is faster under
    /// the first reading and untouched under the second.
    ///
    /// A synthetic cycle, because the real one that has this perk cannot show
    /// it: the Phenmor's base form transmutes on a full gauge and never empties,
    /// so nothing ever arms it there (which is its own measured fact — the perk
    /// is worth exactly zero in that cycle). Here the base form has a 2-round
    /// magazine and charges on direct hits, so it empties, transforms, and
    /// comes back, over and over.
    #[test]
    fn an_empty_magazine_arms_ready_retaliation_before_any_reload() {
        let mk = |rs: f64| {
            // THE PERK IS THE BASE FORM'S, and only the base form's: the
            // evolution loader drops it on a charge-backed form. The outer
            // params below therefore carry ZERO on purpose — setting it in both
            // places hid a real bug for one run, where the animations were
            // reading the Incarnon half's copy and finding nothing there.
            let base_form = DummyParams {
                damage: DamageVector::new().with(DamageType::Impact, 50.0),
                crit_multiplier: 1.0,
                magazine_size: 2.0,
                reload_seconds: 2.0,
                rs_on_reload: rs,
                body_parts: mono_body(1.0),
                ..no_status()
            };
            DummyParams {
                damage: DamageVector::new().with(DamageType::Impact, 100.0),
                crit_multiplier: 1.0,
                magazine_size: 2.0,
                reload_seconds: 2.0,
                ammo_efficiency_applies: false,
                body_parts: mono_body(1.0),
                duration_secs: 60.0,
                cycle: Some(IncarnonCycle {
                    starts_primed: false,
                    base_form: Box::new(base_form),
                    charge_on: crate::loadout::ChargeOn::DirectHits,
                    charges_to_fill: 2,
                    // LONG animations, so the difference between running them at
                    // one speed and at double shows up in whole transforms
                    // rather than in rounding.
                    transmute_out_seconds: 2.0,
                    transmute_seconds: 2.0,
                    reload_bucket: 0.0,
                }),
                ..no_status()
            }
        };
        let off = monte_carlo(&mk(0.0), 20, 0x5EED);
        let on = monte_carlo(&mk(1.0), 20, 0x5EED);
        assert!(
            on.mean_transforms > off.mean_transforms,
            "the transform is faster with the buff already up: {} -> {} transforms",
            off.mean_transforms, on.mean_transforms
        );
        // AND THIS FIXTURE NEVER RELOADS AT ALL, which is the scenario stated
        // rather than a flaw in it: the gauge fills on the shot that empties the
        // magazine, so the weapon transforms instead of reloading, every time.
        // The reload half of the perk has its own test above.
        assert_eq!(off.mean_reloads, 0.0, "the fixture transforms rather than reloading");
    }

    /// A RELOAD IS PAID FOR WHILE IT RUNS, and the bonus composes with the
    /// static bucket rather than replacing it.
    ///
    /// The lapsing-window case this test used to cover is gone with the window:
    /// Ready Retaliation lasts exactly as long as the reload it starts, so
    /// nothing can run out halfway through any more (owner, 2026-08-11). What
    /// is left is the arithmetic that was always the point — mods worth +100%
    /// already halve a 4 s reload to the 2 s that arrives here, and the perk's
    /// +100% on top makes it 4/(1+1+1) of the unmodded four rather than 1 s.
    #[test]
    fn a_reload_bonus_composes_with_the_bucket_it_joins() {
        let plain = reload_span(4.0, 0.0, 0.0);
        assert!((plain - 4.0).abs() < 1e-9, "no bonus at all: {plain}");

        let doubled = reload_span(4.0, 0.0, 1.0);
        assert!((doubled - 2.0).abs() < 1e-9, "+100% on an unmodded reload: {doubled}");

        let stacked = reload_span(2.0, 1.0, 1.0);
        assert!((stacked - 4.0 / 3.0).abs() < 1e-9, "on top of +100% of mods: {stacked}");
    }

    /// A BATTERY REFILLS BETWEEN SHOTS, and slowing the weapon enough removes
    /// its reload entirely.
    ///
    /// The Shedu's numbers (wiki, verbatim in `weapons_data::Battery`): a
    /// 7-round battery, 28 rounds a second, a 0.4 s delay with rounds left.
    /// Only the part of the gap BEYOND the delay pays, so the weapon breaks
    /// even at `0.4 + 1/28 = 0.4357 s` a shot — **2.295 rounds a second**,
    /// 8.2% under its listed 2.50.
    ///
    /// That margin is the whole point and it is why this is not a
    /// differently-spelled reload: the listed rate is 0.036 s above break-even,
    /// so a single fire-rate penalty crosses it and the reload disappears.
    /// A weapon getting strictly better from a NEGATIVE mod is a claim that has
    /// to be asserted rather than described.
    #[test]
    fn a_battery_refills_between_shots_and_a_slow_enough_one_never_reloads() {
        let shedu = |fire_rate: f64| DummyParams {
            fire_rate,
            magazine_size: 7.0,
            reload_seconds: 1.25, // = 1.0 s delay + 7/28 s refill
            duration_secs: 60.0,
            body_parts: mono_body(1.0),
            battery: Some(crate::weapons_data::Battery {
                regen_per_second: 28.0,
                delay_empty_s: 1.0,
                delay_partial_s: 0.4,
            }),
            ..no_status()
        };
        // AT the listed rate the gap IS the delay: nothing regenerates, and the
        // battery runs dry every seven rounds exactly as a magazine would.
        let listed = run_once(&shedu(2.5), &mut Rng::new(4));
        assert!(listed.reloads > 0, "at the listed rate it must still reload");

        // ABOVE break-even it still drains, just slowly — at 2.35/s the gap
        // returns 0.715 rounds against the 1.0 it spends, so the battery lasts
        // 24.6 shots instead of 7. The mechanic is a SLOPE, not a switch, and a
        // test that only tried the two extremes would not have said so.
        //
        // (2.30/s drains too, at 0.026 rounds a shot — 268 of them, which is
        // 116 s and does not fit this fixture's minute. Worth recording: the
        // approach to break-even is asymptotic, so "does it reload" stops being
        // a question about the weapon and becomes one about the clock.)
        assert!(run_once(&shedu(2.35), &mut Rng::new(4)).reloads > 0);

        // JUST BELOW IT (2.25/s) the reload is gone for good.
        let slow = run_once(&shedu(2.25), &mut Rng::new(4));
        assert_eq!(slow.reloads, 0, "a battery under break-even must never empty");

        // …and it is worth REAL rounds: 10% slower and no downtime at all beats
        // the listed rate over a minute.
        assert!(
            slow.shots > listed.shots,
            "slowed {} shots against {} at the listed rate",
            slow.shots, listed.shots
        );

        // THE MECHANIC IS THE DIFFERENCE, not the numbers: the same weapon with
        // no battery reloads at either rate.
        let plain = DummyParams { battery: None, ..shedu(2.25) };
        assert!(run_once(&plain, &mut Rng::new(4)).reloads > 0);
    }

    /// A SPOOL THAT CLIMBS is the same arithmetic pointed the other way, and it
    /// costs a magazine's worth of time rather than a magazine's worth of rounds.
    ///
    /// The Gorgon's numbers — 12.5 rounds/s from 20%, full on the 9th shot
    /// (wiki). Its 90-round magazine takes 7.99 s instead of 7.20 s, i.e. the
    /// spool costs **11% of the time** to fire one magazine, and it is paid once
    /// per magazine rather than once per fight because a reload is a pause.
    ///
    /// This runs beside the faller deliberately: one `spool_factor` serves both,
    /// and the day it stops serving both, one of these two fails.
    #[test]
    fn a_spool_that_climbs_costs_the_first_shots_of_every_magazine() {
        let gorgon = DummyParams {
            fire_rate: 12.5,
            magazine_size: 100_000.0, // one long burst: the climb, uninterrupted
            duration_secs: 10.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        assert_eq!(run_once(&gorgon, &mut Rng::new(1)).shots, 125, "the listed rate, flat");

        let spooled = DummyParams {
            sustained_fire_rate: Some(crate::weapons_data::SustainedFireRate {
                start: 0.20,
                end: 1.00,
                over_shots: 7.5,
            }),
            ..gorgon.clone()
        };
        assert_eq!(run_once(&spooled, &mut Rng::new(1)).shots, 116);

        // ONCE PER MAGAZINE, NOT ONCE PER FIGHT. Nine shots of climb out of 90
        // is 11% of the time; out of a magazine of 9 it would be most of it. The
        // same derived reset the faller uses does both, with no rule of its own.
        let small = DummyParams { magazine_size: 9.0, reload_seconds: 0.0001, ..spooled.clone() };
        let small_flat = DummyParams { sustained_fire_rate: None, ..small.clone() };
        let a = f64::from(run_once(&small, &mut Rng::new(1)).shots);
        let b = f64::from(run_once(&small_flat, &mut Rng::new(1)).shots);
        assert!(a < b * 0.75, "a 9-round magazine is all climb: {a} vs {b}");
    }

    /// The floor is a fraction of the LIVE rate, so a fire-rate mod raises both
    /// ends and never buys its way out of the spool. Rapid Wrath's +20% is
    /// worth +20% at the floor as well as at the ceiling — which is also why
    /// the spool cannot be folded into the listed stat.
    #[test]
    fn a_fire_rate_bonus_scales_the_spooled_rate_too() {
        let spooled = DummyParams {
            fire_rate: 13.33,
            sustained_fire_rate: Some(crate::weapons_data::SustainedFireRate {
                start: 1.00,
                end: 0.60,
                over_shots: 51.0,
            }),
            magazine_size: 100_000.0,
            duration_secs: 60.0, // long past the 51 shots, so the floor dominates
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let faster = DummyParams { fire_rate: 13.33 * 1.2, ..spooled.clone() };
        let slow = f64::from(run_once(&spooled, &mut Rng::new(1)).shots);
        let fast = f64::from(run_once(&faster, &mut Rng::new(1)).shots);
        assert!((fast / slow - 1.2).abs() < 0.01, "{fast} / {slow}");
    }

    fn single_part(part: BodyPart) -> DummyParams {
        DummyParams {
            body_parts: vec![part],
            ..no_status()
        }
    }

    /// A BOW paces on its draw alone, not on `1 / fire_rate`. Cernos Prime's
    /// numbers: 0.5 s draw + 0.65 s reload of its single nocked arrow = 1.15 s
    /// a shot, against the 1.65 s the fire-rate stat alone would give. The
    /// stat itself is untouched — it is what fire-rate GATES read.
    ///
    /// `DrawOnly` is stated because it is the exception: every OTHER charge
    /// weapon adds the listed rate's interval to the draw (see
    /// `a_general_charge_weapon_pays_the_draw_AND_the_rate`).
    #[test]
    fn a_charge_weapon_paces_on_the_draw_not_the_fire_rate() {
        let bow = DummyParams {
            fire_rate: 1.0,
            charge_seconds: Some(0.5),
            charge_cadence: crate::weapons_data::ChargeCadence::DrawOnly,
            magazine_size: 1.0,
            reload_seconds: 0.65,
            duration_secs: 10.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        // Shots at 0, 1.15, 2.30 … 9.20 — nine of them inside 10 s.
        let r = run_once(&bow, &mut Rng::new(1));
        assert_eq!(r.shots, 9, "10 s / 1.15 s + 1");

        // The same weapon read as an ordinary 1.0 fire-rate gun: 1.65 s a
        // shot, seven shots. This is what the roster did before charge times.
        let as_rate = DummyParams { charge_seconds: None, ..bow.clone() };
        assert_eq!(run_once(&as_rate, &mut Rng::new(1)).shots, 7);

        // A TAPPED bow: no draw to pay, so the 0.65 s nock is the whole cycle
        // (wiki Fire Rate's bow formula with a zero charge term). Shots at 0,
        // 0.65, 1.30 … 9.75 — sixteen inside 10 s, against the charged form's
        // nine, for half the damage each.
        let tapped = DummyParams { charge_seconds: Some(0.0), ..bow.clone() };
        assert_eq!(run_once(&tapped, &mut Rng::new(1)).shots, 16, "10 s / 0.65 s + 1");

        // `fire_rate` here is the RESOLVED stat and `charge_seconds` the
        // RESOLVED draw — the panel already spent the mod bucket on both, so
        // raising the stat alone must NOT shorten the draw a second time.
        // Only a live in-sim buff does, and it divides by its own factor.
        let stat_only = DummyParams { fire_rate: 2.0, ..bow.clone() };
        assert_eq!(run_once(&stat_only, &mut Rng::new(1)).shots, 9, "mods are not re-applied");
    }

    /// A target that is nothing but head, so every pellet headshots.
    pub(super) fn all_head() -> Vec<BodyPart> {
        vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: true,
            crit_bonus: false,
        }]
    }

    pub(super) fn mono_body(multiplier: f64) -> Vec<BodyPart> {
        vec![BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier,
            is_head: false,
            crit_bonus: false,
        }]
    }

    #[test]
    fn faction_mult_scales_direct_damage_linearly() {
        // Status off: only the direct-hit multiply applies (no DoT double-dip),
        // and faction_mult is applied AFTER the RNG rolls, so the same seed
        // yields damage scaled exactly by faction_mult.
        let plain = single_part(BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        });
        let boosted = DummyParams {
            faction_mult: 1.30,
            ..plain.clone()
        };
        let a = monte_carlo(&plain, 3000, 7);
        let b = monte_carlo(&boosted, 3000, 7);
        assert!(
            (b.mean_damage / a.mean_damage - 1.30).abs() < 1e-9,
            "ratio was {}",
            b.mean_damage / a.mean_damage
        );
    }

    #[test]
    fn faction_bonus_applies_only_vs_matching_target_faction() {
        use crate::loadout::{
            resolve, Faction, ModDef, ModEffect, Rarity, StackPolicy, WeaponBase,
        };
        use crate::mods::Polarity;
        let expel = ModDef {
            exclusive_to: &[],
            unmodeled: false,
            out_of_scope: false,
            id: "expel_grineer",
            name: "expel_grineer",
            base_drain: 9,
            max_rank: 5,
            polarity: Polarity::Madurai,
            rarity: Rarity::Uncommon,
            exilus: false,
            family: None,
            requires_weapon: None,
            excludes_weapon: Vec::new(),
            set: None,
            requires: None,
            disables: Vec::new(),
            effects: vec![ModEffect::FactionDamage(Faction::Grineer, 0.30)],
        };
        let base = WeaponBase::from_data(
            "dual_toxocyst",
            true,
            &[
                "dual_toxocyst_commodores_fortune",
                "dual_toxocyst_evolved_autoloader",
                "dual_toxocyst_fevered_frenzy",
            ],
        );
        let panel = resolve(&base, &[&expel], StackPolicy::AssumedMax);
        let parts = mono_body(1.0);
        let grineer_target = {
            let mut t = TargetParams::training_dummy();
            t.faction = Faction::Grineer;
            t
        };
        let arena = |target, body_parts| crate::arena::Arena {
            target,
            body_parts,
            ..crate::arena::Arena::training(10.0)
        };
        let vs_grineer = DummyParams::from_panel(&panel, &arena(grineer_target, parts.clone()), &ArcaneFx::none());
        let vs_other = DummyParams::from_panel(&panel, &arena(TargetParams::training_dummy(), parts), &ArcaneFx::none());
        assert!(
            (vs_grineer.faction_mult - 1.30).abs() < 1e-9,
            "grineer {}",
            vs_grineer.faction_mult
        );
        assert!(
            (vs_other.faction_mult - 1.0).abs() < 1e-9,
            "unknown {}",
            vs_other.faction_mult
        );
    }

    #[test]
    fn ten_shots_in_ten_seconds_at_one_per_second() {
        let s = monte_carlo(&DummyParams::default(), 100, 1);
        assert!((s.mean_shots - 10.0).abs() < 1e-9);
    }

    /// The set promotes a hit that ALREADY crit, and only that one — the wiki
    /// is explicit that it "triggers exclusively on critical hits", so a
    /// normal hit can never be turned into one.
    #[test]
    fn the_set_bonus_promotes_only_a_hit_that_already_crit() {
        let mut rng = Rng::new(7);
        assert_eq!(upgrade_crit_tier(0, 1.0, &mut rng), 0, "a normal hit stays normal");
        assert_eq!(upgrade_crit_tier(1, 1.0, &mut rng), 2, "yellow -> orange");
        assert_eq!(upgrade_crit_tier(2, 1.0, &mut rng), 3, "orange -> red");
        assert_eq!(upgrade_crit_tier(1, 0.0, &mut rng), 1, "no set, no promotion");
        // At 20% (all four primary members) roughly a fifth of crits move up.
        let n = 20_000;
        let up = (0..n).filter(|_| upgrade_crit_tier(1, 0.20, &mut rng) == 2).count();
        let f = up as f64 / n as f64;
        assert!((f - 0.20).abs() < 0.02, "promoted {f:.3} of crits, expected ~0.20");
    }

    #[test]
    fn monte_carlo_is_deterministic() {
        let a = monte_carlo(&DummyParams::default(), 500, 12345);
        let b = monte_carlo(&DummyParams::default(), 500, 12345);
        assert_eq!(a.mean_damage, b.mean_damage);
        assert_eq!(a.std_damage, b.std_damage);
    }

    #[test]
    fn headshot_rate_is_about_half() {
        let s = monte_carlo(&DummyParams::default(), 1000, 999);
        assert!((s.mean_headshot_rate - 0.5).abs() < 0.02);
    }

    #[test]
    fn produces_positive_damage() {
        let s = monte_carlo(&DummyParams::default(), 1000, 7);
        assert!(s.mean_damage > 0.0);
        assert!(s.dps > 0.0);
    }

    #[test]
    fn mean_damage_matches_hand_computed_expectation_without_status() {
        // Status off, 10 shots: Enervate ramps cc = 5%,15%,...,95% (sum 5.0).
        // Per shot: E = 0.5*75*(1+cc) + 0.5*(75*3)*(1+3cc) = 150 + 375*cc,
        // so E[total] = 10*150 + 375*5.0 = 3375. (The Dual Toxocyst vector
        // quantizes to a total of exactly 75.)
        let s = monte_carlo(&no_status(), 2000, 42);
        assert!(
            (s.mean_damage - 3375.0).abs() / 3375.0 < 0.02,
            "mean damage was {}",
            s.mean_damage
        );
        assert_eq!(s.mean_dot_damage, 0.0);
        assert_eq!(s.mean_procs, 0.0);
    }

    #[test]
    fn multishot_doubles_pellets_and_damage_deterministically() {
        // Multishot 2.0: every pull fires exactly 2 pellets, each its own
        // instance. crit 1.0, mono body, no status: 10 pulls x 2 x 75 = 1500,
        // 20 pellets, ammo still 10 (one per pull -> no reload at mag 12).
        let p = DummyParams {
            crit_multiplier: 1.0,
            multishot: 2.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 50, 6);
        assert!(
            (s.mean_damage - 1500.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
        assert!((s.mean_pellets - 20.0).abs() < 1e-9);
        assert!((s.mean_shots - 10.0).abs() < 1e-9);
        assert_eq!(s.mean_reloads, 0.0);
    }

    /// Final Fusillade: +3 multishot on the LAST round of the magazine, and on
    /// no other. A 5-round magazine fired dry gives four 1-pellet pulls and one
    /// 4-pellet pull — 8 pellets, not 5 (gate never fires) and not 20 (gate
    /// always fires).
    #[test]
    fn final_fusillade_adds_multishot_only_on_the_magazines_last_round() {
        let p = |bonus: f64| DummyParams {
            multishot: 1.0,
            multishot_on_last_round: bonus,
            fire_rate: 1.0,
            magazine_size: 5.0,
            // Exactly one magazine: dry reserves stop the run rather than
            // reloading into a second one, so the counts are exact.
            infinite_reserve: false,
            reserve_ammo: 0.0,
            duration_secs: 10.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let off = monte_carlo(&p(0.0), 4, 3);
        let on = monte_carlo(&p(3.0), 4, 3);
        assert!((off.mean_shots - 5.0).abs() < 1e-9, "shots {}", off.mean_shots);
        assert!((on.mean_shots - off.mean_shots).abs() < 1e-9, "cadence must not change");
        assert!((off.mean_pellets - 5.0).abs() < 1e-9, "baseline {}", off.mean_pellets);
        assert!((on.mean_pellets - 8.0).abs() < 1e-9, "boosted {}", on.mean_pellets);
        assert_eq!(off.mean_reloads, 0.0);
    }

    /// Plentiful Mayhem, discrete branch: "only applies to projectiles
    /// GENERATED BY multishot". Two pellets a pull means ONE plain and ONE at
    /// x1.6, so a pull deals 2.6 pellet-units where it used to deal 2.0 — not
    /// 3.2, which is what treating it as a weapon-wide bonus would give.
    #[test]
    fn plentiful_mayhem_pays_only_the_multishot_generated_projectiles() {
        let p = |bonus: f64| DummyParams {
            multishot: 2.0,
            multishot_ammo_bonus: bonus,
            // No crit variance: every pellet must deal the SAME number, or the
            // ratio below would depend on which pellet happened to crit.
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let off = monte_carlo(&p(0.0), 4, 3);
        let on = monte_carlo(&p(0.6), 4, 3);
        assert!(
            (on.mean_pellets - off.mean_pellets).abs() < 1e-9,
            "the perk must not change the pellet count"
        );
        let ratio = on.mean_damage / off.mean_damage;
        assert!((ratio - 2.6 / 2.0).abs() < 1e-9, "ratio {ratio}");
        // With NO multishot source there is no generated projectile at all, so
        // the perk is worth exactly nothing — the wiki's rule, stated as a test.
        let solo = |bonus: f64| DummyParams { multishot: 1.0, ..p(bonus) };
        let (a, b) = (monte_carlo(&solo(0.0), 4, 3), monte_carlo(&solo(0.6), 4, 3));
        assert!((a.mean_damage - b.mean_damage).abs() < 1e-9, "inert at 1x multishot");
    }

    /// …and it follows the generated grenade into the CLOUD it leaves (user,
    /// 2026-07-30). One pull, two grenades, ten ticks each: 400 plain + 640
    /// boosted against a 800 baseline. That is where the perk's value is on
    /// this weapon — the cloud is most of its damage.
    #[test]
    fn plentiful_mayhem_follows_the_generated_grenade_into_its_cloud() {
        let p = |bonus: f64| DummyParams {
            damage: DamageVector::default(), // inert impact: the fields alone
            lingering: Some(cloud(crate::loadout::FieldStacking::Stack)),
            multishot: 2.0,
            multishot_ammo_bonus: bonus,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            fire_rate: 1.0,
            // Exactly one pull — a one-round magazine behind a reload longer
            // than the run — then a 60 s window so both clouds finish. Reserves
            // stay INFINITE so the generated grenade can always pay its round;
            // starving it is the next test, and it must not leak into this one.
            magazine_size: 1.0,
            reload_seconds: 999.0,
            infinite_reserve: true,
            duration_secs: 60.0,
            ..no_status()
        };
        let off = monte_carlo(&p(0.0), 4, 3);
        let on = monte_carlo(&p(0.6), 4, 3);
        assert!((off.mean_shots - 1.0).abs() < 1e-9, "shots {}", off.mean_shots);
        assert!(
            (off.mean_field_ticks - 20.0).abs() < 1e-9,
            "two grenades, ten ticks each: {}",
            off.mean_field_ticks
        );
        assert!(
            (on.mean_field_ticks - off.mean_field_ticks).abs() < 1e-9,
            "a damage bonus must not change the tick COUNT"
        );
        assert!((off.mean_damage - 800.0).abs() < 1e-9, "baseline {}", off.mean_damage);
        assert!(
            (on.mean_damage - (400.0 + 640.0)).abs() < 1e-9,
            "boosted {} (expected 400 plain + 640 from the generated grenade)",
            on.mean_damage
        );
    }

    /// Ammo efficiency is a DIVIDED COST and the magazine keeps the fraction —
    /// ✅ measured (MEASUREMENTS M14). A 5-round magazine at 75% efficiency
    /// takes 20 shots, because each costs 0.25.
    #[test]
    fn ammo_efficiency_divides_the_cost_and_the_magazine_keeps_the_fraction() {
        let p = |eff: f64| DummyParams {
            arcane: ArcaneFx { ammo_efficiency: eff, ..ArcaneFx::none() },
            ammo_efficiency_applies: true,
            fire_rate: 1.0,
            magazine_size: 5.0,
            // One magazine only: dry reserves stop the run instead of
            // reloading, so the shot count is exactly what the magazine bought.
            infinite_reserve: false,
            reserve_ammo: 0.0,
            duration_secs: 100.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let plain = monte_carlo(&p(0.0), 4, 3);
        assert!((plain.mean_shots - 5.0).abs() < 1e-9, "shots {}", plain.mean_shots);
        let eff = monte_carlo(&p(0.75), 4, 3);
        assert!(
            (eff.mean_shots - 20.0).abs() < 1e-9,
            "5 rounds at 0.25 each = 20 shots, got {}",
            eff.mean_shots
        );
    }

    /// …and an overdraw's DEBT survives the reload — ✅ measured (M14, user's
    /// in-game run on a 5-round magazine): 3 buffed shots leave 4.25, five
    /// full-cost shots take that to −0.75 and trigger the reload, and the fresh
    /// magazine comes back at 4.25, not 5. The tell in game is the UI, which
    /// shows the CEILING: one more 0.25 shot moves 4.25 to exactly 4.00 and the
    /// readout drops 5 -> 4, which a clean 5.00 magazine could never do.
    #[test]
    fn an_efficiency_overdraw_carries_its_debt_through_the_reload() {
        // 60% efficiency = 0.4 a shot, chosen because it does NOT divide a
        // 5-round magazine evenly — which is the only way the two models can
        // be told apart:
        //   magazine 1: 13 shots take 5.0 to -0.2, then reload.
        //   carry    -> 5.0 + (-0.2) = 4.8, which buys exactly 12 more = 25.
        //   reset    -> a clean 5.0, which buys 13 more = 26.
        let p = DummyParams {
            arcane: ArcaneFx { ammo_efficiency: 0.6, ..ArcaneFx::none() },
            ammo_efficiency_applies: true,
            fire_rate: 1.0,
            magazine_size: 5.0,
            reserve_ammo: 5.0, // exactly one reload's worth, then dry
            infinite_reserve: false,
            reload_seconds: 0.001,
            duration_secs: 100.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 4, 3);
        assert!((s.mean_reloads - 1.0).abs() < 1e-9, "reloads {}", s.mean_reloads);
        assert!(
            (s.mean_shots - 25.0).abs() < 1e-9,
            "expected 25 shots (13 + 12, the debt carried); 26 would mean the \
             reload wiped it. got {}",
            s.mean_shots
        );
    }

    /// A LOCK IS ABSOLUTE, AND THE SIM OWNS HALF OF IT (user, 2026-08-04:
    /// "应该要锁定的，好像没锁").
    ///
    /// "Equipping this mod will set weapon's Fire Rate to its default ignoring
    /// other bonuses, EVEN NEGATIVE EFFECTS" (wiki, Semi-Rifle/Shotgun/Pistol
    /// Cannonade); Primary and Pistol Acuity say the same of Multishot.
    /// `resolve` empties the mod bucket, but the weapon's own Frenzy passive is
    /// a x2.5 in the BUFF BAR and an arcane's multishot is added per shot — both
    /// past the resolver, both surviving a lock that stopped at the bucket.
    ///
    /// The measurable consequence, and why it is worth a test rather than a
    /// comment: on Dual Toxocyst a Cannonade build kept Frenzy's x2.5 cadence,
    /// so the sim reported roughly two and a half times the shots the game can
    /// fire.
    #[test]
    fn a_locked_stat_ignores_the_live_sources_too() {
        let head = vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: true,
        }];
        // ---- FIRE RATE: the buff bar's Frenzy multiplier.
        let fr = |frenzy: bool, locked: bool| {
            let p = DummyParams {
                frenzy,
                locked_stats: if locked { vec!["fire_rate"] } else { Vec::new() },
                fire_rate: 4.0,
                magazine_size: 1e9, // no reload to blur the cadence
                body_parts: head.clone(),
                duration_secs: 60.0,
                ..no_status()
            };
            monte_carlo(&p, 6, 5).mean_shots
        };
        let bare = fr(false, false);
        assert!(fr(true, false) > bare * 1.5, "Frenzy pays when nothing locks it");
        assert!(
            (fr(true, true) - bare).abs() < 1e-9,
            "under the lock the weapon fires at its DEFAULT cadence: {} vs {bare}",
            fr(true, true)
        );
        // ...and locking it changes nothing on a build that had no buff to lose,
        // so the assertion above is about the lock and not about the flag.
        assert!((fr(false, true) - bare).abs() < 1e-9);

        // ---- MULTISHOT: an arcane's live stacks (Primary Overcharge, a
        // `Passive` trigger — simply ON, so it needs no event to arm).
        let mut tenno = crate::tenno_data::default_tenno().clone();
        tenno.energy = 1000.0;
        tenno.state.energy_pct = 1.0;
        let over = crate::arcanes_data::for_slot("primary", "primary_overcharge")
            .expect("primary_overcharge");
        let fx = over.fx(5, crate::loadout::StackPolicy::Emergent, &[], &tenno);
        assert!(!fx.buffs.is_empty(), "a 1,000-energy frame arms it");
        let ms = |locked: bool| {
            let p = DummyParams {
                arcane: fx.clone(),
                locked_stats: if locked { vec!["multishot"] } else { Vec::new() },
                multishot: 1.0,
                base_multishot: 1.0,
                magazine_size: 1e9,
                body_parts: head.clone(),
                duration_secs: 30.0,
                ..no_status()
            };
            // Damage stands in for the pellet count: the cadence is untouched,
            // so a run's damage is proportional to the pellets each pull rolls.
            monte_carlo(&p, 6, 9).mean_damage
        };
        let (open, locked) = (ms(false), ms(true));
        assert!(open > locked * 2.0, "+350% multishot is 4.5 pellets a pull: {open} vs {locked}");
        // The locked run is the weapon's DEFAULT pellet count — the same number a
        // build with no arcane at all fires.
        let none = {
            let p = DummyParams {
                // NO arcane at all — the fixture's own would otherwise be the
                // difference being measured.
                arcane: crate::arcanes_data::ArcaneFx::none(),
                multishot: 1.0,
                base_multishot: 1.0,
                magazine_size: 1e9,
                body_parts: head.clone(),
                duration_secs: 30.0,
                ..no_status()
            };
            monte_carlo(&p, 6, 9).mean_damage
        };
        assert!((locked - none).abs() < 1e-6, "{locked} vs {none}");
    }

    /// A FREE shot needs no round in the magazine (user, 2026-07-30). At 100%
    /// ammo efficiency the cost is zero, so an empty magazine is not a reason to
    /// reload — the Dual Toxocyst case, where the last round headshots, the
    /// magazine lands on 0 and that same kill arms Frenzy.
    #[test]
    fn a_zero_cost_shot_fires_off_an_empty_magazine_instead_of_reloading() {
        // A ONE-round magazine is what puts the boundary in reach: shot 1 is
        // taken before Frenzy exists, so it pays full price and lands the
        // magazine on exactly 0 — and that same headshot arms the +100%
        // efficiency. Every later shot is free, so the magazine must never be
        // refilled. A static 100% efficiency could not test this: the magazine
        // would never reach 0 in the first place.
        let head = vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: true,
        }];
        let p = |frenzy: bool| DummyParams {
            frenzy,
            magazine_size: 1.0,
            // One spare round, so a reload is possible AND countable if the
            // gate wrongly fires.
            infinite_reserve: false,
            reserve_ammo: 1.0,
            body_parts: head.clone(),
            ..no_status()
        };
        let with = monte_carlo(&p(true), 20, 4);
        assert_eq!(
            with.mean_reloads, 0.0,
            "a free shot must fire off the empty magazine, not reload"
        );
        // Without Frenzy every shot costs a round, so the same fixture spends
        // its one reserve round on a reload and then runs dry at 2 shots —
        // which is exactly what the old gate did even WITH Frenzy.
        let without = monte_carlo(&p(false), 20, 4);
        assert!((without.mean_reloads - 1.0).abs() < 1e-9, "reloads {}", without.mean_reloads);
        assert!((without.mean_shots - 2.0).abs() < 1e-9, "shots {}", without.mean_shots);
        assert!(
            with.mean_shots > without.mean_shots,
            "free shots keep firing: {} vs {}",
            with.mean_shots,
            without.mean_shots
        );
    }

    /// Ammo efficiency CAPS at 100% (user, 2026-07-30): a shot can cost
    /// nothing, never less. Stacking past the cap buys nothing and must never
    /// start refunding ammo — a magazine cannot grow while the weapon fires.
    #[test]
    fn ammo_efficiency_caps_at_free_and_never_refunds() {
        // The pure function first, since that is where the ceiling lives.
        assert_eq!(ammo_efficiency(true, 1.0, 1.0, 1.0), 1.0, "3x over the cap");
        assert_eq!(ammo_efficiency(true, 0.0, 0.0, 0.0), 0.0);
        assert_eq!(ammo_efficiency(false, 1.0, 1.0, 1.0), 0.0, "charge-backed is exempt");

        // End to end: an absurd stack behaves exactly like a plain 100%. Note
        // this half cannot catch a silent refund on its own — a magazine that
        // grows is not reported anywhere, so nothing downstream would move.
        // The assertions above are the real guard; this one pins that going
        // over the cap does not disturb the cadence.
        let p = |eff: f64| DummyParams {
            arcane: ArcaneFx { ammo_efficiency: eff, ..ArcaneFx::none() },
            ammo_efficiency_applies: true,
            fire_rate: 1.0,
            magazine_size: 3.0,
            infinite_reserve: false,
            reserve_ammo: 0.0, // no reserve at all: a refund would be visible
            duration_secs: 20.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let exact = monte_carlo(&p(1.0), 4, 3);
        let over = monte_carlo(&p(5.0), 4, 3);
        assert!((exact.mean_shots - 20.0).abs() < 1e-9, "shots {}", exact.mean_shots);
        assert!(
            (over.mean_shots - exact.mean_shots).abs() < 1e-9,
            "over-cap must behave as exactly free: {} vs {}",
            over.mean_shots,
            exact.mean_shots
        );
        assert_eq!(over.mean_reloads, 0.0);
    }

    /// Plentiful Mayhem on the real Incarnon numbers — ✅ measured (user,
    /// 2026-07-30, two different multishot values): the 170-charge pool at 8
    /// ticks per second lasts **170 / 8 / multishot** seconds, against 170/8 =
    /// 21.25 s without the perk. That is the whole cost of the +60%.
    #[test]
    fn plentiful_mayhem_shortens_the_incarnon_window_by_the_multishot_factor() {
        // torid_incarnon.yaml: pseudo_reload.magazine 170, attack.fire_rate 8
        // (ticks per second, trigger "held").
        const CHARGES: f64 = 170.0;
        const TICK_RATE: f64 = 8.0;
        let p = |ms: f64, bonus: f64| DummyParams {
            continuous: true,
            fire_rate: TICK_RATE,
            multishot: ms,
            base_multishot: 1.0,
            multishot_ammo_bonus: bonus,
            magazine_size: CHARGES,
            // The charge pool is outside the ammo economy: no reserve behind
            // it, and no efficiency reaches it.
            ammo_efficiency_applies: false,
            infinite_reserve: false,
            reserve_ammo: 0.0,
            duration_secs: 120.0,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            ..no_status()
        };
        // Without the perk the window is multishot-independent: a merged beam
        // still bills ONE charge a tick however many beams it merges.
        for ms in [1.0, 2.0, 5.0] {
            let s = monte_carlo(&p(ms, 0.0), 4, 3);
            assert!(
                (s.mean_shots - CHARGES).abs() < 1e-9,
                "no perk at {ms}x: expected {CHARGES} ticks, got {}",
                s.mean_shots
            );
        }
        // With it, every projectile bills a charge, so the window divides.
        for ms in [1.0, 2.0, 5.0] {
            let s = monte_carlo(&p(ms, 0.6), 4, 3);
            let want = CHARGES / ms;
            assert!(
                (s.mean_shots - want).abs() < 1e-9,
                "{ms}x multishot: expected {want} ticks, got {}",
                s.mean_shots
            );
            // …and that is the user's 170/8/multishot, stated as seconds.
            let seconds = s.mean_shots / TICK_RATE;
            assert!(
                (seconds - CHARGES / TICK_RATE / ms).abs() < 1e-9,
                "{ms}x multishot: expected {} s, got {seconds}",
                CHARGES / TICK_RATE / ms
            );
        }
    }

    /// Ammo efficiency serves the MAGAZINE round only — it never reaches
    /// Plentiful Mayhem's multishot surcharge (✅ measured, user 2026-07-30).
    /// So 100% efficiency makes the shot itself free while every generated
    /// projectile still pays full price out of reserve, and the two pools
    /// empty independently.
    #[test]
    fn ammo_efficiency_does_not_pay_for_plentiful_mayhems_extra_projectiles() {
        let p = DummyParams {
            arcane: ArcaneFx { ammo_efficiency: 1.0, ..ArcaneFx::none() },
            ammo_efficiency_applies: true,
            multishot: 3.0, // 1 magazine round + 2 surcharged extras
            multishot_ammo_bonus: 0.6,
            fire_rate: 1.0,
            magazine_size: 5.0,
            infinite_reserve: false,
            // Exactly two shots' worth of extras, so the starvation boundary
            // lands inside the window and is visible.
            reserve_ammo: 4.0,
            duration_secs: 5.0,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 4, 3);
        // The magazine round is free, so the weapon never reloads and fires
        // the whole window: 5 shots at 1/s.
        assert!((s.mean_shots - 5.0).abs() < 1e-9, "shots {}", s.mean_shots);
        assert_eq!(s.mean_reloads, 0.0, "a free magazine round never reloads");
        // Reserve pays for the extras at FULL price: 2 + 2, then it is dry and
        // the remaining shots fire alone. 3 + 3 + 1 + 1 + 1 = 9 pellets.
        // Were efficiency to reach the surcharge, all five shots would carry
        // three pellets for 15.
        assert!(
            (s.mean_pellets - 9.0).abs() < 1e-9,
            "expected 9 pellets (3+3+1+1+1); 15 would mean efficiency paid for \
             the extras. got {}",
            s.mean_pellets
        );
    }

    /// CO on an AoE part is the EXCEPTION. The mods say Condition Overload
    /// boosts DIRECT hits, so a field gets nothing unless its weapon declares
    /// otherwise — the Torid's cloud does, and the CO catalog gives it a row
    /// precisely because an AoE part taking CO is not supposed to happen (user,
    /// 2026-07-30). This pins the DEFAULT, which no roster weapon exercises yet
    /// and which the engine used to get wrong for every field.
    #[test]
    fn a_field_takes_condition_overload_only_where_the_weapon_declares_it() {
        let p = |takes: bool| DummyParams {
            // Zero-damage impact that still forces an IMPACT proc: the target
            // carries one status TYPE for the CO bracket to count, and a
            // stagger adds no damage of its own to confound the field's total.
            damage: DamageVector::default(),
            forced_procs: vec![DamageType::Impact],
            lingering: Some(crate::loadout::ResolvedLingering {
                takes_condition_overload: takes,
                ..cloud(crate::loadout::FieldStacking::Stack)
            }),
            co_per_type: 1.0,
            co_behavior: crate::loadout::CoBehavior::Independent,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            fire_rate: 1.0,
            magazine_size: 1.0,
            reload_seconds: 999.0,
            duration_secs: 60.0,
            ..no_status()
        };
        let off = monte_carlo(&p(false), 4, 3);
        let on = monte_carlo(&p(true), 4, 3);
        assert!(
            (off.mean_field_ticks - on.mean_field_ticks).abs() < 1e-9,
            "the flag must move damage, not tick counts"
        );
        assert!(
            on.mean_damage > off.mean_damage + 1e-9,
            "declaring it must be worth something: {} vs {}",
            on.mean_damage,
            off.mean_damage
        );
        // The un-declared field is the plain 10 x 40 with no CO bracket at all.
        assert!(
            (off.mean_damage - 400.0).abs() < 1e-9,
            "expected a bare 400, got {}",
            off.mean_damage
        );
    }

    /// …and the same for an EXPLOSION. The mods forbid it — CO boosts direct
    /// hits — but the engine supports the case anyway, because the CO catalog
    /// lists entries that do it: the Zylok's Incarnon radial receives CO "on
    /// target directly hit by bullet", which the arena always is. No roster
    /// weapon declares it, so this test is the only thing holding the branch
    /// open.
    #[test]
    fn a_radial_takes_condition_overload_only_where_the_weapon_declares_it() {
        let radial = |takes: bool| crate::loadout::ResolvedRadial {
            damage: {
                let mut d = DamageVector::default();
                d.set(DamageType::Radiation, 100.0);
                d
            },
            modified_base: 100.0,
            crit_chance: 0.0,
            crit_damage: 1.0,
            base_crit_chance: 0.0,
            base_crit_damage: 1.0,
            status_chance: 0.0,
            base_status_chance: 0.0,
            radius_m: 2.0,
            falloff_start_m: 0.0,
            falloff_reduction: 0.0,
            takes_condition_overload: takes,
            takes_multishot: true,
            co_base_fraction: 1.0,
        };
        // Zero-damage direct hit that still forces an Impact proc, so the only
        // damage reported is the explosion's and the target carries one status
        // type for CO to count.
        let p = |takes: bool| DummyParams {
            damage: DamageVector::default(),
            forced_procs: vec![DamageType::Impact],
            radial: Some(radial(takes)),
            co_per_type: 1.0,
            co_behavior: crate::loadout::CoBehavior::Independent,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            fire_rate: 1.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let off = monte_carlo(&p(false), 4, 3);
        let on = monte_carlo(&p(true), 4, 3);
        assert!(
            (on.mean_shots - off.mean_shots).abs() < 1e-9,
            "the flag must not change the cadence"
        );
        assert!(
            on.mean_damage > off.mean_damage + 1e-9,
            "declaring it must be worth something: {} vs {}",
            on.mean_damage,
            off.mean_damage
        );
        // Shot 1 lands before any status exists, so every shot after it doubles
        // under CO — the un-declared explosion stays flat at 100 a shot.
        assert!(
            (off.mean_damage - 100.0 * off.mean_shots).abs() < 1e-9,
            "expected a flat 100/shot with no CO, got {}",
            off.mean_damage
        );
    }

    /// A reload draws WHOLE rounds — ✅ measured (user, 2026-07-30) on a
    /// 5-round magazine. The table is the measurement, verbatim.
    #[test]
    fn a_reload_draws_whole_rounds_and_leaves_the_fraction_behind() {
        let cap = 5.0;
        // 1.5 -> floor(3.5) = 3 -> 4.5, NOT a full 5.
        assert_eq!(reload_draw(cap, 1.5), 3.0);
        // 3.25 -> floor(1.75) = 1 -> 4.25, which is what the user saw.
        assert_eq!(reload_draw(cap, 3.25), 1.0);
        // 4.25 -> floor(0.75) = 0: the reload is refused, and the HUD's
        // ceiling makes it read as an already-full magazine.
        assert_eq!(reload_draw(cap, 4.25), 0.0);
        // Empty and overdrawn are the same rule, not a special case: a shot
        // cannot overdraw by a whole round, so the draw is always `cap`.
        assert_eq!(reload_draw(cap, 0.0), 5.0);
        assert_eq!(reload_draw(cap, -0.75), 5.0, "M14: comes back at 4.25");
        // Never negative, however overfull the magazine.
        assert_eq!(reload_draw(cap, 9.0), 0.0);
    }

    /// The two arcane stack-decay families, told apart IN THE SIM. Primary
    /// Crux is the `all_drop` one — VERBATIM (wiki): *"All stacks are lost when
    /// the buff's duration expires"*, confirmed in game (user, 2026-07-30):
    /// the timer runs out and the whole pile goes at once. The other family
    /// (Merciless/Deadhead/Dexterity) loses ONE stack and resets the timer.
    ///
    /// `arcanes_data` already pins Crux's flag; this pins that the flag still
    /// means something by the time the shot loop reads it.
    #[test]
    fn arcane_stacks_all_drop_on_timeout_or_bleed_off_one_at_a_time() {
        // 3 stacks x +1 multishot each, on a trigger that can never fire (no
        // status in this fixture), so the run only ever DECAYS from full.
        // Stacks are seeded full with expiry = duration (ArcRuntime::init).
        let p = |all_drop: bool| DummyParams {
            arcane: ArcaneFx {
                buffs: vec![ArcBuffSpec {
                owner: "test".into(),
                    grant: ArcGrant::Multishot,
                    trigger: ArcTrigger::ToxinStatus,
                    per_stack: 1.0,
                    max_stacks: 3,
                    duration: 2.0,
                    all_drop,
                    one_per_instance: false,
                    initial_stacks: 3,
                }],
                ..ArcaneFx::none()
            },
            multishot: 1.0,
            base_multishot: 1.0,
            fire_rate: 1.0,
            magazine_size: 100.0, // no reload inside the window
            duration_secs: 8.0,   // shots at t = 0..7
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        // all_drop: 3 stacks until t=2, then nothing at all.
        //   4 4 1 1 1 1 1 1 = 14
        let cliff = monte_carlo(&p(true), 4, 3);
        assert!((cliff.mean_shots - 8.0).abs() < 1e-9, "shots {}", cliff.mean_shots);
        assert!(
            (cliff.mean_pellets - 14.0).abs() < 1e-9,
            "expected 14 pellets (4 4 1 1 1 1 1 1), got {}",
            cliff.mean_pellets
        );
        // lose-one-and-reset: one stack every 2 s instead of a cliff.
        //   4 4 3 3 2 2 1 1 = 20
        let graceful = monte_carlo(&p(false), 4, 3);
        assert!(
            (graceful.mean_pellets - 20.0).abs() < 1e-9,
            "expected 20 pellets (4 4 3 3 2 2 1 1), got {}",
            graceful.mean_pellets
        );
    }

    /// ONE STACK PER DAMAGE INSTANCE, and the instance is the TRIGGER PULL.
    /// Wiki (Cascadia Flare), verbatim: *"Only one stack can be added per
    /// damage instance; applying multiple Heat status effects, such as via
    /// Multishot or Archon Vitality in a single hit will not generate multiple
    /// stacks."* The sim bumped once per PROC — inside `settle_procs`, which
    /// runs per pellet — so five pellets each proccing Heat granted five.
    ///
    /// Measured through the REPLAY rather than through damage: the claim is
    /// about the stack count, and reading anything else would let a wrong
    /// count pass by cancelling against a right multiplier.
    #[test]
    fn a_per_instance_arcane_gains_one_stack_a_pull_not_one_a_pellet() {
        let p = |one_per_instance: bool| DummyParams {
            arcane: ArcaneFx {
                id: "test".into(),
                buffs: vec![ArcBuffSpec {
                    owner: "test".into(),
                    grant: ArcGrant::CritDamage,
                    trigger: ArcTrigger::HeatStatus,
                    per_stack: 0.0, // observed, never applied: no feedback
                    max_stacks: 40,
                    duration: 1000.0, // no decay inside the window
                    all_drop: true,
                    one_per_instance,
                    initial_stacks: 0,
                }],
                ..ArcaneFx::none()
            },
            // FIVE pellets, every one of them forcing a Heat proc: the exact
            // case the wiki names.
            multishot: 5.0,
            base_multishot: 5.0,
            forced_procs: vec![DamageType::Heat],
            fire_rate: 1.0,
            magazine_size: 100.0,
            duration_secs: 10.0, // pulls at t = 0..9
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let stacks_at_end = |one: bool| {
            let params = p(one);
            let s = monte_carlo(&params, 1, 3);
            // 20 frames over 10 s, so the LAST frame sits at t = 9.5 — after
            // the t = 9 pull rather than on top of it.
            let rep = replay(&params, s.median_run.rng_state, 20);
            let i = rep.buffs.iter().position(|(id, _)| id == "arcane:test").expect("buff in roster");
            *rep.frames.last().expect("frames").stacks.get(i).expect("stack series")
        };
        // 10 pulls, 5 pellets each. Capped: one a pull -> 10. Uncapped: one a
        // pellet -> 40, the ceiling, reached in the first two pulls.
        assert_eq!(stacks_at_end(true), 10, "one stack per trigger pull");
        assert_eq!(stacks_at_end(false), 40, "and per pellet without the cap");
    }

    /// Plentiful Mayhem STARVES: the projectiles are produced in order, each
    /// paying a round as it goes, and one that cannot pay is simply not fired
    /// (user, 2026-07-30). The round itself always comes from the magazine, so
    /// the shot still happens — it just fires fewer pellets.
    #[test]
    fn plentiful_mayhem_drops_the_pellets_the_reserve_cannot_pay_for() {
        // 4x multishot, one magazine round, and only TWO reserve rounds for the
        // three extras: two are afforded, the third is dropped -> 3 pellets.
        let p = |reserve: f64| DummyParams {
            multishot: 4.0,
            multishot_ammo_bonus: 0.6,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            fire_rate: 1.0,
            magazine_size: 1.0,
            infinite_reserve: false,
            reserve_ammo: reserve,
            duration_secs: 30.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let starved = monte_carlo(&p(2.0), 4, 3);
        assert!((starved.mean_shots - 1.0).abs() < 1e-9, "the shot still fires");
        assert!(
            (starved.mean_pellets - 3.0).abs() < 1e-9,
            "1 magazine round + 2 affordable extras = 3 pellets, got {}",
            starved.mean_pellets
        );
        // Dry reserve starves every extra, leaving the weapon's own projectile.
        let dry = monte_carlo(&p(0.0), 4, 3);
        assert!(
            (dry.mean_pellets - 1.0).abs() < 1e-9,
            "only the magazine round's own pellet survives, got {}",
            dry.mean_pellets
        );
        // And the dropped pellets are not billed: damage tracks what FIRED.
        assert!(
            (starved.mean_damage / dry.mean_damage - (1.0 + 2.0 * 1.6)).abs() < 1e-9,
            "ratio {}",
            starved.mean_damage / dry.mean_damage
        );
    }

    /// Plentiful Mayhem, continuous branch. A merged beam has no separable
    /// generated projectile, so the perk scales the multishot BONUS instead —
    /// and lands on the same 1 + 1.6(M-1) the discrete branch reaches. The
    /// ammo draw still bills the RAW rolled count, which is what shortens the
    /// Incarnon window.
    #[test]
    fn plentiful_mayhem_on_a_beam_scales_the_bonus_and_drains_the_charge() {
        let dmg = |bonus: f64| DummyParams {
            continuous: true,
            fire_rate: 8.0,
            multishot: 2.0,
            base_multishot: 1.0,
            multishot_ammo_bonus: bonus,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            // Ammo must not bind here: both runs then tick at the SAME instants,
            // so the beam ramp cancels out of the ratio.
            magazine_size: 10_000.0,
            infinite_reserve: true,
            duration_secs: 2.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let (off, on) = (monte_carlo(&dmg(0.0), 4, 3), monte_carlo(&dmg(0.6), 4, 3));
        assert!(
            (on.mean_shots - off.mean_shots).abs() < 1e-9,
            "same tick count: {} vs {}",
            on.mean_shots,
            off.mean_shots
        );
        let ratio = on.mean_damage / off.mean_damage;
        assert!((ratio - 2.6 / 2.0).abs() < 1e-9, "ratio {ratio}");

        // The charge magazine: 2x multishot bills 2 rounds a tick, so ten
        // rounds last five ticks instead of ten.
        let ammo = |bonus: f64| DummyParams {
            magazine_size: 10.0,
            infinite_reserve: false,
            reserve_ammo: 0.0,
            // The charge-backed marker: no Capacity behind this magazine, so
            // the multishot surcharge comes out of the pool itself.
            ammo_efficiency_applies: false,
            duration_secs: 60.0,
            ..dmg(bonus)
        };
        let (a, b) = (monte_carlo(&ammo(0.0), 4, 3), monte_carlo(&ammo(0.6), 4, 3));
        assert!((a.mean_shots - 10.0).abs() < 1e-9, "baseline ticks {}", a.mean_shots);
        assert!((b.mean_shots - 5.0).abs() < 1e-9, "boosted ticks {}", b.mean_shots);
    }

    #[test]
    fn fractional_multishot_is_a_chance_for_one_more() {
        // Multishot 1.5 -> mean pellets/pull about 1.5.
        let p = DummyParams {
            multishot: 1.5,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 2000, 6);
        let per_pull = s.mean_pellets / s.mean_shots;
        assert!((per_pull - 1.5).abs() < 0.02, "pellets/pull {per_pull}");
    }

    #[test]
    fn magazine_and_reload_cadence_is_exact() {
        // No Frenzy: 12-round magazine at 1 shot/s, 2.35 s reloads, 30 s:
        // shots 0..11 (12), reload -> resume 14.35..25.35 (12), reload ->
        // resume 28.70, 29.70 (2) = 26 shots, 2 reloads.
        let p = DummyParams {
            duration_secs: 30.0,
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 4);
        assert!((s.mean_shots - 26.0).abs() < 1e-9, "shots {}", s.mean_shots);
        assert!((s.mean_reloads - 2.0).abs() < 1e-9);
    }

    /// Primary Crux: a stacking buff on weak-point HITS (not kills), whose
    /// status-chance grant joins the status BUCKET — wiki: "Status Chance
    /// bonus is additive to mods like Rifle Aptitude" — so it is RELATIVE to
    /// the attack part's own base. Pinned by arithmetic rather than a rate
    /// estimate: 25% base + 10 stacks x +30% = 25% + 75% = exactly 100%, i.e.
    /// one proc per instance.
    #[test]
    fn primary_crux_stacks_status_chance_on_weakpoint_hits() {
        let part = |is_head| {
            vec![BodyPart {
                name: "part".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head,
                crit_bonus: false,
            }]
        };
        // Stacks are EARNED (docs/BUFFS.md), so this reads the trigger with
        // nothing else in it: weak-point hits build the buff to its cap, body
        // hits build nothing at all. The body run is therefore not "the buff
        // lapsed" but "the buff never existed" — identical to no arcane,
        // instance for instance, which is a stricter statement than the
        // seeded version of this test could make.
        let mk = |is_head, a: ArcaneFx| DummyParams {
            status_chance: 0.25,
            base_status_chance: 0.25,
            base_crit_chance: 0.0,
            duration_secs: 20.0,
            arcane: a,
            body_parts: part(is_head),
            ..DummyParams::default()
        };
        let bare = run_once(&mk(true, ArcaneFx::none()), &mut Rng::new(11));
        assert!(
            bare.procs < bare.pellets,
            "25% status chance must not proc every instance ({} of {})",
            bare.procs,
            bare.pellets
        );
        // Weak-point hits: every hit is a trigger, so the buff reaches its
        // 10-stack cap within ten instances and 25% + 10 x 30% of 25% = 100%
        // status chance from there on — most instances proc, and far more
        // than the unbuffed run does.
        let head = run_once(&mk(true, arc("primary_crux")), &mut Rng::new(11));
        assert!(
            head.procs > bare.procs,
            "weak-point hits build the buff ({} vs {})",
            head.procs,
            bare.procs
        );
        assert!(
            head.procs >= head.pellets - 10,
            "at the cap every instance procs; only the climb falls short ({} of {})",
            head.procs,
            head.pellets
        );
        // Body only: nothing ever triggers it, so the arcane contributes
        // NOTHING — not "less", nothing. Same seed, same count as no arcane.
        let body = run_once(&mk(false, arc("primary_crux")), &mut Rng::new(11));
        assert_eq!(
            body.procs, bare.procs,
            "no weak-point hit = no stack = the arcane may not change a thing"
        );
    }

    /// Crux's second grant feeds the SAME ammo-efficiency bucket as Frenzy
    /// (wiki: "additive with other sources of Ammo Efficiency"). 10 stacks x
    /// +6% = 60%, so a shot costs 0.4 rounds and the 12-round magazine covers
    /// 30 shots — more than this 25 s window fires at 1 shot/s, so the reloads
    /// disappear entirely.
    #[test]
    fn primary_crux_ammo_efficiency_stretches_the_magazine() {
        let p = |a: ArcaneFx| DummyParams {
            duration_secs: 25.0,
            arcane: a,
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 3.0,
                is_head: true,
                crit_bonus: true,
            }],
            ..no_status()
        };
        let bare = monte_carlo(&p(ArcaneFx::none()), 20, 4);
        assert!(bare.mean_reloads > 0.0, "the fixture must reload without it");
        let crux = monte_carlo(&p(arc_stacked("primary_crux")), 20, 4);
        assert_eq!(crux.mean_reloads, 0.0, "60% efficiency covers the window");
        assert!(
            crux.mean_shots >= bare.mean_shots,
            "shots {} vs {}",
            crux.mean_shots,
            bare.mean_shots
        );
    }

    /// A lingering FIELD fixture in Torid's shape: 40 Toxin a tick, 1 tick/s
    /// for 10 s, its own 15% / 2.0x crit and 25% status, and the Torid's
    /// anomalous CO eligibility. `stacking` picks the branch (MEASUREMENTS M13
    /// measured `stack`; `refresh` is the other weapon-data option).
    fn cloud(stacking: crate::loadout::FieldStacking) -> crate::loadout::ResolvedLingering {
        let mut damage = DamageVector::default();
        damage.set(DamageType::Toxin, 40.0);
        crate::loadout::ResolvedLingering {
            damage,
            modified_base: 40.0,
            crit_chance: 0.0,   // crit off: tick COUNTS are the assertion
            crit_damage: 2.0,
            status_chance: 0.0, // status off: no DoT confounding the total
            base_crit_chance: 0.15,
            base_crit_damage: 2.0,
            base_status_chance: 0.25,
            tick_rate: 1.0,
            duration_s: 10.0,
            radius_m: 3.0,
            falloff_start_m: 0.0,
            falloff_reduction: 1.0,
            stacking,
            takes_condition_overload: true,
        }
    }

    /// The reference case, and the arithmetic the whole field model rests on:
    /// ONE grenade at t=0 leaves TEN 40-damage ticks at t=0..9 — ✅ measured
    /// (MEASUREMENTS M13): the first lands WITH the impact, then nine more over
    /// the remaining nine seconds. A 10 s engagement sees all ten.
    #[test]
    fn one_grenade_leaves_ten_ticks_starting_with_the_impact() {
        let p = DummyParams {
            damage: DamageVector::default(), // inert impact: the field alone
            lingering: Some(cloud(crate::loadout::FieldStacking::Stack)),
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            magazine_size: 1.0,
            reload_seconds: 999.0, // exactly one shot in the window
            infinite_reserve: false,
            reserve_ammo: 0.0,
            ..no_status()
        };
        let s = monte_carlo(&p, 4, 3);
        assert!((s.mean_shots - 1.0).abs() < 1e-9, "shots {}", s.mean_shots);
        // Ticks at 0..9, all ten inside a 10 s run.
        assert!(
            (s.mean_field_ticks - 10.0).abs() < 1e-9,
            "field ticks {}",
            s.mean_field_ticks
        );
        assert!(
            (s.mean_damage - 10.0 * 40.0).abs() < 1e-9,
            "dmg {} (expected 10 x 40 = the field's full 400)",
            s.mean_damage
        );
    }

    /// Overlapping fields STACK — ✅ measured (MEASUREMENTS M13). Both branches
    /// stay pinned because the branch is weapon data, not a global rule: a
    /// future weapon may well refresh instead. Three grenades one second apart:
    /// stacking runs three concurrent streams, refresh keeps re-arming one.
    #[test]
    fn overlapping_fields_stack_or_refresh_per_the_weapon_data() {
        let mk = |stacking| DummyParams {
            damage: DamageVector::default(),
            lingering: Some(cloud(stacking)),
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            // No arcane: the Default fixture's Enervate grants FLAT crit chance,
            // which correctly reaches the field's ticks and would blur a
            // tick-count assertion into a damage one.
            arcane: ArcaneFx::none(),
            fire_rate: 1.0,
            magazine_size: 3.0,
            reload_seconds: 999.0, // 3 shots at t=0,1,2 then dry
            infinite_reserve: false,
            reserve_ammo: 0.0,
            duration_secs: 20.0,
            ..no_status()
        };
        let st = monte_carlo(&mk(crate::loadout::FieldStacking::Stack), 4, 3);
        assert!((st.mean_shots - 3.0).abs() < 1e-9, "shots {}", st.mean_shots);
        // Three independent 10-tick streams, all finishing before t=20.
        assert!(
            (st.mean_field_ticks - 30.0).abs() < 1e-9,
            "stacking ticks {}",
            st.mean_field_ticks
        );
        // A tenth of the damage was riding on the first-tick question alone.
        assert!(
            (st.mean_damage - 30.0 * 40.0).abs() < 1e-9,
            "dmg {}",
            st.mean_damage
        );
        let rf = monte_carlo(&mk(crate::loadout::FieldStacking::Refresh), 4, 3);
        // One field, re-armed at t=1 and t=2 — each re-arm ticks immediately, so
        // 3 shot-time ticks plus the surviving field's own 9 = 12.
        assert!(
            (rf.mean_field_ticks - 12.0).abs() < 1e-9,
            "refresh ticks {}",
            rf.mean_field_ticks
        );
    }

    /// Renewed Horror — ✅ measured (MEASUREMENTS M13): reloading from EMPTY
    /// makes the NEXT shot's pod live twice as long, "1 direct hit + 20 pod
    /// ticks" against the normal 10. A one-round magazine makes every shot after
    /// the first a post-reload shot, so the counts are exact: shot 1 leaves 10
    /// ticks, shot 2 leaves 20.
    #[test]
    fn renewed_horror_doubles_only_the_post_reload_field() {
        let p = |boost: f64| DummyParams {
            damage: DamageVector::default(),
            lingering: Some(cloud(crate::loadout::FieldStacking::Stack)),
            field_duration_on_empty_reload: boost,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            fire_rate: 1.0,
            magazine_size: 1.0,
            reload_seconds: 1.0,
            // EXACTLY two shots: one from the magazine, one from the single
            // reserve round, then dry. The 60 s window then lets every field
            // finish, so the tick counts are exact rather than truncated.
            infinite_reserve: false,
            reserve_ammo: 1.0,
            duration_secs: 60.0,
            ..no_status()
        };
        let off = monte_carlo(&p(1.0), 4, 3);
        let on = monte_carlo(&p(2.0), 4, 3);
        let shots = off.mean_shots;
        assert!((shots - 2.0).abs() < 1e-9, "expected two shots, got {shots}");
        assert!(
            (on.mean_shots - shots).abs() < 1e-9,
            "the buff must not change the cadence"
        );
        // Every shot but the first follows an empty reload, so each of those
        // fields doubles: ticks go from 10n to 10 + 20(n-1).
        let n = shots;
        assert!(
            (off.mean_field_ticks - 10.0 * n).abs() < 1e-9,
            "baseline ticks {} for {n} shots",
            off.mean_field_ticks
        );
        assert!(
            (on.mean_field_ticks - (10.0 + 20.0 * (n - 1.0))).abs() < 1e-9,
            "boosted ticks {} for {n} shots",
            on.mean_field_ticks
        );
    }

    /// The field is a WEAPON damage instance, not a status DoT: it rolls its own
    /// crit off its OWN base stats, and its ticks report in their own bucket
    /// rather than the DoT one.
    #[test]
    fn field_ticks_roll_their_own_crit_and_report_as_field_damage() {
        let mk = |cc: f64| {
            let mut f = cloud(crate::loadout::FieldStacking::Stack);
            f.crit_chance = cc;
            DummyParams {
                damage: DamageVector::default(),
                lingering: Some(f),
                crit_multiplier: 1.0,
                base_crit_chance: 0.0,
                magazine_size: 1.0,
                reload_seconds: 999.0,
                infinite_reserve: false,
                reserve_ammo: 0.0,
                    ..no_status()
            }
        };
        let flat = monte_carlo(&mk(0.0), 4, 3);
        let crit = monte_carlo(&mk(1.0), 4, 3);
        // 100% crit at 2.0x doubles every tick.
        assert!(
            (crit.mean_damage - 2.0 * flat.mean_damage).abs() < 1e-6,
            "{} vs {}",
            crit.mean_damage,
            flat.mean_damage
        );
        // Counted as FIELD damage, never as a DoT tick.
        assert_eq!(flat.mean_dot_damage, 0.0, "a field tick is not a status DoT");
        assert_eq!(flat.median_run.dot_ticks, 0, "no bleed/DoT ticks at all");
        assert!((flat.source_damage.field - flat.mean_damage).abs() < 1e-9);
    }

    /// Status is per TICK — "Toxin clouds can proc Hunter Munitions on each tick
    /// of damage" — so a 100%-status cloud procs once per tick, and those procs
    /// then feed Condition Overload like any other.
    #[test]
    fn field_ticks_proc_status_once_each() {
        let mut f = cloud(crate::loadout::FieldStacking::Stack);
        f.status_chance = 1.0;
        let p = DummyParams {
            damage: DamageVector::default(),
            lingering: Some(f),
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            magazine_size: 1.0,
            reload_seconds: 999.0,
            infinite_reserve: false,
            reserve_ammo: 0.0,
            ..no_status()
        };
        let s = monte_carlo(&p, 4, 3);
        assert!(
            (s.mean_procs - s.mean_field_ticks).abs() < 1e-9,
            "procs {} vs ticks {}",
            s.mean_procs,
            s.mean_field_ticks
        );
        assert!(s.mean_dot_damage > 0.0, "the cloud's Toxin procs must burn");
    }

    /// CONTINUOUS weapons MERGE their multishot instead of making several
    /// instances. VERBATIM (wiki Multishot §Continuous Weapons): the combined
    /// tick has "damage AND Status Chance equal to the SUM of the individual
    /// beams, but the Critical Chance is still equal to that of a single beam."
    ///
    /// Multishot is pinned at exactly 2.0 so the roll is deterministic, and
    /// crit is off so the damage assertion is arithmetic.
    #[test]
    fn a_beam_merges_its_multishot_into_one_instance() {
        let mk = |continuous, ms: f64| DummyParams {
            damage: DamageVector::new().with(DamageType::Toxin, 50.0),
            continuous,
            multishot: ms,
            base_multishot: 1.0,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            duration_secs: 4.0,
            ..no_status()
        };
        // Doubling multishot doubles the damage either way…
        let b1 = monte_carlo(&mk(true, 1.0), 8, 5);
        let b2 = monte_carlo(&mk(true, 2.0), 8, 5);
        assert!(
            (b2.mean_damage - 2.0 * b1.mean_damage).abs() < 1e-6,
            "beam {} vs 2x{}",
            b2.mean_damage,
            b1.mean_damage
        );
        // …but a beam does it in ONE instance per tick, where a gun fires two.
        // That is the whole mechanic, and the reason crit stays a single roll.
        assert!(
            (b2.mean_pellets - b2.mean_shots).abs() < 1e-9,
            "a beam tick is one instance ({} pellets, {} ticks)",
            b2.mean_pellets,
            b2.mean_shots
        );
        let g2 = monte_carlo(&mk(false, 2.0), 8, 5);
        assert!(
            (g2.mean_pellets - 2.0 * g2.mean_shots).abs() < 1e-9,
            "a gun still fires two pellets ({} vs {})",
            g2.mean_pellets,
            g2.mean_shots
        );
        // And the beam pays the ramp a gun does not: the same two beams' worth
        // of damage arrives lower over a short burst.
        assert!(
            b2.mean_damage < g2.mean_damage,
            "the ramp must cost something: beam {} vs gun {}",
            b2.mean_damage,
            g2.mean_damage
        );
    }

    /// THE EXPONENT. A beam's DoT goes as multishot **squared**, and it is the
    /// one number about beams that everybody gets wrong (asked again 2026-08-07,
    /// with the reasonable guess that a beam trades proc COUNT for proc SIZE and
    /// comes out even).
    ///
    /// It does not trade. The merge sums BOTH halves, so nothing is given up:
    ///
    /// | | procs per tick | payload each | DoT |
    /// | --- | --- | --- | --- |
    /// | gun | `M x SC` | 1x | `M` |
    /// | beam, rolled status | `M x SC` | `Mx` | `M²` |
    /// | beam, FORCED proc | 1 | `Mx` | `M` |
    ///
    /// Wiki (Multishot §Continuous Weapons), verbatim: *"The total output of
    /// damaging status effects … is affected **twice** by multishot on all
    /// continuous weapons"*, and the exception that proves the mechanism —
    /// forced procs *"are applied after the damage instances are merged. Because
    /// of this their damage output is not affected twice by multishot, instead
    /// being equivalent to use on standard weapons."*
    ///
    /// Deterministic on purpose: status chance is 1.0 at M=1, so the merged
    /// chance is exactly `M` procs a tick and no ratio here is a sample mean.
    /// Three ticks x M=3 is 9 Toxin stacks, one under the cap that would
    /// flatten the very effect being measured.
    #[test]
    fn a_beams_dot_scales_with_multishot_squared() {
        let mk = |continuous, ms: f64, forced: bool| {
            let mut p = DummyParams {
                damage: DamageVector::new().with(DamageType::Toxin, 50.0),
                continuous,
                multishot: ms,
                base_multishot: 1.0,
                status_chance: if forced { 0.0 } else { 1.0 },
                base_status_chance: if forced { 0.0 } else { 1.0 },
                forced_procs: if forced { vec![DamageType::Toxin] } else { Vec::new() },
                crit_multiplier: 1.0,
                base_crit_chance: 0.0,
                fire_rate: 1.0,
                duration_secs: 2.5,
                ..DummyParams::default()
            };
            // NOTHING MAY DIE and nothing may ramp: a target that dies truncates
            // the run, and the fixture's default arcane is Secondary Enervate,
            // whose crit ramps with the number of instances a shot makes — which
            // is the very thing multishot changes.
            p.target.base_health = 1e12;
            p.target.base_armor = 0.0;
            p.target.base_shield = 0.0;
            p.target.base_overguard = 0.0;
            p.arcane = ArcaneFx::none();
            // ONE body part at 1x. A proc's payload carries the procing hit's
            // part multiplier, so the fixture's 50/50 body/3x-head draw is
            // noise sitting on top of the very ratio being measured.
            p.body_parts = vec![BodyPart {
                name: "body".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: false,
                crit_bonus: false,
            }];
            p
        };
        let run = |c, ms, f| monte_carlo(&mk(c, ms, f), 8, 11);
        let close = |a: f64, b: f64| (a - b).abs() < 0.02 * b;

        for m in [2.0_f64, 3.0] {
            let (one, many) = (run(true, 1.0, false), run(true, m, false));
            // Procs are NOT traded away: the summed status chance still lands M
            // of them a tick, exactly as a gun's M pellets would.
            let procs = many.mean_procs / one.mean_procs;
            assert!(close(procs, m), "beam procs x{procs} at M={m}");
            // …and each is built from the merged instance, so the total squares.
            let dot = many.mean_dot_damage / one.mean_dot_damage;
            assert!(close(dot, m * m), "beam dot x{dot} at M={m}, want x{}", m * m);

            // A GUN is linear in both halves — same proc count, payload untouched.
            let (g1, gm) = (run(false, 1.0, false), run(false, m, false));
            let gdot = gm.mean_dot_damage / g1.mean_dot_damage;
            assert!(close(gdot, m), "gun dot x{gdot} at M={m}");

            // FORCED procs: merged first, so ONE a tick whatever M is — and the
            // linear payload puts the beam back level with the gun.
            let (f1, fm) = (run(true, 1.0, true), run(true, m, true));
            let fprocs = fm.mean_procs / f1.mean_procs;
            assert!(close(fprocs, 1.0), "forced procs x{fprocs} at M={m} — merged, so one a tick");
            let fdot = fm.mean_dot_damage / f1.mean_dot_damage;
            assert!(close(fdot, m), "forced dot x{fdot} at M={m}, want x{m}");
        }
    }

    /// The damage RAMP: "Initial damage starts at a lower percentage, and ramps
    /// up to 100% of its damage over 0.6 seconds of hitting a target … this
    /// lower percentage is 20%." At 10 ticks/s that is the first tick at 20% and
    /// full damage from the 7th on.
    #[test]
    fn a_beam_ramps_from_a_fifth_to_full_over_point_six_seconds() {
        let mut ramp = BeamRamp::default();
        let dt = 0.1; // 10 ticks/s
        let mults: Vec<f64> = (0..8).map(|i| ramp.tick(i as f64 * dt, dt, BEAM_RAMP_FLOOR)).collect();
        assert!((mults[0] - 0.20).abs() < 1e-9, "first tick {}", mults[0]);
        // Each held tick adds 0.1/0.6 of the way from 20% to 100%.
        assert!((mults[1] - (0.2 + 0.8 / 6.0)).abs() < 1e-9, "second {}", mults[1]);
        assert!((mults[6] - 1.0).abs() < 1e-9, "7th tick should be full: {}", mults[6]);
        assert!((mults[7] - 1.0).abs() < 1e-9);

        // Stopping decays it: "0.8 seconds after the weapon stops hitting a
        // target, the damage decays back to its initial point over 2 seconds."
        // Idle time is the gap MINUS the tick that would have been due, so a
        // 1.9 s gap is 1.8 s idle, 1.0 s of it past the delay = half the ramp.
        let mut r2 = BeamRamp::default();
        for i in 0..8 {
            r2.tick(i as f64 * dt, dt, BEAM_RAMP_FLOOR);
        }
        let after = r2.tick(0.7 + 1.9, dt, BEAM_RAMP_FLOOR);
        assert!((after - (0.2 + 0.8 * 0.5)).abs() < 1e-9, "after a gap {after}");
        // And a long enough gap returns it all the way to the floor.
        let mut r3 = BeamRamp::default();
        for i in 0..8 {
            r3.tick(i as f64 * dt, dt, BEAM_RAMP_FLOOR);
        }
        assert!((r3.tick(0.7 + 3.0, dt, BEAM_RAMP_FLOOR) - 0.20).abs() < 1e-9);
    }

    #[test]
    fn finite_reserve_stops_the_gun() {
        // Reserve off: 12 in the mag + 12 in reserve = 24 shots, then dry.
        let p = DummyParams {
            duration_secs: 60.0,
            infinite_reserve: false,
            reserve_ammo: 12.0,
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 4);
        assert!((s.mean_shots - 24.0).abs() < 1e-9, "shots {}", s.mean_shots);
    }

    #[test]
    fn frenzy_ammo_efficiency_prevents_reloads() {
        // All-head with Frenzy: only the first shot consumes ammo (Frenzy's
        // +100% efficiency zeroes the rest), so the 25-shot cadence holds
        // with zero reloads despite the 12-round magazine.
        let p = DummyParams {
            frenzy: true,
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 3.0,
                is_head: true,
                crit_bonus: true,
            }],
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 4);
        assert!((s.mean_shots - 25.0).abs() < 1e-9, "shots {}", s.mean_shots);
        assert_eq!(s.mean_reloads, 0.0);
    }

    #[test]
    fn frenzy_accelerates_fire_rate_on_headshots() {
        // All-head aim: the first headshot grants Frenzy (fire rate x2.5 ->
        // interval 0.4 s), refreshed by every subsequent headshot. Shots at
        // t = 0, 0.4, 0.8, ... -> 1 + floor(9.99../0.4) = 25 shots in 10 s
        // (vs 10 without Frenzy).
        let p = DummyParams {
            frenzy: true,
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 3.0,
                is_head: true,
                crit_bonus: true,
            }],
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 4);
        assert!((s.mean_shots - 25.0).abs() < 1e-9, "shots {}", s.mean_shots);
        // Body-only aim: Frenzy never triggers -> plain 10 shots.
        let q = DummyParams {
            frenzy: true,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s2 = monte_carlo(&q, 20, 4);
        assert!(
            (s2.mean_shots - 10.0).abs() < 1e-9,
            "shots {}",
            s2.mean_shots
        );
    }

    #[test]
    fn locked_frenzy_is_always_active_without_headshots() {
        // Body-only aim never triggers Frenzy naturally, but the lock keeps
        // it up from t=0: cadence 0.4 s -> 25 shots in 10 s.
        let p = DummyParams {
            frenzy: true,
            locked_buffs: vec![BuffLock::permanent(LockedBuff::Frenzy)],
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 4);
        assert!((s.mean_shots - 25.0).abs() < 1e-9, "shots {}", s.mean_shots);
    }

    #[test]
    fn status_procs_occur_at_the_listed_rate() {
        // 37% SC, no forced procs: mean procs per shot ≈ 0.37 on the
        // never-dying training dummy.
        let s = monte_carlo(&DummyParams::default(), 2000, 8);
        let per_shot = s.mean_procs / s.mean_shots;
        assert!((per_shot - 0.37).abs() < 0.02, "procs/shot {per_shot}");
        // Bleeds contribute extra damage on top of the 3375 baseline.
        assert!(s.mean_dot_damage > 0.0);
        assert!(s.mean_damage > 3375.0);
    }

    #[test]
    fn forced_bleed_dot_is_exactly_deterministic() {
        // Forced Slash on every shot, SC 0, mono body 1x, crit_multiplier 1
        // (tier changes nothing): every shot procs one bleed with tick value
        // 0.35 × 75 = 26.25, ticking at +1..+6 s. Ticks beyond the 10 s
        // engagement are lost: shots at 0..9 yield 6,6,6,6,5,4,3,2,1,0 ticks
        // = 39 ticks → dot = 39 × 26.25 = 1023.75; direct = 10 × 75 = 750.
        let p = DummyParams {
            crit_multiplier: 1.0,
            forced_procs: vec![DamageType::Slash],
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 50, 5);
        assert!(
            (s.mean_dot_damage - 1023.75).abs() < 1e-9,
            "dot {}",
            s.mean_dot_damage
        );
        assert!(
            (s.mean_damage - 1773.75).abs() < 1e-9,
            "total {}",
            s.mean_damage
        );
        assert!((s.mean_procs - 10.0).abs() < 1e-9);
    }

    /// HUNTER MUNITIONS. A guaranteed crit with a 100% roll must bleed on
    /// every shot, on a weapon whose vector holds NO Slash at all — the mod's
    /// whole point is that it does not draw from the damage types.
    #[test]
    fn hunter_munitions_bleeds_off_a_crit_on_a_weapon_with_no_slash() {
        // no_status() gives status chance 0, so a bleed here can only have
        // come from the crit roll. Crit chance 1.0, multiplier 1.0 keeps the
        // arithmetic the same as the forced-Slash case above.
        let p = DummyParams {
            damage: DamageVector::new().with(DamageType::Puncture, 75.0),
            base_crit_chance: 1.0,
            crit_multiplier: 1.0,
            slash_on_crit: 1.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 50, 11);
        // Same schedule as the forced-Slash test: 39 ticks x 0.35 x 75.
        assert!((s.mean_dot_damage - 1023.75).abs() < 1e-9, "dot {}", s.mean_dot_damage);
        assert!((s.mean_procs - 10.0).abs() < 1e-9, "one proc per shot");

        // Without the mod the same build bleeds not at all.
        let none = DummyParams { slash_on_crit: 0.0, ..p.clone() };
        assert_eq!(monte_carlo(&none, 50, 11).mean_dot_damage, 0.0);
    }

    /// The chance is independent of status chance and rolls PER PELLET, so a
    /// 30% mod on a guaranteed crit bleeds about 30% of pellets.
    #[test]
    fn hunter_munitions_rolls_its_own_chance_per_pellet() {
        let p = DummyParams {
            damage: DamageVector::new().with(DamageType::Puncture, 75.0),
            base_crit_chance: 1.0,
            crit_multiplier: 1.0,
            slash_on_crit: 0.30,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 4000, 12);
        let per_shot = s.mean_procs / 10.0; // 10 shots in the window
        assert!((per_shot - 0.30).abs() < 0.02, "procced {per_shot:.3} per shot, expected ~0.30");
    }

    /// Hunter Munitions is a CRITICAL HIT'S PRIVILEGE, not a bleed of its own:
    /// the trigger changed, nothing else did (user, 2026-07-31). Every DoT in
    /// this engine hangs off a PARENT hit and inherits that hit's multipliers,
    /// and the whole point of pushing `Slash` onto the pellet's proc list is
    /// that the mod's bleed gets the same parent as a naturally rolled one.
    ///
    /// Proven rather than argued: against a FORCED Slash proc under identical
    /// conditions the bleeds must agree, in every condition a bleed reads.
    ///
    /// They agree to ~0.06% rather than exactly, and that gap is not
    /// mechanical: the mod's own roll consumes an RNG draw per pellet, so the
    /// two builds walk different random streams. It shrinks with runs
    /// (1.6% at 200, 0.06% at 6000), which is what sampling noise does and a
    /// real difference does not.
    #[test]
    fn a_hunter_munitions_bleed_is_indistinguishable_from_any_other_slash() {
        // Guaranteed crit + guaranteed roll, so both builds proc exactly once
        // per pellet and the only difference is WHERE the proc came from.
        let pair = |tweak: fn(&mut DummyParams)| {
            let base = || {
                let mut p = DummyParams {
                    damage: DamageVector::new().with(DamageType::Puncture, 75.0),
                    base_crit_chance: 1.0,
                    body_parts: mono_body(1.0),
                    ..no_status()
                };
                tweak(&mut p);
                p
            };
            let mut forced = base();
            forced.forced_procs = vec![DamageType::Slash];
            let mut hm = base();
            hm.slash_on_crit = 1.0;
            let (f, h) = (monte_carlo(&forced, 6000, 21), monte_carlo(&hm, 6000, 21));
            assert_eq!(f.mean_procs, h.mean_procs, "one proc per pellet either way");
            (f.mean_dot_damage, h.mean_dot_damage)
        };
        let same = |(f, h): (f64, f64), what: &str| {
            assert!(
                (f - h).abs() / f < 0.005,
                "{what}: forced {f:.2} vs hunter munitions {h:.2}"
            );
            h
        };

        // The bleed coefficient and its armour bypass.
        let plain = same(pair(|_| {}), "plain bleed");
        // The PARENT's crit multiplier — the tier that pellet rolled.
        same(pair(|p| p.crit_multiplier = 3.0), "crit multiplier");
        // The PARENT's body part — a 3x head multiplies the bleed too.
        same(pair(|p| p.body_parts = mono_body(3.0)), "part multiplier");
        // Red crits: cc 2.0 is tier 2 guaranteed on both.
        let tier2 = same(
            pair(|p| {
                p.base_crit_chance = 2.0;
                p.crit_multiplier = 3.0;
            }),
            "tier 2",
        );
        // Status DURATION — its own set mate, Hunter Track, does exactly this.
        let longer = same(pair(|p| p.status_duration_mult = 1.9), "status duration");
        // The status DAMAGE bucket.
        same(pair(|p| p.status_damage_mult = 1.5), "status damage");
        // A Vigilante promotion happens BEFORE this roll reads the tier, so
        // the promoted crit is the parent.
        let promoted = same(
            pair(|p| {
                p.crit_tier_upgrade_chance = 1.0;
                p.crit_multiplier = 3.0;
            }),
            "vigilante-promoted parent",
        );

        // And each of those conditions actually moved the bleed — an
        // agreement between two numbers that never change proves nothing.
        // Only ~14%, not 90%: the engagement window truncates the tail, so a
        // longer bleed only pays for shots with room left to tick.
        assert!(longer > plain * 1.10, "duration added ticks: {plain:.0} -> {longer:.0}");
        let crit_only = same(pair(|p| p.crit_multiplier = 3.0), "crit multiplier");
        assert!(tier2 > crit_only * 1.4, "tier 2 hit harder: {crit_only:.0} -> {tier2:.0}");
        assert!(promoted > crit_only * 1.4, "the promotion reached the bleed");
    }

    /// INTERNAL BLEEDING's bleed is an ordinary bleed too. It has always fed
    /// the same per-pellet proc list, so it has always had the same parent —
    /// this makes that checkable rather than something to take on trust, and
    /// pins that the two mods differ ONLY in what triggers them.
    #[test]
    fn an_internal_bleeding_bleed_is_indistinguishable_from_any_other_slash() {
        use crate::loadout::ProcConv;
        let base = || DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 75.0),
            base_crit_chance: 0.0,
            crit_multiplier: 3.0,
            status_chance: 1.0,
            base_status_chance: 1.0,
            body_parts: mono_body(3.0), // a 3x part, so the parent matters
            ..DummyParams::default()
        };
        // Converted at 100%: every pellet lands Impact and turns it into Slash.
        let mut ib = base();
        ib.proc_conversion = Some(ProcConv {
            from: DamageType::Impact,
            to: DamageType::Slash,
            chance: 1.0,
            low_rate_threshold: 0.0, // never doubled, so the rate is irrelevant
            low_rate_mult: 1.0,
        });
        // The same bleed, forced, on a weapon that cannot roll one itself.
        let mut forced = base();
        forced.damage = DamageVector::new().with(DamageType::Puncture, 75.0);
        forced.status_chance = 0.0;
        forced.base_status_chance = 0.0;
        forced.forced_procs = vec![DamageType::Slash];

        let (i, f) = (monte_carlo(&ib, 6000, 41), monte_carlo(&forced, 6000, 41));
        assert!(
            (i.mean_dot_damage - f.mean_dot_damage).abs() / f.mean_dot_damage < 0.005,
            "internal bleeding {:.2} vs forced Slash {:.2}",
            i.mean_dot_damage,
            f.mean_dot_damage
        );
        assert!(i.mean_dot_damage > 0.0);
    }

    /// The Vigilante promotion reaches EVERY attack part that can crit. The
    /// set says "enhance Critical Hits from Primary Weapons" with no qualifier
    /// about which part made the hit, so a direct hit, a lingering field tick
    /// and an EXPLOSION all take it — the explosion was left out at first,
    /// which was an artifact of where the code was edited and nothing else.

    #[test]
    fn the_vigilante_promotion_reaches_an_explosion_too() {
        let radial = crate::loadout::ResolvedRadial {
            damage: {
                let mut d = DamageVector::default();
                d.set(DamageType::Radiation, 100.0);
                d
            },
            modified_base: 100.0,
            crit_chance: 1.0, // always a tier-1 crit, so a promotion is visible
            crit_damage: 3.0,
            base_crit_chance: 1.0,
            base_crit_damage: 3.0,
            status_chance: 0.0,
            base_status_chance: 0.0,
            radius_m: 2.0,
            falloff_start_m: 0.0,
            falloff_reduction: 0.0,
            takes_condition_overload: false,
            takes_multishot: true,
            co_base_fraction: 1.0,
        };
        // A zero-damage direct hit, so everything reported is the explosion's.
        let p = |promote: f64| DummyParams {
            damage: DamageVector::default(),
            radial: Some(radial),
            crit_tier_upgrade_chance: promote,
            base_crit_chance: 0.0,
            crit_multiplier: 1.0,
            fire_rate: 1.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let off = monte_carlo(&p(0.0), 200, 51);
        let on = monte_carlo(&p(1.0), 200, 51);
        // The explosion always crits at tier 1; promoting it makes every one a
        // BIG crit, which is the exact statement — the damage ratio is not,
        // because this fixture's total is not all crit-scaled.
        assert!(off.mean_big_crit_rate < 1e-9, "no promotion, no big crits");
        // EVERY crit is a big crit — an identity, not a threshold. This used to
        // read `> 0.44` against a rate that is ~0.445 because the denominator
        // counts all instances and the zero-damage direct hit never crits; the
        // margin was 0.002 and the seed that produced it was the only evidence
        // for it. Splitting the RNG streams moved this sample to 0.430 and the
        // test failed on a fixture nothing was wrong with (old spread over ten
        // seeds 0.438-0.457, new 0.430-0.453 — the same distribution).
        //
        // The claim was never about the rate. Promotion is certain here, so
        // every crit is promoted, and that is exact at any seed.
        assert!(on.mean_big_crit_rate > 0.0, "nothing crit at all");
        assert!(
            (on.mean_big_crit_rate - on.mean_crit_rate).abs() < 1e-12,
            "every explosion crit promoted: big {:.4} of crit {:.4}",
            on.mean_big_crit_rate, on.mean_crit_rate
        );
        assert!(
            on.mean_damage > off.mean_damage * 1.3,
            "and it is worth damage: {:.0} -> {:.0}",
            off.mean_damage,
            on.mean_damage
        );
    }

    /// HUNTER MUNITIONS + INTERNAL BLEEDING, against a number the wiki
    /// publishes: on a shot that both crits and applies an Impact status the
    /// Slash chance is **54.5%** at fire rate >= 2.5, and **79%** below it.
    ///
    /// The two are "drawn independently, and if both proc at the same time,
    /// only 1 slash proc is applied" — so the combined chance is a union,
    /// 1 - (1-0.30)(1-0.35) = 0.545, and with Internal Bleeding doubled below
    /// 2.5, 1 - (1-0.30)(1-0.70) = 0.79. Reproducing both from the two rolls
    /// is what shows the exclusion is modeled as an exclusion and not as some
    /// second bleed quietly going missing.
    #[test]
    fn hunter_munitions_and_internal_bleeding_union_to_the_wikis_numbers() {
        use crate::loadout::ProcConv;
        // Pure Impact at 100% status: every pellet lands the Impact proc that
        // Internal Bleeding converts from, and every pellet crits.
        let build = |fire_rate: f64| DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 75.0),
            base_crit_chance: 1.0,
            crit_multiplier: 1.0,
            status_chance: 1.0,
            base_status_chance: 1.0,
            fire_rate,
            slash_on_crit: 0.30,
            proc_conversion: Some(ProcConv {
                from: DamageType::Impact,
                to: DamageType::Slash,
                chance: 0.35,
                low_rate_threshold: 2.5,
                low_rate_mult: 2.0,
            }),
            body_parts: mono_body(1.0),
            ..DummyParams::default()
        };
        // procs per pellet = 1 Impact + P(Slash), so P falls straight out.
        let p_slash = |fr: f64, seed: u64| {
            let p = build(fr);
            let s = monte_carlo(&p, 8000, seed);
            s.mean_procs / s.mean_pellets - 1.0 // reloads make shots != duration x rate
        };
        let fast = p_slash(4.0, 31);
        let slow = p_slash(2.0, 32);
        assert!((fast - 0.545).abs() < 0.015, "fire rate 4.0: {fast:.3}, wiki 0.545");
        assert!((slow - 0.79).abs() < 0.015, "fire rate 2.0: {slow:.3}, wiki 0.79");
    }

    /// The roll hangs off the CRIT, so at half the crit chance it bleeds half
    /// as often — "indirectly affected by its Critical Chance" (wiki).
    #[test]
    fn hunter_munitions_tracks_the_crit_rate() {
        let at = |cc: f64, seed: u64| {
            let p = DummyParams {
                damage: DamageVector::new().with(DamageType::Impact, 75.0),
                base_crit_chance: cc,
                crit_multiplier: 1.0,
                slash_on_crit: 1.0,
                body_parts: mono_body(1.0),
                ..no_status()
            };
            monte_carlo(&p, 4000, seed).mean_procs / 10.0 // 10 shots in the window
        };
        let full = at(1.0, 13);
        let half = at(0.5, 14);
        let none = at(0.0, 15);
        assert!((full - 1.0).abs() < 0.01, "every crit bleeds: {full:.3}");
        assert!(full > half && half > none, "{full:.3} > {half:.3} > {none:.3}");
        assert!(full - none > 0.3, "the crit rate has to move it: {none:.3}");
        // NOT an exact ratio, and not for a reason that belongs to this mod:
        // the raw `DummyParams::default()` fixture carries a crit FLOOR of
        // ~0.45 that `base_crit_chance: 0.0` does not clear (it survives
        // zeroing `unmodded_crit_chance` and does not depend on the damage
        // type, so it is neither the relative term nor Puncture's Weakened).
        // Existing tests never saw it because they neutralise crits with
        // `crit_multiplier: 1.0` instead of the chance. Worth chasing on its
        // own; asserting a clean 0.5-of-full here would only be encoding it.
    }

    #[test]
    fn bleed_snapshots_the_proccing_hits_multipliers() {
        // 3x part, forced Slash, crit disabled: tick = 0.35 × 75 × 3 = 78.75.
        let p = DummyParams {
            crit_multiplier: 1.0,
            forced_procs: vec![DamageType::Slash],
            body_parts: mono_body(3.0),
            duration_secs: 2.0, // one shot at t=0 (+ t=1): first bleed ticks once at t=1...
            fire_rate: 0.5,     // single shot at t=0 in a 2 s window
            ..no_status()
        };
        // Shot at t=0 procs a bleed ticking at t=1 (once before 2 s).
        let s = monte_carlo(&p, 10, 6);
        assert!(
            (s.mean_dot_damage - 78.75).abs() < 1e-9,
            "dot {}",
            s.mean_dot_damage
        );
    }

    #[test]
    fn weakened_raises_our_crit_chance() {
        // Forced Puncture, SC 0, mono 1x, cd 2.0, base cc 0:
        // shot k has Enervate 0.10k + Weakened 0.05·min(k,5) flat cc,
        // E[crit_mult] = 1 + cc → E[total] = 75 × (10 + Σcc) = 75 × 16.25.
        // Σcc = 0+.15+.30+.45+.60+.75+.85+.95+1.05+1.15 = 6.25.
        let p = DummyParams {
            base_crit_chance: 0.0,
            forced_procs: vec![DamageType::Puncture],
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 4000, 77);
        let expect = 75.0 * 16.25;
        assert!(
            (s.mean_damage - expect).abs() / expect < 0.02,
            "mean {} expect {expect}",
            s.mean_damage
        );
    }

    /// PRELUDE OF MIGHT IS CHECKED AT THE HIT, NOT ON THE ARSENAL SCREEN.
    ///
    /// Wiki (Furis / Braton Incarnon Genesis), the perk's own row: "With
    /// Critical Chance below 40%: Increase Base Critical Damage Multiplier by
    /// +3x" — and, on the same row, "Condition is affected by the critical
    /// chance increase effect of Puncture status". Weakened is +5% flat crit
    /// chance received per stack, so a build that starts under the line walks
    /// over it on its own Puncture procs and loses the perk while they hold.
    ///
    /// Arithmetic rather than statistics: one forced Puncture per shot, one
    /// shot a second, no other crit source, cd 5.0 granted (2.0 + 3.0).
    ///   shot 0   0 stacks   cc .32   on    1 + .32 x (5 - 1) = 2.28
    ///   shot 1   1 stack    cc .37   on                        2.48
    ///   shot 2   2 stacks   cc .42   off   1 + .42 x (2 - 1) = 1.42
    ///   shot 3   3 stacks   cc .47   off                       1.47
    ///   shot 4   4 stacks   cc .52   off                       1.52
    /// Sum 9.17, against 5 x 2.28 = 11.40 with the forced proc taken away —
    /// the control, whose only difference is whether Weakened lands at all.
    /// (Status IMMUNITY would not have been that control: a forced proc goes
    /// on regardless of it.)
    #[test]
    fn weakened_takes_prelude_of_might_away() {
        let build = || DummyParams {
            damage: DamageVector::new().with(DamageType::Puncture, 100.0),
            base_crit_chance: 0.32,
            unmodded_crit_chance: 0.32,
            crit_multiplier: 5.0,
            crit_mult_below_cc: Some((3.0, 0.40)),
            unmodded_crit_damage: 2.0,
            forced_procs: vec![DamageType::Puncture],
            body_parts: mono_body(1.0),
            fire_rate: 1.0,
            duration_secs: 5.0,
            arcane: crate::arcanes_data::ArcaneFx::none(),
            target: TargetParams { base_health: 1e15, ..DummyParams::default().target },
            ..no_status()
        };
        let lost = monte_carlo(&build(), 4000, 91).mean_damage;
        assert!(
            (lost - 917.0).abs() / 917.0 < 0.02,
            "with Weakened {lost}, expected 917"
        );

        let mut unproced = build();
        unproced.forced_procs.clear();
        let kept = monte_carlo(&unproced, 4000, 91).mean_damage;
        assert!(
            (kept - 1140.0).abs() / 1140.0 < 0.02,
            "without Weakened {kept}, expected 1140"
        );
    }

    /// "BELOW" IS STRICT. Same fixture with no procs at all, and a threshold
    /// set exactly ON the build's crit chance: the perk is gone, so the run is
    /// worth 5 x (1 + .32 x (2 - 1)) = 6.60 -> 660.
    #[test]
    fn prelude_of_might_is_off_exactly_at_its_threshold() {
        let p = DummyParams {
            damage: DamageVector::new().with(DamageType::Puncture, 100.0),
            base_crit_chance: 0.32,
            unmodded_crit_chance: 0.32,
            crit_multiplier: 5.0,
            crit_mult_below_cc: Some((3.0, 0.32)),
            unmodded_crit_damage: 2.0,
            body_parts: mono_body(1.0),
            fire_rate: 1.0,
            duration_secs: 5.0,
            arcane: crate::arcanes_data::ArcaneFx::none(),
            target: TargetParams { base_health: 1e15, ..DummyParams::default().target },
            ..no_status()
        };
        let s = monte_carlo(&p, 4000, 91).mean_damage;
        assert!((s - 660.0).abs() / 660.0 < 0.02, "at the threshold {s}, expected 660");
    }

    /// "+5 **BASE** MULTISHOT" IS NOT "+5 MULTISHOT", and the difference is the
    /// whole perk.
    ///
    /// Forceful Finality's card carries the word Base, and the wiki attaches a
    /// note to that row: *"Multishot bonus is added before mods, and is thus
    /// multiplied by multishot bonuses."* The Torid's Final Fusillade — the
    /// other perk that grants a flat multishot on the magazine's last round —
    /// says only "+3 Multishot", with no such note, and is flat.
    ///
    /// Both were modelled as flat until 2026-08-11. This measures the pellets a
    /// full magazine actually fires, because that is the only place the two
    /// readings differ: the panel is identical under either.
    #[test]
    fn a_base_multishot_grant_is_multiplied_by_multishot_mods() {
        let arena = crate::arena::Arena::training(20.0);
        let pellets = |evo: &[&str], mods: &[&crate::loadout::ModDef]| {
            let base = crate::loadout::WeaponBase::from_data("burston_prime", true, evo);
            let panel = crate::loadout::resolve(&base, mods, crate::loadout::StackPolicy::Emergent);
            let p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
            let s = monte_carlo(&p, 200, 0xB0A2);
            (s.mean_pellets / s.mean_shots, panel.multishot, panel.magazine_size)
        };
        let pool = crate::mods_data::class_pool("rifle");
        let split = pool.iter().find(|m| m.id == "split_chamber").expect("split chamber");
        let mods: Vec<&crate::loadout::ModDef> = vec![split];

        let (bare_off, _, mag) = pellets(&[], &[]);
        let (bare_on, _, _) = pellets(&["burston_prime_forceful_finality"], &[]);
        let (mod_off, ms, _) = pellets(&[], &mods);
        let (mod_on, _, _) = pellets(&["burston_prime_forceful_finality"], &mods);
        let (bare_gain, mod_gain) = (bare_on - bare_off, mod_on - mod_off);

        // THE CLAIM IS A RATIO, and it is asserted as one. "Worth 0.333 pellets
        // a shot" would be hostage to how many whole magazines fit in the
        // engagement — 150 rounds over a 45-round magazine is three and a
        // third, and the third never reaches its last burst. What the card
        // says is that the same +5 is multiplied by the multishot bonuses, so
        // the two gains stand in exactly that ratio however many bursts landed.
        assert!(
            (mod_gain / bare_gain - ms).abs() < 0.05,
            "a BASE grant scales with the bucket: x{:.2} of the bare gain, bucket is x{ms:.2}              ({bare_gain:.3} -> {mod_gain:.3} pellets a shot)",
            mod_gain / bare_gain
        );
        // …and it is worth roughly the whole burst, which is what says the
        // window is three rounds rather than one.
        let per_magazine = bare_gain * mag;
        assert!(
            (11.0..=15.5).contains(&per_magazine),
            "5 pellets on each of a 3-round burst, less the magazine the fight              ends mid-way through: {per_magazine:.1} a magazine"
        );
    }

    /// The OTHER spelling, unchanged: the Torid's is flat, so a multishot mod
    /// does not touch it. Same shape, opposite answer — which is the point.
    #[test]
    fn a_plain_multishot_grant_is_not_multiplied() {
        let arena = crate::arena::Arena::training(20.0);
        let pellets = |evo: &[&str], mods: &[&crate::loadout::ModDef]| {
            let base = crate::loadout::WeaponBase::from_data("torid", true, evo);
            let panel = crate::loadout::resolve(&base, mods, crate::loadout::StackPolicy::Emergent);
            let p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
            let s = monte_carlo(&p, 200, 0xB0A2);
            (s.mean_pellets / s.mean_shots, panel.magazine_size)
        };
        let pool = crate::mods_data::class_pool("rifle");
        let split = pool.iter().find(|m| m.id == "split_chamber").expect("split chamber");
        let mods: Vec<&crate::loadout::ModDef> = vec![split];

        let (bare_off, mag) = pellets(&[], &[]);
        let (bare_on, _) = pellets(&["torid_final_fusillade"], &[]);
        let (mod_off, _) = pellets(&[], &mods);
        let (mod_on, _) = pellets(&["torid_final_fusillade"], &mods);
        let want = 3.0 / mag;
        assert!(
            (bare_on - bare_off - want).abs() < 0.02 && (mod_on - mod_off - want).abs() < 0.03,
            "flat either way ({want:.3}): {bare_off:.3}->{bare_on:.3}, {mod_off:.3}->{mod_on:.3}"
        );
    }

    #[test]
    fn status_immunities_renormalize_toward_other_procs() {
        // Slash-immune target: no bleeds ever, but procs still occur at the
        // full 37% rate (renormalized onto Impact/Puncture).
        let mut p = DummyParams::default();
        p.target.status_immunities = vec![DamageType::Slash];
        let s = monte_carlo(&p, 1000, 15);
        assert_eq!(s.mean_dot_damage, 0.0);
        let per_shot = s.mean_procs / s.mean_shots;
        assert!((per_shot - 0.37).abs() < 0.03, "procs/shot {per_shot}");
    }

    #[test]
    fn non_head_weak_spot_never_triggers_headshot() {
        // MOA-fanny-pack-like: 3x location, not a head, no crit bonus.
        // Headshot effects must never fire; damage uses plain cd.
        // Per shot: E = 225*(1+cc) -> total = 2250 + 225*5.0 = 3375.
        let p = single_part(BodyPart {
            name: "fanny pack".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: false,
            crit_bonus: false,
        });
        let s = monte_carlo(&p, 2000, 11);
        assert_eq!(s.mean_headshot_rate, 0.0);
        assert!(
            (s.mean_damage - 3375.0).abs() / 3375.0 < 0.02,
            "mean damage was {}",
            s.mean_damage
        );
    }

    #[test]
    fn helmeted_head_triggers_headshot_without_crit_bonus() {
        // Helmeted-Corpus-like: true head (triggers headshot effects) but not
        // eligible for the critical-location bonus -> same expectation as the
        // fanny pack: 3375, yet headshot rate is 100%.
        let p = single_part(BodyPart {
            name: "helmeted head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        });
        let s = monte_carlo(&p, 2000, 13);
        assert_eq!(s.mean_headshot_rate, 1.0);
        assert!(
            (s.mean_damage - 3375.0).abs() / 3375.0 < 0.02,
            "mean damage was {}",
            s.mean_damage
        );
    }

    #[test]
    fn one_x_location_gets_no_crit_bonus_even_if_flagged() {
        // Charger-mouth-like: 1x, not a head. Even with crit_bonus set, a 1x
        // location never receives the critical-location bonus.
        // Per shot: E = 75*(1+cc) -> total = 750 + 75*5.0 = 1125.
        let p = single_part(BodyPart {
            name: "mouth".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: true,
        });
        let s = monte_carlo(&p, 2000, 17);
        assert!(
            (s.mean_damage - 1125.0).abs() / 1125.0 < 0.02,
            "mean damage was {}",
            s.mean_damage
        );
    }

    /// THE OCUCOR'S TENDRILS, through Sentient Surge: every kill is worth
    /// crit chance and status chance, and a RELOAD takes all of it away.
    ///
    /// The reload half is the one worth pinning. "Tendrils disappear upon
    /// reloading or emptying the magazine", and the mod's own page repeats it
    /// — "Reloading the weapon will reset the bonuses, as they are directly
    /// tied to its tendril effect which resets on reload." A model that let
    /// the stacks ride through a reload would look right in every reading that
    /// never reloads, which is exactly the reading a short test does.
    ///
    /// The tendrils' own DAMAGE is deliberately absent and is not tested here,
    /// because there is nothing to test: on the beam's own target a tendril is
    /// cosmetic (wiki), and this arena has no second target.
    #[test]
    fn tendrils_buy_crit_chance_and_a_reload_takes_it_back() {
        // Base crit 0, so EVERY crit in the result came from a tendril: the
        // measurement cannot be diluted by a base the weapon already had.
        let build = |mag: f64, per_tendril: f64| DummyParams {
            base_crit_chance: 0.0,
            unmodded_crit_chance: 1.0, // the base a relative bonus multiplies
            crit_multiplier: 2.0,
            tendril_max: 4,
            cc_per_tendril: per_tendril,
            magazine_size: mag,
            reload_seconds: 0.5,
            fire_rate: 10.0,
            duration_secs: 60.0,
            body_parts: mono_body(1.0),
            target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            // flat_base(), not no_status(): the default fixture carries an
            // arcane that crits on its own, which would supply the very thing
            // this test is trying to attribute to tendrils.
            ..flat_base()
        };
        let crit_rate = |p: &DummyParams| {
            let r = run_once(p, &mut Rng::new(7));
            assert!(r.kills > 0, "the fixture must actually kill something");
            r.crits as f64 / r.pellets.max(1) as f64
        };

        // A magazine that never runs out: kills accumulate tendrils and the
        // crit chance climbs to the cap.
        let deep = crit_rate(&build(100_000.0, 0.25));
        assert!(deep > 0.5, "tendrils should be carrying the crit rate, got {deep}");

        // OFF, same fixture: nothing but the zero base is left.
        let none = crit_rate(&build(100_000.0, 0.0));
        assert!(none < 1e-9, "no tendril bonus means no crits at all, got {none}");

        // A ONE-ROUND magazine reloads after every shot, so no tendril ever
        // survives to the next pull. Same kills, same bonus, no benefit.
        let shallow = crit_rate(&build(1.0, 0.25));
        assert!(
            shallow < deep / 2.0,
            "a reload must clear the tendrils: deep magazine {deep}, one-round {shallow}"
        );
    }

    /// THE TENDRIL CARD: the one way to measure this mod in the fight it is
    /// actually played in.
    ///
    /// A tendril costs a KILL, so against a target that does not die the
    /// Ocucor's only augment is worth exactly nothing and the weapon reads as
    /// if it were unmodded — which is what a player reported (2026-08-08:
    /// 视使之触的专属卡无法选择层数，测不了视使的伤害). The count is a buff by
    /// every test that matters, so it takes a buff card's two knobs, and this
    /// pins both: the seed is worth its stacks, and the LOCK is what carries
    /// them past a reload.
    #[test]
    fn the_tendril_card_seeds_the_count_and_the_lock_holds_it() {
        // No kills at all: the target never dies, so the sim can never grant a
        // tendril and every crit below came from the card.
        let build = |stacks: u32, held: bool, mag: f64| DummyParams {
            base_crit_chance: 0.0,
            unmodded_crit_chance: 1.0,
            crit_multiplier: 2.0,
            tendril_max: 4,
            cc_per_tendril: 0.25,
            tendrils_initial: stacks,
            tendrils_held: held,
            magazine_size: mag,
            reload_seconds: 0.5,
            fire_rate: 10.0,
            duration_secs: 60.0,
            body_parts: mono_body(1.0),
            target: frail_target(TargetMode::InfiniteHealth, 0.0, 1e12),
            ..flat_base()
        };
        let crit_rate = |p: &DummyParams| {
            let r = run_once(p, &mut Rng::new(7));
            assert_eq!(r.kills, 0, "this fixture must never kill: the card is the only source");
            r.crits as f64 / r.pellets.max(1) as f64
        };

        // Unset, the mod is unmeasurable — the state the report describes.
        assert!(crit_rate(&build(0, false, 100_000.0)) < 1e-9);
        // Four tendrils at +25% each, off a base of 1.0: every shot crits.
        assert!(
            crit_rate(&build(4, false, 100_000.0)) > 0.99,
            "the card must reach the fight"
        );
        // ...and it is the COUNT, not a switch: two tendrils buy half of it.
        let two = crit_rate(&build(2, false, 100_000.0));
        assert!((0.45..0.55).contains(&two), "two tendrils should be worth ~50%, got {two}");

        // A ONE-ROUND magazine reloads after every shot, and a reload clears
        // tendrils — the seed included, because the seed is the same buff.
        let unheld = crit_rate(&build(4, false, 1.0));
        assert!(unheld < 0.2, "a reload must spend the seed too, got {unheld}");
        // LOCKED ("no timeout"): for a buff whose end is an event rather than
        // a clock, that event is what stops happening.
        let held = crit_rate(&build(4, true, 1.0));
        assert!(held > 0.99, "a locked card must survive every reload, got {held}");
    }

    /// SENTIENT SURGE'S REFILL PAYS EACH KILL ONCE — and is not a reload.
    ///
    /// "On Kill: Refill X% of the Magazine", drawn from the reserve, so a kill
    /// is worth a FIXED number of rounds and then it is spent. On the Ocucor
    /// that is 20% of 60 = 12 rounds against 6 ammo a second, i.e. one kill
    /// every two seconds and the weapon never reloads again — which is the
    /// mod's whole reputation, and is a property of the KILL RATE rather than
    /// of the refill being generous.
    ///
    /// The bug this pins re-earned every kill on every loop iteration, which
    /// topped the magazine up on every shot and quietly handed the weapon an
    /// infinite one. It showed up nowhere except the reload count, and the
    /// tendril test never looked at reloads.
    #[test]
    fn the_magazine_refill_pays_each_kill_once() {
        // 10 rounds, 1 ammo a shot, 1 shot a second, and a target that dies to
        // every shot: 10 kills a magazine, each worth 20% of 10 = 2 rounds.
        let build = |refill: f64| DummyParams {
            magazine_size: 10.0,
            ammo_cost: 1.0,
            fire_rate: 1.0,
            reload_seconds: 1.0,
            duration_secs: 120.0,
            mag_refill_on_kill: refill,
            body_parts: mono_body(1.0),
            target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            ..flat_base()
        };
        let run = |refill: f64| {
            let r = run_once(&build(refill), &mut Rng::new(3));
            (r.shots, r.reloads)
        };

        let (bare_shots, bare_reloads) = run(0.0);
        assert!(bare_reloads > 5, "the fixture must reload a lot, got {bare_reloads}");

        // Every shot kills, so every shot returns 2 rounds while spending 1:
        // the magazine fills faster than it drains and the reloads stop.
        let (surge_shots, surge_reloads) = run(0.20);
        assert!(surge_reloads < bare_reloads, "the refill must save reloads");
        assert!(surge_shots > bare_shots, "and buy shots with the time saved");

        // THE ARITHMETIC, on a refill too small to outrun the drain. 5% of 10
        // is 0.5 rounds a kill against 1 spent, so the magazine still empties —
        // just half as often. NEVER near zero, which is where the bug put it.
        let (_, slow_reloads) = run(0.05);
        assert!(
            slow_reloads > bare_reloads / 3 && slow_reloads < bare_reloads,
            "a 5% refill halves the drain, it does not remove it: bare {bare_reloads}, \
             with refill {slow_reloads}"
        );

        // AND A REFILL IS NOT A RELOAD, which is the entire point of pairing it
        // with a passive that a reload destroys. Measured as tendrils SURVIVING:
        // a per-tendril crit bonus off a zero base, so every crit counted here
        // belongs to a tendril that lived through a magazine being topped up.
        let surging = DummyParams {
            base_crit_chance: 0.0,
            unmodded_crit_chance: 1.0,
            tendril_max: 4,
            cc_per_tendril: 0.25,
            ..build(0.20)
        };
        let r = run_once(&surging, &mut Rng::new(3));
        assert!(
            r.crits as f64 / r.pellets.max(1) as f64 > 0.5,
            "tendrils must survive a refill — if the refill cleared them like a \
             reload does, the crit rate would sit near zero"
        );
    }

    /// A SYNDICATE RADIAL IS ARMED BY AFFINITY AND CAPPED BY ITS COOLDOWN.
    ///
    /// "All affinity earned by the weapon during a mission fills a gauge",
    /// 1000 points at a maxed augment, and a weapon kill gives the weapon HALF
    /// the enemy's affinity — "Kill with weapons: Half Affinity goes to the
    /// Warframe and half to the killing weapon".
    ///
    /// Two things worth pinning separately, because a model can get either one
    /// right alone: that kills ARM it at the rate the wiki gives, and that the
    /// 30 s cooldown BOUNDS it however fast you kill.
    #[test]
    fn a_syndicate_radial_arms_on_affinity_and_is_capped_by_its_cooldown() {
        let truth = *crate::syndicates_data::get("truth").expect("truth");
        // A target worth 200 affinity at level 1: the multiplier is
        // 1 + 0.1425 = 1.1425, so 228 floored, and the weapon's half is 114.
        // Nine kills fill the 1000-point gauge.
        let mut t = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
        t.base_affinity = 200.0;
        let build = |secs: f64, radial: Option<crate::syndicates_data::SyndicateDef>| DummyParams {
            syndicate_radial: radial,
            magazine_size: 100_000.0,
            fire_rate: 10.0,
            duration_secs: secs,
            body_parts: mono_body(1.0),
            target: t.clone(),
            ..flat_base()
        };
        let blasts = |p: &DummyParams| {
            let r = run_once(p, &mut Rng::new(11));
            (r.sources.syndicate / truth.damage).round() as u32
        };

        // OFF: no augment, no explosions, however much dies.
        assert_eq!(blasts(&build(300.0, None)), 0, "no augment, no radial");

        // ON, and every shot kills, so the gauge refills far faster than the
        // cooldown allows: 300 s / 30 s = 10 detonations and not one more.
        assert_eq!(
            blasts(&build(300.0, Some(truth))),
            10,
            "the cooldown bounds it however fast the gauge fills"
        );
        // ...and the bound is the COOLDOWN, not the clock: half the time, half
        // the blasts.
        assert_eq!(blasts(&build(150.0, Some(truth))), 5);

        // THE ARMING IS REAL, not a timer wearing a gauge's name. A target
        // worth a tenth as much affinity takes ten times the kills to fill it,
        // and at this fire rate that is slow enough to miss cooldowns.
        let mut poor = t.clone();
        poor.base_affinity = 2.0;
        let starved = blasts(&DummyParams { target: poor, ..build(300.0, Some(truth)) });
        assert!(
            starved < 10,
            "a low-affinity target must fill the gauge more slowly, got {starved} blasts"
        );
    }

    fn frail_target(mode: TargetMode, armor: f64, overguard: f64) -> TargetParams {
        TargetParams {
            name: "test target".into(),
            base_level: 1,
            level: 1,
            base_health: 50.0, // below the weakest possible shot (75)
            base_armor: armor,
            base_overguard: overguard,
            base_affinity: 0.0,
            base_shield: 0.0,
            health_curve: crate::scaling::health::UNAFFILIATED,
            shield_curve: crate::scaling::shield::GRINEER,
            attenuation: None,
            stack_caps: None,
            cannot_be_frozen: false,
            steel_path: false,
            eximus: false,
            can_be_eximus: false,
            status_immunities: Vec::new(),
            faction: crate::loadout::Faction::Unknown,
            type_mods: crate::factions_data::Columns::NEUTRAL,
            mode,
        }
    }

    /// A radial params fixture: the direct hit is inert (no damage roll
    /// noise, no status), so everything the run reports comes from the
    /// explosion.
    fn radial_only(radial: crate::loadout::ResolvedRadial) -> DummyParams {
        DummyParams {
            damage: DamageVector::default(),
            radial: Some(radial),
            // No arcane: `DummyParams::default()` carries Enervate, whose FLAT
            // crit chance reaches the explosion (absolute sources land on every
            // stage) - real behaviour, but not what this fixture is isolating.
            arcane: ArcaneFx::none(),
            status_chance: 0.0,
            base_status_chance: 0.0,
            base_crit_chance: 0.0,
            forced_procs: Vec::new(),
            ..DummyParams::default()
        }
    }

    fn radial_of(status_chance: f64, crit_chance: f64) -> crate::loadout::ResolvedRadial {
        let mut damage = DamageVector::default();
        damage.set(DamageType::Heat, 300.0);
        crate::loadout::ResolvedRadial {
            damage,
            modified_base: 300.0,
            crit_chance,
            crit_damage: 2.0,
            base_crit_chance: crit_chance,
            base_crit_damage: 2.0,
            status_chance,
            base_status_chance: status_chance,
            radius_m: 2.0,
            falloff_start_m: 0.0,
            falloff_reduction: 0.2,
            takes_condition_overload: false,
            takes_multishot: true,
            co_base_fraction: 1.0, // the default: an explosion gets no CO
        }
    }

    #[test]
    fn the_explosion_rolls_its_own_status_apart_from_the_direct_hit() {
        // Wiki (Laetum): "Initial hit and explosion apply status
        // separately." The direct hit here can never proc (0% SC), so any
        // proc at all is the explosion's own draw.
        let quiet = run_once(&radial_only(radial_of(0.0, 0.0)), &mut Rng::new(7));
        assert_eq!(quiet.procs, 0, "0% radial SC = no procs anywhere");

        let loud = run_once(&radial_only(radial_of(1.0, 0.0)), &mut Rng::new(7));
        assert_eq!(
            loud.procs, loud.shots,
            "100% radial SC = exactly one proc per landed explosion"
        );
        assert!(
            loud.dot_damage > 0.0,
            "the explosion's Heat procs must burn on their own"
        );
        assert_eq!(quiet.shots, loud.shots, "status does not change the cadence");
    }

    #[test]
    fn the_explosion_rolls_its_own_crit_and_never_counts_as_a_pellet_crit() {
        // Separate instance = separate crit roll. `crits`/`headshots` stay
        // direct-pellet counters (an explosion never headshots), but the
        // damage must show the radial's own multiplier.
        let flat = run_once(&radial_only(radial_of(0.0, 0.0)), &mut Rng::new(3));
        let crit = run_once(&radial_only(radial_of(0.0, 1.0)), &mut Rng::new(3));
        assert_eq!(
            crit.crits, flat.crits,
            "the radial's crit chance must not move the pellet crit counter"
        );
        assert_eq!(
            crit.headshots, flat.headshots,
            "an explosion never headshots, so it adds nothing to that counter"
        );
        assert!(
            (crit.sources.radial - 2.0 * flat.sources.radial).abs() < 1e-6,
            "100% radial crit at 2x = double the explosion damage ({} vs {})",
            crit.sources.radial,
            flat.sources.radial
        );
        assert_eq!(flat.sources.direct, 0.0, "the fixture's direct hit is inert");
    }

    /// A RELATIVE crit buff joins the crit BUCKET, so it reaches the explosion
    /// — and scales the EXPLOSION's own base, not the direct hit's. Both halves
    /// matter: under `AssumedMax` these bonuses arrive inside `r.crit_damage`
    /// through the mod bucket, so an Emergent path that skipped them made one
    /// mod behave differently under two policies.
    ///
    /// Pinned stacks make it arithmetic, and the direct part is given a very
    /// different base (10x) on purpose: had the buff been resolved against the
    /// direct base — the bug — the explosion would have landed at cd 12, not 4.
    /// Laetum Incarnon uses the same 22%/2.2x for both parts, so only a fixture
    /// with deliberately different bases can catch that half.
    #[test]
    fn a_relative_crit_buff_reaches_the_explosion_against_its_own_base() {
        use crate::arcanes_data::{ArcBuffSpec, ArcGrant, ArcTrigger};
        let radial = |crit_damage: f64| {
            let mut damage = DamageVector::default();
            damage.set(DamageType::Heat, 300.0);
            crate::loadout::ResolvedRadial {
                damage,
                modified_base: 300.0,
                crit_chance: 1.0, // always crits: no crit-roll noise
                crit_damage,
                base_crit_chance: 1.0,
                base_crit_damage: 2.0,
                status_chance: 0.0,
                base_status_chance: 0.0,
                radius_m: 2.0,
                falloff_start_m: 0.0,
                falloff_reduction: 0.0,
                takes_condition_overload: false,
                takes_multishot: true,
                co_base_fraction: 1.0, // the default: an explosion gets no CO
            }
        };
        // +50% x 2 pinned stacks = +100% of the part's base crit damage.
        let buff = ArcaneFx {
            buffs: vec![ArcBuffSpec {
                owner: "test".into(),
                grant: ArcGrant::CritDamage,
                trigger: ArcTrigger::ToxinStatus,
                per_stack: 0.5,
                max_stacks: 2,
                duration: crate::loadout::NO_TIMEOUT,
                all_drop: true,
                one_per_instance: false,
                initial_stacks: 2,
            }],
            ..ArcaneFx::none()
        };
        let live = run_once(
            &DummyParams {
                unmodded_crit_damage: 10.0, // NOT the base the radial must use
                arcane: buff,
                ..radial_only(radial(2.0))
            },
            &mut Rng::new(4),
        );
        // What the same bonus looks like folded into the radial's resolved crit
        // damage — i.e. what AssumedMax produces through the bucket.
        let folded = run_once(&radial_only(radial(4.0)), &mut Rng::new(4));
        assert!(
            (live.sources.radial - folded.sources.radial).abs() < 1e-6,
            "explosion {} vs bucket-equivalent {}",
            live.sources.radial,
            folded.sources.radial
        );
        // And it really is a change: cd 2 -> 4 doubles a guaranteed crit.
        let bare = run_once(&radial_only(radial(2.0)), &mut Rng::new(4));
        assert!(
            (live.sources.radial - 2.0 * bare.sources.radial).abs() < 1e-6,
            "explosion {} vs unbuffed {}",
            live.sources.radial,
            bare.sources.radial
        );
    }

    /// M11 (in-game, 2026-07-30): on a LONE enemy one Laetum shot grants
    /// TWO stacks of Overwhelming Attrition — the direct hit and the
    /// explosion each arm it. The clean way to assert that here is a
    /// ZERO-damage radial: it still runs as a stage, so any extra damage
    /// can only come from the buff having been armed a second time.
    #[test]
    fn the_explosion_arms_an_on_hit_buff_of_its_own() {
        let mk = |with_radial: bool| {
            let radial = with_radial.then(|| crate::loadout::ResolvedRadial {
                damage: DamageVector::default(), // 0 damage: a pure extra INSTANCE
                modified_base: 0.0,
                crit_chance: 0.0,
                crit_damage: 2.0,
                base_crit_chance: 0.0,
                base_crit_damage: 2.0,
                status_chance: 0.0,
                base_status_chance: 0.0,
                radius_m: 2.0,
                falloff_start_m: 0.0,
                falloff_reduction: 0.0,
                takes_condition_overload: false,
                takes_multishot: true,
                co_base_fraction: 1.0, // the default: an explosion gets no CO
            });
            let p = DummyParams {
                radial,
                stacking_buffs: vec![crate::loadout::StackingBuff {
                id: "on_plain_hit_damage",
                trigger: crate::loadout::BuffTrigger::PlainHit,
                grant: crate::loadout::BuffGrant::BaseDamage,
                chance: 1.0,
                decay: crate::loadout::BuffDecay::LoseOneAndReset,
                    per_stack: 4.0,
                    max_stacks: 3,
                    duration: 10.0,
                    // Earn them in the run — that is what is under test.
                    initial_stacks: 0,
                    stacks_per_trigger: 1,
                    per_shell: false,
                    cleared_by: crate::loadout::ClearedBy::Nothing,
                }],
                // Never crits, never procs: every instance is "plain", so
                // the only variable is HOW MANY instances a shot produces.
                base_crit_chance: 0.0,
                status_chance: 0.0,
                forced_procs: Vec::new(),
                // ONE body part at 1x, so no aim variance rides on top of the
                // effect being measured.
                body_parts: mono_body(1.0),
                ..DummyParams::default()
            };
            // AVERAGED, not one engagement. Adding the radial adds a real
            // extra instance that makes its own crit decision, so the two
            // builds do not share a sample path and one run of each is a coin
            // flip — it used to be read as evidence, and it landed the right
            // way up only because of the order the old single RNG stream
            // happened to serve its numbers in. Over 200 engagements the
            // mechanism is plain: 11867 -> 12842.
            let s = monte_carlo(&p, 200, 4);
            (s.source_damage.direct, s.source_damage.radial)
        };
        let (solo, no_blast) = mk(false);
        let (paired, blast) = mk(true);
        assert_eq!(no_blast, 0.0, "control has no radial at all");
        assert_eq!(blast, 0.0, "the radial deals zero damage by construction");
        assert!(
            paired > solo,
            "the zero-damage explosion still arms the buff, so the DIRECT              damage must climb faster: {solo:.0} -> {paired:.0}"
        );
    }

    /// EVERY buff the roster offers must be READ by `apply_buff_config`.
    ///
    /// A card whose setting reaches nothing is the failure mode this whole
    /// area keeps producing: `buff_roster` (what exists), `enumerate_buffs`
    /// (what is drawn) and `apply_buff_config` (what is obeyed) are three
    /// lists, and Deadly Efficiency was in the first two and missing from the
    /// third — so its card was drawn, set, and dropped, for as long as it has
    /// existed. Nothing about the UI could reveal that: a knob that does
    /// nothing looks exactly like a knob whose buff is not up.
    ///
    /// The check is generic on purpose. It does not name the fields a buff
    /// writes into — it sets one id at a time and asserts the params CHANGED,
    /// so a buff added later is covered without anyone remembering to come
    /// back here.
    #[test]
    fn every_buff_the_roster_offers_is_actually_read() {
        use crate::loadout::{StackSpec, TimedBuff};
        let stack = |per_stack: f64| StackSpec {
            per_stack,
            max_stacks: 3,
            duration: 6.0,
            initial_stacks: 0,
        };
        let timed = |value: f64| TimedBuff {
            value,
            duration: 4.0,
            initial_active: false,
        };
        // One params carrying every configurable buff at once.
        let params = DummyParams {
            co_stack: Some(stack(0.2)),
            ms_stack: Some(stack(0.3)),
            cc_stack: Some(stack(0.1)),
            stacking_buffs: vec![crate::loadout::StackingBuff {
                id: "on_plain_hit_damage",
                trigger: crate::loadout::BuffTrigger::PlainHit,
                grant: crate::loadout::BuffGrant::BaseDamage,
                chance: 1.0,
                decay: crate::loadout::BuffDecay::LoseOneAndReset,
                per_stack: 4.0,
                max_stacks: 3,
                duration: 10.0,
                initial_stacks: 0,
                stacks_per_trigger: 1,
                per_shell: false,
                cleared_by: crate::loadout::ClearedBy::Nothing,
            }, crate::loadout::StackingBuff {
                id: "on_headshot_reload_speed",
                trigger: crate::loadout::BuffTrigger::Headshot,
                grant: crate::loadout::BuffGrant::ReloadSpeed,
                chance: 1.0,
                decay: crate::loadout::BuffDecay::LoseOneAndReset,
                per_stack: 0.1,
                max_stacks: 3,
                duration: 6.0,
                initial_stacks: 0,
                stacks_per_trigger: 1,
                per_shell: false,
                cleared_by: crate::loadout::ClearedBy::Nothing,
            }],
            cc_on_headshot: Some(timed(0.5)),
            cd_on_kill: Some(timed(0.6)),
            fr_on_reload: Some(timed(0.7)),
            bd_on_reload: Some(timed(0.8)),
            // Both halves, for the same reason the replay fixture carries
            // them: the tendril card exists only where a mod reads the count.
            tendril_max: 4,
            cc_per_tendril: 0.1,
            ..DummyParams::default()
        };

        // Applied OUTSIDE this function, deliberately — the weapon passive is
        // a `locked_buffs` entry built by the api (`frenzy_apply`), not a
        // field of these params. It is exempt from the check, not from
        // being read.
        const ELSEWHERE: [&str; 1] = ["frenzy"];

        for (id, _max) in params.buff_roster() {
            if ELSEWHERE.contains(&id.as_str()) {
                continue;
            }
            let mut configured = params.clone();
            let mut cfg = BuffConfig::new();
            cfg.insert(id.clone(), (1, true));
            configured.apply_buff_config(&cfg);
            assert_ne!(
                format!("{configured:?}"),
                format!("{params:?}"),
                "the card for '{id}' is drawn but nothing reads it"
            );
        }
    }

    #[test]
    fn overwhelming_attrition_takes_the_buff_cards_two_knobs() {
        // LOCKED = NO TIMEOUT, not frozen (user, 2026-08-02). This buff was
        // left on the old reading when the rest moved: its stacks decayed from
        // the seed and its trigger was skipped while locked, so "no timeout"
        // meant "decays to zero and can never come back" — the exact opposite
        // of the label, and a player reported it as the buff not working at
        // all (2026-08-03: 选无限持续后直接不生效).
        let mk = |initial: u32, locked: bool, fire_rate: f64, secs: f64| {
            let mut p = DummyParams {
                stacking_buffs: vec![crate::loadout::StackingBuff {
                id: "on_plain_hit_damage",
                trigger: crate::loadout::BuffTrigger::PlainHit,
                grant: crate::loadout::BuffGrant::BaseDamage,
                chance: 1.0,
                decay: crate::loadout::BuffDecay::LoseOneAndReset,
                    per_stack: 4.0,
                    max_stacks: 3,
                    // Locking IS this: the card's duration, overwritten.
                    duration: if locked { crate::loadout::NO_TIMEOUT } else { 10.0 },
                    initial_stacks: initial,
                    stacks_per_trigger: 1,
                    per_shell: false,
                    cleared_by: crate::loadout::ClearedBy::Nothing,
                }],
                fire_rate,
                duration_secs: secs,
                ..DummyParams::default()
            };
            // No crits and no procs, so EVERY hit is a plain hit and the buff
            // arms on all of them.
            p.base_crit_chance = 0.0;
            p.status_chance = 0.0;
            p.forced_procs = Vec::new();
            run_once(&p, &mut Rng::new(11)).total_damage
        };
        let without = |fire_rate: f64, secs: f64| {
            run_once(
                &DummyParams {
                    base_crit_chance: 0.0,
                    status_chance: 0.0,
                    forced_procs: Vec::new(),
                    fire_rate,
                    duration_secs: secs,
                    ..DummyParams::default()
                },
                &mut Rng::new(11),
            )
            .total_damage
        };

        // ---- 1 shot/s, 10 s buff: nothing ever expires ---------------------
        // The clock is what locking removes, so where no stack would have
        // expired anyway, locking must change NOTHING.
        let (earned, locked_none) = (mk(0, false, 1.0, 10.0), mk(0, true, 1.0, 10.0));
        assert!(
            (locked_none - earned).abs() < 1e-9,
            "locking a buff nothing was expiring changed it: {locked_none} vs {earned}"
        );
        // …and above all, it is not a way to switch the buff off.
        assert!(
            locked_none > without(1.0, 10.0),
            "a buff locked at 0 stacks still EARNS: {locked_none} vs {}",
            without(1.0, 10.0)
        );
        // Seeding it full only helps.
        assert!(mk(3, true, 1.0, 10.0) >= locked_none);

        // ---- one shot every 20 s, 10 s buff: the timeout bites -------------
        // Unlocked, every stack has expired before the next shot lands (and
        // the hit that grants a stack does not benefit from it), so the buff
        // is worth nothing. Locked, it climbs and HOLDS — which is the whole
        // of what the setting promises.
        let (slow_open, slow_locked) = (mk(0, false, 0.05, 100.0), mk(0, true, 0.05, 100.0));
        assert!(
            (slow_open - without(0.05, 100.0)).abs() < 1e-9,
            "a 10 s buff cannot survive 20 s between shots: {slow_open}"
        );
        assert!(
            slow_locked > slow_open,
            "NO TIMEOUT must hold the stacks across the gap: {slow_locked} vs {slow_open}"
        );
    }

    #[test]
    #[should_panic(expected = "cannot be an Eximus")]
    fn impossible_eximus_combination_panics_at_spawn() {
        let mut t = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
        t.eximus = true; // can_be_eximus is false -> impossible in-game
        let p = DummyParams {
            target: t,
            ..DummyParams::default()
        };
        let _ = run_once(&p, &mut Rng::new(1));
    }

    #[test]
    fn eximus_boosts_health_and_grants_overguard() {
        let mut t = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);
        t.can_be_eximus = true;
        t.eximus = true;
        t.level = 200;
        // Unarmored/unshielded: base health max(50*1.1, 0.375*(50+900)*6).
        let base = (50.0f64 * 1.1).max(0.375 * 950.0 * 6.0);
        let expect = base * t.health_curve.multiplier(199.0);
        assert!((t.max_health() - expect).abs() < 1e-6);
        // Eximus overguard: base 12, scaled.
        assert!(t.overguard() > 0.0);
    }

    #[test]
    fn instant_respawn_kills_every_shot_on_a_frail_target() {
        let p = DummyParams {
            target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            ..DummyParams::default()
        };
        let s = monte_carlo(&p, 200, 5);
        // 50 HP, no armor, no overguard: every shot (>= 75 raw) kills, and the
        // target respawns in place — 10 kills per 10-shot run, no variance.
        // Killing hits' procs are discarded, so no bleeds ever tick.
        assert!((s.mean_kills - 10.0).abs() < 1e-9, "kills {}", s.mean_kills);
        assert_eq!(s.std_kills, 0.0);
        assert_eq!((s.min_kills, s.max_kills), (10, 10));
        // The final target respawned untouched: no partial credit.
        assert!((s.mean_kill_progress - 10.0).abs() < 1e-9);
        assert_eq!(s.mean_dot_damage, 0.0);
        assert_eq!(s.mean_procs, 0.0);
    }

    #[test]
    fn infinite_health_never_dies_and_applies_armor_dr() {
        // 300 armor (>= the 200 spawn minimum, stays 300) -> post-U36 DR
        // = 0.9 * sqrt(300/2700) = 30%: effective is exactly 70% of raw,
        // and no kills. Status off so no armor-ignoring Cinematic ticks mix in.
        let p = DummyParams {
            target: frail_target(TargetMode::InfiniteHealth, 300.0, 0.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 200, 5);
        assert_eq!(s.mean_kills, 0.0);
        assert!(
            (s.mean_effective_damage - s.mean_damage * 0.7).abs() < 1e-9,
            "effective {} vs raw {}",
            s.mean_effective_damage,
            s.mean_damage
        );
    }

    #[test]
    fn bleed_ticks_ignore_armor_entirely() {
        // Forced Slash, capped armor (90% DR): direct hits take 0.1x but
        // ticks land at full value. crit off, mono 1x, 10 s:
        // direct effective = 750 × 0.1 = 75; dot effective = 1023.75 (full).
        let p = DummyParams {
            crit_multiplier: 1.0,
            forced_procs: vec![DamageType::Slash],
            body_parts: mono_body(1.0),
            target: frail_target(TargetMode::InfiniteHealth, 2700.0, 0.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 50, 5);
        assert!(
            (s.mean_dot_damage - 1023.75).abs() < 1e-9,
            "dot {}",
            s.mean_dot_damage
        );
        assert!(
            (s.mean_effective_damage - (75.0 + 1023.75)).abs() < 1e-9,
            "effective {}",
            s.mean_effective_damage
        );
    }

    #[test]
    fn armor_reduced_damage_floors_at_one_per_type() {
        // Tiny hits vs capped armor: 5 raw x (1 - 0.9) = 0.5 -> floored to
        // 1 (scalar hit = one damage type). crit_multiplier 1.0 keeps every
        // shot's raw at exactly base x part multiplier; pure-Impact vector so
        // the only possible proc is a harmless Stagger.
        let p = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 5.0),
            crit_multiplier: 1.0,
            body_parts: mono_body(1.0),
            target: frail_target(TargetMode::InfiniteHealth, 2700.0, 0.0),
            ..DummyParams::default()
        };
        let s = monte_carlo(&p, 100, 3);
        // 10 shots per run, each floored to exactly 1 effective damage.
        assert!(
            (s.mean_effective_damage - 10.0).abs() < 1e-9,
            "effective {}",
            s.mean_effective_damage
        );
    }

    #[test]
    fn overguard_ignores_armor_and_does_not_spill() {
        // Huge armor but active overguard: hits are neutral (effective == raw)
        // until overguard breaks; the pool is large enough to absorb all runs.
        // (Bleed ticks land on overguard at full value too.)
        let mut t = frail_target(TargetMode::InfiniteHealth, 2700.0, 1e12);
        t.base_health = 1.0;
        let p = DummyParams {
            target: t,
            ..DummyParams::default()
        };
        let s = monte_carlo(&p, 100, 9);
        assert_eq!(s.mean_kills, 0.0);
        assert!(
            (s.mean_effective_damage - s.mean_damage).abs() < 1e-9,
            "overguard hits must be unmitigated"
        );
    }

    #[test]
    fn default_training_dummy_passes_damage_through() {
        let s = monte_carlo(&DummyParams::default(), 200, 21);
        assert_eq!(s.mean_kills, 0.0);
        assert!((s.mean_effective_damage - s.mean_damage).abs() < 1e-9);
    }

    #[test]
    fn incarnon_procs_at_the_listed_rate_per_pellet() {
        // Incarnon profile vs the plain dummy: 43% SC per PELLET (multishot
        // 2.0 doubles opportunities, not the per-pellet chance).
        let s = monte_carlo(&DummyParams::dual_toxocyst_incarnon(), 500, 11);
        let per_pellet = s.mean_procs / s.mean_pellets;
        assert!(
            (per_pellet - 0.43).abs() < 0.02,
            "procs/pellet {per_pellet}"
        );
        // Bleeds are flowing and feeding damage.
        assert!(s.mean_dot_damage > 0.0);
    }

    #[test]
    fn thrax_9999_takes_everything_on_overguard_neutrally() {
        // THE ULTIMATE STRESS TEST benchmark: Thrax @9999 STEEL PATH
        // (9.67M health behind 15.5M overguard) - every instance (direct
        // pellets AND Cinematic bleed ticks) lands on the neutral Overguard
        // pool, so effective == raw exactly, and nothing ever dies.
        let spec = crate::enemy_data::EnemySpec::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/enemies/thrax_centurion.yaml"
        )))
        .unwrap();
        let p = DummyParams {
            target: spec
                .target_params(9999, true, false, TargetMode::InstantRespawn)
                .unwrap(),
            duration_secs: 60.0,
            ..DummyParams::dual_toxocyst_incarnon()
        };
        let s = monte_carlo(&p, 50, 12);
        assert_eq!(s.mean_kills, 0.0);
        assert!(
            (s.mean_effective_damage - s.mean_damage).abs() < 1e-6,
            "overguard must be neutral: eff {} raw {}",
            s.mean_effective_damage,
            s.mean_damage
        );
        assert!(
            s.mean_procs > 0.0,
            "procs must still apply behind overguard"
        );
    }

    /// No-arcane, crit-off, mono-1x profile for exact payload expectations.
    fn bare(forced: DamageType) -> DummyParams {
        DummyParams {
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            forced_procs: vec![forced],
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            ..no_status()
        }
    }

    #[test]
    fn viral_amps_health_damage_live() {
        // Forced Viral on the infinite dummy at 1 shot/s: stacks expire
        // after 6 s, so the count before shot k is 0,1,2,3,4,5 then a
        // steady 5. Amps 1,2,2.25,2.5,2.75,3,3,3,3,3 -> Σ = 25.5.
        let s = monte_carlo(&bare(DamageType::Viral), 20, 3);
        assert!(
            (s.mean_effective_damage - 75.0 * 25.5).abs() < 1e-9,
            "eff {}",
            s.mean_effective_damage
        );
        // Raw is pre-mitigation: the amp is defender-side.
        assert!((s.mean_damage - 750.0).abs() < 1e-9);
    }

    #[test]
    fn magnetic_amps_overguard_damage_live() {
        // Same amp curve, but on the overguard pool.
        let p = DummyParams {
            target: frail_target(TargetMode::InfiniteHealth, 0.0, 1e12),
            ..bare(DamageType::Magnetic)
        };
        let s = monte_carlo(&p, 20, 3);
        assert!(
            (s.mean_effective_damage - 75.0 * 25.5).abs() < 1e-9,
            "eff {}",
            s.mean_effective_damage
        );
    }

    // ---- System B: the faction vulnerability column ----------------------
    // Keyed by `FactionDamageOverride ?? Faction`, per COMPONENT, and chosen
    // by the POOL the damage lands on (docs/MECHANICS.md §8).

    /// The infinite dummy, wearing one faction's column.
    fn column_dummy(key: &str, overguard: f64) -> TargetParams {
        TargetParams {
            type_mods: crate::factions_data::columns_for(key),
            ..frail_target(TargetMode::InfiniteHealth, 0.0, overguard)
        }
    }

    /// One run's effective damage from a fixed vector against one column.
    fn eff_vs(key: &str, v: DamageVector) -> f64 {
        let p = DummyParams {
            damage: v,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            target: column_dummy(key, 0.0),
            ..no_status()
        };
        monte_carlo(&p, 20, 3).mean_effective_damage
    }

    #[test]
    fn the_vulnerability_column_scales_each_component_on_its_own() {
        let impact = DamageVector::new().with(DamageType::Impact, 100.0);
        let neutral = eff_vs("unknown", impact);
        assert!(neutral > 0.0);

        // Grineer: Impact ×1.5 (and Corrosive, which this hit has none of).
        assert!(
            (eff_vs("grineer", impact) - neutral * 1.5).abs() < 1e-6,
            "grineer impact {}",
            eff_vs("grineer", impact)
        );
        // A RESISTANCE is the same mechanism pointing down.
        let rad = DamageVector::new().with(DamageType::Radiation, 100.0);
        assert!(
            (eff_vs("orokin", rad) - eff_vs("unknown", rad) * 0.5).abs() < 1e-6,
            "orokin radiation"
        );
        // Per COMPONENT, not per hit: half a vector at ×1.5 is ×1.25 overall.
        // This is the "dilution" the wiki means — compositional, not a bucket.
        let mixed = DamageVector::new()
            .with(DamageType::Impact, 50.0)
            .with(DamageType::Slash, 50.0);
        assert!(
            (eff_vs("grineer", mixed) - eff_vs("unknown", mixed) * 1.25).abs() < 1e-6,
            "mixed {} vs {}",
            eff_vs("grineer", mixed),
            eff_vs("unknown", mixed)
        );
        // A faction with nothing to say about this type changes nothing.
        assert!((eff_vs("stalker", impact) - neutral).abs() < 1e-9);
    }

    /// The damage-by-type breakdown is what a reader consults to decide what
    /// to add next, so it splits by what each component actually DID.
    #[test]
    fn the_by_type_breakdown_follows_the_column_too() {
        let v = DamageVector::new()
            .with(DamageType::Impact, 50.0)
            .with(DamageType::Slash, 50.0);
        let grineer = crate::factions_data::column("grineer");
        let mut dst = [0.0f64; 15];
        // 125 effective is what 50 Impact ×1.5 + 50 Slash ×1.0 comes to.
        add_by_type(&mut dst, &v, 125.0, &grineer);
        assert!((dst[DamageType::Impact as usize] - 75.0).abs() < 1e-9, "{dst:?}");
        assert!((dst[DamageType::Slash as usize] - 50.0).abs() < 1e-9, "{dst:?}");
        // Neutral: the plain proportional split it always was.
        let mut flat = [0.0f64; 15];
        add_by_type(&mut flat, &v, 100.0, &crate::factions_data::Column::NEUTRAL);
        assert!((flat[DamageType::Impact as usize] - 50.0).abs() < 1e-9);
    }

    /// Overguard is a LAYER over the unit, with its own table — the unit's
    /// column must not reach it, and Void must.
    #[test]
    fn overguard_reads_its_own_column_not_the_units() {
        let shot = |key: &str, t: DamageType| {
            let p = DummyParams {
                damage: DamageVector::new().with(t, 100.0),
                crit_multiplier: 1.0,
                base_crit_chance: 0.0,
                arcane: ArcaneFx::none(),
                body_parts: mono_body(1.0),
                target: column_dummy(key, 1e12),
                ..no_status()
            };
            monte_carlo(&p, 20, 3).mean_effective_damage
        };
        let base = shot("unknown", DamageType::Impact);
        // Grineer's Impact ×1.5 stops at the overguard layer.
        assert!((shot("grineer", DamageType::Impact) - base).abs() < 1e-9);
        // Void ×1.5 is the Overguard column's own entry, on any unit.
        assert!((shot("grineer", DamageType::Void) - base * 1.5).abs() < 1e-6);
        assert!((shot("unknown", DamageType::Void) - base * 1.5).abs() < 1e-6);
    }

    /// Bleed is stored under Slash — the proc that made it — but the damage is
    /// CINEMATIC, which takes no faction modifier anywhere. An Infested target
    /// (Slash ×1.5) must boost the HIT and not the bleed it leaves.
    #[test]
    fn bleed_takes_no_faction_modifier_though_it_is_filed_under_slash() {
        let run = |key: &str| {
            let p = DummyParams {
                damage: DamageVector::new().with(DamageType::Slash, 100.0),
                target: column_dummy(key, 0.0),
                ..bare(DamageType::Slash)
            };
            let s = monte_carlo(&p, 20, 3);
            (s.mean_effective_damage - s.mean_dot_damage, s.mean_dot_damage)
        };
        let (direct_n, dot_n) = run("unknown");
        let (direct_i, dot_i) = run("infested");
        assert!(dot_n > 0.0 && direct_n > 0.0, "the fixture must do both");
        // The direct Slash hit takes the column…
        assert!(
            (direct_i - direct_n * 1.5).abs() < 1e-6,
            "direct {direct_i} vs {direct_n}"
        );
        // …and the bleed it leaves does not.
        assert!((dot_i - dot_n).abs() < 1e-9, "dot {dot_i} vs {dot_n}");
    }

    /// The column and Toxin's shield bypass are two readings of ONE shape:
    /// the Toxin part is scaled by Toxin's factor on its way past the shield,
    /// the rest by theirs on the way into it.
    #[test]
    fn the_column_follows_each_component_into_the_pool_it_lands_in() {
        // Narmer: Toxin ×1.5, Magnetic ×0.5. Seven shots of 16 Toxin + 16
        // Magnetic into a shield that never breaks and 160 health:
        //   neutral  7 × 16       = 112  -> alive
        //   narmer   7 × 16 × 1.5 = 168  -> dead
        // The Magnetic half never reaches health under either column.
        let kills = |key: &str| {
            let p = DummyParams {
                damage: DamageVector::new()
                    .with(DamageType::Toxin, 16.0)
                    .with(DamageType::Magnetic, 16.0),
                crit_multiplier: 1.0,
                base_crit_chance: 0.0,
                arcane: ArcaneFx::none(),
                body_parts: mono_body(1.0),
                fire_rate: 10.0,
                duration_secs: 0.65,
                magazine_size: 100.0,
                target: TargetParams {
                    type_mods: crate::factions_data::columns_for(key),
                    base_shield: 10_000.0,
                    base_health: 160.0,
                    ..frail_target(TargetMode::InstantRespawn, 0.0, 0.0)
                },
                ..no_status()
            };
            monte_carlo(&p, 20, 5).mean_kills
        };
        assert_eq!(kills("unknown"), 0.0, "112 damage must not kill 160 health");
        assert_eq!(kills("narmer"), 1.0, "Toxin x1.5 past the shield kills it");
    }

    #[test]
    fn toxin_dot_matches_the_bleed_shape_at_half_base() {
        // Toxin mirrors the forced-bleed test at coefficient 0.5 vs 0.35:
        // 39 ticks × 37.5 = 1462.5 (no armor: full value).
        let s = monte_carlo(&bare(DamageType::Toxin), 20, 5);
        assert!(
            (s.mean_dot_damage - 1462.5).abs() < 1e-9,
            "dot {}",
            s.mean_dot_damage
        );
    }

    #[test]
    fn electricity_dot_ticks_immediately() {
        // Delay-0: shot at k ticks at k..k+5; before 10 s that is
        // 6+6+6+6+6+5+4+3+2+1 = 45 ticks × 37.5 = 1687.5.
        let s = monte_carlo(&bare(DamageType::Electricity), 20, 5);
        assert!(
            (s.mean_dot_damage - 45.0 * 37.5).abs() < 1e-9,
            "dot {}",
            s.mean_dot_damage
        );
    }

    #[test]
    fn heat_is_a_single_refreshing_accumulator() {
        // Ten forced Heat procs, one per second: ONE entity born at t=0,
        // each proc adds 37.5 to the tick and refreshes the expiry, ticks
        // anchored at 1,2,...,9 (< 10 s): tick k has k contributions ->
        // Σ k=1..9 of 37.5k = 37.5 × 45 = 1687.5. Same total as the
        // independent-stack Electricity case ONLY by coincidence of this
        // cadence — the entity count differs (1 vs 10).
        let s = monte_carlo(&bare(DamageType::Heat), 20, 5);
        assert!(
            (s.mean_dot_damage - 1687.5).abs() < 1e-9,
            "dot {}",
            s.mean_dot_damage
        );
    }

    #[test]
    fn condition_overload_multiplies_by_distinct_status_types() {
        // Forced Impact (Stagger) with co_per_type = 1.0, no base-damage
        // mods: shot 1 sees 0 types, shots 2..10 see 1 ->
        // 75 × (1 + 9 × 2) = 1425.
        let p = DummyParams {
            co_per_type: 1.0,
            ..bare(DamageType::Impact)
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 1425.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn condition_overload_is_diluted_by_base_damage_mods() {
        // Additive class: with +100% base damage, one status type gives
        // (1 + 1 + 1)/(1 + 1) = 1.5× instead of 2× ->
        // 75 × (1 + 9 × 1.5) = 1087.5.
        let p = DummyParams {
            base_damage_bonus: 1.0,
            co_per_type: 1.0,
            ..bare(DamageType::Impact)
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 1087.5).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn condition_overload_behavior_classes_differ_per_weapon() {
        use crate::loadout::CoBehavior;
        // Same +100% base damage, one active type. Independent ignores
        // the dilution: 75 × (1 + 9 × 2) = 1425. Inert: 75 × 10 = 750.
        let p = |b| DummyParams {
            base_damage_bonus: 1.0,
            co_per_type: 1.0,
            co_behavior: b,
            ..bare(DamageType::Impact)
        };
        let ind = monte_carlo(&p(CoBehavior::Independent), 20, 5);
        assert!((ind.mean_damage - 1425.0).abs() < 1e-9);
        let inert = monte_carlo(&p(CoBehavior::Inert), 20, 5);
        assert!((inert.mean_damage - 750.0).abs() < 1e-9);
    }

    #[test]
    fn cold_raises_crit_damage_received() {
        // Forced Cold, cc 100%, cd 2.0: stack counts 0,1,...,5 then a
        // steady 5 (6 s expiry) -> b = 0,.10,.15,.20,.25,.30 then .30
        // (Σ = 2.20); total = 75 × Σ(2 + b) = 75 × 22.2.
        let p = DummyParams {
            base_crit_chance: 1.0,
            crit_multiplier: 2.0,
            forced_procs: vec![DamageType::Cold],
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 75.0 * 22.2).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn blast_stacks_fire_singly_on_fuse_expiry() {
        // Forced Blast at 1 shot/s: each stack's 1.5 s fuse fires a
        // 0.3 × 75 = 22.5 hit; fuses at 1.5..9.5 land before 10 s = 9 of
        // 10; never 10 simultaneous stacks -> no early detonation.
        let s = monte_carlo(&bare(DamageType::Blast), 20, 5);
        assert!(
            (s.mean_dot_damage - 9.0 * 22.5).abs() < 1e-9,
            "burst {}",
            s.mean_dot_damage
        );
    }

    #[test]
    fn overguard_break_with_disrupt_fires_the_tesla_payload() {
        // 100 overguard, forced Magnetic (InstantRespawn so pools deplete;
        // health big enough that nothing dies): shot 1 (amp 1, no stacks
        // yet) leaves 25 and lands a stack; shot 2 (amp 2.0) breaks ->
        // break proc = 3% × 1 stack × 100 = 3 total over 6 ticks. Health
        // then takes shots 3..10 at amp 1: dot damage == 3 exactly.
        let mut t = frail_target(TargetMode::InstantRespawn, 0.0, 100.0);
        t.base_health = 10_000.0;
        let p = DummyParams {
            target: t,
            ..bare(DamageType::Magnetic)
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_dot_damage - 3.0).abs() < 1e-9,
            "break proc {}",
            s.mean_dot_damage
        );
    }

    /// THE CYCLE, EARNED — and the same fixture opening primed, for contrast.
    ///
    /// The engagement starts in the BASE form and pays for its first transmute
    /// like everything else consumable in this sim. It used to open transformed
    /// with a full charge magazine, which is a gift a fight should not make;
    /// `starts_primed` keeps that reading available and this test pins both, so
    /// the difference between them is a number rather than a memory.
    #[test]
    fn incarnon_cycle_alternates_forms_deterministically() {
        // Incarnon: 100 dmg, mag 2, 1/s. Base: 50 dmg, aim 100% head
        // (each pellet charges), 2 charges to fill, revert 0.5 s,
        // transmute 1.0 s.
        //
        // EARNED, over 12 s:
        //   base @0,1 -> transmute -> inc @3,4 | revert 5->5.5
        //   base @5.5,6.5 -> transmute -> inc @8.5,9.5 | revert 10.5->11
        //   base @11. Totals: 4x100 + 5x50 = 650; 9 shots; 2 transforms
        //   (transmutes INTO the form only — the reverts do not count).
        //
        // PRIMED, same window: inc @0,1 | revert 2->2.5 | base @2.5,3.5 ->
        //   transmute -> inc @5.5,6.5 | revert 7.5->8 | base @8,9 -> transmute
        //   -> inc @11. 5x100 + 4x50 = 700, one Incarnon shot traded for one
        //   base shot.
        //
        // THE SHOT THAT FILLS THE GAUGE PAYS ITS OWN INTERVAL, which is why the
        // Incarnon rounds land at 3 and not at 2. The transform used to
        // `continue` past the cadence, so the completing shot was followed
        // IMMEDIATELY by an Incarnon one and every transform was worth a free
        // shot (owner, 2026-08-10: "变身的时机应该是在完成之后射击的末尾（也就
        // 是下次射击的开头）").
        //
        // The window is 12 s rather than 10 for a reason worth keeping: at 10 s
        // the two readings TIE at 600, because the primed run's free magazine is
        // exactly given back by where the clock falls. A fixture that ties
        // cannot show what priming is worth, and the tie is an artefact of the
        // window rather than a fact about the gift.
        let head = vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 1.0, // 1x so no crit-location bonus, pure counts
            is_head: true,
            crit_bonus: false,
        }];
        let base_form = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 50.0),
            crit_multiplier: 1.0,
            body_parts: head.clone(),
            ..no_status()
        };
        let p = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            crit_multiplier: 1.0,
            magazine_size: 2.0,
            ammo_efficiency_applies: false,
            arcane: ArcaneFx::none(),
            body_parts: head,
            cycle: Some(IncarnonCycle {
                // These fixtures test the EARNED cycle, which is the standard one.
                starts_primed: false,
                base_form: Box::new(base_form),
                charge_on: crate::loadout::ChargeOn::WeakpointHits,
                charges_to_fill: 2,
                transmute_out_seconds: 0.5,
                transmute_seconds: 1.0,
                reload_bucket: 0.0,
            }),
            ..no_status()
        };
        let p = DummyParams { duration_secs: 12.0, ..p };
        let s = monte_carlo(&p, 5, 9);
        assert!((s.mean_damage - 650.0).abs() < 1e-9, "earned dmg {}", s.mean_damage);
        assert!((s.mean_shots - 9.0).abs() < 1e-9, "shots {}", s.mean_shots);
        assert!((s.mean_transforms - 2.0).abs() < 1e-9);
        assert_eq!(s.mean_reloads, 0.0);

        // ...and the reading that walks in already charged.
        let mut primed = p.clone();
        if let Some(c) = primed.cycle.as_mut() {
            c.starts_primed = true;
        }
        let q = monte_carlo(&primed, 5, 9);
        assert!((q.mean_damage - 700.0).abs() < 1e-9, "primed dmg {}", q.mean_damage);
        // The FREE MAGAZINE, priced: one Incarnon shot traded for one base
        // shot, over an engagement this short. On a real weapon at a headshot
        // rate that can never refill the gauge it is the whole of its Incarnon
        // damage.
        assert!(q.mean_damage > s.mean_damage, "the gift is worth something");
    }

    /// `charge_on` is WEAPON DATA, not a constant. It used to be documented in
    /// the yaml and ignored by the loader, so every weapon charged off
    /// weak-point hits — wrong for the Torid, which the wiki charges through
    /// plain direct hits ("Angstrum Incarnon Genesis and Torid Incarnon Genesis
    /// are instead charged through direct hits").
    ///
    /// Body-only aim is the discriminator: there are no weak-point hits at all,
    /// so a `WeakpointHits` weapon never transforms again and a `DirectHits`
    /// one keeps cycling.
    #[test]
    fn the_gauge_charges_off_whatever_the_weapon_data_says() {
        let base_form = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 50.0),
            crit_multiplier: 1.0,
            body_parts: mono_body(1.0), // NO heads: no weak-point hits ever
            ..no_status()
        };
        let mk = |charge_on| DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            crit_multiplier: 1.0,
            magazine_size: 2.0,
            ammo_efficiency_applies: false,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            cycle: Some(IncarnonCycle {
                // These fixtures test the EARNED cycle, which is the standard one.
                starts_primed: false,
                base_form: Box::new(base_form.clone()),
                charge_on,
                charges_to_fill: 2,
                transmute_out_seconds: 0.5,
                transmute_seconds: 1.0,
                reload_bucket: 0.0,
            }),
            ..no_status()
        };
        let wp = monte_carlo(&mk(crate::loadout::ChargeOn::WeakpointHits), 5, 9);
        assert_eq!(
            wp.mean_transforms, 0.0,
            "no weak-point hits = the gauge never fills again"
        );
        let direct = monte_carlo(&mk(crate::loadout::ChargeOn::DirectHits), 5, 9);
        assert!(
            direct.mean_transforms > 0.0,
            "plain direct hits must fill a direct-hit gauge (transforms {})",
            direct.mean_transforms
        );
    }

    /// THE EXPLOSION BELONGS TO THE FORM THAT HAS ONE. A cycle fires two
    /// different weapons in turn, and the radial stage was read off the OUTER
    /// params — which are the Incarnon form's — for every shot, base phase
    /// included. So a weapon whose Incarnon detonates threw that explosion on
    /// every base-form shot as well, for free and forever.
    ///
    /// Where it showed: a board pinned to a 0% headshot rate, where eight of
    /// the nine Incarnon forms never charge at all. Measured on the real
    /// Burston Prime (Serration only, 4 s, 400 runs, zero headshots, ZERO
    /// transforms on both sides): `incarnon_cycle` 2470 DPS against a pinned
    /// base form's 1738, +42%, the whole gap in a `radial` source dealing Heat
    /// — an element the base form has nowhere in its vector. See MEASUREMENTS
    /// M32.
    ///
    /// Body-only aim is the discriminator again, for the same reason as the
    /// test above: no weak-point hits, so the gauge never fills and the
    /// engagement is base form from end to end. What it must equal is the SAME
    /// base form fired on its own.
    #[test]
    fn a_cycle_that_never_transforms_is_its_base_form() {
        let base_form = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 50.0),
            crit_multiplier: 1.0,
            body_parts: mono_body(1.0), // NO heads: the gauge can never fill
            ..no_status()
        };
        let cycling = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            crit_multiplier: 1.0,
            magazine_size: 2.0,
            ammo_efficiency_applies: false,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            // The Incarnon form's, and ONLY the Incarnon form's — `base_form`
            // above declares none.
            radial: Some(radial_of(0.0, 0.0)),
            cycle: Some(IncarnonCycle {
                starts_primed: false,
                base_form: Box::new(base_form.clone()),
                charge_on: crate::loadout::ChargeOn::WeakpointHits,
                charges_to_fill: 2,
                transmute_out_seconds: 0.5,
                transmute_seconds: 1.0,
                reload_bucket: 0.0,
            }),
            ..no_status()
        };
        let cyc = monte_carlo(&cycling, 5, 9);
        let alone = monte_carlo(&base_form, 5, 9);
        assert_eq!(cyc.mean_transforms, 0.0, "the fixture must never transform");
        assert_eq!(
            cyc.mean_damage, alone.mean_damage,
            "a cycle stuck in its base form must deal exactly what that form deals              ({} vs {}) — the difference is the other form's explosion",
            cyc.mean_damage, alone.mean_damage
        );
    }

    #[test]
    fn initial_lock_grants_frenzy_once_then_mechanics_rule() {
        // Body-only aim (no natural headshots), Frenzy at Initial: the
        // t=0 grant runs out at 3 s. Shots at 0,0.4,...,2.8 (8) then
        // 3.2,4.2,...,9.2 (7) = 15 — vs 25 Permanent, 10 unlocked.
        let p = DummyParams {
            frenzy: true,
            locked_buffs: vec![BuffLock::initial(LockedBuff::Frenzy, 1)],
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 4);
        assert!((s.mean_shots - 15.0).abs() < 1e-9, "shots {}", s.mean_shots);
    }

    #[test]
    fn galvanized_decay_loses_one_stack_and_resets_duration() {
        let mut s = LiveStacks {
            stacks: 3,
            expiry: 5.0,
            each: Vec::new(),
            per_stack: false,
        };
        assert_eq!(s.current(4.9, 10.0), 3);
        assert_eq!(s.current(5.1, 10.0), 2); // lost one, next decay at 15
        assert_eq!(s.current(14.9, 10.0), 2);
        assert_eq!(s.current(15.1, 10.0), 1);
        assert_eq!(s.current(26.0, 10.0), 0);
    }

    #[test]
    fn emergent_multishot_stacks_are_earned_by_kills_from_zero() {
        // Cold-start config (initial 0): frail 50 HP target dies to every
        // pellet; +1.0 pellet per stack, cap 2, long duration. Shot k
        // fires (1 + stacks) pellets and the FIRST pellet's kill bumps
        // the stack: pellets per shot 1, 2, 3, 3, ... = 1 + 2 + 8×3 = 27.
        let spec = crate::loadout::StackSpec {
            per_stack: 1.0,
            max_stacks: 2,
            duration: 100.0,
            initial_stacks: 0,
        };
        let p = DummyParams {
            ms_stack: Some(spec),
            target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            arcane: ArcaneFx::none(),
            crit_multiplier: 1.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_pellets - 27.0).abs() < 1e-9,
            "pellets {}",
            s.mean_pellets
        );

        // Initial-full (the user's default): every shot fires 3 pellets
        // from t = 0 (kills keep the stacks refreshed) -> 30 pellets.
        let full = DummyParams {
            ms_stack: Some(crate::loadout::StackSpec {
                initial_stacks: 2,
                ..spec
            }),
            ..p
        };
        let s2 = monte_carlo(&full, 20, 5);
        assert!(
            (s2.mean_pellets - 30.0).abs() < 1e-9,
            "pellets {}",
            s2.mean_pellets
        );
    }

    #[test]
    fn co_base_fraction_scales_the_co_bonus() {
        // Additive class, bd 0, fraction 0.6, one active type:
        // 75 × (1 + 9 × (1 + 0.6)) = 75 × 15.4 = 1155.
        let p = DummyParams {
            co_per_type: 1.0,
            co_base_fraction: 0.6,
            ..bare(DamageType::Impact)
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 1155.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn deadhead_adds_base_damage_stacks_and_headshot_bonus() {
        // Deadhead full stacks (initial): arc bd = 3 × 1.2 = 3.6 -> ratio
        // 4.6 (bd 0). Headshot bonuses multiply the base multiplier via an
        // additive bracket (Enemy_Body_Parts verbatim: 3 × (1 + 30% + …)):
        // this 3x head becomes 3.9x.
        // 10 shots × 75 × 3.9 × 4.6 = 13,455. No kills: no decay in 10 s.
        let p = DummyParams {
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: arc_stacked("secondary_deadhead"),
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 3.0,
                is_head: true,
                crit_bonus: false,
            }],
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 13_455.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn cascadia_flare_hard_resets_without_fresh_heat() {
        // Initial 40 stacks (+480% -> ×5.8), 10 s shared timer. 15 s at
        // 1/s with the 12-round magazine: shots at 0..11, reload 2.35 s,
        // one more at 14.35 (13 shots). Without Heat procs (forced
        // Impact): only t < 10 boosted: 10×435 + 3×75 = 4575.
        let starved = DummyParams {
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: arc_stacked("cascadia_flare"),
            forced_procs: vec![DamageType::Impact],
            body_parts: mono_body(1.0),
            duration_secs: 15.0,
            ..no_status()
        };
        let s = monte_carlo(&starved, 20, 5);
        assert!(
            (s.mean_damage - 4575.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
        // Forced Heat procs refresh the shared timer every shot (the
        // 2.35 s reload gap is well under 10 s): all 13 direct shots
        // boosted = 13 × 435 = 5655. The Heat singleton itself also
        // benefits (mb_live): each proc adds 0.5 × 435 = 217.5 to the
        // tick; ticks at 1..14 carry Σ min(k,12) = 102 contributions →
        // DoT = 22,185; total 27,840.
        let sustained = DummyParams {
            forced_procs: vec![DamageType::Heat],
            ..starved
        };
        let s2 = monte_carlo(&sustained, 20, 5);
        assert!(
            (s2.mean_dot_damage - 22_185.0).abs() < 1e-9,
            "dot {}",
            s2.mean_dot_damage
        );
        assert!(
            (s2.mean_damage - 27_840.0).abs() < 1e-9,
            "dmg {}",
            s2.mean_damage
        );
    }

    #[test]
    fn merciless_stacks_join_the_base_damage_bucket_and_decay_one_by_one() {
        // Full 12 stacks × 30% = +360% -> ratio 4.6 (bd 0). Within the
        // first 4 s no decay: 4 shots × 75 × 4.6 = 1380.
        let p = DummyParams {
            arcane: arc_stacked("secondary_merciless"),
            duration_secs: 3.9,
            ..flat_base()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 1380.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
        // Kill family without kills: lose ONE stack per 4 s timeout.
        // Shots t0-3 @12, t4-7 @11, t8-9 @10 stacks:
        // 75 × (4×4.6 + 4×4.3 + 2×4.0) = 3270.
        let p10 = DummyParams {
            duration_secs: 10.0,
            ..p
        };
        let s10 = monte_carlo(&p10, 20, 5);
        assert!(
            (s10.mean_damage - 3270.0).abs() < 1e-9,
            "dmg {}",
            s10.mean_damage
        );
    }

    #[test]
    fn conjunction_voltage_adds_multishot_and_reload_speed() {
        // 40 stacks × 3% multishot = +120% -> 2.2 expected pellets/shot;
        // forced Electricity keeps the shared 12 s timer refreshed.
        let p = DummyParams {
            arcane: arc_stacked("conjunction_voltage"),
            forced_procs: vec![DamageType::Electricity],
            ..flat_base()
        };
        let s = monte_carlo(&p, 2000, 7);
        let per_shot = s.mean_pellets / s.mean_shots;
        assert!((per_shot - 2.2).abs() < 0.05, "pellets/shot {per_shot}");
        // Reload speed: 40 × 1.5% = +60% -> 2.35 s / 1.6. A 2-round
        // magazine over 20 s fits in more shots than without the arcane.
        let slow = DummyParams {
            magazine_size: 2.0,
            duration_secs: 20.0,
            forced_procs: vec![DamageType::Electricity],
            ..flat_base()
        };
        let fast = DummyParams {
            arcane: arc_stacked("conjunction_voltage"),
            ..slow.clone()
        };
        let a = monte_carlo(&slow, 20, 5);
        let b = monte_carlo(&fast, 20, 5);
        assert!(
            b.mean_shots > a.mean_shots,
            "shots {} vs {}",
            b.mean_shots,
            a.mean_shots
        );
    }

    #[test]
    fn shiver_adds_damage_per_cold_status_on_the_target() {
        // Forced Cold procs land AFTER each shot and last 6 s each: shot k
        // sees min(k, 5) live stacks (older procs lapse), Σ = 35. GunCO
        // bracket on the additive-with-bd weapon:
        // 75 × Σ(1 + 0.45 × stacks) = 75 × 25.75 = 1931.25.
        let p = DummyParams {
            arcane: arc("secondary_shiver"),
            forced_procs: vec![DamageType::Cold],
            ..flat_base()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 1931.25).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn shiver_is_scaled_by_the_gunco_base_fraction() {
        // GunCO sources compute on the ORIGINAL base, excluding evolution
        // flat damage (wiki CO catalog) — Shiver is one of them, so its
        // per-stack bonus scales by co_base_fraction like Galvanized Shot's.
        // Same setup as above with fraction 0.5: 75 × Σ(1 + 0.45×0.5×min(k,5))
        // = 75 × (10 + 0.225 × 35) = 1340.625.
        let p = DummyParams {
            arcane: arc("secondary_shiver"),
            forced_procs: vec![DamageType::Cold],
            co_base_fraction: 0.5,
            ..flat_base()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 1340.625).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn fortifier_multiplies_damage_while_overguard_holds() {
        // ×9 on every direct hit while the (infinite) overguard is up:
        // 10 × 75 × 9 = 6750. NINE, not eight — the card's "x8" is the EXTRA
        // (MEASUREMENTS M38, owner 2026-08-09).
        let mut t = TargetParams::training_dummy();
        t.base_overguard = 1e9;
        let p = DummyParams {
            arcane: arc("secondary_fortifier"),
            target: t,
            ..flat_base()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 6750.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn empowered_adds_a_flat_instance_per_applied_status() {
        // Each forced Impact proc adds +750 flat (unscaled by mods/crit):
        // 10 × (75 + 750) = 8250.
        let p = DummyParams {
            arcane: arc("cascadia_empowered"),
            forced_procs: vec![DamageType::Impact],
            ..flat_base()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 8250.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn encumber_rolls_one_extra_random_status_per_pull() {
        // 24% chance per proc-carrying pull, at most one: procs/run ≈
        // 10 × 1.24.
        let p = DummyParams {
            arcane: arc("secondary_encumber"),
            forced_procs: vec![DamageType::Impact],
            ..flat_base()
        };
        let s = monte_carlo(&p, 4000, 11);
        assert!((s.mean_procs - 12.4).abs() < 0.3, "procs {}", s.mean_procs);
    }

    /// The wiki states Encumber's per-shot rate as a CLOSED FORM, and it is
    /// about multishot — the one thing the single-pellet test above cannot
    /// see:
    ///
    ///   1 − (1 − chance × min(statusChance, 1)) ^ pelletCount
    ///
    /// Every pellet that applied a status rolls `chance`, first success wins.
    /// With forced procs each of 3 pellets always applies one, so the rate is
    /// 1 − 0.76³ = 0.561024, and a 10-shot run lands
    /// 10 × (3 forced + 0.561024) = 35.61 procs. The value of pinning this is
    /// that the naive readings both give a different number: one roll per
    /// PULL would give 10 × 3.24 = 32.4, and one roll per PELLET with no
    /// per-instant limit would give 10 × 3.72 = 37.2.
    #[test]
    fn encumbers_per_shot_rate_matches_the_wikis_closed_form_under_multishot() {
        let p = DummyParams {
            arcane: arc("secondary_encumber"),
            forced_procs: vec![DamageType::Impact],
            multishot: 3.0,
            ..flat_base()
        };
        let s = monte_carlo(&p, 4000, 17);
        let want = 10.0 * (3.0 + (1.0 - 0.76_f64.powi(3)));
        assert!(
            (s.mean_procs - want).abs() < 0.3,
            "procs {} vs closed form {want}",
            s.mean_procs
        );
        // And it must NOT be either naive reading.
        assert!((s.mean_procs - 32.4).abs() > 0.5, "one roll per PULL");
        assert!((s.mean_procs - 37.2).abs() > 0.5, "one roll per PELLET, uncapped");
    }

    #[test]
    fn cryogenic_cold_bursts_raise_crit_damage_received() {
        // Rank 5: every Puncture status also applies 3 Cold stacks; Cold
        // raises crit damage RECEIVED, so a guaranteed-crit run does more
        // damage with the arcane than without.
        let base = DummyParams {
            base_crit_chance: 1.0,
            crit_multiplier: 2.0,
            forced_procs: vec![DamageType::Puncture],
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            ..no_status()
        };
        let with = DummyParams {
            arcane: arc("secondary_cryogenic"),
            ..base.clone()
        };
        let a = monte_carlo(&base, 300, 5);
        let b = monte_carlo(&with, 300, 5);
        assert!(
            b.mean_damage > a.mean_damage * 1.05,
            "with {} vs without {}",
            b.mean_damage,
            a.mean_damage
        );
    }

    #[test]
    fn surge_assumed_max_is_a_final_multiplier() {
        // AssumedMax: the ×8 cap on every shot — 10 × 75 × 8 = 6000.
        let fx = crate::arcanes_data::secondary("secondary_surge")
            .unwrap()
            .fx(5, crate::loadout::StackPolicy::AssumedMax, &[], crate::tenno_data::default_tenno());
        let p = DummyParams {
            arcane: fx,
            ..flat_base()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 6000.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn hemorrhage_converts_impact_procs_to_bleeds() {
        // Fire rate 1 < 2.5: chance 0.35 × 2 = 0.7 per damage instance —
        // forced Impact procs seed Slash bleeds (35% ticks of ModifiedBase).
        let with = DummyParams {
            proc_conversion: Some(crate::loadout::ProcConv {
                from: DamageType::Impact,
                to: DamageType::Slash,
                chance: 0.35,
                low_rate_threshold: 2.5,
                low_rate_mult: 2.0,
            }),
            forced_procs: vec![DamageType::Impact],
            ..flat_base()
        };
        let without = DummyParams {
            proc_conversion: None,
            ..with.clone()
        };
        let a = monte_carlo(&without, 200, 5);
        let b = monte_carlo(&with, 200, 5);
        assert!(a.mean_dot_damage == 0.0, "impact alone must not bleed");
        assert!(b.mean_dot_damage > 0.0, "hemorrhage must bleed");
        // ~0.7 of 10 shots convert; ticks land at t+1..t+6 but only while
        // t < 10, so shot k contributes min(6, 9−k) ticks — 39 total.
        let expect = 0.7 * 39.0 * 0.35 * 75.0;
        assert!(
            (b.mean_dot_damage / expect - 1.0).abs() < 0.10,
            "dot {} vs expect {expect}",
            b.mean_dot_damage
        );
    }

    /// PROC CONVERSION'S THREE UNWRITTEN RULES — the ones the card states in
    /// its Notes and the test above never touched.
    ///
    /// All three are Magnetic Welt's wording (owner, 2026-08-10) and all three
    /// are Hemorrhage's too, since they are one effect:
    ///
    /// 1. **Exactly 2.5 does NOT get the 2x.** "If the weapon's fire rate is
    ///    exactly 2.5, it will not receive the 2x bonus" — a STRICT `<`. This
    ///    is not a corner: `strun`, `strun_wraith` and `strun_prime_incarnon`
    ///    are all listed at exactly 2.50, so the boundary decides the mod's
    ///    value on three roster weapons.
    /// 2. **One roll per damage instance, however many Impact procs land.**
    ///    "Proccing Impact more than once in a single instance of damage will
    ///    not allow this mod to proc more than once, nor will it increase the
    ///    chance of the mod proccing."
    /// 3. **Nothing if the instance already carries the target status.**
    ///    "Cannot produce multiple procs in a single instance of damage
    ///    alongside any other Magnetic sources such as a weapon's innate
    ///    Magnetic."
    #[test]
    fn proc_conversion_obeys_its_three_notes() {
        let welt = |rate: f64, forced: Vec<DamageType>| DummyParams {
            fire_rate: rate,
            proc_conversion: Some(crate::loadout::ProcConv {
                from: DamageType::Impact,
                to: DamageType::Slash,
                chance: 0.35,
                low_rate_threshold: 2.5,
                low_rate_mult: 2.0,
            }),
            forced_procs: forced,
            ..flat_base()
        };
        // Per-shot bleed, so the fire rate does not smuggle itself into the
        // comparison by changing how many shots land.
        let per_shot = |p: &DummyParams| {
            let s = monte_carlo(p, 400, 5);
            s.mean_dot_damage / f64::from(monte_carlo(p, 400, 5).mean_shots.max(1.0) as u32)
        };
        let imp = vec![DamageType::Impact];

        // 1. THE BOUNDARY. 2.49 doubles, 2.50 does not — and the gap is the
        //    factor 2 itself, so nothing subtler could be mistaken for it.
        let under = per_shot(&welt(2.49, imp.clone()));
        let exactly = per_shot(&welt(2.50, imp.clone()));
        assert!(
            (under / exactly - 2.0).abs() < 0.15,
            "2.49 must double and 2.50 must not: {under} vs {exactly}"
        );

        // 2. A SECOND IMPACT PROC IN THE SAME INSTANCE CHANGES NOTHING. Two
        //    forced Impacts are still one membership test and one roll.
        let twice = per_shot(&welt(2.49, vec![DamageType::Impact, DamageType::Impact]));
        assert!(
            (twice / under - 1.0).abs() < 0.12,
            "a second Impact proc must not add a roll: {twice} vs {under}"
        );

        // 3. AN INSTANCE THAT ALREADY CARRIES THE TARGET STATUS gets nothing —
        //    the innate-Slash case, worth exactly the same as no mod at all.
        let already = welt(2.49, vec![DamageType::Impact, DamageType::Slash]);
        let bare = DummyParams { proc_conversion: None, ..already.clone() };
        let a = monte_carlo(&already, 400, 5).mean_dot_damage;
        let b = monte_carlo(&bare, 400, 5).mean_dot_damage;
        assert!(
            (a / b - 1.0).abs() < 1e-9,
            "an innate source must shut the mod out entirely: {a} vs {b}"
        );
    }

    #[test]
    fn weakpoint_damage_adds_into_the_part_multiplier_at_1_5x() {
        // Acuity r10 on a 3x head, 100% weak-point aim: 3 + 3.5 × 1.5 =
        // 8.25x -> 10 × 75 × 8.25 = 6187.5 (wiki Pistol_Acuity example).
        let p = DummyParams {
            weakpoint_damage: 3.5,
            crit_tier_upgrade_chance: 0.0,
            slash_on_crit: 0.0,
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 3.0,
                is_head: true,
                crit_bonus: false,
            }],
            ..flat_base()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 6187.5).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn weakpoint_crit_chance_applies_on_weakpoint_pellets_only() {
        // +100% absolute cc on head hits with cd 2.0: every head pellet
        // tier-1 crits (×2); body pellets never crit. 100% head aim:
        // 10 × 75 × 2 = 1500.
        let head = DummyParams {
            // RELATIVE now: 1.0 x a base of 1.0 = +100% absolute.
            weakpoint_cc_rel: 1.0,
            unmodded_crit_chance: 1.0,
            crit_multiplier: 2.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: true,
                crit_bonus: false,
            }],
            ..no_status()
        };
        let s = monte_carlo(&head, 20, 5);
        assert!(
            (s.mean_damage - 1500.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
        let body = DummyParams {
            body_parts: mono_body(1.0),
            ..head
        };
        let s2 = monte_carlo(&body, 20, 5);
        assert!(
            (s2.mean_damage - 750.0).abs() < 1e-9,
            "dmg {}",
            s2.mean_damage
        );
    }

    #[test]
    fn sharpened_bullets_cd_buff_refreshes_on_kills() {
        // Frail 50 HP respawning targets: kills keep the +100%-absolute cd
        // buff up; guaranteed crits make the buff visible in raw damage.
        let mut frail = TargetParams::training_dummy();
        frail.base_health = 50.0;
        frail.mode = TargetMode::InstantRespawn;
        let base = DummyParams {
            base_crit_chance: 1.0,
            crit_multiplier: 2.0,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            target: frail,
            ..no_status()
        };
        let with = DummyParams {
            cd_on_kill: Some(crate::loadout::TimedBuff {
                value: 1.0,
                duration: 9.0,
                initial_active: false,
            }),
            ..base.clone()
        };
        let a = monte_carlo(&base, 50, 5);
        let b = monte_carlo(&with, 50, 5);
        assert!(
            b.mean_damage > a.mean_damage * 1.3,
            "with {} vs without {}",
            b.mean_damage,
            a.mean_damage
        );
    }

    #[test]
    fn pressurized_magazine_fire_rate_buff_follows_reloads() {
        // A 2-round magazine reloads constantly; +100%-absolute fire rate
        // for 9 s after each reload fits in more shots over 20 s.
        let base = DummyParams {
            magazine_size: 2.0,
            duration_secs: 20.0,
            ..flat_base()
        };
        let with = DummyParams {
            fr_on_reload: Some(crate::loadout::TimedBuff {
                value: 1.0,
                duration: 9.0,
                initial_active: false,
            }),
            ..base.clone()
        };
        let a = monte_carlo(&base, 20, 5);
        let b = monte_carlo(&with, 20, 5);
        assert!(
            b.mean_shots > a.mean_shots,
            "shots {} vs {}",
            b.mean_shots,
            a.mean_shots
        );
    }

    fn shielded_target(shield: f64, health: f64) -> TargetParams {
        TargetParams {
            base_shield: shield,
            base_health: health,
            ..frail_target(TargetMode::InstantRespawn, 0.0, 0.0)
        }
    }

    #[test]
    fn shield_depletion_counts_toward_kill_progress() {
        // Pure Impact never reaches health, but denting the SHIELD now earns
        // partial credit (user 2026-07-25: the whole overguard+shield+health
        // bar counts, so shield damage — and regen — moves the score). One
        // 75-Impact shot into a 1000+1000 = 2000 bar -> 75/2000 = 0.0375,
        // with health still full (0 kills).
        let p = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 75.0),
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            target: shielded_target(1000.0, 1000.0),
            duration_secs: 1.0,
            ..no_status()
        };
        let s = monte_carlo(&p, 10, 7);
        assert_eq!(s.mean_kills, 0.0);
        assert!(
            (s.mean_kill_progress - 0.0375).abs() < 1e-9,
            "score {}",
            s.mean_kill_progress
        );
    }

    #[test]
    fn toxin_share_bypasses_shields_into_health() {
        // Vector 16 Toxin / 16 Impact (quantization-exact): each 32-damage
        // shot sends 16 to the shield and 16 straight to health. Health
        // 160 dies exactly on the 10th shot while 840 shield remains.
        let p = DummyParams {
            damage: DamageVector::new()
                .with(DamageType::Toxin, 16.0)
                .with(DamageType::Impact, 16.0),
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            target: shielded_target(1000.0, 160.0),
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!((s.mean_kills - 1.0).abs() < 1e-9, "kills {}", s.mean_kills);
        // Control: an all-Impact vector never touches health.
        let q = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 32.0),
            ..p
        };
        let s2 = monte_carlo(&q, 20, 5);
        assert_eq!(s2.mean_kills, 0.0);
    }

    #[test]
    fn shield_gate_multiplies_damage_for_a_tenth_second() {
        // Shield 100, 75-damage shots at 20/s (0.05 s cadence): shots at
        // 0 and 0.05 break the shield (gate until 0.15); the 0.10 shot is
        // gated ×0.05 (3.75); the 0.15 shot is full again.
        // Effective = 75 + 75 + 3.75 + 75 = 228.75.
        let p = DummyParams {
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            fire_rate: 20.0,
            duration_secs: 0.2,
            magazine_size: 100.0,
            body_parts: mono_body(1.0),
            target: shielded_target(100.0, 1e9),
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_effective_damage - 228.75).abs() < 1e-9,
            "eff {}",
            s.mean_effective_damage
        );
        // Weakpoint hits bypass the gate: all-head aim -> 4 × 75 = 300.
        let head = DummyParams {
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: true,
                crit_bonus: false,
            }],
            ..p
        };
        let s2 = monte_carlo(&head, 20, 5);
        assert!(
            (s2.mean_effective_damage - 300.0).abs() < 1e-9,
            "eff {}",
            s2.mean_effective_damage
        );
    }

    #[test]
    fn attenuation_caps_damage_per_instance_and_per_second() {
        // Instance cap 5% × 1000 HP = 50: each 75 shot clamps to 50.
        let mut t = shielded_target(0.0, 1000.0);
        t.base_health = 1000.0;
        t.mode = TargetMode::InfiniteHealth;
        t.attenuation = Some(Attenuation {
            instance_frac: 0.05,
            dps_frac: 0.50,
        });
        let p = DummyParams {
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            target: t,
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_effective_damage - 500.0).abs() < 1e-9,
            "eff {}",
            s.mean_effective_damage
        );
        // DPS cap 50%/s = 500: at 20 shots/s only the first 10 clamped
        // shots fit the 1 s bucket -> 500 total in the 1 s run.
        let q = DummyParams {
            fire_rate: 20.0,
            duration_secs: 1.0,
            magazine_size: 100.0,
            ..p
        };
        let s2 = monte_carlo(&q, 20, 5);
        assert!(
            (s2.mean_effective_damage - 500.0).abs() < 1e-9,
            "eff {}",
            s2.mean_effective_damage
        );
    }

    #[test]
    fn crosshairs_buff_is_refreshed_by_headshot_hits() {
        // Head aim: every hit refreshes the +0.12 buff, so all 18 shots
        // in 20 s (12-mag + reload) fire at cc 0.22 with cd 2:
        // E = 18 × 75 × 1.22 = 1647.
        let p = DummyParams {
            base_crit_chance: 0.1,
            // The buff value is RELATIVE now (it joins the crit bucket, so it
            // scales each part's own base). A base of 1.0 makes 0.12 land as
            // +12% absolute, leaving the arithmetic above unchanged.
            unmodded_crit_chance: 1.0,
            cc_on_headshot: Some(crate::loadout::TimedBuff {
                value: 0.12,
                duration: 12.0,
                initial_active: true,
            }),
            arcane: ArcaneFx::none(),
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: true,
                crit_bonus: false,
            }],
            duration_secs: 20.0,
            ..no_status()
        };
        let s = monte_carlo(&p, 4000, 23);
        assert!(
            (s.mean_damage - 1647.0).abs() / 1647.0 < 0.02,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn crosshairs_cc_buffs_start_full_and_expire_without_headshots() {
        // Body-only aim: nothing refreshes the initial-full buffs, so both
        // lapse at 12 s. cc: 0.1 + 0.12 + 5×0.04 = 0.42 before, 0.1 after.
        // 20 s at 1/s with the 12-mag: shots 0..11 (all < 12 s), reload
        // 2.35, shots 14.35..19.35 (6 bare). cd 2 -> E = 75 × (1 + cc):
        // E[total] = 75 × (12×1.42 + 6×1.1) = 1773.
        let p = DummyParams {
            base_crit_chance: 0.1,
            unmodded_crit_chance: 1.0, // relative buff values — see above
            cc_on_headshot: Some(crate::loadout::TimedBuff {
                value: 0.12,
                duration: 12.0,
                initial_active: true,
            }),
            cc_stack: Some(crate::loadout::StackSpec {
                per_stack: 0.04,
                max_stacks: 5,
                duration: 12.0,
                initial_stacks: 5,
            }),
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            duration_secs: 20.0,
            ..no_status()
        };
        let s = monte_carlo(&p, 4000, 21);
        assert!(
            (s.mean_damage - 1773.0).abs() / 1773.0 < 0.02,
            "dmg {}",
            s.mean_damage
        );
    }

    #[test]
    fn kill_progress_gives_partial_credit_for_depleted_pools() {
        // 1000 HP target, one 75-damage shot in the window: 0 kills but
        // 7.5% of the pool depleted -> score 0.075.
        let mut t = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
        t.base_health = 1000.0;
        let p = DummyParams {
            crit_multiplier: 1.0,
            body_parts: mono_body(1.0),
            target: t,
            duration_secs: 1.0,
            ..no_status()
        };
        let s = monte_carlo(&p, 10, 3);
        assert_eq!(s.mean_kills, 0.0);
        assert!(
            (s.mean_kill_progress - 0.075).abs() < 1e-9,
            "score {}",
            s.mean_kill_progress
        );
    }

    #[test]
    fn frozen_state_machine_follows_the_recorded_rules() {
        let mut d = DebuffState::default();
        // Nine Freeze stacks build normally (no overguard).
        for k in 0..9 {
            d.apply_cold_proc(k as f64 * 0.1, 1.0, false, None, false);
        }
        assert_eq!(d.freeze.len(), 9);
        assert!((d.cold_cd_bonus(0.9) - 0.50).abs() < 1e-9); // 0.10+0.05×8
                                                             // The 10th proc CONSUMES the stacks and enters Frozen (3 s).
        d.apply_cold_proc(1.0, 1.0, false, None, false);
        assert!(d.freeze.is_empty());
        assert_eq!(d.frozen_until, Some(4.0));
        assert!((d.cold_cd_bonus(1.5) - 1.00).abs() < 1e-9); // supersedes
                                                             // Cold procs are inert while Frozen.
        d.apply_cold_proc(2.0, 1.0, false, None, false);
        assert!(d.freeze.is_empty());
        // Thaw: hard reset to exactly 3 stacks with FRESH 6 s timers
        // anchored at the thaw instant (expire at 4 + 6 = 10 s).
        d.prune(4.5, 1.0);
        assert_eq!(d.frozen_until, None);
        assert_eq!(d.freeze.len(), 3);
        d.prune(9.9, 1.0);
        assert_eq!(d.freeze.len(), 3);
        d.prune(10.1, 1.0);
        assert!(d.freeze.is_empty());
    }

    #[test]
    fn overguard_caps_freeze_at_four_and_never_freezes() {
        let mut d = DebuffState::default();
        for k in 0..20 {
            d.apply_cold_proc(k as f64 * 0.1, 1.0, true, None, false);
        }
        assert_eq!(d.freeze.len(), 4);
        assert_eq!(d.frozen_until, None);
        assert!((d.cold_cd_bonus(2.0) - 0.25).abs() < 1e-9); // 0.10+0.05×3
    }

    #[test]
    fn heat_strip_ramps_up_and_decays_after_the_entity_dies() {
        let mut d = DebuffState {
            heat: Some(HeatEntity {
                born: 0.0,
                expiry: 6.0,
                next_tick: 1.0,
                value: 1.0,
                recent: Vec::new(),
            }),
            ..Default::default()
        };
        // Ramp-up: 0.5 s steps 15/30/40/50%.
        assert_eq!(d.heat_strip(0.4, 1.0), 0.0);
        assert_eq!(d.heat_strip(0.6, 1.0), 0.15);
        assert_eq!(d.heat_strip(1.6, 1.0), 0.40);
        assert_eq!(d.heat_strip(2.5, 1.0), 0.50);
        // Entity dies at 6.0 -> ramp-down every 1.5 s: 50/40/30/15/0.
        d.prune(6.5, 1.0);
        assert!(d.heat.is_none());
        assert_eq!(d.heat_strip(6.5, 1.0), 0.50);
        assert_eq!(d.heat_strip(7.6, 1.0), 0.40);
        assert_eq!(d.heat_strip(9.1, 1.0), 0.30);
        assert_eq!(d.heat_strip(10.6, 1.0), 0.15);
        assert_eq!(d.heat_strip(12.1, 1.0), 0.0);
    }

    #[test]
    fn longer_status_duration_slows_the_heat_strip_ramp() {
        // ignite.yaml: the ramp steps scale WITH status duration —
        // +100% duration means 1.0 s steps, full strip only at 4 s
        // (counter-intuitive: longer duration = SLOWER strip).
        let d = DebuffState {
            heat: Some(HeatEntity {
                born: 0.0,
                expiry: 12.0,
                next_tick: 1.0,
                value: 1.0,
                recent: Vec::new(),
            }),
            ..Default::default()
        };
        // sd = 2.0: steps at 1.0 s intervals.
        assert_eq!(d.heat_strip(0.9, 2.0), 0.0);
        assert_eq!(d.heat_strip(1.1, 2.0), 0.15);
        assert_eq!(d.heat_strip(3.9, 2.0), 0.40);
        assert_eq!(d.heat_strip(4.1, 2.0), 0.50);
        // sd = 0.5: full strip already at 1.0 s.
        assert_eq!(d.heat_strip(1.1, 0.5), 0.50);
    }

    #[test]
    fn heat_cap_keeps_the_most_recent_contributions_fifo() {
        // Uncapped: every contribution folds into the consolidated tick.
        let mut d = DebuffState::default();
        for c in [1.0, 2.0, 3.0, 4.0, 5.0] {
            d.apply_heat(0.0, c, 6.0, None);
        }
        assert_eq!(d.heat.as_ref().unwrap().value, 15.0);

        // Capped at 3 (a per-unit cap): hold the 3 MOST RECENT contributions,
        // FIFO dropping the oldest — {3,4,5}=12, NOT the first three (the old
        // ignore-new model would have frozen it at 1+2+3=6).
        let mut d = DebuffState::default();
        for c in [1.0, 2.0, 3.0, 4.0, 5.0] {
            d.apply_heat(0.0, c, 6.0, Some(3));
        }
        let h = d.heat.as_ref().unwrap();
        assert_eq!(h.recent, vec![3.0, 4.0, 5.0]);
        assert_eq!(h.value, 12.0);
    }

    #[test]
    fn independent_dots_cap_per_type_fifo() {
        let dot = |v: f64, ty| Dot {
            next_tick: 0.0,
            ticks_left: 6,
            value: v,
            dtype: ty,
            ignores_armor: false,
        };
        let mut d = DebuffState::default();
        // Cap 2 per type: four Toxin procs keep only the two newest (3,4).
        for v in [1.0, 2.0, 3.0, 4.0] {
            d.push_dot_capped(dot(v, DamageType::Toxin), Some(2));
        }
        // A different DoT type is capped independently.
        d.push_dot_capped(dot(9.0, DamageType::Slash), Some(2));
        let tox: Vec<f64> = d
            .dots
            .iter()
            .filter(|x| x.dtype == DamageType::Toxin)
            .map(|x| x.value)
            .collect();
        let sla: Vec<f64> = d
            .dots
            .iter()
            .filter(|x| x.dtype == DamageType::Slash)
            .map(|x| x.value)
            .collect();
        assert_eq!(tox, vec![3.0, 4.0]);
        assert_eq!(sla, vec![9.0]);
    }

    #[test]
    fn corrosion_strips_armor_multiplicatively() {
        // Capped armor (90% DR): forced Corrosive procs stack 20%+6%/stack
        // strip, so later shots take less DR than the first. Exact: shot k
        // has n = k−1 stacks; strip(0)=0, strip(n)=0.20+0.06n.
        let p = DummyParams {
            target: frail_target(TargetMode::InfiniteHealth, 2700.0, 0.0),
            ..bare(DamageType::Corrosive)
        };
        let s = monte_carlo(&p, 20, 5);
        // Stack counts before each shot (8 s expiry, 1 shot/s):
        // 0,1,2,...,7 then a steady 7.
        let expected: f64 = [0, 1, 2, 3, 4, 5, 6, 7, 7, 7]
            .iter()
            .map(|&n| {
                let strip = if n == 0 { 0.0 } else { 0.20 + 0.06 * n as f64 };
                let dr = 0.9 * ((2700.0 * (1.0 - strip)) / 2700.0_f64).sqrt();
                (75.0 * (1.0 - dr)).max(1.0)
            })
            .sum();
        assert!(
            (s.mean_effective_damage - expected).abs() < 1e-9,
            "eff {} vs {expected}",
            s.mean_effective_damage
        );
    }

    #[test]
    fn enervate_raises_crit_rate_above_base() {
        // Base crit is 5%, but Enervate stacks flat crit as the fight goes on, so
        // the observed crit rate should exceed 5%.
        let s = monte_carlo(&DummyParams::default(), 2000, 3);
        assert!(
            s.mean_crit_rate > 0.05,
            "crit rate was {}",
            s.mean_crit_rate
        );
    }

    /// A magazine reloads when the NEXT SHOT cannot be paid for, and the test
    /// is `cost <= ceil(current)` — not `current > 0`.
    ///
    /// Both cases are the owner's (2026-08-01), and the rule hid for as long
    /// as it did because it only bites above 1: for any cost <= 1 the two
    /// tests agree on every positive magazine, since `ceil(x) >= 1` there.
    /// Ammo efficiency put costs in the 0.x range and exposed nothing. The
    /// Larkspur Prime's alt-fire costs TEN, and there they part company.
    #[test]
    fn a_reload_is_decided_by_what_the_next_shot_costs() {
        // 7 left, the shot costs 10: it does NOT fire. Under `current > 0` it
        // did, and landed on −3 — a debt one whole-round draw cannot clear.
        assert!(!can_fire(7.0, 10.0), "7 cannot pay for 10");
        assert!(can_fire(10.0, 10.0), "10 pays for 10 exactly");
        // 0.2 left, the shot costs 1: it DOES fire, because ceil(0.2) is 1.
        assert!(can_fire(0.2, 1.0), "the ceiling is what lets this through");
        assert!(!can_fire(0.0, 1.0), "empty is empty");
        // A beam tick at 0.5 clears the same ceiling with room to spare.
        assert!(can_fire(0.2, 0.5), "0.5 <= ceil(0.2)");
        // A FREE shot is not a special case, it is `cost == 0`: the ceiling
        // lets it through on an empty magazine, which is what the `free_shot`
        // flag used to be for.
        assert!(can_fire(0.0, 0.0), "0 <= ceil(0)");
        assert!(can_fire(-0.8, 0.0), "and on an overdrawn one");

        // The overdraw the second case creates is bounded to (−1, 0], which is
        // what keeps `reload_draw`'s whole-round rule (M14) correct: a
        // magazine sitting at −0.8 comes back at capacity − 0.8, not capacity.
        let after = -0.8 + reload_draw(100.0, -0.8);
        assert!((after - 99.2).abs() < 1e-9, "capacity - 0.8: {after}");
    }

    /// The DEFAULT is what this is about. `finite_reserve_stops_the_gun`
    /// already proves a finite pool ends the run; what matters for the data
    /// path is that switching it on is the ONLY thing that does, and that the
    /// size of the pool is then worth modding for.
    ///
    /// Ammo PICKUPS are not modelled, so a weapon that can be resupplied
    /// mid-fight must keep the infinite default or it would run dry for a
    /// reason the game does not have. A ground Arch-Gun is the case that
    /// cannot: "Archguns only have a limited amount of ammo", and when it is
    /// gone the weapon is removed for a five-minute cooldown (wiki Arch-Gun).
    #[test]
    fn only_a_finite_reserve_ends_a_run_early_and_its_size_then_matters() {
        let params = |finite: bool, reserve: f64| DummyParams {
            magazine_size: 10.0,
            fire_rate: 20.0,
            reload_seconds: 0.1,
            duration_secs: 30.0,
            infinite_reserve: !finite,
            reserve_ammo: reserve,
            arcane: crate::arcanes_data::ArcaneFx::none(),
            ..Default::default()
        };
        let shots = |p: &DummyParams| monte_carlo(p, 1, 5).mean_shots;

        // The same weapon, the same reserve, one flag apart: 600 rounds of
        // clock against 35 rounds of ammunition.
        let finite = shots(&params(true, 25.0));
        let infinite = shots(&params(false, 25.0));
        assert!(finite < 40.0 && infinite > 300.0, "{finite} vs {infinite}");

        // A bigger pool lasts longer, which is what makes Ammo Chain and a
        // riven's Ammo Maximum worth a slot on such a weapon at all.
        assert!(shots(&params(true, 90.0)) > finite);

        // And the reserve is not the magazine: with none, the magazine is all
        // there is.
        let one = shots(&params(true, 0.0));
        assert!((one - 10.0).abs() < 1e-9, "just the magazine: {one}");
    }

    /// Past 100% crit chance the RATE stops saying anything — every pellet
    /// crits, so it reads 1.0 whether the build is at 110% or 410%. The mean
    /// TIER is the number that keeps going, and it is the one that
    /// multiplies the damage (user, via the QQ group, 2026-07-31).
    ///
    /// Red is NOT the ceiling: tier 4 and above are real, the game shows
    /// them, and `crit_mult = 1 + tier x (cd - 1)` has no cap either — so
    /// neither may the report.
    #[test]
    fn the_crit_tier_keeps_climbing_where_the_rate_saturates() {
        let at = |cc: f64| {
            let p = DummyParams {
                base_crit_chance: cc,
                // Measure the ROLL, not a promotion or a stacking arcane.
                crit_tier_upgrade_chance: 0.0,
                arcane: crate::arcanes_data::ArcaneFx::none(),
                ..Default::default()
            };
            monte_carlo(&p, 400, 11)
        };
        // Below 100% the two are the SAME number — the tier is not a second
        // opinion, it is the rate without the >= 1 truncation.
        let half = at(0.5);
        assert!(
            (half.mean_crit_tier - half.mean_crit_rate).abs() < 1e-9,
            "below 100% they must agree: tier {} vs rate {}",
            half.mean_crit_tier,
            half.mean_crit_rate
        );

        // At 100% and beyond the rate is pinned at 1.0 and only the tier moves.
        let one = at(1.0);
        let past_red = at(4.2);
        assert!((one.mean_crit_rate - 1.0).abs() < 1e-9);
        assert!((past_red.mean_crit_rate - 1.0).abs() < 1e-9, "the rate saturates");
        assert!(
            past_red.mean_crit_tier > 4.0,
            "above RED and still counting: {}",
            past_red.mean_crit_tier
        );
        assert!(past_red.mean_crit_tier > one.mean_crit_tier + 3.0);
    }

    /// EVERY on-status trigger the data can express must actually be fired by
    /// the sim. Toxin, Electricity and Heat were wired and COLD was not, so
    /// Primary Frostbite spent one duration at its seeded stack count and then
    /// sat at zero for the rest of every run — it looked implemented, listed
    /// in the picker, and quietly did nothing after twelve seconds.
    ///
    /// The check is mechanical rather than by name: it asserts each variant
    /// appears in a `bump_trigger` call in this file's source. A new
    /// on-status arcane cannot be added without wiring it.
    #[test]
    fn every_on_status_trigger_is_fired_somewhere() {
        let src = include_str!("dummy.rs");
        for name in ["ToxinStatus", "ElectricityStatus", "HeatStatus", "ColdStatus"] {
            let needle = format!("ArcTrigger::{name}");
            let fired = src
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .any(|l| l.contains("bump_trigger") && l.contains(&needle));
            assert!(fired, "ArcTrigger::{name} is never bumped — an arcane that \
                waits on it can never earn a stack");
        }
    }

    /// PUNCTURE'S WEAKENED DOES NOT REACH AN EXPLOSION.
    ///
    /// Wiki (Damage/Puncture_Damage): "Weapon damage that the victim receives
    /// has 5% increased Critical Chance per proc up to 25% at max stacks …
    /// This is a flat critical chance buff (like Arcane Avenger), but does not
    /// apply to Area of Effect damage or Warframe abilities."
    ///
    /// The radial had its own copy of the crit line and that copy added the
    /// buff, so a build that stacked Puncture crit its explosion up to 25% of
    /// the time for free. The fixture makes that visible rather than statistical:
    /// the explosion has ZERO crit chance of its own, so any crit at all can
    /// only have come from Weakened.
    #[test]
    fn weakened_never_crits_an_explosion() {
        let mut p = radial_only(radial_of(0.0, 0.0));
        // The direct hit forces Puncture every shot, so Weakened saturates.
        p.forced_procs = vec![DamageType::Puncture];
        p.status_chance = 1.0;
        p.base_status_chance = 1.0;
        let mut damage = DamageVector::default();
        damage.set(DamageType::Puncture, 100.0);
        p.damage = damage;
        p.magazine_size = 200.0;
        p.reload_seconds = 0.1;
        let s = monte_carlo(&p, 8, 11);
        // The DIRECT hit is allowed to crit off Weakened — it is weapon damage
        // the victim receives, which is exactly what the buff is for. Only the
        // explosion is excluded, so the explosion's own bucket is the
        // assertion: with zero crit chance it must be shots x its flat base,
        // and any crit at all would show as more than that.
        let expected = s.mean_shots * 300.0;
        assert!(
            (s.source_damage.radial - expected).abs() < 1e-6,
            "explosion dealt {} where a never-critting one deals {} — Weakened              reached the AoE",
            s.source_damage.radial,
            expected
        );
        assert!(s.median_run.crits > 0, "the DIRECT hit should still crit off Weakened");
    }

    /// LOCKED MEANS "NO TIMEOUT", NOT "FROZEN" (user, 2026-08-02).
    ///
    /// The old reading froze a locked buff at whatever count it was configured
    /// with, so locking one at a partial count also stopped it EARNING — and
    /// locking one at zero meant it could never turn on at all, which is
    /// exactly what a visitor assumed the control did. It now only removes the
    /// expiry: the count starts where it was set and still climbs.
    ///
    /// A killable target and an on-kill stacking arcane make it arithmetic:
    /// seeded at ONE of three stacks and locked, the buff must end the run
    /// worth more than one stack.
    #[test]
    fn a_locked_buff_still_earns_stacks() {
        use crate::arcanes_data::{ArcBuffSpec, ArcGrant, ArcTrigger};
        let mk = |initial: u32| {
            let mut damage = DamageVector::default();
            damage.set(DamageType::Impact, 100.0);
            DummyParams {
                damage,
                // Dies to every shot and comes straight back, so the on-kill
                // trigger fires on a schedule instead of by luck.
                target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
                arcane: ArcaneFx {
                    buffs: vec![ArcBuffSpec {
                        owner: "test".into(),
                        grant: ArcGrant::BaseDamage,
                        trigger: ArcTrigger::Kill,
                        per_stack: 1.0,          // +100% of base per stack
                        max_stacks: 3,
                        // LOCKED — which, since 2026-08-04, IS a duration.
                        duration: crate::loadout::NO_TIMEOUT,
                        all_drop: false,
                        one_per_instance: false,
                        initial_stacks: initial,
                    }],
                    ..ArcaneFx::none()
                },
                magazine_size: 60.0,
                reload_seconds: 0.1,
                base_crit_chance: 0.0,
                crit_multiplier: 1.0,
                ..no_status()
            }
        };
        let one = monte_carlo(&mk(1), 6, 5);
        let full = monte_carlo(&mk(3), 6, 5);
        // Frozen at one stack it would never approach the seeded-full run.
        assert!(
            one.mean_damage > 0.9 * full.mean_damage,
            "locked at 1 stack stayed at 1: {} vs {}",
            one.mean_damage,
            full.mean_damage
        );
    }

    /// ONE ARCANE IS ONE CARD, so one setting must reach every spec it owns.
    ///
    /// Frostbite grants crit damage AND multishot off the same Cold proc and
    /// they are the same count by construction — there is no state of the game
    /// where one is at 1 and the other at 10. The card was per GRANT, which
    /// offered a setting that cannot exist and keyed each half separately;
    /// configuring the arcane now has to move both halves or the merge is a
    /// display trick over a split model.
    #[test]
    fn one_config_reaches_every_grant_of_its_arcane() {
        use crate::arcanes_data::{ArcBuffSpec, ArcGrant, ArcTrigger};
        let spec = |grant: ArcGrant| ArcBuffSpec {
            owner: "primary_frostbite".into(),
            grant,
            trigger: ArcTrigger::ColdStatus,
            per_stack: 0.03,
            max_stacks: 40,
            duration: 12.0,
            all_drop: true,
            one_per_instance: false,
            initial_stacks: 40,
        };
        let mut p = DummyParams {
            arcane: ArcaneFx {
                buffs: vec![spec(ArcGrant::CritDamage), spec(ArcGrant::Multishot)],
                ..ArcaneFx::none()
            },
            ..DummyParams::default()
        };
        let mut cfg = BuffConfig::new();
        cfg.insert("arcane:primary_frostbite".into(), (7, true));
        p.apply_buff_config(&cfg);
        for b in &p.arcane.buffs {
            assert_eq!(b.initial_stacks, 7, "{:?} kept its own count", b.grant);
            assert_eq!(
                b.duration,
                crate::loadout::NO_TIMEOUT,
                "{:?} took the config's lock",
                b.grant
            );
        }
    }

    /// A COLD PROC ON A FROZEN TARGET IS INERT, so it stacks nothing.
    ///
    /// `data/debuffs/frozen.yaml` is explicit — `refreshable: false`, "cannot
    /// be extended: Cold procs are inert" — and `apply_cold_proc` has always
    /// honoured it for the debuff. Primary Frostbite did not: the trigger was
    /// bumped BEFORE the proc and unconditionally, so an arcane whose card
    /// reads "On Cold Status Effect" earned a stack from a proc that applied
    /// no status (user, 2026-08-02).
    ///
    /// It hid well. Frozen lasts 3 s against the arcane's 12 s all-drop timer,
    /// so it never looked like the buff going dark — it looked like the buff
    /// never quite decaying, in exactly the fight (heavy Cold, one target)
    /// where you would credit the arcane for it.
    #[test]
    fn a_cold_proc_on_a_frozen_target_stacks_nothing() {
        // Ten procs, one per shot, on a target with no overguard and no
        // per-unit caps: the 10th freezes it, and everything after is inert
        // until Frozen expires.
        let mut d = DebuffState::default();
        let mut applied = 0;
        for i in 0..10 {
            if d.apply_cold_proc(i as f64 * 0.1, 1.0, false, None, false) {
                applied += 1;
            }
        }
        assert_eq!(applied, 10, "nine stacks, then the tenth converts to Frozen");
        assert!(d.frozen_until.is_some_and(|f| f > 1.0), "it is Frozen");
        // While Frozen: every further proc reports FALSE. That is the whole
        // fix — the caller stacks the arcane off this answer, not off the
        // attempt.
        for i in 0..5 {
            assert!(
                !d.apply_cold_proc(1.0 + i as f64 * 0.1, 1.0, false, None, false),
                "a proc during Frozen applied a status"
            );
        }
        // ...and once it thaws, procs land again.
        assert!(d.apply_cold_proc(1.0 + FROZEN_DURATION + 0.01, 1.0, false, None, false));

        // A CAPPED list is not the same case: pushing past a cap replaces the
        // oldest, which IS an application, so the arcane keeps stacking there.
        let mut og = DebuffState::default();
        for i in 0..8 {
            assert!(
                og.apply_cold_proc(i as f64 * 0.1, 1.0, true, None, false),
                "under overguard the list caps at 4 but every proc still lands"
            );
        }
        assert_eq!(og.freeze.len(), FREEZE_CAP_UNDER_OVERGUARD);
        assert!(og.frozen_until.is_none(), "an overguard holder never freezes");
    }

    /// A REPLAY IS THE SAME FIGHT, not a re-roll of it.
    ///
    /// The whole design rests on `Rng` being SplitMix64 with one `u64` of
    /// state: a run records what it started from, and replaying from that
    /// number reproduces it exactly. If that ever stops being true the replay
    /// silently starts showing a DIFFERENT engagement than the one whose
    /// number is on screen — which is worse than having no replay.
    #[test]
    fn a_replay_reproduces_the_run_it_came_from() {
        let p = DummyParams {
            arcane: arc_stacked("secondary_merciless"),
            duration_secs: 30.0,
            ..flat_base()
        };
        let s = monte_carlo(&p, 12, 99);
        let rep = replay(&p, s.median_run.rng_state, 60);
        assert_eq!(rep.frames.len(), 60, "one frame per slot, gaps filled");
        assert!((rep.dt - 0.5).abs() < 1e-9, "30 s over 60 frames");

        // Re-running from the same state gives the identical RunResult.
        let again = run_once(&p, &mut Rng::new(s.median_run.rng_state));
        assert_eq!(again.total_damage.to_bits(), s.median_run.total_damage.to_bits());
        assert_eq!(again.pellets, s.median_run.pellets);
        assert_eq!(again.crits, s.median_run.crits);

        // The last frame's cumulative damage is the run's own effective total
        // minus whatever landed after the final sample — never more.
        let last = rep.frames.last().unwrap();
        assert!(last.damage <= s.median_run.effective_damage + 1e-6);
        assert!(last.damage > 0.0, "something happened");
        // Frames advance in time and never go backwards.
        for w in rep.frames.windows(2) {
            assert!(w[1].t > w[0].t);
            assert!(w[1].damage >= w[0].damage, "cumulative damage cannot fall");
        }
    }

    /// The roster names every buff the sampler can answer for, and the
    /// sampler answers for every buff the roster names. A rostered buff the
    /// sampler does not know would draw a flat zero and read as a finding.
    #[test]
    fn every_rostered_buff_is_sampled() {
        let p = DummyParams {
            arcane: arc_stacked("secondary_merciless"),
            duration_secs: 10.0,
            ..DummyParams::default()
        };
        let roster = p.buff_roster();
        assert!(!roster.is_empty(), "this fixture carries buffs");
        let rep = replay(&p, 12345, 20);
        assert_eq!(rep.buffs, roster);
        for f in &rep.frames {
            assert_eq!(f.stacks.len(), roster.len(), "one sample per rostered buff");
        }
        // Merciless was seeded full, so its series starts at its cap rather
        // than at the zero an unknown id would produce.
        let at = roster.iter().position(|(id, _)| id.starts_with("arcane:")).expect("an arcane");
        assert_eq!(u32::from(rep.frames[0].stacks[at]), roster[at].1);
    }

    /// THE FACTION LADDER, which is the whole reason Primary Debilitate is
    /// worth more than its card reads. Each derivation step re-applies the
    /// bonus, and 3 is never written down — it falls out of the depth.
    #[test]
    fn faction_compounds_once_per_derivation_step() {
        let f = 1.55; // +55% faction, the wiki's worked example
        assert!((faction_at(f, DEPTH_HIT) - 1.55).abs() < 1e-9);
        assert!((faction_at(f, DEPTH_PROC) - 2.4025).abs() < 1e-9);
        assert!((faction_at(f, DEPTH_DERIVED_PROC) - 3.723875).abs() < 1e-6);

        // The wiki's own numbers for a 100-base melee with +90% Electricity:
        // hit 294, its Electricity proc 228, a SPREAD Electricity proc 353.
        let hit = (100.0 + 90.0) * faction_at(f, DEPTH_HIT);
        let proc = 0.5 * (100.0 + 90.0) * faction_at(f, DEPTH_PROC);
        let spread = 0.5 * (100.0 + 90.0) * faction_at(f, DEPTH_DERIVED_PROC);
        assert!((hit - 294.5).abs() < 0.5, "{hit}");
        assert!((proc - 228.2).abs() < 0.5, "{proc}");
        assert!((spread - 353.8).abs() < 0.5, "{spread}");
    }

    /// PRIMARY DEBILITATE'S DECISION, pinned without a fight.
    ///
    /// The argument is the count the target is AT, this application included —
    /// so `DEBILITATE_STACKS` here is the ninth stack plus the tenth being
    /// applied, and that is the shot that splits (owner, 2026-08-10).
    #[test]
    fn debilitate_splits_only_a_saturated_combination() {
        let mut rng = Rng::new(0xD0D0);
        // Below the threshold: never, whatever the roll would have been.
        for stacks in 0..DEBILITATE_STACKS {
            assert_eq!(
                debilitate_split(DamageType::Corrosive, stacks, 1.0, &mut rng),
                None,
                "{stacks} stacks is under the bar"
            );
        }
        // At it, with certainty, it always splits — and only into a COMPONENT.
        // ALL SIX combinations, not just the one the reports come in about:
        // Blast is Cold and Heat (owner, 2026-08-08: "blast是可触发冰和火的"),
        // and a table that answered for five of six would be wrong in exactly
        // the way nobody checks.
        for combined in [
            DamageType::Corrosive,
            DamageType::Blast,
            DamageType::Viral,
            DamageType::Magnetic,
            DamageType::Radiation,
            DamageType::Gas,
        ] {
            let (a, b) = crate::elements::components_of(combined).expect("a combination");
            for _ in 0..64 {
                let got = debilitate_split(combined, DEBILITATE_STACKS, 1.0, &mut rng)
                    .expect("certain at rank 5");
                assert!(
                    got == a || got == b,
                    "{combined:?} splits into {a:?}/{b:?}, got {got:?}"
                );
            }
        }
        // A PRIMARY has nothing to split into, saturated or not.
        assert_eq!(debilitate_split(DamageType::Heat, 99, 1.0, &mut rng), None);
        assert_eq!(debilitate_split(DamageType::Slash, 99, 1.0, &mut rng), None);
        // No arcane, no split.
        assert_eq!(debilitate_split(DamageType::Viral, 99, 0.0, &mut rng), None);
    }

    /// THE TENTH APPLICATION SPLITS, and the ninth does not — end to end, on an
    /// ordinary combination where the count is observable.
    ///
    /// This is the rule the Blast case turned out to be an instance of (owner,
    /// 2026-08-10). Asserted by CAPPING the fight at nine applications and at
    /// ten: nine must pay nothing at all, ten must pay. A test that only
    /// asserted "ten splits" would pass just as well under the old
    /// already-holds-ten reading, which fires on the eleventh.
    #[test]
    fn the_tenth_application_is_the_one_that_splits() {
        // Pure Viral, forced, one proc per shot, and exactly `shots` of them:
        // the stack count after shot n is n, capped at ten.
        //
        // THE AMMO is what bounds the shot count, not the clock. Bounding it by
        // duration was the first attempt and it silently asserted nothing: the
        // tenth shot lands at the last instant of the fight and its DoT ticks
        // start a second AFTER the end, so both arms read zero and "nine splits
        // nothing" passed for the wrong reason.
        let run = |shots: f64, chance: f64| {
            let p = DummyParams {
                damage: DamageVector::new().with(DamageType::Viral, 100.0),
                dot_modified_base: Some(100.0),
                status_chance: 0.0,
                base_status_chance: 0.0,
                forced_procs: vec![DamageType::Viral],
                fire_rate: 10.0,
                // Long enough for the last shot's DoT to tick out in full.
                duration_secs: 20.0,
                magazine_size: shots,
                infinite_reserve: false,
                reserve_ammo: 0.0,
                base_crit_chance: 0.0,
                unmodded_crit_chance: 0.0,
                target: TargetParams { base_health: 1e15, ..DummyParams::default().target },
                // Viral splits into Cold and Toxin; only Toxin ticks, so a
                // split that lands shows up as DoT damage and nothing else can
                // put damage in that bucket.
                elem_dot_bonus: vec![(DamageType::Toxin, 1.0)],
                arcane: crate::arcanes_data::ArcaneFx {
                    debilitate_chance: chance,
                    ..crate::arcanes_data::ArcaneFx::none()
                },
                ..DummyParams::default()
            };
            monte_carlo(&p, 200, 0x51CE).mean_dot_damage
        };
        // NINE applications: the target never reaches ten, so nothing splits —
        // and Viral's own proc is a multiplier, not a DoT, so the bucket is
        // empty for a reason that cannot be anything else.
        assert_eq!(run(9.0, 1.0), 0.0, "nine applications must split nothing");
        // TEN: the tenth is the one, and at rank 5 it is certain.
        assert!(run(10.0, 1.0) > 0.0, "the tenth application must split");
        // …and it is the ARCANE doing it: same fight, no arcane, no DoT.
        assert_eq!(run(10.0, 0.0), 0.0);
    }

    /// BLAST REACHES THE THRESHOLD, END TO END — the table above says it may
    /// split, and this says the sim ever gets it there.
    ///
    /// It is the case where the threshold rule is the whole mechanic rather
    /// than one shot: Blast DETONATES at ten and drains every stack, so the
    /// count a later application reads is 0..=9 forever. Reading the
    /// pre-application count made the arcane dead on Blast, silently, with a
    /// passing unit test on the split function itself. Only a run finds that.
    #[test]
    fn a_blast_build_actually_reaches_the_debilitate_threshold() {
        let build = |chance: f64| DummyParams {
            damage: DamageVector::new().with(DamageType::Blast, 100.0),
            status_chance: 1.0,
            base_status_chance: 1.0,
            fire_rate: 10.0,
            magazine_size: 1e9,
            infinite_reserve: true,
            // Heat is the half of Blast that ticks, so a split that lands is
            // visible as damage; Cold's half is a slow and adds none.
            elem_dot_bonus: vec![(DamageType::Heat, 2.0), (DamageType::Cold, 2.0)],
            arcane: crate::arcanes_data::ArcaneFx {
                debilitate_chance: chance,
                ..crate::arcanes_data::ArcaneFx::none()
            },
            ..DummyParams::default()
        };
        const RUNS: u32 = 200;
        const SEED: u64 = 0xB1A5;
        let added = monte_carlo(&build(1.0), RUNS, SEED).mean_damage
            - monte_carlo(&build(0.0), RUNS, SEED).mean_damage;
        assert!(
            added > 0.0,
            "a saturating Blast build must split at least once; added {added}"
        );
    }

    /// ...and the two components come up about evenly (50/50, per the owner).
    #[test]
    fn debilitate_picks_either_component_evenly() {
        let mut rng = Rng::new(0x5EED);
        let mut tox = 0;
        const N: usize = 4000;
        for _ in 0..N {
            match debilitate_split(DamageType::Corrosive, DEBILITATE_STACKS, 1.0, &mut rng) {
                Some(DamageType::Toxin) => tox += 1,
                Some(DamageType::Electricity) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        let share = tox as f64 / N as f64;
        assert!((share - 0.5).abs() < 0.05, "toxin share {share}, want ~0.5");
    }

    /// A SPLIT PROC TAKES ITS OWN ELEMENT'S BRACKET, which is the whole reason
    /// the split is implemented as an ordinary proc one depth down rather than
    /// as a bespoke damage formula. Corrosive is Electricity + Toxin: a build
    /// carrying a Toxin mod and no Electricity mod scales a split TOXIN tick by
    /// the Toxin bonus and a split ELECTRICITY tick by 1.0 — "otherwise you
    /// only get the base portion" (owner, 2026-08-05).
    #[test]
    fn a_split_proc_scales_by_its_own_elements_mods() {
        // +90% Toxin and no Electricity mod at all.
        let p = DummyParams {
            elem_dot_bonus: vec![(DamageType::Toxin, 1.9)],
            ..Default::default()
        };
        assert!((p.elem_bracket(DamageType::Toxin) - 1.9).abs() < 1e-9);
        assert!(
            (p.elem_bracket(DamageType::Electricity) - 1.0).abs() < 1e-9,
            "an element with no mod contributes nothing to the bracket"
        );
        // ...and the combined element's own bracket is neither of them, so
        // reusing it for the split would be wrong in both directions.
        assert!((p.elem_bracket(DamageType::Corrosive) - 1.0).abs() < 1e-9);
    }

    /// PRIMARY DEBILITATE, END TO END — and the test MEASURES the faction
    /// exponent rather than asserting the constant the code was written from.
    ///
    /// The split procs are the only difference between the arcane on and off,
    /// so the damage they ADD is isolated by subtraction. If those procs sit at
    /// depth 3, that added damage scales by f³ when the faction bonus changes —
    /// while everything else in the fight scales by f or f². Dividing the added
    /// damage at two faction values therefore reads the exponent straight out
    /// of the simulation:
    ///
    ///     added(f) / added(1) == f³
    ///
    /// This is the assertion the wiki's "three separate times" earns. It also
    /// proves the WIRING, not just the arithmetic — `faction_at` was already
    /// tested on its own, and a recursion that never fired would pass that and
    /// fail this.
    #[test]
    fn a_debilitate_split_lands_at_the_third_faction_layer() {
        // A pure-Corrosive weapon that procs constantly: every proc is the same
        // combined type, so the target saturates and stays saturated.
        let base = |faction: f64, chance: f64| DummyParams {
            damage: DamageVector::new().with(DamageType::Corrosive, 100.0),
            status_chance: 1.0,
            base_status_chance: 1.0,
            fire_rate: 10.0,
            magazine_size: 1e9,
            infinite_reserve: true,
            faction_mult: faction,
            // The component bracket is what a split tick scales by; give Toxin
            // and Electricity real mod bonuses so the split has something to
            // read and the two branches are not both 1.0.
            elem_dot_bonus: vec![(DamageType::Toxin, 1.5), (DamageType::Electricity, 1.5)],
            arcane: crate::arcanes_data::ArcaneFx {
                debilitate_chance: chance,
                ..crate::arcanes_data::ArcaneFx::none()
            },
            ..DummyParams::default()
        };

        const RUNS: u32 = 400;
        const SEED: u64 = 0xDEB1;
        let dmg = |faction: f64, chance: f64| {
            monte_carlo(&base(faction, chance), RUNS, SEED).mean_damage
        };

        let f = 1.55;
        let added_plain = dmg(1.0, 1.0) - dmg(1.0, 0.0);
        let added_faction = dmg(f, 1.0) - dmg(f, 0.0);

        assert!(
            added_plain > 0.0,
            "the arcane must add damage at all — the split never fired"
        );
        let exponent_ratio = added_faction / added_plain;
        let want = f * f * f;
        assert!(
            (exponent_ratio / want - 1.0).abs() < 0.02,
            "split damage scaled by {exponent_ratio} across a {f} faction bonus; \
             f³ is {want}, f² would be {}",
            f * f
        );
    }

    /// ...AND IT BURNS OFF `ModifiedBase`, WHICH MAY BE WRONG (M33).
    ///
    /// This pins TODAY'S reading so the open question cannot be resolved by
    /// accident. A DoT's nominal base excludes the elemental portions of the
    /// hit — DE's rule for a status a WEAPON applied — and this says the split
    /// follows it: doubling how much ELEMENT the hit carries, with
    /// `ModifiedBase` held fixed, changes nothing.
    ///
    /// The rival reading is that the split reads the INSTANCE that applied it,
    /// the whole modded hit, the way an ability-applied status does (Toxic
    /// Lash: 78 direct, a tick of 39 — half of 78, not half of the weapon's
    /// 200). Under it this ratio is 2.0. The formula that decodes M33's in-game
    /// 29551 has that shape, but its parent is an ABILITY's hit, so it
    /// demonstrates the ability case and not this one. Flipping the assertion
    /// below to 2.0 is the whole of the change.
    ///
    /// The fixture holds `ModifiedBase` at 100 and varies how much ELEMENT the
    /// hit carries on top of it — 100 of Corrosive against 200 — which is what
    /// an elemental mod does. Under the old reading the split's DoT is `0.5 x
    /// ModifiedBase x child bracket` and does not move at all; under this one
    /// it doubles with the hit that applied it.
    ///
    /// Varying `dot_modified_base` instead proves nothing, and finding that out
    /// is worth the line: `mb_live` is derived FROM it, so halving it halves
    /// `mb_live` and doubles the ratio, and the product — the instance's own
    /// damage — is invariant. Which is the property being claimed, so a test
    /// written that way passes at ratio 1.000 whichever reading is in force.
    #[test]
    fn a_debilitate_split_burns_off_modified_base_not_the_hit() {
        // Corrosive only: it has no DoT of its own, so every point of damage
        // the arcane adds is the split's, and nothing else moves with the base.
        let build = |hit: f64, chance: f64| DummyParams {
            damage: DamageVector::new().with(DamageType::Corrosive, hit),
            dot_modified_base: Some(100.0),
            status_chance: 1.0,
            base_status_chance: 1.0,
            fire_rate: 10.0,
            magazine_size: 1e9,
            infinite_reserve: true,
            elem_dot_bonus: vec![(DamageType::Toxin, 1.5), (DamageType::Electricity, 1.5)],
            arcane: crate::arcanes_data::ArcaneFx {
                debilitate_chance: chance,
                ..crate::arcanes_data::ArcaneFx::none()
            },
            ..DummyParams::default()
        };
        const RUNS: u32 = 400;
        const SEED: u64 = 0xDEB2;
        let added = |hit: f64| {
            monte_carlo(&build(hit, 1.0), RUNS, SEED).mean_damage
                - monte_carlo(&build(hit, 0.0), RUNS, SEED).mean_damage
        };
        let plain = added(100.0); // hit == ModifiedBase: no element on top
        let doubled = added(200.0); // twice the hit, same ModifiedBase
        assert!(plain > 0.0, "the arcane must add damage at all");
        let ratio = doubled / plain;
        assert!(
            (ratio - 1.0).abs() < 0.02,
            "the split reads ModifiedBase today, so doubling the hit's ELEMENT must              not move it; got {ratio} (the rival reading is 2.0 — see M33)"
        );
    }

    /// GOTVA PRIME'S PASSIVE — a status-triggered crit-chance SET, and the
    /// first crit LOCK in the engine.
    ///
    /// Measured out of the sim rather than asserted: with the passive on, the
    /// share of pellets that crit rises toward the armed rate, and it does so
    /// ONLY when statuses are landing. A weapon that procs nothing can never
    /// arm it, which is the cleanest proof that the trigger is the status and
    /// not the shot.
    /// PRIMARY COMPRESSION, and the column that makes it two mechanics.
    ///
    /// The arcane pays per metre of blast radius given up, and the weapon's own
    /// row says WHERE the payment lands: the Shedu `multiplies` (a free-standing
    /// ×6.28 at 6.6 m), the Braton Incarnon `adds` (+240% into the base-damage
    /// bucket). The two are the same number and NOT the same build, and the
    /// test is the difference rather than either one: a bonus that adds is
    /// DILUTED by Serration and one that multiplies is not, so equipping
    /// Serration must shrink the first one's worth and leave the second's
    /// exactly where it was.
    #[test]
    fn compression_pays_into_the_bracket_its_row_names() {
        let fx = crate::arcanes_data::for_slot("primary", "primary_compression")
            .unwrap()
            .fx(5, crate::loadout::StackPolicy::Emergent, &[], crate::tenno_data::default_tenno());
        let arena = crate::arena::Arena::training(30.0);
        let gain = |weapon: &str, mods: &[&crate::loadout::ModDef]| {
            let base = crate::loadout::WeaponBase::from_data(weapon, true, &[]);
            let panel = crate::loadout::resolve(&base, mods, crate::loadout::StackPolicy::Emergent);
            let with = monte_carlo(
                &DummyParams::from_panel(&panel, &arena, &fx), 8, 0xC0FFEE,
            ).mean_damage;
            let without = monte_carlo(
                &DummyParams::from_panel(&panel, &arena, &ArcaneFx::none()), 8, 0xC0FFEE,
            ).mean_damage;
            with / without
        };
        let pool = crate::mods_data::class_pool("rifle");
        let serration = pool.iter().find(|m| m.id == "serration").expect("serration");
        let mods: Vec<&crate::loadout::ModDef> = vec![serration];

        // The bracket each row names, before any fight runs.
        let shedu = crate::loadout::WeaponBase::from_data("shedu", true, &[]);
        let p = DummyParams::from_panel(
            &crate::loadout::resolve(&shedu, &[], crate::loadout::StackPolicy::Emergent), &arena, &fx,
        );
        // 1 + 6.6 x 0.8 — spelled out, because clippy reads the literal 6.28
        // as an approximation of TAU and it is nothing of the sort.
        assert!((p.compression_mult - (1.0 + 6.6 * 0.8)).abs() < 1e-9, "6.6 m -> +528%");
        assert_eq!(p.compression_bd, 0.0);
        let braton = crate::loadout::WeaponBase::from_data("braton_incarnon", true, &[]);
        let p = DummyParams::from_panel(
            &crate::loadout::resolve(&braton, &[], crate::loadout::StackPolicy::Emergent), &arena, &fx,
        );
        assert!((p.compression_bd - 2.4).abs() < 1e-9, "3.0 m x 0.8 = +240%");
        assert_eq!(p.compression_mult, 1.0);

        // …and the fight tells them apart. Serration is +165%, so an ADDING
        // bonus keeps 1/2.65 of its relative worth and a MULTIPLYING one keeps
        // all of it.
        let (adds_bare, adds_serrated) = (gain("braton_incarnon", &[]), gain("braton_incarnon", &mods));
        assert!(
            adds_serrated < adds_bare - 0.5,
            "an `adds` row is diluted by Serration: x{adds_bare:.2} bare, x{adds_serrated:.2} serrated"
        );
        let (mul_bare, mul_serrated) = (gain("shedu", &[]), gain("shedu", &mods));
        assert!(
            (mul_serrated - mul_bare).abs() < 0.05,
            "a `multiplies` row is not: x{mul_bare:.2} bare, x{mul_serrated:.2} serrated"
        );
        assert!(mul_bare > 2.0, "and it is worth something at all: x{mul_bare:.2}");
    }

    #[test]
    fn gotva_super_crit_arms_on_status_and_only_on_status() {
        let sc = crate::weapons_data::SuperCritSpec { chance: 0.15, crit_chance: 3.0 };
        let build = |status: f64, passive: bool| DummyParams {
            // TOXIN, not Gotva Prime's own Puncture: a Puncture proc applies
            // Weakened, which grants crit chance of its own — the control would
            // then crit for a reason that is not the passive. (In play the two
            // do stack, and that is part of why this weapon likes statuses.)
            damage: DamageVector::new().with(DamageType::Toxin, 100.0),
            // `base_crit_chance` is the RESOLVED one despite the name, so zero
            // here means every crit observed came from the passive.
            base_crit_chance: 0.0,
            unmodded_crit_chance: 0.0,
            status_chance: status,
            base_status_chance: status,
            fire_rate: 10.0,
            magazine_size: 1e9,
            infinite_reserve: true,
            super_crit_on_status: passive.then_some(sc),
            // A CLEAN baseline. `DummyParams::default()` is the Dual Toxocyst
            // fixture WITH Secondary Enervate, whose arcane contributes crit —
            // so without these the "0% crit weapon never crits" control fails
            // for a reason that has nothing to do with the passive.
            arcane: crate::arcanes_data::ArcaneFx::none(),
            crit_tier_upgrade_chance: 0.0,
            weakpoint_cc_rel: 0.0,
            body_parts: vec![BodyPart {
                name: "body".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: false,
                crit_bonus: false,
            }],
            ..DummyParams::default()
        };
        let crit_share = |p: &DummyParams| monte_carlo(p, 300, 0x607A).mean_crit_rate;

        // No passive: a 0% crit weapon never crits, whatever it procs.
        let ctl = crit_share(&build(1.0, false));
        assert!(ctl < 1e-9, "0% crit and no passive, got {ctl}");
        // Passive, but NOTHING to trigger it: still never.
        assert!(
            crit_share(&build(0.0, true)) < 1e-9,
            "a weapon that applies no status can never arm it"
        );
        // Passive AND statuses landing: it crits, and at roughly the armed rate.
        // Every pellet procs, so ~15% of them arm the NEXT one, and an armed
        // pellet crits with certainty (300% is three guaranteed tiers).
        let on = crit_share(&build(1.0, true));
        assert!(
            (on - 0.15).abs() < 0.03,
            "armed share {on}, want ~0.15 (15% of pellets arm the next)"
        );

        // ...AND IT DOES NOT CARE WHERE THE PELLET LANDS. The card says "the
        // next hit", not "the next weak-point hit", so an armed body shot is
        // 300% exactly as an armed headshot is — and a weak-point crit bonus
        // (Pistol/Primary Acuity) contributes NOTHING to an armed pellet,
        // because the value is SET and not added to.
        //
        // Built with a weak-point crit bonus present and a 0% base, so the only
        // way a head pellet could out-crit a body pellet is if the bonus
        // survived the set. It does not.
        let aimed = |head: bool| DummyParams {
            weakpoint_cc_rel: 3.5, // Acuity rank 10
            body_parts: vec![BodyPart {
                name: if head { "head".into() } else { "body".into() },
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: head,
                crit_bonus: false,
            }],
            ..build(1.0, true)
        };
        let (h, b) = (crit_share(&aimed(true)), crit_share(&aimed(false)));
        assert!(
            (h - b).abs() < 0.02,
            "armed crit share differs by body part: head {h}, body {b} — the SET is              supposed to replace the weak-point bonus, not stack with it"
        );
    }

}

/// A BUFF IS THREE PLACES, and a card that appears without them lies.
///
/// `EvolutionDef::buff_cards` decides what the UI OFFERS. The sim decides what
/// it DOES, and that takes three separate arms in this file: the roster (what
/// the replay draws), the sampler (what its curve reads), and the config (what
/// a locked/seeded stack count means). Headcracker shipped with the card and
/// none of the three for one commit, which is the worst shape available — the
/// panel offered a control, the control did nothing, and nothing said so.
///
/// Asserted for BOTH on-headshot buffs, because the failure is silent: a buff
/// missing from the roster simply never appears, and no test that looks only at
/// damage would notice.
#[cfg(test)]
mod headshot_buff_wiring_tests {
    use super::*;

    fn roster_of(evo: &str) -> Vec<String> {
        let base = crate::loadout::WeaponBase::from_data(
            "furis_incarnon",
            true,
            &["furis_evo1_incarnon_form", evo],
        );
        let p = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
        let params = DummyParams::from_panel(&p, &crate::arena::Arena::training(30.0), &ArcaneFx::none());
        params.buff_roster().into_iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn headcracker_is_on_the_replay_roster() {
        let r = roster_of("furis_headcracker");
        assert!(
            r.iter().any(|id| id == "on_headshot_fire_rate"),
            "the card is offered, so the curve must exist too: {r:?}"
        );
    }

    /// ...and it is NOT there when the perk is not taken — a roster that always
    /// lists it would draw a flat zero line and read as a finding.
    #[test]
    fn and_only_when_the_perk_is_taken() {
        let r = roster_of("furis_elemental_balance");
        assert!(!r.iter().any(|id| id == "on_headshot_fire_rate"), "{r:?}");
    }

    /// EVERY card the evolution loader offers must have a sim arm behind it.
    /// This is the general form of the bug rather than the instance: it walks
    /// the whole evolution pool, so the next perk to gain a card fails here
    /// until the three arms exist.
    #[test]
    fn every_evolution_buff_card_is_backed_by_the_sim() {
        for e in crate::evolutions_data::pool() {
            for card in e.buff_cards() {
                let base = crate::loadout::WeaponBase::from_data(
                    &e.weapon,
                    true,
                    &[e.id.as_str()],
                );
                let p = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
                let params = DummyParams::from_panel(&p, &crate::arena::Arena::training(30.0), &ArcaneFx::none());
                let listed = params.buff_roster().into_iter().any(|(id, _)| id == card.id);
                assert!(
                    listed,
                    "{} offers a buff card `{}` the sim never rosters — the panel would \
                     show a control that does nothing",
                    e.id, card.id
                );
            }
        }
    }
}

/// A CYCLE RESOLVES ONE BUFF TWICE, once per form, and the two answers differ.
///
/// `BuffGrant::FireRate` carries an ABSOLUTE rate that `resolve` derives from
/// that form's own base — the Furis Incarnon's 12 ticks/s against the base
/// form's 10 — so the same perk is worth a different number in each half of the
/// engagement. The stacks are shared (one buff, one count, one fight); only the
/// conversion is per form.
///
/// This is the bug the stacking-buff refactor introduced and the baseline
/// caught: reading the outer params instead of the ACTIVE form handed the base
/// form the Incarnon form's rate, which showed up as more shots per engagement
/// and moved nothing else. A diff against a hand-captured baseline will not
/// exist next time, so it is asserted here.
#[cfg(test)]
mod cycle_buff_conversion_tests {
    #[test]
    fn a_fire_rate_buff_converts_against_each_forms_own_base() {
        let evos = [
            "furis_evo1_incarnon_form",
            "furis_haven_foray",
            "furis_extended_volley",
            "furis_headcracker",
        ];
        let inc = crate::loadout::WeaponBase::from_data("furis_incarnon", true, &evos);
        let base = crate::loadout::WeaponBase::from_data("furis", true, &evos);
        let pol = crate::loadout::StackPolicy::Emergent;
        let pi = crate::loadout::resolve(&inc, &[], pol);
        let pb = crate::loadout::resolve(&base, &[], pol);

        let rate_of = |p: &crate::loadout::ResolvedPanel| {
            p.stacking_buffs
                .iter()
                .find(|b| b.grant == crate::loadout::BuffGrant::FireRate)
                .map(|b| b.per_stack)
                .expect("Headcracker resolves a fire-rate buff on both forms")
        };
        // +5% of each form's own base: 12 x 0.05 = 0.6, and 10 x 0.05 = 0.5.
        assert!((rate_of(&pi) - 0.6).abs() < 1e-9, "incarnon {}", rate_of(&pi));
        assert!((rate_of(&pb) - 0.5).abs() < 1e-9, "base {}", rate_of(&pb));
        assert!(
            rate_of(&pi) > rate_of(&pb),
            "the faster form must be worth more per stack, or the sim is reading one form's \
             rate while firing the other"
        );
    }
}

/// STORMBURST: a buff whose condition is on the TARGET, not on the shot.
///
/// It was inert until the StackingBuff refactor, and the reason is worth
/// keeping: the static path (`AssumedMaxMultishot`) carries a total and a cap
/// and NO trigger, so declaring it there would have granted +1.2 multishot to
/// a build with no Electricity in it at all. A LIVE buff is bumped inside the
/// fight, where the target's debuffs are in hand — so the condition it could
/// not state statically is simply readable.
///
/// ON THIS WEAPON IT ONLY WORKS AS A CYCLE, which the sim found rather than
/// anyone predicting it. The Incarnon form's base damage is Heat, so a
/// Convulsion's Electricity COMBINES with it: that form deals Radiation 339 and
/// no Electricity whatever. The BASE form has no innate element, so its
/// Electricity stays raw — it is the half of the cycle that puts the status on
/// the target, and the Incarnon half then finds it there. Fire the Incarnon
/// form alone and this perk can never trigger off its own procs.
#[cfg(test)]
mod stormburst_tests {
    use super::*;

    /// Extra pellets beyond one per shot — what a flat multishot bonus looks
    /// like. Run as the CYCLE, because that is the only way this weapon ever
    /// has Electricity on the target (see the note above).
    fn extra_pellets(mods: &[&str]) -> f64 {
        let evos = ["furis_evo1_incarnon_form", "furis_stormburst", "furis_extended_volley"];
        let inc = crate::loadout::WeaponBase::from_data("furis_incarnon", true, &evos);
        let base = crate::loadout::WeaponBase::from_data("furis", true, &evos);
        let pool = crate::mods_data::pool_for_weapon("furis_incarnon");
        let picked: Vec<&crate::loadout::ModDef> = mods
            .iter()
            .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id}")))
            .collect();
        let pol = crate::loadout::StackPolicy::Emergent;
        let pi = crate::loadout::resolve(&inc, &picked, pol);
        let pb = crate::loadout::resolve(&base, &picked, pol);
        let params = DummyParams::incarnon_cycle_from_panels(
            &pi,
            &pb,
            false,
            LockMode::Initial(0),
            &crate::arena::Arena::training(60.0),
            &ArcaneFx::none(),
        );
        let s = monte_carlo(&params, 6, 777);
        s.mean_pellets - s.mean_shots
    }

    /// With no Electricity anywhere the perk grants nothing: one pellet a shot.
    /// This is the assertion that fails if it is ever modelled the static way.
    #[test]
    fn it_grants_nothing_without_electricity() {
        let extra = extra_pellets(&[]);
        assert!(extra.abs() < 1e-9, "extra pellets with no Electricity: {extra}");
    }

    /// ...and pays out once the target is shocked.
    #[test]
    fn it_pays_out_once_the_target_is_shocked() {
        let extra = extra_pellets(&["primed_convulsion"]);
        assert!(extra > 0.0, "expected extra pellets once Electricity lands, got {extra}");
    }
}

/// THE TWO DECAY FAMILIES, told apart on the clock.
///
/// `docs/BUFFS.md` has named three since the buff vocabulary was written and
/// only one of the timed ones was implemented; every stacking buff therefore
/// decayed the Galvanized way whether or not that was its rule. Stormburst is
/// the first that is not (owner, in game 2026-08-07: each stack keeps its own
/// 2 s clock, FIFO, cap 3), and the difference is not cosmetic — under the
/// Galvanized rule ONE hit per window holds the whole pile, under this one it
/// holds exactly one stack.
#[cfg(test)]
mod buff_decay_family_tests {
    use super::*;

    #[test]
    fn a_shared_clock_lets_one_hit_hold_every_stack() {
        let mut s = LiveStacks::seed(0, 3, 2.0);
        s.bump(0.0, 2.0, 3);
        s.bump(0.1, 2.0, 3);
        s.bump(0.2, 2.0, 3);
        assert_eq!(s.current(0.3, 2.0), 3);
        // One more hit just before the shared clock falls due, and NOTHING is
        // lost — the bump restarted the timer for all three.
        s.bump(2.0, 2.0, 3);
        assert_eq!(s.current(3.9, 2.0), 3);
    }

    #[test]
    fn a_per_stack_clock_makes_one_hit_hold_exactly_one() {
        let mut s = LiveStacks::seed_per_stack(0, 3, 2.0);
        s.bump(0.0, 2.0, 3);
        s.bump(0.1, 2.0, 3);
        s.bump(0.2, 2.0, 3);
        assert_eq!(s.current(0.3, 2.0), 3);
        // The same single hit at t=2.0. The first three expire on their own
        // clocks at 2.0/2.1/2.2 regardless, so by 3.9 only the new one is left.
        s.bump(2.0, 2.0, 3);
        assert_eq!(s.current(3.9, 2.0), 1, "each stack expires on its own clock");
    }

    /// FIFO at the cap: a fourth stack pushes the OLDEST out rather than being
    /// dropped, so a capped pile still rolls forward.
    #[test]
    fn at_the_cap_the_oldest_stack_leaves_first() {
        let mut s = LiveStacks::seed_per_stack(0, 3, 2.0);
        for i in 0..4 {
            s.bump(i as f64 * 0.1, 2.0, 3);
        }
        assert_eq!(s.current(0.4, 2.0), 3, "still capped at 3");
        // The oldest (expiring at 2.0) is gone; the youngest survives past it.
        assert_eq!(s.current(2.05, 2.0), 3, "the evicted one took no live stack with it");
    }
}

/// A CHANGE THAT PAYS NOTHING MUST READ AS NOTHING.
///
/// The simulator is a sampler, so two builds are compared by running both and
/// subtracting — and that only means anything if the seed means the same thing
/// in both. It did not: every roll came off one stream, so a status chance high
/// enough to land one more proc drew one more number to pick its element, and
/// every crit and body part after it was a different draw. Two builds that
/// differ in nothing that pays came back differing by noise, and the page
/// printed that noise as a recommendation (owner, 2026-08-07).
///
/// IMPACT is the clean case. It pushes a stagger stack and nothing else — a
/// single-target damage sim has no notion of an enemy being interrupted — so
/// more Impact procs must be worth EXACTLY nothing.
///
/// COLD IS NOT that case, which is worth writing down because it was the one
/// reported. A Cold status raises the crit damage the target TAKES (+10% on the
/// first stack, +5% on each further, +100% while Frozen), so more Cold procs
/// really are more damage. That is the buff-shaped effect the owner expected to
/// be the only way status can pay — it is simply that Cold has one.
#[cfg(test)]
mod stream_independence_tests {
    use super::*;

    /// Pure Impact, ordinary crit, nothing on the build that reads status.
    fn inert(status_chance: f64) -> DummyParams {
        DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            status_chance,
            base_status_chance: status_chance,
            base_crit_chance: 0.3,
            crit_multiplier: 2.0,
            multishot: 1.6,
            base_multishot: 1.6,
            duration_secs: 60.0,
            ..DummyParams::default()
        }
    }

    #[test]
    fn more_status_that_pays_nothing_reads_as_nothing() {
        let low = monte_carlo(&inert(0.1), 40, 12345);
        let high = monte_carlo(&inert(0.9), 40, 12345);
        // It really did change the fight...
        assert!(
            high.mean_procs > low.mean_procs * 2.0,
            "the premise is wrong — procs {} vs {}",
            low.mean_procs, high.mean_procs
        );
        // ...and none of it was worth a point of damage.
        assert!(
            (high.mean_damage - low.mean_damage).abs() < 1e-9,
            "an Impact-only build paid {} for status it cannot spend (low {} high {})",
            high.mean_damage - low.mean_damage, low.mean_damage, high.mean_damage
        );
        // The crits are the same crits, not merely the same average.
        assert!(
            (high.mean_crit_rate - low.mean_crit_rate).abs() < 1e-12,
            "the crit stream moved: {} vs {}", low.mean_crit_rate, high.mean_crit_rate
        );
        assert!(
            (high.mean_headshot_rate - low.mean_headshot_rate).abs() < 1e-12,
            "the body-part stream moved: {} vs {}", low.mean_headshot_rate, high.mean_headshot_rate
        );
    }

    /// ...and the split did not cost the sampler its randomness: the spine
    /// still answers to the seed.
    #[test]
    fn the_spine_still_varies_with_the_seed() {
        let a = monte_carlo(&inert(0.5), 20, 1);
        let b = monte_carlo(&inert(0.5), 20, 2);
        assert!(
            (a.mean_damage - b.mean_damage).abs() > 1e-9,
            "two seeds produced one fight"
        );
    }
}

/// ...AND EVERY BUFF THE ROSTER OFFERS MUST BE READ BACK BY THE REPLAY.
///
/// This is the THIRD list. `buff_roster` says what exists, `apply_buff_config`
/// obeys the card, and `sample_stacks` reads the live count for the curve on
/// screen — and that last one ends in a catch-all `_ => 0`, so a rostered buff
/// with no arm there does not fail, it draws a flat zero line for the whole
/// engagement. That reads as "the buff never came up", which is a sentence
/// about the build rather than about the code, and it is wrong.
///
/// `every_buff_the_roster_offers_is_actually_read` already pins roster against
/// config. This pins roster against the replay, the same way: seed every buff
/// at its cap with the card LOCKED, replay one frame, and assert nothing that
/// was offered reads zero.
#[cfg(test)]
mod replay_reads_every_buff_tests {
    use super::*;

    #[test]
    fn no_rostered_buff_draws_a_flat_zero_it_did_not_earn() {
        use crate::loadout::{StackSpec, StackingBuff, TimedBuff};
        let stack = |per_stack: f64| StackSpec {
            per_stack,
            max_stacks: 3,
            duration: 6.0,
            initial_stacks: 0,
        };
        let timed = |value: f64| TimedBuff { value, duration: 4.0, initial_active: false };
        let buff = |id: &'static str, grant, trigger| StackingBuff {
            id,
            trigger,
            grant,
            chance: 1.0,
            decay: crate::loadout::BuffDecay::LoseOneAndReset,
            per_stack: 0.1,
            max_stacks: 3,
            duration: 10.0,
            initial_stacks: 0,
            stacks_per_trigger: 1,
            per_shell: false,
            cleared_by: crate::loadout::ClearedBy::Nothing,
        };
        let mut params = DummyParams {
            co_stack: Some(stack(0.2)),
            ms_stack: Some(stack(0.3)),
            cc_stack: Some(stack(0.1)),
            stacking_buffs: vec![
                buff("on_plain_hit_damage", crate::loadout::BuffGrant::BaseDamage,
                     crate::loadout::BuffTrigger::PlainHit),
                buff("on_headshot_reload_speed", crate::loadout::BuffGrant::ReloadSpeed,
                     crate::loadout::BuffTrigger::Headshot),
            ],
            cc_on_headshot: Some(timed(0.5)),
            cd_on_kill: Some(timed(0.6)),
            fr_on_reload: Some(timed(0.7)),
            bd_on_reload: Some(timed(0.8)),
            // Rostered only where a mod reads the count, so the fixture
            // carries both halves — the passive and the mod that pays for it.
            tendril_max: 4,
            cc_per_tendril: 0.1,
            ..DummyParams::default()
        };

        // Seed EVERY offered buff at its cap and lock it, so a zero on the
        // first frame can only mean nothing read it.
        let roster = params.buff_roster();
        assert!(roster.len() >= 9, "the fixture stopped covering the roster: {roster:?}");
        let mut cfg = BuffConfig::new();
        for (id, max) in &roster {
            cfg.insert(id.clone(), (if *max == 0 { 1 } else { *max }, true));
        }
        params.apply_buff_config(&cfg);

        let rep = replay(&params, 12345, 4);
        let first = &rep.frames[0];
        // The weapon passive is applied by the api, not by these params — the
        // same exemption `every_buff_the_roster_offers_is_actually_read` makes.
        for (i, (id, _)) in rep.buffs.iter().enumerate() {
            if id == "frenzy" {
                continue;
            }
            assert!(
                first.stacks[i] > 0,
                "`{id}` is offered and configured to its cap, and the replay reads 0 — \
                 nothing in `sample_stacks` answers for it, so its curve is a flat line"
            );
        }
    }
}
#[cfg(test)]
mod attrition_times_co_tests {
    use super::*;

    /// DEVASTATING ATTRITION AND GUN CO MULTIPLY, they do not share a bracket.
    ///
    /// MEASURED IN GAME by the owner (2026-08-08: "我已经测试过了"), which is
    /// what makes this a fact rather than a reading. It also agrees with both
    /// sources: the perk's own wiki note says "multiplicative to base damage
    /// bonuses such as Hornet Strike", and the weapon sits on the CO catalog's
    /// Multiplying row — so neither term is in the base-damage bucket and they
    /// have nothing to share.
    ///
    /// The measurement is what this test is for. "They are separate factors in
    /// the product" is a claim about the code; a ratio of ratios is a claim
    /// about the number, and only the second one can be wrong quietly.
    ///
    /// The test is a ratio of ratios: whatever the perk is worth alone, and
    /// whatever CO is worth alone, having both must be worth their product. If
    /// either ever joined the other's bucket the product would collapse toward
    /// the larger of the two.
    #[test]
    fn devastating_attrition_multiplies_with_gun_condition_overload() {
        let base = crate::loadout::WeaponBase::from_data("felarx", true, &[]);
        let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::AssumedMax);
        assert_eq!(
            panel.co_behavior,
            crate::loadout::CoBehavior::Independent,
            "the Felarx is on the catalog's Multiplying row, both modes"
        );
        let arena = crate::arena::Arena::training(30.0);

        // Four fights that differ ONLY in the two terms under test, on one
        // seed, so the crit rolls and the shot timing are identical.
        let run = |attrition: bool, co: bool| {
            let mut p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
            // Chance 1.0, not the perk's own 0.5: a coin flip inside the
            // measurement would need thousands of runs to say anything, and
            // the question is about the BRACKET, not about the odds.
            p.noncrit_bonus = attrition.then_some((1.0, 20.0));
            p.co_per_type = if co { 0.8 } else { 0.0 };
            let mut rng = crate::rng::Rng::new(11);
            run_once(&p, &mut rng).total_damage
        };
        let plain = run(false, false);
        let with_attr = run(true, false);
        let with_co = run(false, true);
        let with_both = run(true, true);
        assert!(plain > 0.0 && with_attr > plain && with_co > plain);

        let r_attr = with_attr / plain;
        let r_co = with_co / plain;
        let r_both = with_both / plain;
        assert!(
            (r_both - r_attr * r_co).abs() < r_both * 0.02,
            "multiplicative: attrition x{r_attr:.2}, CO x{r_co:.2}, both x{r_both:.2}              (their product is {:.2})",
            r_attr * r_co
        );
        // …and NOT additive, which is the reading this rules out. With both
        // worth several times the base, the two answers are far apart.
        let additive = 1.0 + (r_attr - 1.0) + (r_co - 1.0);
        assert!(
            (r_both - additive).abs() > r_both * 0.2,
            "and nowhere near sharing a bracket: {r_both:.2} vs {additive:.2}"
        );
    }
}
#[cfg(test)]
mod debilitate_attrition_tests {
    use super::*;

    /// A DEBILITATE DoT EATS ATTRITION TWICE, and the calculator gave it zero
    /// (player report through the owner, 2026-08-08: "衰弱触发的 dot 可以再次触
    /// 发……外围 20 倍但是网站里的计算器显示不出来", then measured: the final DoT
    /// eats three faction layers and "441倍强袭损耗" = 21x21). It is a BUG of
    /// DE's — the split fires a ZERO-damage instance that still takes its own
    /// faction bracket and its own Attrition roll, and when the DoT replaces the
    /// zero with the parent hit's value those two multipliers stay behind.
    ///
    /// Three claims:
    ///   1. an ORDINARY status DoT carries the applying hit's roll, and only
    ///      that — exactly 21x;
    ///   2. a SPLIT's DoT carries a second one — 21x21 = 441x;
    ///   3. and the second one lands EVEN ON A CRITTING HIT, where the parent's
    ///      own roll is worth nothing. This is the counter-intuitive half: the
    ///      zero instance has no crit of its own, so the perk's condition holds
    ///      whatever the parent did. ✅ Confirmed in game on a SECOND weapon
    ///      (owner, 2026-08-10): Phenmor at guaranteed crit, Devouring
    ///      Attrition, and the split's DoT still comes out x21 some of the time
    ///      — so the split is permanently non-critical rather than a zero that
    ///      rolls crit, which is the distinction claim 3 exists to make.
    ///
    /// The roll is forced to 1.0 throughout: the perk's own 50% would need
    /// thousands of runs to separate 21 from 22, and the question is which
    /// layers apply rather than the odds. Health is enormous so nothing dies —
    /// otherwise a 21x direct hit ends the fight and the DoT totals fall while
    /// every tick grows.
    #[test]
    fn the_debilitate_dot_carries_two_attrition_layers() {
        let base = crate::loadout::WeaponBase::from_data("felarx", true, &[]);
        let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::AssumedMax);
        let arena = crate::arena::Arena::training(30.0);
        // AVERAGED OVER 200 RUNS. Turning the perk on consumes an extra RNG
        // draw per instance, so the two fights diverge shot for shot and a
        // single pair of runs compares two different fights — the ratio is only
        // a statement about the multiplier in the aggregate.
        let dots = |attrition: bool, debilitate: f64, crit: bool| {
            (0..200u64)
                .map(|seed| {
                    let mut p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
                    p.target.base_health = 1e15;
                    p.crit_tier_upgrade_chance = 0.0;
                    p.super_crit_on_status = None;
                    p.base_crit_chance = if crit { 1.0 } else { 0.0 };
                    p.unmodded_crit_chance = 0.0;
                    // PUNCTURE IS IMMUNE, so the crit rate is the one this test
                    // sets. Weakened is a flat crit-chance buff on the victim and
                    // a saturating status build keeps it up — a critical instance
                    // is not eligible for Attrition, so leaving it in would mix
                    // untouched instances into every ratio below.
                    p.target.status_immunities = vec![DamageType::Puncture];
                    p.status_chance = 4.0;
                    p.arcane.debilitate_chance = debilitate;
                    if debilitate > 0.0 {
                        // PURE CORROSIVE, so every DoT in the run is a SPLIT's.
                        // The arcane splits a COMBINED element into a component,
                        // and the Felarx's own vector is IPS — nothing to split,
                        // which is why claim 1's fight sees no splits at all.
                        // With no Toxin of its own the weapon cannot apply a
                        // Toxin DoT by any other route, so the whole of
                        // `dot_damage` here went through the split.
                        let total = p.damage.total();
                        p.damage = crate::damage::DamageVector::new()
                            .with(DamageType::Corrosive, total);
                    }
                    p.noncrit_bonus = attrition.then_some((1.0, 20.0));
                    let mut rng = crate::rng::Rng::new(seed);
                    run_once(&p, &mut rng).dot_damage
                })
                .sum::<f64>()
        };
        // 1. ordinary DoTs: one layer, and the whole of it.
        let plain = dots(false, 0.0, false);
        assert!(plain > 0.0);
        let one_layer = dots(true, 0.0, false) / plain;
        assert!(
            (one_layer - 21.0).abs() < 1.0,
            "an ordinary DoT takes the hit's roll and nothing else: x{one_layer:.2}"
        );
        // 2. a split's DoT: two layers, 21x21 = 441x — the owner's own number.
        let plain_split = dots(false, 1.0, false);
        assert!(plain_split > 0.0, "the Corrosive fight has to produce split DoTs");
        let two_layers = dots(true, 1.0, false) / plain_split;
        assert!(
            (two_layers - 441.0).abs() < 25.0,
            "the split's DoT takes the hit's roll AND its own: x{two_layers:.1},              measured 441 (one layer is x{one_layer:.2})"
        );
        // 2b. AND THE SPLIT'S ROLL IS ITS OWN — the half the forced-chance
        //     runs above cannot see (owner, 2026-08-10: "衰弱自己再判定一次是
        //     否触发21倍伤害（自己的）… 衰弱自己的那个0伤害extra hit要自己再判断
        //     一次").
        //
        //     At the perk's real 50% the two readings are far apart, and a mean
        //     tells them apart on its own:
        //
        //       two INDEPENDENT rolls   E[hit] x E[split] = 11 x 11 = 121
        //       the split COPYING it    E[hit^2] = .5x441 + .5x1    = 221
        //
        //     Nothing else in the fight moves, so the ratio against a run with
        //     the perk off is that expectation.
        // THE SAME 200 RUNS the baseline used. A ratio against a different
        // number of runs is a ratio against a different fight, and it reads as
        // a mechanic — this said x243 for a while, which is 121 x 2.
        let half = |seed: u64| {
            (0..200u64)
                .map(|k| {
                    let mut p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
                    p.target.base_health = 1e15;
                    p.crit_tier_upgrade_chance = 0.0;
                    p.super_crit_on_status = None;
                    p.base_crit_chance = 0.0;
                    p.unmodded_crit_chance = 0.0;
                    p.target.status_immunities = vec![DamageType::Puncture];
                    p.status_chance = 4.0;
                    p.arcane.debilitate_chance = 1.0;
                    let total = p.damage.total();
                    p.damage = crate::damage::DamageVector::new()
                        .with(DamageType::Corrosive, total);
                    p.noncrit_bonus = Some((0.5, 20.0));
                    let mut rng = crate::rng::Rng::new(seed + k);
                    run_once(&p, &mut rng).dot_damage
                })
                .sum::<f64>()
        };
        let coin = half(1000) / plain_split;
        assert!(
            (coin - 121.0).abs() < 25.0,
            "at a real 50% the split's own roll is independent: x{coin:.0}              (121 = two coins, 221 = the split copying the hit)"
        );

        // 3. every hit crits, so the HIT's roll is worth nothing — and the
        //    split's is worth its full 21x anyway.
        let crit_split = dots(false, 1.0, true);
        assert!(crit_split > 0.0);
        let on_crit = dots(true, 1.0, true) / crit_split;
        assert!(
            (on_crit - 21.0).abs() < 1.0,
            "the zero instance has no crit to disqualify it: x{on_crit:.2} on a              fight that crits every shot (x441 when nothing crits)"
        );
    }

    /// A CRIT TAKES THE HIT'S COIN AWAY AND KEEPS ITS OWN MULTIPLIER — so on
    /// the Debilitate DoT, and only there, critting can be worth LESS than not.
    ///
    /// The owner's reading (2026-08-10: "如果直击是暴击的，但是后面的衰弱 dot 还
    /// 是可以 roll 出 21，那么此时会带着前面的各种 multiplier……因为衰弱永远不暴
    /// 击"). Both halves are true and they pull opposite ways:
    ///
    /// - a critical hit is not eligible for Devouring Attrition, so the HIT's
    ///   coin is gone — one coin instead of two;
    /// - the split instance never crits, so ITS coin is always live, and the
    ///   DoT still inherits the hit's crit multiplier and its body part.
    ///
    /// Which makes the comparison arithmetic rather than opinion:
    ///
    ///     not critting   E = 11 x 11         = 121
    ///     critting       E = crit_mult x 11
    ///
    /// **They cross at a crit multiplier of 11.** Measured here: Attrition is
    /// worth x121 with no crits and x11 with them at ANY multiplier — the same
    /// x11 whether the crit is 3x or 21x, which is what shows it is the hit's
    /// coin that went missing rather than a scaled version of it.
    ///
    /// This is the DoT bucket alone; the direct damage still wants crits and no
    /// real build gives them up. It is worth pinning because it is the one
    /// place in this model where two of the weapon's own perks pull apart.
    #[test]
    fn a_crit_costs_the_split_a_coin_and_pays_it_back_in_multiplier() {
        let base = crate::loadout::WeaponBase::from_data("felarx", true, &[]);
        let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::AssumedMax);
        let arena = crate::arena::Arena::training(30.0);
        let dots = |attrition: bool, cc: f64, cd: f64| {
            (0..200u64)
                .map(|seed| {
                    let mut p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
                    p.target.base_health = 1e15;
                    p.crit_tier_upgrade_chance = 0.0;
                    p.super_crit_on_status = None;
                    p.base_crit_chance = cc;
                    p.unmodded_crit_chance = 0.0;
                    p.crit_multiplier = cd;
                    p.target.status_immunities = vec![DamageType::Puncture];
                    p.status_chance = 4.0;
                    p.arcane.debilitate_chance = 1.0;
                    let tot = p.damage.total();
                    p.damage = crate::damage::DamageVector::new()
                        .with(DamageType::Corrosive, tot);
                    p.noncrit_bonus = attrition.then_some((0.5, 20.0));
                    let mut rng = crate::rng::Rng::new(seed);
                    run_once(&p, &mut rng).dot_damage
                })
                .sum::<f64>()
        };
        let plain = dots(true, 0.0, 1.0) / dots(false, 0.0, 1.0);
        assert!((plain - 121.0).abs() < 12.0, "no crit: x{plain:.0}");
        for cd in [3.0, 11.0, 21.0] {
            let crit = dots(true, 1.0, cd) / dots(false, 1.0, cd);
            assert!((crit - 11.0).abs() < 1.5, "crit x{cd}: attrition worth x{crit:.1}");
        }
        // …AND THE CROSSOVER IS AT ELEVEN. Below it the DoT is bigger WITHOUT
        // the crit, which is the counterintuitive half and the reason this test
        // exists at all.
        let none = dots(true, 0.0, 1.0);
        assert!(dots(true, 1.0, 3.0) < none, "a 3x crit build should lose here");
        assert!(dots(true, 1.0, 21.0) > none, "and a 21x one should win");
    }
}
#[cfg(test)]
mod warframe_ability_tests {
    use super::*;
    use crate::abilities_data::{resolve, AbilityPick};
    use crate::damage::DamageVector;

    /// A fixed weapon and a fixed fight, so the only thing moving is the buff.
    fn params(abilities: &[(&'static str, Option<f64>)], strength: f64) -> DummyParams {
        let picks: Vec<AbilityPick<'static>> = abilities
            .iter()
            .map(|(id, secs)| AbilityPick { id, duration_s: *secs, element: None })
            .collect();
        DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            dot_modified_base: Some(100.0),
            fire_rate: 1.0,
            magazine_size: 1e9,
            duration_secs: 10.0,
            base_crit_chance: 0.0,
            unmodded_crit_chance: 0.0,
            body_parts: vec![BodyPart {
                name: "body".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: false,
                crit_bonus: false,
            }],
            abilities: resolve(&picks, strength, ""),
            ..DummyParams::default()
        }
    }

    fn direct(p: &DummyParams) -> f64 {
        run_once(p, &mut crate::rng::Rng::new(3)).sources.direct
    }

    /// ROAR IS A BANE MOD, and this asserts exactly that and nothing more: it
    /// lands in the bracket `faction_mult` already is, so a +50% Roar is x1.5
    /// on the hit — and the bracket's own squaring on status follows without a
    /// line of code, which the DoT test below is for.
    #[test]
    fn roar_multiplies_the_hit_by_its_faction_bracket() {
        let none = direct(&params(&[], 1.0));
        let roar = direct(&params(&[("roar", None)], 1.0));
        assert!(none > 0.0);
        assert!((roar / none - 1.5).abs() < 1e-9, "x{:.4}", roar / none);

        // …and STRENGTH is linear, so 200% strength is +100%.
        let strong = direct(&params(&[("roar", None)], 2.0));
        assert!((strong / none - 2.0).abs() < 1e-9, "x{:.4}", strong / none);
    }

    /// AND IT DOUBLE-DIPS ON STATUS, which is the difference from Eclipse.
    /// A Slash DoT applied under +50% Roar ticks for 1.5^2 = 2.25x — the wiki's
    /// "the bonus is used twice in the calculation of status damage".
    #[test]
    fn roar_is_used_twice_on_a_status_tick_and_eclipse_once() {
        let bleed = |abilities: &[(&'static str, Option<f64>)]| {
            let mut p = params(abilities, 1.0);
            p.damage = DamageVector::new().with(DamageType::Slash, 100.0);
            p.status_chance = 1.0;
            p.base_status_chance = 1.0;
            p.target.base_health = 1e15;
            run_once(&p, &mut crate::rng::Rng::new(3)).dot_damage
        };
        let plain = bleed(&[]);
        assert!(plain > 0.0);
        let roar = bleed(&[("roar", None)]) / plain;
        assert!((roar - 2.25).abs() < 1e-6, "roar on a DoT: x{roar:.4}");
        // Eclipse (+200%) is x3 on the hit and x3 on the tick — "Unlike faction
        // damage, which double dips for status effects, the one from Eclipse is
        // applied once". Nine would be the wrong answer.
        let ecl = bleed(&[("eclipse", None)]) / plain;
        assert!((ecl - 3.0).abs() < 1e-6, "eclipse on a DoT: x{ecl:.4}");
    }

    /// THE ADDED ELEMENT DOES NOT COMBINE (owner, 2026-08-08: "注意不合成").
    /// A weapon whose vector is pure Heat, under Shock Trooper, deals Heat AND
    /// Electricity — never Radiation, which is what an elemental MOD would have
    /// made of the same two.
    #[test]
    fn an_ability_element_lands_beside_the_weapons_own_instead_of_combining() {
        let mut p = params(&[("shock_trooper", None)], 1.0);
        p.damage = DamageVector::new().with(DamageType::Heat, 100.0);
        p.dot_modified_base = Some(100.0);
        let r = run_once(&p, &mut crate::rng::Rng::new(3));
        let by = &r.sources.direct_by_type;
        let at = |t: DamageType| by[t as usize];
        assert!(at(DamageType::Heat) > 0.0, "the weapon keeps its own element");
        assert!(at(DamageType::Electricity) > 0.0, "the ability adds its own");
        assert_eq!(at(DamageType::Radiation), 0.0, "and they DO NOT combine");
        // +100% of ModifiedBase, so the two halves are equal.
        let ratio = at(DamageType::Electricity) / at(DamageType::Heat);
        assert!((ratio - 1.0).abs() < 1e-6, "x{ratio:.4}");
    }

    /// A DURATION ENDS IT. Half a fight of Roar is worth less than all of it
    /// and more than none — asserted as an ORDERING rather than a number,
    /// because where the shots fall inside the window is the sim's business.
    #[test]
    fn a_duration_ends_the_buff_mid_fight() {
        let none = direct(&params(&[], 1.0));
        let half = direct(&params(&[("roar", Some(5.0))], 1.0));
        let all = direct(&params(&[("roar", None)], 1.0));
        assert!(none < half && half < all, "{none:.0} / {half:.0} / {all:.0}");
        // The whole-fight run is exactly the 1.5x of the test above, so the
        // partial one is a real fraction of it rather than a rounding.
        assert!((all / none - 1.5).abs() < 1e-9);
    }

    /// THE MEASURED FIGHT (MEASUREMENTS M40) — a Magnus at 98 base with two
    /// 60/60s making Blast (+120%) and a Primed Bane of Grineer (+55%), which
    /// is the capture the owner supplied on 2026-08-09. Every extra-hit test
    /// below runs on it, so the numbers in them are the numbers on the video.
    fn measured() -> DummyParams {
        let mut p = params(&[("xatas_whisper", None)], 1.0);
        // 98 IPS + 98 x 1.2 as Blast, and ModifiedBase is the 98: an elemental
        // mod's damage is not part of the base a status burns off.
        p.damage = DamageVector::new()
            .with(DamageType::Impact, 98.0)
            .with(DamageType::Blast, 98.0 * 1.2);
        p.dot_modified_base = Some(98.0);
        p.faction_mult = 1.55;
        // NO STATUS unless a test asks for it. The fixture behind `params` is a
        // real weapon and procs; a stray Blast would fold a detonation's extra
        // hit into the ratio the first two tests are about, which is exactly
        // the confusion the last two exist to tell apart.
        p.status_chance = 0.0;
        p.base_status_chance = 0.0;
        p.forced_procs = Vec::new();
        // Nothing may die: these are per-instance numbers, and a respawn would
        // put a fresh bar under half of them.
        p.target.base_health = 1e15;
        p
    }

    /// THE WIKI'S OWN WORKED EXAMPLE, to the digit — four numbers, one chain,
    /// and it is the strongest citation this interaction has (owner supplied it
    /// verbatim, 2026-08-09).
    ///
    /// > A gun deals 100 damage per bullet, and we have Thermite Rounds, Rime
    /// > Rounds, Stormbringer, Primed Bane of Grineer, and Xata's whisper at
    /// > base strength: The initial hit will deal
    /// > `100 × (1 + 0.6 + 0.6 + 0.9) × (1 + 0.55) = 480.5`, and Xata's whisper
    /// > will deal `0.26 × 480.5 × (1 + 0.55) = 193.6415` (the Faction Damage
    /// > Bonus is applied again). If the hit proc'd Blast, then the detonation
    /// > damage will be `0.3 × 100 × (1 + 0.55)^2 = 72.075` (Elemental Damage
    /// > doesn't apply to Blast detonations and the Faction Damage Bonus is
    /// > applied again). Then, Xata's whisper will trigger off said detonation,
    /// > dealing `0.26 × 72.075 × (1 + 0.55) × (1 + 0.6 + 0.6 + 0.9) = 90.0433`.
    ///
    /// Every oddity of the category is in those four lines and they check each
    /// other: the elemental bracket is INSIDE the hit and OUTSIDE the
    /// detonation's extra hit, and the faction bonus lands one more time at
    /// every step — `f¹` on the hit, `f²` on its extra hit and on the
    /// detonation, `f³` on the extra hit off the detonation.
    ///
    /// Thermite Rounds and Rime Rounds are Heat and Cold, so they COMBINE: the
    /// vector is 100 Impact + 120 Blast + 90 Electricity, which is also why the
    /// example has a Blast proc to detonate at all.
    fn wiki_example() -> DummyParams {
        let mut p = params(&[("xatas_whisper", None)], 1.0);
        p.damage = DamageVector::new()
            .with(DamageType::Impact, 100.0)
            .with(DamageType::Blast, 120.0)
            .with(DamageType::Electricity, 90.0);
        // THE BASE A STATUS BURNS OFF is the 100, not the 310: an elemental
        // mod's damage is not part of it. That is the whole reason the
        // detonation is 30 and not 93.
        p.dot_modified_base = Some(100.0);
        p.faction_mult = 1.55;
        p.status_chance = 0.0;
        p.base_status_chance = 0.0;
        p.forced_procs = Vec::new();
        p.target.base_health = 1e15;
        p
    }

    #[test]
    fn the_wiki_worked_example_reproduces_to_the_digit() {
        // ONE SHOT, so every figure below is one instance rather than a mean.
        let one = |p: &DummyParams| {
            let mut q = p.clone();
            q.duration_secs = 0.001;
            run_once(&q, &mut crate::rng::Rng::new(3))
        };

        // QUANTISATION IS THE ONE DIFFERENCE, and it is ours being right rather
        // than the example being wrong: DE rounds each element of the vector
        // down to a step of the base, which an illustration written to show a
        // formula has no reason to carry. So the example's 310 is 300.3125 here
        // and every absolute figure below moves with it — the four RELATIONS,
        // which are what the example is demonstrating, are exact.
        let q = wiki_example().damage.quantized().total();
        assert!(q < 310.0 && q > 295.0, "quantised vector {q}");
        let f = 1.55;
        let mb = 100.0;
        // The bracket the extra hit off a detonation picks up: the vector over
        // the base a status burns off — the example's `1 + 0.6 + 0.6 + 0.9`.
        let bracket = q / mb;

        // 1 + 2. THE HIT AND ITS EXTRA HIT.
        let r = one(&wiki_example());
        assert!((r.sources.direct - q * f).abs() < 1e-6, "hit {}", r.sources.direct);
        assert!(
            (r.sources.extra_hit - 0.26 * (q * f) * f).abs() < 1e-4,
            "extra hit {}",
            r.sources.extra_hit
        );

        // 3 + 4. THE DETONATION AND THE EXTRA HIT OFF IT. Forced, because the
        // example says "if the hit proc'd Blast" — and long enough for the fuse.
        let mut p = wiki_example();
        p.forced_procs = vec![DamageType::Blast];
        p.duration_secs = 30.0;
        let r = one_shot_with_fuse(&p);
        // The detonation is a status payload: 30% of the 100 ModifiedBase,
        // faction squared, and NO elemental bracket.
        let want_det = 0.3 * mb * f * f;
        assert!(
            (r.blast - want_det).abs() < 1e-4,
            "detonation {} wanted {want_det}",
            r.blast
        );
        // …and the extra hit off it takes faction a THIRD time and the whole
        // elemental bracket the detonation itself was denied.
        let want_xh_det = 0.26 * want_det * f * bracket;
        let off_det = r.extra_hit - 0.26 * (q * f) * f;
        assert!(
            (off_det - want_xh_det).abs() < 1e-3,
            "extra hit off the detonation {off_det}, wanted {want_xh_det}"
        );
        // …AND IT IS NEITHER OF THE TWO NUMBERS IT WOULD BE IF EITHER ODDITY
        // WERE MISSING. Both alternatives are what a careful reader would
        // expect — a detonation takes no elemental bonus, so why would the hit
        // off it; and two faction layers is what every other status gets — so
        // ruling them out is the whole of the claim.
        let without_bracket = 0.26 * want_det * f;
        let without_third_faction = 0.26 * want_det * bracket;
        assert!((off_det - without_bracket).abs() > 1.0, "the bracket is missing");
        assert!(
            (off_det - without_third_faction).abs() > 1.0,
            "the third faction layer is missing"
        );
    }

    /// One shot, then the clock run out so the Blast fuse expires — the
    /// detonation and its extra hit are what this returns.
    struct FusedRun {
        blast: f64,
        extra_hit: f64,
    }
    fn one_shot_with_fuse(p: &DummyParams) -> FusedRun {
        let mut q = p.clone();
        // One pull, then nothing but time: `magazine_size` of 1 with a reload
        // longer than the run leaves the fuse alone to expire.
        q.magazine_size = 1.0;
        q.reload_seconds = 1e6;
        let r = run_once(&q, &mut crate::rng::Rng::new(3));
        FusedRun {
            blast: r.sources.status[DamageType::Blast as usize],
            extra_hit: r.sources.extra_hit,
        }
    }

    /// FACTION, TWICE — the whole of the ordinary case. The extra hit is 26% of
    /// a hit that already carried the bonus, and it carries it again:
    /// `0.26 x 1.55 = 0.403` of the hit, against the 0.26 a reading of the card
    /// would predict.
    #[test]
    fn the_extra_hit_takes_the_faction_bonus_a_second_time() {
        let r = run_once(&measured(), &mut crate::rng::Rng::new(3));
        let ratio = r.sources.extra_hit / r.sources.direct;
        assert!(
            (ratio - 0.26 * 1.55).abs() < 1e-9,
            "extra/direct = {ratio:.6}, wanted {:.6}",
            0.26 * 1.55
        );
        // …and it is VOID, whatever the weapon deals: a separate instance, not
        // a share of the vector ("does not dilute weapon elements").
        let by = &r.sources.extra_hit_by_type;
        assert!(by[DamageType::Void as usize] > 0.0);
        assert_eq!(by[DamageType::Impact as usize], 0.0);
        assert_eq!(by[DamageType::Blast as usize], 0.0);
    }

    /// …AND THE BODY PART, TWICE, for the same reason and stated in the same
    /// breath (DE's CN card: "同理，弱点倍率也会被计算两次"). On a 3x head the
    /// hit is tripled once and the extra hit off it is tripled again, so the
    /// RATIO between them moves — which is the only way to see a double dip
    /// without trusting an absolute number.
    #[test]
    fn the_extra_hit_takes_the_body_part_multiplier_a_second_time() {
        let head = |mult: f64| {
            let mut p = measured();
            p.body_parts = vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: mult,
                is_head: true,
                crit_bonus: false,
            }];
            let r = run_once(&p, &mut crate::rng::Rng::new(3));
            r.sources.extra_hit / r.sources.direct
        };
        let body = head(1.0);
        let three_x = head(3.0);
        assert!((three_x / body - 3.0).abs() < 1e-9, "x{:.4}", three_x / body);
    }

    /// THE BLAST CHAIN, decoded number by number against the capture.
    ///
    /// One shot, one forced Blast stack, and 1.5 s later the fuse fires. Three
    /// numbers come out of it and all three are on the video:
    ///
    /// | payload | formula | measured |
    /// | --- | --- | --- |
    /// | the hit | `98 x 2.2 x 1.55` | 334.18 (read as 323 through the target) |
    /// | its extra hit | `x 0.26 x 1.55` | 135 |
    /// | the detonation | `0.3 x 98 x 1.55^2` | 71 |
    /// | ITS extra hit | `x 0.26 x 1.55 x 2.2` | 63 |
    ///
    /// The last row is the one worth having a test for: the faction bonus lands
    /// a THIRD time, and the elemental bracket lands on a payload that is
    /// explicitly denied elemental bonuses.
    #[test]
    fn an_extra_hit_fires_off_a_blast_detonation_at_the_third_faction_layer() {
        let mut p = measured();
        // Exactly one shot, and long enough after it for the 1.5 s fuse.
        p.fire_rate = 0.5;
        p.duration_secs = 1.9;
        p.status_chance = 0.0;
        p.forced_procs = vec![DamageType::Blast];
        let r = run_once(&p, &mut crate::rng::Rng::new(3));

        let hit = 98.0 * 2.2 * 1.55;
        let deto = BLAST_COEFFICIENT * 98.0 * 1.55 * 1.55;
        assert!((r.sources.direct - hit).abs() < 1e-6, "hit {:.3}", r.sources.direct);
        assert!(
            (r.sources.status[DamageType::Blast as usize] - deto).abs() < 1e-6,
            "detonation {:.3} vs {deto:.3}",
            r.sources.status[DamageType::Blast as usize]
        );
        // The hit's own extra hit, plus the detonation's.
        let from_hit = hit * 0.26 * 1.55;
        let from_deto = deto * 0.26 * 1.55 * 2.2;
        assert!(
            (r.sources.extra_hit - (from_hit + from_deto)).abs() < 1e-6,
            "extra {:.3} vs {:.3} + {:.3}",
            r.sources.extra_hit,
            from_hit,
            from_deto
        );
        // 62.6 on a 70.6 detonation: the extra hit off a Blast proc is worth
        // 89% of the proc, which is only possible with both of the layers above.
        assert!((from_deto / deto - 0.887).abs() < 0.002, "{:.4}", from_deto / deto);
    }

    /// NO OTHER STATUS PAYLOAD TRIGGERS ONE — the negative control, and the
    /// reason the Blast case is filed as a bug rather than as a rule. A Slash
    /// bleed ticks six times under the same buff and pays no extra hit at all.
    #[test]
    fn a_dot_tick_triggers_no_extra_hit() {
        let mut p = measured();
        p.damage = DamageVector::new().with(DamageType::Slash, 98.0);
        p.status_chance = 0.0;
        p.forced_procs = vec![DamageType::Slash];
        let r = run_once(&p, &mut crate::rng::Rng::new(3));
        assert!(r.dot_damage > 0.0, "the bleed has to be ticking for this to mean anything");
        // Only the hits paid one, so the ratio is the plain 0.26 x faction —
        // exactly as if the DoT were not there.
        let ratio = r.sources.extra_hit / r.sources.direct;
        assert!((ratio - 0.26 * 1.55).abs() < 1e-9, "{ratio:.6}");
    }

    /// THE VOID PROC IS WORTH A CONDITION OVERLOAD STACK AND NOTHING ELSE. It
    /// deals no damage — a Bullet Attractor is a field, not a payload — so the
    /// only way to see it at all is to put a CO weapon behind it and watch the
    /// counter move.
    #[test]
    fn the_void_proc_pays_condition_overload_and_no_damage() {
        let co = |on: bool| {
            let mut p = measured();
            p.status_chance = if on { 4.0 } else { 0.0 };
            p.base_status_chance = p.status_chance;
            p.co_per_type = 0.8;
            p.co_behavior = crate::loadout::CoBehavior::Independent;
            // Pure Impact: its own proc is a Stagger, which pays no damage
            // either, so any movement in the hit is the CO counter and not a
            // second damage source.
            p.damage = DamageVector::new().with(DamageType::Impact, 98.0);
            let r = run_once(&p, &mut crate::rng::Rng::new(7));
            (r.sources.direct, r.sources.status[DamageType::Void as usize])
        };
        let (quiet, _) = co(false);
        let (loud, void_damage) = co(true);
        assert!(loud > quiet, "CO never moved: {quiet:.0} -> {loud:.0}");
        assert_eq!(void_damage, 0.0, "a Bullet Attractor deals no damage");
    }

    /// AND A FIGHT WITH NO ABILITIES IS THE FIGHT WE ALWAYS HAD. The board
    /// sends none of these, so this is the assertion that the feature costs a
    /// board row nothing.
    #[test]
    fn no_ability_changes_no_number() {
        let bare = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            ..params(&[], 1.0)
        };
        let mut with_empty = bare.clone();
        with_empty.abilities = resolve(&[], 3.0, "");
        assert_eq!(direct(&bare), direct(&with_empty));
    }
}
#[cfg(test)]
mod incarnon_reload_route_tests {
    use super::tests::no_status;
    use super::*;

    /// A hand-built cycle, because the route is about SHELLS and a fixture is
    /// the only way to say how many are missing at the moment of transmuting.
    ///
    /// Base form: 4-round magazine, 1 shot/s, 2 weak-point hits fill the gauge.
    /// So the base form fires twice, transmutes with 2 of 4 loaded, and the
    /// reload that transmute IS loads two shells.
    fn cycle_with(per_shell_perk: bool, base_mag: f64) -> DummyParams {
        let head = vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: true,
            crit_bonus: false,
        }];
        let base_form = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 50.0),
            crit_multiplier: 1.0,
            magazine_size: base_mag,
            reload_seconds: 1.0,
            body_parts: head.clone(),
            ..no_status()
        };
        let mut p = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            crit_multiplier: 1.0,
            magazine_size: 2.0,
            ammo_efficiency_applies: false,
            arcane: ArcaneFx::none(),
            body_parts: head,
            duration_secs: 40.0,
            cycle: Some(IncarnonCycle {
                starts_primed: false,
                base_form: Box::new(base_form),
                charge_on: crate::loadout::ChargeOn::WeakpointHits,
                charges_to_fill: 2,
                transmute_out_seconds: 0.5,
                transmute_seconds: 1.0,
                reload_bucket: 0.0,
            }),
            ..no_status()
        };
        if per_shell_perk {
            // Mounting Momentum's shape: one stack per SHELL loaded, +50% fire
            // rate each, cleared by an empty magazine. Big per-stack so the
            // effect is a shot count rather than a rounding.
            p.stacking_buffs = vec![crate::loadout::StackingBuff {
                id: "per_shell_fire_rate",
                trigger: crate::loadout::BuffTrigger::ReloadComplete,
                grant: crate::loadout::BuffGrant::FireRate,
                per_stack: 0.5,
                max_stacks: 99,
                duration: crate::loadout::NO_TIMEOUT,
                chance: 1.0,
                decay: crate::loadout::BuffDecay::LoseOneAndReset,
                initial_stacks: 0,
                stacks_per_trigger: base_mag as u32,
                per_shell: true,
                cleared_by: crate::loadout::ClearedBy::EmptyMagazine,
            }];
        }
        p
    }

    /// THE GAUGE FILLS ON A SHOT, NOT ON A PELLET — so it OVERSHOOTS, and the
    /// transform lands at the end of the shot that completed it.
    ///
    /// A shotgun puts 7 pellets into a head at once and the gauge wants 30: you
    /// cannot stop at 30, you arrive at 35 on the fifth shot (owner,
    /// 2026-08-10: "如果此时要求命中30个弹头才可以变身，但是我每次是7个弹头，那
    /// 么我肯定要第5次射击的时候才可以变身啊。变身的时机应该是在完成之后射击的
    /// 末尾（也就是下次射击的开头）").
    ///
    /// Both halves are asserted because both could be wrong on their own: the
    /// COUNT (four shots must not be enough at 28 of 30) and the MOMENT (the
    /// fifth shot itself is fired in the BASE form — the transform is paid
    /// after it, not instead of it).
    #[test]
    fn the_gauge_overshoots_and_transforms_at_the_end_of_the_shot() {
        let head = vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: true,
            crit_bonus: false,
        }];
        // 7 pellets a shot, 1 shot/s, gauge 30. Base and Incarnon forms are
        // told apart by their damage so the SHOT COUNT of each is readable
        // from the totals.
        let base_form = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            crit_multiplier: 1.0,
            multishot: 7.0,
            base_multishot: 7.0,
            magazine_size: 1e9,
            fire_rate: 1.0,
            body_parts: head.clone(),
            ..no_status()
        };
        let p = DummyParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            crit_multiplier: 1.0,
            multishot: 1.0,
            base_multishot: 1.0,
            magazine_size: 1.0, // one Incarnon round, so it reverts at once
            ammo_efficiency_applies: false,
            arcane: ArcaneFx::none(),
            body_parts: head,
            fire_rate: 1.0,
            // Long enough for exactly one fill-and-transform, and no more.
            duration_secs: 5.5,
            target: TargetParams { base_health: 1e15, ..DummyParams::default().target },
            cycle: Some(IncarnonCycle {
                starts_primed: false,
                base_form: Box::new(base_form),
                charge_on: crate::loadout::ChargeOn::WeakpointHits,
                charges_to_fill: 30,
                transmute_out_seconds: 0.0,
                transmute_seconds: 0.0,
                reload_bucket: 0.0,
            }),
            ..no_status()
        };
        // Shots land at t = 0,1,2,…, so the clock is cut just after the
        // Incarnon round to make the count readable: `base` 7-pellet shots and
        // then exactly one Incarnon round.
        // The clock is given explicitly per case: shots land at t = 0,1,2,…
        // and the completing shot pays its own interval AND the transform, so
        // the Incarnon round is one interval after the last base shot.
        let run = |gauge: u32, secs: f64| {
            let q = DummyParams {
                duration_secs: secs,
                cycle: Some(IncarnonCycle { charges_to_fill: gauge, ..p.cycle.clone().unwrap() }),
                ..p.clone()
            };
            let r = run_once(&q, &mut Rng::new(9));
            (r.transforms, r.pellets)
        };
        // 30 AT 7 A SHOT: 7,14,21,28,35 — the fourth is SHORT at 28, so the
        // fifth is the one, and the fifth is itself fired in the BASE form.
        assert_eq!(run(30, 5.5), (1, 5 * 7 + 1), "a gauge of 30 needs five 7-pellet shots");
        // …AND FOUR SHOTS ARE NOT ENOUGH. Cut the clock at t = 4.0, before the
        // fifth: 28 of 30, and nothing has transformed. This is the half that
        // fails if the gauge is ever allowed to fill mid-shot.
        assert_eq!(run(30, 4.0).0, 0, "28 of 30 must not transform");
        // A GAUGE THAT DIVIDES EVENLY transforms on the shot that REACHES it,
        // never the one before.
        assert_eq!(run(28, 4.5), (1, 4 * 7 + 1), "28 of 28 is the fourth shot");
        assert_eq!(run(21, 3.5), (1, 3 * 7 + 1), "21 of 21 is the third");
    }

    /// ENTERING THE INCARNON FORM IS A RELOAD, and it pays a reload's stacks.
    ///
    /// The owner's own account (2026-08-08), and the transmute animation being
    /// the weapon's reload time is how you can tell: "假如我现在的shell是13，
    /// 进入的时候是10/13，那么进入的时候会叠加1层，退出的时候会加上其余的层数
    /// （这里是2）。如果是13/13进入的，进入退出都不会叠层". The whole reload runs
    /// across the cycle — one shell going in, the rest coming out — so nothing
    /// here is a rule about transforming: it is a rule about shells.
    ///
    /// This sim transmutes on a GAUGE, so before this the route was worth zero
    /// stacks — on the exact mode the weapon is played in.
    #[test]
    fn the_incarnon_route_pays_the_shells_it_loads() {
        let with = monte_carlo(&cycle_with(true, 4.0), 1, 9);
        let without = monte_carlo(&cycle_with(false, 4.0), 1, 9);
        assert!(with.mean_transforms >= 2.0, "the fixture has to transmute");
        // MORE SHOTS, and only the route can have paid for them: the base form
        // never empties its magazine here, so its own reloads grant nothing.
        assert!(
            with.mean_shots > without.mean_shots,
            "the route is worth nothing: {} vs {}",
            with.mean_shots,
            without.mean_shots
        );
    }

    /// A FULL MAGAZINE PAYS NOTHING — 13/13 in is 13/13 out. This is what keeps
    /// the route a rule about SHELLS rather than a fee for transforming, and it
    /// is the case the owner named first.
    #[test]
    fn transmuting_on_a_full_magazine_grants_no_stacks() {
        // A ONE-ROUND base magazine: the shot that fills the gauge is the shot
        // that empties it, so the reload happens BEFORE the transmute and the
        // transmute finds nothing to load.
        let one = monte_carlo(&cycle_with(true, 1.0), 1, 9);
        let none = monte_carlo(&cycle_with(false, 1.0), 1, 9);
        assert!(one.mean_transforms >= 2.0);
        // The perk still pays for the base form's OWN reloads, which is why
        // this is not an equality — what it must not do is pay twice for one
        // shell. The route's share is zero here and the ordinary reload's is
        // not, so the two runs differ by the ordinary reloads alone.
        assert!(one.mean_shots >= none.mean_shots);
    }

    /// …and the panel keeps the RULE, not just the number it resolves to. "13"
    /// and "one per shell" are the same integer on a 13-shell magazine, and the
    /// route needs to tell them apart.
    #[test]
    fn the_panel_remembers_that_the_perk_counts_shells() {
        let base = crate::loadout::WeaponBase::from_data(
            "felarx",
            true,
            &["felarx_evo1_incarnon_form", "felarx_mounting_momentum"],
        );
        let panel =
            crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
        let mm = panel
            .stacking_buffs
            .iter()
            .find(|b| b.id == "per_shell_fire_rate")
            .expect("the perk is on the panel");
        assert!(mm.per_shell, "it counts shells, and the sim needs to know");
        assert_eq!(
            mm.stacks_per_trigger, panel.magazine_size as u32,
            "one per shell in the modded magazine"
        );
    }
}
#[cfg(test)]
mod fortifier_tick_tests {
    use super::tests::no_status;
    use super::*;
    use crate::damage::DamageVector;

    /// SECONDARY FORTIFIER MULTIPLIES A TICK, ONCE, WHILE THE OVERGUARD HOLDS.
    ///
    /// Three claims in one fixture, because they only mean anything together:
    /// the tick is multiplied at all, it is multiplied ONCE (a faction bonus
    /// would be squared here and this is not one), and it stops the moment the
    /// pool it is about is gone.
    fn bleeder(og: f64, mult: f64) -> DummyParams {
        let mut p = DummyParams {
            damage: DamageVector::new().with(DamageType::Slash, 100.0),
            dot_modified_base: Some(100.0),
            status_chance: 1.0,
            base_status_chance: 1.0,
            fire_rate: 1.0,
            magazine_size: 1e9,
            duration_secs: 12.0,
            base_crit_chance: 0.0,
            unmodded_crit_chance: 0.0,
            body_parts: super::tests::mono_body(1.0),
            ..no_status()
        };
        p.status_chance = 1.0;
        p.base_status_chance = 1.0;
        p.arcane.overguard_mult = mult;
        p.target.base_overguard = og;
        p.target.base_health = 1e15;
        p
    }

    #[test]
    fn the_arcane_multiplies_a_status_tick_exactly_once() {
        let dot = |og: f64, mult: f64| {
            let mut rng = crate::rng::Rng::new(4);
            run_once(&bleeder(og, mult), &mut rng).dot_damage
        };
        // A pool deep enough that it survives the run, so every tick lands on
        // Overguard and the ratio is the multiplier itself.
        let plain = dot(1e15, 1.0);
        assert!(plain > 0.0);
        let buffed = dot(1e15, 8.0);
        let r = buffed / plain;
        assert!((r - 8.0).abs() < 1e-6, "once, not squared: x{r:.4} (64 would be twice)");

        // NO OVERGUARD, NO BONUS — "lost entirely after depleting the Overguard
        // from an enemy". Same build, same seed, a target that never had one.
        assert!((dot(0.0, 8.0) - dot(0.0, 1.0)).abs() < 1e-6);
    }
}
#[cfg(test)]
mod overguard_status_tests {
    use super::tests::{mono_body, no_status};
    use super::*;
    use crate::damage::DamageVector;

    /// A DAMAGING STATUS LANDS ON A FULL OVERGUARD BAR, and its ticks come off
    /// the Overguard rather than waiting for it.
    ///
    /// Owner-confirmed in game (2026-08-09: "可以在敌人身上啊"). Overguard
    /// blocks CROWD CONTROL, not damage — and the difference decides how a DoT
    /// weapon is scored against every Eximus in the roster, because Overguard
    /// carries no armor: while it is up, a tick lands unmitigated on a unit
    /// whose health would keep 10% of it.
    ///
    /// It is also the mechanism behind M39 — Secondary Fortifier coming out
    /// NEGATIVE at low level, because breaking the pool sooner throws that
    /// window away — so if this ever silently flips, that result flips with it
    /// and nothing else would say why.
    #[test]
    fn a_dot_ticks_into_a_full_overguard_bar() {
        let mut p = DummyParams {
            damage: DamageVector::new().with(DamageType::Slash, 100.0),
            dot_modified_base: Some(100.0),
            fire_rate: 1.0,
            magazine_size: 1e9,
            duration_secs: 10.0,
            base_crit_chance: 0.0,
            unmodded_crit_chance: 0.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        p.status_chance = 1.0;
        p.base_status_chance = 1.0;
        // A pool deep enough to survive the run, and ARMOR under it — so a tick
        // that waited for the Overguard would be worth a tenth of one that did
        // not, and the two readings could never be confused.
        p.target.base_overguard = 1e15;
        p.target.base_armor = 2700.0;
        p.target.base_health = 1e15;
        let r = run_once(&p, &mut crate::rng::Rng::new(7));
        assert!(r.procs > 0, "the status has to land at all");
        assert!(r.dot_damage > 0.0, "and its ticks have to do damage");
        // UNMITIGATED: Overguard has no armor, so the tick keeps its full
        // value. At 2700 armor a tick that had landed on health would keep 10%.
        let ticks = r.dot_damage / 35.0; // Slash is 35% of ModifiedBase (100)
        assert!(ticks > 5.0, "ticks came out mitigated: {:.1}", ticks);
    }
}


