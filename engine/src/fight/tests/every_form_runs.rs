use super::*;

/// EVERY FORM OF EVERY WEAPON ACTUALLY FIRES.
///
/// The roster has 224 entries and only 136 of them are the arsenal's form.
/// The other 88 are reachable — a FORM IS PART OF A BUILD,
/// picked as the fight's `mode` and saved in a build preset — and until
/// this test nothing ever built one. Every guard, every intake check and
/// every UI pass walked `roster()`, which is the DEFAULT forms only.
///
/// What that hid: five entries carried `charge_seconds: 0.0` on a
/// non-bow, which `base_panel` refuses outright ("a 0.0 charge outside a
/// bow is just a fire rate"). They parsed, they passed every test, they
/// shipped — and the server PANICKED the first time anybody asked for
/// `mode: alternate` on a Velocitus.
///
/// So this builds and briefly runs EVERY entry, not every weapon. It is
/// cheap — two seconds for the roster — and it is the only test that would
/// have caught that class of failure.
#[test]
fn every_entry_builds_and_fires() {
    let arena = crate::arena::Arena::training(3.0);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let mut ran = 0;
    for w in crate::weapons_data::all() {
        let base = crate::model::WeaponBase::from_data(&w.id, false, &[]);
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        let s = monte_carlo(&p, 2, 3);
        assert!(
            s.mean_damage > 0.0,
            "{}: a form that fires nothing is a form nobody can play",
            w.id
        );
        ran += 1;
    }
    assert!(ran > 200, "only {ran} entries built");
}

