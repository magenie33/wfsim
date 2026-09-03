//! Enemy stat scaling with level, plus Steel Path modifiers.
//!
//! Source: wiki `Enemy_Level_Scaling` (community-derived from in-game testing;
//! DE has never confirmed these — **unverified**, see docs/MECHANICS.md §8) and
//! `The_Steel_Path`.
//!
//! Common structure for health / shields / armor / overguard: two power curves
//! `f1 = 1 + c1·Δ^e1` (low level) and `f2 = 1 + c2·Δ^e2` (high level) blended
//! by smoothstep over a transition window of the level difference
//! `Δ = current − base` (70–80 for health/shields/armor, 45–50 for overguard,
//! which uses `x − 1` regardless of base level):
//!
//! ```text
//! multiplier(Δ) = f1(Δ)·(1 − S(Δ)) + f2(Δ)·S(Δ)
//! S(Δ) = 3T² − 2T³,  T = clamp((Δ − lo) / (hi − lo), 0, 1)
//! ```

/// Enemy levels are normally capped at 9999 (exceeded only in Void Fissures).
pub const LEVEL_CAP: u32 = 9999;

/// The cap the STAT SYSTEM imposes on enemy armor *values* (applied in
/// [`armor_at`]). This is data-side discipline, not a property of the DR
/// formula: DE simply never lets a value above 2,700 exist (a 10k-armor
/// enemy is theoretically expressible), and the scaling curve tops out
/// here. At this value the DR formula evaluates to 90%.
pub const ARMOR_CAP: f64 = 2700.0;

/// Enemies that would spawn with less armor get this instead (initial value
/// only — strips can still push armor below it).
pub const ARMOR_SPAWN_MIN: f64 = 200.0;

/// Steel Path: enemy level +100 (+50 Archwing/Railjack, +20 Duviri).
pub const STEEL_PATH_LEVEL_BONUS: u32 = 100;
/// Steel Path: health ×2.5 and shields ×2.5. Armor is NOT increased (removed
/// in U36; shields were also accidentally ×6.25 before U36).
pub const STEEL_PATH_HEALTH_MULT: f64 = 2.5;
pub const STEEL_PATH_SHIELD_MULT: f64 = 2.5;

/// One two-curve scaling rule.
#[derive(Debug, Clone, Copy)]
pub struct Curve {
    pub c1: f64,
    pub e1: f64,
    pub c2: f64,
    pub e2: f64,
    /// Smoothstep transition window in level difference.
    pub lo: f64,
    pub hi: f64,
}

impl Curve {
    /// Stat multiplier at a level difference `delta` (clamped at 0).
    pub fn multiplier(&self, delta: f64) -> f64 {
        let d = delta.max(0.0);
        let f1 = 1.0 + self.c1 * d.powf(self.e1);
        let f2 = 1.0 + self.c2 * d.powf(self.e2);
        let t = ((d - self.lo) / (self.hi - self.lo)).clamp(0.0, 1.0);
        let s = 3.0 * t * t - 2.0 * t * t * t;
        f1 * (1.0 - s) + f2 * s
    }
}

/// Health curves per faction (transition 70–80).
pub mod health {
    use super::Curve;
    const W: (f64, f64) = (70.0, 80.0);
    pub const GRINEER: Curve = curve(0.015, 2.12, 10.7332, 0.72); // + Scaldra
    pub const CORPUS: Curve = curve(0.015, 2.12, 13.4165, 0.55);
    pub const INFESTED: Curve = curve(0.0225, 2.12, 16.0998, 0.72);
    pub const CORRUPTED: Curve = curve(0.015, 2.1, 10.7332, 0.685); // + Anarchs
    /// Murmur, Sentient, and Unaffiliated (includes Zariman/Thrax units).
    pub const UNAFFILIATED: Curve = curve(0.015, 2.0, 10.7332, 0.5);
    pub const TECHROT: Curve = curve(0.02, 2.12, 15.0998, 0.7);

