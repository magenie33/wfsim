use super::*;

/// Which pool a damage instance just emptied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BrokenPool {
    Overguard,
    Shield,
}

/// WHICH POOL a portion of a damage instance landed in.
///
/// The game pops ONE NUMBER PER POOL, which is why this is carried rather than
/// summed away: Toxin bypasses a shield while its siblings do not, so a single
/// pellet on a shielded Corpus unit shows two numbers side by side. An engine
/// that reports their sum cannot be checked against a recording.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Pool {
    Overguard,
    Shield,
    /// THE DEFAULT, because it is the pool every fight ends in and the only one
    /// a target is guaranteed to have.
    #[default]
    Health,
}

impl Pool {
    pub fn name(self) -> &'static str {
        match self {
            Pool::Overguard => "overguard",
            Pool::Shield => "shield",
            Pool::Health => "health",
        }
    }
}

/// ONE POOL'S SHARE of a damage instance, with every factor between the raw
/// number and what the pool actually lost.
///
/// This is the DEFENSIVE half of a hit's account, and it is a list rather than
/// a single mitigation ratio because the ratio hides everything that makes it:
/// a wrong vulnerability column and a wrong armour value produce the same
/// `×0.081` and are two different bugs.
///
/// The product is exact: `raw × gate × share × column × disrupt_amp ×
/// virus_amp × armor × attenuation = effective`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Portion {
    pub pool: Pool,
    /// WHAT COLOUR THIS NUMBER IS — the dominant type of the share that landed
    /// here, which on a split is NOT the instance's own: the half a shield
    /// stops is the physical one and the half that goes through is Toxin, so
    /// the two numbers the game shows are two different colours.
    pub dtype: DamageType,
    /// The fraction of the instance's damage VECTOR routed into this pool,
    /// before the vulnerability column. 1.0 unless a shield split the instance.
    pub share: f64,
    /// The column that share read — the weighted mean over the types in it, so
    /// `share × column` is exactly the portion `apply` computed. The Overguard
    /// layer reads its own table (wiki `Overguard`), which is why this is per
    /// portion rather than per instance.
    pub column: f64,
    pub disrupt_amp: f64,
    /// HOW MUCH OF THE INSTANCE THE SHIELD DID NOT ABSORB — the overflow as a
    /// fraction of what arrived, or 1.0 where no shield stood in the way.
    ///
    /// It is a separate question from the gate beside it, and the two are
    /// multiplied in order: a 160-damage hit on a 120-point shield has 40 of
    /// overflow (`past_shield` = 0.25) and five per cent of THAT reaches health
    /// (`shield_gate` = 0.05), which is the 2 the game pops. Folding them into
    /// one factor would report `×0.0125` and name it nothing a reader could
    /// check against the wiki.
    pub past_shield: f64,
    /// WHAT GOT THROUGH A SHIELD THIS INSTANCE BROKE — 0.05, or 1.0 for a
    /// weakpoint hit that ignored the gate, or 1.0 where no shield broke.
    ///
    /// wiki `Shield`: *"5% of the damage dealt when hitting the shield gate
    /// will target enemy Health"*, and *"Any Headshots or shots to Weakspots
    /// completely bypass Corpus enemy Shield Gating"*. See
    /// [`ENEMY_SHIELD_GATE_LEAK`] and MEASUREMENTS M61.
    pub shield_gate: f64,
    pub virus_amp: f64,
    /// The armour term ACTUALLY APPLIED — normally `1 − DR`, but the health
    /// path floors a mitigated instance at 1 damage, and a ledger that printed
    /// `1 − DR` there would not multiply out. [`Self::floored`] says which of
    /// the two this is.
    pub armor: f64,
    pub floored: bool,
    /// The clamp an attenuating target applied, or 1.
    pub attenuation: f64,
    /// WHAT WAS LEFT IN THE POOL, as a factor — 1.0 unless the pool ran out
    /// mid-instance.
    ///
    /// A shield with 88 points left takes 88 from a 228-damage hit, not 228,
    /// and without this the ledger would describe an instance the pool never
    /// took: `base × steps × mitigation` would come out at 228 beside a row
    /// reading 88, which is precisely the shape of error this whole record
    /// exists to make visible (found by `check_combat_record`).
    ///
    /// It is a fact about the TARGET rather than about the hit, which is why it
    /// is here and last: everything above it is what the attacker and the
    /// defender did to the number, and this is the pool simply not having that
    /// much left to lose.
    pub pool_remaining: f64,
    pub effective: f64,
}

/// A portion of nothing: the slot a [`Settled`] starts its three with. Written
/// by hand because `DamageType` has no default and should not gain one — there
/// is no neutral damage type, only an unused slot.
impl Default for Portion {
    fn default() -> Self {
        Self {
            pool: Pool::Health,
            dtype: DamageType::Impact,
            share: 0.0,
            column: 1.0,
            disrupt_amp: 1.0,
            past_shield: 1.0,
            shield_gate: 1.0,
            virus_amp: 1.0,
            armor: 1.0,
            floored: false,
            attenuation: 1.0,
            pool_remaining: 1.0,
            effective: 0.0,
        }
    }
}

