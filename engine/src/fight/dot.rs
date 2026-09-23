use super::*;

/// A live DoT instance: the proccing hit's deferred damage (provenance
/// snapshot baked into `value`). Bleed (Cinematic ticks) ignores armor;
/// elemental DoTs (Toxin/Electricity/Gas, Disrupt's break-proc Tesla) take
/// live armor mitigation. `dtype` is the STATUS type (for CO counting).
///
/// COPY, because an AREA proc hands the same tick to every body near the one it
/// landed on — a gas cloud is ONE entity and its neighbours take its number.
#[derive(Debug, Clone, Copy)]
pub(super) struct Dot {
    pub(super) next_tick: f64,
    pub(super) ticks_left: u32,
    /// WHICH SHOT SEEDED THIS, by combat-record event id (`record::Event::id`).
    ///
    /// A bleed that settles four seconds after the round that applied it is the
    /// one damage in this engine whose origin nothing could name: by the time
    /// it pays, the shot is long gone. Carrying the id is what lets a reader
    /// ask what a trigger pull was ACTUALLY worth — its pellets, its explosion
    /// AND the burns it left — which is a number no aggregate here can produce.
    ///
    /// `u32::MAX` is "nobody recorded it", which is every run that is not being
    /// read and every DoT applied before recording began.
    pub(super) cause: u32,
    /// THE INSTANCE'S OWN HALF of the tick — coefficient, ModifiedBase, status
    /// damage, and the crit and body part of the hit that applied it. Frozen,
    /// because those are facts about a hit that has already happened.
    ///
    /// The SOURCE's half is not here: see [`Dot::live`].
    pub(super) frozen: f64,
    /// WHERE THE TICK ITSELF LANDS, over the WHOLE tick — accumulator included.
    /// 1.0 on a body; on a head, the headshot-damage brackets over a 1x base
    /// (`lands_on_a_part`). Separate from `frozen`, which holds the part the
    /// HIT struck. MEASUREMENTS M100.
    pub(super) landing: f64,
    /// The MOD-side element bracket, `1 + Σ this element's bonuses` — frozen
    /// because mods do not change mid-fight, and carried rather than re-read
    /// because a transform weapon's ACTIVE form is not always the form that
    /// applied the status.
    pub(super) bracket: f64,
    /// Which DERIVATION STEP the faction bonus is re-applied at for this tick
    /// (2 for a status a hit applied). See `faction_at`.
    pub(super) depth: u32,
    /// Does the SOURCE scale this at all?
    ///
    /// False for the one DoT that is a share of the TARGET's own pool — a
    /// shield-break Electricity tick is `3% x stacks` of what was broken, and
    /// no bonus the shooter carries touches it. Without this flag it would pick
    /// up Lavos and Eclipse through `live`, which is a bonus on a number that
    /// was never the weapon's.
    pub(super) source_scaled: bool,
    /// WHAT THE ACCUMULATOR'S INITIAL `1` IS WORTH here — `C x` the multipliers
    /// that are NOT re-read live, and 0.0 for a tick the rule does not cover.
    /// See [`Dot::accumulator_unit`].
    pub(super) unit: f64,
    pub(super) dtype: DamageType,
    pub(super) ignores_armor: bool,
}

impl Dot {
    /// WHAT THIS TICK IS WORTH RIGHT NOW.
    ///
    /// A DoT TRACKS ITS SOURCE: a buff the shooter gains while it is burning
    /// strengthens it immediately, measured on an elemental bonus (Lavos) and
    /// on a faction bonus. It leaves the INSTANCE's own facts alone — the crit
    /// and the body part belong to a hit that is over, and no later buff can
    /// change where a bullet landed.
    ///
    /// WHICH FACTORS: the element and faction brackets are re-read, the FINAL
    /// multiplier is not, and Eclipse draws that line. Both are abilities with
    /// a duration, so it is about the BRACKET rather than where the buff came
    /// from — the same split the wiki states for another reason: *"Unlike
    /// faction damage, which double dips for status effects, the one from
    /// Eclipse is applied once."* Eclipse lives in `frozen`, applied once at
    /// the moment the proc landed.
    pub(super) fn live(&self, params: &FightParams, now: f64, w: &CardWindows) -> f64 {
        if !self.source_scaled {
            return self.frozen;
        }
        self.frozen
            * (self.bracket + params.element_at(self.dtype, now, w))
            * faction_at(params.faction_at_time(now), self.depth)
    }

    /// **THE ACCUMULATOR STARTS AT 1, NOT AT 0** (wiki `Damage/Calculation`
    /// §Damage Over Time): *"the temporary damage accumulator for each tick
    /// group starts at 1 rather than 0"*, giving
    ///
    /// ```text
    /// Unrounded Tick Damage = (Σ Sᵢ + 1) × C × M
    /// ```
    ///
    /// `Sᵢ` each stored seed, `C` 0.5 (Heat, Electricity, Toxin, Gas) or 0.35
    /// (Slash), `M` the elemental, faction and status-damage bonuses. So the `1`
    /// is worth `C × M` — this method — rather than a flat +1 of damage.
    ///
    /// IT IS PER TICK GROUP, ONCE: *"If several seeds are consolidated into a
    /// single tick … its initial value of 1 is included only once."* Heat,
    /// Electricity and Gas consolidate; Slash and Toxin each carry their own.
    /// ONLY ONE FACTION LAYER, since the seed already holds the hit's own, so a
    /// payload at `depth` puts `depth − 1` in the seed and one here. M56.
    pub(super) fn accumulator_unit(&self, params: &FightParams, now: f64, w: &CardWindows) -> f64 {
        if !self.source_scaled || self.unit == 0.0 {
            return 0.0;
        }
        self.unit
            * (self.bracket + params.element_at(self.dtype, now, w))
            * params.faction_at_time(now)
    }

