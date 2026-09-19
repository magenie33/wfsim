use super::*;

/// A RIVEN AS A PUBLIC RECORD STATES IT: which stats it carries, and which one
/// is the malus. The ROLLS are deliberately absent.
///
/// A riven is an item that exists on one machine, which is why no board row
/// could ever hold one — and this is the shape that CAN be held, because it is
/// a statement anybody can act on: roll this weapon for these stats. What a
/// particular copy landed on is luck, and the board has never ranked luck. It
/// scores every row at full Forma, every mod at max rank and every valence at
/// the roll's ceiling for the same reason.
///
/// So a shape is scored at ITS OWN ceiling, and [`perfect`] is what finds it.
/// `Deserialize` so a BOARD row can carry one: `data::boards::BoardEntry` reads
/// the same block the scorer writes. `rolls` sits beside it in the file and is
/// not part of the shape — serde ignores it, which is right: a shape is scored
/// at its own ceiling and the rolls are what THIS engine found there.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
pub struct RivenShape {
    /// Bonus stat ids, SORTED — a riven's stats do not combine with each other,
    /// so two players listing them in different orders described one riven and
    /// must produce one row. (Mod ORDER is the opposite and stays as placed:
    /// elements pair in the order the mods sit in, which is why `canonical_mods`
    /// exists at all.)
    pub bonuses: Vec<String>,
    /// The malus, when the riven has one. A riven without one rolls smaller
    /// bonuses, so "no malus" is a different shape rather than a better one.
    pub malus: Option<String>,
}

impl RivenShape {
    /// The shape of a rolled riven — what survives when the luck is removed.
    pub fn of(spec: &RivenSpec) -> Self {
        let mut bonuses: Vec<String> = spec.bonuses.iter().map(|b| b.id.clone()).collect();
        bonuses.sort();
        Self { bonuses, malus: spec.malus.as_ref().map(|m| m.id.clone()) }
    }

    /// This shape as a rolled riven, at the rolls given.
    ///
    /// PUBLIC because a stored build names its own corner. `wfsim-intake`
    /// resolves a shape once, by asking every ruler, and the scorer then builds
    /// the card the record names rather than searching for it again.
    pub fn at(&self, class: &str, rolls: &[f64]) -> RivenSpec {
        RivenSpec {
            class: class.to_string(),
            bonuses: self
                .bonuses
                .iter()
                .zip(rolls)
                .map(|(id, &roll)| RolledStat { id: id.clone(), roll })
                .collect(),
            malus: self
                .malus
                .as_ref()
                .map(|id| RolledStat { id: id.clone(), roll: rolls[self.bonuses.len()] }),
            // AT THE CEILING, like every other investment the board scores. A
            // rank is levelled and a polarity is Forma'd, so neither is part of
            // what a row states.
            rank: MAX_RANK,
            polarity: Polarity::Madurai,
        }
    }

    /// How many stats have a roll to choose — bonuses plus the malus.
    pub fn stat_count(&self) -> usize {
        self.bonuses.len() + usize::from(self.malus.is_some())
    }

    /// THE ELEMENTS THIS SHAPE CARRIES, in the order its stats are listed.
    ///
    /// A riven with an elemental stat PAIRS with the build's other elementals,
    /// so where it sits among the mods changes the combined element and
    /// therefore the fight — the same fact that makes `board::builds::canonical_mods`
    /// keep mod order at all. A shape that carries none is
    /// position-independent like any other plain mod.
    ///
    /// A MALUS IS NEVER ONE. The five bonus-only stats aside, a negative
    /// elemental roll still adds that element to the pool — but no riven can
    /// take an elemental stat as its malus, which `stat_pool` already encodes
    /// (`malus: false` on those entries) and `RivenSpec::illegal` enforces.
    pub fn elements(&self, class: &str) -> Vec<crate::rules::damage::DamageType> {
        let p = pool(class);
        self.bonuses
            .iter()
            .filter_map(|id| p.iter().find(|x| &x.id == id))
            .filter(|d| d.kind == "elemental_damage_bonus")
            .filter_map(|d| d.arg.as_deref().map(crate::data::weapons::damage_type))
            .collect()
    }
}

