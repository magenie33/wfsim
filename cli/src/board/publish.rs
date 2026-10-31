// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the page fetches: one row as the page receives it. The files those
//! rows are written into are `board::assemble`'s.

use serde_json::value::RawValue;
use serde_json::{json, Value};

/// One scored row.
pub(crate) struct Row {
    pub(crate) weapon: String,
    /// THE BUILD THIS ROW IS, without the mode — `board::builds::build_id`, which is
    /// what the `builds` table is keyed by.
    ///
    /// Carried so the run can prove every validated build ended up somewhere:
    /// published, or deferred. It is NOT written to the yaml, which
    /// states the build itself and from which the id is recomputed — a stored
    /// copy of a derived fact is the one that goes stale.
    pub(crate) identity: String,
    /// HOW the weapon was played — `base`, `cycle`, `alternate`. Part of the
    /// entrant's identity, not of the fight: a Torid through its Incarnon
    /// cycle and a Torid that never transmutes are two things to hold, and a
    /// board that keeps one row for both can only ever show whichever the
    /// benchmark happened to pin.
    pub(crate) mode: String,
    pub(crate) score: f64,
    pub(crate) mods: Vec<String>,
    pub(crate) evolutions: Vec<String>,
    pub(crate) arcanes: Vec<String>,
    /// An ADVERSARY weapon's progenitor element. Empty on every other weapon.
    /// The BONUS is not a row field: the ruler scores every row at the roll's
    /// maximum, which is investment rather than a choice.
    pub(crate) valence: String,
    /// THE EXILUS SLOT'S MOD, empty on almost every row. Optional as of
    /// 2026-08-25 — see `board::benchmarks::BuildRequirement::allows_exilus`.
    pub(crate) exilus: String,
    /// THE PARTS, on a modular weapon's row; empty on every other. They are the
    /// BUILD — a grip sets damage, fire rate and the charge — so a row that
    /// dropped them would publish a number for a weapon nobody submitted, and
    /// two assemblies of one chamber would be one row.
    pub(crate) assembly: Option<wfsim_engine::data::weapons::assembly::Assembly>,
    /// THE RIVEN THIS BUILD CARRIES, as a SHAPE — which stats and which is the
    /// malus, never a roll. A row states a shape and the shape is stored at its
    /// own ceiling — the god roll at max rank — for the reason every row is
    /// scored at full Forma: what one copy landed on is luck, and the board
    /// does not rank luck.
    ///
    /// WHERE it sits is in `mods`, which carries `riven` at its own position —
    /// an elemental riven pairs with the build's other elementals, so position
    /// is part of the build.
    ///
    /// The ROLLS the scorer settled on go in `riven_rolls`, because opening
    /// this row has to be able to build that riven on the reader's machine.
    pub(crate) riven: Option<RowRiven>,
    /// WHEN THIS NUMBER WAS MEASURED — its fact's `finished_at`, the scores
    /// table's own clock. Published to the minute, so a reader sees how old a
    /// row is rather than how old the board is.
    pub(crate) measured_at: String,
    /// …AND WHICH COMMIT MEASURED IT: engine, data and the ruler's terms are all
    /// compiled from the repo, so one hash names every input of the number.
    /// Each build carries its own, because a weapon's rows are measured at
    /// different times by different builds, and a rescore replaces them row by row.
    pub(crate) measured_by: String,
}

/// A row's riven: the SHAPE it states, and the ROLLS this engine found best for
/// it. The shape is what a player acts on; the rolls are what the number rests
/// on and what a reader needs to reproduce it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RowRiven {
    pub(crate) bonuses: Vec<String>,
    pub(crate) malus: Option<String>,
    /// One roll per stat, bonuses first then the malus — the corner this ROW
    /// is, since the rolls are part of the build's id. Not part of the shape a
    /// reader acts on, which is the two lists above.
    pub(crate) rolls: Vec<f64>,
}

/// …AND THE PART OF ONE THAT IS READ BACK. Only the four fields the carry and
/// the index decide on: everything else travels as bytes, because a publish that
/// reparses a number it is not measuring publishes a number nobody computed.
#[derive(serde::Deserialize)]
pub(crate) struct Published<'a> {
    pub(crate) benchmark: &'a str,
    #[serde(default)]
    pub(crate) mode: Option<&'a str>,
    #[serde(borrow)]
    pub(crate) score: &'a RawValue,
    #[serde(default, borrow)]
    pub(crate) riven: Option<&'a RawValue>,
}

/// A row the page would not understand is a row this cannot speak for either, so
/// it is CARRIED and never indexed — dropping it would take it off the board.
pub(crate) fn published(raw: &RawValue) -> Option<Published<'_>> {
    serde_json::from_str(raw.get()).ok()
}

