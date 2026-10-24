// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE ASSEMBLY — every ruler's board from the library and the facts, fighting
//! nothing (docs/BOARD.md §"The assembly").
//!
//!   wfsim-board --project <dir> --facts-in <facts> [--state <yaml>]  < library.json
//!   wfsim-board --serve <dir> --library <library.json> --builds <new.ndjson> \
//!     --facts-in <facts> --state <yaml>
//!
//! A BUILD IS READ ONCE, NOT ONCE PER RULER. The door's legal half is the same
//! for every ruler and was most of a pass; it runs once a record, and a ruler
//! adds only its own admission. `--project` assembles every ruler in one
//! process and exits; `--serve` holds the library and the facts and moves the
//! board a fact at a time, on commands read from stdin, so a cycle costs the
//! weapons it moved and never the library.
//!
//! WHAT IS HELD IS WHAT A ROW IS MADE FROM, never the rows: a weapon's file is
//! assembled when it moves and written, so the board server holds the library
//! and the facts and nothing the size of the board.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde_json::value::RawValue;
use serde_json::Value;
use wfsim_engine::board::benchmarks::family;

use crate::args::flag;

use super::entry::{collapse_shapes, entry_floor, unaccounted};
use super::par::par_map;
use super::measure::card_of;
use super::publish::{page_row, published, Row, RowRiven, INDEX_STEM};
use super::state::{record_states, BOARD_STATE};

/// One ruler of the roster and the directory its rows are written under (`""`
/// is the board's own; a scene is `Benchmark::cell_dir`).
struct Ruler {
    id: String,
    dir: String,
}

fn roster() -> Vec<Ruler> {
    let out: Vec<Ruler> = wfsim_engine::board::benchmarks::all()
        .iter()
        .map(|b| Ruler { id: b.id.clone(), dir: b.cell_dir().unwrap_or_default() })
        .collect();
    assert!(out.len() <= 64, "a build's admission is a bit a ruler: {} rulers", out.len());
    out
}

/// A library record read through the door's legal half, with what every row
/// of it publishes and which rulers admit it (bit `r` of `admit`).
struct Built {
    weapon: String,
    identity: String,
    v: wfsim_engine::board::builds::ValidBuild,
    riven: Option<RowRiven>,
    /// Each mode it is played in — `board_rows::row_requests`'s set, without
    /// building the fights this pass never runs.
    modes: Vec<&'static str>,
    admit: u64,
}

/// `None` is a record refused at the legal half.
type Derived = Arc<Option<Built>>;

fn refusal(rec: &Value, weapon: &str, ruler: Option<&str>, e: &str) {
    let list = |k: &str| -> String {
        rec.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
            .unwrap_or_default()
    };
    let valence = rec.get("valence").and_then(Value::as_str).unwrap_or("");
    let under = ruler.map(|r| format!(" under {r}")).unwrap_or_default();
    eprintln!(
        "refused {weapon}{under}: {e}\n  mode={} mods=[{}] evolutions=[{}] arcanes=[{}] valence={}",
        rec.get("mode").and_then(Value::as_str).unwrap_or("—"),
        list("mods"),
        list("evolutions"),
        list("arcanes"),
        if valence.is_empty() { "—" } else { valence },
    );
}

fn derive(raw: &RawValue, rulers: &[Ruler]) -> Option<Built> {
    use wfsim_webapi::board_rows::{admitted, board_build};
    let rec: Value = serde_json::from_str(raw.get()).ok()?;
    let weapon = rec.get("weapon").and_then(Value::as_str).unwrap_or("");
    let v = match board_build(&rec) {
        Ok(v) => v,
        Err(e) => {
            refusal(&rec, weapon, None, &e);
            return None;
        }
    };
    let mut admit = 0u64;
    for (r, ruler) in rulers.iter().enumerate() {
        match admitted(&rec, &ruler.id, &v) {
            Ok(()) => admit |= 1 << r,
            Err(e) => refusal(&rec, weapon, Some(&ruler.id), &e),
        }
    }
    let modes: Vec<&'static str> = wfsim_engine::data::weapons::play_modes(&v.weapon)
        .into_iter()
        .filter(|m| m.sustainable)
        .map(|m| m.id)
        .collect();
    if modes.is_empty() {
        eprintln!("refused {weapon}: it has no mode that can be sustained for an engagement");
    }
    let riven = v.riven.as_ref().map(|shape| RowRiven {
        bonuses: shape.bonuses.clone(),
        malus: shape.malus.clone(),
        rolls: card_of(&v, shape),
    });
    Some(Built {
        weapon: v.weapon.clone(),
        identity: wfsim_engine::board::builds::build_id(&v),
        v,
        riven,
        modes,
        admit,
    })
}

/// A record's text as a key: the same text derives the same build, so a
/// reload derives only what it has not seen.
fn content_key(raw: &RawValue) -> (u64, u64) {
    let text = raw.get();
    let mut a = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut a);
    let mut b = std::collections::hash_map::DefaultHasher::new();
    (text.len(), text).hash(&mut b);
    (a.finish(), b.finish())
}

/// The library in submission order, each record derived once.
#[derive(Default)]
struct Library {
    records: Vec<Derived>,
    /// Record positions by the weapon its rows land in, in library order.
    by_weapon: BTreeMap<String, Vec<usize>>,
    by_identity: HashMap<String, BTreeSet<String>>,
    cache: HashMap<(u64, u64), Derived>,
}

