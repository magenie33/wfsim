// SPDX-License-Identifier: AGPL-3.0-or-later
//! OFFICIAL TEST SCENARIOS — `data/benchmarks/`.
//!
//! A benchmark is a scenario that belongs to no weapon: read-only, defined
//! once, and on every weapon, because a number only means something against a
//! ruler someone else can pick up.
//!
//! **A benchmark may not name a weapon, a mod, an arcane, an evolution, or a
//! weapon form**, which [`no_benchmark_names_a_weapon`] enforces. That is the
//! feature — a definition that never speaks about a member of the roster cannot
//! be wrong about a member it has never seen, so adding weapon 201 can never
//! require editing a benchmark. It is stronger than tagging weapons, which is
//! correct only while every weapon is tagged correctly.
//!
//! What replaces the per-weapon settings is POLICY the engine already resolves:
//! `form: default`, an omitted `headshot_pct` (0 for a sentinel weapon, 100
//! otherwise), and omitted `buffs` meaning each opens at the start state
//! docs/BUFFS.md gives it.
//!
//! NO VERSION NUMBERS: there is one board per benchmark, regenerated WHOLE
//! whenever this file or the engine changes. CHANGING A TERM HERE RE-SCORES
//! rather than discarding — a build submitted when the fight was 300 s is still
//! a build at 400 s — which is the whole reason a submission stores the BUILD.
//! `id` is the identity and is stable across rewordings.

use std::sync::OnceLock;

use serde::Deserialize;

/// What a benchmark ADMITS: the shape a build must have to be scored on it.
///
/// Separate from the fight on purpose, and separate from IDENTITY on purpose.
/// Canonicalisation (`board::builds::canonical_mods`) is universal — two boards must
/// agree about whether two builds are the same build, or dedup and displacement
/// stop working. Admission is per-benchmark, and that split is exactly what
/// lets a future ruler demand a weapon class or a required mod without touching
/// how any build is identified.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct BuildRequirement {
    /// `full` = every main slot. Absent = no requirement.
    #[serde(default)]
    pub mods: Option<String>,
    /// `full` = every tier THIS weapon has, which is often none.
    #[serde(default)]
    pub evolutions: Option<String>,
    /// `full` = every arcane seat THIS weapon has, which for a sentinel is none.
    #[serde(default)]
    pub arcanes: Option<String>,
    /// `optional` = a row MAY wear an exilus mod and a row without one is not a
    /// lesser build; `excluded` (and silence) = the slot is not counted and
    /// never travels.
    ///
    /// It was `excluded` everywhere until 2026-08-25, on the reasoning that
    /// exilus mods are handling and mobility with no damage model. That is true
    /// of most of the pool and false of the part that matters: BEAM RANGE is
    /// exilus, and beam range decides how many bodies a beam reaches — on a
    /// group ruler, much of the damage. The rule stays per RULER because
    /// admission always has been.
    ///
    /// NOT `full`. Requiring one would force a choice worth nothing on most
    /// weapons and publish whichever mod the dice favoured.
    #[serde(default)]
    pub exilus: Option<String>,
    /// `full` = an ADVERSARY weapon must name its Valence element, and the
    /// BONUS is the roll's maximum whatever the entrant said.
    ///
    /// The two halves are not the same kind of thing, which is why one value
    /// governs both. The ELEMENT is a choice — a different element is a
    /// different build, exactly as a different evolution is. The PERCENTAGE is
    /// investment: every player can Valence-fuse to 60%, so a board that ranked
    /// a 25% roll against a 60% one would be ranking how many duplicates
    /// someone farmed, which is the same reason every row here is scored at
    /// full Forma and every evolution tier.
    #[serde(default)]
    pub valence: Option<String>,
}

impl BuildRequirement {
    /// Does this axis demand a full house?
    pub fn requires_full(v: &Option<String>) -> bool {
        v.as_deref() == Some("full")
    }

    /// May a row wear an exilus mod?
    ///
    /// `optional` yes, `excluded` no — and SILENCE is no, which is what every
    /// ruler written before the slot was countable meant. The safe direction:
    /// a ruler that has not been told about the slot refuses one rather than
    /// silently scoring a build nobody could compare against its neighbours.
    pub fn allows_exilus(v: &Option<String>) -> bool {
        v.as_deref() == Some("optional")
    }
}

