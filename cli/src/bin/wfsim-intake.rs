// SPDX-License-Identifier: AGPL-3.0-or-later
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
use wfsim_cli::args::flag;
use wfsim_engine::board::benchmarks::family;

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
fn riven_of(rec: &Value) -> Option<wfsim_engine::build::rivens::RivenShape> {
    let bonuses = ids(rec, "riven_pos");
    if bonuses.is_empty() {
        return None;
    }
    let malus = id(rec, "riven_neg");
    Some(wfsim_engine::build::rivens::RivenShape {
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
fn canonical(v: &wfsim_engine::board::builds::ValidBuild) -> Value {
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
/// stopped saying which end of its band is better — AND THE RANK EACH CARD ON
/// THE EVERY-RANK LIST IS STORED AT, max unless a lower one is better.
///
/// THE DEFAULT IS THE GOD ROLL AT MAX RANK AND IT COSTS NOTHING. Every bonus at
/// its ceiling and the malus at its floor is the same rule that scores every
/// row at full Forma and every valence at the roll's maximum: anything a player
/// can eventually reach is not part of what a row states.
///
/// A STAT LOSES ITS SIGN FOR A LISTED REASON (`build::rivens::ambiguous_stats`),
/// AND A CARD LOSES ITS MAX RANK ONLY BY BEING NAMED in
/// `data/search/every_rank.yaml`. Only those are asked, every combination of
/// them: a Status Duration malus at its deep end is off the -100% cliff, and a
/// low-rank Hunter Track is what lifts it back above.
///
/// AND EACH `(ruler, mode)` ANSWERS FOR ITSELF. One ruler cannot speak for
/// another and the corner that wins a crowd need not win one target, so what
/// comes back is the SET — usually one build. Empty is a riven this engine
/// cannot resolve.
fn corners_for(v: &wfsim_engine::board::builds::ValidBuild) -> Vec<wfsim_engine::board::builds::ValidBuild> {
    let class = wfsim_engine::build::rivens::class_for_weapon(&v.weapon);
    let rolls: Vec<Vec<f64>> = match (&v.riven, class) {
        (None, _) => vec![Vec::new()],
        (Some(_), None) => return Vec::new(),
        (Some(shape), Some(_)) => wfsim_engine::build::rivens::corners(
            shape,
            &wfsim_engine::build::rivens::ambiguous_stats(shape, &v.weapon),
        ),
    };
    let mods = rank_choices(v);
    let default = (rolls[0].clone(), mods[0].clone());
    let alternatives: Vec<(Vec<f64>, Vec<String>)> = rolls
        .iter()
        .flat_map(|r| mods.iter().map(move |m| (r.clone(), m.clone())))
        .filter(|c| *c != default)
        .collect();
    let with = |c: &(Vec<f64>, Vec<String>)| {
        let mut b = v.clone().with_riven_rolls(c.0.clone());
        b.mods = c.1.clone();
        b
    };
    if alternatives.is_empty() {
        return vec![with(&default)];
    }
    let spec_of = |r: &[f64]| v.riven.as_ref().zip(class).map(|(shape, class)| shape.at(class, r));

    let modes: Vec<wfsim_engine::data::weapons::WeaponPlayMode> =
        wfsim_engine::data::weapons::play_modes(&v.weapon)
            .into_iter()
            .filter(|m| m.sustainable)
            .collect();
    let mut found: BTreeMap<String, (Vec<f64>, Vec<String>)> = BTreeMap::new();
    for bench in wfsim_engine::board::benchmarks::all() {
        let metric = bench.metric();
        let scenario: Value = serde_json::to_value(&bench.scenario).expect("scenario");
        let duration = scenario.get("duration").and_then(Value::as_f64).unwrap_or(300.0);
        for played in &modes {
            let best = wfsim_engine::build::rivens::perfect(
                default.clone(),
                alternatives.iter().cloned(),
                |c| {
                    let mut req = wfsim_webapi::simulate_request(&scenario, &with(c), *played);
                    if let (Some(o), Some(spec)) = (req.as_object_mut(), spec_of(&c.0)) {
                        o.insert("rivens".into(), wfsim_webapi::riven_request(&spec));
                    }
                    score_of(&wfsim_webapi::simulate_json(&req), metric, duration)
                },
            );
            // KEYED ON THE CORNER ITSELF, so two rulers landing on one card
            // leave one build. The text is only a key; the corner is the value.
            found.insert(format!("{best:?}"), best);
        }
    }
    found.values().map(with).collect()
}

/// A FIGHT'S ANSWER IN THE RULER'S UNITS, AND ITS OWN STANDARD ERROR.
///
/// `metric.field` on the wire IS the mean over the runs — the scorer's own
/// reading — and `<field>_se` sits beside it. The ruler's run count is a term
/// of the scenario, so the request already carries it. A fight that did not
/// run, or reports no spread, answers None and the default stands: without a
/// spread nothing says whether a gap is a difference or a draw.
fn score_of(
    out: &Value,
    metric: &wfsim_engine::rules::metrics::MetricDef,
    duration: f64,
) -> Option<(f64, f64)> {
    if !out.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let read = |k: &str| out.get(k).and_then(Value::as_f64);
    Some((
        metric.of(read(metric.field)?, duration),
        metric.of(read(&format!("{}_se", metric.field))?, duration),
    ))
}

/// EVERY MOD LIST A BUILD MAY BE STORED WITH, the build as it stands first:
/// each card the every-rank list names at each of its ranks, crossed.
fn rank_choices(v: &wfsim_engine::board::builds::ValidBuild) -> Vec<Vec<String>> {
    let listed = &wfsim_engine::data::mods::every_rank().mods;
    let pool = wfsim_engine::data::mods::pool_for_weapon(&v.weapon);
    let mut out: Vec<Vec<String>> = vec![v.mods.clone()];
    for (i, id) in v.mods.iter().enumerate() {
        let Some(card) = pool.iter().find(|m| m.id == id.as_str() && listed.iter().any(|l| l == m.id))
        else {
            continue;
        };
        let lower: Vec<String> = (0..card.max_rank)
            .map(|r| wfsim_engine::data::mods::ranked_id(card.id, r, card.max_rank))
            .filter(|r| wfsim_engine::data::mods::at_rank(r).is_some())
            .collect();
        out = out
            .into_iter()
            .flat_map(|m| {
                let at: Vec<Vec<String>> = lower
                    .iter()
                    .map(|r| {
                        let mut x = m.clone();
                        x[i] = r.clone();
                        x
                    })
                    .collect();
                std::iter::once(m).chain(at)
            })
            .collect();
    }
    out
}

/// A CARD'S RANK IS NOT PART OF WHAT ARRIVES: every card is stored at max rank,
/// and [`corners_for`] asks the fight about the ones the every-rank list names.
fn at_max_rank(id: &str) -> String {
    wfsim_engine::data::mods::split_rank(id).0.to_string()
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
    v: &wfsim_engine::board::builds::ValidBuild,
    key: &str,
) -> Option<Value> {
    // A RULER THE ROSTER STILL HAS, by family — a `_vN` record names the ruler
    // it is a version of. One it no longer has asks for nothing: `purge_queue`
    // would delete the row and the reconciliation covers the build anyway.
    let ruler = wfsim_engine::board::benchmarks::all()
        .iter()
        .map(|b| b.id.clone())
        .find(|b| family(b) == family(sent_ruler))?;
    // …AND A MODE THE WEAPON CAN SUSTAIN. The scorer enumerates sustainable
    // modes and matches the queue on that id, so a row naming any other mode is
    // a row nothing will ever take.
    let modes: Vec<String> = wfsim_engine::data::weapons::play_modes(&v.weapon)
        .into_iter()
        .filter(|m| m.sustainable)
        .map(|m| m.id.to_string())
        .collect();
    let mode = modes.iter().find(|m| *m == sent_mode).or_else(|| modes.first())?;
    Some(json!({ "build_id": key, "ruler": ruler, "mode": mode }))
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
        let assembly = wfsim_engine::data::weapons::kitguns::assembly_of(
            &weapon,
            &id(&rec, "grip"),
            &id(&rec, "loader"),
        );
        let mods: Vec<String> = ids(&rec, "mods").iter().map(|m| at_max_rank(m)).collect();
        let exilus = at_max_rank(&id(&rec, "exilus"));
        let v = match wfsim_engine::board::builds::validate_with(
            &weapon,
            &mods,
            &ids(&rec, "evolutions"),
            &ids(&rec, "arcanes"),
            &id(&rec, "valence"),
            riven.as_ref(),
            Some(exilus).filter(|x| !x.is_empty()).as_deref(),
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
        if corners.is_empty() {
            eprintln!("refused {weapon}: its riven names a shape this engine cannot resolve");
            refused += 1;
            continue;
        }
        for corner in corners {
            // A CORNER'S MODS ARE CANONICALISED AGAIN: a lower rank drains
            // less, and the representative orders plain cards by drain.
            let v = match wfsim_engine::board::builds::validate_with(
                &corner.weapon,
                &corner.mods,
                &corner.evolutions,
                &corner.arcanes,
                &corner.valence,
                corner.riven.as_ref(),
                corner.exilus.as_deref(),
                corner.assembly.as_ref(),
            ) {
                Ok(b) => b.with_riven_rolls(corner.riven_rolls.clone()),
                Err(why) => {
                    eprintln!("refused {weapon} at {:?}: {why}", corner.mods);
                    continue;
                }
            };
            let rolls = v.riven_rolls.clone();
            // THE ROLLS GO ON THE BUILD BEFORE THE KEY IS TAKEN. They are part
            // of the fight, so they are part of the identity the id hashes —
            // two ends of one shape are two builds with two numbers.
            let key = wfsim_engine::board::builds::build_id(&v);
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

    /// A RANK ARRIVING ON A CARD THE EVERY-RANK LIST DOES NOT NAME IS DROPPED:
    /// investment is not a choice, so that card is stored at max rank.
    #[test]
    fn a_submitted_rank_is_stored_at_max() {
        let mut mods = FULL.to_vec();
        mods[3] = "vital_sense@2";
        let (builds, .., refused) = intake(vec![row("r", &mods, json!({}))].into_iter());
        assert_eq!((refused, builds.len()), (0, 1));
        let stored: Vec<&str> = builds[0]["record"]["mods"]
            .as_array()
            .expect("mods")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(stored.contains(&"vital_sense"), "{stored:?}");
        assert_eq!(ids_of(vec![row("r", &mods, json!({}))]), ids_of(vec![row("f", &FULL, json!({}))]));
    }

    /// A CARD THE LIST NAMES IS ASKED AT EVERY RANK, and two such cards at
    /// every PAIR of ranks: Hunter Track's six by Continuous Misery's four.
    #[test]
    fn every_listed_card_is_crossed_at_every_rank() {
        let mods: Vec<String> = ["hunter_track", "continuous_misery", "serration"].map(String::from).to_vec();
        let v = wfsim_engine::board::builds::validate("braton_prime", &mods, &[], &[], "").expect("legal");
        let choices = rank_choices(&v);
        assert_eq!(choices.len(), 6 * 4);
        assert_eq!(choices[0], v.mods, "the build as it stands comes first");
        assert!(choices.iter().any(|m| m.contains(&"hunter_track@0".to_string())
            && m.contains(&"continuous_misery@2".to_string())));
        assert!(choices.iter().all(|m| m.contains(&"serration".to_string())), "an unlisted card is pinned");
    }

    /// A PROBE READS AN ANSWER OUT OF A REAL RESPONSE, under every ruler. A
    /// field name the simulator does not send makes every probe "a fight that
    /// did not run", and the default then stands without a word.
    #[test]
    fn a_probe_reads_the_fight_it_ran() {
        for bench in wfsim_engine::board::benchmarks::all() {
            let mut req: Value = serde_json::to_value(&bench.scenario).expect("scenario");
            let o = req.as_object_mut().expect("a mapping");
            o.insert("weapon".into(), json!("braton_prime"));
            o.insert("mods".into(), json!(["serration"]));
            o.insert("runs".into(), json!(4));
            o.insert("duration".into(), json!(5));
            let got = score_of(&wfsim_webapi::simulate_json(&req), bench.metric(), 5.0);
            assert!(got.is_some_and(|(s, e)| s.is_finite() && e.is_finite()), "{}: {got:?}", bench.id);
        }
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
        let modes: Vec<String> = wfsim_engine::data::weapons::play_modes("torid")
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
