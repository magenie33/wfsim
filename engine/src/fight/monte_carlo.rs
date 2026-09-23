use super::*;

/// Aggregate statistics over many engagements.
#[derive(Debug, Clone, Copy)]
pub struct Summary {
    /// MEAN EFFECTIVE DAMAGE PER BODY, index for index with
    /// [`RunResult::damage_by_body`] — 0 is the aimed one.
    ///
    /// Aggregated because a per-BODY figure is the only thing that can say a
    /// crowd was reached rather than a big number produced:
    /// five bodies and six bodies can deal the same total, and only one of them
    /// is the weapon.
    pub mean_damage_by_body: BodyDamage,
    /// …and the same total cut by WHO DEALT IT, seat for seat with
    /// [`FightParams::combatant_ids`]. Mean over the runs, like everything else
    /// here.
    pub mean_damage_by_combatant: CombatantDamage,
    pub runs: u32,
    pub duration_seconds: f64,
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
    /// What each run threw away — see [`RunResult::overkill`] and
    /// [`RunResult::spilled`] — averaged, so the waste rate is a mean too.
    pub mean_overkill: f64,
    pub mean_spilled: f64,
    /// MEAN DAMAGE PER RUN THAT REACHED HEALTH — see [`Settled::health`]. It
    /// is `mean_virus_stacks`'s denominator, and it is reported because the
    /// ratio alone cannot say whether a fight spent itself on health at all:
    /// a build held off by overguard reports the same Viral average as one
    /// that never met any.
    pub mean_health_damage: f64,
    /// THE AVERAGE VIRAL PILE EVERY POINT OF HEALTH DAMAGE WAS DEALT THROUGH,
    /// over every body and every run — see [`Settled::virus_stack_health`].
    ///
    /// Weighted by that damage rather than by the hit, because it exists to
    /// explain a damage total: a DoT tick on a body carrying ten stacks and a
    /// charged shot on one carrying none are not one vote each. It is the
    /// figure the replay's eight followed bodies cannot give — that is one
    /// engagement and a sample of the crowd, and this is all of both.
    pub mean_virus_stacks: f64,
    /// THE SHARE OF THE TARGET'S ARMOUR STILL STANDING when a point of health
    /// damage arrived, over every body and every run — the companion to
    /// `mean_virus_stacks` and the other factor the health path reads.
    ///
    /// It answers what a stack count cannot: a build whose Corrosive piles to
    /// ten AFTER the kill has stripped nothing, and a build that never carries
    /// it reads 1.0 rather than reading nothing.
    pub mean_armor_left: f64,
    pub mean_procs: f64,
    /// Mean lingering-FIELD ticks that landed (Torid's cloud).
    pub mean_field_ticks: f64,
    /// Mean STATUS DoT ticks that paid — see [`RunResult::dot_ticks`]. Reported
    /// because `mean_dot_damage` is not a proxy for it: a fight can carry a
    /// large one and no burn at all.
    pub mean_dot_ticks: f64,
    pub mean_reloads: f64,
    /// MEAN ROUNDS PICKED UP off the bodies — 0 with an infinite reserve, and
    /// what the ammo economy is worth when there is not one.
    pub mean_picked_up_ammo: f64,
    pub mean_transforms: f64,
    /// GHOSTS — how many a run left standing, and the most at once. Counted,
    /// and nothing in the fight reads them back.
    pub mean_ghosts: f64,
    pub ghosts_peak: u32,
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
    /// spread of the statistic being ranked. NOT `std_kills`, which is a
    /// different statistic (whole kills, no partial credit) that merely looks
    /// like it: a build that never finishes its second kill has `std_kills` 0
    /// and a kill progress that moves all run long.
    pub std_kill_progress: f64,
    pub mean_shots: f64,
    pub mean_pellets: f64,
    pub mean_crit_rate: f64,
    pub mean_big_crit_rate: f64,
    /// What the wielder took from their own build, per run — see
    /// [`SelfDamage`]. Zero for every build that charges nothing.
    pub mean_self_damage: SelfDamage,
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
    pub mean_downtime_seconds: f64,
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
    /// Mean effective damage by source (the damage-meter view).
    pub source_damage: SourceDamage,
    /// THE BENCHMARK FIGHT: the runs ranked by `FightParams::sample_by`, and the
    /// one at index `len / 2` — the exact middle of an odd count, the upper of
    /// the two middles of an even one. Every figure above is a MEAN and is what
    /// ranks and what the headline shows; this is ONE run, kept so the replay,
    /// the damage meter and the combat record show a fight that happened. Its
    /// numbers differ from the means, and the page says so beside them.
    pub median_run: RunResult,
}

