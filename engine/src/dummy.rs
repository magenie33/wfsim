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
//!   Proc selection uses the **quantized** vector's shares
//!   (`status::procs_for_hit`); status damage never procs status; a killing
//!   hit's procs are discarded (the respawned target is a fresh individual
//!   with a clean DebuffBar — decision 2026-07-24).
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

/// A buff forced permanently active for the whole run — the "buff lock"
/// simulation setting (e.g. assume 100% Frenzy uptime). Locks are asserted
/// each shot, overriding natural expiry; the perk's own trigger still fires
/// harmlessly alongside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockedBuff {
    Frenzy,
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

    /// Apply one damage instance. Returns `(effective_damage, killed)`.
    ///
    /// Mitigation model (docs/MECHANICS.md §8, unverified):
    /// - Overguard takes raw damage (neutral, ignores armor) and does **not**
    ///   spill excess into health when it breaks.
    /// - Health takes `raw × (1 − 0.9·√(armor/2700))`, floored at 1 per
    ///   damage type — unless the instance ignores armor (Cinematic ticks).
    /// - Overkill damage is lost on death.
    fn apply(&mut self, raw: f64, p: &TargetParams, ignores_armor: bool) -> (f64, bool) {
        if self.overguard > 0.0 {
            if p.mode != TargetMode::InfiniteHealth {
                self.overguard = (self.overguard - raw).max(0.0);
            }
            return (raw, false);
        }

        let dr = if ignores_armor {
            0.0
        } else {
            scaling::armor_damage_reduction(p.armor())
        };
        let effective = if dr > 0.0 {
            (raw * (1.0 - dr)).max(1.0)
        } else {
            raw
        };
        if p.mode == TargetMode::InfiniteHealth {
            return (effective, false);
        }
        self.health -= effective;
        if self.health <= 0.0 {
            *self = TargetState::spawn(p); // instant respawn, no transformation
            (effective, true)
        } else {
            (effective, false)
        }
    }
}

/// A live Bleed instance: the proccing hit's deferred damage (provenance
/// snapshot baked into `value`).
struct BleedInstance {
    next_tick: f64,
    ticks_left: u32,
    value: f64,
}

/// Target-side debuff state, v1 (the procs an IPS vector can produce).
#[derive(Default)]
struct DebuffState {
    /// Stagger stack expiries (uniform 6 s duration → FIFO order == expiry
    /// order). No combat payload on a dummy; tracked for stats.
    stagger: Vec<f64>,
    /// Weakened stack expiries (10 s); each active stack grants +5% flat
    /// crit chance to weapon damage the target receives.
    weakened: Vec<f64>,
    bleeds: Vec<BleedInstance>,
}

const STAGGER_DURATION: f64 = 6.0;
const STAGGER_CAP: usize = 5;
const WEAKENED_DURATION: f64 = 10.0;
const WEAKENED_CAP: usize = 5;
const WEAKENED_FLAT_CC_PER_STACK: f64 = 0.05;
const BLEED_COEFFICIENT: f64 = 0.35;
const BLEED_DELAY: f64 = 1.0;
const BLEED_TICKS: u32 = 6;

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
    /// Buffs forced permanently active regardless of triggers/expiry.
    pub locked_buffs: Vec<LockedBuff>,
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
            // Sim settings (user, 2026-07-26): Frenzy locked at 100% uptime
            // and Fevered Frenzy pre-stacked to 20 (+100% multishot).
            locked_buffs: vec![LockedBuff::Frenzy],
            multishot: 2.0,
            ..Self::default()
        }
    }

    /// Dual Toxocyst **Incarnon Form** (data module: 15 I / 37.5 P / 22.5 S,
    /// 11% crit, 3.0x, 43% status, 4.5 fire rate, full-auto). Whether Frenzy
    /// can trigger while transformed is an open question — off for now.
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
            frenzy: false,
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
            magazine_size: 12.0,
            reload_seconds: 2.35,
            infinite_reserve: true,
            reserve_ammo: 72.0,
            ammo_efficiency_applies: true,
            multishot: 1.0,
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
    pub shots: u32,     // trigger pulls
    pub pellets: u32,   // multishot instances (>= shots)
    pub crits: u32,     // tier >= 1, counted per pellet
    pub big_crits: u32, // tier >= 2
    pub headshots: u32, // hits on an `is_head` part
    pub procs: u32,     // status procs applied (all types)
    pub dot_ticks: u32, // bleed ticks that landed
    pub reloads: u32,   // magazine reloads performed
    pub kills: u32,     // InstantRespawn deaths (0 with InfiniteHealth)
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