    /// THE MULTIPLIERS OVER EACH HALF, for the ledger and nothing else:
    /// `(over the seeds, over the accumulator)`. They differ in one place and
    /// that is the point of drawing them — the seed carries the payload's own
    /// faction depth and the accumulator carries one layer, so one tick holds
    /// x2.4025 and x1.55 side by side.
    pub(super) fn explain(
        &self,
        params: &FightParams,
        now: f64,
        w: &CardWindows,
    ) -> (Vec<crate::record::Scale>, Vec<crate::record::Scale>) {
        use crate::record::{Factor, Scale};
        let elem = self.bracket + params.element_at(self.dtype, now, w);
        let f = params.faction_bracket_at(now);
        let m = params.foe.faction_bracket_multiplier;
        let at = |depth: u32| {
            vec![
                Scale { factor: Factor::ElementBracket, value: elem },
                Scale { factor: Factor::Faction, value: faction_at(f, depth) },
                Scale { factor: Factor::TargetMultiplier, value: faction_at(m, depth) },
            ]
        };
        (at(self.depth), at(DEPTH_HIT))
    }
}

/// The Heat singleton accumulator (data/debuffs/ignite.yaml): ONE entity
/// per target; each proc adds its contribution to the tick value and
/// refreshes the shared expiry; ticks stay anchored to the FIRST proc.
/// THE SOURCE-SIDE FACTS A HEAT ENTITY IS BORN WITH, taken from its first proc.
///
/// One type because they are set together, read together and never
/// individually: Heat is one entity per target with one clock, so the first
/// proc decides all three and every later contribution on this target is the
/// same weapon in the same fight.
#[derive(Clone, Copy)]
pub(super) struct HeatOrigin {
    /// The mod-side element bracket, as [`Dot::bracket`].
    pub(super) bracket: f64,
    /// The faction derivation step, as [`Dot::depth`].
    pub(super) depth: u32,
    /// What the accumulator's initial `1` is worth, as
    /// [`Dot::accumulator_unit`].
    pub(super) unit: f64,
}

pub(super) struct HeatEntity {
    pub(super) born: f64,
    pub(super) expiry: f64,
    pub(super) next_tick: f64,
    /// Current consolidated tick value (sum of the live contributions).
    pub(super) value: f64,
    /// THE MOD-SIDE ELEMENT BRACKET these contributions were frozen WITHOUT.
    /// Heat is one entity per target with one clock, so it carries one — taken
    /// from the first proc, which on every build in this roster is the only
    /// answer there is (one form, one set of mods).
    pub(super) bracket: f64,
    /// The faction derivation step, as [`Dot::depth`].
    pub(super) depth: u32,
    /// What the accumulator's initial `1` is worth for this entity, set by the
    /// FIRST proc for the same reason `bracket` is. Heat is the archetype of a
    /// consolidated tick group, so it is counted ONCE however many stacks fold
    /// into `value` — see [`Dot::accumulator_unit`].
    pub(super) unit: f64,
    /// Individual contributions, oldest first — only tracked when a per-unit
    /// stack cap applies, so a capped Heat can drop its OLDEST contribution
    /// (FIFO) when a new proc lands, exactly like every other capped status.
    /// Empty when uncapped (contributions just fold into `value`).
    pub(super) recent: Vec<f64>,
    /// HOW MANY PROCS ARE IN `value`, which is the number a reader wants and
    /// the one thing an accumulator has no other way to report.
    ///
    /// Heat is the only status DoT that CONSOLIDATES: every proc folds into one
    /// entity whose tick grows linearly and whose single clock is refreshed
    /// (data/debuffs/ignite.yaml, `dot_model: singleton_accumulator`). The
    /// damage is right either way; without this the debuff table can only say
    /// `heat.is_some()` — 0 or 1 — so a player watching a ten-stack burn is
    /// told it is one stack, with no way to see the ramp that is most of what
    /// Heat does.
    ///
    /// UNCAPPED, because the spec says so: `max_stacks: null`, *"indefinite
    /// ramp while kept refreshed"*. Under a per-unit cap it holds at the cap,
    /// since the oldest contribution is dropped as the new one lands.
    pub(super) stacks: u32,
}

/// A Blast (Detonate) stack: the fuse fires a single-target hit; the 10th
/// stack detonates everything early. The radial is ignored — single-target
/// sim, and the host is excluded from the radial anyway.
pub(super) struct BlastStack {
    pub(super) fuse: f64,
    pub(super) value: f64,
    /// The applying weapon's [`InstanceScale::xh_bracket`], carried so that the
    /// EXTRA HIT this detonation triggers can be scaled by an elemental bracket
    /// the detonation itself never gets. The one thing a Blast stack has to
    /// remember about the gun that made it.
    pub(super) xh_bracket: f64,
}

/// AN INSTANT AREA HIT queued on one body and paid out to its neighbours.
///
/// Two mechanisms produce these and they are not the same shape, which is why
/// this is a struct and not the `(damage, radius)` pair it was: a Blast
/// detonation reaches everyone inside its sphere for the same number, while
/// Acid Shells' corpse explosion has "linear damage falloff from 100% to 0%
/// from the central enemy" and carries two damage types at once.
#[derive(Debug, Clone, Copy)]
pub(super) struct AreaHit {
    pub(super) damage: f64,
    pub(super) radius_m: f64,
    /// What it is made of — Blast for a detonation, and Acid Shells' own
    /// Corrosive/Blast split for the augment.
    pub(super) shares: TypeShares,
    /// Does the share fall off with distance? A Blast detonation does not; the
    /// corpse explosion falls linearly to nothing at the rim.
    pub(super) linear_falloff: bool,
}
