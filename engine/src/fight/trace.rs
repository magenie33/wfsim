use super::*;

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
    /// instant of the fight.
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
    /// …and by ATTACKER, cumulative, seat for seat with
    /// [`FightParams::combatant_ids`]. The meter cuts one total by what KIND of
    /// damage it was; this cuts the same total by WHO DEALT IT, and a replay
    /// that carried only the first cannot answer "whose damage was that" at
    /// any instant of a fight with more than one thing firing.
    pub dealt: CombatantDamage,
    /// Live stacks per buff PER SEAT, positionally matching [`Replay::buffs`]
    /// — one series set per seat, in [`Replay::seats`]' order.
    ///
    /// A BUFF IS A SEAT'S OR IT IS NOBODY'S. Two seats in one fight are two
    /// builds, so they carry two rosters and two piles; a single flat list
    /// could only ever hold whichever seat last took a turn, which is a curve
    /// that belongs to neither of them.
    pub stacks: Vec<Vec<u16>>,
    /// …and the same for the TARGETS, one series per body in
    /// [`Replay::tracked`], each positionally matching [`DEBUFF_ROSTER`].
    ///
    /// The mirror of the line above, because the page draws one table from each
    /// and the two are the same component — and a Vec OF
    /// series since 2026-08-17, because a fight has up to 400 bodies and each
    /// one carries its own debuffs. Which of them are here is
    /// `Replay::tracked`'s decision, not this struct's.
    pub debuffs: Vec<Vec<u16>>,
}

/// HOW A STACK COUNT READS AS THE NUMBER IT BUYS.
///
/// Almost every buff in the app is capped by a STACK COUNT, and the count is
/// what DE publishes for it — "Stacks up to 4x" — so the honest chart is a
/// chart of stacks. A few are the other way round: the card publishes the
/// NUMBER the pile stops at and lets the counter run (Hata-Satya's 500%), and
/// for those a stack count is a chart of the wrong quantity — it climbs past
/// the ceiling it is drawn against, and the ceiling it is drawn against is one
/// nobody printed.
///
/// Declared where the ceiling is a value; absent everywhere else, which is what
/// keeps the four-stack buffs reading `3/4` exactly as they always have.
///
/// The numbers are the ENGINE's (0.012, 5.0) and `unit` is how they are read:
/// "%" means the reader multiplies by 100, the same convention `pct()` uses one
/// module over. The engine states the quantity; the page states the
/// presentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackValue {
    pub per_stack: f64,
    /// Where it stops climbing — the published ceiling, not `per_stack ×
    /// max_stacks`, which is a hair above it by construction.
    pub max: f64,
    pub unit: &'static str,
}

/// One rostered buff: the id every surface joins on, the stack ceiling (0 =
/// uncapped), and — where the ceiling is a number rather than a count — how to
/// read the stacks as that number.
#[derive(Debug, Clone, PartialEq)]
pub struct BuffSeries {
    pub id: String,
    pub max_stacks: u32,
    pub value: Option<StackValue>,
}

impl BuffSeries {
    /// The ordinary one: a count out of a count.
    pub(super) fn stacked(id: String, max_stacks: u32) -> Self {
        Self { id, max_stacks, value: None }
    }
}

/// A replay of one engagement: the buff roster it was fought with, and a frame
/// every `frame_seconds` seconds.
///
/// Sampling costs ONE extra run, not one per Monte Carlo iteration: `Rng` is
/// SplitMix64 with a single `u64` of state, so a run records the state it
/// STARTED from ([`RunResult::rng_state`]) and can be replayed bit-for-bit
/// afterwards. Carrying frames on every `RunResult` would have cost the trace
/// times `runs` — 20,000 of them at the cap.
#[derive(Debug, Clone, Default)]
pub struct Replay {
    /// WHOSE DEBUFFS THE FRAMES CARRY, by `formation::FoeSpec::id`, in the
    /// order `Frame::debuffs` holds them. `tracked[0]` is always the aimed
    /// body.
    ///
    /// A CAP, AND IT IS STATED. A series is 600 frames x 15 debuffs, so a body
    /// costs 18 KB and a 19x19 ruler would be 6.5 MB — larger than the whole
    /// wasm. So the replay follows the aimed body plus the hardest-hit few, and
    /// the page SAYS how many took damage and were not followed: a silent cap
    /// reads as "that is everyone", which is the one thing it must not.
    pub tracked: Vec<String>,
    /// The same list as INDICES, which is what the sampler reads. Not public:
    /// an index is the engine's business and a name is everyone else's.
    pub(crate) follow: Vec<usize>,
    /// Seconds between frames.
    pub frame_seconds: f64,
    /// WHOSE BUFFS THE FRAMES CARRY, by seat, in the order [`Frame::stacks`]
    /// holds them — the same list and the same order as
    /// [`FightParams::combatant_ids`], so `seats[0]` is the wielder.
    ///
    /// The mirror of [`Replay::tracked`] on the other side of the fight, and
    /// uncapped for the same reason it does not need to be: seats are capped
    /// at [`MAX_COMBATANTS`] before a fight opens.
    pub seats: Vec<String>,
    /// The rostered buffs OF EACH SEAT, in `seats`' order and then in the
    /// order [`Frame::stacks`] holds them — ids are the SAME vocabulary as
    /// [`BuffConfig`] and the web's buff cards, because they come from one
    /// place: [`FightParams::buff_roster`], asked of each seat's own build.
    pub buffs: Vec<Vec<BuffSeries>>,
    pub frames: Vec<Frame>,
}

impl Replay {
    /// One seat's roster. A CALLER NAMES THE SEAT IT MEANS — there is no
    /// "the buffs" on a fight with more than one thing acting in it, and a
    /// default would quietly hand back the wielder's to a question about
    /// somebody else. Out of range is empty, never a panic: the seat list is
    /// the caller's to check.
    pub fn buffs_of(&self, seat: usize) -> &[BuffSeries] {
        self.buffs.get(seat).map_or(&[], Vec::as_slice)
    }
}

impl Frame {
    /// One seat's live stacks at this instant, in [`Replay::buffs_of`]'s
    /// order for the same seat.
    pub fn stacks_of(&self, seat: usize) -> &[u16] {
        self.stacks.get(seat).map_or(&[], Vec::as_slice)
    }
}

/// Frames in a replay, whatever the engagement length. 600 over 300 s is one
/// every half second — smooth enough to scrub, small enough to ship as JSON.
pub const REPLAY_FRAMES: usize = 600;
/// HOW MANY BODIES A REPLAY FOLLOWS — the aimed one plus the hardest-hit few.
///
/// Eight because the cost is real and the reader's attention is not infinite: a
/// series is `REPLAY_FRAMES x DEBUFF_ROSTER.len()` u16, so 18 KB a body and
/// 6.5 MB for a 19x19 ruler — larger than the whole wasm. Eight is ~145 KB,
/// and it covers what a reader would actually open.
///
/// THE CAP IS STATED ON SCREEN, never applied silently: `Replay::tracked` says
/// who was followed and the roll call says who took damage, so "five more were
/// hit" is readable rather than implied by an absence.
pub const REPLAY_TRACKED: usize = 8;

/// Per-buff configured policy: buff id → (initial stacks, locked). Ids match
/// the web's `enumerate_buffs` (`condition_overload`, `on_kill_multishot`,
/// `on_headshot_cc`, `on_headshot_kill_cc`, `on_kill_cd`, `on_reload_fr`,
/// `arcane:{id}`). Frenzy is configured via [`LockMode`], not here.
pub type BuffConfig = std::collections::HashMap<String, (u32, bool)>;
