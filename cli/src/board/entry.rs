// SPDX-License-Identifier: AGPL-3.0-or-later
//! The entry line, on both sides of it — which rows are published and which
//! builds keep earning fights — and the accounting that every validated build
//! reached one outcome.

use super::facts::CrossFact;
use super::publish::Row;

/// WHICH GROUP A BUILD'S FACTS FALL IN — the two halves of a group key that
/// belong to the BUILD rather than to the measurement.
///
/// THE OTHER TWO ARE THE MEASUREMENT'S. A build has no ruler and no mode:
/// `wfsim-intake`'s `canonical` drops both on purpose, because mods are
/// equipped on the WEAPON and a mode is how it is fired. So nothing here can
/// ask what fight a submitter ran — that is a property of the ARRIVAL, and
/// intake asks for it as a queue row while it still knows.
pub(crate) struct Who {
    pub(crate) weapon: String,
    pub(crate) riven: bool,
}

/// THE ENTRY LINE OVER THE ROWS THIS RULER MEASURED, and which builds it left
/// with none. `kept` comes back holding only rows that clear it.
///
/// A GROUP IS (weapon, mode, riven-ness) HERE, because every row in hand is
/// this one ruler's: the ruler is the fourth term and it is constant. Reading
/// it off the rows rather than being told it is what keeps this in step with
/// the page's own grouping.
pub(crate) fn entry_floor(kept: &mut Vec<Row>) -> std::collections::BTreeSet<String> {
    let mut leader: std::collections::HashMap<(String, String, bool), f64> = Default::default();
    for r in kept.iter() {
        let k = (r.weapon.clone(), r.mode.clone(), r.riven.is_some());
        let e = leader.entry(k).or_insert(f64::NEG_INFINITY);
        *e = e.max(r.score);
    }
    let before: std::collections::BTreeSet<String> =
        kept.iter().map(|r| r.identity.clone()).collect();
    kept.retain(|r| {
        let k = (r.weapon.clone(), r.mode.clone(), r.riven.is_some());
        wfsim_engine::data::boards::clears_entry(r.score, leader[&k])
    });
    // WHOSE ROWS ALL FELL UNDER IT. A build that reached no row is a FIFTH
    // outcome beside refused, published, deferred and paused, and the run's
    // accounting asserts against exactly that — so a build parked by the line
    // has to be one the check knows about, or the first flood this refuses
    // reads as the pipeline losing builds.
    let after: std::collections::BTreeSet<String> =
        kept.iter().map(|r| r.identity.clone()).collect();
    before.difference(&after).cloned().collect()
}

