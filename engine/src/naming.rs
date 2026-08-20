//! THE NAMING CONVENTION, enforced — see `docs/NAMING.md`.
//!
//! The owner's rule (2026-08-20): a name may be LONG, but it must have
//! STRUCTURE and LOGIC, and information is never traded away for brevity.
//!
//! A document alone would not have held. The survey that produced the
//! convention found three spellings of ONE concept — `duration_s`,
//! `duration_secs` and `duration_seconds`, all in this engine, all meaning a
//! duration in seconds — which is what a rule with nothing checking it decays
//! into. This module is the check.
//!
//! IT IS A SPELLING CHECK AND NOT A STYLE OPINION. It refuses the SECOND
//! spelling of a unit or a role that already has one, because that is the
//! failure that costs a reader something: a bare number whose unit they have to
//! go and look up, or two fields they cannot tell apart.

/// A forbidden suffix or prefix, and what to write instead.
///
/// Ordered longest-first where two could match, so `_secs` is reported as
/// `_secs` rather than as `_s`.
const FORBIDDEN: &[(&str, &str)] = &[
    // ONE SPELLING PER UNIT (docs/NAMING.md §2).
    ("_secs", "_seconds"),
    ("_sec", "_seconds"),
    ("_s", "_seconds"),
    ("_meters", "_m"),
    ("_metres", "_m"),
    ("_degrees", "_deg"),
    ("_percent", "_pct"),
    // ONE SPELLING PER ROLE (§3), and no abbreviated words (§4).
    ("_mult", "_multiplier"),
    ("_mul", "_multiplier"),
    ("_dmg", "_damage"),
    ("dmg_", "damage_"),
    ("_eff", "_effectiveness"),
];

/// AN ABBREVIATED SUBJECT, anywhere in a name — the class the owner asked about
/// second (2026-08-20), and a worse one than a mis-spelled unit: a reader of
/// `bd_eximus_expiry` has to ALREADY KNOW that `bd` is base damage before the
/// name tells them anything at all.
///
/// Matched on whole underscore-separated PARTS, so `cd` is caught in `cd_rel`
/// and `bodyshot_cd` while `cold` and `radius` are left alone. This is the rule
/// that cannot be a suffix table, because the abbreviation is usually at the
/// front.
///
/// `multishot` IS THE ONE THAT PROVES THE RULE. It meant MULTISHOT in the engine and
/// MILLISECONDS in `one_fight` — one two-letter name, two units, in one
/// codebase. Neither use survives.
/// A BULK RENAME ONCE ATE THIS TABLE (2026-08-20): a sweep that expanded `ms`
/// everywhere rewrote the LEFT column too, turning `("ms", "multishot")` into
/// `("multishot", "multishot")` — after which every name containing `multishot`
/// was reported as needing to become `multishot`. `the_table_is_not_a_fixed_point`
/// below is the guard, because the failure reads as 335 violations rather than
/// as a broken checker.
const ABBREVIATED: &[(&str, &str)] = &[
    ("bd", "base_damage"),
    ("cc", "crit_chance"),
    ("cd", "crit_damage"),
    ("sd", "status_damage"),
    ("ms", "multishot"),
    ("fr", "fire_rate"),
    ("pt", "punch_through"),
    ("dt", "frame_seconds"),
    ("dmg", "damage"),
    ("mult", "multiplier"),
    ("frac", "fraction"),
    ("mag", "magazine"),
    ("amt", "amount"),
    ("val", "value"),
    ("cnt", "count"),
    ("eff", "effectiveness"),
    ("rel", "relative"),
];

/// Names that break a rule and STAY — the frozen wire and stored-preset
/// spellings of `docs/NAMING.md` §6, and nothing else.
///
/// A name earns a place here by being DURABLE: renaming it would migrate every
/// stored preset or invalidate every share link already posted. That is a much
/// higher bar than "it would be annoying to change", and the list is meant to
/// shrink rather than grow.
const FROZEN: &[&str] = &[
    // The Tenno block's wire fields, which sit inside stored scenario presets.
    "wf_energy_pct",
    "wf_armor",
    // 0..100 on the wire because it is what a person types into a box, where
    // every other `_pct` in the engine is a 0..1 fraction.
    "headshot_pct",
    // A NEGATIVE BOOLEAN, and a real defect (§5) — `if !no_resupply` is a
    // double negative every reader unpicks. It is in stored presets.
    "no_resupply",
];

