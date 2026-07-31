//! i18n overlays: `data/i18n/<locale>/*.yaml` → one [`LocaleSpec`] per locale.
//!
//! English is not a locale here — it is the source of truth living on each
//! entity's `name` field. Overlay files exist only for other languages, may
//! be arbitrarily incomplete (missing entries fall back to English in the
//! UI), and never touch ids. Referential integrity is enforced by the tests
//! below: every key must be a real id.
//!
//! **A locale is a DIRECTORY, and its files are merged** — because the content
//! in it has different AUTHORS, and they have different lifecycles:
//!
//! | file | author | lifecycle |
//! |---|---|---|
//! | `names.yaml`, `ui.yaml` | a translator | edited by hand |
//! | `descriptions.yaml` | DE, via their export | rewritten wholesale by `scripts/wfcd_i18n.py descriptions` |
//! | `evolutions.yaml` | DE, via a wiki transcription | edited by hand — there is no export to regenerate it from |
//!
//! Sharing one file would bury the hand-written parts under a thousand
//! generated lines and make regeneration a merge problem.
//!
//! Which sections come from where is not enforced — a locale's tables are
//! simply the union of its files, and the same table may not be filled twice
//! (a duplicate key is a hard error, not a last-one-wins).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct LocaleSpec {
    #[serde(default)]
    pub weapons: BTreeMap<String, String>,
    #[serde(default)]
    pub enemies: BTreeMap<String, String>,
    #[serde(default)]
    pub damage_types: BTreeMap<String, String>,
    #[serde(default)]
    pub mods: BTreeMap<String, String>,
    #[serde(default)]
    pub arcanes: BTreeMap<String, String>,
    #[serde(default)]
    pub evolutions: BTreeMap<String, String>,
    /// UI strings keyed by the English source string.
    #[serde(default)]
    pub ui: BTreeMap<String, String>,
    /// ORDERED effect-line substitutions: [regex, replacement] or
    /// [regex, replacement, flags]. The FALLBACK path — see
    /// [`LocaleSpec::mod_descriptions`] for what supersedes it.
    #[serde(default)]
    pub effect_phrases: Vec<Vec<String>>,
    /// A mod's card text in DE's OWN words, one entry per rank (rank 0 first),
    /// keyed by mod id.
    ///
    /// This is the whole sentence, already localized and already carrying that
    /// rank's numbers — not a template. It exists because a card is not a bag
    /// of terms: "+30% Fire Rate (x2 for Bows)" is "+30% 射速（弓类武器效果加
    /// 倍）" in DE's client, and no substitution table gets from "(x2 for
    /// Bows)" to "（弓类武器效果加倍）" — `effect_phrases` translated the terms
    /// and left the idiom in English, which is what shipped.
    ///
    /// Term-level substitution still runs for what DE never wrote: our own
    /// engine-generated effect lines, panel labels, and the entities DE's
    /// export cannot be joined to (Incarnon evolutions).
    #[serde(default)]
    pub mod_descriptions: BTreeMap<String, Vec<String>>,
    /// The same, for arcanes (`arcanes/*/*.yaml` ids).
    #[serde(default)]
    pub arcane_descriptions: BTreeMap<String, Vec<String>>,
    /// An Incarnon evolution's card text — ONE string (they have no ranks),
    /// `\n` between lines like the English `description` it mirrors.
    ///
    /// Hand-transcribed rather than generated: evolutions are not items, so
    /// they are in neither DE's PublicExport nor WFCD's derivative of it (see
    /// `data/i18n/zh/evolutions.yaml` for what was checked). That makes them
    /// the one card the phrase table used to mangle — "Increase Base Damage
    /// by +60." came out as "Increase Base 伤害 by +60." — and the reason the
    /// UI now shows a whole sentence or clean English, never a half-swapped
    /// one.
    #[serde(default)]
    pub evolution_descriptions: BTreeMap<String, String>,
}

