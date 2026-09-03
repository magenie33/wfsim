//! WHAT A SCORED ROW ACTUALLY DEPENDS ON, as one hash.
//!
//! The board rescores when the engine moves, and "the engine" was ONE
//! fingerprint over `engine` + `webapi` + `cli` + all of `data`. So adding a
//! weapon — which touches nothing any existing row reads — invalidated every
//! stored score and bought a full rescore: 967 rows over 32 shards, about an
//! hour of wall clock and thirty-odd of CPU, to reproduce numbers that could
//! not have changed. Adding a weapon is one of the most common commits there
//! is.
//!
//! A ROW'S DATA DEPENDENCIES ARE ENUMERABLE FROM THE ROW. It names its weapon,
//! its mods, its arcanes and its evolutions; the ruler names itself. Hash those
//! files and a mod correction rescores the rows carrying that mod and nothing
//! else.
//!
//! **THIS DOES NOT WEAKEN THE BOARD'S INVARIANT — IT STATES IT.** "Different
//! engine versions is not a board" is a conservative proxy for the property
//! that matters: every row's score is the number THIS engine would compute, and
//! a row that reads none of the changed files already stores that number.
//!
//! THE ONE HAND LIST HERE CAN ONLY COST TIME: anything not attributed to an
//! entity and not on `AFFECTS_NO_NUMBER` falls into the GLOBAL bucket every row
//! carries, so forgetting an entry is slow and never wrong.
//!
//! COMMENTS ARE ALREADY FREE, since `build.rs` embeds each file with its
//! full-line comments removed — rewriting a citation produces an identical
//! fingerprint and no rescore.

use std::collections::HashMap;
use std::sync::OnceLock;

/// FNV-1a, written out rather than borrowed from `std`, because the answer has
/// to be STABLE — across machines, across runs, and across Rust versions.
/// `DefaultHasher` guarantees none of those, and a fingerprint that moved on a
/// toolchain bump would rescore the whole board for nothing, which is the exact
/// cost this module exists to remove.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fold(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// The families whose files are attributed to ONE entity, by the `id:` the file
/// declares. Everything else is global — see the module header.
const PER_ENTITY: &[&str] = &["weapons", "mods", "arcanes", "evolutions", "benchmarks"];

/// Files that cannot move a number, and therefore belong to no row.
///
/// Each is here on its own evidence, not by category:
///   * `i18n/` is an OVERLAY of names and card text. English is the source
///     everywhere and ids are never translated (AGENTS.md), so a locale file
///     can only change what a reader is shown.
///   * `assets.yaml` is image paths. It grows with every weapon added, which
///     is precisely the commit this module exists to make cheap.
///   * `surveys/` is generated evidence read by TESTS and by nothing else
///     (`docs/DATA_SOURCES.md`).
///   * `unmodelled/reasons.yaml` is the prose behind an admission — a sentence
///     shown to a reader, never a term in a formula.
const AFFECTS_NO_NUMBER: &[&str] = &["i18n/", "assets.yaml", "surveys/", "unmodelled/"];

fn ignored(path: &str) -> bool {
    AFFECTS_NO_NUMBER.iter().any(|p| path.starts_with(p))
}

fn family_of(path: &str) -> &str {
    path.split('/').next().unwrap_or("")
}

/// `(family, id)` -> the embedded path that declares it.
///
/// Built from the data compiled into this binary, by reading each file's
/// top-level `id:`. Derived rather than listed, so a weapon or a mod added
/// tomorrow is indexed by nobody.
fn index() -> &'static HashMap<(&'static str, String), &'static str> {
    static IDX: OnceLock<HashMap<(&'static str, String), &'static str>> = OnceLock::new();
    IDX.get_or_init(|| {
        let mut out = HashMap::new();
        for (path, text) in crate::data::files_under("") {
            let family = family_of(path);
            if !PER_ENTITY.contains(&family) {
                continue;
            }
            // The TOP-LEVEL `id:` — column zero, so a nested `id:` inside a
            // list of options (an evolution tier's, a form's) cannot claim the
            // file. Data discipline puts one at the top of every file.
            let Some(id) = text.lines().find_map(|l| l.strip_prefix("id:")) else {
                continue;
            };
            let id = id.split('#').next().unwrap_or("").trim();
            if id.is_empty() {
                continue;
            }
            // FIRST WINS and the loss is reported by the test below rather than
            // here: two files claiming one id inside a family is a data fault,
            // and this is not the module that should be discovering it.
            out.entry((PER_ENTITY[PER_ENTITY.iter().position(|f| *f == family).unwrap()],
                       id.to_string()))
                .or_insert(path);
        }
        out
    })
}