    const fn curve(c1: f64, e1: f64, c2: f64, e2: f64) -> Curve {
        Curve {
            c1,
            e1,
            c2,
            e2,
            lo: W.0,
            hi: W.1,
        }
    }
}

/// Shield curves per faction (transition 70–80).
pub mod shield {
    use super::Curve;
    const W: (f64, f64) = (70.0, 80.0);
    pub const CORPUS: Curve = curve(0.02, 1.76, 2.0, 0.76);
    pub const CORRUPTED: Curve = curve(0.02, 1.75, 2.0, 0.75); // + Anarchs
    pub const GRINEER: Curve = curve(0.02, 1.75, 1.6, 0.75); // + Sentient
    pub const TECHROT: Curve = curve(0.02, 1.76, 3.5, 0.76);

    const fn curve(c1: f64, e1: f64, c2: f64, e2: f64) -> Curve {
        Curve {
            c1,
            e1,
            c2,
            e2,
            lo: W.0,
            hi: W.1,
        }
    }
}

/// Armor curve — same for all factions (transition 70–80).
pub const ARMOR: Curve = Curve {
    c1: 0.005,
    e1: 1.75,
    c2: 0.4,
    e2: 0.75,
    lo: 70.0,
    hi: 80.0,
};

/// Overguard curve (transition 45–50). Note: uses `current level − 1`, not the
/// difference to the enemy's base level. All Eximus have base overguard 12.
pub const OVERGUARD: Curve = Curve {
    c1: 0.0015,
    e1: 4.0,
    c2: 260.0,
    e2: 0.9,
    lo: 45.0,
    hi: 50.0,
};

/// Scaled armor value: two-curve multiplier, then the spawn minimum of 200,
/// then the hard cap of 2,700.
pub fn armor_at(base_armor: f64, current_level: u32, base_level: u32) -> f64 {
    if base_armor <= 0.0 {
        return 0.0;
    }
    let delta = current_level.saturating_sub(base_level) as f64;
    let scaled = base_armor * ARMOR.multiplier(delta);
    scaled.clamp(ARMOR_SPAWN_MIN, ARMOR_CAP)
}

/// Scaled overguard value (uses `current level − 1`).
pub fn overguard_at(base_overguard: f64, current_level: u32) -> f64 {
    if base_overguard <= 0.0 {
        return 0.0;
    }
    base_overguard * OVERGUARD.multiplier((current_level.saturating_sub(1)) as f64)
}

/// Enemy damage-output scaling, default: `1 + 0.015·Δ^1.55` (single curve —
/// expressed as a [`Curve`] with both halves identical).
pub const ENEMY_DAMAGE_DEFAULT: Curve = Curve {
    c1: 0.015,
    e1: 1.55,
    c2: 0.015,
    e2: 1.55,
    lo: 0.0,
    hi: 1.0,
};

/// Grineer / Corpus / Techrot damage-output scaling: smoothstep between
/// `1 + 0.015·Δ^1.75` (Δ < 1) and `1 + 0.0075·Δ^1.55` (Δ > 25).
pub const ENEMY_DAMAGE_GRINEER_CORPUS_TECHROT: Curve = Curve {
    c1: 0.015,
    e1: 1.75,
    c2: 0.0075,
    e2: 1.55,
    lo: 1.0,
    hi: 25.0,
};

/// Flat multiplier on attacks, applied on top of damage scaling:
/// Grineer / Corpus / Techrot 2x, Infested 3x, everyone else 1x.
pub const ENEMY_ATTACK_MULT_GRINEER_CORPUS_TECHROT: f64 = 2.0;
pub const ENEMY_ATTACK_MULT_INFESTED: f64 = 3.0;

/// Affinity multiplier. Special case: uses the **current level** (base level
/// is NOT subtracted) and the final affinity value is floored when applied.
/// Eximus replace the leading 1 with 3.
pub fn affinity_multiplier(current_level: u32, eximus: bool) -> f64 {
    let lead = if eximus { 3.0 } else { 1.0 };
    lead + 0.1425 * (current_level as f64).sqrt()
}

