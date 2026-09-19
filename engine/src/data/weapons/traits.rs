use super::*;

/// Weapon behavior traits consumed by arcane/mod `requires` gates. Traits
/// describe the WEAPON (its base form's trigger family), so both forms of a
/// transform group report the base entry's trigger.
/// What a `requires:` gate on a mod or arcane is checked against.
///
/// TWO KINDS: the firing TRIGGER (`semi_auto`, `auto`) and the weapon CLASS
/// (`shotgun`, `bow`, `dual_pistols`, …). With the class missing,
/// `requires: dual_pistols` can never be satisfied and Akimbo Slip Shot is
/// silently inert — a unit test passing `&["dual_pistols"]` by hand proves the
/// gate works and never asks whether anything produces the trait.
///
/// The trigger comes from the BASE entry of a transform group; the class is
/// the weapon's own, shared by both halves by construction.
/// The `independent_procs:` ids this entry declares, as a static slice.
///
/// Leaked through a cache exactly like [`traits_for`], and for the same reason:
/// a `WeaponBase` is built per request and the ids come from a yaml the loader
/// owns for the life of the process. The set of legal ids is validated HERE, so
/// a typo fails at load with the whole roster's names rather than silently
/// applying nothing in a fight.
pub(super) fn independent_procs_for(s: &WeaponSpec) -> &'static [&'static str] {
    static CACHE: OnceLock<Mutex<BTreeMap<String, &'static [&'static str]>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut g = cache.lock().expect("independent proc cache");
    if let Some(t) = g.get(&s.id) {
        return t;
    }
    let out: Vec<&'static str> = s
        .independent_procs
        .iter()
        .map(|p| match p.as_str() {
            "lifted" => "lifted",
            "knockdown" => "knockdown",
            other => panic!(
                "{}: unknown independent proc `{other}` — the engine implements `lifted` and `knockdown`;                  add the effect to fight::DebuffState before declaring it",
                s.id
            ),
        })
        .collect();
    let leaked: &'static [&'static str] = Box::leak(out.into_boxed_slice());
    g.insert(s.id.clone(), leaked);
    leaked
}

/// The traits a weapon has, for an EQUIP rule — [`traits_for`], public.
pub fn traits_of(s: &WeaponSpec) -> &'static [&'static str] {
    traits_for(s)
}

pub(super) fn traits_for(s: &WeaponSpec) -> &'static [&'static str] {
    static CACHE: OnceLock<Mutex<BTreeMap<String, &'static [&'static str]>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut g = cache.lock().expect("weapon traits cache");
    if let Some(t) = g.get(&s.id) {
        return t;
    }
    // THE TRIGGER IS THE WEAPON'S, WHICH IS THE GROUP'S DEFAULT FORM — the same
    // entry `data::mods::triggers_of` reads, and for the same reason: a mod's
    // `requires:` is an EQUIP rule, decided once for the weapon, and a mod the
    // weapon may legally wear has to keep working on every form of it.
    //
    // NOT `transforms_from`, which reaches only GAUGE-FED forms (an entry may
    // not carry that field without a gauge): under it a free alternate fire
    // reports its OWN trigger. The Tenet Detron is the case — its primary fire
    // is Semi-Auto and its Mag Burst is not — so Semi-Pistol Cannonade would be
    // offered on the weapon, go inert on the alternate form, AND TAKE ITS
    // FIRE-RATE LOCK WITH IT. A mod that locks a stat on one form and not the
    // other is the worst of the three possible answers.
    //
    // The narrow blast radius is what makes this safe: three mods in the whole
    // data set gate on a trigger (the Cannonades, `semi_auto`), and the only
    // entries whose answer moves are ones whose DEFAULT form is semi-auto and
    // whose alternate is not — the Tenet Detron and the Tenet Plinx. Everywhere
    // else `pool_for_build` had already refused the mod, so nothing could reach
    // this gate to change.
    let group = s.transform_group.as_deref().unwrap_or(&s.id);
    let base = all()
        .iter()
        .find(|x| x.transform_group.as_deref().unwrap_or(&x.id) == group && x.default_form)
        .unwrap_or(s);
    let mut out: Vec<&'static str> = Vec::new();
    match base.attack.trigger.as_str() {
        "semi_auto" => out.push("semi_auto"),
        "auto" => out.push("auto"),
        // A burst trigger is its OWN family, not a semi-auto that fires three
        // times: the wiki lists the Burston's trigger as "Burst", and the
        // Semi-* mods gate on the listed trigger. So a Burston takes no
        // Semi-Rifle Cannonade, which is exactly what the roster's Cannonade
        // table asserts weapon by weapon.
        "burst" => out.push("burst"),
        _ => {}
    }
    // Leaked because the class is data-driven and the caller wants a 'static
    // slice; the set is one entry per weapon and never grows at runtime.
    out.push(Box::leak(s.class.clone().into_boxed_str()));
    // MODULAR, which no class can say. A primary Tombfinger is a `rifle` and a
    // secondary one a `pistol` — the same two classes as a Braton and a Lex —
    // and the eight Pax and Residual arcanes go on neither. DERIVED from the
    // one field that makes the weapon modular rather than written on the entry
    // beside it, so a chamber transcribed tomorrow cannot arrive without it.
    if s.kitgun.is_some() {
        out.push("modular");
    }
    let leaked: &'static [&'static str] = Box::leak(out.into_boxed_slice());
    g.insert(s.id.clone(), leaked);
    leaked
}