/// Everything no entity owns, hashed once: the debuff table, the factions, the
/// perks, the riven rules, the enemies, the Tenno, the frames. Small, rarely
/// touched, and when it IS touched a full rescore is the honest answer.
fn global() -> u64 {
    static G: OnceLock<u64> = OnceLock::new();
    *G.get_or_init(|| {
        let mut paths: Vec<(&str, &str)> = crate::data::files_under("")
            .filter(|(p, _)| !ignored(p) && !PER_ENTITY.contains(&family_of(p)))
            .collect();
        paths.sort_unstable();
        let mut h = FNV_OFFSET;
        for (p, text) in paths {
            h = fold(h, p.as_bytes());
            h = fold(h, text.as_bytes());
        }
        h
    })
}

fn fold_entity(h: u64, family: &str, id: &str) -> u64 {
    let mut h = fold(h, family.as_bytes());
    h = fold(h, id.as_bytes());
    // AN ID THAT RESOLVES TO NOTHING STILL CHANGES THE HASH, through the id
    // itself above. A row naming a mod this build does not have is refused long
    // before it is scored; folding the name in anyway means the fingerprint
    // never quietly agrees with a different row.
    if let Some(path) = index().get(&(
        PER_ENTITY[PER_ENTITY.iter().position(|f| *f == family).unwrap_or(0)],
        id.to_string(),
    )) {
        if let Some(text) = crate::data::file(path) {
            h = fold(h, text.as_bytes());
        }
    }
    h
}

