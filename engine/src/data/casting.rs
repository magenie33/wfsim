// SPDX-License-Identifier: AGPL-3.0-or-later
//! **WHAT THE FRAME AND ITS OPERATOR DO, DECIDED AS THE FIGHT RUNS** —
//! docs/BUFFS.md §"Cast, or assumed up".
//!
//! Every planned rule of the action list (`Apl::planned`) is asked BETWEEN
//! SHOTS, top down, whether it acts now ([`FrameRuntime::act`]): a cast, the
//! summoning of an Exalted weapon, an Operator trip. Asked in the fight rather
//! than laid out before it, because what decides them happens in it — a melee
//! kill that lengthens Warcry (Eternal War) moves the moment it lapses, and a
//! kill that adds a Molt Augmented stack moves what the next cast snapshots.
//!
//! **A WARFRAME BUFF IS A SNAPSHOT** (M105). A cast reads Ability Strength at
//! the instant it is cast and keeps that number for its whole window; a buff
//! that lapses later does not reach back into it. So each cast is its own entry,
//! resolved at its own strength, and an Exalted weapon's damage is the strength
//! its summoning cast read.

use std::sync::{Arc, Mutex};

use crate::data::abilities::{self, AbilityPick, ActiveAbility, Caster, CAST_SECONDS_UNMEASURED};
use crate::data::apl::{Action, Apl, When};
use crate::data::warframes::{focus_school, FrameEffect, FrameStat, NodeTrigger};

/// **HOW LONG AN OPERATOR TRIP TAKES**: Transference out and back, 1 s (M105),
/// plus a Chained Sling and the school's ability where the trip has them, which
/// are not measured yet. None of it is modded — casting speed does not touch
/// Transference.
pub const TRANSFERENCE_SECONDS: f64 = 1.0;
pub const CHAINED_SLING_SECONDS_UNMEASURED: f64 = 1.0;
pub const OPERATOR_ABILITY_SECONDS_UNMEASURED: f64 = 1.0;

/// **THE WIELDER'S ARCANES THAT MOVE A CAST** — whose number depends on what the
/// frame did before it, so only the fight can spend them.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CastArcanes {
    /// Molt Vigor: added to the next Warframe cast after an Operator ability.
    #[serde(default)]
    pub after_operator_ability: f64,
    /// Arcane Power Ramp: `(strength per stack, max stacks)`.
    #[serde(default)]
    pub per_cast_stack: Option<(f64, u32)>,
    /// Molt Augmented: `(strength per stack, max stacks, stacks it opens with)`.
    /// The opening stacks are already in the frame's own strength; a kill in
    /// the fight adds one more, "Kills from all sources" (W`Molt_Augmented`).
    #[serde(default)]
    pub per_kill: Option<(f64, u32, u32)>,
}

impl CastArcanes {
    /// A frame with none of them.
    pub const NONE: CastArcanes = CastArcanes { after_operator_ability: 0.0, per_cast_stack: None, per_kill: None };
}

/// A stretch of time Ability Strength is raised, and what raised it.
#[derive(Debug, Clone, PartialEq)]
pub struct StrengthWindow {
    /// The Focus node's id.
    pub from: String,
    pub starts_at_seconds: f64,
    pub ends_at_seconds: f64,
    /// A fraction, added to the frame's strength (1.0 = 100%).
    pub bonus: f64,
}

impl StrengthWindow {
    fn live_at(&self, t: f64) -> bool {
        self.starts_at_seconds <= t && t < self.ends_at_seconds
    }
}

/// When the Exalted weapon came out, and the strength its cast snapshotted.
/// `at_seconds` is `f64::NEG_INFINITY` for one assumed out before the fight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summon {
    pub at_seconds: f64,
    pub strength: f64,
}

/// An ability pick that owns its strings, so a spec can outlive the request.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub id: String,
    pub duration_seconds: Option<f64>,
    pub element: Option<String>,
}

