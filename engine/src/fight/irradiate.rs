// SPDX-License-Identifier: AGPL-3.0-or-later
//! Immediate scene adapter; native producer and Sleep(0) ordering remain unbound.

use super::*;
use crate::rules::perks::secondary_irradiate::{HitClass, Listener, Source};

#[derive(Default, Clone)]
pub(super) struct SceneListener { listener: Listener, generation: u32 }

pub(super) struct ReadyEcho {
    victim: usize,
    coefficient: f64,
    scene_calibration: f64,
    strike: SpreadStrike,
}

#[wfsim_gas_macro::metered]
pub(super) fn submit_immediate_direct_echo(foe: &mut Body, victim: usize,
    params: &FightParams, arc: &mut ArcRuntime, context: impl FnOnce() -> SpreadStrike)
{
    if params.arcane.echo_share <= 0.0 || params.arcane.echo_radius_m <= 0.0 { return; }
    let radiation = foe.debuffs.confusion.len();
    if radiation < params.arcane.echo_needs_radiation_stacks as usize { return; }
    if foe.irradiate.generation != foe.state.generation {
        foe.irradiate.listener.invalidate();
        foe.irradiate.generation = foe.state.generation;
    }
    // One resolved scene loadout supplies a logical type and one fresh submission.
    // This is not a binding of native GetType, IsA, collision or callback ordering.
    let source = Some(Source { weapon_type: 0, source_proc_type: 30, arcane_intermediate: false });
    let Some(job) = arc.irradiate.activate(&mut foe.irradiate.listener, victim,
        radiation, HitClass::Ordinary, source, context()) else { return; };
    let packet = crate::model::DamageData::new(0.0_f32);
    arc.irradiate.collect(&foe.irradiate.listener, victim, radiation, source,
        |stored| stored == 0, false, packet.GetNumHits());
    // M59 remains a recipient calibration until the native producer/window is bound.
    // It must not be converted into a fabricated GetNumHits value.
    let Some(emission) = arc.irradiate.resume(job, true, params.arcane.echo_share, |original, _| {
        (original.raw_per_bucket, original.modded_base)
    }) else { return; };
    let mut strike = emission.context;
    strike.raw_per_bucket = emission.queried.0;
    strike.modded_base = emission.queried.1;
    arc.ready_echoes.push(ReadyEcho { victim, coefficient: emission.coefficient,
        scene_calibration: params.echo_multiplier, strike });
}

#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn drain_echoes(seat: Seat, w: &mut CardWindows, bodies: &mut [Body],
    params: &FightParams, active: &FightParams, gal: &mut GalStacks, arc: &mut ArcRuntime,
    r: &mut RunResult, rec: &mut crate::record::Record, d: &mut crate::rules::rng::Draws, t: f64)
{
    if arc.ready_echoes.is_empty() { return; }
    let mut ready = std::mem::take(&mut arc.ready_echoes);
    for echo in ready.drain(..) {
        let s = echo.strike;
        spread_from_echo(seat, w, bodies, echo.victim, echo.coefficient * echo.scene_calibration, params, active,
            s.raw_per_bucket, s.shares, s.crit_multiplier, s.crit_tier, s.attrition,
            s.modded_base, s.status_chance, &s.forced, &s.status_vector, gal, arc, r, rec, d, t);
    }
    arc.ready_echoes = ready;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fight::tests::{mono_body, windows_for};

    fn run(by: SpreadBy, radiation: usize) -> RunResult {
        run_with_seed(by, radiation, 100.0, 1.0)
    }

    fn run_with_seed(by: SpreadBy, radiation: usize, seed: f64, calibration: f64) -> RunResult {
        let mut p = FightParams { foe: Foe::training_dummy(), echo_multiplier: calibration, ..FightParams::default() };
        p.arcane.echo_share = 1.8;
        p.arcane.echo_radius_m = 7.0;
        p.arcane.echo_needs_radiation_stacks = 10;
        p.others.push(crate::formation::FoeSpec {
            id: "other".into(), params: p.foe.clone(), body_parts: mono_body(1.0),
            at: crate::rules::space::Vec2::new(p.target_at.x + 1.0, p.target_at.y),
        });
        let mut bodies: Vec<Body> = (0..p.body_count()).map(|i| {
            let spec = p.body(i).unwrap();
            Body { state: TargetState::spawn(spec.params, spec.at), debuffs: DebuffState::default(),
                fallen: false, irradiate: SceneListener::default() }
        }).collect();
        bodies[1].debuffs.confusion = vec![20.0; radiation];
        let mut arc = ArcRuntime::init(&p);
        let mut gal = GalStacks::default();
        let mut r = RunResult::default();
        let mut rec = crate::record::Record::window(0.0, 10.0, 10000, 0);
        let mut d = crate::rules::rng::Draws::new(41);
        let mut w = windows_for(&p);
        let vector = DamageVector::new().with(DamageType::Impact, seed);
        let shares = TypeShares::of(&vector);
        let composition = TargetComposition { vector, stage_mb: seed, aimed_total: seed, adaptive_enabled: false };
        let inst = crate::rules::chain::Instance { target: 1, share: 1.0, multishot: false,
            headshot: false, part_factor: 1.0, status_part_factor: 1.0, status_landing: 1.0 };
        spread_hit(Seat::WIELDER, &mut w, &inst, &mut bodies[1], &p.body(1).unwrap(),
            seed, shares, 1.0, 0, 1.0, seed, 0.0, &[], &composition, &p, &p,
            &mut gal, &mut arc, &mut r, &mut rec, &mut d, 0.0, by, false, true);
        drain_echoes(Seat::WIELDER, &mut w, &mut bodies, &p, &p, &mut gal, &mut arc, &mut r, &mut rec, &mut d, 0.0);
        assert!(arc.ready_echoes.is_empty());
        r
    }

    #[test]
    fn secondary_direct_bodies_can_echo_back_to_aimed_body() {
        for by in [SpreadBy::Chain, SpreadBy::Tendril, SpreadBy::Ricochet, SpreadBy::PunchThrough, SpreadBy::Contact] {
            let on = run(by, 10);
            let off = run(by, 9);
            assert!(on.taken.by_body().0[0] > off.taken.by_body().0[0], "{by:?}");
        }
    }

    #[test]
    fn radial_and_echo_damage_do_not_create_new_activations() {
        for by in [SpreadBy::Blast, SpreadBy::Echo] {
            assert_eq!(run(by, 10).taken.by_body().0[0].to_bits(), run(by, 9).taken.by_body().0[0].to_bits());
        }
    }

    #[test]
    fn m59_echo_damage_readings_keep_the_measured_seed_calibration() {
        for (weapon, direct, measured) in [
            ("laetum", 1536.0, 2764.8),
            ("laetum_incarnon", 320.0, 1152.0),
            ("laetum_incarnon", 960.0, 3456.0),
        ] {
            let calibration = crate::data::weapons::spec(weapon).unwrap().echo_multiplier;
            let r = run_with_seed(SpreadBy::Contact, 10, direct, calibration);
            assert!((r.taken.by_body().0[0] - measured).abs() < 1e-9, "{weapon}: {r:?}");
        }
    }
}
