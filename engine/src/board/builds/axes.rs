/// A benchmark build is judged with a Catalyst installed and polarized to the
/// weapon's own ceiling — the state a build worth submitting is in.
///
/// Never a constant: 60 is only a rank-30 weapon's answer. Capacity
/// "correlates to their Rank" and a rank-40 weapon reaches 80, so a fixed 60
/// refuses builds the game allows on every Kuva weapon (`crate::rules::capacity::fit`).
pub const BENCHMARK_INVESTMENT: crate::rules::capacity::Investment = crate::rules::capacity::Investment {
    catalyst: true,
    polarize_to_max: true,
    use_omni: false,
    use_umbra: false,
};

/// Slots a benchmark build may fill. EIGHT — the exilus slot is out of scope;
/// see the slot check in [`validate`].
pub const MAIN_SLOTS: usize = 8;

/// ONE AXIS OF A BUILD. See [`BUILD_AXES`].
pub struct BuildAxis {
    /// The stable id, and the only name shared across the product. Never a
    /// wire field and never a display string: each protocol spells its own
    /// half — `arcane` on a request, `arcanes` on a board record, `arcaneRank`
    /// in the page's state — and a spelling is a detail of the protocol that
    /// carries it, not a fact about builds.
    pub id: &'static str,
    /// Where a SIMULATE REQUEST carries it, which is the one spelling the
    /// engine itself answers to.
    pub request_field: &'static str,
    /// Does the BOARD keep it? A ruler fixes some of these rather than
    /// recording them — every row is scored at full mod rank and at the
    /// valence roll's ceiling — and a riven is an item that exists on one
    /// machine, so it can never identify a public row.
    pub on_board: bool,
}

/// WHAT A BUILD CONSISTS OF, declared once for the whole product.
///
/// A build travels through eight representations, from the page's live state
/// to a share link. A hand-written answer to "which axes are there" in each
/// means adding one is eight edits, and the copy nobody edits drops that axis
/// in silence, because a missing axis and a defaulted one are the same absence
/// on the wire. It happened four times, the last measured by a player shown 26
/// KPM on a ranking and 15 in the simulator for the same build.
///
/// This does not unify the SPELLINGS, which are protocol details and would cost
/// a migration of every stored preset. It unifies the LIST: served at
/// `/api/meta.build_axes`, with every surface declaring which axis each of its
/// fields carries and `scripts/check_build_axes.mjs` asserting the coverage is
/// total.
///
/// The other half of the guarantee is not a list at all and cannot go stale:
/// every ranked row carries a simulate request that reproduces it, and
/// `scripts/check_opt_replay.mjs` asserts the number comes back. A list can be
/// forgotten; an answer that has to match cannot.
pub const BUILD_AXES: &[BuildAxis] = &[
    BuildAxis { id: "mods", request_field: "mods", on_board: true },
    BuildAxis { id: "evolutions", request_field: "evolutions", on_board: true },
    BuildAxis { id: "arcanes", request_field: "arcane", on_board: true },
    // NOT on the board: a ruler scores every arcane at its own maximum, the
    // same rule that scores every row fully forma'd — investment is not a
    // choice, so it is not part of what a row states.
    BuildAxis { id: "arcane_ranks", request_field: "arcane_rank", on_board: false },
    BuildAxis { id: "mode", request_field: "mode", on_board: true },
    // A MODULAR WEAPON'S PARTS. On the board because the assembly IS the stat
    // line — two assemblies of one chamber are two different weapons in every
    // number a row states — which is the same reason `mode` is there and the
    // opposite of `arcane_ranks`, where a ruler fixes the answer for everyone.
    BuildAxis { id: "assembly", request_field: "assembly", on_board: true },
    // The ELEMENT only on the board, for the same reason: the roll is scored at
    // its ceiling, which every player can Valence-fuse to.
    BuildAxis { id: "valence", request_field: "valence_element", on_board: true },
    // A RIVEN IS A MOD, and rides in `mods` as an id — but the item itself
    // exists only on the machine that rolled it, so the request carries its
    // definition too and no public record can ever hold one.
    // ON THE BOARD SINCE 2026-08-22, as a SHAPE. The item still exists on one
    // machine and still cannot identify a public row — what a row holds is
    // which stats it rolled and which is the malus, scored at that shape's own
    // ceiling. Two players who rolled the same stats submitted the same build.
    BuildAxis { id: "rivens", request_field: "rivens", on_board: true },
    // WHO HOLDS IT: a linked Warframe build, whose stats, shards and passives
    // the fight reads. NOT on the board, and not recorded on a submission: every
    // ruler scores its rows in the Prototype's hands, so a build tested on Ash
    // enters the library as the same build it would be on anyone.
    BuildAxis { id: "wielder", request_field: "wielder", on_board: false },
];

