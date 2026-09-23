use crate::model::*;
use crate::rules::capacity::Polarity;

/// NIGHTWATCH NAPALM leaves a field on a weapon that has none, and the
/// field obeys THREE rules the rest of the app does not.
///
/// Its damage is a flat 150 Heat that only the base-damage bucket and the
/// faction bonus may touch — *"Damage output can only be increased by base
/// damage mods … and faction mods"* — it cannot crit *"via any means"*, and
/// its 68% Heat chance is *"not affected by mods"*. Each is asserted
/// against a build carrying the mod that would move it if the rule were
/// missing, which is the only way to tell a rule from a coincidence.
#[test]
fn nightwatch_napalm_leaves_a_field_outside_every_bucket_but_base_damage() {
    let pool = crate::data::mods::pool_for_build("ogris", &[]);
    let by = |id: &str| pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id}"));
    let napalm = by("nightwatch_napalm");
    let base = super::WeaponBase::from_data("ogris", false, &[]);
    assert!(base.lingering.is_none(), "the Ogris leaves no fire of its own");

    let field = |mods: &[&_]| {
        super::resolve(&base, mods, super::StackPolicy::Emergent)
            .lingering
            .expect("the mod leaves one")
    };
    let plain = field(&[napalm]);
    assert!((plain.damage.total() - 150.0).abs() < 1e-9,
        "a flat 150 unmodded, not a share of the rocket: {}", plain.damage.total());
    assert_eq!(plain.crit_chance, 0.0, "it cannot crit via any means");
    assert!((plain.status_chance - 0.68).abs() < 1e-9);
    assert!((plain.duration_seconds - 6.0).abs() < 1e-9);

    // BASE DAMAGE REACHES IT…
    let serration = by("serration");
    let bd = field(&[napalm, serration]);
    let want = 150.0 * (1.0 + serration_bonus(serration));
    assert!((bd.damage.total() - want).abs() < 1e-6,
        "the base-damage bucket is the one that reaches it: {} vs {want}", bd.damage.total());

    // …AND AN ELEMENT DOES NOT, which is the sentence's other half. The
    // same mod moves the ROCKET, so a build cannot be told the two apart
    // by looking at the weapon.
    let cryo = by("cryo_rounds");
    let el = field(&[napalm, cryo]);
    assert!((el.damage.total() - 150.0).abs() < 1e-9,
        "no element goes near the fire: {}", el.damage.total());
    assert!(
        super::resolve(&base, &[napalm, cryo], super::StackPolicy::Emergent)
            .damage
            .get(crate::rules::damage::DamageType::Cold)
            > 0.0,
        "…while the rocket itself takes it"
    );

    // …AND NEITHER DOES A STATUS MOD.
    let rifle_aptitude = by("rifle_aptitude");
    assert!((field(&[napalm, rifle_aptitude]).status_chance - 0.68).abs() < 1e-9,
        "68% is pinned");

    // …AND NEITHER DOES A CRIT MOD, in either half. "It cannot crit via any
    // means", so the zero above has to survive a build that is TRYING to
    // make it crit — a zero asserted only on a bare weapon says nothing
    // about the mod that would move it (owner asked: this
    // decides how the weapon is built).
    //
    // BOTH HALVES, because they are two buckets and a field could leak
    // either: Point Strike multiplies the base crit CHANCE, which is zero
    // and stays zero, while Vital Sense multiplies the crit DAMAGE, which
    // is 1.0 and would quietly become 3.4 — invisible until something
    // rolled a crit that cannot happen.
    let point_strike = by("point_strike");
    let vital_sense = by("vital_sense");
    let critted = field(&[napalm, point_strike, vital_sense]);
    assert_eq!(critted.crit_chance, 0.0, "a crit-chance mod moved a field that cannot crit");
    assert!((critted.crit_damage - 1.0).abs() < 1e-9,
        "a crit-damage mod reached it: x{}", critted.crit_damage);
    // …while the ROCKET takes both, which is what makes the two rows
    // different and the assertion worth making.
    let rocket = super::resolve(&base, &[napalm, point_strike, vital_sense],
                                super::StackPolicy::Emergent);
    assert!(rocket.crit_chance > 0.05, "the rocket still crits: {}", rocket.crit_chance);
    assert!(rocket.crit_damage > 2.0, "…and still crits harder: {}", rocket.crit_damage);
}

/// …AND THE FIRE IS A SHARE OF THE BLAST, so Firestorm grows it.
///
/// DE's card says "across 90% of the explosion area", which is an AREA and
/// a FRACTION — not the rank table's "+90%" — so the radius is sqrt(0.9) of
/// the rocket's, and a blast-radius mod moves both without this having an
/// arm for it.
#[test]
fn the_napalm_covers_a_share_of_the_rockets_own_blast() {
    let pool = crate::data::mods::pool_for_build("ogris", &[]);
    let napalm = pool.iter().find(|m| m.id == "nightwatch_napalm").expect("the augment");
    let firestorm = pool.iter().find(|m| m.id == "primed_firestorm").expect("primed_firestorm");
    let base = super::WeaponBase::from_data("ogris", false, &[]);
    let of = |mods: &[&_]| {
        let p = super::resolve(&base, mods, super::StackPolicy::Emergent);
        (p.radial.expect("a rocket explodes").radius_m, p.lingering.expect("fire").radius_m)
    };
    let (blast, fire) = of(&[napalm]);
    assert!((fire - blast * 0.9f64.sqrt()).abs() < 1e-9, "{fire} is sqrt(0.9) of {blast}");
    let (big_blast, big_fire) = of(&[napalm, firestorm]);
    assert!(big_blast > blast, "Firestorm grows the rocket's own blast");
    assert!(
        (big_fire - big_blast * 0.9f64.sqrt()).abs() < 1e-6 && big_fire > fire,
        "…and the fire grows with it: fire {big_fire} against a blast of {big_blast}              (want {}), was {fire}",
        big_blast * 0.9f64.sqrt()
    );
}

/// The base-damage share a mod contributes, read off its own effect rather
/// than typed in — a rank table nobody re-checks is how a test starts
/// asserting last year's numbers.
fn serration_bonus(m: &super::ModDef) -> f64 {
    m.effects
        .iter()
        .find_map(|e| match e {
            super::ModEffect::BaseDamage(v) => Some(*v),
            _ => None,
        })
        .expect("Serration is a base-damage mod")
}

/// HARKONAR SCOPE adds seconds to a window the WEAPON owns, which is what
/// makes the wiki's two answers one rule.
///
/// VERBATIM: *"All sniper rifles have a 2 second combo duration, with the
/// exception of the Lanka, which has a 6 second combo duration. Harkonar
/// Scope at max rank extends this to 14 seconds, and to 18 seconds for
/// Lanka."* Both numbers fall out of adding +12 to the weapon's own field;
/// a mod that SET the window would have to name the Lanka.
#[test]
fn harkonar_scope_extends_each_snipers_own_combo_window() {
    let scope = crate::data::mods::pool_for_build("vectis_prime", &[])
        .into_iter()
        .find(|m| m.id == "harkonar_scope")
        .expect("a sniper is offered it");
    let window = |weapon: &str, mods: &[&_]| {
        let base = super::WeaponBase::from_data(weapon, false, &[]);
        super::resolve(&base, mods, super::StackPolicy::Emergent)
            .sniper_combo
            .map(|c| c.seconds)
    };
    assert_eq!(window("vectis_prime", &[]), Some(2.0), "the ordinary sniper's own");
    assert_eq!(window("lanka", &[]), Some(6.0), "and the Lanka's own");
    assert_eq!(window("vectis_prime", &[&scope]), Some(14.0));
    assert_eq!(window("lanka", &[&scope]), Some(18.0));
    // …and the MINIMUM combo is untouched: this mod buys time, not hits.
    let base = super::WeaponBase::from_data("vectis_prime", false, &[]);
    let with = super::resolve(&base, &[&scope], super::StackPolicy::Emergent);
    assert_eq!(with.sniper_combo.map(|c| c.min), base.sniper_combo.map(|c| c.min));
}

/// …AND FROM THE HIP IT ADDS SECONDS TO NOTHING.
///
/// The counter is the AIMED fight's — "building combo and benefiting from
/// its multiplier requires being scoped in" — so `resolve` drops the whole
/// spec, and a card that extended a window that does not exist would be the
/// plainest kind of lie a panel can tell. The mod is still OFFERED, which
/// is right: the weapon can hold it and the fight is what decides.
#[test]
fn a_hip_fired_sniper_has_no_window_for_the_scope_to_extend() {
    let scope = crate::data::mods::pool_for_build("vectis_prime", &[])
        .into_iter()
        .find(|m| m.id == "harkonar_scope")
        .expect("still in the pool");
    let base = super::WeaponBase::from_data("vectis_prime", false, &[]);
    let mut tenno = crate::data::tenno::default_tenno().clone();
    tenno.state.aiming = false;
    let hip = super::resolve_for(&base, &[&scope], super::StackPolicy::Emergent, &tenno);
    assert!(hip.sniper_combo.is_none(), "no counter from the hip, and none to extend");
}

/// BEAM LENGTH: a flat mod adds AFTER a percentage, and the wiki says so
/// exactly once — on the other mod's page.
///
/// VERBATIM (Galvanized Acceleration, Notes): *"Sinister Reach will not be
/// affected by this mod as it gives a flat beam length increase after
/// percent bonuses."* That is the opposite of how every damage bucket here
/// works, and it is worth 3.6 m on a 20 m beam carrying both: 38 rather
/// than 41.6.
///
/// The Phantasma is the whole reason the order is testable — it is the one
/// weapon in the roster that can hold a flat beam mod and the percentage
/// one at the same time, being a continuous SHOTGUN.
#[test]
fn beam_length_takes_the_flat_metres_after_the_percentage() {
    // Off the PHANTASMA's own pool, which is the assertion's other half:
    // a mod it could not equip could not be measured on it either.
    let pool = crate::data::mods::pool_for_build("phantasma", &[]);
    let by_id = |id: &str| {
        pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id} is in the pool"))
    };
    let reach = by_id("sinister_reach");
    let accel = by_id("galvanized_acceleration");
    let base = super::WeaponBase::from_data("phantasma", false, &[]);
    assert!((base.range_m - 20.0).abs() < 1e-9, "the Phantasma's own reach: {}", base.range_m);
    let range = |mods: &[&_]| super::resolve(&base, mods, super::StackPolicy::Emergent).range_m;
    assert!((range(&[reach]) - 32.0).abs() < 1e-9, "flat alone: {}", range(&[reach]));
    // The percentage is +90%: the card's own +30% plus its on-kill half at
    // the assumed max every indirect buff grant is read at, two stacks of
    // +30%. 20 x 1.9 = 38.
    assert!((range(&[accel]) - 38.0).abs() < 1e-9, "percent alone: {}", range(&[accel]));
    // …and BOTH is 20 x 1.9 + 12, never (20 + 12) x 1.9 = 60.8. The two
    // readings are 10.8 m apart on this weapon, which is three ranks of the
    // group ruler's grid.
    let both = range(&[reach, accel]);
    assert!((both - 50.0).abs() < 1e-9, "the flat metres land outside the percentage: {both}");
    assert!((both - (base.range_m + 12.0) * 1.9).abs() > 1.0, "and not inside it");
    // AN ATTACK THAT STATES NO REACH KEEPS NONE under either bonus, which
    // is what makes the page's per-attack notes need no transcription —
    // "only affects Panthera's Alt-fire", "only affects Phantasma's primary
    // fire" exclude exactly the entries with nothing to extend.
    let no_reach = super::WeaponBase { range_m: f64::INFINITY, ..base.clone() };
    for mods in [&[reach][..], &[accel][..], &[reach, accel][..]] {
        assert!(
            super::resolve(&no_reach, mods, super::StackPolicy::Emergent)
                .range_m
                .is_infinite(),
            "no reach stays no reach"
        );
    }
}

