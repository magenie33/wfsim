
/// What a KILL gives the weapon that made it, as a fraction of the enemy's
/// affinity. VERBATIM (wiki Affinity): "Kill with weapons: Half Affinity goes
/// to the Warframe and half to the killing weapon."
///
/// The general 25/75 split on that page is for SHARED affinity — orbs, ally
/// kills, ability casts — and is not this. A weapon that killed something takes
/// half, whatever else is equipped.
pub(super) const WEAPON_AFFINITY_SHARE: f64 = 0.5;
pub(super) const BEAM_RAMP_SECONDS: f64 = 0.6;
pub(super) const BEAM_DECAY_DELAY: f64 = 0.8;
pub(super) const BEAM_DECAY_SECONDS: f64 = 2.0;

/// Source-bound state is elapsed seconds (binary32); legacy state is normalized progress.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BeamRamp {
    state: f64,
    pub(super) last_tick: Option<f64>,
}

#[derive(Clone, Copy)]
struct NativeRamp {
    floor: f32,
    ceiling: f32,
    rise: f32,
    delay: f32,
    divisor: f32,
}

#[wfsim_gas_macro::metered]
impl NativeRamp {
    fn global(self, dt: f32, time: f32, idle: f32, owner: bool, record: bool) -> (f32, f32) {
        if owner || record {
            ((time + dt).min(self.rise), 0.)
        } else {
            let idle = idle + dt;
            (self.decay(dt, time, idle, 0.), idle)
        }
    }

    fn record_gate(dt: f32, period: f32, idle: f32, owner: bool, record: bool) -> (f32, bool) {
        let updated = idle + dt;
        let eligible = if dt <= period { period >= updated } else { dt - period >= idle };
        (updated, (owner || record) && eligible)
    }

    fn decay(self, dt: f32, time: f32, shared_idle: f32, local_idle: f32) -> f32 {
        if shared_idle > self.delay || local_idle > self.delay {
            (time - dt / self.divisor).max(0.)
        } else { time }
    }

    fn factor(self, local: &mut f32, shared: &mut f32, per_target: bool, marker: f32, promote: bool) -> (f32, f32) {
        let selected = if per_target { local } else { shared };
        let progress = (*selected / self.rise).clamp(0., 1.);
        if marker != 1. || promote { *selected = selected.max(self.rise); }
        (self.floor + (self.ceiling - self.floor) * progress, progress)
    }

