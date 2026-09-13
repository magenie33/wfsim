//! THE DOOR'S SECOND HALF — inbox records in, library builds out.
//!
//!   cat inbox.ndjson | wfsim-intake --done done.txt > builds.ndjson
//!
//! WHY THIS EXISTS AT ALL. The endpoint that takes a submission runs on
//! Cloudflare and has no game data, so it cannot say what a build IS: telling
//! two apart needs the mod POOL, because an elemental card enters the element
//! sequence and a plain one does not, and the sequence decides the pairing
//! (Torid, six mods: 12,424 DPS against 46,583). A key derived without it would
//! be a SECOND answer to the one question that must have one — and it would be
//! the half with no evidence. So the door stores the record verbatim and this
//! derives the key, with the engine.
//!
//! AND THE CLIENT MAY NOT DERIVE IT EITHER, though it has the engine: an id
//! that arrives over the wire is an id an attacker chooses, and choosing one is
//! choosing which stored build to overwrite.
//!
//! WHAT IT DOES NOT DO IS ADMIT. `validate_with` is the LEGALITY door — could a
//! player equip this — and admission is the RULER's, asked per ruler in the
//! scorer. The store is a library of builds and every ruler crosses the whole
//! of it, so a build no current ruler would take is still worth keeping: the
//! next ruler is scored from the library the day it lands.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use serde_json::{json, Value};

/// The ids a record carries under `key`, in order.
fn ids(rec: &Value, key: &str) -> Vec<String> {
    rec.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default()
}