impl Library {
    /// Append `recs`, deriving in parallel what the cache does not hold.
    /// Returns the (identity, weapon) of each record it added.
    fn extend(&mut self, recs: &[&RawValue], rulers: &[Ruler]) -> Vec<(String, String)> {
        let keys: Vec<(u64, u64)> = recs.iter().map(|r| content_key(r)).collect();
        let todo: Vec<usize> = (0..recs.len()).filter(|&i| !self.cache.contains_key(&keys[i])).collect();
        for (i, d) in par_map(&todo, |&i| (i, Arc::new(derive(recs[i], rulers)))) {
            self.cache.insert(keys[i], d);
        }
        let mut added = Vec::new();
        for k in keys {
            let d = self.cache[&k].clone();
            if let Some(b) = d.as_ref() {
                self.by_weapon.entry(b.weapon.clone()).or_default().push(self.records.len());
                self.by_identity.entry(b.identity.clone()).or_default().insert(b.weapon.clone());
                added.push((b.identity.clone(), b.weapon.clone()));
            }
            self.records.push(d);
        }
        added
    }

    /// A fresh library over `recs`, deriving only what `self` has not seen.
    fn reread(&mut self, recs: &[&RawValue], rulers: &[Ruler]) {
        let cache = std::mem::take(&mut self.cache);
        *self = Library { cache, ..Default::default() };
        self.extend(recs, rulers);
        let live: HashSet<*const Option<Built>> = self.records.iter().map(Arc::as_ptr).collect();
        self.cache.retain(|_, d| live.contains(&Arc::as_ptr(d)));
    }
}

/// What a fact publishes: its score, the minute it finished
/// (`publish::measured_minute` reads no further) and who measured it, the two
/// strings by their place in a table — there are few of each and six million
/// facts — and when it finished, which decides between two facts of one row.
#[derive(Clone, Copy)]
struct Stored {
    score: f64,
    at: u64,
    minute: u32,
    by: u32,
}

/// An `at` that is a place in `Facts::odd`, not a clock's digits.
const ODD_AT: u64 = 1 << 63;

/// THE CLOCK AS `load_facts` ORDERS IT, which is the text's order: an instant
/// spelled as every writer of the file spells it (`2026-10-24T01:02:03.456Z`)
/// is its seventeen digits, which order as the text does; anything else is
/// kept as text.
fn clock_digits(at: &str) -> Option<u64> {
    let b = at.as_bytes();
    if b.len() != 24 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':'
        || b[19] != b'.' || b[23] != b'Z'
    {
        return None;
    }
    let mut n = 0u64;
    for (i, c) in b.iter().enumerate() {
        if matches!(i, 4 | 7 | 10 | 13 | 16 | 19 | 23) {
            continue;
        }
        if !c.is_ascii_digit() {
            return None;
        }
        n = n * 10 + u64::from(c - b'0');
    }
    Some(n)
}

/// A fact with no clock.
const NO_MINUTE: u32 = u32::MAX;

/// Each distinct string once, by its place.
#[derive(Default)]
struct Table {
    all: Vec<String>,
    at: HashMap<String, u32>,
}

impl Table {
    fn of(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.at.get(s) {
            return i;
        }
        let i = self.all.len() as u32;
        self.all.push(s.to_string());
        self.at.insert(s.to_string(), i);
        i
    }
}

/// Every ruler's facts, newest line of a key last — `facts::load_facts` for
/// the whole roster at once.
#[derive(Default)]
struct Facts {
    rulers: HashMap<String, usize>,
    ids: Table,
    modes: Table,
    by: Table,
    minutes: Table,
    /// The `finished_at` texts no clock's digits spell.
    odd: Vec<String>,
    rows: Vec<HashMap<(u32, u32), Stored>>,
}

#[derive(serde::Deserialize)]
struct FactLine<'a> {
    #[serde(borrow, default)]
    identity: Option<&'a RawValue>,
    #[serde(borrow, default)]
    ruler: Option<&'a RawValue>,
    #[serde(borrow, default)]
    mode: Option<&'a RawValue>,
    #[serde(borrow, default)]
    score: Option<&'a RawValue>,
    #[serde(borrow, default)]
    measured_by: Option<&'a RawValue>,
    #[serde(borrow, default)]
    finished_at: Option<&'a RawValue>,
}

/// A field read as `Value::as_str` reads it: a string, or nothing.
fn text(raw: Option<&RawValue>) -> Option<Cow<'_, str>> {
    let raw = raw?.get();
    raw.starts_with('"').then(|| serde_json::from_str::<Cow<'_, str>>(raw).ok()).flatten()
}

impl Facts {
    fn new(rulers: &[Ruler]) -> Facts {
        Facts {
            rulers: rulers.iter().enumerate().map(|(i, r)| (r.id.clone(), i)).collect(),
            rows: rulers.iter().map(|_| HashMap::new()).collect(),
            ..Default::default()
        }
    }

