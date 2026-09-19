use super::*;

/// What one deployment changes about a weapon. Every field is optional: a
/// column states only where it differs from the entry's own.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DeploymentSpec {
    #[serde(default)]
    pub reload_seconds: Option<f64>,
    #[serde(default)]
    pub magazine: Option<f64>,
    #[serde(default)]
    pub ammo_max: Option<f64>,
    #[serde(default)]
    pub no_resupply: Option<bool>,
    /// THIS COLUMN'S DAMAGE, transcribed from the infobox tab rather than
    /// derived from the entry's.
    ///
    /// VERBATIM (wiki `Archgun`): *"most Heavy Weapons (a.k.a. Archguns when
    /// used via the Archgun Deployer) have had their damage doubled"* — so a
    /// ground Arch-Gun hits for twice what the same weapon does in an Archwing
    /// mission, and the two tabs of its infobox say so field for field.
    ///
    /// THE DEPLOYMENT AXIS IS NOT ONLY A SUSTAIN AXIS. Crit, multiplier,
    /// status, fire rate and magazine ARE identical between the two columns;
    /// the DAMAGE is not, so reading the table as "same damage, only the
    /// sustain differs" scores an Arch-Gun on the board at half its ground
    /// damage.
    ///
    /// BOTH COLUMNS ARE WRITTEN DOWN rather than the `x0.5` that produces the
    /// same numbers, because a multiplier is DERIVED and a reader cannot check
    /// it against the page. The doubling is "most", not all, so the ratio is an
    /// observation about a weapon and never a rule to compute with —
    /// `deployment_tests` NAMES the weapons that do not follow it.
    #[serde(default)]
    pub damage: Option<BTreeMap<String, f64>>,
    /// ...and the same tab's RADIAL, when the attack has one. Required
    /// alongside `damage` by `validate` rather than optional in practice: an
    /// explosion left on the other column is exactly the half-applied
    /// deployment this whole field exists to prevent.
    #[serde(default)]
    pub radial_damage: Option<BTreeMap<String, f64>>,
    /// ...and its lingering FIELD, under the same rule.
    #[serde(default)]
    pub lingering_damage: Option<BTreeMap<String, f64>>,
    /// THE STATS THAT ALSO MOVE. Damage is the field that cost something, but
    /// it is not the only one a tab can change: the Kuva Grattler's critical
    /// multiplier is 2.10x in Archwing and 2.00x on the ground, and nothing
    /// about "a deployment is a sustain axis" would have predicted that either.
    ///
    /// Only the three that reach a number are here. Blast RADIUS, projectile
    /// SPEED and falloff differ too on several Arch-Guns (the Kuva Ayanga's
    /// explosion is 9.0 m in space and 6.0 m on the ground, its grenade 300 m/s
    /// against 55) and none of them changes a number in an arena with one
    /// target and no distance — so those are transcribed into each weapon's
    /// comments and named in its `unmodeled:`, rather than carried as fields
    /// that would move nothing.
    #[serde(default)]
    pub crit_chance: Option<f64>,
    #[serde(default)]
    pub crit_multiplier: Option<f64>,
    #[serde(default)]
    pub status_chance: Option<f64>,
}

/// THE VALENCE BONUS an ADVERSARY weapon carries — a Kuva Lich's, a Sister's, a
/// Coda's — as the weapon declares what it CAN have.
///
/// VERBATIM (wiki, Kuva Weapons §Elemental Bonus): *"The Kuva weapons
/// additionally have bonus damage of one damage type which can either be
/// Impact, Heat, Cold, Electricity, Toxin, Magnetic, or Radiation, ranging from
/// 25-60% of the weapon's base damage determined randomly. … This additional
/// bonus damage applies as weapon base damage, meaning elemental mods and
/// status that scale from base / modified base damage will be affected."*
///
/// So it is not a bucket and not a buff: it is the WEAPON's own base vector,
/// which is why nothing downstream needs to know it exists. An innate element
/// already composes with the mod elements the way MECHANICS §3 rule 2 says, and
/// this arrives as one.
///
/// The SPEC is what a weapon may have; the CHOICE (which element, what
/// percentage) belongs to the build, because it is a property of the copy a
/// player owns rather than of the model — the same shape a riven has.
#[derive(Debug, Clone, Deserialize)]
pub struct ValenceSpec {
    /// The progenitor elements this weapon's bonus can be, in the wiki's order.
    pub elements: Vec<String>,
    /// The roll's floor and ceiling as fractions of base damage (0.25–0.60 on
    /// every Kuva weapon). Both are stated rather than assumed: a Tenet or Coda
    /// entry may differ and the page is the only thing that knows.
    pub min: f64,
    pub max: f64,
}

