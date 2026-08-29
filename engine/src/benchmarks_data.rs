//! OFFICIAL TEST SCENARIOS — `data/benchmarks/`.
//!
//! A benchmark is a scenario that belongs to no weapon. Every other scenario in
//! the app is a PRESET: per weapon, editable, stored in the browser. This one is
//! read-only, defined once, and appears on every weapon — which is the whole
//! point, because a number only means something against a ruler someone else
//! can pick up.
//!
//! # The rule that makes it scale
//!
//! **A benchmark may not name a weapon, a mod, an arcane, an evolution, or a
//! weapon form.** [`no_benchmark_names_a_weapon`] enforces it, and the
//! enforcement is the feature: a definition that never speaks about a member of
//! the roster cannot be wrong about a member it has never seen. Adding weapon
//! 201 can therefore never require editing a benchmark.
//!
//! It is a stronger guarantee than tagging weapons would give. A tag scheme is
//! correct only while every weapon is tagged correctly — one missed tag on
//! weapon 201 is a silently wrong board. "Mentions no weapon at all" has no
//! per-weapon step to get wrong.
//!
//! What replaces the per-weapon settings is POLICY, which the engine already
//! resolves: `form: default` is the Incarnon cycle where a weapon has one and
//! its arsenal default where it does not; an omitted `headshot_pct` is 0 for a
//! sentinel weapon and 100 otherwise; omitted `buffs` means each buff opens at
//! the start state docs/BUFFS.md gives it, which is a per-BUFF property and so
//! identical on every weapon.
//!
//! # No version numbers
//!
//! There is one board per benchmark, it is regenerated WHOLE whenever this file
//! or the engine changes, and what is deployed is always the current answer —
//! so a version number would mark a distinction nobody could act on. What this file said last week is git's business.
//!
//! CHANGING A TERM HERE RE-SCORES; it does not discard. A build submitted when
//! the fight was 300 s is still a build when it becomes 400 s — the standard
//! changed and the build did not, so it carries over and competes, and whatever
//! beats it displaces it. That is the whole reason a submission stores the
//! BUILD and never a score.
//!
//! `id` is the identity, and it is stable across rewordings. A genuinely
//! different ruler — `group_clear` — is a different id and keeps its own board.

use std::sync::OnceLock;

use serde::Deserialize;

/// What a benchmark ADMITS: the shape a build must have to be scored on it.
///
/// Separate from the fight on purpose, and separate from IDENTITY on purpose.
/// Canonicalisation (`builds::canonical_mods`) is universal — two boards must
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
    /// 19x19 group ruler, most of the damage. The rule stays per RULER because
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
    pub rules: Vec<String>,
    /// What a build must look like to be admitted. Absent = admit anything
    /// legal, which is a real answer for a benchmark that wants one.
    #[serde(default)]
    pub build: BuildRequirement,
    /// THE ONE A READER MEETS FIRST, and there is at most one.
    ///
    /// The rulers were in PATH ORDER and that was the same thing as "the
    /// primary one" while `single_target.yaml` sorted first. Adding
    /// `group_clear.yaml` broke it in two places at once: the
    /// board page opened on a brand-new EMPTY ranking, and — worse — every
    /// first-time visitor's default SCENARIO became a 361-body fight, because
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
}

/// A CROWD IN THREE NUMBERS, expanded HERE and nowhere else.
///
/// `formation_grid: {cols, rows, spacing_m}` lays a regular grid around the
/// body being aimed at — front rank centred on it, every other rank one spacing
/// further along the shot's own line — and becomes an ordinary `formation`
/// list before anything downstream sees the scenario.
///
/// WHY THE SHORTHAND EXISTS: 361 bodies written out is 360 lines nobody can
/// check by reading, and a ruler whose terms cannot be argued with is not a
/// ruler.
///
/// WHY IT IS EXPANDED HERE: because the PAGE has to draw the crowd. The arena
/// is the source — what you see is what gets simulated — and it reads
/// `formation`. Expanding at simulate time instead would have left the canvas
/// drawing one body for a 361-body fight, and expanding in both places is the
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
    let point = |k: &str, d: [f64; 2]| -> crate::space::Vec2 {
        sc.get(k)
            .and_then(Value::as_sequence)
            .filter(|a| a.len() == 2)
            .map_or(crate::space::Vec2::new(d[0], d[1]), |a| {
                crate::space::Vec2::new(
                    a[0].as_f64().unwrap_or(d[0]),
                    a[1].as_f64().unwrap_or(d[1]),
                )
            })
    };
    let player = point("player_at", [0.0, 0.0]);
    let target = point("target_at", [0.0, crate::space::CONTACT_RANGE_M]);
    let forward = crate::space::Vec2::new(target.x - player.x, target.y - player.y);
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