/// WHAT ONE DAMAGE INSTANCE DID, as every aggregate in this engine reads it.
///
/// DELIBERATELY THE SIZE OF THE TUPLE IT REPLACED. `apply` is called once per
/// damage instance — about five million times in a thousand-run Monte Carlo,
/// and four hundred million on the densest build measured — so what it returns
/// travels in registers or it is a tax on every fight nobody is watching. The
/// first version of this carried the whole breakdown here and cost **+13.3%**
/// across `one_fight`'s four shapes with every answer unchanged.
/// The breakdown lives in [`Breakdown`] instead, written into a slot on
/// `RunResult` that is copied once per RUN.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Settled {
    /// WHAT `apply` WAS ASKED TO SETTLE, carried back rather than re-stated by
    /// the caller. A damage site adding its own local to a total is a second
    /// spelling of "the number I just handed over", with nothing tying the two
    /// together, so a site can book one figure and settle another. [`settle`]
    /// books this one.
    pub raw: f64,
    pub effective: f64,
    pub killed: bool,
    /// WHAT THE KILLING BLOW SPENT ON A CORPSE — the part of this instance
    /// that took health below zero. The unit dies once however far past zero
    /// it goes, so this is damage that bought nothing, and a build can deal
    /// MORE of it while killing FEWER units.
    pub overkill: f64,
    /// WHAT A BROKEN SHIELD THREW AWAY — the 95% of the breaking instance's
    /// overflow the shield gate does not pass down (`ENEMY_SHIELD_GATE_LEAK`).
    /// It is a different waste from `overkill` and counted apart from it: one
    /// is killing a corpse, the other is hitting a gate too hard.
    ///
    /// OVERGUARD NEVER SPILLS ANY. Its excess carries into what is under it in
    /// full — see [`TargetState::apply`] — so an enemy has exactly two places
    /// to waste damage, and this is the second one.
    pub spilled: f64,
    /// WHAT REACHED HEALTH, which is the only pool Viral multiplies — the
    /// weight the statistic below is averaged against. Overguard and shields
    /// are damage this build dealt under a Viral pile that did nothing to it,
    /// so counting them would move the average with the fight's armour rather
    /// than with its Viral.
    pub health: f64,
    /// …AND THAT DAMAGE TIMES THE STACKS IN FORCE BEHIND IT. Summed over the
    /// engagement and divided by `health`, this is the average Viral pile
    /// every point of health damage was dealt through — over every body and
    /// every run, which is what the replay's eight followed bodies cannot say.
    pub virus_stack_health: f64,
    /// …AND THE OTHER HALF OF WHAT THE TARGET WAS WEARING: the same damage
    /// times the share of the target's ARMOUR still standing behind it, so
    /// the pair covers both factors the health path reads. 1.0 is armour
    /// untouched; 0.24 is a target three quarters stripped when the damage
    /// arrived, which is a fact about the FIGHT and not about the mod that
    /// strips — a build can carry Corrosive and land everything before it
    /// piles.
    pub armor_left_health: f64,
    pub(super) broken: Option<BrokenPool>,
}

/// THE LEDGER OF ONE INSTANCE'S MITIGATION — up to three numbers and every
/// factor behind each.
///
/// Filled by [`TargetState::apply`] only while the combat record is being
/// taken, and read by [`log_damage`] immediately afterwards. It lives on
/// `RunResult` rather than on the return value for the cost reason above:
/// returning it measured +13.3% on a run nobody reads it for.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Breakdown {
    /// The shield-gate window's ×0.05, or 1. Outside every portion because it
    /// is applied to the instance before it is routed anywhere.
    pub gate: f64,
    /// The live armour VALUE the health portion's term read, after strips.
    /// Reported beside the multiplier because "why is my armour term 0.08" is
    /// answered by the value and not by the ratio.
    pub armor_effective: f64,
    /// THE TARGET AS IT STOOD, the instant before this instance landed.
    ///
    /// Taken HERE rather than by each of the nine callers, and that is a cost
    /// decision as much as a tidiness one: `apply` spends the pools and, on a
    /// kill, respawns the body outright, so a caller would have to snapshot on
    /// the line above and hold the value across the call. Nine gated blocks and
    /// nine live 32-byte locals across the hottest call in the engine measure
    /// **+9%** on `one_fight` with every answer unchanged — and `apply` is
    /// already the one place that knows whether anyone is reading.
    pub before: crate::record::TargetAt,
    /// One per pool that took something, in the order the game shows them.
    /// `len` is 0 (nothing landed), 1 (the ordinary case), or 2 (a shield split
    /// the instance) — three is unreachable today and costs nothing to allow.
    pub(super) portions: [Portion; 3],
    pub(super) n_portions: u8,
}

impl Breakdown {
    pub fn portions(&self) -> &[Portion] {
        &self.portions[..self.n_portions as usize]
    }

    pub(super) fn reset(&mut self, gate: f64, armor_effective: f64, before: crate::record::TargetAt) {
        self.gate = gate;
        self.armor_effective = armor_effective;
        self.before = before;
        self.n_portions = 0;
    }

    pub(super) fn push(&mut self, p: Portion) {
        if p.effective <= 0.0 {
            return;
        }
        if let Some(slot) = self.portions.get_mut(self.n_portions as usize) {
            *slot = p;
            self.n_portions += 1;
        }
    }
}