/// THE PER-RUN SERIES of the two statistics builds are compared by, in RUN
/// ORDER — which is what makes them PAIRED.
///
/// [`monte_carlo`] advances its master rng exactly once per run whatever the
/// run does, and `Draws::new` derives the three streams from that one number,
/// so run `i` of one build and run `i` of another are drawn from the same luck.
/// Two summaries cannot say that: `mean ± σ/√n` describes each build alone, and
/// combining them treats two paired samples as independent — which overstates
/// the spread and cannot tell "the same fight" from "a difference below what I
/// can measure".
///
/// The cheap proxy fails in the direction that matters: on the Kuva Nukor all
/// seven progenitor elements report the same MEDIAN proc count while the fights
/// differ by up to 30%.
///
/// BESIDE [`Summary`] rather than in it: `Summary` is `Copy` and the optimizer
/// copies one per candidate, so two `Vec`s in it would be a cost paid by
/// millions of evaluations that never read them.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RunSeries {
    pub kill_progress: Vec<f64>,
    pub effective: Vec<f64>,
}

/// Run `runs` engagements from a single seed and summarize.
pub fn monte_carlo(params: &FightParams, runs: u32, seed: u64) -> Summary {
    monte_carlo_inner(params, runs, seed, false, &mut |_| {}).0
}

/// …REPORTING HOW FAR IT HAS GOT, for a caller that has to wait.
///
/// `on_run` is handed the number of completed runs. A single-target fight is
/// ~1 multishot a run and nobody needs this; a 361-body one is tens of milliseconds,
/// so the rulers' 1000 runs is a minute in the browser and a button that says
/// "Simulating…" for a minute reads as a hang.
///
/// THE ANSWER IS UNCHANGED: the callback observes, it never steers. Everything
/// about the run — the seed, the order, the arithmetic — is what it was.
pub fn monte_carlo_reporting(
    params: &FightParams,
    runs: u32,
    seed: u64,
    on_run: &mut impl FnMut(u32),
) -> Summary {
    monte_carlo_inner(params, runs, seed, false, on_run).0
}

/// ...keeping the per-run series. See [`RunSeries`].
pub fn monte_carlo_series(params: &FightParams, runs: u32, seed: u64) -> (Summary, RunSeries) {
    monte_carlo_inner(params, runs, seed, true, &mut |_| {})
}

/// …both at once.
pub fn monte_carlo_series_reporting(
    params: &FightParams,
    runs: u32,
    seed: u64,
    on_run: &mut impl FnMut(u32),
) -> (Summary, RunSeries) {
    monte_carlo_inner(params, runs, seed, true, on_run)
}

