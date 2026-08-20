//! Secondary Enervate — a secondary-weapon arcane, modeled as a [`Perk`].
//!
//! On the trigger it grants/updates a stacking, weapon-scoped buff in the
//! [`BuffBar`] (the same buff the player sees in the HUD). This perk keeps the
//! private bookkeeping the UI does not show (the big-crit counter and the
//! rate-limit timer); the visible stacks live on the [`Buff`].
//!
//! Data / source of truth: the inline `kind: buff` block in
//! `data/arcanes/secondary/secondary_enervate.yaml` (trigger + rank fields +
//! hit-counting notes in one place)
//! (<https://wiki.warframe.com/w/Secondary_Enervate>). Terminology per
//! `docs/GLOSSARY.md` (flat crit chance, big crit, Hit).
//!
//! Mechanic (status: **unverified** — pending in-game golden test):
//! - On Hit: gain one stack of `+flat_crit_per_stack` **flat crit chance** (an
//!   absolute percentage-point bonus, not scaled by base). A Lato (10% base)
//!   with 7 stacks has `10% + 7×10% = 80%`.
//! - After landing `reset_after_big_crits` big crits (crit tier >= 2), the buff
//!   resets (stacks -> 0, buff removed from the bar).
//! - Both the stack gain and the big-crit counter are rate-limited to
//!   `rate_limit_hz` triggers per second (30/s in game).
//! - Rank only changes `reset_after_big_crits` (1 at rank 0 .. 6 at rank 5);
//!   the per-stack flat crit chance is 10% at every rank.
//!
//! Open questions when calibrating:
//! - Whether a hit's own stack survives when that same hit triggers the reset
//!   (this impl gains the stack first, then resets — to confirm).
//! - Whether the 30/s cap is a min-interval or a rolling window (min-interval).

use crate::buffs::{Buff, BuffBar, BuffScope, Contributions};
use crate::perks::Perk;
use crate::sim::Event;

/// The arcane's maximum rank.
pub const MAX_RANK: u8 = 5;

const BUFF_ID: &str = "secondary_enervate";

/// Flat crit chance gained per stack, at every rank (see data file).
const FLAT_CRIT_PER_STACK: f64 = 0.10;

/// The 30-triggers-per-second rate cap on both stack gain and big-crit counting.
const RATE_LIMIT_HZ: f64 = 30.0;

/// Perk for one equipped Secondary Enervate.
#[derive(Debug, Clone)]
pub struct SecondaryEnervate {
    // Configuration (from rank).
    flat_crit_per_stack: f64,
    reset_after_big_crits: u32,
    min_trigger_interval_seconds: f64,

    // Private bookkeeping (not shown in the UI).
    big_crit_count: u32,
    last_trigger_seconds: Option<f64>,
}

impl SecondaryEnervate {
    /// Build for a specific rank (0..=[`MAX_RANK`]). Panics if `rank > MAX_RANK`.
    pub fn from_rank(rank: u8) -> Self {
        assert!(rank <= MAX_RANK, "rank {rank} exceeds MAX_RANK {MAX_RANK}");
        Self {
            flat_crit_per_stack: FLAT_CRIT_PER_STACK,
            // Rank 0 -> 1 big crit to reset, ... rank 5 -> 6.
            reset_after_big_crits: rank as u32 + 1,
            min_trigger_interval_seconds: 1.0 / RATE_LIMIT_HZ,
            big_crit_count: 0,
            last_trigger_seconds: None,
        }
    }

    /// Put `stacks` on the bar before the run starts.
    ///
    /// The perk reads its own count back off the [`BuffBar`] on every hit, so
    /// this is the whole of what a configured starting pile needs — the ramp
    /// simply continues from it. 0 removes the buff, which is the default: the
    /// arcane is untimed but CONSUMABLE (a big crit wipes it), so a fight you
    /// have just walked into has not got it.
    pub fn seed(&self, stacks: u32, bar: &mut BuffBar) {
        if stacks == 0 {
            bar.remove(BUFF_ID);
            return;
        }
        bar.upsert(Buff {
            id: BUFF_ID.into(),
            scope: BuffScope::Weapon,
            stacks,
            expiry_seconds: None,
            contributions: Contributions {
                flat_crit_chance: stacks as f64 * self.flat_crit_per_stack,
                ..Default::default()
            },
        });
    }

    /// Whether a trigger is allowed at `t_secs` given the 30/s rate cap.
    fn trigger_allowed(&self, t_secs: f64) -> bool {
        match self.last_trigger_seconds {
            None => true,
            // Small epsilon so an interval landing exactly on the cap (e.g. 8
            // ticks at 240 fps == 1/30 s) counts as allowed despite rounding.
            Some(last) => t_secs - last >= self.min_trigger_interval_seconds - 1e-9,
        }
    }
}

/// Defaults to the maximum rank — the sensible default for a real build.
impl Default for SecondaryEnervate {
    fn default() -> Self {
        Self::from_rank(MAX_RANK)
    }
}

impl Perk for SecondaryEnervate {
    fn id(&self) -> &str {
        BUFF_ID
    }

