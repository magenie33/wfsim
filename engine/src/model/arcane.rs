// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT AN ARCANE'S CARD SAYS — the loader's vocabulary for an arcane effect,
//! beside [`super::ModEffect`] for a mod and [`super::EvoEffect`] for an
//! evolution.

use super::*;

/// One rank-parameterized arcane effect (the loader's vocabulary — every
/// structured kind in data/arcanes; kinds with no single-target sim payload
/// load as `Inert` so the arcane still resolves).
#[derive(Debug, Clone, PartialEq)]
pub enum ArcEffect {
    Buff {
        trigger: ArcTrigger,
        grant: ArcGrant,
        scale: Scale,
        max_stacks: u32,
        duration: f64,
        all_drop: bool,
        one_per_instance: bool,
    },
    /// Scales off a WARFRAME STAT rather than off anything the weapon does:
    /// `per_unit x (stat - above)`, capped at the rank's value, gated on the
    /// player holding at least `min_energy_pct` of their energy pool.
    ///
    /// The neutral Tenno has no frame, so every one of these resolves to zero
    /// until a fight says what the player is wearing — which is the honest
    /// answer, and the reason the stat block exists.
    TennoScaled {
        stat: TennoStat,
        above: f64,
        per_unit: f64,
        min_energy_pct: f64,
        grant: ArcGrant,
        cap: Scale,
    },
    /// Relative crit chance under a non-simmed condition (Overcharge).
    CondCritChance(Scale),
    /// Outburst: relative CC and CD per combo tier consumed (assumed-max =
    /// full 12x combo), duration-limited buff on a non-simmed trigger.
    CondCritChanceStacked { scale: Scale, max_stacks: u32 },
    CondCritDamageStacked { scale: Scale, max_stacks: u32 },
    /// Cascadia Accuracy: relative crit chance on weak-point hits (on-roll
    /// buff — assumed-max only).
    WeakpointCritChance(Scale),
    /// Surge: final damage-multiplier cap (stored as bonus; assumed-max).
    FinalDamageCap(Scale),
    /// Fractalized Reset: reload speed on a trigger the arena cannot fire (an
    /// ability cast). The GRANT is modeled, so it follows the house policy for
    /// non-simmed triggers — assumed-max only, a no-op under `Emergent`.
    CondReloadSpeed(Scale),
    HeadshotMultiplier { value: f64, unlocks_at: u32 },
    ReloadSpeed { value: f64, unlocks_at: u32 },
    PerColdDamage { scale: Scale, max_stacks: u32 },
    /// AN ADDED ELEMENT, at every stack it can hold — see
    /// [`ArcaneFx::added_elements`].
    AddedElement { element: crate::rules::damage::DamageType, scale: Scale, max_stacks: u32 },
    FlatDamageOnStatus(Scale),
    EncumberChance(Scale),
    ColdBurst { scale: Scale, radius0: f64, radius1: f64 },
    OverguardDamage(Scale),
    AmmoEfficiency(Scale),
    /// PRIMARY COMPRESSION, whose worth is a property of the WEAPON: it shrinks
    /// the explosion to a fifth while aiming and pays per metre given up. Both
    /// ramps are per METRE, so neither is a number until a radius is known —
    /// which is why the fold happens in `build::loadout::resolve_for` (it has the
    /// modded radius and the weapon's own row) rather than here.
    CompressionDamage(Scale),
    CompressionAmmoEfficiency(Scale),
    /// Kinship: per ally-affecting buff — team context, uncapped: inert in
    /// the sim, but its per-rank value still renders in the description.
    PerAllyCritChance(Scale),
    /// Irradiate: % of the hit damage echoed in a radius — AoE, inert in
    /// the single-target sim; values render in the description.
    AoeEcho { scale: Scale, radius0: f64, radius1: f64, needs_radiation: u32 },
    /// Melee Influence: a chance, on a melee Electricity STATUS, to open a
    /// window in which every elemental status this weapon applies is applied
    /// again to everything within `radius` of the body it struck.
    ///
    /// Three ramps rather than one. The chance is flat at every rank — the one
    /// arcane in the pool whose headline number does not climb — and what the
    /// ranks buy is the RADIUS and the CLOCK.
    StatusSpread { scale: Scale, radius0: f64, radius1: f64, seconds0: f64, seconds1: f64 },
    /// `kind: unmodeled` — an effect whose payload is OUT OF THE SIM'S WORLD
    /// (Warframe armor/energy, enemy behaviour, a mechanic still to be built).
    /// No sim payload, but it OWNS a description `X`: its per-rank value still
    /// has to render, or the config page shows a literal "X". The yaml `note`
    /// is what the panel says instead of a computed line.
    /// An effect the sim does not compute, and WHY IT DOES NOT is two very
    /// different answers:
    ///
    /// - `Unmodeled` — real damage we have not built yet. A todo.
    /// - `OutOfScope` — it acts on something this simulator does not have:
    ///   Warframe energy, enemy behaviour, traversal, reviving. Never a todo,
    ///   because building it would not move a single damage figure.
    ///
    /// Telling a player "not modelled" for both makes the whole app look
    /// unfinished when four of the seven cases are the model's own edge.
    /// Neither carries text: the explanation belongs in a YAML comment.
    /// Primary Debilitate: on a damage instance landing a COMBINED status that
    /// brings the target TO [`DEBILITATE_STACKS`] — this stack counted, so at
    /// nine the shot that makes it ten fires (M34) — a chance to also inflict
    /// one of its two components, chosen 50/50, one stack, dealt as its own
    /// damage INSTANCE (which is why the status it leaves carries the faction
    /// bonus a third time).
    Debilitate(Scale),
    /// PAX CHARGE: the magazine becomes a rechargeable BATTERY.
    ///
    /// The whole mechanic is a reload-speed bonus plus a rate the WEAPON states,
    /// so this effect carries only the first — "recharge rate is **not**
    /// affected by mods or abilities. Recharge rate depends on the chamber
    /// part". A weapon with no rate of its own gets nothing.
    RechargeableMagazine { scale: Scale },
    Unmodeled { scale: Scale },
    OutOfScope { scale: Scale },
    /// No single-target sim payload and NO description number of its own
    /// (recoil, combo duration, overguard-on-damage: the description states
    /// those literally). Kept so the arcane loads.
    Inert(String),
    /// An effect this loader deliberately does NOT build, because something
    /// else already does. Secondary Enervate's on-hit trigger is implemented
    /// in the perk layer, so filing it with the effects that do nothing would
    /// put "partly modelled" on a card that is fully modelled — a lie in the
    /// other direction, and the more expensive one.
    Elsewhere(String),
}

