use super::*;

impl ValidBuild {
    /// THE NUMBERS THIS RIVEN ROLLED, once somebody knows them.
    ///
    /// A record states a SHAPE and `wfsim-intake` resolves it by asking every
    /// ruler which end of each band the fight likes; a record that already
    /// carries the answer hands it straight over. Either way it lands here
    /// before [`identity`] is taken, because the rolls are part of the fight.
    #[must_use]
    pub fn with_riven_rolls(mut self, rolls: Vec<f64>) -> Self {
        self.riven_rolls = rolls;
        self
    }
}

/// THE BUILD THIS CORNER IS AN ALTERNATIVE TO — `None` when it IS that build.
///
/// A riven arrives as a SHAPE and every corner of it is a build (`wfsim-intake`
/// stores them all). The DEFAULT corner is the god roll at max rank, and it is
/// the one a fight is asked for; the others wait until it has earned its place
/// (`board::entry`). So a corner has to be able to name its default, and it can
/// — the rolls and the ranks are in the build itself, and the default is a
/// function of the shape rather than of anything measured.
///
/// A BUILD WITH NO RIVEN AND NO LOWERED CARD IS ALREADY THE DEFAULT and answers
/// `None`, which is what keeps this out of every other build's way.
pub fn default_corner(b: &ValidBuild) -> Option<String> {
    let rolls = b
        .riven
        .as_ref()
        .map(crate::build::rivens::default_rolls)
        .unwrap_or_default();
    let mods: Vec<String> = b
        .mods
        .iter()
        .map(|m| crate::data::mods::split_rank(m).0.to_string())
        .collect();
    if rolls == b.riven_rolls && mods == b.mods {
        return None;
    }
    let mut d = b.clone().with_riven_rolls(rolls);
    d.mods = mods;
    Some(build_id(&d))
}

/// THE BOARD'S OWN ROW KEY: a [`build_id`] and the MODE it was played in.
///
/// One row per (build, mode) — a build played two ways is two entrants, and
/// collapsing them would keep whichever arrived first. It is here rather than
/// at the scorer's two call sites because the PAGE asks the same question now
/// ("is what I am looking at already a row?") and a second spelling of a key is
/// how one side quietly stops matching the other.
///
/// THE ID AND NOT THE TEXT IT HASHES, because a score names a row in `builds`
/// and that table is keyed by the id. Two spellings of "which build" is the one
/// thing a foreign key may not have.
pub fn board_key(b: &ValidBuild, mode: &str) -> String {
    let mode = if mode.is_empty() { "base" } else { mode };
    format!("{}#{}", build_id(b), mode)
}

/// A BUILD'S ID: [`identity`], hashed.
///
/// DERIVED AND NOT ALLOCATED. The same build always hashes the same, so nothing
/// reads the table before writing to it, two writers cannot disagree about
/// whether they hold one build, and a resubmission is the same row with no
/// lookup and no race. A random id would need all three.
///
/// FNV-1a, WRITTEN OUT, TWICE. The answer is a permanent key, so it has to be
/// stable across machines, across runs and across Rust versions —
/// `DefaultHasher` guarantees none of those. Two passes from different offsets
/// give 128 bits: the identity it folds is up to ~300 bytes of ids and rides on
/// every score row, and a library four orders of magnitude under the birthday
/// bound will not see a collision.
pub fn build_id(b: &ValidBuild) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let fold = |mut h: u64, bytes: &[u8]| {
        for byte in bytes {
            h ^= u64::from(*byte);
            h = h.wrapping_mul(PRIME);
        }
        h
    };
    let text = identity(b);
    let lo = fold(OFFSET, text.as_bytes());
    let hi = fold(OFFSET ^ 0xffff_ffff_ffff_ffff, text.as_bytes());
    format!("{hi:016x}{lo:016x}")
}

/// The FIGHT this build is, as one stable string.
///
/// Everything that changes the number and nothing that does not — see the
/// module header for why polarity, Forma, slot position, mod rank and order are
/// all absent. Two submissions with the same key are one board row.
pub fn identity(b: &ValidBuild) -> String {
    let set = |xs: &[String]| xs.iter().cloned().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(",");
    // THE VALENCE IS PART OF THE IDENTITY, and it has to be: two Kuva Nukors
    // differing only in progenitor element are two builds with two scores, and
    // an identity that could not tell them apart would file the second under
    // the first's number. Appended rather than inserted, so every identity
    // already computed for an ordinary weapon is unchanged — it ends in `|`
    // and nothing else moved.
    let key = format!(
        "{}|{}|{}|{}|{}",
        b.weapon,
        b.mods.join(","),
        set(&b.evolutions),
        b.arcanes.join(","),
        b.valence
    );
    // THE RIVEN'S SHAPE, appended — so every identity already computed for a
    // build without one is unchanged, byte for byte, and the board is not
    // re-keyed by a feature it does not use. Same reason the valence went on
    // the end rather than into the middle.
    //
    // The SHAPE and not the rolls: two players who rolled the same stats
    // submitted the same build, and the board scores it at the ceiling either
    // way. WHERE it sits is already in `mods`, which carries `riven` in place.
    let key = match &b.riven {
        None => key,
        Some(r) => format!(
            "{key}|{}{}",
            r.bonuses.join("+"),
            r.malus.as_ref().map_or(String::new(), |m| format!("-{m}"))
        ),
    };
    // THE EXILUS SLOT'S MOD, appended for the same reason the valence and the
    // riven shape were: every identity already computed for a build without one
    // is unchanged byte for byte, so the board is not re-keyed by a feature most
    // of it does not use.
    //
    // IT IS PART OF THE BUILD. The slot was excluded from the board until
    // 2026-08-25, so an identity that ignored it was right and stopped being:
    // the first submission carrying one collapsed into the same row as the
    // build without it and the score that survived was whichever arrived first.
    // Caught by scoring two Atomos builds differing only in `ruinous_extension`
    // and getting one row.
    let key = match &b.exilus {
        None => key,
        Some(x) => format!("{key}|x:{x}"),
    };
    // THE ASSEMBLY, appended for the reason the three above were: a build that
    // takes no parts is keyed byte for byte as it was, so the board is not
    // re-keyed by a feature almost none of it uses. The CHAMBER is not here —
    // it is the weapon, and `b.weapon` already carries it.
    let key = match &b.assembly {
        None => key,
        Some(a) => format!("{key}|a:{}+{}", a.grip, a.loader),
    };
    // THE ROLLS, AND THEY GO LAST. Every identity already computed for a build
    // that states only a shape is unchanged byte for byte — the same reason the
    // three above are appended rather than inserted.
    //
    // THEY ARE IN HERE AT ALL because two ends of one shape are two builds with
    // two numbers: a riven at 1.1 damage and the same card at 0.9 are different
    // fights, and an id that could not tell them apart would file the second
    // under the first's.
    match b.riven_rolls.as_slice() {
        [] => key,
        rolls => {
            let mut key = key;
            for r in rolls {
                key.push('|');
                key.push_str(&r.to_string());
            }
            key
        }
    }
}
