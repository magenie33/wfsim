use super::*;

/// Load a weapon class's embedded mod pool — `data/mods/<class>/*.yaml`
/// (each class gets its own subfolder so the flat pool doesn't get muddled
/// as the mod count grows). Sorted by file path, i.e. by id.
pub fn load_class(class: &str) -> Vec<ModDef> {
    crate::data::files_under(&format!("mods/{class}/"))
        .map(|(path, text)| {
            let mf: ModFile =
                serde_norway::from_str(text).unwrap_or_else(|e| panic!("parse {path}: {e}"));
            to_moddef(mf)
        })
        .collect()
}

/// Every mod CLASS present in the data — one per `data/mods/<class>/`
/// directory, sorted. The registry publishes a pool per class, so adding
/// `data/mods/rifle/` is enough to make rifle mods reachable: no code.
pub fn classes() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = crate::data::files_under("mods/")
        .filter_map(|(p, _)| p.strip_prefix("mods/")?.split('/').next())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The mod pool of one class — `data/mods/<class>/*.yaml`. Cached per class
/// (each entry leaks its id/family strings once); cloned so callers own it.
pub fn class_pool(class: &str) -> Vec<ModDef> {
    static POOLS: OnceLock<Mutex<BTreeMap<String, &'static [ModDef]>>> = OnceLock::new();
    let cache = POOLS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut g = cache.lock().expect("mod pool cache");
    g.entry(class.to_string())
        .or_insert_with(|| Box::leak(load_class(class).into_boxed_slice()))
        .to_vec()
}

/// A CARD BELOW ITS MAX RANK IS AN ID OF ITS OWN: `<card>@<rank>`.
///
/// The rank rides inside the id so every surface that already carries a mod
/// list — a request, a search's scope, a board record, a share link — carries
/// the rank with it; a parallel list would be one more axis each of them could
/// drop. Max rank is the bare id, so no stored build changes.
pub const RANK_MARK: char = '@';

/// `<card>@<rank>` → (`card`, `Some(rank)`); anything else → (`id`, `None`).
pub fn split_rank(id: &str) -> (&str, Option<u32>) {
    match id.split_once(RANK_MARK).and_then(|(c, r)| Some((c, r.parse().ok()?))) {
        Some((card, r)) => (card, Some(r)),
        None => (id, None),
    }
}

/// The id of `card` at `rank`: the bare id at (or past) max rank.
pub fn ranked_id(card: &str, rank: u32, max_rank: u32) -> String {
    if rank >= max_rank {
        card.to_string()
    } else {
        format!("{card}{RANK_MARK}{rank}")
    }
}

/// The card a ranked id names at its rank, or None: not a ranked id, an unknown
/// card, a rank at or past max (that card is the bare id), or a card whose
/// lower ranks are unmodelled. Built once per id and kept.
pub fn at_rank(id: &str) -> Option<ModDef> {
    let (card, Some(rank)) = split_rank(id) else { return None };
    static TEXTS: OnceLock<BTreeMap<String, &'static str>> = OnceLock::new();
    static BUILT: OnceLock<Mutex<BTreeMap<String, Option<ModDef>>>> = OnceLock::new();
    let texts = TEXTS.get_or_init(|| {
        crate::data::files_under("mods/")
            .filter_map(|(_, text)| {
                let v: Value = serde_norway::from_str(text).ok()?;
                Some((v.get("id")?.as_str()?.to_string(), text))
            })
            .collect()
    });
    let mut built = BUILT.get_or_init(|| Mutex::new(BTreeMap::new())).lock().expect("ranked mods");
    built
        .entry(id.to_string())
        .or_insert_with(|| {
            let mf: ModFile = serde_norway::from_str(texts.get(card)?).ok()?;
            (rank < mf.max_rank && !mf.lower_ranks_unmodelled)
                .then(|| to_moddef_at(mf, Some(rank)))
        })
        .clone()
}

