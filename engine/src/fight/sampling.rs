use super::*;

/// The live stack count of every buff in [`Replay::buffs`], in that order.
///
/// The roster and this reader are two halves of one fact and sit as close
/// together as the code allows: `buff_roster` says what exists, this says
/// where it currently is. A `_ => 0` arm would let a rostered buff draw a flat
/// line forever and look like a finding, so the match is written to be read
/// against the roster, entry for entry.
///
/// Several containers hold "stacks" here because the sim's buffs genuinely
/// live in different shapes — decaying stack lists, single expiries, a
/// weapon passive. Normalising them into one number is this function's whole
/// job; nothing downstream should learn the difference.
#[allow(clippy::too_many_arguments)]
pub(super) fn sample_stacks(
    params: &FightParams,
    // THE ROSTER the answer is positional against — `FightParams::buff_roster`.
    // A LIST, not `&Replay`: the combat record needs the same roster and never
    // builds a replay.
    roster: &[BuffSeries],
    now: f64,
    arc: &mut ArcRuntime,
    gal: &mut GalStacks,
    buff_stacks: &mut [LiveStacks],
    ch_stacks: &[f64],
    ch_buff_expiry: f64,
    fire_rate_reload_expiry_seconds: f64,
    base_damage_reload_expiry_seconds: f64,
    base_damage_eximus_expiry_seconds: f64,
    streak_expiry: f64,
    tendrils: u32,
    crit_chance_hit_stacks: u32,
    bar: &BuffBar,
    combo: u32,
    // When a melee Incarnon's window closes; 0.0 before it is ever armed, so
    // "up" is `now < this` and needs no second flag.
    incarnon_until: f64,
    // MELEE INFLUENCE'S WINDOW, read the same way — and it is the one buff on a
    // melee build whose COVERAGE is the whole question, since the card is an
    // 18 s clock a roll opens rather than something a swing keeps up.
    influence_until: f64,
) -> Vec<(u16, f64)> {
    // u16, not u8: the Shot Combo Counter runs into the hundreds and a
    // capped curve would be a chart that lies about the fight it draws.
    let cap = |n: u32| n.min(u32::from(u16::MAX)) as u16;
    let live = |on: bool| u16::from(on);
    // WHEN IT RUNS OUT, beside how many are up. `f64::INFINITY` is a buff with
    // no clock at all — a permanent grant, or one whose end this loop does not
    // model — and `f64::NAN` is "the engine does not know", which the page
    // draws as no time rather than as a guess. Exact or absent, which is the
    // rule the ledger's own term names follow.
    let never = f64::INFINITY;
    let unknown = f64::NAN;
    let until = |on: bool, e: f64| if on { e } else { unknown };
    roster
        .iter()
        .map(|b| match b.id.as_str() {
            // A weapon passive, not a stack: it is up or it is not.
            "frenzy" => (live(bar.get(crate::perks::frenzy::BUFF_ID).is_some()), never),
            // PERMANENT (no trigger, no decay): whatever it was configured to,
            // for the whole run. `multishot` already carries it, so the count
            // is reconstructed from the fraction that survived the config.
            "evo_multishot" => (params.evo_multishot.map_or(0, |m| cap(m.stacks)), never),
            // Permanent like the one above: whatever it was configured to.
            "evo_reload_damage" => (params.evo_base_damage.map_or(0, |b| cap(b.stacks)), never),
            "condition_overload" => (cap(gal.co.current(now, dur(&params.co_stack))), unknown),
            "on_kill_multishot" => (cap(gal.multishot.current(now, dur(&params.multishot_stack))), unknown),
            "on_headshot_kill_cc" => {
                let n = ch_stacks.iter().filter(|&&e| e > now).count() as u32;
                (cap(n), ch_stacks.iter().copied().filter(|&e| e > now)
                    .fold(unknown, |a: f64, e| if a.is_nan() { e } else { a.min(e) }))
            }

            crate::rage::BUFF_ID => (cap((arc.rage_bonus(now) * 100.0).round() as u32), unknown),
            // THE WHOLE STACKING FAMILY, by id. The roster pushed these ids
            // from the same Vec this reads, so a rostered buff can never fall
            // through to a zero it did not earn.
            other if params.stacking_buffs.iter().any(|b| b.id == other) => {
                let i = params.stacking_buffs.iter().position(|b| b.id == other).unwrap_or(0);
                let d = params.stacking_buffs[i].duration;
                let n = cap(buff_stacks[i].current(now, d));
                (n, buff_stacks[i].expires_at().unwrap_or(unknown))
            }
            // Read off the loop's own counter rather than re-derived: the
            // fight is the only thing that knows how many are up.
            "tendrils" => (cap(tendrils), never),
            // Both halves of Pyrana Prime's passive live on the bar.
            id @ (crate::weapons_data::KillStreakSummonSpec::BUFF_ID
            | crate::weapons_data::KillStreakSummonSpec::STREAK_BUFF_ID) => bar
                .get(id)
                .map_or((0, unknown), |x| (cap(x.stacks), x.expiry_seconds.unwrap_or(never))),
            // ...and the same for the combo, which is why the frame series is
            // u16: this is the first buff whose honest count runs past 255.
            "sniper_combo" => (cap(combo), unknown),
            // Off the loop's own clock, like the windows around it: the fight
            // is the only thing that knows when the heavy attack that armed it
            // went down.
            "melee_incarnon" => (live(now < incarnon_until), until(now < incarnon_until, incarnon_until)),
            "arcane:melee_influence" => (
                live(now < influence_until),
                until(now < influence_until, influence_until),
            ),
            "on_headshot_cc" => (live(now < ch_buff_expiry), until(now < ch_buff_expiry, ch_buff_expiry)),
            "on_kill_cd" => {
                let e = arc.crit_damage_kill_expiry_seconds();
                (live(now < e), until(now < e, e))
            }
            "on_reload_bd" => (live(now < base_damage_reload_expiry_seconds), until(now < base_damage_reload_expiry_seconds, base_damage_reload_expiry_seconds)),
            "on_eximus_weakpoint_bd" => (live(now < base_damage_eximus_expiry_seconds), until(now < base_damage_eximus_expiry_seconds, base_damage_eximus_expiry_seconds)),
            // Off the loop's own counter, like the tendrils: only the fight
            // knows how many hits are in the pile.
            "crit_per_hit" => (cap(crit_chance_hit_stacks), never),
            "on_reload_fr" => (live(now < fire_rate_reload_expiry_seconds), until(now < fire_rate_reload_expiry_seconds, fire_rate_reload_expiry_seconds)),
            "evo_headshot_streak" => (live(now < streak_expiry), until(now < streak_expiry, streak_expiry)),
            // The perk keeps its stacks on the BAR, not in `arcane.buffs`.
            "arcane:secondary_enervate" => {
                (cap(bar.get("secondary_enervate").map_or(0, |b| b.stacks)), unknown)
            }
            other => match other.strip_prefix("arcane:") {
                // One card per arcane: every spec it owns shares a count, so
                // the first one answers for all of them.
                Some(owner) => (cap(arc.owner_stacks(&params.arcane, owner, now)), unknown),
                None => (0, unknown),
            },
        })
        .collect()
}
