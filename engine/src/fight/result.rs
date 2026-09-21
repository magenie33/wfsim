use super::*;

/// Time-bucketed effective damage for the results' DPS-over-time curve. ONE-SECOND buckets (the precision should be
/// 1 s); the array is capacity for the longest supported engagement —
/// callers slice to the actual duration. A wrapper type because a large
/// array has no derived `Default`.
pub const TIMELINE_BUCKETS: usize = 600;

/// EFFECTIVE DAMAGE PER BODY — index 0 is the one being aimed at, 1.. the
/// formation in the order it was handed in.
///
/// A newtype with its own `Default` for the same reason [`Timeline`] is one:
/// `RunResult` is `Copy` and derives `Default`, and an array this long has
/// neither.
#[derive(Debug, Clone, Copy)]
pub struct Timeline(pub [f64; TIMELINE_BUCKETS]);

impl Default for Timeline {
    fn default() -> Self {
        Timeline([0.0; TIMELINE_BUCKETS])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BodyDamage(pub [f64; crate::formation::MAX_BODIES + 1]);

impl Default for BodyDamage {
    fn default() -> Self {
        BodyDamage([0.0; crate::formation::MAX_BODIES + 1])
    }
}

/// Effective damage attributed by SOURCE — the WoW-damage-meter view: direct pellet hits, each status settlement type
/// (Slash bleed, Heat/Toxin/Gas/Electricity DoTs, Blast detonations —
/// keyed by the proc's type), and the on-status arcane instance.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
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
    pub extra_hit_by_type: [f64; DamageType::ALL.len()],
    /// A SYNDICATE RADIAL's explosion (Truth, Justice, …) — its own bucket
    /// because it is neither weapon damage nor a status tick: a flat 1000 of
    /// the syndicate's element, unscaled by anything the build does.
    pub syndicate: f64,
    /// The syndicate blast split by type — one entry in practice, since a
    /// blast is a single element, kept parallel to the other buckets.
    pub syndicate_by_type: [f64; DamageType::ALL.len()],
    /// Indexed by `DamageType as usize` (15 variants).
    pub status: [f64; DamageType::ALL.len()],
    /// The three WEAPON-damage buckets above, each split across the damage
    /// VECTOR that dealt them (same indexing as `status`).
    ///
    /// A status row is already one type — that is what a proc is. A weapon
    /// hit is not: "direct 3.1 G" hides that it was Corrosive and Magnetic in
    /// a 76/24 split, which is the part of a build a player actually tunes.
    pub direct_by_type: [f64; DamageType::ALL.len()],
    pub radial_by_type: [f64; DamageType::ALL.len()],
    pub field_by_type: [f64; DamageType::ALL.len()],
    /// `arcane_on_status` split the same way. Cascadia Empowered's instance
    /// takes the PROC's damage type ("matching the Damage Type of the Status
    /// Effect"), so this row has a vector too — it is just a vector of one
    /// type per instance rather than a mixed one.
    pub arcane_by_type: [f64; DamageType::ALL.len()],
}

impl SourceDamage {
    pub(super) fn add_status(&mut self, t: DamageType, v: f64) {
        self.status[t as usize] += v;
    }
}

/// THE ROW ITSELF — [`ledger::settle`]'s cold half, and NOT generic.
///
/// The split is a cost decision, measured. `settle` is generic over the closure
/// that produces its arguments, so it is stamped out once per damage site, and
/// with the whole body inside that it cost **+2.7%** on `one_fight` — nine
/// copies of a hundred lines that run on one run in a thousand, for the sake of
/// three additions that run on all of them. Keeping the generic part down to
/// the booking and the branch puts the code back where it was.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_row(
    rec: &mut crate::record::Record,
    t: f64,
    body: usize,
    dtype: DamageType,
    kind: PopKind,
    breakdown: &Breakdown,
    settled: Settled,
    debuffs: Option<&DebuffState>,
    inst: Instance,
) {
    let stacks = debuffs.map(|d| d.sample(t)).unwrap_or_default();
    // WHAT THIS INSTANCE SET OFF ON THE SHOOTER — collected by `bump_buffs!`
    // before the row exists, and drained onto it here.
    let triggered = rec.take_triggers();
    // WHAT THE SHOOTER HAD UP, held by the recorder the way the weapon is — see
    // `Record::set_stacks`. Sampling it here would mean threading a dozen run
    // loop locals into nine functions.
    let rec_buffs = rec.stacks().to_vec();
    let subject = Some(body.min(u16::MAX as usize) as u16);
    let one = breakdown.portions().len() == 1;
    for p in breakdown.portions() {
        // THE DEFENSIVE LEDGER, in the order `apply` uses its factors, with
        // the ones that did nothing left out — a `×1.00` from a term this
        // fight never had is noise, and the owner asked for it gone. What is NOT left out is a term that exists and paid
        // nothing; that distinction lives on the page, which knows what the
        // build is carrying and this does not.
        let mut mit: Vec<crate::record::Step> = Vec::with_capacity(6);
        let mut keep = |factor: crate::record::Factor, v: f64| {
            if (v - 1.0).abs() > 1e-12 {
                mit.push((factor, v));
            }
        };
        keep(crate::record::Factor::ShieldGateWindow, breakdown.gate);
        keep(crate::record::Factor::PoolShare, p.share);
        keep(crate::record::Factor::DamageTypeColumn, p.column);
        keep(crate::record::Factor::DisruptAmp, p.disrupt_amp);
        keep(crate::record::Factor::PastTheShield, p.past_shield);
        keep(crate::record::Factor::ShieldGate, p.shield_gate);
        keep(crate::record::Factor::ViralAmp, p.virus_amp);
        keep(crate::record::Factor::Armour, p.armor);
        keep(crate::record::Factor::Attenuation, p.attenuation);
        keep(crate::record::Factor::PoolRanOut, p.pool_remaining);
        rec.push(
            t,
            subject,
            crate::record::Kind::Damage(Box::new(crate::record::Damage {
                origin: inst.origin,
                pellet: inst.pellet,
                radial: inst.radial,
                pool: p.pool,
                // A SINGLE PORTION KEEPS THE CALLER'S TYPE, which is every
                // instance in every fight without shields and therefore every
                // number this engine has ever produced. It matters because a
                // caller sometimes means something the vector does not say — a
                // lingering field reads as `Cinematic` — and only a SPLIT has
                // no honest single answer, so only a split overrides it.
                dtype: if one { dtype } else { p.dtype },
                kind,
                part: inst.part.clone(),
                head: inst.head,
                crit_tier: inst.crit_tier,
                base: inst.base,

                crit_damage: inst.crit_damage,
                layers: inst.layers.clone(),
                // A SPLIT SHARES ONE RAW NUMBER between its portions, and the
                // share is already the first term of the ledger below — so the
                // raw is the instance's either way, and only what happens to it
                // after it arrives differs.
                raw: inst.layers.last().map_or(inst.base, layer_out),
                mitigation: mit,
                effective: p.effective,
                before: breakdown.before,
                debuffs: stacks.clone(),
                buffs: rec_buffs.clone(),
                procs: inst.procs.clone(),
                triggered: triggered.clone(),
                killed: settled.killed,
            })),
        );
    }
}