/// A `condition:` ON AN ARCANE EFFECT, typed — and the point of typing it is
/// that an unknown one is LOUD.
///
/// `data/arcanes/` carries four. Three are TENNO states this sim does not model
/// (sliding or aim-gliding, overshields up, buffing an ally), and for those the
/// house reading is assumed-max: documented, optimistic, and disclosed on the
/// card. The fourth is a state of the TARGET, which the engine has tracked
/// since it had statuses at all — and the loader read neither, because it read
/// `trigger`/`grants` and the per-rank numbers and nothing else. So Secondary
/// Irradiate echoed off a target with no Radiation on it, which is what a
/// player reported.
///
/// The fix is not "read this one string". It is that a condition the loader
/// does not understand can no longer be dropped in silence: this returns
/// `Unknown`, and a test walks the whole roster refusing one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArcCondition {
    /// The body being hit is at N stacks of Radiation. SIMULATED.
    TargetRadiationStacks(u32),
    /// A Tenno or squad state this sim does not model. ASSUMED satisfied, and
    /// the arcane's own file says so.
    AssumedTennoState,
    /// Something nobody has taught this loader. Never silently ignored.
    Unknown(String),
}

/// A per-rank value: `rank0`→`rankMax` linear unless an explicit non-linear
/// `ranks:` table overrides it (Kinship, Outburst, Cryogenic).
#[derive(Debug, Clone, PartialEq)]
pub struct Scale {
    pub rank0: Option<f64>,
    pub rank_max: f64,
    pub ranks: Option<Vec<f64>>,
}

impl Scale {
    pub fn at(&self, rank: u32, max_rank: u32) -> f64 {
        if let Some(t) = &self.ranks {
            return t[(rank as usize).min(t.len().saturating_sub(1))];
        }
        let r0 = match self.rank0 {
            Some(v) => v,
            None => return self.rank_max,
        };
        if max_rank == 0 {
            return self.rank_max;
        }
        r0 + (self.rank_max - r0) * rank.min(max_rank) as f64 / max_rank as f64
    }
}