/// `pool` plus every ranked card `ids` names whose card is in it — the pool a
/// build or a search scope that names lower ranks resolves against. A card that
/// gains a variant gains its own id as a family too, so the two exclude each
/// other wherever a family is asked.
pub fn with_ranks<'a>(pool: &mut Vec<ModDef>, ids: impl IntoIterator<Item = &'a str>) {
    for id in ids {
        if pool.iter().any(|m| m.id == id) {
            continue;
        }
        let (card, Some(_)) = split_rank(id) else { continue };
        let Some(base) = pool.iter_mut().find(|m| m.id == card) else { continue };
        let Some(variant) = at_rank(id) else { continue };
        base.family = variant.family;
        pool.push(variant);
    }
}

/// The cards a search tries at every rank by default —
/// `data/search/every_rank.yaml`, by id.
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct EveryRank {
    pub mods: Vec<String>,
    pub arcanes: Vec<String>,
}

pub fn every_rank() -> &'static EveryRank {
    static LIST: OnceLock<EveryRank> = OnceLock::new();
    LIST.get_or_init(|| {
        let text = crate::data::file("search/every_rank.yaml").expect("search/every_rank.yaml");
        serde_norway::from_str(text).expect("parse search/every_rank.yaml")
    })
}

/// [`pool_for_weapon`], plus the lower ranks `ids` name — the pool a build
/// written as a mod list resolves against.
pub fn pool_naming<S: AsRef<str>>(weapon_id: &str, ids: &[S]) -> Vec<ModDef> {
    let mut pool = pool_for_weapon(weapon_id);
    with_ranks(&mut pool, ids.iter().map(AsRef::as_ref));
    pool
}

/// The pool a weapon actually sees: the UNION of the named pools, in order,
/// deduplicated by mod id.
///
/// The game's compatibility is not one flat list per weapon. DE tags a mod
/// PRIMARY (fits any primary weapon), Rifle (the class), or narrower still —
/// Assault Rifle, Bow, Sniper — and a weapon draws every tag that applies to
/// it. Collapsing that into a single directory per weapon was right only
/// while every rifle-class weapon in the roster was a launcher.
pub fn pool_union(pools: &[String]) -> Vec<ModDef> {
    let mut out: Vec<ModDef> = Vec::new();
    for p in pools {
        for m in class_pool(p) {
            if !out.iter().any(|x| x.id == m.id) {
                out.push(m);
            }
        }
    }
    out.sort_by_key(|m| m.id);
    out
}

/// The pool a weapon can EQUIP WITH NOTHING INSTALLED: its pools unioned, minus
/// mods whose equip requirement the weapon does not meet. [`pool_for_build`] is
/// the same rule once evolutions are chosen.
///
/// The compat tag is not the whole rule. Sinister Reach and Combustion Beam
/// are tagged PRIMARY and still cannot go on the Torid:
/// they need a CONTINUOUS weapon. The Torid is the case that shows where the
/// line falls — its Incarnon form IS a continuous beam and it still cannot
/// take them, because its OTHER firing mode is a semi-auto grenade launcher
/// and an equip rule is asked of every mode a weapon has.
pub fn pool_for_weapon(weapon_id: &str) -> Vec<ModDef> {
    pool_for_build(weapon_id, &[])
}

