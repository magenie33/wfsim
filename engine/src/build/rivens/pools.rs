use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct PoolFile {
    #[allow(dead_code)]
    pub(super) class: String,
    pub(super) stats: Vec<RivenStat>,
}

/// Riven stats THIS WEAPON cannot roll, out of its class pool.
///
/// The pool is per CLASS, but two rifles do not roll the same stats. What
/// generates one is docs/DATA_SOURCES.md §"Riven pools", and THE DERIVATION IS
/// THE LAST OF THREE SOURCES — `data/rivens/exceptions.yaml` overrides per
/// riven FAMILY, and these rules fill in for a family nobody has a card from.
/// `data/rivens/pools.yaml` is neither: a count over live listings, read by
/// `the_survey_still_agrees_with_the_rules` and by nothing in the calculation.
///
/// 1. **Physical damage** — *"Weapons without more than 25% of a physical
///    damage type usually cannot roll that respective attribute … Exceptions
///    exist on a case by case basis."* That clause is why this is a derivation
///    and not a law.
/// 2. **A stat the weapon does not have** is inert whatever DE rolls, and the
///    weapon's own wiki table is the evidence — Verglas Prime has no Zoom row,
///    no Recoil row, "Ammo Max: ∞" and "Projectile Type: Hit-Scan".
///
/// Both rules read the weapon as ONE THING WITH FORMS, over the union of the
/// forms you can fire for free: Larkspur Prime's beam is 11% Impact and its
/// alt-fire 33%, and a real card rolls negative Impact. A GAUGE-SWITCHED form
/// stays out.
pub fn derived_for(weapon_id: &str) -> Vec<&'static str> {
    let Some(s) = crate::data::weapons::spec(weapon_id) else { return Vec::new() };
    // A RIVEN BELONGS TO A FAMILY, so the pool is the family's. One card equips on every member — a Ballistica riven is a
    // Ballistica Prime riven and a Rakta Ballistica riven — so a pool derived
    // from ONE member describes a card that does not exist. Fifteen families
    // disagreed with themselves before this: a real card carrying negative
    // Slash rolls legally on the Ballistica Prime (18% Slash on its charged
    // shot) and was refused as "not a legal riven" on the other two.
    //
    // The members, then each member's own free FORMS, and the shot rules read
    // the union of all of it — which is the same argument the alt-fire rule
    // already makes, one level up.
    let family: Vec<&'static crate::data::weapons::WeaponSpec> = match &s.riven_family {
        Some(f) => crate::data::weapons::all()
            .iter()
            .filter(|w| w.riven_family.as_deref() == Some(f.as_str()))
            .collect(),
        // A weapon with no family answers for itself, which is what it means.
        None => vec![s],
    };
    // `s` is the entry the caller named — what the rules that read the WEAPON
    // (its ammo pool, its class) go to. `forms` is what the rules that read a
    // SHOT go to, and there can be more than one of those, on more than one
    // member.
    let forms: Vec<_> = family
        .iter()
        .flat_map(|w| crate::data::weapons::forms_of(&w.id))
        .filter(|f| !f.kind.is_adapter_form())
        .filter_map(|f| crate::data::weapons::spec(f.weapon_id))
        .collect();
    let mut out: Vec<&'static str> = Vec::new();

    for (stat, key) in [("impact", "impact"), ("puncture", "puncture"), ("slash", "slash")] {
        let best = forms
            .iter()
            .map(|f| {
                let total: f64 = f.attack.damage.values().sum();
                if total > 0.0 {
                    f.attack.damage.get(key).copied().unwrap_or(0.0) / total
                } else {
                    0.0
                }
            })
            .fold(0.0_f64, f64::max);
        if best <= 0.25 {
            out.push(stat);
        }
    }
    // The player never aims a sentinel weapon, so it has neither stat. Same
    // `class.contains("sentinel")` test the exilus and arcane rules use.
    //
    // ASKED OF EVERY MEMBER, like the shot rules above: a stat is inert only if
    // it is inert on the whole family, because one card covers the whole family.
    if family.iter().all(|w| w.class.contains("sentinel")) {
        out.push("zoom");
        out.push("weapon_recoil");
    }
    // No ammo pool at all — a percentage of infinity is not a stat.
    if family.iter().all(|w| w.ammo_max.is_none()) {
        out.push("ammo_maximum");
    }
    // NOTHING FOR FLIGHT SPEED TO ACT ON — and there are TWO ways to give it
    // something. Wiki (`Projectile Speed`), verbatim: *"Mods including Rivens
    // that have positive or negative Projectile speeds will affect a weapon's
    // entire Damage Falloff range accordingly"*, and *"Hitscan weapons that do
    // **not** list Damage Falloff values in their UI are completely unaffected
    // by Projectile Speed modifications"*.
    //
    // So a falloff counts even with nothing in the air, which is why a shotgun
    // rolls the stat: the Boar keeps 50% past 25 m and the riven moves that
    // whole range. Reading only `shot_type` said no to every shotgun in the
    // roster.
    let flies = |f: &&'static crate::data::weapons::WeaponSpec| {
        f.attack.shot_type.is_some_and(|t| t.flies()) || f.attack.falloff.is_some()
    };
    if !forms.iter().any(flies) {
        out.push("projectile_speed");
    }

    out
}