pub(super) fn add_by_type(
    dst: &mut [f64; DamageType::ALL.len()],
    v: &DamageVector,
    effective: f64,
    col: &crate::data::factions::Column,
) {
    let weighted: f64 = v.iter_nonzero().map(|(t, a)| a * col.get(t)).sum();
    if weighted <= 0.0 {
        return;
    }
    for (t, amount) in v.iter_nonzero() {
        dst[t as usize] += effective * amount * col.get(t) / weighted;
    }
}

impl RunResult {
    /// Raw damage dealt (pre-mitigation), direct hits + DoT ticks.
    #[inline]
    pub fn total_damage(&self) -> f64 {
        self.meter.raw()
    }

    /// Damage after target mitigation (overguard neutrality / armour DR).
    #[inline]
    pub fn effective_damage(&self) -> f64 {
        self.meter.effective()
    }
}

/// Result of a single engagement.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunResult {
    pub shots: u32,      // trigger pulls
    pub pellets: u32,    // multishot instances (>= shots)
    pub crits: u32,      // tier >= 1, counted per pellet
    pub big_crits: u32,  // tier >= 2
    /// What the WIELDER took from their own build — see [`SelfDamage`].
    pub self_damage: SelfDamage,
    /// HOW MANY MOMENTS A BLAST WENT OFF IN — not how many stacks did.
    ///
    /// Nine expiring one at a time are nine; any number expiring together are
    /// one, and a max-stack detonation is none of them because it is fired
    /// where the tenth stack lands rather than by a fuse. It is what an arcane
    /// counting hits sees, so it is counted where that is decided.
    pub blast_pops: u32,
    /// Sum of every DIRECT pellet's crit TIER, normal hits included as 0.
    ///
    /// Its mean is the number `crits / pellets` stops being once a build
    /// passes 100% crit chance: the rate saturates at 1.0 while the tier
    /// keeps climbing, and it is the tier that multiplies the damage
    /// (`crit_multiplier = 1 + tier x (cd - 1)`, uncapped — red is not the top).
    /// Below 100% the two are the same number.
    pub crit_tier_sum: u32,
    pub headshots: u32,  // hits on an `is_head` part
    /// WEAK POINTS THE AIMING DID NOT PICK — a body the round punched through
    /// and entered at the head, or one a bounce arrived at.
    ///
    /// SEPARATE FROM `headshots` because that one is a RATE's numerator, read
    /// against `pellets`, and both count the aimed body alone. The Incarnon
    /// gauge is not a rate: it counts weak-point LANDINGS, so it reads the sum
    /// of the two (MEASUREMENTS M103).
    pub headshots_on_others: u32,
    pub procs: u32,      // status procs applied (all types)
    /// STATUS DoT TICKS THAT PAID — a Heat/Electricity/Toxin/Gas/Slash burn,
    /// counted per tick GROUP (see [`Dot::accumulator_unit`]). A Blast
    /// detonation is not one and does not count.
    ///
    /// NOT derivable from `dot_damage`, which is the trap: that bucket also
    /// holds Blast detonations and area hits, so a fight with tens of
    /// thousands of "DoT damage" and not one burn in it looks identical to a
    /// fight full of burns. `one_fight`'s default build was exactly that
    /// fight for as long as the tool existed, which is why this
    /// counter is now REPORTED rather than only recorded.
    pub dot_ticks: u32,
    /// Lingering-FIELD ticks that landed (Torid's cloud) — its own counter
    /// because a field tick is weapon damage, not a status DoT tick.
    pub field_ticks: u32,
    pub reloads: u32,    // magazine reloads performed
    /// KILLS WHOSE BODY FELL WITHIN REACH of the player — the only ones whose
    /// drop can be collected (`FightParams::drop_is_in_reach`). Equal to
    /// `kills` while the reach is infinite, which is the default.
    pub kills_in_reach: u32,
    /// ROUNDS PICKED UP off the bodies — what the reserve was resupplied by,
    /// after the waste (`rules::ammo::credit` consumes a whole pack for whatever
    /// headroom is left). 0 with an infinite reserve, which is what the rulers
    /// are scored under.
    pub picked_up_ammo: f64,
    pub transforms: u32, // TRANSMUTES into the Incarnon form (reverts don't count)
    /// KILLS THAT LEFT SOMETHING STANDING — `spawn_on_kill`, one per body.
    pub ghost_kills: u32,
    /// The most standing at once — where the duration is visible.
    pub ghosts_peak: u32,
    pub kills: u32,      // InstantRespawn deaths (0 with InfiniteHealth)
    /// See [`Settled::overkill`] — summed over the engagement.
    pub overkill: f64,
    /// See [`Settled::spilled`] — summed over the engagement.
    pub spilled: f64,
    /// See [`Settled::health`] — summed over the engagement.
    pub health_damage: f64,
    /// See [`Settled::virus_stack_health`] — summed over the engagement, and
    /// only ever read as the ratio `virus_stack_health / health_damage`.
    pub virus_stack_health: f64,
    /// See [`Settled::armor_left_health`] — the same, over the same
    /// denominator.
    pub armor_left_health: f64,
    /// …of which this many were taken DIRECTLY by a tendril — see
    /// [`RunResult::note_tendril_kills`]. Those spawn no tendril of their own,
    /// and it is the only kind of kill in this engine that is worth less than
    /// another.
    pub kills_by_tendril: u32,
    /// EFFECTIVE DAMAGE PER BODY — index 0 is the one being aimed at, 1.. are
    /// the formation in the order it was handed in.
    ///
    /// All zero for a single-target fight, which costs nothing to read. It exists
    /// because a formation's TOTAL cannot answer the question a formation
    /// raises: how many bodies a shot actually reaches is a CLAIM about the
    /// weapon — the Ocucor's four tendrils plus its beam are five, and nothing
    /// but a per-body figure can show it is five rather than four or six.
    /// A FIXED ARRAY rather than a `Vec` because `RunResult` is `Copy` and is
    /// copied per run; index 0 is the aimed body and 1.. the formation, so it
    /// is one longer than the cap.
    /// Kills + the depleted fraction of the CURRENT target's total pool
    /// (overguard + health) at engagement end — partial credit so the
    /// objective is not a step function ("draining 80%
    /// of the total pool scores 0.8").
    pub kill_progress: f64,
    /// Effective damage by source (direct / per-proc-type / arcane).
    pub sources: SourceDamage,
    /// SECONDS THE WEAPON WAS NOT FIRING because it was reloading or mid
    /// transform. Burst DPS is the damage over the time that is left, which is
    /// what a room-clear is actually paced by — a weapon that reloads for a
    /// third of the fight has two very different numbers and only one of them
    /// is on the card.
    pub downtime_seconds: f64,
    /// When the FIRST target died. `None` if none did — an honest absence
    /// rather than a zero, which would read as "instantly".
    pub first_kill_at: Option<f64>,
    /// Effective damage dealt before the first reload started: the opening
    /// window, which is what decides whether a room dies before it reacts.
    pub first_magazine_damage: f64,
    /// The single biggest damage INSTANCE of the run — the number people chase.
    pub max_hit: f64,
    /// Effective damage by time bucket (the damage-over-time curve).
    /// The `Rng` state this run STARTED from. SplitMix64 keeps all of its
    /// state in one `u64`, so this is the whole of what it takes to replay
    /// the run bit-for-bit — which is how the MEDIAN engagement gets traced
    /// without every run carrying a trace (see [`Replay`]).
    pub rng_state: u64,
    /// THE RUN'S DAMAGE, and the only way in is [`ledger::settle`].
    ///
    /// Its fields are private to `ledger`, which is the whole point: a damage
    /// site CANNOT book a number without writing the row that explains it,
    /// because the language will not let it reach the field. Two `+=` lines
    /// beside a `log_damage` call are kept in step by whoever remembers, with a
    /// test comparing the sum against the meter as the only guard — a guard
    /// rather than a guarantee.
    pub meter: ledger::Meter,
    /// The DPS-over-time curve — booked by [`ledger::settle`] and by nothing
    /// else, for the same reason the totals are.
    pub curve: ledger::Curve,
    /// Whose damage it was — same door, same reason.
    pub spread: ledger::Spread,
}

/// The one number of a run a metric reads — here because `RunResult` is.
impl RunStat {
    pub fn of(self, r: &RunResult) -> f64 {
        match self {
            RunStat::KillProgress => r.kill_progress,
            RunStat::EffectiveDamage => r.effective_damage(),
        }
    }
}