impl LocaleSpec {
    /// Fold another file of the same locale in. Each table may be filled by
    /// only ONE file: two files claiming the same key is a data error (which
    /// half wins would depend on filename order), so it fails the build.
    fn merge(&mut self, other: LocaleSpec, path: &str) {
        fn maps(
            into: &mut BTreeMap<String, String>,
            from: BTreeMap<String, String>,
            path: &str,
            table: &str,
        ) {
            for (k, v) in from {
                assert!(into.insert(k.clone(), v).is_none(), "{path}: duplicate {table} key '{k}'");
            }
        }
        fn lists(
            into: &mut BTreeMap<String, Vec<String>>,
            from: BTreeMap<String, Vec<String>>,
            path: &str,
            table: &str,
        ) {
            for (k, v) in from {
                assert!(into.insert(k.clone(), v).is_none(), "{path}: duplicate {table} key '{k}'");
            }
        }
        maps(&mut self.weapons, other.weapons, path, "weapons");
        maps(&mut self.enemies, other.enemies, path, "enemies");
        maps(&mut self.damage_types, other.damage_types, path, "damage_types");
        maps(&mut self.mods, other.mods, path, "mods");
        maps(&mut self.arcanes, other.arcanes, path, "arcanes");
        maps(&mut self.evolutions, other.evolutions, path, "evolutions");
        maps(&mut self.ui, other.ui, path, "ui");
        maps(&mut self.evolution_descriptions, other.evolution_descriptions, path, "evolution_descriptions");
        lists(&mut self.mod_descriptions, other.mod_descriptions, path, "mod_descriptions");
        lists(&mut self.arcane_descriptions, other.arcane_descriptions, path, "arcane_descriptions");
        // ORDERED and therefore appended, not merged by key. Files are
        // embedded in path order (engine/build.rs sorts), so the result is
        // deterministic — but a locale that splits its phrase table across
        // files is asking for an order it cannot see, so: don't.
        self.effect_phrases.extend(other.effect_phrases);
    }
}