/// …AND THE METRES REACH THE FIGHT, which is the only assertion here that
/// could not have passed before 2026-08-19.
///
/// A range is a WALL (`gap_m <= active.range_m`), so a beam short of its target
/// deals literally nothing and the same beam extended past it deals its
/// whole output. That is the widest a mod's effect gets in this engine, and
/// it is why beam range stopped being an indirect stat: at 27 m an Ignis is
/// worth zero and an Ignis carrying Sinister Reach is worth a build.
#[test]
fn a_beam_that_reaches_deals_damage_and_one_that_does_not_deals_none() {
    let reach = crate::data::mods::pool_for_build("ignis", &[])
        .into_iter()
        .find(|m| m.id == "sinister_reach")
        .expect("the Ignis is on the catalog");
    let base = super::WeaponBase::from_data("ignis", false, &[]);
    assert!((base.range_m - 20.0).abs() < 1e-9, "the Ignis reaches 20 m");
    // And it BITES both ways round: the sabotage that would hide this bug
    // is a wall that never applies, which the first two assertions catch,
    // and one the mod cannot move, which the third does.

    // 27 m: past the weapon's own wall, inside the modded one (20 + 12).
    // `training` stands the two bodies at CONTACT; the gap is set here, and
    // it is measured surface to surface the way the scene prints it.
    let at = |mods: &[&_], gap: f64| {
        let panel = super::resolve(&base, mods, super::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(5.0);
        arena.target_at =
            crate::rules::space::Vec2::new(0.0, gap + crate::rules::space::CONTACT_RANGE_M);
        let p = crate::fight::FightParams::from_panel(
            &panel, &arena, &crate::data::arcanes::ArcaneFx::none(),
        );
        crate::fight::monte_carlo(&p, 3, 11).mean_damage
    };
    assert!(at(&[], 5.0) > 0.0, "inside its own reach it fires normally");
    assert_eq!(at(&[], 27.0), 0.0, "past the wall a beam deals nothing at all");
    assert!(
        at(&[&reach], 27.0) > 0.0,
        "and the mod moves the wall — this is the whole point of modelling it"
    );
}

/// …AND A PERK MOVES THE SAME WALL A MOD DOES.
///
/// Moonrise Velocity reads *"Increase Range by +7"*, and on a beam weapon
/// the range that means is the beam's. It lands in `flat` beside Ruinous
/// Extension rather than in `pct`, so the two are additive metres and a
/// weapon carrying both reaches the sum.
///
/// THE ATOMOS IS THE ONE WHOSE CARD ALSO SAYS *"Does not affect Incarnon
/// Form"*, which is `base_form_only` in its entry — asserted here on the
/// BASE form, which is the form the perk is about.
#[test]
fn a_range_perk_moves_the_beam_wall_the_way_a_range_mod_does() {
    let plain = super::WeaponBase::from_data("atomos", false, &[]);
    assert!((plain.range_m - 15.0).abs() < 1e-9, "the Atomos beam is 15 m");
    let perked = super::WeaponBase::from_data("atomos", false, &["atomos_moonrise_velocity"]);

    let at = |base: &super::WeaponBase, mods: &[&_], gap: f64| {
        let panel = super::resolve(base, mods, super::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(5.0);
        arena.target_at =
            crate::rules::space::Vec2::new(0.0, gap + crate::rules::space::CONTACT_RANGE_M);
        let p = crate::fight::FightParams::from_panel(
            &panel, &arena, &crate::data::arcanes::ArcaneFx::none(),
        );
        crate::fight::monte_carlo(&p, 3, 11).mean_damage
    };
    // 18 m: past the bare beam's 15, inside the perked 22. The perk is the
    // only difference between the two runs, so it is the only thing that
    // can account for one dealing nothing and the other dealing damage.
    assert_eq!(at(&plain, &[], 18.0), 0.0, "past its wall the bare beam deals nothing");
    assert!(at(&perked, &[], 18.0) > 0.0, "and the perk moves the wall");

    // AND THE METRES ADD. Ruinous Extension is +8 flat, so 22 + 8 = 30 —
    // a distance neither reaches on its own.
    let ext = crate::data::mods::pool_for_build("atomos", &[])
        .into_iter()
        .find(|m| m.id == "ruinous_extension")
        .expect("the Atomos is on Ruinous Extension's catalog");
    assert_eq!(at(&perked, &[], 26.0), 0.0, "the perk alone stops at 22 m");
    assert_eq!(at(&plain, &[&ext], 26.0), 0.0, "the mod alone stops at 23 m");
    assert!(at(&perked, &[&ext], 26.0) > 0.0, "together they reach 30 m");
}

/// …AND WHO MAY EQUIP ONE IS A CATALOG, not a property of the weapon.
///
/// DE gates both flat mods on a hidden `BEAM` compatibility tag the export
/// does not carry, so the wiki's per-weapon list is the source. The two
/// halves asserted here are the ones a "is this weapon continuous" test got
/// wrong in both directions: the Furis's Incarnon form is a beam and cannot
/// take it — *"Despite its Incarnon form being a beam weapon, Furis cannot
/// equip this mod"*, the same base-form rule the Torid taught — and the
/// Ocucor is a beam that is simply not on the list.
#[test]
fn a_beam_range_mod_goes_only_where_the_catalog_says() {
    let offered = |weapon: &str, id: &str| {
        crate::data::mods::pool_for_build(weapon, &[]).iter().any(|m| m.id == id)
    };
    assert!(offered("nukor", "ruinous_extension"), "the Nukor is on the list");
    assert!(offered("ignis", "sinister_reach"), "and the Ignis is on the other");
    assert!(!offered("ocucor", "ruinous_extension"), "the Ocucor is not on the list");
    assert!(!offered("furis_incarnon", "ruinous_extension"),
        "a beam FORM of a weapon that is not on the list stays off it");
    assert!(!offered("torid_incarnon", "sinister_reach"),
        "and so does the Torid's, which is the case that killed the old rule");
    // …and a weapon that is NOT a beam at all can be on the list: the
    // page says so in as many words.
    assert!(offered("tenet_spirex", "ruinous_extension"),
        "Tenet Spirex carries the BEAM tag despite not being a beam weapon");
}

/// A TERMINAL BLAST TAKES PUNCH THROUGH, AND PAYS FOR IT — the whole of
/// MEASUREMENTS M53, asserted end to end on the weapon it was measured on.
///
/// Three claims, and the second is the one nobody would guess. The mod
/// REACHES a form that carries a `radial:`, which the AoE class rule refuses
/// on every ordinary explosive; against a LONE enemy the explosion is then
/// lost, because the round crosses the only body and leaves the field; and
/// a weapon of the same class with NO punch through is untouched, which is
/// what says the change is scoped to the mod rather than to the form.
#[test]
fn a_terminal_blast_takes_punch_through_and_loses_its_explosion_for_it() {
    use crate::model::BlastKind;
    let id = "burston_prime_incarnon";
    let base = WeaponBase::from_data(id, true, &[]);
    assert_eq!(
        base.radial.as_ref().map(|r| r.blast_kind),
        Some(BlastKind::Terminal),
        "the form carries a terminal blast"
    );

    // 1. THE MOD REACHES IT. An ordinary radial would have refused this.
    let pool = crate::data::mods::pool_for_build(id, &[]);
    let auger = pool.iter().find(|m| m.id == "metal_auger").expect("metal_auger in the pool");
    let bare = resolve(&base, &[], StackPolicy::Emergent);
    let with = resolve(&base, &[auger], StackPolicy::Emergent);
    assert_eq!(bare.punch_through_m, 0.0, "the form has none of its own");
    assert!(
        with.punch_through_m > 2.0,
        "a terminal blast must not refuse punch through: {}",
        with.punch_through_m
    );

    // 2. …AND THE EXPLOSION IS LOST FOR IT, against one standing enemy.
    //    Measured through the fight rather than the panel, because where a
    //    blast lands is geometry and not a stat.
    let blast = |p: &ResolvedPanel| {
        // ONE STANDING ENEMY, which is the fight the measurement was taken
        // in and the only one where the trade is pure loss.
        let arena = crate::arena::Arena::training(12.0);
        let dp = crate::fight::FightParams::from_panel(
            p, &arena, &crate::data::arcanes::ArcaneFx::none());
        crate::fight::monte_carlo(&dp, 40, 11).mean_damage
    };
    let (a, b) = (blast(&bare), blast(&with));
    assert!(
        b < a,
        "punch through must COST this form its blast on a lone target: {a} -> {b}"
    );

    // 3. A CONTACT BLAST IS UNTOUCHED by all of it, which is what says this
    //    is a property of the KIND rather than of having a radial at all.
    //    The mod stays EQUIPPABLE and resolves to nothing — the card's own
    //    honest shape, `+0m` with the mod still listed as a source.
    let ogris_base = WeaponBase::from_data("kuva_ogris", true, &[]);
    assert_eq!(
        ogris_base.radial.as_ref().map(|r| r.blast_kind),
        Some(BlastKind::Contact),
        "the Kuva Ogris is the contact case"
    );
    let ogris_pool = crate::data::mods::pool_for_build("kuva_ogris", &[]);
    let ogris_auger = ogris_pool
        .iter()
        .find(|m| m.id == "metal_auger")
        .expect("the mod is still offered — it is applied at 0, not refused at the slot");
    assert_eq!(
        resolve(&ogris_base, &[ogris_auger], StackPolicy::Emergent).punch_through_m,
        0.0,
        "a contact blast must pay nothing for a punch-through mod"
    );
}

/// A PUNCH-THROUGH MOD REACHES THE RESOLVED PANEL — reported by a player
/// who asked whether Metal Auger does anything on a Burston Prime. The stats panel says "+2.1m" and the simulation behaved as
/// though it were zero, so the question is which of the two is lying.
#[test]
fn a_punch_through_mod_reaches_the_panel() {
    let base = crate::model::WeaponBase::from_data("burston_prime", false, &[]);
    let bare = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    assert_eq!(bare.punch_through_m, 0.0, "the weapon has none of its own");
    let pool = crate::data::mods::pool_for_weapon("burston_prime");
    let auger = pool
        .iter()
        .find(|m| m.id == "metal_auger")
        .expect("metal_auger is in a Burston Prime pool");
    let with = crate::build::loadout::resolve(&base, &[auger], crate::model::StackPolicy::Emergent);
    // …AND UNDER THE POLICY THE APP ACTUALLY SENDS.  runs
    // AssumedMax unless a buff is configured, and a panel that only holds
    // under Emergent is a panel no player ever sees.
    let assumed = crate::build::loadout::resolve(&base, &[auger], crate::model::StackPolicy::AssumedMax);
    assert!(
        (assumed.punch_through_m - 2.1).abs() < 1e-9,
        "AssumedMax resolved {} where Emergent resolved {}",
        assumed.punch_through_m, with.punch_through_m
    );
    assert!(
        (with.punch_through_m - 2.1).abs() < 1e-9,
        "Metal Auger is +2.1 m and the panel resolved {}",
        with.punch_through_m
    );

    // …AND IT HAS TO REACH THE FIGHT, which is the half the player could
    // see and this one could not. Four bodies in a line half a metre
    // apart: a 2.1 m budget spends 0.5 a body (`rules::space::BODY_MATERIAL_M`)
    // and should strike all four.
    let line = |p: &crate::build::loadout::ResolvedPanel| {
        let mut arena = crate::arena::Arena::training(10.0);
        arena.target_at = crate::rules::space::Vec2::new(0.0, 5.4);
        arena.others = [5.9, 6.4, 6.9]
            .iter()
            .map(|y| crate::formation::FoeSpec {
                id: String::new(),
                params: crate::target::Foe::training_dummy(),
                body_parts: crate::target::BodyPart::humanoid(),
                at: crate::rules::space::Vec2::new(0.0, *y),
            })
            .collect();
        let dp = crate::fight::FightParams::from_panel(
            p, &arena, &crate::data::arcanes::ArcaneFx::none());
        (dp.punch_through_m, dp.struck_bodies().len())
    };
    assert_eq!(line(&bare), (0.0, 1), "no mod, no bodies behind");
    assert_eq!(
        line(&with), (2.1, 4),
        "the mod's 2.1 m must reach the fight and strike all four"
    );

    // …AND THE BODIES BEHIND HAVE TO TAKE SOMETHING. Striking them is the
    // geometry; being paid is the damage path, and only the second is what
    // a player sees.
    let took = |p: &crate::build::loadout::ResolvedPanel| {
        let mut arena = crate::arena::Arena::training(10.0);
        arena.target_at = crate::rules::space::Vec2::new(0.0, 5.4);
        arena.others = [5.9, 6.4, 6.9]
            .iter()
            .map(|y| crate::formation::FoeSpec {
                id: String::new(),
                params: crate::target::Foe::training_dummy(),
                body_parts: crate::target::BodyPart::humanoid(),
                at: crate::rules::space::Vec2::new(0.0, *y),
            })
            .collect();
        let dp = crate::fight::FightParams::from_panel(
            p, &arena, &crate::data::arcanes::ArcaneFx::none());
        let r = crate::fight::run_once(&dp, &mut crate::rules::rng::Rng::new(0x5EED));
        r.taken.by_body().0.iter().filter(|d| **d > 0.0).count()
    };
    assert_eq!(took(&bare), 1, "no mod: only the aimed body is paid");
    assert_eq!(took(&with), 4, "the mod's punch through must PAY the bodies behind");
}
use super::*;
use DamageType::*;

/// Verglas Prime, from `data/weapons/sentinel/verglas_prime.yaml` — the
/// engine's reference elemental-innate weapon (100% Cold(32)), which is
/// what exercises the innate-element combination branch and the BaseOnly
/// policy a sentinel forces.
///
/// It was a hand-built struct here while the weapon was ONLY a test
/// fixture, so its numbers had two homes and only one of them shipped.
fn verglas_prime() -> WeaponBase {
    WeaponBase::from_data("verglas_prime", true, &[])
}

fn m(id: &'static str, effects: Vec<ModEffect>) -> ModDef {
    ModDef {
        stance: None,
        exclusive_to: &[],
        unmodeled: false,
        out_of_scope: false,   // a hand-built test mod discloses nothing
        id,
        name: id,
        base_drain: 10,
        max_rank: 10,
        polarity: Polarity::Madurai,
        rarity: Rarity::Common,
        exilus: false,
        family: None,
        requires_weapon: None,
        excludes_weapon: Vec::new(),
        set: None,
        requires: None,
        disables: Vec::new(),
        effects,
    }
}

fn m_req(id: &'static str, requires: Option<&'static str>, disables: Vec<&'static str>, effects: Vec<ModEffect>) -> ModDef {
    ModDef {
        stance: None, requires, disables, ..m(id, effects) }
}

/// The neutral Tenno with one state flipped — how a fight says "hip fire",
/// "invisible", "airborne". There is one knob shape for all of them now.
fn tenno_who(f: impl FnOnce(&mut crate::data::tenno::TennoState)) -> crate::data::tenno::Tenno {
    let mut t = crate::data::tenno::default_tenno().clone();
    f(&mut t.state);
    t
}

/// Spectral Serration reads "+330% Damage while Invisible". As a flat
/// `base_damage_bonus` every shot of every build would collect it; the
/// condition is a TENNO question, so the neutral player in
/// `data/tenno/` is visible, so the mod contributes nothing, and the same
/// mod on an invisible Tenno pays in full through the same code path.
///
/// That last half is the point of the seam. Nothing in the engine, the
/// data, or the UI has to learn about invisibility again when a real frame
/// arrives — it arrives as a `Tenno`.
#[test]
fn a_player_state_condition_is_asked_of_the_tenno() {
    let base = verglas_prime();
    let ss = crate::data::mods::load_class("rifle")
        .into_iter()
        .find(|d| d.id == "spectral_serration")
        .expect("spectral_serration is in the rifle pool");
    // It loads WRAPPED. Asserting the bare bonus would pass on a build
    // where the gate had been dropped, which is the bug this exists to stop.
    assert!(
        ss.effects.iter().any(|e| matches!(e,
            ModEffect::WhileTenno(TennoCondition::Invisible, inner)
                if matches!(**inner, ModEffect::BaseDamage(v) if (v - 3.3).abs() < 1e-9))),
        "spectral_serration is gated on invisibility, got {:?}",
        ss.effects
    );

    let plain = resolve(&base, &[], StackPolicy::AssumedMax).modified_base;
    let neutral = crate::data::tenno::default_tenno();
    assert!(!neutral.state.invisible, "the default Tenno is visible");
    assert!(
        (resolve_for(&base, &[&ss], StackPolicy::AssumedMax, neutral).modified_base
            - plain)
            .abs()
            < 1e-9,
        "a visible Tenno collects nothing from a while-Invisible mod"
    );

    let mut hidden = neutral.clone();
    hidden.state.invisible = true;
    let paid =
        resolve_for(&base, &[&ss], StackPolicy::AssumedMax, &hidden).modified_base;
    assert!(
        (paid - plain * 4.3).abs() < 1e-6,
        "an invisible Tenno collects +330%: {paid} vs {}",
        plain * 4.3
    );
}

/// ONE STAT DERIVED FROM THE OTHER, and the wiki hands over the arithmetic
/// to check it with. High Ground reads "Increase Base Critical Chance by 25%
/// of current Status Chance, up to 35%", and the card's own notes say how
/// much status chance maxing it takes — "+366.7%" for the non-Incarnon Dera
/// Vandal, "+536.4%" for the Incarnon Vandal and the non-Incarnon Dera.
///
/// Those two numbers test THREE things at once. 0.35/0.25 = 1.40 is the
/// current status chance the cap needs, so 0.30 x 4.667 and 0.22 x 6.364
/// both landing on 1.40 says (a) "CURRENT" is the MODDED value, (b) the rate
/// and cap are 0.25/0.35, and (c) this roster's base status chances are
/// 0.30 / 0.22 — which `data/weapons/` carries independently of the note.
#[test]
fn a_derived_stat_reads_the_modded_value_and_lands_on_the_base_one() {
    let hg = "dera_vandal_high_ground";
    let base = WeaponBase::from_data("dera_vandal", false, &[hg]);
    assert!(
        (base.base_status_chance - 0.30).abs() < 1e-9,
        "the wiki groups the non-Incarnon Vandal at 30% status chance, got {}",
        base.base_status_chance
    );
    let cc = |sc_bonus: f64| {
        let sm = m("t_sc", vec![ModEffect::StatusChance(sc_bonus)]);
        resolve(&base, &[&sm], StackPolicy::AssumedMax).crit_chance
    };
    // Just under the wiki's threshold the bonus is still climbing; AT it the
    // cap is exactly reached, and past it nothing more is bought.
    let (under, at, over) = (cc(3.60), cc(3.667), cc(6.0));
    assert!(under < at - 1e-9, "below +366.7% the bonus still climbs: {under} vs {at}");
    assert!((at - over).abs() < 1e-9, "+366.7% maxes it out: {at} vs {over}");

    // …and it lands on BASE crit chance, so the crit mods multiply it. The
    // two readings of "Base" only differ once a crit mod is on, which is
    // what this half pins: a 35% base grant through +150% is worth 87.5%.
    let bare = WeaponBase::from_data("dera_vandal", false, &[]);
    let sm = m("t_sc", vec![ModEffect::StatusChance(6.0)]);
    let cm = m("t_cc", vec![ModEffect::CritChance(1.5)]);
    let a = resolve(&bare, &[&sm, &cm], StackPolicy::AssumedMax).crit_chance;
    let b = resolve(&base, &[&sm, &cm], StackPolicy::AssumedMax).crit_chance;
    assert!(
        ((b - a) - 0.35 * 2.5).abs() < 1e-9,
        "a 35% BASE grant is worth 87.5% through a +150% crit mod, got {}",
        b - a
    );

    // THE CARD IS ONE SENTENCE. It was split into a flat "+25% base crit
    // chance" and an inert "of current Status Chance", so modelling the real
    // clause would have paid the perk twice.
    assert!(
        (a - resolve(&base, &[&cm], StackPolicy::AssumedMax).crit_chance).abs() > 1e-9,
        "sanity: the perk must do something"
    );
    let no_status = m("t_zero", vec![ModEffect::StatusChance(-1.0)]);
    assert!(
        (resolve(&base, &[&no_status, &cm], StackPolicy::AssumedMax).crit_chance
            - resolve(&bare, &[&no_status, &cm], StackPolicy::AssumedMax).crit_chance)
            .abs()
            < 1e-9,
        "at zero status chance the perk grants nothing — there is no flat half"
    );
}

/// WITH OVERSHIELDS — the first gated grant that is not a term added later.
///
/// VERBATIM (Paris_Incarnon_Genesis, Guardian's Might), columns Paris |
/// Mk1-Paris | Paris Prime:
///   *Increase Base Damage by '''+X'''.
///   *With Overshields: Increase Base Damage by '''+Y'''.
///   | X = 40<br>Y = 52  | X = 50<br>Y = 40  | X = 20<br>Y = 74
///
/// The assertion that matters is that the gate changes NOTHING about the
/// number's meaning: a Paris Prime holding overshields is the weapon a
/// plain "+74" perk would make. Both routes
/// THE ORIGINAL BASE IS AN ABSOLUTE, and this is the case that made it one: TWO FLAT-DAMAGE SOURCES THAT DISAGREE.
///
/// One ratio can describe one verdict — everything feeds, or nothing does
/// — so a build carrying a perk that feeds the CO term and one that does
/// not has no value it can take, which is never wrong in practice only
/// because the two flat-damage perks on a weapon are tier-mates. Built by
/// hand: a base of 100, one source of +50 that feeds and one of +30 that
/// does not, so the panel reads 180 and the CO term reads 150.
#[test]
fn two_flat_sources_that_disagree_each_land_where_they_should() {
    let mut b = WeaponBase::from_data("braton", false, &[]);
    let mut v = DamageVector::new();
    v.add(DamageType::Impact, 100.0);
    b.base_vector = v;
    b.co_base = 100.0;
    b.radial = None;

    b.add_flat_base_damage(50.0, 50.0); // feeds
    b.add_flat_base_damage(30.0, 0.0); // does not

    assert_eq!(b.base_vector.total(), 180.0);
    assert_eq!(b.co_base, 150.0);
    assert!((b.co_base_fraction() - 150.0 / 180.0).abs() < 1e-12);

    // …AND NEITHER RATIO ALONE DESCRIBES IT. `original/evolved` over the
    // whole build is 100/180 and over the feeding source is 150/180; the
    // truth is the second, and the old code could only have reached it by
    // knowing which sources to leave out of a division it performed once.
    assert!((b.co_base_fraction() - 100.0 / 180.0).abs() > 0.2);
}

/// …AND THE ORDER THEY ARRIVE IN DOES NOT MATTER, which is what makes the
/// absolute safe to accumulate. The panel folds pro-rata either way and the
/// CO base is a sum.
#[test]
fn the_original_base_does_not_depend_on_the_order_of_the_sources() {
    let build = |a: (f64, f64), c: (f64, f64)| {
        let mut b = WeaponBase::from_data("braton", false, &[]);
        let mut v = DamageVector::new();
    v.add(DamageType::Impact, 100.0);
    b.base_vector = v;
        b.co_base = 100.0;
        b.radial = None;
        b.add_flat_base_damage(a.0, a.1);
        b.add_flat_base_damage(c.0, c.1);
        (b.base_vector.total(), b.co_base)
    };
    assert_eq!(build((50.0, 50.0), (30.0, 0.0)), build((30.0, 0.0), (50.0, 50.0)));
}

/// call `WeaponBase::add_flat_base_damage`, and this is what says so.
#[test]
fn a_gated_flat_base_damage_folds_exactly_as_an_ungated_one() {
    let neutral = crate::data::tenno::default_tenno();
    assert!(!neutral.state.overshields, "the default player has none");
    let shielded = tenno_who(|s| s.overshields = true);

    let base = WeaponBase::from_data("paris_prime", true, &["paris_prime_guardians_might"]);
    let off = resolve_for(&base, &[], StackPolicy::AssumedMax, neutral);
    let on = resolve_for(&base, &[], StackPolicy::AssumedMax, &shielded);
    assert!(on.modified_base > off.modified_base,
        "overshields are worth +74 base: {} vs {}", on.modified_base, off.modified_base);

    // THE REFERENCE: the same weapon with the whole +94 as one plain perk.
    // 20 + 74 is what the card pays a player who has them, so a base panel
    // carrying that flat outright must resolve to the same numbers.
    let mut plain = WeaponBase::from_data("paris_prime", true, &[]);
    plain.add_flat_base_damage(20.0 + 74.0, 0.0);
    let want = resolve_for(&plain, &[], StackPolicy::AssumedMax, neutral);
    assert!((on.modified_base - want.modified_base).abs() < 1e-9,
        "gated {} vs plain {}", on.modified_base, want.modified_base);
    // …and the COMPOSITION, not just the total: a pro-rata scale leaves the
    // shares untouched, and getting that wrong moves every status payload
    // while the damage total still reads right.
    assert!((on.damage.total() - want.damage.total()).abs() < 1e-9);
    for ty in crate::rules::damage::DamageType::ALL {
        assert!((on.damage.get(ty) - want.damage.get(ty)).abs() < 1e-9,
            "{ty:?}: gated {} vs plain {}", on.damage.get(ty), want.damage.get(ty));
    }

    // A GATE THAT IS SHUT COSTS NOTHING. The unshielded panel is the weapon
    // with only the perk's unconditional +20 — which is what the card says,
    // and what a build that never picks up an overshield actually gets.
    let mut just_x = WeaponBase::from_data("paris_prime", true, &[]);
    just_x.add_flat_base_damage(20.0, 0.0);
    let x_only = resolve_for(&just_x, &[], StackPolicy::AssumedMax, neutral);
    assert!((off.modified_base - x_only.modified_base).abs() < 1e-9,
        "shut: {} vs {}", off.modified_base, x_only.modified_base);

    // …AND THE CO BASE IS WHAT "EXACTLY" MEANS HERE. `modified_base` and the
    // vector agree under either answer, which is how a gate that moved the
    // CO base rode along unseen: the absolute the term reads is the ONLY
    // number the overshield may not touch (MEASUREMENTS M83).
    let reads = |p: &ResolvedPanel| p.co_base.fraction() * p.co_base.of();
    assert!((reads(&on) - reads(&off)).abs() < 1e-9,
        "the gate moved the CO base: {} shielded vs {}", reads(&on), reads(&off));
    assert!((reads(&on) - 0.5 * 360.0).abs() < 1e-9,
        "the catalog's half-base survives the gate: {}", reads(&on));
    // The share SHRINKS as the panel grows, because the absolute does not.
    assert!(on.co_base_fraction() < off.co_base_fraction());
}

/// THE FURIS'S CO READS ITS OWN BASE IN BOTH FORMS, and both halves of
/// Haven Foray — the +28 and the overshield's +30 — stay out of it (M101).
/// +220% base damage, Galvanized Strike, overshields up. The base form's
/// 3 / 14 / 3 lands on whole units and the Incarnon is all Heat, so
/// quantization is exact here and the bracket IS the reading.
#[test]
fn furis_co_reads_its_own_base_under_haven_foray_in_both_forms() {
    let shielded = tenno_who(|s| s.overshields = true);
    for (id, own, readings) in [
        ("furis", 20.0, &[(3.0, 2.0, 298.0), (2.0, 2.0, 282.0), (1.0, 2.0, 266.0)][..]),
        ("furis_incarnon", 100.0, &[(3.0, 1.0, 626.0)][..]),
    ] {
        let base = WeaponBase::from_data(id, false, &["furis_haven_foray"]);
        let p = resolve_for(&base, &[], StackPolicy::AssumedMax, &shielded);
        let panel = p.modified_base;
        assert!((panel - (own + 28.0 + 30.0)).abs() < 1e-9, "{id}: panel {panel}");
        let co = p.co_base.fraction() * p.co_base.of();
        assert!((co - own).abs() < 1e-9, "{id}: CO reads {co}");
        for &(stacks, types, measured) in readings {
            let got = panel * (1.0 + 2.2) + 0.4 * stacks * types * co;
            assert!((got - measured).abs() < 0.5, "{id} {stacks}x{types}: {got} against {measured}");
        }
    }
}

/// THE SAME WEAPON ANSWERS THE CO QUESTION TWO OPPOSITE WAYS, and one
/// build reads both (MEASUREMENTS M83, M84).
///
/// Paris Prime, +155% base damage, Galvanized Aptitude at 2 stacks,
/// Guardian's Might and Striking Succession at its 4-stack cap. The base
/// form is `Adding` on HALF its base and every flat add — the gated +74
/// included — stays out of it; the Incarnon form is `Multiplying` on ALL of
/// its evolved base, so the very same adds feed in full.
///
/// QUANTIZATION IS A FLAT FACTOR HERE and each form has its own, which is
/// the whole of the residual between the bracket and the pop-up: the
/// charged shot's 2.5 / 17.5 / 80 lands on 0.8 + 5.6 + 25.6 units and
/// rounds to 33 for a x1.03125 gain, while the Incarnon's 100 / 420 lands
/// on 6.15 + 25.85 and rounds back to 32 for nothing at all. It cannot be
/// left out and called a target multiplier — the two forms would then need
/// two different targets in one session.
#[test]
fn paris_prime_reads_its_co_base_one_way_charged_and_the_other_incarnon() {
    let neutral = crate::data::tenno::default_tenno();
    let shielded = tenno_who(|s| s.overshields = true);
    let evos = ["paris_prime_guardians_might", "paris_prime_striking_succession"];
    let (serration, galvanized) = (1.55f64, 0.4 * 2.0);

    let hit = |id: &str, is_base: bool, t: &crate::data::tenno::Tenno, types: f64| -> f64 {
        let base = WeaponBase::from_data(id, is_base, &evos);
        let p = resolve_for(&base, &[], StackPolicy::AssumedMax, t);
        // The panel with the gate answered, before a damage mod goes in.
        let unmodded = p.modified_base;
        let striking = 4.0 * 15.0 * (1.0 + serration) / unmodded;
        let co = galvanized * types * p.co_base.fraction();
        // `Adding` puts the CO term in the base bucket, so it is inside the
        // number quantization divides by; `Multiplying` applies after it
        // (M81).
        let bracket = if is_base { 1.0 + serration + striking + co } else { 1.0 + serration + striking };
        let modded_base = unmodded * bracket;
        let quantized = p
            .damage
            .scale(modded_base / p.damage.total())
            .quantized_against(modded_base);
        quantized.total() * if is_base { 1.0 } else { 1.0 + co }
    };

    for (id, is_base, t, types, measured) in [
        ("paris_prime", true, &shielded, 1.0, 1501.0),
        ("paris_prime", true, neutral, 1.0, 1306.0),
        ("paris_prime_incarnon", false, &shielded, 1.0, 3095.0),
        ("paris_prime_incarnon", false, &shielded, 2.0, 4470.0),
    ] {
        let got = hit(id, is_base, t, types);
        assert!(
            (got - measured).abs() / measured < 0.002,
            "{id} at {types} status types: {got:.1} against a measured {measured}"
        );
    }

    // …AND THE TWO BASES ARE THE CLAIM. Half the charged shot's own 360,
    // untouched by 94 points of flat add; the whole of the Incarnon's
    // evolved 614, which is those same adds inside it.
    let reads = |id: &str, is_base: bool| {
        let base = WeaponBase::from_data(id, is_base, &evos);
        let p = resolve_for(&base, &[], StackPolicy::AssumedMax, &shielded);
        p.co_base.fraction() * p.co_base.of()
    };
    assert!((reads("paris_prime", true) - 180.0).abs() < 1e-9);
    assert!((reads("paris_prime_incarnon", false) - 614.0).abs() < 1e-9);
}

/// EVERY CARD THAT ASKS ABOUT OVERSHIELDS, and what each one pays.
///
/// Ten cards across four Genesis families, and the numbers are transcribed
/// per VARIANT because the wiki prints them per variant — the column order
/// comes from each page's own table header, which is the part that is easy
/// to get backwards and impossible to notice afterwards.
///
///   Angstrum | Prisma Angstrum          one colspan="2" cell, both +50
///   Lato | Lato Vandal | Lato Prime      +40 / +40 / +34
///   Paris | Mk1-Paris | Paris Prime      +52 / +40 / +74
///   Furis | Mk1-Furis                    +30 written into the bullet, both
///
/// Asserted as the DIFFERENCE the state makes, so it reads as the card
/// does: tick overshields, gain exactly this much base damage.
#[test]
fn every_overshield_card_pays_the_number_on_its_own_variant() {
    let roster: &[(&str, &str, f64)] = &[
        ("angstrum", "angstrum_haven_foray", 50.0),
        ("prisma_angstrum", "prisma_angstrum_haven_foray", 50.0),
        ("lato", "lato_haven_foray", 40.0),
        ("lato_vandal", "lato_vandal_haven_foray", 40.0),
        ("lato_prime", "lato_prime_haven_foray", 34.0),
        ("paris", "paris_guardians_might", 52.0),
        ("mk1_paris", "mk1_paris_guardians_might", 40.0),
        ("paris_prime", "paris_prime_guardians_might", 74.0),
        ("furis", "furis_haven_foray", 30.0),
        ("mk1_furis", "mk1_furis_haven_foray", 30.0),
    ];
    let neutral = crate::data::tenno::default_tenno();
    let shielded = tenno_who(|s| s.overshields = true);
    for (weapon, evo, want) in roster {
        let base = WeaponBase::from_data(weapon, true, &[evo]);
        let off = resolve_for(&base, &[], StackPolicy::AssumedMax, neutral);
        let on = resolve_for(&base, &[], StackPolicy::AssumedMax, &shielded);
        // The panel's modified base is the weapon's total through the
        // damage bucket, and with no mods that bucket is 1 — so the
        // difference IS the card's number.
        let got = on.modified_base - off.modified_base;
        assert!((got - want).abs() < 1e-6,
            "{evo}: overshields are worth {want} on the card, the panel moved by {got}");
    }
    // …and the roster is CLOSED: exactly these ten ask, so a new card that
    // spells the condition some other way fails here rather than paying
    // nothing in silence.
    let asking: Vec<&str> = crate::data::evolutions::pool()
        .iter()
        .filter(|e| {
            WeaponBase::from_data(&e.weapon, true, &[e.id.as_str()])
                .gated
                .iter()
                .any(|t| t.gate == TennoCondition::Overshields)
        })
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(asking.len(), roster.len(),
        "cards asking about overshields: {asking:?}");
}

/// LONE GUN — the first perk that asks about the LOADOUT, and the first
/// gated grant that is not damage.
///
/// VERBATIM (Vasto_Incarnon_Genesis, EVO2 Perk 1):
///   `* Increase Base Damage by '''+X'''.`
///   `* With No Primary Equipped:`
///   `** Increase Base Damage by '''+40'''`
///   `** Increase Base Magazine Capacity by '''+14'''.`
///   `| X = 66   | X = 24`        (Vasto | Vasto Prime)
///   `* Increased Base Magazine Capacity does not affect Incarnon Form.`
///
/// So the conditional half is NOT per variant — only X is — which is the
/// part a per-variant transcription gets wrong by habit.
///
/// "The Tenno walks in with a full loadout" is the DEFAULT and only the
/// default: the scenario says which, and a shut gate costs exactly nothing,
/// which is what keeps every board row meaning what
/// it meant.
#[test]
fn lone_gun_pays_its_two_halves_only_with_no_other_weapon() {
    let solo = tenno_who(|s| s.solo_weapon = true);
    let neutral = crate::data::tenno::default_tenno();
    for (weapon, evo) in [("vasto", "vasto_lone_gun"), ("vasto_prime", "vasto_prime_lone_gun")]
    {
        let base = WeaponBase::from_data(weapon, true, &[evo]);
        let off = resolve_for(&base, &[], StackPolicy::AssumedMax, neutral);
        let on = resolve_for(&base, &[], StackPolicy::AssumedMax, &solo);
        // With no mods the damage bucket is 1, so the difference IS the
        // card's number — the same reading the overshield roster uses.
        let dmg = on.modified_base - off.modified_base;
        assert!((dmg - 40.0).abs() < 1e-6, "{evo}: base damage moved by {dmg}, card says +40");
        let mag = on.magazine_size - off.magazine_size;
        assert!((mag - 14.0).abs() < 1e-9, "{evo}: magazine moved by {mag}, card says +14");

        // A SHUT GATE COSTS NOTHING: the weapon is its plain +X and nothing
        // else, which is the fight the board is scored under.
        let x = crate::data::evolutions::get(evo).expect(evo).flat_base_damage();
        let mut just_x = WeaponBase::from_data(weapon, true, &[]);
        just_x.add_flat_base_damage(x, x);
        let x_only = resolve_for(&just_x, &[], StackPolicy::AssumedMax, neutral);
        assert!((off.modified_base - x_only.modified_base).abs() < 1e-9,
            "{evo} shut: {} vs {}", off.modified_base, x_only.modified_base);
        assert!((off.magazine_size - x_only.magazine_size).abs() < 1e-9,
            "{evo} shut: magazine {} vs {}", off.magazine_size, x_only.magazine_size);
    }

    // "Increased Base Magazine Capacity does not affect Incarnon Form" —
    // and the DAMAGE half still does, which is why this is asserted on the
    // same panel rather than as "the perk is off in Incarnon Form".
    for (form, evo) in
        [("vasto_incarnon", "vasto_lone_gun"), ("vasto_prime_incarnon", "vasto_prime_lone_gun")]
    {
        let base = WeaponBase::from_data(form, true, &[evo]);
        let off = resolve_for(&base, &[], StackPolicy::AssumedMax, neutral);
        let on = resolve_for(&base, &[], StackPolicy::AssumedMax, &solo);
        assert!((on.magazine_size - off.magazine_size).abs() < 1e-9,
            "{form}: the Incarnon magazine must not move, it went {} -> {}",
            off.magazine_size, on.magazine_size);
        assert!(on.modified_base > off.modified_base,
            "{form}: the damage half still pays in Incarnon Form");
    }

    // …and the roster is CLOSED. Exactly these two cards ask about the
    // loadout, so a third that spells it some other way fails here rather
    // than paying nothing in silence — the same guard the overshield roster
    // carries, and the reason it is worth carrying: the option exists to
    // make these clauses reachable, so one that quietly is not is the whole
    // failure.
    let asking: Vec<&str> = crate::data::evolutions::pool()
        .iter()
        .filter(|e| {
            WeaponBase::from_data(&e.weapon, true, &[e.id.as_str()])
                .gated
                .iter()
                .any(|t| t.gate == TennoCondition::SoloWeapon)
        })
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(asking, ["vasto_lone_gun", "vasto_prime_lone_gun"],
        "cards asking about the loadout: {asking:?}");
}

/// A FIGHT BONUS IS ONE MORE MOD, and that is the whole claim.
///
/// Asserted as an EQUALITY against the real card rather than as a
/// direction: a scenario's +165% base damage has to resolve to the same
/// panel Serration does, or "like a mod" is a description of the UI and not
/// of the arithmetic. Nine buckets, each checked against the mod that owns
/// it, so a bonus wired to the wrong local fails on the stat it landed in.
#[test]
fn a_fight_bonus_resolves_exactly_like_the_mod_of_that_stat() {
    let base = WeaponBase::from_data("torid", true, &[]);
    let pool = crate::data::mods::class_pool("rifle");
    let by = |id: &str| {
        pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id} missing"))
    };
    let neutral = crate::data::tenno::default_tenno();
    let with_bonus = |f: fn(&mut crate::data::tenno::StatBonuses)| {
        let mut t = neutral.clone();
        f(&mut t.bonuses);
        resolve_for(&base, &[], StackPolicy::Emergent, &t)
    };
    let with_mod = |id: &str| {
        resolve_for(&base, &[by(id)], StackPolicy::Emergent, neutral)
    };
    // (the mod, the bucket setter, and what to read off it)
    type Case = (&'static str, fn(&mut crate::data::tenno::StatBonuses), fn(&ResolvedPanel) -> f64);
    let cases: &[Case] = &[
        ("serration", |b| b.base_damage = 1.65, |p| p.modified_base),
        ("split_chamber", |b| b.multishot = 0.90, |p| p.multishot),
        ("point_strike", |b| b.crit_chance = 1.50, |p| p.crit_chance),
        ("vital_sense", |b| b.crit_damage = 1.20, |p| p.crit_damage),
        ("rifle_aptitude", |b| b.status_chance = 0.90, |p| p.status_chance),
        ("speed_trigger", |b| b.fire_rate = 0.60, |p| p.fire_rate),
        ("magazine_warp", |b| b.magazine = 0.30, |p| p.magazine_size),
        ("continuous_misery", |b| b.status_duration = 1.0, |p| p.status_duration_multiplier),
    ];
    for (id, set, read) in cases {
        let a = read(&with_mod(id));
        let b = read(&with_bonus(*set));
        assert!((a - b).abs() < 1e-9,
            "{id}: the mod resolves to {a}, the same number as a fight bonus resolves to {b}");
    }
    // RELOAD IS THE OTHER DIRECTION — a bigger bucket is a SHORTER time —
    // so it is read separately rather than being one more row above.
    let m = with_mod("fast_hands").reload_seconds;
    let f = with_bonus(|b| b.reload_speed = 0.30).reload_seconds;
    assert!((m - f).abs() < 1e-9, "fast hands {m}s vs a fight bonus {f}s");

    // …AND THEY ADD, which is what "one more mod" means when there is
    // already one: Serration + a +165% fight bonus is one bucket at +330%,
    // never two multipliers.
    let mut both = neutral.clone();
    both.bonuses.base_damage = 1.65;
    let stacked = resolve_for(&base, &[by("serration")], StackPolicy::Emergent, &both);
    let plain = resolve_for(&base, &[], StackPolicy::Emergent, neutral);
    assert!((stacked.modified_base / plain.modified_base - 4.30).abs() < 1e-9,
        "1 + 1.65 + 1.65 = 4.30, got x{}", stacked.modified_base / plain.modified_base);

    // …AND A LOCK STILL WINS. "Set to its default ignoring other bonuses"
    // makes no exception for where a bonus came from, and the fight is not
    // a loophole in a rule the mods obey.
    let mut multishot = neutral.clone();
    multishot.bonuses.multishot = 5.0;
    let locked = resolve_for(&base, &[by("primary_acuity")], StackPolicy::Emergent, &multishot);
    let unlocked = resolve_for(&base, &[], StackPolicy::Emergent, neutral);
    assert!((locked.multishot - unlocked.multishot).abs() < 1e-9,
        "a locked multishot ignores a fight bonus too: {} vs {}",
        locked.multishot, unlocked.multishot);

    // A NEGATIVE BONUS IS A MALUS CARD: -87.5% status duration leaves an
    // eighth, and it subtracts from a mod rather than scaling it.
    let mut short = neutral.clone();
    short.bonuses.status_duration = -0.875;
    let p = resolve_for(&base, &[by("continuous_misery")], StackPolicy::Emergent, &short);
    assert!((p.status_duration_multiplier - 1.125).abs() < 1e-9, "{}", p.status_duration_multiplier);

    // FLAT CRIT IS POINTS AFTER MODS, not a bucket: +25 lands as +0.25 on
    // the bare panel and on Point Strike's alike, and -25 takes it back.
    let cc = |mods: &[&ModDef], flat: f64| {
        let mut t = neutral.clone();
        t.bonuses.flat_crit_chance = flat;
        resolve_for(&base, mods, StackPolicy::Emergent, &t).crit_chance
    };
    let ps = [by("point_strike")];
    for mods in [&[][..], &ps[..]] {
        assert!((cc(mods, 0.25) - cc(mods, 0.0) - 0.25).abs() < 1e-9);
        assert!((cc(mods, -0.25) - (cc(mods, 0.0) - 0.25).max(0.0)).abs() < 1e-9);
    }
}

/// WITH A CHANNELED ABILITY ACTIVE — the second player-declared state, and
/// the family where the conditional half is the BIGGER one.
///
/// VERBATIM (Braton_Incarnon_Genesis, Daring Reverie):
///   * Increase Base Damage by '''+X'''.
///   * With [[Channeled Abilities|Channeled Ability]] active: Increase Base
///     Damage by '''+Y'''. '''+50%''' Ammo Efficiency
///     | X = 24<br>Y = 30 | X = 28<br>Y = 22 | X = 12<br>Y = 34 | X = 4<br>Y = 38
///
/// THE COLUMNS COME FROM THE TABLE HEADER — Braton | Mk1-Braton | Braton
/// Vandal | Braton Prime — and NOT from the page's opening sentence, which
/// lists the same four in a different order. Reading the sentence makes
/// three of the four look wrong, so the mapping is pinned here.
#[test]
fn every_channeled_ability_card_pays_the_number_on_its_own_variant() {
    let roster: &[(&str, &str, f64, f64)] = &[
        ("braton", "braton_daring_reverie", 24.0, 30.0),
        ("mk1_braton", "mk1_braton_daring_reverie", 28.0, 22.0),
        ("braton_vandal", "braton_vandal_daring_reverie", 12.0, 34.0),
        ("braton_prime", "braton_prime_daring_reverie", 4.0, 38.0),
    ];
    let neutral = crate::data::tenno::default_tenno();
    assert!(!neutral.state.channeling, "the default player is casting nothing");
    let channeling = tenno_who(|s| s.channeling = true);
    for (weapon, evo, x, y) in roster {
        let bare = WeaponBase::from_data(weapon, true, &[]);
        let with = WeaponBase::from_data(weapon, true, &[evo]);
        let off = resolve_for(&with, &[], StackPolicy::AssumedMax, neutral);
        let on = resolve_for(&with, &[], StackPolicy::AssumedMax, &channeling);
        let plain = resolve_for(&bare, &[], StackPolicy::AssumedMax, neutral);
        // The unconditional half is X…
        assert!((off.modified_base - plain.modified_base - x).abs() < 1e-6,
            "{evo}: the unconditional half is +{x}, panel moved by {}",
            off.modified_base - plain.modified_base);
        // …and ticking the state adds Y on top of it.
        assert!((on.modified_base - off.modified_base - y).abs() < 1e-6,
            "{evo}: a channeled ability is worth +{y}, panel moved by {}",
            on.modified_base - off.modified_base);
    }
    // THE CONDITIONAL HALF IS THE BIGGER ONE for three of the four, which is
    // the fact a player needs before reading an unticked Braton as its
    // ceiling — and a sign-flipped transcription would break it.
    assert_eq!(roster.iter().filter(|(_, _, x, y)| y > x).count(), 3);
}

/// A FORM THAT CANNOT ZOOM CANNOT BE AIMING, so every aim-gated bonus pays
/// nothing in it — and the SAME WEAPON's base form is unaffected.
///
/// VERBATIM (Vasto_Incarnon_Genesis): "Incarnon Form transforms into a
/// 6-round burst with '''6''' base [[multishot]] … has significantly higher
/// [[Recoil]], and cannot [[Zoom]]."
///
/// "Zoom" is the wiki's word for the aim STATE — its page opens "Zoom (or
/// aiming, aiming down sights (ADS))" and the Galvanized mods write the
/// condition as `[[Zoom|aiming]]` — and DE settled the consequence in a
/// patch note about Mesa's Regulators: the buffs "never actually applied
/// due to the 'on aim' criteria not being fulfilled".
///
/// The scenario is left ALONE. A player who ticks "aiming" is not corrected
/// and their other weapons still aim; the form answers the question for
/// itself, which is why this is on the weapon and not on the Tenno.
#[test]
fn a_form_that_cannot_zoom_pays_no_aim_gated_bonus() {
    use crate::data::mods::class_pool;
    let pool = class_pool("pistol");
    let gc = pool.iter().find(|m| m.id == "galvanized_crosshairs")
        .expect("galvanized_crosshairs is in the pistol pool");
    let aiming = crate::data::tenno::default_tenno();
    assert!(aiming.state.aiming, "the default player aims");
    let hipfire = tenno_who(|s| s.aiming = false);

    let cc = |id: &str, t: &crate::data::tenno::Tenno| {
        let base = WeaponBase::from_data(id, false, &[]);
        resolve_for(&base, &[gc], StackPolicy::AssumedMax, t).crit_chance
    };

    // THE BASE FORM is an ordinary weapon: aiming is worth something and
    // hipfiring is not.
    let (base_aim, base_hip) = (cc("vasto_prime", aiming), cc("vasto_prime", &hipfire));
    assert!(base_aim > base_hip,
        "the base form pays the aim mod: {base_aim} vs {base_hip}");

    // THE INCARNON FORM cannot zoom, so the two are the same number — the
    // mod is equipped, resolves, and grants nothing.
    let (inc_aim, inc_hip) = (cc("vasto_prime_incarnon", aiming),
                              cc("vasto_prime_incarnon", &hipfire));
    assert!((inc_aim - inc_hip).abs() < 1e-9,
        "cannot Zoom means the aim mod pays nothing: aiming {inc_aim}, hipfire {inc_hip}");

    // …and it is the FLAG doing it, not the weapon happening to ignore crit
    // mods: a weapon whose form CAN zoom still pays.
    let (lex_aim, lex_hip) = (cc("lex_prime_incarnon", aiming),
                              cc("lex_prime_incarnon", &hipfire));
    assert!(lex_aim > lex_hip,
        "an Incarnon form that CAN zoom still pays it: {lex_aim} vs {lex_hip}");

    // THE ROSTER IS CLOSED at the two Vastos — the only "cannot Zoom" in the
    // whole Incarnon Evolutions page. The four "-30% Zoom" perks there cut
    // magnification and leave the aim state alone, so they must not appear.
    let flagged: Vec<&str> = crate::data::weapons::all().iter()
        .filter(|w| w.cannot_zoom).map(|w| w.id.as_str()).collect();
    assert_eq!(flagged, vec!["vasto_incarnon", "vasto_prime_incarnon"], "{flagged:?}");
}

/// A SCOPE GRANTS ONE OF THREE THINGS, AND THEY LAND IN THREE PLACES.
///
/// The Vectis family's zoom gives headshot damage, the Rubico's a critical
/// MULTIPLIER, the Lanka's a critical CHANCE — and the Lanka's is the one
/// the mechanic page calls an exception: *"The zoom bonus adds a flat
/// +20/30/50 critical chance, applied after mods"*. That is a different
/// layer from the other two. A relative +50% on the Lanka's 25% base is
/// five points; the flat one is fifty, and putting it in the ordinary
/// bucket would have understated the weapon by an order of magnitude.
#[test]
fn a_scopes_three_kinds_land_in_three_buckets() {
    let refs: Vec<&ModDef> = Vec::new();
    let mut hip = crate::data::tenno::default_tenno().clone();
    hip.state.aiming = false;
    let pair = |id: &str| {
        let b = WeaponBase::from_data(id, false, &[]);
        (
            resolve(&b, &refs, StackPolicy::Emergent),
            resolve_for(&b, &refs, StackPolicy::Emergent, &hip),
        )
    };

    // THE RUBICO: +50% critical multiplier, relative, so 3.0x -> 4.5x.
    let (aim, no) = pair("rubico");
    assert!((no.crit_damage - 3.0).abs() < 1e-9, "hip: {}", no.crit_damage);
    assert!((aim.crit_damage - 4.5).abs() < 1e-9, "scoped: {}", aim.crit_damage);

    // THE LANKA: fifty POINTS of crit chance, not half of its 25%.
    let (aim, no) = pair("lanka");
    assert!((no.crit_chance - 0.25).abs() < 1e-9, "hip: {}", no.crit_chance);
    assert!(
        (aim.crit_chance - 0.75).abs() < 1e-9,
        "the Lanka's scope is a FLAT +50 applied after mods, so 25% becomes 75%              and not 37.5%: {}",
        aim.crit_chance
    );

    // THE VULKAR: +70% headshot damage, the Vectis family's kind.
    let (aim, no) = pair("vulkar");
    assert!((aim.headshot_damage_bonus - no.headshot_damage_bonus - 0.7).abs() < 1e-9);
    // ...and none of the three touches a weapon without a scope.
    let (aim, no) = pair("torid");
    assert!((aim.crit_chance - no.crit_chance).abs() < 1e-9);
    assert!((aim.crit_damage - no.crit_damage).abs() < 1e-9);
}

/// BOTH SNIPER MECHANICS ARE THE SCOPE'S. *"Building combo and benefiting
/// from its multiplier requires being scoped in"* (wiki `Sniper Rifle`),
/// and the zoom buff is a property of a zoom level — so a hip-fired
/// scenario gets neither, and it is `resolve` that says so, once, for the
/// simulator and the optimizer alike.
#[test]
fn a_sniper_fired_from_the_hip_has_no_combo_and_no_scope() {
    let base = WeaponBase::from_data("vectis_prime", false, &[]);
    let refs: Vec<&ModDef> = Vec::new();
    let mut hip = crate::data::tenno::default_tenno().clone();
    hip.state.aiming = false;

    let aimed = resolve(&base, &refs, StackPolicy::Emergent);
    let from_hip = resolve_for(&base, &refs, StackPolicy::Emergent, &hip);

    assert_eq!(aimed.sniper_combo.map(|c| c.min), Some(5), "scoped in, it has one");
    assert!(from_hip.sniper_combo.is_none(), "from the hip it has none");
    // ...and the scope's headshot bonus travels with it, in the additive
    // bracket the wiki puts it in.
    assert!(
        (aimed.headshot_damage_bonus - from_hip.headshot_damage_bonus - 0.6).abs() < 1e-12,
        "the scope is worth +60% headshot damage and only while aiming: {} vs {}",
        aimed.headshot_damage_bonus,
        from_hip.headshot_damage_bonus
    );
}

/// A DERIVED STAT READS THE FORM IT IS ON — which is the in-mission
/// behaviour, and NOT the one the Arsenal shows.
///
/// VERBATIM (Sicarus_Incarnon_Genesis, Wiseman's Regard):
///   * Incarnon Form Status Chance is displayed in the [[Arsenal]] screen as
///     if using the normal form's Critical Chance, but will properly use
///     its' own Critical chance while in-mission.
///
/// So the panel must NOT reproduce the Arsenal's bug: each form's status
/// chance comes from THAT form's crit chance. Free here, because a form is
/// its own weapon entry and resolves its own panel — which is exactly why
/// it is worth pinning, since nothing else would notice if it stopped.
///
/// FOUR NUMBERS OUT OF ONE SENTENCE: "achievable with '''+734%''' (Sicarus)
/// / '''+434%''' (Prime) modded Critical Chance, or '''+567%''' / '''+345%'''
/// in Incarnon Form". The cap needs 0.40/0.30 = 1.3333 current crit, so each
/// threshold implies that form's BASE crit chance — 1.3333/8.34 = 0.160,
/// /6.67 = 0.200, /5.34 = 0.250, /4.45 = 0.300 — which `data/weapons/` was
/// written from the weapon pages to match.
#[test]
fn wisemans_regard_reads_each_forms_own_crit_chance() {
    let rows: &[(&str, &str, f64, f64)] = &[
        // (entry, perk, that form's base crit, the wiki's "+X%" threshold)
        ("sicarus", "sicarus_wisemans_regard", 0.16, 7.34),
        ("sicarus_incarnon", "sicarus_wisemans_regard", 0.20, 5.67),
        ("sicarus_prime", "sicarus_prime_wisemans_regard", 0.25, 4.34),
        ("sicarus_prime_incarnon", "sicarus_prime_wisemans_regard", 0.30, 3.45),
    ];
    for (id, perk, base_cc, threshold) in rows {
        let bare = WeaponBase::from_data(id, false, &[]);
        let with = WeaponBase::from_data(id, false, &[perk]);
        let b = resolve(&bare, &[], StackPolicy::AssumedMax);
        let w = resolve(&with, &[], StackPolicy::AssumedMax);

        assert!((w.crit_chance - base_cc).abs() < 1e-9,
            "{id}: the wiki's +{:.0}% threshold implies a base crit of {base_cc}, ours is {}",
            threshold * 100.0, w.crit_chance);
        // THE CONVERSION IS OFF THIS FORM'S OWN NUMBER, not the base form's.
        let got = w.status_chance - b.status_chance;
        assert!((got - 0.30 * base_cc).abs() < 1e-9,
            "{id}: 30% of {base_cc} is {}, the panel moved by {got}", 0.30 * base_cc);
        // …and the threshold reproduces the cap, which is the other half of
        // the sentence: at +X% modded crit the conversion is exactly 40%.
        let capped = (0.30 * base_cc * (1.0 + threshold)).min(0.40);
        assert!((capped - 0.40).abs() < 0.002,
            "{id}: at +{:.0}% the conversion should sit on the 40% cap, got {capped}",
            threshold * 100.0);
    }

    // THE ARSENAL'S BUG, as a negative control: the Incarnon form's answer
    // must NOT equal what the base form's crit would give it. On the Prime
    // that is 0.090 against 0.075, so a regression to the base form's
    // number is a visible 1.5-point difference and not a rounding one.
    let inc = resolve(
        &WeaponBase::from_data("sicarus_prime_incarnon", false,
            &["sicarus_prime_wisemans_regard"]), &[], StackPolicy::AssumedMax);
    let inc_bare = resolve(
        &WeaponBase::from_data("sicarus_prime_incarnon", false, &[]),
        &[], StackPolicy::AssumedMax);
    let base_form_would_give = 0.30 * 0.25;
    assert!(((inc.status_chance - inc_bare.status_chance) - base_form_would_give).abs() > 1e-6,
        "the Incarnon form is using the BASE form's crit chance — the Arsenal's bug");
}

/// Satisfying `aiming` silently fires every aim-gated buff whether or
/// not the scenario implies aiming. `resolve_with(.., aiming)` is the knob;
/// `resolve` assumes aim.
#[test]
fn aiming_gates_the_while_aiming_effects_and_only_those() {
    use crate::data::mods::class_pool;
    let pool = class_pool("pistol");
    let by = |id: &str| pool.iter().find(|m| m.id == id).expect(id);
    let base = WeaponBase::from_data("dual_toxocyst", true, &[]);
    let hipfire = tenno_who(|s| s.aiming = false);

    // Galvanized Crosshairs is entirely aim-gated: dropping aim must
    // remove its whole crit-chance contribution.
    let gc = vec![by("galvanized_crosshairs")];
    let aimed = resolve(&base, &gc, StackPolicy::AssumedMax);
    let hip = resolve_for(&base, &gc, StackPolicy::AssumedMax, &hipfire);
    assert!(
        aimed.crit_chance > hip.crit_chance,
        "aim-gated crit must vanish: {} vs {}",
        aimed.crit_chance,
        hip.crit_chance
    );
    let bare = resolve_for(&base, &[], StackPolicy::AssumedMax, &hipfire);
    assert!(
        (hip.crit_chance - bare.crit_chance).abs() < 1e-12,
        "with no aim the mod contributes NOTHING, not something reduced"
    );

    // An UNGATED mod is untouched by the flag - the wrapper must not leak.
    let hs = vec![by("hornet_strike")];
    let a2 = resolve(&base, &hs, StackPolicy::AssumedMax);
    let h2 = resolve_for(&base, &hs, StackPolicy::AssumedMax, &hipfire);
    assert!(
        (a2.modified_base - h2.modified_base).abs() < 1e-12,
        "Hornet Strike does not care about aiming"
    );

    // And the plain entry point still assumes aim (30 callers rely on it).
    let legacy = resolve(&base, &gc, StackPolicy::AssumedMax);
    assert!((legacy.crit_chance - aimed.crit_chance).abs() < 1e-12);
}

#[test]
fn requires_gate_disables_and_cond_buff() {
    let base = WeaponBase::from_data("dual_toxocyst", true, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]); // traits: semi_auto
    let p0 = resolve(&base, &[], StackPolicy::AssumedMax);
    // requires: a mod needing `beam` is INERT on Dual Toxocyst (no beam);
    // a mod needing `semi_auto` applies.
    let beam_mod = m_req("beam", Some("beam"), vec![], vec![ModEffect::BaseDamage(3.0)]);
    assert!((resolve(&base, &[&beam_mod], StackPolicy::AssumedMax).modified_base - p0.modified_base).abs() < 1e-9);
    let semi = m_req("semi", Some("semi_auto"), vec![], vec![ModEffect::BaseDamage(3.0)]);
    assert!(resolve(&base, &[&semi], StackPolicy::AssumedMax).modified_base > p0.modified_base);
    // disables: a mod locking `multishot` sets it to the weapon's DEFAULT.
    //
    // Not "voids other multishot MODS" — the card says "set weapon's
    // Multishot to its default ignoring other bonuses, even negative
    // effects" (wiki, Primary/Pistol Acuity), and this base carries Fevered
    // Frenzy, whose permanent stacked multishot never passed through the mod
    // bucket — so a lock that reaches only the mod bucket leaves an
    // Acuity build on Dual Toxocyst holding the evolution's pellets.
    let lock = m_req("acuity", None, vec!["multishot"], vec![]);
    let ms_mod = m("multishot", vec![ModEffect::Multishot(1.0)]);
    let locked = resolve(&base, &[&ms_mod, &lock], StackPolicy::AssumedMax);
    assert!((locked.multishot - base.base_multishot).abs() < 1e-9, "the weapon's default");
    assert!(locked.evo_multishot.is_none(), "and the evolution's buff card goes with it");
    // ...and the evolution IS worth something without the lock, so the line
    // above is not passing because it contributes nothing.
    assert!(p0.multishot > base.base_multishot + 1e-9, "Fevered Frenzy pays unlocked");
    assert_eq!(locked.locked, vec!["multishot"], "the panel states the lock");
    // CondBuff: contributes under AssumedMax, nothing under Emergent.
    let cb = m("cond", vec![ModEffect::CondBuff(CondBucket::StatusChance, 0.90)]);
    let amax = resolve(&base, &[&cb], StackPolicy::AssumedMax);
    let emerg = resolve(&base, &[&cb], StackPolicy::Emergent);
    assert!((amax.status_chance - base.base_status_chance * 1.90).abs() < 1e-9);
    assert!((emerg.status_chance - base.base_status_chance).abs() < 1e-9);
}

/// THE SIX MEASURED CHAIN COUNTS, reproduced from the multishot alone.
///
/// ✅ MEASURED (M63,) by counting Invocation stacks — those
/// four mods gain one per hit, so how many bodies a strike reached is
/// readable off the buff instead of being inferred from a damage total:
///
/// ```text
///   multishot   1.0   1.6   2.1   2.7   3.6   3.9
///   measured      3     4     6     8    10    11
/// ```
///
/// IT IS THE PAIR AT 2.1 THAT MATTERS. `floor(3 x multishot)` and
/// `multishot + 2` agree at x1.0 and part company immediately after — 6
/// against 5 — so a test pinned only at the unmodded count would pass on
/// the reading this replaced. The whole table is asserted for that reason.
#[test]
fn the_orbs_chain_reaches_three_bodies_per_point_of_multishot() {
    const MEASURED: [(f64, u32); 6] =
        [(1.0, 3), (1.6, 4), (2.1, 6), (2.7, 8), (3.6, 10), (3.9, 11)];
    let base = WeaponBase::from_data("grimoire_active", true, &[]);
    for (multishot, bodies) in MEASURED {
        // The BUILD is not the point here — what a strike reaches is a
        // function of the resolved multishot, so it is set directly rather
        // than assembled out of whichever mods happen to add up to 2.1.
        let mut b = base.clone();
        b.base_multishot = multishot;
        let orb = resolve(&b, &[], StackPolicy::AssumedMax)
            .orb
            .expect("the alt fire deploys an orb");
        assert_eq!(
            orb.chain_bodies, bodies,
            "at x{multishot} a strike reaches {bodies} bodies, got {}",
            orb.chain_bodies
        );
    }
}

/// A RANGE MOD WIDENS WHAT AN ORB CAN TOUCH, AND NOT HOW FAR IT JUMPS.
///
/// Three distances on this attack and only two of them move: Fulmination
/// enlarges the orb's REACH and its detonation RADIUS, and leaves a chain
/// HOP at the six metres the page gives it. "Takes no bonus" was about the
/// RANGE bucket, not the damage one.
///
/// Asserted as an asymmetry rather than as three numbers, because the three
/// are all six metres unmodded and a test that only read them apart would
/// pass on an engine that scaled none of them. The MOD has to be seen to
/// bite on two before "and not the third" says anything.
#[test]
fn a_range_mod_widens_an_orbs_reach_and_not_its_chain_hop() {
    let base = WeaponBase::from_data("grimoire_active", true, &[]);
    let bare = resolve(&base, &[], StackPolicy::AssumedMax);
    let bare_orb = bare.orb.expect("the Grimoire's alt fire deploys an orb");
    let pool = crate::data::mods::pool_for_build("grimoire_active", &[]);
    let fulm = pool
        .iter()
        .find(|m| m.id == "fulmination")
        .expect("Fulmination is in this weapon's pool");
    let wide = resolve(&base, &[fulm], StackPolicy::AssumedMax);
    let wide_orb = wide.orb.expect("still an orb");

    assert!(
        wide_orb.strike_radius_m > bare_orb.strike_radius_m + 1e-9,
        "the reach grows: {} against {}",
        wide_orb.strike_radius_m,
        bare_orb.strike_radius_m
    );
    let (bare_blast, wide_blast) = (
        bare.radial.expect("and it detonates").radius_m,
        wide.radial.expect("and it detonates").radius_m,
    );
    assert!(
        wide_blast > bare_blast + 1e-9,
        "…and so does the detonation: {wide_blast} against {bare_blast}"
    );
    assert!(
        (wide_orb.chain_range_m - bare_orb.chain_range_m).abs() < 1e-9,
        "…and a hop does not: {} against {}",
        wide_orb.chain_range_m,
        bare_orb.chain_range_m
    );
}

/// A LOCK REACHES A LIVE BUFF, not just the static buckets.
///
/// Acuity "set[s] weapon's Multishot to its default ignoring other bonuses,
/// even negative effects" — and a buff earned DURING the fight is a bonus
/// like any other, so Stormburst's "+0.4 Multishot for 2s, stacks 3x" is
/// worth nothing under one. It was worth +1.2: the
/// resolver knew about locks in exactly one arm, `BuffGrant::FireRate`, and
/// a buff that fed any other stat walked straight past it.
///
/// The pair below is the whole point — the same build, once locked and once
/// not — because a filter that removed the buff unconditionally would pass
/// the first half on its own.
#[test]
fn a_multishot_lock_removes_a_live_multishot_buff_too() {
    let base = WeaponBase::from_data("furis", true, &["furis_stormburst"]);
    let ms_of = |b: &ResolvedPanel| {
        b.stacking_buffs
            .iter()
            .filter(|s| s.grant == BuffGrant::FlatMultishot)
            .map(|s| s.per_stack * s.max_stacks as f64)
            .sum::<f64>()
    };
    let free = resolve(&base, &[], StackPolicy::Emergent);
    assert!(
        (ms_of(&free) - 1.2).abs() < 1e-9,
        "unlocked, Stormburst is worth +1.2 multishot at three stacks: {}",
        ms_of(&free)
    );

    let lock = m_req("acuity", None, vec!["multishot"], vec![]);
    let locked = resolve(&base, &[&lock], StackPolicy::Emergent);
    assert_eq!(ms_of(&locked), 0.0, "and nothing at all under the lock");
    assert!(
        !locked.stacking_buffs.iter().any(|s| s.grant == BuffGrant::FlatMultishot),
        "the buff is not offered — a card that stacks and grants nothing is              a measurement nobody can make"
    );
    // The lock is about ONE stat: this evolution's OTHER half, a flat +28
    // base damage, is untouched.
    let bare = WeaponBase::from_data("furis", true, &[]);
    assert!(
        locked.modified_base > resolve(&bare, &[&lock], StackPolicy::Emergent).modified_base,
        "the +28 survives — a lock is not a way to switch an evolution off"
    );
}

/// A FIRE-RATE PENALTY DOES NOT STRETCH THE BURST ITSELF — the wiki's one
/// exception, stated outright: *"Burst Delay is not affected by net
/// negative Fire Rate bonuses."*
///
/// So Critical Delay costs a Burston Prime less than its card says. The
/// gap between bursts stretches in full, the two 0.04 s gaps inside the
/// burst do not, and the weapon keeps rate the number does not account
/// for. This is the ONLY place a burst weapon is more than an auto weapon
/// relabelled at its effective rate, which is why it gets its own test.
#[test]
fn a_fire_rate_penalty_leaves_a_burst_weapon_s_own_delay_alone() {
    let base = WeaponBase::from_data("burston_prime", false, &[]);
    let listed = base.burst.expect("burston prime declares a burst").delay_seconds;
    assert!((listed - 0.04).abs() < 1e-9, "the module's BurstDelay");

    // Critical Delay: -36% fire rate, and nothing else that matters here.
    let slow = [m("critical_delay", vec![ModEffect::FireRate(-0.36)])];
    let refs: Vec<&ModDef> = slow.iter().collect();
    let p = resolve(&base, &refs, StackPolicy::AssumedMax);
    assert!(
        (p.burst.unwrap().delay_seconds - listed).abs() < 1e-9,
        "a NET NEGATIVE fire-rate bonus must leave the burst delay alone"
    );
    // ...while the rate itself takes the penalty in full.
    assert!((p.fire_rate - 5.0 * 0.64).abs() < 1e-9);

    // A POSITIVE bonus shortens BOTH — "Fire Rate bonuses affect both the
    // speed of the burst as well as the time between bursts" — which is
    // what makes a burst weapon scale linearly like every other gun. Shred
    // is +30%.
    let fast = [m("shred", vec![ModEffect::FireRate(0.30)])];
    let refs: Vec<&ModDef> = fast.iter().collect();
    let q = resolve(&base, &refs, StackPolicy::AssumedMax);
    assert!((q.burst.unwrap().delay_seconds - 0.04 / 1.30).abs() < 1e-9);
    assert!((q.fire_rate - 5.0 * 1.30).abs() < 1e-9);
}

#[test]
fn dual_toxocyst_panel_resolves_by_hand() {
    // Hornet +220%, Frostbite (cold 60/SC 60), Jolt (elec 60/SC 60),
    // PPG +187% cc, PTC +110% cd, Lethal Torrent (FR 60/MS 60),
    // Galvanized Strike (+80% SC, CO 0.4×3), Galvanized Diffusion
    // (+110% MS, +30%×4 on kill).
    let mods = [
        m("hornet", vec![ModEffect::BaseDamage(2.20)]),
        m(
            "frostbite",
            vec![
                ModEffect::Element(Cold, 0.60),
                ModEffect::StatusChance(0.60),
            ],
        ),
        m(
            "jolt",
            vec![
                ModEffect::Element(Electricity, 0.60),
                ModEffect::StatusChance(0.60),
            ],
        ),
        m("ppg", vec![ModEffect::CritChance(1.87)]),
        m("ptc", vec![ModEffect::CritDamage(1.10)]),
        m(
            "lt",
            vec![ModEffect::FireRate(0.60), ModEffect::Multishot(0.60)],
        ),
        m(
            "gshot",
            vec![
                ModEffect::StatusChance(0.80),
                ModEffect::ConditionOverload {
                    per_stack: 0.40,
                    max_stacks: 3,
                    duration: 14.0,
                    earned_on: Some("kill"),
                },
            ],
        ),
        m(
            "gdiff",
            vec![
                ModEffect::Multishot(1.10),
                ModEffect::OnKillMultishot {
                    per_stack: 0.30,
                    max_stacks: 4,
                    duration: 20.0,
                },
            ],
        ),
    ];
    let refs: Vec<&ModDef> = mods.iter().collect();
    let p = resolve(
        &WeaponBase::from_data("dual_toxocyst_incarnon", false, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]),
        &refs,
        StackPolicy::AssumedMax,
    );

    // ModifiedBase: 125 × 3.2 = 400.
    assert!((p.modified_base - 400.0).abs() < 1e-9);
    // Physical keeps its 3.2×; Cold+Electricity (adjacent) -> Magnetic
    // of 2 × 0.6 × 400 = 480; total = 400 + 480 = 880.
    assert!((p.damage.get(Magnetic) - 480.0).abs() < 1e-9);
    assert!((p.damage.get(Puncture) - 200.0).abs() < 1e-9);
    assert!((p.damage.total() - 880.0).abs() < 1e-9);
    // cc 0.31 × 2.87; cd 3 × 2.1; sc 0.43 × 3.0; fr 4.5 × 1.6.
    assert!((p.crit_chance - 0.8897).abs() < 1e-9);
    assert!((p.crit_damage - 6.3).abs() < 1e-9);
    assert!((p.status_chance - 1.29).abs() < 1e-9);
    assert!((p.fire_rate - 7.2).abs() < 1e-9);
    // MS: 1 × (1 + 1.0 Fevered + 0.6 + 1.1 + 1.2 stacks) = 4.9.
    assert!((p.multishot - 4.9).abs() < 1e-9);
    // CO assumed max: 0.4 × 3 = 1.2. No status-damage mods.
    assert!((p.co_per_type - 1.2).abs() < 1e-9);
    assert!((p.status_damage_multiplier - 1.0).abs() < 1e-9);
    // Elemental DoT brackets: cold 1.6, electricity 1.6.
    assert!(p.elem_dot_bonus.contains(&(Cold, 1.6)));
    assert!(p.elem_dot_bonus.contains(&(Electricity, 1.6)));
}