/// One official scenario, as `data/benchmarks/*.yaml` states it.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Benchmark {
    /// Stable slug. Survives a reworded `name`; changes only when the
    /// definition does (and then it is a new file, not an edit).
    pub id: String,
    /// The display name, which states the whole definition — see the yaml.
    /// Localized through the ordinary i18n overlay, never in this file.
    pub name: String,
    /// THE RULES, in prose, for the page that publishes them.
    ///
    /// The `name` states the terms in one line so a rank is quotable; this is
    /// the same standard at length, for a reader deciding whether the ranking
    /// answers their question. Consumed data rather than narrative — it is
    /// rendered — which is why it is a field and not the comments around it.
    ///
    /// A LIST, one claim per entry: it translates as sentences rather than as
    /// a wall, and the page can lay it out.
    #[serde(default)]
    pub rules: Vec<crate::prose::Prose>,
    /// WHAT THIS RULER STANDS FOR, in one sentence of idea and no detail — on a
    /// `standard_` ruler, what the prefix rests on. The detail belongs in
    /// `rules`, one claim each. Consumed data: the page renders it above them.
    #[serde(default)]
    pub standard: crate::prose::Prose,
    /// What a build must look like to be admitted. Absent = admit anything
    /// legal, which is a real answer for a benchmark that wants one.
    #[serde(default)]
    pub build: BuildRequirement,
    /// THE ONE A READER MEETS FIRST, and there is at most one.
    ///
    /// The rulers were in PATH ORDER and that was the same thing as "the
    /// primary one" while `standard_single_target.yaml` sorted first. Adding
    /// `standard_multi_target.yaml` broke it in two places at once: the
    /// board page opened on a brand-new EMPTY ranking, and — worse — every
    /// first-time visitor's default SCENARIO became a crowd fight, because
    /// the app seeds the active scenario from the first builtin.
    ///
    /// Declared rather than derived, and declared ONCE: `all()` sorts on it, so
    /// every consumer downstream inherits the order instead of each one
    /// carrying its own idea of which ruler leads.
    #[serde(default)]
    pub primary: bool,
    /// The fight, as the wire scenario the web api already parses. Kept as a
    /// free-form map ON PURPOSE: a benchmark is defined in the SAME vocabulary
    /// a scenario preset uses, so a field added to scenarios needs no second
    /// definition here, and the "names no weapon" check below still covers it.
    pub scenario: serde_norway::Value,
    /// THE AXES THIS RULER IS ALSO MEASURED ALONG, by [`Axis`] id. Each one
    /// multiplies the ruler into cells (see [`expand_ladder`]); the file's own
    /// values are the base cell, which keeps the file's id.
    #[serde(default)]
    pub ladder: Vec<String>,
    /// The id of the ruler written in the file this one was expanded from —
    /// its own id on the base cell.
    #[serde(skip)]
    pub base: String,
    /// WHERE THIS CELL SITS OFF ITS BASE: `(axis, rung tag)` for every axis
    /// on which it is not the first rung, in `ladder` order. Empty on the base.
    #[serde(skip)]
    pub cell: Vec<(String, String)>,
}

/// ONE CHOSEN SETTING OF AN AXIS — a scene: the fields it sets, and what a
/// ruler's text says for it (`short` in the name, `long` in the standard and
/// the rules). A rung is a CHOSEN combination, not a product of its fields, so
/// an axis lists only the fights someone decided to measure.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rung {
    pub tag: String,
    pub set: serde_norway::Mapping,
    pub short: String,
    pub long: String,
}

/// AN AXIS A RULER MAY BE MEASURED ALONG — `data/benchmarks/axes/<id>.yaml`.
///
/// `values` are its rungs, and the FIRST is the setting every spanning ruler's
/// own file states — its base cell, which keeps the file's id — so a rung added
/// once lines up on every board. A later rung overwrites only the fields it
/// sets, so it states every field in which its fight differs.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Axis {
    pub id: String,
    pub values: Vec<Rung>,
}

impl Axis {
    fn check(&self) -> Result<(), String> {
        use crate::build::scenario::SCENARIO_AXES;
        if self.values.is_empty() {
            return Err("lists no rung: the first is every spanning ruler's own".into());
        }
        for (i, r) in self.values.iter().enumerate() {
            let t = &r.tag;
            if t.is_empty() || t.contains("__") || !t.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                return Err(format!("tag `{t}` is not lowercase letters, digits and single underscores"));
            }
            if self.values[..i].iter().any(|o| &o.tag == t) {
                return Err(format!("tag `{t}` twice"));
            }
            if r.set.is_empty() {
                return Err(format!("`{t}` sets no field"));
            }
            for k in r.set.keys() {
                let k = k.as_str().unwrap_or("");
                // `squad_size` is read by `parse_fight` and is not a page control.
                if k != "squad_size" && !SCENARIO_AXES.iter().any(|a| a.id == k) {
                    return Err(format!("`{t}` sets `{k}`, which is not a field of the fight"));
                }
            }
        }
        Ok(())
    }
}

/// Every declared axis.
pub fn axes() -> &'static [Axis] {
    static A: OnceLock<Vec<Axis>> = OnceLock::new();
    A.get_or_init(|| {
        crate::data::files_under("benchmarks/axes/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                let a: Axis = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                let stem = &p["benchmarks/axes/".len()..p.len() - ".yaml".len()];
                assert_eq!(a.id, stem, "{p}: an axis file is named after its id");
                a.check().unwrap_or_else(|why| panic!("{p}: {why}"));
                a
            })
            .collect()
    })
}