/// Process all bleed ticks due strictly before `until` (chronological order).
/// Ticks are Cinematic: they ignore armor, land on whatever pool is active,
/// and — being status damage — never proc anything.
fn process_ticks(
    debuffs: &mut DebuffState,
    until: f64,
    target: &mut TargetState,
    p: &TargetParams,
    r: &mut RunResult,
) {
    loop {
        let Some(idx) = debuffs
            .bleeds
            .iter()
            .enumerate()
            .filter(|(_, b)| b.ticks_left > 0 && b.next_tick < until)
            .min_by(|a, b1| a.1.next_tick.total_cmp(&b1.1.next_tick))
            .map(|(i, _)| i)
        else {
            break;
        };
        let value = debuffs.bleeds[idx].value;
        debuffs.bleeds[idx].next_tick += 1.0;
        debuffs.bleeds[idx].ticks_left -= 1;

        let (effective, killed) = target.apply(value, p, true);
        r.total_damage += value;
        r.effective_damage += effective;
        r.dot_damage += effective;
        r.dot_ticks += 1;
        r.kills += killed as u32;
        if killed {
            // Fresh individual: clean DebuffBar (decision 2026-07-24).
            *debuffs = DebuffState::default();
            break;
        }
    }
    debuffs.bleeds.retain(|b| b.ticks_left > 0);
}

