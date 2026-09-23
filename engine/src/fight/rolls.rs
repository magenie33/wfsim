use super::*;

/// Devouring Attrition's multiplier for ONE damage instance: a
/// non-critical instance (tier 0) rolls `chance` for `1 + bonus`; a
/// critical instance is never eligible. Its own multiplicative bracket
/// (wiki: "multiplicative to base damage bonuses such as Hornet Strike"),
/// and it applies to the radial part too ("Affects both forms").
pub(super) fn noncrit_mult(spec: Option<(f64, f64)>, tier: u32, rng: &mut Rng) -> f64 {
    match spec {
        Some((chance, bonus)) if tier == 0 && rng.chance(chance) => 1.0 + bonus,
        _ => 1.0,
    }
}

/// A mod SET may promote a hit by one critical tier (Vigilante: 5% per
/// equipped member). The wiki is explicit that it "triggers exclusively on
/// critical hits" — a normal hit is never promoted, so tier 0 is untouched.
/// The tier formula does the rest: crit_multiplier = 1 + tier x (cd - 1), so one
/// extra tier is worth exactly one more crit-damage step.
pub(super) fn upgrade_crit_tier(tier: u32, chance: f64, rng: &mut Rng) -> u32 {
    if tier >= 1 && chance > 0.0 && rng.chance(chance) {
        tier + 1
    } else {
        tier
    }
}

/// Roll a critical tier for an effective crit chance that may exceed 1.0.
pub(super) fn roll_crit_tier(effective_cc: f64, rng: &mut Rng) -> u32 {
    let guaranteed = effective_cc.floor().max(0.0);
    let extra_chance = effective_cc - guaranteed;
    guaranteed as u32 + rng.chance(extra_chance) as u32
}

/// Pick the body part a shot lands on, by normalized aim weight.
/// WHERE AN INSTANCE OF AN ATTACK THE PLAYER DOES NOT AIM LANDS.
///
/// [`pick_part`] draws against the scenario's aim weights, which is a statement
/// about the player. Some attacks are not pointed at anything: the Grimoire
/// throws an orb that drifts and *"shock[s] 1 enemy within 6 meters of it every
/// 1 second"*, choosing a body itself, six times. For those the weapon states a
/// flat chance of finding a weak point and this reads it.
///
/// ONE DRAW, exactly as `pick_part` takes one, so a weapon that states nothing
/// consumes the stream identically to before this existed.
///
/// It returns a real [`BodyPart`], not a multiplier — which is the whole reason
/// it is written this way. Everything downstream of a landing instance asks the
/// part its own questions (is it a head for on-headshot buffs, is it eligible
/// for the critical-location fold-in, what is it worth), and handing back a
/// bare number would have made an unaimed hit a second kind of hit that
/// answered some of them and not others.
pub(super) fn unaimed_part<'a>(parts: &'a [BodyPart], head_chance: f64, rng: &mut Rng) -> &'a BodyPart {
    let head = rng.next_f64() < head_chance;
    parts
        .iter()
        .find(|p| p.is_head == head)
        .or_else(|| parts.iter().find(|p| !p.is_head))
        .unwrap_or_else(|| parts.last().expect("dummy needs at least one body part"))
}

pub(super) fn pick_part<'a>(parts: &'a [BodyPart], rng: &mut Rng) -> &'a BodyPart {
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

/// Can a shot be taken right now? This — not "is the magazine empty" — gates
/// the reload: the weapon reloads exactly when it CANNOT fire. The rule is
/// `cost <= ceil(current)`, floored at zero, and both measured facts here are
/// special cases of it.
///
/// A REMAINDER SMALLER THAN THE SHOT still fires (M14): 0.25 left pays a
/// full-cost shot and overdraws the counter negative. The debt is bounded to
/// (−1, 0], so `reload_draw` brings the magazine back at `capacity − 0.75`.
///
/// A SHOT THAT COSTS NOTHING needs no round at all — the Dual Toxocyst case,
/// where the last round headshots and that kill arms Frenzy's +100% ammo
/// efficiency. `0 <= ceil(anything)` holds on an empty magazine, so no
/// `free_shot` flag is needed: the free shot is not the fundamental thing, the
/// COST is.
///
/// What the ceiling ADDS is the case ABOVE one round: seven left and a shot
/// costing ten reloads, where "anything left" fires and lands on −3, a debt one
/// whole-round draw cannot clear. The two tests agree at or below one round
/// (`ceil(x) >= 1` on any positive magazine), so only a cost above one exposes
/// it — the Larkspur Prime's alt-fire costs TEN.
pub(super) fn can_fire(magazine: f64, cost: f64) -> bool {
    cost <= (magazine - 1e-9).max(0.0).ceil() + 1e-9
}