    /// One line of a facts file; the ruler and build it moved, if it counts.
    fn apply(&mut self, line: &str) -> Option<(usize, String)> {
        let f: FactLine = serde_json::from_str(line).ok()?;
        let r = *self.rulers.get(text(f.ruler)?.as_ref())?;
        let identity = text(f.identity)?;
        let score: f64 = f.score?.get().trim_matches('"').parse().ok()?;
        let mode = text(f.mode).unwrap_or_default();
        // AN EMPTY MODE KEYS NO ROW: `load_facts` stores it under the bare
        // identity and every row asks for `<identity>#<mode>`.
        if mode.is_empty() {
            return None;
        }
        let finished = text(f.finished_at).unwrap_or_default();
        let id = self.ids.of(&identity);
        let mode = self.modes.of(&mode);
        let at = match clock_digits(&finished) {
            Some(n) => n,
            None => {
                self.odd.push(finished.to_string());
                ODD_AT | (self.odd.len() - 1) as u64
            }
        };
        // THE NEWEST WINS THE ROW, by the clock and not by the file's order,
        // and a tie goes to the later line — `load_facts`'s rule, the one the
        // scorer reads the same file by.
        if let Some(held) = self.rows[r].get(&(id, mode)) {
            if self.later(held.at, at) {
                return None;
            }
        }
        let minute = match finished.get(..16) {
            Some(m) if finished.as_bytes()[10] == b'T' => self.minutes.of(m),
            _ => NO_MINUTE,
        };
        let by = self.by.of(&text(f.measured_by).unwrap_or_default());
        self.rows[r].insert((id, mode), Stored { score, at, minute, by });
        Some((r, identity.into_owned()))
    }

    /// Whether `a` finished after `b`, as their texts order: two clocks'
    /// digits order as their texts do, and an odd one is compared as text.
    fn later(&self, a: u64, b: u64) -> bool {
        if a & ODD_AT == 0 && b & ODD_AT == 0 {
            return a > b;
        }
        self.clock_text(a) > self.clock_text(b)
    }

    /// An `at` as the text it was read from.
    fn clock_text(&self, at: u64) -> Cow<'_, str> {
        if at & ODD_AT != 0 {
            return Cow::Borrowed(self.odd[(at & !ODD_AT) as usize].as_str());
        }
        let d = format!("{at:017}");
        Cow::Owned(format!("{}-{}-{}T{}:{}:{}.{}Z", &d[0..4], &d[4..6], &d[6..8], &d[8..10], &d[10..12], &d[12..14], &d[14..17]))
    }

    fn get(&self, r: usize, identity: &str, mode: &str) -> Option<&Stored> {
        let id = self.ids.at.get(identity)?;
        let m = self.modes.at.get(mode)?;
        self.rows[r].get(&(*id, *m))
    }

    /// Every line of `path`, a line at a time: the file is every row of
    /// `scores`, past what one string can hold.
    fn read(&mut self, path: &str) {
        let Ok(file) = std::fs::File::open(path) else {
            eprintln!("facts: cannot read {path}");
            return;
        };
        for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
            self.apply(&line);
        }
    }
}

/// One ruler's rows of one weapon, as published, and how many of its rows had
/// no fact yet.
fn assemble(lib: &Library, facts: &Facts, r: usize, bench: &str, weapon: &str) -> (Vec<Box<RawValue>>, usize) {
    let mut rows: Vec<Row> = Vec::new();
    let mut seen: HashSet<(&str, &str)> = HashSet::new();
    let mut scored_ids: BTreeSet<String> = BTreeSet::new();
    let mut deferred_ids: BTreeSet<String> = BTreeSet::new();
    let mut absent = 0;
    for &i in lib.by_weapon.get(weapon).map(Vec::as_slice).unwrap_or(&[]) {
        let Some(b) = lib.records[i].as_ref() else { continue };
        if b.admit & (1 << r) == 0 {
            continue;
        }
        scored_ids.insert(b.identity.clone());
        for &mode in &b.modes {
            // ONE ROW PER BUILD AND MODE (`board_key`), the first record of it
            // in library order.
            let key_mode = if mode.is_empty() { "base" } else { mode };
            if !seen.insert((b.identity.as_str(), key_mode)) {
                continue;
            }
            // A ROW WITH NO FACT IS NOT ON THIS BOARD; it lands on the next one.
            let Some(f) = facts.get(r, &b.identity, key_mode) else {
                absent += 1;
                deferred_ids.insert(b.identity.clone());
                continue;
            };
            let v = &b.v;
            rows.push(Row {
                identity: b.identity.clone(),
                weapon: v.weapon.clone(),
                mode: mode.to_string(),
                score: f.score,
                mods: v.mods.clone(),
                evolutions: v.evolutions.clone(),
                arcanes: v.arcanes.clone(),
                valence: v.valence.clone(),
                exilus: v.exilus.clone().unwrap_or_default(),
                grip: v.assembly.as_ref().map(|a| a.grip.clone()).unwrap_or_default(),
                loader: v.assembly.as_ref().map(|a| a.loader.clone()).unwrap_or_default(),
                riven: b.riven.clone(),
                measured_at: match f.minute {
                    NO_MINUTE => String::new(),
                    m => facts.minutes.all[m as usize].clone(),
                },
                measured_by: facts.by.all[f.by as usize].clone(),
            });
        }
    }
    rows.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let collapsed = collapse_shapes(&mut rows);
    let floored = entry_floor(&mut rows);
    let lost = unaccounted(&rows, &scored_ids, &deferred_ids, &floored, &collapsed);
    assert!(
        lost.is_empty(),
        "{} validated build(s) of {weapon} produced no row on {bench}: {:?}",
        lost.len(),
        &lost[..lost.len().min(5)],
    );
    let out = rows
        .iter()
        .map(|row| serde_json::value::to_raw_value(&page_row(bench, row)).expect("json"))
        .collect();
    (out, absent)
}