/// What this weapon's rivens can NOT roll: the derivation, with the per-family
/// evidence applied over it (`exceptions.yaml` and `physical.yaml`).
///
/// The SURVEY's raw count is not in this path: a scrape that outranks the rules
/// rewrites every pool when it breaks, so evidence reaches here only as entries
/// that carry their count, and `the_survey_still_agrees_with_the_rules` checks
/// the rest.
///
/// The evidence speaks per riven FAMILY, because that is the unit DE rolls: one
/// Boar riven fits the Boar and the Boar Prime, so one entry covers both.
///
/// A PHYSICAL STAT IS REFUSED ONLY BY EVIDENCE. The 25% share rule is wrong in
/// both directions (the Ocucor is 0% Slash and rolls it, the Phenmor is 30%
/// Puncture and does not), so it never refuses a card on its own: what it
/// would have refused and nobody has looked at is [`unconfirmed_for`].
pub fn excluded_for(weapon_id: &str) -> Vec<&'static str> {
    let Some(s) = crate::data::weapons::spec(weapon_id) else { return Vec::new() };
    let mut out = derived_for(weapon_id);
    out.retain(|id| !PHYSICAL.contains(id));
    if let Some(fam) = s.riven_family.as_deref() {
        let ex = exceptions(fam);
        out.retain(|id| !ex.rolls.contains(id));
        for id in &ex.never {
            if !out.contains(id) {
                out.push(*id);
            }
        }
    }
    // NOT NARROWED TO THE CLASS POOL, and the Boar is why: its exception says
    // `never: [zoom]` and the shotgun pool has no Zoom row, so a stat named
    // here need not be one the class rolls. Both the survey and the exceptions
    // speak in the market's stat vocabulary rather than in one class's, and
    // every caller intersects with the pool anyway.
    out
}

/// The three physical stats — the ones whose pool no rule predicts.
pub const PHYSICAL: [&str; 3] = ["impact", "puncture", "slash"];

/// Physical stats this weapon's riven is OFFERED without evidence either way:
/// the share rule would refuse them, and no card or count for the family says
/// whether they roll. Legal until a survey says otherwise, and shown as
/// unconfirmed so a visitor knows the answer is not ours to give yet.
pub fn unconfirmed_for(weapon_id: &str) -> Vec<&'static str> {
    let Some(s) = crate::data::weapons::spec(weapon_id) else { return Vec::new() };
    let ex = s.riven_family.as_deref().map(exceptions);
    derived_for(weapon_id)
        .into_iter()
        .filter(|id| PHYSICAL.contains(id))
        .filter(|id| ex.is_none_or(|e| !e.rolls.contains(id) && !e.never.contains(id)))
        .collect()
}

/// DE's OWN riven family names — `data/rivens/de_families.yaml`, written by
/// `scripts/survey_riven_families.py` from the weekly trade dump's
/// `compatibility` field.
///
/// It is ONE WEEK of trades, so it CONFIRMS a name and can never refute one: a
/// family absent from it is a family nobody traded that week.
pub fn de_families() -> &'static [String] {
    use std::sync::OnceLock;
    static F: OnceLock<Vec<String>> = OnceLock::new();
    F.get_or_init(|| {
        #[derive(Deserialize)]
        struct File {
            families: Vec<String>,
        }
        let (p, text) = crate::data::files_under("rivens/")
            .find(|(p, _)| p.ends_with("de_families.yaml"))
            .expect("data/rivens/de_families.yaml");
        let f: File = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
        f.families
    })
}

/// One riven family's surveyed pool — `data/rivens/pools.yaml`.
pub struct SurveyedPool {
    pub family: String,
    /// How many listings the count is over.
    pub n: u32,
    pub rollable: Vec<&'static str>,
    pub never: Vec<&'static str>,
}

#[derive(Deserialize)]
pub(super) struct PoolsFile {
    #[allow(dead_code)]
    pub(super) surveyed: String,
    pub(super) families: Vec<RawSurvey>,
}