/// **WHO IS DOING IT** — the frame's stats, its picks, its Operator and its
/// arcanes, and the rules it acts on. Built once per fight; every run opens a
/// [`FrameRuntime`] on it.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameSpec {
    /// The planned rules, in list order.
    pub rules: Vec<(Action, When)>,
    /// The frame's own Ability Strength, before anything the fight earns.
    pub strength: f64,
    pub duration: f64,
    pub efficiency: f64,
    pub casting_speed_bonus: f64,
    pub augments: Vec<String>,
    pub picks: Vec<Pick>,
    /// The picks resolved at the frame's own strength — the family contest
    /// already settled.
    pub assumed: Vec<ActiveAbility>,
    /// The linked Operator's active Focus school.
    pub school: String,
    /// The ability that summons the weapon being fought with, if it is Exalted.
    pub summoned_by: Option<String>,
    pub weapon_class: &'static str,
    pub weapon_slot: &'static str,
    pub arcanes: CastArcanes,
}

impl FrameSpec {
    /// A named ability with a stated price — the only kind the list casts.
    fn castable(&self, id: &str) -> Option<&ActiveAbility> {
        self.assumed.iter().find(|a| a.id == id && a.energy_cost.is_some())
    }

    /// **THE FIGHT'S OPENING SET**: every pick the list does not cast, assumed
    /// up as it always was. What it casts opens down and is cast in the fight.
    pub fn opening(&self) -> Vec<ActiveAbility> {
        let named = |a: &ActiveAbility| {
            self.rules.iter().any(|(act, _)| matches!(act, Action::Cast { ability } if ability == a.id))
        };
        self.assumed.iter().filter(|a| !(named(a) && a.energy_cost.is_some())).cloned().collect()
    }

    /// Does a fight under this spec need the frame at all? Only when the list
    /// plans something, or an assumed window can grow (Eternal War).
    pub fn needed(&self) -> bool {
        !self.rules.is_empty() || self.assumed.iter().any(|a| a.extend_per_melee_kill_seconds > 0.0)
    }

    /// **THE SUMMON, BEFORE THE FIGHT**: the opening burst run with nothing
    /// killed, because the weapon it summons has not attacked yet. It is the
    /// WEAPON's number and so has to be known before its build is resolved.
    pub fn summon(self: &Arc<Self>) -> Summon {
        let mut rt = FrameRuntime::new(self.clone(), Arc::new(Mutex::new(self.opening())));
        let mut t = 0.0;
        for _ in 0..(self.rules.len() * 2 + 1) {
            match rt.act(t, 0) {
                Some(s) => t += s,
                None => break,
            }
        }
        rt.summon
    }
}

/// The school's nodes an action earns, as `(id, seconds, strength bonus)`.
fn earned_by(school: &str, trigger: NodeTrigger) -> Vec<(String, f64, f64)> {
    focus_school(school).map_or_else(Vec::new, |s| {
        s.nodes
            .iter()
            .filter_map(|n| {
                let (tr, secs) = n.trigger?;
                (tr == trigger).then(|| {
                    let bonus = n
                        .effects
                        .iter()
                        .filter_map(|e| match e {
                            FrameEffect::Bonus(FrameStat::AbilityStrength, v) => Some(*v),
                            _ => None,
                        })
                        .sum();
                    (n.id.clone(), secs, bonus)
                })
            })
            .collect()
    })
}

/// **ONE RUN'S FRAME**: the windows it has opened, what the next cast gains,
/// and which once-only rules are spent. `live` is shared with the run's params,
/// which is how every reader of an ability window sees a cast made mid-fight.
pub struct FrameRuntime {
    spec: Arc<FrameSpec>,
    live: Arc<Mutex<Vec<ActiveAbility>>>,
    strength: Vec<StrengthWindow>,
    /// Molt Vigor, held from an Operator ability until the next cast spends it.
    vigor: f64,
    /// Power Ramp's stacks, and the ability that earned the last one.
    ramp: (u32, Option<String>),
    once_done: Vec<bool>,
    kill_mark: u32,
    pub summon: Summon,
}

/// HOW OFTEN ONE TURN MAY ACT — a guard, so a rule whose action does not clear
/// its own condition cannot hold the clock still.
const ACTS_PER_CALL: usize = 16;