/// Every official benchmark, parsed once, in path order.
pub fn all() -> &'static [Benchmark] {
    static B: OnceLock<Vec<Benchmark>> = OnceLock::new();
    B.get_or_init(|| {
        let mut v: Vec<Benchmark> = crate::data::files_under("benchmarks/")
            // THIS level only. `benchmarks/boards/` holds the measured results
            // and is a different shape entirely — a prefix scan would try to
            // parse a board as a benchmark and fail on a missing `id`.
            .filter(|(p, _)| p.ends_with(".yaml") && !p["benchmarks/".len()..].contains('/'))
            .map(|(p, text)| {
                let mut b: Benchmark =
                    serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                expand_formation_grid(&mut b.scenario)
                    .unwrap_or_else(|e| panic!("{p}: {e}"));
                b
            })
            .collect::<Vec<_>>();
        // THE PRIMARY RULER FIRST, then path order. See `Benchmark::primary`.
        v.sort_by_key(|b| !b.primary);
        v
    })
}

/// The benchmark with this id.
pub fn get(id: &str) -> Option<&'static Benchmark> {
    all().iter().find(|b| b.id == id)
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
    let mut ids: Vec<String> = crate::weapons_data::all()
        .iter()
        .map(|w| w.id.clone())
        .collect();
    ids.extend(crate::evolutions_data::pool().iter().map(|e| e.id.clone()));
    for class in crate::mods_data::classes() {
        ids.extend(
            crate::mods_data::pool_union(&[class.to_string()])
                .into_iter()
                .map(|m| m.id.to_string()),
        );
    }
    for slot in crate::arcanes_data::slots() {
        ids.extend(
            crate::arcanes_data::slot_pool(slot)
                .iter()
                .map(|a| a.id.to_string()),
        );
    }
    // Named FORMS. `default` is the policy and is the only legal value: it is
    // what resolves per weapon. `base` / `incarnon` / `charged` name a form
    // some weapons have and others do not, so a benchmark asking for one gets
    // a silent per-weapon fallback — one name, two different tests.
    ids.extend(
        crate::weapons_data::all()
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
    fn the_official_single_target_benchmark_is_what_we_published() {
        let b = get("single_target").expect("data/benchmarks/single_target.yaml");
        assert_eq!(b.name, "Single Target · Thrax Centurion Lv 9999 SP · 180 s · KPM");
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
        // AND NO FORM. How a weapon is played belongs to the ENTRANT, not to
        // the ruler: a Torid through its Incarnon cycle and a Torid that never
        // transmutes are two rows here, and a benchmark that pinned one could
        // rank only that one. `form: default` used to sit in the yaml and read
        // as neutral — it resolved to the cycle on a weapon that has one, so
        // the board ranked every Incarnon weapon at its ceiling and could not
        // be asked for anything else.
        assert_eq!(s("form"), None, "a ruler may not say how a weapon is played");
        // Pinned, not defaulted: a published number has to be reproducible by
        // whoever doubts it.
        assert_eq!(s("seed").and_then(|v| v.as_u64()), Some(0xC0FFEE));

        // AIMED, AND EVERY SHOT ON THE WEAK POINT — both pinned, because they
        // are terms of this board's claim and a reader should find them in the
        // ruler rather than in a default. They are two different facts: the
        // stance gates mods like Argon Scope, the rate is where the shots land.
        //
        // This was omitted for a long time on the argument that pinning it
        // would rank a SENTINEL at a headshot rate it cannot reach. That is
        // still true and it is now handled where it belongs: `parse_fight`
        // pins a sentinel's rate at 0 whatever the request says, the same way
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
    /// The spacing is written THREE times in `group_clear.yaml` — the machine
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
    /// compare, only 361 positions.
    ///
    /// It is deliberately not a template. A ruler's name is written for a
    /// reader and generating it would flatten every one of them into the same
    /// sentence; what has to hold is that the numbers in it are this ruler's.
    #[test]
    fn a_rulers_name_and_rules_quote_its_own_spacing() {
        // THE RAW YAML, because `all()` expands the grid away: after
        // `expand_formation_grid` there is no spacing left to compare against,
        // only 361 positions.
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
            let prose = format!("{} {}", b.name, b.rules.join(" "));
            assert!(
                prose.contains(&format!("{token} m")) || prose.contains(&format!("{token} metres")),
                "{id}: the grid is {spacing} m and neither the name nor the rules say so.\n  \
                 name:  {}\n  This is the drift that put '19x19 at 1.5 m' over a 3 m fight.",
                b.name
            );
        }
        assert!(checked > 0, "at least one ruler lays a grid, or this checks nothing");
    }
}