#[derive(Deserialize)]
pub(super) struct RawSurvey {
    pub(super) family: String,
    pub(super) n: u32,
    #[serde(default)]
    pub(super) rollable: Vec<String>,
    #[serde(default)]
    pub(super) never: Vec<String>,
}

/// One family's hand-written exceptions to the derivation.
#[derive(Default)]
pub struct Exceptions {
    /// The rules refuse it and it is real.
    pub rolls: Vec<&'static str>,
    /// The rules allow it and it is not.
    pub never: Vec<&'static str>,
}

#[derive(Deserialize)]
pub(super) struct ExceptionsFile {
    pub(super) families: Vec<RawExceptions>,
}

#[derive(Deserialize)]
pub(super) struct RawExceptions {
    pub(super) family: String,
    #[serde(default)]
    pub(super) rolls: Vec<ExceptionStat>,
    #[serde(default)]
    pub(super) never: Vec<ExceptionStat>,
}

#[derive(Deserialize)]
pub(super) struct ExceptionStat {
    pub(super) stat: String,
    /// What was looked at. REQUIRED — the note IS the evidence, and serde
    /// refuses the entry without one.
    #[allow(dead_code)]
    pub(super) note: String,
}

pub(super) fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Every surveyed family, loaded once.
pub fn surveys() -> &'static [SurveyedPool] {
    static S: OnceLock<Vec<SurveyedPool>> = OnceLock::new();
    S.get_or_init(|| {
        crate::data::files_under("rivens/")
            .filter(|(p, _)| *p == "rivens/pools.yaml")
            .filter_map(|(_, t)| serde_norway::from_str::<PoolsFile>(t).ok())
            .flat_map(|f| f.families)
            .map(|r| SurveyedPool {
                family: r.family,
                n: r.n,
                rollable: r.rollable.into_iter().map(leak).collect(),
                never: r.never.into_iter().map(leak).collect(),
            })
            .collect()
    })
}

/// One family's surveyed pool. VERIFICATION ONLY — nothing in the calculation
/// reads this; `the_survey_still_agrees_with_the_rules` does.
pub fn survey(family: &str) -> Option<&'static SurveyedPool> {
    surveys().iter().find(|s| s.family == family)
}

/// The two evidence files, each parsed on its own: `exceptions.yaml` (hand-
/// written) and `physical.yaml` (the survey's physical verdicts).
pub(super) fn exception_files() -> Vec<(&'static str, Vec<RawExceptions>)> {
    crate::data::files_under("rivens/")
        .filter(|(p, _)| *p == "rivens/exceptions.yaml" || *p == "rivens/physical.yaml")
        .map(|(p, t)| {
            let f = serde_norway::from_str::<ExceptionsFile>(t).unwrap_or_else(|e| panic!("{p}: {e}"));
            (p, f.families)
        })
        .collect()
}

/// One family's exceptions — `data/rivens/exceptions.yaml` and `physical.yaml`, merged.
pub fn exceptions(family: &str) -> &'static Exceptions {
    static S: OnceLock<std::collections::BTreeMap<String, Exceptions>> = OnceLock::new();
    static EMPTY: OnceLock<Exceptions> = OnceLock::new();
    let all = S.get_or_init(|| {
        let mut m: std::collections::BTreeMap<String, Exceptions> = Default::default();
        for (_, fams) in exception_files() {
            for r in fams {
                let e = m.entry(r.family).or_default();
                e.rolls.extend(r.rolls.into_iter().map(|s| leak(s.stat)));
                e.never.extend(r.never.into_iter().map(|s| leak(s.stat)));
            }
        }
        m
    });
    all.get(family).unwrap_or_else(|| EMPTY.get_or_init(Exceptions::default))
}

/// The stat pool of one mod class — `data/rivens/<class>.yaml`.
pub fn pool(class: &str) -> &'static [RivenStat] {
    static POOLS: OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, &'static [RivenStat]>>> =
        OnceLock::new();
    let cache = POOLS.get_or_init(|| std::sync::Mutex::new(std::collections::BTreeMap::new()));
    let mut g = cache.lock().expect("riven pool cache");
    g.entry(class.to_string())
        .or_insert_with(|| {
            let loaded = crate::data::files_under("rivens/")
                .filter_map(|(p, text)| {
                    let want = format!("rivens/{class}.yaml");
                    (p == want).then(|| serde_norway::from_str::<PoolFile>(text).ok())?
                })
                .next()
                .map(|f| f.stats)
                .unwrap_or_default();
            &*Box::leak(loaded.into_boxed_slice())
        })
}