impl FrameRuntime {
    pub fn new(spec: Arc<FrameSpec>, live: Arc<Mutex<Vec<ActiveAbility>>>) -> Self {
        let n = spec.rules.len();
        let summon = Summon { at_seconds: f64::NEG_INFINITY, strength: spec.strength };
        FrameRuntime { spec, live, strength: Vec::new(), vigor: 0.0, ramp: (0, None), once_done: vec![false; n], kill_mark: 0, summon }
    }

    /// The frame's Ability Strength at `t` with `kills` made so far: its own,
    /// every strength window live then — a source counts ONCE however many of
    /// its windows overlap — and the Molt Augmented stacks the fight has added.
    pub fn strength_now(&self, t: f64, kills: u32) -> f64 {
        let mut by_source: Vec<(&str, f64)> = Vec::new();
        for w in self.strength.iter().filter(|w| w.live_at(t)) {
            match by_source.iter_mut().find(|(f, _)| *f == w.from) {
                Some(slot) => slot.1 = slot.1.max(w.bonus),
                None => by_source.push((&w.from, w.bonus)),
            }
        }
        let molt = self.spec.arcanes.per_kill.map_or(0.0, |(per, max, open)| {
            per * f64::from((open + kills).min(max) - open.min(max))
        });
        self.spec.strength + by_source.iter().map(|(_, b)| b).sum::<f64>() + molt
    }

    /// Power Ramp's share of a cast of `id` now, without spending it.
    fn ramp_bonus(&self, id: &str) -> f64 {
        match self.spec.arcanes.per_cast_stack {
            Some((per, _)) if self.ramp.1.as_deref() != Some(id) => per * f64::from(self.ramp.0),
            _ => 0.0,
        }
    }

    /// Spend what the next cast gains: Molt Vigor, and Power Ramp — THE SAME
    /// ABILITY TWICE RUNNING DROPS IT TO ZERO AT ONCE, arming nothing (M105).
    fn spend_cast_bonus(&mut self, id: &str) -> f64 {
        let b = std::mem::take(&mut self.vigor) + self.ramp_bonus(id);
        if let Some((_, max)) = self.spec.arcanes.per_cast_stack {
            self.ramp.0 = if self.ramp.1.as_deref() == Some(id) { 0 } else { (self.ramp.0 + 1).min(max) };
            self.ramp.1 = Some(id.to_string());
        }
        b
    }

    /// **EVERY STACKING SOURCE IS FULL** — the gate on recasting for strength:
    /// a cast one stack in would throw away a window for nothing. False when
    /// the frame carries none, so the gate never opens on a plain frame.
    fn stacks_full(&self, id: &str, kills: u32) -> bool {
        let molt = self.spec.arcanes.per_kill.map(|(_, max, open)| open + kills >= max);
        let ramp = self.spec.arcanes.per_cast_stack.map(|(_, max)| self.ramp.1.as_deref() != Some(id) && self.ramp.0 >= max);
        match (molt, ramp) {
            (None, None) => false,
            (m, r) => m.unwrap_or(true) && r.unwrap_or(true),
        }
    }

    /// The live window of `id` — the one the latest cast opened — if any.
    fn window(&self, id: &str, t: f64) -> Option<(f64, f64)> {
        let live = self.live.lock().expect("one run, one thread");
        live.iter()
            .filter(|a| a.id == id && a.live_at(t))
            .max_by(|a, b| a.starts_at_seconds.total_cmp(&b.starts_at_seconds))
            .map(|a| (a.ends_at_seconds - t, a.snapshot_strength))
    }

    /// How long the sling's earned windows have left at `t`; `None` when the
    /// school earns nothing from a sling, which leaves nothing to keep up.
    fn sling_remains(&self, t: f64) -> Option<f64> {
        let earned = earned_by(&self.spec.school, NodeTrigger::OperatorSling);
        if earned.is_empty() {
            return None;
        }
        Some(
            self.strength
                .iter()
                .filter(|w| earned.iter().any(|(id, _, _)| *id == w.from) && w.live_at(t))
                .map(|w| w.ends_at_seconds - t)
                .fold(0.0, f64::max),
        )
    }

