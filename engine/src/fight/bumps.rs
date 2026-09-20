// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE BUMPS, AND WHY THEY ARE MACROS.
//!
//! These run per HIT and per PELLET, and each one borrows the buff roster, the
//! live stacks and one of the dice at once. Written as functions they cost
//! 4-6% per shot on `one_fight` (gotva_prime, scourge, braton_prime), with
//! `#[inline]` and with `#[inline(always)]` alike — so the macro is a measured
//! decision, not a habit. Everything else that was a macro in the loop is a
//! function.
//!
//! They take what they touch, rather than reading the caller's locals, so the
//! pellet's pipeline can use them from its own file.

/// EVERY BUFF ONE EVENT GRANTS, and the row that says it fired.
macro_rules! bump_buffs {
    ($params:expr, $stacks:expr, $rec_index:expr, $rec:expr, $trigger:expr, $t:expr, $rng:expr) => {
        for (i, b) in $params.stacking_buffs.iter().enumerate() {
            if b.trigger == $trigger && (b.chance >= 1.0 || $rng.chance(b.chance)) {
                // …AND THE ROW SAYS SO. One line here rather than at seven
                // call sites, so a trigger added later is covered by nobody
                // having to remember it.
                if let Some(k) = $rec_index.get(i).copied().flatten() {
                    $rec.triggered(k);
                }
                // ONE TRIGGER, `stacks_per_trigger` STACKS. Every buff written
                // before Mounting Momentum grants one, and that one grants a
                // shell's worth each — so the bump repeats rather than the cap
                // being bypassed.
                for _ in 0..b.stacks_per_trigger.max(1) {
                    $stacks[i].bump($t, b.duration, b.max_stacks);
                }
            }
        }
    };
}

/// …and the target-conditional family, which needs the fight's debuff state as
/// well as the clock. One arm, however many buffs use it.
macro_rules! bump_status_buffs {
    ($params:expr, $stacks:expr, $debuffs:expr, $t:expr, $rng:expr) => {
        for (i, b) in $params.stacking_buffs.iter().enumerate() {
            if let crate::model::BuffTrigger::HitEnemyWithStatus(s) = b.trigger {
                if crate::fight::has_status($debuffs, s) && (b.chance >= 1.0 || $rng.chance(b.chance)) {
                    $stacks[i].bump($t, b.duration, b.max_stacks);
                }
            }
        }
    };
}

pub(super) use bump_buffs;
pub(super) use bump_status_buffs;