/// A build that passed, and what it costs to actually own.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidBuild {
    /// Weapon id.
    pub weapon: String,
    /// Mod ids IN ORDER. The order is the build — it pairs the elements — so
    /// this is what arrived, minus anything the weapon cannot hold.
    pub mods: Vec<String>,
    /// Evolution ids after the ladder is applied, in tier order.
    pub evolutions: Vec<String>,
    /// Arcane ids, one per pool slot, `none` included so position is stable.
    pub arcanes: Vec<String>,
    /// An ADVERSARY weapon's VALENCE ELEMENT — the progenitor bonus this copy
    /// came out of its Lich with. Empty on every weapon that has no valence.
    ///
    /// Part of the build for the same reason an evolution is: a different element is a different weapon, not a
    /// weaker one. The PERCENTAGE is not here — the board scores every
    /// row at the roll's maximum, which every player can reach.
    pub valence: String,
    /// THE EXILUS MOD, if the build wears one. `None` on almost every build.
    ///
    /// ITS OWN FIELD RATHER THAN A NINTH ENTRY IN `mods`, because nothing
    /// downstream could tell which entry it was: an exilus-eligible mod is
    /// legal in a MAIN slot too, so the list alone cannot say whether one was
    /// spent on it. Only the slot it came out of knows, and this is where that
    /// fact is kept.
    pub exilus: Option<String>,
    /// THE RIVEN THIS BUILD CARRIES, as a SHAPE — which stats, and which is the
    /// malus. `None` on a build with no riven, which is almost all of them.
    ///
    /// The ROLLS are not here on purpose: a board row states a shape, and the
    /// shape is stored at its own ceiling — the god roll at max rank — for the
    /// same reason every row is scored at full Forma. What a particular copy
    /// landed on is luck, and luck is not a build. Where a stat's sign has
    /// stopped answering, the other end is a build OF ITS OWN
    /// (`default_corner`), ranked by its own score like anything else.
    ///
    /// WHERE IT SITS IS IN `mods`, as [`RIVEN_SLOT`], because position is part
    /// of the build: an elemental riven pairs with the build's other elementals
    /// and where it sits decides what it pairs with.
    pub riven: Option<crate::build::rivens::RivenShape>,
    /// THE NUMBERS THAT RIVEN ROLLED, when they are known. Empty on a build
    /// without one, and on a record that states only a shape.
    ///
    /// PART OF THE FIGHT, so part of [`identity`]. Which END of the 0.9–1.1
    /// band a stat sits at changes the number, and two ends of one shape are
    /// two builds — `wfsim-intake` resolves them by asking every ruler and
    /// stores each winner as its own build. An identity that could not tell
    /// them apart would file the second under the first's number.
    ///
    /// SET AFTER VALIDATION rather than passed through it, because legality is
    /// a question about the SHAPE: which stats a riven may carry, and which of
    /// them may be the malus. What they rolled cannot make a card illegal.
    pub riven_rolls: Vec<f64>,
    /// THE PARTS A MODULAR WEAPON IS ASSEMBLED FROM. `None` on everything that
    /// takes none, which is all but the Kitguns.
    ///
    /// IT IS THE BUILD, not a setting beside it: a grip sets damage, fire rate
    /// and the charge, and a loader sets the magazine, the reload and three
    /// deltas that can be negative. Two assemblies of one chamber are two
    /// builds with two answers, so an identity that could not tell them apart
    /// would file the second under the first's number — the same failure the
    /// valence and the exilus slot each had before they were part of it.
    pub assembly: Option<crate::data::weapons::kitguns::Assembly>,
    /// Forma the cheapest legal polarity layout needs. Not a legality term —
    /// two builds that are the same FIGHT can cost different amounts to reach,
    /// and the board should show the cheaper one.
    pub forma: u32,
    /// Capacity that layout uses, out of [`CAPACITY`].
    pub drain: u32,
}
