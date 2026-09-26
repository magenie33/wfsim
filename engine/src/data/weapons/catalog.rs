use super::*;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ReasonDef {
    pub(super) text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ReasonFile {
    pub(super) reasons: BTreeMap<String, ReasonDef>,
}

/// The reason table — `data/unmodelled/reasons.yaml`, parsed once.
pub(super) fn reasons() -> &'static BTreeMap<String, String> {
    static R: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    R.get_or_init(|| {
        let mut out = BTreeMap::new();
        for (p, text) in crate::data::files_under("unmodelled/") {
            if !p.ends_with(".yaml") {
                continue;
            }
            let f = serde_norway::from_str::<ReasonFile>(text)
                .unwrap_or_else(|e| panic!("parse {p}: {e}"));
            for (k, v) in f.reasons {
                out.insert(k, v.text);
            }
        }
        out
    })
}

/// Substitute `{named}` holes. Anything the params do not name is LEFT ALONE
/// rather than blanked — a template with a hole nobody filled should read as
/// obviously broken on the page, not as a sentence with a gap in it.
pub fn fill_template(tpl: &str, params: &BTreeMap<String, String>) -> String {
    let mut out = tpl.to_string();
    for (k, v) in params {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// WHAT A FORM INHERITS FROM ITS WEAPON — the fields that describe the
/// WEAPON rather than the shot.
///
/// The line is drawn at "would the arsenal print this once for the gun, or
/// once per firing mode". Mastery, disposition, polarities and the riven
/// family are the gun's. Magazine and reload are the gun's TOO, even though a
/// form may override them: the Scourge's throw really does hold one round
/// against the primary fire's forty, and stating that override is exactly what
/// this mechanism makes visible.
///
/// NOT HERE, deliberately:
///   - everything under `attack:` — a form IS its attack;
///   - `co_behavior`, which the Condition Overload catalog gives per ATTACK
///     (the Mandonel's uncharged shot is Multiplying and its charged one
///     Adding, from two different rows);
///   - `form`, `default_form`, `transform_group`, `transforms_to/from`,
///     `incarnon`, `id`, `name` — the entry's own identity;
///   - `source`, because a form that shares a page still says so itself.
pub(super) const INHERITED: [&str; 27] = [
    "slot", "class", "mod_pools", "mastery_rank", "max_rank", "accuracy", "exalted", "fixed_stance",
    "summoned_by",
    "wielders", "wielder_names",
    "disposition", "polarities", "exilus_polarity", "stance_polarity", "riven_family",
    "internal_name", "noise", "magazine", "reload_seconds", "ammo_type",
    "ammo_max", "ammo_pickup", "traits", "deployment", "no_resupply",
    // THE COMBO COUNTER'S CLOCK IS THE WEAPON'S, not a way of swinging it —
    // five seconds on almost every melee. A form that does not inherit it reads
    // ZERO and is floored at 0.1 s, which kills the counter between every pair
    // of swings: six of the Magistar's seven modes had no counter at all.
    "combo_duration_seconds",
];

/// ...and `deployments` and `valence`, which are MAPS and would need a deep
/// merge to inherit partially. They are all-or-nothing: a form that states
/// neither takes its weapon's whole block.
pub(super) const INHERITED_BLOCKS: [&str; 4] = ["deployments", "valence", "sniper_combo", "scope"];

/// Every weapon entry in `data/weapons/` (embedded), parsed once.
pub fn all() -> &'static [WeaponSpec] {
    static SPECS: OnceLock<Vec<WeaponSpec>> = OnceLock::new();
    SPECS.get_or_init(|| {
        // TWO PASSES, and the merge happens on the YAML rather than on the
        // struct: `WeaponSpec`'s fields have defaults, so once it is
        // deserialized "absent" and "stated at the default" are the same
        // thing, and a form could not inherit a `false` or a `0`.
        use serde_norway::Value;
        let raw: Vec<(&str, Value)> = crate::data::files_under("weapons/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                (p, serde_norway::from_str::<Value>(text)
                    .unwrap_or_else(|e| panic!("parse {p}: {e}")))
            })
            .collect();
        let by_id: std::collections::HashMap<String, Value> = raw
            .iter()
            .filter_map(|(_, v)| {
                v.get("id").and_then(Value::as_str).map(|i| (i.to_string(), v.clone()))
            })
            .collect();
        raw.into_iter()
            .map(|(p, mut v)| {
                if let Some(parent) = v.get("inherits").and_then(Value::as_str) {
                    let up = by_id
                        .get(parent)
                        .unwrap_or_else(|| panic!("{p}: inherits unknown id `{parent}`"));
                    let m = v.as_mapping_mut().unwrap_or_else(|| panic!("{p}: not a mapping"));
                    for k in INHERITED.iter().chain(INHERITED_BLOCKS.iter()) {
                        let key = Value::String((*k).to_string());
                        if !m.contains_key(&key) {
                            if let Some(val) = up.get(*k) {
                                m.insert(key, val.clone());
                            }
                        }
                    }
                    // ADMISSIONS ARE NOT INHERITED, and that is deliberate.
                    // A form's `unmodeled:` is about THAT form — the Lanka's
                    // full draw says "the partial charge is a separate entry",
                    // which is nonsense printed on the partial charge. The
                    // shared lines are the class's rather than the weapon's
                    // anyway (every Arch-Gun repeats the Deployer cooldown),
                    // and de-duplicating THOSE is a different job: they want a
                    // reason id and a template, not a parent.
                }
                render_admissions(&mut v, p);
                serde_norway::from_value::<WeaponSpec>(v)
                    .unwrap_or_else(|e| panic!("parse {p}: {e}"))
            })
            .collect()
    })
}

