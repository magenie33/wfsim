use super::*;

/// SECONDARY FORTIFIER IS ×8 AT MAX RANK, CAPPED BY THE OVERGUARD LEFT, AND
/// NOTHING WHEN THE INSTANCE BREAKS THE POOL ON ITS OWN (MEASUREMENTS M120).
///
/// One instance into one body, read off the pools: `(overguard lost, health
/// lost)`. Impact is neutral on Overguard and the body has no armour or
/// shield, so what each pool lost is the rule and nothing else.
fn land(hit: f64, overguard: f64, mult: f64) -> (f64, f64) {
    let target = Foe {
        base_health: 1e9,
        ..frail_target(TargetMode::InstantRespawn, 0.0, overguard)
    };
    let mut st = TargetState::spawn(&target, crate::rules::space::Vec2::ORIGIN);
    let (og, hp) = (st.overguard, st.health);
    st.apply(
        hit,
        TypeShares::single(DamageType::Impact),
        false,
        0.0,
        &target,
        false,
        &Mitigation { disrupt_amp: 1.0, virus_amp: 1.0, virus_stacks: 0, armor_multiplier: 1.0 },
        mult,
        None,
    );
    (og - st.overguard, hp - st.health)
}

#[test]
fn m120_a_hit_on_a_deep_pool_reads_eight_times() {
    // 512 → 4096 and 10752 → 86016, both columns exactly ×8.
    for (hit, read) in [(512.0, 4096.0), (10752.0, 86016.0)] {
        let (og, hp) = land(hit, 1e9, 8.0);
        assert!((og - read).abs() < 1e-6, "{hit}: overguard took {og}, read {read}");
        assert_eq!(hp, 0.0);
    }
}

#[test]
fn m120_a_hit_that_cannot_break_the_pool_deals_exactly_what_is_left() {
    // 10752 cannot break 50000 alone; ×8 would be 86016. The pop reads the
    // 50000 left and health does not move.
    let (og, hp) = land(10752.0, 50000.0, 8.0);
    assert!((og - 50000.0).abs() < 1e-6, "overguard took {og}");
    assert_eq!(hp, 0.0, "the bonus past the pool went nowhere, health took {hp}");
}

#[test]
fn m120_a_hit_that_breaks_the_pool_alone_takes_no_bonus() {
    // 1000 into 500: the pool takes 500 and 500 carries on, exactly what the
    // same hit does without the arcane. Not `1000 − 500 / 8`.
    let with = land(1000.0, 500.0, 8.0);
    let without = land(1000.0, 500.0, 1.0);
    assert!((with.0 - 500.0).abs() < 1e-6 && (with.1 - 500.0).abs() < 1e-6, "{with:?}");
    assert_eq!(with, without);
}

#[test]
fn m120_no_overguard_no_bonus() {
    assert_eq!(land(512.0, 0.0, 8.0), land(512.0, 0.0, 1.0));
}

/// EACH BODY ASKS ITS OWN OVERGUARD. A round that punches through an Eximus
/// into a body with none behind it reaches that body unfortified, the same
/// number it would be without the arcane, while the aimed body's share is ×8.
#[test]
fn m120_a_body_behind_reads_its_own_overguard() {
    let run = |mult: f64| {
        let base = crate::model::WeaponBase::from_data("braton_prime", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(5.0);
        arena.others = vec![crate::formation::FoeSpec {
            id: String::new(),
            params: Foe { base_health: 1e15, ..Foe::training_dummy() },
            body_parts: BodyPart::humanoid(),
            at: crate::rules::space::Vec2::new(0.0, crate::rules::space::CONTACT_RANGE_M * 2.0),
        }];
        let mut p =
            FightParams::from_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none());
        p.punch_through_m = 1.0;
        p.foe.base_overguard = 1e15;
        p.foe.base_health = 1e15;
        p.arcane.overguard_multiplier = mult;
        let r = run_once(&p, &mut Rng::new(0x5EED));
        let by = r.taken.by_body().0;
        (by[0], by[1])
    };
    let (plain_front, plain_back) = run(1.0);
    let (front, back) = run(8.0);
    assert!(plain_back > 0.0, "the round never reached the body behind");
    assert!((front / plain_front - 8.0).abs() < 1e-9, "front x{}", front / plain_front);
    assert!((back - plain_back).abs() < 1e-9, "behind: {back} with, {plain_back} without");
}
