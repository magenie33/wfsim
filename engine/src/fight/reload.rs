use super::*;

/// Live reload time for the active form: the arcane's reload-speed sources
/// (Merciless r5 static, Conjunction Voltage stacks) and Lethal
/// Rearmament's headshot stacks join the form's reload-speed BUCKET —
/// time = base / (1 + bucket + live additions).
#[wfsim_gas_macro::metered]
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
#[wfsim_gas_macro::metered]
pub(super) fn rescale_reload(secs: f64, bucket: f64, live: f64) -> f64 {
    reload_span(secs, bucket, live)
}

/// A RELOAD IS PAID FOR WHILE IT RUNS, not priced when it starts.
///
/// The arithmetic is a work integral, which is what makes a reload-speed total
/// mean anything: a reload is `secs x (1 + bucket)` seconds of WORK and a total
/// of `add` retires it at `1 + bucket + add` per second.
///
/// NO LAPSING WINDOW. Ready Retaliation changes only when a reload or a
/// transform ENDS, so it is up for a whole reload or not at all — nothing lapses
/// mid-reload and there is no partial-rate branch. If a lapsing reload buff
/// ever exists, this is where it goes.
///
/// `secs` arrives ALREADY divided by `(1 + bucket)` — it is the modded reload —
/// so multiplying it back out is what recovers the work, and a weapon with no
/// live bonus at all falls straight through to `secs`.
#[wfsim_gas_macro::metered]
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
#[wfsim_gas_macro::metered]
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

/// Instant reload-start Cold pulse, queued with the other timed effects.
#[derive(Debug, Clone, Copy)]
pub(super) struct ReloadCold {
    pub(super) owner: Seat,
    pub(super) at: f64,
    pub(super) center: crate::rules::space::Vec2,
    pub(super) rounds: u32,
    pub(super) radius_m: f64,
}

#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn schedule_reload_effects(
    form: &FightParams,
    ammo: &mut Ammo,
    loaded: f64,
    capacity: f64,
    at: f64,
    seconds: f64,
    from_empty: bool,
) {
    if form.reload_grenade.is_some() {
        ammo.grenade_thrown_at = Some((at + seconds * THROW_AT_RELOAD_SHARE_UNMEASURED, from_empty));
    }
    if let Some(radius) = form.reload_cold_radius_per_round_m {
        let rounds = reload_draw(capacity, loaded).max(0.0) as u32;
        ammo.reload_cold = Some(ReloadCold {
            owner: Seat::WIELDER,
            at,
            center: form.player_at,
            rounds,
            radius_m: f64::from(rounds) * radius,
        });
    }
}

#[wfsim_gas_macro::metered]
pub(super) fn enqueue_reload_effects(ammo: &mut Ammo, owner: Seat, entities: &mut Vec<Entity>) {
    if let Some((at, from_empty)) = ammo.grenade_thrown_at.take() {
        entities.push(Entity::Volley(Volley { owner, at, from_empty }));
    }
    if let Some(wave) = ammo.reload_cold.take() {
        entities.push(Entity::ReloadCold(ReloadCold { owner, ..wave }));
    }
}

/// Aksondol wiki: no hit, no lingering field, no Frostbite or Empowered trigger.
/// This arena has no occluders; every body in reach has line of sight.
#[wfsim_gas_macro::metered]
pub(super) fn reload_cold_wave(
    wave: ReloadCold,
    turn: &mut Turn,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    bodies: &mut [Body],
) {
    let active = match &turn.params.cycle {
        Some(cy) if turn.active.reload_cold_radius_per_round_m.is_none() => &cy.base_form,
        _ => turn.active,
    };
    if turn.params.status_duration_multiplier <= 0.0 { return; }
    let mut encumber_done = false;
    for (i, here) in bodies.iter_mut().enumerate() {
        let Some(spec) = turn.params.body(i) else { continue };
        if wave.rounds == 0 || !crate::rules::space::caught_by_blast(spec.at.distance(wave.center), wave.radius_m) {
            continue;
        }
        process_ticks(turn.w, here, turn.gal, turn.arc, wave.at + 1e-9,
            turn.params, active, r, rec, &mut turn.d.status, spec.params, i);
        if !matches!(here.state.phase, Phase::Physical)
            || spec.params.status_immunities.contains(&DamageType::Cold)
        {
            continue;
        }
        here.debuffs.prune(wave.at, turn.params.status_duration_multiplier);
        let mut bonus = None;
        for _ in 0..wave.rounds {
            let applied = here.debuffs.apply_cold_proc(wave.at, turn.params.status_duration_multiplier,
                here.state.overguard > 0.0, spec.params.stack_caps, spec.params.cannot_be_frozen);
            r.procs += u32::from(applied);
            if let Some(proc) = encumber_proc(turn.params.arcane.encumber_chance, applied,
                &mut encumber_done, &mut turn.d.extra)
            {
                bonus = Some(proc);
            }
        }
        if let Some(proc) = bonus.filter(|p| !spec.params.status_immunities.contains(p)) {
            let mit = here.debuffs.mitigation(wave.at, turn.params.status_duration_multiplier,
                active.armor_strip_per_puncture, turn.params.squad.enemy_armor_multiplier);
            let scale = weapon_status_scale(turn, active, wave.at);
            settle_procs(vec![proc], wave.owner, wave.at, scale , &mut here.debuffs, turn.gal, turn.arc, &mut here.state, turn.params,
                active, &mit, r, rec, &mut turn.d.status, spec.params, DEPTH_PROC);
        }
    }
}

