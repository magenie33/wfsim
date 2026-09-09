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

fn flag(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

/// WHAT ONE PASS OF THE INBOX PRODUCES: the builds, and the inbox ids that may
/// now be deleted.
///
/// A FUNCTION rather than a loop inside `main`, because the properties worth
/// asserting are all about this: which records collapse onto one build, which
/// do not, and what a refusal does with the row.
fn intake(lines: impl Iterator<Item = String>) -> (Vec<Value>, Vec<String>, usize, usize) {
    // ONE ROW PER BUILD, DEDUPED HERE TOO. Two inbox rows can be the same build
    // — a resubmission is a second row in a queue on purpose — and emitting it
    // twice would spend two writes on one row for no reason.
    let mut built: BTreeMap<String, Value> = BTreeMap::new();
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
        let at = id(&row, "at");
        let key = wfsim_engine::builds::build_id(&v);
        built
            .entry(key.clone())
            .or_insert_with(|| json!({ "id": key, "at": at, "record": canonical(&v) }));
    }
    (built.into_values().collect(), done, seen, refused)
}

fn main() {
    let done_path = flag("--done");
    let (builds, done, seen, refused) =
        intake(std::io::stdin().lock().lines().map_while(Result::ok));

    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for row in &builds {
        if writeln!(out, "{row}").is_err() {
            break;
        }
    }
    let _ = out.flush();

    if let Some(path) = done_path {
        if let Err(e) = std::fs::write(&path, done.join("
")) {
            eprintln!("intake: cannot write {path}: {e}");
            std::process::exit(1);
        }
    }
    eprintln!(
        "intake: {seen} record(s) in, {refused} refused, {} build(s) out",
        builds.len()
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
        let (builds, done, seen, refused) = intake(
            vec![
                json!({"id":"bad","at":"d","record":{"weapon":"not_a_weapon","mods":[]}}).to_string(),
                row("good", &FULL, json!({})),
            ]
            .into_iter(),
        );
        assert_eq!((seen, refused, builds.len()), (2, 1, 1));
        assert_eq!(done, vec!["bad", "good"], "both rows are spent");
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
}
