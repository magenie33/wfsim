//! i18n overlays: `data/i18n/<locale>/*.yaml` → one [`LocaleSpec`] per locale.
//!
//! English is not a locale here — it is the source of truth on each entity's
//! `name` field. Overlay files exist only for other languages, may be
//! arbitrarily incomplete (missing entries fall back to English), and never
//! touch ids; every key must be a real id.
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
//! generated lines and make regeneration a merge problem. Which sections come
//! from where is not enforced — a locale's tables are the union of its files,
//! and a duplicate key is a hard error rather than last-one-wins.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize, serde::Serialize)]
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
    /// WARFRAME ABILITY BUFFS (`data/abilities/`), keyed by id.
    ///
    /// TRANSCRIBED FROM DE, never translated — the house rule, and this family
    /// is where it bites hardest: Roar is 战吼 and Eclipse is 黯然失色, neither
    /// of which anyone derives from the English. They come from the export's
    /// own `i18n.json`, off the frame's ability list and the augment mods'
    /// cards. The FRAME names are deliberately absent: DE's Chinese client
    /// leaves them in English ("Rhino"), so translating them would be adding a
    /// word DE did not write.
    #[serde(default)]
    pub abilities: BTreeMap<String, String>,
    /// AURAS (`data/auras/`), keyed by id — DE's OWN client strings, joined on
    /// the aura's `internal_name`. An aura IS a mod in the export, so seven of
    /// the eight came straight out of `i18n.json`; EMP Aura is not in it and is
    /// deliberately absent rather than derived (data/i18n/zh/names.yaml says
    /// so).
    #[serde(default)]
    pub auras: BTreeMap<String, String>,
    /// ARCHON SHARDS, keyed by `<shard_id>` for the colour's own name and by
    /// `<shard_id>/<effect_id>` for one socket's card line. Two key shapes in
    /// one map because they are one family and the effect has no id of its own
    /// outside its colour.
    ///
    /// TRANSCRIBED FROM THE CN WIKI, which is the source `data/README.md`
    /// names for what DE's export has no entity for — a shard is a RESOURCE
    /// and `i18n.json` carries none of them.
    #[serde(default)]
    pub shards: BTreeMap<String, String>,
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
    /// the one card a phrase table mangles — "Increase Base Damage by +60."
    /// comes out as "Increase Base 伤害 by +60." — and the reason the UI shows
    /// a whole sentence or clean English, never a half-swapped one.
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
        maps(&mut self.abilities, other.abilities, path, "abilities");
        maps(&mut self.auras, other.auras, path, "auras");
        maps(&mut self.shards, other.shards, path, "shards");

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

    /// EVERY ADMISSION IS TRANSLATED, in every locale — and a REASON is
    /// translated ONCE rather than once per set of numbers.
    ///
    /// A weapon's `unmodeled:` lines are the one family of prose that is BOTH
    /// written by us and shown on the page, so they are the family that
    /// silently accumulates an English backlog: nothing fails when a new one
    /// is added without its overlay entry, the check_disclosure pass still
    /// goes green (it walks a fixed set of weapons), and a Chinese reader gets
    /// the English paragraph. Sixteen had built up that way before anybody
    /// counted.
    ///
    /// WHAT CHANGED THE SAME DAY: an audit found 116 distinct sentences over
    /// 248 uses, thirteen families of near-duplicates inside them — sixteen
    /// spellings of the damage-falloff line differing only in three numbers.
    /// So the repeating ones became REASONS with parameters
    /// (`data/unmodelled/reasons.yaml`), and this asks for the TEMPLATE. A
    /// weapon whose falloff starts at a new distance now costs zero
    /// translation; before, it cost one.
    #[test]
    fn every_unmodelled_line_is_in_every_overlay() {
        let mut missing: Vec<String> = Vec::new();
        for (code, spec) in locales() {
            let mut want: std::collections::BTreeSet<(&str, String)> =
                std::collections::BTreeSet::new();
            for w in crate::weapons_data::all() {
                for part in &w.unmodeled_parts {
                    // A REASON wants its template; prose wants itself.
                    match part.template.as_deref() {
                        Some(tpl) => want.insert((w.id.as_str(), tpl.to_string())),
                        None => want.insert((w.id.as_str(), part.text.clone())),
                    };
                }
            }
            for (wid, line) in want {
                if !spec.ui.contains_key(&line) {
                    // BY CHARACTERS, NOT BYTES. An admission is prose and
                    // prose has em dashes in it, so a byte slice lands inside
                    // one and the test PANICS on the message rather than
                    // failing with it — which reads as a broken test on the
                    // day somebody adds a weapon.
                    missing.push(format!(
                        "[{code}] {wid}: {}",
                        line.chars().take(70).collect::<String>()
                    ));
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "{} admission(s) would render in English on a localized page. A REASON is              translated once — add its template from data/unmodelled/reasons.yaml to              data/i18n/<locale>/ui.yaml:
  {}",
            missing.len(),
            missing.join("
  ")
        );
    }

    /// 赋能 IS ARCANE AND 灵化 IS INCARNON — two of DE's own terms that collide
    /// in one compound, and the collision has shipped once.
    ///
    /// "赋能形态" reads as "Arcane form" and means nothing; the Incarnon form is
    /// 灵化形态, which is what all 75 evolution names and every other line in
    /// the overlay already say. One line said otherwise for a day
    /// — written by translating the English rather than
    /// reaching for the term already in the file, which is the exact failure
    /// the transcribe-never-translate rule exists to stop.
    ///
    /// The assertion is deliberately on the COMPOUND, not on 赋能: the bare
    /// word is correct everywhere it appears, fifteen times, for Arcane.
    #[test]
    fn incarnon_is_never_translated_as_if_it_were_arcane() {
        for (path, text) in crate::data::files_under("i18n/") {
            assert!(
                !text.contains("赋能形态"),
                "{path}: 赋能 is Arcane; the Incarnon form is 灵化形态"
            );
        }
        // …and the right term is in fact there, so this cannot pass by the
        // overlay being empty.
        let zh: String = crate::data::files_under("i18n/zh/")
            .map(|(_, text)| text.to_string())
            .collect();
        assert!(zh.contains("灵化形态"), "the zh overlay lost 灵化形态 entirely");
    }

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

    /// A UI STRING IS NEVER EMPTY, in any locale.
    ///
    /// `ui:` is hand-written prose, so an entry with no value is a key nobody
    /// translated — the page silently falls back to English and nothing says
    /// so. `names.yaml` is deliberately exempt: an empty NAME there means "DE's
    /// own string could not be reached", which is the documented way to say it
    /// (AGENTS: leave it empty and say so).
    ///
    /// The failure it was written for is structural: a LINE-WISE re-sort of
    /// `data/i18n/zh/ui.yaml` splits every two-line entry, leaving the key
    /// where it sorts and sending its indented value line off as if it were a
    /// record of its own. Twenty-nine translations came out empty and the
    /// orphaned value lines folded into whichever key sorted before them — the
    /// share panel's second button was 1,746 characters long.
    ///
    /// EMPTY IS THE COMPLETE TELL: however the orphan lands, the key it left
    /// behind has nothing, so this catches the sort at its source without a
    /// length threshold somebody would have to pick.
    #[test]
    fn no_ui_string_is_empty() {
        for (code, spec) in locales() {
            let blank: Vec<&str> = spec
                .ui
                .iter()
                .filter(|(_, v)| v.trim().is_empty())
                .map(|(k, _)| k.as_str())
                .collect();
            assert!(
                blank.is_empty(),
                "{code}: {} ui strings have no translation and fall back to English silently: {:?}",
                blank.len(),
                &blank[..blank.len().min(8)]
            );
        }
    }

    /// Overlay keys must reference REAL ids — a translator's typo fails the
    /// build instead of silently showing English forever.
    #[test]
    fn overlay_keys_reference_real_ids() {
        // Derived, not re-typed: `DamageType::name()` is the one spelling of
        // a damage type, the same token the data files and the api use. A
        // hand-kept copy here rejected `void` as a typo the moment the enemy
        // card started printing the faction column.
        let damage_types: Vec<&str> = crate::damage::DamageType::ALL
            .iter()
            .map(|t| t.name())
            .collect();
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
            for id in spec.auras.keys() {
                assert!(
                    crate::auras_data::by_id(id.as_str()).is_some(),
                    "i18n/{code}: unknown aura id '{id}'"
                );
            }
            // TWO KEY SHAPES, and both are checked: a colour's own name, and
            // `<shard>/<effect>` for one socket's line. A typo on either half
            // of the second is the one an English fallback hides forever.
            for id in spec.shards.keys() {
                let (shard, effect) = match id.split_once('/') {
                    Some((a, b)) => (a, Some(b)),
                    None => (id.as_str(), None),
                };
                let d = crate::shards_data::all().iter().find(|x| x.id == shard);
                assert!(d.is_some(), "i18n/{code}: unknown shard id '{shard}'");
                if let Some(e) = effect {
                    assert!(
                        d.unwrap().options.iter().any(|o| o.id == e),
                        "i18n/{code}: unknown shard effect '{id}'"
                    );
                }
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

    /// Every INDIRECT stat label is translated.
    ///
    /// The panel renders these rows through `tr(label)`, an EXACT-match
    /// lookup, so a label with no entry silently prints in English on a
    /// Chinese page. All ten of the original ones did exactly that until
    /// 2026-08-01 — nothing was watching, because the miss is invisible from
    /// the English side. Ten more landed that day, which is what made it worth
    /// a guard rather than another sweep.
    ///
    /// Listed explicitly because `IndirectStat` has no `ALL`: a new variant
    /// fails here until it is added to both, which is the point.
    #[test]
    fn every_indirect_stat_label_is_translated() {
        use crate::loadout::IndirectStat as I;
        const ALL: [I; 20] = [
            I::Recoil, I::Noise, I::AmmoMax, I::ProjectileSpeed, I::HolsteredReload,
            I::DodgeSpeed, I::AcrobaticSpeed, I::Accuracy, I::PunchThrough, I::Zoom,
            I::Range, I::BeamRange, I::MovementSpeed, I::SprintSpeed, I::AmmoConversion,
            I::StaggerResist, I::SelfStagger, I::DoubleJump, I::KillExplosion,
            I::StatusSpread,
        ];
        let mut missing: Vec<String> = Vec::new();
        for (code, spec) in locales() {
            for s in ALL {
                if !spec.ui.contains_key(s.label()) {
                    missing.push(format!("i18n/{code}: \"{}\"", s.label()));
                }
            }
        }
        assert!(missing.is_empty(), "untranslated stat labels:\n{}", missing.join("\n"));
    }

    /// A card never states the same line twice.
    ///
    /// DE's `levelStats` is not one line per entry: on a rank that UNLOCKS
    /// extra bonuses, the first entry is the whole card — unlock lines
    /// included — and the remaining entries repeat those same lines on their
    /// own. Joining them verbatim printed them twice, which is what the top
    /// rank of Primary Deadhead read in the UI ("+30% 爆头倍率" twice).
    /// `scripts/wfcd_i18n.py card_text` drops the repeats; this is the guard
    /// that a regeneration cannot bring them back.
    #[test]
    fn no_localized_card_repeats_a_line() {
        let mut bad: Vec<String> = Vec::new();
        for (code, spec) in locales() {
            let cards = spec.mod_descriptions.iter().chain(&spec.arcane_descriptions);
            for (id, ranks) in cards {
                for (rank, text) in ranks.iter().enumerate() {
                    let lines: Vec<&str> =
                        text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
                    let mut seen: std::collections::BTreeSet<&str> = Default::default();
                    for line in &lines {
                        if !seen.insert(line) {
                            bad.push(format!("i18n/{code} {id} rank {rank}: '{line}' twice"));
                        }
                    }
                }
            }
        }
        assert!(bad.is_empty(), "repeated card lines:\n{}", bad.join("\n"));
    }

    /// The localized card and OUR card must state the same numbers.
    ///
    /// The two sides are independent — our English text is the wiki module's
    /// wording with our own values, the localized text DE's rendered string —
    /// which is what catches a rebalance landing in the vendored export first.
    /// Three tolerances, each earned by reading the failures:
    ///
    /// 1. **The ENDPOINTS only.** Our data stores `rank0` and `rankMax` and
    ///    INTERPOLATES between them, while DE ships its own intermediate
    ///    values: Amalgam Serration reads 70.45 at rank 4 where DE prints 71,
    ///    and `metal_auger`'s ramp is non-linear outright. The middle is our
    ///    interpolation showing, not a disagreement about the mod.
    /// 2. **DE's client rounds AND truncates for display** — "+18% 多重射击"
    ///    for 18.25, "254%" for 254.6, "16.6%" for 16.67. A localized number
    ///    matches if it is ours rounded *or* truncated at its own precision.
    /// 3. **SUBSET, not equality** — a translation may drop a literal:
    ///    "(x2 for Bows)" is "（弓类武器效果加倍）". The direction that matters
    ///    is a number DE prints that we cannot produce.
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
        /// Cards where OUR English text deliberately omits a clause DE's card
        /// carries, so its numbers are absent rather than wrong.
        ///
        /// A NARROW list, keyed by id: a number we state must be a number DE
        /// states, so "absent" is the only tolerated difference and has to be a
        /// decision someone wrote down.
        ///
        /// `guided_ordnance`: "On Hit: +30% Accuracy when Aiming for 9s".
        /// Accuracy is an indirect stat with no uptime, so `fill_x` has nothing
        /// to put there and hardcoding "9s" would be wrong at every rank below
        /// max (it ramps 2 → 9). The clause is off our card.
        ///
        /// `double_tap`: "Stacks up to 80x outside of Conclave." The ladder is
        /// 80 / 40 / 26 / 20 and the schema stores two endpoints, so no
        /// interpolation reaches it — it is not linear because the cap is not
        /// what DE ranks. The CEILING is, at a flat +400%, and the cap is that
        /// divided by a per-stack value that does rank linearly (5/10/15/20).
        /// Two endpoints would print 60x and 40x at ranks 1 and 2 against the
        /// real 40x and 26x, so the clause is off our card rather than wrong on
        /// it. The number the sim runs is on the effect line either way.
        const OMITS_A_CLAUSE: [&str; 2] = ["guided_ordnance", "double_tap"];

        let mut bad: Vec<String> = Vec::new();
        for (code, spec) in locales() {
            for (id, ranks) in &spec.mod_descriptions {
                if OMITS_A_CLAUSE.contains(&id.as_str()) {
                    continue;
                }
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
                            // NUDGE BEFORE ROUNDING. `0.35 / 0.1` is
                            // 3.4999999999999996 in binary floating point, so
                            // `round()` gives 3 and a card DE prints as "0.4"
                            // stops matching a value we hold as 0.35 — which
                            // is exactly what this reported, against data that
                            // was right (Seeking Force). The whole
                            // job here is to decide what a number LOOKS like
                            // at a given precision, so the comparison has to
                            // be done at that precision too.
                            let scaled = o.abs() / step;
                            let eps = 1e-9;
                            // rounded, or truncated — DE's client does both.
                            ((scaled + eps).round() * step - z.abs()).abs() <= 1e-6
                                || ((scaled + eps).floor() * step - z.abs()).abs() <= 1e-6
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
    /// EVERY TABLE ON THE SPEC IS MERGED. Adding one is two edits — the field
    /// and the fold — and the second is easy to forget: `abilities` was
    /// declared, filled with DE's own names, served by the api, read by the
    /// page, and dropped on the floor by `merge`, so the section rendered in
    /// English on a Chinese page and nothing anywhere said why.
    ///
    /// This asserts it the only way that survives the next table: every
    /// non-empty map in the file is non-empty after the merge that produced
    /// the locale.
    #[test]
    fn a_declared_table_is_actually_folded_in() {
        let zh = locales()
            .iter()
            .find(|(c, _)| c == "zh")
            .map(|(_, s)| s)
            .expect("zh locale");
        // DERIVED FROM THE STRUCT, not listed. This test caught a table
        // declared and never folded in — and then failed to catch the next two
        // (`auras`, `shards`), because a hand list cannot report what is not on
        // it. Serializing the merged spec asks the question of every field
        // there is, including one added tomorrow by somebody who never read
        // this file.
        let v = serde_norway::to_value(zh).expect("the locale serializes");
        let serde_norway::Value::Mapping(m) = v else { panic!("a locale is a mapping") };
        let mut checked = 0;
        for (k, val) in &m {
            let name = k.as_str().unwrap_or("?");
            // `effect_phrases` is a LIST and is allowed to be empty in a locale
            // that needs no fallback substitutions; every keyed TABLE is not.
            let n = match val {
                serde_norway::Value::Mapping(x) => x.len(),
                _ => continue,
            };
            checked += 1;
            assert!(n > 0, "the zh locale's `{name}` table came out empty");
        }
        assert!(checked >= 10, "every table was asked about: {checked}");
    }