/// Is this a name the convention forbids? Returns what it should be called.
///
/// EVERY RULE THAT APPLIES, not the first one: `dmg_mult` breaks two — an
/// abbreviated subject and an abbreviated role — and reporting it as
/// `dmg_multiplier` would send the reader round again. It rewrites until the
/// name stops changing, so what comes back is a name this function accepts.
pub fn forbidden(name: &str) -> Option<String> {
    if FROZEN.contains(&name) {
        return None;
    }
    // AN ABBREVIATED PART FIRST, because it is usually at the front and the
    // suffix rules below cannot see it.
    let parts: Vec<&str> = name.split('_').collect();
    if let Some(fixed) = parts
        .iter()
        .any(|p| ABBREVIATED.iter().any(|(a, _)| a == p))
        .then(|| {
            parts
                .iter()
                .map(|p| {
                    ABBREVIATED
                        .iter()
                        .find(|(a, _)| a == p)
                        .map_or(*p, |(_, full)| *full)
                })
                .collect::<Vec<_>>()
                .join("_")
        })
    {
        return Some(fixed);
    }
    let mut out = name.to_string();
    // Bounded rather than `loop`: every rule lengthens the name, so it settles
    // in a few passes, and a cap means a rule pair that somehow cycled would
    // fail a test rather than hang one.
    for _ in 0..FORBIDDEN.len() {
        let mut moved = false;
        for (bad, good) in FORBIDDEN {
            if let Some(stem) = out.strip_suffix(bad) {
                // `_s` must not fire on a name that merely ENDS in an s-word:
                // `pellets`, `stacks`, `hops` have no underscore before the s.
                if !stem.is_empty() {
                    out = format!("{stem}{good}");
                    moved = true;
                }
            }
            if bad.ends_with('_') {
                if let Some(rest) = out.strip_prefix(bad) {
                    out = format!("{good}{rest}");
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    (out != name).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EVERY DATA KEY AND EVERY ENGINE FIELD OBEYS THE CONVENTION.
    ///
    /// It walks the yaml and the source rather than a list of names, so a field
    /// added tomorrow is covered by nobody — the same shape as every other
    /// ratchet in this repo.
    #[test]
    fn forbidden_spellings_never_come_back() {
        let mut bad: Vec<String> = Vec::new();
        let mut seen = 0usize;

        // AN ID IS A NAME, NOT A FIELD, and the convention does not govern it.
        // `tainted_mag` is DE's own mod, `tenet_detron_magazine_burst` would be
        // a rename of a weapon id that sits in every stored preset and every
        // share link already posted. Collected from the data itself rather than
        // listed, so a weapon added tomorrow exempts its own id.
        let mut ids: std::collections::BTreeSet<String> = Default::default();
        for (path, text) in crate::data::files_under("") {
            if !path.ends_with(".yaml") {
                continue;
            }
            for line in text.lines() {
                if let Some(v) = line.trim_start().strip_prefix("id:") {
                    ids.insert(v.trim().trim_matches('"').to_string());
                }
            }
        }
        assert!(ids.len() > 400, "only {} ids collected", ids.len());

        // 1. EVERY `data/` YAML KEY. The i18n overlays are skipped: their keys
        //    are English SENTENCES, not identifiers.
        for (path, text) in crate::data::files_under("") {
            if !path.ends_with(".yaml") || path.starts_with("i18n/") {
                continue;
            }
            for line in text.lines() {
                let t = line.trim_start();
                if t.starts_with('#') {
                    continue;
                }
                let Some((key, _)) = t.split_once(':') else { continue };
                let key = key.trim_start_matches("- ").trim();
                if key.is_empty() || !key.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()) {
                    continue;
                }
                if ids.contains(key) {
                    continue;
                }
                seen += 1;
                if let Some(want) = forbidden(key) {
                    bad.push(format!("{path}: `{key}` should be `{want}`"));
                }
            }
        }
        assert!(seen > 2000, "only {seen} yaml keys scanned — the walk broke");

        // 2. EVERY STRUCT FIELD IN THE ENGINE, which is where the mess this
        //    convention was written for actually lived: `crit_mult` and
        //    `crit_multiplier` were ten uses against eight, in one crate.
        //
        //    Read from disk rather than embedded — source is not `data/`, and a
        //    test is the one place that may look at its own crate's files.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut rust = 0usize;
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&p) else { continue };
                let file = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                for line in text.lines() {
                    let t = line.trim_start();
                    // A FIELD DECLARATION, not a use: `name: Type`. Anything
                    // with an `=` is a binding or a literal and is skipped, so
                    // this reads what a struct PROMISES rather than every local.
                    if t.starts_with("//") || t.contains('=') {
                        continue;
                    }
                    let t = t.strip_prefix("pub ").unwrap_or(t);
                    let Some((name, rest)) = t.split_once(':') else { continue };
                    // A TYPE, and PRIMITIVES COUNT. Requiring an uppercase
                    // initial skipped every `f64` field in the crate, which is
                    // most of them — the check ran green over `crit_mult: f64`
                    // and was caught only by sabotaging it (2026-08-20). A
                    // ratchet that cannot fail is not a ratchet.
                    let ty = rest.trim_start();
                    let primitive = ["f64", "f32", "u8", "u16", "u32", "u64", "usize", "i8",
                                     "i16", "i32", "i64", "isize", "bool", "char", "str"]
                        .iter()
                        .any(|p| ty.starts_with(p));
                    if !(ty.starts_with(|c: char| c.is_ascii_uppercase()) || ty.starts_with('&') || primitive) {
                        continue;
                    }
                    let name = name.trim();
                    if name.is_empty()
                        || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
                    {
                        continue;
                    }
                    rust += 1;
                    if let Some(want) = forbidden(name) {
                        bad.push(format!("{file}: `{name}` should be `{want}`"));
                    }
                }
            }
        }
        assert!(rust > 900, "only {rust} rust fields scanned — the walk broke");

        bad.sort();
        bad.dedup();
        assert!(
            bad.is_empty(),
            "{} name(s) break docs/NAMING.md:\n  {}",
            bad.len(),
            bad.join("\n  ")
        );
    }

    /// NO RULE MAY BE ITS OWN ANSWER. An entry whose two halves are equal is a
    /// table a bulk rename has walked over, and it turns the checker into a
    /// machine that reports every correct name as wrong.
    #[test]
    fn the_table_is_not_a_fixed_point() {
        for (bad, good) in ABBREVIATED {
            assert_ne!(bad, good, "`{bad}` expands to itself");
        }
        for (bad, good) in FORBIDDEN {
            assert_ne!(bad, good, "`{bad}` corrects to itself");
        }
    }

    /// THE RULES THEMSELVES, so the checker cannot quietly stop checking.
    #[test]
    fn the_convention_says_what_it_means() {
        // A unit with a second spelling is refused, and told what to be.
        assert_eq!(forbidden("duration_s").as_deref(), Some("duration_seconds"));
        assert_eq!(forbidden("duration_secs").as_deref(), Some("duration_seconds"));
        assert_eq!(forbidden("crit_mult").as_deref(), Some("crit_multiplier"));
        assert_eq!(forbidden("dmg_mult").as_deref(), Some("damage_multiplier"));
        // …and the spelling that WON is fine.
        assert_eq!(forbidden("duration_seconds"), None);
        assert_eq!(forbidden("crit_multiplier"), None);
        assert_eq!(forbidden("punch_through_m"), None);
        assert_eq!(forbidden("falloff_start_m"), None);
        // A PLURAL IS NOT A UNIT. `_s` must not fire on a count.
        assert_eq!(forbidden("pellets"), None);
        assert_eq!(forbidden("stacks"), None);
        assert_eq!(forbidden("hops"), None);
        // AN ABBREVIATED PART, wherever it sits in the name.
        assert_eq!(forbidden("bd_eximus_expiry").as_deref(), Some("base_damage_eximus_expiry"));
        assert_eq!(forbidden("cc_rel").as_deref(), Some("crit_chance_relative"));
        assert_eq!(forbidden("ms_per_run").as_deref(), Some("multishot_per_run"));
        // …and a word that merely CONTAINS one is not an abbreviation: only
        // whole underscore-separated parts count.
        assert_eq!(forbidden("cold_stacks"), None);
        assert_eq!(forbidden("radius_m"), None);
        assert_eq!(forbidden("compression_multiplier"), None);
        // THE FROZEN NAMES pass, and only because they are listed.
        assert_eq!(forbidden("headshot_pct"), None);
        assert_eq!(forbidden("no_resupply"), None);
        assert!(
            FROZEN.len() <= 4,
            "the frozen list is meant to SHRINK; {} names is a growing exemption",
            FROZEN.len()
        );
    }
}