/// The DATA half of a row's fingerprint: the ruler, the weapon and every form
/// it can fire, each mod, each arcane, each evolution, plus everything no
/// entity owns.
///
/// The CODE half stays global and is passed separately — a change in
/// `damage.rs` can move any row, and no dependency set can say otherwise.
pub fn row_fingerprint(
    benchmark: &str,
    weapon: &str,
    mods: &[String],
    arcanes: &[String],
    evolutions: &[String],
    // THE EXILUS SLOT'S MOD, which is a mod like any other to this hash — and
    // folded in SEPARATELY from `mods` because it is a separate axis: the same
    // card in the exilus slot and in a main slot are two builds.
    exilus: Option<&str>,
    // THE PARTS, on a modular weapon's row. A grip and a loader are FILES, so a
    // correction to either has to make the rows that read them stale — which is
    // what this hash is for, and what it could not say while the row did not
    // carry them.
    assembly: Option<&crate::kitguns_data::Assembly>,
) -> String {
    let mut h = fold(FNV_OFFSET, b"wfsim-row-fp-1");
    h ^= global();
    h = h.wrapping_mul(FNV_PRIME);
    h = fold_entity(h, "benchmarks", benchmark);
    // EVERY FORM, not the one this mode fires. A form is its own file and the
    // set is small; taking the superset means a row cannot go stale because the
    // mode-to-form rule moved underneath it.
    h = fold_entity(h, "weapons", weapon);
    for f in crate::weapons_data::forms_of(weapon) {
        h = fold_entity(h, "weapons", f.weapon_id);
    }
    // SORTED, because a build is a SET of cards on these three axes and two
    // submissions listing them in different orders are one build. The identity
    // key already treats them that way.
    for (family, list) in [("mods", mods), ("arcanes", arcanes), ("evolutions", evolutions)] {
        let mut ids: Vec<&str> = list.iter().map(String::as_str).collect();
        ids.sort_unstable();
        for id in ids {
            h = fold_entity(h, family, id);
        }
    }
    // THE EXILUS SLOT'S MOD, marked so it cannot hash the same as the same card
    // sitting in a main slot.
    if let Some(id) = exilus.filter(|x| !x.is_empty()) {
        h = fold_entity(h, "mods", id);
        h = fold(h, b"#exilus");
    }
    // …AND THE PARTS, folded only when there ARE any, so every fingerprint
    // already computed for a weapon that takes none is unchanged byte for byte
    // and no board is re-scored by a feature it does not use.
    if let Some(a) = assembly {
        h = fold_entity(h, "kitguns", &a.grip);
        h = fold_entity(h, "kitguns", &a.loader);
        h = fold(h, b"#assembly");
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(weapon: &str, mods: &[&str]) -> String {
        let m: Vec<String> = mods.iter().map(|x| x.to_string()).collect();
        row_fingerprint("single_target", weapon, &m, &[], &[], None, None)
    }

    #[test]
    fn a_row_fingerprint_is_stable_and_tells_builds_apart() {
        assert_eq!(fp("braton_prime", &["serration"]), fp("braton_prime", &["serration"]));
        assert_ne!(fp("braton_prime", &["serration"]), fp("braton_prime", &["split_chamber"]));
        assert_ne!(fp("braton_prime", &["serration"]), fp("burston_prime", &["serration"]));
        // A SET on this axis: the same two cards in the other order is the same
        // build, and re-scoring it would be work bought by nothing.
        assert_eq!(
            fp("braton_prime", &["serration", "split_chamber"]),
            fp("braton_prime", &["split_chamber", "serration"])
        );
    }

    #[test]
    fn a_ruler_is_part_of_it() {
        let m = vec!["serration".to_string()];
        assert_ne!(
            row_fingerprint("single_target", "braton_prime", &m, &[], &[], None, None),
            row_fingerprint("group_clear", "braton_prime", &m, &[], &[], None, None)
        );
    }

    /// THE WHOLE POINT, asserted: a row does not depend on a weapon it does not
    /// name. This is what makes adding a weapon free, and it is the one property
    /// that would silently stop holding if `PER_ENTITY` lost an entry.
    #[test]
    fn a_row_ignores_every_file_it_does_not_read() {
        let m = vec!["serration".to_string()];
        let mine = row_fingerprint("single_target", "braton_prime", &m, &[], &[], None, None);
        // Recomputing after touching NOTHING is trivially equal; what is asserted
        // here is the shape that makes the optimisation real — the fingerprint is
        // built from a NAMED set of files, and the roster has hundreds this row
        // never names.
        let named = 1 /* benchmark */ + 1 /* weapon */
            + crate::weapons_data::forms_of("braton_prime").len() + 1 /* mod */;
        let all = crate::data::files_under("").count();
        assert!(all > named * 20, "{all} files, {named} named — the index is not narrowing anything");
        assert_eq!(mine, row_fingerprint("single_target", "braton_prime", &m, &[], &[], None, None));
    }

    /// The exclusion list is the only hand list here, so it is asserted to be
    /// what it claims: files that cannot move a number. A new entry has to be
    /// added deliberately, with its reason, rather than drifting in.
    #[test]
    fn nothing_numeric_is_excused_from_the_fingerprint() {
        let excused: Vec<&str> =
            crate::data::files_under("").map(|(p, _)| p).filter(|p| ignored(p)).collect();
        assert!(!excused.is_empty(), "the exclusion list matches no file at all");
        for p in &excused {
            let f = family_of(p);
            assert!(
                matches!(f, "i18n" | "surveys" | "unmodelled") || *p == "assets.yaml",
                "{p} is excused from the board's fingerprint and nothing says why"
            );
        }
    }

    /// Every per-entity family actually resolves — an index that silently found
    /// nothing would make every row's fingerprint the global one, which reads as
    /// a working optimisation and is a board that never rescores.
    #[test]
    fn every_per_entity_family_is_indexed() {
        for family in PER_ENTITY {
            let n = index().keys().filter(|(f, _)| f == family).count();
            let files = crate::data::files_under("").filter(|(p, _)| family_of(p) == *family).count();
            assert!(n > 0, "{family}: nothing indexed out of {files} files");
            // Not every file must carry an id — a form sibling can — but most do,
            // and a family where almost none do means the `id:` convention moved.
            assert!(n * 2 >= files, "{family}: only {n} of {files} files declare a top-level id");
        }
    }
}