/// Every overlay locale, `(code, spec)` — the code is the DIRECTORY name
/// (`i18n/zh/ui.yaml` → `"zh"`), and a locale's files are merged into one
/// spec. A stray `i18n/*.yaml` at the top level belongs to no locale and is
/// rejected rather than silently ignored.
pub fn locales() -> &'static [(String, LocaleSpec)] {
    static L: OnceLock<Vec<(String, LocaleSpec)>> = OnceLock::new();
    L.get_or_init(|| {
        let mut out: Vec<(String, LocaleSpec)> = Vec::new();
        for (p, text) in crate::data::files_under("i18n/").filter(|(p, _)| p.ends_with(".yaml")) {
            let rest = p.strip_prefix("i18n/").unwrap_or(p);
            let code = rest
                .split_once('/')
                .unwrap_or_else(|| panic!("{p}: a locale is a DIRECTORY — move it to i18n/<code>/"))
                .0;
            let spec = serde_norway::from_str::<LocaleSpec>(text)
                .unwrap_or_else(|e| panic!("parse {p}: {e}"));
            match out.iter_mut().find(|(c, _)| c == code) {
                Some((_, acc)) => acc.merge(spec, p),
                None => out.push((code.to_string(), spec)),
            }
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mod by id across EVERY class pool (a weapon's pool is a union, and
    /// so is the set of mods an overlay may name).
    fn known_mod(id: &str) -> Option<crate::loadout::ModDef> {
        crate::mods_data::classes()
            .into_iter()
            .find_map(|c| crate::mods_data::class_pool(c).into_iter().find(|m| m.id == id))
    }

    fn known_arcane(id: &str) -> Option<&'static crate::arcanes_data::ArcaneDef> {
        crate::arcanes_data::slots()
            .into_iter()
            .find_map(|s| crate::arcanes_data::for_slot(s, id))
    }

    #[test]
    fn zh_overlay_loads() {
        let (_, zh) = locales().iter().find(|(c, _)| c == "zh").expect("zh overlay");
        assert_eq!(zh.weapons.get("dual_toxocyst").map(String::as_str), Some("毒囊双枪"));
        assert!(!zh.damage_types.is_empty());
        assert!(!zh.ui.is_empty());
        assert!(zh.effect_phrases.iter().all(|p| p.len() == 2 || p.len() == 3),
            "effect_phrases entries must be [regex, replacement(, flags)]");
    }

    /// Overlay keys must reference REAL ids — a translator's typo fails the
    /// build instead of silently showing English forever.
    #[test]
    fn overlay_keys_reference_real_ids() {
        let damage_types = [
            "impact", "puncture", "slash", "cold", "electricity", "heat", "toxin",
            "blast", "corrosive", "gas", "magnetic", "radiation", "viral", "void", "true",
        ];
        for (code, spec) in locales() {
            for id in spec.weapons.keys() {
                assert!(
                    crate::weapons_data::spec(id.as_str()).is_some(),
                    "i18n/{code}: unknown weapon id '{id}'"
                );
            }
            for id in spec.enemies.keys() {
                assert!(
                    crate::enemy_data::all().iter().any(|e| &e.id == id),
                    "i18n/{code}: unknown enemy id '{id}'"
                );
            }
            for id in spec.damage_types.keys() {
                assert!(
                    damage_types.contains(&id.as_str()),
                    "i18n/{code}: unknown damage type '{id}'"
                );
            }
            // EVERY pool, not just the pistol one: an overlay names mods of
            // whatever class, and checking one pool would reject a rifle
            // mod's translation as a typo.
            for id in spec.mods.keys().chain(spec.mod_descriptions.keys()) {
                assert!(known_mod(id).is_some(), "i18n/{code}: unknown mod id '{id}'");
            }
            for id in spec.arcanes.keys().chain(spec.arcane_descriptions.keys()) {
                assert!(known_arcane(id).is_some(), "i18n/{code}: unknown arcane id '{id}'");
            }
            for id in spec.evolutions.keys().chain(spec.evolution_descriptions.keys()) {
                assert!(
                    crate::evolutions_data::get(id.as_str()).is_some(),
                    "i18n/{code}: unknown evolution id '{id}'"
                );
            }
        }
    }

    /// DE's card text is per RANK, and it has to line up with the rank the
    /// card is showing: entry `r` is what rank `r` reads. A mod whose list is
    /// the wrong length means our `max_rank` and DE's disagree — which is a
    /// data finding about the mod, not a translation problem, and it would
    /// otherwise surface as a card showing the wrong rank's numbers.
    #[test]
    fn official_card_text_has_one_entry_per_rank() {
        for (code, spec) in locales() {
            for (id, ranks) in &spec.mod_descriptions {
                let m = known_mod(id).expect("checked above");
                assert_eq!(
                    ranks.len() as u32,
                    m.max_rank + 1,
                    "i18n/{code}: mod '{id}' has {} localized ranks, max_rank is {}",
                    ranks.len(),
                    m.max_rank
                );
            }
            for (id, ranks) in &spec.arcane_descriptions {
                let a = known_arcane(id).expect("checked above");
                assert_eq!(
                    ranks.len() as u32,
                    a.max_rank + 1,
                    "i18n/{code}: arcane '{id}' has {} localized ranks, max_rank is {}",
                    ranks.len(),
                    a.max_rank
                );
            }
        }
    }

    /// The localized card and OUR card must state the same numbers.
    ///
    /// The two sides are independent: our English text is the wiki module's
    /// wording with our own values filled in, and the localized text is DE's
    /// rendered string. That independence is the point — it is the same
    /// dual-source check the English side already gets (docs/DATA_SOURCES
    /// "rendered text vs levelStats"), and it is what catches a rebalance
    /// landing in the vendored export before it lands in our values.
    ///
    /// Three tolerances, every one of them earned by reading the failures:
    ///
    /// 1. **The ENDPOINTS only.** Our data stores `rank0` and `rankMax` and
    ///    INTERPOLATES the ranks between (`ModDescInfo::at`); DE ships a table
    ///    with its own intermediate values, and the two legitimately differ
    ///    there — Amalgam Serration reads 70.45 at rank 4 where DE prints 71,
    ///    and `metal_auger`'s ramp is documented as non-linear outright
    ///    (docs/DATA_SOURCES). So the middle is not a disagreement about the
    ///    mod, it is our interpolation showing; the two ranks we actually
    ///    store are where a mismatch means something.
    /// 2. **DE's client rounds AND truncates for display** — "+18% 多重射击"
    ///    for 18.25, "254%" for 254.6, "16.6%" for 16.67. A localized number
    ///    matches if it is ours rounded *or* truncated at its own precision.
    /// 3. **SUBSET, not equality** — a translation may drop a literal:
    ///    "(x2 for Bows)" is "（弓类武器效果加倍）", where the 2 became a
    ///    word. The direction that matters is checked: a number DE prints
    ///    that we cannot produce is a disagreement about the mod.
    #[test]
    fn localized_card_numbers_are_numbers_we_also_state() {
        /// Every number in a rendered card, with the DECIMALS it was written
        /// to — the precision is what decides how close a match must be.
        fn nums(s: &str) -> Vec<(f64, u32)> {
            let mut out = Vec::new();
            let mut cur = String::new();
            let mut flush = |cur: &mut String| {
                let t = cur.trim_end_matches('.');
                if !t.is_empty() {
                    if let Ok(v) = t.parse::<f64>() {
                        let dec = t.split_once('.').map_or(0, |(_, d)| d.len() as u32);
                        out.push((v, dec));
                    }
                }
                cur.clear();
            };
            for ch in s.chars() {
                if ch.is_ascii_digit() || (ch == '.' && !cur.is_empty()) {
                    cur.push(ch);
                } else {
                    flush(&mut cur);
                }
            }
            flush(&mut cur);
            out
        }
        let mut bad: Vec<String> = Vec::new();
        for (code, spec) in locales() {
            for (id, ranks) in &spec.mod_descriptions {
                let Some(info) = crate::mods_data::desc_info(id) else { continue };
                for (rank, text) in ranks.iter().enumerate() {
                    let rank = rank as u32;
                    if rank != 0 && rank != info.max_rank {
                        continue; // interpolated — see (1) above
                    }
                    let ours = info.at(rank);
                    let mine = nums(&ours);
                    for (z, dec) in nums(text) {
                        let step = 10f64.powi(-(dec as i32));
                        let shown = |o: f64| {
                            let scaled = o.abs() / step;
                            // rounded, or truncated — DE's client does both.
                            (scaled.round() * step - z.abs()).abs() <= 1e-9
                                || (scaled.floor() * step - z.abs()).abs() <= 1e-9
                        };
                        if !mine.iter().any(|(o, _)| shown(*o)) {
                            bad.push(format!(
                                "i18n/{code} {id} rank {rank}: localized states {z}, ours reads '{}'",
                                ours.replace('\n', " / ")
                            ));
                        }
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{} disagreements:\n{}", bad.len(), bad.join("\n"));
    }

    /// An evolution's NAME and its card text are transcribed together from
    /// the same wiki page, so one without the other is a half-finished entry
    /// — a card headed 灵化形态 whose body is still English, or the reverse.
    /// Partial translation stays legal, but only per ENTITY.
    #[test]
    fn a_transcribed_evolution_has_both_a_name_and_its_text() {
        for (code, spec) in locales() {
            for id in spec.evolutions.keys() {
                assert!(
                    spec.evolution_descriptions.contains_key(id),
                    "i18n/{code}: evolution '{id}' is named but its card text is not"
                );
            }
            for id in spec.evolution_descriptions.keys() {
                assert!(
                    spec.evolutions.contains_key(id),
                    "i18n/{code}: evolution '{id}' has card text but no name"
                );
            }
        }
    }
}