    // The fight has accepted-hit timestamps, not native update callbacks. Use
    // its tick interval for idle updates; the exact gate-crossing frame is open.
    fn idle_time(self, mut time: f32, gap: f64, period: f64) -> f32 {
        if gap <= 0. || time == 0. { return time; }
        if gap > f64::from(self.delay + self.rise * self.divisor) + period { return 0.; }
        let mut idle = 0.;
        let mut remaining = gap;
        while remaining > 1e-12 && time > 0. {
            let dt = remaining.min(period);
            (time, idle) = self.global(dt as f32, time, idle, false, false);
            remaining -= dt;
        }
        time
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct TargetRamp {
    generation: Option<u32>,
    clock: BeamRamp,
    factor: f64,
}

#[wfsim_gas_macro::metered]
impl TargetRamp {
    pub(super) fn hit(
        &mut self, generation: u32, at: f64, step: f64, floor: f64, source: crate::model::BeamRampSpec,
    ) -> f64 {
        if self.generation != Some(generation) {
            *self = Self { generation: Some(generation), ..Default::default() };
        }
        if self.clock.last_tick != Some(at) {
            self.factor = self.clock.tick(at, step, floor, Some(source));
        }
        self.factor
    }
}

#[wfsim_gas_macro::metered]
impl BeamRamp {
    /// Source-bound updates work in elapsed float32 time. The first accepted
    /// hit has no yet-qualified preceding update, so starts at zero; later
    /// contacts advance on the simulator's tick schedule before interpolation.
    pub(super) fn tick(&mut self, now: f64, tick_seconds: f64, floor: f64, source: Option<crate::model::BeamRampSpec>) -> f64 {
        if let Some(source) = source {
            let native = NativeRamp { floor: floor as f32, ceiling: 1., rise: source.rise_seconds as f32,
                delay: source.cooldown_delay_seconds as f32, divisor: source.cooldown_divisor as f32 };
            let mut time = self.state as f32;
            if let Some(prev) = self.last_tick {
                let gap = (now - prev).max(0.);
                let idle = (gap - tick_seconds).max(0.);
                time = native.idle_time(time, idle, tick_seconds);
                // A contact after a fully cold gap opens a new interval.
                if time > 0. || idle <= f64::from(native.delay) {
                    let dt = gap.min(tick_seconds) as f32;
                    if source.per_target {
                        if NativeRamp::record_gate(dt, tick_seconds as f32, 0., true, true).1 {
                            time = (time + dt).min(native.rise);
                        }
                    } else {
                        (time, _) = native.global(dt, time, 0., true, false);
                    }
                }
            }
            let mut unused = 0.;
            let (factor, _) = if source.per_target { native.factor(&mut time, &mut unused, true, 1., false) }
                else { native.factor(&mut unused, &mut time, false, 1., false) };
            self.state = f64::from(time);
            self.last_tick = Some(now);
            return f64::from(factor);
        }
        if let Some(prev) = self.last_tick {
            let idle = now - prev - tick_seconds;
            if idle > BEAM_DECAY_DELAY {
                self.state = (self.state - (idle - BEAM_DECAY_DELAY) / BEAM_DECAY_SECONDS).max(0.);
            }
        }
        let mult = floor + (1.0 - floor) * self.state;
        self.state = (self.state + tick_seconds / BEAM_RAMP_SECONDS).min(1.0);
        self.last_tick = Some(now);
        mult
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simultaneous_links_advance_a_target_clock_once_and_respawn_starts_cold() {
        let source = crate::model::BeamRampSpec {
            rise_seconds: 0.6, cooldown_delay_seconds: 0.8, cooldown_divisor: 2.0, per_target: true,
        };
        let mut ramp = TargetRamp::default();
        assert_eq!(ramp.hit(1, 0., 0.1, 0.2, source), f64::from(0.2_f32));
        assert_eq!(ramp.hit(1, 0., 0.1, 0.2, source), f64::from(0.2_f32));
        assert!((ramp.hit(1, 0.1, 0.1, 0.2, source) - (0.2 + 0.8 / 6.)).abs() < 1e-7);
        assert_eq!(ramp.hit(2, 0.2, 0.1, 0.2, source), f64::from(0.2_f32));
    }

    #[test]
    fn reacquisition_keeps_a_live_record_and_does_not_share_another_instances_time() {
        let source = crate::model::BeamRampSpec { rise_seconds: 0.6, cooldown_delay_seconds: 0.8,
            cooldown_divisor: 2.0, per_target: true };
        let mut a = TargetRamp::default();
        let mut b = TargetRamp::default();
        let cold = f64::from(0.2_f32);
        assert_eq!(a.hit(1, 0., 0.1, 0.2, source), cold);
        assert!(a.hit(1, 0.2, 0.1, 0.2, source) > cold);
        for i in 3..10 { a.hit(1, f64::from(i) * 0.1, 0.1, 0.2, source); }
        assert_eq!(b.hit(1, 1., 0.1, 0.2, source), cold);
        assert_eq!(a.hit(1, 1.2, 0.1, 0.2, source), 1.);
        assert_eq!(a.hit(1, 4., 0.1, 0.2, source), cold);
    }

    #[test]
    fn native_cooldown_divides_time_state_before_normalizing_progress() {
        let source = Some(crate::model::BeamRampSpec {
            rise_seconds: 0.6, cooldown_delay_seconds: 0.8, cooldown_divisor: 2.0, per_target: false,
        });
        let mut native = BeamRamp { state: f64::from(0.6_f32), last_tick: Some(0.0) };
        let mut ordinary = BeamRamp { state: 1.0, last_tick: Some(0.0) };
        // The native state is time, and the entire crossing update is charged.
        let factor = native.tick(1.5, 0.1, 0.2, source);
        assert!(factor > 0.5 && factor < 0.8, "{factor}");
        assert!((ordinary.tick(1.5, 0.1, 0.2, None) - 0.76).abs() < 1e-12);
        let mut empty = BeamRamp { state: f64::from(0.6_f32), last_tick: Some(0.0) };
        assert_eq!(empty.tick(2.2, 0.1, 0.2, source), f64::from(0.2_f32));
    }

    #[test]
    fn native_ramp_arithmetic_matches_every_retained_gate_vector() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/native-beam-timing.json")).unwrap();
        let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
        let flag = |v: &serde_json::Value| v.as_i64().unwrap() != 0;
        let mut count = 0;
        for row in fixture["profiles"].as_array().unwrap() {
            let p = &row["profile"];
            let n = NativeRamp { floor: f(&p[0]), ceiling: f(&p[1]), rise: f(&p[2]), divisor: f(&p[3]), delay: f(&p[4]) };
            let per_target = flag(&p[5]);
            for key in ["global_cases", "record_cases", "decay_cases", "factor_cases"] {
                for case in row[key].as_array().unwrap() {
                    let i = &case["input"];
                    let o = &case["output"];
                    let eq = |actual: f32, wanted: &serde_json::Value| assert_eq!(actual.to_bits(), f(wanted).to_bits(), "{key}: {p} {case}");
                    match key {
                        "global_cases" => {
                            let (time, idle) = n.global(f(&i[0]), f(&i[1]), f(&i[2]), flag(&i[3]), flag(&i[4]));
                            eq(time, &o[0]); eq(idle, &o[1]);
                        }
                        "record_cases" => {
                            let (idle, eligible) = NativeRamp::record_gate(f(&i[0]), f(&i[1]), f(&i[2]), flag(&i[3]), flag(&i[4]));
                            eq(idle, &o[0]); assert_eq!(eligible, o[1].as_bool().unwrap(), "{case}");
                        }
                        "decay_cases" => eq(n.decay(f(&i[0]), f(&i[1]), f(&i[2]), f(&i[3])), o),
                        "factor_cases" => {
                            let (mut local, mut shared) = (f(&i[0]), f(&i[1]));
                            let (factor, progress) = n.factor(&mut local, &mut shared, per_target, f(&i[2]), flag(&i[3]));
                            eq(factor, &o["factor"]); eq(progress, &o["progress"]);
                            eq(local, &o["local_after"]); eq(shared, &o["global_after"]);
                        }
                        _ => unreachable!(),
                    }
                    count += 1;
                }
            }
        }
        assert_eq!(count, 4654);
    }


    #[test]
    fn shared_activity_matches_the_two_instance_native_controls() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/native-beam-instance.json")).unwrap();
        let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
        let mut states = [(0_f32, 0_f32); 2];
        let rows = fixture["cases"].as_array().unwrap();
        for row in rows {
            let p = crate::data::weapons::base_panel(row["mode"].as_str().unwrap(), false);
            let ramp = p.beam_ramp.unwrap();
            assert!(!ramp.per_target);
            let n = NativeRamp { floor: p.beam_ramp_floor as f32, ceiling: 1., rise: ramp.rise_seconds as f32,
                delay: ramp.cooldown_delay_seconds as f32, divisor: ramp.cooldown_divisor as f32 };
            if row["step"] == 0 { states = [(0., 0.); 2]; }
            let i = row["behavior_instance"].as_u64().unwrap() as usize;
            let other = 1-i;
            let eq = |state: (f32, f32), key: &str| {
                assert_eq!(state.0.to_bits(), f(&row[key][0]).to_bits(), "{row}");
                assert_eq!(state.1.to_bits(), f(&row[key][1]).to_bits(), "{row}");
            };
            eq(states[i], "shared_before");
            eq(states[other], "other_instance_before");
            let activity = row["activity"].as_i64().unwrap() != 0;
            states[i] = n.global(f(&row["dt"]), states[i].0, states[i].1, activity, false);
            let (factor, _) = n.factor(&mut 0., &mut states[i].0, false, 1., false);
            assert_eq!(factor.to_bits(), f(&row["factor"]).to_bits(), "{row}");
            eq(states[i], "shared_after");
            eq(states[other], "other_instance_after");
        }
        assert_eq!(rows.len(), 18);
    }

}