/// WHAT A SLICE OF RUNS CONTRIBUTES — everything the summary is derived from,
/// and nothing that depends on how many runs there were.
///
/// It exists so eight workers can each take an eighth. A `Summary` cannot be
/// merged from other Summaries without loss — a standard deviation needs the
/// sum of squares, a percentile the raw list, the MEDIAN RUN which run it was —
/// so this carries all three and a merge is field-wise addition
/// (`eight_shards_are_one_run` asserts it BIT FOR BIT).
///
/// SMALL ON THE WIRE: one `f64` per killing run and two for the median index,
/// about 24 KB at a thousand runs against 8 MB of `RunResult`s.
/// ONE RUN'S PLACE IN THE RANKING: what it did, and how to run it again.
///
/// THE STATE IS TWO 32-BIT HALVES, and that is not tidiness. A shard crosses
/// the wasm boundary as JSON, and **a JSON number in JavaScript is a double** —
/// so a 64-bit RNG state above 2^53 comes back ROUNDED, the merge replays a
/// state that never existed, and the benchmark fight is a different fight.
/// It shows as every mean matching to the last bit while only the fight's own
/// numbers — `sample` and the replay — disagree.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub(super) struct RunKey {
    /// The run's value under `FightParams::sample_by` — what the ranking is by.
    pub(super) v: f64,
    pub(super) hi: u32,
    pub(super) lo: u32,
}

