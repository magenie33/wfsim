//! Parazon Mercy window (wiki `Parazon` §Mercy; docs/MECHANICS.md §8).
//!
//! Pure predicate: given the unit's attributes and current state, is the
//! Mercy prompt available? Rules (unverified until golden-tested):
//! - Only **mercy-eligible** units (per-enemy data flag; list on the
//!   Parazon page).
//! - Base window: **40%** of total health; **60%** on Corpus with all
//!   shields removed; **80%** on Eximus.
//! - Impact (Stagger) stacks: **+8%** each, capped at **80%** (**100%** on
//!   Corpus and Eximus).
//! - Above level **150**: window shrinks **1% per 5 levels**, floor **10%**.
//!
//! Hard gates (both required before any window math):
//! - **Shields must be fully depleted** — MEASURED (2026-07-24, user):
//!   1 HP with 10k shields shows no prompt. The "60% on Corpus with all
//!   their shields removed" wording is a gate, not a bonus condition.
//! - **Overguard must be gone** — wiki Overguard patch history states you
//!   "cannot Mercy Kill enemies with Overguard active".
//!
//! With the gates in place, a Corpus unit inside the window always has
//! shields at zero, so its base is simply 60% / cap 100%.

/// Static + dynamic facts about the target relevant to Mercy.
#[derive(Debug, Clone, Copy)]
pub struct MercyContext {
    /// Per-unit data flag (wiki Parazon heavy-unit list).
    pub mercy_eligible: bool,
    pub eximus: bool,
    /// Corpus-faction unit (60% base / 100% cap once inside the window).
    pub corpus: bool,
    pub level: u32,
    /// Current Impact (Stagger) stacks on the target's DebuffBar.
    pub impact_stacks: u32,
    /// Current shield points — must be 0 for the prompt to exist.
    pub shields: f64,
    /// Current overguard points — must be 0 for the prompt to exist.
    pub overguard: f64,
}

/// The Mercy window as a fraction of total health (0 if not eligible).
pub fn mercy_threshold(ctx: &MercyContext) -> f64 {
    if !ctx.mercy_eligible {
        return 0.0;
    }
    let base = if ctx.eximus {
        0.80
    } else if ctx.corpus {
        0.60
    } else {
        0.40
    };
    let cap = if ctx.eximus || ctx.corpus { 1.00 } else { 0.80 };
    let with_impact = (base + 0.08 * ctx.impact_stacks as f64).min(cap);
    // Level decay: -1% per 5 levels above 150, floor 10%.
    let decay = (ctx.level.saturating_sub(150) / 5) as f64 * 0.01;
    (with_impact - decay).max(0.10)
}

/// Is the Mercy prompt up right now, at `health_fraction` (0..=1) of total
/// health?
pub fn can_mercy(ctx: &MercyContext, health_fraction: f64) -> bool {
    ctx.mercy_eligible
        && ctx.shields <= 0.0
        && ctx.overguard <= 0.0
        && health_fraction > 0.0
        && health_fraction <= mercy_threshold(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> MercyContext {
        MercyContext {
            mercy_eligible: true,
            eximus: false,
            corpus: false,
            level: 1,
            impact_stacks: 0,
            shields: 0.0,
            overguard: 0.0,
        }
    }

    #[test]
    fn ineligible_units_never_show_the_prompt() {
        let c = MercyContext {
            mercy_eligible: false,
            ..ctx()
        };
        assert_eq!(mercy_threshold(&c), 0.0);
        assert!(!can_mercy(&c, 0.01));
    }

    #[test]
    fn base_windows_by_unit_kind() {
        assert_eq!(mercy_threshold(&ctx()), 0.40);
        let corpus = MercyContext {
            corpus: true,
            ..ctx()
        };
        assert_eq!(mercy_threshold(&corpus), 0.60);
        let eximus = MercyContext {
            eximus: true,
            ..ctx()
        };
        assert_eq!(mercy_threshold(&eximus), 0.80);
    }

    #[test]
    fn prompt_tracks_the_threshold_boundary() {
        let c = ctx();
        assert!(can_mercy(&c, 0.40));
        assert!(can_mercy(&c, 0.39));
        assert!(!can_mercy(&c, 0.41));
        assert!(!can_mercy(&c, 0.0)); // dead is not mercyable
    }

    #[test]
    fn shields_and_overguard_are_hard_gates() {
        // MEASURED (2026-07-24, user): 1 HP behind any shields -> no Mercy.
        let shielded = MercyContext {
            corpus: true,
            shields: 10_000.0,
            ..ctx()
        };
        assert!(!can_mercy(&shielded, 0.01));
        // Wiki: cannot Mercy while Overguard is active.
        let overguarded = MercyContext {
            eximus: true,
            overguard: 5.0,
            ..ctx()
        };
        assert!(!can_mercy(&overguarded, 0.01));
        // Both at zero -> gate opens.
        assert!(can_mercy(&ctx(), 0.01));
    }

    #[test]
    fn impact_stacks_widen_the_window_up_to_the_cap() {
        let mut c = ctx();
        c.impact_stacks = 3;
        assert!((mercy_threshold(&c) - 0.64).abs() < 1e-12);
        c.impact_stacks = 6; // 0.40 + 0.48 = 0.88 -> capped at 0.80
        assert_eq!(mercy_threshold(&c), 0.80);
        // Eximus cap is 100%: 0.80 + 3 stacks = 1.04 -> 1.00.
        let eximus = MercyContext {
            eximus: true,
            impact_stacks: 3,
            ..ctx()
        };
        assert_eq!(mercy_threshold(&eximus), 1.00);
    }

    #[test]
    fn window_decays_above_level_150_to_a_10_percent_floor() {
        let mut c = ctx();
        c.level = 150;
        assert_eq!(mercy_threshold(&c), 0.40); // no decay at 150
        c.level = 200; // 50 levels above -> -10%
        assert!((mercy_threshold(&c) - 0.30).abs() < 1e-12);
        c.level = 500; // -70% -> floored at 10%
        assert_eq!(mercy_threshold(&c), 0.10);
        // Decay applies after the Impact bonus and cap.
        c.level = 200;
        c.impact_stacks = 5; // min(0.40 + 0.40, 0.80) = 0.80, then -0.10
        assert!((mercy_threshold(&c) - 0.70).abs() < 1e-12);
    }
}
