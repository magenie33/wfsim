use super::*;

/// Live reload time for the active form: the arcane's reload-speed sources
/// (Merciless r5 static, Conjunction Voltage stacks) and Lethal
/// Rearmament's headshot stacks join the form's reload-speed BUCKET —
/// time = base / (1 + bucket + live additions).
pub(super) fn live_reload_time(
    form: &FightParams,
    outer: &FightParams,
    arc: &mut ArcRuntime,
    live_rs: f64,
    t: f64,
    // A RELOAD FROM EMPTY takes the weapon's own term too (the Catabolyst
    // family's -20%), in the mods' bucket — "additive with Quickdraw".
    from_empty: bool,
) -> f64 {
    let add = outer.arcane.reload_bonus
        + arc.total(&outer.arcane.buffs, ArcGrant::ReloadSpeed, t)
        + live_rs;
    let (secs, bucket) = match form.reload_from_empty_speed {
        innate if from_empty && innate != 0.0 => {
            let b = form.reload_bonus;
            (form.reload_seconds * (1.0 + b) / (1.0 + b + innate).max(1e-9), b + innate)
        }
        _ => (form.reload_seconds, form.reload_bonus),
    };
    reload_span(secs, bucket, add)
}

/// Rescale a time already divided by `(1 + bucket)` so it also carries a
/// LIVE addition to the same bucket — the transmute animations, which the
/// wiki ties to reload speed.
pub(super) fn rescale_reload(secs: f64, bucket: f64, live: f64) -> f64 {
    reload_span(secs, bucket, live)
}

/// A RELOAD IS PAID FOR WHILE IT RUNS, not priced when it starts.
///
/// The arithmetic is a work integral, which is what makes a reload-speed total
/// mean anything: a reload is `secs x (1 + bucket)` seconds of WORK and a total
/// of `add` retires it at `1 + bucket + add` per second.
///
/// NO LAPSING WINDOW. Ready Retaliation is scoped to the reload ACTION — it
/// arrives when the reload starts and is gone when it ends — so nothing lapses
/// mid-reload and there is no partial-rate branch. If a lapsing reload buff
/// ever exists, this is where it goes.
///
/// `secs` arrives ALREADY divided by `(1 + bucket)` — it is the modded reload —
/// so multiplying it back out is what recovers the work, and a weapon with no
/// live bonus at all falls straight through to `secs`.
pub(super) fn reload_span(secs: f64, bucket: f64, add: f64) -> f64 {
    if add <= 0.0 {
        return secs;
    }
    secs * (1.0 + bucket) / (1.0 + bucket + add)
}

/// EVERY LIVE ADDITION TO THE RELOAD-SPEED BUCKET, in one place.
///
/// `owner` is the form the perks belong to — the base half of a cycle, whose
/// evolutions are on it — while `params` carries the buff bar. They differ, and
/// reading the wrong one is not a small mistake: Ready Retaliation is dropped
/// on a charge-backed form, so taking it off the outer params gave every
/// transform animation a bonus of exactly nothing on the only weapon that has
/// the perk.
///
/// READY RETALIATION IS A BUFF THAT IS UP OR DOWN, with no duration and no
/// condition of its own. So it is summed here, where the
/// total is asked for, and NOT tested at each of the four places that ask —
/// the events that REMOVE it are the only place it is reasoned about. Four
/// copies of one `if` is how three of them come to
/// disagree with the fourth.
pub(super) fn live_reload_speed(
    params: &FightParams,
    owner: &FightParams,
    rs_armed: bool,
    stacks: &mut [LiveStacks],
    t: f64,
) -> f64 {
    let ready = if rs_armed { owner.rs_on_reload } else { 0.0 };
    ready +
        params
            .stacking_buffs
            .iter()
            .enumerate()
            .filter(|(_, b)| b.grant == crate::model::BuffGrant::ReloadSpeed)
            .map(|(i, b)| b.per_stack * stacks[i].current(t, b.duration) as f64)
            .sum::<f64>()
}