#[test]
fn faction_bonuses_merge_additively_by_faction() {
    // Two Grineer faction sources (Expel + a hypothetical Bane) add within
    // the Grineer bucket; a Corpus source is its own entry. Wiki: faction
    // bonuses are additive with each other (Bane + Roar share a bracket).
    let mods = [
        m("expel_grineer", vec![ModEffect::FactionDamage(Faction::Grineer, 0.30)]),
        m("bane_grineer", vec![ModEffect::FactionDamage(Faction::Grineer, 0.55)]),
        m("expel_corpus", vec![ModEffect::FactionDamage(Faction::Corpus, 0.30)]),
    ];
    let refs: Vec<&ModDef> = mods.iter().collect();
    let p = resolve(
        &WeaponBase::from_data("dual_toxocyst_incarnon", false, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]),
        &refs,
        StackPolicy::AssumedMax,
    );
    let bonus = |f: Faction| p.faction_damage.iter().find(|(x, _)| *x == f).map(|(_, v)| *v);
    assert!((bonus(Faction::Grineer).unwrap() - 0.85).abs() < 1e-9);
    assert!((bonus(Faction::Corpus).unwrap() - 0.30).abs() < 1e-9);
}

#[test]
fn magazine_and_status_duration_buckets_resolve() {
    let base = WeaponBase::from_data("dual_toxocyst", true, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);
    let baseline = resolve(&base, &[], StackPolicy::AssumedMax).magazine_size;
    let mods = [
        m("mag", vec![ModEffect::MagazineCapacity(0.60)]),   // +60% of base
        m("lasting", vec![ModEffect::StatusDuration(0.40)]),
    ];
    let refs: Vec<&ModDef> = mods.iter().collect();
    let p = resolve(&base, &refs, StackPolicy::AssumedMax);
    // Magazine capacity is +% of base, floored to whole rounds.
    assert!((p.magazine_size - (baseline * 1.60).floor()).abs() < 1e-9);
    assert!((p.status_duration_multiplier - 1.40).abs() < 1e-9);
}