    fn on_event(&mut self, event: &Event, t_secs: f64, bar: &mut BuffBar) {
        let Event::Hit(hit) = event;
        let big_crit = hit.big_crit;

        if !self.trigger_allowed(t_secs) {
            return;
        }
        self.last_trigger_seconds = Some(t_secs);

        // Read current stacks from the buff bar (0 if the buff is absent), gain
        // one on the hit, then apply the reset if this big crit fills the counter.
        let mut stacks = bar.get(BUFF_ID).map_or(0, |b| b.stacks) + 1;
        if big_crit {
            self.big_crit_count += 1;
            if self.big_crit_count >= self.reset_after_big_crits {
                stacks = 0;
                self.big_crit_count = 0;
            }
        }

        if stacks == 0 {
            bar.remove(BUFF_ID);
        } else {
            bar.upsert(Buff {
                id: BUFF_ID.into(),
                scope: BuffScope::Weapon,
                stacks,
                expiry_seconds: None,
                contributions: Contributions {
                    flat_crit_chance: stacks as f64 * self.flat_crit_per_stack,
                    ..Default::default()
                },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::Hit;

    // A time step comfortably larger than the 1/30 s rate cap, so successive
    // hits each count as a distinct trigger.
    const STEP: f64 = 1.0 / 20.0;

    /// ANTI-DRIFT: the hand-written perk must match the arcane yaml's inline
    /// buff block (the data source of truth) — per-stack value, rate limit,
    /// and the rank-scaled reset threshold, at every rank.
    #[test]
    fn from_rank_matches_the_arcane_yaml() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../data/arcanes/secondary/secondary_enervate.yaml");
        let text = std::fs::read_to_string(&path).expect("enervate arcane yaml");
        let v: serde_norway::Value = serde_norway::from_str(&text).expect("parses");
        assert_eq!(
            v.get("perk").and_then(|p| p.as_str()),
            Some("secondary_enervate"),
            "the arcane must reference this perk"
        );
        let eff = &v.get("effects").and_then(|e| e.as_sequence()).expect("effects")[0];
        let f = |k: &str| eff.get(k).and_then(serde_norway::Value::as_f64).unwrap();
        let (r0, rmax) = (f("reset_after_big_crits_rank0"), f("reset_after_big_crits_rankMax"));
        for rank in 0..=MAX_RANK {
            let p = SecondaryEnervate::from_rank(rank);
            // Linear rank0→rankMax over MAX_RANK, like every schema value.
            let expect = r0 + (rmax - r0) * rank as f64 / MAX_RANK as f64;
            assert_eq!(p.reset_after_big_crits, expect.round() as u32, "rank {rank}");
            assert!(approx(p.flat_crit_per_stack, f("rank0")));
            assert!(approx(p.flat_crit_per_stack, f("rankMax")));
            assert!(approx(p.min_trigger_interval_seconds, 1.0 / f("rate_limit_hz")));
        }
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn stacks(bar: &BuffBar) -> u32 {
        bar.get(BUFF_ID).map_or(0, |b| b.stacks)
    }

    #[test]
    fn default_is_max_rank() {
        assert_eq!(SecondaryEnervate::default().reset_after_big_crits, 6);
    }

    #[test]
    fn stacks_accumulate_on_non_big_hits() {
        let mut bar = BuffBar::new();
        let mut e = SecondaryEnervate::default();
        for i in 0..3 {
            e.on_event(&Event::Hit(Hit::default()), i as f64 * STEP, &mut bar);
        }
        assert_eq!(stacks(&bar), 3);
        assert!(approx(bar.total_contributions().flat_crit_chance, 0.30));
    }

    #[test]
    fn rate_cap_limits_to_30_per_second() {
        let mut bar = BuffBar::new();
        let mut e = SecondaryEnervate::default();
        // 10 hits in the very same instant: only the first is a valid trigger.
        for _ in 0..10 {
            e.on_event(&Event::Hit(Hit::default()), 0.0, &mut bar);
        }
        assert_eq!(stacks(&bar), 1);
        // Exactly 1/30 s later, another trigger is allowed.
        e.on_event(&Event::Hit(Hit::default()), 1.0 / 30.0, &mut bar);
        assert_eq!(stacks(&bar), 2);
    }

    #[test]
    fn eight_ticks_at_240fps_is_the_cap_boundary() {
        // 30/s at 240 fps == one trigger every 8 ticks.
        let mut bar = BuffBar::new();
        let mut e = SecondaryEnervate::default();
        let frame_seconds = 1.0 / 240.0;
        e.on_event(&Event::Hit(Hit::default()), 0.0, &mut bar); // stack 1
        e.on_event(&Event::Hit(Hit::default()), 7.0 * frame_seconds, &mut bar); // too soon
        assert_eq!(stacks(&bar), 1);
        e.on_event(&Event::Hit(Hit::default()), 8.0 * frame_seconds, &mut bar); // on cap
        assert_eq!(stacks(&bar), 2);
    }

    #[test]
    fn buff_resets_after_enough_big_crits_at_max_rank() {
        let mut bar = BuffBar::new();
        let mut e = SecondaryEnervate::default(); // resets after 6 big crits
        for i in 0..5 {
            e.on_event(
                &Event::Hit(Hit {
                    big_crit: true,
                    ..Hit::default()
                }),
                i as f64 * STEP,
                &mut bar,
            );
        }
        assert_eq!(stacks(&bar), 5); // 5 big crits: not yet reset

        // 6th big crit fills the counter: buff resets and leaves the bar.
        e.on_event(
            &Event::Hit(Hit {
                big_crit: true,
                ..Hit::default()
            }),
            5.0 * STEP,
            &mut bar,
        );
        assert!(bar.get(BUFF_ID).is_none());
        assert!(approx(bar.total_contributions().flat_crit_chance, 0.0));
    }

    #[test]
    fn rank_zero_resets_on_the_first_big_crit() {
        let mut bar = BuffBar::new();
        let mut e = SecondaryEnervate::from_rank(0); // resets after 1 big crit
        e.on_event(
            &Event::Hit(Hit {
                big_crit: true,
                ..Hit::default()
            }),
            0.0,
            &mut bar,
        );
        assert!(bar.get(BUFF_ID).is_none());
    }

    #[test]
    #[should_panic]
    fn rank_above_max_panics() {
        let _ = SecondaryEnervate::from_rank(MAX_RANK + 1);
    }
}