/// Take `want` rounds out of `reserve`, or all that is left of it.
///
/// The reserve is ONE pool for the whole weapon — both forms of an Incarnon
/// cycle, and an Arch-Gun's alt fire, draw from the same supply. Written once
/// because it was written zero times inside the cycle: every draw there was
/// free until 2026-08-04, which made the Infinite-ammo setting a no-op on every
/// Incarnon weapon (the setting has to be adjustable).
pub(super) fn draw_from(reserve: &mut f64, infinite: bool, want: f64) -> f64 {
    if infinite {
        return want;
    }
    let take = want.min(*reserve);
    *reserve -= take;
    take
}

/// How many WHOLE rounds a reload moves out of reserve — measured, on a
/// 5-round magazine:
///
/// | current | draw | after |
/// | --- | --- | --- |
/// | 1.50 | `floor(3.50)` = 3 | 4.50 |
/// | 3.25 | `floor(1.75)` = 1 | 4.25 |
/// | 4.25 | `floor(0.75)` = 0 | 4.25 — the reload is refused outright |
///
/// The refusal at 4.25 is visible in game as the magazine reading FULL (the HUD
/// ceilings it to 5), the same rounding that made M14 readable.
///
/// It subsumes the reload-from-empty case: a shot can only overdraw by less
/// than one round, so `current` is in (−1, 0] there and the draw is a full
/// `capacity` — which is how a −0.75 counter comes back at 4.25. And it is the
/// GLOBAL rule: an Incarnon transform's auto-reload runs on the same mechanism
/// rather than a separate "fill to full".
pub(super) fn reload_draw(capacity: f64, current: f64) -> f64 {
    (capacity - current).floor().max(0.0)
}

/// The ammo efficiency in force right now: the sum of every source, CAPPED.
///
/// Sources stack ADDITIVELY — VERBATIM (wiki `Ammo`): *"Sources of ammo
/// efficiency stack additively with each other except for Energized Munitions,
/// which stacks multiplicatively."* Energized Munitions is a Warframe ability
/// and out of scope; if it is ever modelled it must MULTIPLY, not join this sum.
///
/// **The cap is 100% and it is a real ceiling, not a clamp of convenience**: a shot can cost nothing, never less. Stacking past 100%
/// buys nothing and in particular never starts REFUNDING ammo, so the magazine
/// cannot climb while firing.
///
/// Charge-backed magazines are outside the ammo economy entirely and take no
/// efficiency at all, which is why `applies` short-circuits to zero.
/// Death Knell's ammo half: the efficiency it grants while ANY stack is up.
pub(super) fn weakpoint_ammo(
    spec: Option<crate::model::WeakpointStacksSpec>,
    pile: &mut LiveStacks,
    t: f64,
) -> f64 {
    spec.map_or(0.0, |w| {
        if pile.current(t, w.duration_seconds) > 0 { w.ammo_efficiency } else { 0.0 }
    })
}

/// DOES A KILL HERE LEAVE ONE STANDING? The weapon has to say so, and the body
/// has to be inside the range the card names.
pub(super) fn leaves_one(active: &FightParams, from: crate::rules::space::Vec2, body: crate::rules::space::Vec2) -> bool {
    active.spawn_on_kill
        .is_some_and(|g| (body.x - from.x).hypot(body.y - from.y) <= g.range_m)
}

pub(super) fn ammo_efficiency(
    applies: bool,
    bar: f64,
    arcane_static: f64,
    arcane_live: f64,
    ability: f64,
) -> f64 {
    if !applies {
        return 0.0;
    }
    let others = (bar + arcane_static + arcane_live).clamp(0.0, 1.0);
    // A WARFRAME ABILITY'S SHARE MULTIPLIES, it does not add: "Stacks
    // multiplicatively with other sources of Ammo Efficiency" (wiki, Energized
    // Munitions). The thing that multiplies is the COST, so 75% on top of 50%
    // is 1 - 0.25 x 0.5 = 87.5% and not the 125% adding would have produced.
    // Everything else here adds, which is what those sources do among
    // themselves and what this function was written for.
    (1.0 - (1.0 - others) * (1.0 - ability)).clamp(0.0, 1.0)
}