/// An ordinary weapon status has the weapon's own base and live damage buffs.
#[wfsim_gas_macro::metered]
pub(super) fn weapon_status_scale(turn: &mut Turn, active: &FightParams, at: f64) -> InstanceScale {
    let (reload, eximus) = turn.w.base_damage_additions(active, at);
    let add = live_base_damage_additions(turn.params, active, turn.arc, turn.buff_stacks,
        reload + eximus, at, false);
    let base = active.dot_modified_base.unwrap_or_else(|| active.damage.total());
    let mb_live = base * (1.0 + active.base_damage_bonus + add) / (1.0 + active.base_damage_bonus);
    let status_damage_live = buff_total(active, crate::model::BuffGrant::StatusDamage, turn.buff_stacks, at);
    InstanceScale { mb_live, crit_multiplier: 1.0, part_factor: 1.0, landing: 1.0,
        attrition: 1.0, xh_bracket: active.extra_hit_bracket(at, turn.w), status_damage_live }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ColdTrailTick {
    pub(super) owner: Seat,
    pub(super) body: usize,
    pub(super) at: f64,
    pub(super) left: u32,
    pub(super) interval_seconds: f64,
}

/// Static bodies enter the trail when the projectile passes them.
#[wfsim_gas_macro::metered]
pub(super) fn lay_cold_trail(owner: Seat, active: &FightParams, at: f64, entities: &mut Vec<Entity>) {
    let Some(trail) = active.cold_trail else { return };
    assert!(trail.tick_interval_seconds > 0.0);
    let ticks = (trail.duration_seconds / trail.tick_interval_seconds).floor() as u32;
    if ticks == 0 { return; }
    for body in active.struck_bodies() {
        let Some(spec) = active.body(body) else { continue };
        let flight = active.flight_seconds((active.range_to(spec.at) - crate::rules::space::BODY_RADIUS_M).max(0.0));
        entities.push(Entity::ColdTrail(ColdTrailTick { owner, body,
            at: at + flight + trail.tick_interval_seconds, left: ticks,
            interval_seconds: trail.tick_interval_seconds }));
    }
}

#[wfsim_gas_macro::metered]
pub(super) fn cold_trail_tick(tick: ColdTrailTick, turn: &mut Turn, r: &mut RunResult,
    rec: &mut crate::record::Record, bodies: &mut [Body]) {
    let active = turn.params;
    let Some(spec) = turn.params.body(tick.body) else { return };
    let Some(here) = bodies.get_mut(tick.body) else { return };
    process_ticks(turn.w, here, turn.gal, turn.arc, tick.at + 1e-9, turn.params, active,
        r, rec, &mut turn.d.status, spec.params, tick.body);
    if !matches!(here.state.phase, Phase::Physical) || spec.params.status_immunities.contains(&DamageType::Cold) { return; }
    let mit = here.debuffs.mitigation(tick.at, turn.params.status_duration_multiplier,
        active.armor_strip_per_puncture, turn.params.squad.enemy_armor_multiplier);
    let mut procs = vec![DamageType::Cold];
    if let Some(proc) = encumber_proc(turn.params.arcane.encumber_chance, true, &mut false, &mut turn.d.extra)
        .filter(|p| !spec.params.status_immunities.contains(p)) { procs.push(proc); }
    let scale = weapon_status_scale(turn, active, tick.at);
    settle_procs(procs, tick.owner, tick.at, scale, &mut here.debuffs, turn.gal, turn.arc,
        &mut here.state, turn.params, active, &mit, r, rec, &mut turn.d.status, spec.params, DEPTH_PROC);
}