#[test]
fn physical_mod_scales_base_of_its_type_not_the_element_hierarchy() {
    // A +90% Impact physical mod scales the BASE Impact by ×1.9 and does
    // NOT add modified_base as a combined element (the old, wrong behavior).
    // Puncture/Slash are untouched; the total rises only by the impact gain.
    let base = WeaponBase::from_data("dual_toxocyst", true, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);
    let p0 = resolve(&base, &[], StackPolicy::AssumedMax);
    let m_imp = m("phys", vec![ModEffect::Physical(Impact, 0.90)]);
    let p1 = resolve(&base, &[&m_imp], StackPolicy::AssumedMax);
    assert!((p1.damage.get(Impact) / p0.damage.get(Impact) - 1.90).abs() < 1e-9);
    assert!((p1.damage.get(Puncture) - p0.damage.get(Puncture)).abs() < 1e-9);
    assert!((p1.damage.get(Slash) - p0.damage.get(Slash)).abs() < 1e-9);
    // Total delta == exactly the impact increase (no element injected).
    assert!((p1.damage.total() - p0.damage.total() - 0.90 * p0.damage.get(Impact)).abs() < 1e-6);
}

#[test]
fn injected_toxin_raises_the_toxin_dot_bracket_too() {
    // Frenzy's +100% Toxin behaves like a Toxin mod: with Pistol
    // Pestilence (+60%) the Poison tick bracket is 1 + 0.6 + 1.0.
    let pest = m(
        "pestilence",
        vec![
            ModEffect::Element(Toxin, 0.60),
            ModEffect::StatusChance(0.60),
        ],
    );
    let base = WeaponBase::from_data("dual_toxocyst_incarnon", true, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);
    let p = resolve(&base, &[&pest], StackPolicy::AssumedMax);
    assert!(p
        .elem_dot_bonus
        .iter()
        .any(|&(t, v)| t == Toxin && (v - 2.6).abs() < 1e-9));
    // And the injection joined the vector: toxin mod + injection all
    // land as pure Toxin (no partner element): 125 × (0.6 + 1.0).
    assert!((p.damage.get(Toxin) - 200.0).abs() < 1e-9);
}