/// WHICH BUILDS STOP EARNING FIGHTS, and `pending` comes back holding only the
/// rows that are still worth asking for.
///
/// TWO PASSES, because a share is against a group's BEST and not against the
/// best seen so far.
pub(crate) fn park_under_entry_line(
    pending: &mut Vec<(String, String)>,
    cross: &[CrossFact],
    who: &std::collections::HashMap<String, Who>,
    already_owed: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeSet<String> {
    let mut leader: std::collections::HashMap<(&str, &str, &str, bool), f64> = Default::default();
    for f in cross {
        let Some(w) = who.get(&f.identity) else { continue };
        let k = (w.weapon.as_str(), f.ruler.as_str(), f.mode.as_str(), w.riven);
        let e = leader.entry(k).or_insert(f64::NEG_INFINITY);
        *e = e.max(f.score);
    }
    let mut best: std::collections::HashMap<&str, f64> = Default::default();
    for f in cross {
        let Some(w) = who.get(&f.identity) else { continue };
        let k = (w.weapon.as_str(), f.ruler.as_str(), f.mode.as_str(), w.riven);
        // A LEADER OF ZERO leaves a ratio nothing to say, so the group is not
        // judged — the same rule `clears_entry` states.
        let l = leader[&k];
        let share = if l > 0.0 { f.score / l } else { 1.0 };
        let e = best.entry(f.identity.as_str()).or_insert(f64::NEG_INFINITY);
        *e = e.max(share);
    }
    let mut parked: std::collections::BTreeSet<String> = Default::default();
    pending.retain(|(id, _mode)| {
        let share = best.get(id.as_str()).copied();
        if !wfsim_engine::data::boards::keeps_earning(share) {
            parked.insert(id.clone());
            return false;
        }
        // A BUILD WITH A FACT SOMEWHERE IS OWED EVERY ROW IT LACKS. One with
        // none is owed ONE, and if a row is already owed for it then that row
        // IS the one — intake asks for the fight its submitter ran, so the
        // other eighty-odd are what clearing the line earns.
        //
        // …AND ONE WITH NOTHING OWED IS ASKED FOR EVERYTHING, which is the net:
        // a build whose arrival row was spent, dropped with its batch, or never
        // written is a build nobody would ever measure, and that is the failure
        // the reconciliation exists to make impossible.
        share.is_some() || !already_owed.contains(id.as_str())
    });
    parked
}

/// THE ENTRY LINE, APPLIED ONCE, BY THE PASS THAT MEASURED THE RULER.
///
/// A GROUP IS ONE RULER'S, so every member of one is in `kept` and the
/// leader is known right here; a carried row belongs to another ruler's
/// group and was floored by the pass that published it. That is what keeps
/// one row from being floored twice against two different leaders.
///
/// ONLY AN UNSHARDED RUN HOLDS A WHOLE GROUP. A shard walks its own slice,
/// where the best row it happens to hold would stand in for a leader it
/// never saw — and a shard publishes nothing, so it has no floor to apply.
pub(crate) fn apply_entry_line(
    kept: &mut Vec<Row>,
    shards: usize,
) -> std::collections::BTreeSet<String> {
    let mut floored_ids: std::collections::BTreeSet<String> = Default::default();
    if shards == 1 {
        let held = kept.len();
        floored_ids = entry_floor(kept);
        let dropped = held - kept.len();
        if dropped > 0 {
            eprintln!(
                "entry line: {dropped} row(s) under {:.0}% of their group's leader, {} build(s) with none left",
                wfsim_engine::data::boards::KEEP_LEADER_SHARE * 100.0,
                floored_ids.len(),
            );
        }
    }
    floored_ids
}

/// Every validated build reached a row, was deferred, or was parked by the
/// entry line — or the run panics naming the ones that did not.
pub(crate) fn account(
    kept: &[Row],
    scored_ids: &std::collections::BTreeSet<String>,
    deferred_ids: &std::collections::BTreeSet<String>,
    floored_ids: &std::collections::BTreeSet<String>,
    shards: usize,
    refused: usize,
) {
    let listed: std::collections::BTreeSet<&str> =
        kept.iter().map(|r| r.identity.as_str()).collect();
    let unaccounted: Vec<&String> = scored_ids
        .iter()
        .filter(|id| {
            !listed.contains(id.as_str())
                // …AND NOT ONE THE BUDGET DEFERRED. A build queued for the next
                // run is the fourth outcome, and the only one that is a
                // statement about this run rather than about the build.
                && !deferred_ids.contains(id.as_str())
                // …NOR ONE THE ENTRY LINE PARKED. Its rows were measured and
                // every one of them came in under a tenth of its group's
                // leader, which is an ANSWER about the build rather than a row
                // going missing.
                && !floored_ids.contains(id.as_str())
        })
        .collect();
    assert!(
        shards > 1 || unaccounted.is_empty(),
        "{} validated build(s) produced no row at all. The library holds them and this board never looked at them: {:?}",
        unaccounted.len(),
        &unaccounted[..unaccounted.len().min(5)],
    );
    eprintln!(
        "accounted: {} published, {refused} refused at the door, {} under the entry line",
        listed.len(),
        floored_ids.len(),
    );
}

/// THE ENTRY LINE, ON BOTH SIDES OF IT: which rows are published, and which
/// builds keep earning fights.
///
/// ON INJECTED ROWS, because the question is about a row at exactly the
/// boundary and about a group with one member — neither of which a real board
/// can be asked to contain on demand.
#[cfg(test)]
mod entry_line_tests {
    use super::*;
    use crate::board::publish::RowRiven;

    fn scored(id: &str, weapon: &str, mode: &str, score: f64, riven: bool) -> Row {
        Row {
            identity: id.into(),
            weapon: weapon.into(),
            mode: mode.into(),
            score,
            mods: vec!["serration".into()],
            evolutions: vec![],
            arcanes: vec!["primary_deadhead".into()],
            valence: String::new(),
            exilus: String::new(),
            grip: String::new(),
            loader: String::new(),
            riven: riven.then(|| RowRiven {
                bonuses: vec!["multishot".into()],
                malus: None,
                rolls: vec![1.1],
            }),
        }
    }

    /// **A ROW UNDER A TENTH OF ITS GROUP'S LEADER IS NOT PUBLISHED**, and one
    /// exactly on the line is. Remove the floor and the third row survives.
    #[test]
    fn a_row_under_the_line_is_not_published() {
        let mut kept = vec![
            scored("lead", "braton_prime", "base", 100.0, false),
            scored("on_the_line", "braton_prime", "base", 10.0, false),
            scored("under", "braton_prime", "base", 9.99, false),
        ];
        let parked = entry_floor(&mut kept);
        assert_eq!(
            kept.iter().map(|r| r.identity.as_str()).collect::<Vec<_>>(),
            ["lead", "on_the_line"],
            "the line is inclusive and drops only what is under it",
        );
        assert_eq!(parked.iter().map(String::as_str).collect::<Vec<_>>(), ["under"]);
    }

    /// **A GROUP IS ONE WEAPON, ONE MODE, ONE RIVEN-NESS.** A leader in one
    /// group may not decide what another publishes — on most weapons the group
    /// that would lose its rows is the plain one, which is the builds most
    /// readers can actually make.
    ///
    /// EVERY ROW HERE IS THE SOLE MEMBER OF ITS GROUP, so all four are kept —
    /// and the scores are spread so that dropping ANY ONE TERM from the key
    /// takes a row off the board: pooling riven-ness buries the plain `cycle`
    /// row under 500, pooling modes buries `base` under 20, and pooling weapons
    /// buries the Torid's `base` row under the Acrid's 30.
    #[test]
    fn one_groups_leader_does_not_reach_another() {
        let mut kept = vec![
            scored("riven_lead", "torid", "cycle", 500.0, true),
            scored("plain_lead", "torid", "cycle", 20.0, false),
            scored("other_mode", "torid", "base", 1.0, false),
            scored("other_weapon", "acrid", "base", 30.0, false),
        ];
        let parked = entry_floor(&mut kept);
        assert_eq!(kept.len(), 4, "every row leads its own group");
        assert!(parked.is_empty());
    }

    /// **A GROUP WHOSE LEADER SCORED ZERO IS NEVER EMPTIED** — every row ties
    /// it, and a ratio has nothing to say with no scale to say it on.
    #[test]
    fn a_group_with_no_scale_keeps_its_rows() {
        let mut kept = vec![
            scored("a", "kuva_nukor", "base", 0.0, false),
            scored("b", "kuva_nukor", "base", 0.0, false),
        ];
        let parked = entry_floor(&mut kept);
        assert_eq!(kept.len(), 2);
        assert!(parked.is_empty());
    }

    fn fact(id: &str, ruler: &str, mode: &str, score: f64) -> CrossFact {
        CrossFact {
            identity: id.into(),
            ruler: ruler.into(),
            mode: mode.into(),
            score,
        }
    }

    /// One library entry as the run recorded it: the build's id, its weapon,
    /// and whether it wears a riven. It carries no ruler and no mode, because a
    /// build has neither.
    struct Entry(&'static str, &'static str, bool);

    fn known(entries: Vec<Entry>) -> std::collections::HashMap<String, Who> {
        entries
            .into_iter()
            .map(|Entry(id, weapon, riven)| {
                (id.to_string(), Who { weapon: weapon.to_string(), riven })
            })
            .collect()
    }

    /// The builds somebody has already asked a fight for, on any ruler.
    fn owed(ids: &[&str]) -> std::collections::BTreeSet<String> {
        ids.iter().map(|s| (*s).to_string()).collect()
    }

    /// **A BUILD THAT REACHES THE LINE NOWHERE STOPS BEING ASKED FOR MORE**,
    /// and one that reaches it under a SINGLE ruler keeps earning every row it
    /// lacks. Drop the gate and the junk build's rows are asked for too.
    #[test]
    fn one_good_ruler_is_enough_and_none_is_not() {
        let who = known(vec![
            Entry("good", "torid", false),
            Entry("junk", "torid", false),
            Entry("lead", "torid", false),
        ]);
        let cross = vec![
            fact("lead", "single_target", "base", 100.0),
            fact("lead", "group_clear", "base", 100.0),
            // Bad on the board it was sent to, and a fifth of the leader on
            // the other one — which is the case the fan-out exists for.
            fact("good", "single_target", "base", 1.0),
            fact("good", "group_clear", "base", 20.0),
            fact("junk", "single_target", "base", 1.0),
            fact("junk", "group_clear", "base", 2.0),
        ];
        let mut pending = vec![
            ("good".to_string(), "cycle".to_string()),
            ("junk".to_string(), "cycle".to_string()),
            ("lead".to_string(), "cycle".to_string()),
        ];
        let parked = park_under_entry_line(&mut pending, &cross, &who, &owed(&[]));
        assert_eq!(
            pending.iter().map(|(i, _)| i.as_str()).collect::<Vec<_>>(),
            ["good", "lead"],
            "a build good on ONE ruler keeps earning rows on every other",
        );
        assert_eq!(parked.iter().map(String::as_str).collect::<Vec<_>>(), ["junk"]);
    }

    /// **A BUILD NOTHING HAS MEASURED IS OWED ONE FIGHT, AND THE ROW ALREADY
    /// ASKED FOR IS IT.** `wfsim-intake` asks for the fight its submitter ran;
    /// the other eighty-odd rows are what a flood would otherwise cost, and
    /// clearing the line on that one is what earns them.
    #[test]
    fn a_build_with_a_row_owed_is_owed_nothing_more() {
        let who = known(vec![Entry("new", "torid", false)]);
        let mut pending = vec![
            ("new".to_string(), "cycle".to_string()),
            ("new".to_string(), "base".to_string()),
        ];
        let parked = park_under_entry_line(&mut pending, &[], &who, &owed(&["new"]));
        assert!(pending.is_empty(), "{pending:?}");
        assert!(parked.is_empty(), "a build with no fact is owed one, not parked");
    }

    /// **A BUILD NOBODY HAS ASKED ABOUT IS ASKED FOR EVERYTHING.** That is the
    /// net: an arrival row that was spent, dropped with its batch, or never
    /// written would otherwise leave a build nothing ever measures, which is
    /// the one failure the reconciliation exists to make impossible.
    #[test]
    fn a_build_with_nothing_owed_is_asked_everywhere() {
        let who = known(vec![Entry("old", "torid", false)]);
        let mut pending = vec![
            ("old".to_string(), "cycle".to_string()),
            ("old".to_string(), "base".to_string()),
        ];
        park_under_entry_line(&mut pending, &[], &who, &owed(&[]));
        assert_eq!(pending.len(), 2);
    }

    /// **A RIVEN BUILD AND A PLAIN ONE ARE TWO GROUPS** on the queue side too.
    /// Pool them and the plain build is parked for being a tenth of a card no
    /// reader can roll.
    #[test]
    fn a_riven_leader_does_not_park_a_plain_build() {
        let who = known(vec![
            Entry("carded", "laetum", true),
            Entry("plain", "laetum", false),
        ]);
        let cross = vec![
            fact("carded", "single_target", "base", 1000.0),
            fact("plain", "single_target", "base", 50.0),
        ];
        let mut pending = vec![("plain".to_string(), "alternate".to_string())];
        let parked = park_under_entry_line(&mut pending, &cross, &who, &owed(&[]));
        assert_eq!(pending.len(), 1, "the plain build leads its own group");
        assert!(parked.is_empty());
    }
}
