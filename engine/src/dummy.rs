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

/// The equipped secondary arcane. All stacking arcane buffs start FULL
/// (user setting) and then run on their own mechanics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arcane {
    None,
    /// +10% flat crit chance per hit stack; resets after 6 big crits.
    Enervate,
    /// On precision headshot kill: +120% BASE damage per stack (max 3,
    /// 24 s, Galvanized-style one-stack decay); passive +30% headshot
    /// multiplier (data/arcanes/secondary_deadhead.yaml).
    Deadhead,
    /// On applying a Heat status: +12% BASE damage per stack (max 40,
    /// one shared 10 s timer, ALL stacks drop on timeout)
    /// (data/arcanes/cascadia_flare.yaml).
    CascadiaFlare,
}

const DEADHEAD_SPEC: crate::loadout::StackSpec = crate::loadout::StackSpec {
    per_stack: 1.20,
    max_stacks: 3,
    duration: 24.0,
    initial_stacks: 3,
};
const DEADHEAD_HEADSHOT_BONUS: f64 = 0.30;
const FLARE_BD_PER_STACK: f64 = 0.12;
const FLARE_MAX_STACKS: u32 = 40;
const FLARE_DURATION: f64 = 10.0;

/// The real Incarnon combat cycle (user flow, 2026-07-24): the run STARTS
/// with a full gauge in Incarnon Form; when the charge magazine empties,
/// revert (`revert_seconds`), fight in the base form until weakpoint hits
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
    /// Weakpoint hits to fill the gauge (Dual Toxocyst: 9).
    pub charges_to_fill: u32,
    /// Incarnon → base transition (already reload-speed scaled).
    pub revert_seconds: f64,
    /// Base → Incarnon transition (already reload-speed scaled).
    pub transmute_seconds: f64,
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
    pub health_curve: scaling::Curve,
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
    pub mode: TargetMode,
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
            health_curve: scaling::health::UNAFFILIATED,
            steel_path: false,
            eximus: false,
            can_be_eximus: false,
            status_immunities: Vec::new(),
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
struct TargetState {
    overguard: f64,
    health: f64,
}

impl TargetState {
    fn spawn(p: &TargetParams) -> Self {
        if let Err(e) = p.validate() {
            panic!("invalid target: {e}");
        }
        Self {
            overguard: p.overguard(),
            health: p.max_health(),
        }
    }