/// Torid is the first weapon whose innate damage is itself an element, so
/// it is the first build where "innate last" is observable: Heat(1) and
/// Electricity(2) combine with EACH OTHER into Radiation and the innate
/// Toxin, last, is left pure. Placing the innate first — the superseded
/// draft — gives Gas + Electricity.
#[test]
fn torid_innate_toxin_takes_what_the_mods_leave() {
    let heat = m("hellfire", vec![ModEffect::Element(Heat, 0.90)]);
    let elec = m("stormbringer", vec![ModEffect::Element(Electricity, 0.90)]);
    let base = WeaponBase::from_data("torid", true, &[]);
    let p = resolve(&base, &[&heat, &elec], StackPolicy::BaseOnly);
    assert!(p.damage.get(Radiation) > 0.0, "Heat(1)+Electricity(2)");
    assert!(p.damage.get(Toxin) > 0.0, "innate Toxin, last, stays pure");
    assert_eq!(p.damage.get(Gas), 0.0, "innate first would have made Gas");
    assert_eq!(p.damage.get(Corrosive), 0.0);
}

#[test]
fn element_mod_order_changes_the_combination() {
    let heat = m("scorch", vec![ModEffect::Element(Heat, 0.60)]);
    let cold = m("frostbite", vec![ModEffect::Element(Cold, 0.60)]);
    let tox = m("pestilence", vec![ModEffect::Element(Toxin, 0.60)]);
    let base = WeaponBase::from_data("dual_toxocyst_incarnon", false, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);

    // Heat,Cold,Toxin -> Blast + trailing Toxin.
    let p1 = resolve(&base, &[&heat, &cold, &tox], StackPolicy::AssumedMax);
    assert!(p1.damage.get(Blast) > 0.0);
    assert!(p1.damage.get(Toxin) > 0.0);
    // Cold,Toxin,Heat -> Viral + trailing Heat.
    let p2 = resolve(&base, &[&cold, &tox, &heat], StackPolicy::AssumedMax);
    assert!(p2.damage.get(Viral) > 0.0);
    assert!(p2.damage.get(Heat) > 0.0);
    // Totals identical either way.
    assert!((p1.damage.total() - p2.damage.total()).abs() < 1e-9);
}