/// A RULER, MULTIPLIED ALONG ITS LADDER: every combination of one rung per
/// axis, base cell first, last axis varying fastest.
///
/// A cell's id is the base id plus `__<tag>` for each axis it is not on the
/// first rung of, so the base keeps the id every stored score, link and record
/// already names. `{<axis>}` in the name, the standard and the rules is filled
/// per cell: a cell that quoted its base's level would publish a term it is not
/// measured at.
fn expand_ladder(b: Benchmark, axes: &[Axis]) -> Result<Vec<Benchmark>, String> {
    let mut lines: Vec<&Axis> = Vec::new();
    for id in &b.ladder {
        let axis = axes.iter().find(|a| &a.id == id).ok_or_else(|| {
            format!("spans the axis `{id}`, which data/benchmarks/axes/ does not declare")
        })?;
        if lines.iter().any(|a| &a.id == id) {
            return Err(format!("spans `{id}` twice"));
        }
        let first = &axis.values[0];
        for (k, v) in &first.set {
            if b.scenario.get(k) != Some(v) {
                return Err(format!(
                    "spans `{id}` and its scenario states {:?} for `{}`, but the first rung `{}` — every spanning ruler's own — sets {v:?}",
                    b.scenario.get(k), k.as_str().unwrap_or("?"), first.tag
                ));
            }
        }
        if !b.name.contains(&format!("{{{id}}}")) {
            return Err(format!("spans `{id}` and its name does not say where: write `{{{id}}}`"));
        }
        lines.push(axis);
    }
    let mut out = Vec::new();
    let mut pick = vec![0usize; lines.len()];
    loop {
        let mut c = b.clone();
        c.base = b.id.clone();
        for (i, axis) in lines.iter().enumerate() {
            let r = &axis.values[pick[i]];
            if pick[i] > 0 {
                c.id.push_str(&format!("__{}", r.tag));
                if let Some(m) = c.scenario.as_mapping_mut() {
                    for (k, v) in &r.set {
                        m.insert(k.clone(), v.clone());
                    }
                }
                c.cell.push((axis.id.clone(), r.tag.clone()));
            }
            let hole = format!("{{{}}}", axis.id);
            c.name = c.name.replace(&hole, &r.short);
            c.standard = c.standard.map(|t| t.replace(&hole, &r.long));
            c.rules = c.rules.iter().map(|t| t.map(|s| s.replace(&hole, &r.long))).collect();
        }
        if !c.cell.is_empty() {
            c.primary = false;
        }
        if let Some(hole) = super::describe::unfilled(&c) {
            return Err(format!("says `{{{hole}}}`, which is not an axis it spans"));
        }
        out.push(c);
        let mut i = lines.len();
        loop {
            if i == 0 {
                return Ok(out);
            }
            i -= 1;
            pick[i] += 1;
            if pick[i] < lines[i].values.len() {
                break;
            }
            pick[i] = 0;
        }
    }
}

impl Benchmark {
    /// WHAT THIS RULER JUDGES BY. Validated when the benchmark loaded, so every
    /// consumer reads the core from here rather than resolving one of its own.
    pub fn metric(&self) -> &'static crate::rules::metrics::MetricDef {
        core_metric(&self.scenario).expect("a benchmark that loaded named its core")
    }

    /// WHERE THIS CELL'S ROWS ARE PUBLISHED under the board's directory: its
    /// rung tags, joined as its id joins them, and `None` on a base cell. One
    /// directory per scene holds every ruler's rows in it, a file per weapon,
    /// so a reader of the base board never meets a scene's row.
    pub fn cell_dir(&self) -> Option<String> {
        (!self.cell.is_empty()).then(|| self.cell.iter().map(|(_, tag)| tag.as_str()).collect::<Vec<_>>().join("__"))
    }

    /// THE RUNG THIS CELL SITS ON along each laddered axis, in `ladder` order:
    /// the one `cell` names, or the axis's first where it names none.
    pub fn rungs(&self) -> Vec<&'static Rung> {
        self.ladder
            .iter()
            .filter_map(|id| {
                let axis = axes().iter().find(|a| &a.id == id)?;
                match self.cell.iter().find(|(a, _)| a == id) {
                    Some((_, tag)) => axis.values.iter().find(|r| &r.tag == tag),
                    None => axis.values.first(),
                }
            })
            .collect()
    }
}

/// A TEST HAS ONE CORE, and this is where that rule is applied.
///
/// ABSENT IS A MISTAKE, NOT A DEFAULT. `rules::metrics::DEFAULT` serves a SCENARIO
/// somebody is running for themselves, where leaving the question unsaid is an
/// ordinary thing to do. A benchmark is not that: a ruler that does not name
/// what it judges by would publish a whole ranking in the units of a question
/// nobody asked, and the number would look exactly like a right one.
///
/// A ruler may READ more than one metric off the same fight. Only the core
/// ranks, which is what keeps a ranking a ranking.
pub fn core_metric(
    scenario: &serde_norway::Value,
) -> Result<&'static crate::rules::metrics::MetricDef, String> {
    let Some(id) = scenario.get("metric").and_then(|v| v.as_str()) else {
        return Err("names no metric, and a ruler ranks by exactly one".into());
    };
    crate::rules::metrics::get(id)
        .ok_or_else(|| format!("names the metric `{id}`, which `engine::metrics::ALL` does not"))
}

