//! THE TWO DECAY FAMILIES, told apart on the clock.
//!
//! `docs/BUFFS.md` has named three since the buff vocabulary was written and
//! only one of the timed ones was implemented; every stacking buff therefore
//! decayed the Galvanized way whether or not that was its rule. Stormburst is
//! the first that is not (in game 2026-08-07: each stack keeps its own
//! 2 s clock, FIFO, cap 3), and the difference is not cosmetic — under the
//! Galvanized rule ONE hit per window holds the whole pile, under this one it
//! holds exactly one stack.
use super::*;

#[test]
fn a_shared_clock_lets_one_hit_hold_every_stack() {
    let mut s = LiveStacks::seed(0, 3, 2.0);
    s.bump(0.0, 2.0, 3);
    s.bump(0.1, 2.0, 3);
    s.bump(0.2, 2.0, 3);
    assert_eq!(s.current(0.3, 2.0), 3);
    // One more hit just before the shared clock falls due, and NOTHING is
    // lost — the bump restarted the timer for all three.
    s.bump(2.0, 2.0, 3);
    assert_eq!(s.current(3.9, 2.0), 3);
}

#[test]
fn a_per_stack_clock_makes_one_hit_hold_exactly_one() {
    let mut s = LiveStacks::seed_per_stack(0, 3, 2.0);
    s.bump(0.0, 2.0, 3);
    s.bump(0.1, 2.0, 3);
    s.bump(0.2, 2.0, 3);
    assert_eq!(s.current(0.3, 2.0), 3);
    // The same single hit at t=2.0. The first three expire on their own
    // clocks at 2.0/2.1/2.2 regardless, so by 3.9 only the new one is left.
    s.bump(2.0, 2.0, 3);
    assert_eq!(s.current(3.9, 2.0), 1, "each stack expires on its own clock");
}

/// FIFO at the cap: a fourth stack pushes the OLDEST out rather than being
/// dropped, so a capped pile still rolls forward.
#[test]
fn at_the_cap_the_oldest_stack_leaves_first() {
    let mut s = LiveStacks::seed_per_stack(0, 3, 2.0);
    for i in 0..4 {
        s.bump(i as f64 * 0.1, 2.0, 3);
    }
    assert_eq!(s.current(0.4, 2.0), 3, "still capped at 3");
    // The oldest (expiring at 2.0) is gone; the youngest survives past it.
    assert_eq!(s.current(2.05, 2.0), 3, "the evicted one took no live stack with it");
}