/// A set bonus scales PER EQUIPPED MEMBER with no threshold — one
/// Vigilante mod already pays its 5%, which is what makes an otherwise
/// weak member (Supplies contributes nothing on its own) worth a slot.
#[test]
fn each_vigilante_member_adds_its_share_of_the_set_bonus() {
    // The Vigilante mods are PRIMARY-tagged, not rifle-tagged, so this has
    // to be the union the Torid actually sees.
    let pool = crate::data::mods::pool_union(&["primary".into(), "rifle".into()]);
    let pick = |id: &str| pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id}"));
    let base = WeaponBase::from_data("verglas_prime", true, &[]);
    let chance = |mods: &[&ModDef]| resolve(&base, mods, StackPolicy::BaseOnly).crit_tier_upgrade_chance;

    assert_eq!(chance(&[]), 0.0);
    assert!((chance(&[pick("vigilante_armaments")]) - 0.05).abs() < 1e-12);
    assert!(
        (chance(&[
            pick("vigilante_armaments"),
            pick("vigilante_fervor"),
            pick("vigilante_offense"),
            pick("vigilante_supplies"),
        ]) - 0.20)
            .abs()
            < 1e-12,
        "all FOUR primary members = 20%; the other two are Warframe mods"
    );
    // A non-member contributes nothing, set or no set.
    assert!(
        (chance(&[pick("vigilante_armaments"), pick("serration")]) - 0.05).abs() < 1e-12
    );
}