/// Turn `unmodeled:` into finished English AND a structured list, in place.
///
/// An entry is either a STRING — prose, for a gap that happens once — or a
/// mapping naming a `reason:` from `data/unmodelled/reasons.yaml` with its
/// parameters. Both end as a sentence in `unmodeled`; only the second can be
/// re-rendered in another language, which is the whole point.
pub(super) fn render_admissions(v: &mut serde_norway::Value, path: &str) {
    use serde_norway::Value;
    let Some(m) = v.as_mapping_mut() else { return };
    let key = Value::String("unmodeled".to_string());
    let Some(list) = m.get(&key).and_then(Value::as_sequence).cloned() else { return };
    let mut text: Vec<Value> = Vec::with_capacity(list.len());
    let mut parts: Vec<Value> = Vec::with_capacity(list.len());
    for one in list {
        let mut part = serde_norway::Mapping::new();
        match &one {
            Value::String(s) => {
                text.push(Value::String(s.clone()));
                part.insert(Value::String("text".into()), Value::String(s.clone()));
            }
            Value::Mapping(mm) => {
                let rid = mm
                    .get(Value::String("reason".into()))
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| panic!("{path}: an admission mapping needs `reason:`"))
                    .to_string();
                let tpl = reasons().get(&rid).unwrap_or_else(|| {
                    panic!("{path}: unknown unmodelled reason `{rid}` — add it to data/unmodelled/reasons.yaml")
                });
                let params: BTreeMap<String, String> = mm
                    .iter()
                    .filter(|(k, _)| k.as_str() != Some("reason"))
                    .map(|(k, val)| {
                        let ks = k.as_str().unwrap_or_default().to_string();
                        let vs = match val {
                            Value::String(s) => s.clone(),
                            other => serde_norway::to_string(other)
                                .unwrap_or_default()
                                .trim()
                                .trim_start_matches("---")
                                .trim()
                                .to_string(),
                        };
                        (ks, vs)
                    })
                    .collect();
                let rendered = fill_template(tpl, &params);
                text.push(Value::String(rendered.clone()));
                part.insert(Value::String("text".into()), Value::String(rendered));
                part.insert(Value::String("reason".into()), Value::String(rid));
                part.insert(Value::String("template".into()), Value::String(tpl.clone()));
                let pm: serde_norway::Mapping = params
                    .into_iter()
                    .map(|(k, val)| (Value::String(k), Value::String(val)))
                    .collect();
                part.insert(Value::String("params".into()), Value::Mapping(pm));
            }
            other => panic!("{path}: an admission is a string or a mapping, got {other:?}"),
        }
        parts.push(Value::Mapping(part));
    }
    m.insert(key, Value::Sequence(text));
    m.insert(Value::String("unmodeled_parts".into()), Value::Sequence(parts));
}

