// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    state::{self, Config, Result},
    trust,
    update::{self, Host, Installed},
};
use semver::Version;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

pub const ENGINE_CHANGED: i32 = 75;

struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn run(dir: &Path, quit: Arc<AtomicBool>) -> Result<()> {
    let _lock = crate::instance_lock(dir, "supervisor.lock")?;
    let base = update::bootstrap(dir)?;
    run_with(
        dir,
        quit,
        base,
        Version::parse(crate::BUILD)?,
        update::sources(&Config::load(dir)?.server),
        trust::PUBLIC_KEYS.iter().map(|s| s.to_string()).collect(),
        update::INTERVAL,
    )
}

fn run_with(
    dir: &Path,
    quit: Arc<AtomicBool>,
    base: PathBuf,
    version: Version,
    sources: Vec<String>,
    keys: Vec<String>,
    interval: Duration,
) -> Result<()> {
    let nonce = state::random_hex(12)?;
    state::write(
        &dir.join("status.json"),
        &json!({"run":nonce,"pid":std::process::id(),"at":state::timestamp(),"state":"starting"}),
    )?;
    let mut installed = update::installed(dir)?;
    let key_refs = keys.iter().map(String::as_str).collect::<Vec<_>>();
    let mut active = update::current(dir, &installed, &key_refs).filter(|h| h.version >= version);
    if installed.current.is_some() && active.is_none() {
        installed.current = installed.previous.take();
        update::save(dir, &installed)?;
        active = update::current(dir, &installed, &key_refs).filter(|h| h.version >= version);
    }
    let mut next_check = Instant::now();
    let mut checking = None;
    let mut activating = false;
    'workers: loop {
        if quit.load(Ordering::Relaxed) || stopped(dir, &nonce) {
            return Ok(());
        }
        let _ = std::fs::remove_file(dir.join("host-update.json"));
        let path = active.as_ref().map(|h| &h.path).unwrap_or(&base);
        let child = Command::new(path)
            .arg("--data-dir")
            .arg(dir)
            .args(["join", "--worker"])
            .env("WFSIM_CLI_RUN", &nonce)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn();
        let mut child = match child {
            Ok(child) => Worker(child),
            Err(error) if active.is_some() => {
                eprintln!("New CLI could not start: {error}; restoring the previous version.");
                active = rollback(
                    dir,
                    &mut installed,
                    active.as_ref().expect("host"),
                    &key_refs,
                    &version,
                )?;
                activating = false;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let began = Instant::now();
        let mut candidate: Option<Host> = None;
        let mut drain_at = None;
        loop {
            let now = Instant::now();
            let stopping = quit.load(Ordering::Relaxed) || stopped(dir, &nonce);
            if quit.load(Ordering::Relaxed) {
                state::write(&dir.join("stop.json"), &json!({"run":nonce}))?;
            }
            if activating && !stopping {
                let ready =
                    state::read::<Value>(&dir.join("host-ready.json")).unwrap_or(Value::Null);
                let host = active.as_ref().expect("activating host");
                if ready["run"] == nonce
                    && ready["pid"] == child.0.id()
                    && ready["version"] == host.version.to_string()
                {
                    installed.previous = installed.current.replace(host.signed.clone());
                    installed.highest = Some(host.version.to_string());
                    update::save(dir, &installed)?;
                    if let Err(error) = update::prune(dir, &installed) {
                        eprintln!("CLI cache cleanup deferred: {error}");
                    }
                    report(dir, "current", Some(&host.version), None)?;
                    activating = false;
                    println!(
                        "WFSim CLI {} is running; device and settings preserved.",
                        host.version
                    );
                } else if began.elapsed() >= Duration::from_secs(30) {
                    let _ = child.0.kill();
                    let _ = child.0.wait();
                    active = rollback(dir, &mut installed, host, &key_refs, &version)?;
                    activating = false;
                    continue 'workers;
                }
            }
            if !stopping
                && !activating
                && candidate.is_none()
                && checking.is_none()
                && now >= next_check
            {
                let (tx, rx) = mpsc::channel();
                checking = Some(rx);
                let (directory, current, sources, keys) = (
                    dir.to_owned(),
                    version.clone(),
                    sources.clone(),
                    keys.clone(),
                );
                std::thread::spawn(move || {
                    let keys = keys.iter().map(String::as_str).collect::<Vec<_>>();
                    let _ = tx.send(update::check(&directory, &current, &sources, &keys));
                });
                let jitter =
                    u64::from_str_radix(&state::random_hex(2)?, 16)? % (interval.as_secs() / 5 + 1);
                next_check = now + interval + Duration::from_secs(jitter);
            }
            if let Some(rx) = &checking {
                if let Ok(result) = rx.try_recv() {
                    checking = None;
                    if !stopping {
                        match result {
                            Ok(Some(host)) => {
                                report(dir, "ready", Some(&host.version), None)?;
                                println!("Verified WFSim CLI {}; returning unfinished work before upgrading.", host.version);
                                candidate = Some(host);
                                drain_at = Some(now);
                                state::write(&dir.join("host-update.json"), &json!({"run":nonce}))?;
                            }
                            Ok(None) => {}
                            Err(error) => {
                                eprintln!(
                                    "CLI update deferred: {error}; current computing continues."
                                );
                                report(dir, "deferred", None, Some(error.to_string()))?;
                            }
                        }
                    }
                }
            }
            if let Some(status) = child.0.try_wait()? {
                if stopping || quit.load(Ordering::Relaxed) || stopped(dir, &nonce) {
                    return Ok(());
                }
                if status.code() == Some(update::HOST_CHANGED) && candidate.is_some() {
                    active = candidate.take();
                    activating = true;
                    continue 'workers;
                }
                if status.code() == Some(ENGINE_CHANGED) && !activating {
                    println!("Engine update ready; restarting computation.");
                    continue 'workers;
                }
                if let Some(host) = &active {
                    active = rollback(dir, &mut installed, host, &key_refs, &version)?;
                    activating = false;
                    continue 'workers;
                }
                eprintln!("Compute process exited with {status}; restarting in 15 seconds.");
                for _ in 0..150 {
                    if quit.load(Ordering::Relaxed) || stopped(dir, &nonce) {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                continue 'workers;
            }
            if !stopping
                && drain_at.is_some_and(|at| now.duration_since(at) >= Duration::from_secs(120))
            {
                let _ = child.0.kill();
                let _ = child.0.wait();
                eprintln!("Upgrade shutdown timed out; pending answers stay on disk and outstanding leases expire.");
                active = candidate.take();
                activating = true;
                continue 'workers;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn stopped(dir: &Path, nonce: &str) -> bool {
    state::read::<Value>(&dir.join("stop.json"))
        .ok()
        .is_some_and(|s| s["run"] == nonce)
}

fn report(
    dir: &Path,
    status: &str,
    version: Option<&Version>,
    error: Option<String>,
) -> Result<()> {
    state::directory(&dir.join("hosts"))?;
    state::write(
        &dir.join("hosts/update.json"),
        &json!({"state":status,"version":version.map(ToString::to_string),"at":state::timestamp(),"error":error}),
    )
}

fn rollback(
    dir: &Path,
    installed: &mut Installed,
    failed: &Host,
    keys: &[&str],
    base: &Version,
) -> Result<Option<Host>> {
    eprintln!(
        "CLI {} failed; returning to the previous version.",
        failed.version
    );
    installed.highest = Some(
        installed
            .highest
            .as_deref()
            .and_then(|v| Version::parse(v).ok())
            .unwrap_or_else(|| base.clone())
            .max(failed.version.clone())
            .to_string(),
    );
    if installed
        .current
        .as_ref()
        .is_some_and(|s| s.body == failed.signed.body)
    {
        installed.current = installed.previous.take();
    }
    update::save(dir, installed)?;
    report(
        dir,
        "rolled back",
        Some(&failed.version),
        Some("new CLI failed to start or exited unexpectedly".into()),
    )?;
    Ok(update::current(dir, installed, keys)
        .filter(|h| h.version >= *base && h.version != failed.version))
}

#[cfg(test)]
mod tests;
