use super::*;

pub(super) fn rarity(name: &str) -> Rarity {
    match name {
        "common" => Rarity::Common,
        "uncommon" => Rarity::Uncommon,
        "rare" => Rarity::Rare,
        "legendary" => Rarity::Legendary,
        other => panic!("unknown arcane rarity: {other}"),
    }
}

/// Load every embedded arcane yaml under a `data/` prefix (e.g.
/// `"arcanes/secondary/"`) into arcane definitions (sorted by id).
pub fn load_pool(prefix: &str) -> Vec<ArcaneDef> {
    let mut out = Vec::new();
    for (path, text) in crate::data::files_under(prefix) {
        // THE DIRECTORY IS THE DEFAULT SEAT, taken from the path rather than
        // from the caller's prefix so a pool loaded any other way still gets it.
        let dir: &'static str = path
            .strip_prefix("arcanes/")
            .and_then(|p| p.split('/').next())
            .map(|d| &*Box::leak(d.to_string().into_boxed_str()))
            .unwrap_or("secondary");
        let af: ArcaneFile =
            serde_norway::from_str(text).unwrap_or_else(|e| panic!("parse {path}: {e}"));
        let effects = af
            .effects
            .iter()
            .filter_map(effect)
            .collect();
        out.push(ArcaneDef {
            id: af.id,
            name: af.name,
            rarity: rarity(&af.rarity),
            max_rank: af.max_rank,
            requires: af.requires,
            seats: if af.seats.is_empty() {
                vec![dir]
            } else {
                af.seats
                    .iter()
                    .map(|s| &*Box::leak(s.clone().into_boxed_str()))
                    .collect()
            },
            equip_traits: af
                .equip_traits
                .iter()
                .map(|s| &*Box::leak(s.clone().into_boxed_str()))
                .collect(),
            equip_classes: af
                .equip_classes
                .into_iter()
                .map(|s| &*Box::leak(s.into_boxed_str()))
                .collect(),
            description: af.description.unwrap_or_default(),
            live_bugs: af.live_bugs,
            perk: af.perk,
            effects,
        });
    }
    out
}

/// Every arcane SLOT present in the data — one per `data/arcanes/<slot>/`
/// directory, sorted. Same discovery rule as the mod classes: dropping in
/// `data/arcanes/primary/` publishes primary arcanes with no code change.
pub fn slots() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = crate::data::files_under("arcanes/")
        .filter_map(|(p, _)| p.strip_prefix("arcanes/")?.split('/').next())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The arcane pool of one slot — `data/arcanes/<slot>/*.yaml`. Cached per
/// slot (each entry leaks once).
pub fn slot_pool(slot: &str) -> &'static [ArcaneDef] {
    static POOLS: OnceLock<Mutex<BTreeMap<String, &'static [ArcaneDef]>>> = OnceLock::new();
    let cache = POOLS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut g = cache.lock().expect("arcane pool cache");
    g.entry(slot.to_string())
        .or_insert_with(|| {
            Box::leak(load_pool(&format!("arcanes/{slot}/")).into_boxed_slice())
        })
}

/// The arcanes THIS WEAPON may equip in this slot.
///
/// `slot_pool` is every arcane filed under the slot; this is the subset the
/// arsenal would actually offer. Two arcanes narrow it — Shotgun Vendetta and
/// Longbow Sharpshot, the only two the wiki types by weapon CLASS rather than
/// by slot — and a crossbow is not a bow (`Class = "Crossbow"`), which is why
/// "cannot be equipped on Attica, Nagantaka or Zhuge" needs no special case.
///
/// The engine decides, once, for the page and the sim alike — the same rule
/// `data::mods::pool_for_weapon` follows.
pub fn pool_for_weapon(weapon: &str, slot: &str) -> Vec<&'static ArcaneDef> {
    let spec = crate::data::weapons::spec(weapon);
    let class = spec.map(|s| s.class.as_str());
    // The DERIVED traits — trigger, class and `modular` — which is the same
    // list a mod's `requires:` is gated on.
    let traits = spec.map(crate::data::weapons::traits_of).unwrap_or(&[]);
    // EVERY POOL, not this slot's, because an arcane may declare SEATS beyond
    // the directory it is filed under — the Kitgun family fits both. The filter
    // below is what puts it back: an arcane that declares nothing gets its own
    // directory as its only seat, so this is the same list it always was for
    // every other arcane in the game.
    slots()
        .into_iter()
        .flat_map(slot_pool)
        .filter(|a| a.seats.contains(&slot))
        .filter(|a| {
            (a.equip_classes.is_empty() || class.is_some_and(|c| a.equip_classes.contains(&c)))
                // AND, not OR: an arcane may narrow by class, by trait, or by
                // both, and each list it states has to be satisfied.
                && (a.equip_traits.is_empty()
                    || a.equip_traits.iter().all(|t| traits.iter().any(|w| w == t)))
        })
        .collect()
}

/// Which slot an arcane id belongs to, if any.
pub fn slot_of(id: &str) -> Option<&'static str> {
    slots()
        .into_iter()
        .find(|s| slot_pool(s).iter().any(|a| a.id == id))
}

/// The secondary-arcane pool — `data/arcanes/secondary/*.yaml`.
pub fn secondary_pool() -> &'static [ArcaneDef] {
    slot_pool("secondary")
}

/// Look up an arcane by id across EVERY slot (ids are globally unique).
///
/// This is the DISPLAY lookup — "what is this arcane?" — and it deliberately
/// ignores where the arcane can go. Anything that APPLIES an arcane to a
/// weapon must use [`for_slot`] instead.
pub fn secondary(id: &str) -> Option<&'static ArcaneDef> {
    slots()
        .into_iter()
        .find_map(|s| slot_pool(s).iter().find(|a| a.id == id))
}

/// Resolve an arcane FOR A SLOT — the lookup every equipping path must use.
///
/// An arcane belongs to exactly one slot, so another slot's arcane is not a
/// questionable choice on this weapon: it cannot be equipped at all. Ids
/// arrive from saved builds, shared URLs and preset imports, so the refusal
/// lives here rather than in each caller's own filtering (a SECONDARY arcane
/// was silently applying to the first primary weapon).
pub fn for_slot(slot: &str, id: &str) -> Option<&'static ArcaneDef> {
    // EVERY POOL, filtered by the SEATS the arcane declares — the same rule
    // `pool_for_weapon` follows, and it has to be the same rule or an arcane
    // the picker offers is one the equip path then refuses. A Kitgun arcane is
    // filed under one directory and fits both seats; everything else declares
    // nothing and gets its own directory as its only seat, which is the list
    // this function always returned.
    slots()
        .into_iter()
        .flat_map(slot_pool)
        .find(|a| a.id == id && a.seats.contains(&slot))
}
