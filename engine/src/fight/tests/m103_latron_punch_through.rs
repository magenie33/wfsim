//! LATRON PRIME against the owner's readings (MEASUREMENTS M103): the base
//! form punches through and the Incarnon form does not, a round through two
//! weak points charges the gauge twice, and no Latron declares a bounce.
use super::*;

/// A LINE OF HEADS — the aimed body and `n` more behind it, one body length
/// apart, each of them nothing but head. The aimed part is pinned to a head
/// too, so every shot is a weak-point hit and the only thing moving between
/// the runs below is how many bodies the round reaches.
fn a_line_of_heads(mods: &[&str], n: usize) -> FightParams {
    let head = || {
        vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        }]
    };
    // BOTH FORMS, because the gauge is charged by the base form and spent by
    // the other one — `incarnon_cycle_from_panels` is the fight the page runs.
    let panel = |id: &str| {
        let evo = ["latron_prime_evo1_incarnon_form"];
        let base = crate::model::WeaponBase::from_data(id, false, &evo);
        let pool = crate::data::mods::pool_for_weapon("latron_prime");
        let refs: Vec<&crate::model::ModDef> = mods
            .iter()
            .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("{m}")))
            .collect();
        crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
    };
    let mut arena = crate::arena::Arena::training(20.0);
    // THE AIMED BODY IS ALL HEAD, in the arena rather than on the outer
    // params: the base form reads its own copy, and that is the form that
    // charges.
    arena.body_parts = head();
    arena.others = (1..=n)
        .map(|i| crate::formation::FoeSpec {
            id: String::new(),
            params: TargetParams::training_dummy(),
            body_parts: head(),
            at: crate::rules::space::Vec2::new(
                0.0,
                crate::rules::space::CONTACT_RANGE_M * (1.0 + i as f64),
            ),
        })
        .collect();
    FightParams::incarnon_cycle_from_panels(
        &panel("latron_prime_incarnon"),
        &panel("latron_prime"),
        false,
        LockMode::Initial(0),
        &arena,
        &ArcaneFx::none(),
    )
}

/// ONE ROUND THROUGH TWO HEADS IS TWO WEAK-POINT HITS, so a gauge that wants
/// eight of them is full in three shots instead of eight — "8 weakpoints will
/// completely fill the gauge" counts LANDINGS, not trigger pulls (M103).
///
/// READ OFF THE SHOTS THE FIRST TRANSMUTE COST, not off a counter and not off
/// the transform count: a charge that is tallied and never spent looks exactly
/// like one that works, and over a whole engagement this cycle is bound by the
/// 40-round dump rather than by the charging.
#[test]
fn m103_a_round_through_two_heads_charges_the_gauge_twice() {
    let shots_to_fill = |mods: &[&str], n: usize| {
        let rec = crate::fight::replay::record(
            &a_line_of_heads(mods, n), 5, 0.0, f64::INFINITY, 200_000, 0,
        );
        let mut shots = 0u32;
        for e in rec.events() {
            match &e.kind {
                crate::record::Kind::Shot { .. } => shots += 1,
                crate::record::Kind::TransformStart { .. } => return shots,
                _ => {}
            }
        }
        panic!("the fixture never transformed");
    };
    // EIGHT WEAK POINTS, one a shot: the gauge's own number.
    assert_eq!(shots_to_fill(&[], 2), 8, "one head a shot");
    // …AND THREE WITH TWO BODIES BEHIND THE FIRST — nine landings, the gauge
    // overshooting by one, which is the same `>=` every gauge crosses on.
    assert_eq!(shots_to_fill(&["primed_shred"], 2), 3, "three heads a shot");
    // …AND IT IS THE BODIES, NOT THE MOD: with nobody behind the target
    // punch through reaches nothing and pays nothing.
    assert_eq!(shots_to_fill(&["primed_shred"], 0), 8, "nobody behind it");
}

/// …AND EVERY OTHER WEAK-POINT TRIGGER IS PAID THE SAME NUMBER OF TIMES.
/// Death Knell is the readable one — a stack per weak-point hit, per pellet —
/// so a round through two more heads climbs its pile three times a shot.
#[test]
fn m103_a_punched_weak_point_fires_what_a_weak_point_fires() {
    let damage = |mods: &[&str], n: usize| {
        let mut p = a_line_of_heads(mods, n);
        p.weakpoint_stacks = Some(crate::model::WeakpointStacksSpec {
            max_stacks: 6,
            duration_seconds: 6.0,
            crit_multiplier: 0.5,
            status_chance: 0.0,
            ammo_efficiency: 0.0,
        });
        // THE AIMED BODY ALONE, so the punched bodies' own damage cannot be
        // what this reads — the claim is about the pile, not about the crowd.
        monte_carlo(&p, 12, 0x103).mean_damage_by_body.0[0]
    };
    let alone = damage(&[], 2);
    let through = damage(&["primed_shred"], 2);
    assert!(
        through > alone * 1.05,
        "the punched heads fill Death Knell's pile too: {alone:.0} -> {through:.0}"
    );
}

/// THE INCARNON FORM PIERCES NOBODY and the base form does — *"Punch Through
/// does not affect the incarnon projectile"*, ✅ measured (M103) — and no
/// Latron declares a BOUNCE, whose rule the same reading put out of reach.
#[test]
fn m103_the_incarnon_form_refuses_punch_through_and_declares_no_bounce() {
    for id in ["latron", "latron_prime", "latron_wraith"] {
        let inc = format!("{id}_incarnon");
        let panel = |w: &str| {
            let base = crate::model::WeaponBase::from_data(w, false, &[]);
            let pool = crate::data::mods::pool_for_weapon(id);
            let shred: Vec<&crate::model::ModDef> =
                vec![pool.iter().find(|d| d.id == "primed_shred").expect("primed_shred")];
            crate::build::loadout::resolve(&base, &shred, crate::model::StackPolicy::Emergent)
        };
        assert!(panel(id).punch_through_m > 2.0, "{id} takes the mod");
        assert!(
            (panel(&inc).punch_through_m - 0.0).abs() < 1e-9,
            "{inc} takes no punch through: {}",
            panel(&inc).punch_through_m
        );
        assert!(
            crate::model::WeaponBase::from_data(&inc, false, &[]).ricochet.is_none(),
            "{inc} declares no bounce"
        );
    }
}
