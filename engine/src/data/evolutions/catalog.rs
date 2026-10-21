use super::*;

/// A parsed Incarnon evolution.
#[derive(Debug, Clone)]
pub struct EvolutionDef {
    pub id: String,
    pub name: String,
    pub weapon: String,
    pub tier: u32,
    /// 1-based, the wiki's order within the tier; 1 is the tier's default.
    pub position_in_tier: u32,
    /// Wiki `File:` name for the evolution's icon.
    pub icon: Option<String>,
    /// Verbatim effect text — what the cards display (like mods/arcanes).
    pub description: crate::prose::Prose,
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
    pub(super) misprints: Vec<crate::prose::Prose>,
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

    /// Σ flat BASE multishot (Rain of Lead).
    pub fn flat_base_multishot(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseMultishot(v) => Some(*v),
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
        let mut out: Vec<EvolutionDef> =
            crate::data::files_under("evolutions/").map(|(path, text)| parse(path, text)).collect();
        in_game_order(&mut out);
        out
    })
}

/// ONE WEAPON'S LADDER, parsed from its own files only. A compute lane fights a
/// few weapons, and the whole catalog parsed for each is memory it holds for
/// life (docs/WASM.md §"Memory"); the files are named `<weapon>_<evolution>`,
/// and `weapon:` settles a prefix two weapons share (`bo_` and `bo_prime_`).
pub fn for_weapon(weapon: &str) -> &'static [EvolutionDef] {
    static BY_WEAPON: OnceLock<std::sync::Mutex<std::collections::HashMap<String, &'static [EvolutionDef]>>> = OnceLock::new();
    let mut by = BY_WEAPON.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(l) = by.get(weapon) {
        return l;
    }
    let prefix = format!("evolutions/{weapon}_");
    let mut out: Vec<EvolutionDef> = crate::data::files_under("evolutions/")
        .filter(|(path, _)| path.starts_with(&prefix))
        .map(|(path, text)| parse(path, text))
        .filter(|e| e.weapon == weapon)
        .collect();
    in_game_order(&mut out);
    let l: &'static [EvolutionDef] = Box::leak(out.into_boxed_slice());
    by.insert(weapon.to_string(), l);
    l
}

/// One evolution file, its naming contract checked.
fn parse(path: &str, text: &str) -> EvolutionDef {
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
    let misprints: Vec<crate::prose::Prose> = ef
        .effects
        .iter()
        .filter_map(|v| {
            let note = crate::prose::Prose::read(v, "misprint")?;
            let kind = v.get("kind").and_then(Value::as_str).unwrap_or("");
            Some(note.map(|n| format!("{} — {n}", kind.replace('_', " "))))
        })
        .collect();
    EvolutionDef {
        id: ef.id,
        name: ef.name,
        weapon: ef.weapon,
        tier: ef.tier,
        position_in_tier: ef.position_in_tier,
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
    }
}

/// The game's order, so `options(..)[0]` is the default; each tier's positions
/// run 1..=n.
fn in_game_order(out: &mut [EvolutionDef]) {
    out.sort_by(|a, b| (&a.weapon, a.tier, a.position_in_tier).cmp(&(&b.weapon, b.tier, b.position_in_tier)));
    for w in out.chunk_by(|a, b| a.weapon == b.weapon && a.tier == b.tier) {
        let positions: Vec<u32> = w.iter().map(|e| e.position_in_tier).collect();
        assert!(
            positions.iter().copied().eq(1..=w.len() as u32),
            "{} tier {}: position_in_tier must run 1..={}, got {positions:?}",
            w[0].weapon,
            w[0].tier,
            w.len()
        );
    }
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
    // THE FILE IS THE ID, and it names its weapon.
    let text = crate::data::file(&format!("evolutions/{id}.yaml"))?;
    let weapon = serde_norway::from_str::<EvoFile>(text).ok()?.weapon;
    for_weapon(&weapon).iter().find(|e| e.id == id)
}

/// A weapon's choosable options at a tier (the web picker's rows), in the
/// game's order: the first is the tier's default.
pub fn options(weapon: &str, tier: u32) -> Vec<&'static EvolutionDef> {
    for_weapon(weapon).iter().filter(|e| e.tier == tier).collect()
}

/// How many evolution tiers this weapon's data declares — the tier count
/// is per weapon (Dual Toxocyst has 4, Laetum has 5), so callers must
/// never assume a fixed range.
pub fn tier_count(weapon: &str) -> u32 {
    for_weapon(weapon).iter().map(|e| e.tier).max().unwrap_or(0)
}

/// EVERY TIER INSTALLED: `ids`' pick where it names one, the tier's first
/// option where it does not, in tier order. A finished Genesis cannot be
/// emptied, so a tier left out of a build is a tier holding its default — not
/// a weapon nobody can hold. `ids` of another weapon, or a second pick at one
/// tier, are dropped.
pub fn complete<S: AsRef<str>>(weapon: &str, ids: &[S]) -> Vec<String> {
    (1..=tier_count(weapon))
        .filter_map(|tier| {
            let opts = options(weapon, tier);
            ids.iter()
                .find_map(|id| opts.iter().find(|o| o.id == id.as_ref()))
                .or(opts.first())
                .map(|o| o.id.clone())
        })
        .collect()
}