impl RunKey {
    pub(super) fn new(v: f64, state: u64) -> Self {
        Self { v, hi: (state >> 32) as u32, lo: state as u32 }
    }
    pub(super) fn state(self) -> u64 {
        (u64::from(self.hi) << 32) | u64::from(self.lo)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Shard {
    pub runs: u32,
    pub(super) sum: f64,
    pub(super) sum_sq: f64,
    pub(super) min: f64,
    pub(super) max: f64,
    pub(super) effective: f64,
    pub(super) effective_sq: f64,
    pub(super) dot: f64,
    pub(super) overkill: f64,
    pub(super) spilled: f64,
    /// The two halves of `Summary::mean_virus_stacks`, summed over every run
    /// of the shard. A ratio has to travel as its numerator and its
    /// denominator or a fleet of workers averages the averages.
    pub(super) health_damage: f64,
    pub(super) virus_stack_health: f64,
    pub(super) armor_left_health: f64,
    pub(super) procs: u64,
    pub(super) field_ticks: u64,
    pub(super) dot_ticks: u64,
    pub(super) reloads: u64,
    /// Rounds picked up off the bodies — see [`RunResult::picked_up_ammo`].
    pub(super) picked_up_ammo: f64,
    pub(super) transforms: u64,
    pub(super) ghost_kills: u64,
    pub(super) ghosts_peak: u32,
    pub(super) kills: u64,
    pub(super) kills_sq: u64,
    pub(super) kill_progress: f64,
    pub(super) kill_progress_sq: f64,
    pub(super) min_kills: u32,
    pub(super) max_kills: u32,
    pub(super) downtime: f64,
    pub(super) first_magazine: f64,
    pub(super) max_hit_sum: f64,
    pub(super) biggest: f64,
    pub(super) ttks: Vec<f64>,
    pub(super) shots: u64,
    pub(super) pellets: u64,
    pub(super) crits: u64,
    pub(super) big_crits: u64,
    pub(super) self_damage: SelfDamage,
    pub(super) crit_tier_sum: u64,
    pub(super) headshots: u64,
    pub(super) sources: SourceDamage,
    pub(super) by_body: Vec<f64>,
    pub(super) by_combatant: Vec<f64>,
    /// One per run: what it scored, and the RNG state it started from.
    ///
    /// The benchmark fight is what the replay shows, and finding it
    /// means ranking every run — so the merge ranks these and REPLAYS the
    /// winner, which is exact because a run is reproducible from its state.
    /// One extra run per simulation, against carrying a thousand of them.
    pub(super) index: Vec<RunKey>,
    pub(super) series: RunSeries,
}

impl Default for Shard {
    fn default() -> Self {
        Self {
            runs: 0,
            sum: 0.0,
            sum_sq: 0.0,
            // The identities for a MIN and a MAX, so an empty shard merges into
            // its neighbour without moving it.
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            effective: 0.0,
            effective_sq: 0.0,
            dot: 0.0,
            overkill: 0.0,
            spilled: 0.0,
            procs: 0,
            field_ticks: 0,
            dot_ticks: 0,
            reloads: 0,
            picked_up_ammo: 0.0,
            transforms: 0,
            ghost_kills: 0,
            ghosts_peak: 0,
            kills: 0,
            kills_sq: 0,
            kill_progress: 0.0,
            kill_progress_sq: 0.0,
            min_kills: u32::MAX,
            max_kills: 0,
            downtime: 0.0,
            first_magazine: 0.0,
            max_hit_sum: 0.0,
            biggest: 0.0,
            ttks: Vec::new(),
            health_damage: 0.0,
            virus_stack_health: 0.0,
            armor_left_health: 0.0,
            shots: 0,
            pellets: 0,
            crits: 0,
            big_crits: 0,
            self_damage: SelfDamage::default(),
            crit_tier_sum: 0,
            headshots: 0,
            sources: SourceDamage::default(),
            by_body: vec![0.0; crate::formation::MAX_BODIES + 1],
            by_combatant: vec![0.0; crate::fight::MAX_COMBATANTS],
            index: Vec::new(),
            series: RunSeries::default(),
        }
    }
}

impl Shard {
    /// TAKE ANOTHER SHARD'S CONTRIBUTION. Sums add, extremes take the better,
    /// lists concatenate — and none of it depends on the ORDER the shards
    /// arrive in, which is what lets eight workers finish whenever they finish.
    pub fn merge(&mut self, o: &Shard) {
        self.runs += o.runs;
        self.sum += o.sum;
        self.sum_sq += o.sum_sq;
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
        self.effective += o.effective;
        self.effective_sq += o.effective_sq;
        self.dot += o.dot;
        self.overkill += o.overkill;
        self.spilled += o.spilled;
        self.health_damage += o.health_damage;
        self.virus_stack_health += o.virus_stack_health;
        self.armor_left_health += o.armor_left_health;
        self.procs += o.procs;
        self.field_ticks += o.field_ticks;
        self.dot_ticks += o.dot_ticks;
        self.reloads += o.reloads;
        self.picked_up_ammo += o.picked_up_ammo;
        self.transforms += o.transforms;
        self.ghost_kills += o.ghost_kills;
        self.ghosts_peak = self.ghosts_peak.max(o.ghosts_peak);
        self.kills += o.kills;
        self.kills_sq += o.kills_sq;
        self.kill_progress += o.kill_progress;
        self.kill_progress_sq += o.kill_progress_sq;
        self.min_kills = self.min_kills.min(o.min_kills);
        self.max_kills = self.max_kills.max(o.max_kills);
        self.downtime += o.downtime;
        self.first_magazine += o.first_magazine;
        self.max_hit_sum += o.max_hit_sum;
        self.biggest = self.biggest.max(o.biggest);
        self.ttks.extend_from_slice(&o.ttks);
        self.shots += o.shots;
        self.pellets += o.pellets;
        self.crits += o.crits;
        self.big_crits += o.big_crits;
        self.self_damage.merge(&o.self_damage);
        self.crit_tier_sum += o.crit_tier_sum;
        self.headshots += o.headshots;
        add_sources(&mut self.sources, &o.sources);
        for (a, b) in self.by_combatant.iter_mut().zip(&o.by_combatant) {
            *a += *b;
        }
        for (a, b) in self.by_body.iter_mut().zip(&o.by_body) {
            *a += b;
        }
        self.index.extend_from_slice(&o.index);
        self.series.kill_progress.extend_from_slice(&o.series.kill_progress);
        self.series.effective.extend_from_slice(&o.series.effective);
    }
}

/// One run's contribution, in one place — so the shard loop and nothing else
/// knows how a `RunResult` becomes a total.
pub(super) fn add_sources(acc: &mut SourceDamage, r: &SourceDamage) {
    acc.direct += r.direct;
    acc.radial += r.radial;
    acc.field += r.field;
    acc.arcane_on_status += r.arcane_on_status;
    acc.extra_hit += r.extra_hit;
    acc.syndicate += r.syndicate;
    for (a, v) in acc.status.iter_mut().zip(r.status) {
        *a += v;
    }
    for (a, v) in acc.extra_hit_by_type.iter_mut().zip(r.extra_hit_by_type) {
        *a += v;
    }
    for (a, v) in acc.syndicate_by_type.iter_mut().zip(r.syndicate_by_type) {
        *a += v;
    }
}

/// The odd 64-bit constant SplitMix64 uses to walk its own state — reused here
/// to spread consecutive run INDICES across the seed space, so run 0 and run 1
/// are as unrelated as two arbitrary seeds.
pub(super) const GOLDEN_GAP: u64 = 0x9E37_79B9_7F4A_7C15;

pub(super) fn monte_carlo_inner(
    params: &FightParams,
    runs: u32,
    seed: u64,
    keep: bool,
    on_run: &mut impl FnMut(u32),
) -> (Summary, RunSeries) {
    monte_carlo_range(params, 0, runs, seed, keep, on_run)
}

/// …A SLICE OF THEM, which is what a shard runs. `from` is the index of the
/// first run, and every run's dice are a pure function of `(seed, index)` — so
/// eight workers each taking an eighth produce exactly what one worker taking
/// all of them would.
pub(super) fn monte_carlo_range(
    params: &FightParams,
    from: u32,
    runs: u32,
    seed: u64,
    keep: bool,
    on_run: &mut impl FnMut(u32),
) -> (Summary, RunSeries) {
    let s = shard(params, from, runs, seed, keep, on_run);
    s.finish(params, runs)
}

/// RUN A SLICE and hand back what it contributed — the shape a WORKER returns.
///
/// `from` is the index of the first run; every run's dice are a pure function
/// of `(seed, index)`, so eight of these over disjoint slices merge into what
/// one call over the whole range produces.
pub fn shard(
    params: &FightParams,
    from: u32,
    runs: u32,
    seed: u64,
    keep: bool,
    on_run: &mut impl FnMut(u32),
) -> Shard {
    let mut a = Shard { runs, ..Shard::default() };
    for i in from..from + runs {
        // EACH RUN'S OWN DICE, derived from the fight's seed and the run's
        // INDEX. It was one Rng chain threaded through every run, so run i
        // depended on how many draws runs 0..i had consumed — which made a run
        // impossible to start without replaying everything before it, and
        // therefore impossible to shard.
        //
        // STILL REPRODUCIBLE, which is the promise that matters: the same seed
        // and the same index give the same run, on any machine and in any
        // order. What moved ONCE is the SAMPLE — measured at 0.001% to 0.09%
        // across the three `one_fight` shapes, against a Monte-Carlo standard
        // error of about 0.3% at a thousand runs. A different draw from the
        // same distribution, smaller than the noise the board already lives
        // with.
        let state = seed ^ u64::from(i).wrapping_mul(GOLDEN_GAP);
        let r = run_once(params, &mut Rng::new(state));
        a.index.push(RunKey::new(params.sample_by.of(&r), state));
        on_run(a.index.len() as u32);
        if keep {
            a.series.kill_progress.push(r.kill_progress);
            a.series.effective.push(r.effective_damage());
        }
        a.sum += r.total_damage();
        a.sum_sq += r.total_damage() * r.total_damage();
        a.min = a.min.min(r.total_damage());
        a.max = a.max.max(r.total_damage());
        a.effective += r.effective_damage();
        a.effective_sq += r.effective_damage() * r.effective_damage();
        a.dot += r.tally.dot();
        a.overkill += r.overkill;
        a.spilled += r.spilled;
        a.health_damage += r.health_damage;
        a.virus_stack_health += r.virus_stack_health;
        a.armor_left_health += r.armor_left_health;
        a.procs += u64::from(r.procs);
        a.field_ticks += u64::from(r.field_ticks);
        a.dot_ticks += u64::from(r.dot_ticks);
        a.reloads += u64::from(r.reloads);
        a.picked_up_ammo += r.picked_up_ammo;
        a.transforms += u64::from(r.transforms);
        a.ghost_kills += u64::from(r.ghost_kills);
        a.ghosts_peak = a.ghosts_peak.max(r.ghosts_peak);
        a.kills += u64::from(r.kills);
        a.kills_sq += u64::from(r.kills) * u64::from(r.kills);
        a.kill_progress += r.kill_progress;
        a.kill_progress_sq += r.kill_progress * r.kill_progress;
        a.min_kills = a.min_kills.min(r.kills);
        a.max_kills = a.max_kills.max(r.kills);
        a.downtime += r.downtime_seconds;
        a.first_magazine += r.first_magazine_damage;
        a.max_hit_sum += r.tally.max_hit();
        a.biggest = a.biggest.max(r.tally.max_hit());
        if let Some(at) = r.first_kill_at {
            a.ttks.push(at);
        }
        a.shots += u64::from(r.shots);
        a.pellets += u64::from(r.pellets);
        a.crits += u64::from(r.crits);
        a.big_crits += u64::from(r.big_crits);
        a.self_damage.merge(&r.self_damage);
        a.crit_tier_sum += u64::from(r.crit_tier_sum);
        a.headshots += u64::from(r.headshots);
        add_sources(&mut a.sources, &r.sources);
        for (acc, v) in a.by_combatant.iter_mut().zip(r.dealt.by_combatant().0) {
            *acc += v;
        }
        for (acc, v) in a.by_body.iter_mut().zip(r.taken.by_body().0) {
            *acc += v;
        }
    }
    a
}

impl Shard {
    /// TURN THE CONTRIBUTIONS INTO THE ANSWER. Everything below divides by the
    /// run count, which is why none of it can live in the shard.
    pub fn finish(self, params: &FightParams, _runs: u32) -> (Summary, RunSeries) {
        let runs = self.runs;
        let series = self.series.clone();
        let (sum, sum_sq, min, max) = (self.sum, self.sum_sq, self.min, self.max);
        let (effective, effective_sq, dot) = (self.effective, self.effective_sq, self.dot);
        let (overkill, spilled) = (self.overkill, self.spilled);
        let (health_damage, virus_stack_health) = (self.health_damage, self.virus_stack_health);
        let armor_left_health = self.armor_left_health;
        let (procs, field_ticks, reloads, transforms) =
            (self.procs, self.field_ticks, self.reloads, self.transforms);
        let picked_up_ammo = self.picked_up_ammo;
        let (ghost_kills, ghosts_peak) = (self.ghost_kills, self.ghosts_peak);
        let dot_ticks = self.dot_ticks;
        let (kills, kills_sq) = (self.kills, self.kills_sq);
        let (kill_progress, kill_progress_sq) = (self.kill_progress, self.kill_progress_sq);
        let (min_kills, max_kills) = (self.min_kills, self.max_kills);
        let (downtime, first_magazine, max_hit_sum, biggest) =
            (self.downtime, self.first_magazine, self.max_hit_sum, self.biggest);
        // A PERCENTILE OVER WHAT ACTUALLY HAPPENED. Sorted here rather than by
        // the caller: an unsorted percentile is a number that looks right. It
        // is also why the shard carries the raw list — a merge cannot recover a
        // percentile from two of them.
        let mut ttks = self.ttks.clone();
        ttks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let pct = |v: &[f64], q: f64| -> f64 {
            if v.is_empty() {
                return 0.0;
            }
            let i = ((v.len() as f64 - 1.0) * q).round() as usize;
            v[i.min(v.len() - 1)]
        };
        let (shots, pellets, crits, big_crits) =
            (self.shots, self.pellets, self.crits, self.big_crits);
        let (crit_tier_sum, headshots) = (self.crit_tier_sum, self.headshots);
        let sources = self.sources;
        let mut mean_damage_by_body = BodyDamage::default();
        let mut mean_damage_by_combatant = CombatantDamage::default();
        if runs > 0 {
            for (acc, v) in mean_damage_by_body.0.iter_mut().zip(&self.by_body) {
                *acc = v / f64::from(runs);
            }
            for (acc, v) in mean_damage_by_combatant.0.iter_mut().zip(&self.by_combatant) {
                *acc = v / f64::from(runs);
            }
        }

        let n = runs.max(1) as f64;
        let mean = sum / n;
        let variance = (sum_sq / n - mean * mean).max(0.0);
        let total_pellets = pellets.max(1) as f64;
        let total_shots = shots;
    let summary = Summary {
        mean_damage_by_body,
        mean_damage_by_combatant,
        runs,
        duration_seconds: params.duration_seconds,
        mean_damage: mean,
        dps: mean / params.duration_seconds,
        std_damage: variance.sqrt(),
        min_damage: if min.is_finite() { min } else { 0.0 },
        max_damage: if max.is_finite() { max } else { 0.0 },
        mean_effective_damage: effective / n,
        std_effective_damage: {
            let mean_e = effective / n;
            (effective_sq / n - mean_e * mean_e).max(0.0).sqrt()
        },
        effective_dps: effective / n / params.duration_seconds,
        mean_dot_damage: dot / n,
        mean_overkill: overkill / n,
        mean_spilled: spilled / n,
        mean_health_damage: health_damage / n,
        // THE RATIO OF THE SUMS, never the mean of the ratios: a run that
        // reached health once and a run that reached it ten thousand times
        // are not one vote each.
        mean_virus_stacks: if health_damage > 0.0 {
            virus_stack_health / health_damage
        } else {
            0.0
        },
        mean_armor_left: if health_damage > 0.0 {
            armor_left_health / health_damage
        } else {
            1.0
        },
        mean_procs: procs as f64 / n,
        mean_field_ticks: field_ticks as f64 / n,
        mean_dot_ticks: dot_ticks as f64 / n,
        mean_reloads: reloads as f64 / n,
        mean_picked_up_ammo: picked_up_ammo / n,
        mean_transforms: transforms as f64 / n,
        mean_ghosts: ghost_kills as f64 / n,
        ghosts_peak,
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
        mean_self_damage: self.self_damage.scale(1.0 / runs.max(1) as f64),
        mean_crit_tier: crit_tier_sum as f64 / total_pellets,
        mean_headshot_rate: headshots as f64 / total_pellets,
        burst_dps: {
            // The time the weapon was NOT reloading, across every run. Guarded
            // at a hundredth of a second: a run that was reloading the whole
            // time has no burst to report and must not report infinity.
            let firing = (params.duration_seconds * runs as f64 - downtime).max(1e-2);
            effective / firing
        },
        mean_downtime_seconds: downtime / runs as f64,
        ttk_mean: if ttks.is_empty() { 0.0 } else { ttks.iter().sum::<f64>() / ttks.len() as f64 },
        ttk_median: pct(&ttks, 0.5),
        ttk_p90: pct(&ttks, 0.9),
        ttk_runs: ttks.len() as u32,
        mean_first_magazine: first_magazine / runs as f64,
        max_hit: biggest,
        mean_max_hit: max_hit_sum / runs as f64,
        damage_per_shot: effective / (total_shots as f64).max(1.0),
        damage_per_pellet: effective / total_pellets,
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
        // THE BENCHMARK FIGHT, REPLAYED. The shard carries one `(value,
        // rng_state)` pair per run rather than the runs themselves — 16 bytes
        // against 8 KB — so ranking them here and re-running the winner is
        // exact and costs one extra engagement per simulation. A tie breaks on
        // the state, so the pick cannot depend on the order shards merged in.
        median_run: {
            let mut idx = self.index;
            idx.sort_by(|a, b| a.v.total_cmp(&b.v).then(a.state().cmp(&b.state())));
            match idx.get(idx.len() / 2) {
                Some(k) => run_once(params, &mut Rng::new(k.state())),
                None => RunResult::default(),
            }
        },
    };
        (summary, series)
    }
}