/// …AND THE GLADIATOR SET PAYS INTO BLOOD RUSH'S BRACKET, per piece.
///
/// *"+X% Critical Chance for Melee Weapons per Combo Multiplier"* — the
/// same `(combo - 1)` scaling and the same bracket the mod buys, at 10% a
/// piece. THREE of the six members are Warframe mods, so 30% is the ceiling
/// a weapon build can reach here and the set file says so.
/// "WITH MELEE WEAPON EQUIPPED" MEANS DRAWN, and a quick-melee swing does
/// not satisfy it.
///
/// Edge of Justice is `Base Damage +20/+100. Attack Speed +40% with a Melee
/// Weapon equipped`, and the second half is a state of the WIELDER rather
/// than a property of the build — so it is a gate the fight answers, not a
/// bonus the panel folds in. Every ruler runs it drawn; the simulator can
/// ask the other question.
///
/// THE FLAT BASE DAMAGE IS NOT GATED, because the card gates only the half
/// that names a condition — which is the whole reason the two halves are
/// two effects.
#[test]
fn attack_speed_with_the_melee_weapon_equipped_is_a_gate_and_the_base_damage_is_not() {
    let speed = |drawn: bool| {
        let mut tenno = crate::data::tenno::default_tenno().clone();
        tenno.state.melee_equipped = drawn;
        let base = WeaponBase::from_data(
            "magistar_heavy_slam",
            false,
            &["magistar_evo1_incarnon_form", "magistar_edge_of_justice"],
        );
        let p = resolve_for(&base, &[], StackPolicy::Emergent, &tenno);
        (p.fire_rate, p.radial.as_ref().expect("the slam IS the attack").damage.total())
    };
    let (drawn, radial_drawn) = speed(true);
    let (quick, radial_quick) = speed(false);
    // 0.833 base, +40% drawn.
    assert!((drawn / quick - 1.4).abs() < 1e-9, "{drawn} against {quick}");
    assert!(
        (radial_drawn - radial_quick).abs() < 1e-9,
        "the +100 lands either way: {radial_drawn} against {radial_quick}",
    );
}

/// A SET THAT ENHANCES ITS OWN MEMBERS PAYS ON COMPLETION, and one card
/// alone is worth its face.
///
/// *"The Sacrificial Set enhances all equipped mods within the set …
/// Increases the effects of both mods by 25% when both are equipped
/// together"* (wiki). So Sacrificial Steel's +220% critical chance is
/// +275% beside Sacrificial Pressure, whose +110% melee damage is +137.5%
/// — each grown by the other's presence and by nothing else.
#[test]
fn the_sacrificial_pair_enhance_each_other_and_neither_alone() {
    let pool = crate::data::mods::pool_for_weapon("magistar");
    let pick = |id: &str| pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id}"));
    let base = WeaponBase::from_data("magistar", false, &[]);
    let of = |mods: &[&ModDef]| {
        let p = resolve(&base, mods, StackPolicy::Emergent);
        (p.crit_chance / p.base_crit_chance - 1.0, p.base_damage_bonus)
    };
    let (cc_alone, _) = of(&[pick("sacrificial_steel")]);
    let (_, bd_alone) = of(&[pick("sacrificial_pressure")]);
    assert!((cc_alone - 2.2).abs() < 1e-9, "alone it is its face: {cc_alone}");
    assert!((bd_alone - 1.1).abs() < 1e-9, "alone it is its face: {bd_alone}");

    let (cc_pair, bd_pair) = of(&[pick("sacrificial_steel"), pick("sacrificial_pressure")]);
    assert!((cc_pair - 2.75).abs() < 1e-9, "+220% becomes +275%: {cc_pair}");
    assert!((bd_pair - 1.375).abs() < 1e-9, "+110% becomes +137.5%: {bd_pair}");
}

/// …AND EVERY MEMBER OF SUCH A SET HAS EFFECTS THE SCALING CAN REACH.
///
/// `ModEffect::scaled` answers `None` for a kind it cannot grow, and a
/// member carrying one would have that half of its card silently unpaid by
/// the set — which is the failure this whole family of bugs is made of.
#[test]
fn every_self_scaling_member_can_be_scaled() {
    for set in crate::data::mod_sets::sets() {
        if set.kind != crate::data::mod_sets::SetBonusKind::SelfScaling {
            continue;
        }
        for w in crate::data::weapons::all() {
            for m in crate::data::mods::pool_for_weapon(&w.id) {
                if m.set != Some(set.id) {
                    continue;
                }
                for e in &m.effects {
                    assert!(
                        e.scaled(1.25).is_some(),
                        "{} carries {e:?}, which {} cannot enhance",
                        m.id,
                        set.id,
                    );
                }
            }
        }
    }
}

/// A FLAT BASE ADD REACHES A SLAM AT ALL, which is the half that was
/// missing, and it reaches it ONCE.
///
/// IT LANDS ONCE, and that is measured to the digit: the Magistar's light
/// slam is 420 and `Base Damage +100` reads exactly 520, its heavy slam is
/// 630 and reads 1095 — `(630 + 100) x 1.5`, the x1.5 being Blast against
/// an Infested Deimos target (MEASUREMENTS M69). Multiplied by the slam's
/// own multiple the light one would read 620 and it does not.
///
/// A ZERO DIRECT VECTOR IS NOT AN EMPTY WEAPON. A form whose whole attack
/// IS its explosion states `damage: {impact: 0}`, so neither the perk path
/// nor the fold may return early on it: the mode with all of its damage in
/// the explosion is the one the add most has to reach.
#[test]
fn a_flat_base_add_reaches_a_slam_and_lands_there_once() {
    let evos = ["magistar_evo1_incarnon_form", "magistar_edge_of_justice"];
    let bare = WeaponBase::from_data("magistar_heavy_slam", false, &[]);
    let evolved = WeaponBase::from_data("magistar_heavy_slam", false, &evos);
    let radial = |b: &WeaponBase| b.radial.as_ref().expect("the slam IS the attack").base_vector.total();
    assert!((radial(&bare) - 630.0).abs() < 1e-9, "3x a 210 base — the class constant");
    assert!(
        (radial(&evolved) - 730.0).abs() < 1e-9,
        "…and the flat add lands once (M69): {}",
        radial(&evolved),
    );

    // …AND THE WEAPON'S OWN TRAILING SLAM, which a combo ends on and which
    // was taking no flat add at all.
    let combo = WeaponBase::from_data("magistar", false, &evos);
    assert!((combo.base_vector.total() - 310.0).abs() < 1e-9);
    let trailing = combo.slam.as_ref().expect("Crushing Ruin ends on one").base_vector.total();
    assert!((trailing - 310.0).abs() < 1e-9, "{trailing}");
}

#[test]
fn each_gladiator_member_adds_its_share_to_the_combo_crit_bracket() {
    let pool = crate::data::mods::pool_for_weapon("magistar");
    let pick = |id: &str| pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id}"));
    let base = WeaponBase::from_data("magistar", false, &[]);
    let per_combo =
        |mods: &[&ModDef]| resolve(&base, mods, StackPolicy::Emergent).crit_chance_per_combo;

    assert_eq!(per_combo(&[]), 0.0);
    assert!((per_combo(&[pick("gladiator_might")]) - 0.10).abs() < 1e-12);
    assert!(
        (per_combo(&[pick("gladiator_might"), pick("gladiator_rush"), pick("gladiator_vice")])
            - 0.30)
            .abs()
            < 1e-12,
        "three weapon members is the ceiling this engine can equip",
    );
    // …AND IT SHARES THE BRACKET WITH THE MOD rather than replacing it.
    assert!(
        (per_combo(&[pick("gladiator_might"), pick("blood_rush")]) - 0.10 - 0.40).abs() < 1e-12
    );
}