    /// Apply one damage instance under a live [`Mitigation`] snapshot.
    /// Returns `(effective_damage, killed, overguard_broke)`.
    ///
    /// Mitigation model (docs/MECHANICS.md §8, unverified):
    /// - Overguard takes raw × Disrupt amp (otherwise neutral, ignores
    ///   armor) and does **not** spill excess into health when it breaks.
    /// - Health takes `raw × Virus amp × (1 − 0.9·√(armor_eff/2700))`
    ///   with `armor_eff = armor × strip factors`, floored at 1 per damage
    ///   type — unless the instance ignores armor (Cinematic ticks).
    /// - Overkill damage is lost on death.
    fn apply(
        &mut self,
        raw: f64,
        p: &TargetParams,
        ignores_armor: bool,
        mit: &Mitigation,
    ) -> (f64, bool, bool) {
        if self.overguard > 0.0 {
            let dmg = raw * mit.disrupt_amp;
            if p.mode == TargetMode::InfiniteHealth {
                return (dmg, false, false);
            }
            self.overguard -= dmg;
            if self.overguard <= 0.0 {
                self.overguard = 0.0; // no spill into health
                return (dmg, false, true);
            }
            return (dmg, false, false);
        }

        let dr = if ignores_armor {
            0.0
        } else {
            scaling::armor_damage_reduction(p.armor() * mit.armor_multiplier)
        };
        let boosted = raw * mit.virus_amp;
        let effective = if dr > 0.0 {
            (boosted * (1.0 - dr)).max(1.0)
        } else {
            boosted
        };
        if p.mode == TargetMode::InfiniteHealth {
            return (effective, false, false);
        }
        self.health -= effective;
        if self.health <= 0.0 {
            *self = TargetState::spawn(p); // instant respawn, no transformation
            (effective, true, false)
        } else {
            (effective, false, false)
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
    value: f64,
}

/// A Blast (Detonate) stack: the fuse fires a single-target hit; the 10th
/// stack detonates everything early. The radial is ignored — single-target
/// sim, and the host is excluded from the radial anyway.
struct BlastStack {
    fuse: f64,
    value: f64,
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
    fn apply_cold_proc(&mut self, t: f64, sd: f64, under_overguard: bool) {
        if self.frozen_until.is_some_and(|f| f > t) {
            return; // inert
        }
        self.freeze.retain(|&e| e > t);
        if under_overguard {
            DebuffState::push_capped(
                &mut self.freeze,
                t + STATUS_DURATION * sd,
                FREEZE_CAP_UNDER_OVERGUARD,
                t,
            );
            return;
        }
        if self.freeze.len() >= FREEZE_STACKS_BEFORE_FROZEN {
            self.freeze.clear();
            self.frozen_until = Some(t + FROZEN_DURATION * sd);
        } else {
            self.freeze.push(t + STATUS_DURATION * sd);
        }
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

    /// Prune and compute the live mitigation snapshot for `now`.
    fn mitigation(&mut self, now: f64, sd: f64) -> Mitigation {
        self.prune(now, sd);
        Mitigation {
            disrupt_amp: ten_stack_amp(self.disrupt.len()),
            virus_amp: ten_stack_amp(self.virus.len()),
            armor_multiplier: (1.0 - self.heat_strip(now, sd)) * (1.0 - self.corrosive_strip()),
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
    pub base_crit_chance: f64,
    pub crit_multiplier: f64,
    /// Listed status chance per hit (may exceed 1.0).
    pub status_chance: f64,
    /// Forced procs on every hit (weapon data, per attack part).
    pub forced_procs: Vec<DamageType>,
    /// Status duration multiplier (1.0 = unmodded).
    pub status_duration_mult: f64,
    /// Base fire rate; multiplied live by BuffBar fire-rate multipliers
    /// (Frenzy x2.5) to schedule the next shot.
    pub fire_rate: f64,
    /// Whether the weapon's Frenzy passive is equipped (Dual Toxocyst base
    /// form). Wired: fire-rate x2.5 on true headshots (3 s, refreshable).
    /// NOT yet wired: +100% Toxin injection (needs the element layer) and
    /// ammo efficiency (ammo is infinite here anyway).
    pub frenzy: bool,
    /// Buff-lock settings (see [`LockMode`]).
    pub locked_buffs: Vec<BuffLock>,
    /// The real Incarnon two-form cycle; `None` = single-phase run.
    pub cycle: Option<IncarnonCycle>,
    /// Magazine size; when it runs dry a reload (below) blocks firing.
    pub magazine_size: f64,
    pub reload_seconds: f64,
    /// Default: infinite reserve ammo (decision 2026-07-24). Toggle off to
    /// simulate finite reserves - firing stops when magazine + reserve are
    /// both dry (DoTs keep ticking).
    pub infinite_reserve: bool,
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
    /// Live on-kill CO stacks, live per StackSpec (Emergent policy).
    pub co_stack: Option<crate::loadout::StackSpec>,
    /// Live on-kill multishot stacks, earned from zero.
    pub ms_stack: Option<crate::loadout::StackSpec>,
    /// (1 + status-damage bonuses): scales every status payload value.
    pub status_damage_mult: f64,
    /// (element, 1 + Σ its bonuses) brackets for elemental DoT ticks.
    pub elem_dot_bonus: Vec<(DamageType, f64)>,
    /// ModifiedBase for status-payload formulas (base × (1 + damage mods),
    /// elemental portions excluded). `None` = the vector total (correct
    /// for purely physical vectors).
    pub dot_modified_base: Option<f64>,
    /// The equipped secondary arcane (fixed equipment per scenario; the
    /// optimizer compares scenarios per arcane). Enervate by default for
    /// the historical calibration profiles.
    pub arcane: Arcane,
    pub body_parts: Vec<BodyPart>,
    pub target: TargetParams,
    pub duration_secs: f64,
}

impl DummyParams {
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

    /// Dual Toxocyst base form damage vector (data/weapons/dual_toxocyst.yaml).
    pub fn dual_toxocyst_base_vector() -> DamageVector {
        DamageVector::new()
            .with(DamageType::Impact, 7.5)
            .with(DamageType::Puncture, 60.0)
            .with(DamageType::Slash, 7.5)
    }

    /// Dual Toxocyst **base form** as played: Frenzy passive + the chosen
    /// build (data/builds/dual_toxocyst_default.yaml): Fevered Frenzy's
    /// +50 base damage scales the vector pro-rata (75 -> 125, x5/3) and
    /// Commodore's Fortune sets base crit to 25%. Evolution layers apply
    /// to BOTH guns of the transform group.
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

    /// Build engagement params from a resolved mod loadout (pipeline
    /// [1]+[2] output). Bare-frame scenario: no arcanes, no Frenzy passive
    /// (Incarnon Form), infinite reserve.
    pub fn from_panel(
        panel: &crate::loadout::ResolvedPanel,
        target: TargetParams,
        body_parts: Vec<BodyPart>,
        duration_secs: f64,
    ) -> Self {
        Self {
            damage: panel.damage,
            base_crit_chance: panel.crit_chance,
            crit_multiplier: panel.crit_damage,
            status_chance: panel.status_chance,
            fire_rate: panel.fire_rate,
            frenzy: false,
            magazine_size: panel.magazine_size,
            reload_seconds: panel.reload_seconds,
            ammo_efficiency_applies: false,
            multishot: panel.multishot,
            base_damage_bonus: panel.base_damage_bonus,
            co_per_type: panel.co_per_type,
            co_behavior: panel.co_behavior,
            co_base_fraction: panel.co_base_fraction,
            co_stack: panel.co_stack,
            ms_stack: panel.ms_stack,
            status_damage_mult: panel.status_damage_mult,
            elem_dot_bonus: panel.elem_dot_bonus.clone(),
            dot_modified_base: Some(panel.modified_base),
            arcane: Arcane::None,
            body_parts,
            target,
            duration_secs,
            ..Self::default()
        }
    }

    /// The REAL Incarnon cycle engagement from both forms' resolved panels
    /// (user flow, 2026-07-24): start transformed with a full gauge; dump
    /// the charge magazine; revert; rebuild 9 weakpoint charges in the
    /// base form (Frenzy per `frenzy_lock`); transmute; repeat. Both
    /// transitions scale by the reload formula (M9).
    pub fn incarnon_cycle_from_panels(
        incarnon: &crate::loadout::ResolvedPanel,
        base: &crate::loadout::ResolvedPanel,
        frenzy_lock: LockMode,
        target: TargetParams,
        body_parts: Vec<BodyPart>,
        duration_secs: f64,
    ) -> Self {
        let rl = 1.0 + incarnon.reload_bonus;
        let base_form = DummyParams {
            frenzy: true,
            ammo_efficiency_applies: true,
            ..Self::from_panel(base, target.clone(), body_parts.clone(), duration_secs)
        };
        Self {
            // Frenzy exists in BOTH forms (user-confirmed 2026-07-24).
            frenzy: true,
            locked_buffs: vec![BuffLock {
                buff: LockedBuff::Frenzy,
                mode: frenzy_lock,
            }],
            cycle: Some(IncarnonCycle {
                base_form: Box::new(base_form),
                // Dual Toxocyst gauge: 9 weakpoint charges
                // (data/weapons/dual_toxocyst_incarnon.yaml).
                charges_to_fill: 9,
                revert_seconds: 1.0 / rl,
                transmute_seconds: 2.35 / rl,
            }),
            ..Self::from_panel(incarnon, target, body_parts, duration_secs)
        }
    }

    /// The (1 + element bonuses) bracket for an elemental DoT's ticks.
    fn elem_bracket(&self, t: DamageType) -> f64 {
        self.elem_dot_bonus
            .iter()
            .find(|(x, _)| *x == t)
            .map_or(1.0, |(_, v)| *v)
    }
}

impl Default for DummyParams {
    /// Dual Toxocyst base form + Secondary Enervate, humanoid dummy, 10 s.
    fn default() -> Self {
        Self {
            damage: Self::dual_toxocyst_base_vector(),
            base_crit_chance: 0.05,
            crit_multiplier: 2.0,
            status_chance: 0.37,
            forced_procs: Vec::new(),
            status_duration_mult: 1.0,
            fire_rate: 1.0,
            frenzy: false,
            locked_buffs: Vec::new(),
            cycle: None,
            magazine_size: 12.0,
            reload_seconds: 2.35,
            infinite_reserve: true,
            reserve_ammo: 72.0,
            ammo_efficiency_applies: true,
            multishot: 1.0,
            base_damage_bonus: 0.0,
            co_per_type: 0.0,
            co_behavior: crate::loadout::CoBehavior::AdditiveWithBaseDamage,
            co_base_fraction: 1.0,
            co_stack: None,
            ms_stack: None,
            status_damage_mult: 1.0,
            elem_dot_bonus: Vec::new(),
            dot_modified_base: None,
            arcane: Arcane::Enervate,
            body_parts: Self::humanoid_parts(),
            target: TargetParams::training_dummy(),
            duration_secs: 10.0,
        }
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
    pub headshots: u32,  // hits on an `is_head` part
    pub procs: u32,      // status procs applied (all types)
    pub dot_ticks: u32,  // bleed ticks that landed
    pub reloads: u32,    // magazine reloads performed
    pub transforms: u32, // Incarnon cycle transitions (each direction counts)
    pub kills: u32,      // InstantRespawn deaths (0 with InfiniteHealth)
    /// Kills + the depleted fraction of the CURRENT target's total pool
    /// (overguard + health) at engagement end — partial credit so the
    /// objective is not a step function (user, 2026-07-24: "打空了80%
    /// 总血条算0.8分").
    pub kill_progress: f64,
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

/// Live on-kill stack state (Galvanized graceful decay: on timeout lose
/// ONE stack and reset the duration for the remainder).
#[derive(Default)]
struct LiveStacks {
    stacks: u32,
    expiry: f64,
}

impl LiveStacks {
    /// Apply pending decay and return the current stack count.
    fn current(&mut self, now: f64, duration: f64) -> u32 {
        while self.stacks > 0 && self.expiry <= now {
            self.stacks -= 1;
            self.expiry += duration;
        }
        self.stacks
    }

    fn on_kill(&mut self, now: f64, spec: &crate::loadout::StackSpec) {
        self.current(now, spec.duration);
        self.stacks = (self.stacks + 1).min(spec.max_stacks);
        self.expiry = now + spec.duration;
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
/// shields/overguard with Disrupt active fires a forced Tesla Chain
/// instance totalling 3% of the max pool per Magnetic stack (cap 30%),
/// over 6 ticks; status-damage mods apply TWICE; base-damage mods never.
fn push_overguard_break_proc(debuffs: &mut DebuffState, params: &DummyParams, now: f64) {
    let stacks = debuffs.disrupt.len();
    if stacks == 0 {
        return;
    }
    let frac = (0.03 * stacks as f64).min(0.30);
    let total = frac * params.target.overguard() * params.status_damage_mult.powi(2);
    debuffs.dots.push(Dot {
        next_tick: now,
        ticks_left: 6,
        value: total / 6.0,
        dtype: DamageType::Electricity,
        ignores_armor: false,
    });
}

/// Timed status events due strictly before `until`, in chronological
/// order: DoT ticks (Bleed/Toxin/Electricity/Gas + break-proc Tesla), the
/// Heat singleton's anchored ticks, and Blast fuse expiries. Mitigation is
/// evaluated LIVE at each event (the snapshot boundary rule); status
/// damage never procs status.
fn process_ticks(
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    until: f64,
    target: &mut TargetState,
    params: &DummyParams,
    r: &mut RunResult,
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

        let mit = debuffs.mitigation(now, sd);
        let (value, ignores_armor, is_dot_tick) = match &ev {
            Ev::Dot(i) => {
                let d = &mut debuffs.dots[*i];
                d.next_tick += 1.0;
                d.ticks_left -= 1;
                let d = &debuffs.dots[*i];
                (d.value, d.ignores_armor, true)
            }
            Ev::Heat => {
                let h = debuffs.heat.as_mut().expect("heat event needs entity");
                h.next_tick += 1.0;
                (h.value, false, true)
            }
            Ev::Blast(i) => (debuffs.blast.remove(*i).value, false, false),
        };

        let (effective, killed, broke) = target.apply(value, p, ignores_armor, &mit);
        r.total_damage += value;
        r.effective_damage += effective;
        r.dot_damage += effective;
        r.dot_ticks += is_dot_tick as u32;
        r.kills += killed as u32;
        if broke {
            push_overguard_break_proc(debuffs, params, now);
        }
        if killed {
            // Status-proc kills grant Galvanized stacks too (GS rules).
            gal.bump_on_kill(params, now);
            // Fresh individual: clean DebuffBar (decision 2026-07-24).
            *debuffs = DebuffState::default();
            break;
        }
    }
    debuffs.dots.retain(|d| d.ticks_left > 0);
}

/// Run one engagement with a fresh buff bar and a fresh Secondary Enervate.
pub fn run_once(params: &DummyParams, rng: &mut Rng) -> RunResult {
    let mut bar = BuffBar::new();
    let mut enervate = SecondaryEnervate::default();
    let mut frenzy = Frenzy::new();
    let mut target = TargetState::spawn(&params.target);
    let mut debuffs = DebuffState::default();
    // On-kill stack buffs start at their configured initial stacks (full
    // per the user's setting) with a fresh duration from t = 0.
    let mut gal = GalStacks::default();
    if let Some(s) = &params.co_stack {
        gal.co = LiveStacks {
            stacks: s.initial_stacks.min(s.max_stacks),
            expiry: s.duration,
        };
    }
    if let Some(s) = &params.ms_stack {
        gal.ms = LiveStacks {
            stacks: s.initial_stacks.min(s.max_stacks),
            expiry: s.duration,
        };
    }
    // Stacking arcanes start FULL (user setting) with a fresh timer.
    let mut deadhead = LiveStacks {
        stacks: DEADHEAD_SPEC.initial_stacks,
        expiry: DEADHEAD_SPEC.duration,
    };
    let mut flare_stacks: u32 = FLARE_MAX_STACKS;
    let mut flare_expiry: f64 = FLARE_DURATION;

    let mut r = RunResult::default();

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
    let sdm = params.status_damage_mult;

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
    let mut magazine = params.magazine_size;
    let mut reserve = params.reserve_ammo;
    // Incarnon cycle state: the run STARTS transformed with a full gauge.
    let mut in_base_form = false;
    let mut charges = 0u32;
    let mut base_mag = params
        .cycle
        .as_ref()
        .map_or(0.0, |c| c.base_form.magazine_size);
    loop {
        if t >= params.duration_secs {
            break;
        }

        // Phase transitions and reloads.
        if let Some(cy) = &params.cycle {
            if !in_base_form && magazine < 1e-9 {
                // Charge magazine spent: revert to the base form. The swap
                // fully reloads the base magazine (wiki side effect).
                t += cy.revert_seconds;
                r.transforms += 1;
                in_base_form = true;
                charges = 0;
                base_mag = cy.base_form.magazine_size;
                continue;
            }
            if in_base_form && base_mag < 1e-9 {
                // Base-form reload (infinite reserve assumed in the cycle).
                t += cy.base_form.reload_seconds;
                r.reloads += 1;
                base_mag = cy.base_form.magazine_size;
                continue;
            }
        } else if magazine < 1e-9 {
            // Out of magazine: reload (blocking) or, with dry finite
            // reserves, stop firing altogether (DoTs still drain below).
            if !params.infinite_reserve && reserve < 1e-9 {
                break;
            }
            t += params.reload_seconds;
            r.reloads += 1;
            let refill = if params.infinite_reserve {
                params.magazine_size
            } else {
                let take = params.magazine_size.min(reserve);
                reserve -= take;
                take
            };
            magazine = refill;
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
        let (qvec, qtotal, modded_base) = if in_base_form {
            let p = base_pre.as_ref().expect("cycle state needs base pre");
            (&p.0, p.1, p.2)
        } else {
            (&main_pre.0, main_pre.1, main_pre.2)
        };

        // Status events scheduled before this shot land first.
        process_ticks(
            &mut debuffs,
            &mut gal,
            t + 1e-9,
            &mut target,
            params,
            &mut r,
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
        let efficiency = if ap.ammo_efficiency_applies {
            contribs.ammo_efficiency.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if in_base_form {
            base_mag -= 1.0 - efficiency;
        } else {
            magazine -= 1.0 - efficiency;
        }

        let flat_crit = contribs.flat_crit_chance;
        let weakened_cc = WEAKENED_FLAT_CC_PER_STACK * debuffs.weakened_active(t) as f64;
        let effective_cc = ap.base_crit_chance + flat_crit + weakened_cc;

        // Multishot: pellets this pull = floor + fractional chance; every
        // pellet is an independent damage instance. Earned Galvanized
        // stacks add live.
        let ms_eff = ap.multishot
            + params
                .ms_stack
                .as_ref()
                .map_or(0.0, |s| s.per_stack * gal.ms.current(t, s.duration) as f64);
        let n_pellets = ms_eff.floor() as u32 + rng.chance(ms_eff.fract()) as u32;
        let (mut any_head, mut any_big) = (false, false);
        let headshots_before = r.headshots;
        r.shots += 1;

        for _ in 0..n_pellets {
            // Live target-side state for THIS pellet (earlier pellets'
            // procs already count): mitigation amps, Cold's flat crit
            // damage received, and Condition Overload's type count.
            let mit = debuffs.mitigation(t, sd);
            let cd_total = ap.crit_multiplier + debuffs.cold_cd_bonus(t);
            // Live arcane BASE-DAMAGE stacks (Deadhead/Cascadia Flare join
            // the Hornet Strike bucket, so they also scale ModifiedBase).
            let arc_bd = match params.arcane {
                Arcane::Deadhead => {
                    DEADHEAD_SPEC.per_stack * deadhead.current(t, DEADHEAD_SPEC.duration) as f64
                }
                Arcane::CascadiaFlare => {
                    if t >= flare_expiry {
                        flare_stacks = 0; // hard reset: ALL stacks drop
                    }
                    FLARE_BD_PER_STACK * flare_stacks as f64
                }
                _ => 0.0,
            };
            let bd = ap.base_damage_bonus;
            let arc_ratio = (1.0 + bd + arc_bd) / (1.0 + bd);
            let mb_live = modded_base * arc_ratio;
            // CO per this weapon's behavior class (direct hits only):
            // (static + earned stacks) × base-effectiveness × types.
            let co_rate = ap.co_per_type
                + params
                    .co_stack
                    .as_ref()
                    .map_or(0.0, |s| s.per_stack * gal.co.current(t, s.duration) as f64);
            let co_total = co_rate * ap.co_base_fraction * debuffs.distinct_statuses() as f64;
            let co_mult = match ap.co_behavior {
                // Joins the base-damage bucket: diluted by Hornet Strike,
                // sharing the bracket with the arcane's bonus.
                crate::loadout::CoBehavior::AdditiveWithBaseDamage => {
                    (1.0 + bd + arc_bd + co_total) / (1.0 + bd)
                }
                crate::loadout::CoBehavior::Independent => arc_ratio * (1.0 + co_total),
                crate::loadout::CoBehavior::Inert => arc_ratio,
            };

            let tier = roll_crit_tier(effective_cc, rng);
            let part = pick_part(&params.body_parts, rng);
            // Deadhead's rank-5 passive: +30% headshot multiplier on true
            // heads (additive with Prowl-type bonuses; rides the part
            // context into DoT snapshots).
            let head_bonus = if part.is_head && params.arcane == Arcane::Deadhead {
                1.0 + DEADHEAD_HEADSHOT_BONUS
            } else {
                1.0
            };
            let part_factor = part.multiplier * head_bonus;
            // Wiki Critical_Hit §Critical Headshots: a crit on an eligible
            // >1x location doubles cd inside the tier formula (a cd_total
            // that INCLUDES Cold's flat bonus — freeze.yaml notes).
            let cd = if part.crit_bonus && part.multiplier > 1.0 {
                2.0 * cd_total
            } else {
                cd_total
            };
            let crit_mult = 1.0 + tier as f64 * (cd - 1.0);

            let raw = qtotal * part_factor * crit_mult * co_mult;
            let (effective, killed, broke) = target.apply(raw, &params.target, false, &mit);
            r.total_damage += raw;
            r.effective_damage += effective;
            r.kills += killed as u32;
            r.pellets += 1;
            r.crits += (tier >= 1) as u32;
            r.big_crits += (tier >= 2) as u32;
            r.headshots += part.is_head as u32;
            any_head |= part.is_head;
            any_big |= tier >= 2;

            if broke {
                push_overguard_break_proc(&mut debuffs, params, t);
            }
            if killed {
                gal.bump_on_kill(params, t);
                // Deadhead: only HEADSHOT kills grant/refresh its stacks.
                if part.is_head && params.arcane == Arcane::Deadhead {
                    deadhead.on_kill(t, &DEADHEAD_SPEC);
                }
                // The killing pellet's procs die with the old individual;
                // remaining pellets hit the fresh spawn.
                debuffs = DebuffState::default();
                continue;
            }
            // Per-pellet proc roll (wiki Multishot/Status_Effect): forced ++
            // SC draws weighted by the QUANTIZED vector, unit immunities
            // renormalized.
            let procs = status::procs_for_hit(
                &ap.forced_procs,
                ap.status_chance,
                qvec,
                &params.target.status_immunities,
                rng,
            );
            // Elemental DoT tick (data/debuffs): 0.5 × ModifiedBase ×
            // (1 + element bonuses) × (1 + status damage) × crit/part
            // snapshot. Delay-1 DoTs tick at +1..+6 s; delay-0 (Electricity/
            // Gas) at 0..+5 s (the +6 s event is a dud).
            let delayed_ticks = ((BLEED_TICKS as f64 * sd - BLEED_DELAY).floor() as u32) + 1;
            let immediate_ticks = ((BLEED_TICKS as f64 * sd).floor() as u32).max(1);
            let push_dot = |debuffs: &mut DebuffState,
                            dtype: DamageType,
                            coeff: f64,
                            bracket: f64,
                            delay: f64,
                            ticks: u32,
                            ignores_armor: bool| {
                debuffs.dots.push(Dot {
                    next_tick: t + delay,
                    ticks_left: ticks,
                    value: coeff * mb_live * bracket * sdm * crit_mult * part_factor,
                    dtype,
                    ignores_armor,
                });
            };
            for proc in procs {
                r.procs += 1;
                match proc {
                    DamageType::Impact => DebuffState::push_capped(
                        &mut debuffs.stagger,
                        t + STAGGER_DURATION * sd,
                        STAGGER_CAP,
                        t,
                    ),
                    DamageType::Puncture => DebuffState::push_capped(
                        &mut debuffs.weakened,
                        t + WEAKENED_DURATION * sd,
                        WEAKENED_CAP,
                        t,
                    ),
                    DamageType::Slash => push_dot(
                        &mut debuffs,
                        DamageType::Slash,
                        BLEED_COEFFICIENT,
                        1.0, // Bleed: elemental mods never scale the ticks
                        BLEED_DELAY,
                        delayed_ticks,
                        true, // Cinematic: ignores armor
                    ),
                    DamageType::Toxin => push_dot(
                        &mut debuffs,
                        DamageType::Toxin,
                        DOT_COEFFICIENT,
                        ap.elem_bracket(DamageType::Toxin),
                        1.0,
                        delayed_ticks,
                        false,
                    ),
                    DamageType::Electricity => push_dot(
                        &mut debuffs,
                        DamageType::Electricity,
                        DOT_COEFFICIENT,
                        ap.elem_bracket(DamageType::Electricity),
                        0.0,
                        immediate_ticks,
                        false,
                    ),
                    DamageType::Gas => push_dot(
                        &mut debuffs,
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
                        // one stack and refreshes the shared timer.
                        if params.arcane == Arcane::CascadiaFlare {
                            if t >= flare_expiry {
                                flare_stacks = 0;
                            }
                            flare_stacks = (flare_stacks + 1).min(FLARE_MAX_STACKS);
                            flare_expiry = t + FLARE_DURATION;
                        }
                        let contrib = DOT_COEFFICIENT
                            * mb_live
                            * ap.elem_bracket(DamageType::Heat)
                            * sdm
                            * crit_mult
                            * part_factor;
                        let expiry = t + STATUS_DURATION * sd;
                        match &mut debuffs.heat {
                            Some(h) => {
                                h.value += contrib;
                                h.expiry = expiry;
                            }
                            None => {
                                debuffs.heat = Some(HeatEntity {
                                    born: t,
                                    expiry,
                                    next_tick: t + 1.0,
                                    value: contrib,
                                })
                            }
                        }
                    }
                    DamageType::Cold => {
                        debuffs.apply_cold_proc(t, sd, target.overguard > 0.0);
                    }
                    DamageType::Magnetic => DebuffState::push_capped(
                        &mut debuffs.disrupt,
                        t + STATUS_DURATION * sd,
                        TEN_STACK_CAP,
                        t,
                    ),
                    DamageType::Viral => DebuffState::push_capped(
                        &mut debuffs.virus,
                        t + STATUS_DURATION * sd,
                        TEN_STACK_CAP,
                        t,
                    ),
                    DamageType::Corrosive => DebuffState::push_capped(
                        &mut debuffs.corrosion,
                        t + CORROSION_DURATION * sd,
                        TEN_STACK_CAP,
                        t,
                    ),
                    DamageType::Radiation => DebuffState::push_capped(
                        &mut debuffs.confusion,
                        t + STATUS_DURATION * sd,
                        TEN_STACK_CAP,
                        t,
                    ),
                    DamageType::Blast => {
                        debuffs.blast.push(BlastStack {
                            fuse: t + BLAST_FUSE * sd,
                            value: BLAST_COEFFICIENT * mb_live * sdm * crit_mult * part_factor,
                        });
                        if debuffs.blast.len() >= TEN_STACK_CAP {
                            // Early detonation: every stack's single-target
                            // hit at once, all stacks consumed (radial
                            // excluded — it never hits the host).
                            let total: f64 = debuffs.blast.drain(..).map(|b| b.value).sum();
                            let mit = debuffs.mitigation(t, sd);
                            let (eff, killed, broke) =
                                target.apply(total, &params.target, false, &mit);
                            r.total_damage += total;
                            r.effective_damage += eff;
                            r.dot_damage += eff;
                            r.kills += killed as u32;
                            if broke {
                                push_overguard_break_proc(&mut debuffs, params, t);
                            }
                            if killed {
                                gal.bump_on_kill(params, t);
                                debuffs = DebuffState::default();
                            }
                        }
                    }
                    _ => {}
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
        if params.arcane == Arcane::Enervate {
            enervate.on_event(&hit, t, &mut bar);
        }
        if ap.frenzy {
            frenzy.on_event(&hit, t, &mut bar);
        }

        // Gauge charging (base phase): every weakpoint PELLET builds one
        // charge (charge_rules); a full gauge transmutes back immediately.
        if let Some(cy) = &params.cycle {
            if in_base_form {
                charges += r.headshots - headshots_before;
                if charges >= cy.charges_to_fill {
                    t += cy.transmute_seconds;
                    r.transforms += 1;
                    in_base_form = false;
                    magazine = params.magazine_size; // full gauge = full charge mag
                    base_mag = cy.base_form.magazine_size; // swap reloads it
                                                           // Frenzy persists across the transform (user-confirmed
                                                           // 2026-07-24: it exists in both forms).
                    continue;
                }
            }
        }

        // Next shot: cadence reflects the bar as of now (Frenzy just
        // granted/refreshed counts immediately).
        bar.expire(t);
        let rate = ap.fire_rate * bar.total_contributions().fire_rate_multiplier;
        t += 1.0 / rate;
    }

    // Drain remaining status events up to the end of the engagement.
    process_ticks(
        &mut debuffs,
        &mut gal,
        params.duration_secs,
        &mut target,
        params,
        &mut r,
    );

    // Partial credit: the fraction of the current individual's total
    // pool already depleted (InfiniteHealth pools never deplete -> 0).
    let pool = params.target.overguard() + params.target.max_health();
    let partial = if pool > 0.0 {
        (1.0 - (target.overguard + target.health) / pool).clamp(0.0, 1.0)
    } else {
        0.0
    };
    r.kill_progress = r.kills as f64 + partial;

    r
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
    pub effective_dps: f64,
    pub mean_dot_damage: f64,
    pub mean_procs: f64,
    pub mean_reloads: f64,
    pub mean_transforms: f64,
    pub mean_kills: f64,
    pub std_kills: f64,
    pub min_kills: u32,
    pub max_kills: u32,
    /// Mean kill score with partial credit (kills + depleted fraction of
    /// the final target's pool).
    pub mean_kill_progress: f64,
    pub mean_shots: f64,
    pub mean_pellets: f64,
    pub mean_crit_rate: f64,
    pub mean_big_crit_rate: f64,
    pub mean_headshot_rate: f64,
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
    let (mut effective, mut kills, mut kills_sq) = (0.0f64, 0u64, 0u64);
    let (mut dot, mut procs, mut reloads) = (0.0f64, 0u64, 0u64);
    let mut transforms = 0u64;
    let (mut min_kills, mut max_kills) = (u32::MAX, 0u32);
    let mut kill_progress = 0.0f64;

    for _ in 0..runs {
        let r = run_once(params, &mut rng);
        sum += r.total_damage;
        sum_sq += r.total_damage * r.total_damage;
        min = min.min(r.total_damage);
        max = max.max(r.total_damage);
        effective += r.effective_damage;
        dot += r.dot_damage;
        procs += r.procs as u64;
        reloads += r.reloads as u64;
        transforms += r.transforms as u64;
        kills += r.kills as u64;
        kills_sq += (r.kills as u64) * (r.kills as u64);
        kill_progress += r.kill_progress;
        min_kills = min_kills.min(r.kills);
        max_kills = max_kills.max(r.kills);
        shots += r.shots as u64;
        pellets += r.pellets as u64;
        crits += r.crits as u64;
        big_crits += r.big_crits as u64;
        headshots += r.headshots as u64;
    }

    let n = runs.max(1) as f64;
    let mean = sum / n;
    let variance = (sum_sq / n - mean * mean).max(0.0);
    let total_pellets = pellets.max(1) as f64;

    Summary {
        runs,
        duration_secs: params.duration_secs,
        mean_damage: mean,
        dps: mean / params.duration_secs,
        std_damage: variance.sqrt(),
        min_damage: if min.is_finite() { min } else { 0.0 },
        max_damage: if max.is_finite() { max } else { 0.0 },
        mean_effective_damage: effective / n,
        effective_dps: effective / n / params.duration_secs,
        mean_dot_damage: dot / n,
        mean_procs: procs as f64 / n,
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
        mean_shots: shots as f64 / n,
        mean_pellets: pellets as f64 / n,
        mean_crit_rate: crits as f64 / total_pellets,
        mean_big_crit_rate: big_crits as f64 / total_pellets,
        mean_headshot_rate: headshots as f64 / total_pellets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Default params with status disabled — for hand-computed expectations
    /// that predate the status sim.
    fn no_status() -> DummyParams {
        DummyParams {
            status_chance: 0.0,
            ..DummyParams::default()
        }
    }

    fn single_part(part: BodyPart) -> DummyParams {
        DummyParams {
            body_parts: vec![part],
            ..no_status()
        }
    }

    fn mono_body(multiplier: f64) -> Vec<BodyPart> {
        vec![BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier,
            is_head: false,
            crit_bonus: false,
        }]
    }

    #[test]
    fn ten_shots_in_ten_seconds_at_one_per_second() {
        let s = monte_carlo(&DummyParams::default(), 100, 1);
        assert!((s.mean_shots - 10.0).abs() < 1e-9);
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

    fn frail_target(mode: TargetMode, armor: f64, overguard: f64) -> TargetParams {
        TargetParams {
            name: "test target".into(),
            base_level: 1,
            level: 1,
            base_health: 50.0, // below the weakest possible shot (75)
            base_armor: armor,
            base_overguard: overguard,
            health_curve: crate::scaling::health::UNAFFILIATED,
            steel_path: false,
            eximus: false,
            can_be_eximus: false,
            status_immunities: Vec::new(),
            mode,
        }
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
            arcane: Arcane::None,
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
            arcane: Arcane::None,
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

    #[test]
    fn incarnon_cycle_alternates_forms_deterministically() {
        // Incarnon: 100 dmg, mag 2, 1/s. Base: 50 dmg, aim 100% head
        // (each pellet charges), 2 charges to fill, revert 0.5 s,
        // transmute 1.0 s. Timeline over 10 s:
        //   inc @0,1 | revert 2->2.5 | base @2.5,3.5 -> transmute ->4.5
        //   inc @4.5,5.5 | revert 6.5->7 | base @7,8 -> transmute ->9
        //   inc @9. Totals: 5x100 + 4x50 = 700; 9 shots; 4 transforms.
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
            arcane: Arcane::None,
            body_parts: head,
            cycle: Some(IncarnonCycle {
                base_form: Box::new(base_form),
                charges_to_fill: 2,
                revert_seconds: 0.5,
                transmute_seconds: 1.0,
            }),
            ..no_status()
        };
        let s = monte_carlo(&p, 5, 9);
        assert!(
            (s.mean_damage - 700.0).abs() < 1e-9,
            "dmg {}",
            s.mean_damage
        );
        assert!((s.mean_shots - 9.0).abs() < 1e-9, "shots {}", s.mean_shots);
        assert!((s.mean_transforms - 4.0).abs() < 1e-9);
        assert_eq!(s.mean_reloads, 0.0);
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
            arcane: Arcane::None,
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
        // 4.6 (bd 0); +30% headshot multiplier on the 1x head part.
        // 10 shots × 75 × 1.3 × 4.6 = 4485. No kills, 24 s > run: no decay.
        let p = DummyParams {
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: Arcane::Deadhead,
            body_parts: vec![BodyPart {
                name: "head".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: true,
                crit_bonus: false,
            }],
            ..no_status()
        };
        let s = monte_carlo(&p, 20, 5);
        assert!(
            (s.mean_damage - 4485.0).abs() < 1e-9,
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
            arcane: Arcane::CascadiaFlare,
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
            d.apply_cold_proc(k as f64 * 0.1, 1.0, false);
        }
        assert_eq!(d.freeze.len(), 9);
        assert!((d.cold_cd_bonus(0.9) - 0.50).abs() < 1e-9); // 0.10+0.05×8
                                                             // The 10th proc CONSUMES the stacks and enters Frozen (3 s).
        d.apply_cold_proc(1.0, 1.0, false);
        assert!(d.freeze.is_empty());
        assert_eq!(d.frozen_until, Some(4.0));
        assert!((d.cold_cd_bonus(1.5) - 1.00).abs() < 1e-9); // supersedes
                                                             // Cold procs are inert while Frozen.
        d.apply_cold_proc(2.0, 1.0, false);
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
            d.apply_cold_proc(k as f64 * 0.1, 1.0, true);
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
}