fn digest(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Each group's best row of one weapon file — the index's entry for it.
fn leaders(rows: &[&RawValue]) -> Vec<Box<RawValue>> {
    let mut best: BTreeMap<(&str, &str, bool), (f64, &RawValue)> = BTreeMap::new();
    for r in rows {
        let Some(p) = published(r) else { continue };
        let k = (p.benchmark, p.mode.unwrap_or("base"), p.riven.is_some());
        let score = p.score.get().trim_matches('"').parse().unwrap_or(f64::NEG_INFINITY);
        let e = best.entry(k).or_insert((f64::NEG_INFINITY, *r));
        if score > e.0 {
            *e = (score, *r);
        }
    }
    best.into_values().map(|(_, r)| r.to_owned()).collect()
}

/// One weapon file as assembled: its text, its group leaders, and each of its
/// directory's rulers' row count.
struct File {
    dir: String,
    weapon: String,
    text: Option<String>,
    leaders: Vec<Box<RawValue>>,
    listed: Vec<(usize, usize)>,
    absent: usize,
}

/// HOW MANY WEAPON FILES ARE ASSEMBLED BEFORE ANY IS WRITTEN: enough to keep
/// every core busy, few enough that the board is never held whole.
const FILES_AT_ONCE: usize = 64;

/// Every ruler's board, kept equal to its files on disk.
struct Board {
    rulers: Vec<Ruler>,
    lib: Library,
    facts: Facts,
    /// By ruler, then weapon: how many rows the file holds, for the state.
    listed: Vec<HashMap<String, usize>>,
    root: PathBuf,
    state: PathBuf,
    /// Rows a file held when the directory was opened that no live ruler
    /// rewrites, kept ahead of every ruler's.
    legacy: HashMap<(String, String), Vec<Box<RawValue>>>,
    written: HashMap<(String, String), u64>,
    leaders: BTreeMap<String, BTreeMap<String, Vec<Box<RawValue>>>>,
    index: HashMap<String, u64>,
}

impl Board {
    fn new(root: PathBuf, state: PathBuf) -> Board {
        let rulers = roster();
        let facts = Facts::new(&rulers);
        Board {
            listed: rulers.iter().map(|_| HashMap::new()).collect(),
            rulers,
            lib: Library::default(),
            facts,
            root,
            state,
            legacy: HashMap::new(),
            written: HashMap::new(),
            leaders: BTreeMap::new(),
            index: HashMap::new(),
        }
    }

    fn dirs(&self) -> BTreeSet<String> {
        self.rulers.iter().map(|r| r.dir.clone()).collect()
    }

    /// The files already under `root`: a row of a live ruler that writes its
    /// directory is that ruler's to replace, a row of a dead one is dropped,
    /// any other row is kept.
    fn open_existing(&mut self) {
        let live: BTreeSet<&str> = self.rulers.iter().map(|r| family(&r.id)).collect();
        for dir in self.dirs() {
            let here: BTreeSet<&str> =
                self.rulers.iter().filter(|r| r.dir == dir).map(|r| family(&r.id)).collect();
            let Ok(rd) = std::fs::read_dir(self.root.join(&dir)) else { continue };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let Some(weapon) = name.strip_suffix(".json") else { continue };
                if weapon == INDEX_STEM {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
                let Ok(rows) = serde_json::from_str::<Vec<Box<RawValue>>>(&text) else { continue };
                let keep: Vec<Box<RawValue>> = rows
                    .into_iter()
                    .filter(|r| match published(r).map(|p| family(p.benchmark)) {
                        None => true,
                        Some(f) => live.contains(f) && !here.contains(f),
                    })
                    .collect();
                self.legacy.insert((dir.clone(), weapon.to_string()), keep);
            }
        }
    }

    /// Every (ruler, weapon) the board has or the library can make.
    fn all_pairs(&self) -> BTreeSet<(usize, String)> {
        let weapons: BTreeSet<&String> =
            self.lib.by_weapon.keys().chain(self.listed.iter().flat_map(|m| m.keys())).collect();
        (0..self.rulers.len()).flat_map(|r| weapons.iter().map(move |w| (r, (*w).clone()))).collect()
    }

    /// One weapon's file in one directory, every ruler of it assembled.
    fn file(&self, dir: &str, weapon: &str) -> File {
        let mut rows: Vec<Box<RawValue>> = Vec::new();
        let mut listed = Vec::new();
        let mut absent = 0;
        for (r, ruler) in self.rulers.iter().enumerate().filter(|(_, x)| x.dir == dir) {
            let (mine, a) = assemble(&self.lib, &self.facts, r, &ruler.id, weapon);
            absent += a;
            listed.push((r, mine.len()));
            rows.extend(mine);
        }
        let key = (dir.to_string(), weapon.to_string());
        let legacy = self.legacy.get(&key);
        let all: Vec<&RawValue> =
            legacy.into_iter().flatten().map(AsRef::as_ref).chain(rows.iter().map(AsRef::as_ref)).collect();
        // A FILE EXISTS WHILE IT HAS ROWS, or while the directory it was
        // opened in held it — the per-ruler pass rewrote such a file as `[]`.
        let text = (!all.is_empty() || legacy.is_some()).then(|| serde_json::to_string(&all).expect("json"));
        File { dir: dir.to_string(), weapon: weapon.to_string(), leaders: leaders(&all), text, listed, absent }
    }

    /// Re-rank every weapon `touched` names, in every directory one of its
    /// rulers writes; write the files that moved and the state. Returns (files
    /// written, files removed).
    fn publish(&mut self, touched: BTreeSet<(usize, String)>) -> (usize, usize) {
        let t = Instant::now();
        let mut dirty: Vec<(String, String)> = touched
            .iter()
            .map(|(r, w)| (self.rulers[*r].dir.clone(), w.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        // THE COSTLIEST FIRST, so a batch is files of a size and no core waits
        // on one weapon of fifty thousand builds.
        let cost = |w: &str| self.lib.by_weapon.get(w).map_or(0, Vec::len);
        dirty.sort_by_key(|(_, w)| std::cmp::Reverse(cost(w)));
        let (mut wrote, mut removed, mut absent) = (0, 0, 0);
        for batch in dirty.chunks(FILES_AT_ONCE) {
            for f in par_map(batch, |(dir, w)| self.file(dir, w)) {
                absent += f.absent;
                for (r, n) in f.listed {
                    if n == 0 {
                        self.listed[r].remove(&f.weapon);
                    } else {
                        self.listed[r].insert(f.weapon.clone(), n);
                    }
                }
                let entry = self.leaders.entry(f.dir.clone()).or_default();
                if f.leaders.is_empty() {
                    entry.remove(&f.weapon);
                } else {
                    entry.insert(f.weapon.clone(), f.leaders);
                }
                let path = self.root.join(&f.dir).join(format!("{}.json", f.weapon));
                let key = (f.dir, f.weapon);
                match f.text {
                    None => {
                        if self.written.remove(&key).is_some() {
                            let _ = std::fs::remove_file(&path);
                            removed += 1;
                        }
                    }
                    Some(text) => {
                        let h = digest(&text);
                        if self.written.get(&key) != Some(&h) {
                            std::fs::create_dir_all(path.parent().expect("a file's directory"))
                                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                            std::fs::write(&path, &text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                            self.written.insert(key, h);
                            wrote += 1;
                        }
                    }
                }
            }
        }
        for dir in self.dirs() {
            let empty = BTreeMap::new();
            let text = serde_json::to_string(self.leaders.get(&dir).unwrap_or(&empty)).expect("json");
            let h = digest(&text);
            if self.index.get(&dir) != Some(&h) {
                let path = self.root.join(&dir).join(format!("{INDEX_STEM}.json"));
                std::fs::create_dir_all(self.root.join(&dir)).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                std::fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                self.index.insert(dir, h);
                wrote += 1;
            }
        }
        let seen = self.lib.records.len();
        let states: Vec<(String, usize, usize)> = self
            .rulers
            .iter()
            .enumerate()
            .map(|(r, ruler)| (ruler.id.clone(), seen, self.listed[r].values().sum()))
            .collect();
        record_states(&self.state, &states);
        eprintln!(
            "assemble: {} weapon file(s) re-ranked, {wrote} written, {removed} removed, {absent} row(s) with no fact yet, {:.1}s",
            dirty.len(),
            t.elapsed().as_secs_f64(),
        );
        (wrote, removed)
    }

    /// Records the library gained, and the (ruler, weapon) pairs they move.
    ///
    /// A NEW BUILD MOVES A ROW ONLY WITH A FACT: appended last, it claims no
    /// key an earlier record holds, and a key with no fact is no row. A fact
    /// can have arrived before its build.
    fn add_builds(&mut self, recs: &[&RawValue]) -> BTreeSet<(usize, String)> {
        let rulers = std::mem::take(&mut self.rulers);
        let added = self.lib.extend(recs, &rulers);
        self.rulers = rulers;
        let mut out = BTreeSet::new();
        for (id, w) in added {
            if self.facts.ids.at.contains_key(&id) {
                out.extend((0..self.rulers.len()).map(|r| (r, w.clone())));
            }
        }
        out
    }

    /// Fact lines `scores` gained, and the (ruler, weapon) pairs they move.
    fn add_facts<'a>(&mut self, lines: impl IntoIterator<Item = &'a str>) -> BTreeSet<(usize, String)> {
        let moved: Vec<(usize, String)> = lines.into_iter().filter_map(|l| self.facts.apply(l)).collect();
        let mut out = BTreeSet::new();
        for (r, id) in moved {
            for w in self.lib.by_identity.get(&id).into_iter().flatten() {
                out.insert((r, w.clone()));
            }
        }
        out
    }

    /// The library from scratch, from `library` (a JSON array) and `builds`
    /// (`{id, record}` a line), deriving only what it has not seen.
    fn reread_library(&mut self, library: Option<&str>, builds: Option<&str>) {
        let text = library.map(|p| std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{p}: {e}")));
        let mut recs: Vec<&RawValue> = match &text {
            Some(t) => serde_json::from_str(t).unwrap_or_else(|e| panic!("{}: {e}", library.unwrap_or(""))),
            None => Vec::new(),
        };
        let late = builds.map(read_builds).unwrap_or_default();
        recs.extend(late.iter().map(AsRef::as_ref));
        let rulers = std::mem::take(&mut self.rulers);
        self.lib.reread(&recs, &rulers);
        self.rulers = rulers;
    }
}

/// `new-builds.ndjson`: one `{id, record}` a line; the records.
fn read_builds(path: &str) -> Vec<Box<RawValue>> {
    #[derive(serde::Deserialize)]
    struct Line {
        record: Box<RawValue>,
    }
    let Ok(file) = std::fs::File::open(path) else { return Vec::new() };
    std::io::BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str::<Line>(&l).ok())
        .map(|l| l.record)
        .collect()
}

/// `wfsim-board --project <dir>`: every ruler, once, into `dir` as it stands.
pub fn project() {
    let dir = std::env::args().nth(2).filter(|d| !d.starts_with("--")).unwrap_or_else(|| {
        eprintln!("usage: wfsim-board --project <dir> --facts-in <facts> [--state <yaml>]  (library on stdin)");
        std::process::exit(2);
    });
    let t = Instant::now();
    let state = PathBuf::from(flag("--state").unwrap_or_else(|| BOARD_STATE.to_string()));
    let mut board = Board::new(PathBuf::from(&dir), state);
    if let Some(f) = flag("--facts-in") {
        board.facts.read(&f);
    }
    eprintln!("facts: {:.1}s", t.elapsed().as_secs_f64());
    let mut raw = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw).expect("stdin");
    let recs: Vec<&RawValue> = serde_json::from_str(&raw).unwrap_or_default();
    board.add_builds(&recs);
    drop(recs);
    drop(raw);
    eprintln!("library: {} record(s), {:.1}s", board.lib.records.len(), t.elapsed().as_secs_f64());
    board.open_existing();
    let mut pairs = board.all_pairs();
    for (dir, w) in board.legacy.keys() {
        if let Some(r) = board.rulers.iter().position(|x| &x.dir == dir) {
            pairs.insert((r, w.clone()));
        }
    }
    board.publish(pairs);
}

/// `wfsim-board --serve <dir>`: the live board's ranker. It writes the whole
/// board into `<dir>.next`, swaps it in, says `ready`, then answers one line
/// per command:
///
///   builds <ndjson>                       records the library gained
///   facts <ndjson>                        fact lines `scores` gained
///   publish                               re-rank what moved, write it
///   reload <library> <builds> <facts>     the store read afresh: rebuild
///
/// Each answer is one line, `ok …` or `error …`.
pub fn serve() {
    let dir = std::env::args().nth(2).filter(|d| !d.starts_with("--")).unwrap_or_else(|| {
        eprintln!("usage: wfsim-board --serve <dir> --library <json> --builds <ndjson> --facts-in <ndjson> --state <yaml>");
        std::process::exit(2);
    });
    let state = PathBuf::from(flag("--state").unwrap_or_else(|| BOARD_STATE.to_string()));
    let root = PathBuf::from(&dir);
    let next = PathBuf::from(format!("{dir}.next"));
    let _ = std::fs::remove_dir_all(&next);
    std::fs::create_dir_all(&next).unwrap_or_else(|e| panic!("{}: {e}", next.display()));
    let mut board = Board::new(next.clone(), state);
    let t = Instant::now();
    load(
        &mut board,
        flag("--library").as_deref(),
        flag("--builds").as_deref(),
        flag("--facts-in").as_deref(),
    );
    let pairs = board.all_pairs();
    board.publish(pairs);
    swap(&root, &next);
    board.root = root;
    eprintln!("serve: ready in {:.1}s", t.elapsed().as_secs_f64());
    let mut out = std::io::stdout();
    writeln!(out, "ok ready {}", board.lib.records.len()).and_then(|_| out.flush()).expect("stdout");

    let mut pending: BTreeSet<(usize, String)> = BTreeSet::new();
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let words: Vec<&str> = line.split_whitespace().collect();
        let reply = match words.as_slice() {
            ["builds", path] => {
                let recs = read_builds(path);
                let refs: Vec<&RawValue> = recs.iter().map(AsRef::as_ref).collect();
                pending.extend(board.add_builds(&refs));
                format!("ok builds {}", recs.len())
            }
            ["facts", path] => match std::fs::read_to_string(path) {
                Ok(text) => {
                    let moved = board.add_facts(text.lines());
                    let n = moved.len();
                    pending.extend(moved);
                    format!("ok facts {n}")
                }
                Err(e) => format!("error {path}: {e}"),
            },
            ["publish"] => {
                let (wrote, removed) = board.publish(std::mem::take(&mut pending));
                format!("ok publish {wrote} {removed}")
            }
            ["reload", library, builds, facts] => {
                let before = board.all_pairs();
                board.facts = Facts::new(&board.rulers);
                load(&mut board, Some(library), Some(builds), Some(facts));
                let mut pairs = board.all_pairs();
                pairs.extend(before);
                pending.clear();
                let (wrote, removed) = board.publish(pairs);
                format!("ok reload {wrote} {removed}")
            }
            _ => format!("error unknown command: {line}"),
        };
        writeln!(out, "{reply}").and_then(|_| out.flush()).expect("stdout");
    }
}

fn load(board: &mut Board, library: Option<&str>, builds: Option<&str>, facts: Option<&str>) {
    let t = Instant::now();
    if let Some(f) = facts {
        board.facts.read(f);
    }
    eprintln!("facts: {:.1}s", t.elapsed().as_secs_f64());
    board.reread_library(library, builds);
    eprintln!("library: {} record(s), {:.1}s", board.lib.records.len(), t.elapsed().as_secs_f64());
}

/// `next` becomes `root`; the previous board is kept beside it as `.old`
/// until the next swap.
fn swap(root: &Path, next: &Path) {
    let old = PathBuf::from(format!("{}.old", root.display()));
    let _ = std::fs::remove_dir_all(&old);
    if root.exists() {
        std::fs::rename(root, &old).unwrap_or_else(|e| panic!("{}: {e}", root.display()));
    }
    std::fs::rename(next, root).unwrap_or_else(|e| panic!("{}: {e}", next.display()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn braton(last: &str) -> Value {
        json!({
            "weapon": "braton_prime",
            "mods": ["vital_sense", "hellfire", "speed_trigger", "primary_acuity",
                     "galvanized_aptitude", "hammer_shot", "vigilante_fervor", last],
            "evolutions": ["braton_prime_evo1_incarnon_form", "braton_prime_daring_reverie",
                           "braton_prime_voids_guidance", "braton_prime_prelude_of_might"],
            "arcanes": ["primary_deadhead"],
        })
    }

    fn furis(rolls: [f64; 3]) -> Value {
        json!({
            "weapon": "furis",
            "mods": ["primed_convulsion", "pathogen_rounds", "galvanized_diffusion", "primed_target_cracker",
                     "galvanized_shot", "gunslinger", "magnetic_might", "riven"],
            "evolutions": ["furis_evo1_incarnon_form", "furis_stormburst", "furis_extended_volley", "furis_headcracker"],
            "arcanes": ["secondary_enervate"],
            "exilus": "eject_magazine",
            "riven_pos": ["damage", "multishot"],
            "riven_neg": "weapon_recoil",
            "riven_rolls": rolls,
        })
    }

    fn library() -> Vec<Value> {
        vec![
            braton("magnetic_capacity"),
            braton("primed_magazine_warp"),
            furis([1.1, 1.1, 0.9]),
            furis([0.9, 1.1, 0.9]),
            braton("magnetic_capacity"),
        ]
    }

    /// A fact line for every (ruler, mode) each record can be played in, its
    /// score a function of `salt` so two passes can disagree.
    fn facts(recs: &[Value], salt: f64) -> Vec<String> {
        let mut out = Vec::new();
        for (i, rec) in recs.iter().enumerate() {
            let v = wfsim_webapi::board_rows::board_build(rec).expect("a legal build");
            let id = wfsim_engine::board::builds::build_id(&v);
            for (r, ruler) in roster().iter().enumerate() {
                for m in wfsim_engine::board::groups::board_modes(&v.weapon) {
                    let score = 100.0 + (i * 7 + r * 3) as f64 + salt * (i + 1) as f64;
                    out.push(
                        json!({
                            "identity": id,
                            "ruler": ruler.id,
                            "mode": m,
                            "measured_by": "verified:67e27b73c3ee",
                            "score": score,
                            "finished_at": "2026-10-24T01:02:03.456Z",
                        })
                        .to_string(),
                    );
                }
            }
        }
        out
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wfsim-assemble-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Every file under `dir`, by its path below it.
    fn files(dir: &Path) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        let mut todo = vec![dir.to_path_buf()];
        while let Some(d) = todo.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    todo.push(p);
                } else {
                    let k = p.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
                    out.insert(k, std::fs::read_to_string(&p).unwrap());
                }
            }
        }
        out
    }

    /// Records handed to the board as the library hands them: as text.
    fn add(b: &mut Board, recs: Vec<Value>) -> BTreeSet<(usize, String)> {
        let raw: Vec<Box<RawValue>> = recs.iter().map(|r| serde_json::value::to_raw_value(r).unwrap()).collect();
        let refs: Vec<&RawValue> = raw.iter().map(AsRef::as_ref).collect();
        b.add_builds(&refs)
    }

    fn whole(dir: &Path, recs: Vec<Value>, lines: &[String]) -> Board {
        let mut b = Board::new(dir.to_path_buf(), dir.join("no-state.yaml"));
        b.add_facts(lines.iter().map(String::as_str));
        add(&mut b, recs);
        let pairs = b.all_pairs();
        b.publish(pairs);
        b
    }

    /// **A BOARD MOVED A FACT AT A TIME IS THE BOARD ASSEMBLED FROM SCRATCH.**
    ///
    /// The live ranker never assembles from scratch after it starts: every
    /// cycle re-ranks the weapons its facts and builds moved. If the two ever
    /// differ, the live board drifts from `scores` with nothing to say so.
    /// Here the second pass's facts replace the first's (the newest line of a
    /// key wins), a fact arrives before its build, and a build arrives with no
    /// fact at all.
    #[test]
    fn a_board_moved_a_fact_at_a_time_is_the_board_from_scratch() {
        let recs = library();
        let first = facts(&recs, 0.0);
        let second = facts(&recs, 13.0);
        // A PLAIN BUILD THAT TOPS ITS WEAPON, so its arrival moves the files.
        let late = braton("primed_shred");
        let late_facts = facts(std::slice::from_ref(&late), 50.0);

        let a = scratch("whole");
        let all: Vec<String> = first.iter().chain(&second).chain(&late_facts).cloned().collect();
        let mut everything = recs.clone();
        everything.push(late.clone());
        everything.push(furis([1.0, 1.0, 1.0]));
        whole(&a, everything, &all);

        let b = scratch("moved");
        let mut live = whole(&b, recs, &first);
        let moved = live.add_facts(second.iter().map(String::as_str));
        assert!(!moved.is_empty(), "a moved fact moved nothing");
        live.publish(moved);
        // EACH ARRIVAL IN A CYCLE OF ITS OWN, so nothing else re-ranks its weapon.
        let early = live.add_facts(late_facts.iter().map(String::as_str));
        assert!(early.is_empty(), "a fact for a build not yet in the library moved {early:?}");
        live.publish(early);
        let arrived = add(&mut live, vec![late]);
        live.publish(arrived);
        let quiet = add(&mut live, vec![furis([1.0, 1.0, 1.0])]);
        assert!(quiet.is_empty(), "a build with no fact re-ranked {quiet:?}");
        live.publish(quiet);

        let (fa, fb) = (files(&a), files(&b));
        assert!(fa.keys().any(|k| k.contains('/')), "no scene was published: {:?}", fa.keys());
        assert_eq!(fa.keys().collect::<Vec<_>>(), fb.keys().collect::<Vec<_>>());
        for (k, v) in &fa {
            assert_eq!(v, &fb[k], "{k} differs");
        }
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// **A SCENE'S ROWS ARE PUBLISHED IN ITS OWN DIRECTORY**, and the base
    /// board's files never hold one: every reader of `site/board/*.json` —
    /// the page, the stamp, the desktop shell's sync — reads the base board.
    #[test]
    fn a_scenes_rows_are_published_under_its_scene() {
        let recs = library();
        let dir = scratch("scene");
        whole(&dir, recs.clone(), &facts(&recs, 0.0));
        for ruler in roster() {
            let text = std::fs::read_to_string(dir.join(&ruler.dir).join("braton_prime.json")).unwrap();
            let rows: Vec<Value> = serde_json::from_str(&text).unwrap();
            assert!(rows.iter().any(|r| r["benchmark"] == ruler.id.as_str()), "{} has no row in its directory", ruler.id);
        }
        let base: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(dir.join("braton_prime.json")).unwrap()).unwrap();
        for r in &base {
            let b = r["benchmark"].as_str().unwrap();
            let home = roster().into_iter().find(|x| x.id == b).map(|x| x.dir);
            assert_eq!(home.as_deref(), Some(""), "{b} wrote into the base board");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A DIRECTORY OPENED WITH FILES IN IT KEEPS WHAT NO LIVE RULER REWRITES.**
    ///
    /// A row of a ruler that writes this directory is replaced from the facts
    /// (by family, so a `_vN` row goes with it); a retired ruler's row is
    /// dropped, or it would be carried for ever and
    /// `every_published_row_is_a_legal_build` would fail on it with nothing
    /// able to clear it; a row this pass cannot read is kept, and so is its
    /// file, even with nothing else left in it.
    #[test]
    fn a_directory_opened_keeps_what_no_live_ruler_rewrites() {
        let dir = scratch("open");
        let scene = roster().into_iter().find(|r| !r.dir.is_empty()).expect("a scene ruler");
        let rows = json!([
            {"benchmark": "standard_single_target", "score": 1.0},
            {"benchmark": "standard_single_target_v2", "score": 1.0},
            {"benchmark": "single_target_no_aim", "score": 1.0},
            {"benchmark": scene.id, "score": 1.0},
            {"note": "unreadable"},
        ]);
        std::fs::write(dir.join("zzz_gone.json"), rows.to_string()).unwrap();
        let mut b = Board::new(dir.clone(), dir.join("no-state.yaml"));
        b.open_existing();
        let mut pairs = b.all_pairs();
        pairs.insert((0, "zzz_gone".to_string()));
        b.publish(pairs);
        let kept: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(dir.join("zzz_gone.json")).unwrap()).unwrap();
        assert_eq!(kept, vec![json!({"benchmark": scene.id, "score": 1.0}), json!({"note": "unreadable"})]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A WEAPON WITH NO ROW LEFT HAS NO FILE**, once its last row goes — the
    /// board assembled from scratch would have none.
    #[test]
    fn a_weapon_that_loses_its_last_row_loses_its_file() {
        let recs = library();
        let dir = scratch("lose");
        let mut b = whole(&dir, recs.clone(), &facts(&recs, 0.0));
        assert!(dir.join("furis.json").exists());
        b.facts = Facts::new(&b.rulers);
        let pairs = b.all_pairs();
        let (_, removed) = b.publish(pairs);
        assert!(removed > 0 && !dir.join("furis.json").exists(), "a weapon with no fact kept its file");
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// **A ROW'S FACT IS THE ONE `load_facts` KEEPS**: the newest by the clock
    /// whatever the file's order, a tie to the later line, and a clock spelled
    /// some other way compared as its text. The scorer reads the same file
    /// with `load_facts`; two rules for one row would publish two numbers.
    #[test]
    fn a_rows_fact_is_the_one_load_facts_keeps() {
        let ruler = "standard_single_target";
        let line = |id: &str, score: f64, at: &str| {
            json!({"identity": id, "ruler": ruler, "mode": "base", "score": score, "finished_at": at}).to_string()
        };
        let lines = [
            line("a", 1.0, "2026-10-24T02:00:00.000Z"),
            line("a", 2.0, "2026-10-24T01:00:00.000Z"),
            line("b", 1.0, "2026-10-24T02:00:00.000Z"),
            line("b", 2.0, "2026-10-24T02:00:00.000Z"),
            line("c", 1.0, "2026-10-24 03:00:00+00"),
            line("c", 2.0, "2026-10-24T02:00:00.000Z"),
            line("d", 1.0, "2026-10-24T02:00:00.000Z"),
            line("d", 2.0, "2026-10-24T02:00:00Z"),
            line("e", 1.0, ""),
            line("e", 2.0, ""),
        ];
        let dir = scratch("clock");
        let path = dir.join("facts.ndjson");
        std::fs::write(&path, lines.join("\n")).unwrap();
        let want = crate::board::facts::load_facts(Some(path.to_string_lossy().into_owned()), ruler);
        let mut facts = Facts::new(&roster());
        facts.read(&path.to_string_lossy());
        let r = roster().iter().position(|x| x.id == ruler).unwrap();
        for id in ["a", "b", "c", "d", "e"] {
            let kept = want[&format!("{id}#base")].score;
            assert_eq!(facts.get(r, id, "base").map(|f| f.score), Some(kept), "{id}");
        }
        assert_eq!(facts.get(r, "a", "base").unwrap().score, 1.0, "an older line replaced a newer fact");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
