use super::*;
use crate::fight::melee::{floor_steps_down, melee_combo_multiplier, melee_combo_points, MeleeState};
use crate::fight::stacks::LiveStacks;

/// A Magistar's params with Galvanized Reflex, and the index of its
/// initial-combo pile.
fn reflex() -> (FightParams, usize) {
    let base = crate::model::WeaponBase::from_data("magistar", false, &[]);
    let pool = crate::data::mods::pool_for_weapon("magistar");
    let refs: Vec<&crate::model::ModDef> = pool.iter().filter(|m| m.id == "galvanized_reflex").collect();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let p = FightParams::from_panel(&panel, &crate::arena::Arena::training(60.0), &ArcaneFx::none());
    let i = p
        .stacking_buffs
        .iter()
        .position(|b| b.grant == crate::model::BuffGrant::InitialCombo)
        .expect("the on-kill initial combo");
    (p, i)
}

fn idle_counter() -> MeleeState {
    MeleeState {
        combo_points: 0.0,
        rage_kill_mark: 0,
        combo_expiry: f64::NEG_INFINITY,
        combo_spent_t: f64::NEG_INFINITY,
        swing_idx: 0,
        heavy_idx: 0,
        input: None,
        tennokai_until: f64::NEG_INFINITY,
        tennokai_chained: false,
        tennokai_hits: 0,
        tennokai_input_landed: false,
        animosity_stacks: 0,
        animosity_input_landed: false,
        detonating: false,
        recalling: false,
    }
}

/// M125: four stacks read 81 and three read 61 — 20.46 a stack, the counter
/// shown floored.
#[test]
fn m125_four_stacks_are_81_and_three_are_61() {
    let (p, i) = reflex();
    let per = p.stacking_buffs[i].per_stack;
    assert!((per - 20.46).abs() < 1e-9, "the package's 1.86 x 11: {per}");
    assert_eq!((4.0 * per).floor(), 81.0);
    assert_eq!((3.0 * per).floor(), 61.0);
}

/// M125: a stack falling leaves the counter at 81 for one combo duration, then
/// it settles at 61. Five tiers held for those 5 s, not four.
#[test]
fn m125_a_falling_stack_holds_the_counter_for_one_combo_duration() {
    let (p, i) = reflex();
    let b = &p.stacking_buffs[i];
    let mut stacks: Vec<LiveStacks> = p
        .stacking_buffs
        .iter()
        .map(|s| LiveStacks::seed(0, s.max_stacks, s.duration).logging_shed())
        .collect();
    // Four stacks, the last earned at 0: the first falls at the buff's duration.
    stacks[i] = LiveStacks::seed(4, b.max_stacks, b.duration).logging_shed();
    let drop = b.duration;
    let mut melee = idle_counter();
    let t = drop + 0.1;
    let floor_now = crate::fight::run::buff_total(&p, crate::model::BuffGrant::InitialCombo, &mut stacks, t);
    assert!((floor_now - 3.0 * 20.46).abs() < 1e-9, "three stacks left: {floor_now}");
    floor_steps_down(&p, &mut stacks, &mut melee, floor_now);
    let d = p.combo_duration_seconds;
    assert!((d - 5.0).abs() < 1e-9, "a bare Magistar's combo duration: {d}");
    assert!((melee.combo_points - 4.0 * 20.46).abs() < 1e-9, "the counter keeps 81.84: {}", melee.combo_points);
    assert!((melee.combo_expiry - (drop + d)).abs() < 1e-9, "on a clock started at the drop");
    let shown = melee_combo_points(melee.combo_points, floor_now, f64::INFINITY);
    assert_eq!(melee_combo_multiplier(shown), 5.0, "81 is the fifth tier");
    // …and once the clock runs out, the counter is the new floor.
    assert_eq!(melee_combo_multiplier(melee_combo_points(0.0, floor_now, f64::INFINITY)), 4.0, "61 is the fourth");
    // A second read finds nothing new to replay.
    floor_steps_down(&p, &mut stacks, &mut melee, floor_now);
    assert!((melee.combo_expiry - (drop + d)).abs() < 1e-9);
}