/// Run one engagement with a fresh buff bar and a fresh Secondary Enervate.
pub fn run_once(params: &DummyParams, rng: &mut Rng) -> RunResult {
    let mut bar = BuffBar::new();
    let mut enervate = SecondaryEnervate::default();
    let mut frenzy = Frenzy::new();
    let mut target = TargetState::spawn(&params.target);
    let mut debuffs = DebuffState::default();
    let mut r = RunResult::default();

    // The dealt vector is quantized once (static per run: no dynamic mods
    // yet); ModdedBase for proc payload formulas stays pre-quantization.
    let qvec = params.damage.quantized();
    let qtotal = qvec.total();
    let modded_base = params.damage.total();
    let sd = params.status_duration_mult;

    // Fire while t < duration; the inter-shot interval is 1/(base rate x
    // live BuffBar fire-rate multiplier), evaluated after each shot (a buff
    // expiring mid-interval is approximated to the shot boundary).
    let mut t = 0.0f64;
    let mut magazine = params.magazine_size;
    let mut reserve = params.reserve_ammo;
    loop {
        if t >= params.duration_secs {
            break;
        }

        // Out of magazine: reload (blocking) or, with dry finite reserves,
        // stop firing altogether (DoTs still drain below).
        if magazine < 1e-9 {
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

        // DoT ticks scheduled before this shot land first.
        process_ticks(&mut debuffs, t + 1e-9, &mut target, &params.target, &mut r);

        // Timed buffs (Frenzy) lapse before this shot reads the bar; locked
        // buffs are then re-asserted (locks beat expiry and trigger churn).
        bar.expire(t);
        for lock in &params.locked_buffs {
            match lock {
                LockedBuff::Frenzy => bar.upsert(Frenzy::permanent_buff()),
            }
        }

        // Crit chance: base + Enervate stacks (attacker BuffBar) + Weakened
        // stacks (target DebuffBar: flat crit chance received, weapon direct
        // damage only — which our shots are).
        let contribs = bar.total_contributions();
        // Ammo: consume (1 - efficiency) per shot; Frenzy's +100% efficiency
        // zeroes consumption (unless this magazine is charge-backed).
        let efficiency = if params.ammo_efficiency_applies {
            contribs.ammo_efficiency.clamp(0.0, 1.0)
        } else {
            0.0
        };
        magazine -= 1.0 - efficiency;

        let flat_crit = contribs.flat_crit_chance;
        let weakened_cc = WEAKENED_FLAT_CC_PER_STACK * debuffs.weakened_active(t) as f64;
        let effective_cc = params.base_crit_chance + flat_crit + weakened_cc;

        // Multishot: pellets this pull = floor + fractional chance; every
        // pellet is an independent damage instance.
        let n_pellets =
            params.multishot.floor() as u32 + rng.chance(params.multishot.fract()) as u32;
        let (mut any_head, mut any_big) = (false, false);
        r.shots += 1;

        for _ in 0..n_pellets {
            let tier = roll_crit_tier(effective_cc, rng);
            let part = pick_part(&params.body_parts, rng);
            // Wiki Critical_Hit §Critical Headshots: a crit on an eligible
            // >1x location doubles cd inside the tier formula.
            let cd = if part.crit_bonus && part.multiplier > 1.0 {
                2.0 * params.crit_multiplier
            } else {
                params.crit_multiplier
            };
            let crit_mult = 1.0 + tier as f64 * (cd - 1.0);

            let raw = qtotal * part.multiplier * crit_mult;
            let (effective, killed) = target.apply(raw, &params.target, false);
            r.total_damage += raw;
            r.effective_damage += effective;
            r.kills += killed as u32;
            r.pellets += 1;
            r.crits += (tier >= 1) as u32;
            r.big_crits += (tier >= 2) as u32;
            r.headshots += part.is_head as u32;
            any_head |= part.is_head;
            any_big |= tier >= 2;

            if killed {
                // The killing pellet's procs die with the old individual;
                // remaining pellets hit the fresh spawn.
                debuffs = DebuffState::default();
                continue;
            }
            // Per-pellet proc roll (wiki Multishot/Status_Effect): forced ++
            // SC draws weighted by the QUANTIZED vector, unit immunities
            // renormalized.
            let procs = status::procs_for_hit(
                &params.forced_procs,
                params.status_chance,
                &qvec,
                &params.target.status_immunities,
                rng,
            );
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
                    DamageType::Slash => {
                        // Provenance snapshot: this pellet's crit and part
                        // multipliers, baked into every tick.
                        let ticks =
                            ((1.0 * (BLEED_TICKS as f64 * sd - BLEED_DELAY)).floor() as u32) + 1;
                        debuffs.bleeds.push(BleedInstance {
                            next_tick: t + BLEED_DELAY,
                            ticks_left: ticks,
                            value: BLEED_COEFFICIENT * modded_base * crit_mult * part.multiplier,
                        });
                    }
                    // Other types cannot occur with an IPS vector; their
                    // payloads land in later slices of the status sim.
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
        enervate.on_event(&hit, t, &mut bar);
        if params.frenzy {
            frenzy.on_event(&hit, t, &mut bar);
        }

        // Next shot: cadence reflects the bar as of now (Frenzy just
        // granted/refreshed counts immediately).
        bar.expire(t);
        let rate = params.fire_rate * bar.total_contributions().fire_rate_multiplier;
        t += 1.0 / rate;
    }

    // Drain remaining ticks up to the end of the engagement.
    process_ticks(
        &mut debuffs,
        params.duration_secs,
        &mut target,
        &params.target,
        &mut r,
    );

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
    pub mean_kills: f64,
    pub std_kills: f64,
    pub min_kills: u32,
    pub max_kills: u32,
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
    let (mut min_kills, mut max_kills) = (u32::MAX, 0u32);

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
        kills += r.kills as u64;
        kills_sq += (r.kills as u64) * (r.kills as u64);
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
        mean_kills: kills as f64 / n,
        std_kills: {
            let mean_k = kills as f64 / n;
            (kills_sq as f64 / n - mean_k * mean_k).max(0.0).sqrt()
        },
        min_kills: if min_kills == u32::MAX { 0 } else { min_kills },
        max_kills,
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
            locked_buffs: vec![LockedBuff::Frenzy],
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