/// THE WEAPON AN ENTRY BELONGS TO — itself, or the DEFAULT form of its
/// transform group. `default_form: true` is "the arsenal's form" (the module's
/// `_TooltipAttackDisplay`), the entry the weapon comparison lists.
///
/// A FORM IS NOT A WEAPON, and every weapon-level question about one is a
/// question about the weapon: the trigger a mod's rule is judged against
/// (`cernos_prime_uncharged` fires semi-auto and the bow it belongs to is
/// listed "Charge"), and the POOL that decides what may be equipped at all,
/// because modding happens on the weapon and not on a firing mode.
///
/// ONE SPELLING, because two drift silently: a form answering a mod's trigger
/// rule with its weapon's answer and then having no pool to apply it to is a
/// refusal that names no reason.
pub(super) fn weapon_of(spec: &'static crate::data::weapons::WeaponSpec)
    -> &'static crate::data::weapons::WeaponSpec
{
    let group = spec.transform_group.as_deref().unwrap_or(&spec.id);
    crate::data::weapons::all()
        .iter()
        .find(|x| x.transform_group.as_deref().unwrap_or(&x.id) == group && x.default_form)
        .unwrap_or(spec)
}


/// Every trigger a BUILD can FIRE: the weapon's own, plus that of any form an
/// installed evolution UNLOCKS.
///
/// A firing MODE is what an equip rule is asked about, and an Incarnon weapon
/// has two of them: "Weapons with an Incarnon mode must have Semi-Auto trigger
/// type for both firing modes in order to equip this mod" (wiki,
/// Semi-Pistol_Cannonade). So Dual Toxocyst — semi-auto, with a full-auto
/// Incarnon form — takes a Cannonade while the Genesis is not installed and
/// refuses it the moment tier 1 is.
///
/// A CHARGED form is NOT a second firing mode: charged vs uncharged is chosen
/// freely on every trigger pull and the weapon comparison lists ONE trigger for
/// such a weapon (Cernos Prime is "Charge", Larkspur Prime "Held"). That is
/// exactly the line [`FormKind::is_adapter_form`] already draws, and it is why
/// only a form an EVOLUTION unlocks joins this list — the arsenal gains a second
/// trigger when the Genesis goes in, not when you hold the button down.
pub(super) fn triggers_of(weapon_id: &str, evolutions: &[&str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    // THE WEAPON'S OWN trigger is its DEFAULT form's, and only an Incarnon mode
    // gets a second trigger of its own — see `weapon_of`.
    if let Some(s) = crate::data::weapons::spec(weapon_id) {
        out.push(weapon_of(s).attack.trigger.as_str());
    }
    for id in evolutions {
        let Some(form) = crate::data::evolutions::get(id).and_then(|e| e.unlocks_form()) else {
            continue;
        };
        if let Some(s) = crate::data::weapons::spec(form) {
            if !out.contains(&s.attack.trigger.as_str()) {
                out.push(s.attack.trigger.as_str());
            }
        }
    }
    out
}

/// The pool a BUILD can equip: [`pool_for_weapon`]'s rules, resolved against
/// every firing mode the chosen `evolutions` give the weapon.
///
/// `evolutions` empty is the weapon as it comes out of the box — which is what
/// [`pool_for_weapon`] means and why it is this function with nothing installed.
pub fn pool_for_build(weapon_id: &str, evolutions: &[&str]) -> Vec<ModDef> {
    let Some(spec) = crate::data::weapons::spec(weapon_id) else {
        return Vec::new();
    };
    // EVERY firing mode must meet the requirement, not just the one you happen
    // to be in. The Torid is the case that shows where the line falls for
    // `continuous`: its Incarnon form IS a beam and it still cannot take
    // Sinister Reach, because its other firing mode is a grenade launcher.
    let triggers = triggers_of(weapon_id, evolutions);
    let all = |t: &str| !triggers.is_empty() && triggers.iter().all(|x| *x == t);
    // What `WeaponBase::continuous` reads, asked of every mode.
    let continuous = all("held");
    // Same rule, other trigger: the Cannonades state "Only compatible with
    // Semi-Auto Trigger" on the card and DE enforces it at the slot.
    let semi_auto = all("semi_auto");
    // "Mods that affect Ammo Maximum have no effect on Robotic weapon because
    // they already have unlimited ammo reserves" (wiki `Sentinel`). Stated for
    // robotic weapons, true of any weapon with no ammo pool, and read off the
    // one fact that says so — `ammo_max` absent. A mod is dropped only when
    // ammo maximum is ALL it does: a dual-stat keeps its other half, whose
    // ammo share is already inert.
    // EVERY WEAPON-LEVEL FACT IS READ THROUGH `weapon_of`, and the list of
    // which facts those are is not this function's to invent: it is
    // `data::weapons::INHERITED`, which already says a form takes its weapon's
    // `class`, `magazine` and `ammo_max`. A form that read its own answered
    // "no ammo pool" and dropped every ammo mod off a weapon that has one.
    let weapon = weapon_of(spec);
    let no_ammo_pool = weapon.ammo_max.is_none();
    let only_ammo_max = |m: &ModDef| {
        !m.effects.is_empty()
            && m.effects.iter().all(|e| {
                matches!(e, ModEffect::Indirect(crate::model::IndirectStat::AmmoMax, _))
            })
    };
    // THE POOL IS THE WEAPON'S, whatever entry was named — `weapon_of`. A form
    // states no `mod_pools` of its own, and an empty pool is indistinguishable
    // from a weapon that refuses everything.
    // A MOD MAY NAME A WEAPON KIND beyond the pools that carry it: a sentinel
    // weapon takes the PRIMARY Vigilante set without drawing the Primary pool
    // (`ModDef::includes_weapon`). A weapon with NO pool refuses everything.
    let mut pool = pool_union(&weapon.mod_pools);
    if weapon.class.contains("sentinel") && !weapon.mod_pools.is_empty() {
        for m in classes().into_iter().flat_map(class_pool) {
            if m.includes_weapon.contains(&"sentinel_weapon") && !pool.iter().any(|x| x.id == m.id) {
                pool.push(m);
            }
        }
        pool.sort_by_key(|m| m.id);
    }
    pool
        .into_iter()
        .filter(|m| match m.requires_weapon {
            None => true,
            Some("continuous") => continuous,
            Some("semi_auto") => semi_auto,
            // SYNTH CHARGE's magazine gate, and it reads the BASE magazine
            // rather than the modded one — the wiki says so in both directions:
            // "If the magazine is increased above 6 on a weapon that has below
            // 6, it will still not be usable on that gun. However, if a gun has
            // a magazine above 6 and it is reduced below that, the mod will
            // still function." So it is an equip rule and not a live check:
            // no mod can buy it and no mod can lose it.
            //
            // `magazine` on the spec IS the base magazine — the mod layer never
            // writes it — which is what makes this the right number to read.
            Some("magazine_6") => weapon.magazine.is_some_and(|m| m >= 6.0),
            // An unknown requirement hides the mod rather than ignoring the
            // restriction — a mod offered where it cannot go is the worse bug.
            Some(_) => false,
        })
        // A mod written for ONE weapon goes nowhere else. Matched against the
        // transform GROUP as well as the id, so an Incarnon form counts as the
        // weapon its mod was written for rather than as a stranger.
        .filter(|m| {
            m.exclusive_to.is_empty()
                || m.exclusive_to.contains(&weapon_id)
                || spec
                    .transform_group
                    .as_deref()
                    .is_some_and(|g| m.exclusive_to.contains(&g))
        })
        .filter(|m| !(no_ammo_pool && only_ammo_max(m)))
        // DE's INCOMPATIBILITY tags — the mirror of `requires_weapon`, and the
        // reason plain Serration goes on a sentinel weapon while Amalgam
        // Serration does not. An Amalgam mod's second half
        // buffs the WARFRAME ("+25% Sprint Speed... always applies, regardless
        // of whether or not you are holding the weapon"), and a companion is
        // not the Warframe, so the wiki states it outright: "This mod cannot be
        // equipped on Sentinel weapons", tags `SENTINEL_WEAPON, POWER_WEAPON`.
        // POWER_WEAPON is the EXALTED weapon; the cards Techrot Encore
        // re-enabled on one carry no such tag (see notes: exalted_mods_reenabled).
        .filter(|m| {
            !(weapon.class.contains("sentinel") && m.excludes_weapon.contains(&"sentinel_weapon"))
                && !(weapon.exalted && m.excludes_weapon.contains(&"power_weapon"))
        })
        .collect()
}

/// The secondary/pistol mod pool — `data/mods/pistol/*.yaml` (Dual Toxocyst's
/// and Laetum's pool).
pub fn pistol_pool() -> Vec<ModDef> {
    class_pool("pistol")
}