fn id(rec: &Value, key: &str) -> String {
    rec.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// THE RIVEN A RECORD STATES, as a shape. `riven_pos` empty means no riven —
/// a malus alone is not one.
fn riven_of(rec: &Value) -> Option<wfsim_engine::rivens_data::RivenShape> {
    let bonuses = ids(rec, "riven_pos");
    if bonuses.is_empty() {
        return None;
    }
    let malus = id(rec, "riven_neg");
    Some(wfsim_engine::rivens_data::RivenShape {
        bonuses,
        malus: Some(malus).filter(|m| !m.is_empty()),
    })
}

/// THE CANONICAL BUILD, as the library stores one.
///
/// WRITTEN FROM THE `ValidBuild` and not copied from the submission: what goes
/// in is what the engine made of it — the mod order canonicalised to one
/// representative per pairing, the evolution ladder truncated to what the
/// weapon has, the arcane list padded to its seats. A record that kept the
/// submitted spelling would key one way and simulate another.
///
/// `mode` DOES NOT SURVIVE. It is where the submitter happened to be standing;
/// mods are equipped on the WEAPON and a mode is how it is fired, so nothing
/// about a build can become a different build by being played differently. It
/// was in the door's key until now, and the same cards sent from two modes were
/// two rows in a table whose whole promise is one row per build.
fn canonical(v: &wfsim_engine::builds::ValidBuild) -> Value {
    let mut rec = json!({
        "weapon": v.weapon,
        "mods": v.mods,
        "evolutions": v.evolutions,
        "arcanes": v.arcanes,
    });
    let o = rec.as_object_mut().expect("object");
    if !v.valence.is_empty() {
        o.insert("valence".into(), json!(v.valence));
    }
    if let Some(x) = &v.exilus {
        o.insert("exilus".into(), json!(x));
    }
    if let Some(a) = &v.assembly {
        o.insert("grip".into(), json!(a.grip));
        o.insert("loader".into(), json!(a.loader));
    }
    if let Some(r) = &v.riven {
        o.insert("riven_pos".into(), json!(r.bonuses));
        if let Some(m) = &r.malus {
            o.insert("riven_neg".into(), json!(m));
        }
    }
    rec
}

/// …AND THE ROLLS THE RIVEN IS STORED WITH. Bonuses first, then the malus,
/// which is the order `RivenShape::at` reads them back in.
///
/// A BUILD WITH A RIVEN IS NOT COMPLETE WITHOUT THEM. The shape says which
/// stats; a fight needs numbers, and a row published without them names a card
/// nobody can go and obtain.
fn with_rolls(mut rec: Value, rolls: &[f64]) -> Value {
    if let Some(o) = rec.as_object_mut() {
        o.insert("riven_rolls".into(), json!(rolls));
    }
    rec
}

/// THE ROLLS A RIVEN IS STORED WITH — the god roll, unless a stat's sign has
/// stopped saying which end of its band is better.
///
/// THE DEFAULT IS THE GOD ROLL AND IT COSTS NOTHING. Every bonus at its ceiling
/// and the malus at its floor is the same rule that scores every row at full
/// Forma, every mod at max rank and every valence at the roll's maximum:
/// anything a player can eventually reach is not part of what a row states.
/// 1,948 of the library's 2,418 riven builds are answered by that sentence and
/// never reach a fight.
///
/// A STAT LOSES ITS SIGN FOR A LISTED REASON — `rivens_data::ambiguous_stats`
/// holds both sources and there is no third. Only those stats are asked about,
/// both ends, everything else pinned at the god roll.
///
/// AND EACH `(ruler, mode)` ANSWERS FOR ITSELF. One ruler cannot speak for
/// another and the corner that wins a crowd need not win one target, so what
/// comes back is the SET — usually one, and then a riven is a build like any
/// other.
fn corners_for(v: &wfsim_engine::builds::ValidBuild) -> Vec<Vec<f64>> {
    let Some(shape) = &v.riven else { return Vec::new() };
    let Some(class) = wfsim_engine::rivens_data::class_for_weapon(&v.weapon) else {
        return Vec::new();
    };
    let rolls_of = |spec: &wfsim_engine::rivens_data::RivenSpec| -> Vec<f64> {
        spec.bonuses.iter().map(|b| b.roll).chain(spec.malus.iter().map(|m| m.roll)).collect()
    };
    let ambiguous = wfsim_engine::rivens_data::ambiguous_stats(shape, &v.weapon);
    if ambiguous.is_empty() {
        return vec![rolls_of(&wfsim_engine::rivens_data::god_roll(shape, class))];
    }

    let modes: Vec<wfsim_engine::weapons_data::WeaponPlayMode> =
        wfsim_engine::weapons_data::play_modes(&v.weapon)
            .into_iter()
            .filter(|m| m.sustainable)
            .collect();
    let mut found: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for bench in wfsim_engine::benchmarks_data::all() {
        let metric = bench.metric();
        let scenario: Value = serde_json::to_value(&bench.scenario).expect("scenario");
        let duration = scenario.get("duration").and_then(Value::as_f64).unwrap_or(300.0);
        // THE MEAN AND ITS OWN STANDARD ERROR, which is what the response
        // carries them for: `metric.field` is the MEDIAN run, the right
        // headline for "what a fight looks like" and the wrong number to rank
        // two cards by. The ruler's run count is a term of the scenario, so the
        // request already carries it and nothing here overrides it — at a
        // cheaper count the answer is about the probe rather than about the
        // cards.
        let (mean_of, se_of) =
            (format!("{}_mean", metric.field), format!("{}_se", metric.field));
        for played in &modes {
            let base = wfsim_webapi::simulate_request(&scenario, v, *played);
            let best =
                wfsim_engine::rivens_data::best_roll(shape, class, &ambiguous, |spec| {
                    let mut req = base.clone();
                    if let Some(o) = req.as_object_mut() {
                        o.insert("rivens".into(), wfsim_webapi::riven_request(spec));
                    }
                    let out = wfsim_webapi::simulate_json(&req);
                    if !out.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                        return None;
                    }
                    // A FIGHT THAT REPORTS NO SPREAD IS NOT ASKED, and the god
                    // roll stands: without one there is nothing to say whether
                    // a gap is a difference or a draw.
                    let read = |k: &str| out.get(k).and_then(Value::as_f64);
                    Some((
                        metric.of(read(&mean_of)?, duration),
                        metric.of(read(&se_of)?, duration),
                    ))
                });
            // KEYED ON THE ROLLS THEMSELVES, so two rulers landing on one card
            // leave one build. The text is only a key; the numbers are the value.
            let rolls = rolls_of(&best);
            found.insert(format!("{rolls:?}"), rolls);
        }
    }
    found.into_values().collect()
}

