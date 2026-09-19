use super::*;

/// A parsed Incarnon evolution.
#[derive(Debug, Clone)]
pub struct EvolutionDef {
    pub id: String,
    pub name: String,
    pub weapon: String,
    pub tier: u32,
    /// Wiki `File:` name for the evolution's icon.
    pub icon: Option<String>,
    /// Verbatim effect text — what the cards display (like mods/arcanes).
    pub description: String,
    pub currently_broken: bool,
    /// What this perk DECLARES about feeding the weapon's GunCO term, or
    /// `None` for "nobody has said" — which is the common case and means the
    /// term IS fed. See [`Self::excludes_co_base`].
    pub co_base_excludes_this_evolution: Option<bool>,
    /// The FORM a declaration was measured on, when it covers only one — see
    /// the loader field of the same name.
    pub co_base_excludes_only_form: Option<crate::model::FormKind>,
    /// Everything this evolution grants applies to the BASE form only — see
    /// the loader field of the same name.
    pub base_form_only: bool,
    /// WHERE THE CARD AND THE GAME DISAGREE — one line per clause, each saying
    /// what the card prints and what the effect actually does.
    ///
    /// THE FOURTH KIND OF ADMISSION, and the second that is not a shortfall of
    /// ours. [`EvoEffect::Inert`] is work someone can do,
    /// [`EvoEffect::OutOfScope`] is the edge of what this simulator is, and
    /// [`EvoEffect::LiveBug`] says the model is right and the game is broken.
    /// This one says the model is right and the CARD is wrong: the effect works,
    /// it simply does not do what it says.
    ///
    /// It is NOT a live bug and must not be reported as one. A live bug tells a
    /// reader not to pick the perk; this tells them the perk is better or worse
    /// than its own text, which is the opposite advice. Swift Punishment prints
    /// "With Sprint Speed 1.2 or Higher" and its own wiki row says *"Despite
    /// the description, the effect only requires 1.1"* — a player reading the
    /// card would mod for a threshold the game does not ask for.
    ///
    /// Owner: anything that differs from what the game DISPLAYS is
    /// to be noted. It rides beside the effect rather than short-circuiting it,
    /// which is the whole difference from `live_bug` in the same position.
    pub(super) misprints: Vec<String>,
    pub(super) effects: Vec<EvoEffect>,
}

