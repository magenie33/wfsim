// SPDX-License-Identifier: AGPL-3.0-or-later
//! AN ARCANE'S CARD, as a reader sees it: the one place in `arcanes` that
//! reads its prose (engine/src/prose.rs).
use super::*;

impl ArcaneDef {
    /// The verbatim in-game DESCRIPTION with its `X` placeholders filled at
    /// `rank` — what the config page shows (docs: description-X schema).
    ///
    /// `X`s map to the effects' display values in yaml order. Two derived
    /// cases close the gaps: adjacent effects sharing one number collapse
    /// (Outburst's cc+cd "by X% per Combo"), and a trailing "Stacks up to
    /// X%" cap is per_stack × max_stacks (Cascadia Flare).
    pub fn desc_at(&self, rank: u32) -> String {
        let rank = rank.min(self.max_rank);
        let at = |sc: &Scale| sc.at(rank, self.max_rank);
        let lerp = |a: f64, b: f64| a + (b - a) * rank as f64 / self.max_rank.max(1) as f64;
        let mut vals: Vec<f64> = Vec::new();
        if self.perk.as_deref() == Some("secondary_enervate") {
            vals.push((rank + 1) as f64); // "Resets after X Big Critical Hit"
        }
        for e in &self.effects {
            match e {
                ArcEffect::Buff { scale, .. }
                | ArcEffect::TennoScaled { cap: scale, .. }
                | ArcEffect::CondCritChance(scale)
                | ArcEffect::WeakpointCritChance(scale)
                | ArcEffect::PerColdDamage { scale, .. }
                | ArcEffect::AddedElement { scale, .. }
                | ArcEffect::RechargeableMagazine { scale }
                | ArcEffect::FlatDamageOnStatus(scale)
                | ArcEffect::EncumberChance(scale)
                | ArcEffect::AmmoEfficiency { scale, .. }
                | ArcEffect::CompressionDamage(scale)
                | ArcEffect::CompressionAmmoEfficiency(scale)
                | ArcEffect::PerAllyCritChance(scale)
                | ArcEffect::CondReloadSpeed(scale)
                // Out of the sim's world, but it still owns its `X`.
                | ArcEffect::Unmodeled { scale, .. }
                | ArcEffect::OutOfScope { scale, .. }
                // Multiplier kinds ("xX"): stored as the bonus — fill_x's
                // xX rule renders the +1.
                | ArcEffect::Debilitate(scale)
                | ArcEffect::FinalDamageCap(scale)
                | ArcEffect::WeakpointNextShotDamage(scale)
                // "Deals x8 Extra Damage to Overguard" prints the total too:
                // measured ×8 (M120).
                | ArcEffect::OverguardDamage(scale) => vals.push(at(scale)),
                ArcEffect::ColdBurst { scale, radius0, radius1 }
                | ArcEffect::AoeEcho { scale, radius0, radius1, .. } => {
                    vals.push(at(scale));
                    vals.push(lerp(*radius0, *radius1));
                }
                // NO `X` TO FILL: this card's text carries its numbers
                // literally, the way every melee arcane's does.
                ArcEffect::StatusSpread { .. }
                | ArcEffect::DuplicateOnBaseCrit(_)
                | ArcEffect::HeavyCritPerInput { .. }
                | ArcEffect::DamageVsFrozen(_)
                | ArcEffect::HeadshotMultiplier { .. }
                | ArcEffect::ReloadSpeed { .. }
                | ArcEffect::Inert(_) | ArcEffect::Elsewhere(_) => {}
            }
        }
        let xs = count_x(self.description.shown());
        // Outburst: cc + cd share the single "by X% per Combo" number.
        while vals.len() > xs {
            let before = vals.len();
            vals.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
            if vals.len() == before {
                break;
            }
        }
        // Cascadia Flare: the trailing stack cap is per_stack × max_stacks.
        if vals.len() + 1 == xs && self.description.shown().contains("Stacks up to X%") {
            if let Some(ArcEffect::Buff { scale, max_stacks, .. }) = self
                .effects
                .iter()
                .find(|e| matches!(e, ArcEffect::Buff { .. }))
            {
                vals.push(at(scale) * *max_stacks as f64);
            }
        }
        fill_x(self.description.shown(), &vals)
    }
}