/// A CROWD IN THREE NUMBERS, expanded HERE and nowhere else.
///
/// `formation_grid: {cols, rows, spacing_m}` lays a regular grid around the
/// body being aimed at — front rank centred on it, every other rank one spacing
/// further along the shot's own line — and becomes an ordinary `formation`
/// list before anything downstream sees the scenario.
///
/// WHY THE SHORTHAND EXISTS: a crowd written out is a line per body nobody can
/// check by reading, and a ruler whose terms cannot be argued with is not a
/// ruler.
///
/// WHY IT IS EXPANDED HERE: because the PAGE has to draw the crowd. The arena
/// is the source — what you see is what gets simulated — and it reads
/// `formation`. Expanding at simulate time instead would have left the canvas
/// drawing one body for a crowd fight, and expanding in both places is the
/// two-implementations bug this repo keeps paying for. One expansion, at the
/// moment the yaml becomes a scenario, and every consumer downstream — the
/// canvas, the payload, `parse_fight`, the board scorer — sees only bodies.
fn expand_formation_grid(sc: &mut serde_norway::Value) -> Result<(), String> {
    use serde_norway::Value;
    let Some(g) = sc.get("formation_grid").cloned() else { return Ok(()) };
    let n = |k: &str| g.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let (cols, rows, spacing) = (n("cols") as usize, n("rows") as usize, n("spacing_m"));
    if cols == 0 || rows == 0 || spacing <= 0.0 {
        return Err("formation_grid needs cols, rows and a positive spacing_m".into());
    }
    let point = |k: &str, d: [f64; 2]| -> crate::rules::space::Vec2 {
        sc.get(k)
            .and_then(Value::as_sequence)
            .filter(|a| a.len() == 2)
            .map_or(crate::rules::space::Vec2::new(d[0], d[1]), |a| {
                crate::rules::space::Vec2::new(
                    a[0].as_f64().unwrap_or(d[0]),
                    a[1].as_f64().unwrap_or(d[1]),
                )
            })
    };
    let player = point("player_at", [0.0, 0.0]);
    let target = point("target_at", [0.0, crate::rules::space::CONTACT_RANGE_M]);
    let forward = crate::rules::space::Vec2::new(target.x - player.x, target.y - player.y);
    let bodies: Vec<Value> = crate::formation::Formation::grid_around(
        target, forward, cols, rows, spacing,
    )
    .into_iter()
    // INDEX 0 IS THE AIMED BODY, which the fight already has as `target_at`.
    .skip(1)
    .map(|p| {
        let mut m = serde_norway::Mapping::new();
        m.insert(
            Value::String("at".into()),
            Value::Sequence(vec![
                Value::Number(p.x.into()),
                Value::Number(p.y.into()),
            ]),
        );
        Value::Mapping(m)
    })
    .collect();
    if let Some(m) = sc.as_mapping_mut() {
        // An explicit `formation` rides alongside: the grid is a shorthand for
        // bodies, not a mode.
        let mut all = m
            .get(Value::String("formation".into()))
            .and_then(Value::as_sequence)
            .cloned()
            .unwrap_or_default();
        all.extend(bodies);
        m.insert(Value::String("formation".into()), Value::Sequence(all));
        m.remove(Value::String("formation_grid".into()));
    }
    Ok(())
}

/// Every official ruler — every cell of every file's ladder — parsed once.
pub fn all() -> &'static [Benchmark] {
    static B: OnceLock<Vec<Benchmark>> = OnceLock::new();
    B.get_or_init(|| {
        let mut v: Vec<Benchmark> = crate::data::files_under("benchmarks/")
            // THIS level only: `axes/` beneath it is a different shape, and a
            // prefix scan would parse an axis as a ruler.
            .filter(|(p, _)| p.ends_with(".yaml") && !p["benchmarks/".len()..].contains('/'))
            .map(|(p, text)| {
                let mut b: Benchmark =
                    serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                expand_formation_grid(&mut b.scenario)
                    .unwrap_or_else(|e| panic!("{p}: {e}"));
                // THE CORE IS CHECKED WHERE THE FILE BECOMES A BENCHMARK, so
                // no consumer downstream has to decide what an unnamed one
                // means — and the only two answers available to it, refusing
                // and defaulting, are both wrong at the point of use.
                core_metric(&b.scenario).unwrap_or_else(|why| panic!("{p} {why}"));
                b
            })
            .collect::<Vec<_>>();
        // THE ORDER A READER MEETS THEM IN, and it is a property of the SET
        // rather than of any one file — which is why it is here and not a
        // number each yaml carries and each new ruler has to guess.
        //
        // The primary one leads (`Benchmark::primary`), then the STANDARDS —
        // the rulers that claim to represent most play, which is what the
        // `standard_` prefix is for — fewer targets before more, because the
        // simpler claim is the one a reader checks first. Everything else
        // follows alphabetically, so a ruler that represents nothing but
        // itself cannot arrive ahead of one that does.
        v.sort_by(|a, b| {
            let key = |x: &Benchmark| {
                (!x.primary, !x.id.starts_with("standard_"),
                 !x.id.contains("single"), x.id.clone())
            };
            key(a).cmp(&key(b))
        });
        // A CELL FOLLOWS ITS BASE, so the order above is still the order of
        // the rulers a reader picks between.
        v.into_iter()
            .flat_map(|b| {
                let id = b.id.clone();
                expand_ladder(b, axes()).unwrap_or_else(|why| panic!("benchmarks/{id}.yaml {why}"))
            })
            .collect()
    })
}