/// DAMAGE FALLOFF IS WIRED, and the Boar's own published window is the
/// ruler: full damage to 15 m, decaying linearly to 50% of it at 25 m and
/// flat past there (`data/weapons/primary/boar.yaml`, from the wiki).
///
/// THE RATIO IS EXACT, and that is the point of asserting it this way. The
/// falloff draws no random number, so the same seed fires the same pellets
/// into the same body parts with the same crit tiers at every range — two
/// runs that differ only in where the shooter stood, with one constant
/// between their direct-damage totals. An approximate assertion here would
/// pass on a factor applied in the wrong bracket.
#[test]
fn a_shotgun_loses_exactly_its_published_share_over_its_published_window() {
    // A GAP, which is what a published window is quoted in and what the
    // page shows — so these are the card's own numbers, unadjusted. The
    // two centres stand one contact further apart than this.
    let direct_at = |gap: f64| {
        let mut arena = crate::arena::Arena::training(10.0);
        arena.target_at =
            crate::space::Vec2::new(0.0, gap + crate::space::CONTACT_RANGE_M);
        let base = crate::model::WeaponBase::from_data("boar", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        // ONE MECHANIC PER TEST. The aim model is switched off here so the
        // only thing that can move between these ranges is the falloff: the
        // Boar's 12.5 accuracy is an 8-degree cone and it drops most of its
        // pellets well inside this window, which is the aim model's own
        // business and is measured by its own tests.
        p.spread = None;
        run_once(&p, &mut Rng::new(0x5EED)).sources.direct
    };
    let full = direct_at(0.0);
    assert!(full > 0.0, "the fixture fired nothing");
    // Inside the window: nothing is lost yet.
    assert_eq!(direct_at(15.0), full, "full damage out to the start");
    // Half way across it: half the loss.
    assert!((direct_at(20.0) / full - 0.75).abs() < 1e-12, "linear across the window");
    // At the end, and flat beyond it — a floor, not a slope that continues.
    assert!((direct_at(25.0) / full - 0.5).abs() < 1e-12, "the published floor");
    assert!((direct_at(120.0) / full - 0.5).abs() < 1e-12, "flat past the end");
}

/// …AND THE FLOOR IS WHAT THE PAGE SAYS REMAINS, which the Boar cannot
/// tell: its 0.5 reads the same either way round. The Hek's page is *"from
/// 100% to 20% from 10m to 20m"* and its module `Reduction` is 0.8, so past
/// 20 m a shot deals a fifth — the share-kept reading gives four times that.
#[test]
fn a_falloff_floor_is_what_remains_not_what_is_removed() {
    let direct_at = |gap: f64| {
        let mut arena = crate::arena::Arena::training(10.0);
        arena.target_at =
            crate::space::Vec2::new(0.0, gap + crate::space::CONTACT_RANGE_M);
        let base = crate::model::WeaponBase::from_data("hek", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        p.spread = None; // the aim model has its own tests
        run_once(&p, &mut Rng::new(0x5EED)).sources.direct
    };
    let full = direct_at(0.0);
    assert!(full > 0.0, "the fixture fired nothing");
    assert!((direct_at(15.0) / full - 0.6).abs() < 1e-12, "half way: 100% to 20%");
    assert!((direct_at(30.0) / full - 0.2).abs() < 1e-12, "the page's floor");
}

/// …AND A WEAPON THAT LISTS NO FALLOFF NOTICES NO RANGE. Absence is not
/// "unknown, so guess a curve": the wiki states it from the other side —
/// *"Hitscan weapons that do not list Damage Falloff values in their UI are
/// completely unaffected"* — and the roster carries a window on nineteen
/// entries out of two hundred and change.
#[test]
fn a_weapon_without_a_published_falloff_is_the_same_at_any_range() {
    // A GAP, which is what a published window is quoted in and what the
    // page shows — so these are the card's own numbers, unadjusted. The
    // two centres stand one contact further apart than this.
    let direct_at = |gap: f64| {
        let mut arena = crate::arena::Arena::training(10.0);
        arena.target_at =
            crate::space::Vec2::new(0.0, gap + crate::space::CONTACT_RANGE_M);
        let base = crate::model::WeaponBase::from_data("latron", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        p.spread = None; // the aim model has its own tests
        run_once(&p, &mut Rng::new(0x5EED)).sources.direct
    };
    let full = direct_at(0.0);
    assert!(full > 0.0, "the fixture fired nothing");
    assert_eq!(direct_at(80.0), full);
}

/// How many pellets of `weapon` LANDED over one engagement at `range`.
#[cfg(test)]
fn landed(weapon: &str, range: f64) -> u32 {
    let mut arena = crate::arena::Arena::training(10.0);
    arena.target_at = crate::space::Vec2::new(0.0, range);
    let base = crate::model::WeaponBase::from_data(weapon, false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
    run_once(&p, &mut Rng::new(0x5EED)).pellets
}

/// A PINPOINT ATTACK NEVER MISSES, at any range. The Torid's grenade is
/// `0 / 0` in the wiki's own weapon module and its page says the same thing
/// in words ("Pinpoint accuracy"), which is a property to transcribe rather
/// than a cone to derive from a rounded Accuracy scalar.
#[test]
fn a_pinpoint_attack_never_misses_however_far_away() {
    let close = landed("torid", 0.0);
    assert!(close > 0, "the fixture fired nothing");
    assert_eq!(landed("torid", 40.0), close);
    assert_eq!(landed("torid", 300.0), close);
    let s = crate::model::WeaponBase::from_data("torid", false, &[]).spread.unwrap();
    assert_eq!((s.min_deg, s.max_deg), (0.0, 0.0));
}

/// …AND SPREAD COSTS MORE THE FURTHER AWAY THE TARGET IS, because spread is
/// an ANGLE from the reticle: the same cone that cannot miss at point blank
/// is metres across a room. The Braton's aimed cone is 2 degrees
/// (`Module:Weapons/data`, MinSpread 2 / MaxSpread 5).
///
/// POINT BLANK IS THE ONE RANGE THAT CANNOT MISS, and that is the property
/// every golden value and every board row in this repo rests on — the aim
/// roll is not even drawn there.
#[test]
fn spread_costs_more_the_further_away_the_target_stands() {
    let (near, mid, far) = (landed("braton", 0.0), landed("braton", 20.0), landed("braton", 60.0));
    assert!(near > 0, "the fixture fired nothing");
    assert!(mid < near, "20 m landed {mid} of {near} — spread cost nothing");
    assert!(far < mid, "60 m landed {far}, 20 m landed {mid} — no range dependence");
    assert!(far > 0, "a 2 degree cone should still land something at 60 m");
}

/// AN ENTRY WHOSE SPREAD NOBODY TRANSCRIBED CANNOT MISS, and says so.
///
/// Absence is NOT "pinpoint" and it is NOT the base form's cone either —
/// taking one silently is the mistake this repo already caught once
/// (AGENTS.md §"A FORM INHERITS ITS WEAPON": the ordinary Larkspur's
/// alt-fire carried its BASE form's accuracy and nothing could see it). The
/// intake refuses any attack it cannot identify uniquely, so the shot lands
/// and the entry carries the admission.
#[test]
fn an_entry_with_no_transcribed_spread_lands_everything_and_admits_it() {
    // A GUN, because this is about the spread INTAKE's remaining gap. A
    // melee entry has no cone by construction and it does miss — past its
    // reach, which is a wall rather than a cone — so it answers this
    // question with a completely different mechanic.
    let id = crate::weapons_data::all()
        .iter()
        .filter(|w| w.slot != "melee")
        .map(|w| w.id.clone())
        .find(|id| crate::model::WeaponBase::from_data(id, false, &[]).spread.is_none())
        .expect("the roster still has entries waiting on the spread intake");
    let close = landed(&id, 0.0);
    assert!(close > 0, "{id}: the fixture fired nothing");
    assert_eq!(landed(&id, 60.0), close, "{id} must not miss");
    let spec = crate::weapons_data::spec(&id).unwrap();
    assert!(
        spec.unmodeled_parts
            .iter()
            .any(|u| u.reason.as_deref() == Some("spread_not_transcribed")),
        "{id}: cannot miss for a DATA reason and does not say so"
    );
}

/// THE TWO CLAUSES A RANGE TURNED ON, and both of them are worth zero at
/// point blank — which is why neither moves a board row.
///
/// They were both filed as EDGES (`reason: no_distance`, "nothing about
/// this engine will ever close it") and the arena gaining a range closed
/// them, so the declarations were false until 2026-08-15. That is a worse
/// state than an open gap: an admission that outlives the gap it names
/// tells a player to distrust a number that is now right.
///
/// - LONE ENFORCER (Vectis): "+25% Multishot if no enemies are within 5m".
///   With one enemy that is exactly "the target stands past 5 m".
/// - HUNTER'S MANTRA (Boltor): "With Channeled Ability active: +40%
///   Accuracy" — a narrower cone, so more pellets land at a distance. Its
///   OTHER half (Punch Through +4) is still an edge and still says so: it
///   needs a second body, which this arena does not have.
#[test]
fn a_range_gated_clause_pays_past_its_range_and_nothing_at_point_blank() {
    let multishot_at = |range: f64| {
        let mut arena = crate::arena::Arena::training(10.0);
        arena.target_at = crate::space::Vec2::new(0.0, range);
        let base = crate::model::WeaponBase::from_data(
            "vectis",
            false,
            &["vectis_lone_enforcer"],
        );
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none()).multishot
    };
    let base = multishot_at(0.0);
    assert!(base > 0.0, "the fixture has no multishot");
    assert_eq!(multishot_at(4.9), base, "inside 5 m it pays nothing");
    // …and past it, exactly a quarter of the weapon's BASE multishot, in
    // the same bucket every other conditional multishot grant feeds.
    assert!(
        (multishot_at(5.1) - base * 1.25).abs() < 1e-9,
        "past 5 m: {} against {}",
        multishot_at(5.1),
        base * 1.25
    );
}

/// …AND ACCURACY IS A REAL STAT NOW, so a card that grants it narrows the
/// cone. Asserted on the CONE rather than on a hit rate: the payload is
/// "the spread is smaller", and a rate would fold in the geometry as well.
#[test]
fn a_card_that_grants_accuracy_narrows_the_cone() {
    let cone = |channeling: bool| {
        let mut tenno = crate::tenno_data::default_tenno().clone();
        tenno.state.channeling = channeling;
        let base = crate::model::WeaponBase::from_data(
            "boltor",
            false,
            &["boltor_hunters_mantra"],
        );
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        crate::loadout::resolve_for(
            &base,
            &refs,
            crate::model::StackPolicy::Emergent,
            &tenno,
        )
        .spread
        .expect("the Boltor has a transcribed cone")
    };
    let (off, on) = (cone(false), cone(true));
    assert!(off.min_deg > 0.0, "the fixture has no cone to narrow");
    // +40% accuracy divides the angle by 1.4 — the wiki's own direction
    // ("bonuses that increase accuracy decrease the deviation").
    assert!(
        (on.min_deg - off.min_deg / 1.4).abs() < 1e-9,
        "{} against {}",
        on.min_deg,
        off.min_deg / 1.4
    );
}

/// POINT BLANK IS THE FIGHT THIS ENGINE HAS ALWAYS RUN, and the 2D layer
/// has to leave it BYTE-IDENTICAL rather than merely close.
///
/// Asserted as a PROPERTY rather than trusted: the same build at range 0
/// with its real cone and with no cone at all produces the same run, field
/// for field, to the last bit.
///
/// WHAT IT CATCHES, established by breaking it:
///
/// - removing the `range > 0.0` GATE does NOT fail it. The offset is
///   `range * tan(theta)`, zero at range 0 either way; the gate saves a
///   draw and is not what makes point blank safe.
/// - dropping the `range *` — the ordinary "forgot to scale by distance"
///   bug — DOES, loudly: the Strun lands 105 pellets against 144, the
///   regression that would move every board row while every test looking
///   at averages still passed.
///
/// `one_fight` makes the same claim across three whole builds at 1000 runs;
/// this makes it cheap enough for every commit.
#[test]
fn at_point_blank_a_cone_changes_nothing_at_all() {
    for id in ["boar", "braton", "strun", "torid", "vectis"] {
        let arena = crate::arena::Arena::training(10.0);
        let base = crate::model::WeaponBase::from_data(id, false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let with = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        assert!(with.spread.is_some(), "{id} has no cone to switch off");
        let without = FightParams { spread: None, ..with.clone() };
        let a = run_once(&with, &mut Rng::new(0x5EED));
        let b = run_once(&without, &mut Rng::new(0x5EED));
        assert_eq!(a.shots, b.shots, "{id}: shots");
        assert_eq!(a.pellets, b.pellets, "{id}: pellets");
        assert_eq!(a.crits, b.crits, "{id}: crits");
        assert_eq!(a.procs, b.procs, "{id}: procs");
        assert_eq!(a.kills, b.kills, "{id}: kills");
        assert_eq!(a.total_damage().to_bits(), b.total_damage().to_bits(), "{id}: damage");
    }
}

/// THE LARKSPUR PAIR IS THE ONE THIS REPO HAS ALREADY GOT WRONG.
///
/// AGENTS.md §"A FORM INHERITS ITS WEAPON" records it: the ordinary
/// Larkspur's alt-fire carried its BASE form's accuracy while its Prime's
/// carried the alt-fire's, and nothing could catch it because nothing knew
/// the entries were one gun. It is exactly the shape the spread intake can
/// fail in, so it is pinned rather than trusted — the module gives "Normal
/// Attack" 10/14 and "Alt-Fire Projectile Impact" 0/0, and each of the four
/// entries has to hold the one that is its own.
#[test]
fn the_larkspur_family_each_carries_its_own_cone() {
    let cone = |id: &str| {
        let s = crate::model::WeaponBase::from_data(id, false, &[])
            .spread
            .unwrap_or_else(|| panic!("{id} has no spread"));
        (s.min_deg, s.max_deg)
    };
    assert_eq!(cone("larkspur"), (10.0, 14.0));
    assert_eq!(cone("larkspur_prime"), (10.0, 14.0));
    // The alt-fire is a PROJECTILE and it is pinpoint — the half that was
    // once written onto the wrong member of this family.
    assert_eq!(cone("larkspur_charged"), (0.0, 0.0));
    assert_eq!(cone("larkspur_prime_charged"), (0.0, 0.0));
}

/// EVERY ENTRY ANSWERS THE QUESTION — with a cone, or with an admission.
///
/// The third state is the one that must not exist: an entry that silently
/// cannot miss and never says why. The ceiling is a RATCHET — re-running
/// `scripts/intake_spread.py` after teaching it another attack shape is
/// only ever allowed to lower it.
#[test]
fn every_entry_either_has_a_spread_or_admits_it_has_none() {
    let (mut with, mut without) = (0, 0);
    for w in crate::weapons_data::all() {
        // A SWING HAS NO CONE, and that is the GAME's answer rather than a
        // gap in ours — the same split `scenario::Absence` draws. A cone is
        // where a projectile went; a melee attack has a REACH and a body
        // either stands inside it or does not. So there is nothing to
        // transcribe, and `spread_not_transcribed` would be a false
        // admission: it says a number exists and we did not write it down.
        if w.slot == "melee" {
            continue;
        }
        if crate::model::WeaponBase::from_data(&w.id, false, &[]).spread.is_some() {
            with += 1;
            continue;
        }
        without += 1;
        assert!(
            w.unmodeled_parts
                .iter()
                .any(|u| u.reason.as_deref() == Some("spread_not_transcribed")),
            "{}: no spread and no admission — it cannot miss and nobody is told",
            w.id
        );
    }
    assert!(with >= 207, "only {with} entries carry a spread");
    assert!(without <= 17, "the gap grew to {without}; it is only allowed to shrink");
}

/// ...and every DEPLOYMENT of every entry, for the same reason: the
/// Archwing column is a choice a player can make, and nothing else builds
/// it either.
#[test]
fn every_deployment_builds_and_fires() {
    let arena = crate::arena::Arena::training(3.0);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let mut ran = 0;
    for w in crate::weapons_data::all() {
        for dep in crate::weapons_data::deployments_of(&w.id) {
            let mut base = crate::model::WeaponBase::from_data(&w.id, false, &[]);
            crate::weapons_data::apply_deployment(&mut base, &w.id, &dep);
            let panel =
                crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
            let p =
                FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
            assert!(
                monte_carlo(&p, 2, 3).mean_damage > 0.0,
                "{} in {dep}: fires nothing",
                w.id
            );
            ran += 1;
        }
    }
    assert!(ran > 50, "only {ran} (entry, deployment) pairs built");
}