impl WeaponSpec {
    /// Does entering this form cost a METER the fight has to fill?
    ///
    /// The one question `has_gauge_switched_form` and the sim's cycle both ask,
    /// and it is answered by what the entry DECLARES — never by its form kind.
    /// An Incarnon adapter is one way to get here; five kills with a Mausolon
    /// is another, and forty-five seconds with a Tome is a third.
    ///
    /// THE THIRD IS A DIFFERENT TYPE and answers the same question. A
    /// [`GaugeFormSpec`] is filled by things you DO — hits, kills — and a
    /// [`MeterSpec`] by time passing; what they share is that the form costs
    /// something, which is all anything upstream of here wants to know.
    pub fn has_gauge(&self) -> bool {
        self.gauge_form.is_some() || self.attack.meter.is_some()
    }
}

pub fn spec(id: &str) -> Option<&'static WeaponSpec> {
    all().iter().find(|s| s.id == id)
}

/// Every perk entry in `data/perks/` (embedded), parsed once.
pub fn perks() -> &'static [PerkSpec] {
    static PERKS: OnceLock<Vec<PerkSpec>> = OnceLock::new();
    PERKS.get_or_init(|| {
        crate::data::files_under("perks/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                let spec = serde_norway::from_str::<PerkSpec>(text)
                    .unwrap_or_else(|e| panic!("parse {p}: {e}"));
                // Convention (data/README.md): the id matches the filename.
                let stem = p.rsplit('/').next().unwrap_or(p).trim_end_matches(".yaml");
                assert!(spec.id == stem, "{p}: id '{}' != filename", spec.id);
                spec
            })
            .collect()
    })
}

/// Find an inline perk definition among the given weapon specs.
pub(super) fn inline_perk_in<'a>(
    id: &str,
    specs: impl Iterator<Item = &'a WeaponSpec>,
) -> Option<&'a PerkSpec> {
    specs
        .flat_map(|w| w.perks.iter())
        .find_map(|pr| match pr {
            PerkRef::Inline(p) if p.id == id => Some(p),
            _ => None,
        })
}

/// Perk lookup over the GLOBAL namespace: the `data/perks/` table first,
/// then every weapon's inline definitions. Defining a perk inline registers
/// it globally — any other entry may reference it by bare id (uniqueness is
/// enforced by the engine test suite, so a bare id is never ambiguous).
pub fn perk(id: &str) -> Option<&'static PerkSpec> {
    perks()
        .iter()
        .find(|p| p.id == id)
        .or_else(|| inline_perk_in(id, all().iter()))
}

/// Does this weapon carry a given perk? Weapon PASSIVES are per weapon —
/// Dual Toxocyst lists `frenzy`, the Laetum lists none — so anything that
/// applies a passive must ask, never assume. A transform group's second
/// form lists its own perks (Frenzy is active in both DT forms).
pub fn has_perk(weapon_id: &str, perk_id: &str) -> bool {
    spec(weapon_id).is_some_and(|s| s.perks.iter().any(|p| p.id() == perk_id))
}

/// Registry view: the SELECTABLE weapons — one row per weapon, which is the
/// entry that declares itself the DEFAULT form. Every other form (an Incarnon
/// form, a bow's tapped shot) is a form of that weapon, not its own row.
pub fn roster() -> impl Iterator<Item = &'static WeaponSpec> {
    all().iter().filter(|s| s.default_form)
}

impl WeaponSpec {
    pub fn form_kind(&self) -> FormKind {
        FormKind::parse(&self.form)
    }

    /// The name this form is SHOWN under: the game's own where it is stated.
    pub fn form_label(&self) -> &str {
        self.form_name.as_deref().unwrap_or_else(|| self.form_kind().label())
    }