fn flag(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

/// THE FIGHT AN ARRIVAL NAMED, as a queue row — and this is the only place in
/// the pipeline that can say it.
///
/// A BUILD HAS NO RULER AND NO MODE: `canonical` drops both on purpose, because
/// mods are equipped on the WEAPON and a mode is how it is fired. So the fight
/// a submitter actually ran is a property of the ARRIVAL, and it exists only
/// here, for the length of this pass. Asked for now, it is the one row a build
/// nothing has measured is owed; unasked, the reconciliation falls back to
/// every row the build could have, which is the entry line paying 84 fights
/// where one would do.
///
/// IT IS NOT GATED, and that is what it is for: somebody pressing upload is a
/// person asking, so a build the entry line parked is re-measured on the fight
/// they just ran it in. A mechanic that changed makes it come back by itself.
/// `sent_ruler` and `sent_mode` are the INBOX record's, taken before the
/// canonical one shadows it — that one has neither, which is the whole point.
fn asked_row(
    sent_ruler: &str,
    sent_mode: &str,
    v: &wfsim_engine::builds::ValidBuild,
    key: &str,
) -> Option<Value> {
    // A RULER THE ROSTER STILL HAS, by family — a `_vN` record names the ruler
    // it is a version of. One it no longer has asks for nothing: `purge_queue`
    // would delete the row and the reconciliation covers the build anyway.
    let ruler = wfsim_engine::benchmarks_data::all()
        .iter()
        .map(|b| b.id.clone())
        .find(|b| benchmark_family(b) == benchmark_family(sent_ruler))?;
    // …AND A MODE THE WEAPON CAN SUSTAIN. The scorer enumerates sustainable
    // modes and matches the queue on that id, so a row naming any other mode is
    // a row nothing will ever take.
    let modes: Vec<String> = wfsim_engine::weapons_data::play_modes(&v.weapon)
        .into_iter()
        .filter(|m| m.sustainable)
        .map(|m| m.id.to_string())
        .collect();
    let mode = modes.iter().find(|m| *m == sent_mode).or_else(|| modes.first())?;
    Some(json!({ "build_id": key, "ruler": ruler, "mode": mode }))
}

/// A benchmark id without its `_v<n>` suffix — the same rule `wfsim-board`
/// applies, so a record naming an older version asks for the current ruler.
fn benchmark_family(id: &str) -> &str {
    id.rsplit_once("_v")
        .filter(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .map(|(base, _)| base)
        .unwrap_or(id)
}

/// WHAT ONE PASS OF THE INBOX PRODUCES: the builds, the fights the arrivals
/// named, and the inbox ids that may now be deleted.
///
/// A FUNCTION rather than a loop inside `main`, because the properties worth
/// asserting are all about this: which records collapse onto one build, which
/// do not, and what a refusal does with the row.
fn intake(
    lines: impl Iterator<Item = String>,
) -> (Vec<Value>, Vec<Value>, Vec<String>, usize, usize) {
    // ONE ROW PER BUILD, DEDUPED HERE TOO. Two inbox rows can be the same build
    // — a resubmission is a second row in a queue on purpose — and emitting it
    // twice would spend two writes on one row for no reason.
    let mut built: BTreeMap<String, Value> = BTreeMap::new();
    // …AND THE FIGHT EACH ARRIVAL NAMED, deduped the same way: one row per
    // (build, ruler, mode), because two people sending one build from one board
    // are asking for one fight.
    let mut asked: BTreeMap<String, Value> = BTreeMap::new();
    // WHAT MAY BE DELETED FROM THE INBOX. A refused record is DONE, not
    // retried: it will never become legal, and a queue that keeps what it
    // cannot use grows for ever.
    let mut done: Vec<String> = Vec::new();
    let (mut seen, mut refused) = (0usize, 0usize);

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else { continue };
        // THE RECORD TRAVELS AS AN OBJECT OR AS TEXT, because the database
        // column is text and a local file may hold either.
        let rec = match row.get("record") {
            Some(Value::String(s)) => serde_json::from_str::<Value>(s).unwrap_or(Value::Null),
            Some(v) => v.clone(),
            None => Value::Null,
        };
        if !rec.is_object() {
            continue;
        }
        seen += 1;
        done.push(id(&row, "id"));

        let weapon = id(&rec, "weapon");
        let riven = riven_of(&rec);
        let assembly = wfsim_engine::kitguns_data::assembly_of(
            &weapon,
            &id(&rec, "grip"),
            &id(&rec, "loader"),
        );
        let v = match wfsim_engine::builds::validate_with(
            &weapon,
            &ids(&rec, "mods"),
            &ids(&rec, "evolutions"),
            &ids(&rec, "arcanes"),
            &id(&rec, "valence"),
            riven.as_ref(),
            Some(id(&rec, "exilus")).filter(|x| !x.is_empty()).as_deref(),
            assembly.as_ref(),
        ) {
            Ok(v) => v,
            // THE REASON IS PRINTED, not counted. "2 refused" tells nobody
            // anything, including the person who submitted them.
            Err(why) => {
                eprintln!("refused {weapon}: {why}");
                refused += 1;
                continue;
            }
        };
        // THE FIGHT THIS ARRIVAL NAMED, read while the INBOX record is still in
        // scope: the canonical one shadows it below and carries neither field.
        let sent = (id(&rec, "benchmark"), id(&rec, "mode"));
        let at = id(&row, "at");
        // A RIVEN BECOMES ITS CORNERS, and a build without one is itself. The
        // record that arrives states a SHAPE; what is stored is a build a player
        // could go and assemble, which needs numbers.
        let corners = corners_for(&v);
        if v.riven.is_some() && corners.is_empty() {
            eprintln!("refused {weapon}: its riven names a shape this engine cannot resolve");
            refused += 1;
            continue;
        }
        let variants: Vec<Vec<f64>> = if corners.is_empty() { vec![Vec::new()] } else { corners };
        for rolls in variants {
            // THE ROLLS GO ON THE BUILD BEFORE THE KEY IS TAKEN. They are part
            // of the fight, so they are part of the identity the id hashes —
            // two ends of one shape are two builds with two numbers.
            let v = v.clone().with_riven_rolls(rolls.clone());
            let key = wfsim_engine::builds::build_id(&v);
            let rec = if rolls.is_empty() {
                canonical(&v)
            } else {
                with_rolls(canonical(&v), &rolls)
            };
            // …AND WHICH QUEUE ROWS PRODUCED IT. Two records can be one build —
            // a resubmission, or two spellings of one pairing — so the build
            // knows what it came from, and the write that lands it is the write
            // that spends them. It is not stored: it is true of this pass, not
            // of the build.
            let entry = built
                .entry(key.clone())
                .or_insert_with(|| json!({ "id": key, "at": at, "record": rec, "from": [] }));
            if let Some(from) = entry.get_mut("from").and_then(Value::as_array_mut) {
                let was = json!(id(&row, "id"));
                if !from.contains(&was) {
                    from.push(was);
                }
            }
            // THE FIGHT THEY RAN IT IN. `sent` was taken before the canonical
            // record shadowed the inbox one, which carries neither field.
            if let Some(ask) = asked_row(&sent.0, &sent.1, &v, &key) {
                asked.insert(
                    format!("{key}#{}#{}", id(&ask, "ruler"), id(&ask, "mode")),
                    ask,
                );
            }
        }
    }
    (built.into_values().collect(), asked.into_values().collect(), done, seen, refused)
}

fn main() {
    let done_path = flag("--done");
    let asked_path = flag("--asked");
    let (builds, asked, done, seen, refused) =
        intake(std::io::stdin().lock().lines().map_while(Result::ok));

    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for row in &builds {
        if writeln!(out, "{row}").is_err() {
            break;
        }
    }
    let _ = out.flush();

    if let Some(path) = done_path {
        if let Err(e) = std::fs::write(&path, done.join("\n")) {
            eprintln!("intake: cannot write {path}: {e}");
            std::process::exit(1);
        }
    }
    // THE FILE IS WRITTEN EVEN WHEN IT IS EMPTY. The shipper reads it, and an
    // absent file and an empty one are the same thing to it — but an absent one
    // after a pass that took records in is a pass that lost them.
    if let Some(path) = asked_path {
        let body: String = asked.iter().map(|r| format!("{r}\n")).collect();
        if let Err(e) = std::fs::write(&path, body) {
            eprintln!("intake: cannot write {path}: {e}");
            std::process::exit(1);
        }
    }
    eprintln!(
        "intake: {seen} record(s) in, {refused} refused, {} build(s) out, {} fight(s) asked for",
        builds.len(),
        asked.len(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record as the door stores one, around a mod list.
    fn row(inbox_id: &str, mods: &[&str], extra: Value) -> String {
        let mut rec = json!({ "weapon": "torid", "mods": mods });
        if let (Some(o), Some(e)) = (rec.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                o.insert(k.clone(), v.clone());
            }
        }
        json!({ "id": inbox_id, "at": "2026-01-01", "record": rec }).to_string()
    }

    const FULL: [&str; 8] = [
        "galvanized_chamber",
        "primed_firestorm",
        "galvanized_scope",
        "vital_sense",
        "magnetic_capacity",
        "primed_cryo_rounds",
        "malignant_force",
        "hellfire",
    ];

    fn ids_of(lines: Vec<String>) -> Vec<String> {
        intake(lines.into_iter())
            .0
            .iter()
            .map(|b| b["id"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    /// THE MODE A SUBMITTER HAPPENED TO BE IN IS NOT A BUILD.
    ///
    /// Mods are equipped on the WEAPON and a mode is how it is fired, so nothing
    /// about a build can become a different build by being played differently —
    /// and one submission is already scored in every mode the weapon sustains.
    /// The door's own key carried `mode` until this file existed, so the same
    /// cards sent from two modes were two rows in a table whose whole promise is
    /// one row per build.
    #[test]
    fn one_build_sent_from_two_modes_is_one_build() {
        let ids = ids_of(vec![
            row("a", &FULL, json!({ "mode": "base" })),
            row("b", &FULL, json!({ "mode": "cycle" })),
            row("c", &FULL, json!({})),
        ]);
        assert_eq!(ids.len(), 1, "three records, one build: {ids:?}");
        // …AND THE MODE IS NOT WRITTEN DOWN EITHER, or the record would state a
        // property the build has not got.
        let (builds, ..) = intake(vec![row("a", &FULL, json!({ "mode": "cycle" }))].into_iter());
        assert!(builds[0]["record"].get("mode").is_none(), "{}", builds[0]);
    }

    /// A RESUBMISSION IS ONE ROW, and the id is what makes it one — derived from
    /// the build, so nothing has to read the table to find out.
    #[test]
    fn the_same_build_twice_is_one_build() {
        let ids = ids_of(vec![row("a", &FULL, json!({})), row("b", &FULL, json!({}))]);
        assert_eq!(ids.len(), 1, "{ids:?}");
    }

    /// THE PAIRING IS THE BUILD, and the ORDER is how it is stated.
    ///
    /// Mods combine elements in the order they sit in, so a list that pairs the
    /// same way is one build however it is spelled — and a list that pairs
    /// differently is another one. Measured on this weapon: Heat/Cold/Toxin is
    /// Blast plus Toxin where Cold/Toxin/Heat is Viral plus Heat, 12,424 DPS
    /// against 46,583.
    #[test]
    fn two_spellings_of_one_pairing_are_one_build_and_two_pairings_are_two() {
        let head = &FULL[..5];
        let same: Vec<&str> = head
            .iter()
            .chain(["malignant_force", "primed_cryo_rounds", "hellfire"].iter())
            .copied()
            .collect();
        let other: Vec<&str> = head
            .iter()
            .chain(["hellfire", "primed_cryo_rounds", "malignant_force"].iter())
            .copied()
            .collect();
        assert_eq!(
            ids_of(vec![row("a", &FULL, json!({})), row("b", &same, json!({}))]).len(),
            1,
            "Cold+Toxin and Toxin+Cold are both Viral"
        );
        assert_eq!(
            ids_of(vec![row("a", &FULL, json!({})), row("b", &other, json!({}))]).len(),
            2,
            "Heat+Cold is Blast, and that is a different fight"
        );
    }

    /// AN ID IS THE BUILD'S OWN, AND IT IS STABLE. Two passes agree, and it is
    /// the shape a database key has to be.
    #[test]
    fn the_id_is_derived_and_stable() {
        let one = ids_of(vec![row("a", &FULL, json!({}))]);
        let two = ids_of(vec![row("b", &FULL, json!({}))]);
        assert_eq!(one, two, "the same build hashes the same twice");
        assert_eq!(one[0].len(), 32, "128 bits of hex: {}", one[0]);
        assert!(one[0].chars().all(|c| c.is_ascii_hexdigit()), "{}", one[0]);
    }

    /// A RECORD NOBODY COULD EQUIP IS DONE, NOT RETRIED. It will never become
    /// legal, and a queue that keeps what it cannot use grows for ever.
    #[test]
    fn a_refused_record_leaves_the_inbox() {
        let (builds, _, done, seen, refused) = intake(
            vec![
                json!({"id":"bad","at":"d","record":{"weapon":"not_a_weapon","mods":[]}}).to_string(),
                row("good", &FULL, json!({})),
            ]
            .into_iter(),
        );
        assert_eq!((seen, refused, builds.len()), (2, 1, 1));
        assert_eq!(done, vec!["bad", "good"], "both rows are spent");
    }

    /// A RIVEN ARRIVES AS A SHAPE AND LEAVES AS A BUILD.
    ///
    /// What a player sends is which stats the card rolled; what a fight needs is
    /// numbers, and which END of the band is best is the fight's answer rather
    /// than the sign on the card. So every corner is asked, on every ruler and
    /// every mode, and what is stored is the winner — or the winnerS, when two
    /// fights disagree.
    ///
    /// THE ROLLS ARE IN THE ID, and the assertion below is why: two ends of one
    /// shape are two builds with two numbers, and an id that could not tell them
    /// apart would file the second under the first's.
    ///
    /// IT RUNS FIGHTS, which is what makes it slow and what makes it worth
    /// having — the alternative is a per-stat table that is wrong on the three
    /// weapons whose Incarnon pays for NOT critting.
    #[test]
    fn a_riven_is_stored_with_the_numbers_a_fight_chose() {
        let rec = json!({
            "weapon": "braton_prime",
            "mods": ["serration", "split_chamber", "riven"],
            "riven_pos": ["critical_damage", "damage"],
            "riven_neg": "zoom",
        });
        let line = json!({ "id": "u1", "at": "2026-01-01", "record": rec }).to_string();
        let (builds, .., refused) = intake(vec![line].into_iter());
        assert_eq!(refused, 0, "a legal riven build is not refused");
        assert!(!builds.is_empty(), "a riven shape resolves to at least one build");

        for b in &builds {
            let rolls = b["record"]["riven_rolls"].as_array().expect("rolls are stored");
            assert_eq!(rolls.len(), 3, "two bonuses and a malus: {rolls:?}");
            // EVERY CORNER IS AN END OF THE BAND, never something between.
            for r in rolls {
                let r = r.as_f64().unwrap_or_default();
                assert!(r == 0.9 || r == 1.1, "a corner is an end of the band: {r}");
            }
            // A STAT THE FIGHT CANNOT READ GOES TO THE PLAYER. Zoom scores the
            // same at both ends against one standing target, and the board is
            // publishing a card somebody will go and try to obtain.
            assert_eq!(rolls[2].as_f64(), Some(0.9), "the malus is at its floor: {rolls:?}");
        }
        // …AND TWO CORNERS ARE TWO IDS, which is what keeps them apart.
        let ids: std::collections::BTreeSet<&str> =
            builds.iter().filter_map(|b| b["id"].as_str()).collect();
        assert_eq!(ids.len(), builds.len(), "each corner has its own id");
    }

    /// ADMISSION IS THE RULER'S, NOT THIS FILE'S. A thin build is legal to
    /// equip and is kept: the store is a library and every ruler crosses the
    /// whole of it, so a build no current ruler would take is what the NEXT
    /// ruler is scored from the day it lands.
    #[test]
    fn a_build_no_ruler_would_admit_is_still_a_build() {
        let ids = ids_of(vec![row("a", &["serration"], json!({}))]);
        assert_eq!(ids.len(), 1, "one legal mod is a legal build");
    }

    fn asked_of(lines: Vec<String>) -> Vec<Value> {
        intake(lines.into_iter()).1
    }

    /// **THE FIGHT THE SUBMITTER RAN IS ASKED FOR**, and it is the only thing
    /// that can be: `canonical` drops the ruler and the mode, so after this
    /// pass nothing in the pipeline knows what board they were on.
    #[test]
    fn an_arrival_asks_for_the_fight_it_named() {
        let asked = asked_of(vec![row(
            "a",
            &FULL,
            json!({ "benchmark": "group_clear", "mode": "cycle" }),
        )]);
        assert_eq!(asked.len(), 1, "one arrival, one fight");
        assert_eq!(asked[0]["ruler"], "group_clear");
        assert_eq!(asked[0]["mode"], "cycle");
    }

    /// **TWO PEOPLE SENDING ONE BUILD FROM ONE BOARD ASK FOR ONE FIGHT**, and
    /// from two boards for two — the row is keyed by what makes it a different
    /// measurement, exactly as `scores` is.
    #[test]
    fn one_fight_per_build_ruler_and_mode() {
        let asked = asked_of(vec![
            row("a", &FULL, json!({ "benchmark": "group_clear", "mode": "cycle" })),
            row("b", &FULL, json!({ "benchmark": "group_clear", "mode": "cycle" })),
            row("c", &FULL, json!({ "benchmark": "single_target", "mode": "cycle" })),
        ]);
        assert_eq!(asked.len(), 2, "{asked:?}");
    }

    /// **A MODE THE WEAPON CANNOT SUSTAIN IS NOT ASKED FOR.** The scorer
    /// enumerates sustainable modes and matches the queue on that id, so a row
    /// naming any other is a row nothing will ever take — and the build would
    /// wait for a fight that never comes.
    #[test]
    fn an_unsustainable_mode_falls_back_to_one_that_is() {
        let asked = asked_of(vec![row(
            "a",
            &FULL,
            json!({ "benchmark": "single_target", "mode": "transformed" }),
        )]);
        assert_eq!(asked.len(), 1);
        let modes: Vec<String> = wfsim_engine::weapons_data::play_modes("torid")
            .into_iter()
            .filter(|m| m.sustainable)
            .map(|m| m.id.to_string())
            .collect();
        let got = asked[0]["mode"].as_str().unwrap_or_default().to_string();
        assert!(modes.contains(&got), "{got} is not sustainable on the Torid");
    }

    /// **A RETIRED RULER ASKS FOR NOTHING.** `purge_queue` would delete the row
    /// and the reconciliation covers the build anyway, so a record naming a
    /// board the roster no longer has is not a build left waiting.
    #[test]
    fn a_ruler_the_roster_lost_asks_for_nothing() {
        let asked = asked_of(vec![row(
            "a",
            &FULL,
            json!({ "benchmark": "a_board_that_was_retired", "mode": "base" }),
        )]);
        assert!(asked.is_empty(), "{asked:?}");
    }

    /// **AN OLDER VERSION OF A LIVE RULER IS THAT RULER.** Records in the store
    /// name `single_target_v1`, and a build aimed at it belongs on the current
    /// one's board — the same `_vN` rule the scorer applies.
    #[test]
    fn an_older_version_of_a_ruler_asks_for_the_current_one() {
        let asked = asked_of(vec![row(
            "a",
            &FULL,
            json!({ "benchmark": "single_target_v1", "mode": "base" }),
        )]);
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert_eq!(asked[0]["ruler"], "single_target");
    }
}
