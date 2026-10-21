// SPDX-License-Identifier: AGPL-3.0-or-later
//! A WARFRAME'S CARDS, as a reader sees them: the one place in `warframes`
//! that reads prose (engine/src/prose.rs).
use super::*;

impl WarframeMod {
    /// The card at `rank`: a modelled bonus restated at that rank, every other
    /// line the card's own max-rank text.
    pub fn card_at(&self, rank: u32) -> Vec<String> {
        card_lines(&self.effects, self.description.shown(), rank, self.max_rank)
    }
}

impl WarframeArcane {
    pub fn card_at(&self, rank: u32) -> Vec<String> {
        card_lines(&self.effects, self.description.shown(), rank, self.max_rank)
    }
}

impl ArtifactMod {
    /// The card with `seated` on the artifact: its last line is the bonus paid out.
    pub fn card_with(&self, seated: &[&ArtifactMod]) -> Vec<String> {
        let mut lines: Vec<String> = self.description.shown().lines().map(str::to_string).collect();
        if let (Some(b), Some(last)) = (&self.bonus, lines.last_mut()) {
            let total = b.value * f64::from(self.bonus_count(seated));
            *last = format!("+{total}{} {}", b.unit, b.stat);
        }
        lines
    }
}

/// One line per effect when the card has exactly that many, else the text as
/// written. A rounded tenth is what the card prints (W`Mod`).
fn card_lines(effects: &[FrameEffect], description: &str, rank: u32, max_rank: u32) -> Vec<String> {
    let lines: Vec<&str> = description.lines().collect();
    if lines.len() != effects.len() {
        return lines.into_iter().map(str::to_string).collect();
    }
    effects
        .iter()
        .zip(lines)
        .map(|(e, line)| match e {
            FrameEffect::Bonus(s, v) => {
                let pct = (at_rank(*v, rank, max_rank) * 1000.0).round() / 10.0;
                format!("{}{}% {}", if pct >= 0.0 { "+" } else { "" }, pct, s.label())
            }
            _ => line.to_string(),
        })
        .collect()
}

/// The admission a frame whose passive is not modelled carries.
pub(super) fn passive_not_modelled(frame: &WarframeDef) -> String {
    format!("Passive not modelled: {}", frame.passive.shown())
}