    /// The transform group this entry belongs to — its own id when it is a
    /// group of one (Verglas Prime: one weapon, one form).
    pub fn group(&self) -> &str {
        self.transform_group.as_deref().unwrap_or(&self.id)
    }
}

/// The forms a weapon REGISTERS, default first.
///
/// A weapon's forms are the entries of its transform group: the two-weapons
/// model gives every form its own yaml entry, and this
/// is the view that reads them back as ONE weapon with several modes. A
/// weapon with a single entry has exactly one form — that is the common case,
/// and it is a real registration (`form: base`), not an absence.
pub fn forms_of(weapon_id: &str) -> Vec<FormRef> {
    let Some(spec) = spec(weapon_id) else { return Vec::new() };
    let group = spec.group();
    let mut out: Vec<FormRef> = all()
        .iter()
        .filter(|s| s.group() == group)
        .map(|s| FormRef {
            weapon_id: &s.id,
            kind: s.form_kind(),
            is_default: s.default_form,
        })
        .collect();
    // Default first; the rest keep the vocabulary's order so two weapons never
    // list the same forms differently.
    out.sort_by_key(|f| (!f.is_default, f.kind as u8));
    out
}

pub(crate) fn damage_type(name: &str) -> DamageType {
    match name {
        "impact" => DamageType::Impact,
        "puncture" => DamageType::Puncture,
        "slash" => DamageType::Slash,
        "heat" => DamageType::Heat,
        "cold" => DamageType::Cold,
        "electricity" => DamageType::Electricity,
        "toxin" => DamageType::Toxin,
        // Innate COMBINED elements (Laetum Incarnon's radial is 300
        // Radiation). They do not re-enter the elemental hierarchy — an
        // innate combined element stays as it is and mod elements combine
        // among themselves (wiki Damage/Elemental combination).
        "blast" => DamageType::Blast,
        "corrosive" => DamageType::Corrosive,
        "gas" => DamageType::Gas,
        "magnetic" => DamageType::Magnetic,
        "radiation" => DamageType::Radiation,
        "viral" => DamageType::Viral,
        "true" => DamageType::True,
        "void" => DamageType::Void,
        // TAU, the Sentient type — "neutral to all health types, meaning its
        // damage is neither increased or decreased against any target"
        // (wiki `Damage/Tau Damage`), which is the Void's rule and is why the
        // enum has carried the variant since before a weapon dealt it. The
        // Haalvu is the first player weapon that does. Its STATUS is Status
        // Chance Vulnerability (+10% received status a stack, ten stacks, 8 s)
        // and this engine has no debuff for it — the weapon's own card says so.
        "tau" => DamageType::Tau,
        other => panic!("unknown damage type in weapon data: {other}"),
    }
}

pub(super) fn rank_30() -> u32 {
    30
}

pub fn polarity(name: &str) -> Polarity {
    match name {
        "madurai" => Polarity::Madurai,
        "naramon" => Polarity::Naramon,
        "vazarin" => Polarity::Vazarin,
        "umbra" => Polarity::Umbra,
        // THE WHOLE ENUM, because a weapon may ship any of them and this
        // panicked on four it could not spell. The Haalvu is the one that found
        // it: its EXILUS slot is Universal — the module says so and the page
        // does not mention the slot at all — and this roster had copied the
        // weapon's own Madurai into it, so the exilus mod was charged full
        // drain unless it happened to be Madurai.
        "zenurik" => Polarity::Zenurik,
        "unairu" => Polarity::Unairu,
        "penjaga" => Polarity::Penjaga,
        // A SLOT polarity only — no mod carries it, and the enum spells it `Omni`
        // after the Forma that grants it. `rules::capacity::slot_drain` already
        // knew about it; only this parser did not.
        "universal" => Polarity::Omni,
        // …and the other slot-only one, which matches nothing at all.
        "aura" => Polarity::Aura,
        other => panic!("unknown polarity in weapon data: {other}"),
    }
}