/// Apply a chosen VALENCE BONUS to a resolved base, in place.
///
/// `bonus` is a fraction of the base TOTAL, added as `element` — merged into
/// that element if the weapon already deals it, which is what a Radiation
/// progenitor on a Radiation weapon does. Written beside `apply_deployment`
/// because it is the same shape of thing: a per-request choice the base cannot
/// carry, applied once, at the one place a request builds its weapon.
///
/// Out of range is CLAMPED rather than refused: the roll's floor and ceiling
/// are the game's, and a request that asks for more gets the ceiling instead of
/// an error nobody can act on. A weapon with no spec is left alone.
pub fn apply_valence(base: &mut WeaponBase, id: &str, element: &str, bonus: f64) {
    let Some(s) = spec(id).and_then(|s| s.valence.as_ref()) else { return };
    if !s.elements.iter().any(|e| e == element) {
        return;
    }
    let Some(ty) = crate::rules::damage::DamageType::from_name(element) else { return };
    let fraction = bonus.clamp(s.min, s.max);
    let total = base.base_vector.total();
    if total <= 0.0 || fraction <= 0.0 {
        return;
    }
    let add = total * fraction;
    base.base_vector = base.base_vector.with(ty, base.base_vector.get(ty) + add);
    // …AND THE CO TERM'S BASE WITH IT. The bonus is the weapon's OWN listed
    // base — "increases the listed base damage of the weapon by 25%-60%" — so
    // there is no smaller original for GunCO to read: every copy in the game
    // has a Lich's element in its printed damage. Left behind, `co_base` makes
    // a maxed Kuva Nukor read 62% of its own base, against the CO catalog's
    // own row for the family ("Kuva Seer … 131 | 131 | 100%").
    base.co_base *= 1.0 + fraction;
    // THE RADIAL TOO, on a weapon that has one: the bonus is base damage, and a
    // radial's base is base damage. The Kuva Bramma, Ogris, Tonkor and Zarr all
    // carry one.
    if let Some(r) = base.radial.as_mut() {
        let rt = r.base_vector.total();
        if rt > 0.0 {
            let radd = rt * fraction;
            r.base_vector = r.base_vector.with(ty, r.base_vector.get(ty) + radd);
            r.co_base *= 1.0 + fraction;
        }
    }
    // …AND A LINGERING FIELD, on the same argument and still on no weapon in
    // this roster. A field a MOD grants is the case that is real today, and it
    // cannot be reached from here — see `WeaponBase::valence_bonus`, which
    // `resolve_for` spends when it builds one.
    if let Some(l) = base.lingering.as_mut() {
        let lt = l.base_vector.total();
        if lt > 0.0 {
            let ladd = lt * fraction;
            l.base_vector = l.base_vector.with(ty, l.base_vector.get(ty) + ladd);
        }
    }
    // WHAT IS LEFT FOR A PART THAT DOES NOT EXIST YET.
    base.valence_bonus = fraction;
}

/// The valence spec of a weapon, if it is an adversary weapon at all.
pub fn valence_of(id: &str) -> Option<&'static ValenceSpec> {
    spec(id).and_then(|s| s.valence.as_ref())
}

/// Apply a DEPLOYMENT's overrides to a resolved base, in place.
///
/// A no-op for the weapon's own deployment (its fields already are that
/// column) and for a name it does not have — an unknown environment leaves the
/// weapon alone rather than half-applying something.
pub fn apply_deployment(base: &mut WeaponBase, id: &str, deployment: &str) {
    let Some(s) = spec(id) else { return };
    if s.deployment.as_deref() == Some(deployment) {
        return;
    }
    let Some(d) = s.deployments.get(deployment) else { return };
    if let Some(v) = d.reload_seconds {
        base.base_reload = v;
    }
    if let Some(v) = d.magazine {
        base.magazine_size = v;
    }
    if let Some(v) = d.ammo_max {
        base.ammo_reserve = v;
    }
    if let Some(v) = d.no_resupply {
        base.no_resupply = v;
    }
    // EVERY ATTACK PART, because the infobox states every attack part. The
    // direct vector alone would leave an Arch-Gun's explosion on the other
    // column — which is the half-applied deployment `validate` refuses.
    let vector_of = |m: &std::collections::BTreeMap<String, f64>| {
        let mut v = crate::rules::damage::DamageVector::new();
        for (name, amount) in m {
            v.add(damage_type(name), *amount);
        }
        v
    };
    if let Some(m) = d.damage.as_ref() {
        base.base_vector = vector_of(m);
    }
    if let Some(m) = d.radial_damage.as_ref() {
        if let Some(r) = base.radial.as_mut() {
            r.base_vector = vector_of(m);
        }
    }
    if let Some(m) = d.lingering_damage.as_ref() {
        if let Some(l) = base.lingering.as_mut() {
            l.base_vector = vector_of(m);
        }
    }
    // The radial inherits the direct part's crit and status unless it states
    // its own (RadialSpec), so a column that moves them moves both — which is
    // what the Kuva Grattler's two tabs actually do.
    if let Some(v) = d.crit_chance {
        base.base_crit_chance = v;
    }
    if let Some(v) = d.crit_multiplier {
        base.base_crit_damage = v;
    }
    if let Some(v) = d.status_chance {
        base.base_status_chance = v;
    }
}

/// Every deployment this weapon has, its OWN first. Fewer than two means the
/// axis does not exist for it and nothing should offer a choice.
pub fn deployments_of(id: &str) -> Vec<String> {
    let Some(s) = spec(id) else { return Vec::new() };
    let Some(own) = s.deployment.clone() else { return Vec::new() };
    let mut out = vec![own];
    out.extend(s.deployments.keys().cloned());
    out
}