/// `2026-10-05T13:52:07Z` → `2026-10-05T13:52Z`: the minute, still UTC and
/// still an ISO time a browser parses.
pub(crate) fn measured_minute(at: &str) -> Option<String> {
    (at.len() >= 16 && at.as_bytes()[10] == b'T').then(|| format!("{}Z", &at[..16]))
}

/// WHAT `wfsim-intake --produced` writes as `measured_by` before the engine:
/// the submitter's machine, not a commit.
pub const CLIENT_MEASURED: &str = "client:";

/// WHAT `worker/board_tasks.js` writes before the engine id of a fact the
/// volunteers settled.
const VERIFIED_MEASURED: &str = "verified:";

/// An engine id's length: `engine_id` in `scripts/build_site_app.py`.
const ENGINE_ID_LEN: usize = 12;

/// THE ENGINE THAT MEASURED A ROW, the one version every row states: a
/// volunteers' fact's `verified:<engine>` or an official one's bare engine id.
/// Anything else — a commit, a client, a migration — names no engine and is
/// omitted, so a board never mixes two kinds of version.
pub(crate) fn measured_engine(by: &str) -> Option<String> {
    let id = by.strip_prefix(VERIFIED_MEASURED).unwrap_or(by);
    (id.len() == ENGINE_ID_LEN && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))).then(|| id.to_string())
}

/// ONE ROW AS THE PAGE RECEIVES IT — `site/board/<weapon>.json`'s shape, in one
/// place.
///
/// Extracted so it can be TESTED, because the one thing that went wrong with it
/// is invisible from the outside: it was a hand-written list of nine keys and
/// `riven` was never one of them, so a riven row reached the yaml and never
/// reached the page. Three separate reports came out of that single missing key
/// — the board's "riven only" view listed nothing, the builder could not group
/// riven builds apart, and TAKING one left an empty mod slot.
///
/// BYTE FOR BYTE WITH `build_site_app.py`, which is what makes a local site
/// build a no-op against the scorer's own output. That is why the riven key is
/// OMITTED rather than written as `null` on a plain row: the Python side copies
/// the yaml entry, and a plain entry simply has no `riven` key.
pub(crate) fn page_row(bench_id: &str, r: &Row) -> Value {
    let mut row = json!({
        "benchmark": bench_id,
        "mode": r.mode,
        "score": r.score,
        // The number stays EXACT and the string beside it is what the page
        // prints. Formatting lives in `data::boards::format_score`, so "four
        // significant figures, four decimals" is one rule in one language
        // rather than a Rust copy and a JS copy that drift.
        "shown": wfsim_engine::data::boards::format_score(r.score),
        "mods": r.mods,
        "evolutions": r.evolutions,
        "arcanes": r.arcanes,
        "valence": r.valence,
    });
    if let Some(rv) = &r.riven {
        if let Some(o) = row.as_object_mut() {
            o.insert(
                "riven".into(),
                json!({ "bonuses": rv.bonuses, "malus": rv.malus, "rolls": rv.rolls }),
            );
        }
    }
    // THE EXILUS SLOT'S MOD, on the same terms as the riven above: OMITTED on a
    // row that wears none rather than written as an empty string, because the
    // Python side copies the yaml entry and a row without one simply has no key.
    if !r.exilus.is_empty() {
        if let Some(o) = row.as_object_mut() {
            o.insert("exilus".into(), json!(r.exilus));
        }
    }
    // A ROW MIGRATED WITH NO CLOCK OMITS IT rather than publishing a time
    // nobody recorded.
    if let Some(at) = measured_minute(&r.measured_at) {
        if let Some(o) = row.as_object_mut() {
            o.insert("measured_at".into(), json!(at));
        }
    }
    if let Some(by) = measured_engine(&r.measured_by) {
        if let Some(o) = row.as_object_mut() {
            o.insert("measured_by".into(), json!(by));
        }
    }
    // A CLIENT'S NUMBER SAYS SO. Only the owner's live board is projected from
    // one (`scripts/live_board.sh`); a public board reads `scores` alone and
    // never carries the flag.
    if r.measured_by.starts_with(CLIENT_MEASURED) {
        if let Some(o) = row.as_object_mut() {
            o.insert("unverified".into(), json!(true));
        }
    }
    // …AND THE PARTS. The builder reads `row.grip` / `row.loader` to open a
    // board row as a build, so a row that omitted them opened as the chamber's
    // DEFAULT assembly — a different weapon from the one the number is for.
    if let Some(a) = &r.assembly {
        row.as_object_mut().expect("row object")
            .extend(a.parts().as_object().expect("parts object").clone());
    }
    row
}

/// THE CROSS-WEAPON INDEX, under the same directory as the weapon files and
/// therefore excluded from them by name. `index` is not a weapon id: ids are
/// `[a-z0-9_]` slugs off the roster and none of them is this.
pub(crate) const INDEX_STEM: &str = "index";