    /// **DOES RULE `k` ACT NOW?**
    fn wants(&self, k: usize, t: f64, kills: u32) -> bool {
        let (action, when) = &self.spec.rules[k];
        let once = !self.once_done[k];
        let kept = |remains: f64| match when {
            When::Once => once,
            When::Always | When::StrengthGain => remains <= 0.0,
            // DOWN, OR THIS MANY SECONDS BEFORE — a lead of 0 is "when down".
            When::BuffRemainsUnder { seconds, .. } => remains <= 0.0 || remains < *seconds,
            _ => false,
        };
        match action {
            Action::Operator { sling, .. } => match (sling, self.sling_remains(t)) {
                (true, Some(r)) => kept(r),
                // A TRIP THAT EARNS NO WINDOW has nothing to keep up: once.
                _ => once,
            },
            Action::Cast { ability } if self.spec.summoned_by.as_deref() == Some(ability.as_str()) => once,
            Action::Cast { ability } => {
                if self.spec.castable(ability).is_none() {
                    return false;
                }
                let (remains, snapshot) = self.window(ability, t).unwrap_or((0.0, f64::NEG_INFINITY));
                // A STRONGER CAST, ONCE EVERY STACK IS IN — a recast one stack
                // in would throw away a window for nothing.
                let stronger = matches!(when, When::StrengthGain)
                    && self.stacks_full(ability, kills)
                    && self.strength_now(t, kills) + self.vigor + self.ramp_bonus(ability) > snapshot + 1e-9;
                kept(remains) || stronger
            }
            _ => false,
        }
    }

    /// Do rule `k` at `t`; the seconds it keeps the weapon from attacking.
    fn perform(&mut self, k: usize, t: f64, kills: u32) -> f64 {
        self.once_done[k] = true;
        let action = self.spec.rules[k].0.clone();
        match action {
            Action::Operator { sling, ability } => {
                let secs = TRANSFERENCE_SECONDS
                    + if sling { CHAINED_SLING_SECONDS_UNMEASURED } else { 0.0 }
                    + if ability { OPERATOR_ABILITY_SECONDS_UNMEASURED } else { 0.0 };
                if ability {
                    self.vigor = self.spec.arcanes.after_operator_ability;
                }
                if sling {
                    let back = t + secs;
                    for (from, s, bonus) in earned_by(&self.spec.school, NodeTrigger::OperatorSling) {
                        self.strength.push(StrengthWindow { from, starts_at_seconds: back, ends_at_seconds: back + s, bonus });
                    }
                }
                secs
            }
            Action::Cast { ability } if self.spec.summoned_by.as_deref() == Some(ability.as_str()) => {
                let extra = self.spend_cast_bonus(&ability);
                self.summon = Summon { at_seconds: t, strength: self.strength_now(t, kills) + extra };
                CAST_SECONDS_UNMEASURED / (1.0 + self.spec.casting_speed_bonus)
            }
            Action::Cast { ability } => self.cast(&ability, t, kills),
            _ => 0.0,
        }
    }