impl EvolutionDef {
    /// Σ flat base damage this evolution adds (0 when broken) — the panel
    /// attributes it as a non-mod source on the Base Damage row.
    pub fn flat_base_damage(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseDamage(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ flat BASE crit chance (Commodore's Fortune; 0 when broken).
    pub fn flat_base_crit_chance(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseCritChance(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ flat BASE status chance (Survivor's Edge, Elemental Balance).
    pub fn flat_base_status_chance(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseStatusChance(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ flat BASE magazine rounds (Extended Volley).
    /// Σ flat BASE magazine this perk grants THIS player — the gated spelling of
    /// [`Self::flat_base_magazine`] below (Lone Gun's "+14 Base Magazine
    /// Capacity", which the card owes only when nothing else is carried).
    ///
    /// It cannot be folded where the ungated one is: `apply` never sees a
    /// Tenno, so the resolver answers the gate against `WeaponBase::gated`. The
    /// panel needs the same number attributed to the perk that grants it —
    /// a magazine that grew with no source listed is the panel telling half a
    /// story — and `base.gated` has no owner to name, so it asks here.
    pub fn gated_flat_magazine(&self, tenno: &crate::data::tenno::Tenno) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::GatedByTenno { gate, grant, value }
                    if *grant == crate::model::GatedGrant::FlatBaseMagazine
                        && gate.holds(tenno) =>
                {
                    Some(*value)
                }
                _ => None,
            })
            .sum()
    }

    pub fn flat_base_magazine(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseMagazine(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ assumed-max multishot from permanent stacks (Fevered Frenzy).
    pub fn assumed_multishot(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::AssumedMaxMultishot { total, .. } => Some(*total),
                _ => None,
            })
            .sum()
    }

    /// The permanent stacked-multishot buff, if this evolution grants one:
    /// (full-stack bonus, max stacks). Drives the configurable buff card.
    pub fn ms_buff(&self) -> Option<(f64, u32)> {
        self.active_effects().find_map(|e| match e {
            EvoEffect::AssumedMaxMultishot { total, max_stacks } => Some((*total, *max_stacks)),
            _ => None,
        })
    }
}

/// Every embedded yaml under data/evolutions (cached).
pub fn pool() -> &'static Vec<EvolutionDef> {
    static POOL: OnceLock<Vec<EvolutionDef>> = OnceLock::new();
    POOL.get_or_init(|| {
        let mut out = Vec::new();
        for (path, text) in crate::data::files_under("evolutions/") {
            // The directory IS the table (data/README.md conventions):
            // everything under evolutions/ must parse as an evolution.
            let ef = serde_norway::from_str::<EvoFile>(text)
                .unwrap_or_else(|e| panic!("parse {path}: {e}"));
            // NAMING CONTRACT, enforced at load (full
            // weapon names, no abbreviations — long but unambiguous):
            //   id = "<weapon>_<evolution>"  and  filename = "<id>.yaml".
            // Scoping is NOT redundant with the `weapon:` field: evolution
            // NAMES repeat across weapons with different values (Marksman's
            // Hand is −50% recoil on Dual Toxocyst, −40% on Laetum), so the
            // id must carry the weapon. Deriving both the file name and the
            // prefix from it means the three can never drift apart.
            let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".yaml");
            assert!(
                ef.id == stem,
                "{path}: id '{}' must match the filename",
                ef.id
            );
            assert!(
                ef.id.strip_prefix(&ef.weapon).is_some_and(|r| r.starts_with('_')),
                "{path}: id '{}' must start with the weapon id '{}_'",
                ef.id,
                ef.weapon
            );
            let effects = ef.effects.iter().filter_map(effect).collect();
            // …AND THE CLAUSES WHOSE CARD IS WRONG. Read off the same effect
            // maps and kept beside them: `misprint:` names the disagreement and
            // changes nothing about how the effect is loaded.
            let misprints: Vec<String> = ef
                .effects
                .iter()
                .filter_map(|v| {
                    let note = v.get("misprint").and_then(Value::as_str)?;
                    let kind = v.get("kind").and_then(Value::as_str).unwrap_or("");
                    Some(format!("{} — {note}", kind.replace('_', " ")))
                })
                .collect();
            out.push(EvolutionDef {
                id: ef.id,
                name: ef.name,
                weapon: ef.weapon,
                tier: ef.tier,
                icon: ef.icon,
                description: ef.description.unwrap_or_default(),
                currently_broken: ef.currently_broken,
                co_base_excludes_this_evolution: ef.co_base_excludes_this_evolution,
                co_base_excludes_only_form: ef.co_base_excludes_only_form.as_deref().map(|s| {
                    match s {
                        "base" => crate::model::FormKind::Base,
                        "incarnon" => crate::model::FormKind::Incarnon,
                        other => panic!("co_base_excludes_only_form: unknown form {other:?}"),
                    }
                }),
                base_form_only: ef.base_form_only,
                misprints,
                effects,
            });
        }
        out
    })
}

/// Look up an evolution by id.
impl EvolutionDef {
    /// The form this evolution unlocks, if it is the transformation itself.
    ///
    /// THE TAG the form resolution reads. It replaces "tier 1's first option",
    /// which was a guess from ladder position that happened to hold for the
    /// four Incarnon weapons in the roster and says nothing about the fifth.
    pub fn unlocks_form(&self) -> Option<&str> {
        self.effects.iter().find_map(|e| match e {
            EvoEffect::UnlocksForm(w) => Some(w.as_str()),
            _ => None,
        })
    }
}

/// Does this evolution state a MELEE INCARNON WINDOW?
///
/// Asked by whoever has to build the UN-ARMED half of the fight: the same
/// weapon, resolved again without the tiers that turn the Incarnon on. It is a
/// predicate over the DATA rather than a list of ids, so the next Genesis is
/// covered by writing its yaml and nothing else.
pub fn states_incarnon_window(id: &str) -> bool {
    get(id).is_some_and(|e| {
        e.effects.iter().any(|x| matches!(x, EvoEffect::IncarnonWindow { .. }))
    })
}

pub fn get(id: &str) -> Option<&'static EvolutionDef> {
    pool().iter().find(|e| e.id == id)
}

/// A weapon's choosable options at a tier (the web picker's rows).
pub fn options(weapon: &str, tier: u32) -> Vec<&'static EvolutionDef> {
    pool()
        .iter()
        .filter(|e| e.weapon == weapon && e.tier == tier)
        .collect()
}

/// How many evolution tiers this weapon's data declares — the tier count
/// is per weapon (Dual Toxocyst has 4, Laetum has 5), so callers must
/// never assume a fixed range.
pub fn tier_count(weapon: &str) -> u32 {
    pool()
        .iter()
        .filter(|e| e.weapon == weapon)
        .map(|e| e.tier)
        .max()
        .unwrap_or(0)
}