#[cfg(test)]
mod page_row_tests {
    use super::*;
    use crate::board::facts::exact_score;

    /// WHAT IS PUBLISHED, READ FROM DISK. `site/board/` is outside `data/`, so
    /// it is not embedded and there is nothing to reach through the binary —
    /// which is the point: the rows are a CI artifact and not something the
    /// browser carries. The WEAPON is the file name, because that is what makes
    /// a weapon page one fetch.
    fn published() -> Vec<(String, Vec<Value>)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../site/board");
        let mut out: Vec<(String, Vec<Value>)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .filter(|p| p.file_stem().is_some_and(|x| x != INDEX_STEM))
            .map(|p| {
                let text = std::fs::read_to_string(&p).expect("read");
                let rows = serde_json::from_str::<Vec<Value>>(&text)
                    .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                (p.file_stem().unwrap().to_string_lossy().into_owned(), rows)
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        assert!(!out.is_empty(), "nothing published under site/board/");
        out
    }

    fn strings(r: &Value, k: &str) -> Vec<String> {
        r.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    }

    /// EVERY PUBLISHED ROW IS A BUILD SOMEONE COULD EQUIP. The board is public
    /// and copyable, so a row that cannot be built is worse than a missing row
    /// — it is an instruction that fails in the arsenal.
    /// `validate_for_board_with` is the same check a submission faces, run here
    /// against what is already published.
    #[test]
    #[ignore = "a fact about publish.yml's output: CI runs it as the published-board job"]
    fn every_published_row_is_a_legal_build() {
        let mut n = 0usize;
        for (weapon, rows) in published() {
            for r in &rows {
                let bench = r.get("benchmark").and_then(Value::as_str).unwrap_or("");
                assert!(
                    wfsim_engine::board::benchmarks::get(bench).is_some(),
                    "{weapon} names benchmark {bench:?}, which does not exist",
                );
                let riven = r.get("riven").map(|rv| wfsim_engine::build::rivens::RivenShape {
                    bonuses: strings(rv, "bonuses"),
                    malus: rv.get("malus").and_then(Value::as_str).map(str::to_owned),
                });
                let part = |key| r.get(key).and_then(Value::as_str).unwrap_or("");
                let assembly = wfsim_engine::data::weapons::assembly::of(
                    &weapon, part("grip"), part("loader"), part("link"));
                let v = wfsim_engine::board::builds::validate_for_board_with(
                    bench,
                    &weapon,
                    &strings(r, "mods"),
                    &strings(r, "evolutions"),
                    &strings(r, "arcanes"),
                    r.get("valence").and_then(Value::as_str).unwrap_or(""),
                    riven.as_ref(),
                    r.get("exilus").and_then(Value::as_str),
                    assembly.as_ref(),
                    // The row's own Warframe, which only an Exalted row has.
                    r.get("wielder")
                        .and_then(|x| serde_json::from_value(x.clone()).ok())
                        .as_ref(),
                )
                .unwrap_or_else(|e| panic!("{weapon} row on {bench}: {e}"));
                // `validate` already refused anything over capacity, and the
                // capacity is the weapon's own — so the assertion is that the
                // row survived the door whole, not that it fits a number here.
                assert_eq!(v.mods.len(), strings(r, "mods").len(), "{weapon} lost a mod");
                let score = exact_score(&serde_json::to_string(r).expect("json"));
                assert!(score.is_some_and(|x| x > 0.0), "{weapon} scored nothing");
                n += 1;
            }
        }
        eprintln!("{n} published row(s) are legal builds");
    }

    /// BEST FIRST INSIDE A GROUP, because "the top 10" is the only order a board
    /// has — and a group is (benchmark, mode, riven-ness), which is what the
    /// page ranks independently.
    #[test]
    fn a_weapons_rows_are_ranked() {
        for (weapon, rows) in published() {
            let mut last: std::collections::HashMap<(String, String, bool), f64> =
                Default::default();
            for r in &rows {
                let k = (
                    r.get("benchmark").and_then(Value::as_str).unwrap_or("").to_string(),
                    r.get("mode").and_then(Value::as_str).unwrap_or("base").to_string(),
                    r.get("riven").is_some(),
                );
                let s = exact_score(&serde_json::to_string(r).expect("json")).unwrap_or(0.0);
                if let Some(prev) = last.insert(k.clone(), s) {
                    assert!(prev >= s, "{weapon} out of order in {k:?}: {prev} then {s}");
                }
            }
        }
    }

    fn row(riven: Option<RowRiven>) -> Row {
        Row {
            identity: "dual_toxocyst|cycle".into(),
            weapon: "dual_toxocyst".into(),
            mode: "cycle".into(),
            score: 139.28,
            mods: vec!["galvanized_diffusion".into()],
            evolutions: vec!["dual_toxocyst_evo1_incarnon_form".into()],
            arcanes: vec!["secondary_deadhead".into()],
            valence: String::new(),
            exilus: String::new(),
            assembly: None,
            riven,
            measured_at: "2026-10-05T13:52:07Z".into(),
            measured_by: "verified:67e27b73c3ee".into(),
        }
    }

    #[test]
    fn published_modular_rows_keep_their_component_kind() {
        for (weapon, grip, loader, link) in [
            ("sepfahn_one_handed", "peye", "", "vargeet_ii_jai"),
            ("tombfinger_secondary", "haymaker", "killstream", ""),
        ] {
            let mut r = row(None);
            r.weapon = weapon.into();
            r.assembly = wfsim_engine::data::weapons::assembly::of(weapon, grip, loader, link);
            let out = page_row("standard_single_target", &r);
            assert_eq!(out["grip"], grip);
            if link.is_empty() {
                assert_eq!(out["loader"], loader);
                assert!(out.get("link").is_none());
            } else {
                assert_eq!(out["link"], link);
                assert!(out.get("loader").is_none());
            }
        }
    }

    /// **A ROW SAYS WHEN AND BY WHICH ENGINE IT WAS MEASURED**, and a row
    /// without either says nothing rather than a value nobody recorded.
    #[test]
    fn a_row_carries_its_minute_and_a_clockless_row_omits_it() {
        let v = page_row("standard_single_target", &row(None));
        assert_eq!(v["measured_at"], json!("2026-10-05T13:52Z"), "{v}");
        assert_eq!(v["measured_by"], json!("67e27b73c3ee"), "{v}");
        let official = Row { measured_by: "67e27b73c3ee".into(), ..row(None) };
        let v = page_row("standard_single_target", &official);
        assert_eq!(v["measured_by"], json!("67e27b73c3ee"), "an official fact names its engine bare: {v}");
        let commit = Row { measured_by: "4f892ee6e4c1a2b3c4d5e6f708192a3b4c5d6e7f".into(), ..row(None) };
        let v = page_row("standard_single_target", &commit);
        assert!(v.get("measured_by").is_none(), "a commit is no engine id: {v}");
        let none = Row { measured_at: String::new(), measured_by: "migrated".into(), ..row(None) };
        let v = page_row("standard_single_target", &none);
        assert!(v.get("measured_at").is_none() && v.get("measured_by").is_none(), "{v}");
        assert!(v.get("unverified").is_none(), "a scorer's row is not a client's: {v}");
        let client = Row { measured_by: "client:r42".into(), ..row(None) };
        let v = page_row("standard_single_target", &client);
        assert_eq!(v["unverified"], json!(true), "a client's number must say so: {v}");
        assert!(v.get("measured_by").is_none(), "a client is no engine to cite: {v}");
    }

    /// **THE RIVEN REACHES THE PAGE**, which is the whole of what went wrong.
    ///
    /// A row wearing one was written to the page's file without it for as
    /// long as the writer existed, and the board simply held none until
    /// 2026-08-24 — so nothing was ever visibly broken until the hour the first
    /// riven build landed, and then three unrelated-looking things were.
    #[test]
    fn a_riven_row_carries_its_riven_to_the_page() {
        let v = page_row(
            "standard_single_target",
            &row(Some(RowRiven {
                bonuses: vec!["critical_chance".into(), "multishot".into()],
                malus: Some("zoom".into()),
                rolls: vec![1.1, 1.1, 0.9],
            })),
        );
        let rv = v.get("riven").expect("the riven reaches the page");
        assert_eq!(rv["bonuses"], json!(["critical_chance", "multishot"]));
        assert_eq!(rv["malus"], json!("zoom"));
        // THE ROLLS TOO: taking a board row has to give the reader that riven,
        // and a shape without its corner is a card they cannot build.
        assert_eq!(rv["rolls"], json!([1.1, 1.1, 0.9]));
    }

    /// …AND A PLAIN ROW OMITS THE KEY RATHER THAN WRITING `null`.
    ///
    /// Not a style choice: this file is compared BYTE FOR BYTE against
    /// `build_site_app.py`'s output, which is what makes a local site build a
    /// no-op against the scorer. The Python side copies the yaml entry and a
    /// plain entry has no `riven` key at all, so a `null` here would leave
    /// every local build dirty.
    #[test]
    fn a_plain_row_omits_the_key_entirely() {
        let v = page_row("standard_single_target", &row(None));
        assert!(v.get("riven").is_none(), "{v}");
        // …and the fields a page reads by name are all still there, so the
        // extraction did not quietly drop one of the other eight.
        for k in [
            "benchmark",
            "mode",
            "score",
            "shown",
            "mods",
            "evolutions",
            "arcanes",
            "valence",
        ] {
            assert!(v.get(k).is_some(), "{k} is missing");
        }
    }
}