/// All Eximus units have this base overguard (scaled by [`OVERGUARD`]).
pub const EXIMUS_BASE_OVERGUARD: f64 = 12.0;

/// Eximus replacement base health (wiki `Enemy_Level_Scaling` §Health,
/// Eximus tab): applied to the unit's base health *before* the faction curve.
/// `factor` is 0.25 for units with shields or armor, 0.375 for units with
/// neither.
pub fn eximus_base_health(base_health: f64, level: u32, has_shields_or_armor: bool) -> f64 {
    let factor = if has_shields_or_armor { 0.25 } else { 0.375 };
    let x = level as f64;
    let g = if x <= 15.0 {
        1.0
    } else if x <= 25.0 {
        1.0 + 0.025 * (x - 15.0)
    } else if x <= 35.0 {
        1.25 + 0.125 * (x - 25.0)
    } else if x <= 50.0 {
        2.5 + (2.0 / 15.0) * (x - 35.0)
    } else if x <= 100.0 {
        4.5 + 0.03 * (x - 50.0)
    } else {
        6.0
    };
    (base_health * 1.1).max(factor * (base_health + 900.0) * g)
}

/// Armor → damage reduction, post-U36 formula (wiki `Damage/Calculation`
/// §Armored Enemies): `DR = 0.9·√(armor/2700)`. The pre-U36 curve was
/// `armor/(armor+300)`; both give exactly 90% at armor 2,700, but the new
/// square-root curve makes partial armor strip far more valuable (300 armor:
/// 30% DR now vs 50% before). `armor` is the value after all strips/debuffs.
///
/// ABOVE THE CAP THE CURVE CHANGES RATHER THAN STOPPING: *"If an enemy's Net
/// Armor exceeds 2,700 under an exceptional condition, damage reduction instead
/// uses `Net Armor/(Net Armor+300)`"* (wiki `Armor`). So 90% is NOT a ceiling on
/// the REDUCTION — it is only where the two curves meet, which is why "the
/// 2,700 cap" and "the 90% cap" get told as one fact and are not one.
///
/// NOTHING NAMED REACHES IT. Level scaling clamps an enemy's initial armor to
/// `[200, 2700]`, and the wiki names no current source that raises the value —
/// *"Steel Path no longer increases Armor values"* (Ver 36), the same update
/// that brought this curve in. The branch is written because the RULE has it,
/// not because a fight here can take it.
pub fn armor_damage_reduction(armor: f64) -> f64 {
    if armor <= 0.0 {
        0.0
    } else if armor > ARMOR_CAP {
        armor / (armor + 300.0)
    } else {
        0.9 * (armor / ARMOR_CAP).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1.0)
    }

    #[test]
    fn smoothstep_window_blends_between_curves() {
        // Below the window: pure f1. Above: pure f2.
        let c = health::UNAFFILIATED;
        let f1 = |d: f64| 1.0 + 0.015 * d.powf(2.0);
        let f2 = |d: f64| 1.0 + 10.7332 * d.powf(0.5);
        assert!(approx(c.multiplier(69.0), f1(69.0), 1e-12));
        assert!(approx(c.multiplier(81.0), f2(81.0), 1e-12));
        // Inside the window the value is between the two curves.
        let m = c.multiplier(75.0);
        let (a, b) = (f1(75.0).min(f2(75.0)), f1(75.0).max(f2(75.0)));
        assert!(m >= a && m <= b, "{m} not in [{a}, {b}]");
    }

    #[test]
    fn unaffiliated_health_curves_intersect_at_80() {
        // The original universal pair intersects at Δ=80: both ≈ 97.
        let f1 = 1.0 + 0.015 * 80.0f64.powf(2.0);
        let f2 = 1.0 + 10.7332 * 80.0f64.powf(0.5);
        assert!(approx(f1, 97.0, 1e-3), "f1 = {f1}");
        assert!(approx(f2, f1, 1e-3), "f2 = {f2}");
    }

    #[test]
    fn armor_curves_intersect_at_80() {
        let f1 = 1.0 + 0.005 * 80.0f64.powf(1.75);
        let f2 = 1.0 + 0.4 * 80.0f64.powf(0.75);
        assert!(approx(f1, f2, 1e-3), "f1 = {f1}, f2 = {f2}");
    }

    #[test]
    fn level_cap_health_multiplier_matches_hand_computation() {
        // Unaffiliated (Thrax) at Δ = 9998: 1 + 10.7332·√9998 ≈ 1074.25.
        let m = health::UNAFFILIATED.multiplier((LEVEL_CAP - 1) as f64);
        assert!(approx(m, 1074.25, 1e-3), "multiplier = {m}");
    }

    #[test]
    fn thrax_stats_at_level_cap() {
        // Thrax Centurion base @L1: 3600 HP, 200 armor, 15 overguard
        // (data/enemies/thrax_centurion.yaml).
        let hp = 3600.0 * health::UNAFFILIATED.multiplier((LEVEL_CAP - 1) as f64);
        assert!(approx(hp, 3_867_300.0, 1e-3), "hp = {hp}");
        // Steel Path: ×2.5 health, armor untouched.
        let sp_hp = hp * STEEL_PATH_HEALTH_MULT;
        assert!(approx(sp_hp, 9_668_250.0, 1e-3), "sp hp = {sp_hp}");
        // Armor scales far past the cap -> clamped to 2700 = 90% DR.
        let armor = armor_at(200.0, LEVEL_CAP, 1);
        assert_eq!(armor, ARMOR_CAP);
        assert!(approx(armor_damage_reduction(armor), 0.9, 1e-12));
        // Overguard at 9999: 15 × (1 + 260·9998^0.9) ≈ 15.53M.
        let og = overguard_at(15.0, LEVEL_CAP);
        assert!(approx(og, 15_530_000.0, 2e-3), "overguard = {og}");
    }

    #[test]
    fn post_u36_armor_curve_anchor_points() {
        // DR = 0.9 * sqrt(armor/2700): 300 -> 30%, 675 -> 45%, 2700 -> 90%.
        assert!(approx(armor_damage_reduction(300.0), 0.30, 1e-12));
        assert!(approx(armor_damage_reduction(675.0), 0.45, 1e-12));
        assert!(approx(armor_damage_reduction(2700.0), 0.90, 1e-12));
    }

    #[test]
    fn above_the_cap_the_curve_switches_and_the_two_meet_there() {
        // AR/(AR+300) past 2,700, and 90% is where the two AGREE rather than a
        // ceiling either one imposes: 2700/3000 = 0.9 = 0.9·sqrt(2700/2700).
        assert!(approx(armor_damage_reduction(2700.0 + 1e-9), 0.90, 1e-9));
        assert!(approx(armor_damage_reduction(10_000.0), 10_000.0 / 10_300.0, 1e-12));
        // It keeps CLIMBING, which the old defensive clamp hid.
        assert!(armor_damage_reduction(10_000.0) > 0.90);
        assert!(armor_damage_reduction(1e9) < 1.0);
    }

    /// NO FIGHT HERE REACHES THAT BRANCH, and this is what says so: `armor_at`
    /// clamps to the cap, and every mitigation factor is a `1 - strip` — the
    /// only aura that touches enemy armor is Corrosive Projection, which
    /// removes. A source that ADDS armor would land above the cap and this test
    /// is where it would first be noticed.
    #[test]
    fn nothing_in_the_model_lifts_armor_past_the_cap() {
        assert_eq!(armor_at(5000.0, LEVEL_CAP, 1), ARMOR_CAP);
        for a in crate::auras_data::all() {
            if let crate::auras_data::AuraEffect::EnemyArmor(v) = a.effect {
                assert!(v <= 0.0, "{} raises enemy armor by {v}", a.id);
            }
        }
    }

    #[test]
    fn armor_spawn_minimum_and_zero_armor() {
        // Base 50 armor at its base level scales to 50 -> raised to 200.
        assert_eq!(armor_at(50.0, 1, 1), ARMOR_SPAWN_MIN);
        // Zero base armor stays zero (the minimum is not conjured from nothing).
        assert_eq!(armor_at(0.0, 9999, 1), 0.0);
        assert_eq!(armor_damage_reduction(0.0), 0.0);
    }

    #[test]
    fn eximus_base_health_piecewise() {
        // x <= 15: max(1.1·bh, 0.25·(bh+900)). For bh = 300: max(330, 300) = 330.
        assert_eq!(eximus_base_health(300.0, 10, true), 330.0);
        // Large bh, armored, x > 100: 0.25·(bh+900)·6.
        let h = eximus_base_health(3600.0, 200, true);
        assert!((h - 0.25 * 4500.0 * 6.0).abs() < 1e-9, "h = {h}");
        // Unarmored/unshielded factor is 0.375.
        let h = eximus_base_health(3600.0, 200, false);
        assert!((h - 0.375 * 4500.0 * 6.0).abs() < 1e-9, "h = {h}");
        // Continuity at the segment joins (x = 25 -> 1.25, x = 35 -> 2.5);
        // small base so the boosted term beats the 1.1x floor.
        let a = eximus_base_health(100.0, 25, true);
        assert!((a - 0.25 * 1000.0 * 1.25).abs() < 1e-9, "a = {a}");
        let b = eximus_base_health(100.0, 35, true);
        assert!((b - 0.25 * 1000.0 * 2.5).abs() < 1e-9, "b = {b}");
    }

    #[test]
    fn commandeered_ash_prime_at_1000_matches_wiki_calculator() {
        // Golden cross-check (wiki Commandeered_Prime page's calculated stat
        // block; user-supplied 2026-07-24). Base @L1: 15,000 HP / 1,750
        // shield / 200 armor / 5,000 affinity; Anarchs faction. This also
        // settles the wiki's Anarchs contradiction: only the Corrupted
        // curves reproduce the numbers (the Unaffiliated pair is 3.6x off).
        let d = 999.0;
        let hp = 15_000.0 * health::CORRUPTED.multiplier(d);
        assert!(approx(hp, 18_275_927.85, 1e-9), "hp = {hp}");
        let sh = 1_750.0 * shield::CORRUPTED.multiplier(d);
        assert!(approx(sh, 623_680.94, 1e-7), "shield = {sh}");
        assert_eq!(armor_at(200.0, 1000, 1), ARMOR_CAP);
        assert_eq!((5_000.0 * affinity_multiplier(1, false)).floor(), 5_712.0);
        assert_eq!(
            (5_000.0 * affinity_multiplier(1000, false)).floor(),
            27_531.0
        );
    }

    #[test]
    fn enemy_damage_scaling_curves() {
        // Default single curve: Δ=100 -> 1 + 0.015·100^1.55 ≈ 19.90.
        let d = ENEMY_DAMAGE_DEFAULT.multiplier(100.0);
        assert!(approx(d, 1.0 + 0.015 * 100.0f64.powf(1.55), 1e-12));
        // GCT: pure f1 below Δ=1, pure f2 above Δ=25.
        let g = ENEMY_DAMAGE_GRINEER_CORPUS_TECHROT;
        assert!(approx(
            g.multiplier(0.5),
            1.0 + 0.015 * 0.5f64.powf(1.75),
            1e-12
        ));
        assert!(approx(
            g.multiplier(30.0),
            1.0 + 0.0075 * 30.0f64.powf(1.55),
            1e-12
        ));
    }

    #[test]
    fn affinity_uses_current_level_not_delta() {
        // Level 100: 1 + 0.1425·10 = 2.425; Eximus lead 3 -> 4.425.
        assert!(approx(affinity_multiplier(100, false), 2.425, 1e-12));
        assert!(approx(affinity_multiplier(100, true), 4.425, 1e-12));
    }

    #[test]
    fn multiplier_is_one_at_base_level() {
        assert_eq!(health::GRINEER.multiplier(0.0), 1.0);
        assert_eq!(ARMOR.multiplier(0.0), 1.0);
        assert_eq!(overguard_at(15.0, 1), 15.0);
    }
}
