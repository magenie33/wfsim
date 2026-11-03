// SPDX-License-Identifier: AGPL-3.0-or-later
//! Status detonation after a heavy hit has applied its own procs.

use super::*;

#[wfsim_gas_macro::metered]
impl DebuffState {
    pub(super) fn take_status_damage(&mut self, params: &FightParams, w: &CardWindows, now: f64) -> DamageVector {
        let mut damage = DamageVector::new();
        let mut groups: Vec<(DamageType, f64)> = Vec::new();
        self.dots.retain(|d| {
            if !matches!(d.dtype, DamageType::Slash | DamageType::Toxin | DamageType::Electricity | DamageType::Gas) {
                return true;
            }
            let source = dot::Source::at(params, d.dtype, now, w);
            let kind = if d.ignores_armor { DamageType::Cinematic } else { d.dtype };
            let landing = if consolidated(d.dtype) { d.landing } else { 1.0 };
            let mut total = d.live(&source) * f64::from(d.ticks_left) * landing;
            for tick in 0..d.ticks_left {
                let at = d.next_tick + f64::from(tick);
                if !consolidated(d.dtype) || !groups.iter().any(|&(ty, t)| ty == d.dtype && (t - at).abs() < 1e-9) {
                    total += d.accumulator_unit(&source) * landing;
                    if consolidated(d.dtype) { groups.push((d.dtype, at)); }
                }
            }
            damage.add(kind, total);
            false
        });
        self.dots_due = Some(self.dots.iter().map(|d| d.next_tick).fold(f64::INFINITY, f64::min));
        if let Some(h) = &self.heat {
            let ticks = if h.next_tick <= h.expiry { (h.expiry - h.next_tick).floor() + 1.0 } else { 0.0 };
            let bracket = h.bracket + params.element_at(DamageType::Heat, now, w);
            let faction = params.faction_at_time(now);
            damage.add(DamageType::Heat, ticks * bracket * (h.value * faction_at(faction, h.depth) + h.unit * faction));
            let strip = self.heat_ramp_up(now, params.status_duration_multiplier);
            self.heat_decay = Some((now, strip));
            self.heat = None;
        }
        damage
    }
}

#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn detonate_heavy_status(
    owner: Seat,
    body_index: usize,
    body: &mut Body,
    foe: &Foe,
    params: &FightParams,
    active: &FightParams,
    w: &CardWindows,
    heavy: bool,
    t: f64,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
) -> bool {
    if !heavy || !active.heavy_status.is_some_and(|s| s.detonate) || body.fallen { return false; }
    let mit = body.debuffs.mitigation(t, params.status_duration_multiplier, params.armor_strip_per_puncture, params.squad.enemy_armor_multiplier);
    let damage = body.debuffs.take_status_damage(params, w, t);
    let raw = damage.total();
    if raw <= 0.0 { return false; }
    let shares = TypeShares::of(&damage);
    let mut breakdown = Breakdown::default();
    let settled = body.state.apply(raw, shares, false, t, foe, false, &mit,
        params.arcane.overguard_multiplier, watching(rec, &mut breakdown));
    let killed = settled.killed;
    let broke = settled.broken;
    for ty in DamageType::ALL {
        if damage.get(ty) > 0.0 {
            let source = if ty == DamageType::Cinematic { DamageType::Slash } else { ty };
            r.sources.add_status(source, settled.effective * shares.share(ty));
        }
    }
    ledger::settle(r, rec, t, owner, body_index, shares.dominant(), PopKind::Status,
        &breakdown, settled, Some(&body.debuffs), ledger::Clock::Dot,
        || Instance { origin: crate::record::Origin::Status, base: raw, ..Instance::default() });
    r.dot_ticks += 1;
    r.note_kills(u32::from(killed), t, params.drop_is_in_reach(body.state.at));
    if let Some(pool) = broke { push_break_proc(&mut body.debuffs, owner, params, t, pool); }
    if killed {
        body.fallen = true;
        gal.bump_on_kill(params, t);
        arc.on_kill(params, t);
        arc.heavy_kill(active, t);
        body.debuffs.on_death(owner, params.acid_shells, foe);
    }
    killed
}