/// The benchmark with this id.
pub fn get(id: &str) -> Option<&'static Benchmark> {
    all().iter().find(|b| b.id == id)
}

/// A benchmark id without its `_v<n>` suffix — `standard_single_target_v2` and
/// `standard_single_target_v1` are the same ruler, and a build aimed at
/// either belongs on the current one's board.
pub fn family(id: &str) -> &str {
    match id.rsplit_once("_v") {
        Some((head, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) => head,
        _ => id,
    }
}

// The two halves of the rule below live under `cfg(test)`: the check is a DATA
// discipline enforced in CI, the same way perk-id uniqueness and the mod-value
// sweep are, and neither is something a running engine re-derives.
/// Every string a benchmark's `scenario` map holds, keys included.
///
/// Keys count: a scenario field named after a weapon would be as wrong as a
/// value, and the map is free-form precisely so new fields need no code here.
#[cfg(test)]
fn strings_in(v: &serde_norway::Value, out: &mut Vec<String>) {
    match v {
        serde_norway::Value::String(s) => out.push(s.clone()),
        serde_norway::Value::Sequence(xs) => xs.iter().for_each(|x| strings_in(x, out)),
        serde_norway::Value::Mapping(m) => {
            for (k, val) in m {
                if let serde_norway::Value::String(s) = k {
                    out.push(s.clone());
                }
                strings_in(val, out);
            }
        }
        _ => {}
    }
}

/// Ids a benchmark may never contain: everything that belongs to ONE weapon.
///
/// Enemies are deliberately absent — an enemy is a property of the fight, which
/// is exactly what a benchmark is allowed to choose.
#[cfg(test)]
fn weapon_bound_ids() -> Vec<String> {
    let mut ids: Vec<String> = crate::data::weapons::all()
        .iter()
        .map(|w| w.id.clone())
        .collect();
    ids.extend(crate::data::evolutions::pool().iter().map(|e| e.id.clone()));
    for class in crate::data::mods::classes() {
        ids.extend(
            crate::data::mods::pool_union(&[class.to_string()])
                .into_iter()
                .map(|m| m.id.to_string()),
        );
    }
    for slot in crate::data::arcanes::slots() {
        ids.extend(
            crate::data::arcanes::slot_pool(slot)
                .iter()
                .map(|a| a.id.to_string()),
        );
    }
    // Named FORMS. `default` is the policy and is the only legal value: it is
    // what resolves per weapon. `base` / `incarnon` / `charged` name a form
    // some weapons have and others do not, so a benchmark asking for one gets
    // a silent per-weapon fallback — one name, two different tests.
    ids.extend(
        crate::data::weapons::all()
            .iter()
            .map(|w| w.form.clone())
            .filter(|f| f != "default"),
    );
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE RULE, enforced. A benchmark that names a weapon — or a mod, an
    /// arcane, an evolution, or a form only some weapons register — is a ruler
    /// that measures different things on different weapons, and it fails the
    /// build rather than the board.
    #[test]
    fn no_benchmark_names_a_weapon() {
        let banned = weapon_bound_ids();
        assert!(!banned.is_empty(), "the ban list itself has to load");
        for b in all() {
            let mut seen = Vec::new();
            strings_in(&b.scenario, &mut seen);
            for s in seen {
                assert!(
                    !banned.contains(&s),
                    "benchmark {} names '{s}', which belongs to one weapon — a benchmark \
                     must hold for a roster it has never seen (see this module's header)",
                    b.id
                );
            }
        }
    }

    /// The official ruler, pinned. Every field here is a term of a public
    /// claim, so a change to one changes what every published number MEANS.
    /// This test is where that gets noticed — not to forbid the change, but so
    /// it is made deliberately and the board is re-scored with it.
    #[test]
    fn the_standard_single_target_benchmark_is_what_we_published() {
        let b = get("standard_single_target").expect("data/benchmarks/standard_single_target.yaml");
        assert_eq!(b.name, "Standard Single Target · Thrax Centurion Lv 9999 SP · 180 s · KPM");
        let s = |k: &str| b.scenario.get(k).cloned();
        assert_eq!(s("enemy").and_then(|v| v.as_str().map(String::from)).as_deref(), Some("thrax_centurion"));
        assert_eq!(s("level").and_then(|v| v.as_u64()), Some(9999));
        assert_eq!(s("steel_path").and_then(|v| v.as_bool()), Some(true));
        // 180 s since 2026-08-10, down from 300 (five minutes stops
        // pricing a build's ramp and starts paying for it twice). The pin moved
        // WITH the change, which is what it is for.
        assert_eq!(s("duration").and_then(|v| v.as_u64()), Some(180));
        // 1000 since 2026-08-11, up from 100. The board's noise floor
        // is the one term of a public claim nobody can improve by trying
        // harder, and it is the cheapest thing here to spend on: paid once per
        // rescore rather than per player. The SIMULATOR's default stays 100 —
        // that one is paid per keystroke, in a browser.
        assert_eq!(s("runs").and_then(|v| v.as_u64()), Some(1000));
        // PICKUPS MODELLED. A weapon that cannot be resupplied ignores this
        // and runs on its real reserve — worth 3x on that weapon — so it is a
        // term of the claim, not a detail.
        assert_eq!(s("infinite_ammo").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(s("metric").and_then(|v| v.as_str().map(String::from)).as_deref(), Some("kpm"));
        assert_eq!(b.metric().id, "kpm");
        // AND NO FORM. How a weapon is played belongs to the ENTRANT, not to
        // the ruler: a Torid through its Incarnon cycle and a Torid that never
        // transmutes are two rows here, and a benchmark that pinned one could
        // rank only that one. A `form: default` in the yaml reads as neutral
        // and is not — it resolves to the cycle on a weapon that has one, so
        // the board would rank every Incarnon weapon at its ceiling and could
        // not be asked for anything else.
        assert_eq!(s("form"), None, "a ruler may not say how a weapon is played");
        // Pinned, not defaulted: a published number has to be reproducible by
        // whoever doubts it.
        assert_eq!(s("seed").and_then(|v| v.as_u64()), Some(0xC0FFEE));

        // AIMED, AND EVERY SHOT ON THE WEAK POINT — both pinned, because they
        // are terms of this board's claim and a reader should find them in the
        // ruler rather than in a default. They are two different facts: the
        // stance gates mods like Argon Scope, the rate is where the shots land.
        //
        // Pinning it does risk ranking a SENTINEL at a headshot rate it
        // cannot reach, and that is handled where it belongs rather than by
        // omission: `parse_fight` pins a sentinel's rate at 0 whatever the
        // request says, the same way
        // `tenno_from` pins its stance. A weapon fact is not a benchmark's to
        // state, and a benchmark's terms are not a default's to hide.
        assert_eq!(s("headshot_pct").and_then(|v| v.as_f64()), Some(100.0));
        assert_eq!(s("aiming").and_then(|v| v.as_bool()), Some(true));
        // ...and buffs stay omitted, which IS the policy: each opens where
        // docs/BUFFS.md says, and pinning them would assert stacks the fight
        // never handed out.
        assert!(s("buffs").is_none(), "each buff opens where docs/BUFFS.md says");
    }

    /// A RULER'S PROSE QUOTES ITS OWN NUMBERS.
    ///
    /// The spacing is written THREE times in `standard_multi_target.yaml` — the machine
    /// field `spacing_m`, the ruler's NAME, and the rule sentence a reader is
    /// shown — and on 2026-08-22 the field moved from 1.5 m to 3 m and the
    /// other two did not. The board went on saying "19x19 at 1.5 m" over a
    /// fight that was 3 m, which is the worst kind of wrong: every number on
    /// the page was right and the page said what produced them was something
    /// else. The owner caught it by reading the board.
    ///
    /// So the prose is CHECKED against the field rather than kept in step by
    /// hand. It reads the grid before expansion, which is why this lives beside
    /// `expand_formation_grid`: once expanded there is no spacing left to
    /// compare, only positions.
    ///
    /// It is deliberately not a template. A ruler's name is written for a
    /// reader and generating it would flatten every one of them into the same
    /// sentence; what has to hold is that the numbers in it are this ruler's.
    #[test]
    fn a_rulers_name_and_rules_quote_its_own_spacing() {
        // THE RAW YAML, because `all()` expands the grid away: after
        // `expand_formation_grid` there is no spacing left to compare against,
        // only positions.
        let mut checked = 0;
        for (path, text) in crate::data::files_under("benchmarks/") {
            if !path.ends_with(".yaml") || path["benchmarks/".len()..].contains('/') {
                continue;
            }
            let raw: serde_norway::Value =
                serde_norway::from_str(text).unwrap_or_else(|e| panic!("{path}: {e}"));
            let Some(g) = raw.get("scenario").and_then(|s| s.get("formation_grid")) else {
                continue;
            };
            let spacing = g.get("spacing_m").and_then(|v| v.as_f64()).expect("spacing_m");
            let id = raw.get("id").and_then(|v| v.as_str()).expect("id");
            let b = get(id).expect("every ruler loads");
            checked += 1;
            // `{:g}`-style: 3.0 reads as "3" and 1.5 as "1.5", which is how a
            // person writes it and therefore how the prose has it.
            let token = if (spacing.fract()).abs() < 1e-9 {
                format!("{}", spacing as i64)
            } else {
                format!("{spacing}")
            };
            let prose = format!("{} {}", b.name, b.rules.iter().map(|r| r.shown()).collect::<Vec<_>>().join(" "));
            assert!(
                prose.contains(&format!("{token} m")) || prose.contains(&format!("{token} metres")),
                "{id}: the grid is {spacing} m and neither the name nor the rules say so.\n  \
                 name:  {}\n  This is the drift that put '19x19 at 1.5 m' over a 3 m fight.",
                b.name
            );
            // …AND ITS SIZE, in the name and in the rules both: the same drift
            // with the other two numbers.
            let side = |k: &str| g.get(k).and_then(|v| v.as_u64()).expect("cols and rows");
            let (cols, rows) = (side("cols"), side("rows"));
            assert!(
                b.name.contains(&format!("{cols}x{rows}")),
                "{id}: the grid is {cols}x{rows} and the name does not say so.\n  name:  {}",
                b.name
            );
            let body = b.rules.iter().map(|r| r.shown()).collect::<Vec<_>>().join(" ");
            assert!(
                body.contains(&format!("{cols} by {rows}")) && body.contains(&format!(" {} ", cols * rows)),
                "{id}: the grid is {cols} by {rows}, {} bodies, and the rules do not say both.",
                cols * rows
            );
        }
        assert!(checked > 0, "at least one ruler lays a grid, or this checks nothing");
    }

    /// A LADDER MULTIPLIES A RULER, AND THE BASE KEEPS EVERYTHING IT HAD.
    /// On a synthetic ruler, because the shipped axes may list one rung yet and
    /// a test that can only see one cell is not a test.
    #[test]
    fn a_ladder_multiplies_a_ruler_into_cells() {
        let ruler = |name: &str, ladder: &str| -> Benchmark {
            serde_norway::from_str(&format!(
                "id: r\nprimary: true\nname: \"{name}\"\n\
                 rules: [\"one target {{scene}}, {{headshot}}\"]\n\
                 ladder: {ladder}\nscenario: {{level: 9999, steel_path: true, headshot_pct: 100, metric: kpm}}\n"
            ))
            .expect("parses")
        };
        let axis = |text: &str| -> Axis {
            let a: Axis = serde_norway::from_str(text).expect("parses");
            a.check().expect("valid");
            a
        };
        let axes = [
            axis("id: scene\nvalues:\n\
                  - {tag: sp9999, set: {level: 9999, steel_path: true}, short: Lv 9999 SP, long: at level 9999 on Steel Path}\n\
                  - {tag: icebind1000, set: {level: 1000, steel_path: false, squad_size: 6}, short: Lv 1000 Icebind, long: at level 1000 in The Icebind}\n\
                  - {tag: sp200, set: {level: 200, steel_path: true}, short: Lv 200 SP, long: at level 200 on Steel Path}"),
            axis("id: headshot\nvalues:\n\
                  - {tag: hs100, set: {headshot_pct: 100}, short: all heads, long: every shot on the head}\n\
                  - {tag: hs50, set: {headshot_pct: 50}, short: half heads, long: half on the head}"),
        ];
        let cells = expand_ladder(ruler("{scene} · {headshot}", "[scene, headshot]"), &axes).expect("expands");
        let ids: Vec<&str> = cells.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, [
            "r", "r__hs50", "r__icebind1000", "r__icebind1000__hs50", "r__sp200", "r__sp200__hs50",
        ]);
        let c = &cells[3];
        assert_eq!((c.base.as_str(), c.primary), ("r", false));
        assert_eq!(c.cell, [("scene".to_string(), "icebind1000".to_string()), ("headshot".to_string(), "hs50".to_string())]);
        assert_eq!(c.cell_dir().as_deref(), Some("icebind1000__hs50"));
        assert_eq!(cells[0].cell_dir(), None, "the base cell is published where it always was");
        let get = |k: &str| c.scenario.get(k).cloned();
        assert_eq!(get("level").and_then(|v| v.as_u64()), Some(1000));
        assert_eq!(get("steel_path").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(get("squad_size").and_then(|v| v.as_u64()), Some(6));
        assert_eq!(get("metric").and_then(|v| v.as_str().map(String::from)).as_deref(), Some("kpm"), "a field no rung sets is the file's");
        assert_eq!(c.name, "Lv 1000 Icebind · half heads");
        assert_eq!(c.rules.iter().map(|r| r.shown()).collect::<Vec<_>>(), ["one target at level 1000 in The Icebind, half on the head"]);
        assert!(cells[0].primary && cells[0].cell.is_empty() && cells[0].name == "Lv 9999 SP · all heads");

        let why = |r| expand_ladder(r, &axes).expect_err("refused");
        assert!(why(ruler("Lv 9999 · {headshot}", "[scene, headshot]")).contains("name does not say"));
        assert!(why(ruler("{scene}", "[scene]")).contains("`{headshot}`"), "a hole no axis fills");
        assert!(why(ruler("{scene}", "[scene, armour]")).contains("does not declare"));
        assert!(why(ruler("{scene}", "[scene, scene]")).contains("twice"));
        let off_base = [axis("id: scene\nvalues: [{tag: sp200, set: {level: 200}, short: a, long: b}]"), axes[1].clone()];
        assert!(expand_ladder(ruler("{scene} {headshot}", "[scene, headshot]"), &off_base).expect_err("refused").contains("first rung"));

        let bad = |text: &str| serde_norway::from_str::<Axis>(text).expect("parses").check().expect_err("refused");
        let rung = |tag: &str, set: &str| format!("{{tag: {tag}, set: {set}, short: a, long: b}}");
        assert!(bad("id: scene\nvalues: []").contains("no rung"));
        assert!(bad(&format!("id: scene\nvalues: [{}, {}]", rung("a", "{level: 1}"), rung("a", "{level: 2}"))).contains("twice"));
        assert!(bad(&format!("id: scene\nvalues: [{}]", rung("a__b", "{level: 1}"))).contains("single underscores"));
        assert!(bad(&format!("id: scene\nvalues: [{}]", rung("a", "{}"))).contains("sets no field"));
        assert!(bad(&format!("id: scene\nvalues: [{}]", rung("a", "{armour: 1}"))).contains("not a field"));
    }

    /// A CELL IS A RULER ID LIKE ANY OTHER: the worker's id pattern, no
    /// version suffix for `family` to strip, and a base that is on the roster.
    #[test]
    fn every_cell_is_a_ruler_id_the_pipeline_accepts() {
        let mut seen = std::collections::BTreeSet::new();
        for b in all() {
            assert!(seen.insert(b.id.as_str()), "{} twice", b.id);
            assert!(b.id.len() <= 64 && b.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{}: outside the worker's id pattern", b.id);
            assert_eq!(family(&b.id), b.id, "{}: reads as a version", b.id);
            let base = get(&b.base).unwrap_or_else(|| panic!("{}: base {} is not a ruler", b.id, b.base));
            assert!(base.cell.is_empty() && (b.cell.is_empty() == (b.id == b.base)), "{}", b.id);
        }
        assert_eq!(all().iter().filter(|b| b.primary).count(), 1, "exactly one ruler leads");
    }

    /// AN AXIS IS WRITTEN AS ITS HOLE, NEVER AS ITS BASE'S WORDS. A file that
    /// spans `scene` and writes "9999" in its prose publishes 9999 on every
    /// cell of its ladder, the level 200 one included.
    #[test]
    fn a_spanned_axis_is_never_written_out() {
        let mut checked = 0;
        for (path, text) in crate::data::files_under("benchmarks/") {
            if !path.ends_with(".yaml") || path["benchmarks/".len()..].contains('/') {
                continue;
            }
            let b: Benchmark = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{path}: {e}"));
            for id in &b.ladder {
                let first = &axes().iter().find(|a| &a.id == id).expect("validated at load").values[0];
                // Its words, and every number it sets standing alone.
                let numbers: Vec<String> = first.set.values().filter_map(|v| v.as_u64()).map(|n| n.to_string()).collect();
                for line in std::iter::once(b.name.as_str()).chain(b.rules.iter().map(|r| r.shown())).chain([b.standard.shown()]) {
                    let words: Vec<&str> = line.split(|c: char| !c.is_ascii_alphanumeric()).collect();
                    let written = [&first.short, &first.long].into_iter().find(|w| line.contains(w.as_str()))
                        .or_else(|| numbers.iter().find(|n| words.contains(&n.as_str())));
                    assert!(written.is_none(), "{path}: spans `{id}` and writes {written:?} out — write `{{{id}}}`:\n  {line}");
                }
                checked += 1;
            }
        }
        assert!(checked > 0, "some ruler spans an axis, or this checks nothing");
    }

    /// A TEST HAS ONE CORE, AND NOT NAMING IT IS A MISTAKE.
    ///
    /// The failure it refuses is silent by construction: a ruler with no
    /// `metric:` answered at the point of use can only be defaulted, which
    /// ranks a whole board in kills per minute whatever it was built to ask,
    /// and every number on it looks exactly like a right one.
    ///
    /// The three cases are the three a benchmark file can be in. Asserted on
    /// the function rather than on the shipped rulers, which all name `kpm` —
    /// a test that can only see the good case is not a test.
    #[test]
    fn a_ruler_names_the_one_metric_it_ranks_by() {
        let of = |yaml: &str| {
            core_metric(&serde_norway::from_str::<serde_norway::Value>(yaml).unwrap())
                .map(|m| m.id)
        };
        assert_eq!(of("metric: kpm\nduration: 180"), Ok("kpm"));
        assert_eq!(of("metric: dps"), Ok("dps"), "every id in `metrics::ALL`, not a list here");

        let absent = of("duration: 180").expect_err("a ruler with no core is refused");
        assert!(absent.contains("no metric"), "{absent}");

        let unknown = of("metric: ttk").expect_err("a metric this engine has no definition of");
        assert!(unknown.contains("ttk"), "{unknown}");
    }
}