/// THE CARD A PLAYER WOULD WANT — every bonus at its ceiling, the malus at its
/// floor. The default, and the answer for all but a few hundred builds.
pub fn god_roll(shape: &RivenShape, class: &str) -> RivenSpec {
    let rolls: Vec<f64> = (0..shape.stat_count())
        .map(|i| if i < shape.bonuses.len() { ROLL_MAX } else { ROLL_MIN })
        .collect();
    shape.at(class, &rolls)
}

/// THE STATS WHOSE SIGN IS ALWAYS AMBIGUOUS: the three physical types.
///
/// A physical bonus does not add damage beside the rest, it changes the SHARE
/// each damage type holds of the total — and a status proc is drawn in
/// proportion to that share. More Impact is fewer Viral and Corrosive procs, so
/// which end of the band is better is a property of the build rather than of
/// the sign, on every weapon.
pub const PHYSICAL_STATS: [&str; 3] = ["impact", "puncture", "slash"];

/// …AND STATUS DURATION, on every weapon, asked at its two ends like the rest.
///
/// It paces Heat's armour strip (its steps scale with it) and nullifies every
/// status at or below -100%, so a deeper malus strips faster until the burn
/// disappears (M99). The true best can therefore sit INSIDE the band, on the
/// edge of that cliff; the ends are asked, and a low-rank card on
/// `data/search/every_rank.yaml` is what reaches the edge.
pub const DURATION_STATS: [&str; 1] = ["status_duration"];

/// …AND THE WEAPONS THAT TAKE THE SIGN OFF ONE MORE, one row each.
///
/// A weapon EARNS A ROW by paying for NOT having something, which makes
/// whatever supplies it a cost:
///
///   - `critical_chance` — an Incarnon form paying `+2000% damage on
///     non-critical hits`, or a crit multiplier granted only BELOW a crit
///     chance threshold, which crossing it loses.
///   - `status_chance` — a crit multiplier granted only below a status count.
///   - `magazine_capacity` — a bonus earned by reloading from EMPTY, which a
///     bigger magazine earns less often.
///
/// BY WEAPON AND NOT BY PERK, so this stays a list a person can read and audit.
/// A build on one of these that never took the perk is asked anyway and the
/// fight answers "the god roll" — a few hundred fights against a rule that
/// cannot go stale differently from the weapon it is about.
pub const SIGN_IS_NOT_THE_ANSWER: &[(&str, &[&str])] = &[
    ("atomos", &["magazine_capacity"]),
    ("boar", &["magazine_capacity"]),
    ("boar_prime", &["magazine_capacity"]),
    ("braton", &["critical_chance"]),
    ("braton_prime", &["critical_chance"]),
    ("braton_vandal", &["critical_chance"]),
    ("felarx", &["critical_chance"]),
    ("furis", &["critical_chance"]),
    ("gorgon", &["magazine_capacity"]),
    ("gorgon_wraith", &["magazine_capacity"]),
    ("laetum", &["critical_chance"]),
    ("lato", &["magazine_capacity"]),
    ("lato_prime", &["magazine_capacity"]),
    ("lato_vandal", &["magazine_capacity"]),
    ("mk1_braton", &["critical_chance"]),
    ("mk1_furis", &["critical_chance"]),
    ("phenmor", &["critical_chance", "status_chance"]),
    ("prisma_gorgon", &["magazine_capacity"]),
    ("stug", &["magazine_capacity"]),
    ("torid", &["magazine_capacity"]),
];