#[test]
fn verglas_innate_cold_combines_with_mod_elements() {
    // Innate Cold(32) sits LAST, so the lone Heat mod is the only thing
    // ahead of it and the two pair into Blast (not pure Cold + pure Heat).
    // One mod element cannot tell first from last — see
    // `innate_elements_go_last_not_first` for the case that can. No
    // base-damage mod -> modified_base 32; Heat mod = 32 × 0.9 = 28.8;
    // Blast = 60.8.
    let heat = m("hellfire", vec![ModEffect::Element(Heat, 0.90)]);
    let p = resolve(&verglas_prime(), &[&heat], StackPolicy::BaseOnly);
    assert!((p.damage.get(Blast) - 60.8).abs() < 1e-9);
    assert_eq!(p.damage.get(Cold), 0.0);
    assert_eq!(p.damage.get(Heat), 0.0);

    // Innate Cold + a Toxin mod -> Viral.
    let tox = m("infected_clip", vec![ModEffect::Element(Toxin, 0.90)]);
    let pv = resolve(&verglas_prime(), &[&tox], StackPolicy::BaseOnly);
    assert!((pv.damage.get(Viral) - 60.8).abs() < 1e-9);
}

#[test]
fn sentinel_base_only_ignores_galvanized_conditional() {
    // Galvanized Chamber: base +55% multishot + on-kill +25%×5. On a
    // sentinel only the BASE applies — no live stacks are generated.
    let gchamber = m(
        "galvanized_chamber",
        vec![
            ModEffect::Multishot(0.55),
            ModEffect::OnKillMultishot { per_stack: 0.25, max_stacks: 5, duration: 20.0 },
        ],
    );
    let p = resolve(&verglas_prime(), &[&gchamber], StackPolicy::BaseOnly);
    assert!((p.multishot - 1.55).abs() < 1e-9);
    assert!(p.multishot_stack.is_none());
}
/// A BIGGER MAGAZINE COSTS RELOAD TIME on a by-round reloader, and it is
/// free on everything else.
///
/// The Felarx loads a shell at a time — 0.8 s to start, 0.4 s a shell,
/// 0.5 s to end — so Ammo Stock buys capacity and pays for it in downtime.
/// Modelled as one flat block, the mod read as pure profit on exactly the
/// weapons the game charges for it. The other half of
/// the claim matters as much: an ordinary reloader must NOT pay, or the
/// fix would have made every magazine mod worse.
#[test]
fn a_magazine_mod_lengthens_a_by_round_reload_and_only_that() {
    let mag_mod = crate::data::mods::class_pool("shotgun")
        .into_iter()
        .find(|m| m.id == "ammo_stock")
        .expect("ammo stock");

    let felarx = WeaponBase::from_data("felarx", true, &[]);
    let plain = resolve(&felarx, &[], StackPolicy::AssumedMax);
    let bigger = resolve(&felarx, &[&mag_mod], StackPolicy::AssumedMax);
    assert!(bigger.magazine_size > plain.magazine_size, "the mod does work");
    assert!(
        bigger.reload_seconds > plain.reload_seconds,
        "a by-round reload must grow with the magazine: {} -> {} rounds,              {:.2} -> {:.2} s",
        plain.magazine_size, bigger.magazine_size,
        plain.reload_seconds, bigger.reload_seconds
    );
    // …by EXACTLY the shells added, not by some proportion of the total.
    let added = bigger.magazine_size - plain.magazine_size;
    assert!(
        ((bigger.reload_seconds - plain.reload_seconds) - added * 0.4).abs() < 1e-6,
        "{added} more shells at 0.4 s each"
    );
    // The unmodded number is still the published one.
    assert!((plain.reload_seconds - 3.7).abs() < 1e-9, "{}", plain.reload_seconds);

    // AND THE CONTROL. The Boar is an ordinary shotgun: same mod, same
    // bigger magazine, same reload.
    let boar = WeaponBase::from_data("boar", true, &[]);
    let a = resolve(&boar, &[], StackPolicy::AssumedMax);
    let b = resolve(&boar, &[&mag_mod], StackPolicy::AssumedMax);
    assert!(b.magazine_size > a.magazine_size);
    assert!((b.reload_seconds - a.reload_seconds).abs() < 1e-9,
        "an ordinary reload is one block: {} vs {}", a.reload_seconds, b.reload_seconds);
}

/// MOUNTING MOMENTUM IS PAID IN SHELLS, so it is worth what the magazine
/// is — and the magazine is not free.
///
/// The perk grants +10% fire rate per shell LOADED, which makes its value
/// a weapon stat rather than a card constant: a magazine mod buys stacks
/// and pays for them in the by-round reload it lengthens. Implementing
/// either half alone would have been worse than neither — stacks without
/// the reload cost is free fire rate for the optimizer to farm, and the
/// reload cost without the stacks is a mod that only ever hurts.
///
/// It also OPENS at one reload's worth: a per-shell counter at zero
/// describes a weapon just holstered, which is not a fight anyone measures.
#[test]
fn mounting_momentum_is_worth_a_magazine_and_grows_with_it() {
    let evo = ["felarx_mounting_momentum".to_string()];
    let ids: Vec<&str> = evo.iter().map(|s| s.as_str()).collect();
    let base = WeaponBase::from_data("felarx", true, &ids);
    let plain = resolve(&base, &[], StackPolicy::AssumedMax);
    let b = plain
        .stacking_buffs
        .iter()
        .find(|b| b.id == "per_shell_fire_rate")
        .expect("the perk grants a buff");
    assert_eq!(b.trigger, BuffTrigger::ReloadComplete);
    // Six shells: six stacks a reload, and NOTHING at t = 0 — the pile
    // is earned, and an empty magazine takes all of it.
    assert_eq!(b.stacks_per_trigger, 6, "one per shell in the magazine");
    assert_eq!(b.initial_stacks, 0, "it is earned, not granted");
    assert!(b.duration.is_infinite(), "no clock ends it");
    assert_eq!(b.cleared_by, ClearedBy::EmptyMagazine, "an empty magazine does");

    // …and it FOLLOWS the magazine, which is the whole trade.
    let mag_mod = crate::data::mods::class_pool("shotgun")
        .into_iter()
        .find(|m| m.id == "ammo_stock")
        .expect("ammo stock");
    let bigger = resolve(&base, &[&mag_mod], StackPolicy::AssumedMax);
    let bb = bigger
        .stacking_buffs
        .iter()
        .find(|b| b.id == "per_shell_fire_rate")
        .unwrap();
    assert_eq!(bb.stacks_per_trigger, bigger.magazine_size as u32);
    assert!(bb.stacks_per_trigger > b.stacks_per_trigger, "more shells, more stacks");
    // The other side of the trade, so this test fails if the reload ever
    // stops charging for it.
    assert!(
        bigger.reload_seconds > plain.reload_seconds,
        "and the reload pays for them"
    );
}

/// FIRING A MAGAZINE DRY EARNS ONE MAGAZINE'S WORTH, AND NEVER MORE.
///
/// This test asserted the opposite for an hour — a ramp to the 99-stack
/// cap — because the buff was first implemented as "a reload grants stacks
/// and nothing takes them". The mechanic is stricter: the pile is cleared
/// the INSTANT the magazine reaches zero, not by the reload and not by a
/// clock. So the loop this sim runs — fire dry,
/// reload, fire dry — holds a magazine's worth through each magazine and
/// loses every stack on its last shot.
///
/// The 99 cap therefore belongs to a play pattern this sim does not have:
/// topping up a magazine that never empties. Asserted as a SHAPE so it
/// cannot drift back — a longer fight must not pay more, which is what a
/// buff reset every magazine looks like from outside.
#[test]
fn mounting_momentum_is_reset_by_an_empty_magazine_so_it_does_not_ramp() {
    let evo = ["felarx_mounting_momentum".to_string()];
    let ids: Vec<&str> = evo.iter().map(|s| s.as_str()).collect();
    let base = WeaponBase::from_data("felarx", true, &ids);
    let panel = resolve(&base, &[], StackPolicy::AssumedMax);
    let dps = |secs: f64| {
        let arena = crate::arena::Arena::training(secs);
        let p = crate::fight::FightParams::from_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none());
        let mut rng = crate::rules::rng::Rng::new(7);
        crate::fight::run_once(&p, &mut rng).total_damage() / secs
    };
    let (d30, d300, d600) = (dps(30.0), dps(300.0), dps(600.0));
    // PAST THE FIRST MAGAZINE the rate is flat: every magazine after it
    // earns the same stacks and loses them the same way. A buff that
    // accumulated would still be climbing at ten minutes.
    assert!(
        (d600 - d300).abs() < d300 * 0.03,
        "a magazine-reset buff pays the same however long the fight:              {d300:.0} vs {d600:.0} dps"
    );
    // …and the FIRST magazine is cheaper, because there is nothing to
    // spend yet. That is the honest cost of a buff you have to earn, and
    // it is also the proof the stacks are not being seeded.
    assert!(d30 < d300 * 0.95, "the opening magazine is unbuffed: {d30:.0} vs {d300:.0}");
}

/// NIGHTWATCH NAPALM'S FIELD, and the two things a measurement settled about it.
#[cfg(test)]
mod napalm_tests {
/// The resolved field on a Kuva Ogris wearing the augment.
fn field(valence: f64, mods: &[&str]) -> super::ResolvedLingering {
    let mut base = super::WeaponBase::from_data("kuva_ogris", true, &[]);
    if valence > 0.0 {
        crate::data::weapons::apply_valence(&mut base, "kuva_ogris", "heat", valence);
    }
    let pool = crate::data::mods::pool_for_weapon("kuva_ogris");
    let picked: Vec<&super::ModDef> = std::iter::once("nightwatch_napalm")
        .chain(mods.iter().copied())
        .map(|id| pool.iter().find(|m| m.id == id).expect("in the pool"))
        .collect();
    super::resolve_for(
        &base, &picked, super::StackPolicy::Emergent, crate::data::tenno::default_tenno())
        .lingering
        .expect("the augment grants a field")
}

/// THE KUVA BONUS REACHES IT, and reaches the BASE.
///
/// It did not before: `apply_valence` spends the roll on the weapon's own
/// vectors and a MOD-granted part arrives later, so the fire stayed at 150
/// however the Lich had rolled. Measured on an impact-valence Kuva Ogris,
/// which is the clean probe — the element itself does nothing to a Heat
/// field, so anything that moves is the base moving: 21,217 DPS to 21,447
/// before, against 26,279 to 33,578 after.
#[test]
fn the_kuva_bonus_raises_the_fields_base_and_the_bucket_multiplies_it() {
    // 150 a tick, stated on the card and on the wiki's own correction of it.
    assert!((field(0.0, &[]).damage.total() - 150.0).abs() < 1e-6);
    // …and 240 on a 60% roll.
    assert!((field(0.6, &[]).damage.total() - 240.0).abs() < 1e-6, "{}",
            field(0.6, &[]).damage.total());

    // THE ORDER IS WHAT WAS MEASURED. Serration is +165%, so the base-damage
    // bucket multiplies the ALREADY-RAISED 240: `240 x 2.65 = 636`. Had the
    // valence joined that bucket instead it would be `150 x (1 + 0.6 +
    // 1.65) = 487.5` — two answers 30% apart, and one reading tells them
    // apart.
    let with = field(0.6, &["serration"]).damage.total();
    assert!((with - 636.0).abs() < 1e-6, "{with}");
    assert!((with - 487.5).abs() > 1.0, "the valence fell into the base-damage bucket");
}

/// IT TAKES NO GUN CONDITION OVERLOAD — "area damage, modelled the ordinary
/// way", which is where it parts company with the Torid's cloud. Already the
/// default for an AoE part; asserted so the default cannot drift under it.
#[test]
fn the_field_takes_no_condition_overload() {
    assert!(!field(0.0, &[]).takes_condition_overload);
}

/// SIX TICKS, THE FIRST ON CONTACT, AND NO SEVENTH. One a second, fixed —
/// fire-rate mods do not move it, which is why `tick_rate` is a constant on
/// the field rather than anything read off the weapon.
#[test]
fn six_ticks_a_second_apart() {
    let f = field(0.0, &[]);
    assert_eq!(f.tick_rate, 1.0);
    assert_eq!(f.duration_seconds, 6.0);
    assert_eq!((f.duration_seconds * f.tick_rate).round() as u32, 6);
    // …and a fire-rate mod leaves both alone.
    let fast = field(0.0, &["vile_acceleration"]);
    assert_eq!(fast.tick_rate, f.tick_rate);
    assert_eq!(fast.duration_seconds, f.duration_seconds);
}

/// **AN EXALTED WEAPON'S DAMAGE IS ITS WIELDER'S ABILITY STRENGTH.** Valkyr
/// Talons' numbers are Hysteria's at 100% — every swing, slide and slam figure
/// on that page carries the Strength icon — so a Valkyr built for strength
/// deals proportionally more, and one built for none deals proportionally less.
///
/// ASSERTED AS A PROPORTION AND AGAINST AN ORDINARY WEAPON, because a scale
/// applied to every weapon would pass a test that only looked at the claws.
#[test]
fn an_exalted_weapon_scales_with_its_wielders_ability_strength() {
    let at = |weapon: &str, strength: f64| {
        let base = super::WeaponBase::from_data(weapon, false, &[]);
        let mut t = crate::data::tenno::default_tenno().clone();
        t.ability_strength = strength;
        super::resolve_for(&base, &[], super::StackPolicy::Emergent, &t).damage.total()
    };
    let hundred = at("valkyr_talons", 1.0);
    assert!(hundred > 0.0);
    // A PROPORTION, both ways: 2.0x doubles it and 0.5x halves it.
    assert!((at("valkyr_talons", 2.0) - hundred * 2.0).abs() < 1e-9);
    assert!((at("valkyr_talons", 0.5) - hundred * 0.5).abs() < 1e-9);
    // …AND THE SPLIT IS UNTOUCHED: a scale moves the total, never the shares.
    let base = super::WeaponBase::from_data("valkyr_talons", false, &[]);
    let mut t = crate::data::tenno::default_tenno().clone();
    t.ability_strength = 3.0;
    let p = super::resolve_for(&base, &[], super::StackPolicy::Emergent, &t);
    let share = |d: &crate::rules::damage::DamageVector, k| d.get(k) / d.total();
    let one = super::resolve(&base, &[], super::StackPolicy::Emergent);
    for k in [crate::rules::damage::DamageType::Puncture, crate::rules::damage::DamageType::Slash] {
        assert!((share(&p.damage, k) - share(&one.damage, k)).abs() < 1e-12, "{k:?}");
    }
    // A WEAPON THE PLAYER CARRIES THEMSELVES IS NOT AN ABILITY, so nothing moves.
    assert_eq!(at("magistar", 3.0), at("magistar", 1.0));
}
}
