// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::state::{Range, Result};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};

struct Tokens {
    at: Instant,
    balance: f64,
    rate: f64,
}

pub struct Budget {
    pub cores: usize,
    pub cancelled: AtomicBool,
    tokens: Mutex<Tokens>,
}

impl Budget {
    pub fn new(cores: usize) -> Self {
        Self {
            cores,
            cancelled: AtomicBool::new(false),
            tokens: Mutex::new(Tokens {
                at: Instant::now(),
                balance: 0.0,
                rate: 0.0,
            }),
        }
    }
    pub fn set(&self, percent: f64) {
        let mut t = self.tokens.lock().expect("budget lock");
        refill(&mut t);
        t.rate = self.cores as f64 * percent.clamp(0.0, 100.0) / 100.0;
        t.balance = t.balance.min(t.rate * 0.25);
    }
    pub fn lanes(&self) -> usize {
        let rate = self.tokens.lock().expect("budget lock").rate;
        rate.ceil().max(1.0) as usize
    }
    pub fn call<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<(T, f64)> {
        let reserved = loop {
            if self.cancelled.load(Ordering::Relaxed) {
                return Err("computation cancelled".into());
            }
            let mut t = self.tokens.lock().expect("budget lock");
            refill(&mut t);
            let need = (t.rate * 0.25).min(0.001);
            if t.rate > 0.0 && t.balance >= need {
                t.balance -= need;
                break need;
            }
            let wait = if t.rate > 0.0 {
                ((need - t.balance) / t.rate).clamp(0.001, 0.1)
            } else {
                0.1
            };
            drop(t);
            std::thread::sleep(Duration::from_secs_f64(wait));
        };
        let began = Instant::now();
        let out = f();
        let spent = began.elapsed().as_secs_f64();
        let mut t = self.tokens.lock().expect("budget lock");
        refill(&mut t);
        t.balance -= spent - reserved;
        drop(t);
        Ok((out?, spent * 1000.0))
    }
}

fn refill(t: &mut Tokens) {
    let now = Instant::now();
    t.balance = (t.balance + now.duration_since(t.at).as_secs_f64() * t.rate).min(t.rate * 0.25);
    t.at = now;
}

pub fn target(range: &Range, total_cpu: f64, own_cpu: f64, previous: f64, held: bool) -> f64 {
    if held || range.max == 0 {
        return 0.0;
    }
    let outside = (total_cpu - own_cpu).clamp(0.0, 100.0);
    let wanted = (100.0 - outside).clamp(range.min as f64, range.max as f64);
    if wanted > previous {
        wanted.min((previous + 5.0).max(range.min as f64))
    } else {
        wanted
    }
}

pub struct Pressure {
    system: sysinfo::System,
    battery: Option<battery::Manager>,
    pid: sysinfo::Pid,
}

impl Pressure {
    pub fn new() -> Result<Self> {
        let mut system = sysinfo::System::new();
        system.refresh_cpu_usage();
        Ok(Self {
            system,
            battery: battery::Manager::new().ok(),
            pid: sysinfo::get_current_pid()?,
        })
    }
    pub fn sample(&mut self) -> (f64, f64, Option<&'static str>) {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[self.pid]),
            true,
            sysinfo::ProcessRefreshKind::nothing().with_cpu(),
        );
        let cores = self.system.cpus().len().max(1) as f64;
        let total = self.system.global_cpu_usage() as f64;
        let own = self
            .system
            .process(self.pid)
            .map(|p| p.cpu_usage() as f64 / cores)
            .unwrap_or(0.0);
        let low_memory = self.system.available_memory()
            < (self.system.total_memory() / 20).max(256 * 1024 * 1024);
        let on_battery = self
            .battery
            .as_ref()
            .and_then(|m| m.batteries().ok())
            .is_some_and(|b| {
                b.filter_map(|x| x.ok())
                    .any(|b| b.state() == battery::State::Discharging)
            });
        (
            total,
            own,
            if on_battery {
                Some("on battery")
            } else if low_memory {
                Some("low memory")
            } else {
                None
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_respond_to_outside_work_and_keep_fixed_targets() {
        let r = Range { min: 0, max: 50 };
        assert_eq!(target(&r, 95.0, 0.0, 50.0, false), 5.0);
        assert_eq!(target(&r, 95.0, 45.0, 50.0, false), 50.0);
        assert_eq!(target(&r, 0.0, 0.0, 0.0, false), 5.0);
        assert_eq!(
            target(&Range { min: 50, max: 100 }, 100.0, 0.0, 100.0, false),
            50.0
        );
        assert_eq!(
            target(&Range { min: 100, max: 100 }, 100.0, 0.0, 0.0, false),
            100.0
        );
        assert_eq!(
            target(&Range { min: 0, max: 0 }, 0.0, 0.0, 100.0, false),
            0.0
        );
        assert_eq!(target(&r, 0.0, 0.0, 50.0, true), 0.0);
    }
    #[test]
    fn cancellation_wakes_a_zero_budget_without_computing() {
        let b = Budget::new(1);
        b.cancelled.store(true, Ordering::Relaxed);
        assert!(b
            .call(|| -> Result<()> { panic!("zero budget ran") })
            .is_err());
    }
    #[test]
    fn one_percent_charges_time_globally_and_excludes_waits() -> Result<()> {
        let b = Budget::new(1);
        b.set(1.0);
        let began = Instant::now();
        let (_, measured) = b.call(|| {
            std::thread::sleep(Duration::from_millis(5));
            Ok(())
        })?;
        b.call(|| Ok(()))?;
        assert!(began.elapsed() >= Duration::from_millis(300));
        assert!(measured < 100.0);
        Ok(())
    }
}