    /// CAST A BUFF: resolved at the strength of this instant, for the ability's
    /// own duration times the frame's, replacing the window it had.
    fn cast(&mut self, id: &str, t: f64, kills: u32) -> f64 {
        let spec = self.spec.clone();
        let Some(def) = abilities::get(id) else { return 0.0 };
        let Some(pick) = spec.picks.iter().find(|p| p.id == id) else { return 0.0 };
        let strength = self.strength_now(t, kills) + self.spend_cast_bonus(id);
        // THE WINDOW IS THE CARD'S DURATION × THE FRAME'S, not a typed one:
        // a cast lasts what the build makes it last.
        let window = def.duration_seconds.map(|d| d * spec.duration).or(pick.duration_seconds).unwrap_or(f64::INFINITY);
        let augments: Vec<&str> = spec.augments.iter().map(String::as_str).collect();
        let caster = Caster {
            strength,
            duration: spec.duration,
            efficiency: spec.efficiency,
            casting_speed_bonus: spec.casting_speed_bonus,
            augments: &augments,
        };
        let one = AbilityPick { id: def.id, duration_seconds: Some(window), element: pick.element.as_deref() };
        let Some(mut cast) = abilities::resolve(&[one], &caster, spec.weapon_class, spec.weapon_slot).into_iter().next()
        else {
            return 0.0;
        };
        cast.starts_at_seconds = t;
        cast.ends_at_seconds = t + window;
        // THE CAP IS THIS CAST'S: "up to a maximum of double the ability's
        // duration after mods" (W`Eternal_War`), counted from when it opened.
        cast.extend_cap_seconds += t;
        let busy = if cast.interrupts_fire { cast.cast_seconds } else { 0.0 };
        let mut live = self.live.lock().expect("one run, one thread");
        for old in live.iter_mut().filter(|a| a.id == id && a.ends_at_seconds > t) {
            old.ends_at_seconds = t;
        }
        live.push(cast);
        busy
    }

    /// **A MELEE KILL LENGTHENS THE WINDOW IT LANDS IN** (Eternal War): every
    /// window with an augment that grows, live now, by the kills since the last
    /// call, up to its cap. A kill while it is down lengthens nothing.
    fn grow(&mut self, t: f64, kills: u32) {
        let fresh = kills.saturating_sub(self.kill_mark);
        self.kill_mark = kills;
        if fresh == 0 || self.spec.weapon_slot != "melee" {
            return;
        }
        let mut live = self.live.lock().expect("one run, one thread");
        for a in live.iter_mut().filter(|a| a.extend_per_melee_kill_seconds > 0.0 && a.live_at(t)) {
            a.ends_at_seconds =
                (a.ends_at_seconds + f64::from(fresh) * a.extend_per_melee_kill_seconds).min(a.extend_cap_seconds);
        }
    }

    /// **THE FRAME'S TURN, between shots**: grow what the kills grew, then the
    /// first rule that acts now acts. Returns the seconds the weapon waits, or
    /// `None` when nothing acted; the caller asks again until it is `None`.
    pub fn act(&mut self, t: f64, kills: u32) -> Option<f64> {
        self.grow(t, kills);
        (0..self.spec.rules.len()).find(|&k| self.wants(k, t, kills)).map(|k| self.perform(k, t, kills))
    }

    /// Every act due at `t`, back to back; the time the weapon is free again.
    pub fn act_all(&mut self, mut t: f64, kills: u32) -> f64 {
        for _ in 0..ACTS_PER_CALL {
            match self.act(t, kills) {
                Some(s) => t += s,
                None => break,
            }
        }
        t
    }
}

/// THE SPEC A REQUEST'S LIST AND FRAME MAKE, with the planned rules pulled out.
#[allow(clippy::too_many_arguments)]
pub fn spec(
    apl: &Apl,
    caster: &Caster<'_>,
    picks: &[AbilityPick<'_>],
    assumed: &[ActiveAbility],
    school: &str,
    summoned_by: Option<&str>,
    weapon_class: &'static str,
    weapon_slot: &'static str,
    arcanes: &CastArcanes,
) -> FrameSpec {
    FrameSpec {
        rules: apl.0.iter().filter(|r| r.action.is_planned()).map(|r| (r.action.clone(), r.when.clone())).collect(),
        strength: caster.strength,
        duration: caster.duration,
        efficiency: caster.efficiency,
        casting_speed_bonus: caster.casting_speed_bonus,
        augments: caster.augments.iter().map(|a| (*a).to_string()).collect(),
        picks: picks
            .iter()
            .map(|p| Pick { id: p.id.to_string(), duration_seconds: p.duration_seconds, element: p.element.map(str::to_string) })
            .collect(),
        assumed: assumed.to_vec(),
        school: school.to_string(),
        summoned_by: summoned_by.map(str::to_string),
        weapon_class,
        weapon_slot,
        arcanes: arcanes.clone(),
    }
}

#[cfg(test)]
mod tests;
