use super::*;

/// The random band every stat rolls within, independently (wiki).
pub const ROLL_MIN: f64 = 0.9;
pub const ROLL_MAX: f64 = 1.1;
/// Ranks 0..=8; the value scales with `(rank + 1) / 9`.
pub const MAX_RANK: u32 = 8;
/// `base x 10 x (rank + 1)`, so 90 at max rank.
pub(super) const PER_RANK: f64 = 10.0;

/// How many bonuses a riven carries, and whether it carries a malus. This
/// is the ONLY thing that decides the config multipliers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub bonuses: u32,
    pub malus: bool,
}

impl Shape {
    /// Multiplier on every POSITIVE stat — the wiki's table, which is the only
    /// published set verified from a primary source AND internally consistent
    /// (1.2375 is exactly `0.99 x 1.25`, 0.9375 exactly `0.75 x 1.25`). The
    /// three tables that disagree, and why this one wins, are
    /// docs/DATA_SOURCES.md §"Riven config multiplier".
    pub fn bonus_mult(&self) -> f64 {
        match (self.bonuses, self.malus) {
            (2, false) => 0.99,
            (2, true) => 1.2375,
            (3, false) => 0.75,
            (3, true) => 0.9375,
            // Not a shape the game rolls; treated as plain so a caller that
            // constructs one still gets a number instead of a panic.
            _ => 0.99,
        }
    }

    /// Multiplier on the MALUS. Negative: it flips the stat's sign.
    pub fn malus_mult(&self) -> f64 {
        if self.bonuses >= 3 {
            -0.75
        } else {
            -0.495
        }
    }

    pub fn is_legal(&self) -> bool {
        (2..=3).contains(&self.bonuses)
    }
}

/// One stat a riven of this class can roll.
#[derive(Debug, Clone, Deserialize)]
pub struct RivenStat {
    /// Stable English slug ("critical_chance"), our id — never DE's tag.
    pub id: String,
    /// DE's internal tag, the join key back to the export.
    pub tag: String,
    /// DE's own index in `upgradeEntries`. Seven rifle stats share one base
    /// value, so two of them at the same roll are worth EXACTLY the same and
    /// the name's magnitude ordering ties — this is what breaks it.
    #[serde(default)]
    pub order: u32,
    /// DE's per-stat base number.
    pub base: f64,
    /// Name fragments. A riven's name is GENERATED from its stats; these are
    /// the pieces.
    pub prefix: String,
    pub suffix: String,
    /// Display template, `|val|` where the number goes.
    pub text: String,
    /// Our effect kind, or `unmodeled`.
    pub kind: String,
    /// Element / physical type / faction, where the kind needs one.
    #[serde(default)]
    pub arg: Option<String>,
    /// May this stat be the MALUS? Wiki lists five that are bonus-only.
    #[serde(default = "yes")]
    pub malus: bool,
    /// May this stat be a BONUS? One melee stat cannot: DE ships the pair
    /// "Additional Combo Count Chance" and "Chance to Gain Combo Count" as two
    /// entries, and the second exists only to be the negative.
    #[serde(default = "yes")]
    pub bonus: bool,
}

pub(super) fn yes() -> bool {
    true
}

/// How the CARD prints a stat's number — the stored value is a fraction and
/// the card is not obliged to agree with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// `x100`, with a sign. Most stats.
    Percent,
    /// The raw number, with a sign. Punch Through is metres.
    Number,
    /// A MULTIPLIER off 1, no sign: the card reads "x0.59 Damage to Corpus"
    /// where the stored value is -0.41. Only the three
    /// faction stats print this way, and it is why their range runs 0.xx-1.xx
    /// instead of straddling zero.
    Multiplier,
}

impl RivenStat {
    pub fn shown_as(&self) -> Shown {
        if self.kind == "faction_damage_bonus" {
            Shown::Multiplier
        } else if self.text.contains('%') {
            Shown::Percent
        } else {
            Shown::Number
        }
    }

    /// Stored fraction -> the number printed on the card.
    pub fn shown(&self, value: f64) -> f64 {
        match self.shown_as() {
            Shown::Percent => value * 100.0,
            Shown::Number => value,
            Shown::Multiplier => 1.0 + value,
        }
    }

    /// The number printed on the card -> the stored fraction. Exactly the
    /// inverse of [`Self::shown`], so a value typed off a real riven means
    /// what it says.
    pub fn from_shown(&self, shown: f64) -> f64 {
        match self.shown_as() {
            Shown::Percent => shown / 100.0,
            Shown::Number => shown,
            Shown::Multiplier => shown - 1.0,
        }
    }

    /// Decimals the CARD shows — and therefore all anyone can read off a
    /// riven they own. A percentage shows ONE, so a box
    /// offering two invites a precision the game never gave you.
    ///
    /// The full-precision value is still what the sim computes with: this is
    /// the reading, not the number. A stat entered at 144.8 keeps whatever
    /// roll 144.8 implies, and that roll is exact.
    pub fn decimals(&self) -> usize {
        match self.shown_as() {
            // The card's own two, and the reason the faction stats read
            // x0.59 rather than x0.6.
            Shown::Multiplier => 2,
            Shown::Percent | Shown::Number => 1,
        }
    }

    /// The whole line, template filled in.
    pub fn print(&self, value: f64) -> String {
        let s = self.shown(value);
        let d = self.decimals();
        let n = match self.shown_as() {
            // A multiplier carries its meaning in the `x`, not in a sign:
            // x0.59 is already the bad one.
            Shown::Multiplier => format!("x{s:.d$}"),
            _ => format!("{}{s:.d$}", if s >= 0.0 { "+" } else { "" }),
        };
        self.text.replace("|val|", &n)
    }
}

/// Where in its 0.9-1.1 band a roll landed, as 0-100.
///
/// This is the number riven traders read first: it says how good the ROLL is
/// with the stat, the weapon's disposition and the shape all divided out, so
/// two stats on one card are comparable and so are two cards. 100 is the top
/// of the band for a bonus AND for a malus — it is the size of the roll, not
/// a judgement about it.
///
/// Uniform in the roll, because that is what the band is: the wiki gives a
/// +/-10% randomisation and no shape to it.
pub fn percentile(roll: f64) -> f64 {
    ((roll - ROLL_MIN) / (ROLL_MAX - ROLL_MIN) * 100.0).clamp(0.0, 100.0)
}
