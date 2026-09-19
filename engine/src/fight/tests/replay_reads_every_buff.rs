//! ...AND EVERY BUFF THE ROSTER OFFERS MUST BE READ BACK BY THE REPLAY.
//!
//! This is the THIRD list. `buff_roster` says what exists, `apply_buff_config`
//! obeys the card, and `sample_stacks` reads the live count for the curve on
//! screen — and that last one ends in a catch-all `_ => 0`, so a rostered buff
//! with no arm there does not fail, it draws a flat zero line for the whole
//! engagement. That reads as "the buff never came up", which is a sentence
//! about the build rather than about the code, and it is wrong.
//!
//! `every_buff_the_roster_offers_is_actually_read` already pins roster against
//! config. This pins roster against the replay, the same way: seed every buff
//! at its cap with the card LOCKED, replay one frame, and assert nothing that
//! was offered reads zero.
use super::*;

#[test]
fn no_rostered_buff_draws_a_flat_zero_it_did_not_earn() {
    use crate::model::{StackingBuff, TimedBuff};
    use crate::model::StackSpec;
    let stack = |per_stack: f64, earned_on: &'static str| StackSpec {
        per_stack,
        max_stacks: 3,
        duration: 6.0,
        initial_stacks: 0,
        earned_on: Some(earned_on),
    };
    let timed = |value: f64| TimedBuff { value, duration: 4.0, initial_active: false };
    let buff = |id: &'static str, grant, trigger| StackingBuff {
        id,
        trigger,
        grant,
        chance: 1.0,
        decay: crate::model::BuffDecay::LoseOneAndReset,
        per_stack: 0.1,
        max_stacks: 3,
        duration: 10.0,
        initial_stacks: 0,
        stacks_per_trigger: 1,
        per_shell: false,
        cleared_by: crate::model::ClearedBy::Nothing,
        card_opens_full: false,
    };
    let mut params = FightParams {
        co_stack: Some(stack(0.2, "kill")),
        multishot_stack: Some(stack(0.3, "kill")),
        crit_chance_stack: Some(stack(0.1, "headshot_kill")),
        stacking_buffs: vec![
            buff("on_plain_hit_damage", crate::model::BuffGrant::BaseDamage,
                 crate::model::BuffTrigger::PlainHit),
            buff("on_headshot_reload_speed", crate::model::BuffGrant::ReloadSpeed,
                 crate::model::BuffTrigger::Headshot),
        ],
        crit_chance_on_headshot: Some(timed(0.5)),
        crit_damage_on_kill: Some(timed(0.6)),
        fire_rate_on_reload: Some(timed(0.7)),
        base_damage_on_reload: Some(timed(0.8)),
        // Rostered only where a mod reads the count, so the fixture
        // carries both halves — the passive and the mod that pays for it.
        tendril_max: 4,
        crit_chance_per_tendril: 0.1,
        ..FightParams::default()
    };

    // Seed EVERY offered buff at its cap and lock it, so a zero on the
    // first frame can only mean nothing read it.
    let roster = params.buff_roster();
    assert!(roster.len() >= 9, "the fixture stopped covering the roster: {roster:?}");
    let mut cfg = BuffConfig::new();
    for b in &roster {
        cfg.insert(b.id.clone(), (if b.max_stacks == 0 { 1 } else { b.max_stacks }, true));
    }
    params.apply_buff_config(&cfg);

    let rep = replay(&params, 12345, 4);
    let first = &rep.frames[0];
    // The weapon passive is applied by the api, not by these params — the
    // same exemption `every_buff_the_roster_offers_is_actually_read` makes.
    for (i, b) in rep.buffs.iter().enumerate() {
        let id = &b.id;
        if id == "frenzy" {
            continue;
        }
        assert!(
            first.stacks[i] > 0,
            "`{id}` is offered and configured to its cap, and the replay reads 0 — \
                 nothing in `sample_stacks` answers for it, so its curve is a flat line"
        );
    }
}