/// WHICH OF THIS SHAPE'S STATS THE SIGN DOES NOT DECIDE — the only ones a fight
/// has to be asked about.
///
/// Three sources and no fourth: [`PHYSICAL_STATS`] and [`DURATION_STATS`],
/// always, and the weapon's own row in [`SIGN_IS_NOT_THE_ANSWER`].
///
/// EMPTY IS THE COMMON CASE, and it means no fight at all: the card is the god
/// roll. Measured over the library: four riven builds in five.
pub fn ambiguous_stats(shape: &RivenShape, weapon: &str) -> BTreeSet<String> {
    let named: BTreeSet<&str> = shape
        .bonuses
        .iter()
        .map(String::as_str)
        .chain(shape.malus.as_deref())
        .collect();
    let by_weapon = SIGN_IS_NOT_THE_ANSWER
        .iter()
        .find(|(w, _)| *w == weapon)
        .map_or(&[][..], |(_, s)| *s);
    PHYSICAL_STATS
        .into_iter()
        .chain(DURATION_STATS)
        .chain(by_weapon.iter().copied())
        .filter(|s| named.contains(s))
        .map(String::from)
        .collect()
}

/// THE ROLLS TO STORE THIS SHAPE WITH, asked of the fight only where the sign
/// cannot answer: [`perfect`] over its [`corners`], the god roll the default.
pub fn best_roll(
    shape: &RivenShape,
    class: &str,
    ambiguous: &BTreeSet<String>,
    mut score: impl FnMut(&RivenSpec) -> Option<(f64, f64)>,
) -> RivenSpec {
    let mut all = corners(shape, ambiguous).into_iter();
    let god = all.next().expect("the god roll is always a corner");
    let best = perfect(god, all, |rolls| score(&shape.at(class, rolls)));
    shape.at(class, &best)
}

/// EVERY CORNER A SHAPE IS ASKED AT, the god roll first: every bonus at its
/// ceiling and the malus at its floor, then every combination of the
/// `ambiguous` stats flipped to their other end — `2^k` in all, where k is one
/// on all but five builds in the library.
pub fn corners(shape: &RivenShape, ambiguous: &BTreeSet<String>) -> Vec<Vec<f64>> {
    let stats: Vec<&str> = shape
        .bonuses
        .iter()
        .map(String::as_str)
        .chain(shape.malus.as_deref())
        .collect();
    let god: Vec<f64> = (0..stats.len())
        .map(|i| if i < shape.bonuses.len() { ROLL_MAX } else { ROLL_MIN })
        .collect();
    let asked: Vec<usize> =
        (0..stats.len()).filter(|&i| ambiguous.contains(stats[i])).collect();
    (0..(1u32 << asked.len()))
        .map(|m| {
            let mut rolls = god.clone();
            for (bit, &i) in asked.iter().enumerate() {
                if m >> bit & 1 == 1 {
                    rolls[i] = if rolls[i] == ROLL_MAX { ROLL_MIN } else { ROLL_MAX };
                }
            }
            rolls
        })
        .collect()
}

/// THE DEFAULT, UNLESS A FIGHT PROVES AN ALTERNATIVE BETTER.
///
/// `score` returns the fight's number and ITS OWN standard error, and the
/// second is what makes this deterministic without a tolerance anybody picked:
/// an alternative wins only by beating the default by more than two standard
/// errors of the difference, the ruler's own resolution. Among those the best
/// wins. Two corners the ruler cannot separate are not two builds, and between
/// them the player gets the default. `None` from `score` is a fight that did
/// not run; a default that did not run stands without asking the rest.
pub fn perfect<C>(
    default: C,
    alternatives: impl IntoIterator<Item = C>,
    mut score: impl FnMut(&C) -> Option<(f64, f64)>,
) -> C {
    let mut alternatives = alternatives.into_iter().peekable();
    if alternatives.peek().is_none() {
        return default;
    }
    let Some((base, base_err)) = score(&default) else { return default };
    let mut best: Option<(f64, C)> = None;
    for c in alternatives {
        let Some((s, err)) = score(&c) else { continue };
        if s - base > 2.0 * (base_err * base_err + err * err).sqrt()
            && best.as_ref().is_none_or(|(b, _)| s > *b)
        {
            best = Some((s, c));
        }
    }
    best.map_or(default, |(_, c)| c)
}
